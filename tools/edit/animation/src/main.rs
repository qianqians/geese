//! 骨骼动画预览入口：winit + wgpu + egui 集成。
//!
//! 用法：`animation-preview --workdir <工作目录>`
//! 工作目录是必填的：记录写在 `<workdir>/.data/animation-preview.json`，文件对话框也以它为
//! 初始目录。模型只来自记录或对话框（不接受命令行直接指定）。
//!
//! 窗口背景每帧渲染模型的蒙皮网格（`scene::Scene` → Forward+），UI 由 egui 叠在上面。

mod app;
mod model;
mod swap;

use std::path::PathBuf;
use std::sync::Arc;

use app::PreviewApp;
use material::MaterialLibrary;
use render::{SceneRenderer, WgpuSceneRenderer, WgpuSceneRendererDescriptor};
use scene::SceneObject;
use winit::{
    dpi::LogicalSize,
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    window::{Window, WindowAttributes},
};

struct PreviewState {
    window: Arc<Window>,
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    renderer: WgpuSceneRenderer,
    depth_texture: wgpu::Texture,
    depth_view: wgpu::TextureView,
    egui_ctx: egui::Context,
    egui_state: egui_winit::State,
    egui_renderer: egui_wgpu::Renderer,
    app: PreviewApp,
    /// 没有模型时也照常走一遍 `prepare` + `render`：空队列 + 空材质库，
    /// 这样清屏、深度测试等每一帧行为一致。
    empty_renderer: SceneRenderer,
    empty_materials: MaterialLibrary,
}

impl PreviewState {
    async fn new(window: Arc<Window>, workdir: PathBuf) -> Result<Self, Box<dyn std::error::Error>> {
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
                    label: Some("Animation Preview Device"),
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

        let depth_texture = create_depth_texture(&device, size.width, size.height);
        let depth_view = depth_texture.create_view(&wgpu::TextureViewDescriptor::default());

        // ── 场景渲染器（Forward+）──
        let renderer = WgpuSceneRenderer::new(
            &device,
            &queue,
            WgpuSceneRendererDescriptor::forward_plus(surface_format, size.width, size.height),
        );

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

        // ── 预览状态：读工作目录里的记录 → 否则弹窗 ──
        let mut app = PreviewApp::new(workdir);
        app.update_aspect(size.width, size.height);
        app.startup();

        Ok(Self {
            window,
            surface,
            device,
            queue,
            config,
            renderer,
            depth_texture,
            depth_view,
            egui_ctx,
            egui_state,
            egui_renderer,
            app,
            empty_renderer: SceneRenderer::new(render::Material::default()),
            empty_materials: MaterialLibrary::default(),
        })
    }

    fn resize(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }
        self.config.width = width;
        self.config.height = height;
        self.surface.configure(&self.device, &self.config);

        self.depth_texture = create_depth_texture(&self.device, width, height);
        self.depth_view = self
            .depth_texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        self.renderer
            .resize(&self.device, &self.queue, width, height, 0.1, 500.0);
        self.app.update_aspect(width, height);
    }

    fn render(&mut self) -> Result<(), wgpu::SurfaceError> {
        // ── 1. 推进动画 ──
        self.app.advance();

        // ── 2. egui 输入 + UI ──
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

        // ── 3. 预览相机输入（指针不在 UI 上时）──
        self.handle_preview_input();

        // ── 4. 渲染模型 ──
        let output = self.surface.get_current_texture()?;
        let surface_view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("animation_preview_encoder"),
            });

        self.renderer.update_camera(
            &self.queue,
            self.app.view_projection(),
            self.app.camera_position(),
        );
        self.renderer
            .update_lights(&self.queue, self.app.ambient, &self.app.lights);

        match self.app.model() {
            Some(model) => {
                let render_queue = model.scene.render_queue(&model.scene_renderer, None);
                self.renderer.prepare(
                    &self.device,
                    &self.queue,
                    &model.scene.materials,
                    &render_queue,
                );
            }
            None => {
                let render_queue = self
                    .empty_renderer
                    .build_queue(&self.empty_materials, std::iter::empty::<&SceneObject>());
                self.renderer.prepare(
                    &self.device,
                    &self.queue,
                    &self.empty_materials,
                    &render_queue,
                );
            }
        }

        self.renderer.render(
            &self.device,
            &mut encoder,
            &surface_view,
            Some(&self.depth_view),
        );

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
            self.egui_renderer.render(
                &mut egui_pass.forget_lifetime(),
                &clipped_primitives,
                &screen_descriptor,
            );
        }

        for id in &textures_delta.free {
            self.egui_renderer.free_texture(id);
        }

        // ── 6. 提交 ──
        self.queue.submit(std::iter::once(encoder.finish()));
        output.present();

        Ok(())
    }

    fn handle_preview_input(&mut self) {
        let (delta, primary_down, scroll) = self.egui_ctx.input(|i| {
            (
                i.pointer.delta(),
                i.pointer.primary_down(),
                i.raw_scroll_delta.y,
            )
        });
        if self.egui_ctx.wants_pointer_input() {
            return;
        }

        if primary_down {
            self.app.orbit(delta.x, delta.y);
        }
        if scroll != 0.0 {
            self.app.zoom(scroll * 0.5);
        }
    }
}

fn create_depth_texture(device: &wgpu::Device, width: u32, height: u32) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("depth"),
        size: wgpu::Extent3d {
            width: width.max(1),
            height: height.max(1),
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Depth32Float,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    })
}

#[allow(deprecated)] // `EventLoop::create_window` 与 `EventLoop::run` 沿用引擎 game_runtime / 材质编辑器的写法
fn main() {
    env_logger::init();

    let workdir = match edit_prefs::workdir_from_args() {
        Ok(dir) => dir,
        Err(message) => {
            eprintln!("{message}");
            std::process::exit(2);
        }
    };

    let event_loop = EventLoop::new().expect("创建事件循环失败");
    let window = Arc::new(
        event_loop
            .create_window(
                WindowAttributes::default()
                    .with_title("Geese 骨骼动画预览")
                    .with_inner_size(LogicalSize::new(1280.0, 800.0)),
            )
            .expect("创建窗口失败"),
    );

    let mut state = pollster::block_on(PreviewState::new(window.clone(), workdir))
        .expect("初始化骨骼动画预览失败");

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
