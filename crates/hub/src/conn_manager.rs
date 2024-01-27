use std::sync::Arc;
use std::collections::BTreeMap;

use tokio::sync::Mutex;

use tcp::tcp_connect::TcpConnect;
use close_handle::CloseHandle;
use net::{NetReader, NetWriter};

use crate::hub_service_manager::{ConnProxyReaderCallback, ConnProxy, ConnCallbackMsgHandle};
use crate::dbproxy_manager::DBProxyProxy;
use crate::hub_proxy_manager::HubProxy;
use crate::gate_proxy_manager::GateProxy;

pub struct ConnManager {
    hub_name: String,
    locks: BTreeMap<String, String>,
    wrs: BTreeMap<String, Arc<Mutex<Box<dyn NetWriter + Send + 'static>>>>,
    dbproxys: BTreeMap<String, Arc<Mutex<DBProxyProxy>>>,
    hubproxys: BTreeMap<String, Arc<Mutex<HubProxy>>>,
    gateproxys: BTreeMap<String, Arc<Mutex<GateProxy>>>
}

impl ConnManager {
    pub fn new(_hub_name: String) -> ConnManager 
    {
        ConnManager {
            hub_name: _hub_name,
            locks: BTreeMap::new(),
            wrs: BTreeMap::new(),
            dbproxys: BTreeMap::new(),
            hubproxys: BTreeMap::new(),
            gateproxys: BTreeMap::new()
        }
    }

    pub fn add_lock(&mut self, lock_key: String, value: String) {
        self.locks.insert(lock_key, value);
    }

    pub fn remove_lock(&mut self, lock_key: String) -> String {
        return self.locks.remove(&lock_key).clone().unwrap();
    }

    pub async fn direct_connect_server(&mut self, name: String, host: String, _handle: Arc<Mutex<ConnCallbackMsgHandle>>, _close: Arc<Mutex<CloseHandle>>) 
        -> Option<Arc<Mutex<Box<dyn NetWriter + Send + 'static>>>>
    {
        if let Some(wr) = self.wrs.get(&name) {
            return Some(wr.clone());
        }

        if let Ok((rd, wr)) = TcpConnect::connect(host.clone()).await {
            let _wr_arc: Arc<Mutex<Box<dyn NetWriter + Send + 'static>>> = Arc::new(Mutex::new(Box::new(wr)));
                                    
            let _conn_proxy = Arc::new(Mutex::new(
                ConnProxy::new(_wr_arc.clone(), _handle.clone())));

            let _ = rd.start(Arc::new(Mutex::new(Box::new(
                ConnProxyReaderCallback::new(_conn_proxy)))), _close);

            self.wrs.insert(name.clone(), _wr_arc.clone());

            return Some(_wr_arc);
        }

        None
    }

    pub fn get_hub_name(&self) -> String {
        self.hub_name.clone()
    }

    pub async fn add_dbproxy_proxy(&mut self, _proxy: Arc<Mutex<DBProxyProxy>>) {
        let _db = _proxy.as_ref().lock().await;
        self.dbproxys.insert(_db.dbproxy_name.clone(), _proxy.clone());
    }

    pub fn get_dbproxy_proxy(&mut self, _dbproxy_name: &String) -> Option<&Arc<Mutex<DBProxyProxy>>> {
        self.dbproxys.get(_dbproxy_name)
    }

    pub fn add_hub_proxy(&mut self, _name: String, _proxy: Arc<Mutex<HubProxy>>) {
        self.hubproxys.insert(_name, _proxy.clone());
    }

    pub fn get_hub_proxy(&mut self, _hub_name: &String) -> Option<&Arc<Mutex<HubProxy>>> {
        self.hubproxys.get(_hub_name)
    }

    pub fn add_gate_proxy(&mut self, _name: String, _proxy: Arc<Mutex<GateProxy>>) {
        self.gateproxys.insert(_name, _proxy.clone());
    }

    pub fn get_gate_proxy(&mut self, _gate_name: &String) -> Option<&Arc<Mutex<GateProxy>>> {
        self.gateproxys.get(_gate_name)
    }
}