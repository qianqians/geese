//! 纯 Rust 桌面入口（无需 Python）。
//!
//! Feature gate: `native-desktop`。
//!
//! ## 用法
//! - 无参数 → 启动 Launcher（项目管理窗口）
//! - `--editor <project_path>` → 直接打开 Editor

mod desktop_app;

use std::process::{Child, Command};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();

    let args: Vec<String> = std::env::args().collect();
    let mode = parse_mode(&args);

    match mode {
        AppMode::Launcher => run_launcher(),
        AppMode::Editor(path) => run_editor(&path),
    }
}

enum AppMode {
    Launcher,
    Editor(String),
}

fn parse_mode(args: &[String]) -> AppMode {
    if args.len() >= 3 && args[1] == "--editor" {
        AppMode::Editor(args[2].clone())
    } else if args.len() >= 2 && !args[1].starts_with('-') {
        // 向后兼容: 直接传路径 = Editor 模式
        AppMode::Editor(args[1].clone())
    } else {
        AppMode::Launcher
    }
}

// ── Launcher 模式 ──────────────────────────────────────────────────────────

fn run_launcher() -> Result<(), Box<dyn std::error::Error>> {
    eprintln!("[native-desktop] Starting Launcher");

    let native_options = eframe::NativeOptions {
        renderer: eframe::Renderer::Wgpu,
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1080.0, 640.0])
            .with_resizable(false)
            .with_title("Geese Launcher (Native)"),
        ..Default::default()
    };

    eframe::run_native(
        "Geese Launcher (Native)",
        native_options,
        Box::new(|cc| {
            desktop_app::setup_chinese_fonts(cc);
            Ok(Box::new(NativeLauncherApp::new(cc)))
        }),
    )?;

    Ok(())
}

/// Launcher 的纯 Rust 管理结构（负责启动/监控 Editor 子进程）
struct NativeLauncherApp {
    launcher: launcher::Launcher,
    editor_process: Option<Child>,
    /// 当前运行的 native-desktop 二进制路径
    exe_path: String,
}

impl NativeLauncherApp {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        desktop_app::setup_chinese_fonts(cc);
        let exe_path = std::env::current_exe()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|_| "native-desktop".to_string());
        Self {
            launcher: launcher::Launcher::new(),
            editor_process: None,
            exe_path,
        }
    }

    fn spawn_editor(&mut self, project_path: &str) {
        if self.editor_process.is_some() {
            eprintln!("[NativeLauncher] Editor already running");
            return;
        }
        eprintln!("[NativeLauncher] Spawning editor for: {}", project_path);
        match Command::new(&self.exe_path)
            .arg("--editor")
            .arg(project_path)
            .spawn()
        {
            Ok(child) => {
                eprintln!("[NativeLauncher] Editor spawned (PID: {})", child.id());
                self.editor_process = Some(child);
                self.launcher.set_status(
                    format!("Editor 运行中: {}", project_path),
                    false,
                );
            }
            Err(e) => {
                eprintln!("[NativeLauncher] Failed to spawn editor: {e}");
                self.launcher.set_status(format!("启动失败: {e}"), true);
            }
        }
    }
}

impl eframe::App for NativeLauncherApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // 检查 editor 子进程是否已退出
        if let Some(ref mut child) = self.editor_process {
            if let Ok(Some(status)) = child.try_wait() {
                eprintln!("[NativeLauncher] Editor exited: {:?}", status);
                self.editor_process = None;
                self.launcher.set_status("编辑完成".to_string(), false);
            }
        }

        // 关闭拦截
        if self.editor_process.is_some() && ctx.input(|i| i.viewport().close_requested()) {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.launcher.set_status("请先关闭 Editor 窗口".to_string(), true);
            return;
        }

        self.launcher.show(ctx);

        // 处理打开项目请求
        if let Some(project_path) = self.launcher.take_open_request() {
            self.spawn_editor(&project_path);
        }

        ctx.request_repaint();
    }
}

// ── Editor 模式 ────────────────────────────────────────────────────────────

fn run_editor(project_path: &str) -> Result<(), Box<dyn std::error::Error>> {
    eprintln!("[native-desktop] Starting editor for: {}", project_path);

    let project_path = project_path.to_string();
    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1600.0, 900.0])
            .with_title(format!("Geese Editor (Native) - {}", project_path)),
        ..Default::default()
    };

    eframe::run_native(
        "Geese Editor (Native)",
        native_options,
        Box::new(move |cc| {
            let render_state = cc
                .wgpu_render_state
                .clone()
                .expect("wgpu render state required");
            let editor = editor::Editor::open(project_path.clone(), Some(render_state));
            Ok(Box::new(NativeEditorApp { editor }))
        }),
    )?;

    Ok(())
}

struct NativeEditorApp {
    editor: editor::Editor,
}

impl eframe::App for NativeEditorApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.editor.update(ctx);
        if self.editor.close_requested {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        ctx.request_repaint();
    }
}
