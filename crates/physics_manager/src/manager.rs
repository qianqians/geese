//! [`PhysicsManager`] — local physics manager.

use std::path::Path;

use physics::{PhysicsWorld, SceneId};
use physics::handles::BodyHandle;
use physics::math::{Vec3, Quat};
use physics::world::{BodyDesc, BodyKind};
use physics::shapes::ShapeDesc;

/// Collider transform snapshot for debug rendering.
#[derive(Debug, Clone)]
pub struct BodySnapshot {
    pub id: String,
    pub position: Position3,
    pub rotation: Quat4,
}

/// Full rigid-body state snapshot (position + rotation + velocities).
#[derive(Debug, Clone)]
pub struct StateSnapshot {
    pub position: cgmath::Vector3<f32>,
    pub rotation: cgmath::Quaternion<f32>,
    pub linear_velocity: cgmath::Vector3<f32>,
    pub angular_velocity: cgmath::Vector3<f32>,
}

/// 3D position (f64 for compatibility).
#[derive(Debug, Clone, Copy)]
pub struct Position3 {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

/// Quaternion rotation (f64 for compatibility).
#[derive(Debug, Clone, Copy)]
pub struct Quat4 {
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub w: f64,
}

// ---------------------------------------------------------------------------
// PhysicsManager
// ---------------------------------------------------------------------------

/// Manages local physics simulation using a [`PhysicsWorld`] with a single scene.
pub struct PhysicsManager {
    world: PhysicsWorld,
    scene_id: SceneId,
}

impl PhysicsManager {
    /// Create a new manager with the given gravity.
    ///
    /// `gravity`: `[gx, gy, gz]` in m/s^2.
    pub fn new(gravity: [f32; 3]) -> Self {
        let mut world = PhysicsWorld::new();
        let scene_id = world.create_scene(Vec3::new(gravity[0], gravity[1], gravity[2]));
        Self {
            world,
            scene_id,
        }
    }

    // -------------------------------------------------------------------
    // Scene loading
    // -------------------------------------------------------------------

