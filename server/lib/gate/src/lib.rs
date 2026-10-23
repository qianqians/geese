use std::sync::Arc;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tokio::task::JoinHandle;
use tokio::sync::Mutex;
use tokio::time::interval;
use tracing::info;

use tcp::tcp_server::TcpServer;
use wss::wss_server::WSSServer;
use redis_service::redis_service::{RedisService, create_host_cache_key};
use close_handle::CloseHandle;
use health::HealthHandle;
use consul::ConsulImpl;
use time::OffsetTime;

mod client_proxy_manager;
mod hub_proxy_manager;
#[cfg(test)]
mod vec_transport_test;
mod conn_manager;
mod hub_msg_handle;
mod client_msg_handle;
mod entity_manager;

use crate::hub_proxy_manager::HubProxyManager;
use crate::conn_manager::ConnManager;
use crate::hub_msg_handle::GateHubMsgHandle;
use crate::client_msg_handle::GateClientMsgHandle;
use crate::client_proxy_manager::{
    TcpClientProxyManager, 
    WSSClientProxyManager
};

#[derive(Deserialize, Serialize, Debug)]
pub struct WSSCfg {
    client_wss_port: u16,
    client_wss_pfx: String,
    client_wss_password: String
}

impl WSSCfg {
    pub fn new(_port: u16, _pfx: String, _password: String) -> WSSCfg {
        WSSCfg {
            client_wss_port: _port,
            client_wss_pfx: _pfx,
            client_wss_password: _password
        }
    }
}

async fn start_conn_heartbeats_poll(conn_mgr: Arc<Mutex<ConnManager>>) {
    let mut interval = interval(Duration::from_secs(10));
    loop {
        interval.tick().await;

        // 第一步：只持锁"挑出超时的连接"（纯筛选，很快）
        let stale = {
            let mut mgr = conn_mgr.lock().await;
            let now = mgr.get_utc_unix_time_with_offset().await;
            mgr.collect_stale_clients(now).await
        };

        // 第二步：逐个关闭。每次只短暂持锁，中间留给登录/断开/主循环抢锁的机会。
        // （原来是把整段"关连接 + 通知 hub"都放在一把锁里，一个慢 hub 就堵死全服。）
        for id in stale.iter() {
            let mut mgr = conn_mgr.lock().await;
            mgr.close_stale_client(id).await;
        }
    }
}

pub struct GateServer {
    gate_name: String,
    tcp_server: TcpServer,
    client_server: Option<TcpServer>,
    ws_server: Option<WSSServer>,
    wss_server: Option<WSSServer>,
    conn_mgr: Arc<Mutex<ConnManager>>,
    redis: Arc<Mutex<RedisService>>,
    health: Arc<Mutex<HealthHandle>>,
    offset_time: Arc<Mutex<OffsetTime>>,
    heartbeats_join: JoinHandle<()>,
    close: Arc<Mutex<CloseHandle>>
}

