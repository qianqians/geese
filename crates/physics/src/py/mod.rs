//! pyo3 绑定层（feature = "pyo3"）。
//!
//! 不在此处声明 `#[pymodule]`，而是提供 [`add_to_module`]，由宿主进程的
//! 顶层 pymodule（例如 server 的 `pyhub`）调用，将物理类挂载到现有命名空间。

use pyo3::prelude::*;

pub mod body;
pub mod query;
pub mod shape;
pub mod world;

pub use body::PyBody;
pub use query::PyRayHit;
pub use shape::PyShape;
pub use world::{PyCollisionEvent, PyPhysicsWorld};

/// 把物理 pyclass 全部挂到给定 Python 模块。
pub fn add_to_module(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PyPhysicsWorld>()?;
    m.add_class::<PyShape>()?;
    m.add_class::<PyBody>()?;
    m.add_class::<PyRayHit>()?;
    m.add_class::<PyCollisionEvent>()?;
    m.add_function(wrap_pyfunction!(query::cast_ray, m)?)?;
    Ok(())
}
