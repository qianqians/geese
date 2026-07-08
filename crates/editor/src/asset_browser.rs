//! 资源浏览器（Asset Browser）。
//!
//! 浏览项目 assets 目录，支持按类型过滤和拖拽导入。
//! 数据源从 AssetDatabase 获取，而非直接扫描文件系统。

use crate::panels::{EditorPanel, EditorState};
use asset::database::AssetDatabase;
use asset::meta::AssetTypeKind;

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
            AssetType::Folder => "📁",
            AssetType::Scene => "🎬",
            AssetType::Model => "🔷",
            AssetType::Texture => "🖼",
            AssetType::Audio => "🔊",
            AssetType::Avatar => "🧑",
            AssetType::Prefab => "📦",
            AssetType::Other => "📄",
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

    #[allow(dead_code)]
    fn from_extension(ext: &str) -> Self {
        match ext.to_lowercase().as_str() {
            "gltf" | "glb" => AssetType::Model,
            "png" | "jpg" | "jpeg" | "hdr" | "exr" | "ktx2" => AssetType::Texture,
            "wav" | "ogg" | "mp3" | "flac" => AssetType::Audio,
            "geese" | "scene" => AssetType::Scene,
            _ => AssetType::Other,
        }
    }

    #[allow(dead_code)]
    /// 根据文件名判断资源类型（支持复合后缀如 `.scene.json`、`.avatar.json`、`.prefab.json`）。
    fn from_filename(name: &str) -> Self {
        let lower = name.to_lowercase();
        if lower.ends_with(".scene.json") {
            AssetType::Scene
        } else if lower.ends_with(".avatar.json") {
            AssetType::Avatar
        } else if lower.ends_with(".prefab.json") {
            AssetType::Prefab
        } else if let Some(ext) = std::path::Path::new(name).extension().and_then(|e| e.to_str()) {
            Self::from_extension(ext)
        } else {
            AssetType::Other
        }
    }
}

// ---------------------------------------------------------------------------
// AssetBrowser
// ---------------------------------------------------------------------------

/// 资源浏览器面板。
pub struct AssetBrowser {
    /// 当前浏览的目录路径
    current_path: String,
    /// 当前目录下的条目
    entries: Vec<AssetEntry>,
    /// 类型过滤器
    filter: AssetFilter,
    /// 选中条目索引
    selected_index: Option<usize>,
    /// 视图模式
    view_mode: ViewMode,
}

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ViewMode {
    List,
    Grid,
}

impl AssetBrowser {
    pub fn new() -> Self {
        Self {
            current_path: "assets".into(),
            entries: Vec::new(),
            filter: AssetFilter::All,
            selected_index: None,
            view_mode: ViewMode::List,
        }
    }

