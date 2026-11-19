//! 命令行解析：`geese <子命令> [...]` → [`Command`]。
//!
//! 纯逻辑，不碰文件系统与进程，便于单测。

/// 可用的编辑器工具。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Editor {
    Scene,
    Material,
    Animation,
}

impl Editor {
    /// 生成的二进制文件名（不含平台后缀）。
    pub fn binary(self) -> &'static str {
        match self {
            Editor::Scene => "scene",
            Editor::Material => "material-editor",
            Editor::Animation => "animation-preview",
        }
    }

    /// 在 `tools/edit/` 下的目录名。
    pub fn dir(self) -> &'static str {
        match self {
            Editor::Scene => "scene",
            Editor::Material => "material",
            Editor::Animation => "animation",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Editor::Scene => "场景编辑器",
            Editor::Material => "材质编辑器",
            Editor::Animation => "动画预览",
        }
    }
}

/// 一条待执行的命令。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Command {
    /// `create proj`
    CreateProj,
    /// `scene` / `material` / `animation`
    OpenEditor(Editor),
    /// `run`
    Run,
    /// `reload`
    Reload,
    /// 无参数 / `help` / `--help` / `-h`
    Help,
}

pub const USAGE: &str = "\
用法：geese <子命令>

子命令：
  create proj   在当前目录创建一个最小工程（已存在工程或目录非空都会拒绝）
  scene         打开场景编辑器（工程根作为工作目录）
  material      打开材质编辑器
  animation     打开动画预览
  run           拉起调试窗口：有 Python 游戏就跑游戏，否则预览工程场景（前台，Ctrl+C 结束）
  reload        重启当前调试窗口（读 `.geese/dev.json` 记下的进程）
  help          显示本用法

环境变量：
  GEESE_BIN_DIR  先到这个目录找编辑器/运行时二进制
";

/// 解析失败：`message` 面向用户，调用方补上用法并以参数错误（码 2）退出。
pub fn parse(args: &[String]) -> Result<Command, String> {
    let Some(first) = args.first().map(String::as_str) else {
        return Ok(Command::Help);
    };

    let rest = &args[1..];
    let reject_extra = |name: &str| -> Result<(), String> {
        if rest.is_empty() {
            Ok(())
        } else {
            Err(format!("`{name}` 不接受参数：{}", rest.join(" ")))
        }
    };

    match first {
        "help" | "--help" | "-h" => Ok(Command::Help),
        "create" => match rest.first().map(String::as_str) {
            Some("proj") => {
                let tail = &rest[1..];
                if tail.is_empty() {
                    Ok(Command::CreateProj)
                } else {
                    Err(format!("`create proj` 不接受参数：{}", tail.join(" ")))
                }
            }
            Some(other) => Err(format!("未知的 create 子命令：{other}（只有 `proj`）")),
            None => Err("`create` 需要子命令：`create proj`".to_string()),
        },
        "scene" => reject_extra("scene").map(|()| Command::OpenEditor(Editor::Scene)),
        "material" => reject_extra("material").map(|()| Command::OpenEditor(Editor::Material)),
        "animation" => reject_extra("animation").map(|()| Command::OpenEditor(Editor::Animation)),
        "run" => {
            if let Some(extra) = rest.first() {
                Err(format!(
                    "`run` 不接受参数：{extra}（`run` 直接按工程配置启动，不再区分 scene）"
                ))
            } else {
                Ok(Command::Run)
            }
        }
        "reload" => reject_extra("reload").map(|()| Command::Reload),
        other => Err(format!("未知子命令：{other}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn empty_and_help_forms_print_usage() {
        assert_eq!(parse(&[]).unwrap(), Command::Help);
        assert_eq!(parse(&args(&["help"])).unwrap(), Command::Help);
        assert_eq!(parse(&args(&["--help"])).unwrap(), Command::Help);
        assert_eq!(parse(&args(&["-h"])).unwrap(), Command::Help);
    }

    #[test]
    fn create_proj_is_two_words() {
        assert_eq!(parse(&args(&["create", "proj"])).unwrap(), Command::CreateProj);
        assert!(parse(&args(&["create"])).is_err(), "缺子命令要报错");
        assert!(parse(&args(&["create", "game"])).is_err());
        assert!(parse(&args(&["create", "proj", "x"])).is_err(), "多余参数要报错");
    }

    #[test]
    fn editors_map_to_their_binaries() {
        assert_eq!(
            parse(&args(&["scene"])).unwrap(),
            Command::OpenEditor(Editor::Scene)
        );
        assert_eq!(Editor::Material.binary(), "material-editor");
        assert_eq!(Editor::Animation.dir(), "animation");
    }

    #[test]
    fn run_takes_no_arguments() {
        assert_eq!(parse(&args(&["run"])).unwrap(), Command::Run);
        let err = parse(&args(&["run", "scene"])).unwrap_err();
        assert!(err.contains("不接受参数"), "提示要说清 run 不再收 scene：{err}");
    }

    #[test]
    fn unknown_subcommand_is_an_error() {
        assert!(parse(&args(&["build"])).is_err());
        assert!(parse(&args(&["scene", "x.scene.json"])).is_err(), "编辑器不收文件参数");
    }
}
