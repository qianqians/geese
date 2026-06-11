//! 编辑器进程内本地物理世界。
//!
//! [`LocalPhysics`] 直接嵌入 [`PhysicsWorld`]，在 Edit 模式下零网络、
//! 零子进程、零序列化地步进物理模拟。供 PhysicsDebug 渲染碰撞体线框。

use crate::physics_client::BodySnapshot;
use physics::{PhysicsWorld, SceneId};
use std::path::Path;

/// 编辑器本地物理世界薄封装。
pub struct LocalPhysics {
    world: PhysicsWorld,
    scene_id: SceneId,
}

impl LocalPhysics {
    /// 创建本地物理世界并初始化默认场景。
    pub fn new(gravity: [f32; 3]) -> Self {
        let mut world = PhysicsWorld::new();
        let scene_id = world.create_scene(physics::math::Vec3::new(
            gravity[0],
            gravity[1],
            gravity[2],
        ));
        Self { world, scene_id }
    }

    /// 从 `.scene.json` 加载碰撞几何到当前物理场景。
    ///
    /// 解析 manifest 中的 `models`（collision_enabled=true 的 GLTF）
    /// 和 `objects`（cube / plane），创建 Fixed 刚体。
    pub fn load_scene(&mut self, manifest_path: &str) {
        let path = Path::new(manifest_path);
        let Ok(content) = std::fs::read_to_string(path) else {
            eprintln!("[LocalPhysics] scene manifest not found: {manifest_path}");
            return;
        };
        let Ok(manifest): Result<serde_json::Value, _> = serde_json::from_str(&content) else {
            eprintln!("[LocalPhysics] failed to parse scene manifest");
            return;
        };
        let base_dir = path.parent().unwrap_or(Path::new("."));

        // 1. GLTF 模型碰撞体
        if let Some(models) = manifest["models"].as_array() {
            for model in models {
                if !model["collision_enabled"].as_bool().unwrap_or(false) {
                    continue;
                }
                let Some(gltf_rel) = model["path"].as_str() else {
                    continue;
                };
                let gltf_path = base_dir.join(gltf_rel);
                if !gltf_path.exists() {
                    eprintln!("[LocalPhysics] gltf not found: {:?}, skipping", gltf_path);
                    continue;
                }

                let transform = &model["transform"];
                let translation = [
                    transform["translation"][0].as_f64().unwrap_or(0.0) as f32,
                    transform["translation"][1].as_f64().unwrap_or(0.0) as f32,
                    transform["translation"][2].as_f64().unwrap_or(0.0) as f32,
                ];
                let euler_deg = [
                    transform["rotation"][0].as_f64().unwrap_or(0.0) as f32,
                    transform["rotation"][1].as_f64().unwrap_or(0.0) as f32,
                    transform["rotation"][2].as_f64().unwrap_or(0.0) as f32,
                ];
                let rot = euler_to_quat(euler_deg[0], euler_deg[1], euler_deg[2]);
                let iso = physics::math::iso_from_parts(
                    (translation[0], translation[1], translation[2]),
                    rot,
                );

                match physics::scene_builder::extract_gltf_trimeshes(
                    &gltf_path.to_string_lossy(),
                ) {
                    Ok(meshes) => {
                        if let Some(scene) = self.world.scene_mut(self.scene_id) {
                            if let Err(e) = scene.add_static_trimeshes(&meshes, iso, 0.5, 0.0) {
                                eprintln!("[LocalPhysics] add_static_trimeshes failed: {e}");
                            }
                        }
                    }
                    Err(e) => eprintln!("[LocalPhysics] extract_gltf_trimeshes failed: {e}"),
                }
            }
        }

        // 2. 程序化对象（cube / plane）
        if let Some(objects) = manifest["objects"].as_array() {
            for obj in objects {
                let obj_type = obj["object_type"].as_str().unwrap_or("");
                let pos = [
                    obj["position"][0].as_f64().unwrap_or(0.0) as f32,
                    obj["position"][1].as_f64().unwrap_or(0.0) as f32,
                    obj["position"][2].as_f64().unwrap_or(0.0) as f32,
                ];
                let scale = [
                    obj["scale"][0].as_f64().unwrap_or(1.0) as f32,
                    obj["scale"][1].as_f64().unwrap_or(1.0) as f32,
                    obj["scale"][2].as_f64().unwrap_or(1.0) as f32,
                ];
                let euler = obj["rotation_euler"].as_array().map(|a| {
                    [
                        a.first().and_then(|v| v.as_f64()).unwrap_or(0.0) as f32,
                        a.get(1).and_then(|v| v.as_f64()).unwrap_or(0.0) as f32,
                        a.get(2).and_then(|v| v.as_f64()).unwrap_or(0.0) as f32,
                    ]
                }).unwrap_or([0.0, 0.0, 0.0]);
                let rot = euler_to_quat(euler[0], euler[1], euler[2]);

                let shape = match obj_type {
                    "plane" => physics::shapes::ShapeDesc::Cuboid {
                        half_extents: physics::math::Vec3::new(
                            scale[0] * 0.5,
                            0.01,
                            scale[2] * 0.5,
                        ),
                    },
                    "cube" => physics::shapes::ShapeDesc::Cuboid {
                        half_extents: physics::math::Vec3::new(
                            scale[0] * 0.5,
                            scale[1] * 0.5,
                            scale[2] * 0.5,
                        ),
                    },
                    _ => continue,
                };

                let desc = physics::world::BodyDesc {
                    kind: physics::world::BodyKind::Fixed,
                    position: physics::math::iso_from_parts(
                        (pos[0], pos[1], pos[2]),
                        rot,
                    ),
                    ..Default::default()
                };

                if let Some(scene) = self.world.scene_mut(self.scene_id) {
                    if let Err(e) = scene.add_body(desc, shape) {
                        eprintln!("[LocalPhysics] add_body '{obj_type}' failed: {e}");
                    }
                }
            }
        }
    }

