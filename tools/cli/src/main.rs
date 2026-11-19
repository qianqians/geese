//! `geese` — Geese 工程的命令行入口。
//!
//! 用法见 [`cli::USAGE`]。这个 crate 只做「找工程 + 拉起别的可执行文件」两件事：
//! 不链接引擎、不自动构建，编辑器与运行时的产物要事先用 cargo 建好。

mod binaries;
mod cli;
mod dev;
mod project;

use std::path::Path;
use std::process::{Command as Process, Stdio};

use cli::{Command, Editor};
use project::CreateOutcome;

fn main() {
    env_logger::init();

    let args: Vec<String> = std::env::args().skip(1).collect();
    let code = match cli::parse(&args) {
        Ok(Command::Help) => {
            print!("{}", cli::USAGE);
            0
        }
        Err(message) => {
            eprintln!("{message}\n");
            eprint!("{}", cli::USAGE);
            2
        }
        Ok(command) => match execute(command) {
            Ok(code) => code,
            Err(message) => {
                eprintln!("geese: {message}");
                1
            }
        },
    };
    std::process::exit(code);
}

fn execute(command: Command) -> Result<i32, String> {
    let cwd = std::env::current_dir()
        .map_err(|e| format!("取当前目录失败：{e}"))?;

    match command {
        Command::CreateProj => create_proj(&cwd),
        Command::OpenEditor(editor) => open_editor(editor, &cwd),
        Command::Run => run(&cwd),
        Command::Reload => reload(&cwd),
        Command::Help => Ok(0),
    }
}

fn create_proj(cwd: &Path) -> Result<i32, String> {
    match project::create(cwd)? {
        CreateOutcome::AlreadyExists(root) => {
            let name = project::load(&root)
                .ok()
                .map(|config| config.project.name)
                .filter(|name| !name.is_empty());
            match name {
                Some(name) => println!("已存在工程：{name}（{}），什么也没做", root.display()),
                None => println!("已存在工程：{}，什么也没做", root.display()),
            }
            Ok(0)
        }
        CreateOutcome::Created { root, files } => {
            println!("已创建工程：{}", root.display());
            for file in files {
                println!("  + {file}");
            }
            println!("\n接下来：geese scene 打开场景编辑器，geese run 拉起调试窗口");
            Ok(0)
        }
    }
}

fn open_editor(editor: Editor, cwd: &Path) -> Result<i32, String> {
    let root = project::require_root(cwd)?;
    let binary = crate::binaries::Binary::Editor(editor);
    let exe = crate::binaries::resolve(binary).map_err(|missing| missing.to_string())?;

    println!("{}：{}（工作目录 {}）", editor.label(), exe.display(), root.display());
    let status = Process::new(&exe)
        .arg("--workdir")
        .arg(&root)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .map_err(|e| format!("启动 {} 失败：{e}", exe.display()))?;
    Ok(status.code().unwrap_or(0))
}

fn run(cwd: &Path) -> Result<i32, String> {
    let root = project::require_root(cwd)?;
    let config = project::load(&root)?;
    let launch = dev::plan(&root, &config)?;
    println!("拉起调试窗口：{}（{}）", launch.program.display(), launch.kind);
    dev::run_foreground(&root, &launch)
}

fn reload(cwd: &Path) -> Result<i32, String> {
    let root = project::require_root(cwd)?;
    let previous = dev::read_state(&root);

    let launch = match &previous {
        Some(state) => {
            if dev::is_alive(state.pid) {
                println!("停止调试窗口（pid {}）……", state.pid);
                dev::terminate(state.pid, std::time::Duration::from_secs(5))?;
            } else {
                println!("记录的调试窗口（pid {}）已经退出，直接重新拉起", state.pid);
            }
            match dev::launch_from_state(state) {
                Some(launch) => launch,
                None => {
                    let config = project::load(&root)?;
                    dev::plan(&root, &config)?
                }
            }
        }
        None => {
            println!("没有调试窗口记录，按 `geese run` 处理");
            let config = project::load(&root)?;
            dev::plan(&root, &config)?
        }
    };

    println!("重新拉起：{}（{}）", launch.program.display(), launch.kind);
    dev::run_foreground(&root, &launch)
}
