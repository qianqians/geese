//! 调试窗口：`run` 拉起、`reload` 重启。
//!
//! 状态落在 `<工程>/.geese/dev.json`（pid / cwd / argv / kind），`reload` 读它就能在另一个终端
//! 找到 `run` 拉起的那个窗口。进程存活与终止用系统命令（Windows `taskkill`/`tasklist`，
//! 类 Unix `kill`），不为这点功能引入平台 crate。

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::binaries::{self, Binary};
use crate::project::ProjectConfig;

/// 工程内的运行数据目录。
pub const DEV_DIR: &str = ".geese";
/// 调试窗口的记录文件（相对工程根）。
pub const DEV_FILE: &str = ".geese/dev.json";

/// 一次调试窗口启动的记录。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DevState {
    pub pid: u32,
    /// 子进程的工作目录。
    pub cwd: String,
    /// 完整命令行（`argv[0]` 是程序路径），`reload` 原样重放。
    pub argv: Vec<String>,
    /// `scene`（引擎运行时预览）或 `python`（工程的 `cargo run`）。
    pub kind: String,
    /// 启动时间（Unix 秒），仅用于显示。
    pub started_at: u64,
}

/// 要启动的东西：程序 + 参数（工作目录恒为工程根）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Launch {
    pub program: PathBuf,
    pub args: Vec<String>,
    pub kind: &'static str,
}

impl Launch {
    /// 写进 `dev.json` 的完整命令行。
    pub fn argv(&self) -> Vec<String> {
        let mut argv = vec![self.program.display().to_string()];
        argv.extend(self.args.iter().cloned());
        argv
    }
}

/// 按工程配置决定跑什么：Python 游戏 → `cargo run`；否则 → `geese_game <工程> <场景>`。
pub fn plan(root: &Path, config: &ProjectConfig) -> Result<Launch, String> {
    if config.is_python_game() {
        return plan_python(root);
    }
    let game = binaries::resolve(Binary::Game).map_err(|missing| missing.to_string())?;
    Ok(plan_scene(root, config, game.as_path()))
}

/// Python 工程：在工程根 `cargo run`。
///
/// 只有当工程自带 Cargo 入口时才成立——`create proj` 建的 `empty` 工程没有，走场景预览。
pub fn plan_python(root: &Path) -> Result<Launch, String> {
    if !root.join("Cargo.toml").is_file() {
        return Err(format!(
            "工程声明了 Python 游戏（config/project.toml 里 [game].type = \"python\"），但 {} 下没有 Cargo.toml；\
             `geese run` 用 `cargo run` 启动 Python 游戏，请补上工程入口或改回场景预览",
            root.display()
        ));
    }
    Ok(Launch {
        program: PathBuf::from("cargo"),
        args: vec!["run".to_string()],
        kind: "python",
    })
}

/// 场景预览：`<geese_game> <工程根> <配置里的场景>`。
pub fn plan_scene(root: &Path, config: &ProjectConfig, game_exe: &Path) -> Launch {
    Launch {
        program: game_exe.to_path_buf(),
        args: vec![
            root.display().to_string(),
            config.scene(),
        ],
        kind: "scene",
    }
}

/// 用记录里的命令行重建一次启动（`reload` 用）；记录为空时返回 `None`。
pub fn launch_from_state(state: &DevState) -> Option<Launch> {
    let (program, rest) = state.argv.split_first()?;
    Some(Launch {
        program: PathBuf::from(program),
        args: rest.to_vec(),
        kind: if state.kind == "python" { "python" } else { "scene" },
    })
}

pub fn state_path(root: &Path) -> PathBuf {
    root.join(DEV_FILE)
}

/// 读记录；没有文件或内容坏掉都返回 `None`（坏内容会打日志），不阻断启动。
pub fn read_state(root: &Path) -> Option<DevState> {
    let path = state_path(root);
    match fs::read_to_string(&path) {
        Ok(text) => match serde_json::from_str::<DevState>(&text) {
            Ok(state) => Some(state),
            Err(error) => {
                log::warn!("{} 解析失败（{error}），忽略这条记录", path.display());
                None
            }
        },
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => {
            log::warn!("读取 {} 失败（{error}），忽略这条记录", path.display());
            None
        }
    }
}

/// 写记录，顺带保证工程 `.gitignore` 忽略 `.geese/`。
pub fn write_state(root: &Path, state: &DevState) -> Result<(), String> {
    let dir = root.join(DEV_DIR);
    fs::create_dir_all(&dir).map_err(|e| format!("创建 {} 失败：{e}", dir.display()))?;
    ensure_gitignore(root);

    let path = state_path(root);
    let mut text = serde_json::to_string_pretty(state)
        .map_err(|e| format!("序列化调试窗口记录失败：{e}"))?;
    text.push('\n');
    fs::write(&path, text).map_err(|e| format!("写入 {} 失败：{e}", path.display()))
}

