//! 通用引擎 PyO3 绑定 —— 将 geese 引擎核心子系统暴露给 Python。
//!
//! 本 crate 只提供**机制**（Scene / Physics / Input / Camera 桥接），
//! 不包含任何游戏特定逻辑。具体游戏通过 Python 脚本调用这些 API 实现。

use std::sync::Mutex;

use cgmath::{Point3, Quaternion, Vector3};
use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;
use pyo3::types::PyDict;
use slotmap::Key;

use avatar::{SceneNode, Transform};
use camera::Camera;
use input::{InputState, KeyCode};
use physics::handles::{BodyHandle, SceneId};
use physics::math::{Iso3, Quat, Vec3};
use physics::scene::PhysicsScene;
use physics::shapes::ShapeDesc;
use physics::world::{BodyDesc, BodyKind};
use render::{MeshFlags, ModelMesh, Vertex};
use scene::Scene;

// ═══════════════════════════════════════════════════════════════════════════
// 全局网格注册表（cdylib 内部单例，Python 侧独占访问）
// ═══════════════════════════════════════════════════════════════════════════

static MESH_REGISTRY: Mutex<Vec<ModelMesh>> = Mutex::new(Vec::new());

fn register_mesh(mesh: ModelMesh) -> usize {
    let mut reg = MESH_REGISTRY.lock().unwrap();
    let idx = reg.len();
    reg.push(mesh);
    idx
}

fn get_mesh(idx: usize) -> Result<ModelMesh, String> {
    let reg = MESH_REGISTRY.lock().unwrap();
    reg.get(idx)
        .cloned()
        .ok_or_else(|| format!("Mesh index {idx} out of range"))
}

// ═══════════════════════════════════════════════════════════════════════════
// BodyHandle <-> u64 编解码
// ═══════════════════════════════════════════════════════════════════════════

fn encode_body_handle(h: BodyHandle) -> u64 {
    let scene_ffi = h.scene().data().as_ffi();
    let (idx, generation) = h.raw().into_raw_parts();
    ((scene_ffi as u64) << 48) | ((idx as u64) << 16) | (generation as u64)
}

fn decode_body_handle(v: u64) -> BodyHandle {
    let scene_ffi = (v >> 48) as u32;
    let idx = ((v >> 16) & 0xFFFF_FFFF) as u32;
    let generation = (v & 0xFFFF) as u32;
    let scene_id = SceneId::from(slotmap::KeyData::from_ffi(scene_ffi as u64));
    let raw = rapier3d::prelude::RigidBodyHandle::from_raw_parts(idx, generation);
    BodyHandle::new(scene_id, raw)
}

// ═══════════════════════════════════════════════════════════════════════════
// EngineBridge — 帧局部桥接对象
// ═══════════════════════════════════════════════════════════════════════════

/// 每帧由 Rust 主循环创建，持有引擎子系统的裸指针。
/// 仅在帧回调期间有效，帧结束后指针失效。
#[pyclass]
pub struct EngineBridge {
    scene: *mut Scene,
    physics: *mut PhysicsScene,
    input: *const InputState,
    camera: *mut Camera,
}

// 帧局部单线程使用，允许跨 Python/Rust 边界传递。
unsafe impl Send for EngineBridge {}
unsafe impl Sync for EngineBridge {}

impl EngineBridge {
    /// 创建帧局部桥接对象。指针仅在帧回调期间有效。
    pub fn new(
        scene: *mut Scene,
        physics: *mut PhysicsScene,
        input: *const InputState,
        camera: *mut Camera,
    ) -> Self {
        Self { scene, physics, input, camera }
    }

    unsafe fn scene(&self) -> &mut Scene {
        unsafe { &mut *self.scene }
    }
    unsafe fn physics(&self) -> &mut PhysicsScene {
        unsafe { &mut *self.physics }
    }
    unsafe fn input(&self) -> &InputState {
        unsafe { &*self.input }
    }
    unsafe fn camera(&self) -> &mut Camera {
        unsafe { &mut *self.camera }
    }
}

// ─── 辅助：KeyCode 从字符串解析 ───

