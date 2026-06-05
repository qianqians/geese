//! 4.11 状态同步与插值骨架。
//!
//! 提供：
//! - `Snapshot`：单实体单时刻的位置/旋转/速度快照
//! - `SnapshotBuffer`：环形快照缓冲（容量受限），便于按时间戳插值
//! - `InterpolationMode`：滞后插值 / 外推
//! - `sample()`：按给定渲染时刻插值出当前帧状态
//! - `predict_position()`：纯函数，运动外推（适合简易 client-side prediction）
//!
//! 该模块只承担算法层，不耦合具体网络协议。

use std::collections::VecDeque;

pub type EntityId = u64;

/// 单实体单时刻的同步快照。
#[derive(Clone, Copy, Debug)]
pub struct Snapshot {
    /// 服务端时间戳（秒）。
    pub server_time: f64,
    pub position: [f32; 3],
    pub velocity: [f32; 3],
    /// 四元数 (x,y,z,w)。
    pub rotation: [f32; 4],
}

impl Snapshot {
    pub fn new(time: f64, pos: [f32; 3]) -> Self {
        Self { server_time: time, position: pos, velocity: [0.0; 3], rotation: [0.0, 0.0, 0.0, 1.0] }
    }
}

/// 插值模式。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InterpolationMode {
    /// Source Engine 模式：渲染时间 = 当前时间 - interp_delay，插两个历史包之间。
    Lagged,
    /// 速度外推：超出最新包时间用 v*dt 推算（适合短暂丢包补偿）。
    Extrapolated,
}

/// 实体的环形快照缓冲。
pub struct SnapshotBuffer {
    capacity: usize,
    buffer: VecDeque<Snapshot>,
}

impl SnapshotBuffer {
    pub fn new(capacity: usize) -> Self {
        Self { capacity: capacity.max(2), buffer: VecDeque::with_capacity(capacity.max(2)) }
    }

    pub fn push(&mut self, s: Snapshot) {
        // 保持按 server_time 升序;乱序包丢弃
        if let Some(back) = self.buffer.back() {
            if s.server_time <= back.server_time { return; }
        }
        if self.buffer.len() == self.capacity {
            self.buffer.pop_front();
        }
        self.buffer.push_back(s);
    }

    pub fn len(&self) -> usize { self.buffer.len() }
    pub fn is_empty(&self) -> bool { self.buffer.is_empty() }
    pub fn latest(&self) -> Option<&Snapshot> { self.buffer.back() }
    pub fn oldest(&self) -> Option<&Snapshot> { self.buffer.front() }

    /// 在指定时间点采样（线性插值）。
    pub fn sample(&self, render_time: f64, mode: InterpolationMode) -> Option<Snapshot> {
        if self.buffer.is_empty() { return None; }
        if self.buffer.len() == 1 { return self.buffer.front().copied(); }

        // 找到夹住 render_time 的相邻两帧
        for win in self.buffer.as_slices().0.windows(2)
            .chain(self.buffer.as_slices().1.windows(2))
        {
            let a = win[0];
            let b = win[1];
            if a.server_time <= render_time && render_time <= b.server_time {
                let span = (b.server_time - a.server_time).max(1e-6);
                let t = ((render_time - a.server_time) / span).clamp(0.0, 1.0) as f32;
                return Some(interpolate(&a, &b, t, render_time));
            }
        }

        // 超出范围处理
        let latest = *self.buffer.back().unwrap();
        if render_time > latest.server_time {
            match mode {
                InterpolationMode::Lagged => Some(latest),
                InterpolationMode::Extrapolated => {
                    let dt = (render_time - latest.server_time) as f32;
                    Some(Snapshot {
                        server_time: render_time,
                        position: predict_position(latest.position, latest.velocity, dt),
                        velocity: latest.velocity,
                        rotation: latest.rotation,
                    })
                }
            }
        } else {
            Some(*self.buffer.front().unwrap())
        }
    }
}

/// 两个快照之间线性插值（位置 lerp + 旋转 nlerp 近似）。
pub fn interpolate(a: &Snapshot, b: &Snapshot, t: f32, render_time: f64) -> Snapshot {
    let t = t.clamp(0.0, 1.0);
    let pos = [
        a.position[0] + (b.position[0] - a.position[0]) * t,
        a.position[1] + (b.position[1] - a.position[1]) * t,
        a.position[2] + (b.position[2] - a.position[2]) * t,
    ];
    let vel = [
        a.velocity[0] + (b.velocity[0] - a.velocity[0]) * t,
        a.velocity[1] + (b.velocity[1] - a.velocity[1]) * t,
        a.velocity[2] + (b.velocity[2] - a.velocity[2]) * t,
    ];
    let mut rot = [
        a.rotation[0] + (b.rotation[0] - a.rotation[0]) * t,
        a.rotation[1] + (b.rotation[1] - a.rotation[1]) * t,
        a.rotation[2] + (b.rotation[2] - a.rotation[2]) * t,
        a.rotation[3] + (b.rotation[3] - a.rotation[3]) * t,
    ];
    let l = (rot[0]*rot[0] + rot[1]*rot[1] + rot[2]*rot[2] + rot[3]*rot[3]).sqrt();
    if l > 1e-6 {
        for v in &mut rot { *v /= l; }
    } else {
        rot = [0.0, 0.0, 0.0, 1.0];
    }
    Snapshot { server_time: render_time, position: pos, velocity: vel, rotation: rot }
}

