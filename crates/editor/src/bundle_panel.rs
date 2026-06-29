//! Bundle 打包面板。
//!
//! 允许用户选择资产并打包为 Bundle，用于远程加载。

use crate::panels::{EditorPanel, EditorState};
use asset::bundle::BundleBuilder;
use asset::database::AssetDatabase;

/// Bundle 打包面板。
pub struct BundlePanel {
    /// Bundle 名称
    bundle_name: String,
    /// 选中的资产 UUID 列表
    selected_uuids: Vec<String>,
    /// 状态消息
    status_message: Option<String>,
    /// 是否成功
    last_success: bool,
}

impl BundlePanel {
    pub fn new() -> Self {
        Self {
            bundle_name: String::new(),
            selected_uuids: Vec::new(),
            status_message: None,
            last_success: false,
        }
    }

    /// 渲染面板 UI。
    pub fn show_panel(&mut self, ui: &mut egui::Ui, database: &AssetDatabase) {
        ui.horizontal(|ui| {
            ui.strong("Bundle Builder");
        });

        ui.add_space(8.0);

        // Bundle 名称
        ui.horizontal(|ui| {
            ui.label("Name:");
            ui.text_edit_singleline(&mut self.bundle_name);
        });

        ui.add_space(8.0);
        ui.separator();
        ui.add_space(4.0);

        // 资产列表
        ui.label("Select assets to bundle:");
        ui.add_space(4.0);

        egui::ScrollArea::vertical()
            .id_salt("bundle_panel_scroll")
            .max_height(200.0)
            .show(ui, |ui| {
                for entry in database.all_entries() {
                    let is_selected = self.selected_uuids.contains(&entry.uuid);
                    let mut checked = is_selected;
                    let label = format!(
                        "{} {} ({})",
                        Self::type_icon(entry.asset_type),
                        entry.path.split('/').last().unwrap_or(&entry.path),
                        &entry.uuid[..8.min(entry.uuid.len())]
                    );
                    if ui.checkbox(&mut checked, label).changed() {
                        if checked {
                            self.selected_uuids.push(entry.uuid.clone());
                        } else {
                            self.selected_uuids.retain(|u| u != &entry.uuid);
                        }
                    }
                }
            });

        ui.add_space(8.0);

        // 选中数量
        ui.label(format!("Selected: {} assets", self.selected_uuids.len()));

        ui.add_space(4.0);

        // Build 按钮
        let can_build = !self.bundle_name.is_empty() && !self.selected_uuids.is_empty();
        if ui.add_enabled(can_build, egui::Button::new("Build Bundle")).clicked() {
            self.build_bundle(database);
        }

        // 清空选择
        if ui.button("Clear Selection").clicked() {
            self.selected_uuids.clear();
            self.status_message = None;
        }

        ui.add_space(8.0);

        // 状态消息
        if let Some(ref msg) = self.status_message {
            let color = if self.last_success {
                egui::Color32::from_rgb(80, 200, 120)
            } else {
                egui::Color32::from_rgb(220, 80, 80)
            };
            ui.label(egui::RichText::new(msg).color(color));
        }
    }

    fn build_bundle(&mut self, database: &AssetDatabase) {
        let uuid_refs: Vec<&str> = self.selected_uuids.iter().map(|s| s.as_str()).collect();
        match BundleBuilder::build(
            &self.bundle_name,
            &uuid_refs,
            database,
            database.project_root(),
        ) {
            Ok(report) => {
                self.status_message = Some(format!(
                    "Bundle built: {} assets, {} bytes -> {}",
                    report.asset_count,
                    report.total_bytes,
                    report.bundle_path.display()
                ));
                self.last_success = true;
            }
            Err(e) => {
                self.status_message = Some(format!("Build failed: {e}"));
                self.last_success = false;
            }
        }
    }

    fn type_icon(ty: asset::meta::AssetTypeKind) -> &'static str {
        use asset::meta::AssetTypeKind;
        match ty {
            AssetTypeKind::Model => "🔷",
            AssetTypeKind::Texture => "🖼",
            AssetTypeKind::Audio => "🔊",
            AssetTypeKind::Scene => "🎬",
            AssetTypeKind::Avatar => "🧑",
            AssetTypeKind::Material => "🎨",
            AssetTypeKind::Other => "📄",
        }
    }
}

impl EditorPanel for BundlePanel {
    fn title(&self) -> &str {
        "Bundle Builder"
    }

    fn show(&mut self, ui: &mut egui::Ui, _state: &mut EditorState) {
        // BundlePanel 需要通过 show_panel() 传入 AssetDatabase
        // 在 Editor::update() 中直接调用
        ui.label("Use Editor integration to render this panel.");
    }
}
