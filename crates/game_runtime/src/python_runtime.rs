//! 通用 Python 游戏运行时。
//!
//! 将游戏启动逻辑抽取为通用运行时，使任意 Python 游戏模块无需独立
//! Rust cdylib 即可运行。
//!
//! 用法：
//! ```python
//! from geese_game import run_game
//! run_game("my_game_module", "Game", "My Game", 1280, 720)
//! ```
//!
//! Python 游戏模块需要提供指定类（`game_class_name`），该类须有
//! `update(bridge, dt)` 方法。灯光和材质通过 EngineBridge 由 Python
//! 脚本自行配置（`bridge.light_add_directional(...)` 等）。

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
    build_post_uniform, PostChain, PostEffect,
    PostProcessPipeline, RenderQueue, SceneRenderer, WgpuSceneRenderer,
    WgpuSceneRendererDescriptor,
};
use scene::Scene;
use winit::{
    event::{ElementState, Event, KeyEvent, WindowEvent},
    event_loop::ControlFlow,
    keyboard::{KeyCode, PhysicalKey},
    window::WindowAttributes,
};

/// winit KeyCode → input::KeyCode 映射。
fn map_key_code(kc: KeyCode) -> Option<input::KeyCode> {
    Some(match kc {
        KeyCode::KeyA => input::KeyCode::A,
        KeyCode::KeyB => input::KeyCode::B,
        KeyCode::KeyC => input::KeyCode::C,
        KeyCode::KeyD => input::KeyCode::D,
        KeyCode::KeyE => input::KeyCode::E,
        KeyCode::KeyF => input::KeyCode::F,
        KeyCode::KeyG => input::KeyCode::G,
        KeyCode::KeyH => input::KeyCode::H,
        KeyCode::KeyI => input::KeyCode::I,
        KeyCode::KeyJ => input::KeyCode::J,
        KeyCode::KeyK => input::KeyCode::K,
        KeyCode::KeyL => input::KeyCode::L,
        KeyCode::KeyM => input::KeyCode::M,
        KeyCode::KeyN => input::KeyCode::N,
        KeyCode::KeyO => input::KeyCode::O,
        KeyCode::KeyP => input::KeyCode::P,
        KeyCode::KeyQ => input::KeyCode::Q,
        KeyCode::KeyR => input::KeyCode::R,
        KeyCode::KeyS => input::KeyCode::S,
        KeyCode::KeyT => input::KeyCode::T,
        KeyCode::KeyU => input::KeyCode::U,
        KeyCode::KeyV => input::KeyCode::V,
        KeyCode::KeyW => input::KeyCode::W,
        KeyCode::KeyX => input::KeyCode::X,
        KeyCode::KeyY => input::KeyCode::Y,
        KeyCode::KeyZ => input::KeyCode::Z,
        KeyCode::Digit0 => input::KeyCode::Num0,
        KeyCode::Digit1 => input::KeyCode::Num1,
        KeyCode::Digit2 => input::KeyCode::Num2,
        KeyCode::Digit3 => input::KeyCode::Num3,
        KeyCode::Digit4 => input::KeyCode::Num4,
        KeyCode::Digit5 => input::KeyCode::Num5,
        KeyCode::Digit6 => input::KeyCode::Num6,
        KeyCode::Digit7 => input::KeyCode::Num7,
        KeyCode::Digit8 => input::KeyCode::Num8,
        KeyCode::Digit9 => input::KeyCode::Num9,
        KeyCode::F1 => input::KeyCode::F1,
        KeyCode::F2 => input::KeyCode::F2,
        KeyCode::F3 => input::KeyCode::F3,
        KeyCode::F4 => input::KeyCode::F4,
        KeyCode::F5 => input::KeyCode::F5,
        KeyCode::F6 => input::KeyCode::F6,
        KeyCode::F7 => input::KeyCode::F7,
        KeyCode::F8 => input::KeyCode::F8,
        KeyCode::F9 => input::KeyCode::F9,
        KeyCode::F10 => input::KeyCode::F10,
        KeyCode::F11 => input::KeyCode::F11,
        KeyCode::F12 => input::KeyCode::F12,
        KeyCode::Escape => input::KeyCode::Escape,
        KeyCode::Tab => input::KeyCode::Tab,
        KeyCode::Space => input::KeyCode::Space,
        KeyCode::Enter => input::KeyCode::Enter,
        KeyCode::Backspace => input::KeyCode::Backspace,
        KeyCode::ArrowLeft => input::KeyCode::Left,
        KeyCode::ArrowRight => input::KeyCode::Right,
        KeyCode::ArrowUp => input::KeyCode::Up,
        KeyCode::ArrowDown => input::KeyCode::Down,
        KeyCode::ShiftLeft => input::KeyCode::LeftShift,
        KeyCode::ShiftRight => input::KeyCode::RightShift,
        KeyCode::ControlLeft => input::KeyCode::LeftCtrl,
        KeyCode::ControlRight => input::KeyCode::RightCtrl,
        KeyCode::AltLeft => input::KeyCode::LeftAlt,
        KeyCode::AltRight => input::KeyCode::RightAlt,
        _ => return None,
    })
}

