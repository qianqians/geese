// reflection_probe.wgsl
//
// 反射探针采样 — 在延迟/前向 lighting pass 中根据探针距离计算混合权重，
// 从 cubemap 采样反射颜色。
//
// Bind group 由渲染管线统一管理，以下为接口声明。

struct ReflectionProbeData {
    count: u32,
    _pad0: u32,
    _pad1: u32,
    _pad2: u32,
    // xyz = position, w = influence_radius
    positions: array<vec4<f32>, 8>,
    // x = blend_distance, y = has_cubemap (0/1), zw = pad
    params: array<vec4<f32>, 8>,
};

@group(2) @binding(10) var<uniform> reflection_probes: ReflectionProbeData;

// cubemap 采样器（由渲染管线在 bind group 中绑定）
// @group(2) @binding(11) var reflection_cubemaps: array<texture_cube<f32>, 8>;
// @group(2) @binding(12) var reflection_sampler: sampler;

/// 计算某世界位置受第 i 个探针的影响权重。
/// 返回 0.0（无影响）到 1.0（完全影响）。
fn probe_influence_weight(world_pos: vec3<f32>, probe_idx: i32) -> f32 {
    let probe_pos = reflection_probes.positions[probe_idx].xyz;
    let influence_radius = reflection_probes.positions[probe_idx].w;
    let blend_distance = reflection_probes.params[probe_idx].x;
    let has_cubemap = reflection_probes.params[probe_idx].y;

    if (has_cubemap < 0.5) {
        return 0.0;
    }

    let dist = distance(world_pos, probe_pos);
    let outer_radius = influence_radius + blend_distance;

    if (dist > outer_radius) {
        return 0.0;
    }

    if (dist <= influence_radius) {
        return 1.0;
    }

    // 线性衰减：[influence_radius, influence_radius + blend_distance]
    let t = (dist - influence_radius) / max(blend_distance, 0.001);
    return clamp(1.0 - t, 0.0, 1.0);
}

/// 对所有活跃探针求加权反射颜色（简化版）。
/// reflect_dir: 反射方向向量（世界空间）
/// world_pos: 像素世界位置
/// 返回混合后的反射 RGB 颜色。
fn sample_reflection_probes(world_pos: vec3<f32>, reflect_dir: vec3<f32>) -> vec3<f32> {
    var total_weight: f32 = 0.0;
    var reflected_color: vec3<f32> = vec3<f32>(0.0);

    let count = reflection_probes.count;
    for (var i: u32 = 0u; i < count; i = i + 1u) {
        let w = probe_influence_weight(world_pos, i32(i));
        if (w > 0.001) {
            // TODO: 实际采样需绑定 cubemap 数组
            // let cubemap_color = textureSample(
            //     reflection_cubemaps[i],
            //     reflection_sampler,
            //     reflect_dir
            // ).rgb;
            // reflected_color = reflected_color + cubemap_color * w;

            // 占位：返回探针位置编码的伪颜色（仅供调试）
            let probe_pos = reflection_probes.positions[i].xyz;
            let debug_color = normalize(probe_pos) * 0.5 + vec3<f32>(0.5);
            reflected_color = reflected_color + debug_color * w;

            total_weight = total_weight + w;
        }
    }

    if (total_weight > 0.0) {
        reflected_color = reflected_color / total_weight;
    }

    return reflected_color;
}
