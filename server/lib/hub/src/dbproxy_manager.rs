use std::sync::Arc;

use rand::Rng;
use tokio::sync::Mutex;
use async_trait::async_trait;
use tracing::{error, trace};

use thrift::protocol::{TCompactOutputProtocol, TSerializable};
use thrift::transport::{TIoChannel, TBufferChannel};

use net::{NetReaderCallback, NetWriter};
use redis_service::redis_service::{RedisService, create_channel_key};
use close_handle::CloseHandle;
use consul::{ConsulImpl, ServiceInfo};

use proto::dbproxy::{
    DbEvent,
    RegHubEvent
};

use crate::hub_service_manager::StdMutex;
use crate::dbproxy_msg_handle::DBCallbackMsgHandle;
use crate::conn_manager::ConnManager;

pub async fn entry_dbproxy_service(
    _dbproxy_msg_handle: Arc<StdMutex<DBCallbackMsgHandle>>, 
    _conn_mgr: Arc<Mutex<ConnManager>>,
    _redis_mq_service: Arc<Mutex<RedisService>>,
    _consul_impl: Arc<Mutex<ConsulImpl>>,
    _close: Arc<Mutex<CloseHandle>>) -> String
{
    let mut services: Vec<ServiceInfo>;
    {
        let mut _impl = _consul_impl.as_ref().lock().await;
        services = match _impl.services("dbproxy".to_string()).await {
            None => return String::new(),
            Some(s) => s
        };
    }

    loop {
        let mut rng = rand::thread_rng();
        let index = rng.gen_range(0..services.len());
        let service = match services.get(index) {
            None => return String::new(),
            Some(s) => s
        };
        
        let mut _conn_mgr_handle = _conn_mgr.as_ref().lock().await;    
        if let Some(_dbproxy) = _conn_mgr_handle.get_dbproxy_proxy(&service.id) {
            trace!("entry_dbproxy_service dbproxy already exists name:{}", service.id.clone());
            return service.id.clone();
        }
        else {
            let mut _service = _redis_mq_service.as_ref().lock().await;
            if let Ok((rd, wr)) = 
                _service.connect(create_channel_key(service.id.clone())).await 
            {
                let _dbproxy = Arc::new(Mutex::new(
                    DBProxyProxy::new(
                        service.id.clone(), 
                        wr.clone(), 
                        _dbproxy_msg_handle)));

                let mut _rd_ref = rd.as_ref().lock().await;
                let _ = _rd_ref.start(Arc::new(Mutex::new(Box::new(DBProxyReaderCallback::new(_dbproxy.clone())))));

                _conn_mgr_handle.add_dbproxy_proxy(_dbproxy.clone()).await;
                let mut _db_send = _dbproxy.as_ref().lock().await;
                trace!("entry_dbproxy_service _db_send lock!");
                _db_send.send_db_msg(DbEvent::RegHub(RegHubEvent::new(_conn_mgr_handle.get_hub_name()))).await;
                trace!("entry_dbproxy_service send_db_msg done!");
                
                trace!("entry_dbproxy_service new dbproxy name:{}", service.id.clone());
                return service.id.clone();
            }
        }
        services.remove(index);
        if services.len() <= 0 {
            error!("entry_dbproxy_service faild!");
            return String::new();
        }
    }
}

pub struct DBProxyProxy {
    pub dbproxy_name: String,
    pub wr: Arc<Mutex<Box<dyn NetWriter + Send + 'static>>>,
    msg_handle: Arc<StdMutex<DBCallbackMsgHandle>>
}

impl DBProxyProxy {
    pub fn new(_name: String, _wr: Arc<Mutex<Box<dyn NetWriter + Send + 'static>>>, _handle: Arc<StdMutex<DBCallbackMsgHandle>>) -> DBProxyProxy {
        DBProxyProxy {
            dbproxy_name: _name,
            wr: _wr,
            msg_handle: _handle
        }
    }

    pub fn get_msg_handle(&mut self) -> Arc<StdMutex<DBCallbackMsgHandle>> {
        self.msg_handle.clone()
    }

    pub async fn send_db_msg(&mut self, msg: DbEvent) -> bool {
        trace!("DBProxyProxy send_db_msg begin!");
        let t = TBufferChannel::with_capacity(0, 16777216);
        let (rd, wr) = match t.split() {
            Ok(_t) => (_t.0, _t.1),
            Err(_e) => {
                error!("do_get_guid t.split error {}", _e);
                return false;
            }
        };
        let mut o_prot = TCompactOutputProtocol::new(wr);
        let _ = DbEvent::write_to_out_protocol(&msg, &mut o_prot);
        let mut p_send = self.wr.as_ref().lock().await;
        trace!("DBProxyProxy send_db_msg p_send lock!");
        p_send.send(&rd.write_bytes()).await
    }
}

pub struct DBProxyReaderCallback {
    dbproxy: Arc<Mutex<DBProxyProxy>>
}

#[async_trait]
impl NetReaderCallback for DBProxyReaderCallback {
    async fn cb(&mut self, data:Vec<u8>) {
        DBCallbackMsgHandle::on_event(self.dbproxy.clone(), data).await
    }
}

impl DBProxyReaderCallback {
    pub fn new(_dbproxy: Arc<Mutex<DBProxyProxy>>) -> DBProxyReaderCallback {
        DBProxyReaderCallback {
            dbproxy: _dbproxy
        }
    }
}

