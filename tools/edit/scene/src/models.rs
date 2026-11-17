//! 左栏「可选模型」：递归扫 `<工作目录>/assets/models` 下的 glTF/GLB，按子目录分组。

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// 模型目录（相对工作目录）。
pub const MODELS_DIR: &str = "assets/models";

/// 递归深度上限，防止符号链接/超深目录把扫描拖死。
const MAX_DEPTH: u32 = 8;

/// 一个可选模型。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModelEntry {
    /// 模型文件绝对路径。
    pub path: PathBuf,
    /// 相对 `assets/models` 的展示名（分隔符统一为 `/`）。
    pub label: String,
}

impl ModelEntry {
    /// 显示用的短名（文件名）。
    pub fn file_name(&self) -> String {
        self.path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| self.label.clone())
    }
}

/// 一组同目录的模型。
#[derive(Clone, Debug)]
pub struct ModelGroup {
    /// 分组标题，如 `assets/models` 或 `assets/models/props`。
    pub title: String,
    pub entries: Vec<ModelEntry>,
}

/// 模型根目录。
pub fn models_root(workdir: &Path) -> PathBuf {
    workdir.join(MODELS_DIR)
}

/// 扫描工作目录下的模型；根目录不存在返回空列表（不算错误），其它 IO 错误照样是空列表 +
/// 日志——左栏空着总比工具起不来好。
pub fn scan_models(workdir: &Path) -> Vec<ModelGroup> {
    let root = models_root(workdir);
    if !root.is_dir() {
        log::info!("模型目录 {} 不存在，左栏留空", root.display());
        return Vec::new();
    }

    let mut files = Vec::new();
    if let Err(e) = collect(&root, 0, &mut files) {
        log::warn!("扫描模型目录 {} 失败：{e:?}", root.display());
    }
    group_entries(&root, files)
}

/// 是否是模型文件（`.gltf` / `.glb`，大小写不敏感）。
pub fn is_model_file(path: &Path) -> bool {
    matches!(
        path.extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_ascii_lowercase())
            .as_deref(),
        Some("gltf") | Some("glb")
    )
}

/// 模型总数（状态栏用）。
pub fn count_entries(groups: &[ModelGroup]) -> usize {
    groups.iter().map(|g| g.entries.len()).sum()
}

fn collect(dir: &Path, depth: u32, out: &mut Vec<PathBuf>) -> io::Result<()> {
    if depth > MAX_DEPTH {
        log::warn!("模型目录超过 {MAX_DEPTH} 层，{} 及以下被跳过", dir.display());
        return Ok(());
    }

    let mut entries: Vec<_> = fs::read_dir(dir)?.filter_map(Result::ok).collect();
    entries.sort_by_key(|e| e.file_name());

    for entry in entries {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with('.') {
            continue;
        }
        let file_type = match entry.file_type() {
            Ok(t) => t,
            Err(e) => {
                log::warn!("读取 {} 的类型失败：{e:?}", path.display());
                continue;
            }
        };
        // 不跟随符号链接目录，避免环。
        if file_type.is_symlink() {
            continue;
        }
        if file_type.is_dir() {
            collect(&path, depth + 1, out)?;
        } else if file_type.is_file() && is_model_file(&path) {
            out.push(path);
        }
    }
    Ok(())
}

/// 把文件列表按所在子目录分组：顶层一组，其余每个子目录一组，组内与组间都按名字排序。
fn group_entries(root: &Path, files: Vec<PathBuf>) -> Vec<ModelGroup> {
    let mut groups: Vec<ModelGroup> = Vec::new();

    for path in files {
        let rel = path.strip_prefix(root).unwrap_or(&path);
        let rel_dir = rel.parent().map(Path::to_path_buf).unwrap_or_default();
        let title = if rel_dir.as_os_str().is_empty() {
            MODELS_DIR.to_string()
        } else {
            format!("{MODELS_DIR}/{}", rel_dir.to_string_lossy().replace('\\', "/"))
        };
        let label = rel.to_string_lossy().replace('\\', "/");

        match groups.iter_mut().find(|g| g.title == title) {
            Some(group) => group.entries.push(ModelEntry { path, label }),
            None => groups.push(ModelGroup {
                title,
                entries: vec![ModelEntry { path, label }],
            }),
        }
    }

    for group in &mut groups {
        group.entries.sort_by(|a, b| a.label.cmp(&b.label));
    }
    groups.sort_by(|a, b| a.title.cmp(&b.title));
    groups
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_workdir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "scene-editor-models-{}-{tag}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("assets/models/props")).unwrap();
        dir
    }

    #[test]
    fn scan_finds_models_by_group_and_ignores_other_files() {
        let workdir = tmp_workdir("scan");
        let models = workdir.join("assets/models");
        fs::write(models.join("box.glb"), b"x").unwrap();
        fs::write(models.join("chair.GLTF"), b"x").unwrap();
        fs::write(models.join("readme.txt"), b"x").unwrap();
        fs::write(models.join("props/lamp.glb"), b"x").unwrap();
        fs::create_dir_all(models.join(".hidden")).unwrap();
        fs::write(models.join(".hidden/skip.glb"), b"x").unwrap();

        let groups = scan_models(&workdir);
        let titles: Vec<&str> = groups.iter().map(|g| g.title.as_str()).collect();
        assert_eq!(titles, vec!["assets/models", "assets/models/props"]);
        assert_eq!(groups[0].entries.len(), 2, "只收 gltf/glb，忽略隐藏目录");
        assert_eq!(groups[0].entries[0].label, "box.glb");
        assert_eq!(groups[0].entries[1].label, "chair.GLTF");
        assert_eq!(groups[1].entries[0].label, "props/lamp.glb");
        assert_eq!(groups[1].entries[0].file_name(), "lamp.glb");
        fs::remove_dir_all(&workdir).ok();
    }

    #[test]
    fn missing_models_dir_is_empty_not_an_error() {
        let workdir = std::env::temp_dir().join(format!(
            "scene-editor-models-{}-missing",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&workdir);
        fs::create_dir_all(&workdir).unwrap();
        assert!(scan_models(&workdir).is_empty());
        fs::remove_dir_all(&workdir).ok();
    }

    #[test]
    fn model_file_extension_is_case_insensitive() {
        assert!(is_model_file(Path::new("a/b.glb")));
        assert!(is_model_file(Path::new("a/b.GLB")));
        assert!(is_model_file(Path::new("a/b.gltf")));
        assert!(!is_model_file(Path::new("a/b.png")));
        assert!(!is_model_file(Path::new("a/bglb")));
    }
}
