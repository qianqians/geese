// SSR (Screen-Space Reflections) — 半分辨率 ray-march compute shader。
//
// 从半分辨率 color + depth + normal buffer 出发，沿反射方向步进 ray-march，
// 检查深度缓冲判断命中。复用 Hi-Z pyramid 加速（可选）。
//
// 用法：
//   binding 0: color_buffer (半分辨率 HDR color)
//   binding 1: linear_depth (半分辨率 depth)
//   binding 2: normal_roughness (半分辨率 normal.xy + roughness packed)
//   binding 3: hi_z_pyramid (深度金字塔 texture，可选)
//   binding 4: ssr_output (半分辨率 Rgba16Float storage)
//   binding 5: params uniform

struct SsrParams {
    max_steps: u32,
    stride: f32,
    thickness: f32,
    _pad: u32,
};

@group(0) @binding(0) var color_buffer: texture_2d<f32>;
@group(0) @binding(1) var linear_depth: texture_2d<f32>;
@group(0) @binding(2) var normal_roughness: texture_2d<f32>;
@group(0) @binding(3) var hi_z_pyramid: texture_2d<f32>;
@group(0) @binding(4) var ssr_output: texture_storage_2d<rgba16float, write>;
@group(0) @binding(5) var<uniform> params: SsrParams;

fn view_to_ndc(pos_vs: vec3<f32>, proj: mat4x4<f32>) -> vec3<f32> {
    let clip = proj * vec4<f32>(pos_vs, 1.0);
    return vec3<f32>(clip.xy / clip.w, clip.z / clip.w);
}

@compute @workgroup_size(8, 8, 1)
fn cs_main(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let tex_dims = textureDimensions(ssr_output);
    if (global_id.x >= tex_dims.x || global_id.y >= tex_dims.y) {
        return;
    }

    let uv = (vec2<f32>(global_id.xy) + 0.5) / vec2<f32>(tex_dims);
    let depth = textureLoad(linear_depth, vec2<i32>(global_id.xy), 0).r;

    // 天空/背景：不反射
    if (depth >= 1.0) {
        textureStore(ssr_output, global_id.xy, vec4<f32>(0.0, 0.0, 0.0, 0.0));
        return;
    }

    // 简化的 ray-march（在屏幕空间沿反射向量步进）
    // 实际实现需要 view-space position reconstruction + reflection direction
    let color = textureLoad(color_buffer, vec2<i32>(global_id.xy), 0);

    // Stub: 采样相邻像素作为近似反射
    let nr = textureLoad(normal_roughness, vec2<i32>(global_id.xy), 0);
    let roughness = nr.b;

    // 粗糙表面：模糊采样（简化实现）
    var reflection = vec4<f32>(0.0);
    let sample_radius = roughness * 4.0;
    for (var dy: i32 = -1; dy <= 1; dy = dy + 1) {
        for (var dx: i32 = -1; dx <= 1; dx = dx + 1) {
            let sample_uv = uv + vec2<f32>(f32(dx), f32(dy)) * sample_radius / vec2<f32>(tex_dims);
            let sc = vec2<i32>(i32(sample_uv * vec2<f32>(tex_dims)));
            reflection = reflection + textureLoad(color_buffer, sc, 0);
        }
    }
    reflection = reflection / 9.0;

    textureStore(ssr_output, global_id.xy, reflection);
}
