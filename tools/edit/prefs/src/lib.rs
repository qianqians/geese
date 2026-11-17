//! 编辑小工具共用的「上次打开的东西」记录 + 工作目录解析。
//!
//! 每个工具启动时都必须指定一个**工作目录**（`--workdir <dir>`）。记录落在
//! `<工作目录>/.data/<app>.json`，跟着工作目录一起搬走，不再写系统配置目录。
//!
//! 记录里的路径在工作目录内时存相对路径（工作目录整体搬到别的盘/机器仍然有效），
//! 在工作目录外时存绝对路径，指向工作目录本身时存 `.`。对调用方暴露的
//! [`LastOpened`] 永远是已解析好的绝对路径。
//!
//! 落盘失败（目录只读等）只记一条日志，不影响工具本身继续工作。

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// 工作目录下存放工具记录的子目录。
pub const DATA_DIR: &str = ".data";

/// 自动补进工作目录 `.gitignore` 的一行。
const GITIGNORE_ENTRY: &str = ".data/";

/// 一条「上次打开的」记录：名字 + 已解析的绝对路径。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LastOpened {
    /// 显示用的名字（模型文件名、材质文件名、项目目录名）。
    pub name: String,
    /// 绝对路径（相对记录已按工作目录还原）。
    pub path: PathBuf,
}

impl LastOpened {
    /// 记录的路径在磁盘上还存在吗。
    pub fn exists(&self) -> bool {
        self.path.exists()
    }
}

/// 配置文件里的存储形式：路径是相对工作目录的字符串，或绝对路径字符串。
#[derive(Debug, Serialize, Deserialize)]
struct StoredEntry {
    name: String,
    #[serde(default)]
    path: String,
}

/// 配置文件内容：`{ "entries": { "model": { "name": ..., "path": ... } } }`。
#[derive(Debug, Default, Serialize, Deserialize)]
struct ConfigFile {
    #[serde(default)]
    entries: BTreeMap<String, StoredEntry>,
}

/// 一个工具在工作目录里的记录文件。
pub struct Prefs {
    /// 工作目录（绝对路径）。
    workdir: PathBuf,
    /// `<workdir>/.data/<app>.json`。
    file: PathBuf,
    /// 已解析为绝对路径的记录。
    entries: BTreeMap<String, LastOpened>,
}

impl Prefs {
    /// 打开 `<workdir>/.data/<app>.json`；目录不存在会创建，文件缺失或损坏都退化为空记录。
    ///
    /// 同时确保工作目录的 `.gitignore` 忽略 `.data/`（工作目录很可能本身是 git 仓库）。
    pub fn load_in(workdir: &Path, app: &str) -> Self {
        let workdir = absolute(workdir);
        let data_dir = workdir.join(DATA_DIR);
        if let Err(e) = fs::create_dir_all(&data_dir) {
            log::warn!(
                "创建工作目录数据目录 {} 失败（{:?}），本次记录不会落盘",
                data_dir.display(),
                e
            );
        }
        ensure_gitignore(&workdir);

        let file = data_dir.join(format!("{app}.json"));
        let raw = match fs::read_to_string(&file) {
            Ok(text) => match serde_json::from_str::<ConfigFile>(&text) {
                Ok(config) => config.entries,
                Err(e) => {
                    log::warn!("记录文件 {} 解析失败（{:?}），按空记录处理", file.display(), e);
                    BTreeMap::new()
                }
            },
            Err(e) if e.kind() == io::ErrorKind::NotFound => BTreeMap::new(),
            Err(e) => {
                log::warn!("读取记录文件 {} 失败（{:?}），按空记录处理", file.display(), e);
                BTreeMap::new()
            }
        };

        let entries = raw
            .into_iter()
            .map(|(key, entry)| {
                let path = decode(&workdir, &entry.path);
                (key, LastOpened { name: entry.name, path })
            })
            .collect();

        Self {
            workdir,
            file,
            entries,
        }
    }

    /// 工作目录（绝对路径）。
    pub fn workdir(&self) -> &Path {
        &self.workdir
    }

