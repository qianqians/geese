// Shadow depth vertex shader: writes depth only, no fragment output.
// Reuses the same vertex buffer as forward pipeline (position at location 0).

struct ShadowVpUniform {
    view_proj: mat4x4f,
};

struct ShadowObjectUniform {
    model: mat4x4f,
    // Remaining fields of ObjectUniform (normal, skin, joints) are ignored.
    // The GPU binding still covers the full ObjectUniform; WGSL only reads the first field.
};

@group(0) @binding(0) var<uniform> u_vp: ShadowVpUniform;
@group(1) @binding(0) var<uniform> u_object: ShadowObjectUniform;

@vertex
fn vs_main(@location(0) in_pos: vec3f) -> @builtin(position) vec4f {
    return u_vp.view_proj * u_object.model * vec4f(in_pos, 1.0);
}
