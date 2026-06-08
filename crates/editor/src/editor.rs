//! 编辑器主框架。
//!
//! [`Editor`] 是一体化编辑器的顶层入口，管理面板布局、菜单栏、工具栏和全局状态。

use crate::asset_browser::AssetBrowser;
use crate::commands::CommandHistory;
use crate::editor_mode::EditorMode;
use crate::gltf_import_dialog::GltfImportDialog;
use crate::hierarchy::HierarchyPanel;
use crate::inspector::InspectorPanel;
use crate::panel_layer::PanelLayerManager;
use crate::panels::{EditorLayout, EditorPanel, EditorState};
use crate::play_mode::PlayMode;
use crate::viewport::{GizmoMode, ViewportPanel};

/// 编辑器顶层结构体。
pub struct Editor {
    /// 全局共享状态
    pub state: EditorState,
    /// Undo/Redo 命令历史
    pub command_history: CommandHistory,
    /// 面板层级管理器
    pub panel_layer: PanelLayerManager,
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
    /// GLTF 导入对话框
    gltf_import_dialog: GltfImportDialog,
    /// 资源浏览器是否需要重新扫描
    asset_needs_scan: bool,
}

impl Editor {
    /// 从项目路径打开编辑器。
    pub fn open(project_path: String) -> Self {
        let state = EditorState::new(project_path);

        Self {
            state,
            command_history: CommandHistory::default(),
            panel_layer: PanelLayerManager::default(),
            play_mode: PlayMode::new(),
            hierarchy: HierarchyPanel::new(),
            viewport: ViewportPanel::new(),
            inspector: InspectorPanel::new(),
            asset_browser: AssetBrowser::new(),
            gltf_import_dialog: GltfImportDialog::new(),
            asset_needs_scan: true,
        }
    }

    /// 每帧调用，渲染完整的编辑器 UI。
    pub fn update(&mut self, ctx: &egui::Context) {
        // 1. 快捷键始终生效
        self.handle_shortcuts(ctx);

        // 2. 全屏视口（场景渲染纹理填充整个窗口）
        self.show_fullscreen_viewport(ctx);

        // 3. 浮动面板（在全屏视口之上，egui::Window 渲染）
        if self.state.ui_visible || !self.state.mode.is_playing() {
            self.show_floating_panels(ctx);
            self.show_toolbar(ctx);
        }

        // 4. GLTF 导入对话框（模态，始终检测）
        let was_visible = self.gltf_import_dialog.visible;
        self.gltf_import_dialog.show_dialog(ctx, &mut self.state);
        // 对话框关闭时刷新资源浏览器
        if was_visible && !self.gltf_import_dialog.visible && self.gltf_import_dialog.import_success {
            self.asset_needs_scan = true;
        }
    }

    // -------------------------------------------------------------------
    // 菜单栏
    // -------------------------------------------------------------------