    /// 记录文件路径。
    pub fn file(&self) -> &Path {
        &self.file
    }

    /// 某类资源的原始记录，不检查路径是否还在。
    pub fn last(&self, key: &str) -> Option<&LastOpened> {
        self.entries.get(key)
    }

    /// 某类资源的记录，且路径仍然存在——启动时决定「能不能直接用它」走这里。
    pub fn last_existing(&self, key: &str) -> Option<&LastOpened> {
        self.entries.get(key).filter(|entry| entry.exists())
    }

    /// 写入一条记录并立即落盘。内容没变则不写盘；落盘失败只记日志。
    pub fn set(&mut self, key: &str, name: impl Into<String>, path: &Path) {
        let entry = LastOpened {
            name: name.into(),
            path: decode(&self.workdir, &encode(&self.workdir, path)),
        };
        if self.entries.get(key) == Some(&entry) {
            return;
        }
        self.entries.insert(key.to_string(), entry);
        if let Err(e) = self.save() {
            log::warn!("写入记录文件失败（{:?}）", e);
        }
    }

    fn save(&self) -> io::Result<()> {
        if let Some(parent) = self.file.parent() {
            fs::create_dir_all(parent)?;
        }
        let config = ConfigFile {
            entries: self
                .entries
                .iter()
                .map(|(key, entry)| {
                    (
                        key.clone(),
                        StoredEntry {
                            name: entry.name.clone(),
                            path: encode(&self.workdir, &entry.path),
                        },
                    )
                })
                .collect(),
        };
        let text = serde_json::to_string_pretty(&config).map_err(io::Error::other)?;
        fs::write(&self.file, text)
    }
}

/// 工作目录内的路径存相对形式（统一用 `/`），外侧存绝对路径，工作目录本身存 `.`。
fn encode(workdir: &Path, path: &Path) -> String {
    let absolute = absolute(path);
    match absolute.strip_prefix(workdir) {
        Ok(relative) if relative.as_os_str().is_empty() => ".".to_string(),
        Ok(relative) => relative.to_string_lossy().replace('\\', "/"),
        Err(_) => absolute.to_string_lossy().to_string(),
    }
}

/// 相对记录按工作目录还原；`.` 还原成工作目录本身。
fn decode(workdir: &Path, stored: &str) -> PathBuf {
    if stored.is_empty() || stored == "." {
        return workdir.to_path_buf();
    }
    let path = Path::new(stored);
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        workdir.join(path)
    }
}

/// 绝对化路径——不要求路径已存在（`canonicalize` 会要求）。
fn absolute(path: &Path) -> PathBuf {
    std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf())
}

/// 确保 `<workdir>/.gitignore` 里有 `.data/`：文件不存在则创建，已存在则只在缺失时
/// 追加一行，**从不覆盖**工作目录原有的忽略规则。
fn ensure_gitignore(workdir: &Path) {
    let file = workdir.join(".gitignore");
    let existing = fs::read_to_string(&file).unwrap_or_default();

    let already_ignored = existing.lines().any(|line| {
        matches!(
            line.trim(),
            ".data" | ".data/" | "/.data" | "/.data/" | ".data/*"
        )
    });
    if already_ignored {
        return;
    }

    let mut content = existing;
    if !content.is_empty() && !content.ends_with('\n') {
        content.push('\n');
    }
    content.push_str(GITIGNORE_ENTRY);
    content.push('\n');

    if let Err(e) = fs::write(&file, content) {
        log::warn!("更新 {} 失败（{:?}）", file.display(), e);
    }
}

/// 解析两个工具共用的 `--workdir <目录>`（也接受 `--workdir=<目录>` 与 `-h`）。
///
/// 目录不存在会被创建；失败时返回给调用方打印的用法/错误消息（`main` 里以非零码退出）。
pub fn workdir_from_args() -> Result<PathBuf, String> {
    let mut args = std::env::args();
    let program = args
        .next()
        .map(|arg| {
            Path::new(&arg)
                .file_stem()
                .map(|stem| stem.to_string_lossy().to_string())
                .unwrap_or(arg)
        })
        .unwrap_or_else(|| "tool".to_string());

    workdir_from(&program, args)
}

