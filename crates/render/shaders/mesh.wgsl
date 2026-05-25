struct Camera {
    view_projection: mat4x4<f32>,
    camera_position: vec4<f32>,
};

struct Light {
    direction: vec4<f32>,
    color: vec4<f32>,
    ambient: vec4<f32>,
};

struct Material {
    base_color_factor: vec4<f32>,
    params: vec4<f32>,
};

struct Object {
    model: mat4x4<f32>,
    normal: mat4x4<f32>,
    skin: vec4<u32>,
    joints: array<mat4x4<f32>, 128>,
};

@group(0) @binding(0)
var<uniform> camera: Camera;

@group(0) @binding(1)
var<uniform> light: Light;

@group(1) @binding(0)
var<uniform> material: Material;

@group(1) @binding(1)
var normal_texture: texture_2d<f32>;

@group(1) @binding(2)
var normal_sampler: sampler;

@group(2) @binding(0)
var<uniform> object: Object;

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
    output.tangent = vec4<f32>(normalize((model * vec4<f32>(input.tangent.xyz, 0.0)).xyz), input.tangent.w);
    return output;
}

fn normal_from_map(input: VertexOutput) -> vec3<f32> {
    if (material.params.x < 0.5) {
        return normalize(input.normal);
    }

    let n = normalize(input.normal);
    let raw_t = input.tangent.xyz;
    let orthogonal_t = raw_t - n * dot(n, raw_t);
    var t = vec3<f32>(1.0, 0.0, 0.0);
    if (dot(orthogonal_t, orthogonal_t) > 0.000001) {
        t = normalize(orthogonal_t);
    } else if (abs(n.y) < 0.999) {
        t = normalize(cross(vec3<f32>(0.0, 1.0, 0.0), n));
    } else {
        t = normalize(cross(vec3<f32>(1.0, 0.0, 0.0), n));
    }
    let b = normalize(cross(n, t) * input.tangent.w);
    let sampled = textureSample(normal_texture, normal_sampler, input.uv).xyz * 2.0 - vec3<f32>(1.0, 1.0, 1.0);
    let tbn = mat3x3<f32>(t, b, n);
    return normalize(tbn * sampled);
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    let normal = normal_from_map(input);
    let light_dir = normalize(-light.direction.xyz);
    let view_dir = normalize(camera.camera_position.xyz - input.world_position);
    let half_dir = normalize(light_dir + view_dir);

    let diffuse_strength = max(dot(normal, light_dir), 0.0);
    let specular_strength = pow(max(dot(normal, half_dir), 0.0), max(material.params.y, 1.0));

    let base_color = material.base_color_factor;
    let ambient = light.ambient.rgb * base_color.rgb;
    let diffuse = diffuse_strength * light.color.rgb * base_color.rgb;
    let specular = specular_strength * light.color.rgb;

    return vec4<f32>(ambient + diffuse + specular, base_color.a);
}
