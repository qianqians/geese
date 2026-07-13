// SSAO (Screen-Space Ambient Occlusion) — color-space approximation post pass.
//
// 在后处理阶段无法直接访问深度缓冲，因此使用颜色亮度梯度来近似环境光遮蔽：
// 在缝隙/边缘/折角处，相邻像素的亮度变化较大，据此估算遮蔽因子并对原图做柔和变暗。
//
// 算法：
//   1. 以当前像素为中心，在螺旋核中采样 N 个邻域像素
//   2. 计算各采样点与中心的亮度差（梯度）
//   3. 梯度累积作为遮蔽因子，经 intensity 缩放后对原图做乘法变暗
//
// Uniform 参数（来自 PostUniformData.params）:
//   params.x = exposure (tonemap 使用)
//   params.y = bloom_threshold
//   params.z = bloom_intensity
//   params.w = ssao_intensity (0.0 = 无效果, 1.0 = 全强度)
//
// Frame 参数:
//   frame.w = enabled_mask (bit 3 = SSAO)

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

// 螺旋核采样方向（16 个均匀分布的半球投影方向）
const KERNEL_SIZE: u32 = 16u;
const KERNEL: array<vec2f, 16> = array<vec2f, 16>(
    vec2f( 0.5381,  0.1856),
    vec2f( 0.1379,  0.6968),
    vec2f( 0.6852, -0.3995),
    vec2f(-0.2857,  0.5913),
    vec2f( 0.1398, -0.7345),
    vec2f(-0.4979, -0.4618),
    vec2f( 0.7138,  0.2908),
    vec2f(-0.6828,  0.1842),
    vec2f( 0.2809, -0.2517),
    vec2f(-0.2173, -0.7084),
    vec2f( 0.4198,  0.6107),
    vec2f(-0.5148,  0.5156),
    vec2f( 0.1367,  0.2994),
    vec2f(-0.1015, -0.3387),
    vec2f( 0.6129, -0.4893),
    vec2f(-0.3579, -0.5791),
);

@fragment
fn fs_ssao(in: VertexOutput) -> @location(0) vec4f {
    let color = textureSample(t_input, s_input, in.uv);
    // params.w is packed: SSAO_int + DoF_digit/10 + MB_digit/100
    // Extract SSAO intensity from integer part.
    let intensity = floor(u_post.params.w + 0.001);

    // 无强度或 SSAO 未启用：直接返回原图
    if (intensity <= 0.001) {
        return color;
    }

    let center_lum = luminance(color.rgb);
    let texel_size = 1.0 / vec2f(textureDimensions(t_input, 0));

    // 采样半径（像素空间），基于分辨率自适应
    let radius = 3.0 * texel_size;

    var occlusion = 0.0;
    for (var i: u32 = 0u; i < KERNEL_SIZE; i = i + 1u) {
        let sample_uv = in.uv + KERNEL[i] * radius;
        let sample_color = textureSample(t_input, s_input, sample_uv).rgb;
        let sample_lum = luminance(sample_color);

        // 亮度差越大 → 越可能是缝隙/边缘 → 遮蔽越强
        let diff = abs(sample_lum - center_lum);
        // 使用平滑阶跃避免硬边缘
        occlusion += smoothstep(0.0, 0.15, diff);
    }

    occlusion = occlusion / f32(KERNEL_SIZE);

    // 遮蔽因子：0 = 完全遮蔽（最暗），1 = 无遮蔽（原色）
    let ao = 1.0 - occlusion * intensity;
    let ao_clamped = clamp(ao, 0.3, 1.0);

    // 对原图做乘法变暗，保留色相
    return vec4f(color.rgb * ao_clamped, color.a);
}
