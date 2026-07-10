//! GPU 粒子系统 — compute shader 驱动的粒子模拟 + 实例化渲染。
//!
//! Feature gate: `particles`（默认禁用）。
//!
//! ## 架构
//! - **Simulation (compute)**: emit → update (velocity/damping/gravity) → recycle
//! - **Rendering (vertex pulling)**: 无 vertex buffer，从 particle storage buffer
//!   读取位置/颜色/大小，vertex shader 生成 camera-facing quad。
//! - **Indirect draw**: compute 阶段写入活跃粒子数到 indirect draw buffer，
//!   渲染阶段无需 CPU readback。

use bytemuck::{Pod, Zeroable};

/// 单个 GPU 粒子（std430 布局，32 bytes）。
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct GpuParticle {
    pub position: [f32; 3],
    pub lifetime: f32,          // 剩余生命（秒），<= 0 = 死
    pub velocity: [f32; 3],
    pub age: f32,               // 已存活时间（秒）
    pub color: [f32; 4],        // RGBA
    pub size: f32,
    pub _pad: [f32; 3],         // align to 32
}

/// 粒子发射器配置（CPU 端参数）。
#[derive(Clone, Debug)]
pub struct ParticleEmitter {
    /// 每秒生成粒子数
    pub birth_rate: f32,
    /// 最大活跃粒子数
    pub max_particles: u32,
    /// 粒子生命周期（秒）
    pub lifetime: f32,
    /// 初始速度范围
    pub velocity_min: [f32; 3],
    pub velocity_max: [f32; 3],
    /// 初始颜色
    pub start_color: [f32; 4],
    pub end_color: [f32; 4],
    /// 初始大小 → 最终大小
    pub start_size: f32,
    pub end_size: f32,
    /// 重力
    pub gravity: [f32; 3],
    /// 速度阻尼（0-1，1 = 无阻尼）
    pub damping: f32,
}

impl Default for ParticleEmitter {
    fn default() -> Self {
        Self {
            birth_rate: 100.0,
            max_particles: 10000,
            lifetime: 2.0,
            velocity_min: [-1.0, 2.0, -1.0],
            velocity_max: [1.0, 5.0, 1.0],
            start_color: [1.0, 0.8, 0.2, 1.0],
            end_color: [1.0, 0.2, 0.0, 0.0],
            start_size: 0.1,
            end_size: 0.5,
            gravity: [0.0, -9.81, 0.0],
            damping: 0.99,
        }
    }
}

/// 上传 GPU 的粒子模拟 uniform（std140 兼容）。
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct ParticleSimUniform {
    /// [birth_rate, lifetime, max_particles_f, dt]
    pub params: [f32; 4],
    pub velocity_min: [f32; 4],
    pub velocity_max: [f32; 4],
    pub start_color: [f32; 4],
    pub end_color: [f32; 4],
    /// [start_size, end_size, damping, _pad]
    pub size_damping: [f32; 4],
    /// [gravity_x, gravity_y, gravity_z, _pad]
    pub gravity: [f32; 4],
    /// [emitter_x, emitter_y, emitter_z, time]
    pub emitter: [f32; 4],
}

impl Default for ParticleSimUniform {
    fn default() -> Self {
        Self {
            params: [100.0, 2.0, 10000.0, 0.016],
            velocity_min: [-1.0, 2.0, -1.0, 0.0],
            velocity_max: [1.0, 5.0, 1.0, 0.0],
            start_color: [1.0, 0.8, 0.2, 1.0],
            end_color: [1.0, 0.2, 0.0, 0.0],
            size_damping: [0.1, 0.5, 0.99, 0.0],
            gravity: [0.0, -9.81, 0.0, 0.0],
            emitter: [0.0, 0.0, 0.0, 0.0],
        }
    }
}

impl ParticleSimUniform {
    pub fn from_emitter(emitter: &ParticleEmitter, emitter_pos: [f32; 3], dt: f32, time: f32) -> Self {
        Self {
            params: [emitter.birth_rate, emitter.lifetime, emitter.max_particles as f32, dt],
            velocity_min: [
                emitter.velocity_min[0],
                emitter.velocity_min[1],
                emitter.velocity_min[2],
                0.0,
            ],
            velocity_max: [
                emitter.velocity_max[0],
                emitter.velocity_max[1],
                emitter.velocity_max[2],
                0.0,
            ],
            start_color: emitter.start_color,
            end_color: emitter.end_color,
            size_damping: [emitter.start_size, emitter.end_size, emitter.damping, 0.0],
            gravity: [emitter.gravity[0], emitter.gravity[1], emitter.gravity[2], 0.0],
            emitter: [emitter_pos[0], emitter_pos[1], emitter_pos[2], time],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gpu_particle_is_64_bytes() {
        assert_eq!(std::mem::size_of::<GpuParticle>(), 64);
    }

    #[test]
    fn particle_sim_uniform_size() {
        // 8 vec4 = 8 × 16 = 128 bytes
        assert_eq!(std::mem::size_of::<ParticleSimUniform>(), 128);
    }

    #[test]
    fn emitter_defaults_are_valid() {
        let e = ParticleEmitter::default();
        assert!(e.birth_rate > 0.0);
        assert!(e.max_particles > 0);
        assert!(e.lifetime > 0.0);
    }
}
