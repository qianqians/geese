// SSAO (Screen-Space Ambient Occlusion) — HBAO 风格 compute shader。
//
// 半分辨率计算：每个线程处理一个像素，在半球内采样深度缓冲估算遮蔽。
// 结果经 bilateral blur 后双边上采样合成到全分辨率。
//
// 用法：
//   binding 0: linear_depth (半分辨率 depth texture)
//   binding 1: ssao_output (半分辨率 R8Unorm storage texture)
//   binding 2: params uniform

struct SsaoParams {
    radius: f32,
    bias: f32,
    intensity: f32,
    sample_count: u32,
};

@group(0) @binding(0) var linear_depth: texture_2d<f32>;
@group(0) @binding(1) var ssao_output: texture_storage_2d<r8unorm, write>;
@group(0) @binding(2) var<uniform> params: SsaoParams;

// 随机采样方向（16 个半球方向）
const SAMPLE_COUNT: u32 = 16u;
var<private> SAMPLES: array<vec3<f32>, 16> = array<vec3<f32>, 16>(
    vec3<f32>( 0.5381,  0.1856,  0.4319),
    vec3<f32>( 0.1379,  0.6968,  0.2987),
    vec3<f32>( 0.6852, -0.3995,  0.2356),
    vec3<f32>(-0.2857,  0.5913,  0.2573),
    vec3<f32>( 0.1398, -0.7345,  0.3326),
    vec3<f32>(-0.4979, -0.4618,  0.2785),
    vec3<f32>( 0.7138,  0.2908,  0.1526),
    vec3<f32>(-0.6828,  0.1842,  0.1937),
    vec3<f32>( 0.2809, -0.2517,  0.6154),
    vec3<f32>(-0.2173, -0.7084,  0.3182),
    vec3<f32>( 0.4198,  0.6107,  0.2519),
    vec3<f32>(-0.5148,  0.5156,  0.1843),
    vec3<f32>( 0.1367,  0.2994,  0.7815),
    vec3<f32>(-0.1015, -0.3387,  0.8172),
    vec3<f32>( 0.6129, -0.4893,  0.3116),
    vec3<f32>(-0.3579, -0.5791,  0.2694),
);

@compute @workgroup_size(8, 8, 1)
fn cs_main(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let tex_dims = textureDimensions(ssao_output);
    if (global_id.x >= tex_dims.x || global_id.y >= tex_dims.y) {
        return;
    }

    let uv = (vec2<f32>(global_id.xy) + 0.5) / vec2<f32>(tex_dims);
    let depth = textureLoad(linear_depth, vec2<i32>(global_id.xy), 0).r;

    // 背景 (depth >= 1.0)：无遮蔽
    if (depth >= 1.0) {
        textureStore(ssao_output, global_id.xy, vec4<f32>(1.0));
        return;
    }

    var occlusion = 0.0;
    let r = params.radius / (depth + 0.001);
    // 简化：使用随机采样方向（实际 HBAO 需要 view-space normal）
    for (var i: u32 = 0u; i < SAMPLE_COUNT; i = i + 1u) {
        let sample_uv = uv + SAMPLES[i].xy * r;
        if (sample_uv.x < 0.0 || sample_uv.x > 1.0 || sample_uv.y < 0.0 || sample_uv.y > 1.0) {
            continue;
        }
        let sample_coord = vec2<i32>(i32(sample_uv * vec2<f32>(tex_dims)));
        let sample_depth = textureLoad(linear_depth, sample_coord, 0).r;
        if (sample_depth < depth - params.bias) {
            occlusion = occlusion + 1.0;
        }
    }

    let ao = 1.0 - occlusion / f32(SAMPLE_COUNT);
    textureStore(ssao_output, global_id.xy, vec4<f32>(ao, 0.0, 0.0, 1.0));
}
