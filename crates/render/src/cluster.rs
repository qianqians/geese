use bytemuck::{Pod, Zeroable};

/// 屏幕空间 cluster 划分配置（8 × 8 × 16 = 1024 个 cluster）。
///
/// 选择 1024 是为了让 cluster bitmask 数组本身保持小（1024 × 4 = 4 KB），
/// 在 `MAX_LIGHTS = 16` 的当前规模下完全足够，并显著降低调试复杂度。
pub const CLUSTER_TILES_X: u32 = 8;
pub const CLUSTER_TILES_Y: u32 = 8;
pub const CLUSTER_DEPTH_SLICES: u32 = 16;

pub const TOTAL_CLUSTERS: u32 = CLUSTER_TILES_X * CLUSTER_TILES_Y * CLUSTER_DEPTH_SLICES;

/// `cluster_culling.wgsl` 与 `forward_plus.wgsl` / `deferred_lighting.wgsl` 共享的
/// 划分参数 uniform。布局 = 8 个 vec4，共 128 字节。
///
/// 新增 `inv_vp_*` 字段用于 cluster culling compute shader 将
/// NDC cluster 角点反投影到 world-space，构建 AABB 进行光源剔除。
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct ClusterUniform {
    /// xyz=tile_count（X / Y / Z），w=total_clusters
    pub tile_count: [u32; 4],
    /// xy=screen size，z=z_near，w=z_far
    pub screen_z: [f32; 4],
    /// x=log(z_far/z_near)/slices，y=slices/log(z_far/z_near)
    /// z=tile_size_x，w=tile_size_y（屏幕像素）
    pub depth_params: [f32; 4],
    /// x=inverse_view_projection_valid（1.0 表示有效），其它 pad
    pub flags: [f32; 4],
    /// inverse_view_projection row 0（world-space ← clip-space）
    pub inv_vp_0: [f32; 4],
    /// inverse_view_projection row 1
    pub inv_vp_1: [f32; 4],
    /// inverse_view_projection row 2
    pub inv_vp_2: [f32; 4],
    /// inverse_view_projection row 3
    pub inv_vp_3: [f32; 4],
}

impl ClusterUniform {
    /// 根据当前视口尺寸与近远平面构造划分参数。
    /// `inv_vp` 为 inverse(view_projection) 矩阵，用于 cluster culling。
    pub fn new(width: u32, height: u32, z_near: f32, z_far: f32, inv_vp: [[f32; 4]; 4]) -> Self {
        let width_f = width.max(1) as f32;
        let height_f = height.max(1) as f32;
        let near = z_near.max(1e-4);
        let far = z_far.max(near + 1e-3);

        let log_ratio = (far / near).ln();
        let slices = CLUSTER_DEPTH_SLICES as f32;

        Self {
            tile_count: [
                CLUSTER_TILES_X,
                CLUSTER_TILES_Y,
                CLUSTER_DEPTH_SLICES,
                TOTAL_CLUSTERS,
            ],
            screen_z: [width_f, height_f, near, far],
            depth_params: [
                log_ratio / slices,
                slices / log_ratio,
                width_f / CLUSTER_TILES_X as f32,
                height_f / CLUSTER_TILES_Y as f32,
            ],
            flags: [1.0, 0.0, 0.0, 0.0],
            inv_vp_0: inv_vp[0],
            inv_vp_1: inv_vp[1],
            inv_vp_2: inv_vp[2],
            inv_vp_3: inv_vp[3],
        }
    }

    /// 默认 1×1 占位，仅用于 GPU 初始化阶段，运行时必须随后调用 [`update`]。
    pub fn placeholder() -> Self {
        Self::new(1, 1, 0.1, 100.0, crate::common::identity_matrix())
    }

    pub fn update(&mut self, width: u32, height: u32, z_near: f32, z_far: f32) {
        *self = Self::new(width, height, z_near, z_far, self.inv_vp_matrix());
    }

    /// 获取当前存储的 inverse_view_projection 矩阵。
    pub fn inv_vp_matrix(&self) -> [[f32; 4]; 4] {
        [self.inv_vp_0, self.inv_vp_1, self.inv_vp_2, self.inv_vp_3]
    }

