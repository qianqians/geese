use cgmath::{
    Point3, /* , Matrix4, Vector3, InnerSpace, EuclideanSpace, Rad, Deg, PerspectiveFov */
};
use math::AABB;
use render::{ModelMesh, RenderObject};

// 场景对象 trait
pub struct SceneObject {
    pub entity_id: String,
    pub aabb: AABB,
    pub center: Point3<f32>,
    pub mesh: ModelMesh,
}

impl RenderObject for SceneObject {
    fn entity_id(&self) -> &str {
        &self.entity_id
    }

    fn mesh(&self) -> &ModelMesh {
        &self.mesh
    }
}
