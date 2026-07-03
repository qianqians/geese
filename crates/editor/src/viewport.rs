//! 3D 场景视口（Viewport）。
//!
//! 提供：
//! - [`OrbitCamera`]：编辑器摄像机（右键旋转/中键平移/滚轮缩放）
//! - [`ViewportPanel`]：集成到编辑器面板系统的 3D 视口
//! - 射线拾取（Ray Picking）：屏幕坐标 → 世界空间射线
//! - 编辑器网格和世界坐标轴指示器
//! - 物理碰撞体调试渲染（wireframe）
//!
//! ## Suggested module split (future refactoring):
//! - `viewport_camera.rs` — Camera control and projection
//! - `viewport_picking.rs` — Ray casting and object selection
//! - `viewport_grid.rs` — Grid rendering (CPU and GPU)
//! - `viewport_gizmo.rs` — Gizmo interaction and rendering
//! - `viewport_drag_drop.rs` — Asset drag-and-drop handling

use crate::editor_mode::EditorMode;
use crate::gizmo::{GizmoInteraction, draw_gizmo};
use crate::panel_layer::PanelLayer;
use crate::panels::{DropTargetHint, EditorAction, EditorPanel, EditorState, PendingTransform};
use cgmath::{InnerSpace, Matrix4, Point3, Quaternion, SquareMatrix, Vector3, Vector4, perspective, Rad};
use math::AABB;
use render::LineVertex;

// ---------------------------------------------------------------------------
// Named constants (avoid magic numbers)
// ---------------------------------------------------------------------------

/// Pitch angle limit epsilon to prevent gimbal lock at ±90°.
const PITCH_LIMIT_EPSILON: f32 = 0.01;

/// Keyboard move speed as a fraction of camera distance per frame.
const KEYBOARD_MOVE_SPEED_FACTOR: f32 = 0.02;

/// Minimum absolute ray Y direction to treat as non-parallel to the ground plane.
const RAY_PLANE_PARALLEL_EPSILON: f32 = 0.0001;

/// Screen-space radius for the drag-drop preview ring.
const DRAG_PREVIEW_RING_RADIUS: f32 = 40.0;

/// World-space height of the drag-drop indicator dashed line.
const DRAG_PREVIEW_UP_HEIGHT: f32 = 2.0;

/// Screen-space half-size of the drag-drop center crosshair.
const DRAG_PREVIEW_CROSS_SIZE: f32 = 8.0;

/// Minimum viewport height to avoid division by zero.
const MIN_VIEWPORT_HEIGHT: f32 = 100.0;

/// Minimum scale value to prevent degenerate transforms.
const SCALE_MINIMUM: f32 = 0.01;

// Grid LOD distance thresholds.
const GRID_LOD_NEAR: f32 = 5.0;
const GRID_LOD_MID: f32 = 15.0;
const GRID_LOD_FAR: f32 = 50.0;
const GRID_LOD_VERY_FAR: f32 = 150.0;

// Grid cell sizes for each LOD level.
const GRID_CELL_NEAR: f32 = 0.5;
const GRID_CELL_MID: f32 = 1.0;
const GRID_CELL_FAR: f32 = 5.0;
const GRID_CELL_VERY_FAR: f32 = 10.0;
const GRID_CELL_EXTREME: f32 = 50.0;

// Grid extent and fade parameters (GPU grid).
const GRID_EXTENT_MULTIPLIER: f32 = 50.0;
const GRID_EXTENT_MIN: f32 = 100.0;
const GRID_EXTENT_MAX: f32 = 5000.0;
const GRID_CAMERA_FADE_MULTIPLIER: f32 = 80.0;

// CPU grid extent and fade parameters (different from GPU grid).
const CPU_GRID_EXTENT_MULTIPLIER: f32 = 8.0;
const CPU_GRID_EXTENT_MIN: f32 = 50.0;
const CPU_GRID_CAMERA_FADE_MULTIPLIER: f32 = 12.0;

// Grid edge fade parameters (GPU grid).
const GRID_EDGE_FADE_START: f32 = 0.97;
const GRID_EDGE_FADE_END: f32 = 1.0;
const GRID_ALPHA_THRESHOLD: f32 = 0.02;

