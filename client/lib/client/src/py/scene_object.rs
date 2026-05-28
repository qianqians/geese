//! `PySceneObject` / `PySceneNode`：场景对象/节点的轻量视图 pyo3 包装。
//!
//! 注意：这两个类型是 **值拷贝** 视图，不持有对原 `Scene` 的引用。
//! 在调用 `PyScene::get_object(idx)` 时按需克隆构造。

use pyo3::prelude::*;

use scene::{SceneNode, SceneObject};

use super::aabb::PyAABB;
use super::transform::PyTransform;

#[pyclass(module = "pyclient", name = "SceneObject")]
#[derive(Clone)]
pub struct PySceneObject {
    pub(crate) inner: SceneObject,
}

impl From<SceneObject> for PySceneObject {
    fn from(inner: SceneObject) -> Self {
        Self { inner }
    }
}

#[pymethods]
impl PySceneObject {
    #[getter]
    fn entity_id(&self) -> &str {
        &self.inner.entity_id
    }

    #[getter]
    fn node(&self) -> usize {
        self.inner.node
    }

    #[getter]
    fn local_aabb(&self) -> PyAABB {
        PyAABB { inner: self.inner.local_aabb }
    }

    #[getter]
    fn aabb(&self) -> PyAABB {
        PyAABB { inner: self.inner.aabb }
    }

    #[getter]
    fn center(&self) -> (f32, f32, f32) {
        let c = self.inner.center;
        (c.x, c.y, c.z)
    }

    /// 返回行主序 4x4 模型矩阵（嵌套 list）。
    #[getter]
    fn model_matrix(&self) -> [[f32; 4]; 4] {
        column_major_to_row_major(self.inner.model_matrix)
    }

    /// 返回行主序 4x4 法线矩阵（嵌套 list）。
    #[getter]
    fn normal_matrix(&self) -> [[f32; 4]; 4] {
        column_major_to_row_major(self.inner.normal_matrix)
    }

    /// 返回所有骨骼矩阵（每个 4x4，行主序）。
    #[getter]
    fn joint_matrices(&self) -> Vec<[[f32; 4]; 4]> {
        self.inner
            .joint_matrices
            .iter()
            .copied()
            .map(column_major_to_row_major)
            .collect()
    }

    fn vertex_count(&self) -> usize {
        self.inner.mesh.vertices.len()
    }

    fn index_count(&self) -> usize {
        self.inner.mesh.indices.len()
    }

    /// 顶点位置数组（每个为 (x, y, z)）。
    fn positions(&self) -> Vec<(f32, f32, f32)> {
        self.inner
            .mesh
            .vertices
            .iter()
            .map(|v| (v.position.x, v.position.y, v.position.z))
            .collect()
    }

    /// 顶点法线数组（每个为 (x, y, z)）；未提供法线的网格此处可能为单位向量。
    fn normals(&self) -> Vec<(f32, f32, f32)> {
        self.inner
            .mesh
            .vertices
            .iter()
            .map(|v| (v.normal.x, v.normal.y, v.normal.z))
            .collect()
    }

    /// UV0 数组。
    fn uvs(&self) -> Vec<(f32, f32)> {
        self.inner
            .mesh
            .vertices
            .iter()
            .map(|v| (v.uv.x, v.uv.y))
            .collect()
    }

    /// 三角形索引数组（u32）。
    fn indices(&self) -> Vec<u32> {
        self.inner.mesh.indices.clone()
    }

    /// 该对象引用的材质句柄（None 表示无材质）。
    #[getter]
    fn material_handle(&self) -> Option<usize> {
        self.inner.mesh.material.map(|h| h.0)
    }

    /// 该对象引用的蒙皮句柄（None 表示无蒙皮）。
    #[getter]
    fn skin_handle(&self) -> Option<usize> {
        self.inner.mesh.skin.map(|h| h.0)
    }

    fn __repr__(&self) -> String {
        format!(
            "SceneObject(entity_id={}, node={}, vertices={}, indices={})",
            self.inner.entity_id,
            self.inner.node,
            self.inner.mesh.vertices.len(),
            self.inner.mesh.indices.len(),
        )
    }
}

#[pyclass(module = "pyclient", name = "SceneNode")]
#[derive(Clone)]
pub struct PySceneNode {
    pub(crate) inner: SceneNode,
}

impl From<SceneNode> for PySceneNode {
    fn from(inner: SceneNode) -> Self {
        Self { inner }
    }
}

#[pymethods]
impl PySceneNode {
    #[getter]
    fn id(&self) -> usize {
        self.inner.id
    }

    #[getter]
    fn parent(&self) -> Option<usize> {
        self.inner.parent
    }

    #[getter]
    fn children(&self) -> Vec<usize> {
        self.inner.children.clone()
    }

    #[getter]
    fn objects(&self) -> Vec<usize> {
        self.inner.objects.clone()
    }

    #[getter]
    fn base_transform(&self) -> PyTransform {
        PyTransform::from(self.inner.base_transform)
    }

    #[getter]
    fn local_transform(&self) -> PyTransform {
        PyTransform::from(self.inner.local_transform)
    }

    /// 返回行主序 4x4 世界变换矩阵。
    #[getter]
    fn world_transform(&self) -> [[f32; 4]; 4] {
        let m = self.inner.world_transform;
        [
            [m[0][0], m[1][0], m[2][0], m[3][0]],
            [m[0][1], m[1][1], m[2][1], m[3][1]],
            [m[0][2], m[1][2], m[2][2], m[3][2]],
            [m[0][3], m[1][3], m[2][3], m[3][3]],
        ]
    }

    fn __repr__(&self) -> String {
        format!(
            "SceneNode(id={}, parent={:?}, children={}, objects={})",
            self.inner.id,
            self.inner.parent,
            self.inner.children.len(),
            self.inner.objects.len(),
        )
    }
}

/// 把 column-major 4x4 矩阵转换为 row-major（外层 list 是行）。
fn column_major_to_row_major(m: [[f32; 4]; 4]) -> [[f32; 4]; 4] {
    [
        [m[0][0], m[1][0], m[2][0], m[3][0]],
        [m[0][1], m[1][1], m[2][1], m[3][1]],
        [m[0][2], m[1][2], m[2][2], m[3][2]],
        [m[0][3], m[1][3], m[2][3], m[3][3]],
    ]
}
