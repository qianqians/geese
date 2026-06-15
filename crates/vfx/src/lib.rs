//! 4.8 粒子系统骨架（CPU 端模拟 + Billboard/Mesh 数据结构）。
//!
//! 提供：
//! - `Particle`：CPU 端粒子状态（位置/速度/寿命/颜色/尺寸）
//! - `EmitterShape`：发射器形状（Point/Sphere/Cone/Box）
//! - `Emitter`：发射器（emit rate + lifetime + 初速度范围）
//! - `ParticleSystem`：聚合多个 emitter，按 dt 推进
//! - `BillboardKind`：Billboard 朝向模式
//!
//! 不依赖 wgpu，仅做模拟;后续接入时把粒子缓冲上传到 vertex/instance buffer。

use std::ops::Range;

/// 单个粒子的运行时状态。
#[derive(Clone, Copy, Debug)]
pub struct Particle {
    pub position: [f32; 3],
    pub velocity: [f32; 3],
    /// 已存活时间（秒）。
    pub age: f32,
    /// 总寿命（秒），<=0 视为已死亡。
    pub lifetime: f32,
    pub color: [f32; 4],
    pub size: f32,
    /// 所属 emitter 在 ParticleSystem.emitters 中的索引。
    pub emitter_index: usize,
}

impl Particle {
    pub fn is_alive(&self) -> bool { self.age < self.lifetime && self.lifetime > 0.0 }
    /// 寿命比例 0..=1。
    pub fn life_t(&self) -> f32 {
        if self.lifetime <= 0.0 { 1.0 } else { (self.age / self.lifetime).clamp(0.0, 1.0) }
    }
}

/// 发射器形状（决定生成位置与初速度方向）。
#[derive(Clone, Copy, Debug)]
pub enum EmitterShape {
    Point,
    Sphere { radius: f32 },
    Cone { half_angle_deg: f32 },
    Box { extents: [f32; 3] },
}

/// Billboard 朝向模式。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BillboardKind {
    /// 始终面向相机。
    ViewAligned,
    /// 围绕 Y 轴面向相机（适合树叶、火焰）。
    AxisLocked,
    /// 速度方向（适合 trail）。
    VelocityAligned,
    /// Mesh 粒子（不做 billboard）。
    Mesh,
}

/// 发射器定义。
#[derive(Clone, Debug)]
pub struct Emitter {
    pub origin: [f32; 3],
    pub shape: EmitterShape,
    pub billboard: BillboardKind,
    /// 每秒发射数量。
    pub rate_per_second: f32,
    /// 初速度大小范围。
    pub speed: Range<f32>,
    /// 粒子寿命范围。
    pub lifetime: Range<f32>,
    pub start_color: [f32; 4],
    pub end_color: [f32; 4],
    pub start_size: f32,
    pub end_size: f32,
    pub gravity: [f32; 3],
    /// 同时存活上限。
    pub max_particles: usize,
    /// 累积的「待发射」分数（内部状态）。
    emit_accumulator: f32,
}

impl Emitter {
    pub fn new(origin: [f32; 3], rate: f32, lifetime: f32) -> Self {
        Self {
            origin,
            shape: EmitterShape::Point,
            billboard: BillboardKind::ViewAligned,
            rate_per_second: rate,
            speed: 0.5..1.5,
            lifetime: (lifetime * 0.8)..(lifetime * 1.2),
            start_color: [1.0; 4],
            end_color: [1.0, 1.0, 1.0, 0.0],
            start_size: 0.1,
            end_size: 0.1,
            gravity: [0.0, -9.81, 0.0],
            max_particles: 1024,
            emit_accumulator: 0.0,
        }
    }
}

/// 简易 LCG 伪随机（避免引入 rand 依赖）。
#[derive(Clone, Copy, Debug)]
pub struct Rng { state: u32 }

