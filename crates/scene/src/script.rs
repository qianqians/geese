//! 场景 Python 脚本绑定。
//!
//! - [`ScriptComponent`]: 脚本组件数据（路径、类名、属性），无 pyo3 依赖即可序列化。
//! - [`ScriptSystem`]（feature-gated `python_script`）: PyO3 脚本引擎，调用 `on_update(dt)`。
//!
//! 设计:
//! - 默认不启用 pyo3。`ScriptComponent` 数据结构无 pyo3 依赖即可使用。
//! - `ScriptSystem` 在 `Scene::scripts` 上操作，不侵入 `SceneObject`。

use std::collections::HashMap;

// ---------------------------------------------------------------------------
// ScriptComponent — 始终可用（无 pyo3 依赖）
// ---------------------------------------------------------------------------

/// 脚本组件数据。
///
/// 无 pyo3 依赖即可使用（序列化/反序列化），适合编辑器存储和场景序列化。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ScriptComponent {
    /// 脚本文件路径（如 `scripts/player.py`）
    pub path: String,
    /// Python 类名（如 `PlayerController`）
    pub class_name: String,
    /// 脚本属性（键值对，序列化为 JSON）
    #[serde(default)]
    pub properties: HashMap<String, serde_json::Value>,
}

impl ScriptComponent {
    pub fn new(path: impl Into<String>, class_name: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            class_name: class_name.into(),
            properties: HashMap::new(),
        }
    }

    /// 设置脚本属性。
    pub fn set_property(&mut self, key: impl Into<String>, value: serde_json::Value) {
        self.properties.insert(key.into(), value);
    }

    /// 检查是否注册了事件处理器。
    /// 默认返回 false；子类或外部可覆盖以提供实际事件处理能力。
    pub fn has_event_handler(&self) -> bool {
        false
    }

    /// 评估事件触发器，返回触发的 response 名称列表。
    /// 默认返回空；子类或外部可覆盖以提供实际事件触发逻辑。
    pub fn evaluate_triggers(&self) -> Vec<String> {
        Vec::new()
    }
}

impl Default for ScriptComponent {
    fn default() -> Self {
        Self::new("", "Script")
    }
}

// ---------------------------------------------------------------------------
// ScriptSystem — 仅在 python_script feature 下可用
// ---------------------------------------------------------------------------

#[cfg(feature = "python_script")]
pub mod python_script {
    use super::ScriptComponent;
    use pyo3::prelude::*;
    use pyo3::types::PyModule;
    use std::collections::HashMap;
    use std::panic::{catch_unwind, AssertUnwindSafe};
    use std::sync::Mutex;

    /// Python 脚本系统。
    ///
    /// 在 `Scene::scripts` 上操作，不侵入 `SceneObject`。
    /// 每次调用用 `Python::with_gil` + `catch_unwind` 包裹，错误只 log。
    pub struct ScriptSystem {
        /// 缓存已加载的 Python 模块（path → module_name）
        loaded_modules: Mutex<HashMap<String, String>>,
    }

    impl ScriptSystem {
        /// 创建脚本系统。无 Python 环境返回 Err。
        pub fn try_new() -> Result<Self, String> {
            let result = catch_unwind(|| {
                Python::with_gil(|_py| {
                    // 能获取 GIL 说明 Python 环境可用
                });
            });
            if result.is_err() {
                return Err("Python interpreter not available".to_string());
            }
            Ok(Self {
                loaded_modules: Mutex::new(HashMap::new()),
            })
        }

        /// 加载脚本文件，返回 `ScriptComponent`。
        pub fn load_script(&mut self, path: &str) -> Result<ScriptComponent, String> {
            let source =
                std::fs::read_to_string(path).map_err(|e| format!("Failed to read script '{path}': {e}"))?;

            let class_name = extract_class_name(&source).unwrap_or_else(|| "Script".to_string());

            let module_name = path_to_module_name(path);

            Python::with_gil(|py| {
                let module = PyModule::from_code(py, &source, path, &module_name)
                    .map_err(|e| format!("Failed to load Python module '{path}': {e}"))?;

                // 验证类存在
                let _class = module
                    .getattr(&class_name)
                    .map_err(|e| format!("Class '{class_name}' not found in '{path}': {e}"))?;

                self.loaded_modules
                    .lock()
                    .map_err(|e| format!("Lock error: {e}"))?
                    .insert(path.to_string(), module_name);

                Ok::<_, String>(())
            })?;

            Ok(ScriptComponent::new(path, class_name))
        }

