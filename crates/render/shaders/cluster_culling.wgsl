// Cluster light culling compute shader.
//
// 对每个 cluster 计算 world-space AABB，与各光源的影响体积做相交测试，
// 结果写入 `cluster_bitmasks`（u32 bitmask，每 bit 对应一个光源索引）。
//
// 管线不变：bind group 布局、Rust 端代码均无需改动——仅改写本文件即可
// 从保守策略切换到真实剔除。

@group(0) @binding(0) var<uniform> cluster: ClusterUniform;
@group(0) @binding(1) var<uniform> lights: LightStorage;
@group(0) @binding(2) var<storage, read_write> cluster_bitmasks: array<u32, TOTAL_CLUSTERS>;

// ----- 辅助：将 NDC 坐标变换到 world-space -----
fn ndc_to_world(ndc: vec4<f32>) -> vec3<f32> {
    let vp = mat4x4<f32>(
        cluster.inv_vp_0,
        cluster.inv_vp_1,
        cluster.inv_vp_2,
        cluster.inv_vp_3,
    );
    var world = vp * ndc;
    if (abs(world.w) > 1e-6) {
        world = world / world.w;
    }
    return world.xyz;
}

// ----- Cluster world-space AABB -----
struct Aabb {
    min: vec3<f32>,
    max: vec3<f32>,
}

fn cluster_aabb(tile_x: u32, tile_y: u32, slice_idx: u32) -> Aabb {
    let tiles_x = f32(cluster.tile_count.x);
    let tiles_y = f32(cluster.tile_count.y);
    let slices = f32(cluster.tile_count.z);
    let z_near = cluster.screen_z.z;
    let log_per_slice = cluster.depth_params.x;

    // NDC tile 范围（x ∈ [-1, 1], y ∈ [-1, 1] inverted）
    let tx = f32(tile_x);
    let ty = f32(tile_y);
    let ndc_min_x = tx / tiles_x * 2.0 - 1.0;
    let ndc_max_x = (tx + 1.0) / tiles_x * 2.0 - 1.0;
    let ndc_min_y = 1.0 - (ty + 1.0) / tiles_y * 2.0;
    let ndc_max_y = 1.0 - ty / tiles_y * 2.0;

    // View-space 深度范围 → NDC z 范围
    // 使用 wgpu [0, 1] depth range
    let z_near_vs = z_near * exp(f32(slice_idx) * log_per_slice);
    let z_far_vs  = z_near * exp(f32(slice_idx + 1u) * log_per_slice);
    let zf = cluster.screen_z.w; // z_far
    // ndc_z = far * (z_vs - near) / (z_vs * (far - near))
    let ndc_near = zf * (z_near_vs - z_near) / (z_near_vs * (zf - z_near));
    let ndc_far  = zf * (z_far_vs  - z_near) / (z_far_vs  * (zf - z_near));

    // 8 个 NDC 角点（显式声明 + 逐元素赋值，避免 var + array constructor 的 naga 兼容性问题）
    var corners: array<vec4<f32>, 8>;
    corners[0] = vec4<f32>(ndc_min_x, ndc_min_y, ndc_near, 1.0);
    corners[1] = vec4<f32>(ndc_max_x, ndc_min_y, ndc_near, 1.0);
    corners[2] = vec4<f32>(ndc_min_x, ndc_max_y, ndc_near, 1.0);
    corners[3] = vec4<f32>(ndc_max_x, ndc_max_y, ndc_near, 1.0);
    corners[4] = vec4<f32>(ndc_min_x, ndc_min_y, ndc_far,  1.0);
    corners[5] = vec4<f32>(ndc_max_x, ndc_min_y, ndc_far,  1.0);
    corners[6] = vec4<f32>(ndc_min_x, ndc_max_y, ndc_far,  1.0);
    corners[7] = vec4<f32>(ndc_max_x, ndc_max_y, ndc_far,  1.0);

    // 变换到 world-space 并取 min/max
    var aabb: Aabb;
    let first = ndc_to_world(corners[0]);
    aabb.min = first;
    aabb.max = first;
    for (var i = 1u; i < 8u; i++) {
        let p = ndc_to_world(corners[i]);
        aabb.min = min(aabb.min, p);
        aabb.max = max(aabb.max, p);
    }
    return aabb;
}

// ----- Sphere vs AABB 相交测试 -----
fn sphere_aabb_intersect(center: vec3<f32>, radius: f32, aabb: Aabb) -> bool {
    // 找到 AABB 上距球心最近的点
    var closest = clamp(center, aabb.min, aabb.max);
    let dist_sq = dot(center - closest, center - closest);
    return dist_sq <= radius * radius;
}

// ----- 光源影响球体（保守估计）-----
fn light_bounding_sphere(light: Light) -> vec4<f32> {
    // 返回值: xyz = center, w = radius（0 = 不参与剔除 / directional）
    let ltype = light.direction_type.w;
    if (ltype < 0.5) {
        // Directional: 返回 w=0 表示影响所有 cluster
        return vec4<f32>(0.0, 0.0, 0.0, 0.0);
    }
    // Point / Spot: 返回包围球
    let center = light.position_range.xyz;
    let range = light.position_range.w;
    return vec4<f32>(center.x, center.y, center.z, range);
}

@compute @workgroup_size(64, 1, 1)
fn cs_main(@builtin(global_invocation_id) gid: vec3<u32>) {
    let cluster_index = gid.x;
    if (cluster_index >= TOTAL_CLUSTERS) {
        return;
    }

    // 从 1D index 反推 tile + slice
    let tx = cluster_index % CLUSTER_TILES_X;
    let ty_slice = cluster_index / CLUSTER_TILES_X;
    let ty = ty_slice % CLUSTER_TILES_Y;
    let slice = ty_slice / CLUSTER_TILES_Y;

    let count = min(lights.count.x, MAX_LIGHTS);
    if (count == 0u) {
        cluster_bitmasks[cluster_index] = 0u;
        return;
    }

    // 计算该 cluster 的 world-space AABB
    let aabb = cluster_aabb(tx, ty, slice);

    // 对每个光源做相交测试
    var bitmask: u32 = 0u;
    for (var i = 0u; i < count; i++) {
        let sphere = light_bounding_sphere(lights.lights[i]);
        if (sphere.w <= 0.0) {
            // Directional light: 影响所有 cluster
            bitmask |= (1u << i);
        } else if (sphere_aabb_intersect(sphere.xyz, sphere.w, aabb)) {
            bitmask |= (1u << i);
        }
    }
    cluster_bitmasks[cluster_index] = bitmask;
}