    #[allow(dead_code)]
    fn show_menu_bar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("editor_menu_bar").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                // File 菜单
                ui.menu_button("File", |ui| {
                    if ui.button("Import GLTF...").clicked() {
                        self.gltf_import_dialog.open();
                        ui.close_menu();
                    }
                    ui.separator();
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
    // 全屏视口
    // -------------------------------------------------------------------

    /// 渲染全屏沉浸式视口（场景渲染纹理填充整个窗口）。
    fn show_fullscreen_viewport(&mut self, ctx: &egui::Context) {
        EditorLayout::render_fullscreen(ctx, &mut self.state, &mut self.viewport);
    }

    // -------------------------------------------------------------------
    // 工具栏
    // -------------------------------------------------------------------

    fn show_toolbar(&mut self, ctx: &egui::Context) {
        let alpha = self.state.panel_alpha;
        let bg_fill = egui::Color32::from_rgba_unmultiplied(20, 22, 30, (alpha * 220.0) as u8);
        let frame = egui::Frame::window(&ctx.style())
            .fill(bg_fill)
            .rounding(egui::Rounding::same(6.0));

        egui::Window::new("##floating_toolbar")
            .title_bar(false)
            .resizable(false)
            .collapsible(false)
            .anchor(egui::Align2::CENTER_TOP, egui::Vec2::new(0.0, 8.0))
            .frame(frame)
            .show(ctx, |ui| {
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
                    ui.selectable_value(&mut self.viewport.gizmo_mode, GizmoMode::Translate, "W");
                    ui.selectable_value(&mut self.viewport.gizmo_mode, GizmoMode::Rotate, "E");
                    ui.selectable_value(&mut self.viewport.gizmo_mode, GizmoMode::Scale, "R");

                    ui.separator();

                    // 面板显隐总控按钮
                    if ui.button("👁").clicked() {
                        self.state.ui_visible = !self.state.ui_visible;
                    }
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
        let mut toggle_ui = false;
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
            // Tab 切换所有非 pinned 面板
            if input.key_pressed(egui::Key::Tab) && !input.modifiers.ctrl {
                toggle_ui = true;
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
        if toggle_ui {
            self.panel_layer.toggle_all();
            // 同步到 PanelVisibility（保持兼容）
            let all_visible = self.panel_layer.is_visible(&crate::panel_layer::PanelLayer::Hierarchy);
            self.state.panel_visibility.hierarchy = all_visible;
            self.state.panel_visibility.inspector = all_visible;
            self.state.panel_visibility.asset_browser = all_visible;
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

    // -------------------------------------------------------------------
    // 浮动面板
    // -------------------------------------------------------------------

    /// 渲染浮动半透明面板（在全屏视口之上）。
    fn show_floating_panels(&mut self, ctx: &egui::Context) {
        let alpha = self.state.panel_alpha;
        let bg_fill = egui::Color32::from_rgba_unmultiplied(28, 30, 38, (alpha * 220.0) as u8);
        let frame = egui::Frame::window(&ctx.style())
            .fill(bg_fill)
            .rounding(egui::Rounding::same(6.0));

        // 左侧 - Hierarchy 面板
        if self.state.panel_visibility.hierarchy {
            egui::Window::new("Hierarchy")
                .resizable(true)
                .collapsible(true)
                .default_width(250.0)
                .frame(frame)
                .show(ctx, |ui| {
                    self.hierarchy.show(ui, &mut self.state);
                });
        }

        // 右侧 - Inspector 面板
        if self.state.panel_visibility.inspector {
            egui::Window::new("Inspector")
                .resizable(true)
                .collapsible(true)
                .default_width(300.0)
                .frame(frame)
                .show(ctx, |ui| {
                    self.inspector.show(ui, &mut self.state);
                });
        }

        // 底部 - Asset Browser 面板
        if self.state.panel_visibility.asset_browser {
            if self.asset_needs_scan {
                self.asset_browser.scan_directory(&self.state.project_path);
                self.asset_needs_scan = false;
            }
            egui::Window::new("Asset Browser")
                .resizable(true)
                .collapsible(true)
                .default_height(200.0)
                .frame(frame)
                .show(ctx, |ui| {
                    self.asset_browser.show(ui, &mut self.state);
                });
        }
    }

    /// 切换 Play/Stop 模式。
    fn toggle_play_mode(&mut self) {
        if self.play_mode.is_playing {
            // Stop: 恢复编辑模式
            if let Some(mut snapshot) = self.play_mode.stop() {
                PlayMode::restore_panel_state(&snapshot, &mut self.state);
                self.viewport.camera.yaw = snapshot.camera_yaw;
                self.viewport.camera.pitch = snapshot.camera_pitch;
                self.viewport.camera.distance = snapshot.camera_distance;
                self.state.selected_entity = snapshot.selected_entity.take();
            }
            self.state.mode = EditorMode::Edit;
            self.panel_layer.set_edit_alpha();
        } else {
            // Play: 进入播放模式
            self.play_mode.play(
                &self.state,
                self.viewport.camera.yaw,
                self.viewport.camera.pitch,
                self.viewport.camera.distance,
            );
            self.state.mode = EditorMode::Play;
            self.state.selected_entity = None;
            self.panel_layer.set_play_alpha();
            self.state.panel_alpha = self.panel_layer.global_alpha;
        }
    }
}


