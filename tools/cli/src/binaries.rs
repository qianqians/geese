//! 找已构建好的编辑器 / 运行时二进制。
//!
//! 这些工具各自是独立的 Cargo 包（`tools/edit/*`、`crates/game_runtime`），产物散在各自的
//! `target/` 里。查找顺序：`$GEESE_BIN_DIR` → 本 CLI 所在目录 → 仓库内各 `target/{debug,release}`。
//! 找不到就报错并打印该跑哪条 `cargo build`——**不自动构建**。

use std::path::{Path, PathBuf};

use crate::cli::Editor;

/// 一个需要查找的二进制。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Binary {
    Editor(Editor),
    /// 引擎运行时 `geese_game`（`crates/game_runtime`）。
    Game,
}

impl Binary {
    /// 文件名（含平台后缀）。
    pub fn file_name(self) -> String {
        let stem = match self {
            Binary::Editor(editor) => editor.binary(),
            Binary::Game => "geese_game",
        };
        format!("{stem}{}", std::env::consts::EXE_SUFFIX)
    }

    /// 缺失时提示的构建命令。
    pub fn build_hint(self) -> String {
        match self {
            Binary::Editor(editor) => {
                format!("cd tools/edit/{} && cargo build", editor.dir())
            }
            Binary::Game => "cd crates/game_runtime && cargo build".to_string(),
        }
    }

    /// 仓库内相对路径（相对仓库根），按优先级排列。
    pub fn repo_dirs(self) -> &'static [&'static str] {
        match self {
            Binary::Editor(Editor::Scene) => {
                &["tools/edit/scene/target/debug", "tools/edit/scene/target/release"]
            }
            Binary::Editor(Editor::Material) => &[
                "tools/edit/material/target/debug",
                "tools/edit/material/target/release",
            ],
            Binary::Editor(Editor::Animation) => &[
                "tools/edit/animation/target/debug",
                "tools/edit/animation/target/release",
            ],
            Binary::Game => &[
                "target/debug",
                "target/release",
                "crates/game_runtime/target/debug",
                "crates/game_runtime/target/release",
            ],
        }
    }
}

/// 找不到二进制时给用户的说明。
#[derive(Debug, Clone)]
pub struct Missing {
    pub file_name: String,
    pub searched: Vec<PathBuf>,
    pub build_hint: String,
}

impl std::fmt::Display for Missing {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "找不到 {}（没有自动构建）", self.file_name)?;
        if self.searched.is_empty() {
            writeln!(f, "找过的位置：（空）")?;
        } else {
            writeln!(f, "找过的位置：")?;
            for path in &self.searched {
                writeln!(f, "  {}", path.display())?;
            }
        }
        write!(
            f,
            "先构建：{}（或把 GEESE_BIN_DIR 指向产物目录）",
            self.build_hint
        )
    }
}

/// 仓库根：从 `start` 向上找含 `tools/edit` 的目录。
///
/// CLI 自己的产物在 `tools/cli/target/debug/`，所以从那里往上四级就能回到仓库根；
/// 直接把仓库根放在别处时用 `$GEESE_BIN_DIR` 覆盖。
pub fn repo_root(start: &Path) -> Option<PathBuf> {
    let mut dir = Some(start);
    while let Some(current) = dir {
        if current.join("tools/edit").is_dir() {
            return Some(current.to_path_buf());
        }
        dir = current.parent();
    }
    None
}

/// 按顺序拼出要查的目录。
pub fn candidate_dirs(binary: Binary, exe_dir: Option<&Path>, env_bin: Option<&Path>) -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = Vec::new();
    let mut push = |dir: PathBuf| {
        if !dirs.contains(&dir) {
            dirs.push(dir);
        }
    };

    if let Some(env_bin) = env_bin {
        push(env_bin.to_path_buf());
    }
    if let Some(exe_dir) = exe_dir {
        push(exe_dir.to_path_buf());
        if let Some(root) = repo_root(exe_dir) {
            for relative in binary.repo_dirs() {
                push(root.join(relative));
            }
        }
    }
    dirs
}

