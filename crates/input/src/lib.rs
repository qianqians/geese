//! 输入子系统（骨架）。
//!
//! 提供键盘 / 鼠标 / 手柄事件的统一抽象，与具体窗口/手柄库（winit/gilrs）解耦。
//! 后续“可用级”实现将提供 `WinitBackend`、`GilrsBackend`，但 trait 与数据结构在
//! 骨架阶段就固定下来，便于上层按接口编码。
//!
//! 设计要点：
//! - [`InputEvent`]：标准化事件，所有 backend 都把原生事件翻译成这种枚举。
//! - [`InputState`]：当前帧的按键/按钮状态快照，由 backend 在 `poll_events`
//!   过程中累积更新。
//! - [`InputBackend`]：trait，业务层只依赖它，不依赖 winit/sdl2。
//! - [`ActionMap`]：把物理按键映射到业务动作名（如 "jump" → Space），支持
//!   后续从配置文件加载并热重映射。

use std::collections::{HashMap, HashSet};

mod gilrs_backend;
pub use gilrs_backend::{GilrsBackend, GilrsInitError};

// ---------------------------------------------------------------------------
// 物理输入枚举
// ---------------------------------------------------------------------------

/// 键盘按键（精简版，覆盖游戏常用键）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum KeyCode {
    // 字母
    A, B, C, D, E, F, G, H, I, J, K, L, M,
    N, O, P, Q, R, S, T, U, V, W, X, Y, Z,
    // 数字
    Num0, Num1, Num2, Num3, Num4,
    Num5, Num6, Num7, Num8, Num9,
    // 功能键
    F1, F2, F3, F4, F5, F6, F7, F8, F9, F10, F11, F12,
    // 控制
    Escape, Tab, Space, Enter, Backspace,
    Left, Right, Up, Down,
    LeftShift, RightShift, LeftCtrl, RightCtrl, LeftAlt, RightAlt,
}

/// 鼠标按键。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
    Other(u8),
}

/// 手柄按键（XInput 风格命名）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GamepadButton {
    South,   // A / Cross
    East,    // B / Circle
    West,    // X / Square
    North,   // Y / Triangle
    LeftBumper,
    RightBumper,
    LeftTrigger,
    RightTrigger,
    Select,
    Start,
    LeftStick,
    RightStick,
    DPadUp,
    DPadDown,
    DPadLeft,
    DPadRight,
}

/// 手柄轴（双摇杆 + 扳机）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GamepadAxis {
    LeftStickX,
    LeftStickY,
    RightStickX,
    RightStickY,
    LeftZ,
    RightZ,
}

// ---------------------------------------------------------------------------
// 事件
// ---------------------------------------------------------------------------

/// 输入事件。所有 backend 把原生事件翻译成这种枚举送进 [`InputState`]。
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum InputEvent {
    KeyPressed(KeyCode),
    KeyReleased(KeyCode),
    MouseMoved { x: f32, y: f32, dx: f32, dy: f32 },
    MouseButtonPressed(MouseButton),
    MouseButtonReleased(MouseButton),
    /// 鼠标滚轮，单位为「行」（正向上、负向下）。
    Scroll { dx: f32, dy: f32 },
    GamepadConnected(u32),
    GamepadDisconnected(u32),
    GamepadButtonPressed { id: u32, button: GamepadButton },
    GamepadButtonReleased { id: u32, button: GamepadButton },
    /// 手柄轴值变化，范围 [-1.0, 1.0]（扳机为 [0.0, 1.0]）。
    GamepadAxisChanged { id: u32, axis: GamepadAxis, value: f32 },
    /// 窗口失焦时 backend 应发出，以便上层清空按键状态防止「卡键」。
    FocusLost,
}

// ---------------------------------------------------------------------------
// 帧状态
// ---------------------------------------------------------------------------

/// 当前帧的输入状态快照。
///
/// backend 在 `poll_events` 时调用 [`InputState::apply`] 累积事件。
/// 业务层每帧读取 `is_*_down` / `pressed_this_frame` / `mouse_position` 等。
#[derive(Debug, Default)]
pub struct InputState {
    keys_down: HashSet<KeyCode>,
    keys_pressed_this_frame: HashSet<KeyCode>,
    keys_released_this_frame: HashSet<KeyCode>,

    mouse_buttons_down: HashSet<MouseButton>,
    mouse_buttons_pressed_this_frame: HashSet<MouseButton>,
    mouse_buttons_released_this_frame: HashSet<MouseButton>,

    mouse_pos: (f32, f32),
    mouse_delta: (f32, f32),
    scroll_delta: (f32, f32),

    gamepad_axes: HashMap<(u32, GamepadAxis), f32>,
    gamepad_buttons_down: HashSet<(u32, GamepadButton)>,
}

