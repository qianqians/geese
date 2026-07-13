//! 贴花系统（Decal）。
//!
//! 提供屏幕空间投影贴花渲染：根据相机视角将贴花投影到场景中，
//! 仅在贴花位置与场景深度匹配时渲染（深度测试）。
//!
//! 设计要点：
//! - [`Decal`]：单个贴花实例，包含位置、朝向、尺寸、纹理和颜色。
//! - [`DecalSystem`]：管理所有贴花，限制最大数量（默认 256）。
//! - [`project_decal`]：屏幕空间投影，根据相机 VP 矩阵将贴花变换到裁剪空间。
//! - [`depth_test_decal`]：深度测试，判断贴花是否与场景深度匹配。

use cgmath::{Matrix4, SquareMatrix, Vector3, Vector4};

use crate::material::TextureHandle;

/// 单个贴花实例。
#[derive(Debug, Clone)]
pub struct Decal {
    /// 世界空间位置。
    pub position: [f32; 3],
    /// 朝向（四元数 xyzw）。
    pub orientation: [f32; 4],
    /// 尺寸（宽, 高, 深）。
    pub size: [f32; 3],
    /// 纹理句柄。
    pub texture_handle: TextureHandle,
    /// 颜色叠加（RGBA）。
    pub color: [f32; 4],
}

impl Default for Decal {
    fn default() -> Self {
        Self {
            position: [0.0; 3],
            orientation: [0.0, 0.0, 0.0, 1.0],
            size: [1.0, 1.0, 1.0],
            texture_handle: TextureHandle(0),
            color: [1.0, 1.0, 1.0, 1.0],
        }
    }
}

/// 贴花投影结果（裁剪空间中的 AABB + 深度范围）。
#[derive(Debug, Clone, Copy)]
pub struct DecalProjection {
    /// 裁剪空间中心（NDC xyz）。
    pub ndc_center: [f32; 3],
    /// 裁剪空间半尺寸（NDC）。
    pub ndc_half_size: [f32; 3],
    /// 深度范围 [near, far]。
    pub depth_range: [f32; 2],
    /// 是否在视锥体内。
    pub visible: bool,
}

/// 深度测试结果。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DepthTestResult {
    /// 贴花深度与场景深度匹配，应渲染。
    Pass,
    /// 贴花在场景表面前方，不渲染。
    InFront,
    /// 贴花在场景表面后方（被遮挡），不渲染。
    Behind,
}

/// 贴花系统：管理所有活跃贴花，限制最大数量。
pub struct DecalSystem {
    decals: Vec<(u64, Decal)>,
    next_id: u64,
    max_decals: usize,
}

/// 默认最大贴花数量。
pub const DEFAULT_MAX_DECALS: usize = 256;

impl Default for DecalSystem {
    fn default() -> Self {
        Self {
            decals: Vec::new(),
            next_id: 0,
            max_decals: DEFAULT_MAX_DECALS,
        }
    }
}

impl DecalSystem {
    /// 创建指定最大数量的贴花系统。
    pub fn new(max_decals: usize) -> Self {
        Self {
            decals: Vec::with_capacity(max_decals.min(1024)),
            next_id: 0,
            max_decals,
        }
    }

    /// 添加一个贴花，返回贴花 ID。
    ///
    /// 如果已达到最大数量，移除最早的贴花后再添加。
    pub fn add_decal(&mut self, decal: Decal) -> u64 {
        if self.decals.len() >= self.max_decals {
            // 移除最早的贴花
            self.decals.remove(0);
        }
        self.next_id += 1;
        let id = self.next_id;
        self.decals.push((id, decal));
        id
    }

    /// 根据 ID 移除贴花。
    pub fn remove_decal(&mut self, id: u64) -> bool {
        let before = self.decals.len();
        self.decals.retain(|(did, _)| *did != id);
        self.decals.len() < before
    }

    /// 清空所有贴花。
    pub fn clear(&mut self) {
        self.decals.clear();
    }

