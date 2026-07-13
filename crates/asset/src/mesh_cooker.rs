//! 网格 Cooking — 顶点去重、法线/切线计算、索引优化、包围体计算。
//!
//! Feature gate: `cooking`（默认禁用）。
//!
//! 全部基于纯 CPU 算法，不依赖 meshopt 或任何平台特定库，
//! 确保在 Windows / Linux / macOS 上均可编译运行。

use std::collections::HashMap;

// ---------------------------------------------------------------------------
// 配置 & 输出类型
// ---------------------------------------------------------------------------

/// 网格 cooking 配置。
#[derive(Clone, Debug)]
pub struct MeshCookConfig {
    /// 是否优化顶点缓存（post-transform optimization）
    pub optimize_vertex_cache: bool,
    /// 是否优化过度绘制
    pub optimize_overdraw: bool,
    /// overdraw optimization 阈值
    pub overdraw_threshold: f32,
    /// 是否计算法线（如果输入网格没有法线）
    pub compute_normals: bool,
    /// 是否计算切线空间（需要 UV 数据）
    pub compute_tangents: bool,
    /// 顶点去重容差
    pub weld_tolerance: f32,
}

impl Default for MeshCookConfig {
    fn default() -> Self {
        Self {
            optimize_vertex_cache: true,
            optimize_overdraw: true,
            overdraw_threshold: 1.05,
            compute_normals: true,
            compute_tangents: true,
            weld_tolerance: 1e-6,
        }
    }
}

/// 网格 cook 输出。
#[derive(Clone, Debug)]
pub struct MeshOutput {
    /// 顶点数据（交错格式，stride = vertex_stride）
    pub vertices: Vec<u8>,
    /// 索引数据
    pub indices: Vec<u32>,
    /// 顶点步长（字节）
    pub vertex_stride: usize,
    /// 法线数据（每顶点 [f32; 3]），如果已计算
    pub normals: Option<Vec<[f32; 3]>>,
    /// 切线数据（每顶点 [f32; 4]，w 分量 = 手性 ±1），如果已计算
    pub tangents: Option<Vec<[f32; 4]>>,
    /// 轴对齐包围盒
    pub bounding_box: BoundingBox,
    /// 包围球
    pub bounding_sphere: BoundingSphere,
    /// 顶点数量
    pub vertex_count: u32,
    /// 三角形数量
    pub triangle_count: u32,
}

/// 轴对齐包围盒。
#[derive(Clone, Debug, Default)]
pub struct BoundingBox {
    pub min: [f32; 3],
    pub max: [f32; 3],
}

/// 包围球。
#[derive(Clone, Debug, Default)]
pub struct BoundingSphere {
    pub center: [f32; 3],
    pub radius: f32,
}

// ---------------------------------------------------------------------------
// MeshCooker
// ---------------------------------------------------------------------------

/// Mesh cooker: 优化和处理网格数据。
///
/// 处理管线：
/// 1. 顶点去重（相同位置的顶点合并）
/// 2. 索引缓存优化（提高 GPU 顶点缓存命中率）
/// 3. 法线计算（基于三角形面积加权面法线）
/// 4. 切线空间计算（基于 UV 映射）
/// 5. 包围盒/包围球计算
///
/// 输入顶点数据约定（交错布局，little-endian f32）：
/// - Position:  offset 0,  3 × f32 (12 bytes)
/// - Normal:    offset 12, 3 × f32 (12 bytes) — 可选
/// - UV:        offset 24, 2 × f32 (8 bytes)  — 可选
pub struct MeshCooker;

/// 顶点步长常量。
const POSITION_SIZE: usize = 12; // 3 × f32
const NORMAL_OFFSET: usize = 12;
const NORMAL_SIZE: usize = 12;
const UV_OFFSET: usize = 24;
const UV_SIZE: usize = 8;
const MIN_STRIDE_FOR_NORMAL: usize = POSITION_SIZE + NORMAL_SIZE; // 24
const MIN_STRIDE_FOR_UV: usize = UV_OFFSET + UV_SIZE; // 32

