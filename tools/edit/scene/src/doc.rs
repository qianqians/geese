//! 场景文档：`<场景>.scene.json` 的文本 ↔ `SceneManifest`，以及拖入模型时的 JSON 改动。
//!
//! 这里全是纯逻辑（不碰窗口、不碰 GPU），便于单测：解析/格式化、追加 model 条目、
//! 模型路径相对化、id 去重、防抖判定、落盘。

use std::fs;
use std::path::{Component, Path, PathBuf};
use std::time::{Duration, Instant};

use scene::manifest::SceneManifest;
use serde_json::{Map, Value};

/// 停止输入多久之后自动应用（解析 + 重建场景）。
pub const APPLY_DEBOUNCE: Duration = Duration::from_millis(400);

/// 一次应用失败的原因。`line` 用于在行号栏标红。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DocError {
    pub message: String,
    pub line: Option<usize>,
}

impl DocError {
    fn whole(message: impl Into<String>) -> Self {
        Self { message: message.into(), line: None }
    }
}

impl std::fmt::Display for DocError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.line {
            Some(line) => write!(f, "第 {line} 行 · {}", self.message),
            None => write!(f, "{}", self.message),
        }
    }
}

/// 一个打开着的场景：文本、文件路径、以及「自动应用/自动保存」需要的状态。
pub struct SceneDoc {
    pub text: String,
    /// 场景文件路径；`新建` 之后为空，第一次保存/另存为才填上。
    pub path: Option<PathBuf>,
    /// 最近一次尝试应用过的文本（失败也算），避免同一份坏文本反复重建。
    attempted: String,
    /// 文本最后一次变化的时刻；`None` 表示这次变化已经尝试过。
    changed_at: Option<Instant>,
    /// 最近一次成功写入磁盘的文本；与 `text` 不同即「未保存」。
    saved_text: String,
    /// 新建/打开之后立刻应用一次，不等防抖。
    force: bool,
}

impl SceneDoc {
    /// 空白场景：`SceneManifest::empty` 的格式化输出。没有文件路径，因此算「未保存」。
    pub fn blank() -> Self {
        let text = pretty(&manifest_to_value(&SceneManifest::empty("未命名")));
        Self {
            text,
            path: None,
            attempted: String::new(),
            changed_at: None,
            saved_text: String::new(),
            force: true,
        }
    }

    /// 打开一个场景文件；读失败只返回错误，调用方保留原有文档。
    pub fn open(path: PathBuf) -> Result<Self, String> {
        let text = fs::read_to_string(&path)
            .map_err(|e| format!("读取 {} 失败：{e}", path.display()))?;
        Ok(Self {
            text,
            path: Some(path),
            attempted: String::new(),
            changed_at: None,
            saved_text: String::new(),
            force: true,
        })
    }

    /// 用户改了文本。内容没变时什么都不做（egui 每次输入都会回调）。
    pub fn set_text(&mut self, text: String, now: Instant) {
        if text == self.text {
            return;
        }
        self.text = text;
        self.changed_at = Some(now);
    }

    /// 是否该应用：新建/打开后的首次，或改动后停顿够久且还没试过这份文本。
    pub fn due(&self, now: Instant) -> bool {
        if self.force {
            return true;
        }
        match self.changed_at {
            Some(at) => self.text != self.attempted && now.duration_since(at) >= APPLY_DEBOUNCE,
            None => false,
        }
    }

    /// 取走「要应用的文本」，并把这份文本记为已尝试。
    pub fn take_attempt(&mut self) -> String {
        self.attempted = self.text.clone();
        self.changed_at = None;
        self.force = false;
        self.text.clone()
    }

    /// 请求立刻应用一次（不等防抖）：新建/打开/拖入模型后用。
    pub fn request_apply(&mut self) {
        self.force = true;
    }

    /// 当前文本与磁盘内容不同（新建且未保存时恒为真）。
    pub fn is_dirty(&self) -> bool {
        self.path.is_none() || self.text != self.saved_text
    }

    /// 写回当前路径。
    pub fn save(&mut self) -> Result<(), String> {
        match self.path.clone() {
            Some(path) => self.write_to(path),
            None => Err("场景还没有文件路径".to_string()),
        }
    }

