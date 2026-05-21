use render::MaterialLibrary;

use crate::Octree;

pub struct Scene {
    pub octree: Octree,
    pub materials: MaterialLibrary,
}
