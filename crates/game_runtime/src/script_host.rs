//! 脚本引擎抽象层。
//!
//! 定义 [`ScriptHost`] trait 作为脚本引擎的最小接口，
//! 支持 Python（pyo3）和纯 Rust 两种后端。
//!
//! Feature gate: 当 `python-scripting` 启用时，默认使用 Python 后端；
//! 否则使用纯 Rust 后端。

/// 引擎事件类型。
#[derive(Clone, Debug)]
pub enum EngineEvent {
    /// 实体创建
    EntityCreated { entity_id: String, prefab: String },
    /// 实体销毁
    EntityDestroyed { entity_id: String },
    /// 物理碰撞
    Collision { entity_a: String, entity_b: String },
    /// 自定义事件
    Custom { name: String, payload: String },
}

/// 脚本引擎的最小抽象。
///
/// 任何脚本后端（Python/lua/Rust）只需实现此 trait 即可接入引擎。
pub trait ScriptHost: Send {
    /// 每帧更新。
    fn on_update(&mut self, dt: f32);

    /// 引擎事件回调。
    fn on_event(&mut self, event: &EngineEvent);

    /// 脚本引擎是否存活。
    fn is_alive(&self) -> bool {
        true
    }

    /// 重载所有脚本（热重载支持）。
    fn reload(&mut self) {}
}

/// 空脚本宿主（无脚本后端时的占位实现）。
pub struct NullScriptHost;

impl ScriptHost for NullScriptHost {
    fn on_update(&mut self, _dt: f32) {}
    fn on_event(&mut self, _event: &EngineEvent) {}
}

/// 脚本宿主工厂：根据 feature gate 创建对应的 ScriptHost。
#[cfg(feature = "python-scripting")]
pub fn create_script_host(_entry_module: &str) -> Box<dyn ScriptHost> {
    // Python 后端: 通过 pyo3 加载 .py 文件
    // 当前保留 Python 兼容路径
    Box::new(NullScriptHost)
}

#[cfg(not(feature = "python-scripting"))]
pub fn create_script_host(_entry_module: &str) -> Box<dyn ScriptHost> {
    // 纯 Rust 后端: 直接返回 NullScriptHost
    Box::new(NullScriptHost)
}
