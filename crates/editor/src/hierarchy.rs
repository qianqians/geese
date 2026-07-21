//! 层级面板（Hierarchy）。
//!
//! 树形展示场景节点父子关系，支持选择、搜索、右键菜单和可见性切换。

use crate::panels::{DropTargetHint, EditorAction, EditorPanel, EditorState};
use std::collections::HashMap;

// ---------------------------------------------------------------------------
// SceneNodeData - 场景节点数据
// ---------------------------------------------------------------------------

/// 场景树节点的运行时表示。
#[derive(Debug, Clone)]
pub struct SceneNodeData {
    /// 节点唯一标识
    pub id: String,
    /// 显示名称
    pub name: String,
    /// 子节点 ID 列表
    pub children: Vec<String>,
    /// 父节点 ID
    pub parent: Option<String>,
    /// 是否可见
    pub visible: bool,
    /// 是否锁定
    pub locked: bool,
    /// 节点类型标签
    pub node_type: NodeType,
    /// 资产来源 UUID（如果来自 GLTF 导入，记录其 .meta UUID）
    pub asset_source_uuid: Option<String>,
    /// 嵌套 Prefab 引用的 UUID（如果该节点是 prefab_ref 实例）
    pub prefab_ref_uuid: Option<String>,
    /// 物理组件定义（None 表示无物理组件）
    pub physics: Option<scene::manifest::PhysicsComponentDef>,
    /// NavMesh 组件定义（None 表示不参与导航网格构建）
    pub navmesh: Option<scene::manifest::NavMeshComponentDef>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NodeType {
    Empty,
    Mesh,
    Light,
    Camera,
    PlayerSpawn,
}

impl NodeType {
    fn icon(&self) -> &str {
        match self {
            NodeType::Empty => "📦",
            NodeType::Mesh => "🔷",
            NodeType::Light => "💡",
            NodeType::Camera => "📷",
            NodeType::PlayerSpawn => "🎯",
        }
    }
}

/// 场景节点集合（树形结构）。
#[derive(Debug, Clone)]
pub struct SceneNodeTree {
    nodes: HashMap<String, SceneNodeData>,
    root_ids: Vec<String>,
}

impl SceneNodeTree {
    pub fn new() -> Self {
        Self {
            nodes: HashMap::new(),
            root_ids: Vec::new(),
        }
    }

    pub fn add_node(&mut self, node: SceneNodeData) {
        let has_parent = node.parent.is_some();
        let parent_id = node.parent.clone();
        let id = node.id.clone();
        self.nodes.insert(id.clone(), node);

        if !has_parent {
            self.root_ids.push(id.clone());
        } else if let Some(ref pid) = parent_id {
            if !self.nodes.contains_key(pid) {
                eprintln!("[SceneNodeTree] parent '{}' of node '{}' not found, promoted to root", pid, id);
                self.root_ids.push(id.clone());
            } else {
                // 更新父节点的 children 列表
                if let Some(parent) = self.nodes.get_mut(pid) {
                    parent.children.push(id.clone());
                }
            }
        } else {
            self.root_ids.push(id.clone());
        }
    }

    pub fn root_ids(&self) -> &[String] {
        &self.root_ids
    }

    pub fn get(&self, id: &str) -> Option<&SceneNodeData> {
        self.nodes.get(id)
    }

    pub fn get_mut(&mut self, id: &str) -> Option<&mut SceneNodeData> {
        self.nodes.get_mut(id)
    }

