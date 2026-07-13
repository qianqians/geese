//! 基于 [winit](https://github.com/rust-windowing/winit) 的键鼠输入后端。
//!
//! 与 [`crate::GilrsBackend`] 完全对称的设计：
//! - 持有 `pending_events: Vec<InputEvent>` 队列
//! - `push_event(&mut self, event: winit::event::WindowEvent)` 翻译 winit 事件为 [`InputEvent`]
//! - `impl InputBackend for WinitBackend` — `poll_events()` drain 队列到 sink
//!
//! 不持有 winit `EventLoop`/`Window`，由调用方推入 `WindowEvent`。

use super::*;

/// winit 后端：把 `WindowEvent` 翻译为 [`InputEvent`]。
pub struct WinitBackend {
    pending_events: Vec<InputEvent>,
    last_mouse_pos: Option<(f64, f64)>,
}

impl Default for WinitBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl WinitBackend {
    pub fn new() -> Self {
        Self {
            pending_events: Vec::new(),
            last_mouse_pos: None,
        }
    }

    /// 推入一个 winit `WindowEvent`，翻译为 [`InputEvent`] 存入内部队列。
    ///
    /// 调用方在 winit 事件循环中调用此方法，然后在每帧通过 `poll_events` 拉取。
    pub fn push_event(&mut self, event: &winit::event::WindowEvent) {
        use winit::event::WindowEvent;

        match event {
            WindowEvent::KeyboardInput { event, .. } => {
                use winit::event::ElementState;
                let physical_key = match &event.physical_key {
                    winit::keyboard::PhysicalKey::Code(code) => code,
                    _ => return,
                };
                if let Some(key) = Self::map_physical_key(physical_key) {
                    match event.state {
                        ElementState::Pressed => {
                            self.pending_events.push(InputEvent::KeyPressed(key));
                        }
                        ElementState::Released => {
                            self.pending_events.push(InputEvent::KeyReleased(key));
                        }
                    }
                }
            }

            WindowEvent::CursorMoved { position, .. } => {
                let (x, y) = (position.x as f32, position.y as f32);
                let (dx, dy) = match self.last_mouse_pos {
                    Some((ox, oy)) => (x - ox as f32, y - oy as f32),
                    None => (0.0, 0.0),
                };
                self.last_mouse_pos = Some((position.x, position.y));
                self.pending_events.push(InputEvent::MouseMoved { x, y, dx, dy });
            }

            WindowEvent::MouseInput { state, button, .. } => {
                use winit::event::ElementState;
                let mapped = Self::map_mouse_button(*button);
                match state {
                    ElementState::Pressed => {
                        self.pending_events
                            .push(InputEvent::MouseButtonPressed(mapped));
                    }
                    ElementState::Released => {
                        self.pending_events
                            .push(InputEvent::MouseButtonReleased(mapped));
                    }
                }
            }

            WindowEvent::MouseWheel { delta, .. } => {
                use winit::event::MouseScrollDelta;
                let (dx, dy) = match delta {
                    MouseScrollDelta::LineDelta(x, y) => (*x, *y),
                    MouseScrollDelta::PixelDelta(pos) => {
                        // Convert pixels to approximate lines (assume ~50px/line)
                        (pos.x as f32 / 50.0, pos.y as f32 / 50.0)
                    }
                };
                self.pending_events.push(InputEvent::Scroll { dx, dy });
            }

            WindowEvent::Focused(false) => {
                // 窗口失焦时发出 FocusLost 事件，以便上层清空按键状态防止「卡键」
                self.pending_events.push(InputEvent::FocusLost);
            }

            WindowEvent::Touch(touch) => {
                use winit::event::TouchPhase;
                let id = touch.id;
                let x = touch.location.x as f32;
                let y = touch.location.y as f32;
                match touch.phase {
                    TouchPhase::Started => {
                        self.pending_events.push(InputEvent::TouchStart { id, x, y });
                    }
                    TouchPhase::Moved => {
                        self.pending_events.push(InputEvent::TouchMove { id, x, y });
                    }
                    TouchPhase::Ended => {
                        self.pending_events.push(InputEvent::TouchEnd { id });
                    }
                    TouchPhase::Cancelled => {
                        self.pending_events.push(InputEvent::TouchCancel { id });
                    }
                }
            }

            _ => {}
        }
    }

