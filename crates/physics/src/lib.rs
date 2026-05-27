//! Physics crate: rapier3d 的多场景物理世界封装。
//!
//! 不对外暴露 nalgebra 类型；外部只接触本 crate 的 `Vec3` / `Quat` 别名与
//! 元组形式（pyo3 feature 下）。

pub mod handles;
pub mod math;
pub mod scene;
pub mod shapes;
pub mod world;

pub use handles::{BodyHandle, ColliderHandle, SceneId};
pub use math::{Iso3, Quat, Vec3};
pub use scene::{CollisionEvent, ContactForceEvent, PhysicsScene, RayHit};
pub use shapes::ShapeDesc;
pub use world::{BodyDesc, BodyKind, PhysicsWorld};

#[cfg(feature = "pyo3")]
pub mod py;
