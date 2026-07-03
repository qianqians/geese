//! 摄像机视角模式定义。

use cgmath::{InnerSpace, Matrix4, Point3, Vector3};

/// 摄像机视角模式。
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CameraMode {
    /// 第一人称：WASD 移动，鼠标旋转视角
    Fps { yaw: f32, pitch: f32 },
    /// 第三人称轨道：右键旋转，滚轮缩放，中键平移
    Orbit {
        yaw: f32,
        pitch: f32,
        focal_point: Point3<f32>,
        distance: f32,
        min_distance: f32,
        max_distance: f32,
    },
    /// 俯视角：鼠标移动视野，滚轮缩放高度
    TopDown {
        center: Point3<f32>,
        height: f32,
        angle: f32,
    },
}

impl CameraMode {
    /// 从摄像机位置计算眼睛位置（Orbit/TopDown 覆盖此逻辑）。
    pub fn eye_position(&self, position: Point3<f32>) -> Point3<f32> {
        match *self {
            CameraMode::Fps { .. } => position,
            CameraMode::Orbit { yaw, pitch, focal_point, distance, .. } => {
                let x = yaw.sin() * (-pitch).cos() * distance;
                let y = (-pitch).sin() * distance;
                let z = yaw.cos() * (-pitch).cos() * distance;
                Point3::new(focal_point.x + x, focal_point.y + y, focal_point.z + z)
            }
            CameraMode::TopDown { center, height, angle } => {
                // 摄像机在 center 上方 height 处，可绕 Y 轴旋转
                let forward = Vector3::new(angle.sin(), 0.0, angle.cos());
                Point3::new(
                    center.x - forward.x * height * 0.5,
                    center.y + height,
                    center.z - forward.z * height * 0.5,
                )
            }
        }
    }

    /// 前向方向向量。
    pub fn forward(&self) -> Vector3<f32> {
        match *self {
            CameraMode::Fps { yaw, pitch } => {
                Vector3::new(
                    yaw.cos() * pitch.cos(),
                    pitch.sin(),
                    yaw.sin() * pitch.cos(),
                )
            }
            CameraMode::Orbit { yaw, pitch, focal_point, distance, .. } => {
                let eye = self.eye_position(Point3::new(0.0, 0.0, 0.0));
                (focal_point - eye).normalize()
            }
            CameraMode::TopDown { .. } => {
                Vector3::new(0.0, -1.0, 0.0) // 向下看
            }
        }
    }

    /// 视图矩阵。
    pub fn view_matrix(&self, position: Point3<f32>) -> Matrix4<f32> {
        let eye = self.eye_position(position);
        match *self {
            CameraMode::Fps { yaw, pitch } => {
                let forward = Vector3::new(
                    yaw.cos() * pitch.cos(),
                    pitch.sin(),
                    yaw.sin() * pitch.cos(),
                );
                let target = position + forward;
                Matrix4::look_at_rh(eye, target, Vector3::unit_y())
            }
            CameraMode::Orbit { focal_point, .. } => {
                Matrix4::look_at_rh(eye, focal_point, Vector3::unit_y())
            }
            CameraMode::TopDown { .. } => {
                let forward = Vector3::new(0.0, -1.0, 0.0);
                let target = eye + forward;
                Matrix4::look_at_rh(eye, target, Vector3::unit_y())
            }
        }
    }

    /// 获取当前模式的 yaw（如果适用）。
    pub fn yaw(&self) -> Option<f32> {
        match self {
            CameraMode::Fps { yaw, .. } | CameraMode::Orbit { yaw, .. } => Some(*yaw),
            _ => None,
        }
    }

    /// 获取当前模式的 pitch（如果适用）。
    pub fn pitch(&self) -> Option<f32> {
        match self {
            CameraMode::Fps { pitch, .. } | CameraMode::Orbit { pitch, .. } => Some(*pitch),
            _ => None,
        }
    }

    /// 获取 Orbit 模式的 distance（如果适用）。
    pub fn distance(&self) -> Option<f32> {
        match self {
            CameraMode::Orbit { distance, .. } => Some(*distance),
            _ => None,
        }
    }

    /// 获取 Orbit 模式的 focal_point（如果适用）。
    pub fn focal_point(&self) -> Option<Point3<f32>> {
        match self {
            CameraMode::Orbit { focal_point, .. } => Some(*focal_point),
            _ => None,
        }
    }

    /// 获取 TopDown 模式的 center（如果适用）。
    pub fn center(&self) -> Option<Point3<f32>> {
        match self {
            CameraMode::TopDown { center, .. } => Some(*center),
            _ => None,
        }
    }
}
