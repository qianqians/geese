//! 编辑器面板系统。
//!
//! 提供：
//! - [`EditorPanel`] trait：统一的面板接口
//! - [`EditorState`]：编辑器全局状态
//! - [`PanelVisibility`]：面板显隐状态管理
//! - [`EditorLayout`]：面板布局渲染器

use crate::editor_mode::EditorMode;
use crate::physics_client::BodySnapshot;

// ---------------------------------------------------------------------------
// EditorState - 编辑器全局状态
// ---------------------------------------------------------------------------

/// 编辑器全局状态，所有面板共享。
#[derive(Debug, Clone)]
pub struct EditorState {
    /// 项目路径
    pub project_path: String,
    /// 当前选中的实体 ID
    pub selected_entity: Option<String>,
    /// 编辑器运行模式
    pub mode: EditorMode,
    /// 浮动面板总开关
    pub ui_visible: bool,
    /// 全局面板不透明度（0.0-1.0）
    pub panel_alpha: f32,
    /// 面板可见性
    pub panel_visibility: PanelVisibility,
    /// 物理碰撞体调试渲染数据
    pub physics_debug_bodies: Vec<BodySnapshot>,
}

impl EditorState {
    pub fn new(project_path: String) -> Self {
        Self {
            project_path,
            selected_entity: None,
            mode: EditorMode::Edit,
            ui_visible: true,
            panel_alpha: 0.85,
            panel_visibility: PanelVisibility::default(),
            physics_debug_bodies: Vec::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// EditorPanel trait
// ---------------------------------------------------------------------------

/// 编辑器面板统一接口。
pub trait EditorPanel {
    /// 面板标题。
    fn title(&self) -> &str;

    /// 渲染面板 UI。
    fn show(&mut self, ui: &mut egui::Ui, state: &mut EditorState);
}

// ---------------------------------------------------------------------------
// PanelVisibility - 面板显隐管理
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct PanelVisibility {
    pub hierarchy: bool,
    pub inspector: bool,
    pub asset_browser: bool,
}

impl Default for PanelVisibility {
    fn default() -> Self {
        Self {
            hierarchy: true,
            inspector: true,
            asset_browser: true,
        }
    }
}

impl PanelVisibility {
    pub fn toggle_hierarchy(&mut self) {
        self.hierarchy = !self.hierarchy;
    }
    pub fn toggle_inspector(&mut self) {
        self.inspector = !self.inspector;
    }
    pub fn toggle_asset_browser(&mut self) {
        self.asset_browser = !self.asset_browser;
    }
}

// ---------------------------------------------------------------------------
// EditorLayout - 面板布局渲染器
// ---------------------------------------------------------------------------

/// 编辑器布局渲染器。每帧调用 `render_fullscreen` 渲染全屏视口。
pub struct EditorLayout;

impl EditorLayout {
    /// 渲染传统的编辑器布局：左侧 Hierarchy + 中央 Viewport + 右侧 Inspector + 底部 Asset Browser。
    #[allow(dead_code)]
    pub fn render(
        ctx: &egui::Context,
        state: &mut EditorState,
        hierarchy: &mut dyn EditorPanel,
        viewport: &mut dyn EditorPanel,
        inspector: &mut dyn EditorPanel,
        asset_browser: &mut dyn EditorPanel,
    ) {
        // 底部面板必须先于侧边面板声明（egui 布局要求）
        if state.panel_visibility.asset_browser {
            egui::TopBottomPanel::bottom("editor_bottom")
                .resizable(true)
                .default_height(200.0)
                .show(ctx, |ui| {
                    asset_browser.show(ui, state);
                });
        }

        // 左侧面板
        if state.panel_visibility.hierarchy {
            egui::SidePanel::left("editor_left")
                .resizable(true)
                .default_width(250.0)
                .show(ctx, |ui| {
                    hierarchy.show(ui, state);
                });
        }

        // 右侧面板
        if state.panel_visibility.inspector {
            egui::SidePanel::right("editor_right")
                .resizable(true)
                .default_width(300.0)
                .show(ctx, |ui| {
                    inspector.show(ui, state);
                });
        }

        // 中央视口（最后创建，占据剩余空间）
        egui::CentralPanel::default().show(ctx, |ui| {
            viewport.show(ui, state);
        });
    }

    /// 渲染全屏沉浸式视口：单个 CentralPanel 占满整个窗口，场景渲染纹理填充整个区域。
    pub fn render_fullscreen(
        ctx: &egui::Context,
        state: &mut EditorState,
        viewport: &mut dyn EditorPanel,
    ) {
        // 全屏中央面板（占满整个窗口）
        egui::CentralPanel::default().show(ctx, |ui| {
            // 视口使用整个可用空间
            viewport.show(ui, state);
        });
    }
}
