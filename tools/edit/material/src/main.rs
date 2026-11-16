//! 材质编辑器入口：winit + wgpu + egui 集成。
//!
//! 用法：`material-editor [project_dir]`（默认当前目录）。
//! 读取/写入 `<project_dir>/assets/materials/*.material.json`，
//! 窗口背景实时渲染一个球体预览当前材质。

mod app;
mod material_store;
mod sphere;

use std::path::Path;
use std::sync::Arc;

use app::EditorApp;
use material::{Material, MaterialLibrary};
use render::{ModelMesh, RenderObject, SceneRenderer, WgpuSceneRenderer, WgpuSceneRendererDescriptor};
use winit::{
    dpi::LogicalSize,
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    window::{Window, WindowAttributes},
};

/// 预览球体：实现 `RenderObject`，模型矩阵为单位阵（球体已在原点、半径 1）。
struct SphereObject {
    mesh: ModelMesh,
    model: [[f32; 4]; 4],
    normal: [[f32; 4]; 4],
    joints: Vec<[[f32; 4]; 4]>,
}

impl RenderObject for SphereObject {
    fn entity_id(&self) -> &str {
        "preview_sphere"
    }

    fn mesh(&self) -> &ModelMesh {
        &self.mesh
    }

    fn model_matrix(&self) -> [[f32; 4]; 4] {
        self.model
    }

    fn normal_matrix(&self) -> [[f32; 4]; 4] {
        self.normal
    }

    fn joint_matrices(&self) -> &[[[f32; 4]; 4]] {
        &self.joints
    }
}

struct EditorState {
    window: Arc<Window>,
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    renderer: WgpuSceneRenderer,
    scene_renderer: SceneRenderer,
    depth_texture: wgpu::Texture,
    depth_view: wgpu::TextureView,
    egui_ctx: egui::Context,
    egui_state: egui_winit::State,
    egui_renderer: egui_wgpu::Renderer,
    app: EditorApp,
    sphere_object: SphereObject,
}

impl EditorState {
    async fn new(window: Arc<Window>, project_dir: &str) -> Result<Self, Box<dyn std::error::Error>> {
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
            .expect("未找到合适的 GPU 适配器");

        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label: Some("Material Editor Device"),
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

        let depth_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("depth"),
            size: wgpu::Extent3d {
                width: size.width.max(1),
                height: size.height.max(1),
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

        // ── 场景渲染器（Forward+）──
        let renderer = WgpuSceneRenderer::new(
            &device,
            &queue,
            WgpuSceneRendererDescriptor::forward_plus(surface_format, size.width, size.height),
        );
        let scene_renderer = SceneRenderer::new(Material::default());

        // ── egui 集成 ──
        let egui_ctx = egui::Context::default();
        let egui_state = egui_winit::State::new(
            egui_ctx.clone(),
            egui::ViewportId::ROOT,
            window.as_ref(),
            Some(window.scale_factor() as f32),
            None,
            Some(2048),
        );
        let egui_renderer = egui_wgpu::Renderer::new(
            &device,
            surface_format,
            Some(wgpu::TextureFormat::Depth32Float),
            1,
            false,
        );

        // ── 编辑器状态 + 预览球体 ──
        let app = EditorApp::new(Path::new(project_dir))?;
        let sphere_object = SphereObject {
            mesh: sphere::generate_uv_sphere(64, 64),
            model: render::common::identity_matrix(),
            normal: render::common::identity_matrix(),
            joints: Vec::new(),
        };

        Ok(Self {
            window,
            surface,
            device,
            queue,
            config,
            renderer,
            scene_renderer,
            depth_texture,
            depth_view,
            egui_ctx,
            egui_state,
            egui_renderer,
            app,
            sphere_object,
        })
    }

    fn resize(&mut self, width: u32, height: u32) {
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
        self.app.update_aspect(width, height);
    }

    fn render(&mut self) -> Result<(), wgpu::SurfaceError> {
        // ── 1. egui 输入 + UI ──
        let raw_input = self.egui_state.take_egui_input(self.window.as_ref());
        let full_output = self.egui_ctx.run(raw_input, |ctx| {
            self.app.ui(ctx);
        });
        let egui::FullOutput {
            platform_output,
            textures_delta,
            shapes,
            pixels_per_point,
            ..
        } = full_output;
        self.egui_state
            .handle_platform_output(self.window.as_ref(), platform_output);

        // ── 2. 预览相机输入（指针不在 UI 上时）──
        self.handle_preview_input();

        // ── 3. 预览材质库（每帧重建，只含当前材质）──
        let material_library = self.build_preview_library();

        // ── 4. 渲染球体到 surface ──
        let output = self.surface.get_current_texture()?;
        let surface_view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("editor_encoder"),
            });

