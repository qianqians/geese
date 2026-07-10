//! iOS 平台入口（Metal backend）。
//!
//! 使用 `winit::platform::ios` 创建窗口，wgpu 请求 Metal backend。
//!
//! Feature gate: `#[cfg(target_os = "ios")]`

#[cfg(target_os = "ios")]
mod ios_impl {
    use std::sync::Arc;
    use winit::platform::ios::WindowExtIOS;

    /// iOS 平台 GameState 初始化。
    ///
    /// 与桌面版本的主要差异：
    /// - 强制使用 Metal backend（`wgpu::Backends::METAL`）
    /// - 窗口通过 `UIView` 获取 native handle
    /// - `TIMESTAMP_QUERY` 在 Metal 上可用（与 WASM 不同）
    pub async fn init_ios(
        window: winit::window::Window,
        project_dir: &str,
        scene_file: &str,
    ) -> Result<super::GameState, Box<dyn std::error::Error>> {
        let _ = window.ui_view(); // iOS native view handle
        // 当前为 stub，委托给标准 GameState::new
        super::GameState::new(window, project_dir, scene_file).await
    }
}

/// iOS 入口包装（非 iOS 平台编译为空）。
pub async fn init_ios_game(
    window: winit::window::Window,
    project_dir: &str,
    scene_file: &str,
) -> Result<crate::GameState, Box<dyn std::error::Error>> {
    #[cfg(target_os = "ios")]
    {
        ios_impl::init_ios(window, project_dir, scene_file).await
    }
    #[cfg(not(target_os = "ios"))]
    {
        let _ = (window, project_dir, scene_file);
        panic!("ios module called on non-iOS platform");
    }
}