    /// 当前活跃贴花数量。
    pub fn count(&self) -> usize {
        self.decals.len()
    }

    /// 最大贴花数量。
    pub fn max_decals(&self) -> usize {
        self.max_decals
    }

    /// 遍历所有贴花。
    pub fn iter(&self) -> impl Iterator<Item = (u64, &Decal)> {
        self.decals.iter().map(|(id, d)| (*id, d))
    }

    /// 可变遍历所有贴花。
    pub fn iter_mut(&mut self) -> impl Iterator<Item = (u64, &mut Decal)> {
        self.decals.iter_mut().map(|(id, d)| (*id, d))
    }

    /// 根据 ID 获取贴花引用。
    pub fn get(&self, id: u64) -> Option<&Decal> {
        self.decals.iter().find(|(did, _)| *did == id).map(|(_, d)| d)
    }

    /// 根据 ID 获取贴花可变引用。
    pub fn get_mut(&mut self, id: u64) -> Option<&mut Decal> {
        self.decals.iter_mut().find(|(did, _)| *did == id).map(|(_, d)| d)
    }
}

/// 构建贴花的世界矩阵。
///
/// 基于贴花的位置、朝向（四元数）和尺寸生成 TRS 矩阵。
pub fn decal_world_matrix(decal: &Decal) -> Matrix4<f32> {
    use cgmath::Quaternion;

    let q = Quaternion::new(
        decal.orientation[3],
        decal.orientation[0],
        decal.orientation[1],
        decal.orientation[2],
    );
    let rotation: Matrix4<f32> = Matrix4::from(q);

    let scale = Matrix4::from_nonuniform_scale(decal.size[0], decal.size[1], decal.size[2]);
    let translation = Matrix4::from_translation(Vector3::new(
        decal.position[0],
        decal.position[1],
        decal.position[2],
    ));

    translation * rotation * scale
}

/// 屏幕空间投影：将贴花投影到裁剪空间。
///
/// `view_proj` 为相机的 View-Projection 矩阵（`view * projection` 或已合并）。
///
/// 返回 [`DecalProjection`]，包含 NDC 中心、半尺寸和可见性。
pub fn project_decal(decal: &Decal, view_proj: &Matrix4<f32>) -> DecalProjection {
    let world = decal_world_matrix(decal);
    let mvp = view_proj * world;

    // 将单位立方体的 8 个角点变换到裁剪空间
    let corners = [
        [-1.0_f32, -1.0, -1.0],
        [1.0, -1.0, -1.0],
        [-1.0, 1.0, -1.0],
        [1.0, 1.0, -1.0],
        [-1.0, -1.0, 1.0],
        [1.0, -1.0, 1.0],
        [-1.0, 1.0, 1.0],
        [1.0, 1.0, 1.0],
    ];

    let mut min_ndc = [f32::MAX; 3];
    let mut max_ndc = [f32::MIN; 3];
    let mut any_visible = false;

    for corner in &corners {
        let v = mvp * Vector4::new(corner[0], corner[1], corner[2], 1.0);
        if v.w.abs() < 1e-7 {
            continue;
        }
        let ndc_x = v.x / v.w;
        let ndc_y = v.y / v.w;
        let ndc_z = v.z / v.w;

        min_ndc[0] = min_ndc[0].min(ndc_x);
        min_ndc[1] = min_ndc[1].min(ndc_y);
        min_ndc[2] = min_ndc[2].min(ndc_z);
        max_ndc[0] = max_ndc[0].max(ndc_x);
        max_ndc[1] = max_ndc[1].max(ndc_y);
        max_ndc[2] = max_ndc[2].max(ndc_z);
        any_visible = true;
    }

    if !any_visible {
        return DecalProjection {
            ndc_center: [0.0; 3],
            ndc_half_size: [0.0; 3],
            depth_range: [0.0, 0.0],
            visible: false,
        };
    }

    // 视锥体剔除：NDC 范围完全在 [-1,1] 之外则不可见
    let in_frustum = max_ndc[0] >= -1.0
        && min_ndc[0] <= 1.0
        && max_ndc[1] >= -1.0
        && min_ndc[1] <= 1.0
        && max_ndc[2] >= -1.0
        && min_ndc[2] <= 1.0;

    DecalProjection {
        ndc_center: [
            (min_ndc[0] + max_ndc[0]) * 0.5,
            (min_ndc[1] + max_ndc[1]) * 0.5,
            (min_ndc[2] + max_ndc[2]) * 0.5,
        ],
        ndc_half_size: [
            (max_ndc[0] - min_ndc[0]) * 0.5,
            (max_ndc[1] - min_ndc[1]) * 0.5,
            (max_ndc[2] - min_ndc[2]) * 0.5,
        ],
        depth_range: [min_ndc[2].max(0.0), max_ndc[2].min(1.0)],
        visible: in_frustum,
    }
}

