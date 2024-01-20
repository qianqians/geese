use std::sync::Arc;
 use std::marker::Send;

use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use async_trait::async_trait;
use tracing::trace;

use close_handle::CloseHandle;

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
        f: Arc<Mutex<Box<dyn NetReaderCallback + Send + 'static>>>, 
        c: Arc<Mutex<CloseHandle>>) -> JoinHandle<()>;
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

        let len0 = self.buf[0] as usize;
        let len1 = self.buf[1] as usize;
        let len2 = self.buf[2] as usize;
        let len3 = self.buf[3] as usize;
        let new_pack_len: usize = len0 | len1 << 8 | len2 << 16 | len3 << 24;
        trace!("try_get_pack new_pack_len:{} self.buf.len:{}!", new_pack_len, self.buf.len());
        if new_pack_len > self.buf.len() || self.buf.is_empty() || new_pack_len <= 0 {
            None
        }
        else {
            let len = new_pack_len as usize;
            let idx = self.buf.len();
            let mut buf = vec![0u8; len];
            let idx_len = len + 4;
            buf.copy_from_slice(&self.buf[4..idx_len]);

            let remain = idx - len - 4;
            if remain > 0 {
                let mut tmp = vec![0u8; remain];
                tmp.copy_from_slice(&self.buf[idx_len..idx]);
                self.buf.clear();
                self.buf.extend_from_slice(&tmp[..]);
            }
            else {
                self.buf.clear();
            }

            Some(buf)
        }
    }
}