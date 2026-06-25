//! 3D 场景视口（Viewport）。
//!
//! 提供：
//! - [`OrbitCamera`]：编辑器摄像机（右键旋转/中键平移/滚轮缩放）
//! - [`ViewportPanel`]：集成到编辑器面板系统的 3D 视口
//! - 射线拾取（Ray Picking）：屏幕坐标 → 世界空间射线
//! - 编辑器网格和世界坐标轴指示器
//! - 物理碰撞体调试渲染（wireframe）

use crate::editor_mode::EditorMode;
use crate::gizmo::{GizmoInteraction, draw_gizmo};
use crate::panel_layer::PanelLayer;
use crate::panels::{EditorPanel, EditorState, PendingTransform};
use cgmath::{InnerSpace, Matrix4, Point3, SquareMatrix, Vector3, Vector4, perspective, Rad};
use math::AABB;
use render::grid::build_grid_vertices;

// ---------------------------------------------------------------------------
// OrbitCamera - 编辑器摄像机
// ---------------------------------------------------------------------------

/// 编辑器轨道摄像机。
///
/// 交互方式：
/// - 右键拖拽：绕焦点旋转（yaw/pitch）
/// - 中键拖拽：平移焦点
/// - 滚轮：缩放距离
#[derive(Debug, Clone)]
pub struct OrbitCamera {
    /// 摄像机围绕的焦点
    pub focal_point: Point3<f32>,
    /// 水平旋转角（弧度）
    pub yaw: f32,
    /// 垂直旋转角（弧度），范围 [-89°, 89°]
    pub pitch: f32,
    /// 摄像机到焦点的距离
    pub distance: f32,
    /// 最小缩放距离
    pub min_distance: f32,
    /// 最大缩放距离
    pub max_distance: f32,
    /// 视口宽高比
    pub aspect_ratio: f32,
    /// 垂直视场角（弧度）
    pub fov: f32,
    /// 近裁剪面
    pub z_near: f32,
    /// 远裁剪面
    pub z_far: f32,
    /// 轨道灵敏度
    pub orbit_sensitivity: f32,
    /// 平移灵敏度
    pub pan_sensitivity: f32,
    /// 缩放灵敏度
    pub zoom_sensitivity: f32,
}

impl Default for OrbitCamera {
    fn default() -> Self {
        Self {
            focal_point: Point3::new(0.0, 0.0, 0.0),
            yaw: std::f32::consts::FRAC_PI_4,  // 45度水平角
            pitch: -0.1745,  // -10度俯角（约等于-π/18），相机与XZ平面保持10度夹角
            distance: 20.0,  // 从10.0增加到20.0，相机离地面更高
            min_distance: 0.5,
            max_distance: 200.0,
            aspect_ratio: 16.0 / 9.0,
            fov: std::f32::consts::FRAC_PI_4,
            z_near: 0.1,
            z_far: 500.0,
            orbit_sensitivity: 0.005,
            pan_sensitivity: 0.01,
            zoom_sensitivity: 0.5,
        }
    }
}

impl OrbitCamera {
    /// 计算摄像机在世界空间中的位置。
    pub fn eye_position(&self) -> Point3<f32> {
        // Orbit相机：相机围绕焦点旋转，从上方俯视
        // pitch为负值时相机在上方，正值时在下方
        let x = self.yaw.sin() * (-self.pitch).cos() * self.distance;
        let y = (-self.pitch).sin() * self.distance;  // pitch取反，确保负pitch时y为正（在上方）
        let z = self.yaw.cos() * (-self.pitch).cos() * self.distance;
        Point3::new(
            self.focal_point.x + x,
            self.focal_point.y + y,
            self.focal_point.z + z,
        )
    }

    /// 摄像机前向方向（相机看向的方向，从相机指向焦点）
    pub fn forward_direction(&self) -> Vector3<f32> {
        let eye = self.eye_position();
        (self.focal_point - eye).normalize()
    }

    /// 摄像机右向向量。
    pub fn right_direction(&self) -> Vector3<f32> {
        let forward = self.forward_direction();
        let world_up = Vector3::unit_y();
        forward.cross(world_up).normalize()
    }

    /// 摄像机上向向量。
    pub fn up_direction(&self) -> Vector3<f32> {
        let forward = self.forward_direction();
        let right = self.right_direction();
        right.cross(forward).normalize()
    }

    /// 视图矩阵。
    pub fn view_matrix(&self) -> Matrix4<f32> {
        let eye = self.eye_position();
        Matrix4::look_at_rh(eye, self.focal_point, Vector3::unit_y())
    }

    /// 投影矩阵。
    pub fn projection_matrix(&self) -> Matrix4<f32> {
        perspective(
            Rad(self.fov),
            self.aspect_ratio,
            self.z_near,
            self.z_far,
        )
    }

    /// 视图-投影矩阵。
    pub fn view_projection_matrix(&self) -> Matrix4<f32> {
        self.projection_matrix() * self.view_matrix()
    }

    /// 绕焦点旋转摄像机。
    pub fn orbit(&mut self, delta_x: f32, delta_y: f32) {
        self.yaw -= delta_x * self.orbit_sensitivity;
        self.pitch += delta_y * self.orbit_sensitivity;

        // 限制 pitch 防止翻转
        let pitch_limit = std::f32::consts::FRAC_PI_2 - 0.01;
        self.pitch = self.pitch.clamp(-pitch_limit, pitch_limit);
    }

    /// 平移焦点。
    pub fn pan(&mut self, delta_x: f32, delta_y: f32) {
        let right = self.right_direction();
        let up = self.up_direction();
        let pan_speed = self.distance * self.pan_sensitivity;
        self.focal_point += right * (-delta_x * pan_speed);
        self.focal_point += up * (delta_y * pan_speed);
    }

    /// 缩放距离。
    pub fn zoom(&mut self, delta: f32) {
        self.distance -= delta * self.zoom_sensitivity;
        self.distance = self.distance.clamp(self.min_distance, self.max_distance);
    }

    /// 基于鼠标垂直拖拽缩放（Unity风格Alt+右键）
    pub fn zoom_by_drag(&mut self, delta_y: f32) {
        self.distance += delta_y * self.zoom_sensitivity * 0.5;
        self.distance = self.distance.clamp(self.min_distance, self.max_distance);
    }

    /// 聚焦到指定位置。
    pub fn focus_on(&mut self, point: Point3<f32>) {
        self.focal_point = point;
    }

