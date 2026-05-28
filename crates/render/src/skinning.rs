//! 4.9 GPU Skinning 骨架。
//!
//! 提供：
//! - `SkinningMode` 枚举：CPU / JointSsbo / VertexPulling / Morph 模式选择
//! - `JointPalette`：每个 skin 的 joint matrix 调色板，方便上传 SSBO/UBO
//! - `MorphWeights`：morph target 权重
//! - `compute_joint_matrices`：从 local pose 计算最终 joint matrix 的纯函数
//!
//! 与 [`crate::mesh::SkinHandle`] 配合：mesh 引用 SkinHandle，运行时根据 mode
//! 决定 vertex shader 用 uniform palette / SSBO / pulling。

use bytemuck::{Pod, Zeroable};

/// 单帧 joint 上限（与 wgsl uniform 数组对齐）。
pub const MAX_JOINTS: usize = 256;

/// 单 morph slot 上限。
pub const MAX_MORPH_TARGETS: usize = 8;

/// Skinning 模式。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SkinningMode {
    /// CPU 端蒙皮：vertex 写入 CPU 缓冲后整体上传。
    Cpu,
    /// uniform 数组上传 joint matrix（适合 <= 64 joint 的小模型）。
    UniformPalette,
    /// SSBO 上传 joint matrix + vertex shader 索引。
    JointSsbo,
    /// vertex pulling：vertex 数据全部走 SSBO，骨骼也走 SSBO。
    VertexPulling,
    /// morph target，每帧上传权重。
    Morph,
}

impl SkinningMode {
    pub fn requires_ssbo(self) -> bool {
        matches!(self, Self::JointSsbo | Self::VertexPulling)
    }
    pub fn supports_morph(self) -> bool {
        matches!(self, Self::Morph)
    }
}

/// GPU joint matrix 调色板，配合 SSBO/UBO 上传。
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct GpuJointMatrix {
    pub mat: [[f32; 4]; 4],
}

impl GpuJointMatrix {
    pub fn identity() -> Self {
        Self {
            mat: [
                [1.0, 0.0, 0.0, 0.0],
                [0.0, 1.0, 0.0, 0.0],
                [0.0, 0.0, 1.0, 0.0],
                [0.0, 0.0, 0.0, 1.0],
            ],
        }
    }
}

/// 单 skin 的 joint 调色板。
#[derive(Clone, Debug)]
pub struct JointPalette {
    pub matrices: Vec<GpuJointMatrix>,
}

impl JointPalette {
    pub fn new(count: usize) -> Self {
        let n = count.min(MAX_JOINTS);
        Self { matrices: vec![GpuJointMatrix::identity(); n] }
    }
    pub fn len(&self) -> usize { self.matrices.len() }
    pub fn is_empty(&self) -> bool { self.matrices.is_empty() }
    pub fn as_bytes(&self) -> &[u8] { bytemuck::cast_slice(&self.matrices) }
}

/// Morph target 权重数组。
#[derive(Clone, Debug, Default)]
pub struct MorphWeights {
    pub weights: Vec<f32>,
}

impl MorphWeights {
    pub fn new(count: usize) -> Self {
        Self { weights: vec![0.0; count.min(MAX_MORPH_TARGETS)] }
    }
    pub fn set(&mut self, index: usize, w: f32) {
        if index < self.weights.len() {
            self.weights[index] = w.clamp(0.0, 1.0);
        }
    }
    pub fn sum(&self) -> f32 { self.weights.iter().sum() }
}

/// 计算最终 joint matrix：`global_transform[i] * inverse_bind_matrix[i]`。
///
/// 这是 4x4 列主序矩阵乘法，输入数量应一致。多余者忽略。
pub fn compute_joint_matrices(
    global_transforms: &[[[f32; 4]; 4]],
    inverse_binds: &[[[f32; 4]; 4]],
) -> Vec<GpuJointMatrix> {
    let n = global_transforms.len().min(inverse_binds.len()).min(MAX_JOINTS);
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        out.push(GpuJointMatrix { mat: mat4_mul(&global_transforms[i], &inverse_binds[i]) });
    }
    out
}

