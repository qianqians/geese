//! 网格 Cooking — meshopt 顶点/索引优化。
//!
//! Feature gate: `cooking`（默认禁用）。
//!
//! 当前为 stub 实现。完整实现需要集成 `meshopt` crate。

/// Re-export from texture_cooker for mesh cooking.
pub use super::texture_cooker::{MeshCookConfig, MeshCooker};
