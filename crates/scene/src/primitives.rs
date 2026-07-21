//! 程序化基础几何体（Primitive）网格生成。
//!
//! 支持：Cube（立方体）、Sphere（球体）、Plane（平面）、Cylinder（圆柱体）。
//! 所有生成的网格包含 position、normal、uv、tangent 数据。

use cgmath::{Point3, Vector2, Vector3};
use render::{MeshFlags, ModelMesh, Vertex};

// ---------------------------------------------------------------------------
// PrimitiveKind 枚举
// ---------------------------------------------------------------------------

/// 基础几何体类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrimitiveKind {
    Cube,
    Sphere,
    Plane,
    Cylinder,
}

/// 从字符串解析 PrimitiveKind（用于序列化兼容）。
pub fn primitive_kind_from_str(s: &str) -> Option<PrimitiveKind> {
    match s {
        "cube" => Some(PrimitiveKind::Cube),
        "sphere" => Some(PrimitiveKind::Sphere),
        "plane" => Some(PrimitiveKind::Plane),
        "cylinder" => Some(PrimitiveKind::Cylinder),
        _ => None,
    }
}

/// 使用默认参数创建指定类型的 primitive 网格。
pub fn create_primitive_mesh(kind: PrimitiveKind) -> ModelMesh {
    match kind {
        PrimitiveKind::Cube => create_cube_mesh_procedural(1.0, 1.0, 1.0),
        PrimitiveKind::Sphere => create_sphere_mesh_procedural(0.5, 32, 16),
        PrimitiveKind::Plane => create_plane_mesh_procedural(1.0, 1.0),
        PrimitiveKind::Cylinder => create_cylinder_mesh_procedural(0.5, 1.0, 32),
    }
}

// ---------------------------------------------------------------------------
// Plane
// ---------------------------------------------------------------------------

/// 生成 XZ 平面网格（法线朝上 +Y）。
pub fn create_plane_mesh_procedural(size_x: f32, size_z: f32) -> ModelMesh {
    let hx = size_x * 0.5;
    let hz = size_z * 0.5;

    let vertices = vec![
        Vertex {
            position: Point3::new(-hx, 0.0, -hz),
            normal: Vector3::new(0.0, 1.0, 0.0),
            uv: Vector2::new(0.0, 0.0),
            tangent: [1.0, 0.0, 0.0, 1.0],
            joints: [0, 0, 0, 0],
            weights: [1.0, 0.0, 0.0, 0.0],
        },
        Vertex {
            position: Point3::new(hx, 0.0, -hz),
            normal: Vector3::new(0.0, 1.0, 0.0),
            uv: Vector2::new(1.0, 0.0),
            tangent: [1.0, 0.0, 0.0, 1.0],
            joints: [0, 0, 0, 0],
            weights: [1.0, 0.0, 0.0, 0.0],
        },
        Vertex {
            position: Point3::new(hx, 0.0, hz),
            normal: Vector3::new(0.0, 1.0, 0.0),
            uv: Vector2::new(1.0, 1.0),
            tangent: [1.0, 0.0, 0.0, 1.0],
            joints: [0, 0, 0, 0],
            weights: [1.0, 0.0, 0.0, 0.0],
        },
        Vertex {
            position: Point3::new(-hx, 0.0, hz),
            normal: Vector3::new(0.0, 1.0, 0.0),
            uv: Vector2::new(0.0, 1.0),
            tangent: [1.0, 0.0, 0.0, 1.0],
            joints: [0, 0, 0, 0],
            weights: [1.0, 0.0, 0.0, 0.0],
        },
    ];

    let indices = vec![0, 1, 2, 0, 2, 3];

    let mut mesh = ModelMesh::new();
    mesh.vertices = vertices;
    mesh.indices = indices;
    mesh.flags = MeshFlags {
        has_normals: true,
        has_uv0: true,
        has_tangents: true,
        has_skin: false,
    };
    mesh
}

// ---------------------------------------------------------------------------
// Cube
// ---------------------------------------------------------------------------