/// 在给定目录里按顺序找；命中返回完整路径。
pub fn resolve_in(binary: Binary, dirs: &[PathBuf]) -> Option<PathBuf> {
    let file_name = binary.file_name();
    dirs.iter()
        .map(|dir| dir.join(&file_name))
        .find(|path| path.is_file())
}

/// 真要查找时用这个：自己收集目录、查找，失败时给出完整说明。
pub fn resolve(binary: Binary) -> Result<PathBuf, Missing> {
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(Path::to_path_buf));
    let env_bin = std::env::var_os("GEESE_BIN_DIR").map(PathBuf::from);

    let dirs = candidate_dirs(binary, exe_dir.as_deref(), env_bin.as_deref());
    match resolve_in(binary, &dirs) {
        Some(path) => Ok(path),
        None => Err(Missing {
            file_name: binary.file_name(),
            searched: dirs
                .iter()
                .map(|dir| dir.join(binary.file_name()))
                .collect(),
            build_hint: binary.build_hint(),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn tmp(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("geese-cli-bin-{}-{tag}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn env_dir_wins_then_exe_dir_then_repo_targets() {
        let root = tmp("order");
        let env = root.join("bin");
        let exe_dir = root.join("tools/cli/target/debug");
        let scene_debug = root.join("tools/edit/scene/target/debug");
        let scene_release = root.join("tools/edit/scene/target/release");
        for dir in [&env, &exe_dir, &scene_release] {
            fs::create_dir_all(dir).unwrap();
        }
        fs::create_dir_all(root.join("tools/edit")).unwrap();

        let binary = Binary::Editor(Editor::Scene);
        let name = binary.file_name();

        // 一个都没有 → 目录顺序仍然对
        let dirs = candidate_dirs(binary, Some(&exe_dir), Some(&env));
        assert_eq!(dirs[0], env);
        assert_eq!(dirs[1], exe_dir);
        assert!(dirs.contains(&scene_debug));
        assert!(double_checked(&dirs, &scene_debug));
        assert!(resolve_in(binary, &dirs).is_none());

        // release 里有 → 在 debug 之后仍能找到
        fs::create_dir_all(&scene_debug).unwrap();
        fs::write(scene_release.join(&name), b"x").unwrap();
        assert_eq!(resolve_in(binary, &dirs), Some(scene_release.join(&name)));

        // debug 里有 → debug 优先
        fs::write(scene_debug.join(&name), b"x").unwrap();
        assert_eq!(resolve_in(binary, &dirs), Some(scene_debug.join(&name)));

        // exe 同目录有 → 比仓库 target 更早
        fs::write(exe_dir.join(&name), b"x").unwrap();
        assert_eq!(resolve_in(binary, &dirs), Some(exe_dir.join(&name)));

        // env 目录有 → 最优先
        fs::write(env.join(&name), b"x").unwrap();
        assert_eq!(resolve_in(binary, &dirs), Some(env.join(&name)));

        fs::remove_dir_all(&root).ok();
    }

    fn double_checked(dirs: &[PathBuf], needle: &Path) -> bool {
        dirs.iter().filter(|dir| *dir == needle).count() == 1
    }

    #[test]
    fn repo_root_found_by_tools_edit_marker() {
        let root = tmp("repo");
        fs::create_dir_all(root.join("tools/edit/scene/target/debug")).unwrap();
        let deep = root.join("tools/edit/scene/target/debug");
        assert_eq!(repo_root(&deep).as_deref(), Some(root.as_path()));
        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn game_binary_has_both_repo_locations() {
        let dirs = Binary::Game.repo_dirs();
        assert!(dirs.contains(&"target/debug"));
        assert!(dirs.contains(&"crates/game_runtime/target/debug"));
        assert!(Binary::Game.file_name().starts_with("geese_game"));
        assert!(Binary::Game.build_hint().contains("crates/game_runtime"));
    }

    #[test]
    fn missing_display_lists_paths_and_hint() {
        let missing = Missing {
            file_name: "scene.exe".to_string(),
            searched: vec![PathBuf::from("/a/scene.exe")],
            build_hint: "cd tools/edit/scene && cargo build".to_string(),
        };
        let text = missing.to_string();
        assert!(text.contains("/a/scene.exe"));
        assert!(text.contains("cargo build"));
    }
}
