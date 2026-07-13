//! 布局系统：屏幕空间矩形、锚点与绝对定位。
//!
//! 游戏 UI 使用逻辑像素坐标系，左上角为原点 (0,0)，x 向右增长，y 向下增长。
//! 所有坐标 / 尺寸均为 `f32`，由最终渲染后端映射到物理像素。

/// 屏幕空间矩形（逻辑像素）。
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl Rect {
    pub const fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self { x, y, width, height }
    }

    /// 从位置 + 尺寸构造。
    pub const fn from_pos_size(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self { x, y, width: w, height: h }
    }

    pub fn right(&self) -> f32 {
        self.x + self.width
    }

    pub fn bottom(&self) -> f32 {
        self.y + self.height
    }

    pub fn contains(&self, px: f32, py: f32) -> bool {
        px >= self.x && px <= self.right() && py >= self.y && py <= self.bottom()
    }

    /// 在 `parent` 矩形内按锚点 + 偏移定位自身。
    pub fn anchor_in(&self, parent: &Rect, anchor: Anchor, offset_x: f32, offset_y: f32) -> Rect {
        let (ax, ay) = anchor.resolve(parent);
        Rect {
            x: ax + offset_x,
            y: ay + offset_y,
            width: self.width,
            height: self.height,
        }
    }
}

/// 锚点：定义 UI 元素相对于父容器的基准位置。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Anchor {
    #[default]
    TopLeft,
    TopCenter,
    TopRight,
    CenterLeft,
    Center,
    CenterRight,
    BottomLeft,
    BottomCenter,
    BottomRight,
}

impl Anchor {
    /// 返回锚点在 `parent` 矩形内的绝对坐标。
    pub fn resolve(&self, parent: &Rect) -> (f32, f32) {
        let cx = parent.x + parent.width * 0.5;
        let cy = parent.y + parent.height * 0.5;
        let r = parent.right();
        let b = parent.bottom();
        match self {
            Anchor::TopLeft => (parent.x, parent.y),
            Anchor::TopCenter => (cx, parent.y),
            Anchor::TopRight => (r, parent.y),
            Anchor::CenterLeft => (parent.x, cy),
            Anchor::Center => (cx, cy),
            Anchor::CenterRight => (r, cy),
            Anchor::BottomLeft => (parent.x, b),
            Anchor::BottomCenter => (cx, b),
            Anchor::BottomRight => (r, b),
        }
    }
}

/// 元素布局描述：锚点 + 偏移 + 尺寸。
///
/// 通过 [`Layout::compute`] 可在给定父容器时算出最终 [`Rect`]。
#[derive(Debug, Clone, Copy)]
pub struct Layout {
    pub anchor: Anchor,
    pub offset_x: f32,
    pub offset_y: f32,
    pub width: f32,
    pub height: f32,
}

impl Default for Layout {
    fn default() -> Self {
        Self {
            anchor: Anchor::TopLeft,
            offset_x: 0.0,
            offset_y: 0.0,
            width: 100.0,
            height: 40.0,
        }
    }
}

impl Layout {
    pub fn new(anchor: Anchor, offset_x: f32, offset_y: f32, width: f32, height: f32) -> Self {
        Self { anchor, offset_x, offset_y, width, height }
    }

    /// 在 `parent` 内计算最终屏幕矩形。
    pub fn compute(&self, parent: &Rect) -> Rect {
        let (ax, ay) = self.anchor.resolve(parent);
        Rect {
            x: ax + self.offset_x,
            y: ay + self.offset_y,
            width: self.width,
            height: self.height,
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rect_contains_point() {
        let r = Rect::new(10.0, 20.0, 100.0, 50.0);
        assert!(r.contains(10.0, 20.0));
        assert!(r.contains(60.0, 45.0));
        assert!(!r.contains(9.0, 30.0));
        assert!(!r.contains(60.0, 71.0));
    }

    #[test]
    fn anchor_center_resolves_to_middle() {
        let parent = Rect::new(0.0, 0.0, 800.0, 600.0);
        let (x, y) = Anchor::Center.resolve(&parent);
        assert!((x - 400.0).abs() < f32::EPSILON);
        assert!((y - 300.0).abs() < f32::EPSILON);
    }

    #[test]
    fn layout_compute_top_right_with_offset() {
        let parent = Rect::new(0.0, 0.0, 800.0, 600.0);
        let layout = Layout::new(Anchor::TopRight, -120.0, 10.0, 100.0, 30.0);
        let r = layout.compute(&parent);
        assert!((r.x - 680.0).abs() < f32::EPSILON);
        assert!((r.y - 10.0).abs() < f32::EPSILON);
    }
}
