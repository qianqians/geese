//! 资源元数据（.meta 伴生文件）。
//!
//! 每个 `assets/` 下的资源文件对应一个 `.meta` JSON 文件，存储 UUID、资源类型、依赖列表。

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// 资源元数据——每个 assets/ 下的文件对应一个 `.meta` 文件。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssetMeta {
    /// 格式版本，当前 = 1
    pub version: u32,
    /// UUID v4 全局唯一标识
    pub uuid: String,
    /// 资源类型（从扩展名推断，可手动覆盖）
    pub asset_type: AssetTypeKind,
    /// 对其他资产文件的依赖（UUID 列表）
    #[serde(default)]
    pub dependencies: Vec<String>,
    /// 导入设置（预留扩展）
    #[serde(default)]
    pub import_settings: serde_json::Value,
}

/// 资源类型枚举。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssetTypeKind {
    /// 3D 模型：.gltf, .glb
    Model,
    /// 纹理：.png, .jpg, .hdr, .exr, .ktx2
    Texture,
    /// 音频：.wav, .ogg, .mp3, .flac
    Audio,
    /// 场景清单：.scene.json
    Scene,
    /// 骨骼动画清单：.avatar.json
    Avatar,
    /// 材质（未来）：.material.json
    Material,
    /// 其他
    Other,
}

impl AssetTypeKind {
    /// 从文件扩展名推断资源类型。
    pub fn from_extension(ext: &str) -> Self {
        match ext.to_lowercase().as_str() {
            "gltf" | "glb" => AssetTypeKind::Model,
            "png" | "jpg" | "jpeg" | "hdr" | "exr" | "ktx2" | "bmp" | "tga" => {
                AssetTypeKind::Texture
            }
            "wav" | "ogg" | "mp3" | "flac" => AssetTypeKind::Audio,
            _ => AssetTypeKind::Other,
        }
    }

    /// 从文件名推断资源类型（支持复合后缀如 `.scene.json`、`.avatar.json`）。
    pub fn from_filename(name: &str) -> Self {
        let lower = name.to_lowercase();
        if lower.ends_with(".scene.json") {
            AssetTypeKind::Scene
        } else if lower.ends_with(".avatar.json") {
            AssetTypeKind::Avatar
        } else if lower.ends_with(".material.json") {
            AssetTypeKind::Material
        } else if let Some(ext) = Path::new(name).extension().and_then(|e| e.to_str()) {
            Self::from_extension(ext)
        } else {
            AssetTypeKind::Other
        }
    }
}

/// Meta 文件读写错误。
#[derive(Debug)]
pub enum MetaError {
    Io(std::io::Error),
    Parse(serde_json::Error),
}

impl std::fmt::Display for MetaError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MetaError::Io(e) => write!(f, "meta IO error: {e}"),
            MetaError::Parse(e) => write!(f, "meta parse error: {e}"),
        }
    }
}

impl std::error::Error for MetaError {}

impl From<std::io::Error> for MetaError {
    fn from(e: std::io::Error) -> Self {
        MetaError::Io(e)
    }
}

impl From<serde_json::Error> for MetaError {
    fn from(e: serde_json::Error) -> Self {
        MetaError::Parse(e)
    }
}

/// 返回给定资产路径对应的 meta 文件路径。
///
/// 例如 `assets/models/hero.glb` → `assets/models/hero.glb.meta`
pub fn meta_path_for(asset_path: &Path) -> PathBuf {
    let mut meta_path = asset_path.as_os_str().to_owned();
    meta_path.push(".meta");
    PathBuf::from(meta_path)
}

/// 从文件扩展名推断资源类型。
pub fn infer_asset_type(path: &Path) -> AssetTypeKind {
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("");
    AssetTypeKind::from_filename(name)
}

/// 读取 .meta 文件。
pub fn read_meta(path: &Path) -> Result<AssetMeta, MetaError> {
    let content = std::fs::read_to_string(path)?;
    let meta: AssetMeta = serde_json::from_str(&content)?;
    Ok(meta)
}

/// 写入 .meta 文件。
pub fn write_meta(path: &Path, meta: &AssetMeta) -> Result<(), MetaError> {
    let json = serde_json::to_string_pretty(meta)?;
    std::fs::write(path, json)?;
    Ok(())
}

/// 为指定资源文件创建新的 .meta 并写入磁盘。
pub fn create_meta_for(asset_path: &Path) -> Result<AssetMeta, MetaError> {
    let meta = AssetMeta {
        version: 1,
        uuid: uuid::Uuid::new_v4().to_string(),
        asset_type: infer_asset_type(asset_path),
        dependencies: Vec::new(),
        import_settings: serde_json::Value::Null,
    };
    let meta_path = meta_path_for(asset_path);
    write_meta(&meta_path, &meta)?;
    Ok(meta)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn infer_model_type() {
        assert_eq!(infer_asset_type(Path::new("hero.glb")), AssetTypeKind::Model);
        assert_eq!(infer_asset_type(Path::new("hero.gltf")), AssetTypeKind::Model);
    }

    #[test]
    fn infer_texture_type() {
        assert_eq!(infer_asset_type(Path::new("diffuse.png")), AssetTypeKind::Texture);
        assert_eq!(infer_asset_type(Path::new("env.hdr")), AssetTypeKind::Texture);
    }

    #[test]
    fn infer_scene_from_compound_extension() {
        assert_eq!(infer_asset_type(Path::new("my.scene.json")), AssetTypeKind::Scene);
        assert_eq!(infer_asset_type(Path::new("hero.avatar.json")), AssetTypeKind::Avatar);
    }

    #[test]
    fn meta_path_appends_suffix() {
        let p = meta_path_for(Path::new("assets/models/hero.glb"));
        assert_eq!(p, PathBuf::from("assets/models/hero.glb.meta"));
    }

    #[test]
    fn roundtrip_meta() {
        let dir = std::env::temp_dir().join("asset_meta_test");
        let _ = std::fs::create_dir_all(&dir);
        let meta_path = dir.join("test.meta");

        let meta = AssetMeta {
            version: 1,
            uuid: "abc-123".into(),
            asset_type: AssetTypeKind::Model,
            dependencies: vec!["dep-1".into()],
            import_settings: serde_json::Value::Null,
        };
        write_meta(&meta_path, &meta).unwrap();
        let loaded = read_meta(&meta_path).unwrap();
        assert_eq!(loaded.uuid, "abc-123");
        assert_eq!(loaded.asset_type, AssetTypeKind::Model);
        assert_eq!(loaded.dependencies, vec!["dep-1"]);

        let _ = std::fs::remove_dir_all(&dir);
    }
}
