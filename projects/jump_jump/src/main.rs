//! jump_jump - 跳一跳。
//!
//! 通过 game_runtime 的 Render Graph 管线运行 Python 游戏逻辑。
//! 渲染路径：Forward+ (render-graph) → cluster culling → shadow → forward draw → post-process。
//!
//! 独立运行：`cargo run`（从项目根目录）
//! 或通过编辑器 Play 按钮启动。

fn main() {
    env_logger::init();

    // 项目根目录（Cargo 运行时工作目录即为项目根）
    let project_path = std::env::current_dir()
        .expect("failed to get current dir")
        .to_string_lossy()
        .to_string();

    log::info!("🚀 jump_jump 启动 | Render Graph 管线 | 项目路径: {project_path}");

    geese_game::launch_python_game(
        &project_path,
        "jump_game",
        "JumpGame",
        "跳一跳",
        1280,
        720,
    );
}