impl InputState {
    pub fn new() -> Self {
        Self::default()
    }

    /// 每帧开始时调用，清空「本帧」类型的瞬时状态（仍保留 down 状态）。
    pub fn begin_frame(&mut self) {
        self.keys_pressed_this_frame.clear();
        self.keys_released_this_frame.clear();
        self.mouse_buttons_pressed_this_frame.clear();
        self.mouse_buttons_released_this_frame.clear();
        self.mouse_delta = (0.0, 0.0);
        self.scroll_delta = (0.0, 0.0);
    }

    /// 处理单个事件，更新内部状态。
    pub fn apply(&mut self, event: &InputEvent) {
        match *event {
            InputEvent::KeyPressed(k) => {
                if self.keys_down.insert(k) {
                    self.keys_pressed_this_frame.insert(k);
                }
            }
            InputEvent::KeyReleased(k) => {
                if self.keys_down.remove(&k) {
                    self.keys_released_this_frame.insert(k);
                }
            }
            InputEvent::MouseMoved { x, y, dx, dy } => {
                self.mouse_pos = (x, y);
                self.mouse_delta.0 += dx;
                self.mouse_delta.1 += dy;
            }
            InputEvent::MouseButtonPressed(b) => {
                if self.mouse_buttons_down.insert(b) {
                    self.mouse_buttons_pressed_this_frame.insert(b);
                }
            }
            InputEvent::MouseButtonReleased(b) => {
                if self.mouse_buttons_down.remove(&b) {
                    self.mouse_buttons_released_this_frame.insert(b);
                }
            }
            InputEvent::Scroll { dx, dy } => {
                self.scroll_delta.0 += dx;
                self.scroll_delta.1 += dy;
            }
            InputEvent::GamepadButtonPressed { id, button } => {
                self.gamepad_buttons_down.insert((id, button));
            }
            InputEvent::GamepadButtonReleased { id, button } => {
                self.gamepad_buttons_down.remove(&(id, button));
            }
            InputEvent::GamepadAxisChanged { id, axis, value } => {
                self.gamepad_axes.insert((id, axis), value);
            }
            InputEvent::GamepadConnected(_) | InputEvent::GamepadDisconnected(_) => {
                // 业务层自行处理;状态层无副作用。
            }
            InputEvent::FocusLost => {
                // 防卡键：失焦时清空所有 down 状态。
                self.keys_down.clear();
                self.mouse_buttons_down.clear();
                self.gamepad_buttons_down.clear();
            }
        }
    }

    pub fn is_key_down(&self, k: KeyCode) -> bool {
        self.keys_down.contains(&k)
    }

    pub fn key_pressed(&self, k: KeyCode) -> bool {
        self.keys_pressed_this_frame.contains(&k)
    }

    pub fn key_released(&self, k: KeyCode) -> bool {
        self.keys_released_this_frame.contains(&k)
    }

    pub fn is_mouse_down(&self, b: MouseButton) -> bool {
        self.mouse_buttons_down.contains(&b)
    }

    pub fn mouse_pressed(&self, b: MouseButton) -> bool {
        self.mouse_buttons_pressed_this_frame.contains(&b)
    }

    pub fn mouse_released(&self, b: MouseButton) -> bool {
        self.mouse_buttons_released_this_frame.contains(&b)
    }

    pub fn mouse_position(&self) -> (f32, f32) {
        self.mouse_pos
    }

    pub fn mouse_delta(&self) -> (f32, f32) {
        self.mouse_delta
    }

    pub fn scroll_delta(&self) -> (f32, f32) {
        self.scroll_delta
    }

    pub fn gamepad_axis(&self, id: u32, axis: GamepadAxis) -> f32 {
        self.gamepad_axes.get(&(id, axis)).copied().unwrap_or(0.0)
    }

    pub fn is_gamepad_button_down(&self, id: u32, button: GamepadButton) -> bool {
        self.gamepad_buttons_down.contains(&(id, button))
    }
}

// ---------------------------------------------------------------------------
// Backend
// ---------------------------------------------------------------------------

/// 输入后端 trait：负责把原生输入翻译成 [`InputEvent`]。
///
/// 真实实现会包装 winit/sdl2/gilrs 等库;骨架阶段提供 [`NullBackend`]。
pub trait InputBackend {
    /// 拉取自上次调用以来产生的事件追加到 `sink`。
    fn poll_events(&mut self, sink: &mut Vec<InputEvent>);
}

/// 不产生任何事件的占位 backend，用于测试与「无窗口」运行模式。
#[derive(Debug, Default)]
pub struct NullBackend;

impl InputBackend for NullBackend {
    fn poll_events(&mut self, _sink: &mut Vec<InputEvent>) {}
}

