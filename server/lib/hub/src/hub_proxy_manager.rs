use std::sync::Arc;

use rand::Rng;
use tokio::sync::Mutex;
use tracing::{error, warn};

use thrift::protocol::{TCompactOutputProtocol, TSerializable};
use thrift::transport::{TIoChannel, TBufferChannel};

use net::NetWriter;
use redis_service::redis_service::{create_lock_key, create_host_cache_key, RedisService};
use close_handle::CloseHandle;
use consul::ConsulImpl;

use proto::common::RegServer;
use proto::hub::HubService;

use crate::hub_service_manager::{ConnCallbackMsgHandle, StdMutex};
use crate::conn_manager::ConnManager;

/// Thrift 序列化缓冲区容量（1MB）
const THRIFT_BUFFER_CAPACITY: usize = 1_048_576;

pub async fn entry_direct_hub_server(
    _hub_name: String,
    _conn_msg_handle: Arc<StdMutex<ConnCallbackMsgHandle>>, 
    _conn_mgr: Arc<Mutex<ConnManager>>,
    _redis_mq_service: Arc<Mutex<RedisService>>,
    _close: Arc<Mutex<CloseHandle>>)
{
    let mut _hub_host: String = "".to_string();
    {
        let mut _r = _redis_mq_service.as_ref().lock().await;
        _hub_host = _r.get(create_host_cache_key(_hub_name.clone()), None).await.unwrap_or_default();
    }

    let mut _conn_mgr_handle = _conn_mgr.as_ref().lock().await;
    let mut _service = _redis_mq_service.as_ref().lock().await;
    let lock_key = create_lock_key(_conn_mgr_handle.get_hub_name(), _hub_name.clone());
    let value = match _service.acquire_lock(lock_key.clone(), 3, None).await {
        Ok(v) => v,
        Err(e) => {
            error!("Failed to acquire lock for hub '{}': {}", _hub_name, e);
            return;
        }
    };
    if let Some(_hubproxy) = _conn_mgr_handle.get_hub_proxy(&_hub_name) {
        if !value.is_empty() {
            if let Err(e) = _service.release_lock(lock_key.clone(), value.clone(), None).await {
                warn!("Failed to release lock for hub '{}': {}", _hub_name, e);
            }
        }
        return;
    }

    _conn_mgr_handle.add_lock(lock_key.clone(), value);

    if let Some(_wr_arc) = _conn_mgr_handle.direct_connect_server(
        _hub_name.clone(), 
        _hub_host, 
        _conn_msg_handle.clone(), 
        _close.clone()).await
    {
        let _hubproxy = Arc::new(Mutex::new(HubProxy::new(_wr_arc)));
        let _hub_clone = _hubproxy.clone();
        let mut _hub_send = _hubproxy.as_ref().lock().await;
        _hub_send.send_hub_msg(HubService::RegServer(RegServer::new(_conn_mgr_handle.get_hub_name(), "hub".to_string()))).await;
        _hub_send.hub_name = Some(_hub_name.clone());
        _conn_mgr_handle.add_hub_proxy(_hub_name.clone(), _hub_clone).await;
    }
}

pub async fn entry_hub_service(
    _service: String,
    _conn_msg_handle: Arc<StdMutex<ConnCallbackMsgHandle>>, 
    _conn_mgr: Arc<Mutex<ConnManager>>,
    _redis_mq_service: Arc<Mutex<RedisService>>,
    _consul_impl: Arc<Mutex<ConsulImpl>>,
    _close: Arc<Mutex<CloseHandle>>) -> String
{
    let mut _impl = _consul_impl.as_ref().lock().await;
    let mut services = match _impl.services(_service).await {
        None => return String::new(),
        Some(s) => s
    };
    loop {
        let index:usize;
        {
            let mut rng = rand::thread_rng();
            index = rng.gen_range(0..services.len());
        }
        let service = match services.get(index) {
            None => return String::new(),
            Some(s) => s
        };
        let mut _conn_mgr_handle = _conn_mgr.as_ref().lock().await;
        if let Some(_hubproxy) = _conn_mgr_handle.get_hub_proxy(&service.id) {
            return service.id.clone();
        }
        else {
            let mut _service = _redis_mq_service.as_ref().lock().await;
            let lock_key = create_lock_key(_conn_mgr_handle.get_hub_name(), service.id.clone());
            let value = match _service.acquire_lock(lock_key.clone(), 3, None).await {
                Ok(v) => v,
                Err(e) => {
                    error!("Failed to acquire lock for service '{}': {}", service.id, e);
                    continue;
                }
            };
            if let Some(_hubproxy) = _conn_mgr_handle.get_hub_proxy(&service.id) {
                if !value.is_empty() {
                    if let Err(e) = _service.release_lock(lock_key.clone(), value.clone(), None).await {
                        warn!("Failed to release lock for service '{}': {}", service.id, e);
                    }
                }
                return service.id.clone();
            }

            _conn_mgr_handle.add_lock(lock_key.clone(), value);

            if let Some(_wr_arc) = _conn_mgr_handle.direct_connect_server(
                service.id.clone(), 
                format!("{}:{}", service.addr, service.port), 
                _conn_msg_handle.clone(), 
                _close.clone()).await
            {
                let _hubproxy = Arc::new(Mutex::new(HubProxy::new(_wr_arc)));
                let _hub_clone = _hubproxy.clone();

                let mut _hub_send = _hubproxy.as_ref().lock().await;
                _hub_send.send_hub_msg(HubService::RegServer(RegServer::new(_conn_mgr_handle.get_hub_name(), "hub".to_string()))).await;
                _hub_send.hub_name = Some(service.id.clone());
                _conn_mgr_handle.add_hub_proxy(service.id.clone(), _hub_clone).await;
                return service.id.clone();
            }
        }
        services.remove(index);
        if services.len() <= 0 {
            error!("entry_hub_service faild!");
            return String::new();
        }
    }
}

pub struct HubProxy {
    pub hub_name: Option<String>,
    pub wr: Arc<Mutex<Box<dyn NetWriter + Send + 'static>>>
}

impl HubProxy {
    pub fn new(_wr: Arc<Mutex<Box<dyn NetWriter + Send + 'static>>>) -> HubProxy 
    {
        HubProxy {
            hub_name: None,
            wr: _wr
        }
    }

    pub async fn send_hub_msg(&mut self, msg: HubService) -> bool {
        let t = TBufferChannel::with_capacity(0, THRIFT_BUFFER_CAPACITY);
        let (rd, wr) = match t.split() {
            Ok(_t) => (_t.0, _t.1),
            Err(_e) => {
                error!("send_hub_msg t.split error {}", _e);
                return false;
            }
        };
        let mut o_prot = TCompactOutputProtocol::new(wr);
        if let Err(e) = HubService::write_to_out_protocol(&msg, &mut o_prot) {
            error!("Failed to serialize Thrift message in send_hub_msg: {}", e);
            return false;
        }
        let wr = self.wr.clone();
        let mut p_send = wr.as_ref().lock().await;
        p_send.send(&rd.write_bytes()).await
    }
}
