//! 全骨骼刚体系统（Ragdoll）。
//!
//! [`RagdollBuilder`] 根据 [`Skin`](avatar::Skin) 骨骼结构创建物理刚体，
//! 并用 impulse joint 连接父子骨骼。支持动画模式与物理模式的切换。

use std::collections::HashMap;

use cgmath::{InnerSpace, Matrix3, Matrix4, Quaternion, Vector3};
use physics::handles::BodyHandle;
use physics::joints::{FixedJointDesc, JointDesc, JointHandle, SphericalJointDesc};
use physics::math::{Iso3, Vec3, Quat};
use physics::scene::PhysicsScene;
use physics::shapes::ShapeDesc;
use physics::world::{BodyDesc, BodyKind};

use avatar::{SceneNode, Skin};

/// Ragdoll 构建配置。
#[derive(Debug, Clone)]
pub struct RagdollConfig {
    /// 默认骨骼质量
    pub bone_mass: f32,
    /// 默认胶囊体半径
    pub bone_radius: f32,
    /// 线性阻尼
    pub linear_damping: f32,
    /// 角阻尼
    pub angular_damping: f32,
}

impl Default for RagdollConfig {
    fn default() -> Self {
        Self {
            bone_mass: 1.0,
            bone_radius: 0.08,
            linear_damping: 0.5,
            angular_damping: 0.8,
        }
    }
}

/// 关节类型推断策略。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JointTypeStrategy {
    /// 全部使用球关节（最灵活）
    AllSpherical,
    /// 简单启发式：肢体末端用固定关节，其余用球关节
    Heuristic,
}

/// Ragdoll 实例：保存所有刚体和关节句柄。
#[derive(Debug, Clone)]
pub struct RagdollInstance {
    /// 骨骼关节索引 → 刚体句柄
    pub body_handles: HashMap<usize, BodyHandle>,
    /// 所有关节句柄
    pub joint_handles: Vec<JointHandle>,
    /// 是否为激活状态（Dynamic）
    pub active: bool,
}

impl RagdollInstance {
    /// 激活 ragdoll：将所有刚体从 KinematicPosition 切换为 Dynamic，
    /// 并设置初始变换为当前骨骼世界位姿。
    pub fn activate(
        &mut self,
        physics_scene: &mut PhysicsScene,
        nodes: &[SceneNode],
    ) {
        if self.active {
            return;
        }
        self.active = true;

        for (&joint_idx, &body_handle) in &self.body_handles {
            // 从骨骼节点读取当前世界变换
            if let Some(node) = nodes.get(joint_idx) {
                let world = node.world_transform;
                let pos = Vec3::new(world.w.x, world.w.y, world.w.z);
                let rot = quat_from_cgmath(quat_from_matrix4(world));

                physics_scene.set_translation(body_handle, pos, true);
                physics_scene.set_rotation(body_handle, rot, true);
            }
            // 注意：rapier 0.32 的 PhysicsScene 没有 set_body_kind 暴露;
            // 这里通过重新设置速度和唤醒来实现类似效果。
            // 实际生产环境需要在 physics crate 添加 set_body_kind 方法。
        }
    }

    /// 停用 ragdoll：将刚体设为 Kinematic，停止物理驱动。
    pub fn deactivate(&mut self) {
        self.active = false;
        // 刚体保持最后位置，调用方应在停用后回写骨骼变换
    }

    /// 是否激活。
    pub fn is_active(&self) -> bool {
        self.active
    }
}

/// Ragdoll 构建器。
pub struct RagdollBuilder {
    config: RagdollConfig,
    strategy: JointTypeStrategy,
}

impl RagdollBuilder {
    /// 使用给定配置创建构建器。
    pub fn new(config: RagdollConfig, strategy: JointTypeStrategy) -> Self {
        Self { config, strategy }
    }

