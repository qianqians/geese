//! WASM / Web 平台入口（WebGPU backend）。
//!
//! 使用 `wasm-bindgen` + `web-sys` 获取 canvas，
//! wgpu 请求 WebGPU backend。
//!
//! Feature gate: `#[cfg(target_arch = "wasm32")]`
//!
//! 限制：
//! - `TIMESTAMP_QUERY` 在 WebGPU 上不可用 → 使用 `performance.now()` 替代
//! - tokio 相关代码 gate behind `not(target_arch = "wasm32")`

/// WASM 平台 GameState 初始化。
///
/// 与桌面版本的主要差异：
/// - WebGPU backend（`wgpu::Backends::BROWSER_WEBGPU`）
/// - Canvas 通过 `web-sys` 获取
/// - 纹理格式使用 `Bgra8UnormSrgb`（浏览器约定）
/// - 无 `TIMESTAMP_QUERY` 支持
#[cfg(target_arch = "wasm32")]
mod wasm_impl {
    use std::sync::Arc;

    /// WASM 平台初始化入口。
    pub async fn init_wasm(
        window: winit::window::Window,
        project_dir: &str,
        scene_file: &str,
    ) -> Result<super::super::GameState, Box<dyn std::error::Error>> {
        // 当前为 stub，委托给标准 GameState::new。
        // 完整实现需要:
        // 1. 通过 web-sys 获取 canvas element
        // 2. wgpu::Backends::BROWSER_WEBGPU
        // 3. 适配 SurfaceCapabilities（Bgra8UnormSrgb）
        // 4. 排除 TIMESTAMP_QUERY feature
        super::super::GameState::new(window, project_dir, scene_file).await
    }
}

/// WASM 入口包装（非 WASM 平台编译为空）。
pub async fn init_wasm_game(
    window: winit::window::Window,
    project_dir: &str,
    scene_file: &str,
) -> Result<crate::GameState, Box<dyn std::error::Error>> {
    #[cfg(target_arch = "wasm32")]
    {
        wasm_impl::init_wasm(window, project_dir, scene_file).await
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = (window, project_dir, scene_file);
        panic!("wasm module called on non-WASM platform");
    }
}

/// WASM 入口函数（wasm-bindgen 导出）。
///
/// 编译为 WASM 时由 `wasm-pack build` 自动调用。
#[cfg(target_arch = "wasm32")]
#[wasm_bindgen::prelude::wasm_bindgen(start)]
pub fn wasm_main() {
    std::panic::set_hook(Box::new(console_error_panic_hook::hook));
    console_log::init_with_level(log::Level::Info).expect("Failed to init logger");
    // 实际事件循环由 HTML 页面中的 JavaScript 触发
}
