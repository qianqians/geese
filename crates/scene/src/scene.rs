use std::collections::{HashMap, HashSet, VecDeque};

use camera::frustum::Frustum;
use cgmath::InnerSpace;
use cgmath::{Matrix, SquareMatrix, Vector3};
use math::AABB;
use render::MaterialLibrary;
use render::{RenderQueue, SceneRenderer};

use avatar::{
    AnimatedProperty, AnimationClip, AnimationEvent, AnimationOutputs, AnimationPlayer,
    SceneNode, Skin, check_markers_crossed,
    quat_dot, quat_exp, quat_log, sample_clip, sample_indices, sample_quat, sample_vec3,
};
use avatar::AnimationStateMachine;
use crate::character_animation::CharacterAnimationGraph;
#[cfg(feature = "physics")]
use crate::character_physics::CharacterPhysics;
use crate::{Octree, SceneObject};
use crate::scene_object::DirtyFlags;
#[cfg(feature = "physics")]
use physics::scene::PhysicsScene;

pub struct MarkerEvent {
    pub marker_name: String,
    pub clip_index: usize,
    pub clip_name: Option<String>,
    pub entity_id: Option<String>,
}

/// 场景级动画事件，带有实体绑定。
#[derive(Clone, Debug)]
pub enum SceneAnimationEvent {
    /// 状态机完成状态切换。
    StateChanged { entity_id: Option<String>, from: String, to: String },
    /// 非循环动画播放结束。
    AnimationEnd { entity_id: Option<String>, clip_name: Option<String>, clip_index: usize },
    /// 循环动画完成一次循环。
    AnimationLoop { entity_id: Option<String>, clip_name: Option<String>, clip_index: usize },
}

/// 角色动画图与实体绑定。
pub struct EntityAnimationGraph {
    pub entity_id: String,
    pub graph: CharacterAnimationGraph,
}

pub struct Scene {
    pub nodes: Vec<SceneNode>,
    pub objects: Vec<SceneObject>,
    pub octree: Octree,
    pub materials: MaterialLibrary,
    pub animations: Vec<AnimationClip>,
    pub skins: Vec<Skin>,
    animation_names: HashMap<String, usize>,
    /// 静态对象在 self.objects 中的索引——这些对象的 aabb 不会因动画变化，进 octree。
    static_indices: Vec<usize>,
    /// 动态对象索引——会被动画驱动，每帧线性参与 frustum 测试，不进 octree 以避免每帧重建。
    dynamic_indices: Vec<usize>,
    pub bounds: AABB,
    max_objects: usize,
    /// 本帧内 remove_object 操作收集的 entity_id，由 drain_deleted_ids 消费。
    deleted_ids: Vec<String>,
    max_depth: usize,
    /// 物理角色映射列表（Phase 1 桥接）
    #[cfg(feature = "physics")]
    pub character_physics: Vec<CharacterPhysics>,
    /// 物理开关；关闭时跳过物理步进和动画混合
    pub physics_enabled: bool,
    /// 角色动画蓝图列表（Phase 4 动画混合），带实体绑定。
    pub character_anim_graphs: Vec<EntityAnimationGraph>,
    /// 本帧触发的动画标记事件，由外部消费者 drain。
    pub marker_events: Vec<MarkerEvent>,
    /// 本帧触发的动画系统事件，由外部消费者 drain。
    animation_events: Vec<SceneAnimationEvent>,
    /// 动画图每个剪辑的上次时间（用于标记跨越检测）
    graph_prev_times: HashMap<usize, f32>,
    /// entity_id → objects Vec 索引映射，O(1) 查找
    object_index: HashMap<String, usize>,
    /// 动画混合预分配缓冲区（避免每帧分配）
    blend_trans_acc: Vec<Vector3<f32>>,
    blend_rot_acc: Vec<Vector3<f32>>,
    blend_scale_acc: Vec<Vector3<f32>>,
    blend_has_trans: Vec<bool>,
    blend_has_rot: Vec<bool>,
    blend_has_scale: Vec<bool>,
}

impl Scene {
    pub fn new(
        nodes: Vec<SceneNode>,
        objects: Vec<SceneObject>,
        materials: MaterialLibrary,
        animations: Vec<AnimationClip>,
        skins: Vec<Skin>,
        bounds: AABB,
        max_objects: usize,
        max_depth: usize,
    ) -> Self {
        let animation_names: HashMap<String, usize> = animations
            .iter()
            .enumerate()
            .filter_map(|(i, a)| a.name.as_ref().map(|n| (n.clone(), i)))
            .collect();
        let (static_indices, dynamic_indices) =
            classify_objects(&nodes, &objects, &animations);
        let mut scene = Self {
            nodes,
            objects,
            octree: Octree::new(bounds, max_objects, max_depth),
            materials,
            animations,
            skins,
            animation_names,
            static_indices,
            dynamic_indices,
            bounds,
            max_objects,
            max_depth,
            deleted_ids: Vec::new(),
            #[cfg(feature = "physics")]
            character_physics: Vec::new(),
            physics_enabled: true,
            character_anim_graphs: Vec::new(),
            marker_events: Vec::new(),
            animation_events: Vec::new(),
            graph_prev_times: HashMap::new(),
            object_index: HashMap::new(),
            blend_trans_acc: Vec::new(),
            blend_rot_acc: Vec::new(),
            blend_scale_acc: Vec::new(),
            blend_has_trans: Vec::new(),
            blend_has_rot: Vec::new(),
            blend_has_scale: Vec::new(),
        };
        scene.build_object_index();
        scene.update_world_transforms();
        scene.rebuild_octree();
        scene
    }

    pub fn animation_index(&self, name: &str) -> Option<usize> {
        self.animation_names.get(name).copied()
    }

    pub fn animation_duration(&self, index: usize) -> Option<f32> {
        self.animations.get(index).map(|a| a.duration)
    }

    /// 静态对象索引（进 octree 的对象）。
    pub fn static_indices(&self) -> &[usize] {
        &self.static_indices
    }

    /// 动态对象索引（每帧 frustum 线性测试的对象）。
    pub fn dynamic_indices(&self) -> &[usize] {
        &self.dynamic_indices
    }

    /// 静态对象索引的可变引用（供 prefab_loader 等内部使用）。
    pub(crate) fn static_indices_mut(&mut self) -> &mut Vec<usize> {
        &mut self.static_indices
    }

