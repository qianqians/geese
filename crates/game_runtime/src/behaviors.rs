//! 场景物件的 Python behavior 脚本宿主。
//!
//! 场景条目里的 `script: [{ path, class, props }]` 在这里变成真正的 Python 对象：
//!
//! - 脚本类继承引擎提供的 `behavior`（既是内置模块 `geese`，也注入脚本全局）；
//! - `start()` 在场景加载完成、所有脚本实例化之后调一次，每帧 `update(dt)`，
//!   对象销毁/窗口关闭时 `end()`；
//! - `self.transform` 是所属对象节点的**局部变换**（translation / rotation 欧拉角度 / scale），
//!   脚本改完由调用方随后的 `scene.update_world_transforms()` 生效。
//!
//! 错误策略（与用户确认过）：任何一步失败都返回 `Err`，调用方打日志后退出窗口——
//! 不静默跳过、不每帧重试。
//!
//! Feature gate：`behaviors`（依赖 `python-runtime`）。

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use cgmath::{Deg, Euler, Quaternion, Rad};
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyModule};
use scene::{Scene, ScriptDef};

/// 内置模块名：脚本可 `from geese import behavior`。
const MODULE_NAME: &str = "geese";

// ---------------------------------------------------------------------------
// Python 侧的两个类
// ---------------------------------------------------------------------------

/// 对象的局部变换，脚本里通过 `self.transform` 读写。
///
/// 欧拉角单位是度，与场景 JSON 的 `rotation_euler` 一致。
#[pyclass(name = "transform")]
pub struct PyTransform {
    #[pyo3(get, set)]
    pub translation: [f32; 3],
    #[pyo3(get, set)]
    pub rotation: [f32; 3],
    #[pyo3(get, set)]
    pub scale: [f32; 3],
}

impl PyTransform {
    fn snapshot(py: Python<'_>) -> Py<PyTransform> {
        Py::new(
            py,
            PyTransform {
                translation: [0.0; 3],
                rotation: [0.0; 3],
                scale: [1.0; 3],
            },
        )
        .expect("分配 transform 对象失败")
    }
}

/// behavior 基类。脚本类继承它，引擎负责实例化与驱动。
///
/// `entity_id` 是 behavior 实例自己的 uuid（构造时生成），不是场景对象的 entity id；
/// 场景对象的变换通过 `self.transform` 读写。
#[pyclass(subclass, name = "behavior")]
pub struct PyBehavior {
    entity_id: String,
    entity_type: String,
    props: Py<PyAny>,
    enabled: bool,
    dt: f32,
    transform: Py<PyTransform>,
}

#[pymethods]
impl PyBehavior {
    #[new]
    #[pyo3(signature = (**props))]
    fn py_new(py: Python<'_>, props: Option<Bound<'_, PyDict>>) -> PyResult<Self> {
        let props: Py<PyAny> = match props {
            Some(dict) => dict.unbind().into_any(),
            None => PyDict::new(py).unbind().into_any(),
        };
        Ok(Self {
            entity_id: uuid::Uuid::new_v4().to_string(),
            entity_type: String::new(),
            props,
            enabled: true,
            dt: 0.0,
            transform: PyTransform::snapshot(py),
        })
    }

    /// behavior 实例自己的 uuid。
    #[getter]
    fn entity_id(&self) -> &str {
        &self.entity_id
    }

    /// 所属对象的类型（程序化对象为 `object_type`，模型条目为 `model`）。
    #[getter]
    fn entity_type(&self) -> &str {
        &self.entity_type
    }

    /// 场景里给这个脚本的 `props`。
    #[getter]
    fn props(&self, py: Python<'_>) -> Py<PyAny> {
        self.props.clone_ref(py)
    }

    /// 置 false 后引擎不再调用 `update`（`start`/`end` 照常）。
    #[getter]
    fn enabled(&self) -> bool {
        self.enabled
    }

