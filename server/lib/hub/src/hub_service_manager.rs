//! Hub 的连接消息泵：reader 任务入队 → Python 主线程 `poll` 分发。
//!
//! # 锁序不变量（改动本模块前务必先读）
//!
//! 下面的顺序是**单向**的：只允许从左往右获取，任何反向都会形成环。
//!
//! ```text
//! S(HubServer) → CU(Consul) → CP(ConnProxy) → CM(ConnManager) → R(RedisService)
//!              → {HP, GP, DP} → W(wr)
//! ```
//!
//! - `Q`（本文件的 `ConnCallbackMsgHandle`，持有无界队列）是**叶子锁**：
//!   只允许在「取空队列」这种不跨 `await`、不回调 Python 的短临界区里持有。
//!   历史 bug（D1）：`poll` 曾把它一路带到 `rt.block_on` 和 Python 回调里，
//!   而 reader 任务又「持 `CP` 抢 `Q`」，于是与 `CP` 形成 ABBA。
//! - `CP` 与 `CM` 之间只允许 `CP → CM`（`poll` 分发时的取值、`on_close` 的回收）。
//!   `ConnManager` 的方法**不得**去锁 `ConnProxy`（`add_*_proxy` 已改为纯插入），
//!   否则既有 ABBA（D2），也可能对同一条连接自我死锁（tokio Mutex 不可重入）。
//! - `R` 与 `CM` 只允许 `CM → R`；`entry_gate_service` 已改成两段分离，
//!   任何时刻都不同时持有二者（D3）。
//! - 禁止在持有任何锁的情况下 `await` 网络 I/O 或 redis（`brpop` 之类）。
//!
//! 这些约束没有类型系统保护，所以：**新增分支时先写下它要拿哪些锁**。

use std::sync::{Arc, Weak};

use async_trait::async_trait;
use pyo3::prelude::*;
use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use tracing::{error, trace, warn};

use thrift::protocol::{TCompactInputProtocol, TSerializable};
use thrift::transport::TBufferChannel;

use net::{NetReader, NetReaderCallback, NetReaderCloseCallback, NetWriter};
use queue::Queue;
use redis_service::redis_mq_channel::RedisMQReader;
use redis_service::redis_service::{RedisMQListenCallback, RedisService, create_lock_key};
use tcp::tcp_server::TcpListenCallback;
use tcp::tcp_socket::{TcpReader, TcpWriter};

use proto::common::{RegServer, RegServerCallback};
use proto::gate::{GateHubService, HubCallTransferEntityComplete};
use proto::hub::HubService;

pub type StdMutex<T> = std::sync::Mutex<T>;

use crate::lock_watch::WatchMutex;
use crate::conn_manager::{ConnManager, connect_server};
use crate::gate_msg_handle::GateCallbackMsgHandle;
use crate::gate_proxy_manager::GateProxy;
use crate::hub_msg_handle::HubCallbackMsgHandle;
use crate::hub_proxy_manager::HubProxy;

async fn get_hub_name_async(conn_proxy: &Arc<WatchMutex<ConnProxy>>) -> String {
    let mut _conn_proxy = conn_proxy.as_ref().lock().await;
    if let Some(_hub_proxy) = _conn_proxy.hubproxy.clone() {
        let _proxy_tmp = _hub_proxy.as_ref().lock().await;
        _proxy_tmp.hub_name.clone().unwrap_or_default()
    } else {
        String::new()
    }
}

async fn get_gate_name_async(conn_proxy: &Arc<WatchMutex<ConnProxy>>) -> String {
    let mut _conn_proxy = conn_proxy.as_ref().lock().await;
    if let Some(_gate_proxy) = _conn_proxy.gateproxy.clone() {
        let _proxy_tmp = _gate_proxy.as_ref().lock().await;
        _proxy_tmp.gate_name.clone().unwrap_or_default()
    } else {
        String::new()
    }
}