    /// 动态对象索引的可变引用（供 prefab_loader 等内部使用）。
    pub(crate) fn dynamic_indices_mut(&mut self) -> &mut Vec<usize> {
        &mut self.dynamic_indices
    }

    /// 取场景全部对象切片引用。
    pub fn objects(&self) -> &[SceneObject] {
        &self.objects
    }

    /// 视锥剪枝：静态对象走八叉树，动态对象逐个做 AABB 测试，合并返回。
    pub fn visible_objects(&self, frustum: &Frustum) -> Vec<&SceneObject> {
        let mut result: Vec<&SceneObject> = self
            .octree
            .query_frustum(frustum)
            .into_iter()
            .map(|id| &self.objects[id])
            .collect();

        for &id in &self.dynamic_indices {
            let obj = &self.objects[id];
            if frustum.intersects_aabb(obj.aabb.min, obj.aabb.max) {
                result.push(obj);
            }
        }

        result
    }

    pub fn render_queue<'a>(
        &'a self,
        renderer: &'a SceneRenderer,
        frustum: Option<&Frustum>,
    ) -> RenderQueue<'a> {
        match frustum {
            Some(frustum) => renderer.build_queue(&self.materials, self.visible_objects(frustum)),
            None => renderer.build_queue(&self.materials, self.objects()),
        }
    }

    pub fn update_animation(&mut self, player: &mut AnimationPlayer, dt: f32) {
        let Some(clip) = self.animations.get(player.clip) else {
            return;
        };

        let prev_time = player.time;
        player.advance(dt, clip.duration);

        // 检测循环回绕
        if player.time < prev_time {
            self.animation_events.push(SceneAnimationEvent::AnimationLoop {
                entity_id: None,
                clip_name: clip.name.clone(),
                clip_index: player.clip,
            });
        }
        // 检测动画结束
        if player.just_ended {
            self.animation_events.push(SceneAnimationEvent::AnimationEnd {
                entity_id: None,
                clip_name: clip.name.clone(),
                clip_index: player.clip,
            });
        }

        // 检测标记跨越
        if !clip.markers.is_empty() {
            let crossed = check_markers_crossed(
                &clip.markers, prev_time, player.time, clip.duration,
            );
            for idx in crossed {
                let m = &clip.markers[idx];
                self.marker_events.push(MarkerEvent {
                    marker_name: m.name.clone(),
                    clip_index: player.clip,
                    clip_name: clip.name.clone(),
                    entity_id: None,
                });
            }
        }

        sample_clip(clip, player.time, &mut self.nodes);
        self.update_world_transforms();
        // 静态 octree 不需要每帧重建——动态对象的 aabb 已在 update_world_transforms 中更新，
        // visible_objects 通过 dynamic_indices 线性测试覆盖它们。
    }

    pub fn update_animation_graph(&mut self, graph: &mut AnimationStateMachine, dt: f32) {
        use cgmath::Vector3;

        let active = graph.update(dt, &self.animations);

        // 消费状态机事件并转换为场景级事件
        for evt in graph.drain_events() {
            match evt {
                AnimationEvent::StateChanged { from, to } => {
                    self.animation_events.push(SceneAnimationEvent::StateChanged {
                        entity_id: None, from, to,
                    });
                }
                AnimationEvent::AnimationLoop { clip_index } => {
                    let clip_name = self.animations.get(clip_index).and_then(|c| c.name.clone());
                    self.animation_events.push(SceneAnimationEvent::AnimationLoop {
                        entity_id: None, clip_name, clip_index,
                    });
                }
                AnimationEvent::AnimationEnd { clip_index } => {
                    let clip_name = self.animations.get(clip_index).and_then(|c| c.name.clone());
                    self.animation_events.push(SceneAnimationEvent::AnimationEnd {
                        entity_id: None, clip_name, clip_index,
                    });
                }
            }
        }

        // 检测标记跨越（仅主动画 weight > 0.5 触发，避免过渡时两边同时触发）
        for anim in &active {
            if anim.weight > 0.5 {
                if let Some(clip) = self.animations.get(anim.clip) {
                    if !clip.markers.is_empty() {
                        let prev = self.graph_prev_times.get(&anim.clip).copied().unwrap_or(0.0);
                        let crossed = check_markers_crossed(
                            &clip.markers, prev, anim.time, clip.duration,
                        );
                        for idx in crossed {
                            let m = &clip.markers[idx];
                            self.marker_events.push(MarkerEvent {
                                marker_name: m.name.clone(),
                                clip_index: anim.clip,
                                clip_name: clip.name.clone(),
                                entity_id: None,
                            });
                        }
                    }
                }
                self.graph_prev_times.insert(anim.clip, anim.time);
            }
        }

        if active.len() == 1 && (active[0].weight - 1.0).abs() < f32::EPSILON {
            if let Some(clip) = self.animations.get(active[0].clip) {
                sample_clip(clip, active[0].time, &mut self.nodes);
            }
        } else if !active.is_empty() {
            for node in self.nodes.iter_mut() {
                node.local_transform = node.base_transform;
            }

            let n = self.nodes.len();
            self.blend_trans_acc.resize(n, Vector3::new(0.0, 0.0, 0.0));
            self.blend_rot_acc.resize(n, Vector3::new(0.0, 0.0, 0.0));
            self.blend_scale_acc.resize(n, Vector3::new(0.0, 0.0, 0.0));
            self.blend_has_trans.resize(n, false);
            self.blend_has_rot.resize(n, false);
            self.blend_has_scale.resize(n, false);
            for i in 0..n {
                self.blend_trans_acc[i] = Vector3::new(0.0, 0.0, 0.0);
                self.blend_rot_acc[i] = Vector3::new(0.0, 0.0, 0.0);
                self.blend_scale_acc[i] = Vector3::new(0.0, 0.0, 0.0);
                self.blend_has_trans[i] = false;
                self.blend_has_rot[i] = false;
                self.blend_has_scale[i] = false;
            }

            for anim in &active {
                let Some(clip) = self.animations.get(anim.clip) else {
                    continue;
                };
                for channel in &clip.channels {
                    if channel.inputs.is_empty() || channel.target_node >= self.nodes.len() {
                        continue;
                    }
                    let (left, right, factor, interval) =
                        sample_indices(&channel.inputs, anim.time, channel.interpolation);
                    let base = self.nodes[channel.target_node].base_transform;

                    match (&channel.property, &channel.outputs) {
                        (
                            AnimatedProperty::Translation,
                            AnimationOutputs::Translations(_),
                        ) => {
                            let v = sample_vec3(
                                &channel.outputs, left, right, factor, interval, channel.interpolation,
                            );
                            self.blend_trans_acc[channel.target_node] += (v - base.translation) * anim.weight;
                            self.blend_has_trans[channel.target_node] = true;
                        }
                        (AnimatedProperty::Rotation, AnimationOutputs::Rotations(_)) => {
                            let q = sample_quat(
                                &channel.outputs, left, right, factor, interval, channel.interpolation,
                            );
                            let q = if quat_dot(q, base.rotation) < 0.0 {
                                -q
                            } else {
                                q
                            };
                            let relative = quat_log(q * base.rotation.conjugate());
                            self.blend_rot_acc[channel.target_node] += relative * anim.weight;
                            self.blend_has_rot[channel.target_node] = true;
                        }
                        (AnimatedProperty::Scale, AnimationOutputs::Scales(_)) => {
                            let v = sample_vec3(
                                &channel.outputs, left, right, factor, interval, channel.interpolation,
                            );
                            self.blend_scale_acc[channel.target_node] += (v - base.scale) * anim.weight;
                            self.blend_has_scale[channel.target_node] = true;
                        }
                        _ => {}
                    }
                }
            }

            for i in 0..self.nodes.len() {
                if self.blend_has_trans[i] {
                    self.nodes[i].local_transform.translation =
                        self.nodes[i].base_transform.translation + self.blend_trans_acc[i];
                }
                if self.blend_has_rot[i] {
                    self.nodes[i].local_transform.rotation =
                        (quat_exp(self.blend_rot_acc[i]) * self.nodes[i].base_transform.rotation)
                            .normalize();
                }
                if self.blend_has_scale[i] {
                    self.nodes[i].local_transform.scale =
                        self.nodes[i].base_transform.scale + self.blend_scale_acc[i];
                }
            }
        }

        self.update_world_transforms();
        // 同 update_animation：静态 octree 不重建。
    }

    pub fn update_world_transforms(&mut self) {
        let roots: Vec<_> = self
            .nodes
            .iter()
            .filter(|node| node.parent.is_none())
            .map(|node| node.id)
            .collect();

        for root in roots {
            self.update_node_world(root, cgmath::Matrix4::from_scale(1.0), 0);
        }
    }

    /// 从物理场景读取刚体变换，更新关联节点的 `local_transform`，
    /// 然后执行 `update_world_transforms()` 传播到渲染对象。
    ///
    /// 仅在 `physics_enabled` 为 true 时生效。
    #[cfg(feature = "physics")]
    pub fn update_physics(&mut self, physics_scene: &PhysicsScene) {
        if !self.physics_enabled {
            return;
        }
        for cp in &self.character_physics {
            cp.update_physics_transforms(physics_scene, &mut self.nodes);
        }
        self.update_world_transforms();
    }

    /// 角色动画更新：读取物理速度，驱动动画状态机并采样。
    ///
    /// 遍历所有 `character_anim_graphs`，对每个角色从外部传入
    /// 水平速度、垂直速度和着地状态，更新动画状态机后采样动画剪辑到节点。
    ///
    /// 仅操作每个角色动画实际目标的节点子集，避免多角色动画互相覆盖。
    /// 全部角色更新完成后统一调用 `update_world_transforms()`。
    pub fn update_character_animation(
        &mut self,
        velocities: &[f32],
        vertical_velocities: &[f32],
        grounded_flags: &[bool],
        dt: f32,
    ) {
        if !self.physics_enabled {
            return;
        }
        let count = self.character_anim_graphs.len()
            .min(velocities.len())
            .min(vertical_velocities.len())
            .min(grounded_flags.len());
        if count < self.character_anim_graphs.len() {
            log::warn!(
                "[Scene] update_character_animation: {} graphs but only {}/vertical {}/grounded {} arrays, skipping extra",
                self.character_anim_graphs.len(), velocities.len(), vertical_velocities.len(), grounded_flags.len()
            );
        }
        for i in 0..count {
            let eag = &mut self.character_anim_graphs[i];
            let entity_id = eag.entity_id.clone();
            let active = eag.graph.update(
                velocities[i],
                vertical_velocities[i],
                grounded_flags[i],
                dt,
                &self.animations,
            );
            // 消费状态机事件并绑定 entity_id
            for evt in eag.graph.drain_events() {
                match evt {
                    AnimationEvent::StateChanged { from, to } => {
                        self.animation_events.push(SceneAnimationEvent::StateChanged {
                            entity_id: Some(entity_id.clone()), from, to,
                        });
                    }
                    AnimationEvent::AnimationLoop { clip_index } => {
                        let clip_name = self.animations.get(clip_index).and_then(|c| c.name.clone());
                        self.animation_events.push(SceneAnimationEvent::AnimationLoop {
                            entity_id: Some(entity_id.clone()), clip_name, clip_index,
                        });
                    }
                    AnimationEvent::AnimationEnd { clip_index } => {
                        let clip_name = self.animations.get(clip_index).and_then(|c| c.name.clone());
                        self.animation_events.push(SceneAnimationEvent::AnimationEnd {
                            entity_id: Some(entity_id.clone()), clip_name, clip_index,
                        });
                    }
                }
            }
            // 将活跃动画采样到节点（仅操作目标节点子集）
            self.apply_active_animations(&active);
        }
        // 所有角色动画更新完成后统一重建世界变换
        self.update_world_transforms();
    }

    /// 将活跃动画列表采样到场景节点。
    ///
    /// 仅操作活跃动画实际目标的节点子集（通过 clip channels 的 target_node 收集），
    /// 避免多角色场景中一个角色的动画重置另一个角色的节点。
    /// 不内部调用 `update_world_transforms()`，由调用方统一执行。
    fn apply_active_animations(&mut self, active: &[avatar::ActiveAnimation]) {
        use cgmath::Vector3;

        if active.is_empty() {
            return;
        }

        // 1. 收集所有活跃动画的目标节点索引
        let mut target_nodes: HashSet<usize> = HashSet::new();
        for anim in active {
            let Some(clip) = self.animations.get(anim.clip) else {
                continue;
            };
            for channel in &clip.channels {
                if channel.target_node < self.nodes.len() {
                    target_nodes.insert(channel.target_node);
                }
            }
        }

        if target_nodes.is_empty() {
            return;
        }

        // 2. 只重置目标节点到 base_transform（不影响其他角色的节点）
        for &node_idx in &target_nodes {
            self.nodes[node_idx].local_transform = self.nodes[node_idx].base_transform;
        }

        // 3. 采样并叠加动画数据到目标节点
        let mut trans_acc: HashMap<usize, Vector3<f32>> = HashMap::new();
        let mut rot_acc: HashMap<usize, Vector3<f32>> = HashMap::new();
        let mut scale_acc: HashMap<usize, Vector3<f32>> = HashMap::new();

        for anim in active {
            let Some(clip) = self.animations.get(anim.clip) else {
                continue;
            };
            for channel in &clip.channels {
                if channel.inputs.is_empty() || channel.target_node >= self.nodes.len() {
                    continue;
                }
                let (left, right, factor, interval) =
                    sample_indices(&channel.inputs, anim.time, channel.interpolation);
                let base = self.nodes[channel.target_node].base_transform;

                match (&channel.property, &channel.outputs) {
                    (
                        AnimatedProperty::Translation,
                        AnimationOutputs::Translations(_),
                    ) => {
                        let v = sample_vec3(
                            &channel.outputs, left, right, factor, interval, channel.interpolation,
                        );
                        *trans_acc.entry(channel.target_node).or_insert(Vector3::new(0.0, 0.0, 0.0)) +=
                            (v - base.translation) * anim.weight;
                    }
                    (AnimatedProperty::Rotation, AnimationOutputs::Rotations(_)) => {
                        let q = sample_quat(
                            &channel.outputs, left, right, factor, interval, channel.interpolation,
                        );
                        let q = if quat_dot(q, base.rotation) < 0.0 {
                            -q
                        } else {
                            q
                        };
                        let relative = quat_log(q * base.rotation.conjugate());
                        *rot_acc.entry(channel.target_node).or_insert(Vector3::new(0.0, 0.0, 0.0)) +=
                            relative * anim.weight;
                    }
                    (AnimatedProperty::Scale, AnimationOutputs::Scales(_)) => {
                        let v = sample_vec3(
                            &channel.outputs, left, right, factor, interval, channel.interpolation,
                        );
                        *scale_acc.entry(channel.target_node).or_insert(Vector3::new(0.0, 0.0, 0.0)) +=
                            (v - base.scale) * anim.weight;
                    }
                    _ => {}
                }
            }
        }

        // 4. 将叠加结果写回目标节点
        for &node_idx in &target_nodes {
            if let Some(trans) = trans_acc.get(&node_idx) {
                self.nodes[node_idx].local_transform.translation =
                    self.nodes[node_idx].base_transform.translation + trans;
            }
            if let Some(rot) = rot_acc.get(&node_idx) {
                self.nodes[node_idx].local_transform.rotation =
                    (quat_exp(*rot) * self.nodes[node_idx].base_transform.rotation)
                        .normalize();
            }
            if let Some(scale) = scale_acc.get(&node_idx) {
                self.nodes[node_idx].local_transform.scale =
                    self.nodes[node_idx].base_transform.scale + scale;
            }
        }
    }

    /// 重建静态部分八叉树。仅当外部直接修改了静态对象的 aabb 时才需要调用;
    /// 动画驱动的动态对象不进 octree，因此动画更新无需调用本方法。
    pub fn rebuild_octree(&mut self) {
        self.octree = Octree::new(self.bounds, self.max_objects, self.max_depth);
        for &id in &self.static_indices {
            let obj = &self.objects[id];
            self.octree.insert(id, obj.aabb, obj.center);
        }
    }

    /// 递归深度上限，防止因节点树循环引用导致栈溢出。
    const MAX_NODE_DEPTH: usize = 4096;

    fn update_node_world(&mut self, node_id: usize, parent_world: cgmath::Matrix4<f32>, depth: usize) {
        use cgmath::{Matrix, SquareMatrix};

        if node_id >= self.nodes.len() {
            return;
        }

        if depth > Self::MAX_NODE_DEPTH {
            log::error!(
                "[Scene] update_node_world: depth {} exceeded max {} at node {}, possible cycle — aborting branch",
                depth, Self::MAX_NODE_DEPTH, node_id
            );
            return;
        }

        let local = self.nodes[node_id].local_transform.matrix();
        let world = parent_world * local;
        self.nodes[node_id].world_transform = world;

        let normal = world
            .invert()
            .map(|matrix| matrix.transpose())
            .unwrap_or_else(cgmath::Matrix4::identity);

        let object_count = self.nodes[node_id].objects.len();
        for oi in 0..object_count {
            let object_index = self.nodes[node_id].objects[oi];
            let local_aabb = self.objects[object_index].local_aabb;
            let world_aabb = transform_aabb(local_aabb, world);
            self.objects[object_index].aabb = world_aabb;
            self.objects[object_index].center = world_aabb.center();
            self.objects[object_index].model_matrix = world.into();
            self.objects[object_index].normal_matrix = normal.into();
            self.objects[object_index].joint_matrices =
                self.compute_joint_matrices(self.objects[object_index].mesh.skin);
        }

        let child_count = self.nodes[node_id].children.len();
        for ci in 0..child_count {
            let child = self.nodes[node_id].children[ci];
            // 跳过自引用，防止无限递归
            if child == node_id {
                continue;
            }
            self.update_node_world(child, world, depth + 1);
        }
    }

    fn compute_joint_matrices(&self, skin: Option<render::SkinHandle>) -> Vec<[[f32; 4]; 4]> {
        let Some(skin) = skin.and_then(|handle| self.skins.get(handle.0)) else {
            return Vec::new();
        };

        skin.joints
            .iter()
            .zip(skin.inverse_bind_matrices.iter())
            .map(|(joint_node, inverse_bind)| {
                let joint_world = self.nodes[*joint_node].world_transform;
                (joint_world * *inverse_bind).into()
            })
            .collect()
    }

    // -----------------------------------------------------------------------
    // 动态对象 CRUD
    // -----------------------------------------------------------------------

    /// 添加一个静态对象（进八叉树，适合不动的场景物）。
    /// 返回 entity_id。
    pub fn add_static_object(
        &mut self,
        mesh: render::ModelMesh,
        translation: cgmath::Vector3<f32>,
        rotation: cgmath::Quaternion<f32>,
        scale: cgmath::Vector3<f32>,
    ) -> String {
        let entity_id = self.add_object_internal(mesh, translation, rotation, scale, true);
        self.rebuild_octree();
        entity_id
    }

    /// 添加一个动态对象（线性检测，适合经常移动的对象）。
    pub fn add_dynamic_object(
        &mut self,
        mesh: render::ModelMesh,
        translation: cgmath::Vector3<f32>,
        rotation: cgmath::Quaternion<f32>,
        scale: cgmath::Vector3<f32>,
    ) -> String {
        let entity_id = self.add_object_internal(mesh, translation, rotation, scale, false);
        entity_id
    }

    /// 内部移除：不重建索引和八叉树（供批量删除复用）。
    fn remove_object_internal(&mut self, entity_id: &str) -> bool {
        let Some(obj_idx) = self.object_index.remove(entity_id) else { return false; };
        let node_idx = self.objects[obj_idx].node;

        self.objects.swap_remove(obj_idx);
        self.fix_moved_object_node(obj_idx, self.objects.len());

        self.nodes.swap_remove(node_idx);
        self.fix_moved_node_references(node_idx, self.nodes.len());

        true
    }

    /// 按 entity_id 移除对象。
    pub fn remove_object(&mut self, entity_id: &str) -> Result<(), String> {
        if !self.remove_object_internal(entity_id) {
            return Err(format!("object not found: {}", entity_id));
        }
        self.rebuild_object_indices();
        self.rebuild_octree();
        self.deleted_ids.push(entity_id.to_string());
        Ok(())
    }

    /// 更新动态对象的世界变换。
    pub fn update_object_transform(
        &mut self,
        entity_id: &str,
        translation: cgmath::Vector3<f32>,
        rotation: cgmath::Quaternion<f32>,
    ) -> Result<(), String> {
        let obj_idx = *self.object_index.get(entity_id)
            .ok_or_else(|| format!("object not found: {}", entity_id))?;
        let obj = &mut self.objects[obj_idx];
        obj.dirty |= DirtyFlags::TRANSFORM;
        let node_idx = obj.node;

        self.nodes[node_idx].local_transform.translation = translation;
        self.nodes[node_idx].local_transform.rotation = rotation;
        // 仅更新该节点及其子树
        self.update_node_world(node_idx, cgmath::Matrix4::identity(), 0);
        Ok(())
    }

    /// 批量添加静态对象（仅最后重建一次 octree）。
    pub fn add_static_objects_batch(
        &mut self,
        items: Vec<(render::ModelMesh, cgmath::Vector3<f32>, cgmath::Quaternion<f32>, cgmath::Vector3<f32>)>,
    ) -> Vec<String> {
        let ids: Vec<String> = items
            .into_iter()
            .map(|(mesh, t, r, s)| self.add_object_internal(mesh, t, r, s, true))
            .collect();
        self.rebuild_octree();
        ids
    }

    /// 批量移除对象（仅最后重建一次 octree）。
    pub fn remove_objects_batch(&mut self, entity_ids: &[&str]) -> Result<(), String> {
        let mut removed_any = false;
        for id in entity_ids {
            if self.remove_object_internal(id) {
                self.deleted_ids.push(id.to_string());
                removed_any = true;
            } else {
                return Err(format!("object not found: {}", id));
            }
        }
        if removed_any {
            self.rebuild_object_indices();
            self.rebuild_octree();
        }
        Ok(())
    }

    /// 收集本帧脏对象并清零脏标记。
    ///
    /// Returns: ``Vec<(entity_id, dirty_flags_bits)>``
    pub fn collect_dirty_objects(&mut self) -> Vec<(String, u8)> {
        self.objects
            .iter_mut()
            .filter(|o| !o.dirty.is_empty())
            .map(|o| {
                let bits = o.dirty.bits();
                o.dirty = DirtyFlags::empty();
                (o.entity_id.clone(), bits)
            })
            .collect()
    }

    /// 返回本帧已移除对象的 entity_id 列表（消费式取出）。
    pub fn drain_deleted_ids(&mut self) -> Vec<String> {
        std::mem::take(&mut self.deleted_ids)
    }

    /// 消费本帧所有已触发的动画标记事件。
    pub fn drain_marker_events(&mut self) -> Vec<MarkerEvent> {
        std::mem::take(&mut self.marker_events)
    }

    /// 消费本帧所有动画系统事件（状态切换、循环完成、动画结束）。
    pub fn drain_animation_events(&mut self) -> Vec<SceneAnimationEvent> {
        std::mem::take(&mut self.animation_events)
    }

    /// 统一帧更新：驱动动画、更新变换、生成事件。
    ///
    /// 综合调用 `update_animation_graph`、`update_character_animation` 和
    /// `update_physics`，并统一重建世界变换。
    ///
    /// 调用后通过 `drain_marker_events()` 和 `drain_animation_events()` 消费事件。
    pub fn tick(
        &mut self,
        graph: Option<&mut AnimationStateMachine>,
        dt: f32,
    ) {
        let has_graph = graph.is_some();

        // 1. 动画状态机更新
        if let Some(g) = graph {
            self.update_animation_graph(g, dt);
        }

        // 2. 更新世界变换（update_animation_graph 已调用，但无 graph 时需要显式调用）
        if !has_graph {
            self.update_world_transforms();
        }
    }

    // -----------------------------------------------------------------------
    // 内部辅助方法
    // -----------------------------------------------------------------------

    /// 创建一个孤立 SceneNode + SceneObject 并添加到场景。
    fn add_object_internal(
        &mut self,
        mesh: render::ModelMesh,
        translation: cgmath::Vector3<f32>,
        rotation: cgmath::Quaternion<f32>,
        scale: cgmath::Vector3<f32>,
        is_static: bool,
    ) -> String {
        use uuid::Uuid;

        let entity_id = Uuid::new_v4().to_string();
        let node_id = self.nodes.len();
        let object_index = self.objects.len();

        let transform = avatar::Transform {
            translation,
            rotation,
            scale,
        };
        let mut node = SceneNode::new(node_id, None, transform);
        node.objects = vec![object_index];
        self.nodes.push(node);

        let half = cgmath::Vector3::new(0.5, 0.5, 0.5);
        let local_aabb = AABB::new(
            cgmath::Point3::new(-half.x, -half.y, -half.z),
            cgmath::Point3::new(half.x, half.y, half.z),
        );
        let model_matrix = transform.matrix();

        self.objects.push(SceneObject {
            entity_id: entity_id.clone(),
            node: node_id,
            local_aabb,
            aabb: local_aabb,
            center: cgmath::Point3::new(0.0, 0.0, 0.0),
            mesh,
            model_matrix: model_matrix.into(),
            normal_matrix: model_matrix
                .invert()
                .unwrap_or(cgmath::Matrix4::identity())
                .transpose()
                .into(),
            joint_matrices: vec![],
            dirty: DirtyFlags::all(),
            prefab_source: None,
        });

        if is_static {
            self.static_indices.push(object_index);
        } else {
            self.dynamic_indices.push(object_index);
        }

        self.object_index.insert(entity_id.clone(), object_index);
        self.update_node_world(node_id, cgmath::Matrix4::identity(), 0);
        entity_id
    }

    /// swap_remove 后，被移动对象的 node 引用需要调整，并同步 object_index。
    fn fix_moved_object_node(&mut self, removed_idx: usize, old_len: usize) {
        if removed_idx < self.objects.len() {
            let moved_node = self.objects[removed_idx].node;
            // 更新 object_index：被交换元素的新索引
            let moved_eid = self.objects[removed_idx].entity_id.clone();
            self.object_index.insert(moved_eid, removed_idx);
            // 更新节点中对该对象的引用
            if moved_node < self.nodes.len() {
                // 节点原来引用 old_len-1，现在引用 removed_idx
                if let Some(pos) = self.nodes[moved_node].objects.iter().position(|&i| i == old_len) {
                    self.nodes[moved_node].objects[pos] = removed_idx;
                }
            }
        }
    }

    /// swap_remove 节点后，修复所有引用被移动节点的 children / parent 指针。
    ///
    /// `removed_node_idx` 是被删除的位置，`new_len` 是 swap_remove 后的 `self.nodes.len()`。
    /// 原来在 `new_len` 位置的节点被搬到了 `removed_node_idx`。
    fn fix_moved_node_references(&mut self, removed_node_idx: usize, new_len: usize) {
        if removed_node_idx >= self.nodes.len() {
            return;
        }
        let moved_from = new_len; // 被搬移的源位置（已不存在）

        // 1. 修复对象引用：node == new_len → removed_node_idx
        for obj in self.objects.iter_mut() {
            if obj.node == moved_from {
                obj.node = removed_node_idx;
            }
        }

        // 2. 修复 parent 指针：被搬移节点的子节点需要把 parent 从 new_len 改为 removed_node_idx
        let child_count = self.nodes[removed_node_idx].children.len();
        for ci in 0..child_count {
            let child_id = self.nodes[removed_node_idx].children[ci];
            if child_id < self.nodes.len() && self.nodes[child_id].parent == Some(moved_from) {
                self.nodes[child_id].parent = Some(removed_node_idx);
            }
        }

        // 3. 修复父节点的 children 数组中对 new_len 的引用
        if let Some(parent_id) = self.nodes[removed_node_idx].parent {
            if parent_id < self.nodes.len() {
                for child_ref in self.nodes[parent_id].children.iter_mut() {
                    if *child_ref == moved_from {
                        *child_ref = removed_node_idx;
                    }
                }
            }
        }

        // 4. 清除自引用（被搬移节点的 children 中引用了自身旧索引 new_len）
        for ci in 0..child_count {
            if self.nodes[removed_node_idx].children[ci] == moved_from {
                self.nodes[removed_node_idx].children[ci] = removed_node_idx;
            }
        }
        // 移除自引用项
        self.nodes[removed_node_idx].children.retain(|&c| c != removed_node_idx);
    }

    /// 重新构建 static_indices / dynamic_indices。
    pub(crate) fn rebuild_object_indices(&mut self) {
        (self.static_indices, self.dynamic_indices) =
            classify_objects(&self.nodes, &self.objects, &self.animations);
        self.build_object_index();
    }

    /// 重建 object_index HashMap。
    fn build_object_index(&mut self) {
        self.object_index.clear();
        for (i, obj) in self.objects.iter().enumerate() {
            self.object_index.insert(obj.entity_id.clone(), i);
        }
    }
}

