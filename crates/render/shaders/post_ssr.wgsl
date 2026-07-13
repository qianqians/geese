// SSR (Screen-Space Reflections) — fullscreen ray-march post pass.
//
// 在后处理阶段对每个像素沿反射方向做屏幕空间 ray marching，
// 利用颜色纹理和深度纹理（binding 3）查找反射交点，
// 基于 Fresnel 效应混合原始颜色与反射颜色。
//
// 算法：
//   1. 从深度纹理重建视图空间位置（简化：使用 UV + 深度近似）
//   2. 从深度梯度估算表面法线
//   3. 计算反射方向
//   4. 沿反射方向步进（ray march），在屏幕空间中查找深度交点
//   5. 交点处颜色即为反射颜色，基于 Fresnel 混合
//
// Uniform 参数（来自 PostUniformData）:
//   params.x = exposure (tonemap 使用)
//   params.y = bloom_threshold
//   params.z = bloom_intensity
//   params.w = ssao_intensity / taa_feedback
//   extra.x  = ssr_intensity (0.0 = 无效果, 1.0 = 全强度)
//   extra.y  = ssr_max_steps  (最大步数, 默认 32)
//   extra.z  = ssr_stride     (步长, 默认 0.02)
//
// Frame 参数:
//   frame.w = enabled_mask (bit 4 = SSR)
//
// Bindings:
//   binding 0: uniform buffer
//   binding 1: 场景颜色纹理 (来自 SSAO 输出或原始输入)
//   binding 2: sampler
//   binding 3: 深度纹理 (与颜色纹理同源，用于深度查找)

struct PostUniformData {
    params: vec4f,
    frame: vec4f,
    extra: vec4f,
};

@group(0) @binding(0) var<uniform> u_post: PostUniformData;
@group(0) @binding(1) var t_input: texture_2d<f32>;
@group(0) @binding(2) var s_input: sampler;
@group(0) @binding(3) var t_depth: texture_2d<f32>;

struct VertexOutput {
    @builtin(position) position: vec4f,
    @location(0) uv: vec2f,
};

@vertex
fn vs_fullscreen(@builtin(vertex_index) vi: u32) -> VertexOutput {
    var out: VertexOutput;
    let x = f32(vi & 1u) * 4.0 - 1.0;
    let y = f32((vi >> 1u) & 1u) * 4.0 - 1.0;
    out.position = vec4f(x, y, 0.0, 1.0);
    out.uv = vec2f((x + 1.0) * 0.5, (y + 1.0) * 0.5);
    out.uv.y = 1.0 - out.uv.y;
    return out;
}

fn enabled_mask() -> u32 {
    return bitcast<u32>(u_post.frame.w);
}

// 从深度值重建视图空间位置（简化模型：假设 FOV 60°, 近平面 0.1, 远平面 1000）
fn reconstruct_view_pos(uv: vec2f, depth: f32) -> vec3f {
    // 将深度从 [0,1] 映射到近似视图空间 Z
    let near = 0.1;
    let far = 1000.0;
    let z = near * far / (far - depth * (far - near));
    // 假设 FOV ~60°, aspect ~16:9, 用简化投影反算 X/Y
    let tan_half_fov = 0.577; // tan(30°)
    let aspect = 1.778;       // 16:9
    let x = (uv.x * 2.0 - 1.0) * z * tan_half_fov * aspect;
    let y = (1.0 - uv.y * 2.0) * z * tan_half_fov;
    return vec3f(x, y, -z);
}

// 从深度梯度估算表面法线
fn estimate_normal(uv: vec2f, texel_size: vec2f) -> vec3f {
    let d  = textureSample(t_depth, s_input, uv).r;
    let dR = textureSample(t_depth, s_input, uv + vec2f(texel_size.x, 0.0)).r;
    let dU = textureSample(t_depth, s_input, uv + vec2f(0.0, texel_size.y)).r;

    let p  = reconstruct_view_pos(uv, d);
    let pR = reconstruct_view_pos(uv + vec2f(texel_size.x, 0.0), dR);
    let pU = reconstruct_view_pos(uv + vec2f(0.0, texel_size.y), dU);

    let dp_dx = pR - p;
    let dp_dy = pU - p;
    let n = normalize(cross(dp_dx, dp_dy));
    // 确保法线朝向观察者
    if (n.z > 0.0) {
        return -n;
    }
    return n;
}

