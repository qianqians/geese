//! 资源热重载系统（可选 feature `hot-reload`）。
//!
//! 提供基于 `notify` crate 的文件监控：
//! - [`FileWatcher`]：后台线程运行 notify，通过 channel 报告变更
//! - [`HotReloadManager`]：管理监控路径 + 去抖 + 主线程轮询
//!
//! 热重载流程：
//! 1. Editor 主循环调用 `poll_changes()` 获取变更文件列表
//! 2. 对每个变更文件调用 `AssetCache::reload_changed()`
//! 3. 通过 EventBus 发布 `AssetModified` 事件通知下游系统

use notify::{Event, EventKind, RecursiveMode, Watcher};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::{Duration, Instant};

// ---------------------------------------------------------------------------
// FileWatcher
// ---------------------------------------------------------------------------

/// 文件系统监控器。
///
/// 在后台线程运行 `notify::RecommendedWatcher`，通过 `mpsc::Receiver` 报告文件变更。
pub struct FileWatcher {
    /// notify watcher 的生命周期持有
    _watcher: notify::RecommendedWatcher,
    /// 接收文件变更事件
    rx: mpsc::Receiver<notify::Result<Event>>,
}

impl FileWatcher {
    /// 创建新的文件监控器。
    ///
    /// 内部启动一个后台线程运行 notify 事件循环。
    pub fn new() -> Result<Self, notify::Error> {
        let (tx, rx) = mpsc::channel();
        let watcher = notify::RecommendedWatcher::new(
            move |res| {
                let _ = tx.send(res);
            },
            notify::Config::default(),
        )?;
        Ok(Self {
            _watcher: watcher,
            rx,
        })
    }

    /// 监控指定路径（递归）。
    pub fn watch(&mut self, path: &Path) -> Result<(), notify::Error> {
        self._watcher.watch(path, RecursiveMode::Recursive)
    }

    /// 取消监控指定路径。
    pub fn unwatch(&mut self, path: &Path) -> Result<(), notify::Error> {
        self._watcher.unwatch(path)
    }

    /// 非阻塞轮询所有待处理的文件系统事件。
    ///
    /// 返回变更文件路径列表（已去重）。
    pub fn poll(&self) -> Vec<PathBuf> {
        let mut paths = HashSet::new();
        while let Ok(Ok(event)) = self.rx.try_recv() {
            for path in &event.paths {
                paths.insert(path.clone());
            }
            // 也处理被重命名的来源路径
            if matches!(
                event.kind,
                EventKind::Modify(notify::event::ModifyKind::Name(
                    notify::event::RenameMode::From
                ))
            ) {
                // rename-from 路径已被包含在 event.paths 中，无需额外处理
            }
        }
        paths.into_iter().collect()
    }
}

// ---------------------------------------------------------------------------
// HotReloadManager
// ---------------------------------------------------------------------------

/// 热重载管理器：监控路径 + 去抖 + 扩展名过滤。
pub struct HotReloadManager {
    /// 文件监控器（None 表示未启用热重载）
    watcher: Option<FileWatcher>,
    /// 受监控的文件扩展名集合（小写，不含点）
    watched_extensions: HashSet<String>,
    /// 去抖缓冲区：路径 → 上次事件时间
    debounce: HashMap<PathBuf, Instant>,
    /// 去抖窗口（默认 200ms）
    debounce_window: Duration,
}

impl HotReloadManager {
    /// 创建禁用状态的热重载管理器。
    pub fn new() -> Self {
        Self {
            watcher: None,
            watched_extensions: HashSet::new(),
            debounce: HashMap::new(),
            debounce_window: Duration::from_millis(200),
        }
    }

    /// 启用热重载：创建 FileWatcher 并开始监控指定路径。
    ///
    /// `watch_paths`：要递归监控的目录列表。
    /// `extensions`：要监控的文件扩展名（如 `["glb", "gltf", "png", "jpg", "wav"]`）。
    pub fn enable(
        &mut self,
        watch_paths: &[PathBuf],
        extensions: &[&str],
    ) -> Result<(), notify::Error> {
        let mut watcher = FileWatcher::new()?;
        for path in watch_paths {
            if path.exists() {
                watcher.watch(path)?;
            }
        }
        self.watched_extensions = extensions.iter().map(|e| e.to_lowercase()).collect();
        self.watcher = Some(watcher);
        Ok(())
    }

    /// 检查热重载是否已启用。
    pub fn is_enabled(&self) -> bool {
        self.watcher.is_some()
    }

    /// 设置去抖窗口（默认 200ms）。
    pub fn set_debounce_window(&mut self, duration: Duration) {
        self.debounce_window = duration;
    }

    /// 轮询文件变更（主线程每帧调用）。
    ///
    /// 返回经过去抖和扩展名过滤的变更文件路径列表。
    pub fn poll_changes(&mut self) -> Vec<PathBuf> {
        let watcher = match &self.watcher {
            Some(w) => w,
            None => return Vec::new(),
        };

        let raw_paths = watcher.poll();
        let now = Instant::now();
        let mut result = Vec::new();

        for path in raw_paths {
            // 扩展名过滤
            if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                let ext_lower = ext.to_lowercase();
                if !self.watched_extensions.is_empty()
                    && !self.watched_extensions.contains(&ext_lower)
                {
                    continue;
                }
            } else if !self.watched_extensions.is_empty() {
                // 无扩展名文件，跳过
                continue;
            }

            // 去抖：同一路径在 debounce_window 内只报告一次
            if let Some(last_time) = self.debounce.get(&path) {
                if now.duration_since(*last_time) < self.debounce_window {
                    continue;
                }
            }

            self.debounce.insert(path.clone(), now);
            result.push(path);
        }

        // 清理过期的去抖条目（超过 5x debounce_window 的条目）
        let threshold = self.debounce_window * 5;
        self.debounce.retain(|_, t| now.duration_since(*t) < threshold);

        result
    }
}

impl Default for HotReloadManager {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// 测试
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disabled_manager_returns_empty() {
        let mut manager = HotReloadManager::new();
        assert!(!manager.is_enabled());
        assert!(manager.poll_changes().is_empty());
    }

    #[test]
    fn set_debounce_window() {
        let mut manager = HotReloadManager::new();
        manager.set_debounce_window(Duration::from_millis(500));
        // Debounce window is set even when disabled
        manager.set_debounce_window(Duration::from_millis(100));
    }
}