impl GateServer {
    pub async fn new(
        gate_name: String,
        gate_hub_host: String,
        redis_url: String,
        client_tcp_host: Option<String>, 
        client_ws_host: Option<String>, 
        client_wss_cfg: Option<WSSCfg>, 
        consul_impl: Arc<Mutex<ConsulImpl>>,
        health_handle: Arc<Mutex<HealthHandle>>) -> Result<GateServer, Box<dyn std::error::Error>> 
    {
        let offset_time = Arc::new(Mutex::new(OffsetTime::new()));

        let _hub_handle = GateHubMsgHandle::new();
        let _client_handle = GateClientMsgHandle::new(offset_time.clone());
        let _close = Arc::new(Mutex::new(CloseHandle::new()));
        
        let _conn_mgr = Arc::new(Mutex::new(ConnManager::new(
            gate_name.clone(), gate_hub_host.clone(), _hub_handle, _client_handle, consul_impl, offset_time.clone(), _close.clone())));
        let _conn_mgr_clone = _conn_mgr.clone();

        let _redis_service = Arc::new(Mutex::new(RedisService::new(
            redis_url, gate_name.clone()).await?));
        {
            let mut _conn_mgr_handle = _conn_mgr.as_ref().lock().await;
            _conn_mgr_handle.set_redis_service(_redis_service.clone());
        }

        let _tcp_s = TcpServer::listen(gate_hub_host.clone(), _close.clone(),
            HubProxyManager::new(_conn_mgr_clone.clone())).await?;
        let _client_s = match client_tcp_host {
            None => None,
            Some(_host) => Some(TcpServer::listen(_host, _close.clone(),
                TcpClientProxyManager::new(_conn_mgr_clone.clone())).await?)
        };
        let _ws_s = match client_ws_host {
            None => None,
            Some(_host) => Some(WSSServer::listen_ws(_host, _close.clone(),
                WSSClientProxyManager::new(_conn_mgr_clone.clone())).await?)
        };
        let _wss_s = match client_wss_cfg {
            None => None,
            Some(_cfg) => {
                let client_wss_host = format!("0.0.0.0:{}", _cfg.client_wss_port);
                Some(WSSServer::listen_wss(
                    client_wss_host, 
                    _cfg.client_wss_pfx,
                    _cfg.client_wss_password,
                    _close.clone(),
                    WSSClientProxyManager::new(_conn_mgr_clone.clone())).await?)
            }
        };

        let _rs = _redis_service.clone();
        {
            let mut _r = _rs.as_ref().lock().await;
            let _ = _r.set(create_host_cache_key(gate_name.clone()), gate_hub_host.clone(), 10, None).await;
        }

        let conn_mgr_clone_for_poll = _conn_mgr_clone.clone();
        let _join = tokio::spawn(async move {
            start_conn_heartbeats_poll(conn_mgr_clone_for_poll).await;
        });

        Ok(GateServer {
            gate_name: gate_name,
            tcp_server: _tcp_s,
            client_server: _client_s,
            ws_server: _ws_s,
            wss_server: _wss_s,
            conn_mgr: _conn_mgr_clone,
            health: health_handle,
            offset_time: offset_time,
            redis: _redis_service,
            heartbeats_join: _join,
            close: _close
        })
    }

    pub async fn get_utc_unix_time_with_offset(&self) -> i64{
        let offset_time_impl = self.offset_time.as_ref().lock().await;
        offset_time_impl.utc_unix_time_with_offset()
    }

    pub async fn run(&mut self) {
        // 计时一律用**单调时钟** Instant：不受 NTP 校时 / 时区切换影响，
        // 并且不用为了取时间反复锁 offset_time（原来一帧要锁 3 次 async mutex）。
        // offset_time 只服务于"游戏内时间"（日志时间戳、业务计时），不参与限帧。
        let mut flush_gate_key_time = std::time::Instant::now();
        loop {
            // 每轮心跳：表示“主循环仍在正常运转”。
            {
                let mut _health = self.health.as_ref().lock().await;
                _health.heartbeat();
            }

            let begin = std::time::Instant::now();
            
            let hub_msg_handle:Option<Arc<Mutex<GateHubMsgHandle>>>;
            let client_msg_handle: Option<Arc<Mutex<GateClientMsgHandle>>>;
            {
                let mut _h = self.conn_mgr.as_ref().lock().await;
                hub_msg_handle = Some(_h.get_hub_msg_handle());
                client_msg_handle = Some(_h.get_client_msg_handle());
            }
            if let Some(_handle) = hub_msg_handle {
                let mut _handle_l = _handle.as_ref().lock().await;
                _handle_l.poll().await;
            }
            if let Some(_handle) = client_msg_handle {
                let mut _handle_l = _handle.as_ref().lock().await;
                _handle_l.poll().await;
            }

            let tick = begin.elapsed().as_millis() as u64;

            let _c_ref = self.close.as_ref().lock().await;
            if _c_ref.is_closed() {
                break;
            }

            if tick < 33 {
                tokio::time::sleep(Duration::from_millis(33 - tick)).await;
            }

            if flush_gate_key_time.elapsed() > Duration::from_secs(10) {
                flush_gate_key_time = std::time::Instant::now();
                let mut _r = self.redis.as_ref().lock().await;
                let _ = _r.expire(self.gate_name.clone(), 10, None).await;
            }
        }
    }

    pub async fn close(&self) {
        info!("start close!");

        let mut _c_handle = self.close.as_ref().lock().await;
        _c_handle.close();
    }

    pub async fn join(mut self) {
        info!("await work done!");

        self.run().await;

        self.tcp_server.join().await;
        if let Some(client_server) = self.client_server {
            let _ = client_server.join().await;
        }
        if let Some(ws_server) = self.ws_server {
            let _ = ws_server.join().await;
        }
        if let Some(wss_server) = self.wss_server {
            let _ = wss_server.join().await;
        }

        let _ = self.heartbeats_join.await;

        info!("work done!");
    }

}