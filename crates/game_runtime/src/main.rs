//! 独立游戏运行时——winit + wgpu 主循环。
//!
//! 不依赖编辑器或 egui，直接衔接 render / scene / physics 引擎 crates。
//! 启动参数: `geese_game.exe [project_dir] [scene_file]`

use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

use cgmath::{Deg, Matrix4, Point3, Vector3, perspective};
use render::{
    Light, Material, MaterialLibrary, RenderQueue, SceneRenderer,
    WgpuSceneRenderer, WgpuSceneRendererDescriptor,
};
use scene::Scene;
use camera::frustum::Frustum;
use physics_manager::{PhysicsManager, PhysicsSource};
use winit::{
    event::{DeviceEvent, ElementState, Event, KeyEvent, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    keyboard::{KeyCode, PhysicalKey},
    window::WindowAttributes,
};

// ---------------------------------------------------------------------------
// FPS 摄像机
// ---------------------------------------------------------------------------

struct GameCamera {
    position: Point3<f32>,
    yaw: f32,   // 水平旋转（弧度）
    pitch: f32, // 垂直旋转（弧度，限制在 ±89°）
    aspect: f32,
    fov: f32,
    z_near: f32,
    z_far: f32,
}

impl GameCamera {
    fn new(aspect: f32) -> Self {
        Self {
            position: Point3::new(0.0, 2.0, 5.0),
            yaw: -90.0_f32.to_radians(),
            pitch: 0.0,
            aspect,
            fov: 60.0,
            z_near: 0.1,
            z_far: 500.0,
        }
    }

    fn forward(&self) -> Vector3<f32> {
        Vector3::new(
            self.yaw.cos() * self.pitch.cos(),
            self.pitch.sin(),
            self.yaw.sin() * self.pitch.cos(),
        )
    }

    fn right(&self) -> Vector3<f32> {
        Vector3::new(-self.yaw.sin(), 0.0, self.yaw.cos())
    }

    fn view_matrix(&self) -> Matrix4<f32> {
        let forward = self.forward();
        let target = self.position + forward;
        Matrix4::look_at_rh(self.position, target, Vector3::unit_y())
    }

    fn projection_matrix(&self) -> Matrix4<f32> {
        perspective(Deg(self.fov), self.aspect, self.z_near, self.z_far)
    }

    fn view_projection(&self) -> [[f32; 4]; 4] {
        let vp = self.projection_matrix() * self.view_matrix();
        vp.into()
    }

    fn frustum(&self) -> Frustum {
        let vp = self.projection_matrix() * self.view_matrix();
        Frustum::from_view_projection_matrix(&vp)
    }

    fn update_aspect(&mut self, width: u32, height: u32) {
        self.aspect = width as f32 / height.max(1) as f32;
    }

    fn move_forward(&mut self, amount: f32) {
        self.position += self.forward() * amount;
    }

    fn move_right(&mut self, amount: f32) {
        self.position += self.right() * amount;
    }

    fn rotate(&mut self, dx: f32, dy: f32) {
        const SENSITIVITY: f32 = 0.003;
        self.yaw += dx * SENSITIVITY;
        self.pitch = (self.pitch - dy * SENSITIVITY).clamp(
            -89.0_f32.to_radians(),
            89.0_f32.to_radians(),
        );
    }
}

// ---------------------------------------------------------------------------
// 游戏状态
// ---------------------------------------------------------------------------

struct GameState {
    window: Arc<winit::window::Window>,
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    renderer: WgpuSceneRenderer,
    scene_renderer: SceneRenderer,
    scene: Scene,
    physics: PhysicsManager,
    camera: GameCamera,
    materials: MaterialLibrary,
    depth_texture: wgpu::Texture,
    depth_view: wgpu::TextureView,
    last_frame: Instant,
}

impl GameState {
    async fn new(
        window: winit::window::Window,
        project_dir: &str,
        scene_file: &str,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let window = Arc::new(window);
        let size = window.inner_size();

        // ── wgpu 初始化 ──
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::PRIMARY,
            ..Default::default()
        });
        let surface = instance.create_surface(window.clone())?;

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .expect("Failed to find suitable GPU adapter");

        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label: Some("Game Device"),
                    required_features: wgpu::Features::default(),
                    required_limits: wgpu::Limits::default(),
                    memory_hints: wgpu::MemoryHints::default(),
                },
                None,
            )
            .await?;

        let surface_caps = surface.get_capabilities(&adapter);
        let surface_format = surface_caps
            .formats
            .iter()
            .copied()
            .find(|f| f.is_srgb())
            .unwrap_or(surface_caps.formats[0]);

        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: surface_format,
            width: size.width,
            height: size.height,
            present_mode: wgpu::PresentMode::Fifo,
            alpha_mode: surface_caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &config);

        // 深度纹理
        let depth_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("depth"),
            size: wgpu::Extent3d {
                width: size.width,
                height: size.height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth32Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let depth_view = depth_texture.create_view(&wgpu::TextureViewDescriptor::default());

        // ── 渲染器 ──
        let renderer_desc = WgpuSceneRendererDescriptor::forward_plus(
            surface_format,
            size.width,
            size.height,
        );
        let renderer = WgpuSceneRenderer::new(&device, &queue, renderer_desc);

        let default_mat = Material::default();
        let scene_renderer = SceneRenderer::new(default_mat);

        // ── 场景加载 ──
        let scene_path = Path::new(project_dir).join(scene_file);
        let scene = scene::loader::load_scene_from_file(
            &scene_path.to_string_lossy(),
            1000,
            8,
        )?;
        log::info!(
            "Loaded scene: {} objects, {} nodes",
            scene.objects().len(),
            scene.nodes.len()
        );

        let materials = MaterialLibrary::default();

        // ── 物理 ──
        let mut physics = PhysicsManager::new(
            PhysicsSource::Client,
            [0.0, -9.81, 0.0],
        );
        let manifest_path = Path::new(project_dir).join(".scene.json");
        physics.load_scene(&manifest_path.to_string_lossy());

        // ── 摄像机 ──
        let camera = GameCamera::new(size.width as f32 / size.height.max(1) as f32);

        Ok(Self {
            window,
            surface,
            device,
            queue,
            config,
            renderer,
            scene_renderer,
            scene,
            physics,
            camera,
            materials,
            depth_texture,
            depth_view,
            last_frame: Instant::now(),
        })
    }

    fn handle_resize(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }
        self.config.width = width;
        self.config.height = height;
        self.surface.configure(&self.device, &self.config);

        self.depth_texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("depth"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth32Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        self.depth_view = self
            .depth_texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        self.renderer
            .resize(&self.device, &self.queue, width, height, 0.1, 500.0);
        self.camera.update_aspect(width, height);
    }

    fn update(&mut self, dt: f32) {
        self.physics.step(dt);
    }

    fn render(&mut self) -> Result<(), wgpu::SurfaceError> {
        let frustum = self.camera.frustum();
        let queue: RenderQueue<'_> = self
            .scene
            .render_queue(&self.scene_renderer, Some(&frustum));

        self.renderer
            .update_camera(&self.queue, self.camera.view_projection(), [
                self.camera.position.x,
                self.camera.position.y,
                self.camera.position.z,
            ]);

        let ambient = [0.05, 0.05, 0.08];
        let lights: Vec<Light> = vec![
            Light::directional([-0.3, -1.0, -0.5], [1.0, 0.95, 0.85], 1.2),
        ];
        self.renderer.update_lights(&self.queue, ambient, &lights);

        self.renderer
            .prepare(&self.device, &self.queue, &self.materials, &queue);

        let output = self.surface.get_current_texture()?;
        let view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("game_encoder"),
            });

        self.renderer
            .render(&self.device, &mut encoder, &view, Some(&self.depth_view));

        self.queue.submit(std::iter::once(encoder.finish()));
        output.present();

        Ok(())
    }
}

