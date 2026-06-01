//! `PyPlane` / `PyFrustum`：相机模块的 pyo3 包装。

use cgmath::{Matrix4, Point3};
use pyo3::prelude::*;

use ::camera::frustum::{Frustum, Plane};

#[pyclass(module = "pyclient", name = "Plane")]
#[derive(Clone)]
pub struct PyPlane {
    pub inner: Plane,
}

#[pymethods]
impl PyPlane {
    #[new]
    fn new(a: f32, b: f32, c: f32, d: f32) -> Self {
        Self {
            inner: Plane::from_coefficients(a, b, c, d),
        }
    }

    #[getter]
    fn normal(&self) -> (f32, f32, f32) {
        (self.inner.normal.x, self.inner.normal.y, self.inner.normal.z)
    }

    #[getter]
    fn distance(&self) -> f32 {
        self.inner.distance
    }

    fn distance_to_point(&self, p: (f32, f32, f32)) -> f32 {
        self.inner.distance_to_point(Point3::new(p.0, p.1, p.2))
    }

    fn __repr__(&self) -> String {
        format!(
            "Plane(normal=({:.4},{:.4},{:.4}), d={:.4})",
            self.inner.normal.x, self.inner.normal.y, self.inner.normal.z, self.inner.distance
        )
    }
}

/// 视锥体 pyclass。Python 端通过 view_projection 矩阵构造。
#[pyclass(module = "pyclient", name = "Frustum")]
#[derive(Clone)]
pub struct PyFrustum {
    pub inner: Frustum,
}

impl PyFrustum {
    pub fn inner(&self) -> &Frustum {
        &self.inner
    }
}

#[pymethods]
impl PyFrustum {
    /// 从 4x4 view-projection 矩阵构造视锥体。
    /// 矩阵接受 row-major（4 行 × 4 列）的 Python 二维列表。
    #[staticmethod]
    fn from_view_projection(matrix: [[f32; 4]; 4]) -> Self {
        // cgmath::Matrix4 是 column-major（m[col][row]），传入参数视为行主序，
        // 所以这里需要做一次转置。
        let m = Matrix4::new(
            matrix[0][0], matrix[1][0], matrix[2][0], matrix[3][0],
            matrix[0][1], matrix[1][1], matrix[2][1], matrix[3][1],
            matrix[0][2], matrix[1][2], matrix[2][2], matrix[3][2],
            matrix[0][3], matrix[1][3], matrix[2][3], matrix[3][3],
        );
        Self {
            inner: Frustum::from_view_projection_matrix(&m),
        }
    }

    /// 按列主序（与 cgmath 一致）从 4x4 矩阵构造视锥体。
    #[staticmethod]
    fn from_view_projection_column_major(matrix: [[f32; 4]; 4]) -> Self {
        let m = Matrix4::new(
            matrix[0][0], matrix[0][1], matrix[0][2], matrix[0][3],
            matrix[1][0], matrix[1][1], matrix[1][2], matrix[1][3],
            matrix[2][0], matrix[2][1], matrix[2][2], matrix[2][3],
            matrix[3][0], matrix[3][1], matrix[3][2], matrix[3][3],
        );
        Self {
            inner: Frustum::from_view_projection_matrix(&m),
        }
    }

    fn contains_point(&self, p: (f32, f32, f32)) -> bool {
        self.inner.contains_point(Point3::new(p.0, p.1, p.2))
    }

    fn contains_sphere(&self, center: (f32, f32, f32), radius: f32) -> bool {
        self.inner
            .contains_sphere(Point3::new(center.0, center.1, center.2), radius)
    }

    fn contains_aabb(&self, min: (f32, f32, f32), max: (f32, f32, f32)) -> bool {
        self.inner.contains_aabb(
            Point3::new(min.0, min.1, min.2),
            Point3::new(max.0, max.1, max.2),
        )
    }

    fn intersects_aabb(&self, min: (f32, f32, f32), max: (f32, f32, f32)) -> bool {
        self.inner.intersects_aabb(
            Point3::new(min.0, min.1, min.2),
            Point3::new(max.0, max.1, max.2),
        )
    }

    /// 返回 6 个平面 (left, right, bottom, top, near, far) 的拷贝。
    fn planes(&self) -> Vec<PyPlane> {
        self.inner
            .planes
            .iter()
            .copied()
            .map(|p| PyPlane { inner: p })
            .collect()
    }

    fn __repr__(&self) -> String {
        "Frustum(6 planes)".to_string()
    }
}