/// 只在记录里的 pid 仍是自己那个子进程时才删除，避免和 `reload` 刚写的新记录打架。
///
/// 「读 → 比较 → 删除」不是原子的：`reload` 很可能在我们读完旧记录、还没删之前写进新记录。
/// 所以先把文件**改名挪开**拿到独占，再核对内容——挪开的就是刚写进来的新记录时，原样放回去。
pub fn remove_state_if_pid(root: &Path, pid: u32) {
    let Some(state) = read_state(root) else {
        return;
    };
    if state.pid != pid {
        return;
    }

    let path = state_path(root);
    let scratch = root
        .join(DEV_DIR)
        .join(format!(".dev.json.{pid}.tmp"));
    if let Err(error) = fs::rename(&path, &scratch) {
        if error.kind() != std::io::ErrorKind::NotFound {
            log::warn!("挪开 {} 失败（{error}）", path.display());
        }
        return;
    }

    let grabbed = fs::read_to_string(&scratch)
        .ok()
        .and_then(|text| serde_json::from_str::<DevState>(&text).ok());
    match grabbed {
        Some(moved) if moved.pid == pid => {
            let _ = fs::remove_file(&scratch);
        }
        _ => {
            // 挪走的是别人刚写的新记录：放回去；放不回去（已被新记录占位）就丢弃旧的。
            if fs::rename(&scratch, &path).is_err() {
                let _ = fs::remove_file(&scratch);
            }
        }
    }
}

/// 前台启动并阻塞到窗口关闭；返回子进程退出码（拿不到时按 0）。
pub fn run_foreground(root: &Path, launch: &Launch) -> Result<i32, String> {
    let mut child = Command::new(&launch.program)
        .args(&launch.args)
        .current_dir(root)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
        .map_err(|e| format!("启动 {} 失败：{e}", launch.program.display()))?;

    let pid = child.id();
    let state = DevState {
        pid,
        cwd: root.display().to_string(),
        argv: launch.argv(),
        kind: launch.kind.to_string(),
        started_at: unix_now(),
    };
    if let Err(error) = write_state(root, &state) {
        log::warn!("写调试窗口记录失败（{error}），`reload` 将找不到这个窗口");
    }

    let status = child
        .wait()
        .map_err(|e| format!("等待调试窗口退出失败：{e}"))?;
    remove_state_if_pid(root, pid);
    Ok(status.code().unwrap_or(0))
}

/// 进程还在吗。
pub fn is_alive(pid: u32) -> bool {
    if pid == 0 {
        return false;
    }
    #[cfg(windows)]
    {
        let output = Command::new("tasklist")
            .args(["/FI", &format!("PID eq {pid}"), "/NH", "/FO", "CSV"])
            .output();
        match output {
            Ok(output) => String::from_utf8_lossy(&output.stdout).contains(&pid.to_string()),
            Err(error) => {
                log::warn!("调用 tasklist 失败：{error}");
                false
            }
        }
    }
    #[cfg(not(windows))]
    {
        Command::new("kill")
            .arg("-0")
            .arg(pid.to_string())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|status| status.success())
            .unwrap_or(false)
    }
}