/// 生成立方体网格（24 顶点 / 36 索引，每面独立法线）。
pub fn create_cube_mesh_procedural(sx: f32, sy: f32, sz: f32) -> ModelMesh {
    let hx = sx * 0.5;
    let hy = sy * 0.5;
    let hz = sz * 0.5;

    #[rustfmt::skip]
    let positions = [
        [-hx, -hy,  hz], [ hx, -hy,  hz], [ hx,  hy,  hz], [-hx,  hy,  hz], // +Z
        [ hx, -hy, -hz], [-hx, -hy, -hz], [-hx,  hy, -hz], [ hx,  hy, -hz], // -Z
        [ hx, -hy,  hz], [ hx, -hy, -hz], [ hx,  hy, -hz], [ hx,  hy,  hz], // +X
        [-hx, -hy, -hz], [-hx, -hy,  hz], [-hx,  hy,  hz], [-hx,  hy, -hz], // -X
        [-hx,  hy,  hz], [ hx,  hy,  hz], [ hx,  hy, -hz], [-hx,  hy, -hz], // +Y
        [-hx, -hy, -hz], [ hx, -hy, -hz], [ hx, -hy,  hz], [-hx, -hy,  hz], // -Y
    ];

    #[rustfmt::skip]
    let normals = [
        [0.0,0.0,1.0],[0.0,0.0,1.0],[0.0,0.0,1.0],[0.0,0.0,1.0],
        [0.0,0.0,-1.0],[0.0,0.0,-1.0],[0.0,0.0,-1.0],[0.0,0.0,-1.0],
        [1.0,0.0,0.0],[1.0,0.0,0.0],[1.0,0.0,0.0],[1.0,0.0,0.0],
        [-1.0,0.0,0.0],[-1.0,0.0,0.0],[-1.0,0.0,0.0],[-1.0,0.0,0.0],
        [0.0,1.0,0.0],[0.0,1.0,0.0],[0.0,1.0,0.0],[0.0,1.0,0.0],
        [0.0,-1.0,0.0],[0.0,-1.0,0.0],[0.0,-1.0,0.0],[0.0,-1.0,0.0],
    ];

    #[rustfmt::skip]
    let uvs = [
        [0.0,0.0],[1.0,0.0],[1.0,1.0],[0.0,1.0],
        [0.0,0.0],[1.0,0.0],[1.0,1.0],[0.0,1.0],
        [0.0,0.0],[1.0,0.0],[1.0,1.0],[0.0,1.0],
        [0.0,0.0],[1.0,0.0],[1.0,1.0],[0.0,1.0],
        [0.0,0.0],[1.0,0.0],[1.0,1.0],[0.0,1.0],
        [0.0,0.0],[1.0,0.0],[1.0,1.0],[0.0,1.0],
    ];

    let vertices: Vec<Vertex> = (0..24)
        .map(|i| Vertex {
            position: Point3::new(positions[i][0], positions[i][1], positions[i][2]),
            normal: Vector3::new(normals[i][0], normals[i][1], normals[i][2]),
            uv: Vector2::new(uvs[i][0], uvs[i][1]),
            tangent: [1.0, 0.0, 0.0, 1.0],
            joints: [0, 0, 0, 0],
            weights: [1.0, 0.0, 0.0, 0.0],
        })
        .collect();

    #[rustfmt::skip]
    let indices = vec![
        0,1,2, 0,2,3, 4,5,6, 4,6,7,
        8,9,10, 8,10,11, 12,13,14, 12,14,15,
        16,17,18, 16,18,19, 20,21,22, 20,22,23,
    ];

    let mut mesh = ModelMesh::new();
    mesh.vertices = vertices;
    mesh.indices = indices;
    mesh.flags = MeshFlags {
        has_normals: true,
        has_uv0: true,
        has_tangents: true,
        has_skin: false,
    };
    mesh
}

// ---------------------------------------------------------------------------
// Sphere (UV Sphere)
// ---------------------------------------------------------------------------

/// 生成 UV 球体网格。
///
/// - `radius`: 球体半径
/// - `segments`: 经线分段数（绕 Y 轴）
/// - `rings`: 纬线分段数（从顶到底）
pub fn create_sphere_mesh_procedural(radius: f32, segments: u32, rings: u32) -> ModelMesh {
    let mut vertices: Vec<Vertex> = Vec::new();
    let mut indices: Vec<u32> = Vec::new();

    let pi = std::f32::consts::PI;

    // 生成顶点：rings+1 行 × segments+1 列（UV 接缝需要重复顶点）
    for ring in 0..=rings {
        let phi = pi * ring as f32 / rings as f32; // [0, π]
        let sin_phi = phi.sin();
        let cos_phi = phi.cos();

        for seg in 0..=segments {
            let theta = 2.0 * pi * seg as f32 / segments as f32; // [0, 2π]
            let sin_theta = theta.sin();
            let cos_theta = theta.cos();

            // 法线 = 归一化位置（单位球面方向）
            let nx = sin_phi * cos_theta;
            let ny = cos_phi;
            let nz = sin_phi * sin_theta;

            let position = Point3::new(radius * nx, radius * ny, radius * nz);
            let normal = Vector3::new(nx, ny, nz);
            let uv = Vector2::new(
                seg as f32 / segments as f32,
                ring as f32 / rings as f32,
            );

            // 切线沿经线方向（theta 增加方向）
            let tx = cos_theta;
            let tz = sin_theta;
            let tangent = [tx, 0.0, tz, 1.0];

            vertices.push(Vertex {
                position,
                normal,
                uv,
                tangent,
                joints: [0, 0, 0, 0],
                weights: [1.0, 0.0, 0.0, 0.0],
            });
        }
    }

    // 生成索引
    let cols = segments + 1;
    for ring in 0..rings {
        for seg in 0..segments {
            let a = ring * cols + seg;
            let b = a + cols;
            let c = a + 1;
            let d = b + 1;

            // 两个三角形（CCW 从外部看）
            indices.push(a);
            indices.push(b);
            indices.push(c);

            indices.push(c);
            indices.push(b);
            indices.push(d);
        }
    }

    let mut mesh = ModelMesh::new();
    mesh.vertices = vertices;
    mesh.indices = indices;
    mesh.flags = MeshFlags {
        has_normals: true,
        has_uv0: true,
        has_tangents: true,
        has_skin: false,
    };
    mesh
}

