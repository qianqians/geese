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

pub trait NetReader {
    fn start(self, 
        f: Arc<Mutex<Box<dyn NetReaderCallback + Send + 'static>>>) -> JoinHandle<()>;
}

pub struct NetPack {
    buf: Vec<u8>
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

    pub fn try_get_pack(&mut self) -> Option<Vec<u8>> {
        if self.buf.is_empty() {
            return None
        }

        let total = self.buf.len();
        if total < 4 {
            return None
        }

        let len0 = self.buf[0] as usize;
        let len1 = self.buf[1] as usize;
        let len2 = self.buf[2] as usize;
        let len3 = self.buf[3] as usize;
        let new_pack_len: usize = len0 | len1 << 8 | len2 << 16 | len3 << 24;

        const MAX_MESSAGE_SIZE: usize = 16 * 1024 * 1024; // 16MB
        if new_pack_len < 4 {
            error!("Invalid message size: {} bytes", new_pack_len);
            return None;
        }
        if new_pack_len > MAX_MESSAGE_SIZE {
            error!("Message size {} exceeds maximum allowed size {} bytes, dropping connection", new_pack_len, MAX_MESSAGE_SIZE);
            return None;
        }

        let packet_end = new_pack_len + 4;
        if packet_end > total {
            return None
        }
        
        let mut buf = vec![0u8; new_pack_len];
        buf.copy_from_slice(&self.buf[4..packet_end]);

        if total > packet_end {
            self.buf.drain(0..packet_end);
        }
        else {
            self.buf.clear();
        }

        Some(buf)
    }
}