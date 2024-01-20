use std::sync::Arc;
use std::thread;
use std::time::Duration;

use tokio::sync::Mutex;
use tracing::info;

use net::{NetReaderCallback, NetWriter};
use redis_service::redis_service::{RedisMQListenCallback, RedisService, create_channel_key};
use redis_service::redis_mq_channel::RedisMQReader;
use close_handle::CloseHandle;
use mongo::MongoProxy;
use health::HealthHandle;
use time::utc_unix_time;
use async_trait::async_trait;

mod db;
mod handle;

use crate::handle::DBProxyHubMsgHandle;

pub struct DBProxyHubReaderCallback {
    msg_handle: Arc<Mutex<DBProxyHubMsgHandle>>, 
    wr: Arc<Mutex<Box<dyn NetWriter + Send + 'static>>>
}

#[async_trait]
impl NetReaderCallback for DBProxyHubReaderCallback {
    async fn cb(&mut self, data:Vec<u8>) {
        DBProxyHubMsgHandle::on_event(self.msg_handle.clone(), self.wr.clone(), data).await
    }
}

impl DBProxyHubReaderCallback {
    pub fn new(
        _handle: Arc<Mutex<DBProxyHubMsgHandle>>, 
        _wr: Arc<Mutex<Box<dyn NetWriter + Send + 'static>>>) -> DBProxyHubReaderCallback 
    {
        DBProxyHubReaderCallback {
            msg_handle: _handle,
            wr: _wr
        }
    }

}

pub struct HubProxyManager {
    msg_handle: Arc<Mutex<DBProxyHubMsgHandle>>,
    close_handle: Arc<Mutex<CloseHandle>>
}

#[async_trait]
impl RedisMQListenCallback for HubProxyManager {
    async fn redis_mq_cb(&mut self, rd: Arc<Mutex<RedisMQReader>>, wr: Arc<Mutex<Box<dyn NetWriter + Send + 'static>>>){
        let mut _rd_ref = rd.as_ref().lock().await;
        let _ = _rd_ref.start(Arc::new(Mutex::new(Box::new(DBProxyHubReaderCallback::new(self.msg_handle.clone(), wr)))), self.close_handle.clone());
    }
}

impl HubProxyManager {
    pub fn new(_handle: Arc<Mutex<DBProxyHubMsgHandle>>, _close: Arc<Mutex<CloseHandle>>) -> Arc<Mutex<Box<dyn RedisMQListenCallback + Send + 'static>>> {
        Arc::new(Mutex::new(Box::new(HubProxyManager {
            msg_handle: _handle,
            close_handle: _close
        })))
    }
}

pub struct DBProxyServer {
    handle: Arc<Mutex<DBProxyHubMsgHandle>>,
    health: Arc<Mutex<HealthHandle>>,
    close: Arc<Mutex<CloseHandle>>,
    server: RedisService
}

impl DBProxyServer {
    pub async fn new(name:String, redis_url:String, mongo_url:String, health_handle: Arc<Mutex<HealthHandle>>) -> Result<DBProxyServer, Box<dyn std::error::Error>> {
        let _mongo = MongoProxy::new(mongo_url).await?;
        let _handle = DBProxyHubMsgHandle::new(_mongo).await?;
        let _close = Arc::new(Mutex::new(CloseHandle::new()));
        let _s = RedisService::listen(
            redis_url, 
            create_channel_key(name), 
            HubProxyManager::new(_handle.clone(), 
            _close.clone()), _close.clone()).await?;
        Ok(DBProxyServer {
            handle: _handle,
            health: health_handle,
            close: _close,
            server: _s
        })
    }

    pub async fn close(&self) {
        info!("start close!");

        let mut _c_handle = self.close.as_ref().lock().await;
        _c_handle.close();
    }

    pub async fn join(self) {
        info!("await work done!");

        let _ = self.server.join().await;
        let _ = DBProxyHubMsgHandle::poll(self.handle).await;

        info!("work done!");
    }

    pub async fn run(&mut self) {
        loop {
            let begin = utc_unix_time();
            DBProxyHubMsgHandle::poll(self.handle.clone()).await;
            let tick = utc_unix_time() - begin;

            let _c_ref = self.close.as_ref().lock().await;
            if _c_ref.is_closed() {
                break;
            }

            if tick < 33 {
                thread::sleep(Duration::from_millis((33 - tick) as u64));
                let mut _health = self.health.as_ref().lock().await;
                _health.set_health_status(true);
            }
            else if tick > 256 {
                let mut _health = self.health.as_ref().lock().await;
                _health.set_health_status(false);
            }
        }
    }

    pub async fn done(&mut self) {
        DBProxyHubMsgHandle::poll(self.handle.clone()).await;
    }
}