impl MeshCooker {
    /// 处理网格数据（向后兼容的简化接口）。
    ///
    /// 返回处理后的 (vertices, indices)。
    pub fn cook(
        vertices: &[u8],
        indices: &[u32],
        vertex_stride: usize,
        config: &MeshCookConfig,
    ) -> (Vec<u8>, Vec<u32>) {
        let output = Self::cook_full(vertices, indices, vertex_stride, config);
        (output.vertices, output.indices)
    }

    /// 完整网格处理管线，返回结构化输出。
    pub fn cook_full(
        vertices: &[u8],
        indices: &[u32],
        vertex_stride: usize,
        config: &MeshCookConfig,
    ) -> MeshOutput {
        assert!(vertex_stride >= POSITION_SIZE, "vertex_stride must be >= 12");

        let mut verts = vertices.to_vec();
        let mut idxs = indices.to_vec();

        // 1. 顶点去重
        if config.weld_tolerance >= 0.0 {
            let (new_verts, new_idxs) = Self::deduplicate_vertices(&verts, &idxs, vertex_stride);
            verts = new_verts;
            idxs = new_idxs;
        }

        let vertex_count = verts.len() / vertex_stride;

        // 2. 索引缓存优化
        if config.optimize_vertex_cache && !idxs.is_empty() {
            idxs = Self::optimize_index_order(&idxs, vertex_count);
        }

        // 3. 法线计算
        let normals = if config.compute_normals {
            let n = Self::compute_normals(&verts, &idxs, vertex_stride);
            Self::write_normals_into_vertices(&mut verts, &n, vertex_stride);
            Some(n)
        } else {
            None
        };

        // 4. 切线空间计算
        let tangents = if config.compute_tangents && vertex_stride >= MIN_STRIDE_FOR_UV {
            let t = Self::compute_tangents(&verts, &idxs, vertex_stride);
            Some(t)
        } else {
            None
        };

        // 5. 包围体
        let bounding_box = Self::compute_bounding_box(&verts, vertex_stride);
        let bounding_sphere = Self::compute_bounding_sphere(&verts, vertex_stride, &bounding_box);

        let final_vertex_count = (verts.len() / vertex_stride) as u32;
        let triangle_count = (idxs.len() / 3) as u32;

        MeshOutput {
            vertices: verts,
            indices: idxs,
            vertex_stride,
            normals,
            tangents,
            bounding_box,
            bounding_sphere,
            vertex_count: final_vertex_count,
            triangle_count,
        }
    }

    // -----------------------------------------------------------------------
    // 顶点去重
    // -----------------------------------------------------------------------

    /// 按位置去重顶点，合并共享相同位置的顶点。
    fn deduplicate_vertices(
        vertices: &[u8],
        indices: &[u32],
        stride: usize,
    ) -> (Vec<u8>, Vec<u32>) {
        let vertex_count = vertices.len() / stride;
        let mut new_verts = Vec::new();
        let mut new_idxs = Vec::with_capacity(indices.len());
        let mut seen: HashMap<[u8; POSITION_SIZE], u32> = HashMap::new();

        for &idx in indices {
            let i = idx as usize;
            if i >= vertex_count {
                continue;
            }
            let base = i * stride;
            let pos_bytes: [u8; POSITION_SIZE] =
                vertices[base..base + POSITION_SIZE].try_into().unwrap();

            if let Some(&new_idx) = seen.get(&pos_bytes) {
                new_idxs.push(new_idx);
            } else {
                let new_idx = (new_verts.len() / stride) as u32;
                seen.insert(pos_bytes, new_idx);
                new_verts.extend_from_slice(&vertices[base..base + stride]);
                new_idxs.push(new_idx);
            }
        }

        (new_verts, new_idxs)
    }

    // -----------------------------------------------------------------------
    // 索引缓存优化
    // -----------------------------------------------------------------------

