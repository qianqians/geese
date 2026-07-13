//! Widget trait、绘制命令中间表示、内置基础组件。
//!
//! 所有游戏 UI 元素均实现 [`Widget`] trait；每帧由 HUD 调用
//! `update → layout → draw`，绘制结果写入 [`DrawList`]，
//! 后续由渲染后端（wgpu / egui 桥接 / 自绘）消费。

use super::layout::{Layout, Rect};

// ---------------------------------------------------------------------------
// 游戏输入快照（每帧由上层填充）
// ---------------------------------------------------------------------------

/// 单帧输入快照，供 Widget 处理交互。
#[derive(Debug, Clone, Copy, Default)]
pub struct GameInput {
    /// 鼠标 / 主指针在屏幕逻辑坐标中的位置。
    pub pointer_x: f32,
    pub pointer_y: f32,
    /// 主按键（左键）本帧刚按下。
    pub just_pressed: bool,
    /// 主按键本帧刚释放。
    pub just_released: bool,
    /// 主按键当前是否处于按住状态。
    pub pressed: bool,
}

impl GameInput {
    /// 指针是否落在 `rect` 内。
    pub fn pointer_in(&self, rect: &Rect) -> bool {
        rect.contains(self.pointer_x, self.pointer_y)
    }
}

// ---------------------------------------------------------------------------
// DrawList：渲染中间表示
// ---------------------------------------------------------------------------

/// 纹理句柄（由渲染后端赋予实际含义）。
pub type TextureHandle = u64;

/// UV 坐标矩形，取值范围通常为 [0,1]。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct UvRect {
    pub u0: f32,
    pub v0: f32,
    pub u1: f32,
    pub v1: f32,
}

impl Default for UvRect {
    fn default() -> Self {
        Self { u0: 0.0, v0: 0.0, u1: 1.0, v1: 1.0 }
    }
}

/// 单条绘制命令。
#[derive(Debug, Clone)]
pub enum DrawCmd {
    /// 填充矩形。
    FillRect {
        rect: Rect,
        color: [f32; 4],
    },
    /// 矩形边框。
    StrokeRect {
        rect: Rect,
        color: [f32; 4],
        thickness: f32,
    },
    /// 文本（x/y 为左上角，font_size 为逻辑像素高度）。
    Text {
        x: f32,
        y: f32,
        text: String,
        font_size: f32,
        color: [f32; 4],
    },
    /// 纹理四边形。
    Texture {
        rect: Rect,
        texture: TextureHandle,
        uv: UvRect,
        tint: [f32; 4],
    },
}

/// 收集一帧内所有 Widget 产出的绘制命令。
///
/// 渲染后端遍历 [`DrawList::cmds`] 进行实际提交。
#[derive(Debug, Default)]
pub struct DrawList {
    pub cmds: Vec<DrawCmd>,
}

impl DrawList {
    pub fn new() -> Self {
        Self { cmds: Vec::with_capacity(64) }
    }

    pub fn clear(&mut self) {
        self.cmds.clear();
    }

    /// 追加填充矩形。
    pub fn fill_rect(&mut self, rect: Rect, color: [f32; 4]) {
        self.cmds.push(DrawCmd::FillRect { rect, color });
    }

    /// 追加矩形边框。
    pub fn stroke_rect(&mut self, rect: Rect, color: [f32; 4], thickness: f32) {
        self.cmds.push(DrawCmd::StrokeRect { rect, color, thickness });
    }

    /// 追加文本。
    pub fn draw_text(&mut self, x: f32, y: f32, text: impl Into<String>, font_size: f32, color: [f32; 4]) {
        self.cmds.push(DrawCmd::Text { x, y, text: text.into(), font_size, color });
    }

    /// 追加纹理四边形。
    pub fn draw_texture(&mut self, rect: Rect, texture: TextureHandle, uv: UvRect, tint: [f32; 4]) {
        self.cmds.push(DrawCmd::Texture { rect, texture, uv, tint });
    }

