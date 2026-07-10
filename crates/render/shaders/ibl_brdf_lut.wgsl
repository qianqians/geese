// BRDF LUT compute shader: generates 2D lookup table for split-sum approximation.
// Input: uniform with sample count
// Output: storage texture (Rgba16Float), R = specular scale, G = bias, B = 0, A = 1

struct Params {
    sample_count: u32,
    resolution: u32,
    _pad: u32,
    _pad2: u32,
};

@group(0) @binding(0) var<uniform> params: Params;
@group(0) @binding(1) var out_tex: texture_storage_2d<rgba16float, write>;

// Van der Corput radical inverse
fn radical_inverse_vdc(bits: u32) -> f32 {
    var b = bits;
    b = (b << 16u) | (b >> 16u);
    b = ((b & 0x55555555u) << 1u) | ((b & 0xAAAAAAAAu) >> 1u);
    b = ((b & 0x33333333u) << 2u) | ((b & 0xCCCCCCCCu) >> 2u);
    b = ((b & 0x0F0F0F0Fu) << 4u) | ((b & 0xF0F0F0F0u) >> 4u);
    b = ((b & 0x00FF00FFu) << 8u) | ((b & 0xFF00FF00u) >> 8u);
    return f32(b) * 2.3283064365386963e-10; // 1 / 2^32
}

// Hammersley sequence: returns (xi1, xi2) for index i in [0, N)
fn hammersley(i: u32, n: u32) -> vec2f {
    return vec2f(f32(i) / f32(n), radical_inverse_vdc(i));
}

// GGX NDF
fn distribution_ggx(n: vec3f, h: vec3f, roughness: f32) -> f32 {
    let a = roughness * roughness;
    let a2 = a * a;
    let ndoth = max(dot(n, h), 0.0);
    let ndoth2 = ndoth * ndoth;
    let denom = ndoth2 * (a2 - 1.0) + 1.0;
    return a2 / max(3.14159265 * denom * denom, 1e-7);
}

// Smith geometry (combined Schlick-GGX for V and L)
fn geometry_smith(n: vec3f, v: vec3f, l: vec3f, roughness: f32) -> f32 {
    let r1 = max(roughness, 0.0);
    let a = r1 * r1;
    let a2 = a * a;
    let ndotv = max(dot(n, v), 0.0);
    let ndotl = max(dot(n, l), 0.0);
    let ggx_v = ndotv * (1.0 - a2) + a2;
    let ggx_l = ndotl * (1.0 - a2) + a2;
    return 0.5 / max(ggx_v * ggx_l, 1e-7);
}

// Generate a tangent-space sample direction from Hammersley (xi1, xi2)
fn importance_sample_ggxi(xi: vec2f, n: vec3f, roughness: f32) -> vec3f {
    let a = max(roughness, 0.0);
    let phi = 2.0 * 3.14159265 * xi.x;
    let cos_theta = sqrt((1.0 - xi.y) / (1.0 + (a * a - 1.0) * xi.y));
    let sin_theta = sqrt(1.0 - cos_theta * cos_theta);
    let h = vec3f(
        sin_theta * cos(phi),
        sin_theta * sin(phi),
        cos_theta,
    );
    // Build TBN from N
    let up = select(vec3f(1.0, 0.0, 0.0), vec3f(0.0, 1.0, 0.0), abs(n.z) < 0.999);
    let t = normalize(cross(up, n));
    let b = cross(n, t);
    return normalize(t * h.x + b * h.y + n * h.z);
}

@compute @workgroup_size(8, 8, 1)
fn cs_main(@builtin(global_invocation_id) gid: vec3u) {
    let res = params.resolution;
    if (gid.x >= res || gid.y >= res) {
        return;
    }

    // Map texel coords to NdotV and roughness
    let ndotv = clamp(f32(gid.x) / f32(res - 1u), 0.0, 1.0);
    let roughness = clamp(f32(gid.y) / f32(res - 1u), 0.0, 1.0);

    let n = vec3f(0.0, 0.0, 1.0);
    let v = vec3f(
        sqrt(1.0 - ndotv * ndotv),
        0.0,
        ndotv,
    );

    var scale = 0.0;
    var bias = 0.0;

    for (var i = 0u; i < params.sample_count; i++) {
        let xi = hammersley(i, params.sample_count);
        let h = importance_sample_ggxi(xi, n, roughness);
        let l = normalize(2.0 * dot(v, h) * h - v);

        let ndotl = max(l.z, 0.0);
        let ndoth = max(h.z, 0.0);
        let vdoth = max(dot(v, h), 0.0);

        if (ndotl > 0.0) {
            let g = geometry_smith(n, v, l, roughness);
            let g_vis = g * vdoth / max(ndoth * ndotv, 1e-7);
            scale += g_vis;
            bias += g_vis * (1.0 - vdoth) / max(ndoth, 1e-7);
        }
    }

    let inv_n = 1.0 / f32(params.sample_count);
    let result = vec4f(scale * inv_n, bias * inv_n, 0.0, 1.0);
    textureStore(out_tex, gid.xy, result);
}
