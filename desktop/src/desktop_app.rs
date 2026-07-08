//! DesktopApp —— 管理 Launcher 与 Editor 窗口的生命周期。
//!
//! Launcher 作为主窗口，Editor 通过子进程启动（独立原生窗口）。
//! Editor 子进程退出后自动恢复 Launcher。

use std::process::{Child, Command};

use launcher::Launcher;

/// 配置 eframe 中文字体。
pub fn setup_chinese_fonts(cc: &eframe::CreationContext<'_>) {
    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert(
        "chinese".to_owned(),
        egui::FontData::from_static(include_bytes!("../fonts/SourceHanSansSC-Regular.otf")),
    );
    fonts
        .families
        .entry(egui::FontFamily::Proportional)
        .or_default()
        .insert(0, "chinese".to_owned());
    cc.egui_ctx.set_fonts(fonts);
}

/// 一个运行中的 Editor 子进程。
struct EditorProcess {
    child: Child,
    project_path: String,
}

impl EditorProcess {
    /// 检查子进程是否仍在运行。
    fn is_running(&mut self) -> bool {
        match self.child.try_wait() {
            Ok(Some(_status)) => false, // 进程已退出
            Ok(None) => true,           // 仍在运行
            Err(e) => {
                eprintln!("[DesktopApp] Error checking editor process: {e}");
                false
            }
        }
    }
}

/// Launcher 主应用。
pub struct DesktopApp {
    launcher: Launcher,
    editors: Vec<EditorProcess>,
    /// 项目根目录（用于定位 helper 脚本）
    project_root: String,
}

impl DesktopApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        setup_chinese_fonts(cc);

        // 获取项目根目录（当前工作目录）
        let project_root = std::env::current_dir()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|_| ".".to_string());

        eprintln!("[DesktopApp] project_root: {}", project_root);

        Self {
            launcher: Launcher::new(),
            editors: Vec::new(),
            project_root,
        }
    }

    /// 启动一个 Editor 子进程。
    fn spawn_editor(&mut self, project_path: String) {
        let script_path = format!("{}/open_editor_subprocess.py", self.project_root);

        eprintln!("[DesktopApp] Spawning editor for: {}", project_path);
        eprintln!("[DesktopApp] Script: {}", script_path);

        // 使用 python 运行 helper 脚本
        match Command::new("python")
            .arg(&script_path)
            .arg(&project_path)
            .current_dir(&self.project_root)
            .spawn()
        {
            Ok(child) => {
                eprintln!("[DesktopApp] Editor process spawned (PID: {})", child.id());
                self.editors.push(EditorProcess {
                    child,
                    project_path,
                });
            }
            Err(e) => {
                eprintln!("[DesktopApp] Failed to spawn editor: {e}");
                self.launcher.set_status(format!("无法启动 Editor: {e}"), true);
            }
        }
    }
}

impl eframe::App for DesktopApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // 关闭拦截：在有 Editor 运行时不允许关闭 Launcher
        if ctx.input(|i| i.viewport().close_requested()) {
            // 先清理已退出的 editor 进程
            self.editors.retain_mut(|e| e.is_running());

            let count = self.editors.len();
            if count > 0 {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                self.launcher.set_status(
                    format!("请先关闭 {} 个 Editor 窗口", count),
                    true,
                );
                return;
            }
        }

        // 渲染 Launcher
        self.launcher.show(ctx);

        // 检查打开项目请求
        if let Some(project_path) = self.launcher.take_open_request() {
            eprintln!("[DesktopApp] Open request for: {}", project_path);
            self.spawn_editor(project_path);
        }

        // 检查 editor 子进程状态
        let mut any_exited = false;
        for ep in &mut self.editors {
            if !ep.is_running() {
                eprintln!("[DesktopApp] Editor process exited for: {}", ep.project_path);
                any_exited = true;
            }
        }

        // 清理已退出的进程
        if any_exited {
            self.editors.retain_mut(|e| e.is_running());
            eprintln!("[DesktopApp] {} editor(s) still running", self.editors.len());
        }
    }
}

// ---------------------------------------------------------------------------
// EditorApp —— 独立 Editor 窗口（由子进程使用）
// ---------------------------------------------------------------------------

use editor::Editor;

pub struct EditorApp {
    editor: Editor,
}

impl EditorApp {
    pub fn new(project_path: String, cc: &eframe::CreationContext<'_>) -> Self {
        Self { editor: Editor::open(project_path, cc.wgpu_render_state.clone()) }
    }
}

impl eframe::App for EditorApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.editor.update(ctx);
    }
}
