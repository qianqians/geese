//! 统一摄像机结构体——编辑器 + 游戏运行时共享。

use cgmath::{Deg, InnerSpace, Matrix4, Point3, Vector3, perspective};

use crate::frustum::Frustum;
use crate::mode::CameraMode;

/// 统一摄像机。
///
/// 通过 `CameraMode` 切换 FPS / Orbit / TopDown 视角，投影参数和输入方法统一管理。
#[derive(Debug, Clone)]
pub struct Camera {
    /// 摄像机世界位置（FPS 为玩家位置；Orbit/TopDown 由 CameraMode 计算）
    pub position: Point3<f32>,
    /// 宽高比
    pub aspect: f32,
    /// 垂直视场角（度）
    pub fov: f32,
    pub z_near: f32,
    pub z_far: f32,
    /// 旋转灵敏度
    pub sensitivity: f32,
    mode: CameraMode,
}

impl Camera {
    /// 用指定模式创建摄像机。
    pub fn new(mode: CameraMode, aspect: f32) -> Self {
        Self {
            position: Point3::new(0.0, 2.0, 0.0),
            aspect,
            fov: 60.0,
            z_near: 0.1,
            z_far: 500.0,
            sensitivity: 0.003,
            mode,
        }
    }

    // ── 投影 ──
    pub fn projection_matrix(&self) -> Matrix4<f32> {
        perspective(Deg(self.fov), self.aspect, self.z_near, self.z_far)
    }

    pub fn view_matrix(&self) -> Matrix4<f32> {
        self.mode.view_matrix(self.position)
    }

    pub fn view_projection_matrix(&self) -> Matrix4<f32> {
        self.projection_matrix() * self.view_matrix()
    }

    /// GPU 管线使用 `[[f32;4];4]` 格式。
    pub fn view_projection_raw(&self) -> [[f32; 4]; 4] {
        self.view_projection_matrix().into()
    }

    /// 摄像机位置（用于 GPU uniform）。
    pub fn camera_position_raw(&self) -> [f32; 3] {
        let eye = self.eye_position();
        [eye.x, eye.y, eye.z]
    }

    pub fn frustum(&self) -> Frustum {
        let vp = self.view_projection_matrix();
        Frustum::from_view_projection_matrix(&vp)
    }

    // ── 方向 ──
    pub fn eye_position(&self) -> Point3<f32> {
        self.mode.eye_position(self.position)
    }

    pub fn forward(&self) -> Vector3<f32> {
        self.mode.forward()
    }

    pub fn right(&self) -> Vector3<f32> {
        let fwd = self.forward();
        Vector3::new(-fwd.z, 0.0, fwd.x).normalize()
    }

    pub fn up(&self) -> Vector3<f32> {
        let fwd = self.forward();
        let r = self.right();
        r.cross(fwd).normalize()
    }

    // ── 模式切换 ──
    pub fn set_mode(&mut self, mode: CameraMode) {
        self.mode = mode;
    }

    pub fn mode(&self) -> CameraMode {
        self.mode
    }

    // ── FPS 输入 ──
    pub fn move_forward(&mut self, amount: f32) {
        self.position += self.forward() * amount;
    }

    pub fn move_right(&mut self, amount: f32) {
        self.position += self.right() * amount;
    }

    pub fn move_up(&mut self, amount: f32) {
        self.position.y += amount;
    }

    pub fn rotate_fps(&mut self, dx: f32, dy: f32) {
        if let CameraMode::Fps {
            ref mut yaw,
            ref mut pitch,
        } = self.mode
        {
            *yaw += dx * self.sensitivity;
            *pitch = (*pitch - dy * self.sensitivity)
                .clamp(-89.0_f32.to_radians(), 89.0_f32.to_radians());
        }
    }

    // ── Orbit 输入 ──
    pub fn orbit(&mut self, dx: f32, dy: f32) {
        if let CameraMode::Orbit {
            ref mut yaw,
            ref mut pitch,
            ..
        } = self.mode
        {
            *yaw -= dx * self.sensitivity;
            *pitch = (*pitch + dy * self.sensitivity)
                .clamp(-89.0_f32.to_radians(), 89.0_f32.to_radians());
        }
    }

    pub fn pan(&mut self, dx: f32, dy: f32) {
        // 预先计算 right/forward 避免借用冲突
        let right = self.right();
        let up_v = Vector3::unit_y();
        if let CameraMode::Orbit {
            ref mut focal_point,
            distance,
            ..
        } = self.mode
        {
            let speed = distance * 0.01;
            *focal_point += right * (-dx * speed) + up_v * (dy * speed);
        }
    }

    pub fn zoom(&mut self, delta: f32) {
        if let CameraMode::Orbit {
            ref mut distance,
            min_distance,
            max_distance,
            ..
        } = self.mode
        {
            let d = *distance - delta * 0.5;
            *distance = d.clamp(min_distance, max_distance);
        }
    }

    pub fn focus_on(&mut self, point: Point3<f32>) {
        if let CameraMode::Orbit {
            ref mut focal_point, ..
        } = self.mode
        {
            *focal_point = point;
        }
    }