    /// 映射 winit `KeyCode`（物理键）到引擎 [`KeyCode`]。
    fn map_physical_key(key: &winit::keyboard::KeyCode) -> Option<KeyCode> {
        use winit::keyboard::KeyCode as W;

        Some(match key {
            // 字母
            W::KeyA => KeyCode::A,
            W::KeyB => KeyCode::B,
            W::KeyC => KeyCode::C,
            W::KeyD => KeyCode::D,
            W::KeyE => KeyCode::E,
            W::KeyF => KeyCode::F,
            W::KeyG => KeyCode::G,
            W::KeyH => KeyCode::H,
            W::KeyI => KeyCode::I,
            W::KeyJ => KeyCode::J,
            W::KeyK => KeyCode::K,
            W::KeyL => KeyCode::L,
            W::KeyM => KeyCode::M,
            W::KeyN => KeyCode::N,
            W::KeyO => KeyCode::O,
            W::KeyP => KeyCode::P,
            W::KeyQ => KeyCode::Q,
            W::KeyR => KeyCode::R,
            W::KeyS => KeyCode::S,
            W::KeyT => KeyCode::T,
            W::KeyU => KeyCode::U,
            W::KeyV => KeyCode::V,
            W::KeyW => KeyCode::W,
            W::KeyX => KeyCode::X,
            W::KeyY => KeyCode::Y,
            W::KeyZ => KeyCode::Z,

            // 数字键
            W::Digit0 => KeyCode::Num0,
            W::Digit1 => KeyCode::Num1,
            W::Digit2 => KeyCode::Num2,
            W::Digit3 => KeyCode::Num3,
            W::Digit4 => KeyCode::Num4,
            W::Digit5 => KeyCode::Num5,
            W::Digit6 => KeyCode::Num6,
            W::Digit7 => KeyCode::Num7,
            W::Digit8 => KeyCode::Num8,
            W::Digit9 => KeyCode::Num9,

            // 功能键
            W::F1 => KeyCode::F1,
            W::F2 => KeyCode::F2,
            W::F3 => KeyCode::F3,
            W::F4 => KeyCode::F4,
            W::F5 => KeyCode::F5,
            W::F6 => KeyCode::F6,
            W::F7 => KeyCode::F7,
            W::F8 => KeyCode::F8,
            W::F9 => KeyCode::F9,
            W::F10 => KeyCode::F10,
            W::F11 => KeyCode::F11,
            W::F12 => KeyCode::F12,

            // 控制键
            W::Escape => KeyCode::Escape,
            W::Tab => KeyCode::Tab,
            W::Space => KeyCode::Space,
            W::Enter => KeyCode::Enter,
            W::Backspace => KeyCode::Backspace,

            // 方向键
            W::ArrowLeft => KeyCode::Left,
            W::ArrowRight => KeyCode::Right,
            W::ArrowUp => KeyCode::Up,
            W::ArrowDown => KeyCode::Down,

            // 修饰键
            W::ShiftLeft => KeyCode::LeftShift,
            W::ShiftRight => KeyCode::RightShift,
            W::ControlLeft => KeyCode::LeftCtrl,
            W::ControlRight => KeyCode::RightCtrl,
            W::AltLeft => KeyCode::LeftAlt,
            W::AltRight => KeyCode::RightAlt,

            _ => return None,
        })
    }

    /// 映射 winit `MouseButton` 到引擎 [`MouseButton`]。
    fn map_mouse_button(button: winit::event::MouseButton) -> MouseButton {
        match button {
            winit::event::MouseButton::Left => MouseButton::Left,
            winit::event::MouseButton::Right => MouseButton::Right,
            winit::event::MouseButton::Middle => MouseButton::Middle,
            winit::event::MouseButton::Back => MouseButton::Other(3),
            winit::event::MouseButton::Forward => MouseButton::Other(4),
            winit::event::MouseButton::Other(id) => MouseButton::Other(id as u8),
        }
    }
}