/// 根据动画 channel 的 target_node 集合 + 其后代节点 + 拥有 skin 的对象，
/// 把对象划分为静态/动态两组。判断保守：宁可让对象动态化（性能略差但正确性安全）。
fn classify_objects(
    nodes: &[SceneNode],
    objects: &[SceneObject],
    animations: &[AnimationClip],
) -> (Vec<usize>, Vec<usize>) {
    let mut dynamic_nodes: HashSet<usize> = HashSet::new();
    let mut queue: VecDeque<usize> = VecDeque::new();

    for clip in animations {
        for channel in &clip.channels {
            if channel.target_node < nodes.len() && dynamic_nodes.insert(channel.target_node) {
                queue.push_back(channel.target_node);
            }
        }
    }

    while let Some(id) = queue.pop_front() {
        for &child in &nodes[id].children {
            if dynamic_nodes.insert(child) {
                queue.push_back(child);
            }
        }
    }

    let mut static_indices = Vec::new();
    let mut dynamic_indices = Vec::new();
    for (i, obj) in objects.iter().enumerate() {
        let is_dynamic = obj.mesh.skin.is_some() || dynamic_nodes.contains(&obj.node);
        if is_dynamic {
            dynamic_indices.push(i);
        } else {
            static_indices.push(i);
        }
    }

    (static_indices, dynamic_indices)
}