    /// 合并另一个 DrawList（按顺序追加）。
    pub fn append(&mut self, other: &DrawList) {
        self.cmds.extend(other.cmds.iter().cloned());
    }
}

// ---------------------------------------------------------------------------
// Widget trait
// ---------------------------------------------------------------------------

/// 游戏 UI 元素的统一接口。
///
/// 生命周期（每帧由 HUD 按顺序驱动）：
///   `update(dt, input)` → `layout(available)` → `draw(draw_list)`
pub trait Widget: Send + Sync {
    /// 唯一标识符（在同一 HUD 内不可重复）。
    fn id(&self) -> &str;

    /// 逻辑更新：处理输入、动画、状态变化。
    fn update(&mut self, dt: f32, input: &GameInput);

    /// 在 `available` 父区域内计算自身屏幕矩形。
    fn layout(&mut self, available: &Rect);

    /// 向 `draw` 追加本帧绘制命令。
    fn draw(&self, draw: &mut DrawList);

    /// 是否可见（不可见时跳过 `draw`）。
    fn visible(&self) -> bool;

    /// 切换可见性。
    fn set_visible(&mut self, visible: bool);
}

// ---------------------------------------------------------------------------
// Label
// ---------------------------------------------------------------------------

/// 静态文本标签。
pub struct Label {
    pub id: String,
    pub text: String,
    pub font_size: f32,
    pub color: [f32; 4],
    pub layout: Layout,
    visible: bool,
    rect: Rect,
}

impl Label {
    pub fn new(id: impl Into<String>, text: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            text: text.into(),
            font_size: 16.0,
            color: [1.0, 1.0, 1.0, 1.0],
            layout: Layout::default(),
            visible: true,
            rect: Rect::default(),
        }
    }

    pub fn with_font_size(mut self, size: f32) -> Self {
        self.font_size = size;
        self
    }

    pub fn with_color(mut self, color: [f32; 4]) -> Self {
        self.color = color;
        self
    }

    pub fn with_layout(mut self, layout: Layout) -> Self {
        self.layout = layout;
        self
    }
}

impl Widget for Label {
    fn id(&self) -> &str { &self.id }
    fn update(&mut self, _dt: f32, _input: &GameInput) {}
    fn layout(&mut self, available: &Rect) {
        self.rect = self.layout.compute(available);
    }
    fn draw(&self, draw: &mut DrawList) {
        draw.draw_text(self.rect.x, self.rect.y, &self.text, self.font_size, self.color);
    }
    fn visible(&self) -> bool { self.visible }
    fn set_visible(&mut self, v: bool) { self.visible = v; }
}

// ---------------------------------------------------------------------------
// Button
// ---------------------------------------------------------------------------

/// 可点击按钮：维护 hover / pressed 视觉状态。
pub struct Button {
    pub id: String,
    pub label: String,
    pub font_size: f32,
    pub bg_color: [f32; 4],
    pub hover_color: [f32; 4],
    pub press_color: [f32; 4],
    pub text_color: [f32; 4],
    pub layout: Layout,
    /// 本帧是否被点击（在 `update` 中设置，供外部读取后清零）。
    pub clicked: bool,
    visible: bool,
    rect: Rect,
    is_hover: bool,
    is_pressed: bool,
}

impl Button {
    pub fn new(id: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            font_size: 14.0,
            bg_color: [0.25, 0.25, 0.25, 1.0],
            hover_color: [0.35, 0.35, 0.35, 1.0],
            press_color: [0.15, 0.15, 0.15, 1.0],
            text_color: [1.0, 1.0, 1.0, 1.0],
            layout: Layout::default(),
            clicked: false,
            visible: true,
            rect: Rect::default(),
            is_hover: false,
            is_pressed: false,
        }
    }

    pub fn with_layout(mut self, layout: Layout) -> Self {
        self.layout = layout;
        self
    }
}

impl Widget for Button {
    fn id(&self) -> &str { &self.id }

