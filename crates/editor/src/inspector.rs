//! Inspector 面板。
//!
//! 显示和编辑选中实体的属性：Transform（Position/Rotation/Scale）、
//! Mesh 信息（顶点数、三角形数、材质）、组件列表。

use crate::panels::{EditorAction, EditorPanel, EditorState, PendingTransform};

// ---------------------------------------------------------------------------
// InspectorPanel
// ---------------------------------------------------------------------------

/// Inspector 面板。
pub struct InspectorPanel {
    /// 当前编辑的 Transform 值
    position: [f32; 3],
    rotation: [f32; 3], // Euler 角度
    scale: [f32; 3],
    /// 上次选中的实体 ID，用于检测选中变化
    last_selected: Option<String>,
}

impl InspectorPanel {
    pub fn new() -> Self {
        Self {
            position: [0.0, 0.0, 0.0],
            rotation: [0.0, 0.0, 0.0],
            scale: [1.0, 1.0, 1.0],
            last_selected: None,
        }
    }

    /// 更新 Transform 值以匹配选中实体（从场景数据同步）。
    pub fn sync_transform(&mut self, _entity_id: &str) {
        // TODO: 从 Scene 读取实际 Transform
    }
}

/// 渲染带标签的 DragValue 行（独立函数，避免借用冲突）。
fn drag_value_row(ui: &mut egui::Ui, label: &str, values: &mut [f32; 3], speed: f32) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.label(label);
        ui.add_space(4.0);
        if ui
            .add(
                egui::DragValue::new(&mut values[0])
                    .speed(speed)
                    .prefix("X: "),
            )
            .changed()
        {
            changed = true;
        }
        if ui
            .add(
                egui::DragValue::new(&mut values[1])
                    .speed(speed)
                    .prefix("Y: "),
            )
            .changed()
        {
            changed = true;
        }
        if ui
            .add(
                egui::DragValue::new(&mut values[2])
                    .speed(speed)
                    .prefix("Z: "),
            )
            .changed()
        {
            changed = true;
        }
    });
    changed
}

impl EditorPanel for InspectorPanel {
    fn title(&self) -> &str {
        "Inspector"
    }

    fn show(&mut self, ui: &mut egui::Ui, state: &mut EditorState) {
        ui.strong("Inspector");

        // 检测选中实体变化，从缓存同步变换值
        let selection_changed = state.selected_entity.as_deref() != self.last_selected.as_deref();
        if selection_changed {
            self.last_selected = state.selected_entity.clone();
            if let Some(ref entity_id) = state.selected_entity {
                if let Some(&(pos, rot, scl)) = state.transform_cache.get(entity_id) {
                    self.position = pos;
                    self.rotation = rot;
                    self.scale = scl;
                } else {
                    // 首次选中：初始化缓存
                    let defaults = ([0.0, 0.0, 0.0], [0.0, 0.0, 0.0], [1.0, 1.0, 1.0]);
                    state.transform_cache.insert(entity_id.clone(), defaults);
                    self.position = defaults.0;
                    self.rotation = defaults.1;
                    self.scale = defaults.2;
                }
            }
        }

        match &state.selected_entity {
            Some(entity_id) => {
                ui.add_space(8.0);

                // 实体名称
                ui.horizontal(|ui| {
                    ui.label("Entity:");
                    ui.label(egui::RichText::new(entity_id).strong());
                });

                ui.add_space(8.0);
                ui.separator();
                ui.add_space(4.0);

                // ── Prefab 操作按钮 ──
                ui.horizontal(|ui| {
                    if ui.button("📦 Save as Prefab").clicked() {
                        state.pending_actions.push(EditorAction::SaveAsPrefab {
                            node_id: entity_id.clone(),
                        });
                    }
                });

                ui.add_space(4.0);
                ui.separator();
                ui.add_space(4.0);

                // Transform 组件
                egui::CollapsingHeader::new("▼ Transform")
                    .default_open(true)
                    .show(ui, |ui| {
                        let pos_before = self.position;
                        let rot_before = self.rotation;
                        let scl_before = self.scale;

                        let mut changed = false;
                        if drag_value_row(ui, "Position", &mut self.position, 0.1) {
                            changed = true;
                        }
                        if drag_value_row(ui, "Rotation", &mut self.rotation, 1.0) {
                            changed = true;
                        }
                        if drag_value_row(ui, "Scale   ", &mut self.scale, 0.01) {
                            changed = true;
                        }

                        if changed {
                            if let Some(ref entity_id) = state.selected_entity {
                                // 更新缓存
                                state.transform_cache.insert(
                                    entity_id.clone(),
                                    (self.position, self.rotation, self.scale),
                                );
                                // 推入待提交变更
                                state.pending_transform = Some(PendingTransform {
                                    entity_id: entity_id.clone(),
                                    old_position: pos_before,
                                    new_position: self.position,
                                    old_rotation: rot_before,
                                    new_rotation: self.rotation,
                                    old_scale: scl_before,
                                    new_scale: self.scale,
                                });
                            }
                        }
                    });

                ui.add_space(4.0);

                // Mesh 信息
                egui::CollapsingHeader::new("▼ Mesh Renderer")
                    .default_open(true)
                    .show(ui, |ui| {
                        ui.label("Vertices: —");
                        ui.label("Triangles: —");
                        ui.label("Material: —");
                        ui.label("Bounds: —");
                    });

                ui.add_space(4.0);

                // 组件管理
                egui::CollapsingHeader::new("▼ Components")
                    .default_open(true)
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            if ui.button("+ Add Component").clicked() {
                                // TODO: 添加组件对话框
                            }
                        });
                        ui.add_space(4.0);
                        // 显示已有组件列表
                        ui.label("• Transform");
                        ui.label("• Mesh Renderer");
                    });
            }
            None => {
                ui.add_space(20.0);
                ui.vertical_centered(|ui| {
                    ui.label(
                        egui::RichText::new("No entity selected")
                            .size(14.0)
                            .color(egui::Color32::GRAY),
                    );
                    ui.add_space(4.0);
                    ui.label("Select an entity in the Hierarchy panel");
                });
            }
        }
    }
}