// Schlick Fresnel 近似
fn fresnel_schlick(cos_theta: f32, f0: f32) -> f32 {
    let t = 1.0 - cos_theta;
    let t2 = t * t;
    let t4 = t2 * t2;
    let t5 = t4 * t;
    return f0 + (1.0 - f0) * t5;
}

@fragment
fn fs_ssr(in: VertexOutput) -> @location(0) vec4f {
    let color = textureSample(t_input, s_input, in.uv);
    let intensity = u_post.extra.x;
    let max_steps = u32(u_post.extra.y);
    let stride = u_post.extra.z;

    // 无强度或 SSR 未启用：直接返回原图
    if (intensity <= 0.001) {
        return color;
    }

    let depth = textureSample(t_depth, s_input, in.uv).r;

    // 天空/背景像素 (depth ~1.0): 不做反射
    if (depth >= 0.999) {
        return color;
    }

    let texel_size = 1.0 / vec2f(textureDimensions(t_input, 0));

    // 重建视图空间位置 & 法线
    let view_pos = reconstruct_view_pos(in.uv, depth);
    let normal = estimate_normal(in.uv, texel_size);

    // 视图方向 (从像素指向观察者)
    let view_dir = normalize(-view_pos);

    // 反射方向
    let reflect_dir = reflect(-view_dir, normal);

    // Ray march: 沿反射方向步进，在屏幕空间中寻找交点
    let actual_steps = min(max_steps, 64u); // 硬上限防止性能问题
    let step_size = stride;

    var hit = false;
    var hit_uv = in.uv;
    var prev_ray_z = view_pos.z;

    for (var i: u32 = 1u; i <= actual_steps; i = i + 1u) {
        let t = f32(i) * step_size;
        let ray_pos = view_pos + reflect_dir * t;

        // 将 ray_pos 投影回屏幕 UV
        let near = 0.1;
        let far = 1000.0;
        let ray_depth = (far * (ray_pos.z + near)) / (ray_pos.z * (far - near) + far * near);
        // 简化：用线性深度比例映射回 UV
        let scale = near / (-ray_pos.z);
        let tan_half_fov = 0.577;
        let aspect = 1.778;

        var proj_uv: vec2f;
        proj_uv.x = (ray_pos.x / (-ray_pos.z) / (tan_half_fov * aspect) + 1.0) * 0.5;
        proj_uv.y = 1.0 - (ray_pos.y / (-ray_pos.z) / tan_half_fov + 1.0) * 0.5;

        // 超出屏幕范围：终止
        if (proj_uv.x < 0.0 || proj_uv.x > 1.0 || proj_uv.y < 0.0 || proj_uv.y > 1.0) {
            break;
        }

        let sample_depth = textureSample(t_depth, s_input, proj_uv).r;
        let sample_z = reconstruct_view_pos(proj_uv, sample_depth).z;

        // 判断交点：射线 Z 穿越了场景深度
        let ray_z = ray_pos.z;
        if (ray_z < sample_z && prev_ray_z >= sample_z) {
            hit = true;
            hit_uv = proj_uv;
            break;
        }

        prev_ray_z = ray_z;
    }

    if (!hit) {
        return color;
    }

    // 采样交点处的颜色作为反射颜色
    let reflection_color = textureSample(t_input, s_input, hit_uv).rgb;

    // Fresnel 混合：掠射角反射更强
    let cos_theta = max(dot(view_dir, normal), 0.0);
    let f0 = 0.04; // 非金属基础反射率
    let fresnel = fresnel_schlick(cos_theta, f0);

    // 混合原始颜色和反射颜色
    let mix_factor = fresnel * intensity;
    let result = mix(color.rgb, reflection_color, mix_factor);

    return vec4f(result, color.a);
}
