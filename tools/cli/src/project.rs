//! 工程定位与 `create proj`：向上找 `config/project.toml`，以及最小工程模板的写入规则。

use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;

/// 判定「这里是 Geese 工程」的标志文件（相对工程根）。
pub const PROJECT_FILE: &str = "config/project.toml";

/// 没配场景时的默认场景路径（相对工程根）。
pub const DEFAULT_SCENE: &str = "assets/scenes/default.scene.json";

/// 工程配置里本工具关心的部分；其余段落（camera/player…）不解析也不校验。
#[derive(Debug, Clone, Default, Deserialize)]
pub struct ProjectConfig {
    #[serde(default)]
    pub project: ProjectSection,
    /// 只有 Python 游戏工程才有这一段。
    #[serde(default)]
    pub game: Option<GameSection>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct ProjectSection {
    #[serde(default)]
    pub name: String,
    /// 场景文件，相对工程根。
    #[serde(default)]
    pub scene: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct GameSection {
    /// `python` 时 `run` 走 `cargo run`。
    #[serde(default, rename = "type")]
    pub kind: String,
}

impl ProjectConfig {
    /// 场景文件（相对工程根）；配置没写就用 [`DEFAULT_SCENE`]。
    pub fn scene(&self) -> String {
        if self.project.scene.trim().is_empty() {
            DEFAULT_SCENE.to_string()
        } else {
            self.project.scene.clone()
        }
    }

    /// 是不是 Python 游戏工程。
    pub fn is_python_game(&self) -> bool {
        self.game
            .as_ref()
            .map(|game| game.kind.eq_ignore_ascii_case("python"))
            .unwrap_or(false)
    }
}

/// 从 `start` 向上逐级找工程根；`start` 自己也算。
pub fn find_root(start: &Path) -> Option<PathBuf> {
    let mut dir = Some(start);
    while let Some(current) = dir {
        if current.join(PROJECT_FILE).is_file() {
            return Some(current.to_path_buf());
        }
        dir = current.parent();
    }
    None
}

/// 当前工作目录所在的工程根；不在工程里时报错（给 `create proj` 的提示）。
pub fn require_root(start: &Path) -> Result<PathBuf, String> {
    find_root(start).ok_or_else(|| {
        format!(
            "{} 不在 Geese 工程内（向上找不到 {}）；先用 `geese create proj` 建一个工程",
            start.display(),
            PROJECT_FILE
        )
    })
}

/// 读工程配置。缺文件或解析失败都返回错误说明，不猜默认值。
pub fn load(root: &Path) -> Result<ProjectConfig, String> {
    let path = root.join(PROJECT_FILE);
    let text = fs::read_to_string(&path)
        .map_err(|e| format!("读取 {} 失败：{e}", path.display()))?;
    toml::from_str::<ProjectConfig>(&text)
        .map_err(|e| format!("解析 {} 失败：{e}", path.display()))
}

/// `create proj` 的结果。
#[derive(Debug, PartialEq, Eq)]
pub enum CreateOutcome {
    /// 当前目录或上级已经是工程。
    AlreadyExists(PathBuf),
    /// 建好了，附上写了哪些文件（相对工程根）。
    Created { root: PathBuf, files: Vec<String> },
}

/// 在当前目录建最小工程。
///
/// - 已经是工程（自己或上级）→ [`CreateOutcome::AlreadyExists`]，什么都不动；
/// - 目录里有任何条目 → `Err`（要求空目录，免得覆盖别人的东西）；
/// - 否则只补缺失文件，绝不覆盖已存在的文件。
pub fn create(dir: &Path) -> Result<CreateOutcome, String> {
    if let Some(root) = find_root(dir) {
        return Ok(CreateOutcome::AlreadyExists(root));
    }

    let entries = fs::read_dir(dir)
        .map_err(|e| format!("读取目录 {} 失败：{e}", dir.display()))?;
    if entries.count() > 0 {
        return Err(format!(
            "目录非空：{}（`create proj` 只允许在空目录里创建工程）",
            dir.display()
        ));
    }

    let name = project_name(dir)?;
    let mut files = Vec::new();

    write_if_missing(dir, PROJECT_FILE, &project_toml(&name), &mut files)?;
    write_if_missing(
        dir,
        DEFAULT_SCENE,
        &default_scene_json(&name),
        &mut files,
    )?;
    write_if_missing(dir, "assets/models/.gitkeep", "", &mut files)?;
    write_if_missing(dir, ".gitignore", GITIGNORE, &mut files)?;

    Ok(CreateOutcome::Created {
        root: dir.to_path_buf(),
        files,
    })
}

/// 工程名 = 目录名；取不到（比如根目录）就用 `unnamed`。
pub fn project_name(dir: &Path) -> Result<String, String> {
    let absolute = std::path::absolute(dir)
        .map_err(|e| format!("解析目录 {} 的绝对路径失败：{e}", dir.display()))?;
    match absolute.file_name().and_then(|n| n.to_str()) {
        Some(name) if !name.is_empty() => Ok(name.to_string()),
        _ => Ok("unnamed".to_string()),
    }
}

fn write_if_missing(
    root: &Path,
    relative: &str,
    content: &str,
    written: &mut Vec<String>,
) -> Result<(), String> {
    let path = root.join(relative);
    if path.exists() {
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("创建目录 {} 失败：{e}", parent.display()))?;
    }
    fs::write(&path, content).map_err(|e| format!("写入 {} 失败：{e}", path.display()))?;
    written.push(relative.to_string());
    Ok(())
}

pub const GITIGNORE: &str = "\
# Geese 工程的运行数据与构建产物
.geese/
.data/
target/
__pycache__/
*.pyc
";

/// `config/project.toml` 模板：与 `projects/My` 同形，只留本工具与编辑器都认的字段。
pub fn project_toml(name: &str) -> String {
    format!(
        "\
# {name} 项目配置
# 由 `geese create proj` 生成

[project]
name = \"{name}\"
template = \"empty\"
scene = \"{DEFAULT_SCENE}\"

[camera]
type = \"Free\"
fov = 60
z_near = 0.1
z_far = 1000
eye_height = 1.7
follow_offset = [0, 5, 8]
follow_smooth = 0.1
"
    )
}

/// 默认场景模板：一块地面 + 一盏方向光，打开就有东西看。
///
/// 用 `serde_json` 序列化而不是手写字符串——默认场景的字段与 `SceneManifest` 一一对应，
/// 手写容易在改动后悄悄写坏。
pub fn default_scene_json(name: &str) -> String {
    let scene = serde_json::json!({
        "version": "1.0",
        "name": name,
        "models": [],
        "objects": [
            {
                "object_type": "plane",
                "position": [0.0, -1.0, 0.0],
                "scale": [10.0, 1.0, 10.0],
                "color": [0.35, 0.35, 0.38],
                "physics": { "body_kind": "fixed" }
            }
        ],
        "environment": {
            "ambient": [0.12, 0.12, 0.15],
            "directional_lights": [
                { "direction": [-0.3, -1.0, -0.5], "color": [1.0, 0.95, 0.85], "intensity": 1.2 }
            ]
        },
        "spawn_points": [],
        "prefab_instances": []
    });
    let mut text = serde_json::to_string_pretty(&scene).unwrap_or_else(|_| "{}".to_string());
    text.push('\n');
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("geese-cli-{}-{tag}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn find_root_walks_up_and_includes_itself() {
        let root = tmp("find");
        fs::create_dir_all(root.join("config")).unwrap();
        fs::write(root.join(PROJECT_FILE), "[project]\nname = \"t\"\n").unwrap();
        fs::create_dir_all(root.join("a/b/c")).unwrap();

        assert_eq!(find_root(&root).as_deref(), Some(root.as_path()));
        assert_eq!(find_root(&root.join("a/b/c")).as_deref(), Some(root.as_path()));
        assert!(find_root(&root.join("a")).is_some(), "上级也算");

        let outside = tmp("find-outside");
        assert!(find_root(&outside).is_none());
        assert!(require_root(&outside).is_err());
        fs::remove_dir_all(&root).ok();
        fs::remove_dir_all(&outside).ok();
    }

    #[test]
    fn create_writes_minimal_project_once() {
        let root = tmp("create");
        let outcome = create(&root).unwrap();
        let CreateOutcome::Created { files, .. } = outcome else {
            panic!("空目录应该创建工程");
        };
        assert!(files.contains(&PROJECT_FILE.to_string()));
        assert!(root.join(PROJECT_FILE).is_file());
        assert!(root.join(DEFAULT_SCENE).is_file());
        assert!(root.join("assets/models/.gitkeep").is_file());
        assert!(root.join(".gitignore").is_file());

        let config = load(&root).unwrap();
        assert_eq!(config.scene(), DEFAULT_SCENE);
        assert_eq!(config.project.name, root.file_name().unwrap().to_string_lossy());
        assert!(!config.is_python_game());

        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn create_refuses_non_empty_dir_and_existing_project() {
        let root = tmp("create-nonempty");
        fs::write(root.join("README.md"), "hi").unwrap();
        let err = create(&root).unwrap_err();
        assert!(err.contains("目录非空"), "{err}");
        fs::remove_dir_all(&root).ok();

        // 已是工程：直接返回，不动任何文件
        let project = tmp("create-existing");
        fs::create_dir_all(project.join("config")).unwrap();
        fs::write(project.join(PROJECT_FILE), "[project]\nname = \"x\"\n").unwrap();
        fs::write(project.join("keep.txt"), "keep").unwrap();
        match create(&project).unwrap() {
            CreateOutcome::AlreadyExists(root) => assert_eq!(root, project),
            other => panic!("已有工程应直接返回，实际 {other:?}"),
        }
        assert_eq!(fs::read_to_string(project.join("keep.txt")).unwrap(), "keep");
        assert!(!project.join(".gitignore").exists(), "已有工程不该被补文件");
        fs::remove_dir_all(&project).ok();
    }

    #[test]
    fn created_files_are_usable_config_and_scene() {
        let config: ProjectConfig = toml::from_str(&project_toml("demo")).unwrap();
        assert_eq!(config.project.name, "demo");
        assert_eq!(config.scene(), DEFAULT_SCENE);

        let scene: serde_json::Value =
            serde_json::from_str(&default_scene_json("demo")).unwrap();
        assert_eq!(scene["version"], "1.0");
        assert_eq!(scene["name"], "demo");
        assert!(scene["objects"].as_array().unwrap().len() == 1);
        assert!(default_scene_json("demo").ends_with('\n'));
    }

    #[test]
    fn python_game_detection() {
        let toml = "[project]\nname = \"p\"\nscene = \"s.json\"\n\n[game]\ntype = \"python\"\nmodule = \"m\"\n";
        let config: ProjectConfig = toml::from_str(toml).unwrap();
        assert!(config.is_python_game());
        assert_eq!(config.scene(), "s.json");

        let other: ProjectConfig = toml::from_str("[project]\nname = \"p\"\n").unwrap();
        assert!(!other.is_python_game());
        assert_eq!(other.scene(), DEFAULT_SCENE);
    }
}
