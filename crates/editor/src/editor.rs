//! 编辑器主框架。
//!
//! [`Editor`] 是一体化编辑器的顶层入口，管理面板布局、菜单栏、工具栏和全局状态。

use crate::asset_browser::AssetBrowser;
use crate::commands::CommandHistory;
use crate::hierarchy::HierarchyPanel;
use crate::inspector::InspectorPanel;
use crate::panels::{EditorLayout, EditorState};
use crate::play_mode::PlayMode;
use crate::viewport::{GizmoMode, ViewportPanel};

/// 编辑器顶层结构体。
pub struct Editor {
    /// 全局共享状态
    pub state: EditorState,
    /// Undo/Redo 命令历史
    pub command_history: CommandHistory,
    /// Play/Stop 模式管理器
    play_mode: PlayMode,
    /// 层级面板
    hierarchy: HierarchyPanel,
    /// 3D 视口
    viewport: ViewportPanel,
    /// Inspector 面板
    inspector: InspectorPanel,
    /// 资源浏览器
    asset_browser: AssetBrowser,
}

impl Editor {
    /// 从项目路径打开编辑器。
    pub fn open(project_path: String) -> Self {
        let state = EditorState::new(project_path);

        Self {
            state,
            command_history: CommandHistory::default(),
            play_mode: PlayMode::new(),
            hierarchy: HierarchyPanel::new(),
            viewport: ViewportPanel::new(),
            inspector: InspectorPanel::new(),
            asset_browser: AssetBrowser::new(),
        }
    }

    /// 每帧调用，渲染完整的编辑器 UI。
    pub fn update(&mut self, ctx: &egui::Context) {
        // 顶部菜单栏
        self.show_menu_bar(ctx);

        // 工具栏
        self.show_toolbar(ctx);

        // 快捷键处理
        self.handle_shortcuts(ctx);

        // 面板布局
        EditorLayout::render(
            ctx,
            &mut self.state,
            &mut self.hierarchy,
            &mut self.viewport,
            &mut self.inspector,
            &mut self.asset_browser,
        );
    }

    // -------------------------------------------------------------------
    // 菜单栏
    // -------------------------------------------------------------------

    fn show_menu_bar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("editor_menu_bar").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                // File 菜单
                ui.menu_button("File", |ui| {
                    if ui.button("New Scene").clicked() {
                        ui.close_menu();
                    }
                    if ui.button("Open Scene...").clicked() {
                        ui.close_menu();
                    }
                    if ui.button("Save Scene").clicked() {
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui.button("Exit").clicked() {
                        ui.close_menu();
                    }
                });

                // Edit 菜单
                ui.menu_button("Edit", |ui| {
                    let can_undo = self.command_history.can_undo();
                    let undo_label = self.command_history
                        .last_undo_description()
                        .map(|d| format!("Undo {}", d))
                        .unwrap_or_else(|| "Undo".to_string());
                    if ui
                        .add_enabled(can_undo, egui::Button::new(undo_label))
                        .clicked()
                    {
                        self.command_history.undo();
                        ui.close_menu();
                    }
                    if ui
                        .add_enabled(
                            self.command_history.can_redo(),
                            egui::Button::new("Redo"),
                        )
                        .clicked()
                    {
                        self.command_history.redo();
                        ui.close_menu();
                    }
                });

                // View 菜单
                ui.menu_button("View", |ui| {
                    if ui
                        .checkbox(&mut self.state.panel_visibility.hierarchy, "Hierarchy")
                        .clicked()
                    {
                        ui.close_menu();
                    }
                    if ui
                        .checkbox(&mut self.state.panel_visibility.inspector, "Inspector")
                        .clicked()
                    {
                        ui.close_menu();
                    }
                    if ui
                        .checkbox(
                            &mut self.state.panel_visibility.asset_browser,
                            "Asset Browser",
                        )
                        .clicked()
                    {
                        ui.close_menu();
                    }
                });

                // Help 菜单
                ui.menu_button("Help", |ui| {
                    if ui.button("About Geese Editor").clicked() {
                        ui.close_menu();
                    }
                });
            });
        });
    }

    // -------------------------------------------------------------------
    // 工具栏
    // -------------------------------------------------------------------

    fn show_toolbar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("editor_toolbar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                // Play/Stop 按钮
                let (label, color) = self.play_mode.button_ui();

                if ui
                    .add_sized(
                        [60.0, 24.0],
                        egui::Button::new(
                            egui::RichText::new(label).color(color),
                        ),
                    )
                    .clicked()
                {
                    self.toggle_play_mode();
                }

                ui.separator();

                // Gizmo 模式
                ui.label("Gizmo:");
                ui.selectable_value(&mut self.viewport.gizmo_mode, GizmoMode::Translate, "W");
                ui.selectable_value(&mut self.viewport.gizmo_mode, GizmoMode::Rotate, "E");
                ui.selectable_value(&mut self.viewport.gizmo_mode, GizmoMode::Scale, "R");

                ui.separator();

                // 选中的实体
                if let Some(ref entity) = self.state.selected_entity {
                    ui.label(format!("Selected: {}", entity));
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(format!("Project: {}", self.state.project_path));
                });
            });
        });
    }

    // -------------------------------------------------------------------
    // 快捷键
    // -------------------------------------------------------------------

    fn handle_shortcuts(&mut self, ctx: &egui::Context) {
        // 先收集快捷键动作
        let mut toggle_hierarchy = false;
        let mut toggle_inspector = false;
        let mut toggle_asset_browser = false;
        let mut undo = false;
        let mut redo = false;
        let mut save = false;

        ctx.input(|input| {
            if input.modifiers.ctrl && input.key_pressed(egui::Key::H) {
                toggle_hierarchy = true;
            }
            if input.modifiers.ctrl && input.key_pressed(egui::Key::I) {
                toggle_inspector = true;
            }
            if input.modifiers.ctrl && input.key_pressed(egui::Key::B) {
                toggle_asset_browser = true;
            }
            // Undo/Redo
            if input.modifiers.ctrl && input.key_pressed(egui::Key::Z) {
                if input.modifiers.shift {
                    redo = true;
                } else {
                    undo = true;
                }
            }
            // Save
            if input.modifiers.ctrl && input.key_pressed(egui::Key::S) {
                save = true;
            }
        });

        // 然后应用
        if toggle_hierarchy {
            self.state.panel_visibility.toggle_hierarchy();
        }
        if toggle_inspector {
            self.state.panel_visibility.toggle_inspector();
        }
        if toggle_asset_browser {
            self.state.panel_visibility.toggle_asset_browser();
        }
        if undo {
            self.command_history.undo();
        }
        if redo {
            self.command_history.redo();
        }
        if save {
            // TODO: 场景序列化保存
        }
    }

    /// 切换 Play/Stop 模式。
    fn toggle_play_mode(&mut self) {
        if self.play_mode.is_playing {
            // Stop: 恢复编辑模式
            if let Some(snapshot) = self.play_mode.stop() {
                self.state.selected_entity = snapshot.selected_entity;
                self.viewport.camera.yaw = snapshot.camera_yaw;
                self.viewport.camera.pitch = snapshot.camera_pitch;
                self.viewport.camera.distance = snapshot.camera_distance;
            }
            self.state.is_playing = false;
        } else {
            // Play: 进入播放模式
            self.play_mode.play(
                &self.state,
                self.viewport.camera.yaw,
                self.viewport.camera.pitch,
                self.viewport.camera.distance,
            );
            self.state.is_playing = true;
            self.state.selected_entity = None;
        }
    }
}


