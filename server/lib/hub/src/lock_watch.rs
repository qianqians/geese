//! 带等待看门狗的异步互斥锁。
//!
//! 目的：把「偶发卡死」变成一条能定位的日志。hub 里历史上几次卡死，
//! 现场只有「进程还在、/health 变 503、重启就好」，没有任何线索说明是谁挡住了谁。
//! `WatchMutex` 在**获取超过阈值**时打一条 warn（然后继续等，不改变语义），
//! 于是卡住时日志最后一条就是被哪把锁挡住的。
//!
//! 用法：把字段类型从 `Arc<Mutex<T>>` 换成 `Arc<WatchMutex<T>>`，
//! 构造时 `WatchMutex::new("名字", value)`；调用点 `.lock().await` 完全不变
//! （返回的仍是 `tokio::sync::MutexGuard`）。

use std::time::{Duration, Instant};

use tokio::sync::{Mutex, MutexGuard};
use tracing::warn;

/// 等待超过这个时长就打日志。
///
/// 500ms 是「正常路径绝不该出现、但短暂抖动也够不到」的折中：
/// 一条消息的分发是微秒级，所以 500ms 的等待基本只可能来自环形等待或长时间 I/O。
pub const LOCK_WAIT_WARN: Duration = Duration::from_millis(500);

pub struct WatchMutex<T> {
    inner: Mutex<T>,
    name: &'static str,
}

impl<T> WatchMutex<T> {
    pub fn new(name: &'static str, value: T) -> WatchMutex<T> {
        WatchMutex {
            inner: Mutex::new(value),
            name,
        }
    }

    pub async fn lock(&self) -> MutexGuard<'_, T> {
        let start = Instant::now();

        // biased：先等锁本身，避免「锁已就绪但计时器也到点」时误报。
        let guard = tokio::select! {
            biased;
            g = self.inner.lock() => g,
            _ = tokio::time::sleep(LOCK_WAIT_WARN) => {
                warn!(
                    "lock '{}' 等待超过 {:?}，仍在等待 —— 若后续再无日志，基本可判定死锁",
                    self.name, LOCK_WAIT_WARN
                );
                self.inner.lock().await
            }
        };

        let waited = start.elapsed();
        if waited > LOCK_WAIT_WARN {
            warn!("lock '{}' 最终等待 {:?} 后才拿到", self.name, waited);
        }

        guard
    }
}
