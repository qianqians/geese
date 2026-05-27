// PBR 公共结构体与 BRDF 函数。运行时由 Rust 端拼接到具体管线 shader 前面。
// 与 [crates/render/src/light.rs](file:///Users/qianqians/Documents/geese/crates/render/src/light.rs)
// 的 `GpuLight` / `LightStorage` 严格对齐。

const PI: f32 = 3.14159265359;
const MAX_LIGHTS: u32 = 16u;
const TOTAL_CLUSTERS: u32 = 1024u;
const CLUSTER_TILES_X: u32 = 8u;
const CLUSTER_TILES_Y: u32 = 8u;
const CLUSTER_DEPTH_SLICES: u32 = 16u;

const LIGHT_TYPE_DIRECTIONAL: f32 = 0.0;
const LIGHT_TYPE_POINT: f32 = 1.0;
const LIGHT_TYPE_SPOT: f32 = 2.0;

struct Camera {
    view_projection: mat4x4<f32>,
    inverse_view_projection: mat4x4<f32>,
    camera_position: vec4<f32>,
};

struct Light {
    position_range: vec4<f32>,
    direction_type: vec4<f32>,
    color_intensity: vec4<f32>,
    cone: vec4<f32>,
};

struct LightStorage {
    ambient: vec4<f32>,
    count: vec4<u32>,
    lights: array<Light, MAX_LIGHTS>,
};

struct ClusterUniform {
    tile_count: vec4<u32>,
    screen_z: vec4<f32>,
    depth_params: vec4<f32>,
    flags: vec4<f32>,
};

struct MaterialUniform {
    base_color_factor: vec4<f32>,
    emissive_alpha_cutoff: vec4<f32>,
    metallic_roughness_normal_occlusion: vec4<f32>,
    flags: vec4<u32>,
};

fn material_has_texture(mat: MaterialUniform, bit: u32) -> bool {
    return (mat.flags.x & (1u << bit)) != 0u;
}

// ---- PBR BRDF ----
fn distribution_ggx(n_dot_h: f32, roughness: f32) -> f32 {
    let a = roughness * roughness;
    let a2 = a * a;
    let denom = n_dot_h * n_dot_h * (a2 - 1.0) + 1.0;
    return a2 / max(PI * denom * denom, 1e-5);
}

fn geometry_schlick_ggx(n_dot_v: f32, roughness: f32) -> f32 {
    let r = roughness + 1.0;
    let k = (r * r) / 8.0;
    return n_dot_v / max(n_dot_v * (1.0 - k) + k, 1e-5);
}

fn geometry_smith(n_dot_v: f32, n_dot_l: f32, roughness: f32) -> f32 {
    return geometry_schlick_ggx(max(n_dot_v, 0.0), roughness)
         * geometry_schlick_ggx(max(n_dot_l, 0.0), roughness);
}

fn fresnel_schlick(cos_theta: f32, f0: vec3<f32>) -> vec3<f32> {
    let v = clamp(1.0 - cos_theta, 0.0, 1.0);
    return f0 + (vec3<f32>(1.0, 1.0, 1.0) - f0) * pow(v, 5.0);
}

fn attenuation_inverse_square(distance_sq: f32, range_sq: f32) -> f32 {
    if (distance_sq >= range_sq) {
        return 0.0;
    }
    let factor = 1.0 - distance_sq / range_sq;
    return factor * factor / max(distance_sq, 1e-4);
}

fn spot_cone_attenuation(cos_outer: f32, cos_inner: f32, cos_angle: f32) -> f32 {
    if (cos_inner <= cos_outer) {
        return select(0.0, 1.0, cos_angle >= cos_outer);
    }
    let t = clamp((cos_angle - cos_outer) / (cos_inner - cos_outer), 0.0, 1.0);
    return t * t * (3.0 - 2.0 * t);
}

// 单光源贡献（Cook-Torrance + Lambertian）。
fn shade_light(
    light: Light,
    world_pos: vec3<f32>,
    n: vec3<f32>,
    v: vec3<f32>,
    base_color: vec3<f32>,
    metallic: f32,
    roughness: f32,
    f0: vec3<f32>,
) -> vec3<f32> {
    var l: vec3<f32>;
    var radiance: vec3<f32>;

    if (light.direction_type.w < 0.5) {
        // Directional
        l = normalize(-light.direction_type.xyz);
        radiance = light.color_intensity.rgb * light.color_intensity.a;
    } else if (light.direction_type.w < 1.5) {
        // Point
        let to_light = light.position_range.xyz - world_pos;
        let dist_sq = dot(to_light, to_light);
        let atten = attenuation_inverse_square(dist_sq, light.cone.z);
        if (atten <= 0.0) {
            return vec3<f32>(0.0);
        }
        l = to_light * inverseSqrt(max(dist_sq, 1e-8));
        radiance = light.color_intensity.rgb * light.color_intensity.a * atten;
    } else {
        // Spot
        let to_light = light.position_range.xyz - world_pos;
        let dist_sq = dot(to_light, to_light);
        let atten = attenuation_inverse_square(dist_sq, light.cone.z);
        if (atten <= 0.0) {
            return vec3<f32>(0.0);
        }
        l = to_light * inverseSqrt(max(dist_sq, 1e-8));
        let spot_dir = normalize(light.direction_type.xyz);
        let cos_angle = dot(-spot_dir, l);
        let cone_atten = spot_cone_attenuation(light.cone.y, light.cone.x, cos_angle);
        if (cone_atten <= 0.0) {
            return vec3<f32>(0.0);
        }
        radiance = light.color_intensity.rgb * light.color_intensity.a * atten * cone_atten;
    }

    let h = normalize(v + l);
    let n_dot_l = max(dot(n, l), 0.0);
    let n_dot_v = max(dot(n, v), 0.0);
    let n_dot_h = max(dot(n, h), 0.0);
    let v_dot_h = max(dot(v, h), 0.0);

    let d = distribution_ggx(n_dot_h, roughness);
    let g = geometry_smith(n_dot_v, n_dot_l, roughness);
    let f = fresnel_schlick(v_dot_h, f0);

    let specular = (d * g * f) / max(4.0 * n_dot_v * n_dot_l, 1e-4);
    let kd = (vec3<f32>(1.0) - f) * (1.0 - metallic);
    let diffuse = kd * base_color / PI;

    return (diffuse + specular) * radiance * n_dot_l;
}

// ---- Cluster 工具 ----
fn cluster_index_from_screen(frag_xy: vec2<f32>, view_z: f32, cluster: ClusterUniform) -> u32 {
    let tile_x = u32(clamp(frag_xy.x / cluster.depth_params.z, 0.0, f32(cluster.tile_count.x) - 1.0));
    let tile_y = u32(clamp(frag_xy.y / cluster.depth_params.w, 0.0, f32(cluster.tile_count.y) - 1.0));
    let near = cluster.screen_z.z;
    let log_per_slice = cluster.depth_params.x;
    var slice: u32 = 0u;
    if (view_z > near && log_per_slice > 0.0) {
        let f = log(view_z / near) / log_per_slice;
        slice = u32(clamp(f, 0.0, f32(cluster.tile_count.z) - 1.0));
    }
    return (slice * cluster.tile_count.y + tile_y) * cluster.tile_count.x + tile_x;
}

// 将 clip-space 深度还原为 view-space z（正值，远处更大）。
fn linearize_depth(depth: f32, near: f32, far: f32) -> f32 {
    // 使用反向化方便：z_view = (near * far) / (far - depth * (far - near))
    return (near * far) / max(far - depth * (far - near), 1e-5);
}
