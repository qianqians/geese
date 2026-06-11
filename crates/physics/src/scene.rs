//! 单个物理场景：封装 rapier 的全套 step 状态。

use std::sync::mpsc;

use rapier3d::geometry::{ColliderBuilder, ContactPair};
use rapier3d::pipeline::{ChannelEventCollector, EventHandler, QueryFilter};
use rapier3d::prelude as rp;
use rapier3d::prelude::{ColliderHandle as RpColliderHandle, RigidBodyHandle as RpRigidBodyHandle};

use crate::handles::{BodyHandle, ColliderHandle, SceneId};
use crate::math::{Iso3, Quat, Vec3, vec3_to_tuple};
use crate::shapes::ShapeDesc;
use crate::world::{BodyDesc, BodyKind};

/// 射线命中信息。
#[derive(Debug, Clone, Copy)]
pub struct RayHit {
    pub body: BodyHandle,
    pub collider: ColliderHandle,
    pub toi: f32,
    pub normal: (f32, f32, f32),
    pub point: (f32, f32, f32),
}

/// 碰撞事件（开始/结束）。
#[derive(Debug, Clone, Copy)]
pub struct CollisionEvent {
    pub a: ColliderHandle,
    pub b: ColliderHandle,
    pub started: bool,
    pub sensor: bool,
}

/// 接触力事件（仅在 collider 启用 `CONTACT_FORCE_EVENTS` 时产生）。
#[derive(Debug, Clone, Copy)]
pub struct ContactForceEvent {
    pub a: ColliderHandle,
    pub b: ColliderHandle,
    pub total_force_magnitude: f32,
}

pub struct PhysicsScene {
    id: SceneId,
    gravity: Vec3,

    pub(crate) bodies: rp::RigidBodySet,
    pub(crate) colliders: rp::ColliderSet,
    pub(crate) impulse_joints: rp::ImpulseJointSet,
    pub(crate) multibody_joints: rp::MultibodyJointSet,

    integration_parameters: rp::IntegrationParameters,
    physics_pipeline: rp::PhysicsPipeline,
    island_manager: rp::IslandManager,
    broad_phase: rp::BroadPhaseBvh,
    narrow_phase: rp::NarrowPhase,
    ccd_solver: rp::CCDSolver,

    collision_recv: mpsc::Receiver<rp::CollisionEvent>,
    contact_force_recv: mpsc::Receiver<rp::ContactForceEvent>,
    event_handler: ChannelEventCollector,

    // 保留 sender 副本，避免通道关闭。
    _collision_send_keep: mpsc::Sender<rp::CollisionEvent>,
    _contact_force_send_keep: mpsc::Sender<rp::ContactForceEvent>,
}

impl PhysicsScene {
    pub(crate) fn new(id: SceneId, gravity: Vec3) -> Self {
        let (col_send, col_recv) = mpsc::channel();
        let (cf_send, cf_recv) = mpsc::channel();
        let event_handler = ChannelEventCollector::new(col_send.clone(), cf_send.clone());

        Self {
            id,
            gravity,
            bodies: rp::RigidBodySet::new(),
            colliders: rp::ColliderSet::new(),
            impulse_joints: rp::ImpulseJointSet::new(),
            multibody_joints: rp::MultibodyJointSet::new(),
            integration_parameters: rp::IntegrationParameters::default(),
            physics_pipeline: rp::PhysicsPipeline::new(),
            island_manager: rp::IslandManager::new(),
            broad_phase: rp::BroadPhaseBvh::new(),
            narrow_phase: rp::NarrowPhase::new(),
            ccd_solver: rp::CCDSolver::new(),
            collision_recv: col_recv,
            contact_force_recv: cf_recv,
            event_handler,
            _collision_send_keep: col_send,
            _contact_force_send_keep: cf_send,
        }
    }

    pub fn id(&self) -> SceneId {
        self.id
    }

    pub fn gravity(&self) -> Vec3 {
        self.gravity
    }

    pub fn set_gravity(&mut self, gravity: Vec3) {
        self.gravity = gravity;
    }

    /// 步进物理;dt 单位秒。
    pub fn step(&mut self, dt: f32) {
        self.integration_parameters.dt = dt.max(1e-6);
        let physics_hooks = ();
        let event_handler: &dyn EventHandler = &self.event_handler;
        self.physics_pipeline.step(
            self.gravity,
            &self.integration_parameters,
            &mut self.island_manager,
            &mut self.broad_phase,
            &mut self.narrow_phase,
            &mut self.bodies,
            &mut self.colliders,
            &mut self.impulse_joints,
            &mut self.multibody_joints,
            &mut self.ccd_solver,
            &physics_hooks,
            event_handler,
        );
    }

