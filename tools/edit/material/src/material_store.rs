//! `.material.json` 的读写封装。
//!
//! 材质数据模型与序列化全部直接调用 `material` crate，本模块只负责：
//! 列出项目目录下的材质文件、按文件物化为完整 `Material`、以及把编辑结果落盘。

use std::fs;
use std::path::{Path, PathBuf};

use material::{
    ensure_default_material_files, load_material_file, AlphaMode, AlphaModeName, AlphaSettings,
    Material, MaterialAsset, MaterialAssetError, MATERIAL_EXTENSION, MATERIAL_FORMAT_VERSION,
    MATERIALS_DIR,
};

/// 一个已加载的材质文件。
pub struct LoadedMaterial {
    /// 文件名（含 `.material.json` 后缀）。
    pub file_name: String,
    /// 物化后的完整材质参数（编辑器编辑的就是这份完整定义）。
    pub material: Material,
}

/// 材质读写错误。
#[derive(Debug)]
pub enum StoreError {
    Io(std::io::Error),
    Json(serde_json::Error),
    Asset(MaterialAssetError),
}

impl std::fmt::Display for StoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StoreError::Io(e) => write!(f, "IO 错误: {e}"),
            StoreError::Json(e) => write!(f, "JSON 序列化错误: {e}"),
            StoreError::Asset(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for StoreError {}

/// 某个项目目录下的材质集合。
pub struct MaterialStore {
    materials_dir: PathBuf,
    loaded: Vec<LoadedMaterial>,
}

impl MaterialStore {
    /// 打开项目目录：首次运行补齐内置默认材质，然后加载所有 `.material.json`。
    ///
    /// 单个文件解析失败时记录日志并跳过，其余文件照常加载。
    pub fn open(project_dir: &Path) -> Result<Self, StoreError> {
        ensure_default_material_files(project_dir).map_err(StoreError::Asset)?;

        let dir = project_dir.join(MATERIALS_DIR);
        let mut files = collect_material_files(&dir);
        files.sort();

        let mut loaded = Vec::with_capacity(files.len());
        for path in files {
            match load_material_file(&path) {
                Ok(material) => {
                    let file_name = path
                        .file_name()
                        .and_then(|name| name.to_str())
                        .unwrap_or("unknown.material.json")
                        .to_string();
                    loaded.push(LoadedMaterial {
                        file_name,
                        material,
                    });
                }
                Err(err) => {
                    log::error!("[material-editor] 跳过 {}: {err}", path.display());
                }
            }
        }

        Ok(Self {
            materials_dir: dir,
            loaded,
        })
    }

    pub fn materials(&self) -> &[LoadedMaterial] {
        &self.loaded
    }

    pub fn material_mut(&mut self, index: usize) -> Option<&mut Material> {
        self.loaded.get_mut(index).map(|l| &mut l.material)
    }

    /// 追加一个新材质（不落盘）。
    pub fn push(&mut self, loaded: LoadedMaterial) {
        self.loaded.push(loaded);
    }

    /// 把材质落盘为 `<name>.material.json`。
    ///
    /// 序列化完整 `MaterialAsset`（所有字段显式写出），与 `material::asset` 的
    /// `deny_unknown_fields` 兼容。
    pub fn save(&self, material: &Material) -> Result<(), StoreError> {
        let asset = to_asset(material);
        let file_name = format!("{}.material.json", asset.name);
        let path = self.materials_dir.join(&file_name);
        let json = serde_json::to_string_pretty(&asset).map_err(StoreError::Json)?;
        fs::write(&path, json).map_err(StoreError::Io)?;
        Ok(())
    }
}

/// `Material` → 完整 `MaterialAsset`（版本 + 所有标量字段）。
fn to_asset(material: &Material) -> MaterialAsset {
    MaterialAsset {
        version: MATERIAL_FORMAT_VERSION,
        name: material
            .name
            .clone()
            .unwrap_or_else(|| "default".to_string()),
        base_color: Some(material.base_color_factor),
        metallic: Some(material.metallic_factor),
        roughness: Some(material.roughness_factor),
        emissive: Some(material.emissive_factor),
        normal_scale: Some(material.normal_scale),
        occlusion_strength: Some(material.occlusion_strength),
        alpha: Some(AlphaSettings {
            mode: match material.alpha_mode {
                AlphaMode::Opaque => AlphaModeName::Opaque,
                AlphaMode::Mask => AlphaModeName::Mask,
                AlphaMode::Blend => AlphaModeName::Blend,
            },
            cutoff: Some(material.alpha_cutoff),
        }),
        double_sided: Some(material.double_sided),
    }
}

fn collect_material_files(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_file())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.ends_with(MATERIAL_EXTENSION))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use material::AlphaMode;

    #[test]
    fn to_asset_roundtrips_scalar_params() {
        let mut material = Material::default();
        material.name = Some("test".into());
        material.base_color_factor = [0.1, 0.2, 0.3, 0.4];
        material.metallic_factor = 0.5;
        material.roughness_factor = 0.6;
        material.emissive_factor = [0.7, 0.8, 0.9];
        material.normal_scale = 1.5;
        material.occlusion_strength = 0.25;
        material.alpha_mode = AlphaMode::Mask;
        material.alpha_cutoff = 0.33;
        material.double_sided = true;

        let asset = to_asset(&material);
        assert_eq!(asset.version, MATERIAL_FORMAT_VERSION);
        assert_eq!(asset.name, "test");
        assert_eq!(asset.base_color, Some([0.1, 0.2, 0.3, 0.4]));
        assert_eq!(asset.metallic, Some(0.5));
        assert_eq!(asset.alpha.unwrap().cutoff, Some(0.33));
        assert_eq!(asset.double_sided, Some(true));
    }

    #[test]
    fn to_asset_matches_default_name() {
        let asset = to_asset(&Material::default());
        assert_eq!(asset.name, "default");
    }
}