impl InputBackend for WinitBackend {
    fn poll_events(&mut self, sink: &mut Vec<InputEvent>) {
        // Drain pending events into the sink
        sink.append(&mut self.pending_events);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn map_keycode_letters() {
        use winit::keyboard::KeyCode as W;
        assert_eq!(WinitBackend::map_physical_key(&W::KeyA), Some(KeyCode::A));
        assert_eq!(WinitBackend::map_physical_key(&W::KeyZ), Some(KeyCode::Z));
    }

    #[test]
    fn map_keycode_digits() {
        use winit::keyboard::KeyCode as W;
        assert_eq!(WinitBackend::map_physical_key(&W::Digit0), Some(KeyCode::Num0));
        assert_eq!(WinitBackend::map_physical_key(&W::Digit9), Some(KeyCode::Num9));
    }

    #[test]
    fn map_keycode_function_keys() {
        use winit::keyboard::KeyCode as W;
        assert_eq!(WinitBackend::map_physical_key(&W::F1), Some(KeyCode::F1));
        assert_eq!(WinitBackend::map_physical_key(&W::F12), Some(KeyCode::F12));
    }

    #[test]
    fn map_keycode_control_and_arrows() {
        use winit::keyboard::KeyCode as W;
        assert_eq!(WinitBackend::map_physical_key(&W::Escape), Some(KeyCode::Escape));
        assert_eq!(WinitBackend::map_physical_key(&W::Space), Some(KeyCode::Space));
        assert_eq!(WinitBackend::map_physical_key(&W::Enter), Some(KeyCode::Enter));
        assert_eq!(WinitBackend::map_physical_key(&W::ArrowUp), Some(KeyCode::Up));
        assert_eq!(WinitBackend::map_physical_key(&W::ArrowDown), Some(KeyCode::Down));
        assert_eq!(WinitBackend::map_physical_key(&W::ArrowLeft), Some(KeyCode::Left));
        assert_eq!(WinitBackend::map_physical_key(&W::ArrowRight), Some(KeyCode::Right));
    }

    #[test]
    fn map_keycode_modifiers() {
        use winit::keyboard::KeyCode as W;
        assert_eq!(
            WinitBackend::map_physical_key(&W::ShiftLeft),
            Some(KeyCode::LeftShift)
        );
        assert_eq!(
            WinitBackend::map_physical_key(&W::ShiftRight),
            Some(KeyCode::RightShift)
        );
        assert_eq!(
            WinitBackend::map_physical_key(&W::ControlLeft),
            Some(KeyCode::LeftCtrl)
        );
        assert_eq!(
            WinitBackend::map_physical_key(&W::ControlRight),
            Some(KeyCode::RightCtrl)
        );
        assert_eq!(
            WinitBackend::map_physical_key(&W::AltLeft),
            Some(KeyCode::LeftAlt)
        );
        assert_eq!(
            WinitBackend::map_physical_key(&W::AltRight),
            Some(KeyCode::RightAlt)
        );
    }

    #[test]
    fn map_mouse_buttons() {
        assert_eq!(
            WinitBackend::map_mouse_button(winit::event::MouseButton::Left),
            MouseButton::Left
        );
        assert_eq!(
            WinitBackend::map_mouse_button(winit::event::MouseButton::Right),
            MouseButton::Right
        );
        assert_eq!(
            WinitBackend::map_mouse_button(winit::event::MouseButton::Middle),
            MouseButton::Middle
        );
        assert_eq!(
            WinitBackend::map_mouse_button(winit::event::MouseButton::Other(5)),
            MouseButton::Other(5)
        );
    }

    #[test]
    fn backend_poll_drains_pending() {
        let mut backend = WinitBackend::new();
        // Manually push events
        backend.pending_events.push(InputEvent::KeyPressed(KeyCode::W));
        backend.pending_events.push(InputEvent::KeyPressed(KeyCode::A));

        let mut sink = Vec::new();
        backend.poll_events(&mut sink);
        assert_eq!(sink.len(), 2);
        assert!(matches!(sink[0], InputEvent::KeyPressed(KeyCode::W)));

        // Second poll should be empty
        sink.clear();
        backend.poll_events(&mut sink);
        assert!(sink.is_empty());
    }

    #[test]
    fn unsupported_keycode_returns_none() {
        use winit::keyboard::KeyCode as W;
        // Numpad keys are not in our KeyCode enum
        assert_eq!(WinitBackend::map_physical_key(&W::Numpad0), None);
    }
}
