// Cluster light culling compute shader.
//
// 当前实施保守策略：所有光源都被认为影响所有 cluster（bitmask = (1<<count) - 1），
// 这保证视觉与朴素 forward 一致，并保留 cluster 数据流作为未来真正剔除的接入点。
// 后续要做精细剔除时，仅需改写本文件，无需改动管线 Rust 代码或 fragment shader。

@group(0) @binding(0) var<uniform> cluster: ClusterUniform;
@group(0) @binding(1) var<uniform> lights: LightStorage;
@group(0) @binding(2) var<storage, read_write> cluster_bitmasks: array<u32, TOTAL_CLUSTERS>;

@compute @workgroup_size(64, 1, 1)
fn cs_main(@builtin(global_invocation_id) gid: vec3<u32>) {
    let cluster_index = gid.x;
    if (cluster_index >= TOTAL_CLUSTERS) {
        return;
    }

    let count = min(lights.count.x, MAX_LIGHTS);
    var bitmask: u32 = 0u;
    if (count >= 32u) {
        bitmask = 0xFFFFFFFFu;
    } else if (count > 0u) {
        bitmask = (1u << count) - 1u;
    }

    cluster_bitmasks[cluster_index] = bitmask;
}
