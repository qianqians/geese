// Volumetric fog post-process pass.
// Input:  HDR scene color (binding 1), depth buffer (binding 2)
// Output: Fogged scene color
//
// Reconstructs world-space position from depth, computes distance fog +
// height fog + hash noise, and blends with scene color.

struct FogUniform {
    // [color_r, color_g, color_b, density]
    color_density: vec4f,
    // [height_falloff, start_distance, end_distance, noise_scale]
    params: vec4f,
    // [noise_strength, enabled, time, _pad]
    extra: vec4f,
};

struct CameraData {
    view_projection: mat4x4f,
    inverse_view_projection: mat4x4f,
    camera_position: vec4f,
};

@group(0) @binding(0) var<uniform> u_fog: FogUniform;
@group(0) @binding(1) var t_scene: texture_2d<f32>;
@group(0) @binding(2) var t_depth: texture_depth<f32>;
@group(0) @binding(3) var s_scene: sampler;
@group(0) @binding(4) var<uniform> u_camera: CameraData;

struct VertexOutput {
    @builtin(position) position: vec4f,
    @location(0) uv: vec2f,
};

@vertex
fn vs_fullscreen(@builtin(vertex_index) vi: u32) -> VertexOutput {
    // Full-screen triangle: 3 vertices, no vertex buffer needed.
    var out: VertexOutput;
    let x = f32(vi & 1u) * 4.0 - 1.0;
    let y = f32((vi >> 1u) & 1u) * 4.0 - 1.0;
    out.position = vec4f(x, y, 0.0, 1.0);
    out.uv = vec2f((x + 1.0) * 0.5, (1.0 - (y + 1.0) * 0.5));
    return out;
}

// ----------------------------------------------------------------
// Hash noise — deterministic, no texture lookup.
// ----------------------------------------------------------------
fn hash_noise(p: vec3f) -> f32 {
    var h = sin(dot(p, vec3f(127.1, 311.7, 74.7))) * 43758.5453;
    return fract(h);
}

// ----------------------------------------------------------------
// Reconstruct world-space position from UV + depth.
// ----------------------------------------------------------------
fn world_pos_from_depth(uv: vec2f, depth: f32) -> vec3f {
    // NDC: x/y in [-1,1], z in [0,1] (wgpu convention)
    let ndc = vec4f(uv.x * 2.0 - 1.0, (1.0 - uv.y) * 2.0 - 1.0, depth, 1.0);
    let world = u_camera.inverse_view_projection * ndc;
    return world.xyz / world.w;
}

@fragment
fn fs_fog(in: VertexOutput) -> @location(0) vec4f {
    let scene_color = textureSample(t_scene, s_scene, in.uv).rgb;

    // If fog disabled, pass through.
    if (u_fog.extra.y <= 0.0) {
        return vec4f(scene_color, 1.0);
    }

    // Sample depth (non-linear [0,1]).
    let depth = textureLoad(t_depth, vec2i(i32(in.uv.x * f32(textureDimensions(t_depth).x)),
                                            i32(in.uv.y * f32(textureDimensions(t_depth).y))), 0);

    // Reconstruct world position.
    let world_pos = world_pos_from_depth(in.uv, depth);
    let cam_pos   = u_camera.camera_position.xyz;
    let to_pixel  = world_pos - cam_pos;
    let pixel_distance = length(to_pixel);

    // ---- Distance fog (linear ramp) ----
    let start_dist = u_fog.params.y;
    let end_dist   = u_fog.params.z;
    let dist_range = max(end_dist - start_dist, 0.001);
    let dist_fog   = clamp((pixel_distance - start_dist) / dist_range, 0.0, 1.0);

    // ---- Height fog (exponential falloff) ----
    let height_falloff = u_fog.params.x;
    let height_fog     = exp(-height_falloff * max(world_pos.y, 0.0));

    // ---- Noise perturbation ----
    let noise_scale    = u_fog.params.w;
    let noise_strength = u_fog.extra.x;
    var noise_factor   = 1.0;
    if (noise_strength > 0.0) {
        let n = hash_noise(world_pos * noise_scale);
        noise_factor = 1.0 + (n - 0.5) * noise_strength;
    }

    // ---- Combine ----
    let density  = u_fog.color_density.w;
    let fog_amount = clamp(dist_fog * height_fog * noise_factor * density, 0.0, 1.0);
    let fog_color  = u_fog.color_density.rgb;

    let result = mix(scene_color, fog_color, fog_amount);
    return vec4f(result, 1.0);
}
