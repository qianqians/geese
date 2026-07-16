//! 跳一跳 (Jump Jump) - Python 脚本化版本。
//!
//! Rust 侧负责：窗口 / wgpu 渲染 / 物理步进 / 后处理。
//! Python 侧负责：游戏逻辑（蓄力跳跃、平台生成、着陆检测、计分）。
//!
//! 用法：
//! ```python
//! from jump_jump import run
//! run("jump_game")
//! ```

mod scene_builder;

use std::sync::Arc;
use std::time::Instant;

use camera::{Camera, CameraMode};
use cgmath::Point3;
use config::EngineConfig;
use input::{InputEvent, InputState};
use physics::PhysicsWorld;
use physics::math::Vec3 as PhyVec3;
use pyo3::prelude::*;
use render::{
    build_post_uniform, Light, MaterialLibrary, PostChain, PostEffect,
    PostProcessPipeline, RenderQueue, SceneRenderer, WgpuSceneRenderer,
    WgpuSceneRendererDescriptor,
};
use scene::Scene;
use scene_builder::create_game_materials;
use winit::{
    event::{DeviceEvent, ElementState, Event, KeyEvent, WindowEvent},
    event_loop::ControlFlow,
    keyboard::{KeyCode, PhysicalKey},
    window::WindowAttributes,
};

