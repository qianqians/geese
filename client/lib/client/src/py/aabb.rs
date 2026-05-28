//! `PyAABB`：轴对齐包围盒 pyo3 包装。

use pyo3::prelude::*;

use math::AABB;

#[pyclass(module = "pyclient", name = "AABB")]
#[derive(Clone, Copy)]
pub struct PyAABB {
    pub inner: AABB,
}

#[pymethods]
impl PyAABB {
    #[new]
    fn new(min: (f32, f32, f32), max: (f32, f32, f32)) -> Self {
        Self {
            inner: AABB::new(
                cgmath::Point3::new(min.0, min.1, min.2),
                cgmath::Point3::new(max.0, max.1, max.2),
            ),
        }
    }

    #[getter]
    fn min(&self) -> (f32, f32, f32) {
        (self.inner.min.x, self.inner.min.y, self.inner.min.z)
    }

    #[getter]
    fn max(&self) -> (f32, f32, f32) {
        (self.inner.max.x, self.inner.max.y, self.inner.max.z)
    }

    #[getter]
    fn center(&self) -> (f32, f32, f32) {
        let c = self.inner.center();
        (c.x, c.y, c.z)
    }

    #[getter]
    fn size(&self) -> (f32, f32, f32) {
        let s = self.inner.size();
        (s.x, s.y, s.z)
    }

    fn contains_point(&self, p: (f32, f32, f32)) -> bool {
        self.inner.contains_point(cgmath::Point3::new(p.0, p.1, p.2))
    }

    fn intersects(&self, other: &PyAABB) -> bool {
        self.inner.intersects_aabb(&other.inner)
    }

    fn __repr__(&self) -> String {
        format!(
            "AABB(min=({:.4},{:.4},{:.4}), max=({:.4},{:.4},{:.4}))",
            self.inner.min.x,
            self.inner.min.y,
            self.inner.min.z,
            self.inner.max.x,
            self.inner.max.y,
            self.inner.max.z,
        )
    }
}
