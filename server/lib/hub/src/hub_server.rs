use std::sync::Arc;
use std::collections::HashSet;

use tokio::sync::Mutex;
use tracing::{trace, debug, info, warn, error};

use tcp::tcp_server::TcpServer;
use redis_service::redis_service::{RedisService, create_lock_key, create_host_cache_key, create_channel_key};
use consul::ConsulImpl;
use close_handle::CloseHandle;

use proto::common::RegServer;
use proto::dbproxy::DbEvent;
use proto::hub::HubService;
use proto::gate::GateHubService;

use crate::gate_proxy_manager::GateProxy;
use crate::hub_msg_handle::HubCallbackMsgHandle;
use crate::gate_msg_handle::GateCallbackMsgHandle;
use crate::conn_manager::ConnManager;
use crate::hub_service_manager::{ConnProxyManager, ConnCallbackMsgHandle, StdMutex};
use crate::dbproxy_msg_handle::DBCallbackMsgHandle;
use crate::dbproxy_manager::{DBProxyProxy, entry_dbproxy_service};
use crate::hub_proxy_manager::{entry_direct_hub_server, entry_hub_service};

pub struct HubServer {
    hub_name: String,
    redis_url: String,
    hub_host: String,
    hub_redis_service: Option<Arc<Mutex<RedisService>>>,
    hub_tcp_server: Option<TcpServer>,
    conn_mgr: Arc<Mutex<ConnManager>>,
    db_msg_handle: Arc<StdMutex<DBCallbackMsgHandle>>,
    conn_msg_handle: Arc<StdMutex<ConnCallbackMsgHandle>>,
    consul_impl: Arc<Mutex<ConsulImpl>>,
    close: Arc<Mutex<CloseHandle>>,
    connecting_hubs: Arc<Mutex<HashSet<String>>>,
}

impl HubServer {
    pub fn new(
        _hub_name: String,
        _redis_url: String,
        _hub_host: String,
        _consul_impl: Arc<Mutex<ConsulImpl>>) -> Result<HubServer, Box<dyn std::error::Error>> 
    {
        let _hub_name_server = _hub_name.clone();
        let _conn_mgr = Arc::new(Mutex::new(
            ConnManager::new(
                _hub_name)
            )
        );

        let _close = Arc::new(Mutex::new(CloseHandle::new()));
        let _db_msg_handle = DBCallbackMsgHandle::new();

        let _hub_msg_handle = HubCallbackMsgHandle::new();
        let _gate_msg_handle = GateCallbackMsgHandle::new();
        let _conn_msg_handle = ConnCallbackMsgHandle::new(
            _hub_name_server.clone(), 
            _hub_msg_handle.clone(), 
            _gate_msg_handle.clone(), 
            _conn_mgr.clone(), 
            _close.clone());
        
        Ok(HubServer {
            hub_name: _hub_name_server,
            redis_url: _redis_url,
            hub_host: _hub_host,
            hub_redis_service: None,
            hub_tcp_server: None,
            conn_mgr: _conn_mgr,
            db_msg_handle: _db_msg_handle,
            conn_msg_handle: _conn_msg_handle,
            consul_impl: _consul_impl,
            close: _close,
            connecting_hubs: Arc::new(Mutex::new(HashSet::new())),
        })
    }

    pub fn log(level: String, content: String) {
        match level.as_str() {
            "trace" => trace!("{}", content),
            "debug" => debug!("{}", content),
            "info"  => info!("{}", content),
            "warn"  => warn!("{}", content),
            "error" => error!("{}", content),
            _       => warn!("unknown log level '{}': {}", level, content),
        }
    }

