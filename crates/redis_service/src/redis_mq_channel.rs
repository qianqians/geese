use std::sync::Arc;

use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use thrift::protocol::{TCompactOutputProtocol, TSerializable};
use redis::{Connection, Commands};
use async_trait::async_trait;
use tracing::{error, trace};

use net::{NetReaderCallback, NetWriter};

use proto::common::RedisMsg;


pub struct RedisMQReader {
    cb_handle: Option<Arc<Mutex<Box<dyn NetReaderCallback + Send + 'static>>>>, 
}

impl RedisMQReader {
    pub fn new() -> RedisMQReader {
        RedisMQReader { 
            cb_handle: None
        }
    }

    pub async fn cb(&mut self, data: Vec<u8>) {
        if let Some(arc_h) = self.cb_handle.clone() {
            let mut h = arc_h.as_ref().lock().await;
            h.cb(data).await
        }
    }

    pub fn start(&mut self, 
        f: Arc<Mutex<Box<dyn NetReaderCallback + Send + 'static>>>) -> JoinHandle<()>
    {
        self.cb_handle = Some(f);

        tokio::spawn(async move {})
    }
}

pub struct RedisMQWriter {
    wr: Arc<Mutex<Connection>>, 
    lname: String,
    rname: String,
    /// 序列化用的可复用缓冲（自动扩容，见 net::vec_transport）
    tx_buf: Arc<std::sync::Mutex<Vec<u8>>>
}

impl RedisMQWriter {
    pub fn new(_wr: Arc<Mutex<Connection>>, _lname: String, _rname: String) -> RedisMQWriter {
        RedisMQWriter {
            wr: _wr,
            lname: _lname,
            rname: _rname,
            tx_buf: net::vec_transport::new_buf()
        }
    }

    async fn _redis_mq_send(&mut self, buf: Vec<u8>) -> redis::RedisResult<i32> {
        trace!("redis mq send rname:{}, buf:{:?}", self.rname.clone(), buf);

        let mut _wr = self.wr.as_ref().lock().await;
        trace!("redis mq send rname:{} wr lock!", self.rname.clone());

        let count = _wr.lpush(self.rname.clone(), buf)?;

        trace!("redis mq send rname:{} done!", self.rname.clone());

        return Ok(count)
    }

}

#[async_trait]
impl NetWriter for RedisMQWriter {
    
    async fn send(&mut self, buf: &[u8]) -> bool {
        trace!("NetWriter redis mq send rname:{}, buf:{:?}", self.rname.clone(), buf);

        let msg = RedisMsg::new(self.lname.clone(), buf.to_vec());

        // 复用缓冲 + 自动扩容：这是**每条跨进程消息**都要走的路（位置通知 20Hz），
        // 原来每条都预分配 1MB。这里序列化出来的 Vec 要交给 lpush（需要 owned），
        // 所以每次按实际大小分配一次（~100B），不再有 1MB 的浪费。
        let tmp = match net::vec_transport::serialize_to_bytes(&self.tx_buf, |transport| {
            let mut o_prot = TCompactOutputProtocol::new(transport);
            match RedisMsg::write_to_out_protocol(&msg, &mut o_prot) {
                Ok(_) => true,
                Err(e) => {
                    error!("Failed to serialize Thrift message in RedisMQWriter send: {}", e);
                    false
                }
            }
        }) {
            Some(v) => v,
            None => return false,
        };

        match self._redis_mq_send(tmp).await {
            Ok(_) => {
                return true;
            },
            Err(_e) => {
                error!("RedisMQWriter send _wr.lpush error {}", _e);
                return false;
            }
        };
    }

    async fn close(&mut self) {
        let mut _wr = self.wr.as_ref().lock().await;
    }
}