    pub fn children_of(&self, id: &str) -> Vec<&SceneNodeData> {
        self.nodes
            .get(id)
            .map(|node| {
                node.children
                    .iter()
                    .filter_map(|child_id| self.nodes.get(child_id))
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn remove(&mut self, id: &str) {
        let children_ids: Vec<String> = self.nodes
            .get(id)
            .map(|node| node.children.clone())
            .unwrap_or_default();
        for child_id in children_ids {
            self.remove(&child_id);
        }
        self.nodes.remove(id);
        self.root_ids.retain(|rid| rid != id);
    }

    /// 收集以 `id` 为根的子树中所有节点 ID（包括 `id` 自身）。
    pub fn collect_subtree_ids(&self, id: &str) -> Vec<String> {
        let mut result = vec![id.to_string()];
        let children_ids: Vec<String> = self.nodes
            .get(id)
            .map(|node| node.children.clone())
            .unwrap_or_default();
        for child_id in children_ids {
            result.extend(self.collect_subtree_ids(&child_id));
        }
        result
    }

    pub fn find(&self, name_filter: &str) -> Vec<String> {
        if name_filter.is_empty() {
            return self.nodes.keys().cloned().collect();
        }
        let lower = name_filter.to_lowercase();
        self.nodes
            .iter()
            .filter(|(_, n)| n.name.to_lowercase().contains(&lower))
            .map(|(id, _)| id.clone())
            .collect()
    }
}

// ---------------------------------------------------------------------------
// HierarchyPanel
// ---------------------------------------------------------------------------

/// 层级面板。
pub struct HierarchyPanel {
    /// 场景节点树
    tree: SceneNodeTree,
    /// 搜索文本
    search_text: String,
    /// 展开的节点集合
    pub(crate) expanded: std::collections::HashSet<String>,
    /// 正在重命名的节点 (node_id, 当前编辑名称)
    renaming_node: Option<(String, String)>,
}

impl HierarchyPanel {
    /// 返回场景节点树的只读引用。
    pub fn tree(&self) -> &SceneNodeTree {
        &self.tree
    }

    /// 返回场景节点树的可变引用。
    pub fn tree_mut(&mut self) -> &mut SceneNodeTree {
        &mut self.tree
    }

    /// 从外部添加场景节点到层次树
    pub fn add_scene_node(&mut self, data: SceneNodeData) {
        self.tree.add_node(data);
    }

    pub fn new() -> Self {
        let mut tree = SceneNodeTree::new();

        // 添加示例节点（演示用）
        tree.add_node(SceneNodeData {
            id: "root".into(),
            name: "Scene Root".into(),
            children: vec!["room".into(), "outdoor".into()],
            parent: None,
            visible: true,
            locked: false,
            node_type: NodeType::Empty,
            asset_source_uuid: None,
            prefab_ref_uuid: None,
            physics: None,
            navmesh: None,
        });
        tree.add_node(SceneNodeData {
            id: "room".into(),
            name: "Room".into(),
            children: vec!["floor".into(), "walls".into(), "light_main".into()],
            parent: Some("root".into()),
            visible: true,
            locked: false,
            node_type: NodeType::Empty,
            asset_source_uuid: None,
            prefab_ref_uuid: None,
            physics: None,
            navmesh: None,
        });
        tree.add_node(SceneNodeData {
            id: "floor".into(),
            name: "Floor".into(),
            children: vec![],
            parent: Some("room".into()),
            visible: true,
            locked: false,
            node_type: NodeType::Mesh,
            asset_source_uuid: None,
            prefab_ref_uuid: None,
            physics: None,
            navmesh: None,
        });
        tree.add_node(SceneNodeData {
            id: "walls".into(),
            name: "Walls".into(),
            children: vec![],
            parent: Some("room".into()),
            visible: true,
            locked: false,
            node_type: NodeType::Mesh,
            asset_source_uuid: None,
            prefab_ref_uuid: None,
            physics: None,
            navmesh: None,
        });
        tree.add_node(SceneNodeData {
            id: "light_main".into(),
            name: "Main Light".into(),
            children: vec![],
            parent: Some("room".into()),
            visible: true,
            locked: false,
            node_type: NodeType::Light,
            asset_source_uuid: None,
            prefab_ref_uuid: None,
            physics: None,
            navmesh: None,
        });
        tree.add_node(SceneNodeData {
            id: "outdoor".into(),
            name: "Outdoor Props".into(),
            children: vec!["player_spawn".into()],
            parent: Some("root".into()),
            visible: true,
            locked: false,
            node_type: NodeType::Empty,
            asset_source_uuid: None,
            prefab_ref_uuid: None,
            physics: None,
            navmesh: None,
        });
        tree.add_node(SceneNodeData {
            id: "player_spawn".into(),
            name: "Player Spawn".into(),
            children: vec![],
            parent: Some("outdoor".into()),
            visible: true,
            locked: false,
            node_type: NodeType::PlayerSpawn,
            asset_source_uuid: None,
            prefab_ref_uuid: None,
            physics: None,
            navmesh: None,
        });

        Self {
            tree,
            search_text: String::new(),
            expanded: std::collections::HashSet::new(),
            renaming_node: None,
        }
    }

    /// 递归渲染节点树。
    fn render_node(
        &mut self,
        ui: &mut egui::Ui,
        node_id: &str,
        state: &mut EditorState,
        depth: usize,
    ) {
        let (visible, locked, icon, name, child_ids, is_selected) = match self.tree.get(node_id) {
            Some(n) => (
                n.visible,
                n.locked,
                n.node_type.icon().to_string(),
                n.name.clone(),
                n.children.clone(),
                state.selected_entity.as_deref() == Some(node_id),
            ),
            None => return,
        };
        let has_children = !child_ids.is_empty();

        let indent = depth * 16;
        ui.horizontal(|ui| {
            ui.add_space(indent as f32);

            // 展开/折叠箭头
            let _expand_id = ui.make_persistent_id(format!("expand_{node_id}"));
            let expanded = self.expanded.contains(node_id);
            if has_children {
                let arrow = if expanded { "▼" } else { "▶" };
                if ui
                    .add_sized([16.0, 16.0], egui::Button::new(arrow).fill(egui::Color32::TRANSPARENT))
                    .clicked()
                {
                    if expanded {
                        self.expanded.remove(node_id);
                    } else {
                        self.expanded.insert(node_id.to_string());
                    }
                }
            } else {
                ui.add_sized([16.0, 16.0], egui::Label::new(""));
            }

            // 可见性 toggle
            let eye = if visible { "👁" } else { "—" };
            if ui
                .add_sized([16.0, 16.0], egui::Button::new(eye).fill(egui::Color32::TRANSPARENT))
                .clicked()
            {
                if let Some(n) = self.tree.get_mut(node_id) {
                    n.visible = !n.visible;
                }
            }

            // 锁定 toggle
            let lock = if locked { "🔒" } else { "🔓" };
            if ui
                .add_sized([16.0, 16.0], egui::Button::new(lock).fill(egui::Color32::TRANSPARENT))
                .clicked()
            {
                if let Some(n) = self.tree.get_mut(node_id) {
                    n.locked = !n.locked;
                }
            }

            // 节点标签（或重命名输入框）
            let is_renaming = self.renaming_node.as_ref().map(|(id, _)| id.as_str()) == Some(node_id);
            if is_renaming {
                let rename_name = &mut self.renaming_node.as_mut().unwrap().1;
                let text_edit = egui::TextEdit::singleline(rename_name)
                    .desired_width(120.0)
                    .lock_focus(true);
                let response = ui.add(text_edit);
                response.request_focus();
                // Enter 确认，Escape 取消
                if ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    if let Some((nid, new_name)) = self.renaming_node.take() {
                        if !new_name.is_empty() {
                            state.name_cache.insert(nid.clone(), new_name.clone());
                            state.pending_actions.push(EditorAction::RenameEntity {
                                node_id: nid,
                                new_name,
                            });
                        }
                    }
                }
                if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                    self.renaming_node = None;
                }
            } else {
                let label = format!("{} {}", icon, name);
                let response = ui.selectable_label(is_selected, label);

                // 左键选择
                if response.clicked() {
                    state.selected_entity = Some(node_id.to_string());
                }

                // 双击聚焦（占位：后续实现摄像机聚焦）
                if response.double_clicked() {
                    state.selected_entity = Some(node_id.to_string());
                }

                // 拖放悬停检测：拖拽资产到节点上时，标记为潜在父节点
                let drag_active = state.dragged_asset_uuid.is_some()
                    && state.drag_source.as_deref() == Some("AssetBrowser");
                if drag_active && response.hovered() {
                    state.drop_target_hint = Some(DropTargetHint::Hierarchy {
                        target_node_id: Some(node_id.to_string()),
                    });
                    // 高亮当前悬停节点
                    let rect = response.rect;
                    ui.painter().rect_filled(
                        rect,
                        0.0,
                        egui::Color32::from_rgba_premultiplied(60, 100, 255, 60),
                    );
                }

                // 右键菜单
                response.context_menu(|ui| {
                    if ui.button("✏ Rename").clicked() {
                        ui.close_menu();
                        // 进入重命名模式
                        let current_name = state.name_cache.get(node_id).cloned().unwrap_or_else(|| name.clone());
                        self.renaming_node = Some((node_id.to_string(), current_name));
                    }
                    if ui.button("📋 Duplicate").clicked() {
                        ui.close_menu();
                        // 复制节点：生成新 ID，克隆数据
                        let new_id = format!("node_{}", uuid::Uuid::new_v4());
                        let parent_id = self.tree.get(node_id).and_then(|n| n.parent.clone());
                        if let Some(source_node) = self.tree.get(node_id) {
                            let new_node = SceneNodeData {
                                id: new_id.clone(),
                                name: format!("{}_copy", source_node.name),
                                children: vec![],
                                parent: parent_id.clone(),
                                visible: source_node.visible,
                                locked: source_node.locked,
                                node_type: source_node.node_type.clone(),
                                asset_source_uuid: source_node.asset_source_uuid.clone(),
                                prefab_ref_uuid: source_node.prefab_ref_uuid.clone(),
                                physics: source_node.physics.clone(),
                                navmesh: source_node.navmesh.clone(),
                            };
                            self.tree.add_node(new_node);
                            // 复制缓存
                            if let Some(tx) = state.transform_cache.get(node_id) {
                                state.transform_cache.insert(new_id.clone(), *tx);
                            }
                            if let Some(nm) = state.name_cache.get(node_id) {
                                state.name_cache.insert(new_id.clone(), format!("{}_copy", nm));
                            }
                            if state.mesh_entities.contains(node_id) {
                                state.mesh_entities.insert(new_id.clone());
                            }
                            if let Some(phys) = state.physics_component_cache.get(node_id) {
                                state.physics_component_cache.insert(new_id.clone(), phys.clone());
                            }
                            if let Some(nav) = state.navmesh_component_cache.get(node_id) {
                                state.navmesh_component_cache.insert(new_id.clone(), nav.clone());
                            }
                        }
                    }
                    if ui.button("➕ Create Empty Child").clicked() {
                        ui.close_menu();
                        let new_id = format!("node_{}", uuid::Uuid::new_v4());
                        self.tree.add_node(SceneNodeData {
                            id: new_id.clone(),
                            name: "GameObject".into(),
                            children: vec![],
                            parent: Some(node_id.to_string()),
                            visible: true,
                            locked: false,
                            node_type: NodeType::Empty,
                            asset_source_uuid: None,
                            prefab_ref_uuid: None,
                            physics: None,
                            navmesh: None,
                        });
                        // 填充缓存
                        state.transform_cache.insert(new_id.clone(), ([0.0, 0.0, 0.0], [0.0, 0.0, 0.0], [1.0, 1.0, 1.0]));
                        state.name_cache.insert(new_id, "GameObject".into());
                        // 展开父节点
                        self.expanded.insert(node_id.to_string());
                    }
                    ui.menu_button("🔷 Create Primitive", |ui| {
                        let parent = Some(node_id.to_string());
                        if ui.button("Cube").clicked() {
                            state.pending_actions.push(EditorAction::CreatePrimitive { kind: "cube".into(), position: [0.0, 0.0, 0.0], parent_node_id: parent.clone() });
                            ui.close_menu();
                        }
                        if ui.button("Sphere").clicked() {
                            state.pending_actions.push(EditorAction::CreatePrimitive { kind: "sphere".into(), position: [0.0, 0.0, 0.0], parent_node_id: parent.clone() });
                            ui.close_menu();
                        }
                        if ui.button("Plane").clicked() {
                            state.pending_actions.push(EditorAction::CreatePrimitive { kind: "plane".into(), position: [0.0, 0.0, 0.0], parent_node_id: parent.clone() });
                            ui.close_menu();
                        }
                        if ui.button("Cylinder").clicked() {
                            state.pending_actions.push(EditorAction::CreatePrimitive { kind: "cylinder".into(), position: [0.0, 0.0, 0.0], parent_node_id: parent });
                            ui.close_menu();
                        }
                    });
                    ui.separator();
                    if ui.button("📦 Save as Prefab").clicked() {
                        ui.close_menu();
                        state.pending_actions.push(EditorAction::SaveAsPrefab {
                            node_id: node_id.to_string(),
                        });
                    }
                    ui.separator();
                    if ui.button("🗑 Delete").clicked() {
                        ui.close_menu();
                        // 先收集所有将被删除的节点 ID（包括子树）
                        let deleted_ids = self.tree.collect_subtree_ids(node_id);
                        self.tree.remove(node_id);
                        // 清理所有缓存
                        for id in &deleted_ids {
                            state.transform_cache.remove(id);
                            state.physics_component_cache.remove(id);
                            state.navmesh_component_cache.remove(id);
                            state.name_cache.remove(id);
                            state.mesh_entities.remove(id);
                        }
                        if state.selected_entity.as_deref() == Some(node_id) {
                            state.selected_entity = None;
                        }
                    }
                });
            }
        });

        // 渲染子节点
        if self.expanded.contains(node_id) && has_children {
            for child_id in &child_ids {
                self.render_node(ui, child_id, state, depth + 1);
            }
        }
    }
}

impl EditorPanel for HierarchyPanel {
    fn title(&self) -> &str {
        "Hierarchy"
    }

