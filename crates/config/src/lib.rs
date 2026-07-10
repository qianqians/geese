//! 引擎配置系统。
//!
//! 提供两种加载路径：
//! - 旧版 JSON 路径：`load_data_from_file()` / `load_cfg_from_data()`（保留兼容）
//! - 新版 TOML 路径：`EngineConfig::load_toml()` / `EngineConfig::load_toml_from_str()`

pub mod engine_config;

pub use engine_config::{
    AudioConfig, ConfigError, ConfigRenderingPath, EditorConfig, EngineConfig, LogConfig,
    MountDef, PhysicsConfig, RenderConfig, VfsConfig,
};

use serde::Deserialize;
use std::fs;
use std::io;

/// 从文件读取原始数据（JSON 兼容路径，已废弃）。
#[deprecated(since = "0.2.0", note = "use EngineConfig::load_toml() instead")]
pub fn load_data_from_file(cfg_file: String) -> Result<String, io::Error> {
    let data = fs::read_to_string(cfg_file)?;
    Ok(data)
}

/// 从 JSON 字符串反序列化（JSON 兼容路径，已废弃）。
#[deprecated(since = "0.2.0", note = "use EngineConfig::load_toml_from_str() instead")]
pub fn load_cfg_from_data<'a, C: Deserialize<'a>>(data: &'a str) -> Result<C, io::Error> {
    let cfg: C = serde_json::from_str::<C>(data)?;
    Ok(cfg)
}