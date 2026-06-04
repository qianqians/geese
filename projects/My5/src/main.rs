//! My5 - 由 Geese Launcher 自动生成。
//!
//! 模板类型：空项目
//! 摄像机：Free


use std::time::Instant;
use winit::{event_loop::EventLoop, window::WindowAttributes};

fn main() {
    env_logger::init();

    let event_loop = EventLoop::new().unwrap();
    let window = winit::window::WindowBuilder::new()
        .with_title("My5")
        .with_inner_size(winit::dpi::LogicalSize::new(1280, 720))
        .build(&event_loop)
        .unwrap();

    // TODO: 初始化 wgpu 设备、渲染器、场景、物理世界
    // TODO: 主循环：输入轮询 → 更新 → 渲染

    println!("🚀 My5 已启动！模板：空项目");
}
