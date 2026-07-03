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
    /// 角色控制器参数（模拟）
    cc_move_speed: f32,
    cc_jump_impulse: f32,
    cc_air_control: f32,
    cc_half_height: f32,
    cc_radius: f32,
    cc_enabled: bool,
    /// Physics Body 类型索引 (0=Static, 1=Dynamic)
    body_kind_idx: usize,
}

impl InspectorPanel {
    pub fn new() -> Self {
        Self {
            position: [0.0, 0.0, 0.0],
            rotation: [0.0, 0.0, 0.0],
            scale: [1.0, 1.0, 1.0],
            last_selected: None,
            cc_move_speed: 5.0,
            cc_jump_impulse: 8.0,
            cc_air_control: 0.3,
            cc_half_height: 1.0,
            cc_radius: 0.5,
            cc_enabled: false,
            body_kind_idx: 0,
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

                ui.add_space(4.0);

                // Physics Body 组件
                egui::CollapsingHeader::new("▼ Physics Body")
                    .default_open(false)
                    .show(ui, |ui| {
                        if let Some(ref entity_id) = state.selected_entity {
                            if let Some(kind) = state.body_kind_cache.get(entity_id).copied() {
                                let mut idx = match kind {
                                    scene::manifest::BodyKindDef::Fixed => 0,
                                    scene::manifest::BodyKindDef::Dynamic => 1,
                                };
                                let old_idx = idx;
                                ui.horizontal(|ui| {
                                    ui.label("Type:");
                                    ui.selectable_value(&mut idx, 0, "Static");
                                    ui.selectable_value(&mut idx, 1, "Dynamic");
                                });
                                if idx != old_idx {
                                    let new_kind = if idx == 0 {
                                        scene::manifest::BodyKindDef::Fixed
                                    } else {
                                        scene::manifest::BodyKindDef::Dynamic
                                    };
                                    state.body_kind_cache.insert(entity_id.clone(), new_kind);
                                    state.pending_actions.push(EditorAction::SetBodyKind {
                                        node_id: entity_id.clone(),
                                        body_kind: new_kind,
                                    });
                                    self.body_kind_idx = idx;
                                }
                            } else {
                                ui.label("No collision enabled");
                            }
                        }
                    });

                ui.add_space(4.0);

                // 角色控制器组件
                egui::CollapsingHeader::new("▼ Character Controller")
                    .default_open(false)
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            let label = if self.cc_enabled { "Remove Controller" } else { "Add Controller" };
                            if ui.button(label).clicked() {
                                self.cc_enabled = !self.cc_enabled;
                                // 推送 EditorAction，让 Editor 处理角色控制器的添加/移除
                                state.pending_actions.push(EditorAction::ToggleCharacterController {
                                    node_id: entity_id.clone(),
                                    enabled: self.cc_enabled,
                                    move_speed: self.cc_move_speed,
                                    jump_impulse: self.cc_jump_impulse,
                                    air_control: self.cc_air_control,
                                    half_height: self.cc_half_height,
                                    radius: self.cc_radius,
                                });
                            }
                        });
                        if self.cc_enabled {
                            ui.add_space(4.0);
                            ui.add(egui::Slider::new(&mut self.cc_move_speed, 1.0..=20.0).text("Move Speed"));
                            ui.add(egui::Slider::new(&mut self.cc_jump_impulse, 1.0..=30.0).text("Jump Impulse"));
                            ui.add(egui::Slider::new(&mut self.cc_air_control, 0.0..=1.0).text("Air Control"));
                            ui.add(egui::Slider::new(&mut self.cc_half_height, 0.1..=3.0).text("Half Height"));
                            ui.add(egui::Slider::new(&mut self.cc_radius, 0.1..=1.0).text("Radius"));
                        }
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
