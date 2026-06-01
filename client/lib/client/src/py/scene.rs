//! `PyScene`：场景容器 pyo3 包装。
//!
//! 内部用 `Arc<Mutex<Scene>>` 串行化访问，支持从 Python 侧导入 GLTF、
//! 查询节点/对象、做视锥体可见性查询、播放动画等。

use std::sync::{Arc, Mutex};

use pyo3::exceptions::{PyRuntimeError, PyValueError};
use pyo3::prelude::*;

use scene::{Scene, import_scene};
use avatar::AnimationPlayer;

use super::aabb::PyAABB;
use super::camera::PyFrustum;
use super::scene_object::{PySceneNode, PySceneObject};

pub(crate) type SharedScene = Arc<Mutex<Scene>>;

#[pyclass(module = "pyclient", name = "Scene")]
pub struct PyScene {
    pub(crate) inner: SharedScene,
}

impl PyScene {
    pub(crate) fn share(&self) -> SharedScene {
        Arc::clone(&self.inner)
    }
}

fn lock_scene(scene: &SharedScene) -> PyResult<std::sync::MutexGuard<'_, Scene>> {
    scene
        .lock()
        .map_err(|e| PyRuntimeError::new_err(format!("scene lock poisoned: {e}")))
}

#[pymethods]
impl PyScene {
    /// 从 GLTF/GLB 文件导入场景。
    ///
    /// `max_objects`：八叉树叶子最多承载对象数；
    /// `max_depth`：八叉树最大深度。
    #[staticmethod]
    #[pyo3(signature = (path, max_objects = 8, max_depth = 6))]
    fn import_gltf(path: String, max_objects: usize, max_depth: usize) -> PyResult<Self> {
        let scene = import_scene(path, max_objects, max_depth)
            .map_err(|e| PyRuntimeError::new_err(format!("import gltf failed: {e}")))?;
        Ok(Self {
            inner: Arc::new(Mutex::new(scene)),
        })
    }

    fn node_count(&self) -> PyResult<usize> {
        Ok(lock_scene(&self.inner)?.nodes.len())
    }

    fn object_count(&self) -> PyResult<usize> {
        Ok(lock_scene(&self.inner)?.objects.len())
    }

    fn animation_count(&self) -> PyResult<usize> {
        Ok(lock_scene(&self.inner)?.animations.len())
    }

    fn skin_count(&self) -> PyResult<usize> {
        Ok(lock_scene(&self.inner)?.skins.len())
    }

    /// 查询动画在 clip 数组中的索引。
    fn animation_index(&self, name: &str) -> PyResult<Option<usize>> {
        Ok(lock_scene(&self.inner)?.animation_index(name))
    }

    /// 查询动画时长（秒）。
    fn animation_duration(&self, index: usize) -> PyResult<Option<f32>> {
        Ok(lock_scene(&self.inner)?.animation_duration(index))
    }

    /// 列出所有动画名称（无名动画返回 None）。
    fn animation_names(&self) -> PyResult<Vec<Option<String>>> {
        Ok(lock_scene(&self.inner)?
            .animations
            .iter()
            .map(|a| a.name.clone())
            .collect())
    }

    /// 取节点轻量视图。越界返回 None。
    fn get_node(&self, idx: usize) -> PyResult<Option<PySceneNode>> {
        let scene = lock_scene(&self.inner)?;
        Ok(scene.nodes.get(idx).cloned().map(PySceneNode::from))
    }

    /// 取对象轻量视图。越界返回 None。
    fn get_object(&self, idx: usize) -> PyResult<Option<PySceneObject>> {
        let scene = lock_scene(&self.inner)?;
        Ok(scene.objects.get(idx).cloned().map(PySceneObject::from))
    }

    /// 列出所有顶级节点（parent 为 None 的节点 id）。
    fn root_nodes(&self) -> PyResult<Vec<usize>> {
        let scene = lock_scene(&self.inner)?;
        Ok(scene
            .nodes
            .iter()
            .filter(|n| n.parent.is_none())
            .map(|n| n.id)
            .collect())
    }

    /// 取场景全部对象（拷贝）。注意大场景可能开销较大。
    fn all_objects(&self) -> PyResult<Vec<PySceneObject>> {
        let scene = lock_scene(&self.inner)?;
        Ok(scene
            .objects()
            .into_iter()
            .cloned()
            .map(PySceneObject::from)
            .collect())
    }

    /// 给定视锥体，返回可见的对象（拷贝）。
    fn visible_objects(&self, frustum: &PyFrustum) -> PyResult<Vec<PySceneObject>> {
        let scene = lock_scene(&self.inner)?;
        Ok(scene
            .visible_objects(frustum.inner())
            .into_iter()
            .cloned()
            .map(PySceneObject::from)
            .collect())
    }

    /// 重新计算所有节点的世界矩阵。
    fn update_world_transforms(&self) -> PyResult<()> {
        let mut scene = lock_scene(&self.inner)?;
        scene.update_world_transforms();
        Ok(())
    }

    /// 重建八叉树（在外部直接修改对象 AABB 后调用）。
    fn rebuild_octree(&self) -> PyResult<()> {
        let mut scene = lock_scene(&self.inner)?;
        scene.rebuild_octree();
        Ok(())
    }

    /// 推进单个 AnimationPlayer 状态：
    /// - `clip_index`：要播放的动画索引；
    /// - `time`：上一次的累计时间；
    /// - `dt`：本帧增量；
    /// - `speed` / `looping` / `playing`：播放参数。
    ///
    /// 返回推进后的 (time, playing)。
    #[pyo3(signature = (clip_index, time, dt, speed = 1.0, looping = true, playing = true))]
    fn update_animation(
        &self,
        clip_index: usize,
        time: f32,
        dt: f32,
        speed: f32,
        looping: bool,
        playing: bool,
    ) -> PyResult<(f32, bool)> {
        let mut scene = lock_scene(&self.inner)?;
        if clip_index >= scene.animations.len() {
            return Err(PyValueError::new_err("clip_index out of range"));
        }
        let mut player = AnimationPlayer {
            clip: clip_index,
            time,
            speed,
            looping,
            playing,
        };
        scene.update_animation(&mut player, dt);
        Ok((player.time, player.playing))
    }

    /// 取场景包围盒（基于八叉树根节点）。这里返回一个简化的 AABB，
    /// 通过遍历对象 aabb 求并得到。如果没有对象则返回 None。
    fn bounds(&self) -> PyResult<Option<PyAABB>> {
        let scene = lock_scene(&self.inner)?;
        if scene.objects.is_empty() {
            return Ok(None);
        }
        let mut min = scene.objects[0].aabb.min;
        let mut max = scene.objects[0].aabb.max;
        for obj in &scene.objects[1..] {
            min.x = min.x.min(obj.aabb.min.x);
            min.y = min.y.min(obj.aabb.min.y);
            min.z = min.z.min(obj.aabb.min.z);
            max.x = max.x.max(obj.aabb.max.x);
            max.y = max.y.max(obj.aabb.max.y);
            max.z = max.z.max(obj.aabb.max.z);
        }
        Ok(Some(PyAABB {
            inner: math::AABB::new(min, max),
        }))
    }

    fn __repr__(&self) -> PyResult<String> {
        let scene = lock_scene(&self.inner)?;
        Ok(format!(
            "Scene(nodes={}, objects={}, animations={}, skins={})",
            scene.nodes.len(),
            scene.objects.len(),
            scene.animations.len(),
            scene.skins.len(),
        ))
    }
}
