//! 变换 Gizmo。
//!
//! 在视口中绘制并操作 Translate / Rotate / Scale Gizmo。
//! - W/E/R 切换模式
//! - 鼠标拖拽 Gizmo 手柄执行变换
//! - X/Y/Z 键锁定单轴
//! - Shift+W/E/R 切换坐标系（Local/World）
//!
//! ## Suggested module split (future refactoring):
//! - `gizmo_interaction.rs` — Drag state, hit testing, and keyboard shortcuts
//! - `gizmo_render.rs` — Gizmo drawing (arrows, rings, boxes)
//! - `gizmo_frame.rs` — Shared screen-space axis computation

use cgmath::{InnerSpace, Matrix4, Point3, Vector3, Vector4};
use crate::viewport::{GizmoMode, OrbitCamera};

// ---------------------------------------------------------------------------
// 轴索引
// ---------------------------------------------------------------------------

/// 变换轴。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Axis {
    X,
    Y,
    Z,
    /// 多轴平面（如 XY 平面用于平移）
    Plane(AxisPlane),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AxisPlane {
    XY,
    XZ,
    YZ,
}

// ---------------------------------------------------------------------------
// Gizmo 配置
// ---------------------------------------------------------------------------

/// Gizmo 渲染与交互配置。
pub struct GizmoConfig {
    /// Gizmo 在世界空间中的位置
    pub position: Point3<f32>,
    /// 手柄长度（屏幕像素）
    pub handle_length: f32,
    /// 手柄粗细（屏幕像素）
    pub handle_thickness: f32,
    /// 命中检测半径（屏幕像素）
    pub hit_radius: f32,
    /// 是否使用本地坐标系
    pub local_space: bool,
    /// 本地旋转矩阵（仅 local_space 时使用）
    pub local_rotation: Matrix4<f32>,
}

impl Default for GizmoConfig {
    fn default() -> Self {
        Self {
            position: Point3::new(0.0, 0.0, 0.0),
            handle_length: 80.0,
            handle_thickness: 3.0,
            hit_radius: 12.0,
            local_space: false,
            local_rotation: Matrix4::from_scale(1.0),
        }
    }
}

// ---------------------------------------------------------------------------
// Gizmo 交互状态
// ---------------------------------------------------------------------------

/// Gizmo 拖拽状态。
#[derive(Debug, Clone)]
pub struct GizmoDragState {
    /// 正在拖拽的轴
    pub axis: Axis,
    /// 拖拽起始世界位置
    pub start_world: Point3<f32>,
    /// 拖拽起始鼠标屏幕位置
    pub start_screen: (f32, f32),
    /// 当前世界位置
    pub current_world: Point3<f32>,
    /// 变换增量
    pub delta: Vector3<f32>,
}

/// Drag interaction sensitivities.
const ROTATE_SENSITIVITY: f32 = 0.01;
const SCALE_SENSITIVITY: f32 = 0.01;

/// Gizmo 交互管理器。
pub struct GizmoInteraction {
    /// 当前模式
    pub mode: GizmoMode,
    /// 锁定的单轴（None 表示自由变换）
    pub locked_axis: Option<Axis>,
    /// 是否使用本地坐标系
    pub local_space: bool,
    /// 本地旋转矩阵（仅 local_space 时使用）
    pub local_rotation: Matrix4<f32>,
    /// 拖拽状态（None 表示未拖拽）
    pub dragging: Option<GizmoDragState>,
    /// 悬停的轴
    pub hovered_axis: Option<Axis>,
    /// 上次鼠标位置
    last_mouse: Option<(f32, f32)>,
    /// 是否启用网格吸附
    pub snap_enabled: bool,
    /// 网格吸附间距（世界单位）
    pub snap_increment: f32,
}

impl GizmoInteraction {
    pub fn new() -> Self {
        Self {
            mode: GizmoMode::Translate,
            locked_axis: None,
            local_space: false,
            local_rotation: Matrix4::from_scale(1.0),
            dragging: None,
            hovered_axis: None,
            last_mouse: None,
            snap_enabled: false,
            snap_increment: 1.0,
        }
    }