    /// 重置为默认视角。
    pub fn reset(&mut self) {
        self.focal_point = Point3::new(0.0, 0.0, 0.0);
        self.yaw = std::f32::consts::FRAC_PI_4;
        self.pitch = -0.1745;  // -10度，与默认值保持一致
        self.distance = 20.0;  // 与默认值保持一致
    }

    /// 从屏幕坐标生成世界空间射线。
    pub fn screen_to_world_ray(
        &self,
        screen_x: f32,
        screen_y: f32,
        viewport_width: f32,
        viewport_height: f32,
    ) -> (Point3<f32>, Vector3<f32>) {
        let vp_inv = self.view_projection_matrix()
            .invert()
            .unwrap_or_else(|| Matrix4::from_scale(1.0));

        // 将屏幕坐标映射到 NDC [-1, 1]
        let ndc_x = (screen_x / viewport_width) * 2.0 - 1.0;
        let ndc_y = 1.0 - (screen_y / viewport_height) * 2.0;

        let near_point = vp_inv * Vector4::new(ndc_x, ndc_y, -1.0, 1.0);
        let far_point = vp_inv * Vector4::new(ndc_x, ndc_y, 1.0, 1.0);

        let near = Point3::new(
            near_point.x / near_point.w,
            near_point.y / near_point.w,
            near_point.z / near_point.w,
        );
        let far = Point3::new(
            far_point.x / far_point.w,
            far_point.y / far_point.w,
            far_point.z / far_point.w,
        );

        let direction = (far - near).normalize();
        (near, direction)
    }
}

// ---------------------------------------------------------------------------
// Ray-AABB 相交测试
// ---------------------------------------------------------------------------

/// 射线与 AABB 的相交测试（slab 方法）。
/// 返回最近交点到射线原点的距离 t，若不相交返回 None。
pub fn ray_aabb_intersection(
    ray_origin: Point3<f32>,
    ray_dir: Vector3<f32>,
    aabb: &AABB,
) -> Option<f32> {
    let inv_dir = Vector3::new(
        1.0 / ray_dir.x,
        1.0 / ray_dir.y,
        1.0 / ray_dir.z,
    );

    let t1 = (aabb.min.x - ray_origin.x) * inv_dir.x;
    let t2 = (aabb.max.x - ray_origin.x) * inv_dir.x;
    let t3 = (aabb.min.y - ray_origin.y) * inv_dir.y;
    let t4 = (aabb.max.y - ray_origin.y) * inv_dir.y;
    let t5 = (aabb.min.z - ray_origin.z) * inv_dir.z;
    let t6 = (aabb.max.z - ray_origin.z) * inv_dir.z;

    let tmin = t1.min(t2).max(t3.min(t4)).max(t5.min(t6));
    let tmax = t1.max(t2).min(t3.max(t4)).min(t5.max(t6));

    if tmax < 0.0 || tmin > tmax {
        None
    } else {
        Some(if tmin >= 0.0 { tmin } else { tmax })
    }
}

// ---------------------------------------------------------------------------
// Gizmo 模式
// ---------------------------------------------------------------------------

/// 变换 Gizmo 操作模式。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GizmoMode {
    Translate,
    Rotate,
    Scale,
}

// ---------------------------------------------------------------------------
// ViewportPanel
// ---------------------------------------------------------------------------

/// 3D 视口面板。
pub struct ViewportPanel {
    /// 轨道摄像机
    pub camera: OrbitCamera,
    /// 当前 Gizmo 模式
    pub gizmo_mode: GizmoMode,
    /// 视口中上次鼠标位置（用于计算 delta）
    last_mouse_pos: Option<(f32, f32)>,
    /// 是否正在轨道旋转
    orbiting: bool,
    /// 是否正在平移
    panning: bool,
    /// 视口实际大小
    viewport_size: (f32, f32),
    /// 渲染纹理 ID（由外部设置，渲染场景到纹理后传入）
    pub rendered_texture: Option<egui::TextureId>,
    /// eframe wgpu render state（由外部 CreationContext 传入）。
   pub render_state: Option<egui_wgpu::RenderState>,
   /// 网格大小
    grid_size: f32,
    /// 网格细分数量
    grid_subdivisions: usize,
    /// GPU grid renderer (created lazily when render_state is available)
    gpu_grid: Option<GpuGridRenderer>,
    /// Gizmo 交互状态
    gizmo_interaction: GizmoInteraction,
    /// Pickable scene objects for ray-casting selection
    pub pickable_objects: Vec<(String, Point3<f32>, AABB)>,
}

impl ViewportPanel {
    pub fn new() -> Self {
        Self {
            camera: OrbitCamera::default(),
            gizmo_mode: GizmoMode::Translate,
            last_mouse_pos: None,
            orbiting: false,
            panning: false,
            viewport_size: (800.0, 600.0),
            rendered_texture: None,
            render_state: None,
            grid_size: 10.0,
            grid_subdivisions: 10,
            gpu_grid: None,
            gizmo_interaction: GizmoInteraction::new(),
            pickable_objects: Vec::new(),
        }
    }

