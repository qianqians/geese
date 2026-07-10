//! 异步资源加载器。
//!
//! `AsyncAssetCache` 包装 [`crate::AssetCache`]，在后台线程执行加载（`tokio::task::spawn_blocking`），
//! 完成后在主线程 `poll_completed()` 中 insert 到内部 cache，避免跨线程访问 cache。
//!
//! 不修改同步路径：`AssetCache` 和 [`crate::AssetLoader`] trait 保持不变。

use std::any::TypeId;
use std::collections::HashMap;
use std::sync::Arc;

use crate::{AssetCache, AssetLoader, Handle, LoadError};

/// 异步加载完成的回调 trait（类型擦除）。
trait PendingInsertion: Send {
    fn insert_into(self: Box<Self>, cache: &mut AssetCache);
}

/// 具体类型的待插入任务。
struct Insertion<T: Send + Sync + 'static> {
    path: String,
    result: Result<T, LoadError>,
    sender: Option<tokio::sync::oneshot::Sender<Result<Handle<T>, LoadError>>>,
}

impl<T: Send + Sync + 'static> PendingInsertion for Insertion<T> {
    fn insert_into(self: Box<Self>, cache: &mut AssetCache) {
        let Insertion { path, result, sender } = *self;
        match result {
            Ok(value) => {
                let handle = cache.insert(path, value);
                if let Some(sender) = sender {
                    let _ = sender.send(Ok(handle));
                }
            }
            Err(e) => {
                if let Some(sender) = sender {
                    let _ = sender.send(Err(e));
                }
            }
        }
    }
}

/// 异步资源缓存。
///
/// 内部持有同步 [`AssetCache`] + `tokio::runtime::Runtime` + 完成队列。
/// 加载完成后在主线程 `poll_completed()` 中 insert，避免跨线程访问 cache。
pub struct AsyncAssetCache {
    cache: AssetCache,
    runtime: tokio::runtime::Runtime,
    completed_rx: tokio::sync::mpsc::UnboundedReceiver<Box<dyn PendingInsertion>>,
    completed_tx: tokio::sync::mpsc::UnboundedSender<Box<dyn PendingInsertion>>,
    /// 已在加载中的请求（避免重复 spawn）
    pending: HashMap<(TypeId, String), usize>,
}

impl AsyncAssetCache {
    /// 创建一个新的异步资源缓存。
    ///
    /// 内部创建一个多线程 tokio runtime 用于 spawn 加载任务。
    pub fn new() -> Self {
        Self::with_runtime(
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("Failed to create tokio runtime for AsyncAssetCache"),
        )
    }

    /// 使用已有的 tokio runtime 创建。
    pub fn with_runtime(runtime: tokio::runtime::Runtime) -> Self {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        Self {
            cache: AssetCache::new(),
            runtime,
            completed_rx: rx,
            completed_tx: tx,
            pending: HashMap::new(),
        }
    }

    /// 从已有的同步缓存创建（保留已有条目）。
    pub fn from_cache(cache: AssetCache) -> Self {
        let mut s = Self::new();
        s.cache = cache;
        s
    }

    /// 内部同步缓存的只读引用。
    pub fn sync_cache(&self) -> &AssetCache {
        &self.cache
    }

    /// 内部同步缓存的可变引用（谨慎使用，避免与异步任务竞争）。
    pub fn sync_cache_mut(&mut self) -> &mut AssetCache {
        &mut self.cache
    }

    /// 直接查询缓存（不触发加载）。
    pub fn get<T>(&self, path: &str) -> Option<Handle<T>>
    where
        T: Send + Sync + 'static,
    {
        self.cache.get(path)
    }

    /// 异步请求加载资源。
    ///
    /// 如果缓存命中，立即通过 channel 返回。否则 spawn 一个 tokio 任务执行加载，
    /// 完成后通过 `poll_completed()` 在主线程 insert 到缓存并 resolve receiver。
    ///
    /// 返回 `oneshot::Receiver`，调用方 `await` 或 `try_recv()` 获取结果。
    pub fn request_async<T, L>(
        &self,
        path: &str,
        loader: Arc<L>,
    ) -> tokio::sync::oneshot::Receiver<Result<Handle<T>, LoadError>>
    where
        T: Send + Sync + 'static,
        L: AssetLoader<T> + 'static,
    {
        let (tx, rx) = tokio::sync::oneshot::channel();

        // 快速路径：缓存命中
        if let Some(h) = self.cache.get::<T>(path) {
            let _ = tx.send(Ok(h));
            return rx;
        }

        let path_owned = path.to_string();
        let completed_tx = self.completed_tx.clone();

        // spawn 加载任务
        self.runtime.spawn(async move {
            let result = loader.load(&path_owned);
            let insertion = Insertion::<T> {
                path: path_owned,
                result,
                sender: Some(tx),
            };
            let _ = completed_tx.send(Box::new(insertion));
        });

        rx
    }

