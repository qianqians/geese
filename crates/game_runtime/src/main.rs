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
// 游戏摄像机（camera::Camera 薄封装 + 模式切换）
// ---------------------------------------------------------------------------

use camera::{Camera, CameraMode};

struct GameCamera {
    inner: Camera,
    active_mode: CameraMode,
}

impl GameCamera {
    fn new(aspect: f32) -> Self {
        let mode = CameraMode::Fps {
            yaw: -90.0_f32.to_radians(),
            pitch: 0.0,
        };
        let mut inner = Camera::new(mode, aspect);
        inner.position = Point3::new(0.0, 2.0, 5.0);
        Self { inner, active_mode: mode }
    }

    fn move_forward(&mut self, amount: f32) { self.inner.move_forward(amount); }
    fn move_right(&mut self, amount: f32) { self.inner.move_right(amount); }
    fn rotate_fps(&mut self, dx: f32, dy: f32) { self.inner.rotate_fps(dx, dy); }

    fn orbit(&mut self, dx: f32, dy: f32) { self.inner.orbit(dx, dy); }
    fn pan(&mut self, dx: f32, dy: f32) { self.inner.pan(dx, dy); }
    fn zoom(&mut self, delta: f32) { self.inner.zoom(delta); }

    fn topdown_pan(&mut self, dx: f32, dy: f32) { self.inner.topdown_pan(dx, dy); }
    fn topdown_zoom(&mut self, delta: f32) { self.inner.topdown_zoom(delta); }

    fn update_aspect(&mut self, w: u32, h: u32) { self.inner.update_aspect(w, h); }
    fn view_projection(&self) -> [[f32;4];4] { self.inner.view_projection_raw() }
    fn camera_position(&self) -> [f32;3] { self.inner.camera_position_raw() }
    fn frustum(&self) -> camera::frustum::Frustum { self.inner.frustum() }

    fn switch_mode(&mut self, mode: CameraMode) {
        self.active_mode = mode;
        self.inner.set_mode(mode);
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
            .update_camera(&self.queue, self.camera.view_projection(), self.camera.camera_position());

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

                // 摄像机输入——路由到当前模式
                const MOVE_SPEED: f32 = 5.0;
                match state.camera.active_mode {
                    CameraMode::Fps { .. } => {
                        if keys_pressed.contains(&KeyCode::KeyW) { state.camera.move_forward(MOVE_SPEED * dt); }
                        if keys_pressed.contains(&KeyCode::KeyS) { state.camera.move_forward(-MOVE_SPEED * dt); }
                        if keys_pressed.contains(&KeyCode::KeyA) { state.camera.move_right(-MOVE_SPEED * dt); }
                        if keys_pressed.contains(&KeyCode::KeyD) { state.camera.move_right(MOVE_SPEED * dt); }
                        if mouse_delta.0 != 0.0 || mouse_delta.1 != 0.0 {
                            state.camera.rotate_fps(mouse_delta.0, mouse_delta.1);
                        }
                    }
                    CameraMode::Orbit { .. } => {
                        if mouse_delta.0 != 0.0 || mouse_delta.1 != 0.0 {
                            state.camera.orbit(mouse_delta.0, mouse_delta.1);
                        }
                        if keys_pressed.contains(&KeyCode::KeyW) { state.camera.pan(0.0, 5.0 * dt); }
                        if keys_pressed.contains(&KeyCode::KeyS) { state.camera.pan(0.0, -5.0 * dt); }
                        if keys_pressed.contains(&KeyCode::KeyA) { state.camera.pan(-5.0 * dt, 0.0); }
                        if keys_pressed.contains(&KeyCode::KeyD) { state.camera.pan(5.0 * dt, 0.0); }
                    }
                    CameraMode::TopDown { .. } => {
                        if keys_pressed.contains(&KeyCode::KeyW) { state.camera.topdown_pan(0.0, 5.0 * dt); }
                        if keys_pressed.contains(&KeyCode::KeyS) { state.camera.topdown_pan(0.0, -5.0 * dt); }
                        if keys_pressed.contains(&KeyCode::KeyA) { state.camera.topdown_pan(-5.0 * dt, 0.0); }
                        if keys_pressed.contains(&KeyCode::KeyD) { state.camera.topdown_pan(5.0 * dt, 0.0); }
                    }
                }
                mouse_delta = (0.0, 0.0);

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
