// Hi-Z 深度金字塔构建 compute shader。
//
// 功能：从 src mip level 的深度纹理执行 2×2 max 降采样，写入 dst mip level。
// 每个 workgroup 处理 8×8 像素区域。
//
// 用法：循环调用，每个 mip pair 一次 compute pass。
//   binding 0: src_depth (texture_2d<f32>, readable)
//   binding 1: dst_depth (texture_storage_2d<r32float, write>)

@group(0) @binding(0) var src_depth: texture_2d<f32>;
@group(0) @binding(1) var dst_depth: texture_storage_2d<r32float, write>;

@compute @workgroup_size(8, 8, 1)
fn cs_main(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let dst_coord = vec2<u32>(global_id.x, global_id.y);

    // 检查 dst 边界
    let dst_dims = textureDimensions(dst_depth);
    if (dst_coord.x >= dst_dims.x || dst_coord.y >= dst_dims.y) {
        return;
    }

    // 2×2 max reduction from src at double resolution
    let src_coord = dst_coord * 2u;
    let d00 = textureLoad(src_depth, vec2<i32>(i32(src_coord.x),     i32(src_coord.y)),      0);
    let d10 = textureLoad(src_depth, vec2<i32>(i32(src_coord.x + 1u), i32(src_coord.y)),      0);
    let d01 = textureLoad(src_depth, vec2<i32>(i32(src_coord.x),     i32(src_coord.y + 1u)), 0);
    let d11 = textureLoad(src_depth, vec2<i32>(i32(src_coord.x + 1u), i32(src_coord.y + 1u)), 0);

    // Max depth (farthest) = conservative occlusion
    let max_depth = max(max(d00, d10), max(d01, d11));
    textureStore(dst_depth, dst_coord, vec4<f32>(max_depth, 0.0, 0.0, 0.0));
}
