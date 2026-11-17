//! 编辑小工具共用的「上次打开的东西」记录。
//!
//! `tools/edit/` 下的每个工具都会记住用户上一次打开的资源：例如骨骼动画预览记住
//! 上次的 `.glb`，材质编辑器记住上次的项目目录。记录落在系统配置目录
//! （Windows 为 `%APPDATA%\geese\<app>.json`），这样换工作目录启动也能接上。
//!
//! 落盘失败（目录只读等）只记一条日志，不影响工具本身继续工作。

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// 系统配置目录下 Geese 的子目录名。
const CONFIG_SUBDIR: &str = "geese";

/// 一条「上次打开的」记录：文件名 + 完整路径。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LastOpened {
    /// 显示用的名字（模型文件名或项目目录名）。
    pub name: String,
    /// 完整路径。
    pub path: String,
}

impl LastOpened {
    pub fn new(name: impl Into<String>, path: &Path) -> Self {
        Self {
            name: name.into(),
            path: path.display().to_string(),
        }
    }

    /// 记录的路径在磁盘上还存在吗。
    pub fn exists(&self) -> bool {
        Path::new(&self.path).exists()
    }

    pub fn to_path_buf(&self) -> PathBuf {
        PathBuf::from(&self.path)
    }
}

/// 配置文件内容：`{ "entries": { "model": { "name": ..., "path": ... } } }`。
#[derive(Debug, Default, Serialize, Deserialize)]
struct ConfigFile {
    #[serde(default)]
    entries: BTreeMap<String, LastOpened>,
}

/// 一个工具的记录文件。
pub struct Prefs {
    /// 记录文件路径；系统取不到配置目录时为 `None`（此时只保留内存态）。
    file: Option<PathBuf>,
    entries: BTreeMap<String, LastOpened>,
}

impl Prefs {
    /// 读取 `<系统配置目录>/geese/<app>.json`。文件不存在或内容损坏都退化为空记录。
    pub fn load(app: &str) -> Self {
        let file = dirs::config_dir().map(|dir| dir.join(CONFIG_SUBDIR).join(format!("{app}.json")));
        match &file {
            Some(path) => Self::with_file(path.clone()),
            None => {
                log::warn!("取不到系统配置目录，本次不记录上次打开的资源");
                Self {
                    file: None,
                    entries: BTreeMap::new(),
                }
            }
        }
    }

    /// 从指定文件读取（也让测试可以注入临时路径）。
    pub fn with_file(file: PathBuf) -> Self {
        let entries = match fs::read_to_string(&file) {
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

        Self {
            file: Some(file),
            entries,
        }
    }

    /// 记录文件路径（`None` 表示不落盘）。
    pub fn file(&self) -> Option<&Path> {
        self.file.as_deref()
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
        let entry = LastOpened::new(name, path);
        if self.entries.get(key) == Some(&entry) {
            return;
        }
        self.entries.insert(key.to_string(), entry);
        if let Err(e) = self.save() {
            log::warn!("写入记录文件失败（{:?}）", e);
        }
    }

    fn save(&self) -> io::Result<()> {
        let Some(file) = &self.file else {
            return Ok(());
        };
        if let Some(parent) = file.parent() {
            fs::create_dir_all(parent)?;
        }
        let config = ConfigFile {
            entries: self.entries.clone(),
        };
        let text = serde_json::to_string_pretty(&config).map_err(io::Error::other)?;
        fs::write(file, text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 每个测试独享一个临时目录，避免并行测试互相踩。
    fn temp_file(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "edit-prefs-test-{}-{}",
            tag,
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        dir.join("prefs.json")
    }

    #[test]
    fn set_then_reload_keeps_name_and_path() {
        let file = temp_file("roundtrip");
        let target = file.parent().unwrap().join("hero.glb");
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        fs::write(&target, b"x").unwrap();

        let mut prefs = Prefs::with_file(file.clone());
        assert!(prefs.last("model").is_none());
        prefs.set("model", "hero.glb", &target);

        let reloaded = Prefs::with_file(file);
        let entry = reloaded.last("model").expect("记录应被持久化");
        assert_eq!(entry.name, "hero.glb");
        assert_eq!(entry.path, target.display().to_string());
        assert!(entry.exists());
    }

    #[test]
    fn missing_file_is_empty() {
        let prefs = Prefs::with_file(temp_file("missing"));
        assert!(prefs.last("model").is_none());
        assert!(prefs.last_existing("model").is_none());
    }

    #[test]
    fn broken_json_degrades_to_empty() {
        let file = temp_file("broken");
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(&file, b"{ this is not json").unwrap();

        let prefs = Prefs::with_file(file);
        assert!(prefs.last("model").is_none());
    }

    #[test]
    fn last_existing_ignores_deleted_path() {
        let file = temp_file("deleted");
        let gone = file.parent().unwrap().join("gone.glb");

        let mut prefs = Prefs::with_file(file);
        prefs.set("model", "gone.glb", &gone);

        assert!(prefs.last("model").is_some());
        assert!(prefs.last_existing("model").is_none());
    }

    #[test]
    fn different_keys_are_independent() {
        let file = temp_file("keys-a");
        let mut prefs = Prefs::with_file(file);
        prefs.set("model", "hero.glb", Path::new("D:/a/hero.glb"));
        prefs.set("project", "My", Path::new("D:/proj/My"));

        // 另一个文件里不该有这边的记录。
        let unrelated = Prefs::with_file(temp_file("keys-b"));
        assert!(unrelated.last("model").is_none());

        assert_eq!(prefs.last("model").unwrap().name, "hero.glb");
        assert_eq!(prefs.last("project").unwrap().name, "My");
    }
}