// CPU grid edge fade parameters (different from GPU grid).
const CPU_GRID_EDGE_FADE_START: f32 = 0.90;
const CPU_GRID_EDGE_FADE_END: f32 = 1.0;

// ---------------------------------------------------------------------------
// Core world→screen projection (shared by all variants)
// ---------------------------------------------------------------------------

/// Core projection: world → clip → NDC → screen.
/// Returns `None` only when `clip.w ≈ 0` (point at camera plane).
fn project_to_screen(world_pos: Point3<f32>, vp: &Matrix4<f32>, rect: &egui::Rect) -> Option<egui::Pos2> {
    let clip = vp * Vector4::new(world_pos.x, world_pos.y, world_pos.z, 1.0);
    if clip.w.abs() < f32::EPSILON {
        return None;
    }
    let ndc_x = clip.x / clip.w;
    let ndc_y = clip.y / clip.w;
    Some(egui::Pos2::new(
        rect.left() + (ndc_x * 0.5 + 0.5) * rect.width(),
        rect.top() + (1.0 - (ndc_y * 0.5 + 0.5)) * rect.height(),
    ))
}

/// Permissive projection for grid rendering: returns `None` only when the
/// point is clearly behind the camera (`w < 0.001`). Allows NDC values far
/// outside [-1, 1] so grid lines crossing the viewport remain visible.
fn project_to_screen_unclamped(world_pos: Point3<f32>, vp: &Matrix4<f32>, rect: &egui::Rect) -> Option<egui::Pos2> {
    let clip = vp * Vector4::new(world_pos.x, world_pos.y, world_pos.z, 1.0);
    if clip.w < 0.001 {
        return None;
    }
    let ndc_x = clip.x / clip.w;
    let ndc_y = clip.y / clip.w;
    Some(egui::Pos2::new(
        rect.left() + (ndc_x * 0.5 + 0.5) * rect.width(),
        rect.top() + (1.0 - (ndc_y * 0.5 + 0.5)) * rect.height(),
    ))
}

// ---------------------------------------------------------------------------
// OrbitCamera - 编辑器摄像机
// ---------------------------------------------------------------------------

