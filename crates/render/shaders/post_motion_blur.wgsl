// Motion Blur — fullscreen post pass.
//
// 在后处理阶段无 velocity buffer，因此使用邻域亮度梯度估算运动方向：
//   1. 计算水平/垂直方向的亮度梯度 → 近似屏幕空间运动向量
//   2. 沿运动方向前后采样 N 个点
//   3. 加权平均得到模糊后颜色
//
// Uniform 参数:
//   params.w = packed (fract*10 ≈ mb_strength 0-9)
//   frame.z  = frame_index (用于首帧检测)
//
// 降级：首帧（frame_index < 1）或梯度为零时直接 pass-through。

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

const NUM_SAMPLES: u32 = 12u;

@fragment
fn fs_motion_blur(in: VertexOutput) -> @location(0) vec4f {
    let color = textureSample(t_input, s_input, in.uv);
    let frame_index = u_post.frame.z;

    // 首帧无前帧数据：pass-through 降级
    if (frame_index < 1.0) {
        return color;
    }

    // 从 packed params.w 解码 MotionBlur 强度
    // params.w = SSAO_int + DoF_digit/10 + MB_digit/100
    let packed = u_post.params.w;
    let mb_strength = (packed * 100.0 - floor(packed * 100.0)) * 10.0;

    // 无强度：直接返回原图
    if (mb_strength < 0.01) {
        return color;
    }

    let texel_size = 1.0 / vec2f(textureDimensions(t_input, 0));
    let center_lum = luminance(color.rgb);

    // 使用亮度梯度估算运动方向（Sobel-like 算子）
    let l_l = luminance(textureSample(t_input, s_input, in.uv + vec2f(-2.0 * texel_size.x, 0.0)).rgb);
    let l_r = luminance(textureSample(t_input, s_input, in.uv + vec2f(2.0 * texel_size.x, 0.0)).rgb);
    let l_u = luminance(textureSample(t_input, s_input, in.uv + vec2f(0.0, -2.0 * texel_size.y)).rgb);
    let l_d = luminance(textureSample(t_input, s_input, in.uv + vec2f(0.0, 2.0 * texel_size.y)).rgb);

    var motion_dir = vec2f(l_r - l_l, l_d - l_u);
    let motion_mag = length(motion_dir);

    // 梯度太小 → 无明显运动 → pass-through
    if (motion_mag < 0.01) {
        return color;
    }

    motion_dir = motion_dir / motion_mag;

    // 采样长度：基于运动幅度和强度
    let sample_length = clamp(motion_mag * 6.0, 0.5, 12.0) * mb_strength * texel_size;

    // 沿运动方向前后采样
    var result = color.rgb;
    var total_weight: f32 = 1.0;
    let half_samples = NUM_SAMPLES / 2u;
    for (var i = 1u; i <= half_samples; i = i + 1u) {
        let t = f32(i) / f32(half_samples);
        let offset = motion_dir * t * sample_length;

        let s_fwd = textureSample(t_input, s_input, in.uv + offset).rgb;
        let s_bwd = textureSample(t_input, s_input, in.uv - offset).rgb;

        let w = 1.0 - t * 0.5; // 线性衰减权重
        result += (s_fwd + s_bwd) * w;
        total_weight += 2.0 * w;
    }
    result /= total_weight;

    // 混合：运动越强模糊越多
    let blend = clamp(motion_mag * mb_strength * 3.0, 0.0, 1.0);
    let final = mix(color.rgb, result, blend);

    return vec4f(final, color.a);
}
