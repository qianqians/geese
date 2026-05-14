use cgmath::{Point3/* , Matrix4, Vector3, InnerSpace, EuclideanSpace, Rad, Deg, PerspectiveFov */};
use std::fmt::Debug;
use math::AABB;

// 场景对象 trait
pub trait SceneObject: Debug {
    fn entity_id(&self) -> String;
    fn aabb(&self) -> AABB;
    fn center(&self) -> Point3<f32>;
}
