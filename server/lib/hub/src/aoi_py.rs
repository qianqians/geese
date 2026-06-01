//! AOI（Area of Interest）的 pyo3 绑定层。
//!
//! 不在此处声明 `#[pymodule]`，而是提供 [`add_to_module`]，由宿主进程的
//! 顶层 pymodule（server 的 `pyhub`）调用，将 AOI 类挂到现有命名空间。
//!
//! 暴露给 Python：
//! - `AoiGrid`：九宫格 AOI 容器；接受 entity_id 为 u64
//! - `take_events()` 返回 `list[(str, int, int)]`

use pyo3::prelude::*;

use aoi::{Aoi, AoiEvent, EntityId, GridAoi};

#[pyclass(module = "pyhub", name = "AoiGrid")]
pub struct PyGridAoi {
    inner: GridAoi,
}

#[pymethods]
impl PyGridAoi {
    /// 构造九宫格 AOI；`cell_size` 为单格边长（XZ 平面）。
    #[new]
    fn new(cell_size: f32) -> Self {
        Self { inner: GridAoi::new(cell_size) }
    }

    /// 插入一个 entity；如果 id 已存在则等价于 `update`。
    ///
    /// `position` 为 `(x, y, z)`；AOI 仅取 XZ 投影。
    /// `radius` 为兴趣半径（米），按 `cell_size` 换算成格半径（向上取整，最少 1）。
    fn insert(&mut self, id: u64, position: (f32, f32, f32), radius: f32) {
        self.inner
            .insert(id as EntityId, [position.0, position.1, position.2], radius);
    }

    /// 更新位置；不存在时静默忽略。
    fn update(&mut self, id: u64, position: (f32, f32, f32)) {
        self.inner
            .update(id as EntityId, [position.0, position.1, position.2]);
    }

    /// 移除 entity；自动给所有曾经看见自己的 observer 发 `Leave`。
    fn remove(&mut self, id: u64) {
        self.inner.remove(id as EntityId);
    }

    /// 返回 `target` 当前被哪些 observer 看见。
    fn observers(&self, target: u64) -> Vec<u64> {
        self.inner
            .observers(target as EntityId)
            .into_iter()
            .map(|id| id as u64)
            .collect()
    }

    /// 弹出累积的 Enter/Leave 事件；调用后内部清空。
    /// 返回 `list[(kind, observer, target)]`，`kind` ∈ {"enter", "leave"}。
    fn take_events(&mut self) -> Vec<(&'static str, u64, u64)> {
        self.inner
            .take_events()
            .into_iter()
            .map(|e| match e {
                AoiEvent::Enter { observer, target } => ("enter", observer as u64, target as u64),
                AoiEvent::Leave { observer, target } => ("leave", observer as u64, target as u64),
            })
            .collect()
    }

    /// 当前管理的 entity 数量。
    fn entity_count(&self) -> usize {
        self.inner.entity_count()
    }

    fn __repr__(&self) -> String {
        format!("AoiGrid(entity_count={})", self.inner.entity_count())
    }
}

/// 把 AOI pyclass 全部挂到给定 Python 模块。
pub fn add_to_module(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PyGridAoi>()?;
    Ok(())
}