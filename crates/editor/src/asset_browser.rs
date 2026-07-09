//! 资源浏览器（Asset Browser）— 类 Unity Project 面板。
//!
//! 提供双栏布局：左侧文件夹树 + 右侧资源内容区，
//! 支持面包屑导航、搜索、类型过滤、列表/网格视图切换和拖拽导入。
//! 数据源从 AssetDatabase 获取。

use crate::panels::{EditorPanel, EditorState};
use asset::database::AssetDatabase;
use asset::meta::AssetTypeKind;
use std::collections::HashMap;

// ---------------------------------------------------------------------------
// AssetEntry - 资源条目（UI 展示用）
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct AssetEntry {
    pub name: String,
    pub path: String,
    pub uuid: String,
    pub asset_type: AssetType,
    pub size_bytes: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssetType {
    Folder,
    Scene,
    Model,
    Texture,
    Audio,
    Avatar,
    Prefab,
    Other,
}

impl AssetType {
    fn icon(&self) -> &str {
        match self {
            AssetType::Folder => "\u{1F4C1}",
            AssetType::Scene => "\u{1F3AC}",
            AssetType::Model => "\u{1F537}",
            AssetType::Texture => "\u{1F5BC}",
            AssetType::Audio => "\u{1F50A}",
            AssetType::Avatar => "\u{1F9D1}",
            AssetType::Prefab => "\u{1F4E6}",
            AssetType::Other => "\u{1F4C4}",
        }
    }

    /// 是否可拖拽到场景（仅 Model 和 Prefab）。
    pub fn is_draggable(&self) -> bool {
        matches!(self, AssetType::Model | AssetType::Prefab)
    }

    /// 从 AssetTypeKind 转换。
    fn from_kind(kind: AssetTypeKind) -> Self {
        match kind {
            AssetTypeKind::Model => AssetType::Model,
            AssetTypeKind::Texture => AssetType::Texture,
            AssetTypeKind::Audio => AssetType::Audio,
            AssetTypeKind::Scene => AssetType::Scene,
            AssetTypeKind::Avatar => AssetType::Avatar,
            AssetTypeKind::Prefab => AssetType::Prefab,
            AssetTypeKind::Material | AssetTypeKind::Other => AssetType::Other,
        }
    }
}

// ---------------------------------------------------------------------------
// FolderNode - 文件夹树节点
// ---------------------------------------------------------------------------

/// 文件夹树节点，用于左侧导航面板。
#[derive(Debug, Clone)]
struct FolderNode {
    path: String,
    children: Vec<FolderNode>,
}

impl FolderNode {
    fn new(path: String) -> Self {
        Self { path, children: Vec::new() }
    }
}

// ---------------------------------------------------------------------------
// AssetFilter - 类型过滤器
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AssetFilter {
    All,
    Models,
    Textures,
    Audio,
    Scenes,
    Avatars,
    Prefabs,
}

impl AssetFilter {
    fn label(&self) -> &str {
        match self {
            AssetFilter::All => "All",
            AssetFilter::Models => "Models",
            AssetFilter::Textures => "Textures",
            AssetFilter::Audio => "Audio",
            AssetFilter::Scenes => "Scenes",
            AssetFilter::Avatars => "Avatars",
            AssetFilter::Prefabs => "Prefabs",
        }
    }

    fn matches(&self, asset_type: AssetType) -> bool {
        match self {
            AssetFilter::All => true,
            AssetFilter::Models => matches!(asset_type, AssetType::Model),
            AssetFilter::Textures => matches!(asset_type, AssetType::Texture),
            AssetFilter::Audio => matches!(asset_type, AssetType::Audio),
            AssetFilter::Scenes => matches!(asset_type, AssetType::Scene),
            AssetFilter::Avatars => matches!(asset_type, AssetType::Avatar),
            AssetFilter::Prefabs => matches!(asset_type, AssetType::Prefab),
        }
    }
}

// ---------------------------------------------------------------------------
// ViewMode - 视图模式
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ViewMode {
    List,
    Grid,
}

// ---------------------------------------------------------------------------
// AssetBrowser
// ---------------------------------------------------------------------------

/// 资源浏览器面板（类 Unity Project 面板）。
///
/// 双栏布局：左侧文件夹树，右侧资源内容区。
pub struct AssetBrowser {
    /// 当前浏览的目录路径（如 "assets/models"）
    current_path: String,
    /// 当前目录下的条目（文件 + 子文件夹）
    entries: Vec<AssetEntry>,
    /// 文件夹树（根节点为 "assets"）
    folder_tree: FolderNode,
    /// 各文件夹路径的展开状态
    folder_expanded: HashMap<String, bool>,
    /// 类型过滤器
    filter: AssetFilter,
    /// 选中条目索引（在 entries 中的位置）
    selected_index: Option<usize>,
    /// 视图模式
    view_mode: ViewMode,
    /// 搜索文本
    search_text: String,
    /// 左侧文件夹面板宽度占比（0.0 ~ 1.0）
    folder_split_ratio: f32,
}

impl AssetBrowser {
    pub fn new() -> Self {
        Self {
            current_path: "assets".into(),
            entries: Vec::new(),
            folder_tree: FolderNode::new("assets".into()),
            folder_expanded: HashMap::new(),
            filter: AssetFilter::All,
            selected_index: None,
            view_mode: ViewMode::Grid,
            search_text: String::new(),
            folder_split_ratio: 0.28,
        }
    }

    /// 从 AssetDatabase 扫描当前目录，填充 entries 并构建文件夹树。
    pub fn scan_directory(&mut self, database: &AssetDatabase) {
        self.entries.clear();
        self.selected_index = None;

        let project_root = database.project_root();

        // 构建文件夹树（基于文件系统扫描 assets/ 下的所有目录）
        self.build_folder_tree(project_root);

        // 获取当前目录下的直接子条目（来自 AssetDatabase）
        let db_entries = database.entries_in_directory(&self.current_path);
        for db_entry in db_entries {
            let name = std::path::Path::new(&db_entry.path)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("")
                .to_string();

            let asset_type = AssetType::from_kind(db_entry.asset_type);
            self.entries.push(AssetEntry {
                name,
                path: db_entry.path.clone(),
                uuid: db_entry.uuid.clone(),
                asset_type,
                size_bytes: db_entry.file_size,
            });
        }

        // 同时扫描当前目录下的直接子目录（文件系统层面）
        let dir = format!("{}/{}", project_root.display(), self.current_path);
        if let Ok(read_dir) = std::fs::read_dir(&dir) {
            for entry in read_dir.flatten() {
                let path = entry.path();
                if !path.is_dir() {
                    continue;
                }
                let name = path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("")
                    .to_string();

                if name.starts_with('.') {
                    continue;
                }

                let rel_path = format!("{}/{}", self.current_path, name);
                self.entries.push(AssetEntry {
                    name,
                    path: rel_path,
                    uuid: String::new(),
                    asset_type: AssetType::Folder,
                    size_bytes: 0,
                });
            }
        }

        // 排序：目录在前，文件在后，同类型按名称排序
        self.entries.sort_by(|a, b| {
            match (a.asset_type == AssetType::Folder, b.asset_type == AssetType::Folder) {
                (true, false) => std::cmp::Ordering::Less,
                (false, true) => std::cmp::Ordering::Greater,
                _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
            }
        });

        // 同步展开状态：当前路径上的所有节点设为展开
        self.sync_tree_expansion();
    }

    /// 递归扫描文件系统构建文件夹树。
    fn build_folder_tree(&mut self, project_root: &std::path::Path) {
        let assets_dir = project_root.join("assets");
        if !assets_dir.exists() || !assets_dir.is_dir() {
            return;
        }
        let assets_path = assets_dir.clone();
        let mut root = FolderNode::new("assets".into());
        self.collect_subdirs(&assets_path, "assets", &mut root.children);
        self.folder_tree = root;
    }

    fn collect_subdirs(&self, abs_path: &std::path::Path, rel_path: &str, out: &mut Vec<FolderNode>) {
        let Ok(entries) = std::fs::read_dir(abs_path) else { return };
        let mut dirs: Vec<(String, std::path::PathBuf)> = Vec::new();
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() { continue; }
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("").to_string();
            if name.starts_with('.') { continue; }
            dirs.push((name, path));
        }
        dirs.sort_by(|a, b| a.0.to_lowercase().cmp(&b.0.to_lowercase()));
        for (name, abs) in dirs {
            let child_rel = format!("{}/{}", rel_path, name);
            let mut node = FolderNode::new(child_rel.clone());
            self.collect_subdirs(&abs, &child_rel, &mut node.children);
            out.push(node);
        }
    }

    /// 同步展开状态：当前路径上的所有节点设为展开。
    fn sync_tree_expansion(&mut self) {
        let segments: Vec<&str> = self.current_path.split('/').collect();
        let mut accumulated = String::new();
        for seg in &segments {
            if accumulated.is_empty() {
                accumulated = seg.to_string();
            } else {
                accumulated = format!("{}/{}", accumulated, seg);
            }
            self.folder_expanded.insert(accumulated.clone(), true);
        }
    }

    /// 切换指定路径的文件夹展开/折叠状态。
    fn toggle_folder_expanded(&mut self, path: &str) {
        let entry = self.folder_expanded.entry(path.to_string()).or_insert(false);
        *entry = !*entry;
    }

    /// 获取指定路径的展开状态。
    fn is_expanded(&self, path: &str) -> bool {
        self.folder_expanded.get(path).copied().unwrap_or(false)
    }

    /// 渲染整个文件夹树。
    fn render_folder_tree(&mut self, ui: &mut egui::Ui) {
        let mut to_render: Vec<(String, usize)> = Vec::new();
        let tree = self.folder_tree.clone();
        self.collect_visible_nodes(&tree, 0, &mut to_render);

        for (path, depth) in &to_render {
            let name = std::path::Path::new(path)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or(path)
                .to_string();
            let is_current = path == &self.current_path;

            let has_children = self.folder_has_children(&tree, path);
            let expanded = self.is_expanded(path);

            let arrow = if !has_children {
                "  "
            } else if expanded {
                "\u{25BC}"
            } else {
                "\u{25B6}"
            };

            ui.horizontal(|ui| {
                ui.add_space(*depth as f32 * 14.0);
                let label = format!("{} \u{1F4C1} {}", arrow, name);
                let label_rich = if is_current {
                    egui::RichText::new(label).strong().color(egui::Color32::from_rgb(150, 200, 255))
                } else {
                    egui::RichText::new(label)
                };

                if ui.selectable_label(is_current, label_rich).clicked() {
                    if has_children {
                        self.toggle_folder_expanded(path);
                    }
                    self.navigate_to(path.clone());
                }
            });
        }
    }

    /// 递归收集可见的文件夹节点（DFS，考虑展开状态）。
    fn collect_visible_nodes(&self, node: &FolderNode, depth: usize, out: &mut Vec<(String, usize)>) {
        out.push((node.path.clone(), depth));
        if self.is_expanded(&node.path) {
            for child in &node.children {
                self.collect_visible_nodes(child, depth + 1, out);
            }
        }
    }

    /// 检查指定路径的文件夹是否有子文件夹。
    fn folder_has_children(&self, tree: &FolderNode, path: &str) -> bool {
        if tree.path == path {
            return !tree.children.is_empty();
        }
        for child in &tree.children {
            if self.folder_has_children(child, path) {
                return true;
            }
        }
        false
    }

    /// 导航到指定目录。
    fn navigate_to(&mut self, path: String) {
        self.current_path = path;
        self.search_text.clear();
        self.selected_index = None;
    }

    /// 面包屑路径段。
    fn breadcrumb_segments(&self) -> Vec<(String, String)> {
        let mut segments = Vec::new();
        let mut accumulated = String::new();
        for part in self.current_path.split('/') {
            if accumulated.is_empty() {
                accumulated = part.to_string();
            } else {
                accumulated = format!("{}/{}", accumulated, part);
            }
            segments.push((part.to_string(), accumulated.clone()));
        }
        segments
    }

    fn format_size(bytes: u64) -> String {
        if bytes == 0 {
            return "\u{2014}".into();
        }
        if bytes < 1024 {
            format!("{} B", bytes)
        } else if bytes < 1024 * 1024 {
            format!("{:.1} KB", bytes as f64 / 1024.0)
        } else {
            format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
        }
    }
}

