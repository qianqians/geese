//! 游戏运行时核心库——winit + wgpu 游戏循环。
//!
//! 提供平台无关的 [`GameState`] 和 [`run_event_loop`]，供桌面和 Android
//! 两个入口共享。Android 入口通过 `android_main` 符号由 NativeActivity 调用。

pub mod script_host;

#[cfg(target_os = "ios")]
pub mod ios;

#[cfg(target_arch = "wasm32")]
pub mod wasm;

use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

use cgmath::Point3;
use config::{ConfigRenderingPath, EngineConfig};
use render::{
    build_post_uniform, Light, Material, MaterialLibrary, PostChain, PostEffect,
    PostProcessPipeline, RenderQueue, SceneRenderer, WgpuSceneRenderer,
    WgpuSceneRendererDescriptor,
};
use scene::Scene;
use physics::{PhysicsWorld, SceneId};
use physics::math::Vec3 as PhyVec3;
use winit::{
    event::{DeviceEvent, ElementState, Event, KeyEvent, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    keyboard::{KeyCode, PhysicalKey},
};

// ---------------------------------------------------------------------------
// 游戏摄像机（camera::Camera 薄封装 + 模式切换）
// ---------------------------------------------------------------------------

use camera::{Camera, CameraMode};

pub struct GameCamera {
    inner: Camera,
    active_mode: CameraMode,
}

impl GameCamera {
    pub fn new(aspect: f32) -> Self {
        let mode = CameraMode::Fps {
            yaw: -90.0_f32.to_radians(),
            pitch: 0.0,
        };
        let mut inner = Camera::new(mode, aspect);
        inner.position = Point3::new(0.0, 2.0, 5.0);
        Self { inner, active_mode: mode }
    }

    pub fn move_forward(&mut self, amount: f32) { self.inner.move_forward(amount); }
    pub fn move_right(&mut self, amount: f32) { self.inner.move_right(amount); }
    pub fn rotate_fps(&mut self, dx: f32, dy: f32) { self.inner.rotate_fps(dx, dy); }

    pub fn orbit(&mut self, dx: f32, dy: f32) { self.inner.orbit(dx, dy); }
    pub fn pan(&mut self, dx: f32, dy: f32) { self.inner.pan(dx, dy); }
    pub fn zoom(&mut self, delta: f32) { self.inner.zoom(delta); }

    pub fn topdown_pan(&mut self, dx: f32, dy: f32) { self.inner.topdown_pan(dx, dy); }
    pub fn topdown_zoom(&mut self, delta: f32) { self.inner.topdown_zoom(delta); }

    pub fn update_aspect(&mut self, w: u32, h: u32) { self.inner.update_aspect(w, h); }
    pub fn view_projection(&self) -> [[f32;4];4] { self.inner.view_projection_raw() }
    pub fn camera_position(&self) -> [f32;3] { self.inner.camera_position_raw() }
    pub fn frustum(&self) -> camera::frustum::Frustum { self.inner.frustum() }
    pub fn active_mode(&self) -> CameraMode { self.active_mode }

    pub fn switch_mode(&mut self, mode: CameraMode) {
        self.active_mode = mode;
        self.inner.set_mode(mode);
    }
}

// ---------------------------------------------------------------------------
// 游戏状态
// ---------------------------------------------------------------------------

pub struct GameState {
    pub window: Arc<winit::window::Window>,
    pub surface: wgpu::Surface<'static>,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub config: wgpu::SurfaceConfiguration,
    pub renderer: WgpuSceneRenderer,
    pub scene_renderer: SceneRenderer,
    pub scene: Scene,
    pub physics: PhysicsWorld,
    pub physics_scene_id: SceneId,
    pub camera: GameCamera,
    pub materials: MaterialLibrary,
    pub depth_texture: wgpu::Texture,
    pub depth_view: wgpu::TextureView,
    pub last_frame: Instant,

    // ── Post-processing ──────────────────────────────────────────────────
    /// When `true`, the scene is first rendered to an HDR intermediate texture,
    /// then `PostProcessPipeline::process()` composites Bloom + Tonemap + TAA
    /// onto the surface. When `false`, rendering goes directly to the surface.
    pub post_processing_enabled: bool,
    pub post_pipeline: PostProcessPipeline,
    pub post_chain: PostChain,
    hdr_texture: wgpu::Texture,
    hdr_view: wgpu::TextureView,
    frame_index: u64,
}

impl GameState {
    pub async fn new(
        window: winit::window::Window,
        project_dir: &str,
        scene_file: &str,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        Self::new_with_config(window, project_dir, scene_file, &EngineConfig::default()).await
    }

    pub async fn new_with_config(
        window: winit::window::Window,
        project_dir: &str,
        scene_file: &str,
        engine_config: &EngineConfig,
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

        // ── 渲染器（根据配置选择管线） ──
        let renderer_desc = match engine_config.render.rendering_path {
            ConfigRenderingPath::ForwardPlus => {
                WgpuSceneRendererDescriptor::forward_plus(
                    surface_format,
                    size.width,
                    size.height,
                )
            }
            ConfigRenderingPath::DeferredPlus => {
                WgpuSceneRendererDescriptor::deferred_plus(
                    surface_format,
                    size.width,
                    size.height,
                )
            }
        };
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
        let mut physics = PhysicsWorld::new();
        let physics_scene_id = physics.create_scene(PhyVec3::new(0.0, -9.81, 0.0));

        // ── 摄像机 ──
        let camera = GameCamera::new(size.width as f32 / size.height.max(1) as f32);

        // ── Post-processing ──────────────────────────────────────────────
        // Build a sensible default post-chain: ACES Tonemap + Bloom.
        // Users can modify `state.post_chain` at runtime to add/remove effects.
        let mut post_chain = PostChain::new();
        post_chain
            .push(PostEffect::aces(1.0))
            .push(PostEffect::bloom(1.0, 0.15));

        let post_pipeline = PostProcessPipeline::new(
            &device,
            &queue,
            surface_format,
            size.width,
            size.height,
        );

        let hdr_texture = create_hdr_texture(&device, surface_format, size.width, size.height);
        let hdr_view = hdr_texture.create_view(&wgpu::TextureViewDescriptor::default());

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
            physics_scene_id,
            camera,
            materials,
            depth_texture,
            depth_view,
            last_frame: Instant::now(),

            post_processing_enabled: true,
            post_pipeline,
            post_chain,
            hdr_texture,
            hdr_view,
            frame_index: 0,
        })
    }

    pub fn handle_resize(&mut self, width: u32, height: u32) {
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

        // Rebuild HDR intermediate texture for post-processing.
        self.hdr_texture = create_hdr_texture(
            &self.device,
            self.config.format,
            width,
            height,
        );
        self.hdr_view = self
            .hdr_texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        self.post_pipeline.resize(&self.device, width, height);

        self.renderer
            .resize(&self.device, &self.queue, width, height, 0.1, 500.0);
        self.camera.update_aspect(width, height);
    }

    pub fn update(&mut self, dt: f32) {
        if let Some(scene) = self.physics.scene_mut(self.physics_scene_id) {
            scene.step(dt);
        }
    }

    pub fn render(&mut self) -> Result<(), wgpu::SurfaceError> {
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
        let surface_view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("game_encoder"),
            });

        if self.post_processing_enabled {
            // ── Render scene to HDR intermediate ──────────────────────────
            self.renderer.render(
                &self.device,
                &mut encoder,
                &self.hdr_view,
                Some(&self.depth_view),
            );

            // ── Post-process: HDR → surface ───────────────────────────────
            let post_uniform = build_post_uniform(&self.post_chain, self.frame_index);
            self.post_pipeline.process(
                &self.device,
                &self.queue,
                &mut encoder,
                &self.hdr_view,
                &surface_view,
                &post_uniform,
            );
        } else {
            // ── Direct render to surface (no post-processing) ────────────
            self.renderer
                .render(&self.device, &mut encoder, &surface_view, Some(&self.depth_view));
        }

        self.queue.submit(std::iter::once(encoder.finish()));
        output.present();
        self.frame_index = self.frame_index.wrapping_add(1);

        Ok(())
    }
}

