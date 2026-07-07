//! 物理角色与场景节点的桥接映射。
//!
//! [`CharacterPhysics`] 维护物理刚体句柄到场景骨架节点的映射，
//! 提供 `update_physics_transforms()` 将物理模拟结果回写到动画节点变换。

use std::collections::HashMap;

use cgmath::{Matrix3, Matrix4, Quaternion, SquareMatrix, Vector3};
use physics::handles::BodyHandle;
use physics::math::{Iso3, Vec3 as PhyVec3};
use physics::scene::PhysicsScene;

use avatar::SceneNode;

/// 角色控制器类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CharacterControllerType {
    /// 胶囊体角色控制器
    Capsule,
    /// 全骨骼刚体模拟
    Ragdoll,
}

/// 物理角色：映射物理刚体到场景骨架节点。
///
/// 每一帧物理步进后，调用 `update_physics_transforms()` 读取
/// 各 body 的世界变换并写入对应 `SceneNode.local_transform`。
#[derive(Debug, Clone)]
pub struct CharacterPhysics {
    /// 场景节点索引 → 物理刚体句柄
    pub body_handles: HashMap<usize, BodyHandle>,
    /// 角色根刚体句柄（用于整体位移）
    pub root_body: BodyHandle,
    /// 控制器类型
    pub controller_type: CharacterControllerType,
    /// 根节点在场景节点数组中的索引
    pub root_node: usize,
}

impl CharacterPhysics {
    /// 创建新的物理角色映射。
    pub fn new(
        body_handles: HashMap<usize, BodyHandle>,
        root_body: BodyHandle,
        controller_type: CharacterControllerType,
        root_node: usize,
    ) -> Self {
        Self {
            body_handles,
            root_body,
            controller_type,
            root_node,
        }
    }

    /// 从 `PhysicsScene` 读取各 body 的世界变换，写入对应 `SceneNode.local_transform`。
    ///
    /// - 对于根节点：读取 `root_body` 的 world transform 作为整个骨架的根位移
    /// - 对于非根节点：读取对应 body 的 world transform，再乘上父节点 world transform 的逆得到 local
    ///
    /// 调用后需要执行 `Scene::update_world_transforms()` 传播变换到渲染对象。
    pub fn update_physics_transforms(
        &self,
        physics_scene: &PhysicsScene,
        nodes: &mut [SceneNode],
    ) {
        // 1. 先读取根 body 的世界变换
        let root_world = match physics_scene.body_isometry(self.root_body) {
            Some(iso) => iso,
            None => return,
        };

        let root_translation =
            Vector3::new(root_world.translation.x, root_world.translation.y, root_world.translation.z);
        let root_rotation = Quaternion::new(
            root_world.rotation.w,
            root_world.rotation.x,
            root_world.rotation.y,
            root_world.rotation.z,
        );

        // 更新根节点的 local_transform
        if let Some(root_node) = nodes.get_mut(self.root_node) {
            root_node.local_transform.translation = root_translation;
            root_node.local_transform.rotation = root_rotation;
            // scale 保持 base_transform 的值（物理不改变缩放）
        }

        // 2. 递归更新所有子节点
        self.update_node_recursive(self.root_node, root_world, physics_scene, nodes);
    }

    /// 递归更新子节点的变换。
    fn update_node_recursive(
        &self,
        node_idx: usize,
        parent_world_iso: Iso3,
        physics_scene: &PhysicsScene,
        nodes: &mut [SceneNode],
    ) {
        let Some(node) = nodes.get(node_idx) else {
            return;
        };
        let children = node.children.clone();

        for &child_idx in &children {
            // 如果该子节点有关联的物理刚体，使用物理变换
            let child_world_iso = if let Some(&body_handle) = self.body_handles.get(&child_idx) {
                physics_scene.body_isometry(body_handle).unwrap_or_else(|| {
                    // 回退：使用动画的本地变换乘父世界矩阵
                    let local = nodes[child_idx].local_transform.matrix();
                    let world_m = matrix4_from_iso(parent_world_iso) * local;
                    iso_from_matrix(world_m)
                })
            } else {
                // 无关联刚体：保持动画驱动的变换
                let local = nodes[child_idx].local_transform.matrix();
                let world_m = matrix4_from_iso(parent_world_iso) * local;
                iso_from_matrix(world_m)
            };

            // 计算 local_transform = parent_world.inverse() * child_world
            let parent_world_m = matrix4_from_iso(parent_world_iso);
            let parent_world_inv = parent_world_m
                .invert()
                .unwrap_or_else(Matrix4::identity);

            let child_world_m = matrix4_from_iso(child_world_iso);
            let local_m = parent_world_inv * child_world_m;

            // 从 local 矩阵提取 translation 和 rotation
            if let Some(child_node) = nodes.get_mut(child_idx) {
                child_node.local_transform.translation =
                    Vector3::new(local_m.w.x, local_m.w.y, local_m.w.z);
                child_node.local_transform.rotation = quat_from_matrix4(local_m);
            }

            // 递归处理更深层级
            self.update_node_recursive(child_idx, child_world_iso, physics_scene, nodes);
        }
    }
}

/// 从 physics Iso3 构造 cgmath Matrix4。
fn matrix4_from_iso(iso: Iso3) -> Matrix4<f32> {
    let t = iso.translation;
    let q = iso.rotation;

    // glam Quat → cgmath Quaternion
    let cg_quat = Quaternion::new(q.w, q.x, q.y, q.z);
    let rot_mat: Matrix3<f32> = cg_quat.into();

    Matrix4::new(
        rot_mat.x.x, rot_mat.x.y, rot_mat.x.z, 0.0,
        rot_mat.y.x, rot_mat.y.y, rot_mat.y.z, 0.0,
        rot_mat.z.x, rot_mat.z.y, rot_mat.z.z, 0.0,
        t.x, t.y, t.z, 1.0,
    )
}

/// 从 4x4 矩阵提取旋转四元数（纯旋转部分）。
fn quat_from_matrix4(m: Matrix4<f32>) -> Quaternion<f32> {
    let rot_mat = Matrix3::new(
        m.x.x, m.x.y, m.x.z,
        m.y.x, m.y.y, m.y.z,
        m.z.x, m.z.y, m.z.z,
    );
    rot_mat.into()
}

/// 从 cgmath Matrix4 构造 physics Iso3。
fn iso_from_matrix(m: Matrix4<f32>) -> Iso3 {
    let translation = PhyVec3::new(m.w.x, m.w.y, m.w.z);

    // 提取旋转四元数（纯旋转部分）
    let rot_mat = Matrix3::new(
        m.x.x, m.x.y, m.x.z,
        m.y.x, m.y.y, m.y.z,
        m.z.x, m.z.y, m.z.z,
    );
    let quat: Quaternion<f32> = rot_mat.into();
    let rotation = physics::math::Quat::from_xyzw(quat.v.x, quat.v.y, quat.v.z, quat.s);

    Iso3::from_parts(translation.into(), rotation)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_character_physics_new() {
        let cp = CharacterPhysics::new(
            HashMap::new(),
            BodyHandle::default(),
            CharacterControllerType::Capsule,
            0,
        );
        assert_eq!(cp.controller_type, CharacterControllerType::Capsule);
        assert_eq!(cp.root_node, 0);
    }
}
