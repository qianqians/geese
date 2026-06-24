//! pydesktop —— Geese 桌面工具的 Python 绑定。
//!
//! 通过 pyo3 暴露 native 窗口入口：
//!
//! ```python
//! from pydesktop import run, open_editor
//!
//! # 启动 Launcher（阻塞，管理项目选择和 editor 子进程）
//! run()
//!
//! # 或直接打开 Editor（被 Launcher 作为子进程调用）
//! open_editor("/path/to/project")
//! ```

use pyo3::prelude::*;

mod desktop_app;

/// 启动 Geese Launcher（主窗口）。
///
/// 阻塞直到用户关闭 Launcher 窗口。
/// Launcher 通过子进程启动 Editor 窗口。
#[pyfunction]
fn run() -> PyResult<()> {
    let options = eframe::NativeOptions {
        renderer: eframe::Renderer::Wgpu,
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([900.0, 800.0])
            .with_resizable(false),
        ..Default::default()
    };

    eframe::run_native(
        "Geese Launcher",
        options,
        Box::new(|cc| Ok(Box::new(desktop_app::DesktopApp::new(cc)))),
    )
    .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))
}

/// 启动 Editor 独立窗口。由 Launcher 通过子进程调用。
///
/// 阻塞直到用户关闭 Editor 窗口。
#[pyfunction]
fn open_editor(project_path: String) -> PyResult<()> {
    let options = eframe::NativeOptions {
        renderer: eframe::Renderer::Wgpu,
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 720.0])
            .with_title(format!("Geese Editor - {}", project_path)),
        ..Default::default()
    };

    eframe::run_native(
        "Geese Editor",
        options,
        Box::new(|cc| {
            desktop_app::setup_chinese_fonts(cc);
            Ok(Box::new(desktop_app::EditorApp::new(project_path.clone(), cc)))
        }),
    )
    .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))
}

/// 把入口函数注册到 Python 模块。
#[pymodule]
pub fn pydesktop(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(run, m)?)?;
    m.add_function(wrap_pyfunction!(open_editor, m)?)?;
    Ok(())
}