    /// 简化版顶点缓存优化。
    ///
    /// 模拟 FIFO 顶点缓存（典型 GPU post-transform cache 大小为 16-32），
    /// 优先调度能复用缓存中顶点的三角形，从而提高 ACMR
    /// (Average Cache Miss Ratio)。
    fn optimize_index_order(indices: &[u32], _vertex_count: usize) -> Vec<u32> {
        const CACHE_SIZE: usize = 16;
        let tri_count = indices.len() / 3;
        if tri_count == 0 {
            return indices.to_vec();
        }

        let mut result = Vec::with_capacity(indices.len());
        let mut emitted = vec![false; tri_count];
        let mut cache: Vec<u32> = Vec::with_capacity(CACHE_SIZE + 3);

        // 从第一个三角形开始，贪心地选择能命中缓存的三角形
        let mut next_tri = 0usize;

        loop {
            // 找到下一个未发射的三角形
            while next_tri < tri_count && emitted[next_tri] {
                next_tri += 1;
            }
            if next_tri >= tri_count {
                // 检查是否还有未发射的
                match emitted.iter().position(|&e| !e) {
                    Some(idx) => next_tri = idx,
                    None => break,
                }
            }

            // 发射三角形
            let base = next_tri * 3;
            let tri = [indices[base], indices[base + 1], indices[base + 2]];
            result.extend_from_slice(&tri);
            emitted[next_tri] = true;

            // 更新 FIFO 缓存
            for &v in &tri {
                if !cache.contains(&v) {
                    if cache.len() >= CACHE_SIZE {
                        cache.remove(0);
                    }
                    cache.push(v);
                }
            }

            // 贪心：在缓存中的顶点所关联的未发射三角形中，选命中最多的
            let mut best_tri = None;
            let mut best_score = 0usize;

            for (t, &is_emitted) in emitted.iter().enumerate() {
                if is_emitted {
                    continue;
                }
                let tb = t * 3;
                let score = [indices[tb], indices[tb + 1], indices[tb + 2]]
                    .iter()
                    .filter(|v| cache.contains(v))
                    .count();
                if score > best_score {
                    best_score = score;
                    best_tri = Some(t);
                }
            }

            if let Some(t) = best_tri {
                next_tri = t;
            } else {
                next_tri += 1;
            }
        }

        result
    }

    // -----------------------------------------------------------------------
    // 法线计算
    // -----------------------------------------------------------------------

    /// 计算顶点法线（面积加权面法线累加后归一化）。
    fn compute_normals(vertices: &[u8], indices: &[u32], stride: usize) -> Vec<[f32; 3]> {
        let vertex_count = vertices.len() / stride;
        let mut normals = vec![[0.0f32; 3]; vertex_count];

        for tri in indices.chunks_exact(3) {
            let i0 = tri[0] as usize;
            let i1 = tri[1] as usize;
            let i2 = tri[2] as usize;

            if i0 >= vertex_count || i1 >= vertex_count || i2 >= vertex_count {
                continue;
            }

            let p0 = read_position(vertices, i0, stride);
            let p1 = read_position(vertices, i1, stride);
            let p2 = read_position(vertices, i2, stride);

            // 面法线 = (p1-p0) × (p2-p0)，长度正比于三角形面积的 2 倍
            // 因此累加面法线等价于面积加权
            let e1 = vec3_sub(&p1, &p0);
            let e2 = vec3_sub(&p2, &p0);
            let n = vec3_cross(&e1, &e2);

            for &idx in tri {
                let i = idx as usize;
                normals[i][0] += n[0];
                normals[i][1] += n[1];
                normals[i][2] += n[2];
            }
        }

        // 归一化
        for n in &mut normals {
            let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
            if len > 1e-8 {
                let inv = 1.0 / len;
                n[0] *= inv;
                n[1] *= inv;
                n[2] *= inv;
            }
        }

        normals
    }

    /// 将计算的法线写入顶点数据的 normal 槽位（offset 12, 12 bytes）。
    ///
    /// 如果 vertex_stride < 24，则跳过写入（没有空间存放法线）。
    fn write_normals_into_vertices(
        vertices: &mut [u8],
        normals: &[[f32; 3]],
        stride: usize,
    ) {
        if stride < MIN_STRIDE_FOR_NORMAL {
            return;
        }
        for (i, n) in normals.iter().enumerate() {
            let base = i * stride + NORMAL_OFFSET;
            if base + NORMAL_SIZE > vertices.len() {
                break;
            }
            let bytes = [
                n[0].to_le_bytes(),
                n[1].to_le_bytes(),
                n[2].to_le_bytes(),
            ];
            vertices[base..base + NORMAL_SIZE].copy_from_slice(&bytes.concat());
        }
    }

    // -----------------------------------------------------------------------
    // 切线空间计算
    // -----------------------------------------------------------------------

