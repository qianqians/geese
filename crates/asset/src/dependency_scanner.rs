//! 依赖扫描器——解析资源文件内部引用，转化为 UUID 依赖列表。
//!
//! 支持的文件格式：
//! - `.gltf`：解析 JSON，提取 `buffers[].uri`、`images[].uri` 中的相对路径
//! - `.glb`：自包含二进制，无外部依赖
//! - `.scene.json`：提取 `models[].path`
//! - `.avatar.json`：提取 `gltf_path`
//! - `.prefab.json`：提取 `nodes[].mesh.model_uuid` + `nodes[].prefab_ref`
//! - 其他格式：无依赖

use std::collections::{HashMap, HashSet};
use std::path::Path;

use crate::meta::AssetMeta;

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
    } else if lower.ends_with(".prefab.json") {
        scan_prefab_json_dependencies(asset_path, project_root, path_to_uuid)
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

/// 解析 .prefab.json 文件，提取 models 和嵌套 Prefab 的 UUID 引用。
fn scan_prefab_json_dependencies(
    asset_path: &Path,
    _project_root: &Path,
    _path_to_uuid: &HashMap<String, String>,
) -> Vec<String> {
    let content = match std::fs::read_to_string(asset_path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("[dep_scan] Cannot read prefab json {}: {e}", asset_path.display());
            return Vec::new();
        }
    };

    let json: serde_json::Value = match serde_json::from_str(&content) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("[dep_scan] Cannot parse prefab json {}: {e}", asset_path.display());
            return Vec::new();
        }
    };

    let mut deps = Vec::new();

    if let Some(nodes) = json.get("nodes").and_then(|n| n.as_array()) {
        for node in nodes {
            // 提取 mesh.ModelRef.model_uuid（GLTF 模型依赖）
            if let Some(mesh) = node.get("mesh") {
                if let Some(type_field) = mesh.get("type").and_then(|t| t.as_str()) {
                    if type_field == "model_ref" {
                        if let Some(uuid) = mesh.get("model_uuid").and_then(|u| u.as_str()) {
                            deps.push(uuid.to_string());
                        }
                    }
                }
            }
            // 提取 prefab_ref（嵌套 Prefab 依赖——UUID 引用）
            if let Some(prefab_uuid) = node.get("prefab_ref").and_then(|u| u.as_str()) {
                deps.push(prefab_uuid.to_string());
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

// ---------------------------------------------------------------------------
// 循环依赖检测
// ---------------------------------------------------------------------------

/// 检测 Prefab 之间的循环依赖。
///
/// `all_metas` 是所有资源的 UUID→AssetMeta 映射（例如从 AssetDatabase 获取）。
/// 对每个 Prefab 类型的资源做 DFS，沿 `dependencies` 中的 Prefab→Prefab 引用
/// 遍历，使用 visited + recursion_stack 双 set 检测有向图中的环。
///
/// 返回所有检测到的循环链（每个链以 UUID 列表表示，首尾相同）。
/// 若未发现循环则返回空 Vec。
pub fn check_prefab_cycle(
    all_metas: &HashMap<String, AssetMeta>,
) -> Vec<Vec<String>> {
    let mut cycles: Vec<Vec<String>> = Vec::new();
    let mut visited: HashSet<String> = HashSet::new();
    let mut recursion_stack: Vec<String> = Vec::new();

    // 仅对 Prefab 类型的资源执行 DFS
    for (uuid, meta) in all_metas {
        if meta.asset_type != crate::meta::AssetTypeKind::Prefab {
            continue;
        }
        if visited.contains(uuid) {
            continue;
        }
        dfs_cycle_detect(
            uuid,
            all_metas,
            &mut visited,
            &mut recursion_stack,
            &mut cycles,
        );
    }

    cycles
}

/// DFS 辅助函数：沿 Prefab→Prefab 引用链遍历，检测回溯边。
fn dfs_cycle_detect(
    current_uuid: &str,
    all_metas: &HashMap<String, AssetMeta>,
    visited: &mut HashSet<String>,
    recursion_stack: &mut Vec<String>,
    cycles: &mut Vec<Vec<String>>,
) {
    // 如果已在递归栈中，发现环
    if let Some(pos) = recursion_stack.iter().position(|u| u == current_uuid) {
        let mut cycle: Vec<String> = recursion_stack[pos..].to_vec();
        cycle.push(current_uuid.to_string()); // 闭合环
        cycles.push(cycle);
        return;
    }

    // 如果已访问过（且不在当前递归路径中），跳过
    if visited.contains(current_uuid) {
        return;
    }

    visited.insert(current_uuid.to_string());
    recursion_stack.push(current_uuid.to_string());

    // 检查当前 Prefab 的依赖中是否包含其他 Prefab
    if let Some(meta) = all_metas.get(current_uuid) {
        for dep_uuid in &meta.dependencies {
            // 仅追踪 Prefab→Prefab 引用
            if let Some(dep_meta) = all_metas.get(dep_uuid) {
                if dep_meta.asset_type == crate::meta::AssetTypeKind::Prefab {
                    dfs_cycle_detect(dep_uuid, all_metas, visited, recursion_stack, cycles);
                }
            }
        }
    }

    recursion_stack.pop();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::meta::AssetTypeKind;

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

    #[test]
    fn scan_prefab_json_with_model_ref_and_nested_prefab() {
        let tmp = std::env::temp_dir().join("prefab_dep_test");
        let _ = std::fs::create_dir_all(&tmp);
        let prefab_path = tmp.join("test.prefab.json");
        let json = r#"{
            "version": "1.0",
            "name": "TestPrefab",
            "nodes": [
                {
                    "name": "ModelNode",
                    "mesh": { "type": "model_ref", "model_uuid": "model-uuid-123" }
                },
                {
                    "name": "NestedPrefab",
                    "prefab_ref": "nested-prefab-uuid-456"
                }
            ],
            "root_nodes": [0, 1]
        }"#;
        std::fs::write(&prefab_path, json).unwrap();

        let mut path_to_uuid = HashMap::new();
        path_to_uuid.insert("assets/models/hero.glb".to_string(), "model-uuid-123".to_string());

        let deps = scan_dependencies(&prefab_path, Path::new("/project"), &path_to_uuid);
        // Should find model_uuid and prefab_ref
        assert!(deps.contains(&"model-uuid-123".to_string()), "should contain model uuid");
        assert!(deps.contains(&"nested-prefab-uuid-456".to_string()), "should contain nested prefab uuid");

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn scan_prefab_json_no_nodes() {
        let tmp = std::env::temp_dir().join("prefab_empty_test");
        let _ = std::fs::create_dir_all(&tmp);
        let prefab_path = tmp.join("empty.prefab.json");
        let json = r#"{ "version": "1.0", "name": "Empty" }"#;
        std::fs::write(&prefab_path, json).unwrap();

        let path_to_uuid = HashMap::new();
        let deps = scan_dependencies(&prefab_path, Path::new("/project"), &path_to_uuid);
        assert!(deps.is_empty());

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn check_prefab_cycle_no_cycle() {
        let mut all_metas = HashMap::new();
        all_metas.insert(
            "prefab-a".to_string(),
            AssetMeta {
                version: 1,
                uuid: "prefab-a".to_string(),
                asset_type: AssetTypeKind::Prefab,
                dependencies: vec!["model-x".to_string()],
                import_settings: serde_json::Value::Null,
            },
        );
        all_metas.insert(
            "prefab-b".to_string(),
            AssetMeta {
                version: 1,
                uuid: "prefab-b".to_string(),
                asset_type: AssetTypeKind::Prefab,
                dependencies: vec!["prefab-a".to_string()],
                import_settings: serde_json::Value::Null,
            },
        );
        // model-x is not a prefab, so no cycle
        all_metas.insert(
            "model-x".to_string(),
            AssetMeta {
                version: 1,
                uuid: "model-x".to_string(),
                asset_type: AssetTypeKind::Model,
                dependencies: vec![],
                import_settings: serde_json::Value::Null,
            },
        );

        let cycles = check_prefab_cycle(&all_metas);
        assert!(cycles.is_empty(), "no cycles expected");
    }

    #[test]
    fn check_prefab_cycle_detects_cycle() {
        let mut all_metas = HashMap::new();
        all_metas.insert(
            "prefab-a".to_string(),
            AssetMeta {
                version: 1,
                uuid: "prefab-a".to_string(),
                asset_type: AssetTypeKind::Prefab,
                dependencies: vec!["prefab-b".to_string()],
                import_settings: serde_json::Value::Null,
            },
        );
        all_metas.insert(
            "prefab-b".to_string(),
            AssetMeta {
                version: 1,
                uuid: "prefab-b".to_string(),
                asset_type: AssetTypeKind::Prefab,
                dependencies: vec!["prefab-a".to_string()],
                import_settings: serde_json::Value::Null,
            },
        );

        let cycles = check_prefab_cycle(&all_metas);
        assert!(!cycles.is_empty(), "should detect cycle");
        // 验证环包含 prefab-a → prefab-b → prefab-a
        let cycle_found = cycles.iter().any(|c| c.contains(&"prefab-a".to_string()) && c.contains(&"prefab-b".to_string()));
        assert!(cycle_found, "cycle should contain both prefab-a and prefab-b");
    }
}