#[cfg(test)]
mod tests {
    use super::*;
    use avatar::{
        AnimatedProperty, AnimationChannel, AnimationOutputs, Interpolation, Transform,
    };
    use cgmath::{Matrix4, Point3, Vector3};
    use render::{ModelMesh, SkinHandle};

    fn dummy_obj(node: usize, has_skin: bool, center: Point3<f32>) -> SceneObject {
        let half = Vector3::new(0.1, 0.1, 0.1);
        let aabb = AABB::new(center - half, center + half);
        let mut mesh = ModelMesh::new();
        if has_skin {
            mesh.skin = Some(SkinHandle(0));
        }
        SceneObject {
            entity_id: String::new(),
            node,
            local_aabb: aabb,
            aabb,
            center,
            mesh,
            model_matrix: Matrix4::from_scale(1.0).into(),
            normal_matrix: Matrix4::from_scale(1.0).into(),
            joint_matrices: Vec::new(),
            dirty: DirtyFlags::all(),
            prefab_source: None,
        }
    }

    fn dummy_node(id: usize, parent: Option<usize>) -> SceneNode {
        SceneNode::new(id, parent, Transform::default())
    }

    #[test]
    fn classify_marks_animated_subtree_and_skinned_as_dynamic() {
        // 节点 0(root) → 1(child) → 2(grandchild);节点 3(独立)
        let mut nodes = vec![
            dummy_node(0, None),
            dummy_node(1, Some(0)),
            dummy_node(2, Some(1)),
            dummy_node(3, None),
        ];
        nodes[0].children.push(1);
        nodes[1].children.push(2);

        let objects = vec![
            dummy_obj(0, false, Point3::new(0.0, 0.0, 0.0)), // 0: 动画根 → 动态
            dummy_obj(1, false, Point3::new(0.0, 0.0, 0.0)), // 1: 动画子 → 动态
            dummy_obj(2, false, Point3::new(0.0, 0.0, 0.0)), // 2: 动画孙 → 动态
            dummy_obj(3, false, Point3::new(0.0, 0.0, 0.0)), // 3: 独立节点 → 静态
            dummy_obj(3, true, Point3::new(0.0, 0.0, 0.0)),  // 4: 独立但 has skin → 动态
        ];

        let animations = vec![AnimationClip {
            name: None,
            duration: 1.0,
            channels: vec![AnimationChannel {
                target_node: 0,
                property: AnimatedProperty::Translation,
                interpolation: Interpolation::Linear,
                inputs: vec![0.0, 1.0],
                outputs: AnimationOutputs::Translations(vec![
                    Vector3::new(0.0, 0.0, 0.0),
                    Vector3::new(1.0, 0.0, 0.0),
                ]),
            }],
            markers: vec![],
        }];

        let (statics, dynamics) = classify_objects(&nodes, &objects, &animations);
        assert_eq!(statics, vec![3], "only obj#3 (no anim, no skin) is static");
        let mut sorted_dyn = dynamics.clone();
        sorted_dyn.sort();
        assert_eq!(sorted_dyn, vec![0, 1, 2, 4]);
    }

