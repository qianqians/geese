//! 编辑器主框架。
//!
//! [`Editor`] 是一体化编辑器的顶层入口，管理面板布局、菜单栏、工具栏和全局状态。
//!
//! ## Suggested module split (future refactoring):
//! - `editor_state.rs` — EditorState and shared state definitions
//! - `editor_layout.rs` — Panel layout and menu bar rendering
//! - `editor_prefab.rs` — Prefab save/instantiate logic
//! - `editor_animation.rs` — Animation preview and marker editing
//! - `editor_play_mode.rs` — Play/Stop mode switching and physics integration

use crate::animation_panel::AnimationPanel;
use crate::asset_browser::AssetBrowser;
use crate::build_panel::BuildPanel;
use crate::bundle_panel::BundlePanel;
use crate::commands::CommandHistory;
use crate::commands::TransformCommand;
use crate::editor_mode::EditorMode;
use crate::gltf_import_dialog::GltfImportDialog;
use crate::hierarchy::{HierarchyPanel, SceneNodeData, NodeType};
use crate::inspector::InspectorPanel;
use crate::panel_layer::PanelLayer;
use crate::panels::{EditorAction, EditorLayout, EditorPanel, EditorState};
use crate::physics_debug::PhysicsDebugRenderer;
use crate::play_mode::PlayMode;
use crate::viewport::ViewportPanel;
use std::process::Command;
use asset::database::AssetDatabase;
use asset::meta;
use avatar::AnimationMarker;
use physics_manager::PhysicsManager;
use scene::prefab_manifest::{PrefabManifest, PrefabNodeDef, PrefabMeshDef};
use scene::manifest::TransformDef;
use scene::Scene;

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
    /// 资源数据库
    asset_database: AssetDatabase,
    /// Bundle 打包面板
    bundle_panel: BundlePanel,
    /// Bundle 面板是否可见
    bundle_panel_visible: bool,
    /// 游戏打包面板
    build_panel: BuildPanel,
    /// Build 面板是否可见（独立字段，避免 egui::Window::open() 借用冲突）
    build_panel_visible: bool,
    /// 统一物理管理器（仅本地）
    physics: PhysicsManager,
    /// 物理碰撞体调试渲染器
    physics_debug: PhysicsDebugRenderer,
    /// tokio 异步 Runtime
    rt: tokio::runtime::Runtime,
    /// 上次帧更新时间（用于 dt 计算）
    last_update: Option<std::time::Instant>,
    /// Undo/Redo apply 回调队列（闭包写入，Editor 消费）
    apply_queue: Rc<RefCell<Vec<(String, Point3<f32>, Vector3<f32>, Vector3<f32>)>>>,
    /// 上一条变换命令的 entity_id（用于合并连续拖拽）
    last_transform_entity: Option<String>,
    /// 上一条变换命令的 old 值（合并时保持不变）
    last_transform_old: Option<(Point3<f32>, Vector3<f32>, Vector3<f32>)>,
    /// 物理模拟开关（编辑器全局控制）
    physics_enabled: bool,
    /// 动画面板
    animation_panel: AnimationPanel,
    /// 可选场景引用（供动画预览等使用）
    scene: Option<Scene>,
    /// 编辑器是否请求关闭（供 DesktopApp 检查）
    pub close_requested: bool,
}

impl Editor {
    /// 从项目路径打开编辑器。
    pub fn open(project_path: String, render_state: Option<egui_wgpu::RenderState>) -> Self {
        let state = EditorState::new(project_path.clone());
        let rt = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
        .expect("Failed to create tokio runtime");

        // 创建本地物理世界并加载场景碰撞体
        let mut physics = PhysicsManager::new([0.0, -9.81, 0.0]);
        let manifest_path = format!("{}/.scene.json", project_path);
        physics.load_scene(&manifest_path);

        // 初始化资源数据库（扫描 assets 目录，自动生成 .meta 文件）
        let asset_database = match AssetDatabase::open(&state.project_path) {
            Ok(db) => db,
            Err(e) => {
                eprintln!("[Editor] AssetDatabase init failed: {e}");
                AssetDatabase::new_empty(&state.project_path)
            }
        };

        Self {
            state,
            command_history: CommandHistory::default(),
            play_mode: PlayMode::new(),
            hierarchy: HierarchyPanel::new(),
            viewport: {
                let mut vp = ViewportPanel::new();
                vp.render_state = render_state;
                vp
            },
            inspector: InspectorPanel::new(),
            asset_browser: AssetBrowser::new(),
            gltf_import_dialog: GltfImportDialog::new(),
            asset_needs_scan: true,
            asset_database,
            bundle_panel: BundlePanel::new(),
            bundle_panel_visible: false,
            build_panel: BuildPanel::new(),
            build_panel_visible: false,
            physics,
            physics_debug: PhysicsDebugRenderer::new(),
            rt,
            last_update: None,
            apply_queue: Rc::new(RefCell::new(Vec::new())),
            last_transform_entity: None,
            last_transform_old: None,
            physics_enabled: true,
            animation_panel: AnimationPanel::new(),
            scene: None,
            close_requested: false,
        }
    }