    /// 处理快捷键。
    pub fn handle_shortcuts(&mut self, ui: &egui::Ui) {
        ui.input(|input| {
            // 模式切换
            if input.key_pressed(egui::Key::W) && !input.modifiers.ctrl && !input.modifiers.shift {
                self.mode = GizmoMode::Translate;
            }
            if input.key_pressed(egui::Key::E) && !input.modifiers.ctrl && !input.modifiers.shift {
                self.mode = GizmoMode::Rotate;
            }
            if input.key_pressed(egui::Key::R) && !input.modifiers.ctrl && !input.modifiers.shift {
                self.mode = GizmoMode::Scale;
            }

            // 坐标系切换
            if input.modifiers.shift {
                if input.key_pressed(egui::Key::W) {
                    self.local_space = !self.local_space;
                }
                if input.key_pressed(egui::Key::E) {
                    self.local_space = !self.local_space;
                }
                if input.key_pressed(egui::Key::R) {
                    self.local_space = !self.local_space;
                }
            }

            // 轴锁定
            if input.key_pressed(egui::Key::X) && !input.modifiers.ctrl {
                self.locked_axis = Some(Axis::X);
            }
            if input.key_pressed(egui::Key::Y) && !input.modifiers.ctrl {
                self.locked_axis = Some(Axis::Y);
            }
            if input.key_pressed(egui::Key::Z) && !input.modifiers.ctrl {
                self.locked_axis = Some(Axis::Z);
            }
            // 释放轴锁
            if input.key_pressed(egui::Key::Escape) {
                self.locked_axis = None;
            }
        });
    }

    /// 返回当前 Gizmo 的位置应用后的世界空间轴方向。
    pub fn axis_directions(&self) -> (Vector3<f32>, Vector3<f32>, Vector3<f32>) {
        if self.local_space {
            let rot = self.local_rotation();
            let x = rot * Vector4::new(1.0, 0.0, 0.0, 0.0);
            let y = rot * Vector4::new(0.0, 1.0, 0.0, 0.0);
            let z = rot * Vector4::new(0.0, 0.0, 1.0, 0.0);
            (
                Vector3::new(x.x, x.y, x.z).normalize(),
                Vector3::new(y.x, y.y, y.z).normalize(),
                Vector3::new(z.x, z.y, z.z).normalize(),
            )
        } else {
            (Vector3::unit_x(), Vector3::unit_y(), Vector3::unit_z())
        }
    }

    fn local_rotation(&self) -> Matrix4<f32> {
        self.local_rotation
    }

    /// 开始拖拽。
    pub fn begin_drag(&mut self, axis: Axis, world_pos: Point3<f32>, screen_pos: (f32, f32)) {
        self.dragging = Some(GizmoDragState {
            axis,
            start_world: world_pos,
            start_screen: screen_pos,
            current_world: world_pos,
            delta: Vector3::new(0.0, 0.0, 0.0),
        });
        self.last_mouse = Some(screen_pos);
    }

