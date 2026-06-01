//! 顶层物理世界：管理多个 [`PhysicsScene`]。

use slotmap::SlotMap;

use crate::handles::SceneId;
use crate::math::{Iso3, Vec3};
use crate::scene::PhysicsScene;

/// 刚体类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BodyKind {
    /// 受重力与外力影响。
    Dynamic,
    /// 静止地形。
    Fixed,
    /// 由位置驱动的运动学体（每帧 set_translation/rotation）。
    KinematicPosition,
    /// 由速度驱动的运动学体（每帧 set_linvel/angvel）。
    KinematicVelocity,
}

/// 刚体描述。
#[derive(Debug, Clone)]
pub struct BodyDesc {
    pub kind: BodyKind,
    pub position: Iso3,
    pub linvel: Vec3,
    pub angvel: Vec3,
    pub density: f32,
    pub friction: f32,
    pub restitution: f32,
    pub gravity_scale: f32,
    pub can_sleep: bool,
    pub ccd_enabled: bool,
    pub sensor: bool,
    pub events: bool,
}

impl Default for BodyDesc {
    fn default() -> Self {
        Self {
            kind: BodyKind::Dynamic,
            position: Iso3::identity(),
            linvel: Vec3::ZERO,
            angvel: Vec3::ZERO,
            density: 1.0,
            friction: 0.5,
            restitution: 0.0,
            gravity_scale: 1.0,
            can_sleep: true,
            ccd_enabled: false,
            sensor: false,
            events: false,
        }
    }
}

impl BodyDesc {
    pub fn new(kind: BodyKind) -> Self {
        Self {
            kind,
            ..Default::default()
        }
    }

    pub fn dynamic() -> Self {
        Self::new(BodyKind::Dynamic)
    }

    pub fn fixed() -> Self {
        Self::new(BodyKind::Fixed)
    }

    pub fn kinematic_position() -> Self {
        Self::new(BodyKind::KinematicPosition)
    }

    pub fn kinematic_velocity() -> Self {
        Self::new(BodyKind::KinematicVelocity)
    }

    pub fn position(mut self, iso: Iso3) -> Self {
        self.position = iso;
        self
    }

    pub fn linvel(mut self, v: Vec3) -> Self {
        self.linvel = v;
        self
    }

    pub fn angvel(mut self, v: Vec3) -> Self {
        self.angvel = v;
        self
    }

    pub fn density(mut self, d: f32) -> Self {
        self.density = d;
        self
    }

    pub fn friction(mut self, f: f32) -> Self {
        self.friction = f;
        self
    }

    pub fn restitution(mut self, r: f32) -> Self {
        self.restitution = r;
        self
    }

    pub fn gravity_scale(mut self, s: f32) -> Self {
        self.gravity_scale = s;
        self
    }

    pub fn sensor(mut self, sensor: bool) -> Self {
        self.sensor = sensor;
        self
    }

    pub fn events(mut self, events: bool) -> Self {
        self.events = events;
        self
    }

    pub fn ccd(mut self, enabled: bool) -> Self {
        self.ccd_enabled = enabled;
        self
    }

    pub fn can_sleep(mut self, can_sleep: bool) -> Self {
        self.can_sleep = can_sleep;
        self
    }
}

/// 物理世界顶层管理器。
pub struct PhysicsWorld {
    scenes: SlotMap<SceneId, PhysicsScene>,
}

impl PhysicsWorld {
    pub fn new() -> Self {
        Self {
            scenes: SlotMap::with_key(),
        }
    }

    pub fn create_scene(&mut self, gravity: Vec3) -> SceneId {
        self.scenes
            .insert_with_key(|key| PhysicsScene::new(key, gravity))
    }

    pub fn destroy_scene(&mut self, id: SceneId) -> bool {
        self.scenes.remove(id).is_some()
    }

    pub fn contains_scene(&self, id: SceneId) -> bool {
        self.scenes.contains_key(id)
    }

    pub fn scene(&self, id: SceneId) -> Option<&PhysicsScene> {
        self.scenes.get(id)
    }

    pub fn scene_mut(&mut self, id: SceneId) -> Option<&mut PhysicsScene> {
        self.scenes.get_mut(id)
    }

    pub fn scene_count(&self) -> usize {
        self.scenes.len()
    }
}

impl Default for PhysicsWorld {
    fn default() -> Self {
        Self::new()
    }
}
