//! 物理碰撞体调试渲染。
//!
//! 从物理服务器获取碰撞体变换快照，在 3D 视口中绘制碰撞体线框。



// ---------------------------------------------------------------------------
// PhysicsDebugRenderer
// ---------------------------------------------------------------------------

/// 碰撞体调试渲染器。
///
/// 从 [`super::physics_client::PhysicsClient::get_bodies()`] 获取数据并渲染
/// 线框碰撞体。
pub struct PhysicsDebugRenderer {
    /// 是否启用。
    pub enabled: bool,
    /// 上次获取的碰撞体快照（id → position, rotation）。
    bodies: Vec<super::physics_client::BodySnapshot>,
}

impl PhysicsDebugRenderer {
    pub fn new() -> Self {
        Self {
            enabled: false,
            bodies: Vec::new(),
        }
    }

    /// 更新碰撞体快照。
    pub fn update(&mut self, bodies: Vec<super::physics_client::BodySnapshot>) {
        self.bodies = bodies;
    }

    /// 获取碰撞体快照列表。
    pub fn bodies(&self) -> &[super::physics_client::BodySnapshot] {
        &self.bodies
    }

    /// 切换启用/禁用。
    pub fn toggle(&mut self) {
        self.enabled = !self.enabled;
    }
}