    /// 更新拖拽。
    pub fn update_drag(&mut self, screen_pos: (f32, f32), camera: &OrbitCamera) {
        let drag_axis = self.dragging.as_ref().map(|d| d.axis);
        let mode = self.mode;
        let (axis_x, axis_y, axis_z) = self.axis_directions();

        if let (Some(drag), Some(drag_axis)) = (&mut self.dragging, drag_axis) {
            let last = self.last_mouse.unwrap_or(screen_pos);
            let dx = screen_pos.0 - last.0;
            let dy = screen_pos.1 - last.1;

            match mode {
                GizmoMode::Translate => {
                    // 在世界空间中计算平移量
                    let right = camera.right_direction();
                    let up = camera.up_direction();
                    let pan_speed = camera.distance() * 0.005;

                    // 屏幕移动 → 世界空间移动
                    let world_delta = right * (dx * pan_speed) + up * (-dy * pan_speed);

                    // 投影到锁定轴
                    let projected = match drag_axis {
                        Axis::X => axis_x * world_delta.dot(axis_x),
                        Axis::Y => axis_y * world_delta.dot(axis_y),
                        Axis::Z => axis_z * world_delta.dot(axis_z),
                        Axis::Plane(AxisPlane::XY) => {
                            axis_x * world_delta.dot(axis_x) + axis_y * world_delta.dot(axis_y)
                        }
                        Axis::Plane(AxisPlane::XZ) => {
                            axis_x * world_delta.dot(axis_x) + axis_z * world_delta.dot(axis_z)
                        }
                        Axis::Plane(AxisPlane::YZ) => {
                            axis_y * world_delta.dot(axis_y) + axis_z * world_delta.dot(axis_z)
                        }
                    };

                    drag.current_world = drag.start_world + projected;
                    drag.delta = projected;

                    // 网格吸附
                    if self.snap_enabled {
                        let snap = self.snap_increment;
                        let snapped = Point3::new(
                            (drag.current_world.x / snap).round() * snap,
                            (drag.current_world.y / snap).round() * snap,
                            (drag.current_world.z / snap).round() * snap,
                        );
                        drag.delta = Vector3::new(
                            snapped.x - drag.start_world.x,
                            snapped.y - drag.start_world.y,
                            snapped.z - drag.start_world.z,
                        );
                        drag.current_world = snapped;
                    }
                }
                GizmoMode::Rotate => {
                    // 屏幕拖拽 → 绕轴旋转角度（累积帧增量）
                    let sensitivity = ROTATE_SENSITIVITY;
                    let angle = (dx + dy) * sensitivity;
                    drag.delta += match drag_axis {
                        Axis::X => Vector3::new(angle, 0.0, 0.0),
                        Axis::Y => Vector3::new(0.0, angle, 0.0),
                        Axis::Z => Vector3::new(0.0, 0.0, angle),
                        _ => Vector3::new(0.0, 0.0, 0.0),
                    };
                }
                GizmoMode::Scale => {
                    // 屏幕拖拽 → 缩放因子增量（累积帧增量）
                    let sensitivity = SCALE_SENSITIVITY;
                    let scale_factor = 1.0 + (dx + dy) * sensitivity;
                    let scale_delta = scale_factor - 1.0;
                    drag.delta += match drag_axis {
                        Axis::X => Vector3::new(scale_delta, 0.0, 0.0),
                        Axis::Y => Vector3::new(0.0, scale_delta, 0.0),
                        Axis::Z => Vector3::new(0.0, 0.0, scale_delta),
                        Axis::Plane(AxisPlane::XY) => Vector3::new(scale_delta, scale_delta, 0.0),
                        Axis::Plane(AxisPlane::XZ) => Vector3::new(scale_delta, 0.0, scale_delta),
                        Axis::Plane(AxisPlane::YZ) => Vector3::new(0.0, scale_delta, scale_delta),
                    };
                }
            }

            self.last_mouse = Some(screen_pos);
        }
    }

    /// 结束拖拽，返回最终变换。
    pub fn end_drag(&mut self) -> Option<(Axis, Vector3<f32>)> {
        let drag = self.dragging.take()?;
        self.last_mouse = None;
        Some((drag.axis, drag.delta))
    }