// ---------------------------------------------------------------------------
// EditorPanel 实现
// ---------------------------------------------------------------------------

impl EditorPanel for AssetBrowser {
    fn title(&self) -> &str {
        "Project"
    }

    fn show(&mut self, ui: &mut egui::Ui, state: &mut EditorState) {
        // ---- 标题栏 ----
        ui.horizontal(|ui| {
            ui.strong("Project");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let list_selected = self.view_mode == ViewMode::List;
                if ui.selectable_label(list_selected, "\u{2630}").clicked() {
                    self.view_mode = ViewMode::List;
                }
                if ui.selectable_label(!list_selected, "\u{25A6}").clicked() {
                    self.view_mode = ViewMode::Grid;
                }
            });
        });

        ui.add_space(2.0);
        ui.separator();

        // ---- 双栏布局 ----
        let available = ui.available_size();
        let folder_width = (available.x * self.folder_split_ratio).max(120.0).min(available.x * 0.5);
        let content_width = available.x - folder_width - 6.0;

        ui.horizontal(|ui| {
            // ---- 左侧：文件夹树 ----
            ui.vertical(|ui| {
                ui.set_width(folder_width);
                // 使用 set_min_height 而非 set_height：
                // set_height 同时设置 min+max，会将 max_rect 缩小到 available.y-4，
                // 导致 Frame 返回的 rect 比 panel_rect 小 4px（inner_margin），
                // PanelState 存储这个偏小值后每帧递减，面板缓慢缩小。
                // set_min_height 只扩展 min_rect 不约束 max_rect，避免反馈循环。
                ui.set_min_height(available.y);

                ui.add_space(2.0);
                ui.strong("\u{1F4C2} Assets");
                ui.add_space(2.0);
                ui.separator();

                egui::ScrollArea::vertical()
                    .id_salt("folder_tree_scroll")
                    .show(ui, |ui| {
                        self.render_folder_tree(ui);
                    });
            });

            ui.separator();

            // ---- 右侧：内容区 ----
            ui.vertical(|ui| {
                ui.set_width(content_width);
                ui.set_min_height(available.y);

                // 面包屑导航
                self.render_breadcrumb(ui);
                ui.add_space(2.0);

                // 搜索栏 + 过滤器
                self.render_toolbar(ui);
                ui.add_space(2.0);
                ui.separator();

                // 资源内容区
                self.render_content(ui, state);
            });
        });

        // ---- 拖拽预览浮层 ----
        self.render_drag_preview(ui, state);
    }
}

