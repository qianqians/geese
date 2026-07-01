//! 句柄类型：SceneId / BodyHandle / ColliderHandle。

use rapier3d::prelude as rp;
use slotmap::new_key_type;

new_key_type! {
    /// 物理场景在 [`PhysicsWorld`](crate::PhysicsWorld) 内的句柄。
    pub struct SceneId;
}

/// 刚体句柄：包含所属场景与 rapier 内部句柄，用于跨 API 引用。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct BodyHandle {
    pub scene: SceneId,
    pub(crate) inner: rp::RigidBodyHandle,
}

impl BodyHandle {
    pub(crate) fn new(scene: SceneId, inner: rp::RigidBodyHandle) -> Self {
        Self { scene, inner }
    }

    pub fn raw(&self) -> rp::RigidBodyHandle {
        self.inner
    }

    pub fn scene(&self) -> SceneId {
        self.scene
    }
}

/// 形状句柄：包含所属场景与 rapier 内部 collider 句柄。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ColliderHandle {
    pub scene: SceneId,
    pub(crate) inner: rp::ColliderHandle,
}

impl ColliderHandle {
    pub(crate) fn new(scene: SceneId, inner: rp::ColliderHandle) -> Self {
        Self { scene, inner }
    }

    pub fn raw(&self) -> rp::ColliderHandle {
        self.inner
    }

    pub fn scene(&self) -> SceneId {
        self.scene
    }
}
