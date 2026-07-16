// GPU 粒子渲染 shader — vertex pulling + camera-facing quad。
//
// 功能：无 vertex buffer，从 particle storage buffer 读取每个粒子数据，
//       vertex shader 生成 camera-facing quad（2 triangles = 6 vertices per particle）。
//
// 用法：
//   group 0: camera + frame uniform
//   group 1: particle_buffer (storage, read)
//
// Indirect draw: 使用 `draw_indexed(0..6, 0, 0..active_count)` 绘制所有活跃粒子。

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
};

struct Camera {
    view_projection: mat4x4<f32>,
    inverse_view_projection: mat4x4<f32>,
    camera_position: vec4<f32>,
};

@group(0) @binding(0) var<uniform> camera: Camera;

// Particle data (matches GpuParticle in particle.rs)
struct Particle {
    position: vec3<f32>,
    lifetime: f32,
    velocity: vec3<f32>,
    age: f32,
    color: vec4<f32>,
    size: f32,
    _pad: vec3<f32>,
};

@group(1) @binding(0) var<storage, read> particle_buffer: array<Particle>;

// Quad vertices in local space (XY plane, camera-facing after billboard transform)
var<private> QUAD_VERTICES: array<vec2<f32>, 6> = array<vec2<f32>, 6>(
    vec2<f32>(-1.0, -1.0),
    vec2<f32>( 1.0, -1.0),
    vec2<f32>(-1.0,  1.0),
    vec2<f32>(-1.0,  1.0),
    vec2<f32>( 1.0, -1.0),
    vec2<f32>( 1.0,  1.0),
);

var<private> QUAD_UVS: array<vec2<f32>, 6> = array<vec2<f32>, 6>(
    vec2<f32>(0.0, 0.0),
    vec2<f32>(1.0, 0.0),
    vec2<f32>(0.0, 1.0),
    vec2<f32>(0.0, 1.0),
    vec2<f32>(1.0, 0.0),
    vec2<f32>(1.0, 1.0),
);

@vertex
fn vs_main(
    @builtin(vertex_index) vertex_idx: u32,
    @builtin(instance_index) instance_idx: u32,
) -> VertexOutput {
    let particle = particle_buffer[instance_idx];
    let quad = QUAD_VERTICES[vertex_idx];
    let uv = QUAD_UVS[vertex_idx];

    // Billboard: camera-facing quad in world space
    let cam_right = normalize(vec3<f32>(
        camera.inverse_view_projection[0].x,
        camera.inverse_view_projection[1].x,
        camera.inverse_view_projection[2].x,
    ));
    let cam_up = normalize(vec3<f32>(
        camera.inverse_view_projection[0].y,
        camera.inverse_view_projection[1].y,
        camera.inverse_view_projection[2].y,
    ));

    let world_pos = particle.position
        + cam_right * quad.x * particle.size
        + cam_up * quad.y * particle.size;

    var output: VertexOutput;
    output.clip_position = camera.view_projection * vec4<f32>(world_pos, 1.0);
    output.uv = uv;
    output.color = particle.color;
    return output;
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    // Circular soft particle
    let dist = length(input.uv - vec2<f32>(0.5));
    let alpha = 1.0 - smoothstep(0.4, 0.5, dist);
    return vec4<f32>(input.color.rgb, input.color.a * alpha);
}
