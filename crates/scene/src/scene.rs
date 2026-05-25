use camera::frustum::Frustum;
use math::AABB;
use render::MaterialLibrary;
use render::{RenderQueue, SceneRenderer};

use crate::animation::{AnimationClip, AnimationPlayer, SceneNode, sample_clip};
use crate::{Octree, SceneObject};

pub struct Scene {
    pub nodes: Vec<SceneNode>,
    pub objects: Vec<SceneObject>,
    pub octree: Octree,
    pub materials: MaterialLibrary,
    pub animations: Vec<AnimationClip>,
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
        bounds: AABB,
        max_objects: usize,
        max_depth: usize,
    ) -> Self {
        let mut scene = Self {
            nodes,
            objects,
            octree: Octree::new(bounds, max_objects, max_depth),
            materials,
            animations,
            bounds,
            max_objects,
            max_depth,
        };
        scene.update_world_transforms();
        scene.rebuild_octree();
        scene
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
        }

        let children = self.nodes[node_id].children.clone();
        for child in children {
            self.update_node_world(child, world);
        }
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
