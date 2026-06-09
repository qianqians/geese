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
use crate::panels::{EditorPanel, EditorState};
use cgmath::{EuclideanSpace, InnerSpace, Matrix4, Point3, SquareMatrix, Vector3, Vector4, perspective, Rad};
use math::AABB;

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
            focal_point: Point3::new(0.0, 1.5, 0.0),
            yaw: std::f32::consts::FRAC_PI_4,
            pitch: std::f32::consts::FRAC_PI_4 * 0.5,
            distance: 10.0,
            min_distance: 0.5,
            max_distance: 100.0,
            aspect_ratio: 16.0 / 9.0,
            fov: std::f32::consts::FRAC_PI_3,
            z_near: 0.1,
            z_far: 1000.0,
            orbit_sensitivity: 0.005,
            pan_sensitivity: 0.01,
            zoom_sensitivity: 0.5,
        }
    }
}

impl OrbitCamera {
    /// 计算摄像机在世界空间中的位置。
    pub fn eye_position(&self) -> Point3<f32> {
        let direction = self.forward_direction();
        Point3::from_vec(self.focal_point.to_vec() - direction * self.distance)
    }

    /// 摄像机前向方向（从焦点指向摄像机）。
    pub fn forward_direction(&self) -> Vector3<f32> {
        let yaw_sin = self.yaw.sin();
        let yaw_cos = self.yaw.cos();
        let pitch_sin = self.pitch.sin();
        let pitch_cos = self.pitch.cos();
        Vector3::new(yaw_cos * pitch_cos, pitch_sin, yaw_sin * pitch_cos).normalize()
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

    /// 聚焦到指定位置。
    pub fn focus_on(&mut self, point: Point3<f32>) {
        self.focal_point = point;
    }

    /// 重置为默认视角。
    pub fn reset(&mut self) {
        self.focal_point = Point3::new(0.0, 1.5, 0.0);
        self.yaw = std::f32::consts::FRAC_PI_4;
        self.pitch = std::f32::consts::FRAC_PI_4 * 0.5;
        self.distance = 10.0;
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
    /// 网格大小
    grid_size: f32,
    /// 网格细分数量
    grid_subdivisions: usize,
    /// Gizmo 交互状态
    gizmo_interaction: GizmoInteraction,
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
            grid_size: 10.0,
            grid_subdivisions: 10,
            gizmo_interaction: GizmoInteraction::new(),
        }
    }

    /// 处理视口内的鼠标输入。
    fn handle_input(&mut self, ui: &egui::Ui, response: &egui::Response, state: &EditorState) {
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

        // 按键状态
        let (right_down, middle_down) = ui.input(|input| {
            (
                input.pointer.button_down(egui::PointerButton::Secondary),
                input.pointer.button_down(egui::PointerButton::Middle),
            )
        });

        // 处理拖拽开始
        if hovered {
            if right_down && !self.orbiting {
                self.orbiting = true;
                self.last_mouse_pos = pointer_pos;
            }
            if middle_down && !self.panning {
                self.panning = true;
                self.last_mouse_pos = pointer_pos;
            }
        }

        // 处理拖拽更新
        if self.orbiting {
            if right_down {
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
            if middle_down {
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
                // 聚焦选中物体
                self.camera.focus_on(Point3::new(0.0, 1.5, 0.0));
            }
        });

        // 左键拾取（仅当未悬停 Gizmo 时）
        if hovered {
            let clicked = ui.input(|input| {
                input.pointer.button_clicked(egui::PointerButton::Primary)
            });
            if clicked && !self.orbiting && !self.panning {
                // 进行射线拾取（仅当没有拖拽时）
                if let Some(pos) = pointer_pos {
                    let rect = response.rect;
                    let local_x = pos.0 - rect.left();
                    let local_y = pos.1 - rect.top();
                    let _ray = self.camera.screen_to_world_ray(
                        local_x,
                        local_y,
                        rect.width(),
                        rect.height(),
                    );
                }
            }
        }
    }