/// 编辑器轨道摄像机。
///
/// 交互方式：
/// - 右键拖拽：绕焦点旋转（yaw/pitch）
/// - 滚轮：缩放距离
/// - WASD/方向键：移动焦点
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
            focal_point: Point3::new(0.0, 15.0, 0.0),  // 焦点抬高，网格沉到视口底部
            yaw: std::f32::consts::FRAC_PI_4,  // 45度水平角
            pitch: -0.5236,  // -30度俯角
            distance: 35.0,  // 相机距离
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
        let pitch_limit = std::f32::consts::FRAC_PI_2 - PITCH_LIMIT_EPSILON;
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
        self.focal_point = Point3::new(0.0, 15.0, 0.0);  // 与默认值保持一致
        self.yaw = std::f32::consts::FRAC_PI_4;
        self.pitch = -0.5236;  // -30度，与默认值保持一致
        self.distance = 35.0;  // 与默认值保持一致
    }

    /// 从屏幕坐标生成世界空间射线。
    pub fn screen_to_world_ray(
        &self,
        screen_x: f32,
        screen_y: f32,
        viewport_width: f32,
        viewport_height: f32,
    ) -> Option<(Point3<f32>, Vector3<f32>)> {
        let vp_inv = match self.view_projection_matrix().invert() {
            Some(inv) => inv,
            None => {
                eprintln!("[Viewport] Matrix inversion failed, skipping ray pick");
                return None;
            }
        };

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
        Some((near, direction))
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
    pub gizmo_interaction: GizmoInteraction,
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
            let move_speed = self.camera.distance * KEYBOARD_MOVE_SPEED_FACTOR;
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
        let (right_down, _middle_down) = ui.input(|input| {
            (
                input.pointer.button_down(egui::PointerButton::Secondary),
                input.pointer.button_down(egui::PointerButton::Middle),
            )
        });

        // 处理拖拽开始（自定义风格）
        if hovered {
            // 右键拖拽旋转
            if right_down && !self.orbiting {
                self.orbiting = true;
                self.last_mouse_pos = pointer_pos;
            }
        }

        // 处理拖拽更新（自定义风格）
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
            if clicked && !self.orbiting {
                    let rect = response.rect;
                    let local_x = pos.0 - rect.left();
                    let local_y = pos.1 - rect.top();
                    let (ray_origin, ray_dir) = match self.camera.screen_to_world_ray(
                        local_x,
                        local_y,
                        rect.width(),
                        rect.height(),
                    ) {
                        Some(r) => r,
                        None => return,
                    };
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
                                            (old_scl[0] * (1.0 + delta.x)).max(SCALE_MINIMUM),
                                            (old_scl[1] * (1.0 + delta.y)).max(SCALE_MINIMUM),
                                            (old_scl[2] * (1.0 + delta.z)).max(SCALE_MINIMUM),
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

    /// 处理从 AssetBrowser 拖放资产到视口。
    fn handle_drag_drop(&mut self, ui: &egui::Ui, response: &egui::Response, state: &mut EditorState) {
        let drag_active = state.dragged_asset_uuid.is_some()
            && state.drag_source.as_deref() == Some("AssetBrowser");

        if !drag_active {
            return;
        }

        let hovered = response.hovered();
        let rect = response.rect;

        // 计算世界坐标 drop 位置
        if hovered {
            if let Some(mouse_pos) = ui.input(|input| input.pointer.hover_pos()) {
                let local_x = mouse_pos.x - rect.left();
                let local_y = mouse_pos.y - rect.top();
                let (ray_origin, ray_dir) = match self.camera.screen_to_world_ray(
                    local_x, local_y, rect.width(), rect.height(),
                ) {
                    Some(r) => r,
                    None => return,
                };

                // 射线与 Y=0 地平面求交
                let drop_pos = if ray_dir.y.abs() > RAY_PLANE_PARALLEL_EPSILON {
                    let t = -ray_origin.y / ray_dir.y;
                    if t > 0.0 {
                        Point3::new(
                            ray_origin.x + ray_dir.x * t,
                            0.0,
                            ray_origin.z + ray_dir.z * t,
                        )
                    } else {
                        // 射线远离地平面，使用相机焦点投影
                        Point3::new(
                            self.camera.focal_point.x,
                            0.0,
                            self.camera.focal_point.z,
                        )
                    }
                } else {
                    Point3::new(
                        self.camera.focal_point.x,
                        0.0,
                        self.camera.focal_point.z,
                    )
                };

                state.drop_target_hint = Some(DropTargetHint::Viewport {
                    world_pos: [drop_pos.x, drop_pos.y, drop_pos.z],
                });
            }
        } else {
            state.drop_target_hint = None;
        }

        // 检测鼠标释放 → 实例化资产
        let released = ui.input(|input| {
            input.pointer.button_released(egui::PointerButton::Primary)
        });

        if released && hovered {
            // 从 DropTargetHint 获取最终位置
            if let Some(DropTargetHint::Viewport { world_pos }) = state.drop_target_hint.clone() {
                let prefab_uuid = state.dragged_asset_uuid.clone().unwrap_or_default();
                state.pending_actions.push(EditorAction::InstantiatePrefab {
                    prefab_uuid,
                    position: world_pos,
                    parent_node_id: None,
                });

                // 清除拖拽状态
                state.dragged_asset_uuid = None;
                state.dragged_asset_type = None;
                state.dragged_asset_name = None;
                state.drag_source = None;
                state.drop_target_hint = None;
            }
        }
    }

    /// 绘制编辑器网格。
    fn draw_grid(&self, ui: &mut egui::Ui, rect: egui::Rect) {
        draw_grid_impl(ui, rect, &self.camera, self.grid_size, self.grid_subdivisions);
    }

    /// 绘制物理碰撞体调试线框（含旋转）。
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

            // 构建旋转四元数
            let rot = &body.rotation;
            let quat = Quaternion::new(rot.w as f32, rot.x as f32, rot.y as f32, rot.z as f32);

            for &(a, b) in &edges {
                let ca = corners[a];
                let cb = corners[b];

                // 应用旋转
                let ca_v = Vector3::new(ca.0, ca.1, ca.2);
                let cb_v = Vector3::new(cb.0, cb.1, cb.2);
                let rotated_a = quat * ca_v;
                let rotated_b = quat * cb_v;

                let pa = Point3::new(
                    pos_3.x + rotated_a.x,
                    pos_3.y + rotated_a.y,
                    pos_3.z + rotated_a.z,
                );
                let pb = Point3::new(
                    pos_3.x + rotated_b.x,
                    pos_3.y + rotated_b.y,
                    pos_3.z + rotated_b.z,
                );

                if let (Some(sp_a), Some(sp_b)) = (
                    self.world_to_screen(pa, rect),
                    self.world_to_screen(pb, rect),
                ) {
                    painter.line_segment([sp_a, sp_b], (1.5, color));
                }
            }
        }
    }

    /// 将世界坐标投影到屏幕坐标。超出视口 NDC 范围返回 None。
    fn world_to_screen(&self, world_pos: Point3<f32>, rect: egui::Rect) -> Option<egui::Pos2> {
        let vp = self.camera.view_projection_matrix();
        let screen = project_to_screen(world_pos, &vp, &rect)?;

        // NDC bounds check: reject points outside the visible frustum
        let clip = vp * Vector4::new(world_pos.x, world_pos.y, world_pos.z, 1.0);
        let ndc_x = clip.x / clip.w;
        let ndc_y = clip.y / clip.w;
        let ndc_z = clip.z / clip.w;
        if ndc_x < -1.0 || ndc_x > 1.0 || ndc_y < -1.0 || ndc_y > 1.0 || ndc_z < -1.0 || ndc_z > 1.0 {
            return None;
        }

        Some(screen)
    }

    /// 绘制拖放预览（在 Y=0 平面上显示蓝色圆环 + 资产名称）。
    fn draw_drag_preview(&self, ui: &mut egui::Ui, rect: egui::Rect, state: &EditorState) {
        if let Some(DropTargetHint::Viewport { world_pos }) = &state.drop_target_hint {
            let center = Point3::new(world_pos[0], world_pos[1], world_pos[2]);
            let screen_center = match self.world_to_screen(center, rect) {
                Some(p) => p,
                None => return,
            };

            let painter = ui.painter();
            let ring_color = egui::Color32::from_rgba_premultiplied(60, 140, 255, 200);
            let ring_radius = DRAG_PREVIEW_RING_RADIUS; // screen-space radius

            // 绘制地面位置圆环（屏幕空间近似）
            let segments = 32;
            let mut prev_point = None;
            for i in 0..=segments {
                let angle = (i as f32 / segments as f32) * std::f32::consts::TAU;
                let dx = angle.cos() * ring_radius;
                let dy = angle.sin() * ring_radius;
                let current = egui::Pos2::new(screen_center.x + dx, screen_center.y + dy);
                if let Some(prev) = prev_point {
                    painter.line_segment([prev, current], (2.0, ring_color));
                }
                prev_point = Some(current);
            }

            // 绘制向上的虚线
            let up_point_3d = Point3::new(center.x, center.y + DRAG_PREVIEW_UP_HEIGHT, center.z);
            if let Some(screen_up) = self.world_to_screen(up_point_3d, rect) {
                let dash_color = egui::Color32::from_rgba_premultiplied(60, 140, 255, 160);
                let dash_length = 6.0;
                let gap_length = 4.0;
                let total = screen_center.distance(screen_up);
                if total > 0.0 {
                    let dir = (screen_up - screen_center) / total;
                    let mut t = 0.0;
                    while t < total {
                        let start = screen_center + dir * t;
                        let end = screen_center + dir * (t + dash_length).min(total);
                        painter.line_segment([start, end], (1.5, dash_color));
                        t += dash_length + gap_length;
                    }
                }
            }

            // 绘制中心十字
            let cross_size = DRAG_PREVIEW_CROSS_SIZE;
            let cross_color = egui::Color32::from_rgb(100, 180, 255);
            painter.line_segment(
                [
                    egui::Pos2::new(screen_center.x - cross_size, screen_center.y),
                    egui::Pos2::new(screen_center.x + cross_size, screen_center.y),
                ],
                (2.0, cross_color),
            );
            painter.line_segment(
                [
                    egui::Pos2::new(screen_center.x, screen_center.y - cross_size),
                    egui::Pos2::new(screen_center.x, screen_center.y + cross_size),
                ],
                (2.0, cross_color),
            );

            // 资产名称标签
            let name = state
                .dragged_asset_name
                .as_deref()
                .unwrap_or("Asset");
            let label_pos = egui::Pos2::new(screen_center.x, screen_center.y - ring_radius - 16.0);
            painter.text(
                label_pos,
                egui::Align2::CENTER_BOTTOM,
                name,
                egui::FontId::proportional(12.0),
                egui::Color32::from_rgba_premultiplied(200, 220, 255, 240),
            );
        }
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
                egui::RichText::new("RMB: Orbit | Scroll: Zoom | WASD/Arrows: Move")
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
            egui::Vec2::new(available.x, available.y.max(MIN_VIEWPORT_HEIGHT)),
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
        self.handle_drag_drop(ui, &rect_response, state);

        // 绘制背景
        let bg_color = egui::Color32::from_gray(25);
        ui.painter().rect_filled(rect, 0.0, bg_color);

        // GPU 网格渲染
        if let Some(ref rs) = self.render_state {
            if self.gpu_grid.is_none() {
                self.gpu_grid = Some(GpuGridRenderer::new(&rs.device));
            }
            if let Some(ref mut gpu) = self.gpu_grid {
                let w = (rect.width() as u32).max(1);
                let h = (rect.height() as u32).max(1);
                let mut renderer = rs.renderer.write();
                if let Some(tex_id) = gpu.render(&rs.device, &rs.queue, &mut renderer, &self.camera, (w, h)) {
                    drop(renderer); // release lock before painting
                    ui.painter().image(
                        tex_id,
                        rect,
                        egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                        egui::Color32::WHITE,
                    );
                }
            }
        } else {
            self.draw_grid(ui, rect);  // 无 wgpu 时回退 CPU
        }

        // 绘制物理碰撞体调试线框
        self.draw_physics_debug(ui, rect, state);

        // 绘制 Gizmo（当有选中实体时）
        if let Some(ref eid) = state.selected_entity {
            let (gizmo_world_pos, local_rot) = state.transform_cache.get(eid)
                .map(|&(pos, rot, _)| {
                    let p = Point3::new(pos[0], pos[1], pos[2]);
                    let q = Quaternion::from(cgmath::Euler::new(
                        cgmath::Rad::from(cgmath::Deg(rot[0])),
                        cgmath::Rad::from(cgmath::Deg(rot[1])),
                        cgmath::Rad::from(cgmath::Deg(rot[2])),
                    ));
                    (p, Matrix4::from(q))
                })
                .unwrap_or((Point3::new(0.0, 0.0, 0.0), Matrix4::from_scale(1.0)));
            self.gizmo_interaction.local_rotation = local_rot;
            if let Some(gizmo_screen) = self.world_to_screen(gizmo_world_pos, rect) {
                let screen_pos = (gizmo_screen.x, gizmo_screen.y);
                draw_gizmo(ui.painter(), screen_pos, &self.camera, &self.gizmo_interaction);
            }
        }

        // 绘制拖放预览（资产拖拽时的地面指示器）
        self.draw_drag_preview(ui, rect, state);

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
    /// LineRenderer pipeline created with Rgba8UnormSrgb (matches offscreen texture)
    line_renderer: render::LineRenderer,
    /// Persistent offscreen color texture (Rgba8UnormSrgb, required by register_native_texture)
    color_tex: Option<render::wgpu::Texture>,
    color_view: Option<render::wgpu::TextureView>,
    /// Depth texture for the offscreen render pass
    depth_tex: Option<render::wgpu::Texture>,
    depth_view: Option<render::wgpu::TextureView>,
    /// Stable TextureId registered with egui_wgpu Renderer (not subject to egui texture GC)
    tex_id: Option<egui::TextureId>,
    /// Current offscreen texture size
    current_size: (u32, u32),
}

