// GPU LOD 选择 compute shader。
//
// Feature gate: `lod`（默认禁用）。
//
// 功能：对每个物体，根据相机距离从 LOD 级别列表中选择合适的级别，
//       将选中的 index_count 和 variant_index 写入 indirect draw buffer。
//       与 CPU 端 `lod::select_lod()` 逻辑等价。
//
// 当前阶段：该 shader 作为 GPU-driven LOD 的预留入口（Plan B），
// 默认使用 CPU 端 LOD 选择（见 `lod.rs`）。

struct LodLevel {
    distance: f32,
    variant_index: u32,
    index_count: u32,
    _pad: u32,
};

struct ObjectData {
    camera_distance: f32,
    lod_count: u32,
    lod_offset: u32, // offset into lod_buffer
    _pad: u32,
};

@group(0) @binding(0) var<storage, read> objects: array<ObjectData>;
@group(0) @binding(1) var<storage, read> lod_levels: array<LodLevel>;
@group(0) @binding(2) var<storage, read_write> output: array<u32>; // [variant_index, index_count] per object

@compute @workgroup_size(64, 1, 1)
fn cs_main(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let obj_idx = global_id.x;
    if (obj_idx >= arrayLength(&objects)) {
        return;
    }

    let obj = objects[obj_idx];
    let count = obj.lod_count;

    // 无 LOD → 使用默认 (variant 0, index_count 0 = 完整网格)
    if (count == 0u) {
        output[obj_idx * 2u] = 0u;       // variant_index = 0
        output[obj_idx * 2u + 1u] = 0u;  // index_count = 0 (use full)
        return;
    }

    let base = obj.lod_offset;
    // 从大到小遍历 distance 阈值（lod_levels 按 distance 降序排列）
    for (var i: u32 = 0u; i < count; i = i + 1u) {
        let level = lod_levels[base + i];
        if (obj.camera_distance >= level.distance) {
            output[obj_idx * 2u] = level.variant_index;
            output[obj_idx * 2u + 1u] = level.index_count;
            return;
        }
    }

    // 距离小于所有阈值 → 最高细节（最后一个）
    let last = lod_levels[base + count - 1u];
    output[obj_idx * 2u] = last.variant_index;
    output[obj_idx * 2u + 1u] = last.index_count;
}
