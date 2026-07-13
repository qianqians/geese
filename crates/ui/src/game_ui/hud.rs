//! HUD Overlay：管理一组 Widget 的集合，提供增删查与统一 update/draw。

use super::layout::Rect;
use super::widget::{DrawList, GameInput, Widget};

/// 一个 HUD 层：持有若干 Widget，每帧统一驱动。
///
/// 典型用法：一个 `HudOverlay` 对应屏幕上一个逻辑分组
/// （如"状态栏"、"小地图"、"快捷技能栏"）。
pub struct HudOverlay {
    pub name: String,
    widgets: Vec<Box<dyn Widget>>,
}

impl HudOverlay {
    pub fn new(name: impl Into<String>) -> Self {
        Self { name: name.into(), widgets: Vec::new() }
    }

    /// 添加 Widget；若同 id 已存在则替换。
    pub fn add_widget(&mut self, widget: Box<dyn Widget>) {
        let id = widget.id().to_string();
        if let Some(pos) = self.widgets.iter().position(|w| w.id() == id) {
            self.widgets[pos] = widget;
        } else {
            self.widgets.push(widget);
        }
    }

    /// 按 id 移除，返回被移除的 Widget（若存在）。
    pub fn remove_widget(&mut self, id: &str) -> Option<Box<dyn Widget>> {
        if let Some(pos) = self.widgets.iter().position(|w| w.id() == id) {
            Some(self.widgets.remove(pos))
        } else {
            None
        }
    }

    /// 按 id 查找不可变引用。
    pub fn find_widget(&self, id: &str) -> Option<&dyn Widget> {
        self.widgets.iter().find(|w| w.id() == id).map(|b| b.as_ref())
    }

    /// 按 id 查找可变引用。
    pub fn find_widget_mut(&mut self, id: &str) -> Option<&mut (dyn Widget + '_)> {
        let pos = self.widgets.iter().position(|w| w.id() == id)?;
        Some(self.widgets[pos].as_mut())
    }

    /// Widget 数量。
    pub fn len(&self) -> usize {
        self.widgets.len()
    }

    pub fn is_empty(&self) -> bool {
        self.widgets.is_empty()
    }

    /// 对所有可见 Widget 执行 update → layout → draw。
    pub fn update_all(&mut self, dt: f32, input: &GameInput, screen: &Rect) {
        for w in &mut self.widgets {
            if w.visible() {
                w.update(dt, input);
                w.layout(screen);
            }
        }
    }

    /// 将所有可见 Widget 的绘制命令写入 `draw`。
    pub fn draw_all(&self, draw: &mut DrawList) {
        for w in &self.widgets {
            if w.visible() {
                w.draw(draw);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game_ui::widget::Label;

    #[test]
    fn add_find_remove_widget() {
        let mut hud = HudOverlay::new("test");
        hud.add_widget(Box::new(Label::new("lbl1", "Hello")));
        assert_eq!(hud.len(), 1);
        assert!(hud.find_widget("lbl1").is_some());

        hud.add_widget(Box::new(Label::new("lbl2", "World")));
        assert_eq!(hud.len(), 2);

        let removed = hud.remove_widget("lbl1");
        assert!(removed.is_some());
        assert_eq!(hud.len(), 1);
        assert!(hud.find_widget("lbl1").is_none());
    }

    #[test]
    fn duplicate_id_replaces_existing() {
        let mut hud = HudOverlay::new("test");
        hud.add_widget(Box::new(Label::new("lbl", "v1")));
        hud.add_widget(Box::new(Label::new("lbl", "v2")));
        assert_eq!(hud.len(), 1);
    }
}
