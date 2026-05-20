use cgmath::{Point3/* , Matrix4, Vector3, InnerSpace, EuclideanSpace, Rad, Deg, PerspectiveFov */};
use math::AABB;

// 场景对象 trait
pub struct SceneObject {
    pub entity_id: String,
    pub aabb: AABB,
    pub center: Point3<f32>,
}