    #[setter]
    fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    /// 当前帧间隔（只在 `update` 期间保证是最新的）。
    #[getter]
    fn dt(&self) -> f32 {
        self.dt
    }

    /// 所属对象节点的局部变换，可直接改字段。
    #[getter]
    fn transform(&self, py: Python<'_>) -> Py<PyTransform> {
        self.transform.clone_ref(py)
    }

    #[setter]
    fn set_transform(&mut self, value: Py<PyTransform>) {
        self.transform = value;
    }

    /// 生命周期：场景加载完成后调用一次。
    fn start(&self) {}

    /// 生命周期：每帧调用。
    fn update(&self, _dt: f32) {}

    /// 生命周期：对象销毁 / 窗口关闭时调用。
    fn end(&self) {}
}

// ---------------------------------------------------------------------------
// 宿主
// ---------------------------------------------------------------------------

/// 一个已实例化的脚本。
struct Instance {
    /// `Scene::objects` 里的下标。
    object: usize,
    /// 上一次写进 behavior 的变换，用来判断脚本有没有改过。
    applied: [[f32; 3]; 3],
    behavior: Py<PyAny>,
    class: String,
    path: PathBuf,
}

/// 场景里所有脚本的集合。
///
/// 场景是「可变」的：Python 游戏可能在运行时才 `load_scene` 或增删对象，所以宿主不假设
/// 一次加载定终身——每帧用 [`BehaviorHost::sync`] 比对场景里的脚本集合签名，变了就
/// 「全部 `end()` → 重新实例化 → 全部 `start()`」。
pub struct BehaviorHost {
    project_root: PathBuf,
    /// 脚本文件的 Python 模块缓存：同一份源码只编译一次，多个对象共享。
    modules: HashMap<PathBuf, Py<PyModule>>,
    /// 按场景声明顺序：`start` 正序，`end` 逆序。
    instances: Vec<Instance>,
    /// 上一次构建时的脚本集合签名。
    signature: u64,
}

impl BehaviorHost {
    /// 建一个空宿主；真正实例化发生在第一次 [`Self::sync`]。
    ///
    /// 不需要 GIL、不会失败——所以能在 `GameState::new` 里直接放进来。
    pub fn new(project_root: &Path) -> Self {
        Self {
            project_root: project_root.to_path_buf(),
            modules: HashMap::new(),
            instances: Vec::new(),
            signature: 0,
        }
    }

    /// 场景的脚本集合变了就重建：先 `end()` 旧的，再实例化 + `start()` 新的。
    ///
    /// 场景里没有脚本时什么也不做（`signature` 会记住空集合）。
    pub fn sync(&mut self, py: Python<'_>, scene: &Scene) -> PyResult<()> {
        let signature = scene_script_signature(scene);
        if signature == self.signature {
            return Ok(());
        }
        self.end(py)?;
        self.signature = signature;

        if self.instances_expected(scene) == 0 {
            return Ok(());
        }

        register_geese_module(py)?;
        for (object_index, object) in scene.objects.iter().enumerate() {
            for script in &object.scripts {
                let instance = self.instantiate(py, object_index, object, script)?;
                self.instances.push(instance);
            }
        }
        log::info!("[behavior] 实例化 {} 个脚本", self.instances.len());
        for instance in &self.instances {
            instance
                .behavior
                .bind(py)
                .call_method0("start")
                .map_err(|error| script_error(&instance.path, &instance.class, "start", &error))?;
        }
        Ok(())
    }

    fn instances_expected(&self, scene: &Scene) -> usize {
        scene.objects.iter().map(|object| object.scripts.len()).sum()
    }

    /// 场景里有脚本吗（决定调用方是否要每帧重算世界矩阵）。
    pub fn is_empty(&self) -> bool {
        self.instances.is_empty()
    }
    pub fn len(&self) -> usize {
        self.instances.len()
    }

