//! 依赖扫描器——解析资源文件内部引用，转化为 UUID 依赖列表。
//!
//! 支持的文件格式：
//! - `.gltf`：解析 JSON，提取 `buffers[].uri`、`images[].uri` 中的相对路径
//! - `.glb`：自包含二进制，无外部依赖
//! - `.scene.json`：提取 `models[].path`
//! - `.avatar.json`：提取 `gltf_path`
//! - 其他格式：无依赖

use std::collections::HashMap;
use std::path::Path;

/// 扫描单个资产文件的依赖，返回依赖的 UUID 列表。
///
/// `path_to_uuid` 用于将文件相对路径映射到 UUID。
pub fn scan_dependencies(
    asset_path: &Path,
    project_root: &Path,
    path_to_uuid: &HashMap<String, String>,
) -> Vec<String> {
    let file_name = asset_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("");

    let lower = file_name.to_lowercase();

    if lower.ends_with(".scene.json") {
        scan_scene_json_dependencies(asset_path, project_root, path_to_uuid)
    } else if lower.ends_with(".avatar.json") {
        scan_avatar_json_dependencies(asset_path, project_root, path_to_uuid)
    } else if lower.ends_with(".gltf") {
        scan_gltf_dependencies(asset_path, project_root, path_to_uuid)
    } else {
        // .glb、纹理、音频等无外部依赖
        Vec::new()
    }
}

/// 解析 .gltf 文件（JSON 格式），提取 buffers 和 images 的 URI 引用。
fn scan_gltf_dependencies(
    asset_path: &Path,
    project_root: &Path,
    path_to_uuid: &HashMap<String, String>,
) -> Vec<String> {
    let content = match std::fs::read_to_string(asset_path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("[dep_scan] Cannot read GLTF {}: {e}", asset_path.display());
            return Vec::new();
        }
    };

    let json: serde_json::Value = match serde_json::from_str(&content) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("[dep_scan] Cannot parse GLTF {}: {e}", asset_path.display());
            return Vec::new();
        }
    };

    let mut deps = Vec::new();
    let asset_dir = asset_path.parent().unwrap_or(Path::new(""));

    // 提取 buffers[].uri
    if let Some(buffers) = json.get("buffers").and_then(|b| b.as_array()) {
        for buffer in buffers {
            if let Some(uri) = buffer.get("uri").and_then(|u| u.as_str()) {
                if !uri.starts_with("data:") {
                    let dep_path = resolve_relative_path(asset_dir, uri, project_root);
                    if let Some(uuid) = path_to_uuid.get(&dep_path) {
                        deps.push(uuid.clone());
                    }
                }
            }
        }
    }

    // 提取 images[].uri
    if let Some(images) = json.get("images").and_then(|b| b.as_array()) {
        for image in images {
            if let Some(uri) = image.get("uri").and_then(|u| u.as_str()) {
                if !uri.starts_with("data:") {
                    let dep_path = resolve_relative_path(asset_dir, uri, project_root);
                    if let Some(uuid) = path_to_uuid.get(&dep_path) {
                        deps.push(uuid.clone());
                    }
                }
            }
        }
    }

    deps
}

/// 解析 .scene.json 文件，提取 models[].path 引用。
fn scan_scene_json_dependencies(
    asset_path: &Path,
    _project_root: &Path,
    path_to_uuid: &HashMap<String, String>,
) -> Vec<String> {
    let content = match std::fs::read_to_string(asset_path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("[dep_scan] Cannot read scene json {}: {e}", asset_path.display());
            return Vec::new();
        }
    };

    let json: serde_json::Value = match serde_json::from_str(&content) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("[dep_scan] Cannot parse scene json {}: {e}", asset_path.display());
            return Vec::new();
        }
    };

    let mut deps = Vec::new();

    if let Some(models) = json.get("models").and_then(|m| m.as_array()) {
        for model in models {
            if let Some(path) = model.get("path").and_then(|p| p.as_str()) {
                let normalized = path.replace('\\', "/");
                if let Some(uuid) = path_to_uuid.get(&normalized) {
                    deps.push(uuid.clone());
                }
            }
        }
    }

    deps
}

/// 解析 .avatar.json 文件，提取 gltf_path 引用。
fn scan_avatar_json_dependencies(
    asset_path: &Path,
    _project_root: &Path,
    path_to_uuid: &HashMap<String, String>,
) -> Vec<String> {
    let content = match std::fs::read_to_string(asset_path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("[dep_scan] Cannot read avatar json {}: {e}", asset_path.display());
            return Vec::new();
        }
    };

    let json: serde_json::Value = match serde_json::from_str(&content) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("[dep_scan] Cannot parse avatar json {}: {e}", asset_path.display());
            return Vec::new();
        }
    };

    let mut deps = Vec::new();

    if let Some(gltf_path) = json.get("gltf_path").and_then(|p| p.as_str()) {
        let normalized = gltf_path.replace('\\', "/");
        if let Some(uuid) = path_to_uuid.get(&normalized) {
            deps.push(uuid.clone());
        }
    }

    deps
}

/// 将相对路径解析为项目根目录下的标准化路径字符串。
fn resolve_relative_path(base_dir: &Path, relative: &str, project_root: &Path) -> String {
    let resolved = base_dir.join(relative);
    // 尝试标准化为相对于项目根的路径
    match resolved.strip_prefix(project_root) {
        Ok(rel) => rel.to_string_lossy().replace('\\', "/"),
        Err(_) => {
            // 如果无法相对于 project_root，尝试相对于 assets 目录
            let assets_dir = project_root.join("assets");
            match resolved.strip_prefix(&assets_dir) {
                Ok(rel) => format!("assets/{}", rel.to_string_lossy().replace('\\', "/")),
                Err(_) => resolved.to_string_lossy().replace('\\', "/"),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scan_no_dependencies_for_unknown_format() {
        let path_to_uuid = HashMap::new();
        let deps = scan_dependencies(
            Path::new("test.png"),
            Path::new("/project"),
            &path_to_uuid,
        );
        assert!(deps.is_empty());
    }

    #[test]
    fn scan_glb_has_no_dependencies() {
        let path_to_uuid = HashMap::new();
        let deps = scan_dependencies(
            Path::new("hero.glb"),
            Path::new("/project"),
            &path_to_uuid,
        );
        assert!(deps.is_empty());
    }

    #[test]
    fn resolve_relative_path_basic() {
        let base = Path::new("/project/assets/models");
        let root = Path::new("/project");
        let result = resolve_relative_path(base, "texture.png", root);
        assert_eq!(result, "assets/models/texture.png");
    }
}