    /// 每帧调用，渲染完整的编辑器 UI。
    pub fn update(&mut self, ctx: &egui::Context) {
        // 检查视口关闭请求（系统 X 按钮）
        if ctx.input(|i| i.viewport().close_requested()) {
            self.close_requested = true;
        }

        // 计算每帧时间增量
        let now = std::time::Instant::now();
        let dt = self
            .last_update
            .map(|t| now.duration_since(t).as_secs_f32())
            .unwrap_or(0.016);
        self.last_update = Some(now);

        // 物理步进（本地物理）
        if self.physics_enabled && self.state.mode.is_editing() {
            self.physics.step(dt);
            if self.physics_debug.enabled {
                let bodies = self.physics.get_body_snapshots();
                self.state.physics_debug_bodies = bodies.clone();
                self.physics_debug.update(bodies);
            }
        }
        // 1. 快捷键始终生效
        self.handle_shortcuts(ctx);

        // 1.1 同步动画数据到 EditorState（供 AnimationPanel 使用）
        self.sync_animation_data();

        // 1.2 动画面板预览驱动
        self.update_animation_preview(dt);

        // 1.3 处理面板请求的 Prefab 操作
        self.process_prefab_actions();

        // 1.5 资源数据库扫描
        if self.asset_needs_scan {
            let report = self.asset_database.refresh();
            if !report.errors.is_empty() {
                for err in &report.errors {
                    eprintln!("[Editor] Asset scan error: {err}");
                }
            }
            eprintln!(
                "[Editor] Asset scan: {} new, {} removed, {} updated",
                report.new_assets, report.removed, report.updated
            );
            self.asset_browser.scan_directory(&self.asset_database);
            self.asset_needs_scan = false;
        }

        // 2. 同步可拾取对象列表到视口
        self.viewport.pickable_objects.clear();
        for (entity_id, &(pos, _, scl)) in &self.state.transform_cache {
            let center = cgmath::Point3::new(pos[0], pos[1], pos[2]);
            let half = cgmath::Vector3::new(
                (scl[0] * 0.5).max(0.5),
                (scl[1] * 0.5).max(0.5),
                (scl[2] * 0.5).max(0.5),
            );
            let aabb = math::AABB::new(center - half, center + half);
            self.viewport.pickable_objects.push((entity_id.clone(), center, aabb));
        }

        // 3. 编辑器主布局（菜单栏 + 侧栏 + 视口 + 底部栏）
        self.show_editor_layout(ctx);

        // 3.5 Bundle 打包面板（浮动窗口）
        if self.bundle_panel_visible {
            egui::Window::new("Bundle Builder")
                .open(&mut self.bundle_panel_visible)
                .default_pos([100.0, 100.0])
                .default_width(300.0)
                .show(ctx, |ui| {
                    self.bundle_panel.show_panel(ui, &self.asset_database);
                });
        }

        // 3.5.5 Build 面板轮询（每帧 try_recv 异步构建进度）
        self.build_panel.poll();

        // 3.5.6 Build 面板渲染（浮动窗口）
        let mut build_visible = self.build_panel_visible;
        if build_visible {
            egui::Window::new("Build Game")
                .open(&mut build_visible)
                .default_pos([200.0, 100.0])
                .default_width(420.0)
                .default_height(360.0)
                .show(ctx, |ui| {
                    self.build_panel.show_panel(ui, &self.state.project_path);
                });
            self.build_panel_visible = build_visible;
        }

        // 3.6 动画面板（浮动窗口）
        if self.state.panel_layer.is_visible(&PanelLayer::Animation) {
            let mut open = true;
            egui::Window::new("Animation")
                .open(&mut open)
                .default_pos([100.0, 600.0])  // 调整到底部左侧
                .default_size([700.0, 280.0])
                .show(ctx, |ui| {
                    self.animation_panel.show(ui, &mut self.state);
                });
            if !open {
                self.state.panel_layer.set_visible(PanelLayer::Animation, false);
            }
        }

        // 4. GLTF 导入对话框（模态，始终检测）
        let was_visible = self.gltf_import_dialog.visible;
        self.gltf_import_dialog.show_dialog(ctx, &mut self.state, &mut self.asset_database);
        // 对话框关闭时刷新资源浏览器
        if was_visible && !self.gltf_import_dialog.visible && self.gltf_import_dialog.import_success {
            self.asset_needs_scan = true;
            if let Some(import_name) = self.gltf_import_dialog.imported_name.clone() {
                let project_path = self.state.project_path.clone();
                self.load_imported_scene(&project_path, &import_name);
            }
        }

        // 5. 处理 Inspector 写回的变换变更 -> 推入 CommandHistory
        self.process_pending_transform();
        // 6. 消费 Undo/Redo 回调产生的 apply 事件
        self.process_apply_queue();
    }

    /// 保存当前场景到磁盘。
    pub fn save_scene(&mut self) -> Result<(), String> {
        // TODO: Implement full scene serialization and save.
        // Currently the scene hierarchy and transform cache are in-memory only.
        // A complete implementation should:
        //   1. Serialize the hierarchy tree + transform_cache to a scene manifest
        //   2. Write the manifest to `{project_path}/.scene.json`
        //   3. Update the asset database if needed
        eprintln!("[Editor] Save scene: not yet implemented (stub)");
        Ok(())
    }

