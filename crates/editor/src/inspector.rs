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
    /// 当前编辑的实体名称
    entity_name: String,
    /// 当前编辑的 Transform 值
    position: [f32; 3],
    rotation: [f32; 3],
    scale: [f32; 3],
    /// 上次选中的实体 ID
    last_selected: Option<String>,
    /// 角色控制器参数
    cc_move_speed: f32,
    cc_jump_impulse: f32,
    cc_air_control: f32,
    cc_half_height: f32,
    cc_radius: f32,
    cc_enabled: bool,
    /// Physics Component 状态
    physics_enabled: bool,
    physics_server_enabled: bool,
    physics_client_enabled: bool,
    physics_body_kind_idx: usize,
}

impl InspectorPanel {
    pub fn new() -> Self {
        Self {
            entity_name: String::new(),
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
            physics_enabled: false,
            physics_server_enabled: true,
            physics_client_enabled: true,
            physics_body_kind_idx: 0,
        }
    }

    /// 推送物理组件更新到 pending_actions。
    fn push_physics_update(&self, entity_id: &str, state: &mut EditorState) {
        let body_kind = if self.physics_body_kind_idx == 1 {
            scene::manifest::BodyKindDef::Dynamic
        } else {
            scene::manifest::BodyKindDef::Fixed
        };
        let component = scene::manifest::PhysicsComponentDef {
            server_enabled: self.physics_server_enabled,
            client_enabled: self.physics_client_enabled,
            collision_enabled: true,
            body_kind,
        };
        state.pending_actions.push(EditorAction::SetPhysicsComponent {
            node_id: entity_id.to_string(),
            component: Some(component),
        });
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

        // 检测选中实体变化，从缓存同步
        let selection_changed = state.selected_entity.as_deref() != self.last_selected.as_deref();
        if selection_changed {
            self.last_selected = state.selected_entity.clone();
            if let Some(ref entity_id) = state.selected_entity {
                // 同步名称
                self.entity_name = state.name_cache
                    .get(entity_id)
                    .cloned()
                    .unwrap_or_default();
                // 同步 Transform
                if let Some(&(pos, rot, scl)) = state.transform_cache.get(entity_id) {
                    self.position = pos;
                    self.rotation = rot;
                    self.scale = scl;
                } else {
                    let defaults = ([0.0, 0.0, 0.0], [0.0, 0.0, 0.0], [1.0, 1.0, 1.0]);
                    state.transform_cache.insert(entity_id.clone(), defaults);
                    self.position = defaults.0;
                    self.rotation = defaults.1;
                    self.scale = defaults.2;
                }
                // 同步 Physics Component
                self.physics_enabled = state.physics_component_cache.contains_key(entity_id);
                if self.physics_enabled {
                    if let Some(comp) = state.physics_component_cache.get(entity_id) {
                        self.physics_server_enabled = comp.server_enabled;
                        self.physics_client_enabled = comp.client_enabled;
                        self.physics_body_kind_idx = match comp.body_kind {
                            scene::manifest::BodyKindDef::Dynamic => 1,
                            _ => 0,
                        };
                    }
                }
            }
        }

        match &state.selected_entity {
            Some(entity_id) => {
                let eid = entity_id.clone(); // 克隆以避免借用冲突
                ui.add_space(4.0);

                // ═══ Entity Name ═══
                ui.horizontal(|ui| {
                    ui.label("Name:");
                    let mut name = self.entity_name.clone();
                    let resp = ui.text_edit_singleline(&mut name);
                    if resp.changed() {
                        self.entity_name = name.clone();
                        state.name_cache.insert(eid.clone(), name.clone());
                        state.pending_actions.push(EditorAction::RenameEntity {
                            node_id: eid.clone(),
                            new_name: name,
                        });
                    }
                });

                ui.add_space(4.0);
                ui.separator();

                // ═══ Prefab 操作 ═══
                ui.horizontal(|ui| {
                    if ui.button("📦 Save as Prefab").clicked() {
                        state.pending_actions.push(EditorAction::SaveAsPrefab {
                            node_id: eid.clone(),
                        });
                    }
                });

                ui.add_space(4.0);
                ui.separator();

                // ═══ Transform ═══
                egui::CollapsingHeader::new("▼ Transform")
                    .default_open(true)
                    .show(ui, |ui| {
                        let pos_before = self.position;
                        let rot_before = self.rotation;
                        let scl_before = self.scale;

                        let mut changed = false;
                        if drag_value_row(ui, "Position", &mut self.position, 0.1) { changed = true; }
                        if drag_value_row(ui, "Rotation", &mut self.rotation, 1.0) { changed = true; }
                        if drag_value_row(ui, "Scale   ", &mut self.scale, 0.01) { changed = true; }

                        if changed {
                            if let Some(ref entity_id) = state.selected_entity {
                                state.transform_cache.insert(
                                    entity_id.clone(),
                                    (self.position, self.rotation, self.scale),
                                );
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

                // ═══ Mesh Renderer ═══
                let has_mesh = state.name_cache.contains_key(entity_id);
                if has_mesh {
                    egui::CollapsingHeader::new("▼ Mesh Renderer")
                        .default_open(false)
                        .show(ui, |ui| {
                            ui.label("Vertices: (from GLTF)");
                            ui.label("Triangles: (from GLTF)");
                            ui.label("Material: (from GLTF)");
                        });
                    ui.add_space(4.0);
                }

                // ═══ Physics Component ═══
                let eid2 = eid.clone();
                egui::CollapsingHeader::new("▼ Physics Component")
                    .default_open(false)
                    .show(ui, |ui| {
                        let eid3 = eid2.clone();
                        ui.horizontal(|ui| {
                            let label = if self.physics_enabled { "Remove" } else { "Add Component" };
                            if ui.button(label).clicked() {
                                self.physics_enabled = !self.physics_enabled;
                                let component = if self.physics_enabled {
                                    Some(scene::manifest::PhysicsComponentDef {
                                        server_enabled: true,
                                        client_enabled: true,
                                        collision_enabled: true,
                                        body_kind: scene::manifest::BodyKindDef::Fixed,
                                    })
                                } else {
                                    None
                                };
                                state.pending_actions.push(EditorAction::SetPhysicsComponent {
                                    node_id: eid3.clone(),
                                    component,
                                });
                            }
                        });
                        if self.physics_enabled {
                            ui.add_space(4.0);
                            let mut srv = self.physics_server_enabled;
                            if ui.checkbox(&mut srv, "Server Physics").changed() {
                                self.physics_server_enabled = srv;
                                self.push_physics_update(&eid3, state);
                            }
                            let mut cli = self.physics_client_enabled;
                            if ui.checkbox(&mut cli, "Client Physics").changed() {
                                self.physics_client_enabled = cli;
                                self.push_physics_update(&eid3, state);
                            }
                            ui.separator();
                            // Body Type
                            let mut idx = self.physics_body_kind_idx;
                            let old_idx = idx;
                            ui.horizontal(|ui| {
                                ui.label("Type:");
                                ui.selectable_value(&mut idx, 0, "Static");
                                ui.selectable_value(&mut idx, 1, "Dynamic");
                            });
                            if idx != old_idx {
                                self.physics_body_kind_idx = idx;
                                self.push_physics_update(&eid3, state);
                            }
                        }
                    });
                ui.add_space(4.0);

                // ═══ Character Controller ═══
                egui::CollapsingHeader::new("▼ Character Controller")
                    .default_open(false)
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            let label = if self.cc_enabled { "Disable" } else { "Add Component" };
                            if ui.button(label).clicked() {
                                self.cc_enabled = !self.cc_enabled;
                                state.pending_actions.push(EditorAction::ToggleCharacterController {
                                    node_id: eid.clone(),
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

                ui.add_space(4.0);

                // ═══ Component Overview ═══
                egui::CollapsingHeader::new("▼ All Components")
                    .default_open(false)
                    .show(ui, |ui| {
                        ui.label("• Transform");
                        if has_mesh { ui.label("• Mesh Renderer"); }
                        if self.physics_enabled { ui.label("• Physics Component"); }
                        ui.horizontal(|ui| {
                            if ui.button("+ Add Component").clicked() {}
                        });
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