    /// 处理视口内的鼠标输入。
    fn handle_input(&mut self, ui: &egui::Ui, response: &egui::Response, state: &mut EditorState) {
        // 播放模式下不处理编辑器摄像机输入
        if state.mode == EditorMode::Play {
            return;
        }

        // 浮动窗口遮挡检测：有窗口在鼠标下方时跳过场景交互
        if ui.ctx().wants_pointer_input() && !response.hovered() {
            return;
        }

        let pointer_pos = ui.input(|input| {
            input.pointer.hover_pos().map(|p| (p.x, p.y))
        });

        // 检查鼠标是否在视口内
        let hovered = response.hovered();

        // 处理滚动缩放
        if hovered {
            let scroll_delta = ui.input(|input| input.raw_scroll_delta);
            if scroll_delta.y != 0.0 {
                self.camera.zoom(scroll_delta.y);
            }
        }

        // 处理WASD/方向键移动相机
        if hovered {
            let move_speed = self.camera.distance * 0.02;
            ui.input(|input| {
                let mut move_vec = Vector3::new(0.0, 0.0, 0.0);
                
                // WASD控制
                if input.key_down(egui::Key::W) || input.key_down(egui::Key::ArrowUp) {
                    move_vec += self.camera.forward_direction();
                }
                if input.key_down(egui::Key::S) || input.key_down(egui::Key::ArrowDown) {
                    move_vec -= self.camera.forward_direction();
                }
                if input.key_down(egui::Key::A) || input.key_down(egui::Key::ArrowLeft) {
                    move_vec -= self.camera.right_direction();
                }
                if input.key_down(egui::Key::D) || input.key_down(egui::Key::ArrowRight) {
                    move_vec += self.camera.right_direction();
                }
                
                if move_vec.magnitude() > 0.0 {
                    move_vec = move_vec.normalize() * move_speed;
                    self.camera.focal_point += move_vec;
                }
            });
        }

        // 按键状态（自定义风格）
        let (left_down, right_down, _middle_down) = ui.input(|input| {
            (
                input.pointer.button_down(egui::PointerButton::Primary),
                input.pointer.button_down(egui::PointerButton::Secondary),
                input.pointer.button_down(egui::PointerButton::Middle),
            )
        });

        // 处理拖拽开始（自定义风格）
        if hovered {
            // 左键拖拽旋转
            if left_down && !self.orbiting {
                self.orbiting = true;
                self.last_mouse_pos = pointer_pos;
            }
            // 右键拖拽平移
            if right_down && !self.panning {
                self.panning = true;
                self.last_mouse_pos = pointer_pos;
            }
        }

        // 处理拖拽更新（自定义风格）
        if self.orbiting {
            if left_down {
                if let (Some(last), Some(curr)) = (self.last_mouse_pos, pointer_pos) {
                    let dx = curr.0 - last.0;
                    let dy = curr.1 - last.1;
                    self.camera.orbit(dx, dy);
                    self.last_mouse_pos = Some(curr);
                }
            } else {
                self.orbiting = false;
                self.last_mouse_pos = None;
            }
        }

        if self.panning {
            if right_down {
                if let (Some(last), Some(curr)) = (self.last_mouse_pos, pointer_pos) {
                    let dx = curr.0 - last.0;
                    let dy = curr.1 - last.1;
                    self.camera.pan(dx, dy);
                    self.last_mouse_pos = Some(curr);
                }
            } else {
                self.panning = false;
                self.last_mouse_pos = None;
            }
        }

        // 聚焦快捷键
        ui.input(|input| {
            if input.key_pressed(egui::Key::F) && !input.modifiers.ctrl {
                // 聚焦选中物体或原点
                let focus_pos = state.selected_entity.as_ref()
                    .and_then(|eid| state.transform_cache.get(eid))
                    .map(|&(pos, _, _)| Point3::new(pos[0], pos[1], pos[2]))
                    .unwrap_or(Point3::new(0.0, 0.0, 0.0));
                self.camera.focus_on(focus_pos);
            }
        });

        // 左键拾取（仅当未悬停 Gizmo 时）
        if hovered {
            let clicked = ui.input(|input| {
                input.pointer.button_clicked(egui::PointerButton::Primary)
            });
                if let Some(pos) = pointer_pos {
            if clicked && !self.orbiting && !self.panning {
                    let rect = response.rect;
                    let local_x = pos.0 - rect.left();
                    let local_y = pos.1 - rect.top();
                    let (ray_origin, ray_dir) = self.camera.screen_to_world_ray(
                        local_x,
                        local_y,
                        rect.width(),
                        rect.height(),
                    );
                    // 射线检测拾取对象
                    let mut closest: Option<(String, f32)> = None;
                    for (eid, _, aabb) in &self.pickable_objects {
                        if let Some(t) = ray_aabb_intersection(ray_origin, ray_dir, aabb) {
                            if closest.as_ref().map_or(true, |(_, ct)| t < *ct) {
                                closest = Some((eid.clone(), t));
                            }
                        }
                    }
                    state.selected_entity = closest.map(|(eid, _)| eid);
                }
            }
        }
    }

    /// 处理 Gizmo 的鼠标交互。
    fn handle_gizmo_input(
        &mut self,
        ui: &egui::Ui,
        response: &egui::Response,
        state: &mut EditorState,
    ) {
        if state.mode == EditorMode::Play {
            return;
        }
        let pointer_pos = ui.input(|input| {
            input.pointer.hover_pos().map(|p| (p.x, p.y))
        });

        let hovered = response.hovered();
        let rect = response.rect;

        let gizmo_world_pos = state.selected_entity.as_ref()
            .and_then(|eid| state.transform_cache.get(eid))
            .map(|&(pos, _, _)| Point3::new(pos[0], pos[1], pos[2]))
            .unwrap_or(Point3::new(0.0, 0.0, 0.0));
        let gizmo_screen = self.world_to_screen(gizmo_world_pos, rect);

        if let (Some(gizmo_screen), Some(mouse)) = (gizmo_screen, pointer_pos) {
            let gizmo_sp = (gizmo_screen.x, gizmo_screen.y);

            let left_down = ui.input(|input| {
                input.pointer.button_down(egui::PointerButton::Primary)
            });
            let left_pressed = ui.input(|input| {
                input.pointer.button_clicked(egui::PointerButton::Primary)
            });
            let left_released = ui.input(|input| {
                input.pointer.button_released(egui::PointerButton::Primary)
            });

            if self.gizmo_interaction.dragging.is_none() {
                if hovered {
                    let hit = self.gizmo_interaction.hit_test(mouse, gizmo_sp, &self.camera);
                    self.gizmo_interaction.hovered_axis = hit;

                    if left_pressed && hit.is_some() {
                        self.gizmo_interaction.begin_drag(
                            hit.unwrap(),
                            gizmo_world_pos,
                            mouse,
                        );
                    }
                } else {
                    self.gizmo_interaction.hovered_axis = None;
                }
            }

            if self.gizmo_interaction.dragging.is_some() {
                if left_down {
                    self.gizmo_interaction.update_drag(mouse, &self.camera);
                }
                if left_released {
                    if let Some((_, delta)) = self.gizmo_interaction.end_drag() {
                        if let Some(ref entity_id) = state.selected_entity {
                            let cached = state.transform_cache.get(entity_id).copied();
                            if let Some((old_pos, old_rot, old_scl)) = cached {
                                let mode = self.gizmo_mode;
                                let (new_pos, new_rot, new_scl) = match mode {
                                    GizmoMode::Translate => (
                                        [old_pos[0] + delta.x, old_pos[1] + delta.y, old_pos[2] + delta.z],
                                        old_rot,
                                        old_scl,
                                    ),
                                    GizmoMode::Rotate => (
                                        old_pos,
                                        [old_rot[0] + delta.x, old_rot[1] + delta.y, old_rot[2] + delta.z],
                                        old_scl,
                                    ),
                                    GizmoMode::Scale => (
                                        old_pos,
                                        old_rot,
                                        [
                                            (old_scl[0] * (1.0 + delta.x)).max(0.01),
                                            (old_scl[1] * (1.0 + delta.y)).max(0.01),
                                            (old_scl[2] * (1.0 + delta.z)).max(0.01),
                                        ],
                                    ),
                                };
                                state.pending_transform = Some(PendingTransform {
                                    entity_id: entity_id.clone(),
                                    old_position: old_pos,
                                    new_position: new_pos,
                                    old_rotation: old_rot,
                                    new_rotation: new_rot,
                                    old_scale: old_scl,
                                    new_scale: new_scl,
                                });
                                state.transform_cache.insert(entity_id.clone(), (new_pos, new_rot, new_scl));
                            }
                        }
                    }
                }
            }
        }
    }

