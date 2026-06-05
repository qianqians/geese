//! 基于 [gilrs](https://gitlab.com/gilrs-project/gilrs) 的真实手柄输入后端。
//!
//! gilrs 是 Rust 生态最常用的跨平台手柄库（XInput / DirectInput / evdev /
//! HID），与窗口库（winit/sdl2）独立，可与任意键鼠后端并存。
//!
//! 本后端只处理 **手柄** 事件;键鼠请用基于 winit 的 backend（M1 可用级阶段
//! 添加）。
//!
//! 用法：
//! ```ignore
//! let mut gp = input::GilrsBackend::try_new()?;
//! let mut events = Vec::new();
//! gp.poll_events(&mut events);
//! for ev in &events { state.apply(ev); }
//! ```

use super::*;

/// gilrs 后端：持有 gilrs::Gilrs 实例，负责把 gilrs 事件翻译为 [`InputEvent`]。
pub struct GilrsBackend {
    gilrs: gilrs::Gilrs,
}

#[derive(Debug)]
pub enum GilrsInitError {
    Init(String),
}

impl std::fmt::Display for GilrsInitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GilrsInitError::Init(e) => write!(f, "gilrs init failed: {e}"),
        }
    }
}

impl std::error::Error for GilrsInitError {}

impl GilrsBackend {
    pub fn try_new() -> Result<Self, GilrsInitError> {
        let gilrs = gilrs::Gilrs::new().map_err(|e| GilrsInitError::Init(format!("{e:?}")))?;
        Ok(Self { gilrs })
    }

    /// gilrs 自身已连接的手柄数。可用于 UI 显示「检测到 N 个手柄」。
    pub fn connected_count(&self) -> usize {
        self.gilrs
            .gamepads()
            .filter(|(_, g)| g.is_connected())
            .count()
    }

    fn map_button(b: gilrs::Button) -> Option<GamepadButton> {
        use gilrs::Button as G;
        Some(match b {
            G::South => GamepadButton::South,
            G::East => GamepadButton::East,
            G::West => GamepadButton::West,
            G::North => GamepadButton::North,
            // gilrs 命名：LeftTrigger=L1(bumper), LeftTrigger2=L2(扳机);
            // 我们的枚举：LeftBumper / LeftTrigger 分别对应物理 L1 / L2。
            G::LeftTrigger => GamepadButton::LeftBumper,
            G::LeftTrigger2 => GamepadButton::LeftTrigger,
            G::RightTrigger => GamepadButton::RightBumper,
            G::RightTrigger2 => GamepadButton::RightTrigger,
            G::Select => GamepadButton::Select,
            G::Start => GamepadButton::Start,
            G::LeftThumb => GamepadButton::LeftStick,
            G::RightThumb => GamepadButton::RightStick,
            G::DPadUp => GamepadButton::DPadUp,
            G::DPadDown => GamepadButton::DPadDown,
            G::DPadLeft => GamepadButton::DPadLeft,
            G::DPadRight => GamepadButton::DPadRight,
            // Mode/C/Z/Unknown 暂时忽略。
            _ => return None,
        })
    }

    fn map_axis(a: gilrs::Axis) -> Option<GamepadAxis> {
        use gilrs::Axis as A;
        Some(match a {
            A::LeftStickX => GamepadAxis::LeftStickX,
            A::LeftStickY => GamepadAxis::LeftStickY,
            A::RightStickX => GamepadAxis::RightStickX,
            A::RightStickY => GamepadAxis::RightStickY,
            A::LeftZ => GamepadAxis::LeftZ,
            A::RightZ => GamepadAxis::RightZ,
            // DPadX/DPadY/Unknown 已由按钮事件覆盖，忽略。
            _ => return None,
        })
    }

    fn id_to_u32(id: gilrs::GamepadId) -> u32 {
        // gilrs::GamepadId 实现 From<GamepadId> for usize。
        usize::from(id) as u32
    }
}

impl InputBackend for GilrsBackend {
    fn poll_events(&mut self, sink: &mut Vec<InputEvent>) {
        while let Some(ev) = self.gilrs.next_event() {
            let id = Self::id_to_u32(ev.id);
            match ev.event {
                gilrs::EventType::Connected => sink.push(InputEvent::GamepadConnected(id)),
                gilrs::EventType::Disconnected => sink.push(InputEvent::GamepadDisconnected(id)),
                gilrs::EventType::ButtonPressed(b, _) => {
                    if let Some(button) = Self::map_button(b) {
                        sink.push(InputEvent::GamepadButtonPressed { id, button });
                    }
                }
                gilrs::EventType::ButtonReleased(b, _) => {
                    if let Some(button) = Self::map_button(b) {
                        sink.push(InputEvent::GamepadButtonReleased { id, button });
                    }
                }
                gilrs::EventType::AxisChanged(a, v, _) => {
                    if let Some(axis) = Self::map_axis(a) {
                        sink.push(InputEvent::GamepadAxisChanged { id, axis, value: v });
                    }
                }
                // ButtonChanged / ButtonRepeated / ForceFeedbackEffectCompleted 等暂忽略。
                _ => {}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gilrs_backend_init_or_skip() {
        // gilrs::Gilrs::new() 在缺少底层 HID 权限或 headless 环境可能失败，软跳过。
        let Ok(mut backend) = GilrsBackend::try_new() else {
            return;
        };
        let mut sink = Vec::new();
        backend.poll_events(&mut sink);
        // 不强校验 sink 内容（依赖运行时是否插入手柄）;只确保 poll 不 panic。
        let _ = backend.connected_count();
    }

    #[test]
    fn map_button_covers_xinput_layout() {
        use gilrs::Button as G;
        assert_eq!(GilrsBackend::map_button(G::South), Some(GamepadButton::South));
        assert_eq!(GilrsBackend::map_button(G::LeftTrigger), Some(GamepadButton::LeftBumper));
        assert_eq!(GilrsBackend::map_button(G::LeftTrigger2), Some(GamepadButton::LeftTrigger));
        assert_eq!(GilrsBackend::map_button(G::RightTrigger2), Some(GamepadButton::RightTrigger));
        assert_eq!(GilrsBackend::map_button(G::DPadUp), Some(GamepadButton::DPadUp));
        // Unknown 应返回 None。
        assert_eq!(GilrsBackend::map_button(G::Unknown), None);
    }

    #[test]
    fn map_axis_covers_dual_stick_and_triggers() {
        use gilrs::Axis as A;
        assert_eq!(GilrsBackend::map_axis(A::LeftStickX), Some(GamepadAxis::LeftStickX));
        assert_eq!(GilrsBackend::map_axis(A::RightStickY), Some(GamepadAxis::RightStickY));
        assert_eq!(GilrsBackend::map_axis(A::LeftZ), Some(GamepadAxis::LeftZ));
        assert_eq!(GilrsBackend::map_axis(A::Unknown), None);
    }
}
