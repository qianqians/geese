//! `PyPhysicsWorld`：物理世界 pyo3 包装。
//!
//! 内部用 `Arc<Mutex<PhysicsWorld>>` 串行化访问;多个 `PyBody` 共享同一引用。

use std::sync::{Arc, Mutex};

use pyo3::exceptions::{PyRuntimeError, PyValueError};
use pyo3::prelude::*;
use slotmap::{Key, KeyData};

use crate::handles::SceneId;
use crate::math::Vec3;
use crate::world::PhysicsWorld;

/// 内部 Arc 别名，供同 crate 其他 py 模块复用。
pub(crate) type SharedWorld = Arc<Mutex<PhysicsWorld>>;

#[pyclass(module = "pyhub", name = "PhysicsWorld")]
pub struct PyPhysicsWorld {
    pub(crate) inner: SharedWorld,
}

impl PyPhysicsWorld {
    pub(crate) fn share(&self) -> SharedWorld {
        Arc::clone(&self.inner)
    }
}

pub(crate) fn scene_id_to_u64(id: SceneId) -> u64 {
    id.data().as_ffi()
}

pub(crate) fn scene_id_from_u64(value: u64) -> SceneId {
    SceneId::from(KeyData::from_ffi(value))
}

#[pymethods]
impl PyPhysicsWorld {
    #[new]
    fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(PhysicsWorld::new())),
        }
    }

    /// 创建一个新的物理场景，返回 scene_id（u64 不透明句柄）。
    fn create_scene(&self, gravity: (f32, f32, f32)) -> PyResult<u64> {
        let mut world = lock_world(&self.inner)?;
        let id = world.create_scene(Vec3::new(gravity.0, gravity.1, gravity.2));
        Ok(scene_id_to_u64(id))
    }

    fn destroy_scene(&self, scene_id: u64) -> PyResult<bool> {
        let mut world = lock_world(&self.inner)?;
        Ok(world.destroy_scene(scene_id_from_u64(scene_id)))
    }

    fn contains_scene(&self, scene_id: u64) -> PyResult<bool> {
        let world = lock_world(&self.inner)?;
        Ok(world.contains_scene(scene_id_from_u64(scene_id)))
    }

    fn scene_count(&self) -> PyResult<usize> {
        let world = lock_world(&self.inner)?;
        Ok(world.scene_count())
    }

    fn set_gravity(&self, scene_id: u64, gravity: (f32, f32, f32)) -> PyResult<()> {
        let mut world = lock_world(&self.inner)?;
        let scene = world
            .scene_mut(scene_id_from_u64(scene_id))
            .ok_or_else(|| PyValueError::new_err("scene not found"))?;
        scene.set_gravity(Vec3::new(gravity.0, gravity.1, gravity.2));
        Ok(())
    }

    fn step(&self, scene_id: u64, dt: f32) -> PyResult<()> {
        let mut world = lock_world(&self.inner)?;
        let scene = world
            .scene_mut(scene_id_from_u64(scene_id))
            .ok_or_else(|| PyValueError::new_err("scene not found"))?;
        scene.step(dt);
        Ok(())
    }

    /// 取出累计的 collision events，每条形如
    /// `PyCollisionEvent(a_collider_id, b_collider_id, started, sensor)`。
    fn drain_collision_events(&self, scene_id: u64) -> PyResult<Vec<PyCollisionEvent>> {
        let world = lock_world(&self.inner)?;
        let scene = world
            .scene(scene_id_from_u64(scene_id))
            .ok_or_else(|| PyValueError::new_err("scene not found"))?;
        let events = scene
            .drain_collision_events()
            .into_iter()
            .map(|e| {
                let (a_idx, a_gen) = e.a.raw().into_raw_parts();
                let (b_idx, b_gen) = e.b.raw().into_raw_parts();
                PyCollisionEvent {
                    a: pack_handle(a_idx, a_gen),
                    b: pack_handle(b_idx, b_gen),
                    started: e.started,
                    sensor: e.sensor,
                }
            })
            .collect();
        Ok(events)
    }
}

pub(crate) fn lock_world(world: &SharedWorld) -> PyResult<std::sync::MutexGuard<'_, PhysicsWorld>> {
    world
        .lock()
        .map_err(|e| PyRuntimeError::new_err(format!("physics world lock poisoned: {e}")))
}

#[inline]
pub(crate) fn pack_handle(idx: u32, generation: u32) -> u64 {
    ((generation as u64) << 32) | (idx as u64)
}

#[pyclass(module = "pyhub", name = "PhysicsCollisionEvent")]
#[derive(Clone)]
pub struct PyCollisionEvent {
    #[pyo3(get)]
    pub a: u64,
    #[pyo3(get)]
    pub b: u64,
    #[pyo3(get)]
    pub started: bool,
    #[pyo3(get)]
    pub sensor: bool,
}

#[pymethods]
impl PyCollisionEvent {
    fn __repr__(&self) -> String {
        format!(
            "PhysicsCollisionEvent(a={}, b={}, started={}, sensor={})",
            self.a, self.b, self.started, self.sensor
        )
    }
}
