//! 层级面板（Hierarchy）。
//!
//! 树形展示场景节点父子关系，支持选择、搜索、右键菜单和可见性切换。

use crate::panels::{EditorPanel, EditorState};
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
            .values()
            .filter(|n| n.parent.as_deref() == Some(id))
            .collect()
    }

    pub fn remove(&mut self, id: &str) {
        // 递归删除子节点
        let children_ids: Vec<String> = self.nodes
            .values()
            .filter(|n| n.parent.as_deref() == Some(id))
            .map(|n| n.id.clone())
            .collect();
        for child_id in children_ids {
            self.remove(&child_id);
        }
        self.nodes.remove(id);
        self.root_ids.retain(|rid| rid != id);
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
    /// 右键菜单目标
    context_menu_id: Option<String>,
    /// 展开的节点集合
    expanded: std::collections::HashSet<String>,
}

impl HierarchyPanel {
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
        });
        tree.add_node(SceneNodeData {
            id: "room".into(),
            name: "Room".into(),
            children: vec!["floor".into(), "walls".into(), "light_main".into()],
            parent: Some("root".into()),
            visible: true,
            locked: false,
            node_type: NodeType::Empty,
        });
        tree.add_node(SceneNodeData {
            id: "floor".into(),
            name: "Floor".into(),
            children: vec![],
            parent: Some("room".into()),
            visible: true,
            locked: false,
            node_type: NodeType::Mesh,
        });
        tree.add_node(SceneNodeData {
            id: "walls".into(),
            name: "Walls".into(),
            children: vec![],
            parent: Some("room".into()),
            visible: true,
            locked: false,
            node_type: NodeType::Mesh,
        });
        tree.add_node(SceneNodeData {
            id: "light_main".into(),
            name: "Main Light".into(),
            children: vec![],
            parent: Some("room".into()),
            visible: true,
            locked: false,
            node_type: NodeType::Light,
        });
        tree.add_node(SceneNodeData {
            id: "outdoor".into(),
            name: "Outdoor Props".into(),
            children: vec!["player_spawn".into()],
            parent: Some("root".into()),
            visible: true,
            locked: false,
            node_type: NodeType::Empty,
        });
        tree.add_node(SceneNodeData {
            id: "player_spawn".into(),
            name: "Player Spawn".into(),
            children: vec![],
            parent: Some("outdoor".into()),
            visible: true,
            locked: false,
            node_type: NodeType::PlayerSpawn,
        });

        Self {
            tree,
            search_text: String::new(),
            context_menu_id: None,
            expanded: std::collections::HashSet::new(),
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
        let node = match self.tree.get(node_id) {
            Some(n) => n.clone(),
            None => return,
        };

        let is_selected = state.selected_entity.as_deref() == Some(node_id);
        let has_children = !node.children.is_empty();

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
            let eye = if node.visible { "👁" } else { "—" };
            if ui
                .add_sized([16.0, 16.0], egui::Button::new(eye).fill(egui::Color32::TRANSPARENT))
                .clicked()
            {
                if let Some(n) = self.tree.get_mut(node_id) {
                    n.visible = !n.visible;
                }
            }

            // 锁定 toggle
            let lock = if node.locked { "🔒" } else { "🔓" };
            if ui
                .add_sized([16.0, 16.0], egui::Button::new(lock).fill(egui::Color32::TRANSPARENT))
                .clicked()
            {
                if let Some(n) = self.tree.get_mut(node_id) {
                    n.locked = !n.locked;
                }
            }

            // 节点标签
            let label = format!("{} {}", node.node_type.icon(), node.name);
            let response = ui.selectable_label(is_selected, label);

            // 左键选择
            if response.clicked() {
                state.selected_entity = Some(node_id.to_string());
            }

            // 双击聚焦（占位：后续实现摄像机聚焦）
            if response.double_clicked() {
                state.selected_entity = Some(node_id.to_string());
            }

            // 右键菜单
            response.context_menu(|ui| {
                if ui.button("✏ Rename").clicked() {
                    ui.close_menu();
                }
                if ui.button("📋 Duplicate").clicked() {
                    ui.close_menu();
                }
                if ui.button("➕ Create Empty Child").clicked() {
                    ui.close_menu();
                }
                ui.separator();
                if ui.button("🗑 Delete").clicked() {
                    ui.close_menu();
                    self.tree.remove(node_id);
                    if state.selected_entity.as_deref() == Some(node_id) {
                        state.selected_entity = None;
                    }
                }
            });
        });

        // 渲染子节点
        if self.expanded.contains(node_id) && has_children {
            let child_ids: Vec<String> = self.tree
                .children_of(node_id)
                .iter()
                .map(|n| n.id.clone())
                .collect();
            for child_id in child_ids {
                self.render_node(ui, &child_id, state, depth + 1);
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
                    self.tree.add_node(SceneNodeData {
                        id: new_id,
                        name: "GameObject".into(),
                        children: vec![],
                        parent: state.selected_entity.clone(),
                        visible: true,
                        locked: false,
                        node_type: NodeType::Empty,
                    });
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

        // 节点树
        egui::ScrollArea::vertical()
            .id_salt("hierarchy_scroll")
            .show(ui, |ui| {
                if self.search_text.is_empty() {
                    // 正常显示树
                    let root_ids = self.tree.root_ids().to_vec();
                    for root_id in &root_ids {
                        self.render_node(ui, root_id, state, 0);
                    }
                } else {
                    // 搜索模式：平铺显示匹配节点
                    let matched = self.tree.find(&self.search_text);
                    for id in &matched {
                        if let Some(node) = self.tree.get(id) {
                            let is_selected = state.selected_entity.as_deref() == Some(id);
                            let label = format!("{} {}", node.node_type.icon(), node.name);
                            if ui.selectable_label(is_selected, label).clicked() {
                                state.selected_entity = Some(id.clone());
                            }
                        }
                    }
                }
            });
    }
}
