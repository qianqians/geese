//! 程序化 UV 球体网格（材质编辑器预览用）。
//!
//! 生成一个半径为 1、球心在原点的单位球，包含位置/法线/UV/切线；
//! 材质句柄固定为 `MaterialHandle(0)`（预览材质库只含当前编辑的材质）。

use cgmath::{InnerSpace, Point3, Vector2, Vector3};
use render::{MaterialHandle, MeshFlags, ModelMesh, Vertex};

/// 生成单位 UV 球体。
///
/// `segments_u`：经线分段（绕 Y 轴，≥3）；`segments_v`：纬线分段（北极到南极，≥2）。
pub fn generate_uv_sphere(segments_u: u32, segments_v: u32) -> ModelMesh {
    let su = segments_u.max(3);
    let sv = segments_v.max(2);

    let mut vertices = Vec::with_capacity(((sv + 1) * (su + 1)) as usize);
    let mut indices = Vec::with_capacity((sv * su * 6) as usize);

    for i in 0..=sv {
        // v = 0 在北极，v = 1 在南极
        let v = i as f32 / sv as f32;
        let theta = v * std::f32::consts::PI;
        let sin_theta = theta.sin();
        let cos_theta = theta.cos();

        for j in 0..=su {
            let u = j as f32 / su as f32;
            let phi = u * std::f32::consts::TAU;

            let x = phi.sin() * sin_theta;
            let y = cos_theta;
            let z = phi.cos() * sin_theta;
            let position = Point3::new(x, y, z);
            let normal = Vector3::new(x, y, z);

            // 切线沿经线（U）方向：dP/dphi 归一化；两极退化为任意非零向量。
            let tangent = Vector3::new(phi.cos(), 0.0, -phi.sin());
            let tangent = if tangent.magnitude2() > 1e-6 {
                tangent.normalize()
            } else {
                Vector3::new(1.0, 0.0, 0.0)
            };

            vertices.push(Vertex {
                position,
                normal,
                uv: Vector2::new(u, v),
                tangent: [tangent.x, tangent.y, tangent.z, 1.0],
                joints: [0; 4],
                weights: [0.0; 4],
            });
        }
    }

    let row = su + 1;
    for i in 0..sv {
        for j in 0..su {
            let a = (i * row + j) as u32;
            let b = a + row;
            let c = a + 1;
            let d = b + 1;
            // 渲染管线关闭了背面剔除，绕序不影响可见性；仍保持朝外绕序。
            indices.extend_from_slice(&[a, b, c]);
            indices.extend_from_slice(&[c, b, d]);
        }
    }

    ModelMesh {
        vertices,
        indices,
        material: Some(MaterialHandle(0)),
        skin: None,
        flags: MeshFlags {
            has_normals: true,
            has_uv0: true,
            has_tangents: true,
            has_skin: false,
        },
        lod_levels: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sphere_has_expected_counts() {
        let mesh = generate_uv_sphere(16, 8);
        assert_eq!(mesh.vertices.len(), (17 * 9) as usize);
        assert_eq!(mesh.indices.len(), (16 * 8 * 6) as usize);
        assert_eq!(mesh.material, Some(MaterialHandle(0)));
    }

    #[test]
    fn sphere_normals_are_unit_length() {
        let mesh = generate_uv_sphere(8, 4);
        for v in &mesh.vertices {
            let n = v.normal;
            let len = (n.x * n.x + n.y * n.y + n.z * n.z).sqrt();
            assert!((len - 1.0).abs() < 1e-4, "normal not unit length: {n:?}");
        }
    }
}