impl Rng {
    pub fn new(seed: u32) -> Self { Self { state: if seed == 0 { 0xdeadbeef } else { seed } } }
    pub fn next_u32(&mut self) -> u32 {
        // Lehmer / MINSTD: state = state * 48271 mod (2^31 - 1)
        let prod = (self.state as u64).wrapping_mul(48271);
        self.state = ((prod & 0x7fffffff) + (prod >> 31)) as u32;
        if self.state >= 0x7fffffff { self.state -= 0x7fffffff; }
        self.state
    }
    pub fn next_f32(&mut self) -> f32 {
        (self.next_u32() as f32) / (0x7fffffff as f32)
    }
    pub fn range(&mut self, r: Range<f32>) -> f32 {
        r.start + (r.end - r.start) * self.next_f32()
    }
    pub fn unit_vec3(&mut self) -> [f32; 3] {
        // 球面均匀采样
        let z = self.next_f32() * 2.0 - 1.0;
        let phi = self.next_f32() * std::f32::consts::TAU;
        let r = (1.0 - z * z).max(0.0).sqrt();
        [r * phi.cos(), r * phi.sin(), z]
    }
}

/// 粒子系统：管理一组发射器与活粒子池。
pub struct ParticleSystem {
    pub emitters: Vec<Emitter>,
    pub particles: Vec<Particle>,
    rng: Rng,
}

impl ParticleSystem {
    pub fn new(seed: u32) -> Self {
        Self { emitters: Vec::new(), particles: Vec::new(), rng: Rng::new(seed) }
    }
    pub fn add_emitter(&mut self, e: Emitter) -> usize {
        self.emitters.push(e);
        self.emitters.len() - 1
    }
    pub fn alive_count(&self) -> usize { self.particles.iter().filter(|p| p.is_alive()).count() }

    /// 按 dt 推进：发射 + 物理积分 + 颜色尺寸插值 + 移除过期粒子。
    pub fn tick(&mut self, dt: f32) {
        // 1. 物理积分 + 颜色/尺寸插值
        for p in &mut self.particles {
            if !p.is_alive() { continue; }
            p.age += dt;
        }
        // 2. 移除过期粒子（保持顺序）
        self.particles.retain(|p| p.is_alive());
        // 3. 发射
        let emitters_len = self.emitters.len();
        for i in 0..emitters_len {
            let to_emit = {
                let e = &mut self.emitters[i];
                e.emit_accumulator += e.rate_per_second * dt;
                let n = e.emit_accumulator.floor() as usize;
                e.emit_accumulator -= n as f32;
                let budget = e.max_particles.saturating_sub(
                    self.particles.iter().filter(|p| p.is_alive()).count(),
                );
                n.min(budget)
            };
            for _ in 0..to_emit {
                let p = Self::spawn_one(&self.emitters[i], i, &mut self.rng);
                self.particles.push(p);
            }
        }
        // 4. 位置积分（用更新后的 dt）
        for p in &mut self.particles {
            // 每个粒子使用所属 emitter 的重力；若 emitter 已被移除则无额外重力
            let gravity = self.emitters
                .get(p.emitter_index)
                .map(|e| e.gravity)
                .unwrap_or([0.0; 3]);
            for k in 0..3 {
                p.velocity[k] += gravity[k] * dt;
                p.position[k] += p.velocity[k] * dt;
            }
        }
    }

