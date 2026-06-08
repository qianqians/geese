//! Geese 引擎编辑器。
//!
//! 提供一体化场景编辑器 [`Editor`]。
//!
//! 与 [`launcher`] crate 配合使用：
//! 启动 Launcher → 选择模板 → 生成工程 → 打开 Editor。

pub mod asset_browser;
pub mod commands;
pub mod editor;
pub mod editor_mode;
pub mod gizmo;
pub mod gltf_import_dialog;
pub mod hierarchy;
pub mod inspector;
pub mod panel_layer;
pub mod panels;
pub mod play_mode;
pub mod viewport;

pub use commands::{CommandHistory, SceneSerializer, SerializedEntity};
pub use editor::Editor;
pub use editor_mode::EditorMode;
pub use panel_layer::{PanelLayer, PanelLayerManager};
pub use play_mode::PlayMode;
pub use viewport::{GizmoMode, OrbitCamera, ViewportPanel, ray_aabb_intersection};