    // -------------------------------------------------------------------
    // 场景导入
    // -------------------------------------------------------------------

    /// After GLTF import: parse manifest, walk GLTF node tree, populate hierarchy + transform cache.
    fn load_imported_scene(&mut self, project_path: &str, name: &str) {
        let mpath = format!("{}/assets/{}.scene.json", project_path, name);
        let content = match std::fs::read_to_string(&mpath) {
            Ok(c) => c,
            Err(e) => { eprintln!("[Editor] Cannot read manifest: {e}"); return; }
        };
        let manifest: scene::manifest::SceneManifest = match serde_json::from_str(&content) {
            Ok(m) => m,
            Err(e) => { eprintln!("[Editor] Cannot parse manifest: {e}"); return; }
        };

        for model in &manifest.models {
            let gltf_abs = format!("{}/{}", project_path, model.path);
            let f = match std::fs::File::open(&gltf_abs) {
                Ok(f) => f,
                Err(e) => { eprintln!("[Editor] Cannot open GLTF {gltf_abs}: {e}"); continue; }
            };
            let reader = std::io::BufReader::new(f);
            let document: gltf::Gltf = match gltf::Gltf::from_reader(reader) {
                Ok(d) => d,
                Err(e) => { eprintln!("[Editor] Cannot parse GLTF: {e}"); continue; }
            };

            // 查找该 GLTF 模型在 AssetDatabase 中的 UUID（用于 Prefab 导出）
            let model_uuid = self.asset_database
                .entry_by_path(&model.path)
                .map(|e| e.uuid.clone());

            fn walk_gltf_node(
                node: &gltf::Node,
                parent_id: Option<String>,
                hierarchy: &mut HierarchyPanel,
                tx_cache: &mut std::collections::HashMap<String, ([f32;3],[f32;3],[f32;3])>,
                phys_cache: &mut std::collections::HashMap<String, scene::manifest::PhysicsComponentDef>,
                navmesh_cache: &mut std::collections::HashMap<String, scene::manifest::NavMeshComponentDef>,
                name_cache: &mut std::collections::HashMap<String, String>,
                mesh_entities: &mut std::collections::HashSet<String>,
                model_uuid: Option<String>,
                physics: Option<scene::manifest::PhysicsComponentDef>,
                navmesh: Option<scene::manifest::NavMeshComponentDef>,
            ) {
                let eid = format!("node_{}", node.index());
                let nname = node.name().unwrap_or("Node").to_string();
                let has_mesh = node.mesh().is_some();
                let ntype = if has_mesh { NodeType::Mesh } else { NodeType::Empty };

                let cids: Vec<String> = node.children().map(|c| {
                    let cid = format!("node_{}", c.index());
                    walk_gltf_node(&c, Some(eid.clone()), hierarchy, tx_cache, phys_cache, navmesh_cache, name_cache, mesh_entities, model_uuid.clone(), physics.clone(), navmesh.clone());
                    cid
                }).collect();

                // Mesh 节点记录其来源 GLTF 模型的 UUID
                let asset_uuid = if ntype == NodeType::Mesh {
                    model_uuid.clone()
                } else {
                    None
                };

                hierarchy.add_scene_node(SceneNodeData {
                    id: eid.clone(),
                    name: nname.clone(),
                    children: cids,
                    parent: parent_id,
                    visible: true,
                    locked: false,
                    node_type: ntype,
                    asset_source_uuid: asset_uuid,
                    prefab_ref_uuid: None,
                    physics: physics.clone(),
                    navmesh: navmesh.clone(),
                });

                tx_cache.entry(eid.clone()).or_insert((
                    [0.0, 0.0, 0.0],
                    [0.0, 0.0, 0.0],
                    [1.0, 1.0, 1.0],
                ));
                if let Some(ref phys) = physics {
                    phys_cache.entry(eid.clone()).or_insert_with(|| phys.clone());
                }
                if let Some(ref nm) = navmesh {
                    navmesh_cache.entry(eid.clone()).or_insert_with(|| nm.clone());
                }
                name_cache.entry(eid.clone()).or_insert_with(|| nname.clone());
                if has_mesh {
                    mesh_entities.insert(eid.clone());
                }
            }

            for gltf_scene in document.scenes() {
                for node in gltf_scene.nodes() {
                    walk_gltf_node(&node, None, &mut self.hierarchy, &mut self.state.transform_cache, &mut self.state.physics_component_cache, &mut self.state.navmesh_component_cache, &mut self.state.name_cache, &mut self.state.mesh_entities, model_uuid.clone(), model.effective_physics(), model.navmesh.clone());
                }
            }
            eprintln!("[Editor] Loaded '{}' into hierarchy", model.id);
        }

        self.state.selected_entity = None;
    }

        // -------------------------------------------------------------------
    // 菜单栏
    // -------------------------------------------------------------------