// ---------------------------------------------------------------------------
// 渲染辅助方法
// ---------------------------------------------------------------------------

impl AssetBrowser {
    /// 渲染面包屑导航。
    fn render_breadcrumb(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            let segments = self.breadcrumb_segments();
            for (i, (name, path)) in segments.iter().enumerate() {
                if i > 0 {
                    ui.label(egui::RichText::new(" > ").color(egui::Color32::GRAY));
                }
                if ui.link(name).clicked() {
                    self.navigate_to(path.clone());
                }
            }
        });
    }

    /// 渲染搜索栏。
    fn render_toolbar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label("\u{1F50D}");
            let search_response = ui.add(
                egui::TextEdit::singleline(&mut self.search_text)
                    .hint_text("Search assets...")
                    .desired_width(200.0),
            );
            if search_response.changed() {
                self.selected_index = None;
            }
        });
    }

    /// 渲染资源内容区（列表或网格视图）。
    fn render_content(&mut self, ui: &mut egui::Ui, state: &mut EditorState) {
        let filtered: Vec<AssetEntry> = self.entries
            .iter()
            .filter(|e| {
                let search_match = self.search_text.is_empty()
                    || e.name.to_lowercase().contains(&self.search_text.to_lowercase());
                search_match
            })
            .cloned()
            .collect();

        let item_count = filtered.len();
        let view_mode = self.view_mode;
        let selected_index = self.selected_index;

        egui::ScrollArea::vertical()
            .id_salt("asset_browser_content")
            .show(ui, |ui| {
                match view_mode {
                    ViewMode::List => {
                        Self::render_list_view_static(ui, &filtered, selected_index, state);
                    }
                    ViewMode::Grid => {
                        Self::render_grid_view_static(ui, &filtered, selected_index, state);
                    }
                }
            });

        // 底部状态栏
        ui.add_space(2.0);
        ui.separator();
        ui.horizontal(|ui| {
            ui.label(format!("{} items", item_count));
            if let Some(idx) = self.selected_index {
                if let Some(entry) = self.entries.get(idx) {
                    if !entry.uuid.is_empty() {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.label(
                                egui::RichText::new(format!("UUID: {} | {}", entry.uuid, Self::format_size(entry.size_bytes)))
                                    .monospace()
                                    .size(10.0)
                                    .color(egui::Color32::GRAY),
                            );
                        });
                    }
                }
            }
        });
    }

    /// 列表视图（静态方法，避免借用冲突）。
    fn render_list_view_static(ui: &mut egui::Ui, filtered: &[AssetEntry], _selected_index: Option<usize>, state: &mut EditorState) {
        let available_width = ui.available_width();
        egui::Grid::new("asset_list_grid")
            .striped(true)
            .min_col_width(available_width * 0.45)
            .show(ui, |ui| {
                for entry in filtered.iter() {
                    let label = format!("{}  {}", entry.asset_type.icon(), entry.name);
                    let response = ui.selectable_label(false, label);

                    if response.drag_started()
                        && entry.asset_type.is_draggable()
                        && !entry.uuid.is_empty()
                    {
                        state.dragged_asset_uuid = Some(entry.uuid.clone());
                        state.dragged_asset_type = Some(entry.asset_type);
                        state.dragged_asset_name = Some(entry.name.clone());
                        state.drag_source = Some("AssetBrowser".to_string());
                    }

                    ui.label(Self::format_size(entry.size_bytes));
                    if !entry.uuid.is_empty() {
                        let short_uuid = &entry.uuid[..8.min(entry.uuid.len())];
                        ui.label(
                            egui::RichText::new(short_uuid)
                                .monospace()
                                .size(10.0)
                                .color(egui::Color32::GRAY),
                        );
                    } else {
                        ui.label("");
                    }
                    ui.end_row();
                }
            });
    }

    /// 网格视图（静态方法，避免借用冲突）。
    fn render_grid_view_static(ui: &mut egui::Ui, filtered: &[AssetEntry], _selected_index: Option<usize>, state: &mut EditorState) {
        let card_width = 96.0;
        let available = ui.available_width();
        let cols = (available / (card_width + 8.0)).max(1.0) as usize;

        for row_start in (0..filtered.len()).step_by(cols) {
            let row_end = (row_start + cols).min(filtered.len());
            ui.horizontal(|ui| {
                for idx in row_start..row_end {
                    let entry = &filtered[idx];

                    let (fill, stroke) = (
                        egui::Color32::TRANSPARENT,
                        egui::Stroke::new(1.0, egui::Color32::from_rgb(60, 60, 80)),
                    );

                    let resp = egui::Frame::none()
                        .fill(fill)
                        .stroke(stroke)
                        .rounding(egui::Rounding::same(4.0))
                        .inner_margin(egui::Margin::same(4.0))
                        .show(ui, |ui| {
                            ui.set_width(card_width);
                            ui.vertical_centered(|ui| {
                                ui.add_space(4.0);
                                ui.label(
                                    egui::RichText::new(entry.asset_type.icon())
                                        .size(28.0),
                                );
                                ui.add_space(2.0);
                                let display_name = if entry.name.len() > 14 {
                                    format!("{}\u{2026}", &entry.name[..13])
                                } else {
                                    entry.name.clone()
                                };
                                ui.label(
                                    egui::RichText::new(display_name)
                                        .size(10.0),
                                );
                                if entry.asset_type != AssetType::Folder && entry.size_bytes > 0 {
                                    ui.label(
                                        egui::RichText::new(Self::format_size(entry.size_bytes))
                                            .size(9.0)
                                            .color(egui::Color32::GRAY),
                                    );
                                }
                            });
                        });

                    if resp.response.drag_started()
                        && entry.asset_type.is_draggable()
                        && !entry.uuid.is_empty()
                    {
                        state.dragged_asset_uuid = Some(entry.uuid.clone());
                        state.dragged_asset_type = Some(entry.asset_type);
                        state.dragged_asset_name = Some(entry.name.clone());
                        state.drag_source = Some("AssetBrowser".to_string());
                    }
                }
            });
        }
    }

    /// 渲染拖拽预览浮层。
    fn render_drag_preview(&mut self, ui: &mut egui::Ui, state: &mut EditorState) {
        let drag_active = state.dragged_asset_uuid.is_some()
            && state.drag_source.as_deref() == Some("AssetBrowser");

        if drag_active {
            ui.ctx().request_repaint();
            let cancel = ui.input(|input| {
                input.key_pressed(egui::Key::Escape)
                    || input.pointer.button_clicked(egui::PointerButton::Secondary)
            });
            if cancel {
                state.dragged_asset_uuid = None;
                state.dragged_asset_type = None;
                state.dragged_asset_name = None;
                state.drag_source = None;
                state.drop_target_hint = None;
                return;
            }

            if let Some(mouse_pos) = ui.input(|input| input.pointer.hover_pos()) {
                let preview_label = state
                    .dragged_asset_name
                    .as_deref()
                    .unwrap_or("Asset");
                let icon_str = match state.dragged_asset_type {
                    Some(AssetType::Model) => "\u{1F537}",
                    Some(AssetType::Prefab) => "\u{1F4E6}",
                    _ => "\u{1F4E6}",
                };
                let label = format!("{} {}", icon_str, preview_label);
                egui::Area::new("drag_preview".into())
                    .fixed_pos(mouse_pos + egui::vec2(12.0, 12.0))
                    .order(egui::Order::Foreground)
                    .interactable(false)
                    .show(ui.ctx(), |ui| {
                        ui.label(
                            egui::RichText::new(label)
                                .size(12.0)
                                .background_color(
                                    egui::Color32::from_rgba_premultiplied(40, 40, 60, 220),
                                ),
                        );
                    });
            }
        }
    }
}
