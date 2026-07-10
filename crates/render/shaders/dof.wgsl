// DOF (Depth of Field) — 半分辨率 compute shader。
//
// 基于 Circle of Confusion (CoC) 的景深效果。
// 流程：
//   1. 计算每个像素的 CoC 半径（基于 focus_distance 和 depth）
//   2. 分离式模糊：near blur + far blur
//   3. Composite：基于 CoC 在 sharp 和 blurred 之间插值
//
// 用法：
//   binding 0: hdr_color (半分辨率 HDR input)
//   binding 1: linear_depth (半分辨率 linear depth)
//   binding 2: dof_output (半分辨率 Rgba16Float storage output)
//   binding 3: params uniform

struct DofParams {
    focus_distance: f32,
    aperture: f32,
    max_coc: f32,
    _pad: u32,
};

@group(0) @binding(0) var hdr_color: texture_2d<f32>;
@group(0) @binding(1) var linear_depth: texture_2d<f32>;
@group(0) @binding(2) var dof_output: texture_storage_2d<rgba16float, write>;
@group(0) @binding(3) var<uniform> params: DofParams;

fn compute_coc(depth: f32) -> f32 {
    let fd = params.focus_distance;
    let a = params.aperture;
    // CoC = aperture * |depth - focus_distance| / depth, clamped
    let coc = a * abs(depth - fd) / max(depth, 0.001);
    return clamp(coc, 0.0, params.max_coc);
}

@compute @workgroup_size(8, 8, 1)
fn cs_main(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let tex_dims = textureDimensions(dof_output);
    if (global_id.x >= tex_dims.x || global_id.y >= tex_dims.y) {
        return;
    }

    let center = textureLoad(hdr_color, vec2<i32>(global_id.xy), 0);
    let depth = textureLoad(linear_depth, vec2<i32>(global_id.xy), 0).r;
    let coc = compute_coc(depth);

    // 分离式模糊 (disk blur): 采样周围像素
    var blurred = vec4<f32>(0.0);
    var sample_count: u32 = 0u;
    let radius = i32(coc * 8.0); // CoC to pixel radius
    for (var dy: i32 = -radius; dy <= radius; dy = dy + 1) {
        for (var dx: i32 = -radius; dx <= radius; dx = dx + 1) {
            let sample_coord = vec2<i32>(global_id.xy) + vec2<i32>(dx, dy);
            if (sample_coord.x >= 0 && sample_coord.x < i32(tex_dims.x) &&
                sample_coord.y >= 0 && sample_coord.y < i32(tex_dims.y)) {
                blurred = blurred + textureLoad(hdr_color, sample_coord, 0);
                sample_count = sample_count + 1u;
            }
        }
    }
    if (sample_count > 0u) {
        blurred = blurred / f32(sample_count);
    }

    // CoC 越小越清晰，越大越模糊
    let blend = clamp(coc / params.max_coc, 0.0, 1.0);
    let result = mix(center, blurred, blend);
    textureStore(dof_output, global_id.xy, result);
}