fn workdir_from(program: &str, args: impl Iterator<Item = String>) -> Result<PathBuf, String> {
    let usage = format!("用法: {program} --workdir <工作目录>");
    let mut workdir: Option<PathBuf> = None;

    let mut args = args.peekable();
    while let Some(arg) = args.next() {
        if arg == "-h" || arg == "--help" {
            return Err(usage);
        } else if let Some(value) = arg.strip_prefix("--workdir=") {
            workdir = Some(PathBuf::from(value));
        } else if arg == "--workdir" {
            match args.next() {
                Some(value) => workdir = Some(PathBuf::from(value)),
                None => return Err(format!("--workdir 缺少目录参数\n{usage}")),
            }
        } else {
            return Err(format!("未知参数 {arg}\n{usage}"));
        }
    }

    let workdir = workdir.ok_or_else(|| format!("必须指定工作目录\n{usage}"))?;
    if let Err(e) = fs::create_dir_all(&workdir) {
        return Err(format!("无法创建工作目录 {}: {e}", workdir.display()));
    }
    Ok(absolute(&workdir))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 每个测试独享一个临时目录，避免并行测试互相踩。
    fn temp_workdir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "edit-prefs-test-{}-{}",
            tag,
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// 从记录文件里读出某个 key 的存储形式路径。
    fn stored_path(text: &str, key: &str) -> String {
        let config: serde_json::Value = serde_json::from_str(text).unwrap();
        config["entries"][key]["path"]
            .as_str()
            .expect("记录里应有 path")
            .to_string()
    }

    #[test]
    fn records_land_in_workdir_data_dir() {
        let workdir = temp_workdir("location");
        let model = workdir.join("hero.glb");
        fs::write(&model, b"x").unwrap();

        let mut prefs = Prefs::load_in(&workdir, "animation-preview");
        assert_eq!(prefs.file(), workdir.join(DATA_DIR).join("animation-preview.json"));
        prefs.set("model", "hero.glb", &model);

        assert!(prefs.file().is_file(), "记录应写在 <workdir>/.data 下");
        let text = fs::read_to_string(prefs.file()).unwrap();
        assert!(text.contains("hero.glb"));
        // 工作目录内的路径存相对形式，整个目录可以搬家。
        assert_eq!(stored_path(&text, "model"), "hero.glb");
    }

    #[test]
    fn roundtrip_resolves_back_to_absolute_path() {
        let workdir = temp_workdir("roundtrip");
        let model = workdir.join("assets").join("hero.glb");
        fs::create_dir_all(model.parent().unwrap()).unwrap();
        fs::write(&model, b"x").unwrap();

        let mut prefs = Prefs::load_in(&workdir, "animation-preview");
        prefs.set("model", "hero.glb", &model);

        let reloaded = Prefs::load_in(&workdir, "animation-preview");
        let entry = reloaded.last("model").expect("记录应被持久化");
        assert_eq!(entry.name, "hero.glb");
        assert_eq!(entry.path, absolute(&model));
        assert!(entry.exists());
    }

    #[test]
    fn outside_workdir_stays_absolute() {
        let workdir = temp_workdir("outside");
        let outside = temp_workdir("outside-target").join("hero.glb");
        fs::write(&outside, b"x").unwrap();

        let mut prefs = Prefs::load_in(&workdir, "animation-preview");
        prefs.set("model", "hero.glb", &outside);

        let text = fs::read_to_string(prefs.file()).unwrap();
        // JSON 里反斜杠会被转义，直接比较会假失败——读回存储形式再比。
        let stored = stored_path(&text, "model");
        assert_eq!(Path::new(&stored), absolute(&outside));
        assert!(Path::new(&stored).is_absolute(), "工作目录外应存绝对路径: {stored}");

        let reloaded = Prefs::load_in(&workdir, "animation-preview");
        assert_eq!(reloaded.last("model").unwrap().path, absolute(&outside));
    }

    #[test]
    fn workdir_itself_is_stored_as_dot() {
        let workdir = temp_workdir("dot");
        let mut prefs = Prefs::load_in(&workdir, "material-editor");
        prefs.set("project", "default.material.json", &workdir);

        let text = fs::read_to_string(prefs.file()).unwrap();
        assert_eq!(stored_path(&text, "project"), ".");

        // `.` 必须还原成工作目录本身，否则调用方比较路径会失败。
        let reloaded = Prefs::load_in(&workdir, "material-editor");
        assert_eq!(reloaded.last("project").unwrap().path, absolute(&workdir));
    }

    #[test]
    fn broken_json_degrades_to_empty() {
        let workdir = temp_workdir("broken");
        let file = workdir.join(DATA_DIR).join("animation-preview.json");
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(&file, b"{ this is not json").unwrap();

        let prefs = Prefs::load_in(&workdir, "animation-preview");
        assert!(prefs.last("model").is_none());
    }

    #[test]
    fn last_existing_ignores_deleted_path() {
        let workdir = temp_workdir("deleted");
        let mut prefs = Prefs::load_in(&workdir, "animation-preview");
        prefs.set("model", "gone.glb", &workdir.join("gone.glb"));

        assert!(prefs.last("model").is_some());
        assert!(prefs.last_existing("model").is_none());
    }

    #[test]
    fn keys_are_independent() {
        let workdir = temp_workdir("keys");
        let mut prefs = Prefs::load_in(&workdir, "animation-preview");
        prefs.set("model", "hero.glb", &workdir.join("hero.glb"));
        prefs.set("extra", "skel.bin", &workdir.join("skel.bin"));

        let reloaded = Prefs::load_in(&workdir, "animation-preview");
        assert_eq!(reloaded.last("model").unwrap().name, "hero.glb");
        assert_eq!(reloaded.last("extra").unwrap().name, "skel.bin");
        // 另一个 app 的记录互不影响。
        assert!(Prefs::load_in(&workdir, "material-editor").last("model").is_none());
    }

    #[test]
    fn gitignore_is_created_and_never_overwrites() {
        let workdir = temp_workdir("gitignore");

        Prefs::load_in(&workdir, "animation-preview");
        let gitignore = workdir.join(".gitignore");
        assert_eq!(fs::read_to_string(&gitignore).unwrap(), ".data/\n");

        // 已有别的内容：追加而不是覆盖，且重复加载不会重复追加。
        fs::write(&gitignore, "target/\n").unwrap();
        Prefs::load_in(&workdir, "animation-preview");
        Prefs::load_in(&workdir, "animation-preview");
        let content = fs::read_to_string(&gitignore).unwrap();
        assert_eq!(content, "target/\n.data/\n");
    }

    #[test]
    fn workdir_arg_parsing() {
        let workdir = std::env::temp_dir()
            .join(format!("edit-prefs-args-{}", std::process::id()))
            .join("fresh");
        let _ = fs::remove_dir_all(workdir.parent().unwrap());
        let value = workdir.display().to_string();

        let parsed = workdir_from("tool", vec!["--workdir".to_string(), value.clone()].into_iter())
            .expect("应解析成功");
        assert_eq!(parsed, absolute(&workdir));
        assert!(workdir.is_dir(), "不存在的目录应被创建");

        let parsed = workdir_from("tool", vec![format!("--workdir={value}")].into_iter())
            .expect("--workdir= 形式也应解析成功");
        assert_eq!(parsed, absolute(&workdir));

        let err = workdir_from("tool", std::iter::empty()).unwrap_err();
        assert!(err.contains("必须指定工作目录"), "实际: {err}");

        let err = workdir_from("tool", vec!["model.glb".to_string()].into_iter()).unwrap_err();
        assert!(err.contains("未知参数"), "实际: {err}");

        let err = workdir_from("tool", vec!["--workdir".to_string()].into_iter()).unwrap_err();
        assert!(err.contains("缺少目录参数"), "实际: {err}");
    }
}
