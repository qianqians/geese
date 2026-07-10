//! 材质编辑器面板 — egui 材质属性编辑。
//!
//! 功能：显示选中实体的材质属性（base_color/metallic/roughness 等），
//!       支持实时编辑并同步到 `MaterialLibrary`。

use crate::editor::Editor;
use egui::{Grid, ScrollArea, Slider};

/// 材质编辑器面板状态。
#[derive(Default)]
pub struct MaterialEditorPanel {
    /// 当前选中的材质索引
    selected_material: Option<usize>,
    /// 搜索过滤文本
    filter: String,
}

impl MaterialEditorPanel {
    pub fn new() -> Self {
        Self::default()
    }

    /// 绘制材质编辑器 UI。
    pub fn show(&mut self, ctx: &egui::Context, _editor: &mut Editor) {
        egui::Window::new("Material Editor")
            .default_size([400.0, 600.0])
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label("🔍");
                    ui.text_edit_singleline(&mut self.filter);
                });
                ui.separator();

                ScrollArea::vertical().show(ui, |ui| {
                    self.show_material_list(ui, _editor);
                });
            });
    }

    fn show_material_list(&mut self, ui: &mut egui::Ui, editor: &mut Editor) {
        let library = &mut editor.material_library;
        for (i, material) in library.materials.iter().enumerate() {
            let name = material.name.as_deref().unwrap_or("Unnamed");
            if !self.filter.is_empty() && !name.to_lowercase().contains(&self.filter.to_lowercase()) {
                continue;
            }

            let selected = self.selected_material == Some(i);
            let resp = ui.selectable_label(selected, format!("[{i}] {name}"));
            if resp.clicked() {
                self.selected_material = Some(i);
            }
        }

        if let Some(idx) = self.selected_material {
            if let Some(material) = library.materials.get_mut(idx) {
                ui.separator();
                ui.heading("Properties");
                Grid::new("mat_props").num_columns(2).show(ui, |ui| {
                    ui.label("Name:");
                    let mut name = material.name.clone().unwrap_or_default();
                    if ui.text_edit_singleline(&mut name).changed() {
                        material.name = if name.is_empty() { None } else { Some(name) };
                    }
                    ui.end_row();

                    ui.label("Base Color:");
                    ui.color_edit_button_rgba_unmultiplied(&mut material.base_color_factor);
                    ui.end_row();

                    ui.label("Metallic:");
                    ui.add(Slider::new(&mut material.metallic_factor, 0.0..=1.0));
                    ui.end_row();

                    ui.label("Roughness:");
                    ui.add(Slider::new(&mut material.roughness_factor, 0.0..=1.0));
                    ui.end_row();

                    ui.label("Alpha Cutoff:");
                    ui.add(Slider::new(&mut material.alpha_cutoff, 0.0..=1.0));
                    ui.end_row();

                    ui.label("Double Sided:");
                    ui.checkbox(&mut material.double_sided, "");
                    ui.end_row();

                    ui.label("Alpha Mode:");
                    egui::ComboBox::from_id_salt("alpha_mode")
                        .selected_text(format!("{:?}", material.alpha_mode))
                        .show_ui(ui, |ui| {
                            ui.selectable_value(
                                &mut material.alpha_mode,
                                render::AlphaMode::Opaque,
                                "Opaque",
                            );
                            ui.selectable_value(
                                &mut material.alpha_mode,
                                render::AlphaMode::Mask,
                                "Mask",
                            );
                            ui.selectable_value(
                                &mut material.alpha_mode,
                                render::AlphaMode::Blend,
                                "Blend",
                            );
                        });
                    ui.end_row();

                    ui.label("Custom Shader:");
                    let has_custom = material.custom_shader.is_some();
                    let mut flag = has_custom;
                    if ui.checkbox(&mut flag, "Use Shader Graph").changed() {
                        if flag && material.custom_shader.is_none() {
                            material.custom_shader = Some(
                                std::sync::Arc::new(
                                    render::shader_graph::ShaderGraph::new(),
                                ),
                            );
                        } else if !flag {
                            material.custom_shader = None;
                        }
                    }
                    ui.end_row();
                });
            }
        }
    }
}
