// View-aligned billboard particle renderer.
// Uses instanced rendering: 6 vertices per quad (2 triangles), N instances.

struct CameraUniform {
    view_projection: mat4x4f,
    inverse_view_projection: mat4x4f,
    camera_position: vec4f,
};

struct ParticleInstance {
    position: vec3f,
    color: vec4f,
    size: f32,
};

@group(0) @binding(0) var<uniform> u_camera: CameraUniform;
@group(0) @binding(1) var<storage, read> s_instances: array<ParticleInstance>;

struct VertexOutput {
    @builtin(position) clip_pos: vec4f,
    @location(0) uv: vec2f,
    @location(1) color: vec4f,
};

// Quad vertices: 6 vertices for 2 triangles
// (0,0) bottom-left, (1,1) top-right
fn quad_vertex(index: u32) -> vec2f {
    switch index {
        case 0u: { return vec2f(0.0, 0.0); }
        case 1u: { return vec2f(1.0, 0.0); }
        case 2u: { return vec2f(0.0, 1.0); }
        case 3u: { return vec2f(0.0, 1.0); }
        case 4u: { return vec2f(1.0, 0.0); }
        case 5u: { return vec2f(1.0, 1.0); }
        default: { return vec2f(0.0, 0.0); }
    }
}

@vertex
fn vs_main(
    @builtin(vertex_index) vi: u32,
    @builtin(instance_index) ii: u32,
) -> VertexOutput {
    let instance = s_instances[ii];
    let quad_uv = quad_vertex(vi);

    // Billboard offset: centered on particle position, scaled by size
    // Get camera right and up vectors from inverse view matrix
    let view_matrix = u_camera.inverse_view_projection; // actually need the view matrix itself
    // Extract right and up from view_projection's inverse
    // Right = (col0 of view matrix), Up = (col1 of view matrix)
    // We use the inverse view projection to get world-space camera basis
    let camera_pos = u_camera.camera_position.xyz;

    // Compute view-aligned billboard basis
    let forward = normalize(camera_pos - instance.position);
    let world_up = vec3f(0.0, 1.0, 0.0);
    let right = normalize(cross(world_up, forward));
    let up = cross(forward, right);

    // Quad centered at origin: (-0.5, -0.5) to (0.5, 0.5)
    let offset = (quad_uv - vec2f(0.5)) * instance.size;
    let world_offset = right * offset.x + up * offset.y;

    let world_pos = instance.position + world_offset;
    var out: VertexOutput;
    out.clip_pos = u_camera.view_projection * vec4f(world_pos, 1.0);
    out.uv = quad_uv;
    out.color = instance.color;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4f {
    // Soft circular falloff
    let d = length(in.uv - vec2f(0.5));
    let alpha = smoothstep(0.5, 0.3, d);
    return vec4f(in.color.rgb, in.color.a * alpha);
}