    /// 绘制编辑器网格。
    fn draw_grid(&self, ui: &mut egui::Ui, rect: egui::Rect) {
        draw_grid_impl(ui, rect, &self.camera, self.grid_size, self.grid_subdivisions);
    }

    /// 绘制物理碰撞体调试线框。
    fn draw_physics_debug(&self, ui: &mut egui::Ui, rect: egui::Rect, state: &EditorState) {
        if state.physics_debug_bodies.is_empty() {
            return;
        }

        let painter = ui.painter();
        let box_half = 0.5;
        let color = egui::Color32::from_rgb(0, 255, 100);

        // 单位立方体的 8 个顶点（相对中心）
        let corners: [(f32, f32, f32); 8] = [
            (-box_half, -box_half, -box_half),
            ( box_half, -box_half, -box_half),
            ( box_half,  box_half, -box_half),
            (-box_half,  box_half, -box_half),
            (-box_half, -box_half,  box_half),
            ( box_half, -box_half,  box_half),
            ( box_half,  box_half,  box_half),
            (-box_half,  box_half,  box_half),
        ];

        // 12 条边的索引对
        let edges: [(usize, usize); 12] = [
            (0,1), (1,2), (2,3), (3,0), // 底面
            (4,5), (5,6), (6,7), (7,4), // 顶面
            (0,4), (1,5), (2,6), (3,7), // 竖直边
        ];

        for body in &state.physics_debug_bodies {
            let pos = &body.position;
            let pos_3 = Point3::new(pos.x as f32, pos.y as f32, pos.z as f32);

            for &(a, b) in &edges {
                let ca = corners[a];
                let cb = corners[b];
                let pa = Point3::new(pos_3.x + ca.0, pos_3.y + ca.1, pos_3.z + ca.2);
                let pb = Point3::new(pos_3.x + cb.0, pos_3.y + cb.1, pos_3.z + cb.2);

                if let (Some(sp_a), Some(sp_b)) = (
                    self.world_to_screen(pa, rect),
                    self.world_to_screen(pb, rect),
                ) {
                    painter.line_segment([sp_a, sp_b], (1.5, color));
                }
            }
        }
    }

    /// 将世界坐标投影到屏幕坐标。超出视口范围返回 None。
    fn world_to_screen(&self, world_pos: Point3<f32>, rect: egui::Rect) -> Option<egui::Pos2> {
        let vp = self.camera.view_projection_matrix();
        let clip = vp * Vector4::new(world_pos.x, world_pos.y, world_pos.z, 1.0);

        if clip.w.abs() < f32::EPSILON {
            return None;
        }

        let ndc_x = clip.x / clip.w;
        let ndc_y = clip.y / clip.w;
        let ndc_z = clip.z / clip.w;

        if ndc_x < -1.0 || ndc_x > 1.0 || ndc_y < -1.0 || ndc_y > 1.0 || ndc_z < -1.0 || ndc_z > 1.0 {
            return None;
        }

        let screen_x = rect.left() + (ndc_x * 0.5 + 0.5) * rect.width();
        let screen_y = rect.top() + (1.0 - (ndc_y * 0.5 + 0.5)) * rect.height();

        Some(egui::Pos2::new(screen_x, screen_y))
    }

    /// 绘制 Gizmo 模式标签。
    fn draw_gizmo_overlay(&self, ui: &mut egui::Ui) {
        let mode_text = match self.gizmo_mode {
            GizmoMode::Translate => "Translate [W]",
            GizmoMode::Rotate => "Rotate [E]",
            GizmoMode::Scale => "Scale [R]",
        };

        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new(mode_text)
                    .size(11.0)
                    .color(egui::Color32::from_gray(180)),
            );
            ui.label(
                egui::RichText::new("LMB: Orbit | RMB: Pan | Scroll: Zoom | WASD/Arrows: Move")
                    .size(10.0)
                    .color(egui::Color32::from_gray(140)),
            );
        });
    }
}

impl EditorPanel for ViewportPanel {
    fn title(&self) -> &str {
        "Viewport"
    }

