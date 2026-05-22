use camera::frustum::Frustum;
use render::MaterialLibrary;
use render::{RenderQueue, SceneRenderer};

use crate::{Octree, SceneObject};

pub struct Scene {
    pub octree: Octree,
    pub materials: MaterialLibrary,
}

impl Scene {
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
}
