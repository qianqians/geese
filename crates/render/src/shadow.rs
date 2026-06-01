//! 4.5 阴影 CSM（Cascaded Shadow Map）骨架。
//!
//! 本模块只承担「冷数据计算」与 GPU uniform 描述，不直接调用 wgpu。
//! 后续接入时再编写 shadow pass 与 PCF/PCSS 采样。

use bytemuck::{Pod, Zeroable};

/// CSM 级联数量，硬上限 4（vertex shader 索引为 0..3）。
pub const MAX_CASCADES: usize = 4;

/// 单个 cascade 的视图-投影矩阵 + 远平面（NDC z），按 GPU std140 对齐。
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct CascadeUniform {
    pub view_proj: [[f32; 4]; 4],
    /// xyz = light dir (world)，w = far_plane_view
    pub light_dir_far: [f32; 4],
}

impl CascadeUniform {
    pub fn identity() -> Self {
        Self {
            view_proj: [
                [1.0, 0.0, 0.0, 0.0],
                [0.0, 1.0, 0.0, 0.0],
                [0.0, 0.0, 1.0, 0.0],
                [0.0, 0.0, 0.0, 1.0],
            ],
            light_dir_far: [0.0, -1.0, 0.0, 0.0],
        }
    }
}

/// CSM 所有 cascade 的 GPU uniform（cascade_count 实际有效个数）。
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct CsmUniform {
    pub cascades: [CascadeUniform; MAX_CASCADES],
    /// x = cascade_count, y = atlas_resolution, z = pcf_radius, w = bias
    pub params: [f32; 4],
}

impl Default for CsmUniform {
    fn default() -> Self {
        Self {
            cascades: [CascadeUniform::identity(); MAX_CASCADES],
            params: [0.0, 0.0, 1.0, 0.0005],
        }
    }
}

/// 级联配置。
#[derive(Clone, Debug)]
pub struct CascadeConfig {
    pub count: usize,
    /// 近/远平面（相机空间）
    pub near: f32,
    pub far: f32,
    /// PSSM lambda：0=均匀划分，1=纯对数划分，常用 0.5。
    pub lambda: f32,
    /// 单 cascade 的 shadow map 边长（像素）。
    pub atlas_resolution: u32,
    pub pcf_radius: f32,
    pub depth_bias: f32,
}

impl Default for CascadeConfig {
    fn default() -> Self {
        Self {
            count: 3,
            near: 0.1,
            far: 200.0,
            lambda: 0.5,
            atlas_resolution: 1024,
            pcf_radius: 1.0,
            depth_bias: 0.0005,
        }
    }
}

/// 按 PSSM 算法计算 cascade split 距离（view-space，从近到远，长度=count+1）。
/// 返回 `[near, split1, split2, ..., far]`。
pub fn compute_cascade_splits(cfg: &CascadeConfig) -> Vec<f32> {
    let count = cfg.count.clamp(1, MAX_CASCADES);
    let near = cfg.near.max(1e-4);
    let far = cfg.far.max(near + 1e-3);
    let lambda = cfg.lambda.clamp(0.0, 1.0);
    let ratio = far / near;

    let mut splits = Vec::with_capacity(count + 1);
    splits.push(near);
    for i in 1..count {
        let t = i as f32 / count as f32;
        let log = near * ratio.powf(t);
        let lin = near + (far - near) * t;
        splits.push(lambda * log + (1.0 - lambda) * lin);
    }
    splits.push(far);
    splits
}

/// 单 cascade 的 atlas 区域（在共享 atlas 中的 uv 范围）。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AtlasRect {
    pub offset: [u32; 2],
    pub extent: [u32; 2],
}

