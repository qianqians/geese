//! 场景编辑器入口：winit + wgpu + egui 集成。
//!
//! 用法：`scene --workdir <工作目录>`
//!
//! 工作目录是必填的：记录写在 `<workdir>/.data/scene.json`，模型从 `<workdir>/assets/models`
//! 扫描，场景默认在 `<workdir>/assets/scenes`。窗口背景每帧渲染当前场景（`scene::Scene` →
//! Forward+），三栏 UI 由 egui 叠在上面。

mod app;
mod doc;
mod models;
mod pick;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use app::SceneApp;
use material::MaterialLibrary;
use render::{SceneRenderer, WgpuSceneRenderer, WgpuSceneRendererDescriptor};
use scene::SceneObject;
use winit::{
    dpi::LogicalSize,
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    window::{Window, WindowAttributes},
};

struct EditorState {
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
    app: SceneApp,
    /// 没有场景时也照常走一遍 `prepare` + `render`：空队列 + 空材质库。
    empty_renderer: SceneRenderer,
    empty_materials: MaterialLibrary,
}

impl EditorState {
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
                    label: Some("Scene Editor Device"),
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

        let renderer = WgpuSceneRenderer::new(
            &device,
            &queue,
            WgpuSceneRendererDescriptor::forward_plus(surface_format, size.width, size.height),
        );

        // ── egui 集成 ──
        let egui_ctx = egui::Context::default();
        install_cjk_font(&egui_ctx);
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

        // ── 编辑器状态：读工作目录里的记录 → 否则弹窗 ──
        let mut app = SceneApp::new(workdir);
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
            .resize(&self.device, &self.queue, width, height, 0.1, 2000.0);
        self.app.update_aspect(width, height);
    }

    fn render(&mut self) -> Result<(), wgpu::SurfaceError> {
        // ── 1. egui 输入 + UI（按钮、文本编辑都在这里发生）──
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

        // ── 2. 该重建就重建：先丢旧 GPU 缓存，否则新网格可能命中旧的指针键 ──
        if self.app.rebuild_due(Instant::now()) {
            self.renderer.clear_resource_cache();
            self.app.rebuild();
        }

        // ── 3. 相机输入（指针不在 UI 上、也没有在拖模型时才响应）──
        self.handle_preview_input();

        // ── 4. 渲染场景 ──
        let output = self.surface.get_current_texture()?;
        let surface_view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("scene_editor_encoder"),
            });

        self.renderer.update_camera(
            &self.queue,
            self.app.view_projection(),
            self.app.camera_position(),
        );
        let lights = self.app.lights.clone();
        self.renderer
            .update_lights(&self.queue, self.app.ambient, &lights);

        match self.app.built() {
            Some(built) => {
                let render_queue = built.scene.render_queue(&built.scene_renderer, None);
                self.renderer.prepare(
                    &self.device,
                    &self.queue,
                    &built.scene.materials,
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

        // egui-wgpu 0.29 的 `render` 只读 `self.*_buffer.slices`，顶点/索引缓冲必须在这之前
        // 由 `update_buffers` 上传——漏了它就 `unwrap` 空切片直接 panic。
        let user_command_buffers = self.egui_renderer.update_buffers(
            &self.device,
            &self.queue,
            &mut encoder,
            &clipped_primitives,
            &screen_descriptor,
        );

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
        self.queue.submit(
            user_command_buffers
                .into_iter()
                .chain(std::iter::once(encoder.finish())),
        );
        output.present();

        Ok(())
    }

    fn handle_preview_input(&mut self) {
        let (delta, primary, middle, scroll) = self.egui_ctx.input(|i| {
            (
                i.pointer.delta(),
                i.pointer.primary_down(),
                i.pointer.middle_down(),
                i.raw_scroll_delta.y,
            )
        });
        if self.egui_ctx.wants_pointer_input() {
            return;
        }
        // 正在从左栏拖模型时左键属于「拖放」，不转相机。
        let dragging = egui::DragAndDrop::has_any_payload(&self.egui_ctx);

        if primary && !dragging {
            self.app.orbit(delta.x, delta.y);
        }
        if middle {
            self.app.pan(delta.x, delta.y);
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

/// 候选系统中文字体：egui 内置字体不含 CJK，不装就全是空框。
const CJK_FONT_CANDIDATES: &[&str] = &[
    "C:/Windows/Fonts/msyh.ttc",
    "C:/Windows/Fonts/simhei.ttf",
    "C:/Windows/Fonts/simsun.ttc",
    "/System/Library/Fonts/PingFang.ttc",
    "/System/Library/Fonts/STHeiti Medium.ttc",
    "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
    "/usr/share/fonts/truetype/noto/NotoSansCJK-Regular.ttc",
    "/usr/share/fonts/truetype/wqy/wqy-microhei.ttc",
];

/// 装一款系统中文字体作为兜底字体（拉丁字符仍用 egui 自带字体）。
fn install_cjk_font(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();

    let Some((path, bytes)) = CJK_FONT_CANDIDATES.iter().find_map(|path| {
        std::fs::read(path).ok().map(|bytes| (*path, bytes))
    }) else {
        log::warn!("没找到可用的系统中文字体，界面中文会显示为空框");
        return;
    };

    fonts.font_data.insert(
        "cjk".to_string(),
        egui::FontData::from_owned(bytes),
    );
    for family in [
        egui::FontFamily::Proportional,
        egui::FontFamily::Monospace,
    ] {
        fonts
            .families
            .entry(family)
            .or_default()
            .push("cjk".to_string());
    }
    ctx.set_fonts(fonts);
    log::info!("界面中文字体：{path}");
}

#[allow(deprecated)] // `EventLoop::create_window` 与 `EventLoop::run` 沿用其它编辑工具的写法
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
                    .with_title("Geese 场景编辑器 scene")
                    .with_inner_size(LogicalSize::new(1440.0, 860.0)),
            )
            .expect("创建窗口失败"),
    );

    let mut state = pollster::block_on(EditorState::new(window.clone(), workdir))
        .expect("初始化场景编辑器失败");

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