/// 简单速度外推：pos + v * dt。
pub fn predict_position(pos: [f32; 3], vel: [f32; 3], dt: f32) -> [f32; 3] {
    [pos[0] + vel[0] * dt, pos[1] + vel[1] * dt, pos[2] + vel[2] * dt]
}

/// 推荐的渲染时间偏移（滞后插值），由网络抖动 + 一两个 tick 间隔决定。
pub fn recommended_interp_delay(tick_hz: f32) -> f64 {
    if tick_hz <= 0.0 { 0.1 } else { (2.0 / tick_hz as f64).max(0.05) }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snap(t: f64, x: f32) -> Snapshot {
        let mut s = Snapshot::new(t, [x, 0.0, 0.0]);
        s.velocity = [1.0, 0.0, 0.0];
        s
    }

    #[test]
    fn empty_buffer_returns_none() {
        let b = SnapshotBuffer::new(8);
        assert!(b.sample(1.0, InterpolationMode::Lagged).is_none());
    }

    #[test]
    fn out_of_order_packets_dropped() {
        let mut b = SnapshotBuffer::new(8);
        b.push(snap(1.0, 0.0));
        b.push(snap(0.5, 99.0)); // 应被丢
        assert_eq!(b.len(), 1);
        assert_eq!(b.latest().unwrap().position[0], 0.0);
    }

    #[test]
    fn ring_capacity_drops_oldest() {
        let mut b = SnapshotBuffer::new(3);
        for i in 0..5 {
            b.push(snap(i as f64, i as f32));
        }
        assert_eq!(b.len(), 3);
        assert_eq!(b.oldest().unwrap().position[0], 2.0);
        assert_eq!(b.latest().unwrap().position[0], 4.0);
    }

    #[test]
    fn interpolation_at_midpoint() {
        let mut b = SnapshotBuffer::new(4);
        b.push(snap(0.0, 0.0));
        b.push(snap(1.0, 10.0));
        let s = b.sample(0.5, InterpolationMode::Lagged).unwrap();
        assert!((s.position[0] - 5.0).abs() < 1e-4);
    }

    #[test]
    fn lagged_beyond_latest_returns_latest() {
        let mut b = SnapshotBuffer::new(4);
        b.push(snap(0.0, 0.0));
        b.push(snap(1.0, 10.0));
        let s = b.sample(5.0, InterpolationMode::Lagged).unwrap();
        assert!((s.position[0] - 10.0).abs() < 1e-4);
    }

    #[test]
    fn extrapolated_beyond_latest_uses_velocity() {
        let mut b = SnapshotBuffer::new(4);
        b.push(snap(0.0, 0.0));
        b.push(snap(1.0, 10.0));
        // 最新包 t=1, x=10, v=1，外推 0.5s 后应为 10.5
        let s = b.sample(1.5, InterpolationMode::Extrapolated).unwrap();
        assert!((s.position[0] - 10.5).abs() < 1e-4);
    }

    #[test]
    fn predict_position_is_pure() {
        assert_eq!(predict_position([1.0, 2.0, 3.0], [10.0, 0.0, 0.0], 0.5), [6.0, 2.0, 3.0]);
    }

    #[test]
    fn recommended_delay_scales_with_tick_rate() {
        let d20 = recommended_interp_delay(20.0);
        let d60 = recommended_interp_delay(60.0);
        assert!(d20 > d60);
        assert!(d60 >= 0.05);
    }

    #[test]
    fn interpolate_normalizes_quaternion() {
        let mut a = snap(0.0, 0.0); a.rotation = [0.0, 0.0, 0.0, 2.0];
        let mut b = snap(1.0, 0.0); b.rotation = [0.0, 0.0, 0.0, 4.0];
        let s = interpolate(&a, &b, 0.5, 0.5);
        let l = s.rotation.iter().map(|v| v*v).sum::<f32>().sqrt();
        assert!((l - 1.0).abs() < 1e-5);
    }
}