    /// Load collision geometry from a `.scene.json` manifest into local physics.
    pub fn load_scene(&mut self, manifest_path: &str) {
        let path = Path::new(manifest_path);
        let Ok(content) = std::fs::read_to_string(path) else {
            eprintln!("[PhysicsManager] scene manifest not found: {manifest_path}");
            return;
        };
        let Ok(manifest): Result<serde_json::Value, _> = serde_json::from_str(&content) else {
            eprintln!("[PhysicsManager] failed to parse scene manifest");
            return;
        };
        let base_dir = path.parent().unwrap_or(Path::new("."));

        // 1. GLTF model collision
        if let Some(models) = manifest["models"].as_array() {
            for model in models {
                // 读取物理组件定义（优先新格式 physics 字段，回退旧格式）
                let physics = model.get("physics");
                let collision_enabled = physics
                    .and_then(|p| p.get("collision_enabled"))
                    .and_then(|v| v.as_bool())
                    .unwrap_or_else(|| model["collision_enabled"].as_bool().unwrap_or(false));
                if !collision_enabled {
                    continue;
                }
                let Some(gltf_rel) = model["path"].as_str() else { continue };
                let gltf_path = base_dir.join(gltf_rel);
                if !gltf_path.exists() {
                    eprintln!("[PhysicsManager] gltf not found, skipping: {:?}", gltf_path);
                    continue;
                }

                let t = &model["transform"];
                let translation = [
                    t["translation"][0].as_f64().unwrap_or(0.0) as f32,
                    t["translation"][1].as_f64().unwrap_or(0.0) as f32,
                    t["translation"][2].as_f64().unwrap_or(0.0) as f32,
                ];
                let euler = [
                    t["rotation"][0].as_f64().unwrap_or(0.0) as f32,
                    t["rotation"][1].as_f64().unwrap_or(0.0) as f32,
                    t["rotation"][2].as_f64().unwrap_or(0.0) as f32,
                ];
                let rot = euler_to_quat(euler[0], euler[1], euler[2]);
                let iso = iso_from_parts(translation, rot);

                match physics::scene_builder::extract_gltf_trimeshes(
                    &gltf_path.to_string_lossy(),
                ) {
                    Ok(meshes) => {
                        if let Some(scene) = self.world.scene_mut(self.scene_id) {
                            let body_kind = match physics
                                .and_then(|p| p.get("body_kind"))
                                .and_then(|v| v.as_str())
                                .unwrap_or_else(|| model["body_kind"].as_str().unwrap_or("fixed"))
                            {
                                "dynamic" => BodyKind::Dynamic,
                                _ => BodyKind::Fixed,
                            };

                            if body_kind == BodyKind::Dynamic {
                                let bbox = compute_aabb_from_trimeshes(&meshes);
                                let half = Vec3::new(
                                    (bbox.max_x - bbox.min_x) * 0.5,
                                    (bbox.max_y - bbox.min_y) * 0.5,
                                    (bbox.max_z - bbox.min_z) * 0.5,
                                );
                                let shape = ShapeDesc::Cuboid { half_extents: half };
                                let desc = BodyDesc {
                                    kind: BodyKind::Dynamic,
                                    position: iso,
                                    friction: 0.5,
                                    ..Default::default()
                                };
                                if let Err(e) = scene.add_body(desc, shape) {
                                    eprintln!("[PhysicsManager] add_body (dynamic bbox) failed: {e}");
                                }
                            } else {
                                if let Err(e) = scene.add_trimeshes(&meshes, iso, body_kind, 0.5, 0.0) {
                                    eprintln!("[PhysicsManager] add_trimeshes failed: {e}");
                                }
                            }
                        }
                    }
                    Err(e) => eprintln!("[PhysicsManager] extract_gltf_trimeshes failed: {e}"),
                }
            }
        }

        // 2. Procedural objects (cube / plane)
        if let Some(objects) = manifest["objects"].as_array() {
            for obj in objects {
                // 读取物理组件定义（优先新格式 physics 字段）
                let physics = obj.get("physics");
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
                let iso = iso_from_parts(pos, rot);

                let shape = match obj_type {
                    "plane" => ShapeDesc::Cuboid {
                        half_extents: Vec3::new(scale[0] * 0.5, 0.01, scale[2] * 0.5),
                    },
                    "cube" => ShapeDesc::Cuboid {
                        half_extents: Vec3::new(scale[0] * 0.5, scale[1] * 0.5, scale[2] * 0.5),
                    },
                    _ => continue,
                };

                let desc = BodyDesc {
                    kind: match physics
                        .and_then(|p| p.get("body_kind"))
                        .and_then(|v| v.as_str())
                        .unwrap_or_else(|| obj["body_kind"].as_str().unwrap_or("fixed"))
                    {
                        "dynamic" => BodyKind::Dynamic,
                        _ => BodyKind::Fixed,
                    },
                    position: iso,
                    ..Default::default()
                };

                let _ = self.world.scene_mut(self.scene_id)
                    .and_then(|s| s.add_body(desc, shape).ok());
            }
        }
    }

    // -------------------------------------------------------------------
    // Simulation stepping
    // -------------------------------------------------------------------

    /// Step the physics simulation.
    pub fn step(&mut self, dt: f32) {
        if let Some(scene) = self.world.scene_mut(self.scene_id) {
            scene.step(dt);
        }
    }

    // -------------------------------------------------------------------
    // Body snapshots (for debug rendering)
    // -------------------------------------------------------------------