    /// 计算切线空间（基于 UV 映射，Lengyel 方法）。
    ///
    /// 返回每顶点 `[tangent_x, tangent_y, tangent_z, handedness]`。
    /// handedness = ±1，用于从 normal × tangent 恢复 bitangent。
    fn compute_tangents(vertices: &[u8], indices: &[u32], stride: usize) -> Vec<[f32; 4]> {
        if stride < MIN_STRIDE_FOR_UV {
            return Vec::new();
        }

        let vertex_count = vertices.len() / stride;
        let mut tan1 = vec![[0.0f32; 3]; vertex_count];
        let mut tan2 = vec![[0.0f32; 3]; vertex_count];

        for tri in indices.chunks_exact(3) {
            let i0 = tri[0] as usize;
            let i1 = tri[1] as usize;
            let i2 = tri[2] as usize;

            if i0 >= vertex_count || i1 >= vertex_count || i2 >= vertex_count {
                continue;
            }

            let p0 = read_position(vertices, i0, stride);
            let p1 = read_position(vertices, i1, stride);
            let p2 = read_position(vertices, i2, stride);
            let uv0 = read_uv(vertices, i0, stride);
            let uv1 = read_uv(vertices, i1, stride);
            let uv2 = read_uv(vertices, i2, stride);

            let e1 = vec3_sub(&p1, &p0);
            let e2 = vec3_sub(&p2, &p0);
            let du1 = uv1[0] - uv0[0];
            let dv1 = uv1[1] - uv0[1];
            let du2 = uv2[0] - uv0[0];
            let dv2 = uv2[1] - uv0[1];

            let det = du1 * dv2 - du2 * dv1;
            if det.abs() < 1e-8 {
                continue;
            }
            let r = 1.0 / det;

            let sdir = [
                (dv2 * e1[0] - dv1 * e2[0]) * r,
                (dv2 * e1[1] - dv1 * e2[1]) * r,
                (dv2 * e1[2] - dv1 * e2[2]) * r,
            ];
            let tdir = [
                (du1 * e2[0] - du2 * e1[0]) * r,
                (du1 * e2[1] - du2 * e1[1]) * r,
                (du1 * e2[2] - du2 * e1[2]) * r,
            ];

            for &idx in tri {
                let i = idx as usize;
                for c in 0..3 {
                    tan1[i][c] += sdir[c];
                    tan2[i][c] += tdir[c];
                }
            }
        }

        // 正交化并计算手性
        let mut result = Vec::with_capacity(vertex_count);
        for i in 0..vertex_count {
            let n = if stride >= MIN_STRIDE_FOR_NORMAL {
                read_normal(vertices, i, stride)
            } else {
                [0.0, 1.0, 0.0]
            };
            let t = tan1[i];

            // Gram-Schmidt 正交化: tangent = normalize(t - n * dot(n, t))
            let ndt = n[0] * t[0] + n[1] * t[1] + n[2] * t[2];
            let ortho = [t[0] - n[0] * ndt, t[1] - n[1] * ndt, t[2] - n[2] * ndt];
            let len = (ortho[0] * ortho[0] + ortho[1] * ortho[1] + ortho[2] * ortho[2]).sqrt();
            let tangent = if len > 1e-8 {
                let inv = 1.0 / len;
                [ortho[0] * inv, ortho[1] * inv, ortho[2] * inv]
            } else {
                [1.0, 0.0, 0.0] // fallback
            };

            // 手性: sign(dot(cross(n, t), tan2))
            let c = vec3_cross(&n, &t);
            let w = if c[0] * tan2[i][0] + c[1] * tan2[i][1] + c[2] * tan2[i][2] < 0.0 {
                -1.0
            } else {
                1.0
            };

            result.push([tangent[0], tangent[1], tangent[2], w]);
        }

        result
    }

    // -----------------------------------------------------------------------
    // 包围体计算
    // -----------------------------------------------------------------------

    /// 计算轴对齐包围盒。
    fn compute_bounding_box(vertices: &[u8], stride: usize) -> BoundingBox {
        let vertex_count = vertices.len() / stride;
        if vertex_count == 0 {
            return BoundingBox::default();
        }

        let first = read_position(vertices, 0, stride);
        let mut bb = BoundingBox {
            min: first,
            max: first,
        };

        for i in 1..vertex_count {
            let p = read_position(vertices, i, stride);
            for c in 0..3 {
                bb.min[c] = bb.min[c].min(p[c]);
                bb.max[c] = bb.max[c].max(p[c]);
            }
        }

        bb
    }