    /// 从 AssetDatabase 扫描当前目录，填充 entries。
    pub fn scan_directory(&mut self, database: &AssetDatabase) {
        self.entries.clear();
        self.selected_index = None;

        // 获取当前目录下的直接子条目
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

        // 同时扫描子目录（文件系统层面）
        let dir = format!("{}/{}", database.project_root().display(), self.current_path);
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
                    uuid: String::new(), // 目录没有 UUID
                    asset_type: AssetType::Folder,
                    size_bytes: 0,
                });
            }
        }

        // 目录在前，文件在后
        self.entries.sort_by(|a, b| {
            match (a.asset_type == AssetType::Folder, b.asset_type == AssetType::Folder) {
                (true, false) => std::cmp::Ordering::Less,
                (false, true) => std::cmp::Ordering::Greater,
                _ => a.name.cmp(&b.name),
            }
        });
    }

    fn format_size(bytes: u64) -> String {
        if bytes == 0 {
            return "—".into();
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

impl EditorPanel for AssetBrowser {
    fn title(&self) -> &str {
        "Asset Browser"
    }

    fn show(&mut self, ui: &mut egui::Ui, state: &mut EditorState) {
        // 标题栏
        ui.horizontal(|ui| {
            ui.strong("Assets");

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                // 视图切换
                let list_icon = if self.view_mode == ViewMode::List { "📋" } else { "▦" };
                if ui.button(list_icon).clicked() {
                    self.view_mode = match self.view_mode {
                        ViewMode::List => ViewMode::Grid,
                        ViewMode::Grid => ViewMode::List,
                    };
                }
            });
        });

        ui.add_space(4.0);

        // 路径导航栏
        ui.horizontal(|ui| {
            if ui.button("📁").on_hover_text("Go to project root").clicked() {
                self.current_path = "assets".into();
            }
            ui.label(format!("📂 {}", self.current_path));
        });

        ui.add_space(4.0);

        // 过滤器
        ui.horizontal(|ui| {
            for filter in &[AssetFilter::All, AssetFilter::Models, AssetFilter::Textures, AssetFilter::Audio, AssetFilter::Scenes, AssetFilter::Avatars, AssetFilter::Prefabs] {
                let selected = self.filter == *filter;
                if ui.selectable_label(selected, filter.label()).clicked() {
                    self.filter = *filter;
                }
            }
        });

        ui.add_space(4.0);
        ui.separator();

        // 资源列表
        let filtered: Vec<&AssetEntry> = self.entries
            .iter()
            .filter(|e| self.filter.matches(e.asset_type) || e.asset_type == AssetType::Folder)
            .collect();

        egui::ScrollArea::vertical()
            .id_salt("asset_browser_scroll")
            .show(ui, |ui| {
                match self.view_mode {
                    ViewMode::List => {
                        egui::Grid::new("asset_grid")
                            .striped(true)
                            .show(ui, |ui| {
                                for (i, entry) in filtered.iter().enumerate() {
                                    let is_selected = self.selected_index == Some(i);
                                    let label = format!(
                                        "{} {}",
                                        entry.asset_type.icon(),
                                        entry.name
                                    );
                                    let response = ui.selectable_label(is_selected, label);
                                    if response.clicked() {
                                        self.selected_index = Some(i);
                                    }
                                    // 拖拽启动：仅 Model/Prefab 可拖拽，文件夹不拖拽
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
                                    // 显示 UUID（截断）
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
                    ViewMode::Grid => {
                        // 网格视图：每行放多个缩略图卡片
                        let card_width = 100.0;
                        let available = ui.available_width();
                        let cols = (available / (card_width + 8.0)).max(1.0) as usize;

                        // 按行分组渲染
                        for row_start in (0..filtered.len()).step_by(cols) {
                            let row_end = (row_start + cols).min(filtered.len());
                            ui.horizontal(|ui| {
                                for idx in row_start..row_end {
                                    let entry = &filtered[idx];
                                    let is_selected = self.selected_index == Some(idx);
                                    let (fill, stroke) = if is_selected {
                                        (
                                            egui::Color32::from_rgb(40, 60, 100),
                                            egui::Stroke::new(2.0, egui::Color32::from_rgb(100, 160, 255)),
                                        )
                                    } else {
                                        (
                                            egui::Color32::TRANSPARENT,
                                            egui::Stroke::new(1.0, egui::Color32::from_rgb(60, 60, 80)),
                                        )
                                    };

                                    let resp = egui::Frame::none()
                                        .fill(fill)
                                        .stroke(stroke)
                                        .rounding(egui::Rounding::same(4.0))
                                        .inner_margin(egui::Margin::same(4.0))
                                        .show(ui, |ui| {
                                            ui.set_width(card_width);
                                            ui.vertical_centered(|ui| {
                                                ui.label(egui::RichText::new(entry.asset_type.icon()).size(24.0));
                                                ui.label(egui::RichText::new(&entry.name).size(10.0));
                                            });
                                        });

                                    if resp.response.clicked() {
                                        self.selected_index = Some(idx);
                                    }
                                    // 拖拽启动：仅 Model/Prefab 可拖拽
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
                }
            });

        // 底部信息栏
        ui.add_space(4.0);
        ui.separator();
        ui.label(format!("{} items", filtered.len()));

        // 选中条目时显示 UUID 信息
        if let Some(idx) = self.selected_index {
            if let Some(entry) = self.entries.get(idx) {
                if !entry.uuid.is_empty() {
                    ui.label(
                        egui::RichText::new(format!("UUID: {}", entry.uuid))
                            .monospace()
                            .size(10.0)
                            .color(egui::Color32::GRAY),
                    );
                }
            }
        }

        // ── 拖拽预览浮层 ──
        let drag_active = state.dragged_asset_uuid.is_some()
            && state.drag_source.as_deref() == Some("AssetBrowser");

        // 取消拖拽：ESC 或 右键
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

            // 鼠标松开时清除拖拽（目标面板未消费）
            let released = ui.input(|input| {
                input.pointer.button_released(egui::PointerButton::Primary)
            });
            if released {
                // 不清除状态 — 让目标面板的下一帧消费它
                // 但若下一帧还在 AssetBrowser 手中（未被消费），自行清除
            }
        }

        // 在 AssetBrowser 面板上方渲染拖拽预览
        if drag_active {
            if let Some(mouse_pos) = ui.input(|input| input.pointer.hover_pos()) {
                let preview_label = state
                    .dragged_asset_name
                    .as_deref()
                    .unwrap_or("Asset");
                let icon_str = match state.dragged_asset_type {
                    Some(AssetType::Model) => "🔷",
                    Some(AssetType::Prefab) => "📦",
                    _ => "📦",
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
                                .background_color(egui::Color32::from_rgba_premultiplied(40, 40, 60, 220)),
                        );
                    });
            }
        }
    }
}