impl GpuGridRenderer {
    /// The offscreen color texture format required by `register_native_texture`.
    const OFFSCREEN_FORMAT: render::wgpu::TextureFormat = render::wgpu::TextureFormat::Rgba8UnormSrgb;
    const DEPTH_FORMAT: render::wgpu::TextureFormat = render::wgpu::TextureFormat::Depth32Float;

    fn new(device: &render::wgpu::Device) -> Self {
        // Create LineRenderer pipeline with Rgba8UnormSrgb to match our offscreen texture
        let line_renderer = render::LineRenderer::new(
            device,
            Self::OFFSCREEN_FORMAT,
            Self::DEPTH_FORMAT,
            1, // sample_count
        );
        Self {
            line_renderer,
            color_tex: None,
            color_view: None,
            depth_tex: None,
            depth_view: None,
            tex_id: None,
            current_size: (0, 0),
        }
    }

    /// Create or recreate offscreen textures when viewport size changes.
    /// Also registers the texture with egui_wgpu Renderer to get a stable TextureId.
    fn ensure_textures(
        &mut self,
        device: &render::wgpu::Device,
        renderer: &mut egui_wgpu::Renderer,
        w: u32,
        h: u32,
    ) {
        if w == self.current_size.0 && h == self.current_size.1 && self.tex_id.is_some() {
            return; // already up to date
        }
        let w = w.max(1);
        let h = h.max(1);

        // Create color texture (Rgba8UnormSrgb)
        let color_tex = device.create_texture(&render::wgpu::TextureDescriptor {
            label: Some("grid_offscreen_color"),
            size: render::wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: render::wgpu::TextureDimension::D2,
            format: Self::OFFSCREEN_FORMAT,
            usage: render::wgpu::TextureUsages::RENDER_ATTACHMENT
                | render::wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let color_view = color_tex.create_view(&render::wgpu::TextureViewDescriptor::default());

        // Create depth texture
        let depth_tex = device.create_texture(&render::wgpu::TextureDescriptor {
            label: Some("grid_offscreen_depth"),
            size: render::wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: render::wgpu::TextureDimension::D2,
            format: Self::DEPTH_FORMAT,
            usage: render::wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let depth_view = depth_tex.create_view(&render::wgpu::TextureViewDescriptor::default());

        // Register with egui_wgpu to get a stable TextureId
        let tex_id = renderer.register_native_texture(
            device,
            &color_view,
            render::wgpu::FilterMode::Linear,
        );

        self.color_tex = Some(color_tex);
        self.color_view = Some(color_view);
        self.depth_tex = Some(depth_tex);
        self.depth_view = Some(depth_view);
        self.tex_id = Some(tex_id);
        self.current_size = (w, h);
    }

    /// Render the grid to the offscreen texture. Returns the stable TextureId for display.
    fn render(
        &mut self,
        device: &render::wgpu::Device,
        queue: &render::wgpu::Queue,
        renderer: &mut egui_wgpu::Renderer,
        camera: &OrbitCamera,
        viewport_px: (u32, u32),
    ) -> Option<egui::TextureId> {
        let (w, h) = (viewport_px.0.max(1), viewport_px.1.max(1));

        // Ensure offscreen textures exist and match viewport size
        self.ensure_textures(device, renderer, w, h);

        let color_view = self.color_view.as_ref()?;
        let depth_view = self.depth_view.as_ref()?;
        let tex_id = self.tex_id?;

        // Update camera and grid vertices
        let eye = camera.eye_position();
        let vp = camera.view_projection_matrix();
        self.line_renderer.update_camera(queue, vp.into(), [eye.x, eye.y, eye.z]);
        let vertices = build_camera_grid_vertices(eye, camera.distance);
        self.line_renderer.upload(device, queue, &vertices);

        // Encode the grid render pass
        let mut encoder = device.create_command_encoder(&render::wgpu::CommandEncoderDescriptor {
            label: Some("grid_render_encoder"),
        });

        {
            let mut pass = encoder.begin_render_pass(&render::wgpu::RenderPassDescriptor {
                label: Some("grid_render_pass"),
                color_attachments: &[Some(render::wgpu::RenderPassColorAttachment {
                    view: color_view,
                    resolve_target: None,
                    ops: render::wgpu::Operations {
                        load: render::wgpu::LoadOp::Clear(render::wgpu::Color {
                            r: 0.0, g: 0.0, b: 0.0, a: 0.0,
                        }),
                        store: render::wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(render::wgpu::RenderPassDepthStencilAttachment {
                    view: depth_view,
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

        queue.submit([encoder.finish()]);

        // Notify egui_wgpu that the texture content has changed
        renderer.update_egui_texture_from_wgpu_texture(
            device,
            color_view,
            render::wgpu::FilterMode::Linear,
            tex_id,
        );

        Some(tex_id)
    }
}

/// Generate grid line vertices centered on the camera's XZ projection.
/// Used by GpuGridRenderer for GPU-side grid rendering.
fn build_camera_grid_vertices(eye: Point3<f32>, distance: f32) -> Vec<LineVertex> {
    // Adaptive LOD
    let cell_size = if distance < GRID_LOD_NEAR { GRID_CELL_NEAR }
        else if distance < GRID_LOD_MID { GRID_CELL_MID }
        else if distance < GRID_LOD_FAR { GRID_CELL_FAR }
        else if distance < GRID_LOD_VERY_FAR { GRID_CELL_VERY_FAR }
        else { GRID_CELL_EXTREME };

    // Grid extent: large enough to always cover visible ground area
    // At shallow pitch angles, visible ground extends very far
    let half_extent = (distance * GRID_EXTENT_MULTIPLIER).max(GRID_EXTENT_MIN).min(GRID_EXTENT_MAX);
    let half_cells = (half_extent / cell_size).ceil() as i32;
    let extent = half_cells as f32 * cell_size;
    let major_step = 5;
    // Camera fade distance: very large to avoid premature fading
    let camera_fade_dist = distance * GRID_CAMERA_FADE_MULTIPLIER;

    // Center grid on camera XZ projection, aligned to cell_size
    let grid_offset_x = (eye.x / cell_size).round() * cell_size;
    let grid_offset_z = (eye.z / cell_size).round() * cell_size;

    // Colors (premultiplied alpha will be applied per-vertex)
    let minor_color = [0.31, 0.31, 0.31, 1.0];
    let major_color = [0.63, 0.63, 0.63, 1.0];
    let axis_x_color = [0.94, 0.27, 0.27, 1.0];
    let axis_z_color = [0.24, 0.35, 0.94, 1.0];

    let mut verts = Vec::new();

    let mut push_line = |p1: Point3<f32>, p2: Point3<f32>, color: [f32; 4]| {
        verts.push(LineVertex { position: [p1.x, p1.y, p1.z], color });
        verts.push(LineVertex { position: [p2.x, p2.y, p2.z], color });
    };

    let fade_alpha = |mx: f32, mz: f32, edge: f32, extent: f32| -> f32 {
        let dx = mx - eye.x;
        let dz = mz - eye.z;
        let cam_dist = (dx * dx + dz * dz).sqrt();
        let cam_alpha = 1.0 - (cam_dist / camera_fade_dist).clamp(0.0, 1.0);
        let edge_t = edge.abs() / extent;
        // Edge fade: very gradual, only at extreme edges
        let edge_alpha = if edge_t < GRID_EDGE_FADE_START { 1.0 }
            else { 1.0 - ((edge_t - GRID_EDGE_FADE_START) / (GRID_EDGE_FADE_END - GRID_EDGE_FADE_START)).clamp(0.0, 1.0) };
        (cam_alpha * edge_alpha).max(0.0)
    };

    // X-direction lines (along X axis, Z varies)
    for i in -half_cells..=half_cells {
        let z = i as f32 * cell_size + grid_offset_z;
        let p1 = Point3::new(-extent + grid_offset_x, 0.0, z);
        let p2 = Point3::new(extent + grid_offset_x, 0.0, z);
        let alpha = fade_alpha(0.0, z, z - grid_offset_z, extent);
        if alpha < GRID_ALPHA_THRESHOLD { continue; }
        let is_center = (z - grid_offset_z).abs() < cell_size * 0.5;
        let is_major = is_center || i % major_step == 0;
        let base = if is_center { axis_x_color }
            else if is_major { major_color }
            else { minor_color };
        let color = [base[0], base[1], base[2], base[3] * alpha];
        push_line(p1, p2, color);
    }

    // Z-direction lines (along Z axis, X varies)
    for i in -half_cells..=half_cells {
        let x = i as f32 * cell_size + grid_offset_x;
        let p1 = Point3::new(x, 0.0, -extent + grid_offset_z);
        let p2 = Point3::new(x, 0.0, extent + grid_offset_z);
        let alpha = fade_alpha(x, 0.0, x - grid_offset_x, extent);
        if alpha < GRID_ALPHA_THRESHOLD { continue; }
        let is_center = (x - grid_offset_x).abs() < cell_size * 0.5;
        let is_major = is_center || i % major_step == 0;
        let base = if is_center { axis_z_color }
            else if is_major { major_color }
            else { minor_color };
        let color = [base[0], base[1], base[2], base[3] * alpha];
        push_line(p1, p2, color);
    }

    verts
}

#[allow(dead_code)]
fn screen_from_clip(clip: Vector4<f32>, rect: &egui::Rect) -> Option<egui::Pos2> {
    if clip.w.abs() < f32::EPSILON {
        return None;
    }
    Some(egui::Pos2::new(
        rect.left() + (clip.x / clip.w * 0.5 + 0.5) * rect.width(),
        rect.top() + (1.0 - (clip.y / clip.w * 0.5 + 0.5)) * rect.height(),
    ))
}

/// Permissive world→screen projection for grid rendering.
/// Allows screen coordinates far outside the viewport so grid lines crossing
/// the visible area remain drawable.
fn world_to_screen_safe(world_pos: Point3<f32>, vp: &Matrix4<f32>, rect: &egui::Rect) -> Option<egui::Pos2> {
    project_to_screen_unclamped(world_pos, vp, rect)
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
    let cell_size = if dist < GRID_LOD_NEAR { GRID_CELL_NEAR }
        else if dist < GRID_LOD_MID { GRID_CELL_MID }
        else if dist < GRID_LOD_FAR { GRID_CELL_FAR }
        else if dist < GRID_LOD_VERY_FAR { GRID_CELL_VERY_FAR }
        else { GRID_CELL_EXTREME };

    // 网格范围覆盖更远
    let half_extent = (dist * CPU_GRID_EXTENT_MULTIPLIER).max(CPU_GRID_EXTENT_MIN).min(GRID_EXTENT_MAX);
    let half_cells = (half_extent / cell_size).ceil() as i32;
    let extent = half_cells as f32 * cell_size;
    let major_step = 5;
    let camera_fade_dist = dist * CPU_GRID_CAMERA_FADE_MULTIPLIER;
    
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
        let edge_alpha = if edge_t < CPU_GRID_EDGE_FADE_START { 1.0 }
            else { 1.0 - ((edge_t - CPU_GRID_EDGE_FADE_START) / (CPU_GRID_EDGE_FADE_END - CPU_GRID_EDGE_FADE_START)).clamp(0.0, 1.0) };

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
        let edge_alpha = if edge_t < CPU_GRID_EDGE_FADE_START { 1.0 }
            else { 1.0 - ((edge_t - CPU_GRID_EDGE_FADE_START) / (CPU_GRID_EDGE_FADE_END - CPU_GRID_EDGE_FADE_START)).clamp(0.0, 1.0) };

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
/// Permissive world→screen projection for grid rendering (same as `world_to_screen_safe`).
#[allow(dead_code)]
fn camera_grid_project(
    camera: &OrbitCamera,
    world_pos: Point3<f32>,
    rect: egui::Rect,
) -> Option<egui::Pos2> {
    let vp = camera.view_projection_matrix();
    project_to_screen_unclamped(world_pos, &vp, &rect)
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
        let (_origin, dir) = cam.screen_to_world_ray(400.0, 300.0, 800.0, 600.0).unwrap();
        assert!((dir.magnitude() - 1.0).abs() < 0.01);
    }
}
