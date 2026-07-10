//! 地形网格生成器：从 Heightmap 生成顶点 + 索引缓冲。
//!
//! `TerrainMesher` 按 LOD stride 采样 heightmap，生成顶点（position + normal + uv），
//! 法线复用 `Heightmap::normal_at()`。

use bytemuck::{Pod, Zeroable};
use crate::Heightmap;

/// 地形顶点格式（与 WGSL shader 对齐）。
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct TerrainVertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub uv: [f32; 2],
    pub _pad: [f32; 2],
}

/// 地形网格生成器。
pub struct TerrainMesher;

impl TerrainMesher {
    /// 从 heightmap 生成网格。
    ///
    /// - `cell_size`: 每个高度图格子的世界空间间距（米）。
    /// - `lod`: LOD 等级（0 = 最高精度，数字越大采样 stride 越大）。
    ///
    /// 返回 `(vertices, indices)`。
    pub fn generate_mesh(
        heightmap: &Heightmap,
        cell_size: f32,
        lod: u8,
    ) -> (Vec<TerrainVertex>, Vec<u32>) {
        let stride = (1u32 << lod.min(6)) as u32; // lod 0=1, lod 1=2, ..., cap at 6
        let w = heightmap.width;
        let h = heightmap.height;

        // 计算实际网格分辨率（按 stride 采样）
        let cols = ((w - 1) / stride + 1).max(2);
        let rows = ((h - 1) / stride + 1).max(2);

        let mut vertices: Vec<TerrainVertex> = Vec::with_capacity((cols * rows) as usize);

        // 生成顶点
        for j in 0..rows {
            let hj = (j * stride).min(h - 1);
            for i in 0..cols {
                let hi = (i * stride).min(w - 1);

                let height = heightmap.get(hi, hj);
                let normal = heightmap.normal_at(hi, hj, cell_size * stride as f32);

                // 世界坐标：以 heightmap 左下角为原点
                let x = (hi as f32) * cell_size;
                let y = height;
                let z = (hj as f32) * cell_size;

                // UV: 归一化到 [0, 1]
                let u = if cols > 1 { i as f32 / (cols - 1) as f32 } else { 0.0 };
                let v = if rows > 1 { j as f32 / (rows - 1) as f32 } else { 0.0 };

                vertices.push(TerrainVertex {
                    position: [x, y, z],
                    normal,
                    uv: [u, v],
                    _pad: [0.0; 2],
                });
            }
        }

        // 生成索引（双三角形 strip per quad）
        let mut indices: Vec<u32> = Vec::with_capacity(((cols - 1) * (rows - 1) * 6) as usize);
        for j in 0..rows - 1 {
            for i in 0..cols - 1 {
                let v00 = j * cols + i;
                let v10 = j * cols + (i + 1);
                let v01 = (j + 1) * cols + i;
                let v11 = (j + 1) * cols + (i + 1);

                // 两个三角形（CCW winding）
                indices.extend_from_slice(&[v00, v01, v10]);
                indices.extend_from_slice(&[v10, v01, v11]);
            }
        }

        (vertices, indices)
    }

    /// 返回地形顶点的 wgpu vertex buffer layout。
    pub fn vertex_layout<'a>() -> wgpu::VertexBufferLayout<'a> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<TerrainVertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[
                wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 0,
                    format: wgpu::VertexFormat::Float32x3,
                },
                wgpu::VertexAttribute {
                    offset: std::mem::size_of::<[f32; 3]>() as wgpu::BufferAddress,
                    shader_location: 1,
                    format: wgpu::VertexFormat::Float32x3,
                },
                wgpu::VertexAttribute {
                    offset: (std::mem::size_of::<[f32; 3]>() * 2) as wgpu::BufferAddress,
                    shader_location: 2,
                    format: wgpu::VertexFormat::Float32x2,
                },
            ],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_mesh_flat_terrain() {
        let h = Heightmap::new(4, 4);
        let (verts, indices) = TerrainMesher::generate_mesh(&h, 1.0, 0);
        // 4x4 heightmap → 4*4 = 16 vertices
        assert_eq!(verts.len(), 16);
        // 3*3 quads * 6 indices = 54
        assert_eq!(indices.len(), 54);
        // All heights should be 0
        for v in &verts {
            assert!((v.position[1] - 0.0).abs() < 1e-5);
        }
    }

    #[test]
    fn generate_mesh_lod_reduces_vertex_count() {
        let h = Heightmap::new(16, 16);
        let (v0, _) = TerrainMesher::generate_mesh(&h, 1.0, 0);
        let (v1, _) = TerrainMesher::generate_mesh(&h, 1.0, 1);
        let (v2, _) = TerrainMesher::generate_mesh(&h, 1.0, 2);
        // LOD increases should reduce vertex count
        assert!(v0.len() > v1.len());
        assert!(v1.len() > v2.len());
    }

    #[test]
    fn vertex_size_is_40_bytes() {
        // 3+3+2+2 = 10 f32 = 40 bytes
        assert_eq!(std::mem::size_of::<TerrainVertex>(), 40);
    }

    #[test]
    fn normal_points_up_on_flat_terrain() {
        let h = Heightmap::new(4, 4);
        let (verts, _) = TerrainMesher::generate_mesh(&h, 1.0, 0);
        // On flat terrain, normals should point up (y > 0.9)
        for v in &verts {
            assert!(v.normal[1] > 0.9, "normal y = {}", v.normal[1]);
        }
    }
}
