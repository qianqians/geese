//! 资源浏览器（Asset Browser）。
//!
//! 浏览项目 assets 目录，支持按类型过滤和拖拽导入。

use crate::panels::{EditorPanel, EditorState};

// ---------------------------------------------------------------------------
// AssetEntry - 资源条目
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct AssetEntry {
    pub name: String,
    pub path: String,
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
            AssetType::Other => "📄",
        }
    }

    fn from_extension(ext: &str) -> Self {
        match ext.to_lowercase().as_str() {
            "gltf" | "glb" => AssetType::Model,
            "png" | "jpg" | "jpeg" | "hdr" | "exr" | "ktx2" => AssetType::Texture,
            "wav" | "ogg" | "mp3" | "flac" => AssetType::Audio,
            "geese" | "scene" => AssetType::Scene,
            _ => AssetType::Other,
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
}

impl AssetFilter {
    fn label(&self) -> &str {
        match self {
            AssetFilter::All => "All",
            AssetFilter::Models => "Models",
            AssetFilter::Textures => "Textures",
            AssetFilter::Audio => "Audio",
            AssetFilter::Scenes => "Scenes",
        }
    }

    fn matches(&self, asset_type: AssetType) -> bool {
        match self {
            AssetFilter::All => true,
            AssetFilter::Models => matches!(asset_type, AssetType::Model),
            AssetFilter::Textures => matches!(asset_type, AssetType::Texture),
            AssetFilter::Audio => matches!(asset_type, AssetType::Audio),
            AssetFilter::Scenes => matches!(asset_type, AssetType::Scene),
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
            current_path: "assets/".into(),
            entries: Self::sample_entries(),
            filter: AssetFilter::All,
            selected_index: None,
            view_mode: ViewMode::List,
        }
    }

    fn sample_entries() -> Vec<AssetEntry> {
        vec![
            AssetEntry {
                name: "scenes".into(),
                path: "assets/scenes/".into(),
                asset_type: AssetType::Folder,
                size_bytes: 0,
            },
            AssetEntry {
                name: "models".into(),
                path: "assets/models/".into(),
                asset_type: AssetType::Folder,
                size_bytes: 0,
            },
            AssetEntry {
                name: "textures".into(),
                path: "assets/textures/".into(),
                asset_type: AssetType::Folder,
                size_bytes: 0,
            },
            AssetEntry {
                name: "main.scene".into(),
                path: "assets/scenes/main.scene".into(),
                asset_type: AssetType::Scene,
                size_bytes: 2048,
            },
            AssetEntry {
                name: "player.glb".into(),
                path: "assets/models/player.glb".into(),
                asset_type: AssetType::Model,
                size_bytes: 524288,
            },
            AssetEntry {
                name: "floor.png".into(),
                path: "assets/textures/floor.png".into(),
                asset_type: AssetType::Texture,
                size_bytes: 65536,
            },
        ]
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

    fn show(&mut self, ui: &mut egui::Ui, _state: &mut EditorState) {
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
                self.current_path = "assets/".into();
            }
            ui.label(format!("📂 {}", self.current_path));
        });

        ui.add_space(4.0);

        // 过滤器
        ui.horizontal(|ui| {
            for filter in &[AssetFilter::All, AssetFilter::Models, AssetFilter::Textures, AssetFilter::Audio, AssetFilter::Scenes] {
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
                                    if ui.selectable_label(is_selected, label).clicked() {
                                        self.selected_index = Some(i);
                                    }
                                    ui.label(Self::format_size(entry.size_bytes));
                                    ui.end_row();
                                }
                            });
                    }
                    ViewMode::Grid => {
                        // 网格视图：每行放多个缩略图卡片
                        let card_width = 100.0;
                        let available = ui.available_width();
                        let cols = (available / (card_width + 8.0)).max(1.0) as usize;

                        let mut col = 0;
                        for (i, entry) in filtered.iter().enumerate() {
                            if col == 0 {
                                ui.horizontal(|_ui| {
                                    // 占位
                                });
                            }

                            let is_selected = self.selected_index == Some(i);
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
                                self.selected_index = Some(i);
                            }

                            col += 1;
                            if col >= cols {
                                col = 0;
                                ui.end_row();
                            }
                        }
                    }
                }
            });

        // 底部信息栏
        ui.add_space(4.0);
        ui.separator();
        ui.label(format!("{} items", filtered.len()));
    }
}
