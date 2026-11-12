use std::sync::Arc;

use rand::Rng;
use tokio::sync::Mutex;
use tracing::{error, info, warn};

use thrift::protocol::{TCompactOutputProtocol, TSerializable};

use net::NetWriter;
use redis_service::redis_service::{create_lock_key, create_host_cache_key, RedisService};
use close_handle::CloseHandle;
use consul::ConsulImpl;

use proto::common::RegServer;
use proto::hub::HubService;

use crate::conn_manager::{ConnManager, connect_server};
use crate::hub_service_manager::{ConnCallbackMsgHandle, StdMutex};
use crate::lock_watch::WatchMutex;

/// 确保到某个 hub 的连接存在。语义与
/// `hub_service_manager::ensure_gate_connected` 一致：
/// 查表（短锁）→ 取分布式锁（只持 R）→ 二次检查/记账（只持 CM）→ 建连（**无锁**）
/// → 登记（只持 CM）。
///
/// 旧实现把「拿锁 + 建连 + 登记」全塞在 `ConnManager` 锁里，连接超时（5s）期间
/// 所有 `send_*` 都会被卡住（hub-deadlock-audit.md 第 2 项）。
pub async fn ensure_hub_connected(
    conn_mgr: &Arc<WatchMutex<ConnManager>>,
    redis: &Arc<WatchMutex<RedisService>>,
    conn_msg_handle: &Arc<StdMutex<ConnCallbackMsgHandle>>,
    hub_name: &str,
    host: &str,
) -> bool {
    if conn_mgr
        .as_ref()
        .lock()
        .await
        .get_hub_proxy(hub_name)
        .is_some()
    {
        return true;
    }

    let self_name = { conn_mgr.as_ref().lock().await.get_hub_name() };
    let lock_key = create_lock_key(self_name.clone(), hub_name.to_string());
    let value = {
        let mut s = redis.as_ref().lock().await;
        match s.acquire_lock(lock_key.clone(), 3, None).await {
            Ok(v) => v,
            Err(e) => {
                error!("Failed to acquire lock for hub '{}': {}", hub_name, e);
                return false;
            }
        }
    };

    let exists = {
        let mut cm = conn_mgr.as_ref().lock().await;
        if cm.get_hub_proxy(hub_name).is_some() {
            true
        } else {
            cm.add_lock(lock_key.clone(), value.clone());
            false
        }
    };
    if exists {
        let mut s = redis.as_ref().lock().await;
        if let Err(e) = s.release_lock(lock_key, value, None).await {
            warn!("Failed to release lock for hub '{}': {}", hub_name, e);
        }
        return true;
    }

    let wr = match connect_server(
        conn_mgr,
        hub_name.to_string(),
        host.to_string(),
        conn_msg_handle.clone(),
    )
    .await
    {
        Some(w) => w,
        None => return false,
    };

    let mut _hub_send = HubProxy::new(wr);
    _hub_send.send_hub_msg(HubService::RegServer(RegServer::new(
        self_name,
        "hub".to_string(),
    )));
    _hub_send.hub_name = Some(hub_name.to_string());
    let hubproxy = Arc::new(Mutex::new(_hub_send));
    {
        let mut cm = conn_mgr.as_ref().lock().await;
        cm.add_hub_proxy(hub_name.to_string(), hubproxy);
    }

    true
}

pub async fn entry_direct_hub_server(
    _hub_name: String,
    _conn_msg_handle: Arc<StdMutex<ConnCallbackMsgHandle>>,
    _conn_mgr: Arc<WatchMutex<ConnManager>>,
    _redis_mq_service: Arc<WatchMutex<RedisService>>,
    _close: Arc<Mutex<CloseHandle>>,
) {
    // 对端地址（只持 R；不持 CM 做 redis I/O）
    let _hub_host = {
        let mut _r = _redis_mq_service.as_ref().lock().await;
        _r.get(create_host_cache_key(_hub_name.clone()), None)
            .await
            .unwrap_or_default()
    };

    let _ = ensure_hub_connected(
        &_conn_mgr,
        &_redis_mq_service,
        &_conn_msg_handle,
        &_hub_name,
        &_hub_host,
    )
    .await;
}

pub async fn entry_hub_service(
    _service: String,
    _conn_msg_handle: Arc<StdMutex<ConnCallbackMsgHandle>>,
    _conn_mgr: Arc<WatchMutex<ConnManager>>,
    _redis_mq_service: Arc<WatchMutex<RedisService>>,
    _consul_impl: Arc<Mutex<ConsulImpl>>,
    _close: Arc<Mutex<CloseHandle>>,
) -> String {
    let _service_name = _service.clone();

    // consul 查询：CU 只在这段持有，不再一路带到建连（旧实现会持有整个循环）
    let mut services = {
        let mut _impl = _consul_impl.as_ref().lock().await;
        match _impl.services(_service).await {
            None => return String::new(),
            Some(s) => s,
        }
    };
    // consul 里可能一个实例都查不到（服务没启动 / 服务名对不上 / consul -dev 重启丢了注册）。
    // 此时下面的 gen_range(0..0) 会 panic，而 pyo3-async-runtimes 会把 panic 转成
    // "rust future panicked: unknown error" 抛回 Python，调用方拿不到任何有用信息，
    // 所以必须先判空并返回空串（Python 侧按服务不存在处理）。
    if services.is_empty() {
        error!(
            "entry_hub_service '{}' has no available instance!",
            _service_name
        );
        return String::new();
    }

    loop {
        let index: usize = {
            let mut rng = rand::thread_rng();
            rng.gen_range(0..services.len())
        };
        let (id, addr, port) = match services.get(index) {
            None => return String::new(),
            Some(s) => (s.id.clone(), s.addr.clone(), s.port),
        };
        info!("entry_hub_service:{}-{}", id, addr);

        if ensure_hub_connected(
            &_conn_mgr,
            &_redis_mq_service,
            &_conn_msg_handle,
            &id,
            &format!("{}:{}", addr, port),
        )
        .await
        {
            return id;
        }

        services.remove(index);
        if services.is_empty() {
            error!("entry_hub_service faild!");
            return String::new();
        }
    }
}

pub struct HubProxy {
    pub hub_name: Option<String>,
    /// 发送队列句柄：真正的 socket 写由 `peer_writer` 的任务完成（D5）。
    tx: crate::peer_writer::PeerSender,
}

impl HubProxy {
    pub fn new(_wr: Arc<Mutex<Box<dyn NetWriter + Send + 'static>>>) -> HubProxy {
        HubProxy {
            hub_name: None,
            tx: crate::peer_writer::spawn(_wr),
        }
    }

    /// 序列化并入队；返回 `false` 表示没发出去（序列化失败或队列已满）。
    pub fn send_hub_msg(&mut self, msg: HubService) -> bool {
        match net::vec_transport::serialize_framed(|buf| {
            let mut o_prot = TCompactOutputProtocol::new(net::vec_transport::VecWrite(buf));
            match HubService::write_to_out_protocol(&msg, &mut o_prot) {
                Ok(_) => true,
                Err(e) => {
                    error!("Failed to serialize Thrift message in send_hub_msg: {}", e);
                    false
                }
            }
        }) {
            Some(bytes) => self.tx.try_send(bytes),
            None => false,
        }
    }
}