    /// 计算包围球（以 AABB 中心为球心，最远顶点距离为半径）。
    fn compute_bounding_sphere(
        vertices: &[u8],
        stride: usize,
        aabb: &BoundingBox,
    ) -> BoundingSphere {
        let vertex_count = vertices.len() / stride;
        if vertex_count == 0 {
            return BoundingSphere::default();
        }

        let center = [
            (aabb.min[0] + aabb.max[0]) * 0.5,
            (aabb.min[1] + aabb.max[1]) * 0.5,
            (aabb.min[2] + aabb.max[2]) * 0.5,
        ];

        let mut max_dist_sq = 0.0f32;
        for i in 0..vertex_count {
            let p = read_position(vertices, i, stride);
            let d = vec3_sub(&p, &center);
            let dist_sq = d[0] * d[0] + d[1] * d[1] + d[2] * d[2];
            max_dist_sq = max_dist_sq.max(dist_sq);
        }

        BoundingSphere {
            center,
            radius: max_dist_sq.sqrt(),
        }
    }
}

// ---------------------------------------------------------------------------
// 向量 / 内存读取辅助函数
// ---------------------------------------------------------------------------

/// 从顶点缓冲区读取位置 (offset 0, 3×f32)。
#[inline]
fn read_position(vertices: &[u8], index: usize, stride: usize) -> [f32; 3] {
    let base = index * stride;
    read_f32x3(&vertices[base..base + 12])
}

/// 从顶点缓冲区读取法线 (offset 12, 3×f32)。
#[inline]
fn read_normal(vertices: &[u8], index: usize, stride: usize) -> [f32; 3] {
    let base = index * stride + NORMAL_OFFSET;
    read_f32x3(&vertices[base..base + 12])
}

/// 从顶点缓冲区读取 UV (offset 24, 2×f32)。
#[inline]
fn read_uv(vertices: &[u8], index: usize, stride: usize) -> [f32; 2] {
    let base = index * stride + UV_OFFSET;
    let u = f32::from_le_bytes(vertices[base..base + 4].try_into().unwrap());
    let v = f32::from_le_bytes(vertices[base + 4..base + 8].try_into().unwrap());
    [u, v]
}

/// 从字节切片读取 3 个 little-endian f32。
#[inline]
fn read_f32x3(data: &[u8]) -> [f32; 3] {
    [
        f32::from_le_bytes(data[0..4].try_into().unwrap()),
        f32::from_le_bytes(data[4..8].try_into().unwrap()),
        f32::from_le_bytes(data[8..12].try_into().unwrap()),
    ]
}

