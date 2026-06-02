//! 编辑器面板系统。
//!
//! 提供：
//! - [`EditorPanel`] trait：统一的面板接口
//! - [`EditorState`]：编辑器全局状态
//! - [`PanelVisibility`]：面板显隐状态管理
//! - [`EditorLayout`]：面板布局渲染器

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
    /// 是否处于播放模式
    pub is_playing: bool,
    /// 面板可见性
    pub panel_visibility: PanelVisibility,
}

impl EditorState {
    pub fn new(project_path: String) -> Self {
        Self {
            project_path,
            selected_entity: None,
            is_playing: false,
            panel_visibility: PanelVisibility::default(),
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

/// 编辑器布局渲染器。每帧调用 `render` 渲染所有面板。
pub struct EditorLayout;

impl EditorLayout {
    /// 渲染完整的编辑器布局：左侧 Hierarchy + 中央 Viewport + 右侧 Inspector + 底部 Asset Browser。
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
}