    /// 步进物理模拟（委托给 `PhysicsScene::step`）。
    pub fn step(&mut self, dt: f32) {
        if let Some(scene) = self.world.scene_mut(self.scene_id) {
            scene.step(dt);
        }
    }

    /// 获取所有刚体的位置/旋转快照（供 [`PhysicsDebugRenderer`] 渲染线框）。
    pub fn get_body_snapshots(&self) -> Vec<BodySnapshot> {
        let mut snapshots = Vec::new();
        if let Some(scene) = self.world.scene(self.scene_id) {
            for handle in scene.body_handles() {
                if let Some(iso) = scene.body_isometry(handle) {
                    let (idx, _gen) = handle.raw().into_raw_parts();
                    snapshots.push(BodySnapshot {
                        id: format!("local_{}", idx),
                        position: crate::physics_client::Vec3 {
                            x: iso.translation.x as f64,
                            y: iso.translation.y as f64,
                            z: iso.translation.z as f64,
                        },
                        rotation: crate::physics_client::Quat {
                            x: iso.rotation.x as f64,
                            y: iso.rotation.y as f64,
                            z: iso.rotation.z as f64,
                            w: iso.rotation.w as f64,
                        },
                    });
                }
            }
        }
        snapshots
    }
}

/// 欧拉角（度）转四元数 (x, y, z, w)，旋转顺序 Y * X * Z。
fn euler_to_quat(yaw_deg: f32, pitch_deg: f32, roll_deg: f32) -> (f32, f32, f32, f32) {
    let yaw = yaw_deg.to_radians() * 0.5;
    let pitch = pitch_deg.to_radians() * 0.5;
    let roll = roll_deg.to_radians() * 0.5;

    let cy = yaw.cos();
    let sy = yaw.sin();
    let cp = pitch.cos();
    let sp = pitch.sin();
    let cr = roll.cos();
    let sr = roll.sin();

    let x = sr * cp * cy - cr * sp * sy;
    let y = cr * sp * cy + sr * cp * sy;
    let z = cr * cp * sy - sr * sp * cy;
    let w = cr * cp * cy + sr * sp * sy;
    (x, y, z, w)
}
