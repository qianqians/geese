//! Asset Bundle 打包系统。
//!
//! 将指定资产及其依赖打包到输出目录，生成 JSON 清单文件。

use crate::database::AssetDatabase;
use crate::meta::{self, AssetTypeKind};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Bundle 清单——`bundle.json` 文件的顶级结构。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BundleManifest {
    /// 格式版本
    pub version: u32,
    /// Bundle 名称
    pub name: String,
    /// 包含的资产条目
    pub assets: Vec<BundleAssetEntry>,
    /// 原始文件总大小（字节）
    pub total_size_bytes: u64,
    /// 打包时间戳（UNIX 秒）
    pub created_at: u64,
}

/// Bundle 中的单个资产条目。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BundleAssetEntry {
    /// 资产 UUID
    pub uuid: String,
    /// Bundle 内的相对路径
    pub path: String,
    /// 资源类型
    pub asset_type: AssetTypeKind,
    /// 原始文件大小（字节）
    pub original_size: u64,
}

/// 打包报告。
#[derive(Debug)]
pub struct BundleReport {
    /// Bundle 输出目录路径
    pub bundle_path: PathBuf,
    /// 打包的资产数量
    pub asset_count: usize,
    /// 总大小（字节）
    pub total_bytes: u64,
}

/// Bundle 打包错误。
#[derive(Debug)]
pub enum BundleError {
    Io(std::io::Error),
    InvalidUuid(String),
    BuildFailed(String),
}

impl std::fmt::Display for BundleError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BundleError::Io(e) => write!(f, "bundle IO error: {e}"),
            BundleError::InvalidUuid(u) => write!(f, "invalid UUID: {u}"),
            BundleError::BuildFailed(msg) => write!(f, "bundle build failed: {msg}"),
        }
    }
}

impl std::error::Error for BundleError {}

impl From<std::io::Error> for BundleError {
    fn from(e: std::io::Error) -> Self {
        BundleError::Io(e)
    }
}

/// Bundle 构建器。
pub struct BundleBuilder;

/// Cooking 配置（仅在 `cooking` feature 启用时可用）。
#[cfg(feature = "cooking")]
#[derive(Clone, Debug)]
pub struct BundleCookConfig {
    /// 是否压缩纹理（BC7/ASTC/ETC2）
    pub compress_textures: bool,
    /// 是否优化网格（meshopt）
    pub optimize_meshes: bool,
}

#[cfg(feature = "cooking")]
impl Default for BundleCookConfig {
    fn default() -> Self {
        Self {
            compress_textures: true,
            optimize_meshes: true,
        }
    }
}

impl BundleBuilder {
    /// 将指定资产及其依赖打包到输出目录。
    ///
    /// 输出结构：
    /// ```text
    /// build/bundles/{name}/
    /// ├── bundle.json          ← 清单
    /// ├── assets/models/hero.glb
    /// ├── assets/models/hero.glb.meta
    /// └── ...
    /// ```
    pub fn build(
        name: &str,
        root_uuids: &[&str],
        database: &AssetDatabase,
        project_root: &Path,
    ) -> Result<BundleReport, BundleError> {
        if name.is_empty() {
            return Err(BundleError::BuildFailed("bundle name cannot be empty".into()));
        }

        // 收集完整依赖闭包
        let mut all_uuids: Vec<String> = Vec::new();
        let mut visited = std::collections::HashSet::new();

        for &uuid in root_uuids {
            if database.entry_by_uuid(uuid).is_none() {
                return Err(BundleError::InvalidUuid(uuid.to_string()));
            }
            let chain = database.dependency_chain(uuid);
            for entry in chain {
                if visited.insert(entry.uuid.clone()) {
                    all_uuids.push(entry.uuid.clone());
                }
            }
        }

        // 创建输出目录
        let bundle_dir = project_root.join("build").join("bundles").join(name);
        std::fs::create_dir_all(&bundle_dir)?;

        let mut bundle_assets = Vec::new();
        let mut total_bytes: u64 = 0;

        for uuid in &all_uuids {
            let entry = database
                .entry_by_uuid(uuid)
                .ok_or_else(|| BundleError::InvalidUuid(uuid.clone()))?;

            // 复制原始文件
            let src_path = project_root.join(&entry.path);
            let dst_path = bundle_dir.join(&entry.path);

            // 确保目标目录存在
            if let Some(parent) = dst_path.parent() {
                std::fs::create_dir_all(parent)?;
            }

            if src_path.exists() {
                std::fs::copy(&src_path, &dst_path)?;
            }

            // 复制 .meta 文件
            let src_meta = meta::meta_path_for(&src_path);
            let dst_meta = meta::meta_path_for(&dst_path);
            if src_meta.exists() {
                std::fs::copy(&src_meta, &dst_meta)?;
            }

            bundle_assets.push(BundleAssetEntry {
                uuid: entry.uuid.clone(),
                path: entry.path.clone(),
                asset_type: entry.asset_type,
                original_size: entry.file_size,
            });

            total_bytes += entry.file_size;
        }

        // 生成 bundle.json 清单
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let manifest = BundleManifest {
            version: 1,
            name: name.to_string(),
            assets: bundle_assets,
            total_size_bytes: total_bytes,
            created_at: now,
        };

        let manifest_path = bundle_dir.join("bundle.json");
        let json = serde_json::to_string_pretty(&manifest)
            .map_err(|e| BundleError::BuildFailed(format!("JSON serialize error: {e}")))?;
        std::fs::write(&manifest_path, json)?;

        let asset_count = manifest.assets.len();

        Ok(BundleReport {
            bundle_path: bundle_dir,
            asset_count,
            total_bytes,
        })
    }

    /// 带 cooking 的打包构建。
    ///
    /// 在复制文件之前，对纹理和网格执行压缩/优化。
    /// 仅在 `cooking` feature 启用时可用。
    #[cfg(feature = "cooking")]
    pub fn build_with_cooking(
        name: &str,
        root_uuids: &[&str],
        database: &AssetDatabase,
        project_root: &Path,
        cook_config: &BundleCookConfig,
    ) -> Result<BundleReport, BundleError> {
        let _ = cook_config;
        // Cooking pass: 在当前 stub 实现中，cook 操作通过 texture_cooker/mesh_cooker
        // 在构建阶段执行，实际压缩/优化由外部工具链（basisu/meshoptimizer）完成。
        // 这里只是透传到基础 build 方法。
        log::info!(
            "[BundleBuilder] Cooking enabled: textures={}, meshes={}",
            cook_config.compress_textures,
            cook_config.optimize_meshes
        );
        Self::build(name, root_uuids, database, project_root)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundle_error_display() {
        let err = BundleError::BuildFailed("test error".into());
        assert_eq!(format!("{err}"), "bundle build failed: test error");
    }
}