    /// 写到指定路径（另存为），并把它记为已保存。
    pub fn write_to(&mut self, path: PathBuf) -> Result<(), String> {
        if let Err(e) = fs::write(&path, &self.text) {
            log::warn!(
                "写入 {} 失败：{e:?}（kind={:?}，raw={:?}，元数据={:?}）",
                path.display(),
                e.kind(),
                e.raw_os_error(),
                fs::metadata(&path).map(|m| m.permissions().readonly())
            );
            return Err(format!("写入 {} 失败：{e}", path.display()));
        }
        self.path = Some(path);
        self.saved_text = self.text.clone();
        Ok(())
    }

    /// 文件显示名。
    pub fn file_name(&self) -> String {
        self.path
            .as_ref()
            .and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_string()))
            .unwrap_or_else(|| "未命名.scene.json".to_string())
    }

    /// 场景文件所在目录；没有路径时用 `<workdir>/assets/scenes`（模型路径相对它的基准）。
    pub fn base_dir(&self, workdir: &Path) -> PathBuf {
        self.path
            .as_ref()
            .and_then(|p| p.parent().map(Path::to_path_buf))
            .unwrap_or_else(|| workdir.join("assets/scenes"))
    }
}

/// 解析文本为 `SceneManifest`；错误带出行号。
pub fn parse_manifest(text: &str) -> Result<SceneManifest, DocError> {
    serde_json::from_str::<SceneManifest>(text).map_err(|e| DocError {
        message: e.to_string(),
        line: Some(e.line()),
    })
}

/// 解析文本为未定型的 JSON 值。
pub fn parse_value(text: &str) -> Result<Value, DocError> {
    serde_json::from_str::<Value>(text).map_err(|e| DocError {
        message: e.to_string(),
        line: Some(e.line()),
    })
}

/// 2 空格缩进 + 末尾换行，与仓库里已有的 `.scene.json` 一致。
pub fn pretty(value: &Value) -> String {
    let mut text = serde_json::to_string_pretty(value).unwrap_or_else(|_| "{}".to_string());
    text.push('\n');
    text
}

/// `SceneManifest` → JSON 值，用于「新建」时的模板。
pub fn manifest_to_value(manifest: &SceneManifest) -> Value {
    serde_json::to_value(manifest).unwrap_or(Value::Null)
}

/// 往场景文本的 `models` 数组末尾追加一条模型引用。
///
/// - `base_dir` 是场景文件所在目录，`model_path` 是模型文件的绝对路径；
/// - `translation` 是落点（y 恒为 0，按 0.01 精度取整）；
/// - 文本解析失败、顶层不是对象、`models` 不是数组都返回 `Err`，原文本不动。
pub fn insert_model(
    text: &str,
    base_dir: &Path,
    model_path: &Path,
    translation: [f32; 3],
) -> Result<String, DocError> {
    let mut value = parse_value(text)?;
    let root = value
        .as_object_mut()
        .ok_or_else(|| DocError::whole("场景 JSON 的顶层必须是对象"))?;

    let rel = relative_path(base_dir, model_path);
    let model_path_str = rel.to_string_lossy().replace('\\', "/");

    let models = root
        .entry("models".to_string())
        .or_insert_with(|| Value::Array(Vec::new()));
    let models = models
        .as_array_mut()
        .ok_or_else(|| DocError::whole("`models` 必须是数组"))?;

    let existing: Vec<String> = models
        .iter()
        .filter_map(|m| m.get("id").and_then(Value::as_str).map(str::to_owned))
        .collect();
    let id = unique_model_id(&model_id_base(model_path), &existing);

    let mut entry = Map::new();
    entry.insert("id".to_string(), Value::String(id));
    entry.insert("path".to_string(), Value::String(model_path_str));

    let mut transform = Map::new();
    transform.insert(
        "translation".to_string(),
        Value::Array(translation.iter().map(|v| Value::from(round2(*v))).collect()),
    );
    entry.insert("transform".to_string(), Value::Object(transform));

    models.push(Value::Object(entry));
    Ok(pretty(&value))
}

