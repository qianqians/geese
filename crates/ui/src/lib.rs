//! UI 子系统：基于 [egui](https://github.com/emilk/egui) 0.29 的薄包装。
//!
//! egui 是 Rust 生态事实标准的 immediate-mode UI 库（Bevy / rerun / godot-egui
//! 都在用），核心 crate 不绑定 winit / wgpu，与「骨架阶段不绑后端」的目标天然匹配。
//! 业务层调用 [`UiContext::run`] 在闭包里直接写 egui 代码即可：
//!
//! ```ignore
//! let mut ui = ui::UiContext::new();
//! let out = ui.run(ui::RawInput::default(), |ctx| {
//!     egui::CentralPanel::default().show(ctx, |ui| {
//!         if ui.button("OK").clicked() { /* ... */ }
//!     });
//! });
//! // `out.shapes` / `out.textures_delta` 交给渲染后端 (egui-wgpu / 自绘) 处理。
//! ```
//!
//! 本 crate 只承担三件事，避免把 egui 锁死：
//! 1. 暴露 `UiContext`（`egui::Context` 的薄包装）+ 全局主题。
//! 2. 提供中立的 [`PointerState`] → [`egui::RawInput`] 桥接，方便上层用任意
//!    输入子系统（参见 [`crate::Vec2`]、[`crate::Rect`]）。
//! 3. 重新导出 egui 高频符号，免去上层依赖两套 crate 的麻烦。

// 重新导出 egui 高频符号，业务层无需直接 `use egui::*;`
pub use egui;
pub use egui::{Color32, FullOutput, Pos2, RawInput, Rect as EguiRect};

// ---------------------------------------------------------------------------
// 中立几何/颜色（与 egui 类型可双向转换，但不依赖 egui 即可被引用）
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Vec2 {
    pub x: f32,
    pub y: f32,
}

impl Vec2 {
    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }

    pub fn to_egui(self) -> egui::Vec2 {
        egui::vec2(self.x, self.y)
    }

    pub fn to_pos2(self) -> Pos2 {
        egui::pos2(self.x, self.y)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Rect {
    pub const fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self { x, y, w, h }
    }

    pub fn to_egui(self) -> EguiRect {
        EguiRect::from_min_size(egui::pos2(self.x, self.y), egui::vec2(self.w, self.h))
    }
}

/// 线性 sRGB 颜色 [0,1]。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Color(pub f32, pub f32, pub f32, pub f32);

impl Color {
    pub const WHITE: Color = Color(1.0, 1.0, 1.0, 1.0);
    pub const BLACK: Color = Color(0.0, 0.0, 0.0, 1.0);

    pub const fn rgb(r: f32, g: f32, b: f32) -> Self {
        Self(r, g, b, 1.0)
    }

    pub fn to_color32(self) -> Color32 {
        let to_u8 = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
        Color32::from_rgba_unmultiplied(to_u8(self.0), to_u8(self.1), to_u8(self.2), to_u8(self.3))
    }
}

// ---------------------------------------------------------------------------
// 输入桥接
// ---------------------------------------------------------------------------

/// 中立的指针状态。由 [`crate::Vec2`] / 外部输入子系统（如 `crates/input`）填充，
/// 再通过 [`UiContext::raw_input_from`] 转成 [`egui::RawInput`]。
#[derive(Debug, Clone, Copy, Default)]
pub struct PointerState {
    pub pos: Vec2,
    pub pressed: bool,
    pub just_pressed: bool,
    pub just_released: bool,
}

// ---------------------------------------------------------------------------
// 主题
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy)]
pub enum ThemeMode {
    Dark,
    Light,
}

#[derive(Debug, Clone, Copy)]
pub struct Theme {
    pub mode: ThemeMode,
    pub accent: Color,
}

impl Default for Theme {
    fn default() -> Self {
        Self { mode: ThemeMode::Dark, accent: Color::rgb(0.30, 0.55, 0.95) }
    }
}

impl Theme {
    /// 转 egui Visuals。
    pub fn to_visuals(&self) -> egui::Visuals {
        let mut v = match self.mode {
            ThemeMode::Dark => egui::Visuals::dark(),
            ThemeMode::Light => egui::Visuals::light(),
        };
        v.selection.bg_fill = self.accent.to_color32();
        v.hyperlink_color = self.accent.to_color32();
        v
    }
}

// ---------------------------------------------------------------------------
// UiContext
// ---------------------------------------------------------------------------

