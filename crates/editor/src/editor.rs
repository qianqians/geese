//! 编辑器主框架。
//!
//! [`Editor`] 是一体化编辑器的顶层入口，管理面板布局、菜单栏、工具栏和全局状态。

use crate::asset_browser::AssetBrowser;
use crate::commands::CommandHistory;
use crate::commands::TransformCommand;
use crate::editor_mode::EditorMode;
use crate::gltf_import_dialog::GltfImportDialog;
use crate::hierarchy::HierarchyPanel;
use crate::inspector::InspectorPanel;
use crate::panel_layer::PanelLayer;
use crate::panels::{EditorLayout, EditorPanel, EditorState};
use crate::physics_debug::PhysicsDebugRenderer;
use crate::play_mode::PlayMode;
use crate::viewport::{GizmoMode, ViewportPanel};
use physics_client::BodySnapshot;
use physics_manager::{PhysicsManager, PhysicsSource};

use cgmath::{Point3, Vector3};
use std::cell::RefCell;
use std::rc::Rc;

/// 物理后端：编辑模式本地进程内运行，Play 模式连接远程服务器。

/// 编辑器顶层结构体。
pub struct Editor {
    /// 全局共享状态（含面板可见性 manager，单一真相源）
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
    /// GLTF 导入对话框
    gltf_import_dialog: GltfImportDialog,
    /// 资源浏览器是否需要重新扫描
    asset_needs_scan: bool,
    /// 统一物理管理器（支持本地/远程/二者同时）
    physics: PhysicsManager,
    /// 物理碰撞体调试渲染器
    physics_debug: PhysicsDebugRenderer,
    /// tokio 异步 Runtime（驱动物理通信）
    rt: tokio::runtime::Runtime,
    /// 上次帧更新时间（用于 dt 计算）
    last_update: Option<std::time::Instant>,
    /// Undo/Redo apply 回调队列（闭包写入，Editor 消费）
    apply_queue: Rc<RefCell<Vec<(String, Point3<f32>, Vector3<f32>, Vector3<f32>)>>>,
    /// 上一条变换命令的 entity_id（用于合并连续拖拽）
    last_transform_entity: Option<String>,
    /// 上一条变换命令的 old 值（合并时保持不变）
    last_transform_old: Option<(Point3<f32>, Vector3<f32>, Vector3<f32>)>,
    /// 等待中的远程物理步进结果（避免 block_on 阻塞 UI）
    pending_physics:
        Option<tokio::sync::oneshot::Receiver<Result<Vec<BodySnapshot>, String>>>,
}

impl Editor {
    /// 从项目路径打开编辑器。
    pub fn open(project_path: String) -> Self {
        let state = EditorState::new(project_path.clone());
        let rt = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
        .expect("Failed to create tokio runtime");

        // 创建本地物理世界并加载场景碰撞体
        let mut physics = PhysicsManager::new(PhysicsSource::Client, [0.0, -9.81, 0.0]);
        let manifest_path = format!("{}/.scene.json", project_path);
        physics.load_scene(&manifest_path);

        Self {
            state,
            command_history: CommandHistory::default(),
            play_mode: PlayMode::new(),
            hierarchy: HierarchyPanel::new(),
            viewport: ViewportPanel::new(),
            inspector: InspectorPanel::new(),
            asset_browser: AssetBrowser::new(),
            gltf_import_dialog: GltfImportDialog::new(),
            asset_needs_scan: true,
            physics,
            physics_debug: PhysicsDebugRenderer::new(),
            rt,
            last_update: None,
            apply_queue: Rc::new(RefCell::new(Vec::new())),
            last_transform_entity: None,
            last_transform_old: None,
            pending_physics: None,
        }
    }

