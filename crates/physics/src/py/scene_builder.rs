//! `load_gltf_collision_shapes` pyfunction（feature = "scene-builder"）。
//!
//! 从 GLTF 文件提取碰撞形状列表，返回 `List[PhysicsShape]`。

use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;

use crate::py::shape::PyShape;
use crate::shapes::ShapeDesc;

/// 从 GLTF 文件提取碰撞三角网格形状。
///
/// 仅读取顶点位置与三角形索引；忽略纹理、材质、法线等渲染数据。
/// 顶点的 GLTF 节点世界变换已预先应用。
///
/// 返回 `List[PhysicsShape]`，每个元素对应 GLTF 的一个 mesh primitive。
#[pyfunction]
fn load_gltf_collision_shapes(path: &str) -> PyResult<Vec<PyShape>> {
    let meshes = crate::scene_builder::extract_gltf_trimeshes(path)
        .map_err(|e| PyValueError::new_err(e))?;

    let shapes: Vec<PyShape> = meshes
        .into_iter()
        .map(|m| PyShape::from_desc(ShapeDesc::trimesh(m.vertices, m.indices)))
        .collect();

    Ok(shapes)
}

/// 将 `load_gltf_collision_shapes` 注册到给定 Python 模块。
pub fn add_to_module(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(load_gltf_collision_shapes, m)?)?;
    Ok(())
}