    // ── TopDown 输入 ──
    pub fn topdown_pan(&mut self, dx: f32, dy: f32) {
        // 预先计算 forward/right 避免借用冲突
        let fwd = self.forward();
        let r = self.right();
        if let CameraMode::TopDown {
            ref mut center,
            height,
            ..
        } = self.mode
        {
            let speed = height * 0.001;
            *center += fwd * (dy * speed) + r * (dx * speed);
        }
    }

    pub fn topdown_zoom(&mut self, delta: f32) {
        if let CameraMode::TopDown {
            ref mut height, ..
        } = self.mode
        {
            *height = (*height - delta * 0.5).max(2.0).min(200.0);
        }
    }

    // ── 便利方法 ──
    pub fn update_aspect(&mut self, width: u32, height: u32) {
        self.aspect = width as f32 / height.max(1) as f32;
    }

    /// 设置摄像机世界位置（FPS 模式直接修改 position；Orbit 修改 focal_point）。
    pub fn set_world_position(&mut self, pos: Point3<f32>) {
        match self.mode {
            CameraMode::Fps { .. } => self.position = pos,
            CameraMode::Orbit { ref mut focal_point, .. } => *focal_point = pos,
            CameraMode::TopDown { ref mut center, .. } => *center = pos,
        }
    }

    // ── 开发者控制 API ──

    /// 让摄像机朝向世界空间中的目标点（立即旋转）。
    ///
    /// 计算从当前 eye_position 到 target 的 yaw/pitch 并直接设置。
    /// 适用于 FPS 和 Orbit 模式。
    pub fn look_at(&mut self, target: Point3<f32>) {
        let eye = self.eye_position();
        let dir = (target - eye).normalize();
        let new_yaw = dir.z.atan2(dir.x);
        let new_pitch = dir.y.asin().clamp(-1.0, 1.0);
        match &mut self.mode {
            CameraMode::Fps { yaw, pitch }
            | CameraMode::Orbit { yaw, pitch, .. } => {
                *yaw = new_yaw;
                *pitch = new_pitch;
            }
            _ => {}
        }
    }

    /// 平滑朝向目标点（每帧调用，speed 控制旋转速度，值越大越快）。
    pub fn smooth_look_at(&mut self, target: Point3<f32>, speed: f32, dt: f32) {
        let eye = self.eye_position();
        let dir = (target - eye).normalize();
        let target_yaw = dir.z.atan2(dir.x);
        let target_pitch = dir.y.asin().clamp(-1.0, 1.0);
        match &mut self.mode {
            CameraMode::Fps { yaw, pitch }
            | CameraMode::Orbit { yaw, pitch, .. } => {
                let t = (speed * dt).clamp(0.0, 1.0);
                let mut dy = target_yaw - *yaw;
                while dy > std::f32::consts::PI { dy -= 2.0 * std::f32::consts::PI; }
                while dy < -std::f32::consts::PI { dy += 2.0 * std::f32::consts::PI; }
                *yaw += dy * t;
                *pitch += (target_pitch - *pitch) * t;
            }
            _ => {}
        }
    }

    /// 让 Orbit 摄像机跟随移动目标（每帧调用）。
    ///
    /// `target` 是被跟随的世界坐标。保持当前 distance/pitch/yaw。
    pub fn follow_target(&mut self, target: Point3<f32>) {
        if let CameraMode::Orbit { ref mut focal_point, .. } = self.mode {
            *focal_point = target;
        }
    }

    /// 让 Orbit 摄像机平滑跟随目标。
    pub fn smooth_follow_target(&mut self, target: Point3<f32>, speed: f32, dt: f32) {
        if let CameraMode::Orbit { ref mut focal_point, .. } = self.mode {
            let t = (speed * dt).clamp(0.0, 1.0);
            *focal_point = Point3::new(
                focal_point.x + (target.x - focal_point.x) * t,
                focal_point.y + (target.y - focal_point.y) * t,
                focal_point.z + (target.z - focal_point.z) * t,
            );
        }
    }

    // ── 访问器（OrbitCamera 需要） ──
    pub fn mode_yaw(&self) -> Option<f32> {
        self.mode.yaw()
    }

    pub fn set_mode_yaw(&mut self, v: f32) {
        match self.mode {
            CameraMode::Fps { ref mut yaw, .. }
            | CameraMode::Orbit { ref mut yaw, .. } => *yaw = v,
            _ => {}
        }
    }

    pub fn mode_pitch(&self) -> Option<f32> {
        self.mode.pitch()
    }

    pub fn set_mode_pitch(&mut self, v: f32) {
        match self.mode {
            CameraMode::Fps { ref mut pitch, .. }
            | CameraMode::Orbit { ref mut pitch, .. } => *pitch = v,
            _ => {}
        }
    }

    pub fn mode_distance(&self) -> Option<f32> {
        self.mode.distance()
    }

    pub fn set_mode_distance(&mut self, v: f32) {
        if let CameraMode::Orbit {
            ref mut distance, ..
        } = self.mode
        {
            *distance = v;
        }
    }

    pub fn mode_focal_point(&self) -> Option<Point3<f32>> {
        self.mode.focal_point()
    }

    pub fn set_mode_focal_point(&mut self, v: Point3<f32>) {
        if let CameraMode::Orbit {
            ref mut focal_point,
            ..
        } = self.mode
        {
            *focal_point = v;
        }
    }
}