    /// 主线程调用：drain 已完成的加载任务，insert 到内部 cache。
    ///
    /// 返回本帧处理的任务数量。
    pub fn poll_completed(&mut self) -> usize {
        let mut count = 0;
        loop {
            match self.completed_rx.try_recv() {
                Ok(insertion) => {
                    insertion.insert_into(&mut self.cache);
                    count += 1;
                }
                Err(tokio::sync::mpsc::error::TryRecvError::Empty) => break,
                Err(tokio::sync::mpsc::error::TryRecvError::Disconnected) => break,
            }
        }
        count
    }

    /// 清空内部缓存。
    pub fn clear(&mut self) {
        self.cache.clear();
        self.pending.clear();
    }

    /// 内部缓存条目数。
    pub fn len(&self) -> usize {
        self.cache.len()
    }

    pub fn is_empty(&self) -> bool {
        self.cache.is_empty()
    }
}

impl Default for AsyncAssetCache {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    struct EchoLoader {
        calls: AtomicUsize,
        delay: Option<std::time::Duration>,
    }

    impl EchoLoader {
        fn new() -> Self {
            Self {
                calls: AtomicUsize::new(0),
                delay: None,
            }
        }

        fn with_delay(delay: std::time::Duration) -> Self {
            Self {
                calls: AtomicUsize::new(0),
                delay: Some(delay),
            }
        }

        fn calls(&self) -> usize {
            self.calls.load(Ordering::Relaxed)
        }
    }

    impl AssetLoader<String> for EchoLoader {
        fn load(&self, path: &str) -> Result<String, LoadError> {
            self.calls.fetch_add(1, Ordering::Relaxed);
            if path == "missing" {
                return Err(LoadError::NotFound(std::path::PathBuf::from(path)));
            }
            if let Some(delay) = self.delay {
                std::thread::sleep(delay);
            }
            Ok(format!("content:{path}"))
        }
    }

    #[test]
    fn async_load_completes_after_poll() {
        let mut async_cache = AsyncAssetCache::new();
        let loader = Arc::new(EchoLoader::new());

        // Request async
        let mut rx = async_cache.request_async::<String, _>("test.txt", loader.clone());

        // Wait for the tokio task to finish
        std::thread::sleep(std::time::Duration::from_millis(50));

        // Poll completed
        let n = async_cache.poll_completed();
        assert_eq!(n, 1);

        // The oneshot should be resolved
        let result = rx.try_recv().expect("receiver should have result");
        assert!(result.is_ok());
        let handle = result.unwrap();
        assert_eq!(&*handle, "content:test.txt");

        // Cache should now have the asset
        assert!(async_cache.get::<String>("test.txt").is_some());
    }

    #[test]
    fn async_load_error_propagates() {
        let mut async_cache = AsyncAssetCache::new();
        let loader = Arc::new(EchoLoader::new());

        let mut rx = async_cache.request_async::<String, _>("missing", loader.clone());

        std::thread::sleep(std::time::Duration::from_millis(50));
        async_cache.poll_completed();

        let result = rx.try_recv().expect("receiver should have result");
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), LoadError::NotFound(_)));
    }

    #[test]
    fn cache_hit_returns_immediately() {
        let mut async_cache = AsyncAssetCache::new();

        // Pre-insert into cache
        async_cache.cache.insert("cached.txt", "cached_content".to_string());

        let loader = Arc::new(EchoLoader::new());
        let mut rx = async_cache.request_async::<String, _>("cached.txt", loader.clone());

        // Should resolve immediately without polling
        let result = rx.try_recv().expect("should resolve immediately");
        assert!(result.is_ok());
        assert_eq!(&*result.unwrap(), "cached_content");

        // Loader should not have been called
        assert_eq!(loader.calls(), 0);
    }

    #[test]
    fn async_load_multiple_concurrent() {
        let mut async_cache = AsyncAssetCache::new();
        let loader = Arc::new(EchoLoader::with_delay(std::time::Duration::from_millis(20)));

        let mut rx1 = async_cache.request_async::<String, _>("file1.txt", loader.clone());
        let mut rx2 = async_cache.request_async::<String, _>("file2.txt", loader.clone());
        let mut rx3 = async_cache.request_async::<String, _>("file3.txt", loader.clone());

        std::thread::sleep(std::time::Duration::from_millis(100));
        async_cache.poll_completed();

        let r1 = rx1.try_recv().expect("rx1");
        let r2 = rx2.try_recv().expect("rx2");
        let r3 = rx3.try_recv().expect("rx3");

        assert!(r1.is_ok());
        assert!(r2.is_ok());
        assert!(r3.is_ok());

        assert_eq!(&*r1.unwrap(), "content:file1.txt");
        assert_eq!(&*r2.unwrap(), "content:file2.txt");
        assert_eq!(&*r3.unwrap(), "content:file3.txt");
    }

    #[test]
    fn poll_completed_returns_zero_when_empty() {
        let mut async_cache = AsyncAssetCache::new();
        assert_eq!(async_cache.poll_completed(), 0);
    }
}