        /// 每帧调用所有脚本的 `on_update(dt)`。
        ///
        /// 错误只 log，不中断其他脚本。
        pub fn tick(&mut self, dt: f32, scripts: &HashMap<String, ScriptComponent>) {
            for (entity_id, script) in scripts {
                let result = catch_unwind(AssertUnwindSafe(|| {
                    Python::with_gil(|py| {
                        let source = match std::fs::read_to_string(&script.path) {
                            Ok(s) => s,
                            Err(e) => {
                                log::warn!(
                                    "[ScriptSystem] Cannot read script '{}': {}",
                                    script.path,
                                    e
                                );
                                return Ok::<_, PyErr>(());
                            }
                        };

                        let module_name = path_to_module_name(&script.path);

                        let module = match PyModule::from_code(
                            py,
                            &source,
                            &script.path,
                            &module_name,
                        ) {
                            Ok(m) => m,
                            Err(e) => {
                                log::error!(
                                    "[ScriptSystem] Failed to load module '{}' for entity {}: {}",
                                    script.path,
                                    entity_id,
                                    e
                                );
                                return Ok::<_, PyErr>(());
                            }
                        };

                        let class = match module.getattr(&script.class_name) {
                            Ok(c) => c,
                            Err(e) => {
                                log::error!(
                                    "[ScriptSystem] Class '{}' not found in '{}' for entity {}: {}",
                                    script.class_name,
                                    script.path,
                                    entity_id,
                                    e
                                );
                                return Ok::<_, PyErr>(());
                            }
                        };

                        let instance = match class.call0() {
                            Ok(i) => i,
                            Err(e) => {
                                log::error!(
                                    "[ScriptSystem] Failed to instantiate '{}' for entity {}: {}",
                                    script.class_name,
                                    entity_id,
                                    e
                                );
                                return Ok::<_, PyErr>(());
                            }
                        };

                        // 调用 on_update(dt) 如果方法存在
                        if let Ok(true) = instance.hasattr("on_update") {
                            if let Err(e) = instance.call_method1("on_update", (dt,)) {
                                log::error!(
                                    "[ScriptSystem] on_update error for entity {}: {}",
                                    entity_id,
                                    e
                                );
                            }
                        }

                        Ok::<_, PyErr>(())
                    })
                }));

                if let Err(e) = result {
                    log::error!(
                        "[ScriptSystem] Panic in script for entity {}: {:?}",
                        entity_id,
                        e
                    );
                }
            }
        }
    }

    /// 从 Python 源码中提取第一个 `class XXX(` 的类名。
    fn extract_class_name(source: &str) -> Option<String> {
        for line in source.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("class ") {
                let after_class = &trimmed[6..];
                let class_end = after_class
                    .find(|c: char| c == '(' || c == ':' || c.is_whitespace())?;
                let name = after_class[..class_end].trim();
                if !name.is_empty() {
                    return Some(name.to_string());
                }
            }
        }
        None
    }

    /// 将文件路径转换为合法的 Python 模块名。
    fn path_to_module_name(path: &str) -> String {
        let stem = std::path::Path::new(path)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("script");
        stem.replace('-', "_")
            .replace('.', "_")
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() || c == '_' { c } else { '_' })
            .collect()
    }
}

#[cfg(feature = "python_script")]
pub use python_script::ScriptSystem;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn script_component_new() {
        let sc = ScriptComponent::new("scripts/player.py", "PlayerController");
        assert_eq!(sc.path, "scripts/player.py");
        assert_eq!(sc.class_name, "PlayerController");
        assert!(sc.properties.is_empty());
    }

    #[test]
    fn script_component_set_property() {
        let mut sc = ScriptComponent::new("test.py", "Test");
        sc.set_property("speed", serde_json::json!(3.14));
        assert_eq!(sc.properties.get("speed").unwrap(), &serde_json::json!(3.14));
    }

    #[test]
    fn script_component_serialize_deserialize() {
        let mut sc = ScriptComponent::new("scripts/npc.py", "NPC");
        sc.set_property("health", serde_json::json!(100));
        sc.set_property("name", serde_json::json!("Guard"));

        let json = serde_json::to_string(&sc).unwrap();
        let deserialized: ScriptComponent = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.path, "scripts/npc.py");
        assert_eq!(deserialized.class_name, "NPC");
        assert_eq!(deserialized.properties.len(), 2);
        assert_eq!(deserialized.properties.get("health").unwrap(), &serde_json::json!(100));
    }

    #[test]
    fn script_component_default() {
        let sc = ScriptComponent::default();
        assert_eq!(sc.path, "");
        assert_eq!(sc.class_name, "Script");
    }
}