    fn show(&mut self, ui: &mut egui::Ui, state: &mut EditorState) {
        // 标题
        ui.horizontal(|ui| {
            ui.strong("Scene Hierarchy");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("➕").on_hover_text("Create Empty").clicked() {
                    let new_id = format!("node_{}", uuid::Uuid::new_v4());
                    let parent = state.selected_entity.clone();
                    self.tree.add_node(SceneNodeData {
                        id: new_id.clone(),
                        name: "GameObject".into(),
                        children: vec![],
                        parent: parent.clone(),
                        visible: true,
                        locked: false,
                        node_type: NodeType::Empty,
                        asset_source_uuid: None,
                        prefab_ref_uuid: None,
                        physics: None,
                        navmesh: None,
                    });
                    // 填充缓存
                    state.transform_cache.insert(new_id.clone(), ([0.0, 0.0, 0.0], [0.0, 0.0, 0.0], [1.0, 1.0, 1.0]));
                    state.name_cache.insert(new_id.clone(), "GameObject".into());
                    // 展开父节点
                    if let Some(ref parent_id) = parent {
                        self.expanded.insert(parent_id.clone());
                    }
                    // 选中新节点
                    state.selected_entity = Some(new_id);
                }
            });
        });

        ui.add_space(4.0);

        // 搜索栏
        ui.add(
            egui::TextEdit::singleline(&mut self.search_text)
                .hint_text("🔍 Search...")
                .desired_width(ui.available_width()),
        );
        ui.add_space(4.0);

