// ACES filmic tonemap + exposure adjustment full-screen pass.
// Input: HDR color texture (Rgba16Float or Rgba8Unorm)
// Output: LDR color texture (Rgba8Unorm)

struct PostUniformData {
    // x = exposure, y = bloom_threshold, z = bloom_intensity, w = taa_feedback
    params: vec4f,
    // x = taa_jitter_x, y = taa_jitter_y, z = frame_index, w = enabled_mask (u32 bits)
    frame: vec4f,
};

@group(0) @binding(0) var<uniform> u_post: PostUniformData;
@group(0) @binding(1) var t_input: texture_2d<f32>;
@group(0) @binding(2) var s_input: sampler;
@group(0) @binding(3) var t_bloom: texture_2d<f32>;

struct VertexOutput {
    @builtin(position) position: vec4f,
    @location(0) uv: vec2f,
};

@vertex
fn vs_fullscreen(@builtin(vertex_index) vi: u32) -> VertexOutput {
    // Full-screen triangle: 3 vertices, no vertex buffer needed
    var out: VertexOutput;
    let x = f32(vi & 1u) * 4.0 - 1.0;
    let y = f32((vi >> 1u) & 1u) * 4.0 - 1.0;
    out.position = vec4f(x, y, 0.0, 1.0);
    out.uv = vec2f((x + 1.0) * 0.5, (y + 1.0) * 0.5);
    // Flip Y for texture sampling
    out.uv.y = 1.0 - out.uv.y;
    return out;
}

// ACES filmic tonemap (Krzysztof Narkowicz simplified fit)
fn aces_tonemap(x: f32) -> f32 {
    const a = 2.51;
    const b = 0.03;
    const c = 2.43;
    const d = 0.59;
    const e = 0.14;
    let num = x * (a * x + b);
    let den = x * (c * x + d) + e;
    return clamp(num / den, 0.0, 1.0);
}

fn enabled_mask() -> u32 {
    return bitcast<u32>(u_post.frame.w);
}

@fragment
fn fs_tonemap(in: VertexOutput) -> @location(0) vec4f {
    let color = textureSample(t_input, s_input, in.uv).rgb;
    let exposure = u_post.params.x;

    var result = color * exposure;

    // Bloom synthesis (bit 1 = bloom enabled)
    if ((enabled_mask() & 2u) != 0u) {
        let bloom = textureSample(t_bloom, s_input, in.uv).rgb;
        result = result + bloom * u_post.params.z;  // z = bloom_intensity
    }

    // Check if tonemap is enabled (bit 0)
    if ((enabled_mask() & 1u) != 0u) {
        result = vec3f(
            aces_tonemap(result.r),
            aces_tonemap(result.g),
            aces_tonemap(result.b),
        );
    }

    return vec4f(result, 1.0);
}