/// Python 入口：启动跳一跳游戏。
#[pyfunction]
fn run(game_module: &str, py: Python<'_>) -> PyResult<()> {
    env_logger::init();

    let py_game: Py<PyAny> = {
        let gm = py.import(game_module)?;
        let cls = gm.getattr("JumpGame")?;
        cls.call0()?.unbind()
    };

    let event_loop = winit::event_loop::EventLoop::new().unwrap();
    let window = event_loop
        .create_window(
            WindowAttributes::default()
                .with_title("跳一跳 (Jump Jump) — Python")
                .with_inner_size(winit::dpi::LogicalSize::new(1280, 720)),
        )
        .unwrap();

    let window = Arc::new(window);
    let size = window.inner_size();

    // ── wgpu ──
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
        backends: wgpu::Backends::PRIMARY, ..Default::default()
    });
    let surface = instance.create_surface(window.clone()).unwrap();
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        compatible_surface: Some(&surface), force_fallback_adapter: false,
    })).expect("找不到合适的 GPU 适配器");
    let (device, queue) = pollster::block_on(adapter.request_device(
        &wgpu::DeviceDescriptor {
            label: Some("JumpJump"), required_features: wgpu::Features::default(),
            required_limits: wgpu::Limits::default(), memory_hints: wgpu::MemoryHints::default(),
        }, None,
    )).unwrap();

    let surface_caps = surface.get_capabilities(&adapter);
    let surface_format = surface_caps.formats.iter().copied()
        .find(|f| f.is_srgb()).unwrap_or(surface_caps.formats[0]);
    let mut config = wgpu::SurfaceConfiguration {
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT, format: surface_format,
        width: size.width, height: size.height, present_mode: wgpu::PresentMode::Fifo,
        alpha_mode: surface_caps.alpha_modes[0], view_formats: vec![],
        desired_maximum_frame_latency: 2,
    };
    surface.configure(&device, &config);

    let create_depth = |dev: &wgpu::Device, w: u32, h: u32| -> (wgpu::Texture, wgpu::TextureView) {
        let t = dev.create_texture(&wgpu::TextureDescriptor {
            label: Some("depth"), size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
            mip_level_count: 1, sample_count: 1, dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth32Float, usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let v = t.create_view(&wgpu::TextureViewDescriptor::default());
        (t, v)
    };
    #[allow(unused_variables)]
    let (mut depth_texture, mut depth_view) = create_depth(&device, size.width, size.height);

    let create_hdr = |dev: &wgpu::Device, fmt: wgpu::TextureFormat, w: u32, h: u32| -> (wgpu::Texture, wgpu::TextureView) {
        let t = dev.create_texture(&wgpu::TextureDescriptor {
            label: Some("hdr"), size: wgpu::Extent3d { width: w.max(1), height: h.max(1), depth_or_array_layers: 1 },
            mip_level_count: 1, sample_count: 1, dimension: wgpu::TextureDimension::D2, format: fmt,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING, view_formats: &[],
        });
        let v = t.create_view(&wgpu::TextureViewDescriptor::default());
        (t, v)
    };
    let (mut _hdr_texture, mut hdr_view) = create_hdr(&device, surface_format, size.width, size.height);

    // ── 渲染器 ──
    let engine_config = EngineConfig::default();
    let renderer_desc = match engine_config.render.rendering_path {
        config::ConfigRenderingPath::ForwardPlus => WgpuSceneRendererDescriptor::forward_plus(surface_format, size.width, size.height),
        config::ConfigRenderingPath::DeferredPlus => WgpuSceneRendererDescriptor::deferred_plus(surface_format, size.width, size.height),
    };
    let mut renderer = WgpuSceneRenderer::new(&device, &queue, renderer_desc);
    let scene_renderer = SceneRenderer::new(render::Material::default());
    let mut post_chain = PostChain::new();
    post_chain.push(PostEffect::aces(1.0)).push(PostEffect::bloom(1.0, 0.15));
    let mut post_pipeline = PostProcessPipeline::new(&device, &queue, surface_format, size.width, size.height);

    // ── 场景 / 物理 / 摄像机 ──
    let mut scene = {
        let bounds = math::AABB::new(Point3::new(-50., -50., -50.), Point3::new(50., 50., 50.));
        Scene::new(vec![], vec![], MaterialLibrary::default(), vec![], vec![], bounds, 100, 8)
    };
    scene.materials = create_game_materials();
    let mut physics = PhysicsWorld::new();
    let physics_scene_id = physics.create_scene(PhyVec3::new(0.0, -9.81, 0.0));
    let aspect = size.width as f32 / size.height.max(1) as f32;
    let mut camera = Camera::new(CameraMode::Orbit {
        yaw: 0.0, pitch: -0.35, focal_point: Point3::new(0.0, 0.8, 0.0),
        distance: 6.0, min_distance: 3.0, max_distance: 20.0,
    }, aspect);

    let mut last_frame = Instant::now();
    let mut frame_index: u64 = 0;
    let mut input_state = InputState::new();
    let mut minimized = false;

    #[allow(deprecated)]
    let _ = event_loop.run(move |event, window_target| {
        window_target.set_control_flow(ControlFlow::Poll);
        match event {
            Event::WindowEvent { event, .. } => match event {
                WindowEvent::CloseRequested => window_target.exit(),
                WindowEvent::Resized(sz) => {
                    if sz.width == 0 || sz.height == 0 { minimized = true; return; }
                    minimized = false;
                    config.width = sz.width; config.height = sz.height;
                    surface.configure(&device, &config);
                    let (dt, dv) = create_depth(&device, sz.width, sz.height);
                    depth_texture = dt; depth_view = dv;
                    let (ht, hv) = create_hdr(&device, surface_format, sz.width, sz.height);
                    _hdr_texture = ht; hdr_view = hv;
                    post_pipeline.resize(&device, sz.width, sz.height);
                    renderer.resize(&device, &queue, sz.width, sz.height, 0.1, 500.0);
                }
                WindowEvent::KeyboardInput {
                    event: KeyEvent { physical_key: PhysicalKey::Code(kc), state: es, .. }, ..
                } => {
                    let pressed = es == ElementState::Pressed;
                    match kc {
                        KeyCode::Escape => window_target.exit(),
                        KeyCode::Space => {
                            if pressed { input_state.apply(&InputEvent::KeyPressed(input::KeyCode::Space)); }
                            else { input_state.apply(&InputEvent::KeyReleased(input::KeyCode::Space)); }
                        }
                        KeyCode::KeyR => {
                            if pressed { input_state.apply(&InputEvent::KeyPressed(input::KeyCode::R)); }
                            else { input_state.apply(&InputEvent::KeyReleased(input::KeyCode::R)); }
                        }
                        _ => {}
                    }
                }
                _ => {}
            },
            Event::DeviceEvent { .. } => {}
            Event::AboutToWait => {
                let now = Instant::now();
                let dt = (now - last_frame).as_secs_f32().min(0.1);
                last_frame = now;
                input_state.begin_frame();

                let physics_scene = physics.scene_mut(physics_scene_id).unwrap();

                let (charge_frac, game_over, player_pos) = Python::with_gil(|py| {
                    let bridge = py_engine::EngineBridge::new(
                        &mut scene as *mut Scene,
                        physics_scene as *mut physics::scene::PhysicsScene,
                        &input_state as *const InputState,
                        &mut camera as *mut Camera,
                    );
                    let bridge_py = Py::new(py, bridge).unwrap();
                    let r = py_game.bind(py).call_method1("update", (&bridge_py, dt))
                        .expect("game.update() failed");
                    let c: f32 = r.get_item(0).unwrap().extract().unwrap();
                    let o: bool = r.get_item(1).unwrap().extract().unwrap();
                    let px: f32 = r.get_item(2).unwrap().extract().unwrap();
                    let pyv: f32 = r.get_item(3).unwrap().extract().unwrap();
                    let pz: f32 = r.get_item(4).unwrap().extract().unwrap();
                    (c, o, (px, pyv, pz))
                });

                scene.update_world_transforms();
                camera.smooth_follow_target(Point3::new(player_pos.0, player_pos.1, player_pos.2), 5.0, dt);

                if !minimized {
                    scene.materials.materials[1].emissive_factor = [charge_frac * 0.6, charge_frac * 0.2, 0.0];
                    let frustum = camera.frustum();
                    let render_q: RenderQueue<'_> = scene.render_queue(&scene_renderer, Some(&frustum));
                    renderer.update_camera(&queue, camera.view_projection_raw(), camera.camera_position_raw());
                    let (ambient, lights) = if game_over {
                        ([0.18, 0.06, 0.06], vec![Light::directional([-0.3, -1.0, -0.5], [0.7, 0.3, 0.3], 0.9)])
                    } else {
                        ([0.12, 0.12, 0.15], vec![Light::directional([-0.3, -1.0, -0.5], [1.0, 0.95, 0.85], 1.2)])
                    };
                    renderer.update_lights(&queue, ambient, &lights);
                    renderer.prepare(&device, &queue, &scene.materials, &render_q);
                    match surface.get_current_texture() {
                        Ok(output) => {
                            let sv = output.texture.create_view(&wgpu::TextureViewDescriptor::default());
                            let mut enc = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("jump") });
                            renderer.render(&device, &mut enc, &hdr_view, Some(&depth_view));
                            let pu = build_post_uniform(&post_chain, frame_index);
                            post_pipeline.process(&device, &queue, &mut enc, &hdr_view, &sv, &pu);
                            queue.submit(std::iter::once(enc.finish()));
                            output.present();
                        }
                        Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                            let s = window.inner_size();
                            if s.width > 0 && s.height > 0 { config.width = s.width; config.height = s.height; surface.configure(&device, &config); }
                        }
                        Err(wgpu::SurfaceError::OutOfMemory) => window_target.exit(),
                        Err(wgpu::SurfaceError::Timeout) => {}
                    }
                }
                frame_index = frame_index.wrapping_add(1);
            }
            _ => {}
        }
    });
    Ok(())
}

#[pymodule]
fn jump_jump(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(run, m)?)?;
    Ok(())
}
