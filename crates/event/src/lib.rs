//! 事件组件系统。
//!
//! 提供：
//! - `EventEntryDef`：事件条目（触发函数 → 响应函数）
//! - `EventComponentDef`：实体事件组件定义（server/client enabled + entries 列表）
//!
//! 触发函数签名：`fn() -> bool`，返回 true 时触发响应。
//! 响应函数签名：`fn()`，无参数无返回值。

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

/// 单个事件条目：触发函数 + 响应函数。
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct EventEntryDef {
    /// 触发函数名 fn() -> bool
    pub trigger: String,
    /// 响应函数名 fn()
    pub response: String,
}

/// 实体事件组件定义。
///
/// 每个实体可添加一个 EventComponent，包含多个事件条目。
/// 每帧评估所有条目的触发函数，对返回 true 的条目执行其响应函数。
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct EventComponentDef {
    /// 服务器是否启用事件评估
    #[cfg_attr(feature = "serde", serde(default = "default_true"))]
    pub server_enabled: bool,
    /// 客户端是否启用事件评估
    #[cfg_attr(feature = "serde", serde(default = "default_true"))]
    pub client_enabled: bool,
    /// 事件条目列表
    #[cfg_attr(feature = "serde", serde(default))]
    pub entries: Vec<EventEntryDef>,
}

fn default_true() -> bool {
    true
}

impl Default for EventComponentDef {
    fn default() -> Self {
        Self {
            server_enabled: true,
            client_enabled: true,
            entries: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_component_has_empty_entries() {
        let def = EventComponentDef::default();
        assert!(def.server_enabled);
        assert!(def.client_enabled);
        assert!(def.entries.is_empty());
    }

    #[test]
    fn serialize_roundtrip() {
        let mut def = EventComponentDef::default();
        def.entries.push(EventEntryDef {
            trigger: "on_enter".into(),
            response: "open_door".into(),
        });
        def.entries.push(EventEntryDef {
            trigger: "on_pickup".into(),
            response: "add_score".into(),
        });

        let json = serde_json::to_string(&def).unwrap();
        let restored: EventComponentDef = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.server_enabled, def.server_enabled);
        assert_eq!(restored.client_enabled, def.client_enabled);
        assert_eq!(restored.entries.len(), 2);
        assert_eq!(restored.entries[0].trigger, "on_enter");
        assert_eq!(restored.entries[0].response, "open_door");
        assert_eq!(restored.entries[1].trigger, "on_pickup");
        assert_eq!(restored.entries[1].response, "add_score");
    }

    #[test]
    fn deserialize_minimal() {
        let json = r#"{"server_enabled":true,"client_enabled":true,"entries":[]}"#;
        let def: EventComponentDef = serde_json::from_str(json).unwrap();
        assert!(def.entries.is_empty());
    }

    #[test]
    fn deserialize_with_defaults() {
        // 空 JSON 对象应使用默认值
        let json = r#"{}"#;
        let def: EventComponentDef = serde_json::from_str(json).unwrap();
        assert!(def.server_enabled);
        assert!(def.client_enabled);
        assert!(def.entries.is_empty());
    }
}
