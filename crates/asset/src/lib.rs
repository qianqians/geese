//! 资源管线（骨架）。
//!
//! 在原 `gltf::import` 包装的基础上引入统一的资源管理抽象：
//! - [`Handle<T>`]：轻量引用计数句柄，业务层只持 `Handle`，不直接持数据。
//! - [`AssetLoader<T>`]：每类资源（纹理/网格/音频/着色器）的同步加载器。
//! - [`AssetCache`]：按 `(TypeId, path)` 去重的缓存，避免重复解码。
//! - [`LoadError`]：统一错误类型。
//!
//! 骨架阶段实现同步加载;后续"可用级"将加入异步加载（tokio）、文件热重载
//! （notify）、KTX2/Basis 纹理解码、meshopt 顶点优化等。

pub mod meta;
pub mod database;
pub mod dependency_scanner;
pub mod bundle;

use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

// ---------------------------------------------------------------------------
// 现有 gltf 加载（保留向后兼容）
// ---------------------------------------------------------------------------

/// 按文件路径同步加载 glTF 资源。骨架阶段保留原签名以兼容上层调用。
pub fn load(
    path: String,
) -> Result<
    (
        gltf::Document,
        Vec<gltf::buffer::Data>,
        Vec<gltf::image::Data>,
    ),
    Box<dyn std::error::Error>,
> {
    let gltf = gltf::import(path)?;
    Ok(gltf)
}

// ---------------------------------------------------------------------------
// Handle
// ---------------------------------------------------------------------------

/// 资源句柄。`Clone` 廉价;释放最后一份会触发底层数据 drop（但不会从 cache 中
/// 移除——缓存以 `Arc` 强引用，需显式 `AssetCache::evict` 清理）。
#[derive(Debug)]
pub struct Handle<T: ?Sized> {
    inner: Arc<T>,
    path: Arc<str>,
}

impl<T: ?Sized> Clone for Handle<T> {
    fn clone(&self) -> Self {
        Self { inner: self.inner.clone(), path: self.path.clone() }
    }
}

impl<T: ?Sized> Handle<T> {
    pub fn path(&self) -> &str {
        &self.path
    }

    pub fn inner(&self) -> &Arc<T> {
        &self.inner
    }

    /// 当前的强引用计数（含 cache 自身的一份）。仅用于诊断/测试。
    pub fn strong_count(&self) -> usize {
        Arc::strong_count(&self.inner)
    }
}

impl<T> Handle<T> {
    pub fn new(path: impl Into<Arc<str>>, value: T) -> Self {
        Self { inner: Arc::new(value), path: path.into() }
    }
}

impl<T: ?Sized> std::ops::Deref for Handle<T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.inner
    }
}

// ---------------------------------------------------------------------------
// 错误
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub enum LoadError {
    NotFound(PathBuf),
    Io(std::io::Error),
    Decode(String),
}

impl std::fmt::Display for LoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LoadError::NotFound(p) => write!(f, "asset not found: {}", p.display()),
            LoadError::Io(e) => write!(f, "asset io error: {e}"),
            LoadError::Decode(msg) => write!(f, "asset decode failed: {msg}"),
        }
    }
}

impl std::error::Error for LoadError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            LoadError::Io(e) => Some(e),
            _ => None,
        }
    }
}

impl From<std::io::Error> for LoadError {
    fn from(e: std::io::Error) -> Self {
        LoadError::Io(e)
    }
}

// ---------------------------------------------------------------------------
// AssetLoader trait
// ---------------------------------------------------------------------------

/// 单类资源的加载器。
pub trait AssetLoader<T>: Send + Sync
where
    T: Send + Sync + 'static,
{
    fn load(&self, path: &str) -> Result<T, LoadError>;
}

// ---------------------------------------------------------------------------
// AssetCache
// ---------------------------------------------------------------------------

/// 按 `(TypeId, path)` 去重的强引用缓存。
///
/// 主要 API：
/// - [`AssetCache::get_or_load`]：命中即返回 Handle;未命中调用 loader。
/// - [`AssetCache::insert`]：手动注入（适合测试或运行时生成的资源）。
/// - [`AssetCache::get`]：仅查询，不触发加载。
/// - [`AssetCache::evict`] / [`AssetCache::clear`]：释放缓存中的强引用。
#[derive(Default)]
pub struct AssetCache {
    entries: HashMap<(TypeId, String), Box<dyn Any + Send + Sync>>,
}

