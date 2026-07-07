//! 游戏玩法物理模块。
//!
//! 提供胶囊体角色控制器、角色物理桥接映射和全骨骼刚体系统（Ragdoll）。
//! 客户端和服务器均可使用。

pub mod capsule_controller;
pub mod character_physics;
pub mod ragdoll;

pub use capsule_controller::CapsuleController;
pub use character_physics::{CharacterControllerType, CharacterPhysics};
pub use ragdoll::{JointTypeStrategy, RagdollBuilder, RagdollConfig, RagdollInstance};