/// 深度测试：判断贴花是否与场景深度匹配。
///
/// - `decal_depth`：贴花在投影空间中的深度值（0~1）。
/// - `scene_depth`：场景中对应像素的深度缓冲值（0~1）。
/// - `threshold`：深度容差（如 0.01）。
///
/// 返回 [`DepthTestResult`]：
/// - `Pass`：贴花深度与场景深度在容差范围内，应渲染。
/// - `InFront`：贴花在场景表面前方。
/// - `Behind`：贴花在场景表面后方（被遮挡）。
pub fn depth_test_decal(decal_depth: f32, scene_depth: f32, threshold: f32) -> DepthTestResult {
    let diff = decal_depth - scene_depth;
    if diff.abs() <= threshold {
        DepthTestResult::Pass
    } else if diff < 0.0 {
        DepthTestResult::InFront
    } else {
        DepthTestResult::Behind
    }
}

// ---------------------------------------------------------------------------
// 测试
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decal_system_add_remove_clear() {
        let mut sys = DecalSystem::new(4);
        let d1 = sys.add_decal(Decal::default());
        let d2 = sys.add_decal(Decal {
            position: [1.0, 2.0, 3.0],
            ..Default::default()
        });
        assert_eq!(sys.count(), 2);

        assert!(sys.remove_decal(d1));
        assert_eq!(sys.count(), 1);
        assert!(!sys.remove_decal(d1)); // already removed

        assert!(sys.get(d2).is_some());
        sys.clear();
        assert_eq!(sys.count(), 0);
    }

    #[test]
    fn decal_system_evicts_oldest_when_full() {
        let mut sys = DecalSystem::new(2);
        let d1 = sys.add_decal(Decal::default());
        let _d2 = sys.add_decal(Decal::default());
        let d3 = sys.add_decal(Decal {
            position: [5.0, 0.0, 0.0],
            ..Default::default()
        });
        assert_eq!(sys.count(), 2);
        assert!(sys.get(d1).is_none(), "oldest decal should be evicted");
        assert!(sys.get(d3).is_some());
    }

    #[test]
    fn project_decal_identity_vp_visible() {
        // Identity VP: NDC = world coords, so place decal within NDC [-1,1] range
        let decal = Decal {
            position: [0.0, 0.0, 0.0],
            size: [0.5, 0.5, 0.1],
            ..Default::default()
        };
        let vp = Matrix4::identity();
        let proj = project_decal(&decal, &vp);
        assert!(proj.visible);
    }

    #[test]
    fn depth_test_within_threshold_passes() {
        assert_eq!(depth_test_decal(0.5, 0.505, 0.01), DepthTestResult::Pass);
    }

    #[test]
    fn depth_test_in_front() {
        assert_eq!(depth_test_decal(0.3, 0.5, 0.01), DepthTestResult::InFront);
    }

    #[test]
    fn depth_test_behind() {
        assert_eq!(depth_test_decal(0.7, 0.5, 0.01), DepthTestResult::Behind);
    }
}