    /// 更新 inverse_view_projection 矩阵（相机变化时调用）。
    pub fn set_inv_vp(&mut self, inv_vp: [[f32; 4]; 4]) {
        self.inv_vp_0 = inv_vp[0];
        self.inv_vp_1 = inv_vp[1];
        self.inv_vp_2 = inv_vp[2];
        self.inv_vp_3 = inv_vp[3];
        self.flags[0] = 1.0;
    }

    /// 把 view-space `z`（正值，越远越大）映射到 cluster slice index。
    /// 仅供 CPU 端单元测试与调试使用，shader 内有等价实现。
    pub fn slice_for_view_z(&self, view_z: f32) -> u32 {
        let near = self.screen_z[2];
        let log_ratio_per_slice = self.depth_params[0];
        if view_z <= near || log_ratio_per_slice <= 0.0 {
            return 0;
        }
        let slice = ((view_z / near).ln() / log_ratio_per_slice).floor() as i32;
        slice.clamp(0, CLUSTER_DEPTH_SLICES as i32 - 1) as u32
    }
}

impl Default for ClusterUniform {
    fn default() -> Self {
        Self::placeholder()
    }
}

#[cfg(test)]
fn identity() -> [[f32; 4]; 4] {
    [
        [1.0, 0.0, 0.0, 0.0],
        [0.0, 1.0, 0.0, 0.0],
        [0.0, 0.0, 1.0, 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn total_clusters_is_1024() {
        assert_eq!(TOTAL_CLUSTERS, 1024);
    }

    #[test]
    fn uniform_size_is_128_bytes() {
        assert_eq!(std::mem::size_of::<ClusterUniform>(), 128);
    }

    #[test]
    fn new_records_tile_count_and_screen() {
        let u = ClusterUniform::new(1280, 720, 0.1, 100.0, identity());
        assert_eq!(u.tile_count[0], CLUSTER_TILES_X);
        assert_eq!(u.tile_count[1], CLUSTER_TILES_Y);
        assert_eq!(u.tile_count[2], CLUSTER_DEPTH_SLICES);
        assert_eq!(u.tile_count[3], TOTAL_CLUSTERS);
        assert_eq!(u.screen_z[0], 1280.0);
        assert_eq!(u.screen_z[1], 720.0);
        assert!((u.depth_params[2] - 160.0).abs() < 1e-3);
        assert!((u.depth_params[3] - 90.0).abs() < 1e-3);
        // inv_vp stored correctly
        assert_eq!(u.inv_vp_0, identity()[0]);
        assert_eq!(u.inv_vp_3, identity()[3]);
    }

    #[test]
    fn slice_mapping_is_monotonic_and_bounded() {
        let u = ClusterUniform::new(800, 600, 0.1, 100.0, identity());
        let near_slice = u.slice_for_view_z(0.1);
        let far_slice = u.slice_for_view_z(99.9);
        assert_eq!(near_slice, 0);
        assert!(far_slice >= CLUSTER_DEPTH_SLICES - 2);
        // 单调性
        let mut prev = 0;
        for i in 1..16 {
            let z = 0.1 * (10.0_f32).powf(i as f32 / 8.0);
            let s = u.slice_for_view_z(z);
            assert!(s >= prev);
            prev = s;
        }
    }

    #[test]
    fn placeholder_does_not_panic_on_degenerate_size() {
        let _ = ClusterUniform::new(0, 0, 0.0, 0.0, identity());
    }

    #[test]
    fn update_replaces_in_place() {
        let mut u = ClusterUniform::placeholder();
        u.update(640, 480, 0.5, 50.0);
        assert_eq!(u.screen_z[0], 640.0);
        assert_eq!(u.screen_z[1], 480.0);
        assert_eq!(u.screen_z[2], 0.5);
        assert_eq!(u.screen_z[3], 50.0);
    }

    #[test]
    fn set_inv_vp_updates_matrix() {
        let mut u = ClusterUniform::placeholder();
        let new_vp = [
            [2.0, 0.0, 0.0, 0.0],
            [0.0, 2.0, 0.0, 0.0],
            [0.0, 0.0, 2.0, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ];
        u.set_inv_vp(new_vp);
        assert_eq!(u.inv_vp_0, new_vp[0]);
        assert_eq!(u.flags[0], 1.0);
    }
}
