//! [`PhysicsManager`] — unified physics backend with configurable source.

use std::path::Path;
use std::sync::Arc;
use std::process::{Child, Command};
use std::thread;
use std::time::Duration;

use physics::{PhysicsWorld, SceneId};
use physics::handles::BodyHandle;
use physics::math::{Vec3, Quat};
use physics::world::{BodyDesc, BodyKind};
use physics::shapes::ShapeDesc;
use physics_client::{BodySnapshot, PhysicsClient};

/// Where physics simulation runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PhysicsSource {
    /// Local in-process physics only (using `PhysicsWorld`).
    Client,
    /// Remote physics server only (Python process via TCP).
    Server,
    /// Both local and remote physics.
    ClientAndServer,
}

impl PhysicsSource {
    pub fn runs_local(&self) -> bool {
        matches!(self, Self::Client | Self::ClientAndServer)
    }

    pub fn runs_remote(&self) -> bool {
        matches!(self, Self::Server | Self::ClientAndServer)
    }
}

// ---------------------------------------------------------------------------
// PhysicsManager
// ---------------------------------------------------------------------------

/// Manages physics simulation using a configurable source.
///
/// ## Local Physics
/// Always wraps a [`PhysicsWorld`] with a single scene.
///
/// ## Remote Physics
/// Connects to a Python physics server process via TCP + msgpack.
pub struct PhysicsManager {
    source: PhysicsSource,
    /// Local physics world (used when `source.runs_local()`).
    world: PhysicsWorld,
    scene_id: SceneId,
    /// Remote physics client (used when `source.runs_remote()`).
    remote: Option<RemoteState>,
}

struct RemoteState {
    client: Arc<PhysicsClient>,
    process: Option<Child>,
    #[allow(dead_code)]
    port: u16,
}

impl PhysicsManager {
    /// Create a new manager with the given source and gravity.
    ///
    /// `gravity`: `[gx, gy, gz]` in m/s^2.
    pub fn new(source: PhysicsSource, gravity: [f32; 3]) -> Self {
        let mut world = PhysicsWorld::new();
        let scene_id = world.create_scene(Vec3::new(gravity[0], gravity[1], gravity[2]));
        Self {
            source,
            world,
            scene_id,
            remote: None,
        }
    }

    /// Change physics source at runtime.
    pub fn set_source(&mut self, source: PhysicsSource) {
        if source != self.source {
            if !source.runs_remote() {
                self.disconnect_remote();
            }
            self.source = source;
        }
    }

