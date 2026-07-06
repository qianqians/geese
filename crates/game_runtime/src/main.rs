//! 独立游戏运行时——桌面入口。
//!
//! 不依赖编辑器或 egui，直接衔接 render / scene / physics 引擎 crates。
//! 启动参数: `geese_game.exe [project_dir] [scene_file]`
//!
//! 核心逻辑位于 `lib.rs`，供桌面和 Android 两个入口共享。

use geese_game::run_event_loop;
use winit::event_loop::EventLoop;
use winit::window::WindowAttributes;

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

    run_event_loop(event_loop, window, &project_dir, &scene_file);
}
