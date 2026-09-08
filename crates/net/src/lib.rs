use std::sync::Arc;

use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use async_trait::async_trait;
use tracing::error;

#[async_trait]
pub trait NetWriter {
    async fn send(&mut self, buf: &[u8]) -> bool;

    async fn close(&mut self);
}

#[async_trait]
pub trait NetReaderCallback {
    async fn cb(&mut self, data:Vec<u8>);
}

/// 连接关闭回调：读循环因失败/对端关闭而退出时触发，
/// 由 socket 持有者回收 writer 并注销上层 proxy。
#[async_trait]
pub trait NetReaderCloseCallback {
    async fn on_close(&mut self);
}

pub trait NetReader {
    fn start(self, 
        f: Arc<Mutex<Box<dyn NetReaderCallback + Send + 'static>>>,
        close: Option<Arc<Mutex<Box<dyn NetReaderCloseCallback + Send + 'static>>>>) -> JoinHandle<()>;
}

/// 触发连接关闭回调。读循环在所有退出路径上调用，保证 socket 与上层资源被回收。
pub async fn notify_close(close: &Option<Arc<Mutex<Box<dyn NetReaderCloseCallback + Send + 'static>>>>) {
    if let Some(cb) = close {
        let mut c = cb.lock().await;
        c.on_close().await;
    }
}

pub struct NetPack {
    buf: Vec<u8>
}

#[derive(Debug)]
pub enum NetPackError {
    /// 帧头声明的长度超过允许上限，连接应被丢弃。
    MessageTooLarge { size: usize },
}

impl NetPack {
    pub fn new() -> NetPack {
        NetPack {
            buf: Vec::new()
        }
    }

    pub fn input(&mut self, data: &[u8]) {
        self.buf.extend_from_slice(data)
    }

    pub fn try_get_pack(&mut self) -> Result<Option<Vec<u8>>, NetPackError> {
        if self.buf.is_empty() {
            return Ok(None)
        }

        let total = self.buf.len();
        if total < 4 {
            return Ok(None)
        }

        let len0 = self.buf[0] as usize;
        let len1 = self.buf[1] as usize;
        let len2 = self.buf[2] as usize;
        let len3 = self.buf[3] as usize;
        let new_pack_len: usize = len0 | len1 << 8 | len2 << 16 | len3 << 24;

        const MAX_MESSAGE_SIZE: usize = 16 * 1024 * 1024; // 16MB
        if new_pack_len > MAX_MESSAGE_SIZE {
            error!("Message size {} exceeds maximum allowed size {} bytes, closing connection", new_pack_len, MAX_MESSAGE_SIZE);
            return Err(NetPackError::MessageTooLarge { size: new_pack_len });
        }

        let packet_end = new_pack_len + 4;
        if packet_end > total {
            return Ok(None)
        }
        
        let mut buf = vec![0u8; new_pack_len];
        buf.copy_from_slice(&self.buf[4..packet_end]);

        if total > packet_end {
            self.buf.drain(0..packet_end);
        }
        else {
            self.buf.clear();
        }

        Ok(Some(buf))
    }
}