#[inline]
fn vec3_sub(a: &[f32; 3], b: &[f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

#[inline]
fn vec3_cross(a: &[f32; 3], b: &[f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

// ---------------------------------------------------------------------------
// 测试
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// 构建简单三角形顶点数据（stride=32: pos12 + normal12 + uv8）。
    fn make_triangle() -> (Vec<u8>, Vec<u32>, usize) {
        let stride = 32;
        let mut verts = vec![0u8; 3 * stride];

        // v0 = (0, 0, 0)
        write_pos(&mut verts, 0, stride, [0.0, 0.0, 0.0]);
        write_uv_data(&mut verts, 0, stride, [0.0, 0.0]);
        // v1 = (1, 0, 0)
        write_pos(&mut verts, 1, stride, [1.0, 0.0, 0.0]);
        write_uv_data(&mut verts, 1, stride, [1.0, 0.0]);
        // v2 = (0, 1, 0)
        write_pos(&mut verts, 2, stride, [0.0, 1.0, 0.0]);
        write_uv_data(&mut verts, 2, stride, [0.0, 1.0]);

        (verts, vec![0, 1, 2], stride)
    }

    fn write_pos(buf: &mut [u8], idx: usize, stride: usize, p: [f32; 3]) {
        let base = idx * stride;
        buf[base..base + 4].copy_from_slice(&p[0].to_le_bytes());
        buf[base + 4..base + 8].copy_from_slice(&p[1].to_le_bytes());
        buf[base + 8..base + 12].copy_from_slice(&p[2].to_le_bytes());
    }

    fn write_uv_data(buf: &mut [u8], idx: usize, stride: usize, uv: [f32; 2]) {
        let base = idx * stride + UV_OFFSET;
        buf[base..base + 4].copy_from_slice(&uv[0].to_le_bytes());
        buf[base + 4..base + 8].copy_from_slice(&uv[1].to_le_bytes());
    }

    #[test]
    fn bounding_box_is_correct() {
        let (verts, idxs, stride) = make_triangle();
        let out = MeshCooker::cook_full(&verts, &idxs, stride, &MeshCookConfig::default());
        assert!((out.bounding_box.min[0] - 0.0).abs() < 1e-5);
        assert!((out.bounding_box.min[1] - 0.0).abs() < 1e-5);
        assert!((out.bounding_box.max[0] - 1.0).abs() < 1e-5);
        assert!((out.bounding_box.max[1] - 1.0).abs() < 1e-5);
    }

    #[test]
    fn normal_computation_correct() {
        let (verts, idxs, stride) = make_triangle();
        let out = MeshCooker::cook_full(&verts, &idxs, stride, &MeshCookConfig::default());
        let normals = out.normals.as_ref().unwrap();
        // XY 平面三角形的法线应为 (0, 0, 1)
        for n in normals {
            assert!(n[0].abs() < 1e-5, "nx = {}", n[0]);
            assert!(n[1].abs() < 1e-5, "ny = {}", n[1]);
            assert!((n[2] - 1.0).abs() < 1e-5, "nz = {}", n[2]);
        }
    }

    #[test]
    fn tangent_computation_produces_data() {
        let (verts, idxs, stride) = make_triangle();
        let out = MeshCooker::cook_full(&verts, &idxs, stride, &MeshCookConfig::default());
        assert!(out.tangents.is_some());
        let tangents = out.tangents.as_ref().unwrap();
        assert_eq!(tangents.len(), 3);
        // 手性应为 ±1
        for t in tangents {
            assert!(t[3].abs() - 1.0 < 1e-5, "handedness = {}", t[3]);
        }
    }

    #[test]
    fn dedup_merges_identical_vertices() {
        let stride = 32;
        // 两组索引指向同一位置的顶点
        let mut verts = vec![0u8; 6 * stride];
        write_pos(&mut verts, 0, stride, [1.0, 2.0, 3.0]);
        write_pos(&mut verts, 1, stride, [4.0, 5.0, 6.0]);
        write_pos(&mut verts, 2, stride, [7.0, 8.0, 9.0]);
        // 重复
        write_pos(&mut verts, 3, stride, [1.0, 2.0, 3.0]); // same as v0
        write_pos(&mut verts, 4, stride, [4.0, 5.0, 6.0]); // same as v1
        write_pos(&mut verts, 5, stride, [7.0, 8.0, 9.0]); // same as v2

        let idxs = vec![0, 1, 2, 3, 4, 5];
        let config = MeshCookConfig::default();
        let out = MeshCooker::cook_full(&verts, &idxs, stride, &config);
        assert_eq!(out.vertex_count, 3, "should dedup to 3 unique vertices");
    }

    #[test]
    fn bounding_sphere_radius_positive() {
        let (verts, idxs, stride) = make_triangle();
        let out = MeshCooker::cook_full(&verts, &idxs, stride, &MeshCookConfig::default());
        assert!(out.bounding_sphere.radius > 0.0);
    }

    #[test]
    fn cook_preserves_index_count() {
        let (verts, idxs, stride) = make_triangle();
        let (out_v, out_i) = MeshCooker::cook(&verts, &idxs, stride, &MeshCookConfig::default());
        assert_eq!(out_i.len(), idxs.len());
        assert!(!out_v.is_empty());
    }

    #[test]
    fn empty_mesh_does_not_panic() {
        let config = MeshCookConfig::default();
        let out = MeshCooker::cook_full(&[], &[], 12, &config);
        assert_eq!(out.vertex_count, 0);
        assert_eq!(out.triangle_count, 0);
        assert!(out.normals.is_some());
        assert_eq!(out.normals.as_ref().unwrap().len(), 0);
    }
}