// ---------------------------------------------------------------------------
// 辅助函数：创建后处理 HDR 中间纹理
// ---------------------------------------------------------------------------

/// 创建用于后处理的 HDR 中间渲染目标纹理。
///
/// 场景先渲染到此纹理，再由 `PostProcessPipeline::process()` 进行
/// Bloom + Tonemap 合成后输出到 surface。
fn create_hdr_texture(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
    width: u32,
    height: u32,
) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("post_hdr_intermediate"),
        size: wgpu::Extent3d {
            width: width.max(1),
            height: height.max(1),
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    })
}

// ---------------------------------------------------------------------------
// 平台无关的事件循环
// ---------------------------------------------------------------------------

/// 运行游戏事件循环。此函数消耗 [`EventLoop`]，在桌面平台永不返回。
///
/// # 参数
/// - `event_loop`: 已构建的 winit 事件循环（桌面用 `EventLoop::new()`，
///   Android 用 `EventLoopBuilder::new().with_android_app(app).build()`）
/// - `window`: 游戏窗口
/// - `project_dir`: 项目目录路径
/// - `scene_file`: 场景文件路径（相对于 project_dir）
pub fn run_event_loop(
    event_loop: EventLoop<()>,
    window: winit::window::Window,
    project_dir: &str,
    scene_file: &str,
) {
    let project_dir = project_dir.to_string();
    let scene_file = scene_file.to_string();

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
                match state.camera.active_mode() {
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

// ---------------------------------------------------------------------------
// Android 入口
// ---------------------------------------------------------------------------

#[cfg(target_os = "android")]
use winit::platform::android::EventLoopBuilderExtAndroid;

/// Android NativeActivity 入口，由 android.app.NativeActivity 在专用线程上调用。
#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
fn android_main(app: winit::platform::android::activity::AndroidApp) {
    android_logger::init_once(
        android_logger::Config::default()
            .with_max_level(log::LevelFilter::Info)
            .with_tag("geese_game"),
    );

    let project_dir = std::env::args()
        .nth(1)
        .unwrap_or_else(|| ".".to_string());
    let scene_file = std::env::args()
        .nth(2)
        .unwrap_or_else(|| "assets/scenes/default.scene.json".to_string());

    use winit::event_loop::EventLoop;
    use winit::window::WindowAttributes;

    let mut event_loop_builder = EventLoop::builder();
    event_loop_builder.with_android_app(app);
    let event_loop = event_loop_builder.build().unwrap();

    let window = event_loop
        .create_window(WindowAttributes::default().with_title("Geese Game"))
        .unwrap();

    run_event_loop(event_loop, window, &project_dir, &scene_file);
}