    fn spawn_one(e: &Emitter, emitter_index: usize, rng: &mut Rng) -> Particle {
        let mut pos = e.origin;
        let dir = match e.shape {
            EmitterShape::Point => rng.unit_vec3(),
            EmitterShape::Sphere { radius } => {
                let d = rng.unit_vec3();
                let r = rng.next_f32() * radius;
                for k in 0..3 { pos[k] += d[k] * r; }
                d
            }
            EmitterShape::Cone { half_angle_deg } => {
                let cos_max = half_angle_deg.to_radians().cos();
                let z = cos_max + (1.0 - cos_max) * rng.next_f32();
                let phi = rng.next_f32() * std::f32::consts::TAU;
                let r = (1.0 - z * z).max(0.0).sqrt();
                [r * phi.cos(), r * phi.sin(), z]
            }
            EmitterShape::Box { extents } => {
                for k in 0..3 {
                    pos[k] += (rng.next_f32() * 2.0 - 1.0) * extents[k];
                }
                rng.unit_vec3()
            }
        };
        let speed = rng.range(e.speed.clone());
        let lifetime = rng.range(e.lifetime.clone()).max(1e-3);
        Particle {
            position: pos,
            velocity: [dir[0] * speed, dir[1] * speed, dir[2] * speed],
            age: 0.0,
            lifetime,
            color: e.start_color,
            size: e.start_size,
            emitter_index,
        }
    }
}

/// 颜色/尺寸的线性插值（按 life_t）。
pub fn lerp_color(start: [f32; 4], end: [f32; 4], t: f32) -> [f32; 4] {
    let t = t.clamp(0.0, 1.0);
    [
        start[0] + (end[0] - start[0]) * t,
        start[1] + (end[1] - start[1]) * t,
        start[2] + (end[2] - start[2]) * t,
        start[3] + (end[3] - start[3]) * t,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rng_is_deterministic_and_in_range() {
        let mut a = Rng::new(42);
        let mut b = Rng::new(42);
        for _ in 0..16 {
            let va = a.next_f32();
            let vb = b.next_f32();
            assert!((va - vb).abs() < 1e-7);
            assert!(va >= 0.0 && va <= 1.0);
        }
    }

    #[test]
    fn rng_unit_vec_is_unit_length() {
        let mut r = Rng::new(7);
        for _ in 0..16 {
            let v = r.unit_vec3();
            let l = (v[0]*v[0] + v[1]*v[1] + v[2]*v[2]).sqrt();
            assert!((l - 1.0).abs() < 1e-3);
        }
    }

    #[test]
    fn particle_lifecycle_marks_dead() {
        let mut p = Particle {
            position: [0.0; 3], velocity: [0.0; 3], age: 0.0, lifetime: 1.0,
            color: [1.0; 4], size: 1.0,
            emitter_index: 0,
        };
        assert!(p.is_alive());
        p.age = 1.1;
        assert!(!p.is_alive());
    }

    #[test]
    fn system_emits_then_decays() {
        let mut sys = ParticleSystem::new(1);
        // rate=10/s lifetime=0.5s，1 秒后应稳定在 ~5 个
        sys.add_emitter(Emitter::new([0.0; 3], 10.0, 0.5));
        for _ in 0..60 { sys.tick(1.0 / 60.0); }
        let n = sys.alive_count();
        assert!(n > 0 && n <= 12, "alive={n}");
    }

    #[test]
    fn system_respects_max_particles() {
        let mut e = Emitter::new([0.0; 3], 1_000_000.0, 10.0);
        e.max_particles = 8;
        let mut sys = ParticleSystem::new(2);
        sys.add_emitter(e);
        for _ in 0..10 { sys.tick(1.0 / 60.0); }
        assert!(sys.alive_count() <= 8);
    }

    #[test]
    fn lerp_color_endpoints() {
        let a = [1.0, 0.0, 0.0, 1.0];
        let b = [0.0, 0.0, 1.0, 0.0];
        assert_eq!(lerp_color(a, b, 0.0), a);
        assert_eq!(lerp_color(a, b, 1.0), b);
        let m = lerp_color(a, b, 0.5);
        assert!((m[0] - 0.5).abs() < 1e-5);
        assert!((m[2] - 0.5).abs() < 1e-5);
        assert!((m[3] - 0.5).abs() < 1e-5);
    }

    #[test]
    fn billboard_kind_equality() {
        assert_eq!(BillboardKind::ViewAligned, BillboardKind::ViewAligned);
        assert_ne!(BillboardKind::ViewAligned, BillboardKind::Mesh);
    }
}
