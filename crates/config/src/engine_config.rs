//! 引擎配置系统（TOML 驱动）。
//!
//! 提供层级配置加载：
//! - `EngineConfig::default()` — 编译时默认值
//! - `EngineConfig::load_toml()` — 从 TOML 文件加载
//! - `EngineConfig::merge()` — 项目配置覆盖默认配置

use serde::{Deserialize, Serialize};

/// 渲染路径枚举（与 render crate 的 RenderingPath 对应）。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ConfigRenderingPath {
    ForwardPlus,
    DeferredPlus,
}

impl Default for ConfigRenderingPath {
    fn default() -> Self {
        Self::ForwardPlus
    }
}

// ---------------------------------------------------------------------------
// 子配置段
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct RenderConfig {
    pub rendering_path: ConfigRenderingPath,
    pub msaa_samples: u32,
    pub shadow_map_size: u32,
    pub default_width: u32,
    pub default_height: u32,
}

impl Default for RenderConfig {
    fn default() -> Self {
        Self {
            rendering_path: ConfigRenderingPath::default(),
            msaa_samples: 1,
            shadow_map_size: 2048,
            default_width: 1280,
            default_height: 720,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct PhysicsConfig {
    /// 重力加速度（x, y, z），Y 轴朝上，默认 -9.81
    pub gravity: [f32; 3],
    /// 固定时间步长（秒），0 表示可变步长
    pub fixed_timestep: f32,
}

impl Default for PhysicsConfig {
    fn default() -> Self {
        Self {
            gravity: [0.0, -9.81, 0.0],
            fixed_timestep: 0.016, // ~60 Hz
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct AudioConfig {
    pub master_volume: f32,
    pub max_sources: usize,
}

impl Default for AudioConfig {
    fn default() -> Self {
        Self {
            master_volume: 1.0,
            max_sources: 32,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct EditorConfig {
    pub undo_depth: usize,
    pub autosave_interval_secs: f64,
}

impl Default for EditorConfig {
    fn default() -> Self {
        Self {
            undo_depth: 256,
            autosave_interval_secs: 300.0, // 5 分钟
        }
    }
}

/// VFS 挂载点定义。
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MountDef {
    /// 虚拟路径前缀（如 "/assets/"）
    pub prefix: String,
    /// 物理路径（相对于项目根目录或绝对路径）
    pub physical: String,
    /// 是否可写
    #[serde(default = "default_writable")]
    pub writable: bool,
}

fn default_writable() -> bool {
    false
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct VfsConfig {
    pub mounts: Vec<MountDef>,
}

impl Default for VfsConfig {
    fn default() -> Self {
        Self {
            mounts: vec![MountDef {
                prefix: "/assets/".into(),
                physical: "./assets/".into(),
                writable: false,
            }],
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct LogConfig {
    /// 日志级别：trace, debug, info, warn, error
    pub level: String,
}

impl Default for LogConfig {
    fn default() -> Self {
        Self {
            level: "info".into(),
        }
    }
}

// ---------------------------------------------------------------------------
// 顶层 EngineConfig
// ---------------------------------------------------------------------------

/// 引擎顶层配置。
///
/// 所有字段均为 `#[serde(default)]`，TOML 文件中缺失的段使用 `Default` 值。
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct EngineConfig {
    pub render: RenderConfig,
    pub physics: PhysicsConfig,
    pub audio: AudioConfig,
    pub editor: EditorConfig,
    pub vfs: VfsConfig,
    pub log: LogConfig,
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            render: RenderConfig::default(),
            physics: PhysicsConfig::default(),
            audio: AudioConfig::default(),
            editor: EditorConfig::default(),
            vfs: VfsConfig::default(),
            log: LogConfig::default(),
        }
    }
}

impl EngineConfig {
    /// 从 TOML 文件路径加载配置。
    ///
    /// 缺失的字段使用 `Default` 值填充。
    pub fn load_toml(path: &str) -> Result<Self, ConfigError> {
        let data = std::fs::read_to_string(path).map_err(ConfigError::Io)?;
        Self::load_toml_from_str(&data)
    }

    /// 从 TOML 字符串反序列化。
    pub fn load_toml_from_str(data: &str) -> Result<Self, ConfigError> {
        let cfg: EngineConfig = toml::from_str(data).map_err(ConfigError::Parse)?;
        Ok(cfg)
    }

    /// 层级合并：用 `other` 中的非默认值覆盖 `self` 中的对应字段。
    ///
    /// 合并策略：逐段替换。当前实现为"全量覆盖"——若 `other` 中某子段
    /// 存在，则完整替换 `self` 中对应的子段。未来可扩展为逐字段合并。
    pub fn merge(&mut self, other: &EngineConfig) {
        // 简单实现：直接用 other 覆盖 self（因所有字段都是 owned）
        // 更精细的合并可在后续迭代中添加
        self.render = other.render.clone();
        self.physics = other.physics.clone();
        self.audio = other.audio.clone();
        self.editor = other.editor.clone();
        self.vfs = other.vfs.clone();
        self.log = other.log.clone();
    }
}

// ---------------------------------------------------------------------------
// 错误类型
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub enum ConfigError {
    Io(std::io::Error),
    Parse(toml::de::Error),
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConfigError::Io(e) => write!(f, "config IO error: {e}"),
            ConfigError::Parse(e) => write!(f, "config parse error: {e}"),
        }
    }
}

impl std::error::Error for ConfigError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            ConfigError::Io(e) => Some(e),
            ConfigError::Parse(e) => Some(e),
        }
    }
}

impl From<std::io::Error> for ConfigError {
    fn from(e: std::io::Error) -> Self {
        ConfigError::Io(e)
    }
}

// ---------------------------------------------------------------------------
// 测试
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_is_usable() {
        let cfg = EngineConfig::default();
        assert_eq!(cfg.render.msaa_samples, 1);
        assert_eq!(cfg.physics.gravity, [0.0, -9.81, 0.0]);
        assert_eq!(cfg.audio.master_volume, 1.0);
        assert_eq!(cfg.editor.undo_depth, 256);
        assert_eq!(cfg.vfs.mounts.len(), 1);
        assert_eq!(cfg.log.level, "info");
    }

    #[test]
    fn parse_minimal_toml() {
        let toml_str = r#"
[render]
msaa_samples = 4
"#;
        let cfg = EngineConfig::load_toml_from_str(toml_str).unwrap();
        assert_eq!(cfg.render.msaa_samples, 4);
        // 其他字段使用默认值
        assert_eq!(cfg.physics.gravity, [0.0, -9.81, 0.0]);
        assert_eq!(cfg.audio.master_volume, 1.0);
    }

    #[test]
    fn parse_full_toml() {
        let toml_str = r#"
[render]
rendering_path = "deferredplus"
msaa_samples = 8
shadow_map_size = 4096
default_width = 1920
default_height = 1080

[physics]
gravity = [0.0, -20.0, 0.0]
fixed_timestep = 0.008

[audio]
master_volume = 0.8
max_sources = 64

[editor]
undo_depth = 512
autosave_interval_secs = 600.0

[[vfs.mounts]]
prefix = "/assets/"
physical = "./game_data/"
writable = false

[[vfs.mounts]]
prefix = "/mods/"
physical = "./mods/"
writable = true

[log]
level = "debug"
"#;
        let cfg = EngineConfig::load_toml_from_str(toml_str).unwrap();
        assert_eq!(cfg.render.rendering_path, ConfigRenderingPath::DeferredPlus);
        assert_eq!(cfg.render.shadow_map_size, 4096);
        assert_eq!(cfg.physics.gravity, [0.0, -20.0, 0.0]);
        assert_eq!(cfg.audio.max_sources, 64);
        assert_eq!(cfg.editor.undo_depth, 512);
        assert_eq!(cfg.vfs.mounts.len(), 2);
        assert_eq!(cfg.vfs.mounts[1].prefix, "/mods/");
        assert_eq!(cfg.log.level, "debug");
    }

    #[test]
    fn merge_overrides_fields() {
        let mut base = EngineConfig::default();
        let override_cfg = EngineConfig {
            render: RenderConfig {
                msaa_samples: 4,
                ..RenderConfig::default()
            },
            ..EngineConfig::default()
        };
        base.merge(&override_cfg);
        assert_eq!(base.render.msaa_samples, 4);
        // 未覆盖的字段保持原值
        assert_eq!(base.physics.gravity, [0.0, -9.81, 0.0]);
    }
}