    fn show_menu_bar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("editor_top_bar").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                // --- Left: Menu buttons ---
                ui.menu_button("File", |ui| {
                    if ui.button("Import GLTF...").clicked() {
                        self.gltf_import_dialog.open();
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui.button("New Scene").clicked() { ui.close_menu(); }
                    if ui.button("Open Scene...").clicked() { ui.close_menu(); }
                    if ui.button("Save Scene").clicked() { let _ = self.save_scene(); ui.close_menu(); }
                    ui.separator();
                    if ui.button("Exit").clicked() {
                        self.close_requested = true;
                        ui.close_menu();
                    }
                });
                ui.menu_button("Edit", |ui| {
                    let can_undo = self.command_history.can_undo();
                    let undo_label = self.command_history
                        .last_undo_description()
                        .map(|d| format!("Undo {}", d))
                        .unwrap_or_else(|| "Undo".to_string());
                    if ui.add_enabled(can_undo, egui::Button::new(undo_label)).clicked() {
                        self.command_history.undo(); ui.close_menu();
                    }
                    if ui.add_enabled(self.command_history.can_redo(), egui::Button::new("Redo")).clicked() {
                        self.command_history.redo(); ui.close_menu();
                    }
                });
                ui.menu_button("View", |ui| {
                    let mut hv = self.state.panel_layer.is_visible(&PanelLayer::Hierarchy);
                    if ui.checkbox(&mut hv, "Hierarchy").clicked() { self.state.panel_layer.set_visible(PanelLayer::Hierarchy, hv); ui.close_menu(); }
                    let mut iv = self.state.panel_layer.is_visible(&PanelLayer::Inspector);
                    if ui.checkbox(&mut iv, "Inspector").clicked() { self.state.panel_layer.set_visible(PanelLayer::Inspector, iv); ui.close_menu(); }
                    let mut av = self.state.panel_layer.is_visible(&PanelLayer::AssetBrowser);
                    if ui.checkbox(&mut av, "Asset Browser").clicked() { self.state.panel_layer.set_visible(PanelLayer::AssetBrowser, av); ui.close_menu(); }
                    ui.separator();
                    let mut anim_vis = self.state.panel_layer.is_visible(&PanelLayer::Animation);
                    if ui.checkbox(&mut anim_vis, "Animation").clicked() { self.state.panel_layer.set_visible(PanelLayer::Animation, anim_vis); ui.close_menu(); }
                    let mut bv = self.bundle_panel_visible;
                    if ui.checkbox(&mut bv, "Bundle Panel").clicked() { self.bundle_panel_visible = bv; ui.close_menu(); }
                });
                ui.menu_button("Build", |ui| {
                    if ui.button("Package Game...").clicked() {
                        self.build_panel_visible = true;
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui.button("Quick Export (Windows)").clicked() {
                        self.state.pending_actions.push(EditorAction::ExportGameWindows);
                        ui.close_menu();
                    }
                });
                ui.menu_button("Help", |ui| {
                    if ui.button("About Geese Editor").clicked() { ui.close_menu(); }
                });

                ui.separator();

                // --- Right: Editor toolbar (UPBGE/Blender style) ---
                // Play/Stop
                let (label, color) = self.play_mode.button_ui();
                if ui.add_sized([54.0, 22.0], egui::Button::new(egui::RichText::new(label).color(color))).clicked() {
                    self.toggle_play_mode();
                }

                ui.separator();

                // Gizmo mode
                ui.selectable_value(&mut self.viewport.gizmo_mode, crate::viewport::GizmoMode::Translate, "W");
                ui.selectable_value(&mut self.viewport.gizmo_mode, crate::viewport::GizmoMode::Rotate, "E");
                ui.selectable_value(&mut self.viewport.gizmo_mode, crate::viewport::GizmoMode::Scale, "R");

                // Snap toggle
                let snap_label = if self.viewport.gizmo_interaction.snap_enabled { "Snap ON" } else { "Snap OFF" };
                if ui.add_sized([56.0, 22.0], egui::Button::new(snap_label)).clicked() {
                    self.viewport.gizmo_interaction.snap_enabled = !self.viewport.gizmo_interaction.snap_enabled;
                }

                ui.separator();

                // Physics Enable/Disable
                let phy_label = if self.physics_enabled { "Physics ON" } else { "Physics OFF" };
                if ui.add_sized([72.0, 22.0], egui::Button::new(phy_label)).clicked() {
                    self.physics_enabled = !self.physics_enabled;
                    // 联动：同步到场景的 physics_enabled 标志
                    // （当 desktop 应用读取 Editor 时，会通过此标志同步到 Scene）
                    eprintln!("[Editor] Physics {}", if self.physics_enabled { "enabled" } else { "disabled" });
                }

                // Physics Debug
                let dbg_label = if self.physics_debug.enabled { "Debug ON" } else { "Debug OFF" };
                if ui.add_sized([60.0, 22.0], egui::Button::new(dbg_label)).clicked() {
                    self.physics_debug.toggle();
                }
            });
        });
    }

    // -------------------------------------------------------------------
    // 全屏视口
    // -------------------------------------------------------------------



    // -------------------------------------------------------------------
    // 工具栏


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
            // Close window: Ctrl+W or Alt+F4
            if input.modifiers.ctrl && input.key_pressed(egui::Key::W) {
                self.close_requested = true;
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
            let _ = self.save_scene();
        }
    }

    // -------------------------------------------------------------------
    // 浮动面板
    // -------------------------------------------------------------------

    /// 渲染停靠式编辑器布局（菜单栏 + SidePanel 侧栏 + CentralPanel 视口 + 底部栏）。
    fn show_editor_layout(&mut self, ctx: &egui::Context) {
        // 菜单栏
        if !self.state.mode.is_playing() || self.state.ui_visible {
            self.show_menu_bar(ctx);
        }

        // 主布局：侧栏 + 视口 + 底部栏
        EditorLayout::render(
            ctx,
            &mut self.state,
            &mut self.hierarchy,
            &mut self.viewport,
            &mut self.inspector,
            &mut self.asset_browser,
        );
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

    /// 同步 Scene 中的动画数据到 EditorState。
    fn sync_animation_data(&mut self) {
        let Some(ref scene) = self.scene else { return };

        self.state.animation_clips.clear();
        for (i, clip) in scene.animations.iter().enumerate() {
            self.state.animation_clips.push((
                clip.name.clone().unwrap_or_else(|| format!("Clip {}", i)),
                clip.duration,
                i,
            ));
        }

        self.state.animation_markers.clear();
        for clip in &scene.animations {
            let markers: Vec<(f32, String)> = clip
                .markers
                .iter()
                .map(|m| (m.time, m.name.clone()))
                .collect();
            self.state.animation_markers.push(markers);
        }
    }

    /// 驱动动画预览播放。
    fn update_animation_preview(&mut self, dt: f32) {
        self.animation_panel.update_timer(dt);

        let Some(ref mut scene) = self.scene else { return };
        if !self.animation_panel.preview_playing {
            return;
        }
        let Some(clip_idx) = self.animation_panel.selected_clip else {
            return;
        };
        let Some(clip) = scene.animations.get(clip_idx) else {
            return;
        };
        let _ = clip;

        let mut player = avatar::AnimationPlayer::new(clip_idx);
        player.time = self.animation_panel.preview_time;
        player.playing = self.animation_panel.preview_playing;
        player.speed = self.animation_panel.preview_speed;
        player.looping = self.animation_panel.preview_looping;
        scene.update_animation(&mut player, dt);
        self.animation_panel.preview_time = player.time;

        // 收集触发事件
        let events = scene.drain_marker_events();
        if !events.is_empty() {
            self.animation_panel.on_markers_fired(&events);
        }
    }

    /// 处理动画标记的增删操作。
    fn handle_modify_animation_marker(
        &mut self,
        clip_index: usize,
        time: f32,
        name: String,
        remove: bool,
    ) {
        let Some(ref mut scene) = self.scene else { return };
        let Some(clip) = scene.animations.get_mut(clip_index) else {
            return;
        };
        if remove {
            clip.markers.retain(|m| m.time != time || m.name != name);
        } else {
            // 避免重复
            if !clip.markers.iter().any(|m| m.time == time && m.name == name) {
                clip.markers.push(AnimationMarker { time, name });
                // 保持按时间排序
                clip
                    .markers
                    .sort_by(|a, b| a.time.partial_cmp(&b.time).unwrap());
            }
        }
    }

    /// 处理面板请求的 Prefab 操作（Save as Prefab / Instantiate Prefab）。
    fn process_prefab_actions(&mut self) {
        let actions: Vec<EditorAction> = self.state.pending_actions.drain(..).collect();
        for action in actions {
            match action {
                EditorAction::SaveAsPrefab { node_id } => {
                    self.handle_save_as_prefab(&node_id);
                }
                EditorAction::InstantiatePrefab { prefab_uuid, position, parent_node_id } => {
                    self.handle_instantiate_prefab(&prefab_uuid, position, parent_node_id);
                }
                EditorAction::ToggleCharacterController { node_id, enabled, move_speed, jump_impulse, air_control, half_height, radius } => {
                    // 角色控制器切换：由 Editor 处理物理集成
                    // TODO: 当 Editor 持有 Scene 引用时，调用 scene.character_physics 相关 API
                    eprintln!(
                        "[Editor] ToggleCharacterController: node={}, enabled={}, move_speed={}, jump_impulse={}, air_control={}, half_height={}, radius={}",
                        node_id, enabled, move_speed, jump_impulse, air_control, half_height, radius
                    );
                }
                EditorAction::ModifyAnimationMarker { clip_index, time, name, remove } => {
                    self.handle_modify_animation_marker(clip_index, time, name, remove);
                }
                EditorAction::SetPhysicsComponent { node_id, component } => {
                    match component {
                        Some(ref comp) => {
                            self.state.physics_component_cache.insert(node_id.clone(), comp.clone());
                        }
                        None => {
                            self.state.physics_component_cache.remove(&node_id);
                        }
                    }
                    eprintln!("[Editor] SetPhysicsComponent: node={node_id}, enabled={}", component.is_some());
                }
                EditorAction::SetNavMeshComponent { node_id, component } => {
                    match component {
                        Some(ref comp) => {
                            self.state.navmesh_component_cache.insert(node_id.clone(), comp.clone());
                        }
                        None => {
                            self.state.navmesh_component_cache.remove(&node_id);
                        }
                    }
                    eprintln!("[Editor] SetNavMeshComponent: node={node_id}, enabled={}", component.is_some());
                }
                EditorAction::RenameEntity { node_id, new_name } => {
                    self.state.name_cache.insert(node_id.clone(), new_name.clone());
                    if let Some(node) = self.hierarchy.tree_mut().get_mut(&node_id) {
                        node.name = new_name;
                    }
                }
                EditorAction::ToggleVisibility { node_id, visible } => {
                    if let Some(node) = self.hierarchy.tree_mut().get_mut(&node_id) {
                        node.visible = visible;
                    }
                }
                EditorAction::ExportGameWindows => {
                    self.handle_export_windows();
                }
                EditorAction::ExportGameAndroid => {
                    self.build_panel.open_for_target(crate::build_panel::BuildTarget::Android);
                    self.build_panel_visible = true;
                }
                EditorAction::OpenBuildPanel => {
                    self.build_panel_visible = true;
                }
            }
        }
    }

    /// 导出游戏为独立 Windows 可执行文件（Quick Export）。
    /// 先执行 cargo build --release -p game_runtime，再复制产物。
    fn handle_export_windows(&mut self) {
        let project_name = std::path::Path::new(&self.state.project_path)
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "game".to_string());

        let export_dir = format!("{}/export/{}", self.state.project_path, project_name);

        self.state.status_message = Some("Building game_runtime (release)...".into());

        // 定位项目根目录
        let root_path = match std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
        {
            Some(p) => p.to_path_buf(),
            None => {
                self.state.status_message = Some("Cannot determine project root.".into());
                return;
            }
        };
        let manifest = root_path.join("crates").join("game_runtime").join("Cargo.toml");

        // 执行 cargo build
        let build_status = Command::new("cargo")
            .args(["build", "--release", "-p", "game_runtime", "--manifest-path"])
            .arg(&manifest)
            .status();

        match build_status {
            Ok(s) if s.success() => {
                // 构建成功，继续复制
            }
            Ok(s) => {
                self.state.status_message = Some(format!(
                    "Build failed with exit code: {}",
                    s.code().map(|c| c.to_string()).unwrap_or_else(|| "unknown".into())
                ));
                return;
            }
            Err(e) => {
                self.state.status_message = Some(format!("Failed to start cargo: {e}"));
                return;
            }
        }

        // 创建输出目录
        let _ = std::fs::create_dir_all(&export_dir);
        let _ = std::fs::create_dir_all(format!("{}/assets", export_dir));
        let _ = std::fs::create_dir_all(format!("{}/config", export_dir));

        // 复制资产
        let assets_src = format!("{}/assets", self.state.project_path);
        let assets_dst = format!("{}/assets", export_dir);
        if let Err(e) = copy_dir_recursive(&assets_src, &assets_dst) {
            eprintln!("[Editor] copy assets failed: {e}");
        }

        // 复制 config
        let config_src = format!("{}/config", self.state.project_path);
        let config_dst = format!("{}/config", export_dir);
        if let Err(e) = copy_dir_recursive(&config_src, &config_dst) {
            eprintln!("[Editor] copy config failed: {e}");
        }

        // 复制 game_runtime .exe
        let exe_src = root_path
            .join("crates")
            .join("game_runtime")
            .join("target")
            .join("release")
            .join("geese_game.exe");
        let exe_dst = format!("{}/{}.exe", export_dir, project_name);

        if exe_src.exists() {
            if let Err(e) = std::fs::copy(&exe_src, &exe_dst) {
                eprintln!("[Editor] copy exe failed: {e}");
                self.state.status_message = Some(format!("Export failed: {e}"));
                return;
            }
            self.state.status_message = Some(format!(
                "Exported to {}/export/{}/{}.exe",
                self.state.project_path, project_name, project_name
            ));
        } else {
            self.state.status_message = Some(format!(
                "geese_game.exe not found at {}. Build may have failed silently.",
                exe_src.display()
            ));
        }
    }

    /// 将选中节点及其子树保存为 .prefab.json 文件。
    fn handle_save_as_prefab(&mut self, node_id: &str) {
        let node = match self.hierarchy.tree().get(node_id) {
            Some(n) => n.clone(),
            None => {
                eprintln!("[Editor] SaveAsPrefab: node '{}' not found", node_id);
                return;
            }
        };

        // 收集子树中所有节点
        let mut all_node_ids = vec![node_id.to_string()];
        self.collect_subtree_nodes(node_id, &mut all_node_ids);

        // 构建 PrefabManifest
        let mut prefab_nodes: Vec<PrefabNodeDef> = Vec::new();
        let mut id_to_index: std::collections::HashMap<String, usize> = std::collections::HashMap::new();

        for (idx, nid) in all_node_ids.iter().enumerate() {
            id_to_index.insert(nid.clone(), idx);
        }

        for nid in &all_node_ids {
            let n = match self.hierarchy.tree().get(nid) {
                Some(n) => n.clone(),
                None => continue,
            };

            // 获取变换
            let transform = self
                .state
                .transform_cache
                .get(nid)
                .map(|&(pos, rot, scl)| TransformDef {
                    translation: pos,
                    rotation: rot,
                    scale: scl,
                })
                .unwrap_or_default();

            let children: Vec<usize> = n
                .children
                .iter()
                .filter_map(|cid| id_to_index.get(cid).copied())
                .collect();

            // 根据节点类型和资产来源确定 mesh 定义
            // 如果节点有 prefab_ref_uuid，则保存为嵌套 prefab 引用（mesh 与 prefab_ref 互斥）
            let (mesh, prefab_ref, overrides) = if let Some(ref prefab_uuid) = n.prefab_ref_uuid {
                // 嵌套 Prefab 引用：保留 prefab_ref 而非 mesh
                (None, Some(prefab_uuid.clone()), None)
            } else {
                let mesh = match n.node_type {
                    NodeType::Mesh => {
                        // 优先使用 asset_source_uuid（来自 GLTF 导入的模型引用）
                        if let Some(ref model_uuid) = n.asset_source_uuid {
                            Some(PrefabMeshDef::ModelRef {
                                model_uuid: model_uuid.clone(),
                                mesh_name: None,
                            })
                        } else {
                            // 无 GLTF 来源，使用程序化占位
                            Some(PrefabMeshDef::Procedural {
                                object_type: "cube".to_string(),
                                color: [0.5, 0.5, 0.5],
                                dimensions: [1.0, 1.0, 1.0],
                            })
                        }
                    }
                    _ => None,
                };
                (mesh, None, None)
            };

            // 获取物理组件和 NavMesh 组件
            let physics = self.state.physics_component_cache.get(nid).cloned();
            let navmesh = self.state.navmesh_component_cache.get(nid).cloned();

            prefab_nodes.push(PrefabNodeDef {
                name: n.name.clone(),
                transform,
                children,
                mesh,
                prefab_ref,
                overrides,
                physics,
                navmesh,
                _body_kind: None,
            });
        }

        let root_indices: Vec<usize> = vec![0]; // 选中的节点是根

        let manifest = PrefabManifest {
            version: "1.0".to_string(),
            name: node.name.clone(),
            nodes: prefab_nodes,
            root_nodes: root_indices,
        };

        // 写入文件
        let sanitized_name = node.name.replace(|c: char| !c.is_alphanumeric() && c != '_' && c != '-', "_");
        let prefab_path = format!(
            "{}/assets/{}.prefab.json",
            self.state.project_path, sanitized_name
        );

        // 文件覆盖保护：如果文件已存在，添加后缀避免覆盖
        let prefab_path = if std::path::Path::new(&prefab_path).exists() {
            let base = format!(
                "{}/assets/{}_copy",
                self.state.project_path, sanitized_name
            );
            eprintln!(
                "[Editor] Prefab file already exists at '{}', saving as '{}_copy.prefab.json' instead",
                prefab_path, base
            );
            format!("{}.prefab.json", base)
        } else {
            prefab_path
        };

        match serde_json::to_string_pretty(&manifest) {
            Ok(json) => {
                if let Err(e) = std::fs::write(&prefab_path, json) {
                    eprintln!("[Editor] Failed to write prefab '{}': {}", prefab_path, e);
                    return;
                }
                eprintln!("[Editor] Prefab saved to '{}'", prefab_path);

                // 生成 .meta 文件
                let prefab_abs = std::path::Path::new(&prefab_path);
                if let Err(e) = meta::create_meta_for(prefab_abs) {
                    eprintln!("[Editor] Failed to create meta for prefab: {}", e);
                }

                // 触发资源重新扫描
                self.asset_needs_scan = true;
            }
            Err(e) => {
                eprintln!("[Editor] Failed to serialize prefab: {}", e);
            }
        }
    }

    /// 在指定位置实例化 Prefab（递归实例化所有子节点）。
    /// `parent_node_id` 指定新实例的父节点（None 为根节点）。
    fn handle_instantiate_prefab(&mut self, prefab_uuid: &str, position: [f32; 3], parent_node_id: Option<String>) {
        let entry = match self.asset_database.entry_by_uuid(prefab_uuid) {
            Some(e) => e.clone(),
            None => {
                eprintln!("[Editor] Prefab UUID '{}' not found in database", prefab_uuid);
                return;
            }
        };

        let prefab_abs_path = self.asset_database.project_root().join(&entry.path);
        let manifest = match scene::prefab_loader::load_prefab_manifest(
            &prefab_abs_path.to_string_lossy(),
        ) {
            Ok(m) => m,
            Err(e) => {
                eprintln!("[Editor] Failed to load prefab '{}': {}", entry.path, e);
                return;
            }
        };

        // 构建 manifest 索引 → 生成的实体 ID 映射
        let node_count = manifest.nodes.len();
        let mut idx_to_eid: Vec<String> = Vec::with_capacity(node_count);
        for _ in 0..node_count {
            idx_to_eid.push(format!("prefab_{}", uuid::Uuid::new_v4()));
        }

        // 第一遍：为每个节点创建 SceneNodeData（暂不设置 children，先收集信息）
        let mut node_datas: Vec<SceneNodeData> = Vec::with_capacity(node_count);

        for (idx, node_def) in manifest.nodes.iter().enumerate() {
            let eid = idx_to_eid[idx].clone();
            let has_mesh = node_def.mesh.is_some();
            let ntype = if has_mesh {
                NodeType::Mesh
            } else {
                NodeType::Empty
            };

            // 子节点的实际实体 ID 列表（第二遍填充后会被重新设置）
            let children_eids: Vec<String> = node_def
                .children
                .iter()
                .map(|&child_idx| idx_to_eid[child_idx].clone())
                .collect();

            // 父节点实体 ID
            let parent_eid = if manifest.root_nodes.contains(&idx) {
                // 根节点：使用指定的 parent_node_id
                parent_node_id.clone()
            } else {
                // 非根节点：找到第一个父节点引用
                manifest.nodes.iter().enumerate()
                    .find(|(_pidx, pdef)| pdef.children.contains(&idx))
                    .map(|(pidx, _)| idx_to_eid[pidx].clone())
            };

            // 提取 asset_source_uuid（如果 mesh 是 ModelRef 类型）
            let asset_uuid = match &node_def.mesh {
                Some(PrefabMeshDef::ModelRef { model_uuid, .. }) => Some(model_uuid.clone()),
                _ => None,
            };

            // 检查是否有嵌套 prefab_ref
            let prefab_ref_uuid = node_def.prefab_ref.clone();

            node_datas.push(SceneNodeData {
                id: eid.clone(),
                name: node_def.name.clone(),
                children: children_eids,
                parent: parent_eid,
                visible: true,
                locked: false,
                node_type: ntype,
                asset_source_uuid: asset_uuid,
                prefab_ref_uuid,
                physics: node_def.effective_physics(),
                navmesh: node_def.navmesh.clone(),
            });

            // 变换：根节点使用世界位置，子节点使用 manifest 中的变换
            let node_pos = if manifest.root_nodes.contains(&idx) {
                position
            } else {
                node_def.transform.translation
            };
            self.state.transform_cache.insert(
                eid.clone(),
                (node_pos, node_def.transform.rotation, node_def.transform.scale),
            );
            if has_mesh {
                self.state.mesh_entities.insert(eid);
            }
        }

        // 第二遍：将所有节点添加到层级树
        for node_data in node_datas {
            self.hierarchy.add_scene_node(node_data);
        }

        eprintln!(
            "[Editor] Instantiated prefab '{}' ({}) with {} nodes at {:?}",
            manifest.name, prefab_uuid, node_count, position
        );
    }

    /// 递归收集节点子树中所有节点 ID。
    /// `depth` 为当前递归深度，超过上限时终止以防栈溢出。
    fn collect_subtree_nodes_inner(&self, node_id: &str, out: &mut Vec<String>, depth: usize) {
        const MAX_DEPTH: usize = 2048;
        if depth > MAX_DEPTH {
            eprintln!("[Editor] collect_subtree_nodes: depth {} exceeded max {}, possible cycle at '{}'", depth, MAX_DEPTH, node_id);
            return;
        }
        if let Some(node) = self.hierarchy.tree().get(node_id) {
            for child_id in &node.children {
                // 跳过自引用
                if child_id == node_id {
                    continue;
                }
                out.push(child_id.clone());
                self.collect_subtree_nodes_inner(child_id, out, depth + 1);
            }
        }
    }

    /// 递归收集节点子树中所有节点 ID。
    fn collect_subtree_nodes(&self, node_id: &str, out: &mut Vec<String>) {
        self.collect_subtree_nodes_inner(node_id, out, 0);
    }

    /// 切换 Play/Stop 模式。
    fn toggle_play_mode(&mut self) {
        if self.play_mode.is_playing {
            // Stop: 恢复编辑模式
            if let Some(mut snapshot) = self.play_mode.stop() {
                PlayMode::restore_panel_state(&snapshot, &mut self.state);
                self.viewport.camera.set_yaw(snapshot.camera_yaw);
                self.viewport.camera.set_pitch(snapshot.camera_pitch);
                self.viewport.camera.set_distance(snapshot.camera_distance);
                self.state.selected_entity = snapshot.selected_entity.take();
            }
            // 重新创建本地物理世界并加载场景碰撞体
            self.physics = PhysicsManager::new([0.0, -9.81, 0.0]);
            {
                let manifest_path = format!("{}/.scene.json", self.state.project_path);
                self.physics.load_scene(&manifest_path);
            }
            self.physics_debug.enabled = false;
            self.state.mode = EditorMode::Edit;
            self.state.panel_layer.set_edit_alpha();
        } else {
            // Play: 进入播放模式（使用本地物理）
            self.play_mode.play(
                &self.state,
                self.viewport.camera.yaw(),
                self.viewport.camera.pitch(),
                self.viewport.camera.distance(),
            );

            // 创建本地物理世界并加载场景碰撞体
            self.physics = PhysicsManager::new([0.0, -9.81, 0.0]);
            let manifest_path = format!("{}/.scene.json", self.state.project_path);
            self.physics.load_scene(&manifest_path);

            self.state.mode = EditorMode::Play;
            self.state.selected_entity = None;
            self.state.panel_layer.set_play_alpha();
        }
    }
}

/// 递归复制目录。
fn copy_dir_recursive(src: &str, dst: &str) -> std::io::Result<()> {
    let src_path = std::path::Path::new(src);
    if !src_path.is_dir() {
        return Ok(());
    }
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src_path)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let src_file = entry.path();
        let dst_file = std::path::Path::new(dst).join(entry.file_name());
        if file_type.is_dir() {
            copy_dir_recursive(&src_file.to_string_lossy(), &dst_file.to_string_lossy())?;
        } else {
            std::fs::copy(&src_file, &dst_file)?;
        }
    }
    Ok(())
}
