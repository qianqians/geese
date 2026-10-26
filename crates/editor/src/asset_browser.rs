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

    /// 从文件名推断资源类型（无 .meta 信息时使用）。
    fn from_filename(name: &str) -> Self {
        Self::from_kind(AssetTypeKind::from_filename(name))
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

    /// 工具栏下拉框展示的全部选项。
    fn all() -> &'static [AssetFilter] {
        &[
            AssetFilter::All,
            AssetFilter::Models,
            AssetFilter::Textures,
            AssetFilter::Audio,
            AssetFilter::Scenes,
            AssetFilter::Avatars,
            AssetFilter::Prefabs,
        ]
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
    /// 选中条目的路径（相对项目根目录，如 "assets/scenes/default.scene.json"）
    selected_path: Option<String>,
    /// 视图模式
    view_mode: ViewMode,
    /// 搜索文本
    search_text: String,
    /// 左侧文件夹面板宽度占比（0.0 ~ 1.0）
    folder_split_ratio: f32,
    /// 导航后需要重新列出当前目录的标志
    path_dirty: bool,
    /// 是否请求全量重扫 AssetDatabase（外部新增文件需要生成 .meta）
    db_refresh_requested: bool,
    /// 当前目录读取失败时的错误信息（供 UI 提示）
    error: Option<String>,
}

impl AssetBrowser {
    pub fn new() -> Self {
        Self {
            current_path: "assets".into(),
            entries: Vec::new(),
            folder_tree: FolderNode::new("assets".into()),
            folder_expanded: HashMap::new(),
            filter: AssetFilter::All,
            selected_path: None,
            view_mode: ViewMode::Grid,
            search_text: String::new(),
            folder_split_ratio: 0.28,
            path_dirty: false,
            db_refresh_requested: false,
            error: None,
        }
    }

    /// 是否需要重新列出当前目录。
    pub fn needs_rescan(&self) -> bool {
        self.path_dirty
    }

    /// 标记当前目录需要重新列出（不影响资源数据库）。
    pub fn mark_dirty(&mut self) {
        self.path_dirty = true;
    }

    /// 清除导航脏标志（在重新扫描后调用）。
    pub fn clear_dirty(&mut self) {
        self.path_dirty = false;
    }

    /// 是否请求全量重扫 AssetDatabase。
    pub fn needs_db_refresh(&self) -> bool {
        self.db_refresh_requested
    }

    /// 清除数据库重扫请求（在重扫后调用）。
    pub fn clear_db_refresh(&mut self) {
        self.db_refresh_requested = false;
    }

    /// 扫描当前目录，填充 entries（内容区）并重建文件夹树。
    ///
    /// 条目以文件系统为准：`assets/<current_path>` 下的直接子目录与文件都会列出，
    /// 再从 [`AssetDatabase`] 补齐 UUID / 资源类型 / 大小等元信息。
    /// 这样即使数据库还没扫描到新文件（或 `.meta` 生成失败），目录内容也能正常显示。
    pub fn scan_directory(&mut self, database: &AssetDatabase) {
        self.entries.clear();
        self.selected_path = None;
        self.error = None;

        let project_root = database.project_root().to_path_buf();

        // 1. 重建文件夹树（直接读文件系统，新增目录无需等数据库刷新）
        self.build_folder_tree(&project_root);

        // 2. 解析当前目录的绝对路径；不存在时回退到 assets 根目录
        let mut dir = project_root.join(&self.current_path);
        if !dir.is_dir() {
            let assets_dir = project_root.join("assets");
            if assets_dir.is_dir() {
                self.current_path = "assets".to_string();
                dir = assets_dir;
            } else {
                self.error = Some(format!("目录不存在: {}", dir.display()));
                self.sync_tree_expansion();
                return;
            }
        }

        // 3. 列出当前目录下的直接子目录与文件
        let read_dir = match std::fs::read_dir(&dir) {
            Ok(rd) => rd,
            Err(e) => {
                self.error = Some(format!("无法读取目录 {}: {e}", dir.display()));
                self.sync_tree_expansion();
                return;
            }
        };

        let mut folders: Vec<AssetEntry> = Vec::new();
        let mut files: Vec<AssetEntry> = Vec::new();

        for entry in read_dir.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();

            // 跳过隐藏文件与 .meta 伴生文件
            if name.starts_with('.') || name.ends_with(".meta") {
                continue;
            }

            let abs_path = entry.path();
            let rel_path = format!("{}/{}", self.current_path, name);

            if abs_path.is_dir() {
                folders.push(AssetEntry {
                    name,
                    path: rel_path,
                    uuid: String::new(),
                    asset_type: AssetType::Folder,
                    size_bytes: 0,
                });
            } else {
                let db_entry = database.entry_by_path(&rel_path);
                let asset_type = db_entry
                    .map(|e| AssetType::from_kind(e.asset_type))
                    .unwrap_or_else(|| AssetType::from_filename(&name));
                let size_bytes = db_entry
                    .map(|e| e.file_size)
                    .filter(|size| *size > 0)
                    .or_else(|| entry.metadata().ok().map(|m| m.len()))
                    .unwrap_or(0);
                let uuid = db_entry.map(|e| e.uuid.clone()).unwrap_or_default();

                files.push(AssetEntry {
                    name,
                    path: rel_path,
                    uuid,
                    asset_type,
                    size_bytes,
                });
            }
        }