// ---------------------------------------------------------------------------
// 主函数
// ---------------------------------------------------------------------------

fn main() {
    env_logger::init();

    let project_dir = std::env::args()
        .nth(1)
        .unwrap_or_else(|| ".".to_string());
    let scene_file = std::env::args()
        .nth(2)
        .unwrap_or_else(|| "assets/scenes/default.scene.json".to_string());

    let event_loop = EventLoop::new().unwrap();
    let window = event_loop
        .create_window(
            WindowAttributes::default()
                .with_title("Geese Game")
                .with_inner_size(winit::dpi::LogicalSize::new(1280, 720)),
        )
        .unwrap();

    let mut state = pollster::block_on(GameState::new(window, &project_dir, &scene_file))
        .expect("Failed to initialize game");

    // 输入状态
    let mut keys_pressed = std::collections::HashSet::new();
    let mut mouse_delta = (0.0f32, 0.0f32);
    let mut cursor_grabbed = false;

    #[allow(deprecated)]
    let _ = event_loop.run(move |event, window_target| {
        window_target.set_control_flow(ControlFlow::Poll);

        match event {
            Event::WindowEvent { event, .. } => match event {
                WindowEvent::CloseRequested => window_target.exit(),
                WindowEvent::Resized(size) => {
                    state.handle_resize(size.width, size.height);
                }
                WindowEvent::KeyboardInput {
                    event: KeyEvent {
                        physical_key: PhysicalKey::Code(key_code),
                        state: element_state,
                        ..
                    },
                    ..
                } => {
                    let pressed = element_state == ElementState::Pressed;
                    match key_code {
                        KeyCode::Escape => window_target.exit(),
                        _ => {
                            if pressed {
                                keys_pressed.insert(key_code);
                            } else {
                                keys_pressed.remove(&key_code);
                            }
                        }
                    }
                }
                WindowEvent::CursorEntered { .. } => {
                    cursor_grabbed = true;
                }
                _ => {}
            },
            Event::DeviceEvent { event, .. } => {
                if let DeviceEvent::MouseMotion { delta } = event {
                    if cursor_grabbed {
                        mouse_delta.0 += delta.0 as f32;
                        mouse_delta.1 += delta.1 as f32;
                    }
                }
            }
            Event::AboutToWait => {
                // ── 每帧更新 ──
                let now = Instant::now();
                let dt = (now - state.last_frame).as_secs_f32().min(0.1);
                state.last_frame = now;

                // 摄像机移动
                const MOVE_SPEED: f32 = 5.0;
                if keys_pressed.contains(&KeyCode::KeyW) {
                    state.camera.move_forward(MOVE_SPEED * dt);
                }
                if keys_pressed.contains(&KeyCode::KeyS) {
                    state.camera.move_forward(-MOVE_SPEED * dt);
                }
                if keys_pressed.contains(&KeyCode::KeyA) {
                    state.camera.move_right(-MOVE_SPEED * dt);
                }
                if keys_pressed.contains(&KeyCode::KeyD) {
                    state.camera.move_right(MOVE_SPEED * dt);
                }

                // 鼠标旋转
                if mouse_delta.0 != 0.0 || mouse_delta.1 != 0.0 {
                    state.camera.rotate(mouse_delta.0, mouse_delta.1);
                    mouse_delta = (0.0, 0.0);
                }

                // 物理
                state.update(dt);

                // 渲染
                match state.render() {
                    Ok(()) => {}
                    Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                        let size = state.window.inner_size();
                        state.handle_resize(size.width, size.height);
                    }
                    Err(wgpu::SurfaceError::OutOfMemory) => window_target.exit(),
                    Err(wgpu::SurfaceError::Timeout) => {} // 重试下一帧
                }
            }
            _ => {}
        }
    });
}
