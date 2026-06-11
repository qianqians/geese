//! `PyBody`：刚体 pyo3 包装。持有共享 world 引用 + BodyHandle。
//!
//! 工厂方法挂在 `PyPhysicsWorld` 上更直观，但为兼容 Python 端 entity component
//! 风格，这里同时暴露 `PyBody.add_dynamic(world, scene_id, ...)` 类方法。

use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;

use crate::handles::{BodyHandle, ColliderHandle};
use crate::math::{Iso3, Vec3, iso_from_parts, quat_to_tuple, vec3_from_tuple, vec3_to_tuple};
use crate::py::shape::PyShape;
use crate::py::world::{PyPhysicsWorld, SharedWorld, lock_world, pack_handle, scene_id_from_u64};
use crate::world::{BodyDesc, BodyKind};

#[pyclass(module = "pyhub", name = "PhysicsBody")]
pub struct PyBody {
    world: SharedWorld,
    handle: BodyHandle,
    collider: ColliderHandle,
}

impl PyBody {
    fn new(world: SharedWorld, handle: BodyHandle, collider: ColliderHandle) -> Self {
        Self { world, handle, collider }
    }
}

fn build_desc(
    kind: BodyKind,
    position: (f32, f32, f32),
    rotation: (f32, f32, f32, f32),
    linvel: (f32, f32, f32),
    angvel: (f32, f32, f32),
    density: f32,
    friction: f32,
    restitution: f32,
    gravity_scale: f32,
    can_sleep: bool,
    ccd_enabled: bool,
    sensor: bool,
    events: bool,
) -> BodyDesc {
    BodyDesc::new(kind)
        .position(iso_from_parts(position, rotation))
        .linvel(vec3_from_tuple(linvel))
        .angvel(vec3_from_tuple(angvel))
        .density(density)
        .friction(friction)
        .restitution(restitution)
        .gravity_scale(gravity_scale)
        .can_sleep(can_sleep)
        .ccd(ccd_enabled)
        .sensor(sensor)
        .events(events)
}

fn add_body_internal(
    world_py: &PyPhysicsWorld,
    scene_id: u64,
    desc: BodyDesc,
    shape: &PyShape,
) -> PyResult<PyBody> {
    let shared = world_py.share();
    let (handle, collider) = {
        let mut guard = lock_world(&shared)?;
        let scene = guard
            .scene_mut(scene_id_from_u64(scene_id))
            .ok_or_else(|| PyValueError::new_err("scene not found"))?;
        scene
            .add_body(desc, shape.desc())
            .map_err(PyValueError::new_err)?
    };
    Ok(PyBody::new(shared, handle, collider))
}

