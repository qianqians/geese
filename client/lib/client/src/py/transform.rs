//! `PyTransform`：T/R/S 三元组 pyo3 包装。

use pyo3::prelude::*;

use scene::Transform;

#[pyclass(module = "pyclient", name = "Transform")]
#[derive(Clone, Copy)]
pub struct PyTransform {
    pub inner: Transform,
}

impl From<Transform> for PyTransform {
    fn from(inner: Transform) -> Self {
        Self { inner }
    }
}

#[pymethods]
impl PyTransform {
    #[getter]
    fn translation(&self) -> (f32, f32, f32) {
        (
            self.inner.translation.x,
            self.inner.translation.y,
            self.inner.translation.z,
        )
    }

    /// 返回 (x, y, z, w) 四元数。
    #[getter]
    fn rotation(&self) -> (f32, f32, f32, f32) {
        let q = self.inner.rotation;
        (q.v.x, q.v.y, q.v.z, q.s)
    }

    #[getter]
    fn scale(&self) -> (f32, f32, f32) {
        (self.inner.scale.x, self.inner.scale.y, self.inner.scale.z)
    }

    /// 返回行主序 4x4 矩阵（嵌套 list）。
    fn matrix(&self) -> [[f32; 4]; 4] {
        let m = self.inner.matrix();
        // cgmath::Matrix4 是 column-major: m[col][row]，转为 row-major 输出。
        [
            [m[0][0], m[1][0], m[2][0], m[3][0]],
            [m[0][1], m[1][1], m[2][1], m[3][1]],
            [m[0][2], m[1][2], m[2][2], m[3][2]],
            [m[0][3], m[1][3], m[2][3], m[3][3]],
        ]
    }

    fn __repr__(&self) -> String {
        format!(
            "Transform(t=({:.3},{:.3},{:.3}), r=({:.3},{:.3},{:.3},{:.3}), s=({:.3},{:.3},{:.3}))",
            self.inner.translation.x,
            self.inner.translation.y,
            self.inner.translation.z,
            self.inner.rotation.v.x,
            self.inner.rotation.v.y,
            self.inner.rotation.v.z,
            self.inner.rotation.s,
            self.inner.scale.x,
            self.inner.scale.y,
            self.inner.scale.z,
        )
    }
}
