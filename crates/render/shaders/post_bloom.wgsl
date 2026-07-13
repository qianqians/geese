// Bloom: downsample + upsample (dual-pass via mip-like reduction)
// First pass: threshold bright pixels and downsample
// Second pass: upsample and blend back

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

fn enabled_mask() -> u32 {
    return bitcast<u32>(u_post.frame.w);
}

// Downsample pass: threshold + box filter
@fragment
fn fs_bloom_downsample(in: VertexOutput) -> @location(0) vec4f {
    let threshold = u_post.params.y;
    let texel_size = 1.0 / vec2f(textureDimensions(t_input, 0));

    // 5-tap cross filter
    let c = textureSample(t_input, s_input, in.uv).rgb;
    let l = textureSample(t_input, s_input, in.uv + vec2f(-texel_size.x, 0.0)).rgb;
    let r = textureSample(t_input, s_input, in.uv + vec2f(texel_size.x, 0.0)).rgb;
    let u = textureSample(t_input, s_input, in.uv + vec2f(0.0, -texel_size.y)).rgb;
    let d = textureSample(t_input, s_input, in.uv + vec2f(0.0, texel_size.y)).rgb;

    let avg = (c + l + r + u + d) * 0.2;

    // Soft threshold: luminance-based
    let lum = dot(avg, vec3f(0.2126, 0.7152, 0.0722));
    let soft = max(lum - threshold, 0.0) / max(lum, 0.0001);
    let contribution = mix(vec3f(0.0), avg, vec3f(soft));

    return vec4f(contribution, 1.0);
}

// Upsample pass: bilinear upsample + additive blend
@fragment
fn fs_bloom_upsample(in: VertexOutput) -> @location(0) vec4f {
    let texel_size = 1.0 / vec2f(textureDimensions(t_input, 0));

    // 9-tap tent filter
    var result = vec3f(0.0);
    let offsets = array(
        vec2f(-1.0, -1.0), vec2f(0.0, -1.0), vec2f(1.0, -1.0),
        vec2f(-1.0, 0.0),  vec2f(0.0, 0.0),  vec2f(1.0, 0.0),
        vec2f(-1.0, 1.0),  vec2f(0.0, 1.0),  vec2f(1.0, 1.0),
    );
    let weights = array(
        0.0625, 0.125, 0.0625,
        0.125,  0.25,  0.125,
        0.0625, 0.125, 0.0625,
    );

    for (var i = 0u; i < 9u; i = i + 1u) {
        let offset = offsets[i] * texel_size;
        result += textureSample(t_input, s_input, in.uv + offset).rgb * weights[i];
    }

    // intensity 乘法延迟到 tonemap 阶段统一处理，避免双重应用
    return vec4f(result, 1.0);
}