/// 计算 atlas 布局：1=单格 / 2=横排 / 3-4=2x2。
pub fn compute_atlas_layout(count: usize, tile: u32) -> Vec<AtlasRect> {
    let mut out = Vec::with_capacity(count);
    match count {
        0 => {}
        1 => out.push(AtlasRect { offset: [0, 0], extent: [tile, tile] }),
        2 => {
            out.push(AtlasRect { offset: [0, 0], extent: [tile, tile] });
            out.push(AtlasRect { offset: [tile, 0], extent: [tile, tile] });
        }
        _ => {
            for i in 0..count.min(4) {
                let x = (i as u32 % 2) * tile;
                let y = (i as u32 / 2) * tile;
                out.push(AtlasRect { offset: [x, y], extent: [tile, tile] });
            }
        }
    }
    out
}

/// 方向光阴影投射器（光方向 + 强度，矩阵在每帧根据相机重算）。
#[derive(Clone, Copy, Debug)]
pub struct DirectionalShadowCaster {
    pub direction: [f32; 3],
    pub intensity: f32,
}

impl DirectionalShadowCaster {
    pub fn new(direction: [f32; 3], intensity: f32) -> Self {
        Self { direction, intensity }
    }
}

/// 后端 trait：M1 仅提供 stub，M2 接入 wgpu shadow pass。
pub trait ShadowAtlas {
    fn cascade_count(&self) -> usize;
    fn upload(&mut self, uniform: &CsmUniform);
}

/// 占位后端：仅记录 upload 次数，用于 pipeline 集成前的单测。
pub struct NullShadowAtlas {
    pub cascade_count: usize,
    pub upload_count: usize,
    pub last_uniform: CsmUniform,
}

impl NullShadowAtlas {
    pub fn new(cascade_count: usize) -> Self {
        Self {
            cascade_count: cascade_count.min(MAX_CASCADES),
            upload_count: 0,
            last_uniform: CsmUniform::default(),
        }
    }
}

impl ShadowAtlas for NullShadowAtlas {
    fn cascade_count(&self) -> usize { self.cascade_count }
    fn upload(&mut self, uniform: &CsmUniform) {
        self.upload_count += 1;
        self.last_uniform = *uniform;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_lambda_zero_is_linear() {
        let cfg = CascadeConfig { count: 3, near: 1.0, far: 100.0, lambda: 0.0, ..Default::default() };
        let s = compute_cascade_splits(&cfg);
        assert_eq!(s.len(), 4);
        assert!((s[1] - 34.0).abs() < 0.5);
        assert!((s[2] - 67.0).abs() < 0.5);
    }

    #[test]
    fn split_lambda_one_is_logarithmic() {
        let cfg = CascadeConfig { count: 3, near: 1.0, far: 1000.0, lambda: 1.0, ..Default::default() };
        let s = compute_cascade_splits(&cfg);
        assert!((s[1] - 10.0).abs() < 0.1);
        assert!((s[2] - 100.0).abs() < 0.1);
    }

    #[test]
    fn split_count_clamped_to_max() {
        let cfg = CascadeConfig { count: 99, near: 0.1, far: 10.0, ..Default::default() };
        let s = compute_cascade_splits(&cfg);
        assert_eq!(s.len(), MAX_CASCADES + 1);
    }

    #[test]
    fn atlas_layout_quad_grid() {
        let rects = compute_atlas_layout(4, 512);
        assert_eq!(rects.len(), 4);
        assert_eq!(rects[0].offset, [0, 0]);
        assert_eq!(rects[3].offset, [512, 512]);
    }

    #[test]
    fn null_atlas_records_upload() {
        let mut atlas = NullShadowAtlas::new(3);
        atlas.upload(&CsmUniform::default());
        atlas.upload(&CsmUniform::default());
        assert_eq!(atlas.upload_count, 2);
        assert_eq!(atlas.cascade_count(), 3);
    }

    #[test]
    fn csm_uniform_size_is_std140_friendly() {
        // 4 个 cascade × (64 矩阵 + 16 vec4) = 320，加 16 字节 params = 336
        assert_eq!(std::mem::size_of::<CsmUniform>(), 80 * MAX_CASCADES + 16);
    }
}