    #[test]
    fn classify_handles_no_animation() {
        let nodes = vec![dummy_node(0, None), dummy_node(1, None)];
        let objects = vec![
            dummy_obj(0, false, Point3::new(0.0, 0.0, 0.0)),
            dummy_obj(1, true, Point3::new(0.0, 0.0, 0.0)),
        ];
        let (statics, dynamics) = classify_objects(&nodes, &objects, &[]);
        assert_eq!(statics, vec![0]);
        assert_eq!(dynamics, vec![1], "skinned object is always dynamic");
    }

    // -----------------------------------------------------------------------
    // Scene integration tests
    // -----------------------------------------------------------------------

    fn empty_materials() -> MaterialLibrary {
        MaterialLibrary {
            materials: Vec::new(),
            textures: Vec::new(),
        }
    }

    fn make_scene(nodes: Vec<SceneNode>, objects: Vec<SceneObject>) -> Scene {
        let bounds = AABB::new(
            Point3::new(-100.0, -100.0, -100.0),
            Point3::new(100.0, 100.0, 100.0),
        );
        Scene::new(nodes, objects, empty_materials(), vec![], vec![], bounds, 10, 8)
    }

    #[test]
    fn test_scene_create_empty() {
        let scene = make_scene(vec![], vec![]);
        assert!(scene.nodes.is_empty());
        assert!(scene.objects().is_empty());
    }

