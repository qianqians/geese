// Deferred+ geometry pass shader：写 G-Buffer。
// 与 [pbr_common.wgsl](file:///Users/qianqians/Documents/geese/crates/render/shaders/pbr_common.wgsl)
// 共享 MaterialUniform 结构。

const MAX_JOINTS: u32 = 128u;

struct Object {
    model: mat4x4<f32>,
    normal: mat4x4<f32>,
    skin: vec4<u32>,
    joints: array<mat4x4<f32>, MAX_JOINTS>,
};

@group(0) @binding(0) var<uniform> camera: Camera;

@group(1) @binding(0) var<uniform> material: MaterialUniform;
@group(1) @binding(1) var base_color_tex: texture_2d<f32>;
@group(1) @binding(2) var metallic_roughness_tex: texture_2d<f32>;
@group(1) @binding(3) var normal_tex: texture_2d<f32>;
@group(1) @binding(4) var occlusion_tex: texture_2d<f32>;
@group(1) @binding(5) var emissive_tex: texture_2d<f32>;
@group(1) @binding(6) var pbr_sampler: sampler;

@group(2) @binding(0) var<uniform> object: Object;

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
};

struct GBufferOutput {
    @location(0) base_metallic: vec4<f32>,
    @location(1) normal_roughness: vec4<f32>,
    @location(2) emissive_occlusion: vec4<f32>,
};

fn skin_matrix(input: VertexInput) -> mat4x4<f32> {
    if (object.skin.x == 0u) {
        return mat4x4<f32>(
            vec4<f32>(1.0, 0.0, 0.0, 0.0),
            vec4<f32>(0.0, 1.0, 0.0, 0.0),
            vec4<f32>(0.0, 0.0, 1.0, 0.0),
            vec4<f32>(0.0, 0.0, 0.0, 1.0),
        );
    }
    return object.joints[input.joints.x] * input.weights.x
         + object.joints[input.joints.y] * input.weights.y
         + object.joints[input.joints.z] * input.weights.z
         + object.joints[input.joints.w] * input.weights.w;
}

@vertex
fn vs_main(input: VertexInput) -> VertexOutput {
    var output: VertexOutput;
    let skin = skin_matrix(input);
    let model = object.model * skin;
    let world_position = model * vec4<f32>(input.position, 1.0);
    output.clip_position = camera.view_projection * world_position;
    output.world_position = world_position.xyz;
    output.normal = normalize((object.normal * skin * vec4<f32>(input.normal, 0.0)).xyz);
    output.uv = input.uv;
    output.tangent = vec4<f32>(
        normalize((model * vec4<f32>(input.tangent.xyz, 0.0)).xyz),
        input.tangent.w,
    );
    return output;
}

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
fn fs_main(input: VertexOutput) -> GBufferOutput {
    let base = sample_base_color(input.uv);
    let alpha_mode = material.flags.y;
    let alpha_cutoff = material.emissive_alpha_cutoff.w;
    if (alpha_mode == 1u && base.a < alpha_cutoff) {
        discard;
    }

    let mr = sample_metallic_roughness(input.uv);
    let n = compute_normal(input);

    var output: GBufferOutput;
    output.base_metallic = vec4<f32>(base.rgb, mr.x);
    // 法线打包到 [0,1]，lighting pass 还原为 *2-1
    let n_packed = n * 0.5 + vec3<f32>(0.5);
    output.normal_roughness = vec4<f32>(n_packed, clamp(mr.y, 0.04, 1.0));
    output.emissive_occlusion = vec4<f32>(sample_emissive(input.uv), sample_occlusion(input.uv));
    return output;
}
