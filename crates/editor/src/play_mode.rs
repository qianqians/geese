//! Play/Stop 播放测试模式。
//!
//! 管理编辑器和运行时模式之间的切换：
//! - Play：捕获场景状态，启用物理和输入；Python 游戏项目会启动子进程
//! - Stop：恢复编辑模式场景状态，终止子进程
//!
//! 工具栏按钮状态指示器：红=播放中/灰=编辑中

use crate::panel_layer::PanelLayer;
use crate::panels::EditorState;
use std::process::{Child, Command};

/// Python 游戏配置（从 config/project.toml 的 [game] 段解析）。
#[derive(Debug, Clone)]
pub struct GameConfig {
    pub game_type: String,
    pub module: String,
    pub class_name: String,
    pub title: String,
    pub width: u32,
    pub height: u32,
}

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
    /// Python 游戏子进程句柄（Play 时启动，Stop 时终止）
    child_process: Option<Child>,
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
            child_process: None,
        }
    }

    /// 启动 Python 游戏子进程。
    ///
    /// 通过 `run_game.py` 脚本以 `--direct` 模式启动游戏。
    /// 返回 `true` 表示成功启动，`false` 表示启动失败。
    pub fn launch_game(
        &mut self,
        project_path: &str,
        engine_root: &str,
        config: &GameConfig,
    ) -> bool {
        // 先终止已有子进程（防止重复启动）
        self.kill_game();

        let run_game_py = std::path::Path::new(engine_root)
            .join("crates")
            .join("game_runtime")
            .join("run_game.py");

        if !run_game_py.exists() {
            eprintln!(
                "[PlayMode] run_game.py not found: {}",
                run_game_py.display()
            );
            return false;
        }

        // 尝试 python 命令，回退 python3
        let python_cmd = if cfg!(windows) { "python" } else { "python3" };

        match Command::new(python_cmd)
            .arg(&run_game_py)
            .arg(project_path)
            .arg(&config.module)
            .arg("--class")
            .arg(&config.class_name)
            .arg("--title")
            .arg(&config.title)
            .arg("--width")
            .arg(config.width.to_string())
            .arg("--height")
            .arg(config.height.to_string())
            .arg("--direct")
            .spawn()
        {
            Ok(child) => {
                println!(
                    "[PlayMode] Python game started (pid={})",
                    child.id()
                );
                self.child_process = Some(child);
                true
            }
            Err(e) => {
                eprintln!("[PlayMode] Failed to start Python game: {}", e);
                false
            }
        }
    }

    /// 终止 Python 游戏子进程。
    pub fn kill_game(&mut self) {
        if let Some(mut child) = self.child_process.take() {
            println!("[PlayMode] Killing Python game (pid={})", child.id());
            let _ = child.kill();
            let _ = child.wait();
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
    /// 自动终止任何正在运行的 Python 游戏子进程。
    pub fn stop(&mut self) -> Option<PlayModeSnapshot> {
        if !self.is_playing {
            return None;
        }

        // 终止 Python 游戏子进程（如果有）
        self.kill_game();

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

impl Drop for PlayMode {
    fn drop(&mut self) {
        // 确保编辑器关闭时子进程被终止
        self.kill_game();
    }
}
