// Motion Blur — 基于 velocity buffer 的运动模糊 compute shader。
//
// 流程：
//   1. 计算 velocity buffer（从上一帧 camera + 当前帧 camera 的 reprojection）
//   2. 对每个像素，沿 velocity 方向采样当前帧 color
//   3. 四分之一分辨率或全分辨率输出
//
// 用法：
//   binding 0: hdr_color (当前帧 HDR input)
//   binding 1: velocity_buffer (per-pixel screen-space velocity, Rg16Float)
//   binding 2: motion_blur_output (Rgba16Float storage output)
//   binding 3: params uniform

struct MotionBlurParams {
    intensity: f32,
    max_samples: u32,
    _pad0: u32,
    _pad1: u32,
};

@group(0) @binding(0) var hdr_color: texture_2d<f32>;
@group(0) @binding(1) var velocity_buffer: texture_2d<f32>;
@group(0) @binding(2) var motion_blur_output: texture_storage_2d<rgba16float, write>;
@group(0) @binding(3) var<uniform> params: MotionBlurParams;

@compute @workgroup_size(8, 8, 1)
fn cs_main(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let tex_dims = textureDimensions(motion_blur_output);
    if (global_id.x >= tex_dims.x || global_id.y >= tex_dims.y) {
        return;
    }

    let uv = (vec2<f32>(global_id.xy) + 0.5) / vec2<f32>(tex_dims);
    let velocity = textureLoad(velocity_buffer, vec2<i32>(global_id.xy), 0).xy * params.intensity;

    // 沿 velocity 方向采样
    let num_samples = min(params.max_samples, 16u);
    var result = vec4<f32>(0.0);
    for (var i: u32 = 0u; i < num_samples; i = i + 1u) {
        let t = (f32(i) + 0.5) / f32(num_samples);
        let sample_uv = uv - velocity * t;
        let sc = vec2<i32>(i32(sample_uv * vec2<f32>(tex_dims)));
        result = result + textureLoad(hdr_color, sc, 0);
    }
    result = result / f32(num_samples);

    textureStore(motion_blur_output, global_id.xy, result);
}