fn mat4_mul(a: &[[f32; 4]; 4], b: &[[f32; 4]; 4]) -> [[f32; 4]; 4] {
    let mut r = [[0.0f32; 4]; 4];
    for i in 0..4 {
        for j in 0..4 {
            let mut sum = 0.0;
            for k in 0..4 {
                // 列主序：a[列][行]
                sum += a[k][i] * b[j][k];
            }
            r[j][i] = sum;
        }
    }
    r
}

/// 上传策略 trait：后端选择把 palette 写到 uniform/SSBO/vertex buffer。
pub trait SkinningUploader {
    fn mode(&self) -> SkinningMode;
    fn upload_palette(&mut self, skin_id: u32, palette: &JointPalette);
}

/// 占位上传器：仅记录调用历史，便于 pipeline 集成前的单测。
pub struct NullSkinningUploader {
    pub mode: SkinningMode,
    pub uploads: Vec<(u32, usize)>,
}

impl NullSkinningUploader {
    pub fn new(mode: SkinningMode) -> Self { Self { mode, uploads: Vec::new() } }
}

impl SkinningUploader for NullSkinningUploader {
    fn mode(&self) -> SkinningMode { self.mode }
    fn upload_palette(&mut self, skin_id: u32, palette: &JointPalette) {
        self.uploads.push((skin_id, palette.len()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ident() -> [[f32; 4]; 4] {
        [[1.0,0.0,0.0,0.0],[0.0,1.0,0.0,0.0],[0.0,0.0,1.0,0.0],[0.0,0.0,0.0,1.0]]
    }

    #[test]
    fn mode_capability_flags() {
        assert!(SkinningMode::JointSsbo.requires_ssbo());
        assert!(SkinningMode::VertexPulling.requires_ssbo());
        assert!(!SkinningMode::Cpu.requires_ssbo());
        assert!(SkinningMode::Morph.supports_morph());
        assert!(!SkinningMode::Cpu.supports_morph());
    }

    #[test]
    fn palette_len_clamped_to_max() {
        let p = JointPalette::new(MAX_JOINTS + 10);
        assert_eq!(p.len(), MAX_JOINTS);
    }

    #[test]
    fn compute_identity_yields_identity() {
        let g = vec![ident(); 4];
        let b = vec![ident(); 4];
        let out = compute_joint_matrices(&g, &b);
        assert_eq!(out.len(), 4);
        for m in out {
            assert!((m.mat[0][0] - 1.0).abs() < 1e-5);
            assert!((m.mat[3][3] - 1.0).abs() < 1e-5);
            assert!(m.mat[1][0].abs() < 1e-5);
        }
    }

    #[test]
    fn compute_truncates_to_min_length() {
        let g = vec![ident(); 5];
        let b = vec![ident(); 3];
        assert_eq!(compute_joint_matrices(&g, &b).len(), 3);
    }

    #[test]
    fn morph_weights_clamped_and_summed() {
        let mut m = MorphWeights::new(3);
        m.set(0, 0.6);
        m.set(1, 1.5); // 应被夹到 1.0
        m.set(99, 0.5); // 越界忽略
        assert!((m.sum() - 1.6).abs() < 1e-5);
    }

    #[test]
    fn null_uploader_records_palette() {
        let mut up = NullSkinningUploader::new(SkinningMode::JointSsbo);
        up.upload_palette(7, &JointPalette::new(12));
        up.upload_palette(7, &JointPalette::new(8));
        assert_eq!(up.uploads, vec![(7, 12), (7, 8)]);
        assert!(up.mode().requires_ssbo());
    }

    #[test]
    fn palette_bytes_match_matrix_size() {
        let p = JointPalette::new(3);
        assert_eq!(p.as_bytes().len(), 3 * 64);
    }
}