    /// 处理 Gizmo 的鼠标交互。
    fn handle_gizmo_input(
        &mut self,
        ui: &egui::Ui,
        response: &egui::Response,
        state: &EditorState,
    ) {
        if state.mode == EditorMode::Play {
            return;
        }
        let pointer_pos = ui.input(|input| {
            input.pointer.hover_pos().map(|p| (p.x, p.y))
        });

        let hovered = response.hovered();
        let rect = response.rect;

        let gizmo_world_pos = Point3::new(0.0, 0.0, 0.0);
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
                    if let Some((axis, delta)) = self.gizmo_interaction.end_drag() {
                        let _ = (axis, delta);
                    }
                }
            }
        }
    }

    /// 绘制编辑器网格。
    fn draw_grid(&self, ui: &mut egui::Ui, rect: egui::Rect) {
        let painter = ui.painter();
        let grid_color = egui::Color32::from_gray(60);
        let axis_color_x = egui::Color32::from_rgb(200, 50, 50);
        let axis_color_z = egui::Color32::from_rgb(50, 50, 200);

        let half = self.grid_size * self.grid_subdivisions as f32 / 2.0;
        let step = self.grid_size;

        for i in 0..=self.grid_subdivisions {
            let offset = -half + i as f32 * step;

            // X 方向线（沿 Z 轴分布）
            {
                let p1 = self.world_to_screen(Point3::new(-half, 0.0, offset), rect);
                let p2 = self.world_to_screen(Point3::new(half, 0.0, offset), rect);
                if let (Some(p1), Some(p2)) = (p1, p2) {
                    if rect.contains(p1) || rect.contains(p2) {
                        let color = if (offset - 0.0).abs() < f32::EPSILON {
                            axis_color_x
                        } else {
                            grid_color
                        };
                        painter.line_segment([p1, p2], (1.0, color));
                    }
                }
            }

            // Z 方向线（沿 X 轴分布）
            {
                let p1 = self.world_to_screen(Point3::new(offset, 0.0, -half), rect);
                let p2 = self.world_to_screen(Point3::new(offset, 0.0, half), rect);
                if let (Some(p1), Some(p2)) = (p1, p2) {
                    if rect.contains(p1) || rect.contains(p2) {
                        let color = if (offset - 0.0).abs() < f32::EPSILON {
                            axis_color_z
                        } else {
                            grid_color
                        };
                        painter.line_segment([p1, p2], (1.0, color));
                    }
                }
            }
        }
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
        let screen_y = rect.top() + (0.5 - ndc_y * 0.5) * rect.height();

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
            ui.checkbox(&mut state.panel_visibility.hierarchy, "Hierarchy");
            ui.checkbox(&mut state.panel_visibility.inspector, "Inspector");
            ui.checkbox(&mut state.panel_visibility.asset_browser, "Asset Browser");
        });

        self.viewport_size = (rect.width(), rect.height());
        self.camera.aspect_ratio = rect.width() / rect.height().max(1.0);

        self.gizmo_interaction.mode = self.gizmo_mode;
        self.gizmo_interaction.handle_shortcuts(ui);
        self.gizmo_mode = self.gizmo_interaction.mode;

        self.handle_input(ui, &rect_response, state);
        self.handle_gizmo_input(ui, &rect_response, state);

        // 绘制背景
        let bg_color = egui::Color32::from_gray(25);
        ui.painter().rect_filled(rect, 0.0, bg_color);

        // 如果有渲染纹理，绘制纹理;否则绘制网格
        if let Some(tex_id) = self.rendered_texture {
            ui.painter().image(
                tex_id,
                rect,
                egui::Rect::from_min_max(egui::Pos2::ZERO, egui::Pos2::new(1.0, 1.0)),
                egui::Color32::WHITE,
            );
        } else {
            self.draw_grid(ui, rect);
        }

        // 绘制物理碰撞体调试线框
        self.draw_physics_debug(ui, rect, state);

        // 绘制 Gizmo（当有选中实体时）
        if state.selected_entity.is_some() {
            let gizmo_world_pos = Point3::new(0.0, 0.0, 0.0);
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