    fn update(&mut self, _dt: f32, input: &GameInput) {
        self.clicked = false;
        self.is_hover = input.pointer_in(&self.rect);
        if self.is_hover {
            if input.just_pressed {
                self.is_pressed = true;
            }
            if input.just_released && self.is_pressed {
                self.is_pressed = false;
                self.clicked = true;
            }
        } else {
            self.is_pressed = false;
        }
    }

    fn layout(&mut self, available: &Rect) {
        self.rect = self.layout.compute(available);
    }

    fn draw(&self, draw: &mut DrawList) {
        let bg = if self.is_pressed {
            self.press_color
        } else if self.is_hover {
            self.hover_color
        } else {
            self.bg_color
        };
        draw.fill_rect(self.rect, bg);
        draw.stroke_rect(self.rect, [0.5, 0.5, 0.5, 1.0], 1.0);
        // 文本居中近似：左内边距 8px，垂直居中
        let tx = self.rect.x + 8.0;
        let ty = self.rect.y + (self.rect.height - self.font_size) * 0.5;
        draw.draw_text(tx, ty, &self.label, self.font_size, self.text_color);
    }

    fn visible(&self) -> bool { self.visible }
    fn set_visible(&mut self, v: bool) { self.visible = v; }
}

// ---------------------------------------------------------------------------
// ProgressBar
// ---------------------------------------------------------------------------

/// 水平进度条，取值 [0.0, 1.0]。
pub struct ProgressBar {
    pub id: String,
    pub value: f32,
    pub fill_color: [f32; 4],
    pub bg_color: [f32; 4],
    pub border_color: [f32; 4],
    pub layout: Layout,
    visible: bool,
    rect: Rect,
}

impl ProgressBar {
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            value: 0.0,
            fill_color: [0.2, 0.75, 0.3, 1.0],
            bg_color: [0.15, 0.15, 0.15, 1.0],
            border_color: [0.4, 0.4, 0.4, 1.0],
            layout: Layout { width: 200.0, height: 20.0, ..Layout::default() },
            visible: true,
            rect: Rect::default(),
        }
    }

    pub fn with_value(mut self, v: f32) -> Self {
        self.value = v.clamp(0.0, 1.0);
        self
    }

    pub fn with_fill_color(mut self, color: [f32; 4]) -> Self {
        self.fill_color = color;
        self
    }

    pub fn with_layout(mut self, layout: Layout) -> Self {
        self.layout = layout;
        self
    }
}

impl Widget for ProgressBar {
    fn id(&self) -> &str { &self.id }
    fn update(&mut self, _dt: f32, _input: &GameInput) {}

    fn layout(&mut self, available: &Rect) {
        self.rect = self.layout.compute(available);
    }

    fn draw(&self, draw: &mut DrawList) {
        draw.fill_rect(self.rect, self.bg_color);
        let fill_w = self.rect.width * self.value.clamp(0.0, 1.0);
        let fill_rect = Rect::new(self.rect.x, self.rect.y, fill_w, self.rect.height);
        draw.fill_rect(fill_rect, self.fill_color);
        draw.stroke_rect(self.rect, self.border_color, 1.0);
    }

    fn visible(&self) -> bool { self.visible }
    fn set_visible(&mut self, v: bool) { self.visible = v; }
}

// ---------------------------------------------------------------------------
// Image
// ---------------------------------------------------------------------------

/// 纹理图像显示。
pub struct Image {
    pub id: String,
    pub texture: TextureHandle,
    pub uv: UvRect,
    pub tint: [f32; 4],
    pub layout: Layout,
    visible: bool,
    rect: Rect,
}

impl Image {
    pub fn new(id: impl Into<String>, texture: TextureHandle) -> Self {
        Self {
            id: id.into(),
            texture,
            uv: UvRect::default(),
            tint: [1.0, 1.0, 1.0, 1.0],
            layout: Layout { width: 64.0, height: 64.0, ..Layout::default() },
            visible: true,
            rect: Rect::default(),
        }
    }

    pub fn with_uv(mut self, uv: UvRect) -> Self {
        self.uv = uv;
        self
    }

    pub fn with_tint(mut self, tint: [f32; 4]) -> Self {
        self.tint = tint;
        self
    }

