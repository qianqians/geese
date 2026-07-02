//! DesktopApp —— 管理 Launcher 与多 Editor 窗口的生命周期。
//!
//! 使用 egui 多视口在同一事件循环中管理所有窗口：
//! - Launcher 主窗口：选择/创建项目
//! - Editor 独立窗口：每个项目一个独立原生窗口
//! - Editor 关闭自动恢复 Launcher，所有 Editor 关闭后才能关闭 Launcher

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use editor::Editor;
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

/// 一个活动的 Editor 视口。
struct EditorViewport {
    viewport_id: egui::ViewportId,
    editor: Editor,
    close_requested: Arc<AtomicBool>,
}

/// Launcher 主应用。
pub struct DesktopApp {
    launcher: Launcher,
    editors: Vec<EditorViewport>,
    launcher_visible: bool,
    next_id: u64,
    render_state: Option<egui_wgpu::RenderState>,
}

impl DesktopApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        setup_chinese_fonts(cc);
        Self {
            launcher: Launcher::new(),
            editors: Vec::new(),
            launcher_visible: true,
            next_id: 0,
            render_state: cc.wgpu_render_state.clone(),
        }
    }
}

impl eframe::App for DesktopApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // 清理已关闭的 editor
        let before = self.editors.len();
        self.editors.retain(|e| !e.close_requested.load(Ordering::SeqCst));
        if self.editors.len() < before && !self.launcher_visible {
            self.launcher_visible = true;
            self.launcher.reset_to_home();
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
            ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
        }

        // 关闭拦截
        if ctx.input(|i| i.viewport().close_requested()) {
            let count = self.editors.len();
            if count > 0 {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                self.launcher.set_status(
                    format!("请先关闭 {} 个 Editor 窗口", count),
                    true,
                );
                // 不 return，继续渲染当前帧
            } else {
                return;
            }
        }

        // 渲染 Launcher
        if self.launcher_visible {
            self.launcher.show(ctx);
        }

        // 检查打开项目请求
        if let Some(project_path) = self.launcher.take_open_request() {
            self.launcher_visible = false;
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));

            let id = egui::ViewportId::from_hash_of(&format!("editor_{}", self.next_id));
            self.next_id += 1;
            self.editors.push(EditorViewport {
                viewport_id: id,
                editor: Editor::open(project_path, self.render_state.clone()),
                close_requested: Arc::new(AtomicBool::new(false)),
            });
        }

        // 渲染所有 editor 视口
        for ev in &mut self.editors {
            let close_flag = &ev.close_requested;
            let editor = &mut ev.editor;
            ctx.show_viewport_immediate(
                ev.viewport_id,
                egui::ViewportBuilder::default()
                    .with_title(format!("Geese Editor - {}", editor.state.project_path))
                    .with_inner_size([1280.0, 720.0]),
                |child_ctx, _class| {
                    if child_ctx.input(|i| i.viewport().close_requested()) {
                        close_flag.store(true, Ordering::SeqCst);
                    }
                    editor.update(child_ctx);
                },
            );
        }
    }
}

// ---------------------------------------------------------------------------
// EditorApp —— 独立 Editor 窗口（由子进程使用，保留兼容性）
// ---------------------------------------------------------------------------

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