// ---------------------------------------------------------------------------
// Cylinder
// ---------------------------------------------------------------------------

/// 生成圆柱体网格（侧面 + 顶盖 + 底盖）。
///
/// - `radius`: 底面半径
/// - `height`: 高度（沿 Y 轴，居中）
/// - `segments`: 圆周分段数
pub fn create_cylinder_mesh_procedural(radius: f32, height: f32, segments: u32) -> ModelMesh {
    let mut vertices: Vec<Vertex> = Vec::new();
    let mut indices: Vec<u32> = Vec::new();

    let half_h = height * 0.5;
    let pi = std::f32::consts::PI;

    // --- 侧面 ---
    // 上下各一圈顶点（法线水平朝外）
    for i in 0..=segments {
        let theta = 2.0 * pi * i as f32 / segments as f32;
        let cos_t = theta.cos();
        let sin_t = theta.sin();

        let nx = cos_t;
        let nz = sin_t;
        let u = i as f32 / segments as f32;

        // 下圈
        vertices.push(Vertex {
            position: Point3::new(radius * cos_t, -half_h, radius * sin_t),
            normal: Vector3::new(nx, 0.0, nz),
            uv: Vector2::new(u, 0.0),
            tangent: [-sin_t, 0.0, cos_t, 1.0],
            joints: [0, 0, 0, 0],
            weights: [1.0, 0.0, 0.0, 0.0],
        });
        // 上圈
        vertices.push(Vertex {
            position: Point3::new(radius * cos_t, half_h, radius * sin_t),
            normal: Vector3::new(nx, 0.0, nz),
            uv: Vector2::new(u, 1.0),
            tangent: [-sin_t, 0.0, cos_t, 1.0],
            joints: [0, 0, 0, 0],
            weights: [1.0, 0.0, 0.0, 0.0],
        });
    }

    // 侧面索引
    for i in 0..segments {
        let base = i * 2;
        let a = base;
        let b = base + 1;
        let c = base + 2;
        let d = base + 3;

        indices.push(a);
        indices.push(c);
        indices.push(b);

        indices.push(b);
        indices.push(c);
        indices.push(d);
    }

    // --- 顶盖 ---
    let top_center_idx = vertices.len() as u32;
    vertices.push(Vertex {
        position: Point3::new(0.0, half_h, 0.0),
        normal: Vector3::new(0.0, 1.0, 0.0),
        uv: Vector2::new(0.5, 0.5),
        tangent: [1.0, 0.0, 0.0, 1.0],
        joints: [0, 0, 0, 0],
        weights: [1.0, 0.0, 0.0, 0.0],
    });

    let top_ring_start = vertices.len() as u32;
    for i in 0..=segments {
        let theta = 2.0 * pi * i as f32 / segments as f32;
        let cos_t = theta.cos();
        let sin_t = theta.sin();
        vertices.push(Vertex {
            position: Point3::new(radius * cos_t, half_h, radius * sin_t),
            normal: Vector3::new(0.0, 1.0, 0.0),
            uv: Vector2::new(0.5 + 0.5 * cos_t, 0.5 + 0.5 * sin_t),
            tangent: [1.0, 0.0, 0.0, 1.0],
            joints: [0, 0, 0, 0],
            weights: [1.0, 0.0, 0.0, 0.0],
        });
    }

    for i in 0..segments {
        indices.push(top_center_idx);
        indices.push(top_ring_start + i);
        indices.push(top_ring_start + i + 1);
    }

    // --- 底盖 ---
    let bottom_center_idx = vertices.len() as u32;
    vertices.push(Vertex {
        position: Point3::new(0.0, -half_h, 0.0),
        normal: Vector3::new(0.0, -1.0, 0.0),
        uv: Vector2::new(0.5, 0.5),
        tangent: [1.0, 0.0, 0.0, 1.0],
        joints: [0, 0, 0, 0],
        weights: [1.0, 0.0, 0.0, 0.0],
    });

    let bottom_ring_start = vertices.len() as u32;
    for i in 0..=segments {
        let theta = 2.0 * pi * i as f32 / segments as f32;
        let cos_t = theta.cos();
        let sin_t = theta.sin();
        vertices.push(Vertex {
            position: Point3::new(radius * cos_t, -half_h, radius * sin_t),
            normal: Vector3::new(0.0, -1.0, 0.0),
            uv: Vector2::new(0.5 + 0.5 * cos_t, 0.5 + 0.5 * sin_t),
            tangent: [1.0, 0.0, 0.0, 1.0],
            joints: [0, 0, 0, 0],
            weights: [1.0, 0.0, 0.0, 0.0],
        });
    }

    for i in 0..segments {
        indices.push(bottom_center_idx);
        indices.push(bottom_ring_start + i + 1);
        indices.push(bottom_ring_start + i);
    }

    let mut mesh = ModelMesh::new();
    mesh.vertices = vertices;
    mesh.indices = indices;
    mesh.flags = MeshFlags {
        has_normals: true,
        has_uv0: true,
        has_tangents: true,
        has_skin: false,
    };
    mesh
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_mesh_valid(mesh: &ModelMesh, expected_vertices: usize, expected_indices: usize) {
        assert_eq!(mesh.vertices.len(), expected_vertices, "vertex count mismatch");
        assert_eq!(mesh.indices.len(), expected_indices, "index count mismatch");
        assert!(mesh.flags.has_normals);
        assert!(mesh.flags.has_uv0);

        // 索引不超出顶点范围
        for &idx in &mesh.indices {
            assert!(
                (idx as usize) < mesh.vertices.len(),
                "index {} out of range (vertices={})",
                idx,
                mesh.vertices.len()
            );
        }

        // 法线归一化
        for v in &mesh.vertices {
            let len = (v.normal.x * v.normal.x + v.normal.y * v.normal.y + v.normal.z * v.normal.z).sqrt();
            assert!(
                (len - 1.0).abs() < 1e-5,
                "normal not normalized: len={}",
                len
            );
        }

        // UV 在 [0, 1] 范围
        for v in &mesh.vertices {
            assert!(v.uv.x >= 0.0 && v.uv.x <= 1.0, "uv.x out of range: {}", v.uv.x);
            assert!(v.uv.y >= 0.0 && v.uv.y <= 1.0, "uv.y out of range: {}", v.uv.y);
        }
    }

    #[test]
    fn test_plane() {
        let mesh = create_plane_mesh_procedural(2.0, 3.0);
        assert_mesh_valid(&mesh, 4, 6);
    }

    #[test]
    fn test_cube() {
        let mesh = create_cube_mesh_procedural(1.0, 1.0, 1.0);
        assert_mesh_valid(&mesh, 24, 36);
    }

    #[test]
    fn test_sphere() {
        let segments = 32u32;
        let rings = 16u32;
        let mesh = create_sphere_mesh_procedural(0.5, segments, rings);
        let expected_verts = ((rings + 1) * (segments + 1)) as usize;
        let expected_indices = (rings * segments * 6) as usize;
        assert_mesh_valid(&mesh, expected_verts, expected_indices);
    }

    #[test]
    fn test_cylinder() {
        let segments = 32u32;
        let mesh = create_cylinder_mesh_procedural(0.5, 1.0, segments);
        // 侧面: (segments+1)*2, 顶盖: 1 + segments+1, 底盖: 1 + segments+1
        let expected_verts = ((segments + 1) * 2 + 1 + (segments + 1) + 1 + (segments + 1)) as usize;
        // 侧面: segments*6, 顶盖: segments*3, 底盖: segments*3
        let expected_indices = (segments * 6 + segments * 3 + segments * 3) as usize;
        assert_mesh_valid(&mesh, expected_verts, expected_indices);
    }

    #[test]
    fn test_primitive_kind_from_str() {
        assert_eq!(primitive_kind_from_str("cube"), Some(PrimitiveKind::Cube));
        assert_eq!(primitive_kind_from_str("sphere"), Some(PrimitiveKind::Sphere));
        assert_eq!(primitive_kind_from_str("plane"), Some(PrimitiveKind::Plane));
        assert_eq!(primitive_kind_from_str("cylinder"), Some(PrimitiveKind::Cylinder));
        assert_eq!(primitive_kind_from_str("unknown"), None);
    }

    #[test]
    fn test_create_primitive_mesh_dispatch() {
        for kind in [PrimitiveKind::Cube, PrimitiveKind::Sphere, PrimitiveKind::Plane, PrimitiveKind::Cylinder] {
            let mesh = create_primitive_mesh(kind);
            assert!(!mesh.vertices.is_empty(), "{:?} should have vertices", kind);
            assert!(!mesh.indices.is_empty(), "{:?} should have indices", kind);
        }
    }
}