/// 通用 Python 游戏入口：启动任意 Python 游戏模块。
///
/// # 参数
/// - `game_module`: Python 模块名（如 `"jump_game"`）
/// - `game_class_name`: 游戏类名（如 `"Game"`），不能为空
/// - `window_title`: 窗口标题
/// - `width`: 窗口宽度
/// - `height`: 窗口高度
///
/// Python 游戏类须提供 `update(bridge, dt)` 方法，通过 EngineBridge
/// 配置场景、物理、材质和灯光。
///
/// ## 材质与灯光
/// - `scene.materials` 从 `py_engine::get_material_library()` 获取（Python 侧
///   通过 `EngineBridge.material_create()` 注册材质后，运行时自动读取）
/// - 灯光每帧从 `py_engine::get_lights()` 读取（Python 侧通过
///   `EngineBridge.light_add_directional()` 等静态方法配置全局灯光注册表）
#[pyfunction]
pub fn run_game(
    game_module: &str,
    game_class_name: &str,
    window_title: &str,
    width: u32,
    height: u32,
    py: Python<'_>,
) -> PyResult<()> {
    // ── 参数校验 ──
    if game_class_name.is_empty() {
        return Err(pyo3::exceptions::PyValueError::new_err(
            "game_class_name must not be empty",
        ));
    }

    env_logger::init();

    // ── 导入 Python 游戏模块并按名称实例化 ──
    let py_game: Py<PyAny> = {
        let gm = py.import(game_module)?;
        let cls = gm.getattr(game_class_name).map_err(|_| {
            pyo3::exceptions::PyImportError::new_err(format!(
                "Class '{game_class_name}' not found in module '{game_module}'"
            ))
        })?;
        cls.call0()?.unbind()
    };

    // ── 窗口 ──
    let event_loop = winit::event_loop::EventLoop::new().unwrap();
    let window = event_loop
        .create_window(
            WindowAttributes::default()
                .with_title(window_title)
                .with_inner_size(winit::dpi::LogicalSize::new(width, height)),
        )
        .unwrap();

    let window = Arc::new(window);
    let size = window.inner_size();

    // ── wgpu ──
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
        backends: wgpu::Backends::PRIMARY,
        ..Default::default()
    });
    let surface = instance.create_surface(window.clone()).unwrap();
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        compatible_surface: Some(&surface),
        force_fallback_adapter: false,
    }))
    .expect("找不到合适的 GPU 适配器");
    let (device, queue) = pollster::block_on(adapter.request_device(
        &wgpu::DeviceDescriptor {
            label: Some("GeeseGame"),
            required_features: wgpu::Features::default(),
            required_limits: wgpu::Limits::default(),
            memory_hints: wgpu::MemoryHints::default(),
        },
        None,
    ))
    .unwrap();

    let surface_caps = surface.get_capabilities(&adapter);
    let surface_format = surface_caps
        .formats
        .iter()
        .copied()
        .find(|f| f.is_srgb())
        .unwrap_or(surface_caps.formats[0]);
    let mut config = wgpu::SurfaceConfiguration {
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

    let create_depth =
        |dev: &wgpu::Device, w: u32, h: u32| -> (wgpu::Texture, wgpu::TextureView) {
            let t = dev.create_texture(&wgpu::TextureDescriptor {
                label: Some("depth"),
                size: wgpu::Extent3d {
                    width: w,
                    height: h,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Depth32Float,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                view_formats: &[],
            });
            let v = t.create_view(&wgpu::TextureViewDescriptor::default());
            (t, v)
        };
    #[allow(unused_variables)]
    let (mut depth_texture, mut depth_view) = create_depth(&device, size.width, size.height);

    let create_hdr = |dev: &wgpu::Device,
                      fmt: wgpu::TextureFormat,
                      w: u32,
                      h: u32|
     -> (wgpu::Texture, wgpu::TextureView) {
        let t = dev.create_texture(&wgpu::TextureDescriptor {
            label: Some("hdr"),
            size: wgpu::Extent3d {
                width: w.max(1),
                height: h.max(1),
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: fmt,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let v = t.create_view(&wgpu::TextureViewDescriptor::default());
        (t, v)
    };
    let (mut _hdr_texture, mut hdr_view) =
        create_hdr(&device, surface_format, size.width, size.height);

    // ── 渲染器 ──
    let engine_config = EngineConfig::default();
    let renderer_desc = match engine_config.render.rendering_path {
        config::ConfigRenderingPath::ForwardPlus => {
            WgpuSceneRendererDescriptor::forward_plus(surface_format, size.width, size.height)
        }
        config::ConfigRenderingPath::DeferredPlus => {
            WgpuSceneRendererDescriptor::deferred_plus(surface_format, size.width, size.height)
        }
    };
    let mut renderer = WgpuSceneRenderer::new(&device, &queue, renderer_desc);
    let scene_renderer = SceneRenderer::new(render::Material::default());
    let mut post_chain = PostChain::new();
    post_chain
        .push(PostEffect::aces(1.0))
        .push(PostEffect::bloom(1.0, 0.15));
    let mut post_pipeline =
        PostProcessPipeline::new(&device, &queue, surface_format, size.width, size.height);

    // ── 场景 / 物理 / 摄像机 ──
    let mut scene = {
        let bounds =
            math::AABB::new(Point3::new(-50., -50., -50.), Point3::new(50., 50., 50.));
        Scene::new(
            vec![],
            vec![],
            py_engine::get_material_library(),
            vec![],
            vec![],
            bounds,
            100,
            8,
        )
    };
    let mut physics = PhysicsWorld::new();
    let physics_scene_id = physics.create_scene(PhyVec3::new(0.0, -9.81, 0.0));
    let aspect = size.width as f32 / size.height.max(1) as f32;
    let mut camera = Camera::new(
        CameraMode::Orbit {
            yaw: 0.0,
            pitch: -0.35,
            focal_point: Point3::new(0.0, 0.8, 0.0),
            distance: 6.0,
            min_distance: 3.0,
            max_distance: 20.0,
        },
        aspect,
    );

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
                    if sz.width == 0 || sz.height == 0 {
                        minimized = true;
                        return;
                    }
                    minimized = false;
                    config.width = sz.width;
                    config.height = sz.height;
                    surface.configure(&device, &config);
                    let (dt, dv) = create_depth(&device, sz.width, sz.height);
                    depth_texture = dt;
                    depth_view = dv;
                    let (ht, hv) = create_hdr(&device, surface_format, sz.width, sz.height);
                    _hdr_texture = ht;
                    hdr_view = hv;
                    post_pipeline.resize(&device, sz.width, sz.height);
                    renderer.resize(&device, &queue, sz.width, sz.height, 0.1, 500.0);
                }
                WindowEvent::KeyboardInput {
                    event:
                        KeyEvent {
                            physical_key: PhysicalKey::Code(kc),
                            state: es,
                            ..
                        },
                    ..
                } => {
                    let pressed = es == ElementState::Pressed;
                    if kc == KeyCode::Escape {
                        window_target.exit();
                        return;
                    }
                    // 将所有按键转发到 InputState
                    if let Some(mapped) = map_key_code(kc) {
                        if pressed {
                            input_state.apply(&InputEvent::KeyPressed(mapped));
                        } else {
                            input_state.apply(&InputEvent::KeyReleased(mapped));
                        }
                    }
                }
                _ => {}
            },
            Event::DeviceEvent { .. } => {}
            Event::AboutToWait => {
                let now = Instant::now();
                let dt = (now - last_frame).as_secs_f32().min(0.1);
                last_frame = now;

                let physics_scene = physics.scene_mut(physics_scene_id).unwrap();

                // 创建帧桥接对象，调用 Python game.update(bridge, dt)
                Python::with_gil(|py| {
                    let bridge = py_engine::EngineBridge::new(
                        &mut scene as *mut Scene,
                        physics_scene as *mut physics::scene::PhysicsScene,
                        &input_state as *const InputState,
                        &mut camera as *mut Camera,
                    );
                    let bridge_py = Py::new(py, bridge).unwrap();
                    let _ = py_game
                        .bind(py)
                        .call_method1("update", (&bridge_py, dt));
                });

                scene.update_world_transforms();

                if !minimized {
                    // 从全局灯光注册表读取（Python 脚本通过 EngineBridge 静态方法配置）
                    let (lights, ambient) = py_engine::get_lights();

                    let frustum = camera.frustum();
                    let render_q: RenderQueue<'_> =
                        scene.render_queue(&scene_renderer, Some(&frustum));
                    renderer.update_camera(
                        &queue,
                        camera.view_projection_raw(),
                        camera.camera_position_raw(),
                    );
                    renderer.update_lights(&queue, ambient, &lights);
                    renderer.prepare(&device, &queue, &scene.materials, &render_q);
                    match surface.get_current_texture() {
                        Ok(output) => {
                            let sv = output
                                .texture
                                .create_view(&wgpu::TextureViewDescriptor::default());
                            let mut enc = device.create_command_encoder(
                                &wgpu::CommandEncoderDescriptor { label: Some("game") },
                            );
                            renderer.render(
                                &device,
                                &mut enc,
                                &hdr_view,
                                Some(&depth_view),
                            );
                            let pu = build_post_uniform(&post_chain, frame_index);
                            post_pipeline.process(
                                &device, &queue, &mut enc, &hdr_view, &sv, &pu,
                            );
                            queue.submit(std::iter::once(enc.finish()));
                            output.present();
                        }
                        Err(
                            wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated,
                        ) => {
                            let s = window.inner_size();
                            if s.width > 0 && s.height > 0 {
                                config.width = s.width;
                                config.height = s.height;
                                surface.configure(&device, &config);
                            }
                        }
                        Err(wgpu::SurfaceError::OutOfMemory) => window_target.exit(),
                        Err(wgpu::SurfaceError::Timeout) => {}
                    }
                }
                // 帧末清空瞬时输入状态
                input_state.begin_frame();
                frame_index = frame_index.wrapping_add(1);
            }
            _ => {}
        }
    });
    Ok(())
}

// ── Python 模块注册 ──

#[pymodule]
fn geese_game(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(run_game, m)?)?;
    Ok(())
}
