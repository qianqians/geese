//! `PyShape`：形状描述的 pyo3 包装（不可变值类型）。

use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;

use crate::shapes::ShapeDesc;

#[pyclass(module = "pyhub", name = "PhysicsShape")]
#[derive(Clone)]
pub struct PyShape {
    pub(crate) inner: ShapeDesc,
}

impl PyShape {
    pub(crate) fn from_desc(desc: ShapeDesc) -> Self {
        Self { inner: desc }
    }

    pub(crate) fn desc(&self) -> ShapeDesc {
        self.inner.clone()
    }
}

#[pymethods]
impl PyShape {
    #[staticmethod]
    fn cuboid(hx: f32, hy: f32, hz: f32) -> Self {
        Self::from_desc(ShapeDesc::cuboid(hx, hy, hz))
    }

    #[staticmethod]
    fn ball(radius: f32) -> Self {
        Self::from_desc(ShapeDesc::ball(radius))
    }

    #[staticmethod]
    fn capsule(half_height: f32, radius: f32) -> Self {
        Self::from_desc(ShapeDesc::capsule(half_height, radius))
    }

    #[staticmethod]
    fn cylinder(half_height: f32, radius: f32) -> Self {
        Self::from_desc(ShapeDesc::cylinder(half_height, radius))
    }

    /// `vertices`: List[Tuple[f32,f32,f32]]; `indices`: List[Tuple[u32,u32,u32]]。
    #[staticmethod]
    fn trimesh(
        vertices: Vec<(f32, f32, f32)>,
        indices: Vec<(u32, u32, u32)>,
    ) -> PyResult<Self> {
        if vertices.is_empty() || indices.is_empty() {
            return Err(PyValueError::new_err(
                "trimesh requires non-empty vertices/indices",
            ));
        }
        let verts: Vec<[f32; 3]> = vertices.into_iter().map(|v| [v.0, v.1, v.2]).collect();
        let inds: Vec<[u32; 3]> = indices.into_iter().map(|i| [i.0, i.1, i.2]).collect();
        Ok(Self::from_desc(ShapeDesc::trimesh(verts, inds)))
    }

    fn __repr__(&self) -> String {
        match &self.inner {
            ShapeDesc::Cuboid { half_extents } => format!(
                "PhysicsShape::Cuboid({}, {}, {})",
                half_extents.x, half_extents.y, half_extents.z
            ),
            ShapeDesc::Ball { radius } => format!("PhysicsShape::Ball({radius})"),
            ShapeDesc::Capsule {
                half_height,
                radius,
            } => format!("PhysicsShape::Capsule({half_height}, {radius})"),
            ShapeDesc::Cylinder {
                half_height,
                radius,
            } => format!("PhysicsShape::Cylinder({half_height}, {radius})"),
            ShapeDesc::TriMesh { vertices, indices } => {
                format!(
                    "PhysicsShape::TriMesh(verts={}, tris={})",
                    vertices.len(),
                    indices.len()
                )
            }
        }
    }
}
