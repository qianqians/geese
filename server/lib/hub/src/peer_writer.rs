//! 每条连接一个写任务（per-peer writer）。
//!
//! 解决的三个问题：
//! 1. **持锁跨网络写**：原来 `send_*` 在 `HubServer`/`ConnManager`/proxy 三层锁里走到
//!    `wr.lock() → write_all`，对端不读（发送窗口满）时整个 hub 的发送路径被堵死。
//!    现在发送只是「序列化 + 入队」，写 socket 由本任务完成，锁只在任务内部短暂持有。
//! 2. **每条消息一次系统调用**：写任务会把队列里已就绪的整包拼起来，一次 `write_all`
//!    发多条（帧头天然支持拼接），显著减少 syscall。
//! 3. **队列满时的行为可控**：有界队列 + 明确丢弃策略，而不是让调用方无限阻塞。
//!
//! 注意：`Vec<u8>` 里必须**已经包含 4 字节长度帧头**（见 `net::vec_transport::serialize_framed`），
//! 写任务走 `NetWriter::send_framed` 原样写出。

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use tokio::sync::mpsc;
use tokio::sync::Mutex;
use tracing::{debug, error};

use net::NetWriter;

/// 每条连接最多积压多少条待发消息。
///
/// 8192 条足够吸收一次 tick 内的广播突发；打满说明对端已经严重滞后，
/// 此时继续无限排队只会把内存吃光，所以明确丢弃并记账。
pub const PEER_QUEUE_CAPACITY: usize = 8192;

/// 一次 `write_all` 最多拼多少字节（超过就先发出去，避免长延迟）。
const MAX_COALESCE_BYTES: usize = 64 * 1024;

/// 写入端句柄：`try_send` 永不阻塞、永不 await。
#[derive(Clone)]
pub struct PeerSender {
    tx: mpsc::Sender<Vec<u8>>,
    /// 队列满导致的丢弃计数（用于观测；每 1024 条打一次日志）
    dropped: Arc<AtomicU64>,
}

impl PeerSender {
    /// 非阻塞入队。返回 false 表示这条消息没发出去（队列满 / 写任务已退出）。
    pub fn try_send(&self, bytes: Vec<u8>) -> bool {
        match self.tx.try_send(bytes) {
            Ok(()) => true,
            Err(mpsc::error::TrySendError::Full(_)) => {
                let n = self.dropped.fetch_add(1, Ordering::Relaxed) + 1;
                if n == 1 || n % 1024 == 0 {
                    error!(
                        "peer write queue full ({} pending), dropping message (total dropped: {})",
                        PEER_QUEUE_CAPACITY, n
                    );
                }
                false
            }
            Err(mpsc::error::TrySendError::Closed(_)) => false,
        }
    }
}

/// 为一条连接启动写任务，返回入队句柄。
///
/// 必须在 tokio runtime 上下文里调用（本 crate 里所有代理对象都在 `block_on` 内创建）。
pub fn spawn(wr: Arc<Mutex<Box<dyn NetWriter + Send + 'static>>>) -> PeerSender {
    let (tx, mut rx) = mpsc::channel::<Vec<u8>>(PEER_QUEUE_CAPACITY);
    let dropped = Arc::new(AtomicU64::new(0));

    tokio::spawn(async move {
        let mut scratch: Vec<u8> = Vec::with_capacity(MAX_COALESCE_BYTES);
        while let Some(first) = rx.recv().await {
            scratch.clear();
            scratch.extend_from_slice(&first);

            // 把已经就绪的整包拼进来：N 条消息 → 1 次 write_all
            let mut coalesced = 1usize;
            while scratch.len() < MAX_COALESCE_BYTES {
                match rx.try_recv() {
                    Ok(next) => {
                        scratch.extend_from_slice(&next);
                        coalesced += 1;
                    }
                    Err(_) => break,
                }
            }

            let mut w = wr.as_ref().lock().await;
            let ok = w.send_framed(&scratch).await;
            drop(w);

            if !ok {
                // 写失败 = 连接已经不可用：主动 close，让读循环收到 EOF 并走
                // ConnProxyCloseCallback 回收连接（与旧的"返回 false 但连接还在"不同）。
                error!("peer writer: write failed, closing connection");
                let mut w = wr.as_ref().lock().await;
                w.close().await;
                break;
            }

            debug!("peer writer: flushed {} messages / {} bytes", coalesced, scratch.len());
        }
    });

    PeerSender { tx, dropped }
}