    /// 添加刚体（含主形状）。返回 BodyHandle 与 ColliderHandle。
    pub fn add_body(
        &mut self,
        desc: BodyDesc,
        shape: ShapeDesc,
    ) -> Result<(BodyHandle, ColliderHandle), String> {
        let mut rb_builder = match desc.kind {
            BodyKind::Dynamic => rp::RigidBodyBuilder::dynamic(),
            BodyKind::Fixed => rp::RigidBodyBuilder::fixed(),
            BodyKind::KinematicPosition => rp::RigidBodyBuilder::kinematic_position_based(),
            BodyKind::KinematicVelocity => rp::RigidBodyBuilder::kinematic_velocity_based(),
        };
        rb_builder = rb_builder
            .pose(desc.position)
            .linvel(desc.linvel)
            .angvel(desc.angvel)
            .can_sleep(desc.can_sleep)
            .ccd_enabled(desc.ccd_enabled)
            .gravity_scale(desc.gravity_scale);
        let rb = rb_builder.build();
        let body_handle = self.bodies.insert(rb);

        let shared = shape.into_shared()?;
        let mut col_builder = ColliderBuilder::new(shared)
            .density(desc.density.max(0.0))
            .friction(desc.friction)
            .restitution(desc.restitution)
            .sensor(desc.sensor);
        if desc.events {
            col_builder = col_builder.active_events(rp::ActiveEvents::COLLISION_EVENTS);
        }
        let collider = col_builder.build();
        let collider_handle =
            self.colliders
                .insert_with_parent(collider, body_handle, &mut self.bodies);

        Ok((
            BodyHandle::new(self.id, body_handle),
            ColliderHandle::new(self.id, collider_handle),
        ))
    }

    /// 批量添加静态三角网格碰撞体（feature = "scene-builder"）。
    ///
    /// 顶点已通过 GLTF 节点世界变换预转换，`transform` 为额外的
    /// manifest 层变换（如平移/旋转/缩放）。
    /// 每个网格创建独立的 `Fixed` 刚体。
    #[cfg(feature = "scene-builder")]
    pub fn add_static_trimeshes(
        &mut self,
        meshes: &[crate::scene_builder::TrimeshData],
        transform: Iso3,
        friction: f32,
        restitution: f32,
    ) -> Result<Vec<(BodyHandle, ColliderHandle)>, String> {
        let mut handles = Vec::new();
        for mesh in meshes {
            if mesh.vertices.is_empty() || mesh.indices.is_empty() {
                continue;
            }
            let transformed_verts: Vec<[f32; 3]> = mesh
                .vertices
                .iter()
                .map(|v| {
                    let p = transform * Vec3::new(v[0], v[1], v[2]);
                    [p.x, p.y, p.z]
                })
                .collect();
            let shape = ShapeDesc::trimesh(transformed_verts, mesh.indices.clone());
            let desc = BodyDesc {
                kind: BodyKind::Fixed,
                position: Iso3::identity(),
                friction,
                restitution,
                ..Default::default()
            };
            handles.push(self.add_body(desc, shape)?);
        }
        Ok(handles)
    }

    /// 移除刚体（连同所有 collider 与 joint）。
    pub fn remove_body(&mut self, handle: BodyHandle) -> bool {
        if handle.scene != self.id {
            return false;
        }
        self.bodies
            .remove(
                handle.inner,
                &mut self.island_manager,
                &mut self.colliders,
                &mut self.impulse_joints,
                &mut self.multibody_joints,
                true,
            )
            .is_some()
    }

    /// 仅移除单个 collider（保留刚体）。
    pub fn remove_collider(&mut self, handle: ColliderHandle) -> bool {
        if handle.scene != self.id {
            return false;
        }
        self.colliders
            .remove(
                handle.inner,
                &mut self.island_manager,
                &mut self.bodies,
                true,
            )
            .is_some()
    }

    pub fn body_isometry(&self, handle: BodyHandle) -> Option<Iso3> {
        self.check_body(handle)
            .and_then(|h| self.bodies.get(h))
            .map(|b| *b.position())
    }

    pub fn body_linvel(&self, handle: BodyHandle) -> Option<Vec3> {
        self.check_body(handle)
            .and_then(|h| self.bodies.get(h))
            .map(|b| b.linvel())
    }

    pub fn body_angvel(&self, handle: BodyHandle) -> Option<Vec3> {
        self.check_body(handle)
            .and_then(|h| self.bodies.get(h))
            .map(|b| b.angvel())
    }

    pub fn set_translation(&mut self, handle: BodyHandle, t: Vec3, wake_up: bool) -> bool {
        if let Some(h) = self.check_body(handle)
            && let Some(rb) = self.bodies.get_mut(h)
        {
            rb.set_translation(t, wake_up);
            return true;
        }
        false
    }

    pub fn set_rotation(&mut self, handle: BodyHandle, rotation: Quat, wake_up: bool) -> bool {
        if let Some(h) = self.check_body(handle)
            && let Some(rb) = self.bodies.get_mut(h)
        {
            rb.set_rotation(rotation, wake_up);
            return true;
        }
        false
    }

    pub fn set_linvel(&mut self, handle: BodyHandle, v: Vec3, wake_up: bool) -> bool {
        if let Some(h) = self.check_body(handle)
            && let Some(rb) = self.bodies.get_mut(h)
        {
            rb.set_linvel(v, wake_up);
            return true;
        }
        false
    }