#[pymethods]
impl PyBody {
    /// 通过 `(world, scene_id, ...)` 创建 dynamic body。
    #[staticmethod]
    #[pyo3(signature = (
        world, scene_id, shape,
        position=(0.0, 0.0, 0.0),
        rotation=(0.0, 0.0, 0.0, 1.0),
        linvel=(0.0, 0.0, 0.0),
        angvel=(0.0, 0.0, 0.0),
        density=1.0,
        friction=0.5,
        restitution=0.0,
        gravity_scale=1.0,
        can_sleep=true,
        ccd_enabled=false,
        sensor=false,
        events=false,
    ))]
    #[allow(clippy::too_many_arguments)]
    fn add_dynamic(
        world: &PyPhysicsWorld,
        scene_id: u64,
        shape: &PyShape,
        position: (f32, f32, f32),
        rotation: (f32, f32, f32, f32),
        linvel: (f32, f32, f32),
        angvel: (f32, f32, f32),
        density: f32,
        friction: f32,
        restitution: f32,
        gravity_scale: f32,
        can_sleep: bool,
        ccd_enabled: bool,
        sensor: bool,
        events: bool,
    ) -> PyResult<Self> {
        let desc = build_desc(
            BodyKind::Dynamic,
            position,
            rotation,
            linvel,
            angvel,
            density,
            friction,
            restitution,
            gravity_scale,
            can_sleep,
            ccd_enabled,
            sensor,
            events,
        );
        add_body_internal(world, scene_id, desc, shape)
    }

    #[staticmethod]
    #[pyo3(signature = (
        world, scene_id, shape,
        position=(0.0, 0.0, 0.0),
        rotation=(0.0, 0.0, 0.0, 1.0),
        friction=0.5,
        restitution=0.0,
        sensor=false,
        events=false,
    ))]
    #[allow(clippy::too_many_arguments)]
    fn add_fixed(
        world: &PyPhysicsWorld,
        scene_id: u64,
        shape: &PyShape,
        position: (f32, f32, f32),
        rotation: (f32, f32, f32, f32),
        friction: f32,
        restitution: f32,
        sensor: bool,
        events: bool,
    ) -> PyResult<Self> {
        let desc = build_desc(
            BodyKind::Fixed,
            position,
            rotation,
            (0.0, 0.0, 0.0),
            (0.0, 0.0, 0.0),
            1.0,
            friction,
            restitution,
            0.0,
            true,
            false,
            sensor,
            events,
        );
        add_body_internal(world, scene_id, desc, shape)
    }

    #[staticmethod]
    #[pyo3(signature = (
        world, scene_id, shape,
        position=(0.0, 0.0, 0.0),
        rotation=(0.0, 0.0, 0.0, 1.0),
        velocity_based=false,
        events=false,
    ))]
    fn add_kinematic(
        world: &PyPhysicsWorld,
        scene_id: u64,
        shape: &PyShape,
        position: (f32, f32, f32),
        rotation: (f32, f32, f32, f32),
        velocity_based: bool,
        events: bool,
    ) -> PyResult<Self> {
        let kind = if velocity_based {
            BodyKind::KinematicVelocity
        } else {
            BodyKind::KinematicPosition
        };
        let desc = build_desc(
            kind,
            position,
            rotation,
            (0.0, 0.0, 0.0),
            (0.0, 0.0, 0.0),
            1.0,
            0.5,
            0.0,
            0.0,
            true,
            false,
            false,
            events,
        );
        add_body_internal(world, scene_id, desc, shape)
    }

    /// 不透明 u64 句柄（含 scene + body idx + body gen），用于事件比对。
    #[getter]
    fn id(&self) -> u64 {
        let (idx, generation) = self.handle.raw().into_raw_parts();
        pack_handle(idx, generation)
    }

    /// 不透明 u64 collider 句柄，用于碰撞事件分发。
    #[getter]
    fn collider_handle(&self) -> u64 {
        let (idx, generation) = self.collider.raw().into_raw_parts();
        pack_handle(idx, generation)
    }

    #[getter]
    fn scene_id(&self) -> u64 {
        crate::py::world::scene_id_to_u64(self.handle.scene())
    }

    fn position(&self) -> PyResult<(f32, f32, f32)> {
        let world = lock_world(&self.world)?;
        let scene = world
            .scene(self.handle.scene())
            .ok_or_else(|| PyValueError::new_err("scene gone"))?;
        let iso: Iso3 = scene
            .body_isometry(self.handle)
            .ok_or_else(|| PyValueError::new_err("body removed"))?;
        Ok(vec3_to_tuple(iso.translation))
    }

    fn rotation(&self) -> PyResult<(f32, f32, f32, f32)> {
        let world = lock_world(&self.world)?;
        let scene = world
            .scene(self.handle.scene())
            .ok_or_else(|| PyValueError::new_err("scene gone"))?;
        let iso: Iso3 = scene
            .body_isometry(self.handle)
            .ok_or_else(|| PyValueError::new_err("body removed"))?;
        Ok(quat_to_tuple(iso.rotation))
    }

    fn linvel(&self) -> PyResult<(f32, f32, f32)> {
        let world = lock_world(&self.world)?;
        let scene = world
            .scene(self.handle.scene())
            .ok_or_else(|| PyValueError::new_err("scene gone"))?;
        let v = scene
            .body_linvel(self.handle)
            .ok_or_else(|| PyValueError::new_err("body removed"))?;
        Ok(vec3_to_tuple(v))
    }

    fn angvel(&self) -> PyResult<(f32, f32, f32)> {
        let world = lock_world(&self.world)?;
        let scene = world
            .scene(self.handle.scene())
            .ok_or_else(|| PyValueError::new_err("scene gone"))?;
        let v = scene
            .body_angvel(self.handle)
            .ok_or_else(|| PyValueError::new_err("body removed"))?;
        Ok(vec3_to_tuple(v))
    }

    #[pyo3(signature = (translation, wake_up=true))]
    fn set_translation(&self, translation: (f32, f32, f32), wake_up: bool) -> PyResult<bool> {
        let mut world = lock_world(&self.world)?;
        let scene = world
            .scene_mut(self.handle.scene())
            .ok_or_else(|| PyValueError::new_err("scene gone"))?;
        Ok(scene.set_translation(self.handle, vec3_from_tuple(translation), wake_up))
    }

    #[pyo3(signature = (rotation, wake_up=true))]
    fn set_rotation(&self, rotation: (f32, f32, f32, f32), wake_up: bool) -> PyResult<bool> {
        use crate::math::quat_from_tuple;
        let mut world = lock_world(&self.world)?;
        let scene = world
            .scene_mut(self.handle.scene())
            .ok_or_else(|| PyValueError::new_err("scene gone"))?;
        Ok(scene.set_rotation(self.handle, quat_from_tuple(rotation), wake_up))
    }

    #[pyo3(signature = (velocity, wake_up=true))]
    fn set_linvel(&self, velocity: (f32, f32, f32), wake_up: bool) -> PyResult<bool> {
        let mut world = lock_world(&self.world)?;
        let scene = world
            .scene_mut(self.handle.scene())
            .ok_or_else(|| PyValueError::new_err("scene gone"))?;
        Ok(scene.set_linvel(self.handle, vec3_from_tuple(velocity), wake_up))
    }

    #[pyo3(signature = (velocity, wake_up=true))]
    fn set_angvel(&self, velocity: (f32, f32, f32), wake_up: bool) -> PyResult<bool> {
        let mut world = lock_world(&self.world)?;
        let scene = world
            .scene_mut(self.handle.scene())
            .ok_or_else(|| PyValueError::new_err("scene gone"))?;
        Ok(scene.set_angvel(self.handle, vec3_from_tuple(velocity), wake_up))
    }

    #[pyo3(signature = (impulse, wake_up=true))]
    fn apply_impulse(&self, impulse: (f32, f32, f32), wake_up: bool) -> PyResult<bool> {
        let mut world = lock_world(&self.world)?;
        let scene = world
            .scene_mut(self.handle.scene())
            .ok_or_else(|| PyValueError::new_err("scene gone"))?;
        Ok(scene.apply_impulse(self.handle, vec3_from_tuple(impulse), wake_up))
    }

    #[pyo3(signature = (torque, wake_up=true))]
    fn apply_torque_impulse(
        &self,
        torque: (f32, f32, f32),
        wake_up: bool,
    ) -> PyResult<bool> {
        let mut world = lock_world(&self.world)?;
        let scene = world
            .scene_mut(self.handle.scene())
            .ok_or_else(|| PyValueError::new_err("scene gone"))?;
        Ok(scene.apply_torque_impulse(self.handle, vec3_from_tuple(torque), wake_up))
    }

    fn remove(&self) -> PyResult<bool> {
        let mut world = lock_world(&self.world)?;
        let scene = match world.scene_mut(self.handle.scene()) {
            Some(s) => s,
            None => return Ok(false),
        };
        Ok(scene.remove_body(self.handle))
    }

    fn is_alive(&self) -> PyResult<bool> {
        let world = lock_world(&self.world)?;
        Ok(world
            .scene(self.handle.scene())
            .map(|s| s.contains_body(self.handle))
            .unwrap_or(false))
    }
}

// 显式让 Vec3 在 set_* 上可读。
#[allow(dead_code)]
fn _unused(_v: Vec3) {}