    #[test]
    fn test_scene_node_hierarchy() {
        let mut nodes = vec![
            dummy_node(0, None),
            dummy_node(1, Some(0)),
            dummy_node(2, Some(1)),
        ];
        nodes[0].children.push(1);
        nodes[1].children.push(2);

        let scene = make_scene(nodes, vec![]);

        // 验证父子关系
        assert_eq!(scene.nodes[0].parent, None);
        assert_eq!(scene.nodes[1].parent, Some(0));
        assert_eq!(scene.nodes[2].parent, Some(1));
        assert_eq!(scene.nodes[0].children, vec![1]);
        assert_eq!(scene.nodes[1].children, vec![2]);
        assert!(scene.nodes[2].children.is_empty());
    }

    #[test]
    fn test_scene_world_transform_propagation() {
        use cgmath::Quaternion;
        // 父节点平移 (1,0,0)，子节点本地平移 (0,2,0)
        let parent_transform = Transform {
            translation: Vector3::new(1.0, 0.0, 0.0),
            rotation: Quaternion::new(1.0, 0.0, 0.0, 0.0),
            scale: Vector3::new(1.0, 1.0, 1.0),
        };
        let child_transform = Transform {
            translation: Vector3::new(0.0, 2.0, 0.0),
            rotation: Quaternion::new(1.0, 0.0, 0.0, 0.0),
            scale: Vector3::new(1.0, 1.0, 1.0),
        };
        let mut parent = SceneNode::new(0, None, parent_transform);
        let child = SceneNode::new(1, Some(0), child_transform);
        parent.children.push(1);

        let scene = make_scene(vec![parent, child], vec![]);

        // 父节点世界变换应该是平移 (1,0,0)
        let parent_world = scene.nodes[0].world_transform;
        let parent_trans = Vector3::new(parent_world[3][0], parent_world[3][1], parent_world[3][2]);
        assert!((parent_trans.x - 1.0).abs() < 1e-5);
        assert!((parent_trans.y - 0.0).abs() < 1e-5);

        // 子节点世界变换应该是 (1,2,0)——父变换叠加
        let child_world = scene.nodes[1].world_transform;
        let child_trans = Vector3::new(child_world[3][0], child_world[3][1], child_world[3][2]);
        assert!((child_trans.x - 1.0).abs() < 1e-5, "child world x should be 1.0, got {}", child_trans.x);
        assert!((child_trans.y - 2.0).abs() < 1e-5, "child world y should be 2.0, got {}", child_trans.y);
        assert!((child_trans.z - 0.0).abs() < 1e-5);
    }