impl AssetCache {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }

    fn key<T: 'static>(path: &str) -> (TypeId, String) {
        (TypeId::of::<T>(), path.to_string())
    }

    pub fn insert<T>(&mut self, path: impl Into<String>, value: T) -> Handle<T>
    where
        T: Send + Sync + 'static,
    {
        let path: String = path.into();
        let handle = Handle::new(Arc::<str>::from(path.as_str()), value);
        self.entries.insert(Self::key::<T>(&path), Box::new(handle.clone()));
        handle
    }

    pub fn get<T>(&self, path: &str) -> Option<Handle<T>>
    where
        T: Send + Sync + 'static,
    {
        self.entries
            .get(&Self::key::<T>(path))
            .and_then(|b| b.downcast_ref::<Handle<T>>())
            .cloned()
    }

    pub fn get_or_load<T, L>(&mut self, path: &str, loader: &L) -> Result<Handle<T>, LoadError>
    where
        T: Send + Sync + 'static,
        L: AssetLoader<T>,
    {
        if let Some(h) = self.get::<T>(path) {
            return Ok(h);
        }
        let value = loader.load(path)?;
        Ok(self.insert(path, value))
    }

    /// 从缓存中移除指定 `(T, path)` 的条目（已有 Handle 仍有效，直到调用方释放）。
    pub fn evict<T>(&mut self, path: &str) -> bool
    where
        T: Send + Sync + 'static,
    {
        self.entries.remove(&Self::key::<T>(path)).is_some()
    }
}

// ---------------------------------------------------------------------------
// 测试
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// 测试用 loader：把 path 当成内容返回，并记录调用次数。
    struct EchoLoader {
        calls: AtomicUsize,
    }
    impl EchoLoader {
        fn new() -> Self {
            Self { calls: AtomicUsize::new(0) }
        }
        fn calls(&self) -> usize {
            self.calls.load(Ordering::Relaxed)
        }
    }
    impl AssetLoader<String> for EchoLoader {
        fn load(&self, path: &str) -> Result<String, LoadError> {
            self.calls.fetch_add(1, Ordering::Relaxed);
            if path == "missing" {
                return Err(LoadError::NotFound(PathBuf::from(path)));
            }
            Ok(format!("content:{path}"))
        }
    }

    /// 第二个类型，用于验证「同 path 不同 T」不冲突。
    #[derive(Debug, PartialEq)]
    struct Mesh(usize);
    struct MeshLoader;
    impl AssetLoader<Mesh> for MeshLoader {
        fn load(&self, path: &str) -> Result<Mesh, LoadError> {
            Ok(Mesh(path.len()))
        }
    }

    #[test]
    fn get_or_load_caches_on_second_call() {
        let mut cache = AssetCache::new();
        let loader = EchoLoader::new();
        let h1 = cache.get_or_load::<String, _>("a.txt", &loader).unwrap();
        let h2 = cache.get_or_load::<String, _>("a.txt", &loader).unwrap();
        assert_eq!(&*h1, "content:a.txt");
        assert_eq!(&*h2, "content:a.txt");
        assert_eq!(loader.calls(), 1, "loader called only once for cached path");
        // 同一底层 Arc
        assert!(Arc::ptr_eq(h1.inner(), h2.inner()));
    }

    #[test]
    fn same_path_different_types_dont_collide() {
        let mut cache = AssetCache::new();
        let text_loader = EchoLoader::new();
        let mesh_loader = MeshLoader;

        let t = cache.get_or_load::<String, _>("foo", &text_loader).unwrap();
        let m = cache.get_or_load::<Mesh, _>("foo", &mesh_loader).unwrap();
        assert_eq!(&*t, "content:foo");
        assert_eq!(*m, Mesh(3));
        assert_eq!(cache.len(), 2);
    }

    #[test]
    fn loader_error_propagates_and_does_not_cache() {
        let mut cache = AssetCache::new();
        let loader = EchoLoader::new();
        let err = cache.get_or_load::<String, _>("missing", &loader).unwrap_err();
        assert!(matches!(err, LoadError::NotFound(_)));
        assert_eq!(cache.len(), 0);
        // 第二次仍会调用 loader（未缓存负面结果）
        let _ = cache.get_or_load::<String, _>("missing", &loader);
        assert_eq!(loader.calls(), 2);
    }

    #[test]
    fn insert_then_get_returns_same_handle() {
        let mut cache = AssetCache::new();
        let h = cache.insert::<String>("manual", "hello".to_string());
        let g = cache.get::<String>("manual").unwrap();
        assert!(Arc::ptr_eq(h.inner(), g.inner()));
        assert_eq!(h.path(), "manual");
    }

    #[test]
    fn evict_removes_cache_entry_but_existing_handle_lives_on() {
        let mut cache = AssetCache::new();
        let h = cache.insert::<String>("x", "data".to_string());
        assert_eq!(cache.len(), 1);

        assert!(cache.evict::<String>("x"));
        assert_eq!(cache.len(), 0);
        // 旧 handle 仍能解引用
        assert_eq!(&*h, "data");
    }

    #[test]
    fn clear_drops_all_cached_entries() {
        let mut cache = AssetCache::new();
        cache.insert::<String>("a", "1".into());
        cache.insert::<String>("b", "2".into());
        cache.insert::<Mesh>("a", Mesh(7));
        assert_eq!(cache.len(), 3);
        cache.clear();
        assert!(cache.is_empty());
    }
}
