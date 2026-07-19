//! jump_jump - 跳一跳 Python 游戏项目。
//!
//! 模板类型：Python 游戏
//! 摄像机：Free
//!
//! 此项目通过编辑器 Play 按钮启动 Python 游戏子进程运行。
//! 独立运行：`python run_game.py <project_path> jump_game --class JumpGame --title "跳一跳"`

use std::time::Instant;
use winit::{event_loop::EventLoop, window::WindowAttributes};

fn main() {
    env_logger::init();

    let event_loop = EventLoop::new().unwrap();
    let window = winit::window::WindowBuilder::new()
        .with_title("跳一跳")
        .with_inner_size(winit::dpi::LogicalSize::new(1280, 720))
        .build(&event_loop)
        .unwrap();

    // TODO: 初始化 wgpu 设备、渲染器、场景、物理世界
    // TODO: 主循环：输入轮询 → 更新 → 渲染

    println!("🚀 jump_jump 已启动！模板：Python 游戏");
    println!("提示：请通过编辑器的 Play 按钮启动游戏，或使用 run_game.py 脚本。");
}
