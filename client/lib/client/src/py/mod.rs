//! 客户端 pyo3 绑定层。
//!
//! 把 `crates/camera`、`crates/scene` 等渲染相关模块的纯逻辑通过本模块统一
//! 暴露给 Python。这些 PyClass **集中在 client 子 crate** 中维护，渲染相关
//! 的纯算法 crate（camera、scene）保持零 pyo3 依赖。
//!
//! 暴露内容：
//! - 相机：`Plane` / `Frustum`
//! - 场景：`AABB` / `Transform` / `SceneObject` / `SceneNode` / `Scene`
//!
//! 由 [`crate::add_to_module`] 调用 [`add_to_module`] 把全部 pyclass 挂到
//! 顶层 cdylib `pyclient`。

use pyo3::prelude::*;

pub mod aabb;
pub mod camera;
pub mod scene;
pub mod scene_object;
pub mod transform;

pub use aabb::PyAABB;
pub use camera::{PyFrustum, PyPlane};
pub use scene::PyScene;
pub use scene_object::{PySceneNode, PySceneObject};
pub use transform::PyTransform;

/// 把全部渲染相关 pyclass 挂到给定 Python 模块。
pub fn add_to_module(m: &Bound<'_, PyModule>) -> PyResult<()> {
    // camera
    m.add_class::<PyPlane>()?;
    m.add_class::<PyFrustum>()?;
    // scene
    m.add_class::<PyAABB>()?;
    m.add_class::<PyTransform>()?;
    m.add_class::<PySceneObject>()?;
    m.add_class::<PySceneNode>()?;
    m.add_class::<PyScene>()?;
    Ok(())
}