/// 保留两位小数，并把 `-0.0` 归一成 `0.0`（否则 JSON 里会出现 `-0.0`）。
///
/// 取整在 f64 域里做：f32 的 1.23 转成 f64 是 1.2300000190734863，而
/// `123.0 / 100.0` 能序列化成干净的 `1.23`。
fn round2(v: f32) -> f64 {
    let r = (v as f64 * 100.0).round() / 100.0;
    if r == 0.0 { 0.0 } else { r }
}

/// 由模型文件名生成 id 的基底：非字母数字换成 `_`，取不到文件名用 `model`。
pub fn model_id_base(model_path: &Path) -> String {
    let stem = model_path
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    let id: String = stem
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '_' || c == '-' { c } else { '_' })
        .collect();
    let id = id.trim_matches('_').to_string();
    if id.is_empty() { "model".to_string() } else { id }
}

/// 在已有 id 里去重：`chair` 已存在则依次试 `chair_2`、`chair_3`……
pub fn unique_model_id(base: &str, existing: &[String]) -> String {
    if !existing.iter().any(|id| id == base) {
        return base.to_string();
    }
    let mut n = 2;
    loop {
        let candidate = format!("{base}_{n}");
        if !existing.iter().any(|id| *id == candidate) {
            return candidate;
        }
        n += 1;
    }
}

/// 把 `to` 表示成相对 `from_dir` 的路径。
///
/// 两边同为绝对路径（或同为相对路径）时用 `..` 拼出相对路径；跨盘符、绝对/相对混用
/// 这类无法表达的情况直接原样返回 `to`（模型列表因此写绝对路径，loader 也接受）。
pub fn relative_path(from_dir: &Path, to: &Path) -> PathBuf {
    if from_dir.is_absolute() != to.is_absolute() {
        return to.to_path_buf();
    }
    let from = normalize(from_dir);
    let target = normalize(to);

    let common = from
        .iter()
        .zip(target.iter())
        .take_while(|(a, b)| a == b)
        .count();

    if common == 0 {
        return to.to_path_buf();
    }

    let mut rel = PathBuf::new();
    for _ in common..from.len() {
        rel.push("..");
    }
    for component in &target[common..] {
        rel.push(component);
    }
    if rel.as_os_str().is_empty() {
        PathBuf::from(".")
    } else {
        rel
    }
}