    /// Current physics source.
    pub fn source(&self) -> PhysicsSource {
        self.source
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
                if !model["collision_enabled"].as_bool().unwrap_or(false) {
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
                            if let Err(e) = scene.add_static_trimeshes(&meshes, iso, 0.5, 0.0) {
                                eprintln!("[PhysicsManager] add_static_trimeshes failed: {e}");
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
                    kind: BodyKind::Fixed,
                    position: iso,
                    ..Default::default()
                };

                let _ = self.world.scene_mut(self.scene_id)
                    .and_then(|s| s.add_body(desc, shape).ok());
            }
        }
    }

    /// Load scene into remote physics server (if connected).
    pub async fn load_scene_remote(&self, manifest_path: &str) -> Result<(), String> {
        if let Some(remote) = &self.remote {
            remote.client.load_scene(manifest_path).await?;
        }
        Ok(())
    }

    /// Initialize remote physics world.
    pub async fn init_physics_remote(&self, gravity: [f32; 3]) -> Result<(), String> {
        if let Some(remote) = &self.remote {
            remote.client.init_physics(gravity).await?;
        }
        Ok(())
    }

    // -------------------------------------------------------------------
    // Simulation stepping
    // -------------------------------------------------------------------

    /// Step the physics simulation.
    ///
    /// Advances local physics and/or remote physics depending on `source`.
    pub fn step(&mut self, dt: f32) {
        if self.source.runs_local() {
            if let Some(scene) = self.world.scene_mut(self.scene_id) {
                scene.step(dt);
            }
        }
    }

    /// Step remote physics asynchronously.
    pub async fn step_remote(&self, dt: f64) -> Result<(), String> {
        if let Some(remote) = &self.remote {
            remote.client.step(dt).await?;
        }
        Ok(())
    }

    // -------------------------------------------------------------------
    // Body snapshots (for debug rendering)
    // -------------------------------------------------------------------

    /// Get snapshots of all bodies from local physics.
    pub fn get_local_body_snapshots(&self) -> Vec<BodySnapshot> {
        let mut snapshots = Vec::new();
        if let Some(scene) = self.world.scene(self.scene_id) {
            for handle in scene.body_handles() {
                if let Some(iso) = scene.body_isometry(handle) {
                    let (idx, _gen) = handle.raw().into_raw_parts();
                    snapshots.push(BodySnapshot {
                        id: format!("local_{}", idx),
                        position: physics_client::Vec3 {
                            x: iso.translation.x as f64,
                            y: iso.translation.y as f64,
                            z: iso.translation.z as f64,
                        },
                        rotation: physics_client::Quat {
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

    /// 按句柄列表读取刚体的世界变换 (translation, rotation)。
    ///
    /// 本地模式直接从 PhysicsWorld 读取；远程模式当前回退到本地。
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

    /// 返回本地物理场景中的刚体数量，用于调试。
    pub fn body_count(&self) -> usize {
        if let Some(scene) = self.world.scene(self.scene_id) {
            scene.body_handles().count()
        } else {
            0
        }
    }

    /// Get snapshots from remote physics.
    pub async fn get_remote_body_snapshots(&self) -> Result<Vec<BodySnapshot>, String> {
        if let Some(remote) = &self.remote {
            remote.client.get_bodies().await
        } else {
            Ok(Vec::new())
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

    // -------------------------------------------------------------------
    // Remote server management
    // -------------------------------------------------------------------

    /// Connect to a remote physics server.
    ///
    /// Spawns a Python process serving at a free port, then connects.
    pub fn connect_remote(
        &mut self,
        python_path: &str,
        server_script: &str,
        rt: &tokio::runtime::Runtime,
    ) -> Result<(), String> {
        // Kill any existing remote connection
        self.disconnect_remote();

        let port = find_free_port(9000)?;
        let child = Command::new(python_path)
            .arg(server_script)
            .arg("--port")
            .arg(port.to_string())
            .spawn()
            .map_err(|e| format!("failed to spawn python server: {e}"))?;

        wait_for_server(port, Duration::from_secs(5))?;

        let addr = format!("127.0.0.1:{}", port);
        let client = rt.block_on(PhysicsClient::connect(&addr))?;

        self.remote = Some(RemoteState {
            client: Arc::new(client),
            process: Some(child),
            port,
        });


        Ok(())
    }

    /// Whether remote physics is connected.
    pub fn is_remote_connected(&self) -> bool {
        self.remote.is_some()
    }

    /// Access the remote client (for advanced operations).
    pub fn remote_client(&self) -> Option<Arc<PhysicsClient>> {
        self.remote.as_ref().map(|r| r.client.clone())
    }

    /// Disconnect and kill remote process.
    pub fn disconnect_remote(&mut self) {
        if let Some(remote) = self.remote.take() {
            drop(remote.client); // drops TCP connection
            if let Some(mut child) = remote.process {
                let _ = child.kill();
                let _ = child.wait();
            }
        }
    }

    /// Reset remote physics world.
    pub async fn reset_remote(&self) -> Result<(), String> {
        if let Some(remote) = &self.remote {
            remote.client.reset().await
        } else {
            Ok(())
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

    /// Access the local physics world.
    pub fn world(&self) -> &PhysicsWorld {
        &self.world
    }
}

impl Drop for PhysicsManager {
    fn drop(&mut self) {
        self.disconnect_remote();
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

fn find_free_port(start_port: u16) -> Result<u16, String> {
    for port in start_port..start_port + 100 {
        if std::net::TcpListener::bind(format!("127.0.0.1:{}", port)).is_ok() {
            return Ok(port);
        }
    }
    Err("no free port found in range 9000-9099".to_string())
}

fn wait_for_server(port: u16, timeout: Duration) -> Result<(), String> {
    let start = std::time::Instant::now();
    while start.elapsed() < timeout {
        if let Ok(stream) = std::net::TcpStream::connect(format!("127.0.0.1:{}", port)) {
            let _ = stream.shutdown(std::net::Shutdown::Both);
            return Ok(());
        }
        thread::sleep(Duration::from_millis(100));
    }
    Err(format!("server startup timeout after {}s", timeout.as_secs()))
}