    fn show(&mut self, ui: &mut egui::Ui, state: &mut EditorState) {
        let available = ui.available_size();
        let rect = egui::Rect::from_min_size(
            ui.next_widget_position(),
            egui::Vec2::new(available.x, available.y.max(100.0)),
        );

        let (rect_response, _painter) =
            ui.allocate_painter(egui::Vec2::new(available.x, available.y), egui::Sense::click_and_drag());

        rect_response.clone().context_menu(|ui| {
            ui.label("Panels");
            ui.separator();
            let mut hier_vis = state.panel_layer.is_visible(&PanelLayer::Hierarchy);
            if ui.checkbox(&mut hier_vis, "Hierarchy").clicked() { state.panel_layer.set_visible(PanelLayer::Hierarchy, hier_vis); }
            let mut insp_vis = state.panel_layer.is_visible(&PanelLayer::Inspector);
            if ui.checkbox(&mut insp_vis, "Inspector").clicked() { state.panel_layer.set_visible(PanelLayer::Inspector, insp_vis); }
            let mut ab_vis = state.panel_layer.is_visible(&PanelLayer::AssetBrowser);
            if ui.checkbox(&mut ab_vis, "Asset Browser").clicked() { state.panel_layer.set_visible(PanelLayer::AssetBrowser, ab_vis); }
        });

        self.viewport_size = (rect.width(), rect.height());
        self.camera.aspect_ratio = rect.width() / rect.height().max(1.0);

        self.gizmo_interaction.mode = self.gizmo_mode;
        self.gizmo_interaction.handle_shortcuts(ui);
        self.gizmo_mode = self.gizmo_interaction.mode;

        self.handle_input(ui, &rect_response, state);
        self.handle_gizmo_input(ui, &rect_response, state);

        // 延迟初始化GPU网格渲染器
        if self.gpu_grid.is_none() {
            if let Some(ref render_state) = self.render_state {
                self.gpu_grid = Some(GpuGridRenderer::new(
                    &render_state.device,
                    render_state.target_format,
                ));
            }
        }

        // 绘制背景
        let bg_color = egui::Color32::from_gray(25);
        ui.painter().rect_filled(rect, 0.0, bg_color);

        // 使用GPU渲染网格
        if let Some(ref mut gpu_grid) = self.gpu_grid {
            if let Some(ref render_state) = self.render_state {
                let viewport_px = (rect.width() as u32, rect.height() as u32);
                if let Some(tex_id) = gpu_grid.render(
                    ui.ctx(),
                    &render_state.device,
                    &render_state.queue,
                    &self.camera,
                    viewport_px,
                    render_state.target_format,
                ) {
                    ui.painter().image(
                        tex_id,
                        rect,
                        egui::Rect::from_min_max(egui::Pos2::ZERO, egui::Pos2::new(1.0, 1.0)),
                        egui::Color32::WHITE,
                    );
                } else {
                    // GPU渲染失败，回退到CPU渲染
                    self.draw_grid(ui, rect);
                }
            } else {
                // 没有render_state，使用CPU渲染
                self.draw_grid(ui, rect);
            }
        } else {
            // GPU渲染器未初始化，使用CPU渲染
            self.draw_grid(ui, rect);
        }

        // 绘制物理碰撞体调试线框
        self.draw_physics_debug(ui, rect, state);

        // 绘制 Gizmo（当有选中实体时）
        if let Some(ref eid) = state.selected_entity {
            let gizmo_world_pos = state.transform_cache.get(eid)
                .map(|&(pos, _, _)| Point3::new(pos[0], pos[1], pos[2]))
                .unwrap_or(Point3::new(0.0, 0.0, 0.0));
            if let Some(gizmo_screen) = self.world_to_screen(gizmo_world_pos, rect) {
                let screen_pos = (gizmo_screen.x, gizmo_screen.y);
                draw_gizmo(ui.painter(), screen_pos, &self.camera, &self.gizmo_interaction);
            }
        }

        // 顶部叠加信息
        let mut top_left = rect.left_top();
        top_left.x += 8.0;
        top_left.y += 4.0;

        egui::Area::new("viewport_overlay_top".into())
            .fixed_pos(top_left)
            .order(egui::Order::Foreground)
            .show(ui.ctx(), |ui| {
                ui.add_space(2.0);
                self.draw_gizmo_overlay(ui);

                let eye = self.camera.eye_position();
                ui.label(
                    egui::RichText::new(format!(
                        "Eye: ({:.1}, {:.1}, {:.1})  Dist: {:.1}",
                        eye.x, eye.y, eye.z, self.camera.distance
                    ))
                    .size(10.0)
                    .color(egui::Color32::from_gray(160)),
                );
            });

        // 右上角坐标轴指示器
        let top_right = egui::Pos2::new(rect.right() - 60.0, rect.top() + 8.0);
        egui::Area::new("viewport_axes".into())
            .fixed_pos(top_right)
            .order(egui::Order::Foreground)
            .show(ui.ctx(), |ui| {
                self.draw_axes_widget(ui);
            });

        // 底部状态栏
        let bottom = egui::Pos2::new(rect.left() + 8.0, rect.bottom() - 20.0);
        egui::Area::new("viewport_status".into())
            .fixed_pos(bottom)
            .order(egui::Order::Foreground)
            .show(ui.ctx(), |ui| {
                ui.horizontal(|ui| {
                    let mode = match self.gizmo_mode {
                        GizmoMode::Translate => "T",
                        GizmoMode::Rotate => "R",
                        GizmoMode::Scale => "S",
                    };
                    let debug_count = state.physics_debug_bodies.len();
                    let debug_info = if debug_count > 0 {
                        format!(" | Bodies: {}", debug_count)
                    } else {
                        String::new()
                    };
                    ui.label(
                        egui::RichText::new(format!(
                            "Gizmo: {} | {}x{}{} | FPS: --",
                            mode,
                            rect.width() as u32,
                            rect.height() as u32,
                            debug_info,
                        ))
                        .size(10.0)
                        .color(egui::Color32::from_gray(140)),
                    );
                });
            });
    }
}

// ---------------------------------------------------------------------------
// 坐标轴指示器
// ---------------------------------------------------------------------------

impl ViewportPanel {
    /// 绘制右上角的坐标轴小部件。
    fn draw_axes_widget(&self, ui: &mut egui::Ui) {
        let painter = ui.painter();

        let view = self.camera.view_matrix();
        let x_axis = Vector3::unit_x();
        let y_axis = Vector3::unit_y();
        let z_axis = Vector3::unit_z();

        let view_inv = view.invert().unwrap_or_else(|| Matrix4::from_scale(1.0));
        let cam_right = Vector3::new(view_inv.x.x, view_inv.x.y, view_inv.x.z).normalize();
        let cam_up = Vector3::new(view_inv.y.x, view_inv.y.y, view_inv.y.z).normalize();

        let center_x = ui.next_widget_position().x + 25.0;
        let center_y = ui.next_widget_position().y + 25.0;
        let center = egui::Pos2::new(center_x, center_y);
        let scale = 20.0;

        let proj = |dir: Vector3<f32>| -> egui::Pos2 {
            let x = cam_right.dot(dir) * scale;
            let y = -cam_up.dot(dir) * scale;
            egui::Pos2::new(center.x + x, center.y + y)
        };

        let ox = proj(x_axis);
        let oy = proj(y_axis);
        let oz = proj(z_axis);

        // X 轴（红色）
        painter.line_segment([center, ox], (2.0, egui::Color32::RED));
        painter.text(
            ox,
            egui::Align2::CENTER_CENTER,
            "X",
            egui::FontId::monospace(10.0),
            egui::Color32::RED,
        );

        // Y 轴（绿色）
        painter.line_segment([center, oy], (2.0, egui::Color32::GREEN));
        painter.text(
            oy,
            egui::Align2::CENTER_CENTER,
            "Y",
            egui::FontId::monospace(10.0),
            egui::Color32::GREEN,
        );

        // Z 轴（蓝色）
        painter.line_segment([center, oz], (2.0, egui::Color32::LIGHT_BLUE));
        painter.text(
            oz,
            egui::Align2::CENTER_CENTER,
            "Z",
            egui::FontId::monospace(10.0),
            egui::Color32::LIGHT_BLUE,
        );

        // 原点
        painter.circle_filled(center, 3.0, egui::Color32::WHITE);
    }
}

// ---------------------------------------------------------------------------
// 测试
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Unity 风格网格渲染（自适应 LOD + 距离衰减 + 边缘淡出）
// ---------------------------------------------------------------------------

