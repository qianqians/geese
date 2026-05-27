use std::collections::HashMap;

use camera::frustum::Frustum;
use cgmath::InnerSpace;
use math::AABB;
use render::MaterialLibrary;
use render::{RenderQueue, SceneRenderer};

use avatar::{
    AnimatedProperty, AnimationClip, AnimationOutputs, AnimationPlayer, SceneNode, Skin,
    quat_dot, quat_exp, quat_log, sample_clip, sample_indices, sample_quat, sample_vec3,
};
use avatar::AnimationStateMachine;
use crate::{Octree, SceneObject};

pub struct Scene {
    pub nodes: Vec<SceneNode>,
    pub objects: Vec<SceneObject>,
    pub octree: Octree,
    pub materials: MaterialLibrary,
    pub animations: Vec<AnimationClip>,
    pub skins: Vec<Skin>,
    animation_names: HashMap<String, usize>,
    bounds: AABB,
    max_objects: usize,
    max_depth: usize,
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
        let mut scene = Self {
            nodes,
            objects,
            octree: Octree::new(bounds, max_objects, max_depth),
            materials,
            animations,
            skins,
            animation_names,
            bounds,
            max_objects,
            max_depth,
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

    pub fn objects(&self) -> Vec<&SceneObject> {
        self.octree.objects()
    }

    pub fn visible_objects(&self, frustum: &Frustum) -> Vec<&SceneObject> {
        self.octree.query_frustum(frustum)
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
        self.rebuild_octree();
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
        self.rebuild_octree();
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

    pub fn rebuild_octree(&mut self) {
        let mut octree = Octree::new(self.bounds, self.max_objects, self.max_depth);
        for object in &self.objects {
            octree.insert(object.clone());
        }
        self.octree = octree;
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