fn handle_hub_event<F>(
    ev_data: &ConnEvent,
    rt: &tokio::runtime::Handle,
    err_msg: &str,
    py: Python<'_>,
    py_handle: Py<PyAny>,
    _hub_msg_handle: &Arc<HubCallbackMsgHandle>,
    f: F,
) where
    F: FnOnce(&mut HubCallbackMsgHandle, Python<'_>, Py<PyAny>, String),
{
    match ev_data.connproxy.upgrade() {
        Some(conn_proxy) => {
            let hub_name = rt.block_on(get_hub_name_async(&conn_proxy));
            // 零状态：直接构造，不加锁（见 HubCallbackMsgHandle::new 的注释）
            let mut handle = HubCallbackMsgHandle {};
            f(&mut handle, py, py_handle, hub_name);
        }
        None => error!("{}", err_msg),
    }
}

fn handle_gate_event<F>(
    ev_data: &ConnEvent,
    rt: &tokio::runtime::Handle,
    err_msg: &str,
    py: Python<'_>,
    py_handle: Py<PyAny>,
    _gate_msg_handle: &Arc<GateCallbackMsgHandle>,
    f: F,
) where
    F: FnOnce(&mut GateCallbackMsgHandle, Python<'_>, Py<PyAny>, String),
{
    match ev_data.connproxy.upgrade() {
        Some(conn_proxy) => {
            let gate_name = rt.block_on(get_gate_name_async(&conn_proxy));
            let mut handle = GateCallbackMsgHandle {};
            f(&mut handle, py, py_handle, gate_name);
        }
        None => error!("{}", err_msg),
    }
}

pub struct ConnProxy {
    pub wr: Arc<Mutex<Box<dyn NetWriter + Send + 'static>>>,
    pub hubproxy: Option<Arc<Mutex<HubProxy>>>,
    pub gateproxy: Option<Arc<Mutex<GateProxy>>>,
    msg_handle: Arc<StdMutex<ConnCallbackMsgHandle>>,
}

impl ConnProxy {
    pub fn new(
        _wr: Arc<Mutex<Box<dyn NetWriter + Send + 'static>>>,
        _msg_handle: Arc<StdMutex<ConnCallbackMsgHandle>>,
    ) -> ConnProxy {
        ConnProxy {
            wr: _wr,
            hubproxy: None,
            gateproxy: None,
            msg_handle: _msg_handle,
        }
    }

    /// 取 handle 的 Arc。`&self` 即可（只看不改），便于在短暂的读锁里调用。
    pub fn get_msg_handle(&self) -> Arc<StdMutex<ConnCallbackMsgHandle>> {
        self.msg_handle.clone()
    }
}

pub struct ConnEvent {
    connproxy: Weak<WatchMutex<ConnProxy>>,
    ev: HubService,
}

pub struct ConnCallbackMsgHandle {
    pub redis_service: Option<Arc<WatchMutex<RedisService>>>,
    hub_name: String,
    hub_msg_handle: Arc<HubCallbackMsgHandle>,
    gate_msg_handle: Arc<GateCallbackMsgHandle>,
    conn_mgr: Arc<WatchMutex<ConnManager>>,
    queue: Queue<Box<ConnEvent>>,
    rt_handle: Option<tokio::runtime::Handle>,
}

fn deserialize(data: Vec<u8>) -> Result<HubService, Box<dyn std::error::Error>> {
    trace!("deserialize begin!");
    let mut t = TBufferChannel::with_capacity(data.len(), 0);
    let _ = t.set_readable_bytes(&data);
    let mut i_prot = TCompactInputProtocol::new(t);
    let ev_data = HubService::read_from_in_protocol(&mut i_prot)?;
    Ok(ev_data)
}

/// 确保到某个 gate 的连接存在。
///
/// **任何时刻都不持锁做 I/O**：查表（短锁）→ 取分布式锁（只持 R）→ 二次检查/记账
/// （只持 CM）→ 建连（无锁）→ 组装 proxy → 入表（只持 CM）→ 回填来源连接（只持 CP）。
/// 旧实现把整段都塞在 `ConnManager` 锁里（含 5s 的连接超时），gate host 不通时
/// 整个 hub 的发送路径都会被拖住（hub-deadlock-audit.md 第 2 项）。
/// 确保到某个 gate 的连接存在，返回该连接对应的 `GateProxy`。
///
/// **任何时刻都不持锁做 I/O**：查表（短锁）→ 取分布式锁（只持 R）→ 二次检查/记账
/// （只持 CM）→ 建连（无锁）→ 组装 proxy → 入表（只持 CM）。
/// 旧实现把整段都塞在 `ConnManager` 锁里（含 5s 的连接超时），gate host 不通时
/// 整个 hub 的发送路径都会被拖住（hub-deadlock-audit.md 第 2 项）。
pub(crate) async fn ensure_gate_connected(
    conn_mgr: &Arc<WatchMutex<ConnManager>>,
    redis_service: &Option<Arc<WatchMutex<RedisService>>>,
    conn_msg_handle: &Arc<StdMutex<ConnCallbackMsgHandle>>,
    hub_name: &str,
    gate_name: &str,
    gate_host: &str,
) -> Option<Arc<Mutex<GateProxy>>> {
    let existing = |cm: &mut ConnManager| cm.get_gate_proxy(gate_name).cloned();

    // 1) 快查
    {
        let mut cm = conn_mgr.as_ref().lock().await;
        if let Some(gp) = existing(&mut cm) {
            return Some(gp);
        }
    }

    // 2) 分布式锁（只持 R）
    let redis = match redis_service.clone() {
        Some(r) => r,
        None => {
            error!("Missing redis_service when connecting gate '{}'", gate_name);
            return None;
        }
    };
    let lock_key = create_lock_key(hub_name.to_string(), gate_name.to_string());
    let value = {
        let mut s = redis.as_ref().lock().await;
        match s.acquire_lock(lock_key.clone(), 3, None).await {
            Ok(v) => v,
            Err(e) => {
                error!("Failed to acquire lock for gate '{}': {}", gate_name, e);
                return None;
            }
        }
    };

    // 3) 二次检查 + 记账（只持 CM）
    let existing_proxy = {
        let mut cm = conn_mgr.as_ref().lock().await;
        match existing(&mut cm) {
            Some(gp) => Some(gp),
            None => {
                cm.add_lock(lock_key.clone(), value.clone());
                None
            }
        }
    };
    if let Some(gp) = existing_proxy {
        // 别人已经建好了：把锁还回去（只持 R）
        let mut s = redis.as_ref().lock().await;
        if let Err(e) = s.release_lock(lock_key, value, None).await {
            error!("Failed to release lock for gate '{}': {}", gate_name, e);
        }
        return Some(gp);
    }

    // 4) 建连（无锁）
    let wr = connect_server(
        conn_mgr,
        gate_name.to_string(),
        gate_host.to_string(),
        conn_msg_handle.clone(),
    )
    .await?;

    // 5) 组装 + 入表（GP / CM 分开持锁）
    let mut _gate_tmp = GateProxy::new(wr);
    _gate_tmp.gate_name = Some(gate_name.to_string());
    _gate_tmp.gate_host = Some(gate_host.to_string());
    _gate_tmp.send_gate_msg(GateHubService::RegServer(RegServer::new(
        hub_name.to_string(),
        "hub".to_string(),
    )));
    let gateproxy = Arc::new(Mutex::new(_gate_tmp));
    {
        let mut cm = conn_mgr.as_ref().lock().await;
        cm.add_gate_proxy(gate_name.to_string(), gateproxy.clone());
    }

    trace!("ensure_gate_connected '{}' ready", gate_name);
    Some(gateproxy)
}

/// 一次 `poll` 的消息分发上下文。
///
/// 它是 `ConnCallbackMsgHandle` 的**快照**（全是 `Arc`/`String` 的 clone），
/// 目的是让分发过程完全脱离队列锁：旧实现把 `MutexGuard<ConnCallbackMsgHandle>`
/// 一直带到 Python 回调结束，于是 `Q` 和 `ConnProxy` 互相等待（D1）。
#[derive(Clone)]
struct PollState {
    hub_name: String,
    hub_msg_handle: Arc<HubCallbackMsgHandle>,
    gate_msg_handle: Arc<GateCallbackMsgHandle>,
    conn_mgr: Arc<WatchMutex<ConnManager>>,
    redis_service: Option<Arc<WatchMutex<RedisService>>>,
    rt: tokio::runtime::Handle,
}

impl ConnCallbackMsgHandle {
    pub fn new(
        _hub_name: String,
        _hub_msg_handle: Arc<HubCallbackMsgHandle>,
        _gate_msg_handle: Arc<GateCallbackMsgHandle>,
        _conn_mgr: Arc<WatchMutex<ConnManager>>,
    ) -> Arc<StdMutex<ConnCallbackMsgHandle>> {
        Arc::new(StdMutex::new(ConnCallbackMsgHandle {
            hub_name: _hub_name,
            hub_msg_handle: _hub_msg_handle,
            gate_msg_handle: _gate_msg_handle,
            redis_service: None,
            conn_mgr: _conn_mgr,
            queue: Queue::new(),
            rt_handle: None,
        }))
    }

    pub fn set_rt_handle(&mut self, handle: tokio::runtime::Handle) {
        self.rt_handle = Some(handle);
    }

    pub fn get_conn_mgr(&self) -> Arc<WatchMutex<ConnManager>> {
        self.conn_mgr.clone()
    }

    fn enque_event(&mut self, ev: ConnEvent) {
        let _ = self.queue.enque(Box::new(ev));
    }

    /// 取出至多 `max` 条事件（`poll` 在释放队列锁之前调用一次）。
    fn drain_events(&mut self, max: usize) -> Vec<Box<ConnEvent>> {
        let mut events = Vec::with_capacity(std::cmp::min(self.queue.len(), max));
        for _ in 0..max {
            match self.queue.deque() {
                Some(ev) => events.push(ev),
                None => break,
            }
        }
        events
    }

    /// 反序列化 + 入队（在连接 reader 任务里执行）。
    ///
    /// ⚠ 锁序：`ConnProxy` **只用于取一次 `msg_handle` 的 Arc，取到就释放**，
    ///   绝不在持有 `ConnProxy` 的情况下去抢队列锁 `Q`。
    ///   旧实现在持 `CP` 时抢 `Q`，而 `poll` 又持 `Q` 去 `block_on` 抢 `CP`，
    ///   构成 ABBA（见 hub-deadlock-audit.md D1）。这一条就是 D1 的入队侧半边。
    pub async fn on_event(_proxy: Arc<WatchMutex<ConnProxy>>, data: Vec<u8>) {
        trace!("do_event begin!");

        // 反序列化不需要任何锁
        let _ev = match deserialize(data) {
            Err(e) => {
                error!("GateClientMsgHandle do_event err:{}", e);
                return;
            }
            Ok(d) => d,
        };

        // 短暂持 CP 只为取 handle，随后立刻释放
        let _handle_arc = {
            let _p = _proxy.as_ref().lock().await;
            _p.get_msg_handle()
        };

        let mut _handle = _handle_arc
            .as_ref()
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        _handle.enque_event(ConnEvent {
            connproxy: Arc::downgrade(&_proxy),
            ev: _ev,
        })
    }

    /// 泵入口：**先把队列整体取空、立刻释放队列锁**，再逐条分发。
    ///
    /// 修复的死锁（D1）：旧实现整条消息处理期间（含 `rt.block_on` 与 Python 回调）
    /// 一直持有队列的 `std::sync::Mutex`；reader 任务则「持 `ConnProxy` 抢队列锁」。
    /// 只要同一连接还有后续报文，两边就会互相等死。
    /// 现在 `Q` 只在「取空队列」这一刻持有，之后全程不碰，
    /// 因此 `Q` 退化为纯叶子锁，不再参与任何环。
    pub fn poll(
        _handle: Arc<StdMutex<ConnCallbackMsgHandle>>,
        py: Python<'_>,
        py_handle: Py<PyAny>,
    ) -> bool {
        // 单次最多处理这么多条：避免一次突发把 GIL 和 tick 一起拖爆，
        // 剩下的留给 Python 侧下一轮 `poll`（调用方本来就是 while 循环）。
        const MAX_BATCH: usize = 1024;

        let (_self, events) = {
            let mut _guard = _handle.as_ref().lock().unwrap_or_else(|e| e.into_inner());
            let events = _guard.drain_events(MAX_BATCH);
            let _self = PollState {
                hub_name: _guard.hub_name.clone(),
                hub_msg_handle: _guard.hub_msg_handle.clone(),
                gate_msg_handle: _guard.gate_msg_handle.clone(),
                conn_mgr: _guard.conn_mgr.clone(),
                redis_service: _guard.redis_service.clone(),
                rt: _guard.rt_handle.as_ref().expect("rt_handle not set").clone(),
            };
            (_self, events)
        }; // ← 队列锁在这里释放，后面不再持有

        if events.is_empty() {
            return false;
        }

        for ev_data in events {
            // Py<PyAny> 不是 Copy：每条消息拿一份引用计数，body 里按值传递的用法不变。
            Self::dispatch_one(&_self, &_handle, ev_data, py, py_handle.clone_ref(py));
        }

        true
    }

    /// 分发单条消息。**调用时不得持有队列锁 `Q`**。
    fn dispatch_one(
        _self: &PollState,
        _handle: &Arc<StdMutex<ConnCallbackMsgHandle>>,
        ev_data: Box<ConnEvent>,
        py: Python<'_>,
        py_handle: Py<PyAny>,
    ) {
        let _handle_clone = _handle.clone();
        let rt = _self.rt.clone();

        match ev_data.ev {
            // hub msg handle
            HubService::RegServer(ev) => {
                trace!("ev_data.ev HubService::RegServer!");
                if let Some(conn_proxy) = ev_data.connproxy.upgrade() {
                    let mut _hub_msg_handle_c = _self.hub_msg_handle.clone();
                    let ev_tmp = ev.clone();
                    rt.block_on(async move {
                        {
                            let mut _conn_proxy = conn_proxy.as_ref().lock().await;

                            let svr_name = ev.name.clone().unwrap_or_default();
                            let svr_type = ev.type_.clone().unwrap_or_default();

                            let cb_msg = RegServerCallback::new(_self.hub_name.clone());

                            if svr_type == "hub" {
                                let _proxy =
                                    Arc::new(Mutex::new(HubProxy::new(_conn_proxy.wr.clone())));

                                // 写字段 / 入表 / 发回执 三段分别持 GP、CM、GP，
                                // 保证「CM 先于 GP」的锁序（不要写成持 GP 再抢 CM）。
                                {
                                    let mut _proxy_tmp = _proxy.as_ref().lock().await;
                                    _proxy_tmp.hub_name = Some(svr_name.clone());
                                }

                                let mut _conn_mgr = _self.conn_mgr.as_ref().lock().await;
                                _conn_mgr.add_hub_proxy(svr_name.clone(), _proxy.clone());
                                _conn_proxy.hubproxy = Some(_proxy.clone());

                                _proxy
                                    .as_ref()
                                    .lock()
                                    .await
                                    .send_hub_msg(HubService::RegServerCallback(cb_msg));
                            } else if svr_type == "gate" {
                                let _gate_name_tmp = svr_name.clone();
                                let mut _gate_tmp = GateProxy::new(_conn_proxy.wr.clone());

                                _gate_tmp.gate_name = Some(svr_name.clone());
                                _gate_tmp
                                    .send_gate_msg(GateHubService::RegServerCallback(cb_msg));

                                let _gateproxy = Arc::new(Mutex::new(_gate_tmp));
                                let mut _conn_mgr = _self.conn_mgr.as_ref().lock().await;
                                _conn_mgr.add_gate_proxy(_gate_name_tmp, _gateproxy);
                            }
                        }
                    });
                    let mut _hub_msg_handle = HubCallbackMsgHandle {};
                    _hub_msg_handle.do_reg_hub(py, py_handle, ev_tmp);
                } else {
                    error!("hub reg hub conn_proxy is destory!");
                }
            }
            HubService::RegServerCallback(ev) => {
                trace!("ev_data.ev HubService::RegServerCallback!");
                rt.block_on(async move {
                    let mut _conn_mgr = _self.conn_mgr.as_ref().lock().await;
                    let lock_key = create_lock_key(
                        _self.hub_name.clone(),
                        ev.name.clone().unwrap_or_default(),
                    );
                    let value = _conn_mgr.remove_lock(lock_key.clone());
                    let _redis_service = match _self.redis_service.clone() {
                        Some(s) => s,
                        None => {
                            error!("Missing redis_service in msg handle");
                            return;
                        }
                    };
                    let mut _service = _redis_service.as_ref().lock().await;
                    if let Err(e) = _service.release_lock(lock_key.clone(), value, None).await {
                        error!("Failed to release lock '{}': {}", lock_key, e);
                    }
                });
            }
            HubService::QueryEntity(ref ev) => {
                trace!("ev_data.ev HubService::QueryEntity!");
                handle_hub_event(
                    &ev_data,
                    &rt,
                    "hub query entity conn_proxy is destory!",
                    py,
                    py_handle,
                    &_self.hub_msg_handle,
                    |h, py, pyh, name| h.do_query_service_entity(py, pyh, name, ev.clone()),
                );
            }
            HubService::CreateServiceEntity(ref ev) => {
                trace!("ev_data.ev HubService::CreateServiceEntity!");
                handle_hub_event(
                    &ev_data,
                    &rt,
                    "hub create service entity conn_proxy is destory!",
                    py,
                    py_handle,
                    &_self.hub_msg_handle,
                    |h, py, pyh, name| h.do_create_service_entity(py, pyh, name, ev.clone()),
                );
            }
            HubService::HubForwardClientRequestService(ev) => {
                trace!("ev_data.ev HubService::HubForwardClientRequestService!");
                if let Some(conn_proxy) = ev_data.connproxy.upgrade() {
                    let mut _hub_msg_handle_c = _self.hub_msg_handle.clone();
                    let ev_tmp = ev.clone();
                    trace!("ev_data.ev HubService::HubForwardClientRequestService! get ev_tmp");
                    let hub_name = rt.block_on(async move {
                        trace!("ev_data.ev HubService::HubForwardClientRequestService! rt.block_on");
                        let mut hub_name: String = "".to_string();
                        {
                            let mut _conn_proxy = conn_proxy.as_ref().lock().await;
                            trace!("ev_data.ev HubService::HubForwardClientRequestService! conn_proxy.lock");

                            if let Some(_hub_proxy) = _conn_proxy.hubproxy.clone() {
                                // 读到名字就地释放 HP，再进 CM → R 的路径：
                                // 持 HP 再抢 CM 会与 `send_hub_msg` 的 CM → HP 形成 ABBA。
                                hub_name = {
                                    let _proxy_tmp = _hub_proxy.as_ref().lock().await;
                                    trace!("ev_data.ev HubService::HubForwardClientRequestService! hubproxy.lock");
                                    _proxy_tmp.hub_name.clone().unwrap_or_default()
                                };
                                let _gate_name = match ev.gate_name.clone() {
                                    Some(n) => n,
                                    None => {
                                        error!("Missing required field 'gate_name' in HubForwardClientRequestService");
                                        return hub_name;
                                    }
                                };
                                let _gate_host = ev.gate_host.clone().unwrap_or_default();

                                // 建连：查表 / 取锁 / 建连 / 登记 分段持锁，不持锁做 I/O
                                if let Some(_gateproxy) =
                                    crate::hub_service_manager::ensure_gate_connected(
                                        &_self.conn_mgr,
                                        &_self.redis_service,
                                        &_handle_clone,
                                        &_self.hub_name,
                                        &_gate_name,
                                        &_gate_host,
                                    )
                                    .await
                                {
                                    _conn_proxy.gateproxy = Some(_gateproxy);
                                }
                                trace!("ev_data.ev HubService::HubForwardClientRequestService! ensure_gate_connected");
                            }
                            else {
                                error!("HubService::HubForwardClientRequestService! wrong msg handle!");
                            }
                        }
                        trace!("ev_data.ev HubService::HubForwardClientRequestService! async move hub_name:{}", hub_name);
                        return hub_name;
                    });

                    trace!(
                        "ev_data.ev HubService::HubForwardClientRequestService! hub_name:{}",
                        hub_name
                    );
                    let mut _hub_msg_handle = HubCallbackMsgHandle {};
                    _hub_msg_handle
                        .do_forward_client_request_service(py, py_handle, hub_name, ev_tmp);
                } else {
                    error!("hub forward client request service conn_proxy is destory!");
                }
            }            HubService::HubForwardClientRequestServiceExt(ev) => {
                trace!("ev_data.ev HubService::HubForwardClientRequestServiceExt!");
                if let Some(conn_proxy) = ev_data.connproxy.upgrade() {
                    let mut _hub_msg_handle_c = _self.hub_msg_handle.clone();
                    let ev_tmp = ev.clone();
                    let hub_name = rt.block_on(async move {
                        let mut hub_name: String = "".to_string();
                        {
                            let mut _conn_proxy = conn_proxy.as_ref().lock().await;

                            if let Some(_hub_proxy) = _conn_proxy.hubproxy.clone() {
                                // 同上：读完名字立刻释放 HP，再进入 CM → R 的路径。
                                hub_name = {
                                    let _proxy_tmp = _hub_proxy.as_ref().lock().await;
                                    _proxy_tmp.hub_name.clone().unwrap_or_default()
                                };

                                for info in ev.request_infos.unwrap_or_default() {
                                    let _gate_name = match info.gate_name.clone() {
                                        Some(n) => n,
                                        None => {
                                            error!("Missing required field 'gate_name' in HubForwardClientRequestServiceExt");
                                            continue;
                                        }
                                    };
                                    let _gate_host = info.gate_host.clone().unwrap_or_default();

                                    // 建连：查表 / 取锁 / 建连 / 登记 分段持锁，不持锁做 I/O
                                    if let Some(_gateproxy) =
                                        crate::hub_service_manager::ensure_gate_connected(
                                            &_self.conn_mgr,
                                            &_self.redis_service,
                                            &_handle_clone,
                                            &_self.hub_name,
                                            &_gate_name,
                                            &_gate_host,
                                        )
                                        .await
                                    {
                                        _conn_proxy.gateproxy = Some(_gateproxy);
                                    }
                                }
                            }
                            else {
                                error!("HubService::HubForwardClientRequestService! wrong msg handle!");
                            }
                        }
                        return hub_name;
                    });

                    let mut _hub_msg_handle = HubCallbackMsgHandle {};
                    _hub_msg_handle
                        .do_forward_client_request_service_ext(py, py_handle, hub_name, ev_tmp);
                } else {
                    error!("hub forward client request service conn_proxy is destory!");
                }
            }
            HubService::HubCallRpc(ref ev) => {
                trace!("ev_data.ev HubService::HubCallRpc!");
                handle_hub_event(
                    &ev_data,
                    &rt,
                    "hub call rpc conn_proxy is destory!",
                    py,
                    py_handle,
                    &_self.hub_msg_handle,
                    |h, py, pyh, name| h.do_call_hub_rpc(py, pyh, name, ev.clone()),
                );
            }
            HubService::HubCallRsp(ref ev) => {
                trace!("ev_data.ev HubService::HubCallRsp!");
                handle_hub_event(
                    &ev_data,
                    &rt,
                    "hub call rsp conn_proxy is destory!",
                    py,
                    py_handle,
                    &_self.hub_msg_handle,
                    |h, py, pyh, name| h.do_call_hub_rsp(py, pyh, name, ev.clone()),
                );
            }
            HubService::HubCallErr(ref ev) => {
                trace!("ev_data.ev HubService::HubCallErr!");
                handle_hub_event(
                    &ev_data,
                    &rt,
                    "hub call err conn_proxy is destory!",
                    py,
                    py_handle,
                    &_self.hub_msg_handle,
                    |h, py, pyh, name| h.do_call_hub_err(py, pyh, name, ev.clone()),
                );
            }
            HubService::HubCallNtf(ref ev) => {
                trace!("ev_data.ev HubService::HubCallNtf!");
                handle_hub_event(
                    &ev_data,
                    &rt,
                    "hub call ntf conn_proxy is destory!",
                    py,
                    py_handle,
                    &_self.hub_msg_handle,
                    |h, py, pyh, name| h.do_call_hub_ntf(py, pyh, name, ev.clone()),
                );
            }
            HubService::WaitMigrateEntity(ref ev) => {
                trace!("ev_data.ev HubService::WaitMigrateEntity!");
                handle_hub_event(
                    &ev_data,
                    &rt,
                    "wait migrate entity conn_proxy is destory!",
                    py,
                    py_handle,
                    &_self.hub_msg_handle,
                    |h, py, pyh, name| h.do_wait_migrate_entity(py, pyh, name, ev.clone()),
                );
            }
            HubService::MigrateEntity(ref ev) => {
                trace!("ev_data.ev HubService::MigrateEntity!");
                handle_hub_event(
                    &ev_data,
                    &rt,
                    "migrate entity conn_proxy is destory!",
                    py,
                    py_handle,
                    &_self.hub_msg_handle,
                    |h, py, pyh, name| h.do_migrate_entity(py, pyh, name, ev.clone()),
                );
            }
            HubService::CreateMigrateEntity(ref ev) => {
                trace!("ev_data.ev HubService::CreateMigrateEntity!");
                handle_hub_event(
                    &ev_data,
                    &rt,
                    "migrate entity complete conn_proxy is destory!",
                    py,
                    py_handle,
                    &_self.hub_msg_handle,
                    |h, py, pyh, name| h.do_create_migrate_entity(py, pyh, name, ev.clone()),
                );
            }
            HubService::MigrateEntityComplete(ref ev) => {
                trace!("ev_data.ev HubService::MigrateEntityComplete!");
                handle_hub_event(
                    &ev_data,
                    &rt,
                    "migrate entity complete conn_proxy is destory!",
                    py,
                    py_handle,
                    &_self.hub_msg_handle,
                    |h, py, pyh, name| h.do_migrate_entity_complete(py, pyh, name, ev.clone()),
                );
            }

            // gate msg handle
            HubService::ClientRequestLogin(ev) => {
                if let Some(conn_proxy) = ev_data.connproxy.upgrade() {
                    let _gate_msg_handle_c = _self.gate_msg_handle.clone();
                    let ev_tmp = ev.clone();
                    rt.block_on(async move {
                        {
                            let mut _conn_proxy = conn_proxy.as_ref().lock().await;

                            let _gate_name = match ev.gate_name.clone() {
                                Some(n) => n,
                                None => {
                                    error!(
                                        "Missing required field 'gate_name' in ClientRequestLogin"
                                    );
                                    return;
                                }
                            };
                            let _proxy =
                                Arc::new(Mutex::new(GateProxy::new(_conn_proxy.wr.clone())));

                            let mut _conn_mgr = _self.conn_mgr.as_ref().lock().await;
                            _conn_mgr.add_gate_proxy(_gate_name.clone(), _proxy.clone());
                            _conn_proxy.gateproxy = Some(_proxy.clone());

                            let mut _proxy_tmp = _proxy.as_ref().lock().await;
                            _proxy_tmp.gate_name = Some(_gate_name.clone());
                            _proxy_tmp.gate_host = Some(ev.clone().gate_host.unwrap_or_default());

                            let cb_msg = RegServerCallback::new(_self.hub_name.clone());
                            let msg = GateHubService::RegServerCallback(cb_msg);
                            _proxy_tmp.send_gate_msg(msg);
                        }
                    });

                    let mut _gate_msg_handle = GateCallbackMsgHandle {};
                    _gate_msg_handle.do_client_request_login(py, py_handle, ev_tmp);
                } else {
                    error!("gate client request login conn_proxy is destory!");
                }
            }
            HubService::ClientRequestReconnect(ev) => {
                if let Some(conn_proxy) = ev_data.connproxy.upgrade() {
                    let _gate_msg_handle_c = _self.gate_msg_handle.clone();
                    let _ev_tmp = ev.clone();
                    rt.block_on(async move {
                        {
                            let mut _conn_proxy = conn_proxy.as_ref().lock().await;

                            let _gate_name = match ev.gate_name.clone() {
                            Some(n) => n,
                            None => {
                                error!("Missing required field 'gate_name' in ClientRequestReconnect");
                                return;
                            }
                        };

                            let _proxy = Arc::new(Mutex::new(
                                GateProxy::new(_conn_proxy.wr.clone())
                            ));
                            // 先把名字/地址写进 proxy 再入表；写的时候单独持 GP，
                            // 入表时单独持 CM —— 绝不同时持 GP 和 CM（锁序 CM → GP）。
                            {
                                let mut _proxy_tmp = _proxy.as_ref().lock().await;
                                _proxy_tmp.gate_name = Some(_gate_name.clone());
                                _proxy_tmp.gate_host = ev.gate_host;
                            }

                            let mut _conn_mgr = _self.conn_mgr.as_ref().lock().await;
                            _conn_mgr.add_gate_proxy(_gate_name.clone(), _proxy.clone());
                            _conn_proxy.gateproxy = Some(_proxy.clone());

                            let cb_msg = RegServerCallback::new(_self.hub_name.clone());
                            let msg = GateHubService::RegServerCallback(cb_msg);
                            _proxy.as_ref().lock().await.send_gate_msg(msg);
                        }
                    });

                    let mut _gate_msg_handle = GateCallbackMsgHandle {};
                    _gate_msg_handle.do_client_request_reconnect(py, py_handle, _ev_tmp);
                } else {
                    error!("gate client request reconnect conn_proxy is destory!");
                }
            }
            HubService::TransferMsgEnd(ev) => {
                let mut _gate_msg_handle_c = GateCallbackMsgHandle {};
                _gate_msg_handle_c.do_transfer_msg_end(py, py_handle, ev);
            }
            HubService::TransferEntityControl(ev) => {
                trace!(
                    "TransferEntityControl conn_id={:?} entity_id={:?}",
                    ev.conn_id, ev.entity_id
                );

                let conn_id = ev.conn_id.clone();
                let entity_id = ev.entity_id.clone();

                // 先释放 gate_msg_handle 再 block_on：否则会形成 GM → CP 的持锁跨 await。
                {
                    let mut _gate_msg_handle = GateCallbackMsgHandle {};
                    _gate_msg_handle.do_transfer_entity_control(py, py_handle, ev);
                }

                if let Some(conn_proxy) = ev_data.connproxy.upgrade() {
                    _ = rt.block_on(async move {
                        let mut _conn_proxy = conn_proxy.as_ref().lock().await;

                        if let Some(_gate_proxy) = &_conn_proxy.gateproxy {
                            let mut _proxy_tmp = _gate_proxy.as_ref().lock().await;
                            _proxy_tmp
                                .send_gate_msg(GateHubService::TransferComplete(
                                    HubCallTransferEntityComplete::new(
                                        conn_id.unwrap_or_default(),
                                        entity_id.unwrap_or_default(),
                                    ),
                                ));
                        } else {
                            error!("HubService::TransferEntityControl! wrong msg handle!");
                        }

                        trace!("TransferEntityControl send_gate_msg end!");
                    });
                } else {
                    error!("gate client Transfer Entity Control conn_proxy is destory!");
                }

                trace!("TransferEntityControl end!");
            }
            HubService::KickOffClient(ref ev) => {
                handle_gate_event(
                    &ev_data,
                    &rt,
                    "gate client kick off conn_proxy is destory!",
                    py,
                    py_handle,
                    &_self.gate_msg_handle,
                    |h, py, pyh, name| h.do_client_kick_off(py, pyh, name, ev.clone()),
                );
            }
            HubService::ClientDisconnnect(ref ev) => {
                handle_gate_event(
                    &ev_data,
                    &rt,
                    "gate client disconnect conn_proxy is destory!",
                    py,
                    py_handle,
                    &_self.gate_msg_handle,
                    |h, py, pyh, name| h.do_client_disconnnect(py, pyh, name, ev.clone()),
                );
            }
            HubService::ClientRequestService(ev) => {
                if let Some(conn_proxy) = ev_data.connproxy.upgrade() {
                    let _gate_msg_handle_c = _self.gate_msg_handle.clone();
                    let _ev_tmp = ev.clone();
                    let gate_name = rt.block_on(async move {
                        let gate_name: String;
                        {
                            let mut _conn_proxy = conn_proxy.as_ref().lock().await;

                            if let Some(_gate_proxy) = _conn_proxy.gateproxy.clone() {
                                let _proxy_tmp = _gate_proxy.as_ref().lock().await;
                                gate_name = _proxy_tmp.gate_name.clone().unwrap_or_default();
                            } else {
                                gate_name = ev.gate_name.unwrap_or_default();
                                let _proxy =
                                    Arc::new(Mutex::new(GateProxy::new(_conn_proxy.wr.clone())));
                                // 同上：写字段与入表分开持锁，避免 GP → CM 的逆序。
                                {
                                    let mut _proxy_tmp = _proxy.as_ref().lock().await;
                                    _proxy_tmp.gate_name = Some(gate_name.clone());
                                    _proxy_tmp.gate_host = ev.gate_host;
                                }

                                let mut _conn_mgr = _self.conn_mgr.as_ref().lock().await;
                                _conn_mgr.add_gate_proxy(gate_name.clone(), _proxy.clone());
                                _conn_proxy.gateproxy = Some(_proxy.clone());

                                let cb_msg = RegServerCallback::new(_self.hub_name.clone());
                                let msg = GateHubService::RegServerCallback(cb_msg);
                                _proxy.as_ref().lock().await.send_gate_msg(msg);

                                warn!("HubService::ClientRequestService! wrong msg handle!");
                            }
                        }
                        return gate_name;
                    });
                    let mut _gate_msg_handle = GateCallbackMsgHandle {};
                    _gate_msg_handle.do_client_request_service(py, py_handle, gate_name, _ev_tmp);
                } else {
                    error!("gate client request service conn_proxy is destory!");
                }
            }
            HubService::ClientCallRpc(ref ev) => {
                handle_gate_event(
                    &ev_data,
                    &rt,
                    "gate client call rpc conn_proxy is destory!",
                    py,
                    py_handle,
                    &_self.gate_msg_handle,
                    |h, py, pyh, name| h.do_client_call_rpc(py, pyh, name, ev.clone()),
                );
            }
            HubService::ClientCallRsp(ref ev) => {
                handle_gate_event(
                    &ev_data,
                    &rt,
                    "gate client call rsp conn_proxy is destory!",
                    py,
                    py_handle,
                    &_self.gate_msg_handle,
                    |h, py, pyh, name| h.do_client_call_rsp(py, pyh, name, ev.clone()),
                );
            }
            HubService::ClientCallErr(ref ev) => {
                handle_gate_event(
                    &ev_data,
                    &rt,
                    "gate client call err conn_proxy is destory!",
                    py,
                    py_handle,
                    &_self.gate_msg_handle,
                    |h, py, pyh, name| h.do_client_call_err(py, pyh, name, ev.clone()),
                );
            }
            HubService::ClientCallNtf(ref ev) => {
                handle_gate_event(
                    &ev_data,
                    &rt,
                    "gate client call ntf conn_proxy is destory!",
                    py,
                    py_handle,
                    &_self.gate_msg_handle,
                    |h, py, pyh, name| h.do_client_call_ntf(py, pyh, name, ev.clone()),
                );
            }
        };
    }
}

pub struct ConnProxyReaderCallback {
    connproxy: Arc<WatchMutex<ConnProxy>>,
}

#[async_trait]
impl NetReaderCallback for ConnProxyReaderCallback {
    async fn cb(&mut self, data: Vec<u8>) {
        ConnCallbackMsgHandle::on_event(self.connproxy.clone(), data).await
    }
}

impl ConnProxyReaderCallback {
    pub fn new(_connproxy: Arc<WatchMutex<ConnProxy>>) -> ConnProxyReaderCallback {
        ConnProxyReaderCallback {
            connproxy: _connproxy,
        }
    }
}

pub struct ConnProxyCloseCallback {
    connproxy: Arc<WatchMutex<ConnProxy>>,
    conn_mgr: Arc<WatchMutex<ConnManager>>,
    name: Option<String>,
}

impl ConnProxyCloseCallback {
    pub fn new(
        _connproxy: Arc<WatchMutex<ConnProxy>>,
        _conn_mgr: Arc<WatchMutex<ConnManager>>,
        _name: Option<String>,
    ) -> ConnProxyCloseCallback {
        ConnProxyCloseCallback {
            connproxy: _connproxy,
            conn_mgr: _conn_mgr,
            name: _name,
        }
    }
}

#[async_trait]
impl NetReaderCloseCallback for ConnProxyCloseCallback {
    async fn on_close(&mut self) {
        let wr: Arc<Mutex<Box<dyn NetWriter + Send + 'static>>>;
        let mut peer_name = self.name.clone();
        {
            let mut _conn = self.connproxy.as_ref().lock().await;
            wr = _conn.wr.clone();
            if peer_name.is_none() {
                if let Some(gp) = &_conn.gateproxy {
                    let g = gp.as_ref().lock().await;
                    peer_name = g.gate_name.clone();
                }
            }
            if peer_name.is_none() {
                if let Some(hp) = &_conn.hubproxy {
                    let h = hp.as_ref().lock().await;
                    peer_name = h.hub_name.clone();
                }
            }
        }

        let mut _wr = wr.as_ref().lock().await;
        _wr.close().await;
        drop(_wr);

        if let Some(n) = peer_name {
            let mut _conn_mgr = self.conn_mgr.as_ref().lock().await;
            _conn_mgr.remove_conn_proxy(&n);
        }
    }
}

pub struct ConnProxyManager {
    conn_msg_handle: Arc<StdMutex<ConnCallbackMsgHandle>>,
    join_list: Vec<JoinHandle<()>>,
}

#[async_trait]
impl RedisMQListenCallback for ConnProxyManager {
    async fn redis_mq_cb(
        &mut self,
        rd: Arc<Mutex<RedisMQReader>>,
        wr: Arc<Mutex<Box<dyn NetWriter + Send + 'static>>>,
    ) {
        self.prune_joins();
        let _connproxy = Arc::new(WatchMutex::new("ConnProxy", ConnProxy::new(wr, self.conn_msg_handle.clone())));
        let mut _rd_ref = rd.as_ref().lock().await;
        self.join_list
            .push(_rd_ref.start(Arc::new(Mutex::new(Box::new(
                ConnProxyReaderCallback::new(_connproxy),
            )))));
    }
}

#[async_trait]
impl TcpListenCallback for ConnProxyManager {
    async fn cb(&mut self, rd: TcpReader, wr: TcpWriter) {
        self.prune_joins();
        let _wr_arc: Arc<Mutex<Box<dyn NetWriter + Send + 'static>>> =
            Arc::new(Mutex::new(Box::new(wr)));
        let _connproxy = Arc::new(WatchMutex::new("ConnProxy", ConnProxy::new(
            _wr_arc,
            self.conn_msg_handle.clone(),
        )));
        let conn_mgr = {
            let h = self
                .conn_msg_handle
                .as_ref()
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            h.get_conn_mgr()
        };
        let _close_cb: Arc<Mutex<Box<dyn NetReaderCloseCallback + Send + 'static>>> =
            Arc::new(Mutex::new(Box::new(ConnProxyCloseCallback::new(
                _connproxy.clone(),
                conn_mgr,
                None,
            ))));
        self.join_list.push(rd.start(
            Arc::new(Mutex::new(Box::new(ConnProxyReaderCallback::new(
                _connproxy,
            )))),
            Some(_close_cb),
        ));
    }
}

impl ConnProxyManager {
    /// 清理已结束的 reader 任务句柄，避免连接重连时 `join_list` 无限增长。
    fn prune_joins(&mut self) {
        self.join_list.retain(|j| !j.is_finished());
    }

    pub fn new_tcp_callback(
        _conn_msg_handle: Arc<StdMutex<ConnCallbackMsgHandle>>,
    ) -> Arc<Mutex<Box<dyn TcpListenCallback + Send + 'static>>> {
        Arc::new(Mutex::new(Box::new(ConnProxyManager {
            conn_msg_handle: _conn_msg_handle,
            join_list: Vec::new(),
        })))
    }

    pub fn new_redis_mq_callback(
        _conn_msg_handle: Arc<StdMutex<ConnCallbackMsgHandle>>,
    ) -> Arc<Mutex<Box<dyn RedisMQListenCallback + Send + 'static>>> {
        Arc::new(Mutex::new(Box::new(ConnProxyManager {
            conn_msg_handle: _conn_msg_handle,
            join_list: Vec::new(),
        })))
    }
}
