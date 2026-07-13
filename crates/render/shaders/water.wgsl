// Water surface rendering — Gerstner-style 3-layer sine wave + Fresnel + specular.
// Input:  water mesh (XZ plane), WaterUniform, camera, environment/skybox
// Output: lit water surface color

struct WaterUniform {
    // [water_level, wave_amplitude, wave_frequency, wave_speed]
    wave: vec4f,
    // [water_r, water_g, water_b, specular_power]
    color_specular: vec4f,
    // [fresnel_power, refraction_strength, reflection_strength, time]
    fresnel_time: vec4f,
    // [extent, subdivisions, _pad, _pad]
    mesh_params: vec4f,
};

struct CameraData {
    view_projection: mat4x4f,
    inverse_view_projection: mat4x4f,
    camera_position: vec4f,
};

struct LightData {
    // direction.xyz + intensity.w
    direction: vec4f,
    // color.rgb + _pad.w
    color: vec4f,
};

@group(0) @binding(0) var<uniform> u_water: WaterUniform;
@group(0) @binding(1) var<uniform> u_camera: CameraData;
@group(0) @binding(2) var<uniform> u_light: LightData;
@group(0) @binding(3) var t_skybox: texture_cube<f32>;
@group(0) @binding(4) var s_skybox: sampler;

struct VertexInput {
    @location(0) position: vec3f,
    @location(1) uv: vec2f,
};

struct VertexOutput {
    @builtin(position) clip_pos: vec4f,
    @location(0) world_pos: vec3f,
    @location(1) uv: vec2f,
    @location(2) @interpolate(flat) normal: vec3f,
};

// ----------------------------------------------------------------
// Wave displacement: 3-layer sine waves, different dirs & freqs.
// ----------------------------------------------------------------
fn wave_height(p: vec3f, time: f32) -> f32 {
    let amp   = u_water.wave.y;
    let freq  = u_water.wave.z;
    let speed = u_water.wave.w;

    let w1 = sin(p.x * freq + time * speed) * amp;
    let w2 = sin(p.z * freq * 0.7 + time * speed * 1.3) * amp * 0.5;
    let w3 = sin((p.x + p.z) * freq * 0.5 + time * speed * 0.8) * amp * 0.25;

    return w1 + w2 + w3;
}

// Partial derivatives of wave_height for normal computation.
fn wave_normal(p: vec3f, time: f32) -> vec3f {
    let amp   = u_water.wave.y;
    let freq  = u_water.wave.z;
    let speed = u_water.wave.w;
    let eps   = 0.01;

    let h  = wave_height(p, time);
    let hx = wave_height(p + vec3f(eps, 0.0, 0.0), time);
    let hz = wave_height(p + vec3f(0.0, 0.0, eps), time);

    let dx = (hx - h) / eps;
    let dz = (hz - h) / eps;

    return normalize(vec3f(-dx, 1.0, -dz));
}

@vertex
fn vs_water(in: VertexInput) -> VertexOutput {
    let time = u_water.fresnel_time.w;
    var out: VertexOutput;

    // Displace vertex Y by wave function.
    var pos = in.position;
    pos.y = u_water.wave.x + wave_height(pos, time); // water_level + displacement

    out.world_pos = pos;
    out.uv = in.uv;
    out.normal = wave_normal(pos, time);
    out.clip_pos = u_camera.view_projection * vec4f(pos, 1.0);

    return out;
}

@fragment
fn fs_water(in: VertexOutput) -> @location(0) vec4f {
    let time      = u_water.fresnel_time.w;
    let cam_pos   = u_camera.camera_position.xyz;
    let water_col = u_water.color_specular.rgb;
    let spec_pow  = u_water.color_specular.w;
    let fres_pow  = u_water.fresnel_time.x;
    let refr_str  = u_water.fresnel_time.y;
    let refl_str  = u_water.fresnel_time.z;

    let N = normalize(in.normal);
    let V = normalize(cam_pos - in.world_pos);
    let L = normalize(-u_light.direction.xyz);
    let light_intensity = u_light.direction.w;
    let light_color     = u_light.color.rgb;

    // ---- Fresnel (Schlick approximation) ----
    let NdotV  = max(dot(N, V), 0.0);
    let fresnel = pow(1.0 - NdotV, fres_pow);
    let fresnel_mix = clamp(fresnel * refl_str, 0.0, 1.0);

    // ---- Reflection: sample skybox along reflected direction ----
    let R = reflect(-V, N);
    let reflection_color = textureSample(t_skybox, s_skybox, R).rgb;

    // ---- Refraction: water body color with depth tinting ----
    // Approximate depth by how far below water_level the original vertex was
    // (0 = surface, deeper = darker).
    let depth_tint = exp(-0.1 * max(u_water.wave.x - in.world_pos.y + 1.0, 0.0));
    let refraction_color = water_col * depth_tint * refr_str;

    // ---- Blend reflection & refraction via Fresnel ----
    let base_color = mix(refraction_color, reflection_color, fresnel_mix);

    // ---- Specular (Blinn-Phong) ----
    let H = normalize(L + V);
    let NdotH = max(dot(N, H), 0.0);
    let spec = pow(NdotH, spec_pow) * light_intensity;
    let specular = light_color * spec;

    // ---- Diffuse (subtle, for underwater caustic feel) ----
    let NdotL = max(dot(N, L), 0.0);
    let diffuse = light_color * NdotL * 0.1;

    let final_color = base_color + specular + diffuse;
    return vec4f(final_color, 0.85); // slight transparency
}