    /// 每帧驱动：同步变换与 dt → `update(dt)` → 把脚本改过的变换写回对象节点。
    pub fn update(&mut self, py: Python<'_>, scene: &mut Scene, dt: f32) -> PyResult<()> {
        for instance in &mut self.instances {
            let Some(parts) = read_transform(scene, instance.object) else {
                // 对象被删了：脚本留着但不再驱动（end 在退出时统一调）。
                continue;
            };

            let bound = instance.behavior.bind(py);
            let base = bound.downcast::<PyBehavior>()?;
            {
                let mut base = base.borrow_mut();
                base.dt = dt;
                let mut transform = base.transform.bind(py).borrow_mut();
                transform.translation = parts.0;
                transform.rotation = parts.1;
                transform.scale = parts.2;
            }
            instance.applied = [parts.0, parts.1, parts.2];

            if !base.borrow().enabled {
                continue;
            }

            bound.call_method1("update", (dt,)).map_err(|error| {
                script_error(&instance.path, &instance.class, "update", &error)
            })?;

            let after = {
                let base = base.borrow();
                let transform = base.transform.bind(py).borrow();
                [transform.translation, transform.rotation, transform.scale]
            };
            if after != instance.applied {
                write_transform(scene, instance.object, after);
                instance.applied = after;
            }
        }
        Ok(())
    }

    /// 逆序调用所有脚本的 `end()`（对象创建顺序的逆序）。
    pub fn end(&mut self, py: Python<'_>) -> PyResult<()> {
        for instance in self.instances.iter().rev() {
            instance
                .behavior
                .bind(py)
                .call_method0("end")
                .map_err(|error| script_error(&instance.path, &instance.class, "end", &error))?;
        }
        self.instances.clear();
        Ok(())
    }

    fn instantiate(
        &mut self,
        py: Python<'_>,
        object_index: usize,
        object: &scene::SceneObject,
        script: &ScriptDef,
    ) -> PyResult<Instance> {
        let path = resolve_script_path(&self.project_root, &script.path);
        let module = self.load_module(py, &path)?;
        let class = module.bind(py).getattr(script.class.as_str()).map_err(|error| {
            pyo3::exceptions::PyAttributeError::new_err(format!(
                "脚本 {} 里找不到类 `{}`：{error}",
                path.display(),
                script.class
            ))
        })?;

        let kwargs = props_to_kwargs(py, &script.props)?;
        let behavior = class.call((), Some(&kwargs))?.unbind();

        // 引擎负责补齐基类字段：子类自定义 `__init__` 不调 super 也能用。
        {
            let base = behavior.bind(py).downcast::<PyBehavior>().map_err(|error| {
                pyo3::exceptions::PyTypeError::new_err(format!(
                    "脚本 {} 的类 `{}` 没有继承 behavior：{error}",
                    path.display(),
                    script.class
                ))
            })?;
            let mut base = base.borrow_mut();
            base.entity_type = object.entity_type.clone();
        }

        Ok(Instance {
            object: object_index,
            applied: [[0.0; 3], [0.0; 3], [1.0; 3]],
            behavior,
            class: script.class.clone(),
            path,
        })
    }

    fn load_module(&mut self, py: Python<'_>, path: &Path) -> PyResult<Py<PyModule>> {
        if let Some(module) = self.modules.get(path) {
            return Ok(module.clone_ref(py));
        }

        let source = std::fs::read_to_string(path).map_err(|error| {
            pyo3::exceptions::PyIOError::new_err(format!("读取脚本 {} 失败：{error}", path.display()))
        })?;
        let module_name = module_name_for(path);
        let code = std::ffi::CString::new(source)
            .map_err(|_| pyo3::exceptions::PyValueError::new_err(
                format!("脚本 {} 含 NUL 字节，无法编译", path.display()),
            ))?;

        // 先把基类放进模块字典再执行源码：脚本里 `class X(behavior)` 不写 import 也能用。
        // （不能用 `PyModule::from_code`——它边创建边执行，来不及注入。）
        let module = PyModule::new(py, &module_name)?;
        let behavior = py.import(MODULE_NAME)?.getattr("behavior")?;
        module.setattr("behavior", behavior)?;
        let globals = module.dict();
        py.run(&code, Some(&globals), None)
            .map_err(|error| pyo3::exceptions::PyRuntimeError::new_err(
                format!("执行脚本 {} 失败：{error}", path.display()),
            ))?;

        let module = module.unbind();
        self.modules.insert(path.to_path_buf(), module.clone_ref(py));
        Ok(module)
    }
}