/// Unity 风格无限网格：自适应 LOD、距离衰减、边缘淡出、轴高亮。
///
/// 核心改进：
/// - **自适应 LOD**：相机距离越近网格越精细（0.5→1→5→10→50 单元），
///   距离越远越粗糙，极限下维持性能
/// - **距离衰减**：线条中点离相机越远越透明，呈大气透视效果
/// - **边缘淡出**：网格边界（后半段）平滑淡出，呈现无限网格观感
/// - **轴高亮**：`X` 轴红色、`Z` 轴蓝色、每 5 格为粗线（`major_step = 5`）
// ---------------------------------------------------------------------------
// GpuGridRenderer - renders grid via GPU LineRenderer to offscreen texture
// ---------------------------------------------------------------------------

struct GpuGridRenderer {
    line_renderer: render::LineRenderer,
    // 存储上一帧的纹理ID，用于在egui GC前清理
    prev_texture_id: Option<egui::TextureId>,
}

impl GpuGridRenderer {
    fn new(device: &render::wgpu::Device, color_format: render::wgpu::TextureFormat) -> Self {
        let depth_format = render::wgpu::TextureFormat::Depth32Float;
        let line_renderer = render::LineRenderer::new(device, color_format, depth_format, 1);
        Self {
            line_renderer,
            prev_texture_id: None,
        }
    }

