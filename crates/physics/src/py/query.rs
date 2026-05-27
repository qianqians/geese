//! 射线查询的 pyo3 包装。

use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;

use crate::math::vec3_from_tuple;
use crate::py::world::{PyPhysicsWorld, lock_world, pack_handle, scene_id_from_u64};

#[pyclass(module = "pyhub", name = "PhysicsRayHit")]
#[derive(Clone)]
pub struct PyRayHit {
    /// `pack_handle(idx, gen)`，与 `PyBody.id` 同源。
    #[pyo3(get)]
    pub body: u64,
    /// `pack_handle(idx, gen)` of collider。
    #[pyo3(get)]
    pub collider: u64,
    #[pyo3(get)]
    pub toi: f32,
    #[pyo3(get)]
    pub normal: (f32, f32, f32),
    #[pyo3(get)]
    pub point: (f32, f32, f32),
}

#[pymethods]
impl PyRayHit {
    fn __repr__(&self) -> String {
        format!(
            "PhysicsRayHit(body={}, collider={}, toi={}, normal={:?}, point={:?})",
            self.body, self.collider, self.toi, self.normal, self.point
        )
    }
}

/// 模块级射线查询：`cast_ray(world, scene_id, origin, dir, max_toi, solid=True)`。
#[pyfunction]
#[pyo3(signature = (world, scene_id, origin, dir, max_toi, solid=true))]
pub fn cast_ray(
    world: &PyPhysicsWorld,
    scene_id: u64,
    origin: (f32, f32, f32),
    dir: (f32, f32, f32),
    max_toi: f32,
    solid: bool,
) -> PyResult<Option<PyRayHit>> {
    let shared = world.share();
    let guard = lock_world(&shared)?;
    let scene = guard
        .scene(scene_id_from_u64(scene_id))
        .ok_or_else(|| PyValueError::new_err("scene not found"))?;
    let hit = scene.cast_ray(
        vec3_from_tuple(origin),
        vec3_from_tuple(dir),
        max_toi,
        solid,
    );
    Ok(hit.map(|h| {
        let (b_idx, b_gen) = h.body.raw().into_raw_parts();
        let (c_idx, c_gen) = h.collider.raw().into_raw_parts();
        PyRayHit {
            body: pack_handle(b_idx, b_gen),
            collider: pack_handle(c_idx, c_gen),
            toi: h.toi,
            normal: h.normal,
            point: h.point,
        }
    }))
}
