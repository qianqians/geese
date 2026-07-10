// Forward+ Instanced 渲染管线 vertex + fragment shader。
//
// 与 forward_plus.wgsl 的区别：vertex 入口使用 `@builtin(instance_index)`，
// 从 instance storage buffer 读取 per-instance model 矩阵，替代 Object uniform。
// Fragment shader 完全复用 pbr_common.wgsl 中的 BRDF。

const MAX_JOINTS: u32 = 32u;

// ---- Instance data (storage buffer, group 2) ----
struct InstanceData {
    model: mat4x4<f32>,
};

@group(0) @binding(0) var<uniform> camera: Camera;
@group(0) @binding(1) var<uniform> lights: LightStorage;
@group(0) @binding(2) var<uniform> cluster: ClusterUniform;
@group(0) @binding(3) var<storage, read> cluster_bitmasks: array<u32, TOTAL_CLUSTERS>;

@group(1) @binding(0) var<uniform> material: MaterialUniform;
@group(1) @binding(1) var base_color_tex: texture_2d<f32>;
@group(1) @binding(2) var metallic_roughness_tex: texture_2d<f32>;
@group(1) @binding(3) var normal_tex: texture_2d<f32>;
@group(1) @binding(4) var occlusion_tex: texture_2d<f32>;
@group(1) @binding(5) var emissive_tex: texture_2d<f32>;
@group(1) @binding(6) var pbr_sampler: sampler;

// 复用 group 2 的 object layout，但绑定 instance buffer 而不是 Object uniform
@group(2) @binding(0) var<storage, read> instances: array<InstanceData>;

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) tangent: vec4<f32>,
    @location(4) joints: vec4<u32>,
    @location(5) weights: vec4<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) world_position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) tangent: vec4<f32>,
    @location(4) view_z: f32,
};

// 从 model 矩阵提取旋转部分作为 normal matrix（假设均匀缩放）。
fn normal_matrix_from_model(model: mat4x4<f32>) -> mat3x3<f32> {
    return mat3x3<f32>(
        model[0].xyz,
        model[1].xyz,
        model[2].xyz,
    );
}

@vertex
fn vs_main_instanced(
    input: VertexInput,
    @builtin(instance_index) instance_idx: u32,
) -> VertexOutput {
    var output: VertexOutput;
    let model = instances[instance_idx].model;
    let world_position = model * vec4<f32>(input.position, 1.0);
    output.clip_position = camera.view_projection * world_position;
    output.world_position = world_position.xyz;
    let normal_mat = normal_matrix_from_model(model);
    output.normal = normalize(normal_mat * input.normal);
    output.uv = input.uv;
    output.tangent = vec4<f32>(
        normalize((model * vec4<f32>(input.tangent.xyz, 0.0)).xyz),
        input.tangent.w,
    );
    let to_camera = world_position.xyz - camera.camera_position.xyz;
    output.view_z = length(to_camera);
    return output;
}

// ---- Fragment shader (与 forward_plus.wgsl 完全相同) ----

fn sample_base_color(uv: vec2<f32>) -> vec4<f32> {
    let factor = material.base_color_factor;
    if (material_has_texture(material, 0u)) {
        return textureSample(base_color_tex, pbr_sampler, uv) * factor;
    }
    return factor;
}

fn sample_metallic_roughness(uv: vec2<f32>) -> vec2<f32> {
    let metallic_factor = material.metallic_roughness_normal_occlusion.x;
    let roughness_factor = material.metallic_roughness_normal_occlusion.y;
    if (material_has_texture(material, 1u)) {
        let tex = textureSample(metallic_roughness_tex, pbr_sampler, uv);
        return vec2<f32>(tex.b * metallic_factor, tex.g * roughness_factor);
    }
    return vec2<f32>(metallic_factor, roughness_factor);
}

fn sample_occlusion(uv: vec2<f32>) -> f32 {
    let strength = material.metallic_roughness_normal_occlusion.w;
    if (material_has_texture(material, 3u)) {
        let occ = textureSample(occlusion_tex, pbr_sampler, uv).r;
        return mix(1.0, occ, strength);
    }
    return 1.0;
}

fn sample_emissive(uv: vec2<f32>) -> vec3<f32> {
    let factor = material.emissive_alpha_cutoff.rgb;
    if (material_has_texture(material, 4u)) {
        return textureSample(emissive_tex, pbr_sampler, uv).rgb * factor;
    }
    return factor;
}

fn compute_normal(input: VertexOutput) -> vec3<f32> {
    let n = normalize(input.normal);
    if (!material_has_texture(material, 2u)) {
        return n;
    }
    let raw_t = input.tangent.xyz;
    let orthogonal_t = raw_t - n * dot(n, raw_t);
    var t = vec3<f32>(1.0, 0.0, 0.0);
    if (dot(orthogonal_t, orthogonal_t) > 1e-6) {
        t = normalize(orthogonal_t);
    } else if (abs(n.y) < 0.999) {
        t = normalize(cross(vec3<f32>(0.0, 1.0, 0.0), n));
    } else {
        t = normalize(cross(vec3<f32>(1.0, 0.0, 0.0), n));
    }
    let b = normalize(cross(n, t) * input.tangent.w);
    let sampled = textureSample(normal_tex, pbr_sampler, input.uv).xyz * 2.0 - vec3<f32>(1.0);
    let scale = material.metallic_roughness_normal_occlusion.z;
    let scaled = vec3<f32>(sampled.xy * scale, sampled.z);
    let tbn = mat3x3<f32>(t, b, n);
    return normalize(tbn * scaled);
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    let base = sample_base_color(input.uv);
    let alpha_mode = material.flags.y;
    let alpha_cutoff = material.emissive_alpha_cutoff.w;
    if (alpha_mode == 1u && base.a < alpha_cutoff) {
        discard;
    }

    let mr = sample_metallic_roughness(input.uv);
    let metallic = mr.x;
    let roughness = clamp(mr.y, 0.04, 1.0);
    let occlusion = sample_occlusion(input.uv);
    let emissive = sample_emissive(input.uv);

    let n = compute_normal(input);
    let v = normalize(camera.camera_position.xyz - input.world_position);
    let f0 = mix(vec3<f32>(0.04), base.rgb, metallic);

    let cluster_index = cluster_index_from_screen(input.clip_position.xy, input.view_z, cluster);
    let bitmask = cluster_bitmasks[min(cluster_index, TOTAL_CLUSTERS - 1u)];
    let count = min(lights.count.x, MAX_LIGHTS);

    var color = vec3<f32>(0.0);
    for (var i: u32 = 0u; i < count; i = i + 1u) {
        if ((bitmask & (1u << i)) == 0u) {
            continue;
        }
        color = color + shade_light(
            lights.lights[i],
            input.world_position,
            n,
            v,
            base.rgb,
            metallic,
            roughness,
            f0,
        );
    }

    color = color + lights.ambient.rgb * base.rgb * occlusion;
    color = color + emissive;

    let out_alpha = select(1.0, base.a, alpha_mode == 2u);
    return vec4<f32>(color, out_alpha);
}