    /// Get snapshots of all bodies from local physics.
    pub fn get_body_snapshots(&self) -> Vec<BodySnapshot> {
        let mut snapshots = Vec::new();
        if let Some(scene) = self.world.scene(self.scene_id) {
            for handle in scene.body_handles() {
                if let Some(iso) = scene.body_isometry(handle) {
                    let (idx, _gen) = handle.raw().into_raw_parts();
                    snapshots.push(BodySnapshot {
                        id: format!("local_{}", idx),
                        position: Position3 {
                            x: iso.translation.x as f64,
                            y: iso.translation.y as f64,
                            z: iso.translation.z as f64,
                        },
                        rotation: Quat4 {
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

    /// Read world transforms for a list of body handles.
    pub fn get_body_transforms(&self, handles: &[BodyHandle]) -> Vec<(Vec3, Quat)> {
        let mut transforms = Vec::with_capacity(handles.len());
        if let Some(scene) = self.world.scene(self.scene_id) {
            for &handle in handles {
                if let Some(iso) = scene.body_isometry(handle) {
                    transforms.push((iso.translation, iso.rotation));
                }
            }
        }
        transforms
    }

    /// Return the number of bodies in the local physics scene (for debugging).
    pub fn body_count(&self) -> usize {
        if let Some(scene) = self.world.scene(self.scene_id) {
            scene.body_handles().count()
        } else {
            0
        }
    }

    /// Drain collision events from local physics.
    pub fn drain_collision_events(&self) -> Vec<physics::CollisionEvent> {
        if let Some(scene) = self.world.scene(self.scene_id) {
            scene.drain_collision_events()
        } else {
            Vec::new()
        }
    }

    /// Access the local physics scene ID.
    pub fn scene_id(&self) -> SceneId {
        self.scene_id
    }

    /// Access the local physics world (mutable).
    pub fn world_mut(&mut self) -> &mut PhysicsWorld {
        &mut self.world
    }

    // -------------------------------------------------------------------
    // Dynamic body creation / removal
    // -------------------------------------------------------------------

    /// Create a dynamic capsule body in the managed scene.
    ///
    /// Returns the [`BodyHandle`] that can later be passed to
    /// [`remove_body`](Self::remove_body) or query methods.
    pub fn create_capsule_body(
        &mut self,
        position: [f32; 3],
        half_height: f32,
        radius: f32,
    ) -> Result<BodyHandle, String> {
        let scene = self
            .world
            .scene_mut(self.scene_id)
            .ok_or_else(|| "physics scene not available".to_string())?;
        let desc = BodyDesc {
            kind: BodyKind::Dynamic,
            position: physics::math::iso_from_parts(
                (position[0], position[1], position[2]),
                (0.0, 0.0, 0.0, 1.0),
            ),
            can_sleep: false,
            ccd_enabled: true,
            ..Default::default()
        };
        let shape = ShapeDesc::Capsule {
            half_height,
            radius,
        };
        scene.add_body(desc, shape).map(|(h, _)| h)
    }

    /// Remove a body from the managed scene.
    pub fn remove_body(&mut self, handle: BodyHandle) {
        if let Some(scene) = self.world.scene_mut(self.scene_id) {
            scene.remove_body(handle);
        }
    }

    /// Access the local physics world.
    pub fn world(&self) -> &PhysicsWorld {
        &self.world
    }

    // -------------------------------------------------------------------
    // State migration (export / switch / import)
    // -------------------------------------------------------------------

    /// Export full state snapshots of all bodies in the current scene.
    pub fn export_snapshots(&self) -> Vec<StateSnapshot> {
        let mut snapshots = Vec::new();
        if let Some(scene) = self.world.scene(self.scene_id) {
            for handle in scene.body_handles() {
                let iso = scene.body_isometry(handle);
                let linvel = scene.body_linvel(handle);
                let angvel = scene.body_angvel(handle);
                if let Some(iso) = iso {
                    let lv = linvel.unwrap_or(Vec3::ZERO);
                    let av = angvel.unwrap_or(Vec3::ZERO);
                    snapshots.push(StateSnapshot {
                        position: cgmath::Vector3::new(
                            iso.translation.x,
                            iso.translation.y,
                            iso.translation.z,
                        ),
                        rotation: cgmath::Quaternion::new(
                            iso.rotation.w,
                            iso.rotation.x,
                            iso.rotation.y,
                            iso.rotation.z,
                        ),
                        linear_velocity: cgmath::Vector3::new(lv.x, lv.y, lv.z),
                        angular_velocity: cgmath::Vector3::new(av.x, av.y, av.z),
                    });
                }
            }
        }
        snapshots
    }

    /// Switch to a new physics scene, returning the new `SceneId` and
    /// snapshots exported from the old scene before the switch.
    pub fn switch_scene(&mut self, gravity: [f32; 3]) -> (SceneId, Vec<StateSnapshot>) {
        let snapshots = self.export_snapshots();
        let new_id = self.world.create_scene(Vec3::new(gravity[0], gravity[1], gravity[2]));
        self.scene_id = new_id;
        (new_id, snapshots)
    }

    /// Restore rigid-body states from snapshots into the current scene.
    ///
    /// Bodies are matched by iteration order (index). The scene must already
    /// contain the same number of bodies (e.g. loaded via `load_scene`)
    /// for the mapping to be meaningful.
    pub fn import_snapshots(&mut self, snapshots: &[StateSnapshot]) {
        if let Some(scene) = self.world.scene_mut(self.scene_id) {
            let handles: Vec<_> = scene.body_handles().collect();
            for (i, snap) in snapshots.iter().enumerate() {
                if i >= handles.len() {
                    break;
                }
                let h = handles[i];
                let pos = Vec3::new(snap.position.x, snap.position.y, snap.position.z);
                let rot = Quat::from_xyzw(snap.rotation.v.x, snap.rotation.v.y, snap.rotation.v.z, snap.rotation.s);
                let lv = Vec3::new(snap.linear_velocity.x, snap.linear_velocity.y, snap.linear_velocity.z);
                let av = Vec3::new(snap.angular_velocity.x, snap.angular_velocity.y, snap.angular_velocity.z);
                scene.set_translation(h, pos, true);
                scene.set_rotation(h, rot, true);
                scene.set_linvel(h, lv, true);
                scene.set_angvel(h, av, true);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

fn euler_to_quat(yaw_deg: f32, pitch_deg: f32, roll_deg: f32) -> (f32, f32, f32, f32) {
    let yaw = yaw_deg.to_radians() * 0.5;
    let pitch = pitch_deg.to_radians() * 0.5;
    let roll = roll_deg.to_radians() * 0.5;
    let (cy, sy) = (yaw.cos(), yaw.sin());
    let (cp, sp) = (pitch.cos(), pitch.sin());
    let (cr, sr) = (roll.cos(), roll.sin());
    (
        sr * cp * cy - cr * sp * sy,
        cr * sp * cy + sr * cp * sy,
        cr * cp * sy - sr * sp * cy,
        cr * cp * cy + sr * sp * sy,
    )
}

fn iso_from_parts(t: [f32; 3], rot: (f32, f32, f32, f32)) -> physics::math::Iso3 {
    physics::math::iso_from_parts((t[0], t[1], t[2]), rot)
}

/// Axis-aligned bounding box.
struct Aabb {
    min_x: f32,
    min_y: f32,
    min_z: f32,
    max_x: f32,
    max_y: f32,
    max_z: f32,
}

/// Compute AABB from triangle mesh vertices.
fn compute_aabb_from_trimeshes(meshes: &[physics::scene_builder::TrimeshData]) -> Aabb {
    let mut min_x = f32::MAX;
    let mut min_y = f32::MAX;
    let mut min_z = f32::MAX;
    let mut max_x = f32::MIN;
    let mut max_y = f32::MIN;
    let mut max_z = f32::MIN;

    for mesh in meshes {
        for v in &mesh.vertices {
            min_x = min_x.min(v[0]);
            min_y = min_y.min(v[1]);
            min_z = min_z.min(v[2]);
            max_x = max_x.max(v[0]);
            max_y = max_y.max(v[1]);
            max_z = max_z.max(v[2]);
        }
    }

    Aabb {
        min_x,
        min_y,
        min_z,
        max_x,
        max_y,
        max_z,
    }
}
