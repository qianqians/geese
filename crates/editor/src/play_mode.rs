//! Play/Stop 播放测试模式。
//!
//! 管理编辑器和运行时模式之间的切换：
//! - Play：捕获场景状态，启用物理和输入
//! - Stop：恢复编辑模式场景状态
//!
//! 工具栏按钮状态指示器：红=播放中/灰=编辑中

use crate::panel_layer::PanelLayer;
use crate::panels::EditorState;

// ---------------------------------------------------------------------------
// PlayMode - 播放模式管理
// ---------------------------------------------------------------------------

/// 播放模式管理器。
pub struct PlayMode {
    /// 是否处于播放模式
    pub is_playing: bool,
    /// 播放前的编辑状态快照
    snapshot: Option<PlayModeSnapshot>,
    /// 播放开始时间（单调时钟）
    pub play_start_time: Option<std::time::Instant>,
    /// 播放耗时（秒）
    pub elapsed: f64,
}

/// 编辑状态快照，Play 时保存、Stop 时恢复。
#[derive(Debug, Clone)]
pub struct PlayModeSnapshot {
    /// 选中的实体
    pub selected_entity: Option<String>,
    /// 摄像机状态（用于恢复视角）
    pub camera_yaw: f32,
    pub camera_pitch: f32,
    pub camera_distance: f32,
    /// 面板状态快照
    pub panel_state: PanelStateSnapshot,
}

/// 面板状态快照，Play 时保存 UI 状态、Stop 时恢复。
#[derive(Debug, Clone)]
pub struct PanelStateSnapshot {
    pub global_alpha: f32,
    pub ui_visible: bool,
    pub hierarchy_visible: bool,
    pub inspector_visible: bool,
    pub asset_browser_visible: bool,
}

impl PlayMode {
    pub fn new() -> Self {
        Self {
            is_playing: false,
            snapshot: None,
            play_start_time: None,
            elapsed: 0.0,
        }
    }

    /// 进入播放模式。
    ///
    /// 保存当前编辑状态快照，之后可以调用 `stop()` 恢复。
    pub fn play(&mut self, state: &EditorState, camera_yaw: f32, camera_pitch: f32, camera_distance: f32) {
        if self.is_playing {
            return;
        }

        self.snapshot = Some(PlayModeSnapshot {
            selected_entity: state.selected_entity.clone(),
            camera_yaw,
            camera_pitch,
            camera_distance,
            panel_state: PanelStateSnapshot {
                global_alpha: state.panel_layer.global_alpha,
                ui_visible: state.ui_visible,
                hierarchy_visible: state.panel_layer.is_visible(&PanelLayer::Hierarchy),
                inspector_visible: state.panel_layer.is_visible(&PanelLayer::Inspector),
                asset_browser_visible: state.panel_layer.is_visible(&PanelLayer::AssetBrowser),
            },
        });

        self.is_playing = true;
        self.play_start_time = Some(std::time::Instant::now());
        self.elapsed = 0.0;
    }

    /// 退出播放模式。
    ///
    /// 返回之前保存的快照，调用方可用于恢复编辑状态。
    pub fn stop(&mut self) -> Option<PlayModeSnapshot> {
        if !self.is_playing {
            return None;
        }

        self.is_playing = false;
        self.play_start_time = None;
        self.elapsed = 0.0;

        self.snapshot.take()
    }

    /// 从快照恢复面板状态到 EditorState。
    pub fn restore_panel_state(snapshot: &PlayModeSnapshot, state: &mut EditorState) {
        state.panel_layer.global_alpha = snapshot.panel_state.global_alpha;
        state.ui_visible = snapshot.panel_state.ui_visible;
        state.panel_layer.set_visible(PanelLayer::Hierarchy, snapshot.panel_state.hierarchy_visible);
        state.panel_layer.set_visible(PanelLayer::Inspector, snapshot.panel_state.inspector_visible);
        state.panel_layer.set_visible(PanelLayer::AssetBrowser, snapshot.panel_state.asset_browser_visible);
    }

    /// 更新播放模式时间。
    pub fn update(&mut self) {
        if let Some(start) = self.play_start_time {
            self.elapsed = start.elapsed().as_secs_f64();
        }
    }

    /// 工具栏 Play/Stop 按钮的显示文本和颜色。
    pub fn button_ui(&self) -> (&'static str, egui::Color32) {
        if self.is_playing {
            ("⏹ Stop", egui::Color32::RED)
        } else {
            ("▶ Play", egui::Color32::GREEN)
        }
    }
}

impl Default for PlayMode {
    fn default() -> Self {
        Self::new()
    }
}
