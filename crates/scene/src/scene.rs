use std::collections::{HashMap, HashSet, VecDeque};

use camera::frustum::Frustum;
use cgmath::InnerSpace;
use cgmath::{Matrix, SquareMatrix};
use math::AABB;
use render::MaterialLibrary;
use render::{RenderQueue, SceneRenderer};

use avatar::{
    AnimatedProperty, AnimationClip, AnimationOutputs, AnimationPlayer, SceneNode, Skin,
    quat_dot, quat_exp, quat_log, sample_clip, sample_indices, sample_quat, sample_vec3,
};
use avatar::AnimationStateMachine;
use crate::character_animation::CharacterAnimationGraph;
use crate::character_physics::CharacterPhysics;
use crate::{Octree, SceneObject};
use crate::scene_object::DirtyFlags;
use physics::scene::PhysicsScene;

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
    pub character_physics: Vec<CharacterPhysics>,
    /// 物理开关；关闭时跳过物理步进和动画混合
    pub physics_enabled: bool,
    /// 角色动画蓝图列表（Phase 4 动画混合）
    pub character_anim_graphs: Vec<CharacterAnimationGraph>,
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
            character_physics: Vec::new(),
            physics_enabled: true,
            character_anim_graphs: Vec::new(),
        };
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

    /// 取场景全部对象引用。
    pub fn objects(&self) -> Vec<&SceneObject> {
        self.objects.iter().collect()
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

        player.advance(dt, clip.duration);
        sample_clip(clip, player.time, &mut self.nodes);
        self.update_world_transforms();
        // 静态 octree 不需要每帧重建——动态对象的 aabb 已在 update_world_transforms 中更新，
        // visible_objects 通过 dynamic_indices 线性测试覆盖它们。
    }

    pub fn update_animation_graph(&mut self, graph: &mut AnimationStateMachine, dt: f32) {
        use cgmath::Vector3;

        let active = graph.update(dt, &self.animations);

        if active.len() == 1 && (active[0].weight - 1.0).abs() < f32::EPSILON {
            if let Some(clip) = self.animations.get(active[0].clip) {
                sample_clip(clip, active[0].time, &mut self.nodes);
            }
        } else if !active.is_empty() {
            for node in self.nodes.iter_mut() {
                node.local_transform = node.base_transform;
            }

            let mut trans_acc = vec![Vector3::new(0.0, 0.0, 0.0); self.nodes.len()];
            let mut rot_acc = vec![Vector3::new(0.0, 0.0, 0.0); self.nodes.len()];
            let mut scale_acc = vec![Vector3::new(0.0, 0.0, 0.0); self.nodes.len()];
            let mut has_trans = vec![false; self.nodes.len()];
            let mut has_rot = vec![false; self.nodes.len()];
            let mut has_scale = vec![false; self.nodes.len()];

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
                            AnimationOutputs::Translations(values),
                        ) => {
                            let v = sample_vec3(
                                values, left, right, factor, interval, channel.interpolation,
                            );
                            trans_acc[channel.target_node] += (v - base.translation) * anim.weight;
                            has_trans[channel.target_node] = true;
                        }
                        (AnimatedProperty::Rotation, AnimationOutputs::Rotations(values)) => {
                            let q = sample_quat(
                                values, left, right, factor, interval, channel.interpolation,
                            );
                            let q = if quat_dot(q, base.rotation) < 0.0 {
                                -q
                            } else {
                                q
                            };
                            let relative = quat_log(q * base.rotation.conjugate());
                            rot_acc[channel.target_node] += relative * anim.weight;
                            has_rot[channel.target_node] = true;
                        }
                        (AnimatedProperty::Scale, AnimationOutputs::Scales(values)) => {
                            let v = sample_vec3(
                                values, left, right, factor, interval, channel.interpolation,
                            );
                            scale_acc[channel.target_node] += (v - base.scale) * anim.weight;
                            has_scale[channel.target_node] = true;
                        }
                        _ => {}
                    }
                }
            }

            for i in 0..self.nodes.len() {
                if has_trans[i] {
                    self.nodes[i].local_transform.translation =
                        self.nodes[i].base_transform.translation + trans_acc[i];
                }
                if has_rot[i] {
                    self.nodes[i].local_transform.rotation =
                        (quat_exp(rot_acc[i]) * self.nodes[i].base_transform.rotation)
                            .normalize();
                }
                if has_scale[i] {
                    self.nodes[i].local_transform.scale =
                        self.nodes[i].base_transform.scale + scale_acc[i];
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
            self.update_node_world(root, cgmath::Matrix4::from_scale(1.0));
        }
    }

    /// 从物理场景读取刚体变换，更新关联节点的 `local_transform`，
    /// 然后执行 `update_world_transforms()` 传播到渲染对象。
    ///
    /// 仅在 `physics_enabled` 为 true 时生效。
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
    /// 遍历所有 `character_anim_graphs`，对每个角色从物理场景
    /// 读取当前线速度，计算水平速度大小和着地状态，
    /// 更新动画状态机后采样动画剪辑到节点。
    pub fn update_character_animation(
        &mut self,
        _physics_scene: &PhysicsScene,
        velocities: &[f32],
        grounded_flags: &[bool],
        dt: f32,
    ) {
        if !self.physics_enabled {
            return;
        }
        let count = self.character_anim_graphs.len().min(velocities.len()).min(grounded_flags.len());
        for i in 0..count {
            let graph = &mut self.character_anim_graphs[i];
            let active = graph.update(velocities[i], grounded_flags[i], dt, &self.animations);
            // 将活跃动画采样到节点
            self.apply_active_animations(&active);
        }
    }

    /// 将活跃动画列表采样到场景节点。
    fn apply_active_animations(&mut self, active: &[avatar::ActiveAnimation]) {
        use cgmath::Vector3;

        if active.is_empty() {
            return;
        }
        if active.len() == 1 && (active[0].weight - 1.0).abs() < f32::EPSILON {
            if let Some(clip) = self.animations.get(active[0].clip) {
                sample_clip(clip, active[0].time, &mut self.nodes);
            }
        } else {
            for node in self.nodes.iter_mut() {
                node.local_transform = node.base_transform;
            }

            let mut trans_acc = vec![Vector3::new(0.0, 0.0, 0.0); self.nodes.len()];
            let mut rot_acc = vec![Vector3::new(0.0, 0.0, 0.0); self.nodes.len()];
            let mut scale_acc = vec![Vector3::new(0.0, 0.0, 0.0); self.nodes.len()];
            let mut has_trans = vec![false; self.nodes.len()];
            let mut has_rot = vec![false; self.nodes.len()];
            let mut has_scale = vec![false; self.nodes.len()];

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
                            AnimationOutputs::Translations(values),
                        ) => {
                            let v = sample_vec3(
                                values, left, right, factor, interval, channel.interpolation,
                            );
                            trans_acc[channel.target_node] += (v - base.translation) * anim.weight;
                            has_trans[channel.target_node] = true;
                        }
                        (AnimatedProperty::Rotation, AnimationOutputs::Rotations(values)) => {
                            let q = sample_quat(
                                values, left, right, factor, interval, channel.interpolation,
                            );
                            let q = if quat_dot(q, base.rotation) < 0.0 {
                                -q
                            } else {
                                q
                            };
                            let relative = quat_log(q * base.rotation.conjugate());
                            rot_acc[channel.target_node] += relative * anim.weight;
                            has_rot[channel.target_node] = true;
                        }
                        (AnimatedProperty::Scale, AnimationOutputs::Scales(values)) => {
                            let v = sample_vec3(
                                values, left, right, factor, interval, channel.interpolation,
                            );
                            scale_acc[channel.target_node] += (v - base.scale) * anim.weight;
                            has_scale[channel.target_node] = true;
                        }
                        _ => {}
                    }
                }
            }

            for i in 0..self.nodes.len() {
                if has_trans[i] {
                    self.nodes[i].local_transform.translation =
                        self.nodes[i].base_transform.translation + trans_acc[i];
                }
                if has_rot[i] {
                    self.nodes[i].local_transform.rotation =
                        (quat_exp(rot_acc[i]) * self.nodes[i].base_transform.rotation)
                            .normalize();
                }
                if has_scale[i] {
                    self.nodes[i].local_transform.scale =
                        self.nodes[i].base_transform.scale + scale_acc[i];
                }
            }
        }

        self.update_world_transforms();
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

    fn update_node_world(&mut self, node_id: usize, parent_world: cgmath::Matrix4<f32>) {
        use cgmath::{Matrix, SquareMatrix};

        let local = self.nodes[node_id].local_transform.matrix();
        let world = parent_world * local;
        self.nodes[node_id].world_transform = world;

        let normal = world
            .invert()
            .map(|matrix| matrix.transpose())
            .unwrap_or_else(cgmath::Matrix4::identity);

        let object_indices = self.nodes[node_id].objects.clone();
        for object_index in object_indices {
            let local_aabb = self.objects[object_index].local_aabb;
            let world_aabb = transform_aabb(local_aabb, world);
            self.objects[object_index].aabb = world_aabb;
            self.objects[object_index].center = world_aabb.center();
            self.objects[object_index].model_matrix = world.into();
            self.objects[object_index].normal_matrix = normal.into();
            self.objects[object_index].joint_matrices =
                self.compute_joint_matrices(self.objects[object_index].mesh.skin);
        }

        let children = self.nodes[node_id].children.clone();
        for child in children {
            self.update_node_world(child, world);
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
        let Some(obj_idx) = self
            .objects
            .iter()
            .position(|o| o.entity_id == entity_id)
            else { return false; };
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
        let obj = self
            .objects
            .iter_mut()
            .find(|o| o.entity_id == entity_id)
            .ok_or_else(|| format!("object not found: {}", entity_id))?;
        obj.dirty |= DirtyFlags::TRANSFORM;
        let node_idx = obj.node;

        self.nodes[node_idx].local_transform.translation = translation;
        self.nodes[node_idx].local_transform.rotation = rotation;
        // 仅更新该节点及其子树
        self.update_node_world(node_idx, cgmath::Matrix4::identity());
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

        self.update_node_world(node_id, cgmath::Matrix4::identity());
        entity_id
    }

    /// swap_remove 后，被移动对象的 node 引用需要调整。
    fn fix_moved_object_node(&mut self, removed_idx: usize, old_len: usize) {
        if removed_idx < self.objects.len() {
            let moved_node = self.objects[removed_idx].node;
            // 更新节点中对该对象的引用
            if moved_node < self.nodes.len() {
                // 节点原来引用 old_len-1，现在引用 removed_idx
                if let Some(pos) = self.nodes[moved_node].objects.iter().position(|&i| i == old_len) {
                    self.nodes[moved_node].objects[pos] = removed_idx;
                }
            }
        }
    }

    /// swap_remove 节点后，所有引用被移动节点的对象需修正 node 字段。
    fn fix_moved_node_references(&mut self, removed_node_idx: usize, new_len: usize) {
        if removed_node_idx < self.nodes.len() {
            for obj in self.objects.iter_mut() {
                if obj.node == new_len {
                    obj.node = removed_node_idx;
                }
            }
        }
    }

    /// 重新构建 static_indices / dynamic_indices。
    pub(crate) fn rebuild_object_indices(&mut self) {
        (self.static_indices, self.dynamic_indices) =
            classify_objects(&self.nodes, &self.objects, &self.animations);
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
