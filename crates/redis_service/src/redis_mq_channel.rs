use std::sync::Arc;
use std::marker::Send;

use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use thrift::protocol::{TCompactOutputProtocol, TSerializable};
use thrift::transport::{TIoChannel, TBufferChannel};
use redis::{Connection, Commands};
use async_trait::async_trait;
use tracing::{error, trace};

use net::{NetReaderCallback, NetWriter};
use close_handle::CloseHandle;

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
        f: Arc<Mutex<Box<dyn NetReaderCallback + Send + 'static>>>, 
        _: Arc<Mutex<CloseHandle>>) -> JoinHandle<()>
    {
        self.cb_handle = Some(f);

        tokio::spawn(async move {})
    }
}

pub struct RedisMQWriter {
    wr: Arc<Mutex<Connection>>, 
    lname: String,
    rname: String
}

impl RedisMQWriter {
    pub fn new(_wr: Arc<Mutex<Connection>>, _lname: String, _rname: String) -> RedisMQWriter {
        RedisMQWriter {
            wr: _wr,
            lname: _lname,
            rname: _rname
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

        let t = TBufferChannel::with_capacity(0, 16384);
        let (rd, wr) = match t.split() {
            Ok(_t) => (_t.0, _t.1),
            Err(_e) => {
                error!("RedisMQWriter send t.split error {}", _e);
                return false;
            }
        };
        let mut o_prot = TCompactOutputProtocol::new(wr);
        let _ = RedisMsg::write_to_out_protocol(&msg, &mut o_prot);
        let tmp = rd.write_bytes();

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