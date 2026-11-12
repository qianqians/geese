use std::sync::Arc;
use std::collections::BTreeMap;

use tokio::sync::Mutex;
use tracing::{error, trace, warn};

use tcp::tcp_connect::TcpConnect;
use net::{NetReaderCloseCallback, NetReader, NetWriter};

use crate::hub_service_manager::{ConnProxyReaderCallback, ConnProxyCloseCallback, ConnProxy, ConnCallbackMsgHandle};
use crate::dbproxy_manager::DBProxyProxy;
use crate::hub_proxy_manager::HubProxy;
use crate::gate_proxy_manager::GateProxy;
use crate::lock_watch::WatchMutex;
use crate::hub_service_manager::StdMutex;

pub struct ConnManager {
    hub_name: String,
    locks: BTreeMap<String, String>,
    wrs: BTreeMap<String, Arc<Mutex<Box<dyn NetWriter + Send + 'static>>>>,
    dbproxys: BTreeMap<String, Arc<Mutex<DBProxyProxy>>>,
    connproxys: BTreeMap<String, Arc<WatchMutex<ConnProxy>>>,
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
            connproxys: BTreeMap::new(),
            hubproxys: BTreeMap::new(),
            gateproxys: BTreeMap::new()
        }
    }

    pub fn add_lock(&mut self, lock_key: String, value: String) {
        self.locks.insert(lock_key, value);
    }

    pub fn remove_lock(&mut self, lock_key: String) -> String {
        match self.locks.remove(&lock_key) {
            Some(value) => value,
            None => {
                warn!("Lock key '{}' not found in ConnManager", lock_key);
                String::new()
            }
        }
    }

    pub fn get_hub_name(&self) -> String {
        self.hub_name.clone()
    }

    /// 纯查表：已有 writer 就直接返回（不做 I/O）。
    pub fn get_writer(&self, name: &str) -> Option<Arc<Mutex<Box<dyn NetWriter + Send + 'static>>>> {
        self.wrs.get(name).cloned()
    }

    /// 纯登记：把已经建好的连接写进表里。
    pub fn register_conn(
        &mut self,
        name: String,
        wr: Arc<Mutex<Box<dyn NetWriter + Send + 'static>>>,
        conn_proxy: Arc<WatchMutex<ConnProxy>>,
    ) {
        self.wrs.insert(name.clone(), wr);
        self.connproxys.insert(name, conn_proxy);
    }

    pub async fn add_dbproxy_proxy(&mut self, _proxy: Arc<Mutex<DBProxyProxy>>) {
        let _db = _proxy.as_ref().lock().await;
        self.dbproxys.insert(_db.dbproxy_name.clone(), _proxy.clone());
    }

    pub fn get_dbproxy_proxy(&mut self, _dbproxy_name: &String) -> Option<&Arc<Mutex<DBProxyProxy>>> {
        self.dbproxys.get(_dbproxy_name)
    }

    /// 注册 hub proxy。
    ///
    /// ⚠ 这里**不再**顺手去锁 `connproxys` 里那条连接、回填它的 `hubproxy` 字段：
    ///   那会在「持 `ConnManager`」时再去锁 `ConnProxy`，与 `poll` / `on_close`
    ///   的 `CP → CM` 方向相反，构成 ABBA（见 hub-deadlock-audit.md D2）；
    ///   而且如果 `connproxys` 里恰好就是同一条连接，就是同任务自我死锁
    ///   （tokio Mutex 不可重入，会永久 await）。
    ///   出站连接的对端名字在建立时已经由 `ConnProxyCloseCallback::name` 记录，
    ///   这份回填本来就是冗余的。
    pub fn add_hub_proxy(&mut self, _name: String, _proxy: Arc<Mutex<HubProxy>>) {
        self.hubproxys.insert(_name, _proxy);
    }

    pub fn get_hub_proxy(&mut self, _hub_name: &str) -> Option<&Arc<Mutex<HubProxy>>> {
        self.hubproxys.get(_hub_name)
    }

    /// 注册 gate proxy。锁序约束同 [`Self::add_hub_proxy`]。
    pub fn add_gate_proxy(&mut self, _name: String, _proxy: Arc<Mutex<GateProxy>>) {
        self.gateproxys.insert(_name, _proxy);
    }

    pub fn get_gate_proxy(&mut self, _gate_name: &str) -> Option<&Arc<Mutex<GateProxy>>> {
        self.gateproxys.get(_gate_name)
    }

    /// 连接断开后回收：按对端名称移除 writer、连接 proxy 及其关联的 hub/gate proxy。
    pub fn remove_conn_proxy(&mut self, name: &String) {
        self.wrs.remove(name);
        self.connproxys.remove(name);
        self.hubproxys.remove(name);
        self.gateproxys.remove(name);
    }
}

/// 建连 + 登记，**全程不持 `ConnManager` 做 I/O**。
///
/// 旧实现是 `ConnManager::direct_connect_server(&mut self, …)`：在持有 `ConnManager`
/// 的情况下 `await TcpConnect::connect`（超时 5s）。对端 host 不通时，所有要拿
/// `ConnManager` 的 `send_*` 都会被按 5s 一次地卡住（hub-deadlock-audit.md 第 2 项）。
///
/// 现在拆成三步：查表（短锁）→ 建连（无锁）→ 登记（短锁，带二次检查）。
pub async fn connect_server(
    conn_mgr: &Arc<WatchMutex<ConnManager>>,
    name: String,
    host: String,
    handle: Arc<StdMutex<ConnCallbackMsgHandle>>,
) -> Option<Arc<Mutex<Box<dyn NetWriter + Send + 'static>>>> {
    trace!("connect_server name:{} host:{}", name, host);

    // 1) 查表（极短持锁，不做 I/O）
    if let Some(wr) = conn_mgr.as_ref().lock().await.get_writer(&name) {
        return Some(wr);
    }

    // 2) 建连（无锁）
    let (rd, wr) = match TcpConnect::connect(host.clone()).await {
        Ok(v) => v,
        Err(e) => {
            error!("connect_server '{}' ({}) failed: {}", name, host, e);
            return None;
        }
    };

    let _wr_arc: Arc<Mutex<Box<dyn NetWriter + Send + 'static>>> =
        Arc::new(Mutex::new(Box::new(wr)));
    let _conn_proxy = Arc::new(WatchMutex::new(
        "ConnProxy",
        ConnProxy::new(_wr_arc.clone(), handle),
    ));

    let _close_cb: Arc<Mutex<Box<dyn NetReaderCloseCallback + Send + 'static>>> =
        Arc::new(Mutex::new(Box::new(ConnProxyCloseCallback::new(
            _conn_proxy.clone(),
            conn_mgr.clone(),
            Some(name.clone()),
        ))));
    let _ = rd.start(
        Arc::new(Mutex::new(Box::new(ConnProxyReaderCallback::new(
            _conn_proxy.clone(),
        )))),
        Some(_close_cb),
    );

    // 3) 登记（可能被别人抢先：保留先到的，自己这条关掉）
    {
        let mut cm = conn_mgr.as_ref().lock().await;
        if let Some(existing) = cm.get_writer(&name) {
            drop(cm);
            trace!(
                "connect_server '{}' lost the race, closing the extra connection",
                name
            );
            let _ = _wr_arc.as_ref().lock().await.close().await;
            return Some(existing);
        }
        cm.register_conn(name.clone(), _wr_arc.clone(), _conn_proxy);
    }

    trace!("connect_server '{}' connected", name);
    Some(_wr_arc)
}