    pub fn with_layout(mut self, layout: Layout) -> Self {
        self.layout = layout;
        self
    }
}

impl Widget for Image {
    fn id(&self) -> &str { &self.id }
    fn update(&mut self, _dt: f32, _input: &GameInput) {}

    fn layout(&mut self, available: &Rect) {
        self.rect = self.layout.compute(available);
    }

    fn draw(&self, draw: &mut DrawList) {
        draw.draw_texture(self.rect, self.texture, self.uv, self.tint);
    }

    fn visible(&self) -> bool { self.visible }
    fn set_visible(&mut self, v: bool) { self.visible = v; }
}

// ---------------------------------------------------------------------------
// Panel
// ---------------------------------------------------------------------------

/// 容器面板：可持有子 Widget，绘制背景 + 边框。
pub struct Panel {
    pub id: String,
    pub bg_color: [f32; 4],
    pub border_color: Option<[f32; 4]>,
    pub border_thickness: f32,
    pub layout: Layout,
    pub children: Vec<Box<dyn Widget>>,
    visible: bool,
    rect: Rect,
}

impl Panel {
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            bg_color: [0.1, 0.1, 0.1, 0.85],
            border_color: Some([0.3, 0.3, 0.3, 1.0]),
            border_thickness: 1.0,
            layout: Layout { width: 300.0, height: 200.0, ..Layout::default() },
            children: Vec::new(),
            visible: true,
            rect: Rect::default(),
        }
    }

    pub fn with_bg_color(mut self, color: [f32; 4]) -> Self {
        self.bg_color = color;
        self
    }

    pub fn with_layout(mut self, layout: Layout) -> Self {
        self.layout = layout;
        self
    }

    pub fn add_child(&mut self, widget: Box<dyn Widget>) {
        self.children.push(widget);
    }
}

impl Widget for Panel {
    fn id(&self) -> &str { &self.id }

    fn update(&mut self, dt: f32, input: &GameInput) {
        for child in &mut self.children {
            if child.visible() {
                child.update(dt, input);
            }
        }
    }

    fn layout(&mut self, available: &Rect) {
        self.rect = self.layout.compute(available);
        for child in &mut self.children {
            child.layout(&self.rect);
        }
    }

    fn draw(&self, draw: &mut DrawList) {
        draw.fill_rect(self.rect, self.bg_color);
        if let Some(bc) = self.border_color {
            draw.stroke_rect(self.rect, bc, self.border_thickness);
        }
        for child in &self.children {
            if child.visible() {
                child.draw(draw);
            }
        }
    }

    fn visible(&self) -> bool { self.visible }
    fn set_visible(&mut self, v: bool) { self.visible = v; }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn button_click_requires_hover_and_release() {
        let mut btn = Button::new("btn1", "OK");
        let screen = Rect::new(0.0, 0.0, 800.0, 600.0);
        btn.layout(&screen);

        // Pointer outside → no click
        let mut input = GameInput { pointer_x: 999.0, pointer_y: 999.0, just_released: true, ..Default::default() };
        btn.update(0.016, &input);
        assert!(!btn.clicked);

        // Pointer inside, press then release
        input = GameInput { pointer_x: 50.0, pointer_y: 20.0, just_pressed: true, ..Default::default() };
        btn.update(0.016, &input);
        input = GameInput { pointer_x: 50.0, pointer_y: 20.0, just_released: true, ..Default::default() };
        btn.update(0.016, &input);
        assert!(btn.clicked);
    }

    #[test]
    fn progress_bar_clamps_value() {
        let bar = ProgressBar::new("hp").with_value(1.5);
        assert!((bar.value - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn draw_list_collects_commands() {
        let mut dl = DrawList::new();
        dl.fill_rect(Rect::new(0.0, 0.0, 10.0, 10.0), [1.0; 4]);
        dl.draw_text(5.0, 5.0, "hi", 12.0, [1.0; 4]);
        assert_eq!(dl.cmds.len(), 2);
    }
}