        // 检测拖拽状态
        let drag_active = state.dragged_asset_uuid.is_some()
            && state.drag_source.as_deref() == Some("AssetBrowser");

        // 节点树
        let scroll_response = egui::ScrollArea::vertical()
            .id_salt("hierarchy_scroll")
            .show(ui, |ui| {
                if self.search_text.is_empty() {
                    // 正常显示树
                    let root_ids = self.tree.root_ids().to_vec();
                    for root_id in &root_ids {
                        self.render_node(ui, root_id, state, 0);
                    }

                    // 拖拽时留出空白区域供 drop 到根级别
                    if drag_active && root_ids.is_empty() {
                        ui.add_space(40.0);
                        ui.label(
                            egui::RichText::new("Drop here to add root node")
                                .color(egui::Color32::from_gray(120))
                                .size(12.0),
                        );
                    }
                } else {
                    // 搜索模式：平铺显示匹配节点
                    let matched = self.tree.find(&self.search_text);
                    for id in &matched {
                        if let Some(node) = self.tree.get(id) {
                            let is_selected = state.selected_entity.as_deref() == Some(id);
                            let label = format!("{} {}", node.node_type.icon(), node.name);
                            let resp = ui.selectable_label(is_selected, label);
                            if resp.clicked() {
                                state.selected_entity = Some(id.clone());
                            }
                            if drag_active && resp.hovered() {
                                state.drop_target_hint = Some(DropTargetHint::Hierarchy {
                                    target_node_id: Some(id.clone()),
                                });
                            }
                        }
                    }
                }
            });

