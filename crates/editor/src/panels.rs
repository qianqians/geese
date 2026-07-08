//! 编辑器面板系统。
//!
//! 提供：
//! - [`EditorPanel`] trait：统一的面板接口
//! - [`EditorState`]：编辑器全局状态
//! - 面板可见性由 [`PanelLayerManager`] 统一管理
//! - [`EditorLayout`]：面板布局渲染器

use crate::editor_mode::EditorMode;
use crate::panel_layer::{PanelLayer, PanelLayerManager};
use physics_manager::BodySnapshot;
use std::collections::HashMap;

// ---------------------------------------------------------------------------
// EditorAction - 面板请求的编辑器操作
// ---------------------------------------------------------------------------

/// 面板通过 EditorState 请求 Editor 执行的操作。
#[derive(Debug, Clone)]
pub enum EditorAction {
    /// 将指定节点及其子树保存为 Prefab
    SaveAsPrefab { node_id: String },
    /// 在指定位置实例化 Prefab（拖放或菜单触发）
    InstantiatePrefab {
        prefab_uuid: String,
        position: [f32; 3],
        /// 目标父节点 ID（Hierarchy 拖放时指定，None 为根节点）
        parent_node_id: Option<String>,
    },
    /// 添加/移除角色控制器
    ToggleCharacterController {
        node_id: String,
        enabled: bool,
        move_speed: f32,
        jump_impulse: f32,
        air_control: f32,
        half_height: f32,
        radius: f32,
    },
    /// 修改动画标记
    ModifyAnimationMarker {
        clip_index: usize,
        time: f32,
        name: String,
        remove: bool,
    },
    /// 设置/移除物理组件
    SetPhysicsComponent {
        node_id: String,
        component: Option<scene::manifest::PhysicsComponentDef>,
    },
    /// 设置/移除 NavMesh 组件
    SetNavMeshComponent {
        node_id: String,
        component: Option<scene::manifest::NavMeshComponentDef>,
    },
    /// 重命名实体
    RenameEntity {
        node_id: String,
        new_name: String,
    },
    /// 切换实体可见性
    ToggleVisibility {
        node_id: String,
        visible: bool,
    },
    /// 导出游戏（构建独立 .exe + assets）
    ExportGameWindows,
    /// 导出游戏（Android .so 构建）
    ExportGameAndroid,
    /// 打开构建面板
    OpenBuildPanel,
}

// ---------------------------------------------------------------------------
// DropTargetHint - 拖放目标提示
// ---------------------------------------------------------------------------

/// 拖放时在目标面板显示的提示信息。
#[derive(Debug, Clone)]
pub enum DropTargetHint {
    /// 在视口上拖放，显示世界坐标预览位置
    Viewport { world_pos: [f32; 3] },
    /// 在层级面板上拖放，显示插入目标节点
    Hierarchy { target_node_id: Option<String> },
}

// ---------------------------------------------------------------------------
// PendingTransform - Inspector 写回的变换变更
// ---------------------------------------------------------------------------

/// 由 Inspector 面板生成的变换变更，帧末由 Editor 消费并推入 CommandHistory。
#[derive(Debug, Clone)]
pub struct PendingTransform {
    pub entity_id: String,
    pub old_position: [f32; 3],
    pub new_position: [f32; 3],
    pub old_rotation: [f32; 3],
    pub new_rotation: [f32; 3],
    pub old_scale: [f32; 3],
    pub new_scale: [f32; 3],
}

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
    /// 面板可见性管理器（单一真相源，替代原 PanelVisibility + panel_alpha 双轨）
    pub panel_layer: PanelLayerManager,
    /// 物理碰撞体调试渲染数据
    pub physics_debug_bodies: Vec<BodySnapshot>,
    /// 实体变换缓存（selection 时填入上次确认值，用于 undo）
    pub transform_cache: HashMap<String, ([f32; 3], [f32; 3], [f32; 3])>,
    /// 实体物理组件缓存 (entity_id → PhysicsComponentDef)
    pub physics_component_cache: HashMap<String, scene::manifest::PhysicsComponentDef>,
    /// 实体 NavMesh 组件缓存 (entity_id → NavMeshComponentDef)
    pub navmesh_component_cache: HashMap<String, scene::manifest::NavMeshComponentDef>,
    /// 实体名称缓存 (entity_id → name)，用于 Inspector 即时编辑
    pub name_cache: HashMap<String, String>,
    /// Inspector 写回的待提交变换变更
    pub pending_transform: Option<PendingTransform>,
    /// 面板请求的待处理操作队列
    pub pending_actions: Vec<EditorAction>,
    /// 当前拖拽的资产 UUID（非空时表示正在拖拽中）
    pub dragged_asset_uuid: Option<String>,
    /// 拖拽资产的类型（用于 Viewport 区分模型/Prefab）
    pub dragged_asset_type: Option<crate::asset_browser::AssetType>,
    /// 拖拽来源面板名称（"AssetBrowser"）
    pub drag_source: Option<String>,
    /// 拖拽资产的名称（用于预览标签）
    pub dragged_asset_name: Option<String>,
    /// 视口/层级面板显示的拖放提示
    pub drop_target_hint: Option<DropTargetHint>,
    /// 动画剪辑信息: (name, duration, index)
    pub animation_clips: Vec<(String, f32, usize)>,
    /// 每个剪辑的标记列表: Vec<(time, name)>
    pub animation_markers: Vec<Vec<(f32, String)>>,
    /// 状态栏消息
    pub status_message: Option<String>,
}

impl EditorState {
    pub fn new(project_path: String) -> Self {
        Self {
            project_path,
            selected_entity: None,
            mode: EditorMode::Edit,
            ui_visible: true,
            panel_layer: PanelLayerManager::default(),
            physics_debug_bodies: Vec::new(),
            transform_cache: HashMap::new(),
            physics_component_cache: HashMap::new(),
            navmesh_component_cache: HashMap::new(),
            name_cache: HashMap::new(),
            pending_transform: None,
            pending_actions: Vec::new(),
            dragged_asset_uuid: None,
            dragged_asset_type: None,
            drag_source: None,
            dragged_asset_name: None,
            drop_target_hint: None,
            animation_clips: Vec::new(),
            animation_markers: Vec::new(),
            status_message: None,
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
        if state.panel_layer.is_visible(&PanelLayer::AssetBrowser) {
            egui::TopBottomPanel::bottom("editor_bottom")
                .resizable(true)
                .default_height(200.0)
                .show(ctx, |ui| {
                    asset_browser.show(ui, state);
                });
        }

        // 左侧面板
        if state.panel_layer.is_visible(&PanelLayer::Hierarchy) {
            egui::SidePanel::left("editor_left")
                .resizable(true)
                .default_width(250.0)
                .show(ctx, |ui| {
                    hierarchy.show(ui, state);
                });
        }

        // 右侧面板
        if state.panel_layer.is_visible(&PanelLayer::Inspector) {
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
