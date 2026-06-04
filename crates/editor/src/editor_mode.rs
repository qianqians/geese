//! 编辑器模式状态机。
//!
//! 定义编辑器的三种运行模式：
//! - Edit：编辑模式，轨道摄像机，面板可交互
//! - Play：播放模式，运行时摄像机，面板透明
//! - Pause：暂停模式，保持 Play 状态但恢复编辑器交互

/// 编辑器运行模式。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorMode {
    /// 编辑模式：轨道摄像机、Gizmo、面板全交互
    Edit,
    /// 播放模式：运行时摄像机、面板透明、场景运行
    Play,
    /// 暂停模式：Play 中的暂停，可检查场景状态
    Pause,
}

impl EditorMode {
    /// 是否处于编辑模式。
    pub fn is_editing(&self) -> bool {
        matches!(self, Self::Edit)
    }

    /// 是否处于播放模式（包括暂停）。
    pub fn is_playing(&self) -> bool {
        matches!(self, Self::Play | Self::Pause)
    }

    /// 是否可以与场景交互（选择实体、Gizmo 等）。
    /// Edit 和 Pause 模式下可交互。
    pub fn can_interact_with_scene(&self) -> bool {
        matches!(self, Self::Edit | Self::Pause)
    }
}