        // 排序：目录在前，文件在后，同类型按名称排序
        folders.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
        files.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
        self.entries = folders;
        self.entries.extend(files);

        // 4. 同步展开状态：当前路径上的所有节点设为展开
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
                    // 选中行已有高亮底色，文字沿用主题的选中描边色，避免与底色同色不可见
                    egui::RichText::new(label)
                        .strong()
                        .color(ui.visuals().selection.stroke.color)
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
        if self.current_path == path {
            return;
        }
        self.current_path = path;
        self.search_text.clear();
        self.selected_path = None;
        self.path_dirty = true;
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

    /// 渲染搜索栏 + 类型过滤 + 刷新。
    fn render_toolbar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label("\u{1F50D}");
            let search_response = ui.add(
                egui::TextEdit::singleline(&mut self.search_text)
                    .hint_text("Search assets...")
                    .desired_width(180.0),
            );
            if search_response.changed() {
                self.selected_path = None;
            }

            ui.separator();

            // 类型过滤器
            egui::ComboBox::from_id_salt("asset_filter_combo")
                .selected_text(self.filter.label())
                .width(86.0)
                .show_ui(ui, |ui| {
                    for filter in AssetFilter::all() {
                        if ui
                            .selectable_label(self.filter == *filter, filter.label())
                            .clicked()
                        {
                            self.filter = *filter;
                            self.selected_path = None;
                        }
                    }
                });

