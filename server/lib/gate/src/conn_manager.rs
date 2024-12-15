use std::sync::Arc;
use std::collections::BTreeMap;

use tokio::sync::Mutex;

use consul::ConsulImpl;
use close_handle::CloseHandle;
use redis_service::redis_service::RedisService;
use time::OffsetTime;

use proto::hub::{
    HubService,
    ClientDisconnnect
};
use tracing::{trace, info};

use crate::client_proxy_manager::ClientProxy;
use crate::hub_proxy_manager::HubProxy;
use crate::entity_manager::{Entity, EntityManager};
use crate::hub_msg_handle::GateHubMsgHandle;
use crate::client_msg_handle::GateClientMsgHandle;

pub struct ConnManager {
    offset_time: Arc<Mutex<OffsetTime>>,
    gate_name: String,
    gate_host: String,
    redis_service: Option<Arc<Mutex<RedisService>>>,
    locks: BTreeMap<String, String>,
    hubs: BTreeMap<String, Arc<Mutex<HubProxy>>>,
    hub_msg_handle: Arc<Mutex<GateHubMsgHandle>>,
    clients: BTreeMap<String, Arc<Mutex<ClientProxy>>>,
    client_msg_handle: Arc<Mutex<GateClientMsgHandle>>,
    entities: EntityManager,
    consul_impl: Arc<Mutex<ConsulImpl>>,
    close: Arc<Mutex<CloseHandle>>
}

impl ConnManager {
    pub fn new(
        _gate_name: String, 
        _gate_host: String,
        _hub_handle: Arc<Mutex<GateHubMsgHandle>>, 
        _client_handle: Arc<Mutex<GateClientMsgHandle>>, 
        _consul_impl: Arc<Mutex<ConsulImpl>>,
        _offset_time: Arc<Mutex<OffsetTime>>,
        _close: Arc<Mutex<CloseHandle>>) -> ConnManager 
    {
        ConnManager {
            gate_name: _gate_name,
            gate_host: _gate_host,
            redis_service: None,
            locks: BTreeMap::new(),
            hubs: BTreeMap::new(),
            hub_msg_handle: _hub_handle,
            clients: BTreeMap::new(),
            client_msg_handle: _client_handle,
            entities: EntityManager::new(),
            consul_impl: _consul_impl,
            offset_time: _offset_time,
            close: _close
        }
    }

    pub fn add_lock(&mut self, lock_key: String, value: String) {
        trace!("ConnManager add_lock lock_key:{} value:{}!", lock_key, value);
        self.locks.insert(lock_key, value);
    }

    pub fn remove_lock(&mut self, lock_key: String) -> Option<String> {
        trace!("ConnManager remove_lock lock_key:{}!", lock_key);
        return self.locks.remove(&lock_key).clone();
    }

    pub fn get_consul_impl(&self) -> Arc<Mutex<ConsulImpl>> {
        self.consul_impl.clone()
    }

    pub fn set_redis_service(&mut self, _service: Arc<Mutex<RedisService>>) {
        self.redis_service = Some(_service)
    }

    pub fn get_redis_service(&self) -> Arc<Mutex<RedisService>> {
        self.redis_service.clone().unwrap()
    }

    pub fn get_close_handle(&self) -> Arc<Mutex<CloseHandle>> {
        self.close.clone()
    }

    pub fn get_gate_name(&self) -> String {
        self.gate_name.clone()
    }

    pub fn get_gate_host(&self) -> String {
        self.gate_host.clone()
    }

    pub fn update_entity(&mut self, e: Entity) {
        self.entities.update_entity(e)
    }

    pub fn get_entity_mut(&mut self, entity_id: &String) -> Option<&mut Entity> {
        self.entities.get_entity_mut(entity_id)
    }

    pub fn get_entity(&self, entity_id: &String) -> Option<&Entity> {
        self.entities.get_entity(entity_id)
    }

    pub fn delete_entity(&mut self, entity_id: &String) -> Option<Entity> {
        self.entities.delete_entity(entity_id)
    }

    pub async fn add_client_proxy(&mut self, proxy: Arc<Mutex<ClientProxy>>) {
        let _clientproxy = proxy.clone();
        let _client = proxy.as_ref().lock().await;
        let conn_id = _client.get_conn_id().clone();
        self.clients.insert(conn_id.clone(), _clientproxy);
        trace!("add_client_proxy conn_id:{}", conn_id);
    }

    pub fn get_client_proxy(&self, conn_id: &String) -> Option<&Arc<Mutex<ClientProxy>>> {
        self.clients.get(conn_id)
    }

    pub fn delete_client_proxy(&mut self, conn_id: &String) {
        trace!("delete_client_proxy conn_id:{}", conn_id);
        let _ = self.clients.remove(conn_id);
    }

    pub fn get_client_msg_handle(&self) -> Arc<Mutex<GateClientMsgHandle>> {
        self.client_msg_handle.clone()
    }

    pub fn get_all_client_proxy(&mut self) -> Vec<Arc<Mutex<ClientProxy>>> {
        let _client_clone = self.clients.clone();
        _client_clone.into_values().collect()
    }

    pub async fn add_hub_proxy(&mut self, name: String, proxy: Arc<Mutex<HubProxy>>) {
        let _hubproxy = proxy.clone();
        let _hub = proxy.as_ref().lock().await;
        self.hubs.insert(name, _hubproxy);
    }

    pub fn get_hub_proxy(&self, name: &String) -> Option<&Arc<Mutex<HubProxy>>> {
        self.hubs.get(name)
    }

    pub fn delete_hub_proxy(&mut self, name: &String) {
        let _ = self.hubs.remove(name);
    }

    pub fn get_hub_msg_handle(&self) -> Arc<Mutex<GateHubMsgHandle>> {
        self.hub_msg_handle.clone()
    }

    pub async fn close_client(&mut self, conn_id: &String) {
        if let Some(client) = self.clients.remove(conn_id) {
            let _c = client.as_ref().lock().await;
            {
                let mut _wr = _c.wr.as_ref().lock().await;
                _wr.close().await;
            }
            if let Some(_j) = &_c.join {
                _j.abort();
            }
        }
    }

    pub async fn get_utc_unix_time_with_offset(&self) -> i64 {
        let _offset_time = self.offset_time.as_ref().lock().await;
        _offset_time.utc_unix_time_with_offset()
    }

    pub async fn poll(&mut self) {
        let _timetmp = self.get_utc_unix_time_with_offset().await;
        let _clients = self.get_all_client_proxy();
        for _client_arc in _clients.iter() {
            let _client_conn_id:String;
            {
                let _client = _client_arc.as_ref().lock().await;
                if (_timetmp - _client.last_heartbeats_timetmp) < 6000 {
                    continue;
                }
                _client_conn_id = _client.conn_id.clone();
            }
            
            info!("ConnManager poll delete_client_proxy!");
            self.delete_client_proxy(&_client_conn_id);
            if let Some(vec_hub) = self.entities.delete_client(&_client_conn_id) {
                let mut invaild_hubs: Vec<String> = Vec::new();
                for hub_name in vec_hub.iter() {
                    if let Some(_hub_arc) = self.get_hub_proxy(hub_name) {
                        let mut _hub = _hub_arc.as_ref().lock().await;
                        if !_hub.send_hub_msg(HubService::ClientDisconnnect(ClientDisconnnect::new(_client_conn_id.clone()))).await {
                            invaild_hubs.push(hub_name.to_string());
                        }
                    }
                }
                for invaild_hub_name in invaild_hubs {
                    self.delete_hub_proxy(&invaild_hub_name);
                }
            }
        }
    }
}