    #[test]
    fn test_scene_deep_hierarchy() {
        // 100+ 层级的节点树不应栈溢出
        let depth = 200;
        let mut nodes: Vec<SceneNode> = Vec::with_capacity(depth);
        for i in 0..depth {
            let parent = if i == 0 { None } else { Some(i - 1) };
            let t = Transform {
                translation: Vector3::new(1.0, 0.0, 0.0),
                ..Transform::default()
            };
            nodes.push(SceneNode::new(i, parent, t));
        }
        // 设置 children
        for i in 1..depth {
            nodes[i - 1].children.push(i);
        }

        let scene = make_scene(nodes, vec![]);

        // 最深节点的世界变换应正确累积
        let last = &scene.nodes[depth - 1];
        let trans_x = last.world_transform[3][0];
        assert!(
            (trans_x - depth as f32).abs() < 1e-2,
            "expected x ~{}, got {}",
            depth as f32,
            trans_x
        );
    }

    #[test]
    fn test_scene_add_and_remove_dynamic_object() {
        let scene_nodes = vec![dummy_node(0, None)];
        let mut scene = make_scene(scene_nodes, vec![]);
        assert!(scene.objects().is_empty());

        let mesh = ModelMesh::new();
        let id = scene.add_dynamic_object(
            mesh,
            Vector3::new(0.0, 0.0, 0.0),
            cgmath::Quaternion::new(1.0, 0.0, 0.0, 0.0),
            Vector3::new(1.0, 1.0, 1.0),
        );

        assert_eq!(scene.objects().len(), 1);
        assert_eq!(scene.dynamic_indices().len(), 1);

        scene.remove_object(&id).unwrap();
        assert!(scene.objects().is_empty());
        assert!(scene.dynamic_indices().is_empty());
    }

