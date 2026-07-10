// Irradiance convolution compute shader: convolve a cubemap against hemisphere.
// Output: 2D storage texture (face per dispatch; 6 faces dispatched separately)
// 
// Simplified: generates a procedural sky cubemap and convolves it.
// For SolidColor input, the irradiance is just the color itself.

struct Params {
    face_size: u32,
    sample_count: u32,
    face_index: u32,
    _pad: u32,
    sun_direction: vec4f,
    ground_albedo: vec4f,
    sky_color: vec4f,
    horizon_color: vec4f,
    ground_color: vec4f,
};

@group(0) @binding(0) var<uniform> params: Params;
@group(0) @binding(1) var out_tex: texture_storage_2d<rgba16float, write>;

const PI: f32 = 3.141592653589793238;

// Direction from face index and texel coords (equirectangular mapping per face)
fn sample_direction(face: u32, u: f32, v: f32) -> vec3f {
    // Map u,v in [0,1] to [-1,1]
    let a = 2.0 * u - 1.0;
    let b = 2.0 * v - 1.0;
    switch face {
        case 0u: { return vec3f(1.0, b, -a); }   // +X
        case 1u: { return vec3f(-1.0, b, a); }   // -X
        case 2u: { return vec3f(a, 1.0, -b); }   // +Y
        case 3u: { return vec3f(a, -1.0, b); }    // -Y
        case 4u: { return vec3f(a, b, 1.0); }    // +Z
        case 5u: { return vec3f(-a, b, -1.0); }  // -Z
        default: { return vec3f(0.0, 1.0, 0.0); }
    }
}

// Procedural sky color (simplified atmospheric scattering)
fn procedural_sky(dir: vec3f) -> vec3f {
    let up = max(dir.y, 0.0);
    let sun_dir = normalize(params.sun_direction.xyz);
    let sun_dot = max(dot(dir, sun_dir), 0.0);

    // Sky gradient
    let sky = mix(params.horizon_color.xyz, params.sky_color.xyz, pow(up, 0.5));

    // Sun glow
    let sun_glow = pow(sun_dot, 64.0) * vec3f(1.0, 0.8, 0.4) * 2.0;
    let sun_halo = pow(sun_dot, 8.0) * vec3f(1.0, 0.7, 0.3) * 0.3;

    // Ground
    let ground = mix(params.horizon_color.xyz, params.ground_color.xyz, max(-dir.y, 0.0));

    let result = select(sky, ground, dir.y < 0.0) + sun_glow + sun_halo;
    return result;
}

@compute @workgroup_size(8, 8, 1)
fn cs_main(@builtin(global_invocation_id) gid: vec3u) {
    let size = params.face_size;
    if (gid.x >= size || gid.y >= size) {
        return;
    }

    let u = (f32(gid.x) + 0.5) / f32(size);
    let v = (f32(gid.y) + 0.5) / f32(size);

    // Normal direction for this texel
    let normal = normalize(sample_direction(params.face_index, u, v));

    // Build TBN
    let up = select(vec3f(1.0, 0.0, 0.0), vec3f(0.0, 1.0, 0.0), abs(normal.z) < 0.999);
    let t = normalize(cross(up, normal));
    let b = cross(normal, t);

    var irradiance = vec3f(0.0);
    var sample_weight = 0.0;

    let sample_count = params.sample_count;
    let inv_samples = 1.0 / f32(sample_count);
    let inv_pi = 1.0 / PI;

    for (var i = 0u; i < sample_count; i++) {
        // Fibonacci spiral sampling on hemisphere
        let fi = f32(i);
        let phi = fi * 2.39996323 * inv_samples; // golden angle
        let cos_theta = 1.0 - (fi + 0.5) * 2.0 * inv_samples;
        let sin_theta = sqrt(1.0 - cos_theta * cos_theta);

        // Tangent space direction
        let tangent = vec3f(
            sin_theta * cos(phi),
            sin_theta * sin(phi),
            cos_theta,
        );

        let wi = normalize(t * tangent.x + b * tangent.y + normal * tangent.z);
        let ndotl = max(dot(normal, wi), 0.0);

        if (ndotl > 0.0) {
            let color = procedural_sky(wi);
            irradiance += color * ndotl;
            sample_weight += 1.0;
        }
    }

    irradiance *= PI * inv_samples;

    textureStore(out_tex, gid.xy, vec4f(irradiance, 1.0));
}
