// GPU 粒子模拟 compute shader。
//
// 功能：
//   1. Emit: 按 birth_rate 生成新粒子
//   2. Update: 速度积分 + 重力 + 阻尼 + 生命衰减
//   3. Recycle: 生命周期到期的粒子复位（标记为 dead）
//
// 用法：
//   binding 0: particles (storage, read_write array of GpuParticle)
//   binding 1: dead_list (storage, read_write atomic counter + indices)
//   binding 2: indirect_buffer (storage, read_write: particle_count, instance_count, ...)
//   binding 3: params (ParticleSimUniform)

struct GpuParticle {
    position: vec3<f32>,
    lifetime: f32,
    velocity: vec3<f32>,
    age: f32,
    color: vec4<f32>,
    size: f32,
    _pad: vec3<f32>,
};

struct SimParams {
    params: vec4<f32>,       // birth_rate, lifetime, max_particles, dt
    velocity_min: vec4<f32>,
    velocity_max: vec4<f32>,
    start_color: vec4<f32>,
    end_color: vec4<f32>,
    size_damping: vec4<f32>, // start_size, end_size, damping, _
    gravity: vec4<f32>,
    emitter: vec4<f32>,      // emitter_x, emitter_y, emitter_z, time
};

@group(0) @binding(0) var<storage, read_write> particles: array<GpuParticle>;
@group(0) @binding(1) var<storage, read_write> dead_list: array<u32>;  // [count, idx0, idx1, ...]
@group(0) @binding(2) var<storage, read_write> indirect: array<u32>;   // [vertex_count, instance_count, first_vertex, first_instance]
@group(0) @binding(3) var<uniform> params: SimParams;

// 伪随机数生成器 (PCG)
fn rand(seed: u32) -> u32 {
    var state = seed * 747796405u + 2891336453u;
    var word = ((state >> ((state >> 28u) + 4u)) ^ state) * 277803737u;
    return (word >> 22u) ^ word;
}

fn rand_f32(seed: u32) -> f32 {
    return f32(rand(seed)) / 4294967295.0;
}

fn rand_vec3(seed: u32) -> vec3<f32> {
    return vec3<f32>(rand_f32(seed), rand_f32(seed + 1u), rand_f32(seed + 2u));
}

@compute @workgroup_size(64, 1, 1)
fn cs_main(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let idx = global_id.x;
    let max_particles = u32(params.params.z);
    if (idx >= max_particles) {
        return;
    }

    let dt = params.params.w;
    let particle = particles[idx];

    // ---- 活着：更新 ---- 
    if (particle.lifetime > 0.0) {
        // 重力 + 阻尼
        var vel = particle.velocity + params.gravity.xyz * dt;
        vel = vel * params.size_damping.z; // damping
        var pos = particle.position + vel * dt;
        var life = particle.lifetime - dt;
        var age = particle.age + dt;

        // 颜色插值 (基于 age / total_lifetime)
        let total_life = params.params.y; // lifetime param
        let t = clamp(age / total_life, 0.0, 1.0);
        var col = mix(params.start_color, params.end_color, vec4<f32>(t));

        // 大小插值
        let sz = mix(params.size_damping.x, params.size_damping.y, t);

        if (life <= 0.0) {
            // 死亡：写入 dead_list
            let dead_idx = atomicAdd(&dead_list[0], 1u) + 1u;
            if (dead_idx <= max_particles) {
                dead_list[dead_idx] = idx;
            }
            life = 0.0;
        }

        particles[idx].position = pos;
        particles[idx].velocity = vel;
        particles[idx].lifetime = life;
        particles[idx].age = age;
        particles[idx].color = col;
        particles[idx].size = sz;
    }
    // ---- 死：尝试从 dead_list 分配（由 emit 逻辑处理，此处跳过） ----
}