/// 结束进程并等它真的退出（最多 `wait`）。
pub fn terminate(pid: u32, wait: Duration) -> Result<(), String> {
    #[cfg(windows)]
    {
        let status = Command::new("taskkill")
            .args(["/PID", &pid.to_string(), "/T", "/F"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map_err(|e| format!("调用 taskkill 失败：{e}"))?;
        if !status.success() && is_alive(pid) {
            return Err(format!("结束进程 {pid} 失败（taskkill 退出码 {:?}）", status.code()));
        }
    }
    #[cfg(not(windows))]
    {
        let _ = Command::new("kill").arg("-TERM").arg(pid.to_string()).status();
        let deadline = std::time::Instant::now() + wait;
        while std::time::Instant::now() < deadline {
            if !is_alive(pid) {
                return Ok(());
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        let _ = Command::new("kill").arg("-KILL").arg(pid.to_string()).status();
    }

    let deadline = std::time::Instant::now() + wait;
    while std::time::Instant::now() < deadline {
        if !is_alive(pid) {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    Err(format!("进程 {pid} 在 {:?} 内没有退出", wait))
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// 保证工程 `.gitignore` 里有 `.geese/`：文件不存在则创建，已存在则只在缺失时追加一行。
fn ensure_gitignore(root: &Path) {
    const ENTRY: &str = ".geese/";
    let path = root.join(".gitignore");
    let existing = fs::read_to_string(&path).unwrap_or_default();
    if existing.lines().any(|line| line.trim() == ENTRY) {
        return;
    }
    let mut text = existing;
    if !text.is_empty() && !text.ends_with('\n') {
        text.push('\n');
    }
    text.push_str(ENTRY);
    text.push('\n');
    if let Err(error) = fs::write(&path, text) {
        log::warn!("更新 {} 失败（{error}）", path.display());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("geese-cli-dev-{}-{tag}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn config(text: &str) -> ProjectConfig {
        toml::from_str(text).unwrap()
    }

    #[test]
    fn state_round_trip_and_selective_removal() {
        let root = tmp("state");
        let state = DevState {
            pid: 4242,
            cwd: root.display().to_string(),
            argv: vec!["/bin/geese_game".into(), "proj".into(), "s.json".into()],
            kind: "scene".into(),
            started_at: 1_700_000_000,
        };
        write_state(&root, &state).unwrap();
        assert_eq!(read_state(&root), Some(state.clone()));

        // pid 不匹配 → 删不掉（reload 刚写的新记录不该被旧 run 清掉）
        remove_state_if_pid(&root, 9999);
        assert!(state_path(&root).is_file());
        // pid 匹配 → 删除
        remove_state_if_pid(&root, 4242);
        assert!(!state_path(&root).is_file());
        assert_eq!(read_state(&root), None);

        // 写记录顺手把 .geese/ 补进 .gitignore，且不重复
        write_state(&root, &state).unwrap();
        write_state(&root, &state).unwrap();
        let gitignore = fs::read_to_string(root.join(".gitignore")).unwrap();
        assert_eq!(gitignore.matches(".geese/").count(), 1, "{gitignore}");

        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn removal_keeps_a_newer_record() {
        let root = tmp("state-newer");
        let old_state = DevState {
            pid: 111,
            cwd: root.display().to_string(),
            argv: vec!["a".into()],
            kind: "scene".into(),
            started_at: 1,
        };
        let new_state = DevState {
            pid: 222,
            ..old_state.clone()
        };

        // 旧 run 先是读到自己的记录；期间 reload 写入了新记录，然后旧 run 才执行「删除」
        write_state(&root, &old_state).unwrap();
        let _ = read_state(&root);
        write_state(&root, &new_state).unwrap();

        // 删除时挪开的是新记录 → 必须原样放回
        remove_state_if_pid(&root, 111);
        assert_eq!(read_state(&root), Some(new_state), "新记录不能被旧 run 删掉");

        // 自己的记录仍在时才会被删
        remove_state_if_pid(&root, 222);
        assert_eq!(read_state(&root), None);

        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn broken_state_file_is_ignored() {        let root = tmp("broken");
        fs::create_dir_all(root.join(DEV_DIR)).unwrap();
        fs::write(state_path(&root), "{ not json").unwrap();
        assert_eq!(read_state(&root), None);
        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn scene_plan_passes_project_and_configured_scene() {
        let root = tmp("plan-scene");
        let cfg = config("[project]\nname = \"p\"\nscene = \"assets/scenes/a.scene.json\"\n");
        let launch = plan_scene(&root, &cfg, Path::new("/bin/geese_game"));
        assert_eq!(launch.kind, "scene");
        assert_eq!(launch.program, PathBuf::from("/bin/geese_game"));
        assert_eq!(
            launch.args,
            vec![root.display().to_string(), "assets/scenes/a.scene.json".to_string()]
        );
        assert_eq!(launch.argv()[0], "/bin/geese_game");

        // 没配场景时用默认场景
        let default_cfg = config("[project]\nname = \"p\"\n");
        let launch = plan_scene(&root, &default_cfg, Path::new("/bin/geese_game"));
        assert_eq!(launch.args[1], crate::project::DEFAULT_SCENE);
        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn python_plan_needs_cargo_entry() {
        let root = tmp("plan-python");
        let cfg = config("[project]\nname = \"p\"\n\n[game]\ntype = \"python\"\n");
        let err = plan_python(&root).unwrap_err();
        assert!(err.contains("Cargo.toml"), "{err}");

        fs::write(root.join("Cargo.toml"), "[package]\nname = \"p\"\n").unwrap();
        let launch = plan_python(&root).unwrap();
        assert_eq!(launch.kind, "python");
        assert_eq!(launch.program, PathBuf::from("cargo"));
        assert_eq!(launch.args, vec!["run".to_string()]);
        assert!(cfg.is_python_game());
        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn launch_from_state_replays_recorded_argv() {
        let state = DevState {
            pid: 1,
            cwd: ".".into(),
            argv: vec!["C:/g/geese_game.exe".into(), "proj".into(), "s.json".into()],
            kind: "python".into(),
            started_at: 0,
        };
        let launch = launch_from_state(&state).unwrap();
        assert_eq!(launch.program, PathBuf::from("C:/g/geese_game.exe"));
        assert_eq!(launch.args, vec!["proj".to_string(), "s.json".to_string()]);
        assert_eq!(launch.kind, "python", "kind 要跟着记录走");

        let empty = DevState { argv: vec![], ..state };
        assert!(launch_from_state(&empty).is_none());
    }

    #[test]
    fn pid_zero_is_never_alive() {
        assert!(!is_alive(0));
    }
}
