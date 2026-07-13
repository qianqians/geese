//! 游戏内 UI 系统（不依赖 egui）。
//!
//! 与编辑器 UI（基于 egui）完全独立，面向游戏运行时设计：
//!
//! - [`widget::Widget`] trait + 内置组件（Label / Button / Panel / ProgressBar / Image）
//! - [`layout::Anchor`] 锚点定位系统
//! - [`hud::HudOverlay`] HUD 层管理
//! - [`GameUI`] 顶层入口，持有多个 HUD 层
//!
//! # 快速示例
//!
//! ```ignore
//! use ui::game_ui::*;
//!
//! let mut gui = GameUI::new(800.0, 600.0);
//! let mut hud = HudOverlay::new("main");
//! hud.add_widget(Box::new(Label::new("score", "Score: 0")));
//! gui.add_overlay(hud);
//!
//! // 每帧
//! gui.update(0.016, &GameInput::default());
//! let draw_list = gui.draw();
//! ```

pub mod hud;
pub mod layout;
pub mod widget;

use hud::HudOverlay;
use layout::Rect;
use widget::{DrawList, GameInput};

/// 游戏 UI 顶层管理器：持有多个 [`HudOverlay`]，每帧统一驱动。
///
/// 设计为可被 `game_runtime` 直接持有并每帧调用。
pub struct GameUI {
    overlays: Vec<HudOverlay>,
    draw_list: DrawList,
    screen: Rect,
}

impl GameUI {
    /// 以给定逻辑分辨率（宽 × 高）创建。
    pub fn new(screen_width: f32, screen_height: f32) -> Self {
        Self {
            overlays: Vec::new(),
            draw_list: DrawList::new(),
            screen: Rect::new(0.0, 0.0, screen_width, screen_height),
        }
    }

    /// 添加一个 HUD 层。
    pub fn add_overlay(&mut self, overlay: HudOverlay) {
        self.overlays.push(overlay);
    }

    /// 按名称移除 HUD 层。
    pub fn remove_overlay(&mut self, name: &str) -> Option<HudOverlay> {
        if let Some(pos) = self.overlays.iter().position(|o| o.name == name) {
            Some(self.overlays.remove(pos))
        } else {
            None
        }
    }

    /// 按名称查找 HUD 层（不可变）。
    pub fn find_overlay(&self, name: &str) -> Option<&HudOverlay> {
        self.overlays.iter().find(|o| o.name == name)
    }

    /// 按名称查找 HUD 层（可变）。
    pub fn find_overlay_mut(&mut self, name: &str) -> Option<&mut HudOverlay> {
        self.overlays.iter_mut().find(|o| o.name == name)
    }

    /// 更新逻辑分辨率（窗口 resize 时调用）。
    pub fn set_screen_size(&mut self, width: f32, height: f32) {
        self.screen = Rect::new(0.0, 0.0, width, height);
    }

    /// 每帧更新：驱动所有 HUD 层的 update + layout。
    pub fn update(&mut self, dt: f32, input: &GameInput) {
        for overlay in &mut self.overlays {
            overlay.update_all(dt, input, &self.screen);
        }
    }

    /// 每帧绘制：清空 DrawList 并收集所有 HUD 层的绘制命令，返回引用。
    pub fn draw(&mut self) -> &DrawList {
        self.draw_list.clear();
        for overlay in &self.overlays {
            overlay.draw_all(&mut self.draw_list);
        }
        &self.draw_list
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use widget::Label;

    #[test]
    fn game_ui_add_overlay_and_draw() {
        let mut gui = GameUI::new(800.0, 600.0);
        let mut hud = HudOverlay::new("main");
        hud.add_widget(Box::new(Label::new("lbl", "Hello")));
        gui.add_overlay(hud);

        gui.update(0.016, &GameInput::default());
        let dl = gui.draw();
        // Label 应产出一条 Text 命令
        assert!(!dl.cmds.is_empty());
    }

    #[test]
    fn remove_overlay_by_name() {
        let mut gui = GameUI::new(800.0, 600.0);
        gui.add_overlay(HudOverlay::new("hud1"));
        gui.add_overlay(HudOverlay::new("hud2"));
        assert!(gui.remove_overlay("hud1").is_some());
        assert!(gui.find_overlay("hud1").is_none());
        assert!(gui.find_overlay("hud2").is_some());
    }
}