    /// 检测鼠标命中了哪个 Gizmo 手柄。
    pub fn hit_test(
        &self,
        screen_pos: (f32, f32),
        gizmo_screen_pos: (f32, f32),
        camera: &OrbitCamera,
    ) -> Option<Axis> {
        let frame = GizmoScreenFrame::build(gizmo_screen_pos, camera, self);
        let origin = frame.origin;
        if origin.0 < 0.0 || origin.1 < 0.0 {
            return None;
        }

        let handle_len = GizmoConfig::default().handle_length;
        let hit_r = GizmoConfig::default().hit_radius;

        let x_tip = frame.raw_tip(frame.axis_x_screen, handle_len);
        let y_tip = frame.raw_tip(frame.axis_y_screen, handle_len);
        let z_tip = frame.raw_tip(frame.axis_z_screen, handle_len);

        // 点到线段的距离
        let dist_to_seg = |p: (f32, f32), a: (f32, f32), b: (f32, f32)| -> f32 {
            let abx = b.0 - a.0;
            let aby = b.1 - a.1;
            let apx = p.0 - a.0;
            let apy = p.1 - a.1;
            let t = ((apx * abx + apy * aby) / (abx * abx + aby * aby + 1e-6)).clamp(0.0, 1.0);
            let cx = a.0 + t * abx;
            let cy = a.1 + t * aby;
            let dx = p.0 - cx;
            let dy = p.1 - cy;
            (dx * dx + dy * dy).sqrt()
        };

        let mouse = screen_pos;

        match self.mode {
            GizmoMode::Translate => {
                if dist_to_seg(mouse, origin, x_tip) < hit_r {
                    Some(Axis::X)
                } else if dist_to_seg(mouse, origin, y_tip) < hit_r {
                    Some(Axis::Y)
                } else if dist_to_seg(mouse, origin, z_tip) < hit_r {
                    Some(Axis::Z)
                } else {
                    None
                }
            }
            GizmoMode::Rotate | GizmoMode::Scale => {
                if dist_to_seg(mouse, origin, x_tip) < hit_r {
                    Some(Axis::X)
                } else if dist_to_seg(mouse, origin, y_tip) < hit_r {
                    Some(Axis::Y)
                } else if dist_to_seg(mouse, origin, z_tip) < hit_r {
                    Some(Axis::Z)
                } else {
                    None
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Shared gizmo screen-space frame (used by both hit_test and draw_gizmo)
// ---------------------------------------------------------------------------

/// Precomputed screen-space gizmo frame: projected axes and origin.
/// Shared between hit testing and drawing to avoid duplicate coordinate math.
struct GizmoScreenFrame {
    /// Gizmo origin in screen pixels.
    origin: (f32, f32),
    /// X axis projected to screen (unnormalized 2D direction).
    axis_x_screen: (f32, f32),
    /// Y axis projected to screen.
    axis_y_screen: (f32, f32),
    /// Z axis projected to screen.
    axis_z_screen: (f32, f32),
}

impl GizmoScreenFrame {
    /// Build a gizmo screen frame from camera and interaction state.
    fn build(
        screen_pos: (f32, f32),
        camera: &OrbitCamera,
        interaction: &GizmoInteraction,
    ) -> Self {
        let (axis_x, axis_y, axis_z) = interaction.axis_directions();
        let right = camera.right_direction();
        let up = camera.up_direction();

        let proj = |v: Vector3<f32>| -> (f32, f32) {
            let sx = right.dot(v);
            let sy = up.dot(v);
            (sx, -sy)
        };

        Self {
            origin: screen_pos,
            axis_x_screen: proj(axis_x),
            axis_y_screen: proj(axis_y),
            axis_z_screen: proj(axis_z),
        }
    }

    /// Compute the normalized screen-space tip position for an axis direction.
    fn tip(&self, axis_screen: (f32, f32), handle_len: f32) -> (f32, f32) {
        let len = (axis_screen.0 * axis_screen.0 + axis_screen.1 * axis_screen.1).sqrt();
        if len < 1e-6 {
            return self.origin;
        }
        (
            self.origin.0 + axis_screen.0 / len * handle_len,
            self.origin.1 + axis_screen.1 / len * handle_len,
        )
    }

    /// Compute the unnormalized screen-space tip (for hit testing).
    fn raw_tip(&self, axis_screen: (f32, f32), handle_len: f32) -> (f32, f32) {
        (
            self.origin.0 + axis_screen.0 * handle_len,
            self.origin.1 + axis_screen.1 * handle_len,
        )
    }
}

// ---------------------------------------------------------------------------
// Gizmo 绘制
// ---------------------------------------------------------------------------

/// 在 egui painter 上绘制 Gizmo。
pub fn draw_gizmo(
    painter: &egui::Painter,
    screen_pos: (f32, f32),
    camera: &OrbitCamera,
    interaction: &GizmoInteraction,
) {
    if screen_pos.0 < -GIZMO_OFFSCREEN_THRESHOLD || screen_pos.1 < -GIZMO_OFFSCREEN_THRESHOLD {
        return;
    }

    let frame = GizmoScreenFrame::build(screen_pos, camera, interaction);
    let handle_len = GizmoConfig::default().handle_length;
    let origin = frame.origin;

    let x_tip = frame.tip(frame.axis_x_screen, handle_len);
    let y_tip = frame.tip(frame.axis_y_screen, handle_len);
    let z_tip = frame.tip(frame.axis_z_screen, handle_len);

    let p = |(x, y): (f32, f32)| egui::Pos2::new(x, y);

    match interaction.mode {
        GizmoMode::Translate => {
            draw_arrow(painter, origin, x_tip, egui::Color32::RED, interaction.hovered_axis == Some(Axis::X));
            draw_arrow(painter, origin, y_tip, egui::Color32::GREEN, interaction.hovered_axis == Some(Axis::Y));
            draw_arrow(painter, origin, z_tip, egui::Color32::LIGHT_BLUE, interaction.hovered_axis == Some(Axis::Z));
        }
        GizmoMode::Rotate => {
            draw_rotation_ring(painter, p(origin), camera, interaction);
        }
        GizmoMode::Scale => {
            draw_scale_box(painter, p(origin), x_tip, egui::Color32::RED, interaction.hovered_axis == Some(Axis::X));
            draw_scale_box(painter, p(origin), y_tip, egui::Color32::GREEN, interaction.hovered_axis == Some(Axis::Y));
            draw_scale_box(painter, p(origin), z_tip, egui::Color32::LIGHT_BLUE, interaction.hovered_axis == Some(Axis::Z));
        }
    }

    // Gizmo 中心点
    painter.circle_filled(p(origin), 4.0, egui::Color32::WHITE);
}

fn draw_arrow(
    painter: &egui::Painter,
    from: (f32, f32),
    to: (f32, f32),
    color: egui::Color32,
    highlight: bool,
) {
    let p = |(x, y): (f32, f32)| egui::Pos2::new(x, y);
    let thickness = if highlight { 4.0 } else { 2.5 };
    let c = if highlight {
        egui::Color32::YELLOW
    } else {
        color
    };

    // 线
    painter.line_segment([p(from), p(to)], (thickness, c));

    // 箭头三角形
    let dx = to.0 - from.0;
    let dy = to.1 - from.1;
    let len = (dx * dx + dy * dy).sqrt();
    if len > 1.0 {
        let nx = dx / len;
        let ny = dy / len;
        let arrow_size = 8.0;
        let a1 = p((to.0 - nx * arrow_size + ny * arrow_size * 0.5, to.1 - ny * arrow_size - nx * arrow_size * 0.5));
        let a2 = p(to);
        let a3 = p((to.0 - nx * arrow_size - ny * arrow_size * 0.5, to.1 - ny * arrow_size + nx * arrow_size * 0.5));
        painter.add(egui::Shape::convex_polygon(
            vec![a1, a2, a3],
            c,
            (0.0, c),
        ));
    }
}

/// Off-screen threshold for gizmo visibility (pixels).
const GIZMO_OFFSCREEN_THRESHOLD: f32 = 100.0;

fn draw_rotation_ring(
    painter: &egui::Painter,
    center: egui::Pos2,
    camera: &OrbitCamera,
    interaction: &GizmoInteraction,
) {
    let radius = 60.0;
    let num_segments = 48;
    let right = camera.right_direction();
    let up = camera.up_direction();

    let proj = |v: Vector3<f32>| -> egui::Vec2 {
        egui::Vec2::new(right.dot(v), -up.dot(v))
    };

    let rings = [
        (Axis::X, Vector3::unit_y(), Vector3::unit_z(), egui::Color32::RED),
        (Axis::Y, Vector3::unit_z(), Vector3::unit_x(), egui::Color32::GREEN),
        (Axis::Z, Vector3::unit_x(), Vector3::unit_y(), egui::Color32::LIGHT_BLUE),
    ];

    for &(axis, perp1, perp2, color) in &rings {
        let s1 = proj(perp1);
        let s2 = proj(perp2);

        let is_highlighted = interaction.hovered_axis == Some(axis)
            || interaction.dragging.as_ref().map_or(false, |d| d.axis == axis);
        let thickness = if is_highlighted { 3.5 } else { 2.0 };
        let draw_color = if is_highlighted { egui::Color32::YELLOW } else { color };

        let mut prev: Option<egui::Pos2> = None;
        for i in 0..=num_segments {
            let theta = i as f32 * 2.0 * std::f32::consts::PI / num_segments as f32;
            let point = center + (s1 * theta.cos() + s2 * theta.sin()) * radius;
            if let Some(p) = prev {
                painter.line_segment([p, point], (thickness, draw_color));
            }
            prev = Some(point);
        }
    }
}

fn draw_scale_box(
    painter: &egui::Painter,
    origin: egui::Pos2,
    tip: (f32, f32),
    color: egui::Color32,
    highlight: bool,
) {
    let c = if highlight { egui::Color32::YELLOW } else { color };
    let thickness = if highlight { 3.0 } else { 2.0 };

    let p = |(x, y): (f32, f32)| egui::Pos2::new(x, y);
    painter.line_segment([origin, p(tip)], (thickness, c));

    // 末端方块
    let half = 5.0;
    let rect = egui::Rect::from_center_size(
        p(tip),
        egui::Vec2::new(half * 2.0, half * 2.0),
    );
    painter.rect_filled(rect, 0.0, c);
}
