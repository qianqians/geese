//! 胶囊体角色控制器。
//!
//! [`CapsuleController`] 在物理场景中创建一个 Dynamic 胶囊体刚体，
//! 并暴露输入驱动的移动接口。每帧调用 [`update`](CapsuleController::update)
//! 进行地面检测和速度施加。

use cgmath::{Quaternion, Vector3};
use physics::handles::BodyHandle;
use physics::math::{Iso3, Vec3, Quat};
use physics::scene::PhysicsScene;
use physics::shapes::ShapeDesc;
use physics::world::{BodyDesc, BodyKind};

use avatar::SceneNode;
use avatar::Transform;

/// 胶囊体角色控制器。
///
/// 管理一个胶囊体物理刚体，提供地面检测、输入驱动的移动和跳跃。
#[derive(Debug, Clone)]
pub struct CapsuleController {
    /// 物理刚体句柄
    pub body_handle: BodyHandle,
    /// 场景节点索引
    pub node_index: usize,
    /// 胶囊体半高（不含半球帽）
    pub half_height: f32,
    /// 胶囊体半径
    pub radius: f32,
    /// 水平移动速度 (m/s)
    pub move_speed: f32,
    /// 跳跃冲量大小
    pub jump_impulse: f32,
    /// 空中移动控制系数 (0-1)
    pub air_control_factor: f32,
    /// 是否着地
    pub grounded: bool,
    /// 地面法线（着地时有效）
    pub ground_normal: Vec3,
    /// 输入驱动的目标速度（水平方向）
    pub target_velocity: Vec3,
    /// 是否请求跳跃（下一帧 update 时消费）
    jump_requested: bool,
}

impl CapsuleController {
    /// 在物理场景中创建胶囊体刚体，并在节点数组中添加对应节点。
    ///
    /// # Arguments
    /// * `physics_scene` - 可变的物理场景引用
    /// * `nodes` - 场景节点数组
    /// * `position` - 初始世界位置
    /// * `half_height` - 胶囊体半高（不含半球帽）
    /// * `radius` - 胶囊体半径
    pub fn new(
        physics_scene: &mut PhysicsScene,
        nodes: &mut Vec<SceneNode>,
        position: Vec3,
        half_height: f32,
        radius: f32,
    ) -> Result<Self, String> {
        // 创建物理刚体
        let body_desc = BodyDesc {
            kind: BodyKind::Dynamic,
            position: Iso3::from_parts(position.into(), Quat::IDENTITY),
            can_sleep: false,
            ccd_enabled: true,
            ..Default::default()
        };
        let shape = ShapeDesc::Capsule {
            half_height,
            radius,
        };
        let (body_handle, _collider) = physics_scene.add_body(body_desc, shape)?;

        // 创建场景节点
        let node_id = nodes.len();
        let transform = Transform {
            translation: Vector3::new(position.x, position.y, position.z),
            rotation: Quaternion::new(1.0, 0.0, 0.0, 0.0),
            scale: Vector3::new(1.0, 1.0, 1.0),
        };
        nodes.push(SceneNode::new(node_id, None, transform));

        Ok(Self {
            body_handle,
            node_index: node_id,
            half_height,
            radius,
            move_speed: 5.0,
            jump_impulse: 8.0,
            air_control_factor: 0.3,
            grounded: false,
            ground_normal: Vec3::Y,
            target_velocity: Vec3::ZERO,
            jump_requested: false,
        })
    }

    /// 每帧更新：执行地面检测、施加移动速度、处理跳跃。
    ///
    /// 应在物理步进前调用。
    pub fn update(&mut self, physics_scene: &mut PhysicsScene, dt: f32) {
        // 1. 地面检测
        self.detect_ground(physics_scene);

        // 2. 应用移动速度
        self.apply_movement(physics_scene, dt);

        // 3. 处理跳跃
        if self.jump_requested {
            self.jump_requested = false;
            if self.grounded {
                let impulse = Vec3::new(0.0, self.jump_impulse, 0.0);
                physics_scene.apply_impulse(self.body_handle, impulse, true);
                self.grounded = false;
            }
        }
    }

    /// 从胶囊体底部向下发射射线检测地面。
    fn detect_ground(&mut self, physics_scene: &PhysicsScene) {
        // 获取当前 body 位置
        let Some(iso) = physics_scene.body_isometry(self.body_handle) else {
            return;
        };

        let origin = iso.translation;
        // 从胶囊体底部向下检测（留一点余量）
        let ray_origin = Vec3::new(
            origin.x,
            origin.y - self.half_height - self.radius + 0.05,
            origin.z,
        );
        let ray_dir = Vec3::new(0.0, -1.0, 0.0);
        let max_toi = 0.3; // 检测距离

        if let Some(hit) = physics_scene.cast_ray(ray_origin, ray_dir, max_toi, true) {
            let normal = Vec3::new(hit.normal.0, hit.normal.1, hit.normal.2);
            // 法线朝上（地面）则视为着地
            self.grounded = normal.y > 0.5;
            if self.grounded {
                self.ground_normal = normal;
            }
        } else {
            self.grounded = false;
        }
    }

    /// 根据 grounded 状态施加水平速度。
    fn apply_movement(&mut self, physics_scene: &mut PhysicsScene, _dt: f32) {
        let Some(current_vel) = physics_scene.body_linvel(self.body_handle) else {
            return;
        };

        let target_horizontal = self.target_velocity;

        if self.grounded {
            // 地面：直接设置水平速度，保留垂直分量
            let new_vel = Vec3::new(
                target_horizontal.x * self.move_speed,
                current_vel.y, // 保留垂直速度（重力累积）
                target_horizontal.z * self.move_speed,
            );
            physics_scene.set_linvel(self.body_handle, new_vel, true);
        } else {
            // 空中：有限控制
            let control = self.air_control_factor;
            let desired_x = target_horizontal.x * self.move_speed;
            let desired_z = target_horizontal.z * self.move_speed;

            let new_vel_x = current_vel.x + (desired_x - current_vel.x) * control;
            let new_vel_z = current_vel.z + (desired_z - current_vel.z) * control;

            let new_vel = Vec3::new(new_vel_x, current_vel.y, new_vel_z);
            physics_scene.set_linvel(self.body_handle, new_vel, true);
        }
    }

    /// 返回当前线速度（供动画混合使用）。
    pub fn get_velocity(&self, physics_scene: &PhysicsScene) -> Vec3 {
        physics_scene.body_linvel(self.body_handle).unwrap_or(Vec3::ZERO)
    }

    /// 设置水平移动方向（归一化向量）。
    pub fn apply_move_input(&mut self, direction: Vec3) {
        self.target_velocity = direction;
    }

    /// 请求跳跃（下一帧 update 时处理）。
    pub fn apply_jump(&mut self) {
        self.jump_requested = true;
    }

    /// 返回控制器对应的骨骼根节点索引。
    pub fn root_node(&self) -> usize {
        self.node_index
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use physics::PhysicsWorld;

    #[test]
    fn test_capsule_controller_creation() {
        let mut world = PhysicsWorld::new();
        let scene_id = world.create_scene(Vec3::new(0.0, -9.81, 0.0));
        let physics_scene = world.scene_mut(scene_id).unwrap();
        let mut nodes = Vec::new();

        let result = CapsuleController::new(
            physics_scene,
            &mut nodes,
            Vec3::new(0.0, 1.0, 0.0),
            1.0,
            0.5,
        );
        assert!(result.is_ok());
        let controller = result.unwrap();
        assert_eq!(controller.node_index, 0);
        assert_eq!(nodes.len(), 1);
        assert!(!controller.grounded);
    }
}
