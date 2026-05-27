//! 形状描述：构造 rapier 的 `SharedShape`。

use rapier3d::math::Vector;
use rapier3d::prelude as rp;

use crate::math::Vec3;

/// 与 pyo3 / 业务层无关的形状描述。
#[derive(Debug, Clone)]
pub enum ShapeDesc {
    /// 立方体；半边长 (hx, hy, hz)。
    Cuboid { half_extents: Vec3 },
    /// 球；半径。
    Ball { radius: f32 },
    /// 胶囊：沿 Y 轴；half_height 为圆柱段半高。
    Capsule { half_height: f32, radius: f32 },
    /// 圆柱：沿 Y 轴。
    Cylinder { half_height: f32, radius: f32 },
    /// 三角形网格。
    TriMesh {
        vertices: Vec<[f32; 3]>,
        indices: Vec<[u32; 3]>,
    },
}

impl ShapeDesc {
    pub fn cuboid(hx: f32, hy: f32, hz: f32) -> Self {
        ShapeDesc::Cuboid {
            half_extents: Vec3::new(hx, hy, hz),
        }
    }

    pub fn ball(radius: f32) -> Self {
        ShapeDesc::Ball { radius }
    }

    pub fn capsule(half_height: f32, radius: f32) -> Self {
        ShapeDesc::Capsule {
            half_height,
            radius,
        }
    }

    pub fn cylinder(half_height: f32, radius: f32) -> Self {
        ShapeDesc::Cylinder {
            half_height,
            radius,
        }
    }

    pub fn trimesh(vertices: Vec<[f32; 3]>, indices: Vec<[u32; 3]>) -> Self {
        ShapeDesc::TriMesh { vertices, indices }
    }

    pub(crate) fn into_shared(self) -> Result<rp::SharedShape, String> {
        match self {
            ShapeDesc::Cuboid { half_extents } => Ok(rp::SharedShape::cuboid(
                half_extents.x.max(1e-6),
                half_extents.y.max(1e-6),
                half_extents.z.max(1e-6),
            )),
            ShapeDesc::Ball { radius } => Ok(rp::SharedShape::ball(radius.max(1e-6))),
            ShapeDesc::Capsule {
                half_height,
                radius,
            } => Ok(rp::SharedShape::capsule_y(
                half_height.max(0.0),
                radius.max(1e-6),
            )),
            ShapeDesc::Cylinder {
                half_height,
                radius,
            } => Ok(rp::SharedShape::cylinder(
                half_height.max(1e-6),
                radius.max(1e-6),
            )),
            ShapeDesc::TriMesh { vertices, indices } => {
                if vertices.is_empty() || indices.is_empty() {
                    return Err("trimesh requires non-empty vertices/indices".to_string());
                }
                let verts: Vec<Vector> = vertices
                    .into_iter()
                    .map(|p| Vector::new(p[0], p[1], p[2]))
                    .collect();
                rp::SharedShape::trimesh(verts, indices).map_err(|e| format!("{:?}", e))
            }
        }
    }
}