    pub async fn listen_hub_service(&mut self) -> bool {
        trace!("listen_hub_service begin!");

        let _conn_msg_handle = self.conn_msg_handle.clone();

        let name = self.hub_name.clone();
        self.hub_redis_service = match RedisService::listen(
            self.redis_url.clone(), 
            create_channel_key(name.clone()), 
            self.close.clone(),
            ConnProxyManager::new_redis_mq_callback(_conn_msg_handle.clone())).await
        {
            Err(e) => {
                error!("listen_hub_service faild err:{}!", e);
                return false;
            },
            Ok(s) => Some(Arc::new(Mutex::new(s)))
        };
        {
            let mut _conn_msg_handle_ref = self.conn_msg_handle.as_ref().lock().unwrap();
            _conn_msg_handle_ref.redis_service = self.hub_redis_service.clone();
        }

        self.hub_tcp_server = match TcpServer::listen(
            self.hub_host.clone(), 
            self.close.clone(),
            ConnProxyManager::new_tcp_callback(_conn_msg_handle)).await 
        {
            Err(e) => {
                error!("listen_hub_service faild err:{}!", e);
                return false;
            },
            Ok(s) => Some(s)
        };

        {    
            let _rs = self.hub_redis_service.as_mut().unwrap();
            let mut _r = _rs.as_ref().lock().await;
            let _ = _r.set(create_host_cache_key(name.clone()), self.hub_host.clone(), 10, None).await;
        }
        trace!("listen_hub_service end!");

        return true;
    }

    pub async fn entry_dbproxy_service(&mut self) -> String {
        let redis_mq_service = self.hub_redis_service.clone();
        entry_dbproxy_service(
            self.db_msg_handle.clone(), 
            self.conn_mgr.clone(),
            redis_mq_service.unwrap(), 
            self.consul_impl.clone(),
            self.close.clone()).await
    }

    pub async fn check_connect_hub_server(&self, hub_name: String) -> bool {
        let mut _conn_mgr = self.conn_mgr.as_ref().lock().await;
        let _hub = _conn_mgr.get_hub_proxy(&hub_name);

        return _hub.is_some();
    }

    pub async fn entry_hub_service(&self, service_name: String) -> String {
        let redis_mq_service = self.hub_redis_service.clone();
        entry_hub_service(
            service_name, 
            self.conn_msg_handle.clone(),
            self.conn_mgr.clone(),
            redis_mq_service.unwrap(),
            self.consul_impl.clone(),
            self.close.clone()).await
    }

    pub async fn entry_direct_hub_server(&self, hub_name: String) {
        let redis_mq_service = self.hub_redis_service.clone();
        entry_direct_hub_server(
            hub_name,
            self.conn_msg_handle.clone(),
            self.conn_mgr.clone(),
            redis_mq_service.unwrap(),
            self.close.clone()
        ).await
    }

    pub async fn entry_gate_service(&mut self, _gate_name: String) {
        let mut gate_host = "".to_string();
        {
            if let Some(rs) = &self.hub_redis_service {
                let mut _r = rs.as_ref().lock().await;
                gate_host = _r.get(create_host_cache_key(_gate_name.clone()), None).await.unwrap_or_default();
            }
        }

        let redis_service = self.hub_redis_service.clone().unwrap();
        let mut _service = redis_service.as_ref().lock().await;
        let lock_key = create_lock_key( self.hub_name.clone(), _gate_name.clone());
        let value = match _service.acquire_lock(lock_key.clone(), 3, None).await {
            Ok(v) => v,
            Err(e) => {
                error!("Failed to acquire lock for gate '{}': {}", _gate_name, e);
                return;
            }
        };
        let mut _conn_mgr = self.conn_mgr.as_ref().lock().await;
        if _conn_mgr.get_gate_proxy(&_gate_name).is_none() {
            _conn_mgr.add_lock(lock_key, value);

            if let Some(wr) = _conn_mgr.direct_connect_server(
                _gate_name.clone(), 
                gate_host, 
                self.conn_msg_handle.clone(), 
                self.close.clone()).await 
            {
                let _wr_arc_clone = wr.clone();
                
                let _gate_name_tmp = _gate_name.clone();
                let mut _gate_tmp = GateProxy::new(_wr_arc_clone);
                _gate_tmp.send_gate_msg(GateHubService::RegServer(RegServer::new(_conn_mgr.get_hub_name(), "hub".to_string()))).await;
            
                _gate_tmp.gate_name = Some(_gate_name);

                let _gateproxy = Arc::new(Mutex::new(_gate_tmp));
                _conn_mgr.add_gate_proxy(_gate_name_tmp, _gateproxy).await;
            }  
        }
        else {
            if let Err(e) = _service.release_lock(lock_key.clone(), value, None).await {
                error!("Failed to release lock for gate '{}': {}", _gate_name, e);
            }
        }
    }

    pub fn get_db_msg_handle(&self) -> Arc<StdMutex<DBCallbackMsgHandle>> {
        self.db_msg_handle.clone()
    }

