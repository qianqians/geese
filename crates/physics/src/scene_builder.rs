//! GLTF 碰撞几何提取（feature = "scene-builder"）。
//!
//! 从 GLTF 文件提取顶点位置与三角形索引，构造 rapier3d 的
//! `ShapeDesc::TriMesh`。仅读取几何数据，忽略纹理、材质、法线、
//! UV、蒙皮、动画等渲染专有数据。

use gltf::mesh::util::ReadIndices;

/// 单个三角网格的碰撞数据（已应用节点世界变换）。
#[derive(Debug, Clone)]
pub struct TrimeshData {
    /// 已变换到世界空间的顶点位置
    pub vertices: Vec<[f32; 3]>,
    /// 三角形索引（每元素 3 个 u32）
    pub indices: Vec<[u32; 3]>,
    /// GLTF 节点名称（可能为空）
    pub node_name: Option<String>,
}

/// 从 GLTF 文件提取所有 mesh 的碰撞三角网格。
///
/// 递归遍历场景节点树，累积世界变换矩阵，对每个 mesh primitive
/// 提取顶点位置与三角形索引。顶点已通过节点世界变换矩阵转换到
/// 世界空间。
///
/// # 参数
/// - `path`: GLTF 文件路径
///
/// # 返回
/// - `Vec<TrimeshData>`: 每个 primitive 一个独立网格
pub fn extract_gltf_trimeshes(path: &str) -> Result<Vec<TrimeshData>, String> {
    let (document, buffers, _images) =
        gltf::import(path).map_err(|e| format!("gltf import failed: {e}"))?;
    // _images 立即 drop，不持有贴图内存

    let mut results = Vec::new();

    for scene in document.scenes() {
        for root_node in scene.nodes() {
            let identity = [
                [1.0_f32, 0.0, 0.0, 0.0],
                [0.0, 1.0, 0.0, 0.0],
                [0.0, 0.0, 1.0, 0.0],
                [0.0, 0.0, 0.0, 1.0],
            ];
            collect_trimeshes(&root_node, &buffers, identity, &mut results);
        }
    }

    Ok(results)
}

/// 递归遍历节点树，累积世界变换并提取 mesh 几何。
fn collect_trimeshes(
    node: &gltf::Node,
    buffers: &[gltf::buffer::Data],
    parent_transform: [[f32; 4]; 4],
    out: &mut Vec<TrimeshData>,
) {
    let local = node.transform().matrix();
    let world = mul_mat4(&parent_transform, &local);

    if let Some(mesh) = node.mesh() {
        for primitive in mesh.primitives() {
            let reader = primitive.reader(|buf| Some(buffers.get(buf.index())?.0.as_slice()));

            let positions: Vec<[f32; 3]> = match reader.read_positions() {
                Some(iter) => iter.collect(),
                None => continue,
            };

            if positions.is_empty() {
                continue;
            }

            // 读取索引：支持 U8 / U16 / U32 三种格式
            let mut indices: Vec<u32> = Vec::new();
            if let Some(idx_iter) = reader.read_indices() {
                match idx_iter {
                    ReadIndices::U8(iter) => indices.extend(iter.map(u32::from)),
                    ReadIndices::U16(iter) => indices.extend(iter.map(u32::from)),
                    ReadIndices::U32(iter) => indices.extend(iter),
                }
            }

            // 无索引缓冲时，生成顺序索引 (0,1,2), (3,4,5), ...
            if indices.is_empty() {
                indices.extend(0..positions.len() as u32);
            }

            // 将顶点通过节点世界变换矩阵转换到世界空间
            let transformed_verts: Vec<[f32; 3]> = positions
                .iter()
                .map(|p| transform_point(&world, p))
                .collect();

            // 转换为三角形索引格式 [u32; 3]
            let tri_indices: Vec<[u32; 3]> = indices
                .chunks_exact(3)
                .map(|chunk| [chunk[0], chunk[1], chunk[2]])
                .collect();

            if tri_indices.is_empty() {
                continue;
            }

            out.push(TrimeshData {
                vertices: transformed_verts,
                indices: tri_indices,
                node_name: node.name().map(String::from),
            });
        }
    }

    for child in node.children() {
        collect_trimeshes(&child, buffers, world, out);
    }
}

/// 4x4 矩阵乘法 `C = A * B`。
fn mul_mat4(a: &[[f32; 4]; 4], b: &[[f32; 4]; 4]) -> [[f32; 4]; 4] {
    let mut out = [[0.0_f32; 4]; 4];
    for i in 0..4 {
        for j in 0..4 {
            out[i][j] = a[i][0] * b[0][j]
                + a[i][1] * b[1][j]
                + a[i][2] * b[2][j]
                + a[i][3] * b[3][j];
        }
    }
    out
}

/// 使用 4x4 变换矩阵变换一个点 `(x, y, z, w=1.0)`。
fn transform_point(m: &[[f32; 4]; 4], p: &[f32; 3]) -> [f32; 3] {
    let x = m[0][0] * p[0] + m[0][1] * p[1] + m[0][2] * p[2] + m[0][3];
    let y = m[1][0] * p[0] + m[1][1] * p[1] + m[1][2] * p[2] + m[1][3];
    let z = m[2][0] * p[0] + m[2][1] * p[1] + m[2][2] * p[2] + m[2][3];
    [x, y, z]
}