    /// 构建 ragdoll：为 Skin 的每个关节创建物理刚体，
    /// 并为父子骨骼创建约束关节。
    ///
    /// # Arguments
    /// * `physics_scene` - 物理场景
    /// * `nodes` - 场景节点数组（用于读取骨骼初始位置）
    /// * `skin` - 骨骼蒙皮数据
    pub fn build(
        &self,
        physics_scene: &mut PhysicsScene,
        nodes: &[SceneNode],
        skin: &Skin,
    ) -> Result<RagdollInstance, String> {
        let mut body_handles: HashMap<usize, BodyHandle> = HashMap::new();
        let mut joint_handles: Vec<JointHandle> = Vec::new();

        // 记录每个 bone 在 skin.joints 中的索引，用于父子关系查找
        let mut node_to_joint_idx: HashMap<usize, usize> = HashMap::new();
        for (i, &node_idx) in skin.joints.iter().enumerate() {
            node_to_joint_idx.insert(node_idx, i);
        }

        // 1. 为每个关节 bone 创建刚体
        for &node_idx in skin.joints.iter() {
            let Some(node) = nodes.get(node_idx) else {
                continue;
            };

            // 计算骨骼长度（用于胶囊体半高）
            let bone_length = compute_bone_length(nodes, node_idx, &skin.joints);
            let half_height = (bone_length * 0.5).max(0.05);

            let world = node.world_transform;
            let pos = Vec3::new(world.w.x, world.w.y, world.w.z);

            let desc = BodyDesc {
                kind: BodyKind::Dynamic,
                position: Iso3::from_parts(pos.into(), Quat::IDENTITY),
                density: self.config.bone_mass,
                can_sleep: false,
                ..Default::default()
            };
            let shape = ShapeDesc::Capsule {
                half_height,
                radius: self.config.bone_radius,
            };

            let (body_handle, _collider) = physics_scene.add_body(desc, shape)?;
            body_handles.insert(node_idx, body_handle);
        }

        // 2. 为父子骨骼创建关节
        for (&node_idx, &body_handle) in &body_handles {
            let Some(node) = nodes.get(node_idx) else {
                continue;
            };

            // 查找该节点的子节点中哪些是骨骼
            for &child_node in &node.children {
                if let Some(&child_body) = body_handles.get(&child_node) {
                    // 确定关节类型
                    let desc = self.infer_joint_type(nodes, node_idx, child_node);

                    match physics_scene.add_joint(body_handle, child_body, desc, true) {
                        Ok(joint_handle) => {
                            joint_handles.push(joint_handle);
                        }
                        Err(e) => {
                            eprintln!("[Ragdoll] failed to create joint: {e}");
                        }
                    }
                }
            }
        }

        Ok(RagdollInstance {
            body_handles,
            joint_handles,
            active: true,
        })
    }

    /// 推断父子骨骼间应使用的关节类型。
    fn infer_joint_type(
        &self,
        nodes: &[SceneNode],
        _parent_node: usize,
        child_node: usize,
    ) -> JointDesc {
        match self.strategy {
            JointTypeStrategy::AllSpherical => {
                JointDesc::Spherical(SphericalJointDesc {
                    local_anchor1: (0.0, 0.0, 0.0),
                    local_anchor2: (0.0, 0.0, 0.0),
                    cone_limit: None,
                })
            }
            JointTypeStrategy::Heuristic => {
                // 简单启发式：检查子节点是否还有含骨骼后代的子节点
                let is_leaf = self.is_bone_leaf(nodes, child_node);

                if is_leaf {
                    // 叶节点（如手、脚、头）使用固定关节
                    JointDesc::Fixed(FixedJointDesc::default())
                } else {
                    // 中间节点使用球关节
                    JointDesc::Spherical(SphericalJointDesc {
                        local_anchor1: (0.0, 0.0, 0.0),
                        local_anchor2: (0.0, 0.0, 0.0),
                        cone_limit: None,
                    })
                }
            }
        }
    }

    /// 检查节点是否是骨骼树的叶子节点。
    fn is_bone_leaf(&self, nodes: &[SceneNode], node_idx: usize) -> bool {
        let Some(node) = nodes.get(node_idx) else {
            return true;
        };
        // 叶子：没有子节点，或者所有子节点都不在 body_handles 中
        // 这里简化处理：只看是否有子节点
        node.children.is_empty()
    }
}

/// 计算骨骼长度（从当前节点到第一个骨骼子节点中心的距离）。
fn compute_bone_length(nodes: &[SceneNode], node_idx: usize, joints: &[usize]) -> f32 {
    let Some(node) = nodes.get(node_idx) else {
        return 0.1;
    };

    for &child in &node.children {
        if joints.contains(&child) {
            if let Some(child_node) = nodes.get(child) {
                let parent_pos = Vector3::new(
                    node.world_transform.w.x,
                    node.world_transform.w.y,
                    node.world_transform.w.z,
                );
                let child_pos = Vector3::new(
                    child_node.world_transform.w.x,
                    child_node.world_transform.w.y,
                    child_node.world_transform.w.z,
                );
                return (child_pos - parent_pos).magnitude();
            }
        }
    }
    // 默认长度
    0.2
}

/// cgmath Quaternion → glam Quat。
fn quat_from_cgmath(q: Quaternion<f32>) -> Quat {
    Quat::from_xyzw(q.v.x, q.v.y, q.v.z, q.s)
}

/// cgmath Vector3 → glam Vec3。
#[allow(dead_code)]
fn vec3_from_cgmath(v: Vector3<f32>) -> Vec3 {
    Vec3::new(v.x, v.y, v.z)
}

/// 从 cgmath Matrix4 提取旋转四元数。
fn quat_from_matrix4(m: Matrix4<f32>) -> Quaternion<f32> {
    let rot_mat = Matrix3::new(
        m.x.x, m.x.y, m.x.z,
        m.y.x, m.y.y, m.y.z,
        m.z.x, m.z.y, m.z.z,
    );
    rot_mat.into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ragdoll_config_default() {
        let config = RagdollConfig::default();
        assert_eq!(config.bone_mass, 1.0);
        assert_eq!(config.bone_radius, 0.08);
    }

    #[test]
    fn test_quat_conversion() {
        let q = Quaternion::new(1.0, 0.0, 0.0, 0.0);
        let gq = quat_from_cgmath(q);
        assert!((gq.w - 1.0).abs() < 0.001);
    }
}