    pub fn get_conn_msg_handle(&self) -> Arc<StdMutex<ConnCallbackMsgHandle>> {
        self.conn_msg_handle.clone()
    }

    pub fn set_conn_rt_handle(&self, handle: tokio::runtime::Handle) {
        let mut conn_handle = self.conn_msg_handle.as_ref().lock().unwrap();
        conn_handle.set_rt_handle(handle);
    }

    pub async fn gate_host(&self, gate_name: String) -> String {
        let mut _conn_mgr = self.conn_mgr.as_ref().lock().await;
        if let Some(_gate_proxy) = _conn_mgr.get_gate_proxy(&gate_name) {
            let mut _gate = _gate_proxy.as_ref().lock().await;
            return _gate.gate_host.clone().unwrap();
        }
        return "".to_string();
    }

    pub async fn flush_hub_host_cache(&mut self) {
        let _rs = self.hub_redis_service.as_mut().unwrap();
        let mut _r = _rs.as_ref().lock().await;
        let _ = _r.set(create_host_cache_key(self.hub_name.clone()), self.hub_host.clone(), 10, None).await;
    }

    pub async fn send_db_msg(&mut self, db_name: String, msg: DbEvent) -> bool {
        let _db_arc: Arc<Mutex<DBProxyProxy>>;
        {
            let mut _conn_mgr = self.conn_mgr.as_ref().lock().await;
            trace!("send_db_msg conn_mgr lock!");
            match _conn_mgr.get_dbproxy_proxy(&db_name) {
                Some(db) => _db_arc = db.clone(),
                None => {
                    error!("DBProxy '{}' not found", db_name);
                    return false;
                }
            }
        }
            
        let send_result: bool;
        {
            let mut _db = _db_arc.as_ref().lock().await;
            trace!("send_db_msg _db lock!");
            send_result = _db.send_db_msg(msg).await;
        }
        return send_result;
    }

    pub async fn send_hub_msg(&mut self, hub_name: String, msg: HubService) -> bool {
        // 检查是否已有可用的 hub 连接
        {
            let mut _conn_mgr = self.conn_mgr.as_ref().lock().await;
            if let Some(_hub_arc) = _conn_mgr.get_hub_proxy(&hub_name) {
                let mut _hub = _hub_arc.as_ref().lock().await;
                return _hub.send_hub_msg(msg).await;
            }
        }

        // 检查是否正在尝试连接该 hub，避免重复连接
        {
            let mut connecting = self.connecting_hubs.as_ref().lock().await;
            if !connecting.insert(hub_name.clone()) {
                warn!("Already attempting to connect to hub '{}', skipping", hub_name);
                return false;
            }
        }

        // 尝试重新连接
        let connect_result = async {
            self.entry_direct_hub_server(hub_name.clone()).await;

            let mut _conn_mgr = self.conn_mgr.as_ref().lock().await;
            if let Some(_hub_arc) = _conn_mgr.get_hub_proxy(&hub_name) {
                let mut _hub = _hub_arc.as_ref().lock().await;
                _hub.send_hub_msg(msg).await
            } else {
                warn!("Failed to connect to hub '{}'", hub_name);
                false
            }
        }.await;

        // 无论成功失败，都从 connecting_hubs 中移除
        {
            let mut connecting = self.connecting_hubs.as_ref().lock().await;
            connecting.remove(&hub_name);
        }

        connect_result
    }

    pub async fn send_gate_msg(&mut self, gate_name: String, msg: GateHubService) -> bool {
        {
            let mut _conn_mgr = self.conn_mgr.as_ref().lock().await;
            if let Some(_gate_arc) = _conn_mgr.get_gate_proxy(&gate_name) {
                let mut _gate = _gate_arc.as_ref().lock().await;
                return _gate.send_gate_msg(msg).await;
            }
        }

        {
            self.entry_gate_service(gate_name.clone()).await;

            let mut _conn_mgr = self.conn_mgr.as_ref().lock().await;
            if let Some(_gate_arc) = _conn_mgr.get_gate_proxy(&gate_name) {
                let mut _gate = _gate_arc.as_ref().lock().await;
                return _gate.send_gate_msg(msg).await;
            }
        }

        return false;
    }
}