// ---------------------------------------------------------------------------
// 动作映射（ActionMap）
// ---------------------------------------------------------------------------

/// 一个动作可以绑定多个输入源（按键 / 鼠标 / 手柄按钮）。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum InputBinding {
    Key(KeyCode),
    Mouse(MouseButton),
    GamepadButton(GamepadButton),
}

/// 业务动作名 → 输入绑定 的映射。
#[derive(Debug, Default)]
pub struct ActionMap {
    bindings: HashMap<String, Vec<InputBinding>>,
}

impl ActionMap {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn bind(&mut self, action: impl Into<String>, binding: InputBinding) {
        self.bindings.entry(action.into()).or_default().push(binding);
    }

    pub fn unbind_all(&mut self, action: &str) {
        self.bindings.remove(action);
    }

    /// 任意一个绑定按下即视为动作激活。
    pub fn is_active(&self, action: &str, state: &InputState) -> bool {
        let Some(list) = self.bindings.get(action) else {
            return false;
        };
        list.iter().any(|b| match *b {
            InputBinding::Key(k) => state.is_key_down(k),
            InputBinding::Mouse(m) => state.is_mouse_down(m),
            // 手柄绑定需要指定 gamepad id;默认查询 id=0（单人本地）。
            InputBinding::GamepadButton(g) => state.is_gamepad_button_down(0, g),
        })
    }

    pub fn bindings(&self, action: &str) -> Option<&[InputBinding]> {
        self.bindings.get(action).map(|v| v.as_slice())
    }
}

// ---------------------------------------------------------------------------
// 测试
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_press_release_tracks_down_and_edge() {
        let mut s = InputState::new();
        s.apply(&InputEvent::KeyPressed(KeyCode::W));
        assert!(s.is_key_down(KeyCode::W));
        assert!(s.key_pressed(KeyCode::W));
        assert!(!s.key_released(KeyCode::W));

        s.begin_frame();
        assert!(s.is_key_down(KeyCode::W));
        assert!(!s.key_pressed(KeyCode::W), "press edge cleared at frame begin");

        s.apply(&InputEvent::KeyReleased(KeyCode::W));
        assert!(!s.is_key_down(KeyCode::W));
        assert!(s.key_released(KeyCode::W));
    }

    #[test]
    fn focus_lost_clears_all_held_state() {
        let mut s = InputState::new();
        s.apply(&InputEvent::KeyPressed(KeyCode::Space));
        s.apply(&InputEvent::MouseButtonPressed(MouseButton::Left));
        s.apply(&InputEvent::GamepadButtonPressed { id: 0, button: GamepadButton::South });
        assert!(s.is_key_down(KeyCode::Space));

        s.apply(&InputEvent::FocusLost);
        assert!(!s.is_key_down(KeyCode::Space));
        assert!(!s.is_mouse_down(MouseButton::Left));
        assert!(!s.is_gamepad_button_down(0, GamepadButton::South));
    }

    #[test]
    fn mouse_delta_and_scroll_accumulate_within_frame() {
        let mut s = InputState::new();
        s.apply(&InputEvent::MouseMoved { x: 10.0, y: 20.0, dx: 5.0, dy: 6.0 });
        s.apply(&InputEvent::MouseMoved { x: 12.0, y: 21.0, dx: 2.0, dy: 1.0 });
        s.apply(&InputEvent::Scroll { dx: 0.0, dy: 1.0 });
        s.apply(&InputEvent::Scroll { dx: 0.0, dy: 2.0 });

        assert_eq!(s.mouse_position(), (12.0, 21.0));
        assert_eq!(s.mouse_delta(), (7.0, 7.0));
        assert_eq!(s.scroll_delta(), (0.0, 3.0));

        s.begin_frame();
        assert_eq!(s.mouse_delta(), (0.0, 0.0));
        assert_eq!(s.scroll_delta(), (0.0, 0.0));
    }

    #[test]
    fn action_map_matches_any_binding() {
        let mut map = ActionMap::new();
        map.bind("jump", InputBinding::Key(KeyCode::Space));
        map.bind("jump", InputBinding::GamepadButton(GamepadButton::South));

        let mut s = InputState::new();
        assert!(!map.is_active("jump", &s));

        s.apply(&InputEvent::GamepadButtonPressed { id: 0, button: GamepadButton::South });
        assert!(map.is_active("jump", &s));

        s.apply(&InputEvent::GamepadButtonReleased { id: 0, button: GamepadButton::South });
        s.apply(&InputEvent::KeyPressed(KeyCode::Space));
        assert!(map.is_active("jump", &s));
    }

    #[test]
    fn null_backend_emits_nothing() {
        let mut b = NullBackend;
        let mut sink = Vec::new();
        b.poll_events(&mut sink);
        assert!(sink.is_empty());
    }
}