    /// 每帧调用，渲染完整的编辑器 UI。
    pub fn update(&mut self, ctx: &egui::Context) {
        // 计算每帧时间增量
        let now = std::time::Instant::now();
        let dt = self
            .last_update
            .map(|t| now.duration_since(t).as_secs_f32())
            .unwrap_or(0.016);
        self.last_update = Some(now);

        // 物理步进
        // 本地物理步进
        if self.physics.source().runs_local() && self.state.mode.is_editing() {
            self.physics.step(dt);
            if self.physics_debug.enabled {
                let bodies = self.physics.get_local_body_snapshots();
                self.state.physics_debug_bodies = bodies.clone();
                self.physics_debug.update(bodies);
            }
        }

        // 远程物理异步步进
        if self.physics.source().runs_remote() && self.state.mode.is_playing() && self.physics.is_remote_connected() {
            if let Some(rx) = &mut self.pending_physics {
                match rx.try_recv() {
                    Ok(result) => {
                        self.pending_physics = None;
                        match result {
                            Ok(bodies) => {
                                self.state.physics_debug_bodies = bodies.clone();
                                self.physics_debug.update(bodies);
                            }
                            Err(e) => eprintln!("[Editor] physics step error: {e}"),
                        }
                    }
                    Err(tokio::sync::oneshot::error::TryRecvError::Closed) => {
                        self.pending_physics = None;
                        eprintln!("[Editor] physics task closed");
                    }
                    Err(tokio::sync::oneshot::error::TryRecvError::Empty) => {}
                }
            }

            if self.pending_physics.is_none() {
                if let Some(client) = self.physics.remote_client() {
                    let debug_enabled = self.physics_debug.enabled;
                    let (tx, rx) = tokio::sync::oneshot::channel();
                    let dt_f64 = dt as f64;
                    self.rt.spawn(async move {
                        let result = async {
                            client.step(dt_f64).await?;
                            if debug_enabled {
                                client.get_bodies().await
                            } else {
                                Ok(Vec::new())
                            }
                        }
                        .await;
                        let _ = tx.send(result);
                    });
                    self.pending_physics = Some(rx);
                }
            }
        }
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

        // 5. 处理 Inspector 写回的变换变更 -> 推入 CommandHistory
        self.process_pending_transform();
        // 6. 消费 Undo/Redo 回调产生的 apply 事件
        self.process_apply_queue();
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
                    let mut hier_vis = self.state.panel_layer.is_visible(&PanelLayer::Hierarchy);
                    if ui.checkbox(&mut hier_vis, "Hierarchy").clicked() {
                        self.state.panel_layer.set_visible(PanelLayer::Hierarchy, hier_vis);
                        ui.close_menu();
                    }
                    let mut insp_vis = self.state.panel_layer.is_visible(&PanelLayer::Inspector);
                    if ui.checkbox(&mut insp_vis, "Inspector").clicked() {
                        self.state.panel_layer.set_visible(PanelLayer::Inspector, insp_vis);
                        ui.close_menu();
                    }
                    let mut ab_vis = self.state.panel_layer.is_visible(&PanelLayer::AssetBrowser);
                    if ui.checkbox(&mut ab_vis, "Asset Browser").clicked() {
                        self.state.panel_layer.set_visible(PanelLayer::AssetBrowser, ab_vis);
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
        let alpha = self.state.panel_layer.global_alpha;
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

                    // Physics Debug 开关
                    {
                        let debug_label = if self.physics_debug.enabled {
                            "🔍 Debug ON"
                        } else {
                            "🔍 Debug OFF"
                        };
                        if ui
                            .add_sized(
                                [90.0, 24.0],
                                egui::Button::new(egui::RichText::new(debug_label).size(11.0)),
                            )
                            .clicked()
                        {
                            self.physics_debug.toggle();
                        }
                        ui.separator();

                        // Physics Source 选择
                        ui.label("Physics:");
                        let mut phy_src = self.physics.source();
                        let src_old = phy_src;
                        ui.selectable_value(&mut phy_src, physics_manager::PhysicsSource::Client, "Local");
                        ui.selectable_value(&mut phy_src, physics_manager::PhysicsSource::Server, "Remote");
                        ui.selectable_value(&mut phy_src, physics_manager::PhysicsSource::ClientAndServer, "Both");
                        if !self.state.mode.is_playing() && phy_src != src_old {
                            self.physics.set_source(phy_src);
                        }
                        ui.separator();
                    }

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
            self.state.panel_layer.toggle_visible(PanelLayer::Hierarchy);
        }
        if toggle_inspector {
            self.state.panel_layer.toggle_visible(PanelLayer::Inspector);
        }
        if toggle_asset_browser {
            self.state.panel_layer.toggle_visible(PanelLayer::AssetBrowser);
        }
        if toggle_ui {
            self.state.panel_layer.toggle_all();
        }
        if undo {
            self.command_history.undo();
            self.last_transform_entity = None;
            self.last_transform_old = None;
            self.process_apply_queue();
        }
        if redo {
            self.command_history.redo();
            self.last_transform_entity = None;
            self.last_transform_old = None;
            self.process_apply_queue();
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
        let alpha = self.state.panel_layer.global_alpha;
        let bg_fill = egui::Color32::from_rgba_unmultiplied(28, 30, 38, (alpha * 220.0) as u8);
        let frame = egui::Frame::window(&ctx.style())
            .fill(bg_fill)
            .rounding(egui::Rounding::same(6.0));

        // 左侧 - Hierarchy 面板
        if self.state.panel_layer.is_visible(&PanelLayer::Hierarchy) {
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
        if self.state.panel_layer.is_visible(&PanelLayer::Inspector) {
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
        if self.state.panel_layer.is_visible(&PanelLayer::AssetBrowser) {
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

    /// 处理 Inspector 写回的变换变更，生成 TransformCommand 并推入历史。
    /// 连续拖拽同一实体时自动合并为单次 Undo 条目。
    fn process_pending_transform(&mut self) {
        let Some(change) = self.state.pending_transform.take() else {
            // 无拖拽帧：拖拽已松手，重置合并状态，下一轮拖拽 = 新 Undo 条目
            self.last_transform_entity = None;
            self.last_transform_old = None;
            return;
        };

        let new_pos = Point3::new(
            change.new_position[0],
            change.new_position[1],
            change.new_position[2],
        );
        let new_rot = Vector3::new(
            change.new_rotation[0],
            change.new_rotation[1],
            change.new_rotation[2],
        );
        let new_scl = Vector3::new(
            change.new_scale[0],
            change.new_scale[1],
            change.new_scale[2],
        );

        // 检查是否需要合并：同一实体连续拖拽
        let same_entity = self.last_transform_entity.as_deref() == Some(&change.entity_id);
        if same_entity {
            // 弹出上一条命令，保留其 old 值
            let _ = self.command_history.pop_last_undo();
        } else {
            // 不同实体或新一轮拖拽，重置合并状态
            self.last_transform_entity = None;
            self.last_transform_old = None;
        }

        let (old_pos, old_rot, old_scl) = if same_entity {
            // 合并：沿用最初的 old 值（clone 而非 take，支持多次合并）
            self.last_transform_old
                .clone()
                .expect("merge must have old values")
        } else {
            // 新拖拽：记录 old 值供后续合并使用
            let old = (
                Point3::new(
                    change.old_position[0],
                    change.old_position[1],
                    change.old_position[2],
                ),
                Vector3::new(
                    change.old_rotation[0],
                    change.old_rotation[1],
                    change.old_rotation[2],
                ),
                Vector3::new(
                    change.old_scale[0],
                    change.old_scale[1],
                    change.old_scale[2],
                ),
            );
            self.last_transform_old = Some(old.clone());
            self.last_transform_entity = Some(change.entity_id.clone());
            old
        };

        // 设置 apply 回调：通过 apply_queue 将变换写回 transform_cache
        let queue = self.apply_queue.clone();
        let entity_id = change.entity_id;
        let cmd = TransformCommand::new(
            entity_id.clone(),
            old_pos, new_pos, old_rot, new_rot, old_scl, new_scl,
        ).with_apply(move |eid, pos, rot, scl| {
            queue.borrow_mut().push((eid.to_string(), pos, rot, scl));
        });

        self.command_history.execute(Box::new(cmd));
    }

    /// 消费 Undo/Redo 回调产生的 apply 事件，写入 transform_cache。
    fn process_apply_queue(&mut self) {
        let pending: Vec<_> = self.apply_queue.borrow_mut().drain(..).collect();
        for (eid, pos, rot, scl) in pending {
            self.state.transform_cache.insert(
                eid,
                (
                    [pos.x, pos.y, pos.z],
                    [rot.x, rot.y, rot.z],
                    [scl.x, scl.y, scl.z],
                ),
            );
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
            // 停止远程物理服务器，切换到本地物理
            self.physics.disconnect_remote();
            self.physics = PhysicsManager::new(PhysicsSource::Client, [0.0, -9.81, 0.0]);
            // 重新加载场景碰撞体
            {
                let manifest_path = format!("{}/.scene.json", self.state.project_path);
                self.physics.load_scene(&manifest_path);
            }
            self.physics_debug.enabled = false;
            self.state.mode = EditorMode::Edit;
            self.state.panel_layer.set_edit_alpha();
        } else {
            // Play: 进入播放模式
            self.play_mode.play(
                &self.state,
                self.viewport.camera.yaw,
                self.viewport.camera.pitch,
                self.viewport.camera.distance,
            );

            // 切换到远程物理服务器
            let python_path = "python3";
            let server_script = concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/scripts/physics_editor_server.py"
            );
            self.physics = PhysicsManager::new(PhysicsSource::ClientAndServer, [0.0, -9.81, 0.0]);
            if let Err(e) = self.physics.connect_remote(python_path, server_script, &self.rt) {
                eprintln!("[Editor] Failed to start physics server: {e}");
            } else {
                if let Err(e) = self.rt.block_on(self.physics.init_physics_remote([0.0, -9.81, 0.0])) {
                    eprintln!("[Editor] Failed to init physics: {e}");
                }
                let manifest_path = format!("{}/.scene.json", self.state.project_path);
                if let Err(e) = self.rt.block_on(self.physics.load_scene_remote(&manifest_path)) {
                    eprintln!("[Editor] Failed to load scene physics: {e}");
                }
                self.physics.load_scene(&manifest_path);
            }

            self.state.mode = EditorMode::Play;
            self.state.selected_entity = None;
            self.state.panel_layer.set_play_alpha();
        }
    }
}