/// 去掉 `.` 与可消的 `..` 之后的组件序列（用 `OsString` 保留原始形式）。
fn normalize(path: &Path) -> Vec<std::ffi::OsString> {
    let mut out: Vec<std::ffi::OsString> = Vec::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                let pops = matches!(out.last().map(|s| s.to_str()), Some(Some(name)) if name != ".." && name != ".");
                if pops {
                    out.pop();
                } else {
                    out.push(component.as_os_str().to_owned());
                }
            }
            other => out.push(other.as_os_str().to_owned()),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "scene-editor-doc-{}-{tag}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn blank_scene_parses_as_manifest() {
        let doc = SceneDoc::blank();
        let manifest = parse_manifest(&doc.text).unwrap();
        assert_eq!(manifest.version, "1.0");
        assert!(manifest.models.is_empty());
        assert!(doc.is_dirty(), "还没落盘的场景永远算未保存");
    }

    #[test]
    fn insert_model_appends_relative_path_and_dedups_id() {
        let dir = tmp_dir("insert");
        let scenes = dir.join("assets/scenes");
        fs::create_dir_all(&scenes).unwrap();
        let model = dir.join("assets/models/chair.glb");

        let text = pretty(&manifest_to_value(&SceneManifest::empty("t")));
        let once = insert_model(&text, &scenes, &model, [1.234, 0.0, -2.0]).unwrap();
        let value: Value = serde_json::from_str(&once).unwrap();
        let models = value["models"].as_array().unwrap();
        assert_eq!(models.len(), 1);
        assert_eq!(models[0]["id"], "chair");
        assert_eq!(models[0]["path"], "../models/chair.glb");
        assert_eq!(models[0]["transform"]["translation"][0], 1.23);
        assert_eq!(models[0]["transform"]["translation"][2], -2.0);

        let twice = insert_model(&once, &scenes, &model, [0.0, 0.0, 0.0]).unwrap();
        let value: Value = serde_json::from_str(&twice).unwrap();
        let ids: Vec<&str> = value["models"]
            .as_array()
            .unwrap()
            .iter()
            .map(|m| m["id"].as_str().unwrap())
            .collect();
        assert_eq!(ids, vec!["chair", "chair_2"]);
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn insert_model_rejects_broken_text_and_bad_models_field() {
        let err = insert_model("{ not json", Path::new("."), Path::new("a.glb"), [0.0; 3]).unwrap_err();
        assert!(err.line.is_some(), "解析错误要能定位到行");

        let err = insert_model(r#"{"models": 3}"#, Path::new("."), Path::new("a.glb"), [0.0; 3])
            .unwrap_err();
        assert!(err.message.contains("models"));

        let err = insert_model("[]", Path::new("."), Path::new("a.glb"), [0.0; 3]).unwrap_err();
        assert!(err.message.contains("顶层"));
    }

    #[test]
    fn insert_model_tolerates_missing_models_array() {
        let text = "{\n  \"version\": \"1.0\",\n  \"name\": \"t\"\n}\n";
        let out = insert_model(text, Path::new("/a/scenes"), Path::new("/a/models/box.glb"), [0.0; 3])
            .unwrap();
        let value: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(value["models"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn relative_path_handles_siblings_parents_and_foreign_roots() {
        assert_eq!(
            relative_path(Path::new("/a/b/scenes"), Path::new("/a/b/models/x.glb")),
            PathBuf::from("../models/x.glb")
        );
        assert_eq!(
            relative_path(Path::new("/a/b"), Path::new("/a/b/c/d.glb")),
            PathBuf::from("c/d.glb")
        );
        assert_eq!(
            relative_path(Path::new("/a/b"), Path::new("/a/b")),
            PathBuf::from(".")
        );
        // 跨盘符 / 绝对对相对：原样返回
        assert_eq!(
            relative_path(Path::new("/a"), Path::new("D:/m/x.glb")),
            PathBuf::from("D:/m/x.glb")
        );
        // `..` 可消
        assert_eq!(
            relative_path(Path::new("/a/b/../b/scenes"), Path::new("/a/b/models/x.glb")),
            PathBuf::from("../models/x.glb")
        );
    }

    #[test]
    fn model_id_base_sanitizes_names() {
        assert_eq!(model_id_base(Path::new("a/Chair 01.glb")), "Chair_01");
        assert_eq!(model_id_base(Path::new("a/box.gltf")), "box");
        assert_eq!(model_id_base(Path::new("a/...glb")), "model");
    }

    #[test]
    fn unique_model_id_walks_suffixes() {
        let existing = vec!["box".to_string(), "box_2".to_string()];
        assert_eq!(unique_model_id("box", &existing), "box_3");
        assert_eq!(unique_model_id("chair", &existing), "chair");
    }

    #[test]
    fn debounce_waits_for_quiet_period_then_blocks_repeat_attempt() {
        let mut doc = SceneDoc::blank();
        doc.force = false;
        doc.attempted = doc.text.clone();
        let now = Instant::now();
        doc.set_text("{\"name\":\"x\"}".to_string(), now);
        assert!(!doc.due(now + APPLY_DEBOUNCE / 2), "停顿不足时不该应用");
        assert!(doc.due(now + APPLY_DEBOUNCE), "停顿够了就该应用");

        let attempt = doc.take_attempt();
        assert_eq!(attempt, doc.text);
        assert!(!doc.due(now + APPLY_DEBOUNCE * 4), "同一份文本不重复应用");
    }

    #[test]
    fn write_to_clears_dirty_and_keeps_path() {
        let dir = tmp_dir("save");
        let path = dir.join("a.scene.json");
        let mut doc = SceneDoc::blank();
        assert!(doc.save().is_err(), "没有路径时保存报错");
        doc.write_to(path.clone()).unwrap();
        assert!(!doc.is_dirty());
        assert_eq!(fs::read_to_string(&path).unwrap(), doc.text);
        assert_eq!(doc.file_name(), "a.scene.json");
        fs::remove_dir_all(&dir).ok();
    }
}