fn parse_key_code(name: &str) -> PyResult<KeyCode> {
    match name {
        "Space" => Ok(KeyCode::Space),
        "R" => Ok(KeyCode::R),
        "W" => Ok(KeyCode::W),
        "A" => Ok(KeyCode::A),
        "S" => Ok(KeyCode::S),
        "D" => Ok(KeyCode::D),
        "Q" => Ok(KeyCode::Q),
        "E" => Ok(KeyCode::E),
        "Escape" => Ok(KeyCode::Escape),
        "Enter" => Ok(KeyCode::Enter),
        "Tab" => Ok(KeyCode::Tab),
        "Left" => Ok(KeyCode::Left),
        "Right" => Ok(KeyCode::Right),
        "Up" => Ok(KeyCode::Up),
        "Down" => Ok(KeyCode::Down),
        "LeftShift" => Ok(KeyCode::LeftShift),
        "LeftCtrl" => Ok(KeyCode::LeftCtrl),
        "LeftAlt" => Ok(KeyCode::LeftAlt),
        _ => Err(PyRuntimeError::new_err(format!("Unknown key: {name}"))),
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// PyO3 方法实现
// ═══════════════════════════════════════════════════════════════════════════

#[pymethods]
impl EngineBridge {
    // ── 输入 ──────────────────────────────────────────────

    fn input_key_pressed(&self, key: &str) -> bool {
        let k = parse_key_code(key).unwrap_or(KeyCode::Space);
        unsafe { self.input().key_pressed(k) }
    }

    fn input_key_released(&self, key: &str) -> bool {
        let k = parse_key_code(key).unwrap_or(KeyCode::Space);
        unsafe { self.input().key_released(k) }
    }

    // ── 摄像机 ────────────────────────────────────────────

    fn camera_smooth_follow(&self, x: f32, y: f32, z: f32, speed: f32, dt: f32) {
        unsafe {
            self.camera()
                .smooth_follow_target(Point3::new(x, y, z), speed, dt);
        }
    }

    // ── 场景 ──────────────────────────────────────────────

    /// 添加静态对象（mesh 来自全局注册表）。
    fn scene_add_static(
        &self,
        mesh_idx: usize,
        tx: f32, ty: f32, tz: f32,
        sx: f32, sy: f32, sz: f32,
    ) -> PyResult<String> {
        let mesh = get_mesh(mesh_idx).map_err(|e| PyRuntimeError::new_err(e))?;
        let eid = unsafe {
            self.scene().add_static_object(
                mesh,
                Vector3::new(tx, ty, tz),
                Quaternion::new(1.0, 0.0, 0.0, 0.0),
                Vector3::new(sx, sy, sz),
            )
        };
        Ok(eid)
    }

    /// 添加静态对象（带材质索引）。
    fn scene_add_static_with_material(
        &self,
        mesh_idx: usize,
        material_idx: usize,
        tx: f32, ty: f32, tz: f32,
        sx: f32, sy: f32, sz: f32,
    ) -> PyResult<String> {
        let mut mesh = get_mesh(mesh_idx).map_err(|e| PyRuntimeError::new_err(e))?;
        mesh.material = Some(render::MaterialHandle(material_idx));
        let eid = unsafe {
            self.scene().add_static_object(
                mesh,
                Vector3::new(tx, ty, tz),
                Quaternion::new(1.0, 0.0, 0.0, 0.0),
                Vector3::new(sx, sy, sz),
            )
        };
        Ok(eid)
    }

    /// 添加动态对象（带材质索引），返回 (entity_id, node_index)。
    fn scene_add_dynamic_with_material(
        &self,
        mesh_idx: usize,
        material_idx: usize,
        tx: f32, ty: f32, tz: f32,
        sx: f32, sy: f32, sz: f32,
    ) -> PyResult<(String, usize)> {
        let mut mesh = get_mesh(mesh_idx).map_err(|e| PyRuntimeError::new_err(e))?;
        mesh.material = Some(render::MaterialHandle(material_idx));
        let node_idx = unsafe { self.scene().nodes.len() };
        let eid = unsafe {
            self.scene().add_dynamic_object(
                mesh,
                Vector3::new(tx, ty, tz),
                Quaternion::new(1.0, 0.0, 0.0, 0.0),
                Vector3::new(sx, sy, sz),
            )
        };
        Ok((eid, node_idx))
    }

    fn scene_remove_object(&self, entity_id: &str) -> PyResult<()> {
        unsafe {
            self.scene()
                .remove_object(entity_id)
                .map_err(|e| PyRuntimeError::new_err(e))?;
        }
        Ok(())
    }

    fn scene_update_transforms(&self) {
        unsafe { self.scene().update_world_transforms() };
    }

    // ── 节点变换 ──────────────────────────────────────────

    fn node_set_translation(&self, idx: usize, x: f32, y: f32, z: f32) {
        unsafe {
            let nodes = &mut self.scene().nodes;
            if let Some(n) = nodes.get_mut(idx) {
                n.local_transform.translation = Vector3::new(x, y, z);
            }
        }
    }

    fn node_set_scale(&self, idx: usize, x: f32, y: f32, z: f32) {
        unsafe {
            let nodes = &mut self.scene().nodes;
            if let Some(n) = nodes.get_mut(idx) {
                n.local_transform.scale = Vector3::new(x, y, z);
            }
        }
    }

    fn node_get_scale_y(&self, idx: usize) -> f32 {
        unsafe {
            self.scene()
                .nodes
                .get(idx)
                .map(|n| n.local_transform.scale.y)
                .unwrap_or(1.0)
        }
    }

    fn node_set_scale_y(&self, idx: usize, y: f32) {
        unsafe {
            if let Some(n) = self.scene().nodes.get_mut(idx) {
                n.local_transform.scale.y = y;
            }
        }
    }

    fn node_count(&self) -> usize {
        unsafe { self.scene().nodes.len() }
    }

    // ── 材质 ──────────────────────────────────────────────

    fn material_set_emissive(&self, idx: usize, r: f32, g: f32, b: f32) {
        unsafe {
            if let Some(m) = self.scene().materials.materials.get_mut(idx) {
                m.emissive_factor = [r, g, b];
            }
        }
    }

    // ── 物理 ──────────────────────────────────────────────

    /// 创建刚体，返回 (body_handle_u64, collider_handle_u64)。
    fn physics_add_body(
        &self,
        kind: &str,
        px: f32, py: f32, pz: f32,
        friction: f32,
        shape_type: &str,
        shape_args: &Bound<'_, PyDict>,
    ) -> PyResult<(u64, u64)> {
        let body_kind = match kind {
            "dynamic" => BodyKind::Dynamic,
            "fixed" => BodyKind::Fixed,
            "kinematic_position" => BodyKind::KinematicPosition,
            "kinematic_velocity" => BodyKind::KinematicVelocity,
            _ => return Err(PyRuntimeError::new_err(format!("Unknown body kind: {kind}"))),
        };

        let mut desc = BodyDesc::new(body_kind).friction(friction);
        desc.position = Iso3::from_parts(Vec3::new(px, py, pz), Quat::IDENTITY);

        let shape = match shape_type {
            "cuboid" => {
                let hx = shape_args.get_item("hx")?.unwrap().extract::<f32>()?;
                let hy = shape_args.get_item("hy")?.unwrap().extract::<f32>()?;
                let hz = shape_args.get_item("hz")?.unwrap().extract::<f32>()?;
                ShapeDesc::cuboid(hx, hy, hz)
            }
            "capsule" => {
                let hh = shape_args.get_item("half_height")?.unwrap().extract::<f32>()?;
                let r = shape_args.get_item("radius")?.unwrap().extract::<f32>()?;
                ShapeDesc::Capsule { half_height: hh, radius: r }
            }
            "ball" => {
                let r = shape_args.get_item("radius")?.unwrap().extract::<f32>()?;
                ShapeDesc::Ball { radius: r }
            }
            _ => return Err(PyRuntimeError::new_err(format!("Unknown shape: {shape_type}"))),
        };

        let (bh, ch) = unsafe {
            self.physics()
                .add_body(desc, shape)
                .map_err(|e| PyRuntimeError::new_err(e))?
        };
        Ok((encode_body_handle(bh), 0u64))
    }

    fn physics_remove_body(&self, handle: u64) {
        let bh = decode_body_handle(handle);
        unsafe { self.physics().remove_body(bh); };
    }

    fn physics_apply_impulse(&self, handle: u64, ix: f32, iy: f32, iz: f32) {
        let bh = decode_body_handle(handle);
        unsafe {
            self.physics()
                .apply_impulse(bh, Vec3::new(ix, iy, iz), true);
        };
    }

    fn physics_get_position(&self, handle: u64) -> (f32, f32, f32) {
        let bh = decode_body_handle(handle);
        unsafe {
            self.physics()
                .body_isometry(bh)
                .map(|iso| {
                    (iso.translation.x as f32, iso.translation.y as f32, iso.translation.z as f32)
                })
                .unwrap_or((0.0, 0.0, 0.0))
        }
    }

    fn physics_set_translation(&self, handle: u64, x: f32, y: f32, z: f32) {
        let bh = decode_body_handle(handle);
        unsafe {
            self.physics()
                .set_translation(bh, Vec3::new(x, y, z), true);
        };
    }

    fn physics_set_linvel(&self, handle: u64, vx: f32, vy: f32, vz: f32) {
        let bh = decode_body_handle(handle);
        unsafe {
            self.physics()
                .set_linvel(bh, Vec3::new(vx, vy, vz), true);
        };
    }

    fn physics_get_linvel(&self, handle: u64) -> (f32, f32, f32) {
        let bh = decode_body_handle(handle);
        unsafe {
            self.physics()
                .body_linvel(bh)
                .map(|v| (v.x as f32, v.y as f32, v.z as f32))
                .unwrap_or((0.0, 0.0, 0.0))
        }
    }

    fn physics_step(&self, dt: f32) {
        unsafe { self.physics().step(dt) };
    }

    /// 从胶囊体底部向下射线检测地面。
    fn physics_check_ground(&self, body_handle: u64, half_height: f32, radius: f32) -> bool {
        let bh = decode_body_handle(body_handle);
        unsafe {
            let ps = self.physics();
            let Some(iso) = ps.body_isometry(bh) else {
                return false;
            };
            let origin = Vec3::new(
                iso.translation.x,
                iso.translation.y - half_height - radius - 0.02,
                iso.translation.z,
            );
            let dir = Vec3::new(0.0, -1.0, 0.0);
            match ps.cast_ray_excluding(origin, dir, 0.3, true, bh) {
                Some(hit) => hit.normal.1 > 0.5,
                None => false,
            }
        }
    }

    // ── 玩家创建辅助 ──────────────────────────────────────

    /// 创建胶囊体控制器刚体 + 场景节点。
    /// 返回 (body_handle_u64, node_index)。
    fn create_player(
        &self,
        px: f32, py: f32, pz: f32,
        half_height: f32, radius: f32,
    ) -> PyResult<(u64, usize)> {
        let pos = Vec3::new(px, py, pz);
        let desc = BodyDesc {
            kind: BodyKind::Dynamic,
            position: Iso3::from_parts(pos, Quat::IDENTITY),
            can_sleep: false,
            ccd_enabled: true,
            ..Default::default()
        };
        let shape = ShapeDesc::Capsule { half_height, radius };

        let (bh, _ch) = unsafe {
            self.physics()
                .add_body(desc, shape)
                .map_err(|e| PyRuntimeError::new_err(e))?
        };

        let node_idx = unsafe { self.scene().nodes.len() };
        let transform = Transform {
            translation: Vector3::new(px, py, pz),
            rotation: Quaternion::new(1.0, 0.0, 0.0, 0.0),
            scale: Vector3::new(1.0, 1.0, 1.0),
        };
        let mut node = SceneNode::new(node_idx, None, transform);
        node.id = node_idx;
        unsafe { self.scene().nodes.push(node) };

        Ok((encode_body_handle(bh), node_idx))
    }

    // ── 网格构建 ──────────────────────────────────────────

    /// 构建单位立方体网格（24 顶点，6 面），注册到全局网格表。
    /// 返回网格索引。
    #[staticmethod]
    fn build_cube(sx: f32, sy: f32, sz: f32, material_index: usize) -> usize {
        let mesh = build_cube_mesh(sx, sy, sz, material_index);
        register_mesh(mesh)
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// 通用立方体网格构建（与 scene_builder 逻辑一致）
// ═══════════════════════════════════════════════════════════════════════════

fn build_cube_mesh(sx: f32, sy: f32, sz: f32, material_index: usize) -> ModelMesh {
    let hx = sx * 0.5;
    let hy = sy * 0.5;
    let hz = sz * 0.5;

    #[rustfmt::skip]
    let positions = [
        [-hx,-hy, hz], [ hx,-hy, hz], [ hx, hy, hz], [-hx, hy, hz],
        [ hx,-hy,-hz], [-hx,-hy,-hz], [-hx, hy,-hz], [ hx, hy,-hz],
        [ hx,-hy, hz], [ hx,-hy,-hz], [ hx, hy,-hz], [ hx, hy, hz],
        [-hx,-hy,-hz], [-hx,-hy, hz], [-hx, hy, hz], [-hx, hy,-hz],
        [-hx, hy, hz], [ hx, hy, hz], [ hx, hy,-hz], [-hx, hy,-hz],
        [-hx,-hy,-hz], [ hx,-hy,-hz], [ hx,-hy, hz], [-hx,-hy, hz],
    ];
    #[rustfmt::skip]
    let normals = [
        [0.,0.,1.],[0.,0.,1.],[0.,0.,1.],[0.,0.,1.],
        [0.,0.,-1.],[0.,0.,-1.],[0.,0.,-1.],[0.,0.,-1.],
        [1.,0.,0.],[1.,0.,0.],[1.,0.,0.],[1.,0.,0.],
        [-1.,0.,0.],[-1.,0.,0.],[-1.,0.,0.],[-1.,0.,0.],
        [0.,1.,0.],[0.,1.,0.],[0.,1.,0.],[0.,1.,0.],
        [0.,-1.,0.],[0.,-1.,0.],[0.,-1.,0.],[0.,-1.,0.],
    ];
    #[rustfmt::skip]
    let uvs = [
        [0.,0.],[1.,0.],[1.,1.],[0.,1.],
        [0.,0.],[1.,0.],[1.,1.],[0.,1.],
        [0.,0.],[1.,0.],[1.,1.],[0.,1.],
        [0.,0.],[1.,0.],[1.,1.],[0.,1.],
        [0.,0.],[1.,0.],[1.,1.],[0.,1.],
        [0.,0.],[1.,0.],[1.,1.],[0.,1.],
    ];
    #[rustfmt::skip]
    let tangents: [[f32;4];24] = [
        [1.,0.,0.,1.],[1.,0.,0.,1.],[1.,0.,0.,1.],[1.,0.,0.,1.],
        [-1.,0.,0.,1.],[-1.,0.,0.,1.],[-1.,0.,0.,1.],[-1.,0.,0.,1.],
        [0.,0.,-1.,1.],[0.,0.,-1.,1.],[0.,0.,-1.,1.],[0.,0.,-1.,1.],
        [0.,0.,1.,1.],[0.,0.,1.,1.],[0.,0.,1.,1.],[0.,0.,1.,1.],
        [1.,0.,0.,1.],[1.,0.,0.,1.],[1.,0.,0.,1.],[1.,0.,0.,1.],
        [1.,0.,0.,1.],[1.,0.,0.,1.],[1.,0.,0.,1.],[1.,0.,0.,1.],
    ];

    let vertices: Vec<Vertex> = (0..24)
        .map(|i| Vertex {
            position: Point3::new(positions[i][0], positions[i][1], positions[i][2]),
            normal: Vector3::new(normals[i][0], normals[i][1], normals[i][2]),
            uv: cgmath::Vector2::new(uvs[i][0], uvs[i][1]),
            tangent: tangents[i],
            joints: [0; 4],
            weights: [1.0, 0.0, 0.0, 0.0],
        })
        .collect();

    #[rustfmt::skip]
    let indices = vec![
        0,1,2, 0,2,3,  4,5,6, 4,6,7,  8,9,10, 8,10,11,
        12,13,14, 12,14,15,  16,17,18, 16,18,19,  20,21,22, 20,22,23,
    ];

    let mut mesh = ModelMesh::new();
    mesh.vertices = vertices;
    mesh.indices = indices;
    mesh.material = Some(render::MaterialHandle(material_index));
    mesh.flags = MeshFlags {
        has_normals: true,
        has_uv0: true,
        has_tangents: true,
        has_skin: false,
    };
    mesh
}

// ═══════════════════════════════════════════════════════════════════════════
// 材质构建辅助
// ═══════════════════════════════════════════════════════════════════════════

/// 创建单个 PBR 材质（供 Python 侧调用构建材质列表）。
#[pyfunction]
fn make_material(name: &str, r: f32, g: f32, b: f32) -> Py<PyAny> {
    Python::attach(|py| {
        let dict = PyDict::new(py);
        dict.set_item("name", name).unwrap();
        dict.set_item("r", r).unwrap();
        dict.set_item("g", g).unwrap();
        dict.set_item("b", b).unwrap();
        dict.into_any().unbind()
    })
}

// ═══════════════════════════════════════════════════════════════════════════
// 模块定义
// ═══════════════════════════════════════════════════════════════════════════

#[pymodule]
fn py_engine(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<EngineBridge>()?;
    m.add_function(wrap_pyfunction!(make_material, m)?)?;
    Ok(())
}