/// 把 `geese` 模块注册进 `sys.modules`（已存在则跳过）。
fn register_geese_module(py: Python<'_>) -> PyResult<()> {
    let sys = py.import("sys")?;
    let modules = sys.getattr("modules")?;
    if modules.contains(MODULE_NAME)? {
        return Ok(());
    }
    let module = PyModule::new(py, MODULE_NAME)?;
    module.add_class::<PyBehavior>()?;
    module.add_class::<PyTransform>()?;
    module.add("__doc__", "Geese 引擎脚本基类模块")?;
    modules.set_item(MODULE_NAME, module)?;
    Ok(())
}

/// 场景里所有脚本声明 + 所属对象下标 的哈希，用来判断要不要重建宿主。
fn scene_script_signature(scene: &Scene) -> u64 {
    // FNV-1a：够用且不引额外依赖
    let mut hash: u64 = 1469598103934665603;
    let mut feed = |bytes: &[u8]| {
        for byte in bytes {
            hash ^= *byte as u64;
            hash = hash.wrapping_mul(1099511628211);
        }
    };
    feed(&(scene.objects.len() as u64).to_le_bytes());
    for (index, object) in scene.objects.iter().enumerate() {
        if object.scripts.is_empty() {
            continue;
        }
        feed(&(index as u64).to_le_bytes());
        for script in &object.scripts {
            feed(script.path.as_bytes());
            feed(&[0]);
            feed(script.class.as_bytes());
            feed(&[0]);
            let props = serde_json::Value::Object(script.props.clone()).to_string();
            feed(props.as_bytes());
            feed(&[0xff]);
        }
    }
    hash
}

/// 工程根：`launch_python_game` 会把它记下来；没记过就退回进程工作目录。
static PROJECT_ROOT: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();

/// 记下工程根（Python 游戏入口调用；多次调用只有第一次生效）。
pub fn set_project_root(path: &Path) {
    let _ = PROJECT_ROOT.set(path.to_path_buf());
}

/// 当前工程根：脚本路径相对它解析。
pub fn project_root() -> PathBuf {
    PROJECT_ROOT
        .get()
        .cloned()
        .or_else(|| std::env::current_dir().ok())
        .unwrap_or_else(|| PathBuf::from("."))
}

/// 脚本路径相对工程根解析；已经是绝对路径就用原样。
pub fn resolve_script_path(project_root: &Path, declared: &str) -> PathBuf {
    let declared = PathBuf::from(declared);
    if declared.is_absolute() {
        declared
    } else {
        project_root.join(declared)
    }
}

/// 给 `PyModule::from_code` 用的唯一模块名：文件名净化 + 路径哈希，避免不同目录下同名脚本互相覆盖。
fn module_name_for(path: &Path) -> String {
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "script".to_string());
    let clean: String = stem
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();
    let mut hash: u64 = 1469598103934665603;
    for byte in path.display().to_string().as_bytes() {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(1099511628211);
    }
    format!("geese_script_{clean}_{hash:016x}")
}

fn props_to_kwargs<'py>(
    py: Python<'py>,
    props: &serde_json::Map<String, serde_json::Value>,
) -> PyResult<Bound<'py, PyDict>> {
    let kwargs = PyDict::new(py);
    for (key, value) in props {
        kwargs.set_item(key, json_to_py(py, value)?)?;
    }
    Ok(kwargs)
}