        // ── 拖放高亮边框 ──
        if drag_active {
            let scroll_rect = scroll_response.inner_rect;
            let highlight = egui::Color32::from_rgba_premultiplied(60, 100, 255, 80);
            let stroke = egui::Stroke::new(2.0, egui::Color32::from_rgb(80, 130, 255));
            ui.painter().rect_filled(scroll_rect, 0.0, highlight);
            ui.painter().rect_stroke(scroll_rect, 4.0, stroke);

            // 在空白区域 drop → 根节点
            let hovered = ui.rect_contains_pointer(scroll_rect);
            if hovered && state.drop_target_hint.is_none() {
                state.drop_target_hint = Some(DropTargetHint::Hierarchy {
                    target_node_id: None,
                });
            }
        }

        // ── 鼠标释放时消费拖放 ──
        if drag_active {
            let released = ui.input(|input| {
                input.pointer.button_released(egui::PointerButton::Primary)
            });
            if released {
                let scroll_rect = scroll_response.inner_rect;
                if ui.rect_contains_pointer(scroll_rect) {
                    let parent_id = match &state.drop_target_hint {
                        Some(DropTargetHint::Hierarchy { target_node_id }) => target_node_id.clone(),
                        _ => None,
                    };
                    let prefab_uuid = state.dragged_asset_uuid.clone().unwrap_or_default();
                    state.pending_actions.push(EditorAction::InstantiatePrefab {
                        prefab_uuid,
                        position: [0.0, 0.0, 0.0],
                        parent_node_id: parent_id,
                    });
                }
                // 清除拖拽状态
                state.dragged_asset_uuid = None;
                state.dragged_asset_type = None;
                state.dragged_asset_name = None;
                state.drag_source = None;
                state.drop_target_hint = None;
            }
        }
    }
}
