// DOF (Depth of Field) — fullscreen post pass.
//
// 在后处理阶段无深度缓冲，因此使用颜色梯度（亮度变化）来估算散焦程度：
//   - 平坦区域（梯度低）→ 近似在焦 → 保持清晰
//   - 边缘/高梯度区域 → 近似散焦 → 应用 bokeh 模糊
//
// 算法：
//   1. 计算中心像素与邻域的亮度差作为梯度代理
//   2. 梯度越大 → 模糊半径越大（模拟 CoC）
//   3. 使用黄金角螺旋核采样实现圆形 bokeh
//   4. 近景模糊 + 远景模糊（双向梯度检测）
//
// Uniform 参数:
//   params.w = packed_dof (floor = ssao_intensity, fract*10 ≈ dof_strength 0-9)
//
// 降级：梯度为零（纯黑/平坦区域）时直接 pass-through。

struct PostUniformData {
    params: vec4f,
    frame: vec4f,
};

@group(0) @binding(0) var<uniform> u_post: PostUniformData;
@group(0) @binding(1) var t_input: texture_2d<f32>;
@group(0) @binding(2) var s_input: sampler;

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

fn luminance(c: vec3f) -> f32 {
    return dot(c, vec3f(0.2126, 0.7152, 0.0722));
}

fn enabled_mask() -> u32 {
    return bitcast<u32>(u_post.frame.w);
}

const NUM_SAMPLES: u32 = 22u;
const GOLDEN_ANGLE: f32 = 2.39996323;

@fragment
fn fs_dof(in: VertexOutput) -> @location(0) vec4f {
    let color = textureSample(t_input, s_input, in.uv);

    // 从 packed params.w 解码 DoF 强度
    // params.w = SSAO_int + DoF_digit/10 + MB_digit/100
    let packed = u_post.params.w;
    let dof_strength = (packed * 10.0 - floor(packed * 10.0)) * 10.0;

    // 无强度或 DoF 未启用：直接返回原图
    if (dof_strength < 0.01) {
        return color;
    }

    let texel_size = 1.0 / vec2f(textureDimensions(t_input, 0));
    let center_lum = luminance(color.rgb);

    // 在较大邻域内计算亮度梯度（作为深度变化的代理）
    var gradient: f32 = 0.0;
    var grad_offsets = array<vec2f, 8>(
        vec2f(-3.0, 0.0), vec2f(3.0, 0.0),
        vec2f(0.0, -3.0), vec2f(0.0, 3.0),
        vec2f(-2.0, -2.0), vec2f(2.0, -2.0),
        vec2f(-2.0, 2.0), vec2f(2.0, 2.0),
    );
    for (var i = 0u; i < 8u; i = i + 1u) {
        let sample_uv = in.uv + grad_offsets[i] * texel_size;
        let sl = luminance(textureSample(t_input, s_input, sample_uv).rgb);
        gradient += abs(sl - center_lum);
    }
    gradient /= 8.0;

    // 梯度映射到模糊半径（像素），受 dof_strength 控制
    let blur_radius = clamp(gradient * 12.0, 0.0, 8.0) * dof_strength * texel_size;

    // 半径过小则跳过模糊（pass-through 降级）
    if (length(blur_radius) < 0.0001) {
        return color;
    }

    // 黄金角螺旋核采样（圆形 bokeh）
    var blurred = color.rgb;
    var total_weight: f32 = 1.0;
    for (var i = 1u; i < NUM_SAMPLES; i = i + 1u) {
        let r = sqrt(f32(i) / f32(NUM_SAMPLES));
        let theta = f32(i) * GOLDEN_ANGLE;
        let offset = vec2f(cos(theta), sin(theta)) * r * blur_radius;
        let s = textureSample(t_input, s_input, in.uv + offset).rgb;
        let w = 1.0 - r * 0.5; // 中心权重更高
        blurred += s * w;
        total_weight += w;
    }
    blurred /= total_weight;

    // 基于梯度混合：梯度小 → 清晰（在焦），梯度大 → 模糊（散焦）
    let blend = smoothstep(0.02, 0.25, gradient) * dof_strength;
    let result = mix(color.rgb, blurred, clamp(blend, 0.0, 1.0));

    return vec4f(result, color.a);
}