    fn render(
        &mut self,
        ctx: &egui::Context,
        device: &render::wgpu::Device,
        queue: &render::wgpu::Queue,
        camera: &OrbitCamera,
        viewport_px: (u32, u32),
        color_format: render::wgpu::TextureFormat,
    ) -> Option<egui::TextureId> {
        let (w, h) = (viewport_px.0.max(1), viewport_px.1.max(1));
        
        // 每帧创建新纹理，避免与egui GC冲突
        let aligned_w = ((w + 63) / 64) * 64;
        let tex_size = (aligned_w, h);
        
        let color_tex = device.create_texture(&render::wgpu::TextureDescriptor {
            label: Some("grid color texture"),
            size: render::wgpu::Extent3d {
                width: tex_size.0, height: tex_size.1, depth_or_array_layers: 1,
            },
            mip_level_count: 1, sample_count: 1,
            dimension: render::wgpu::TextureDimension::D2,
            format: color_format,
            usage: render::wgpu::TextureUsages::RENDER_ATTACHMENT
                | render::wgpu::TextureUsages::TEXTURE_BINDING
                | render::wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let color_view = color_tex.create_view(&render::wgpu::TextureViewDescriptor::default());
        
        let depth_tex = device.create_texture(&render::wgpu::TextureDescriptor {
            label: Some("grid depth texture"),
            size: render::wgpu::Extent3d {
                width: tex_size.0, height: tex_size.1, depth_or_array_layers: 1,
            },
            mip_level_count: 1, sample_count: 1,
            dimension: render::wgpu::TextureDimension::D2,
            format: render::wgpu::TextureFormat::Depth32Float,
            usage: render::wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let depth_view = depth_tex.create_view(&render::wgpu::TextureViewDescriptor::default());
        
        // 更新相机和顶点
        let eye = camera.eye_position();
        let vp = camera.view_projection_matrix();
        self.line_renderer.update_camera(queue, vp.into(), [eye.x, eye.y, eye.z]);
        let vertices = build_grid_vertices(eye, camera.distance);
        self.line_renderer.upload(device, queue, &vertices);
        
        let mut encoder = device.create_command_encoder(&render::wgpu::CommandEncoderDescriptor {
            label: Some("grid render encoder"),
        });

        {
            let mut pass = encoder.begin_render_pass(&render::wgpu::RenderPassDescriptor {
                label: Some("grid render pass"),
                color_attachments: &[Some(render::wgpu::RenderPassColorAttachment {
                    view: &color_view,
                    resolve_target: None,
                    ops: render::wgpu::Operations {
                        load: render::wgpu::LoadOp::Clear(render::wgpu::Color { r: 0.02, g: 0.02, b: 0.03, a: 1.0 }),
                        store: render::wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(render::wgpu::RenderPassDepthStencilAttachment {
                    view: &depth_view,
                    depth_ops: Some(render::wgpu::Operations {
                        load: render::wgpu::LoadOp::Clear(1.0),
                        store: render::wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            self.line_renderer.draw(&mut pass);
        }

        let bytes_per_row = (tex_size.0 * 4) as usize;
        let buffer_size = bytes_per_row * tex_size.1 as usize;
        let read_buffer = device.create_buffer(&render::wgpu::BufferDescriptor {
            label: Some("grid readback buffer"),
            size: buffer_size as render::wgpu::BufferAddress,
            usage: render::wgpu::BufferUsages::MAP_READ | render::wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        encoder.copy_texture_to_buffer(
            render::wgpu::ImageCopyTexture {
                texture: &color_tex,
                mip_level: 0,
                origin: render::wgpu::Origin3d::ZERO,
                aspect: render::wgpu::TextureAspect::All,
            },
            render::wgpu::ImageCopyBuffer {
                buffer: &read_buffer,
                layout: render::wgpu::ImageDataLayout {
                    offset: 0,
                    bytes_per_row: Some(bytes_per_row as u32),
                    rows_per_image: Some(tex_size.1),
                },
            },
            render::wgpu::Extent3d { width: tex_size.0, height: tex_size.1, depth_or_array_layers: 1 },
        );

        queue.submit([encoder.finish()]);

        let slice = read_buffer.slice(..);
        slice.map_async(render::wgpu::MapMode::Read, |_| {});
        device.poll(render::wgpu::Maintain::Wait);
        let pixels = { let data = slice.get_mapped_range(); data.to_vec() };
        read_buffer.unmap();

        let image = egui::ColorImage::from_rgba_unmultiplied([tex_size.0 as usize, tex_size.1 as usize], &pixels);

        // 创建新纹理并保存ID
        // 注意：egui会自动管理纹理生命周期，我们不需要手动释放
        let tex = ctx.load_texture("grid_render", image, egui::TextureOptions::LINEAR);
        let tex_id = tex.id();
        
        self.prev_texture_id = Some(tex_id);
        
        Some(tex_id)
    }
}

#[allow(dead_code)]
fn screen_from_clip(clip: Vector4<f32>, rect: &egui::Rect) -> egui::Pos2 {
    egui::Pos2::new(
        rect.left() + (clip.x / clip.w * 0.5 + 0.5) * rect.width(),
        rect.top() + (1.0 - (clip.y / clip.w * 0.5 + 0.5)) * rect.height(),
    )
}

/// 将世界坐标投影到屏幕坐标。如果点在相机后面，返回 None。
fn world_to_screen_safe(world_pos: Point3<f32>, vp: &Matrix4<f32>, rect: &egui::Rect) -> Option<egui::Pos2> {
    let clip = vp * Vector4::new(world_pos.x, world_pos.y, world_pos.z, 1.0);
    
    // 点在相机后面，跳过
    if clip.w < 0.001 {
        return None;
    }
    
    let ndc_x = clip.x / clip.w;
    let ndc_y = clip.y / clip.w;
    
    // 完全不裁剪NDC范围，让egui自动处理屏幕外绘制
    // 网格线端点可能在NDC空间超出视口几百倍，但只要线段穿过视口就应该显示
    
    let screen_x = rect.left() + (ndc_x * 0.5 + 0.5) * rect.width();
    let screen_y = rect.top() + (1.0 - (ndc_y * 0.5 + 0.5)) * rect.height();
    
    Some(egui::Pos2::new(screen_x, screen_y))
}

fn draw_grid_impl(
    ui: &mut egui::Ui,
    rect: egui::Rect,
    camera: &OrbitCamera,
    _grid_size: f32,
    _grid_subdivisions: usize,
) {
    let painter = ui.painter();
    let vp = camera.view_projection_matrix();
    let eye = camera.eye_position();
    let dist = camera.distance;

    // --- 自适应网格 LOD ---
    // 相机越近网格越精细
    let cell_size = if dist < 5.0 { 0.5 }
        else if dist < 15.0 { 1.0 }
        else if dist < 50.0 { 5.0 }
        else if dist < 150.0 { 10.0 }
        else { 50.0 };

    // 网格范围覆盖更远
    let half_extent = (dist * 8.0).max(50.0).min(5000.0);  // 从4.0增加到8.0，最小值从20增加到50
    let half_cells = (half_extent / cell_size).ceil() as i32;
    let extent = half_cells as f32 * cell_size;
    let major_step = 5;
    let camera_fade_dist = dist * 12.0;  // 从6.0增加到12.0，让相机距离淡出更平缓
    
    // 以相机在XZ平面的投影位置为中心生成网格
    let cam_x = eye.x;
    let cam_z = eye.z;
    
    // 计算网格偏移，使网格对齐到cell_size的整数倍
    let grid_offset_x = (cam_x / cell_size).round() * cell_size;
    let grid_offset_z = (cam_z / cell_size).round() * cell_size;

    // 颜色基值（非预乘，alpha 单独预乘）
    let minor_base = (80, 80, 80);
    let major_base = (160, 160, 160);
    let axis_x_base = (240, 70, 70);
    let axis_z_base = (60, 90, 240);

    // 辅助函数：正确预乘 alpha
    let premul = |r: u32, g: u32, b: u32, a: u8| -> egui::Color32 {
        let a32 = a as u32;
        egui::Color32::from_rgba_premultiplied(
            (r * a32 / 255) as u8,
            (g * a32 / 255) as u8,
            (b * a32 / 255) as u8,
            a,
        )
    };

    // --- 绘制 X 方向线（沿 X 轴，Z 变化）---
    for i in -half_cells..=half_cells {
        let z = i as f32 * cell_size + grid_offset_z;
        let p1 = Point3::new(-extent + grid_offset_x, 0.0, z);
        let p2 = Point3::new(extent + grid_offset_x, 0.0, z);

        // 转换到屏幕坐标
        let sp1 = world_to_screen_safe(p1, &vp, &rect);
        let sp2 = world_to_screen_safe(p2, &vp, &rect);
        
        // 如果两个端点都不可见，跳过
        if sp1.is_none() && sp2.is_none() {
            continue;
        }
        
        // 至少有一个端点可见，使用可见的端点绘制
        // egui会自动裁剪超出屏幕的部分
        let (sp1, sp2) = match (sp1, sp2) {
            (Some(s1), Some(s2)) => (s1, s2),
            (Some(s1), None) => {
                // 一个端点可见，另一个不可见，使用可见端点和远处点
                let far_point = Point3::new(0.0, 0.0, z);
                if let Some(sf) = world_to_screen_safe(far_point, &vp, &rect) {
                    (s1, sf)
                } else {
                    continue;
                }
            },
            (None, Some(s2)) => {
                let far_point = Point3::new(0.0, 0.0, z);
                if let Some(sf) = world_to_screen_safe(far_point, &vp, &rect) {
                    (sf, s2)
                } else {
                    continue;
                }
            },
            (None, None) => continue,
        };

        // 线条中点 → 相机眼（XZ 平面距离）
        let dz = z - eye.z;
        let dx = 0.0 - eye.x;
        let cam_dist = (dx * dx + dz * dz).sqrt();
        let cam_alpha = 1.0 - (cam_dist / camera_fade_dist).clamp(0.0, 1.0);

        // 边缘淡出：t ∈ [0, 0.90) → 1.0, [0.90, 1.0] → 渐变到 0
        // 从0.75改为0.90，让更多网格线完整显示
        let edge_t = z.abs() / extent;
        let edge_alpha = if edge_t < 0.90 { 1.0 }
            else { 1.0 - ((edge_t - 0.90) / 0.10).clamp(0.0, 1.0) };

        let alpha_f = cam_alpha * edge_alpha;
        let alpha = (alpha_f * 255.0) as u8;
        if alpha < 4 { continue; }

        let is_center = (z - grid_offset_z).abs() < cell_size * 0.5;  // 判断是否是中心线（Z=0的线）
        let is_major = is_center || i % major_step == 0;

        if is_center {
            let (br, bg, bb) = axis_x_base;
            let color = premul(br, bg, bb, alpha);
            painter.line_segment([sp1, sp2], (2.5, color));
        } else if is_major {
            let (br, bg, bb) = major_base;
            let color = premul(br, bg, bb, alpha);
            painter.line_segment([sp1, sp2], (1.5, color));
        } else {
            let (br, bg, bb) = minor_base;
            let color = premul(br, bg, bb, alpha);
            painter.line_segment([sp1, sp2], (1.0, color));
        }
    }

    // --- 绘制 Z 方向线（沿 Z 轴，X 变化）---
    for i in -half_cells..=half_cells {
        let x = i as f32 * cell_size + grid_offset_x;
        let p1 = Point3::new(x, 0.0, -extent + grid_offset_z);
        let p2 = Point3::new(x, 0.0, extent + grid_offset_z);

        // 转换到屏幕坐标
        let sp1 = world_to_screen_safe(p1, &vp, &rect);
        let sp2 = world_to_screen_safe(p2, &vp, &rect);
        
        // 如果两个端点都不可见，跳过
        if sp1.is_none() && sp2.is_none() {
            continue;
        }
        
        // 至少有一个端点可见，使用可见的端点绘制
        let (sp1, sp2) = match (sp1, sp2) {
            (Some(s1), Some(s2)) => (s1, s2),
            (Some(s1), None) => {
                let far_point = Point3::new(x, 0.0, 0.0);
                if let Some(sf) = world_to_screen_safe(far_point, &vp, &rect) {
                    (s1, sf)
                } else {
                    continue;
                }
            },
            (None, Some(s2)) => {
                let far_point = Point3::new(x, 0.0, 0.0);
                if let Some(sf) = world_to_screen_safe(far_point, &vp, &rect) {
                    (sf, s2)
                } else {
                    continue;
                }
            },
            (None, None) => continue,
        };

        let dx = x - eye.x;
        let dz = 0.0 - eye.z;
        let cam_dist = (dx * dx + dz * dz).sqrt();
        let cam_alpha = 1.0 - (cam_dist / camera_fade_dist).clamp(0.0, 1.0);

        // 边缘淡出：t ∈ [0, 0.90) → 1.0, [0.90, 1.0] → 渐变到 0
        let edge_t = x.abs() / extent;
        let edge_alpha = if edge_t < 0.90 { 1.0 }
            else { 1.0 - ((edge_t - 0.90) / 0.10).clamp(0.0, 1.0) };

        let alpha_f = cam_alpha * edge_alpha;
        let alpha = (alpha_f * 255.0) as u8;
        if alpha < 4 { continue; }

        let is_center = (x - grid_offset_x).abs() < cell_size * 0.5;  // 判断是否是中心线（X=0的线）
        let is_major = is_center || i % major_step == 0;

        if is_center {
            let (br, bg, bb) = axis_z_base;
            let color = premul(br, bg, bb, alpha);
            painter.line_segment([sp1, sp2], (2.5, color));
        } else if is_major {
            let (br, bg, bb) = major_base;
            let color = premul(br, bg, bb, alpha);
            painter.line_segment([sp1, sp2], (1.5, color));
        } else {
            let (br, bg, bb) = minor_base;
            let color = premul(br, bg, bb, alpha);
            painter.line_segment([sp1, sp2], (1.0, color));
        }
    }
}
/// 网格专用世界→屏幕投影，允许点超出视口范围。
/// 与 `world_to_screen` 的区别：不裁剪 NDC `w < 0` 以外的范围。
#[allow(dead_code)]
fn camera_grid_project(
    camera: &OrbitCamera,
    world_pos: Point3<f32>,
    rect: egui::Rect,
) -> Option<egui::Pos2> {
    let vp = camera.view_projection_matrix();
    let clip = vp * Vector4::new(world_pos.x, world_pos.y, world_pos.z, 1.0);

    if clip.w < f32::EPSILON {
        return None;
    }

    let ndc_x = clip.x / clip.w;
    let ndc_y = clip.y / clip.w;

    let screen_x = rect.left() + (ndc_x * 0.5 + 0.5) * rect.width();
    let screen_y = rect.top() + (1.0 - (ndc_y * 0.5 + 0.5)) * rect.height();

    Some(egui::Pos2::new(screen_x, screen_y))
}

// ---------------------------------------------------------------------------
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_orbit_camera_eye_position() {
        let cam = OrbitCamera::default();
        let eye = cam.eye_position();
        let dist = (eye - cam.focal_point).magnitude();
        assert!((dist - cam.distance).abs() < 0.01);
    }

    #[test]
    fn test_orbit_camera_orbit_clamps_pitch() {
        let mut cam = OrbitCamera::default();
        cam.orbit(0.0, 10000.0);
        assert!(cam.pitch.abs() <= std::f32::consts::FRAC_PI_2);
    }

    #[test]
    fn test_orbit_camera_zoom_clamped() {
        let mut cam = OrbitCamera::default();
        cam.distance = 5.0;
        cam.zoom(100.0);
        assert!(cam.distance >= cam.min_distance);
        cam.distance = 90.0;
        cam.zoom(-100.0);
        assert!(cam.distance <= cam.max_distance);
    }

    #[test]
    fn test_orbit_camera_focus_on() {
        let mut cam = OrbitCamera::default();
        let target = Point3::new(5.0, 10.0, -3.0);
        cam.focus_on(target);
        assert_eq!(cam.focal_point.x, 5.0);
        assert_eq!(cam.focal_point.y, 10.0);
        assert_eq!(cam.focal_point.z, -3.0);
    }

    #[test]
    fn test_orbit_camera_view_projection_is_valid() {
        let cam = OrbitCamera::default();
        let vp = cam.view_projection_matrix();
        let trace = vp.x.x + vp.y.y + vp.z.z + vp.w.w;
        assert!(trace.abs() > 0.001);
    }

    #[test]
    fn test_ray_aabb_intersection_hit() {
        let aabb = AABB::new(
            Point3::new(-1.0, -1.0, -1.0),
            Point3::new(1.0, 1.0, 1.0),
        );
        let origin = Point3::new(0.0, 0.0, -5.0);
        let dir = Vector3::new(0.0, 0.0, 1.0);
        let t = ray_aabb_intersection(origin, dir, &aabb);
        assert!(t.is_some());
        assert!(t.unwrap() > 0.0);
    }

    #[test]
    fn test_ray_aabb_intersection_miss() {
        let aabb = AABB::new(
            Point3::new(-1.0, -1.0, -1.0),
            Point3::new(1.0, 1.0, 1.0),
        );
        let origin = Point3::new(5.0, 0.0, -5.0);
        let dir = Vector3::new(0.0, 0.0, 1.0);
        let t = ray_aabb_intersection(origin, dir, &aabb);
        assert!(t.is_none());
    }

    #[test]
    fn test_ray_aabb_intersection_origin_inside() {
        let aabb = AABB::new(
            Point3::new(-2.0, -2.0, -2.0),
            Point3::new(2.0, 2.0, 2.0),
        );
        let origin = Point3::new(0.0, 0.0, 0.0);
        let dir = Vector3::new(0.0, 0.0, 1.0);
        let t = ray_aabb_intersection(origin, dir, &aabb);
        assert!(t.is_some());
    }

    #[test]
    fn test_orbit_camera_screen_to_world_ray() {
        let cam = OrbitCamera::default();
        let (_origin, dir) = cam.screen_to_world_ray(400.0, 300.0, 800.0, 600.0);
        assert!((dir.magnitude() - 1.0).abs() < 0.01);
    }
}