    pub fn set_angvel(&mut self, handle: BodyHandle, v: Vec3, wake_up: bool) -> bool {
        if let Some(h) = self.check_body(handle)
            && let Some(rb) = self.bodies.get_mut(h)
        {
            rb.set_angvel(v, wake_up);
            return true;
        }
        false
    }

    pub fn apply_impulse(&mut self, handle: BodyHandle, impulse: Vec3, wake_up: bool) -> bool {
        if let Some(h) = self.check_body(handle)
            && let Some(rb) = self.bodies.get_mut(h)
        {
            rb.apply_impulse(impulse, wake_up);
            return true;
        }
        false
    }

    pub fn apply_torque_impulse(
        &mut self,
        handle: BodyHandle,
        torque: Vec3,
        wake_up: bool,
    ) -> bool {
        if let Some(h) = self.check_body(handle)
            && let Some(rb) = self.bodies.get_mut(h)
        {
            rb.apply_torque_impulse(torque, wake_up);
            return true;
        }
        false
    }

    pub fn cast_ray(
        &self,
        origin: Vec3,
        dir: Vec3,
        max_toi: f32,
        solid: bool,
    ) -> Option<RayHit> {
        let ray = rp::Ray::new(origin, dir);
        let query_pipeline = self.broad_phase.as_query_pipeline(
            self.narrow_phase.query_dispatcher(),
            &self.bodies,
            &self.colliders,
            QueryFilter::default(),
        );
        query_pipeline
            .cast_ray_and_get_normal(&ray, max_toi, solid)
            .map(|(collider_h, intersection)| {
                let body_h = self
                    .colliders
                    .get(collider_h)
                    .and_then(|c| c.parent())
                    .unwrap_or_else(RpRigidBodyHandle::invalid);
                let point = ray.origin + ray.dir * intersection.time_of_impact;
                RayHit {
                    body: BodyHandle::new(self.id, body_h),
                    collider: ColliderHandle::new(self.id, collider_h),
                    toi: intersection.time_of_impact,
                    normal: vec3_to_tuple(intersection.normal),
                    point: (point.x, point.y, point.z),
                }
            })
    }

    /// 取出累计的碰撞事件（开始/结束）。
    pub fn drain_collision_events(&self) -> Vec<CollisionEvent> {
        let mut out = Vec::new();
        while let Ok(event) = self.collision_recv.try_recv() {
            let (a, b, started, sensor) = match event {
                rp::CollisionEvent::Started(a, b, flags) => (
                    a,
                    b,
                    true,
                    flags.contains(rp::CollisionEventFlags::SENSOR),
                ),
                rp::CollisionEvent::Stopped(a, b, flags) => (
                    a,
                    b,
                    false,
                    flags.contains(rp::CollisionEventFlags::SENSOR),
                ),
            };
            out.push(CollisionEvent {
                a: ColliderHandle::new(self.id, a),
                b: ColliderHandle::new(self.id, b),
                started,
                sensor,
            });
        }
        out
    }

    pub fn drain_contact_force_events(&self) -> Vec<ContactForceEvent> {
        let mut out = Vec::new();
        while let Ok(event) = self.contact_force_recv.try_recv() {
            out.push(ContactForceEvent {
                a: ColliderHandle::new(self.id, event.collider1),
                b: ColliderHandle::new(self.id, event.collider2),
                total_force_magnitude: event.total_force_magnitude,
            });
        }
        out
    }

    pub fn contains_body(&self, handle: BodyHandle) -> bool {
        self.check_body(handle).is_some()
    }

    pub fn contains_collider(&self, handle: ColliderHandle) -> bool {
        if handle.scene != self.id {
            return false;
        }
        self.colliders.get(handle.inner).is_some()
    }

    /// 遍历当前帧的接触对，便于业务侧自查（性能敏感场景请慎用）。
    pub fn for_each_contact_pair<F: FnMut(&ContactPair)>(&self, mut f: F) {
        for pair in self.narrow_phase.contact_pairs() {
            f(pair);
        }
    }

    /// 返回场景中所有刚体句柄的迭代器（供调试渲染遍历碰撞体快照）。
    pub fn body_handles(&self) -> impl Iterator<Item = BodyHandle> + '_ {
        self.bodies.iter().map(|(handle, _)| BodyHandle::new(self.id, handle))
    }

    fn check_body(&self, handle: BodyHandle) -> Option<RpRigidBodyHandle> {
        if handle.scene != self.id {
            return None;
        }
        self.bodies.get(handle.inner).map(|_| handle.inner)
    }

    /// 仅供内部辅助，避免外部暴露内部 collider 句柄。
    #[allow(dead_code)]
    pub(crate) fn raw_collider_handle(
        &self,
        handle: ColliderHandle,
    ) -> Option<RpColliderHandle> {
        if handle.scene != self.id {
            return None;
        }
        self.colliders.get(handle.inner).map(|_| handle.inner)
    }
}