/// egui::Context 的薄包装：管理主题、提供 `run(input, build_ui) -> FullOutput`。
///
/// 业务侧每帧调用 [`UiContext::run`]，在闭包里直接写 egui 代码;返回的
/// [`FullOutput`] 包含 shapes + textures_delta，交给渲染后端消费。
pub struct UiContext {
    ctx: egui::Context,
    theme: Theme,
}

impl Default for UiContext {
    fn default() -> Self {
        Self::new()
    }
}

impl UiContext {
    pub fn new() -> Self {
        Self::with_theme(Theme::default())
    }

    pub fn with_theme(theme: Theme) -> Self {
        let ctx = egui::Context::default();
        ctx.set_visuals(theme.to_visuals());
        Self { ctx, theme }
    }

    pub fn egui_ctx(&self) -> &egui::Context {
        &self.ctx
    }

    pub fn theme(&self) -> Theme {
        self.theme
    }

    pub fn set_theme(&mut self, theme: Theme) {
        self.theme = theme;
        self.ctx.set_visuals(theme.to_visuals());
    }

    /// 单帧渲染：传入 RawInput + UI 构建闭包，返回本帧 FullOutput。
    pub fn run<F: FnMut(&egui::Context)>(&self, input: RawInput, build_ui: F) -> FullOutput {
        self.ctx.run(input, build_ui)
    }

    /// 由中立的 [`PointerState`] + 视口尺寸生成 [`egui::RawInput`]。
    ///
    /// 仅处理鼠标位置与左键 press/release 事件，足够覆盖 HUD/调试面板的基本交互;
    /// 复杂场景请直接构造 `egui::RawInput`（键盘/IME/手势等）。
    pub fn raw_input_from(pointer: PointerState, viewport: Vec2) -> RawInput {
        let mut input = RawInput::default();
        input.screen_rect = Some(EguiRect::from_min_size(
            Pos2::ZERO,
            egui::vec2(viewport.x, viewport.y),
        ));

        let pos = pointer.pos.to_pos2();
        input.events.push(egui::Event::PointerMoved(pos));

        if pointer.just_pressed {
            input.events.push(egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: Default::default(),
            });
        }
        if pointer.just_released {
            input.events.push(egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: Default::default(),
            });
        }
        input
    }
}

// ---------------------------------------------------------------------------
// 测试
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_empty_ui_produces_full_output() {
        let ui = UiContext::new();
        let out = ui.run(RawInput::default(), |_ctx| {});
        // 空 UI 仍应产出（可能 0 个）shapes，且无 panic。
        let _shapes = out.shapes;
    }

    #[test]
    fn central_panel_with_button_runs_without_panic() {
        let ui = UiContext::new();
        let mut clicked = false;
        let out = ui.run(RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                if ui.button("Hello").clicked() {
                    clicked = true;
                }
            });
        });
        assert!(!clicked, "no pointer input → not clicked");
        // 应有至少一些绘制基元（按钮 + 文本）
        assert!(!out.shapes.is_empty());
    }

    #[test]
    fn pointer_state_translates_to_egui_events() {
        let pointer = PointerState {
            pos: Vec2::new(50.0, 25.0),
            pressed: true,
            just_pressed: true,
            just_released: false,
        };
        let input = UiContext::raw_input_from(pointer, Vec2::new(800.0, 600.0));
        assert!(input.screen_rect.is_some());
        // 应至少包含 PointerMoved + PointerButton(pressed=true)
        let has_move = input.events.iter().any(|e| matches!(e, egui::Event::PointerMoved(_)));
        let has_press = input.events.iter().any(|e| matches!(
            e,
            egui::Event::PointerButton { pressed: true, .. }
        ));
        assert!(has_move);
        assert!(has_press);
    }

    #[test]
    fn theme_switch_applies_to_egui_visuals() {
        let mut ui = UiContext::new();
        // 默认 dark
        let dark_bg = ui.egui_ctx().style().visuals.window_fill;
        ui.set_theme(Theme { mode: ThemeMode::Light, accent: Color::rgb(1.0, 0.0, 0.0) });
        let light_bg = ui.egui_ctx().style().visuals.window_fill;
        assert_ne!(dark_bg, light_bg, "switching mode should change window_fill");
    }

    #[test]
    fn color_to_color32_clamps_and_rounds() {
        let c = Color(1.5, -0.2, 0.5, 1.0).to_color32();
        let (r, g, b, a) = (c.r(), c.g(), c.b(), c.a());
        assert_eq!(r, 255);
        assert_eq!(g, 0);
        assert!((b as i32 - 128).abs() <= 1, "got {b}");
        assert_eq!(a, 255);
    }
}
