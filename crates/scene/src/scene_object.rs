use cgmath::{
    Point3, /* , Matrix4, Vector3, InnerSpace, EuclideanSpace, Rad, Deg, PerspectiveFov */
};
use math::AABB;
use render::{ModelMesh, RenderObject};

// 场景对象 trait
#[derive(Clone)]
pub struct SceneObject {
    pub entity_id: String,
    pub local_aabb: AABB,
    pub aabb: AABB,
    pub center: Point3<f32>,
    pub mesh: ModelMesh,
    pub model_matrix: [[f32; 4]; 4],
    pub normal_matrix: [[f32; 4]; 4],
}

impl RenderObject for SceneObject {
    fn entity_id(&self) -> &str {
        &self.entity_id
    }

    fn mesh(&self) -> &ModelMesh {
        &self.mesh
    }

    fn model_matrix(&self) -> [[f32; 4]; 4] {
        self.model_matrix
    }

    fn normal_matrix(&self) -> [[f32; 4]; 4] {
        self.normal_matrix
    }
}