        self.renderer.update_camera(
            &self.queue,
            self.app.view_projection(),
            self.app.camera_position(),
        );
        self.renderer
            .update_lights(&self.queue, self.app.ambient, &self.app.lights);

        let render_queue = self
            .scene_renderer
            .build_queue(&material_library, std::iter::once(&self.sphere_object));
        self.renderer
            .prepare(&self.device, &self.queue, &material_library, &render_queue);
        self.renderer
            .render(&self.device, &mut encoder, &surface_view, Some(&self.depth_view));

        // ── 5. egui 叠加渲染 ──
        let screen_descriptor = egui_wgpu::ScreenDescriptor {
            size_in_pixels: [self.config.width, self.config.height],
            pixels_per_point: self.window.scale_factor() as f32,
        };
        let clipped_primitives = self.egui_ctx.tessellate(shapes, pixels_per_point);

        for (id, image_delta) in &textures_delta.set {
            self.egui_renderer
                .update_texture(&self.device, &self.queue, *id, image_delta);
        }

        {
            let egui_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("egui_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &surface_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.depth_view,
                    depth_ops: None,
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            self.egui_renderer
                .render(&mut egui_pass.forget_lifetime(), &clipped_primitives, &screen_descriptor);
        }

        for id in &textures_delta.free {
            self.egui_renderer.free_texture(id);
        }

        // ── 6. 提交 ──
        self.queue.submit(std::iter::once(encoder.finish()));
        output.present();

        Ok(())
    }

    fn build_preview_library(&self) -> MaterialLibrary {
        let mut library = MaterialLibrary::default();
        library.insert(self.app.preview_material());
        library
    }

    fn handle_preview_input(&mut self) {
        let (delta, primary_down, scroll) = self.egui_ctx.input(|i| {
            (
                i.pointer.delta(),
                i.pointer.primary_down(),
                i.raw_scroll_delta.y,
            )
        });
        let over_ui = self.egui_ctx.wants_pointer_input();

        if primary_down && !over_ui {
            self.app.orbit(delta.x, delta.y);
        }
        if scroll != 0.0 && !over_ui {
            self.app.zoom(scroll * 0.5);
        }
    }
}

#[allow(deprecated)] // `EventLoop::create_window` 与 `EventLoop::run` 沿用引擎 game_runtime 的写法
fn main() {
    env_logger::init();

    let project_dir = std::env::args()
        .nth(1)
        .unwrap_or_else(|| ".".to_string());

    let event_loop = EventLoop::new().expect("创建事件循环失败");
    let window = Arc::new(
        event_loop
            .create_window(
                WindowAttributes::default()
                    .with_title("Geese 材质编辑器")
                    .with_inner_size(LogicalSize::new(1280.0, 800.0)),
            )
            .expect("创建窗口失败"),
    );

    let mut state = pollster::block_on(EditorState::new(window.clone(), &project_dir))
        .expect("初始化材质编辑器失败");

    #[allow(deprecated)]
    let _ = event_loop.run(move |event, window_target| {
        window_target.set_control_flow(ControlFlow::Poll);

        match event {
            Event::WindowEvent { event, .. } => {
                let response = state
                    .egui_state
                    .on_window_event(state.window.as_ref(), &event);
                if response.repaint {
                    state.window.request_redraw();
                }

                match event {
                    WindowEvent::CloseRequested => window_target.exit(),
                    WindowEvent::Resized(size) => state.resize(size.width, size.height),
                    _ => {}
                }
            }
            Event::AboutToWait => match state.render() {
                Ok(()) => {}
                Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                    let size = state.window.inner_size();
                    state.resize(size.width, size.height);
                }
                Err(wgpu::SurfaceError::OutOfMemory) => window_target.exit(),
                Err(wgpu::SurfaceError::Timeout) => {}
            },
            _ => {}
        }
    });
}