    #[test]
    fn test_scene_remove_nonexistent_object() {
        let mut scene = make_scene(vec![dummy_node(0, None)], vec![]);
        let result = scene.remove_object("nonexistent");
        assert!(result.is_err());
    }

    #[test]
    fn test_scene_drain_deleted_ids() {
        let mut scene = make_scene(vec![dummy_node(0, None)], vec![]);
        let mesh = ModelMesh::new();
        let id = scene.add_dynamic_object(
            mesh,
            Vector3::new(0.0, 0.0, 0.0),
            cgmath::Quaternion::new(1.0, 0.0, 0.0, 0.0),
            Vector3::new(1.0, 1.0, 1.0),
        );
        scene.remove_object(&id).unwrap();

        let deleted = scene.drain_deleted_ids();
        assert_eq!(deleted.len(), 1);
        assert_eq!(deleted[0], id);

        // 再次 drain 应为空
        let deleted2 = scene.drain_deleted_ids();
        assert!(deleted2.is_empty());
    }

    #[test]
    fn test_scene_batch_add_and_remove() {
        let mut scene = make_scene(vec![dummy_node(0, None)], vec![]);

        let items: Vec<_> = (0..5)
            .map(|i| {
                (
                    ModelMesh::new(),
                    Vector3::new(i as f32, 0.0, 0.0),
                    cgmath::Quaternion::new(1.0, 0.0, 0.0, 0.0),
                    Vector3::new(1.0, 1.0, 1.0),
                )
            })
            .collect();

        let ids = scene.add_static_objects_batch(items);
        assert_eq!(ids.len(), 5);
        assert_eq!(scene.objects().len(), 5);

        let refs: Vec<&str> = ids.iter().map(|s| s.as_str()).collect();
        scene.remove_objects_batch(&refs).unwrap();
        assert!(scene.objects().is_empty());
    }

    #[test]
    fn test_scene_update_object_transform() {
        let mut scene = make_scene(vec![dummy_node(0, None)], vec![]);
        let mesh = ModelMesh::new();
        let id = scene.add_dynamic_object(
            mesh,
            Vector3::new(0.0, 0.0, 0.0),
            cgmath::Quaternion::new(1.0, 0.0, 0.0, 0.0),
            Vector3::new(1.0, 1.0, 1.0),
        );

        scene
            .update_object_transform(
                &id,
                Vector3::new(5.0, 3.0, 1.0),
                cgmath::Quaternion::new(1.0, 0.0, 0.0, 0.0),
            )
            .unwrap();

        let obj = &scene.objects[0];
        assert!((obj.aabb.center().x - 5.0).abs() < 1e-5);
        assert!((obj.aabb.center().y - 3.0).abs() < 1e-5);
    }

    #[test]
    fn test_scene_collect_dirty_objects() {
        let mut scene = make_scene(vec![dummy_node(0, None)], vec![]);
        let mesh = ModelMesh::new();
        let id = scene.add_dynamic_object(
            mesh,
            Vector3::new(0.0, 0.0, 0.0),
            cgmath::Quaternion::new(1.0, 0.0, 0.0, 0.0),
            Vector3::new(1.0, 1.0, 1.0),
        );

        // 新对象带 DirtyFlags::all()
        let dirty = scene.collect_dirty_objects();
        assert_eq!(dirty.len(), 1);
        assert_eq!(dirty[0].0, id);

        // 收集后脏标记应已清除
        let dirty2 = scene.collect_dirty_objects();
        assert!(dirty2.is_empty());
    }
}

fn transform_aabb(aabb: AABB, matrix: cgmath::Matrix4<f32>) -> AABB {
    use cgmath::Transform;

    let corners = [
        cgmath::Point3::new(aabb.min.x, aabb.min.y, aabb.min.z),
        cgmath::Point3::new(aabb.min.x, aabb.min.y, aabb.max.z),
        cgmath::Point3::new(aabb.min.x, aabb.max.y, aabb.min.z),
        cgmath::Point3::new(aabb.min.x, aabb.max.y, aabb.max.z),
        cgmath::Point3::new(aabb.max.x, aabb.min.y, aabb.min.z),
        cgmath::Point3::new(aabb.max.x, aabb.min.y, aabb.max.z),
        cgmath::Point3::new(aabb.max.x, aabb.max.y, aabb.min.z),
        cgmath::Point3::new(aabb.max.x, aabb.max.y, aabb.max.z),
    ];

    let mut min = matrix.transform_point(corners[0]);
    let mut max = min;
    for corner in corners.iter().skip(1) {
        let point = matrix.transform_point(*corner);
        min.x = min.x.min(point.x);
        min.y = min.y.min(point.y);
        min.z = min.z.min(point.z);
        max.x = max.x.max(point.x);
        max.y = max.y.max(point.y);
        max.z = max.z.max(point.z);
    }

    AABB { min, max }
}