/// `serde_json::Value` → Python 值（只覆盖 props 会用到的几种）。
fn json_to_py(py: Python<'_>, value: &serde_json::Value) -> PyResult<Py<PyAny>> {
    use serde_json::Value;
    Ok(match value {
        Value::Null => py.None(),
        Value::Bool(b) => b.into_pyobject(py)?.to_owned().into_any().unbind(),
        Value::Number(number) => {
            if let Some(int) = number.as_i64() {
                int.into_pyobject(py)?.into_any().unbind()
            } else {
                let float = number.as_f64().unwrap_or(0.0);
                float.into_pyobject(py)?.into_any().unbind()
            }
        }
        Value::String(text) => text.as_str().into_pyobject(py)?.into_any().unbind(),
        Value::Array(items) => {
            let list = pyo3::types::PyList::empty(py);
            for item in items {
                list.append(json_to_py(py, item)?)?;
            }
            list.unbind().into_any()
        }
        Value::Object(map) => {
            let dict = PyDict::new(py);
            for (key, item) in map {
                dict.set_item(key, json_to_py(py, item)?)?;
            }
            dict.unbind().into_any()
        }
    })
}

/// 读对象的局部变换：`(translation, rotation 欧拉角度, scale)`。
fn read_transform(scene: &Scene, object_index: usize) -> Option<([f32; 3], [f32; 3], [f32; 3])> {
    let object = scene.objects.get(object_index)?;
    let node = scene.nodes.get(object.node)?;
    let transform = &node.local_transform;
    let euler: Euler<Rad<f32>> = Euler::from(transform.rotation);
    Some((
        transform.translation.into(),
        [
            euler.x.0.to_degrees(),
            euler.y.0.to_degrees(),
            euler.z.0.to_degrees(),
        ],
        transform.scale.into(),
    ))
}

/// 把 `(translation, rotation 欧拉角度, scale)` 写回对象的节点。
fn write_transform(scene: &mut Scene, object_index: usize, parts: [[f32; 3]; 3]) {
    let Some(object) = scene.objects.get(object_index) else {
        return;
    };
    let node_index = object.node;
    let Some(node) = scene.nodes.get_mut(node_index) else {
        return;
    };
    let [translation, rotation_deg, scale] = parts;
    node.local_transform.translation = translation.into();
    let euler: Euler<Deg<f32>> = Euler::new(
        Deg(rotation_deg[0]),
        Deg(rotation_deg[1]),
        Deg(rotation_deg[2]),
    );
    node.local_transform.rotation = Quaternion::from(euler);
    node.local_transform.scale = scale.into();
}

fn script_error(path: &Path, class: &str, hook: &str, error: &PyErr) -> PyErr {
    let message = Python::with_gil(|py| {
        error
            .value(py)
            .str()
            .map(|text| text.to_string_lossy().into_owned())
            .unwrap_or_else(|_| format!("{error:?}"))
    });
    pyo3::exceptions::PyRuntimeError::new_err(format!(
        "脚本 {} 的类 `{class}` 在 {hook}() 里出错：{message}",
        path.display()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn script_path_resolves_against_project_root() {
        let root = Path::new("/proj");
        assert_eq!(
            resolve_script_path(root, "scripts/cube.py"),
            PathBuf::from("/proj/scripts/cube.py")
        );
        // 绝对路径原样用（跨盘符的工作目录场景）
        let absolute = if cfg!(windows) { "D:/elsewhere/x.py" } else { "/elsewhere/x.py" };
        assert_eq!(
            resolve_script_path(root, absolute),
            PathBuf::from(absolute)
        );
    }

    #[test]
    fn module_name_is_unique_per_path_and_importable() {
        let a = module_name_for(Path::new("/proj/a/cube.py"));
        let b = module_name_for(Path::new("/proj/b/cube.py"));
        assert_ne!(a, b, "同名脚本在不同目录不能共用模块名");
        assert!(a.starts_with("geese_script_cube_"));
        assert!(
            a.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'),
            "{a} 必须是合法标识符"
        );
    }
}