            // 刷新：重新列出当前目录，并请求资源数据库全量重扫
            if ui
                .button("Refresh")
                .on_hover_text("重新扫描当前目录（并刷新资源数据库）")
                .clicked()
            {
                self.path_dirty = true;
                self.db_refresh_requested = true;
            }
        });
    }

    /// 渲染资源内容区（列表或网格视图）——显示当前目录下的子目录与文件。
    fn render_content(&mut self, ui: &mut egui::Ui, state: &mut EditorState) {
        // 过滤：搜索文本 + 类型过滤（文件夹始终显示，便于继续向下导航）
        let needle = self.search_text.to_lowercase();
        let filtered: Vec<AssetEntry> = self
            .entries
            .iter()
            .filter(|e| {
                let type_match = e.asset_type == AssetType::Folder
                    || self.filter.matches(e.asset_type);
                let search_match = needle.is_empty() || e.name.to_lowercase().contains(&needle);
                type_match && search_match
            })
            .cloned()
            .collect();

        let item_count = filtered.len();
        let view_mode = self.view_mode;
        let selected_path = self.selected_path.clone();

        // 空目录 / 读取失败 提示
        if filtered.is_empty() {
            ui.add_space(6.0);
            let (text, color) = if let Some(err) = &self.error {
                (err.clone(), egui::Color32::from_rgb(220, 120, 120))
            } else if self.entries.is_empty() {
                ("当前目录为空".to_string(), egui::Color32::GRAY)
            } else {
                ("没有匹配的资源".to_string(), egui::Color32::GRAY)
            };
            ui.label(egui::RichText::new(text).color(color));
            ui.add_space(6.0);
        }

        // 渲染视图，收集点击结果（闭包里不能借用 &mut self）
        let mut clicked: Option<(String, bool)> = None;
        egui::ScrollArea::vertical()
            .id_salt("asset_browser_content")
            .show(ui, |ui| {
                let hit = match view_mode {
                    ViewMode::List => {
                        self.render_list_view(ui, &filtered, selected_path.as_deref(), state)
                    }
                    ViewMode::Grid => {
                        self.render_grid_view(ui, &filtered, selected_path.as_deref(), state)
                    }
                };
                if hit.is_some() {
                    clicked = hit;
                }
            });

        // 单击选中条目；双击文件夹进入该目录
        if let Some((path, double_clicked)) = clicked {
            let is_folder = self
                .entries
                .iter()
                .any(|e| e.path == path && e.asset_type == AssetType::Folder);
            if is_folder && double_clicked {
                self.navigate_to(path);
            } else {
                self.selected_path = Some(path);
            }
        }

        // 底部状态栏
        ui.add_space(2.0);
        ui.separator();
        ui.horizontal(|ui| {
            ui.label(format!(
                "{} {}",
                item_count,
                if item_count == 1 { "item" } else { "items" }
            ));
            ui.separator();
            ui.label(
                egui::RichText::new(&self.current_path)
                    .monospace()
                    .size(10.0)
                    .color(egui::Color32::GRAY),
            );

            if let Some(entry) = self
                .selected_path
                .as_ref()
                .and_then(|path| self.entries.iter().find(|e| &e.path == path))
            {
                let mut info = entry.name.clone();
                if entry.asset_type != AssetType::Folder {
                    info.push_str(&format!(" | {}", Self::format_size(entry.size_bytes)));
                }
                if !entry.uuid.is_empty() {
                    info.push_str(&format!(" | UUID: {}", &entry.uuid[..8.min(entry.uuid.len())]));
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(
                        egui::RichText::new(info)
                            .monospace()
                            .size(10.0)
                            .color(egui::Color32::GRAY),
                    );
                });
            }
        });
    }

    /// 列表视图；返回被点击条目的路径与是否双击。
    fn render_list_view(
        &self,
        ui: &mut egui::Ui,
        filtered: &[AssetEntry],
        selected_path: Option<&str>,
        state: &mut EditorState,
    ) -> Option<(String, bool)> {
        let mut clicked = None;
        let available_width = ui.available_width();
        egui::Grid::new("asset_list_grid")
            .striped(true)
            .min_col_width(available_width * 0.45)
            .show(ui, |ui| {
                for entry in filtered.iter() {
                    let is_selected = selected_path == Some(entry.path.as_str());
                    let label = format!("{}  {}", entry.asset_type.icon(), entry.name);
                    // 用带 click_and_drag 语义的 Button 同时支持“选中”和“拖进场景”
                    let response = ui.add(
                        egui::Button::new(label)
                            .selected(is_selected)
                            .sense(egui::Sense::click_and_drag()),
                    );

                    if response.clicked() || response.double_clicked() {
                        clicked = Some((entry.path.clone(), response.double_clicked()));
                    }

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
        clicked
    }

    /// 网格视图；返回被点击条目的路径与是否双击。
    fn render_grid_view(
        &self,
        ui: &mut egui::Ui,
        filtered: &[AssetEntry],
        selected_path: Option<&str>,
        state: &mut EditorState,
    ) -> Option<(String, bool)> {
        let mut clicked = None;
        let card_width = 96.0;
        let available = ui.available_width();
        let cols = (available / (card_width + 8.0)).max(1.0) as usize;

        for row_start in (0..filtered.len()).step_by(cols) {
            let row_end = (row_start + cols).min(filtered.len());
            ui.horizontal(|ui| {
                for idx in row_start..row_end {
                    let entry = &filtered[idx];
                    let is_selected = selected_path == Some(entry.path.as_str());

                    let stroke = if is_selected {
                        egui::Stroke::new(1.5, egui::Color32::from_rgb(120, 170, 255))
                    } else {
                        egui::Stroke::new(1.0, egui::Color32::from_rgb(60, 60, 80))
                    };

                    let resp = egui::Frame::none()
                        .fill(egui::Color32::TRANSPARENT)
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
                                let display_name = if entry.name.chars().count() > 14 {
                                    format!("{}\u{2026}", entry.name.chars().take(13).collect::<String>())
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

                    // 卡片内容不可交互，用覆盖层统一处理选中与拖拽
                    let card_id = egui::Id::new(("asset_card", entry.path.as_str()));
                    let card_response = ui.interact(
                        resp.response.rect,
                        card_id,
                        egui::Sense::click_and_drag(),
                    );

                    if card_response.clicked() || card_response.double_clicked() {
                        clicked = Some((entry.path.clone(), card_response.double_clicked()));
                    }

                    if card_response.drag_started()
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
        clicked
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

// ---------------------------------------------------------------------------
// 测试
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// 构造一个临时项目：assets/{models,scenes,empty} + 若干文件
    ///
    /// 目录放在 crate 的 `target/test_tmp/` 下（不用系统临时目录，避免沙箱/权限问题）。
    fn temp_project(tag: &str) -> PathBuf {
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join("test_tmp")
            .join(format!("{}_{}", tag, std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("assets/models")).unwrap();
        std::fs::create_dir_all(dir.join("assets/scenes")).unwrap();
        std::fs::create_dir_all(dir.join("assets/empty")).unwrap();
        std::fs::write(dir.join("assets/models/hero.glb"), b"fake glb").unwrap();
        std::fs::write(dir.join("assets/models/hero.glb.meta"), b"{}").unwrap();
        std::fs::write(dir.join("assets/readme.txt"), b"hello").unwrap();
        std::fs::write(dir.join("assets/scenes/main.scene.json"), b"{}").unwrap();
        dir
    }

    fn browser_for(dir: &PathBuf) -> AssetBrowser {
        // 空数据库：验证内容区以文件系统为准，不依赖 .meta / 数据库索引
        let db = AssetDatabase::new_empty(dir.to_str().unwrap());
        let mut browser = AssetBrowser::new();
        browser.scan_directory(&db);
        browser
    }

    #[test]
    fn root_directory_lists_files_and_folders() {
        let dir = temp_project("root");
        let browser = browser_for(&dir);

        assert_eq!(browser.current_path, "assets");

        let names: Vec<&str> = browser.entries.iter().map(|e| e.name.as_str()).collect();
        assert!(names.contains(&"models"), "缺少子目录: {names:?}");
        assert!(names.contains(&"scenes"), "缺少子目录: {names:?}");
        assert!(names.contains(&"readme.txt"), "缺少文件: {names:?}");
        assert!(!names.iter().any(|n| n.ends_with(".meta")), ".meta 不应显示: {names:?}");

        // 目录排在文件之前
        let first_file = browser
            .entries
            .iter()
            .position(|e| e.asset_type != AssetType::Folder)
            .unwrap();
        assert!(browser.entries[..first_file]
            .iter()
            .all(|e| e.asset_type == AssetType::Folder));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn subdirectory_lists_its_own_files() {
        let dir = temp_project("sub");
        let db = AssetDatabase::new_empty(dir.to_str().unwrap());
        let mut browser = AssetBrowser::new();

        browser.current_path = "assets/models".to_string();
        browser.scan_directory(&db);

        let names: Vec<&str> = browser.entries.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, vec!["hero.glb"]);

        // 类型从文件名推断（数据库为空时也能显示正确的资源类型）
        assert_eq!(browser.entries[0].asset_type, AssetType::Model);
        assert_eq!(browser.entries[0].path, "assets/models/hero.glb");
        assert!(browser.entries[0].size_bytes > 0);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn empty_directory_shows_nothing_and_keeps_path() {
        let dir = temp_project("empty");
        let db = AssetDatabase::new_empty(dir.to_str().unwrap());
        let mut browser = AssetBrowser::new();

        browser.current_path = "assets/empty".to_string();
        browser.scan_directory(&db);

        assert!(browser.entries.is_empty());
        assert!(browser.error.is_none());
        assert_eq!(browser.current_path, "assets/empty");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn missing_directory_falls_back_to_assets_root() {
        let dir = temp_project("missing");
        let db = AssetDatabase::new_empty(dir.to_str().unwrap());
        let mut browser = AssetBrowser::new();

        browser.current_path = "assets/not_exists".to_string();
        browser.scan_directory(&db);

        assert_eq!(browser.current_path, "assets");
        assert!(!browser.entries.is_empty());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn navigation_marks_dirty_and_double_click_enters_folder() {
        let dir = temp_project("nav");
        let browser = browser_for(&dir);

        // 根目录下能定位到 models 文件夹条目
        let models = browser
            .entries
            .iter()
            .find(|e| e.name == "models")
            .expect("缺少 models 目录")
            .clone();
        assert_eq!(models.asset_type, AssetType::Folder);
        assert_eq!(models.path, "assets/models");

        let mut browser = browser;
        browser.navigate_to(models.path.clone());
        assert!(browser.needs_rescan());
        assert_eq!(browser.current_path, "assets/models");

        // 同一路径重复导航不再触发重扫
        browser.clear_dirty();
        browser.navigate_to(models.path.clone());
        assert!(!browser.needs_rescan());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn filter_only_keeps_matching_type() {
        let dir = temp_project("filter");
        let mut browser = browser_for(&dir);

        browser.filter = AssetFilter::Models;
        let needle = browser.search_text.to_lowercase();
        let filtered: Vec<&AssetEntry> = browser
            .entries
            .iter()
            .filter(|e| {
                (e.asset_type == AssetType::Folder || browser.filter.matches(e.asset_type))
                    && (needle.is_empty() || e.name.to_lowercase().contains(&needle))
            })
            .collect();

        // 文件夹始终保留，模型文件保留，其它类型被过滤
        assert!(filtered.iter().any(|e| e.name == "models"));
        assert!(filtered.iter().any(|e| e.name == "readme.txt") == false);
        assert!(filtered.iter().any(|e| e.name == "main.scene.json") == false);

        let _ = std::fs::remove_dir_all(&dir);
    }
}
