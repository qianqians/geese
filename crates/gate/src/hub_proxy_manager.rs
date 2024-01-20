use std::sync::Arc;

use tokio::sync::Mutex;
use tracing::error;

use thrift::protocol::{TCompactOutputProtocol, TSerializable};
use thrift::transport::{TIoChannel, TBufferChannel};
use async_trait::async_trait;

use proto::hub::HubService;

use net::{NetReaderCallback, NetReader, NetWriter};
use tcp::tcp_socket::{TcpReader, TcpWriter};
use tcp::tcp_server::TcpListenCallback;
use close_handle::CloseHandle;

use crate::conn_manager::ConnManager;
use crate::hub_msg_handle::GateHubMsgHandle;

pub struct HubProxy {
    pub name: Option<String>,
    wr: Arc<Mutex<Box<dyn NetWriter + Send + 'static>>>,
    conn_mgr: Arc<Mutex<ConnManager>>
}

impl HubProxy {
    pub fn new(_wr: Arc<Mutex<Box<dyn NetWriter + Send + 'static>>>, _conn_mgr: Arc<Mutex<ConnManager>>) -> HubProxy {
        HubProxy {
            name: None,
            wr: _wr,
            conn_mgr: _conn_mgr
        }
    }

    pub async fn set_hub_info(p: Arc<Mutex<HubProxy>>, name: String) {
        let _p_clone = p.clone();
        let _name_clone = name.clone();
        let mut _p = p.as_ref().clone().lock().await;

        let _name = name.clone();
        _p.name = Some(name);

        let mut _conn_mgr = _p.conn_mgr.as_ref().lock().await;
        _conn_mgr.add_hub_proxy(_name, _p_clone).await;
    }

    pub fn get_hub_name(&self) -> String {
        self.name.as_ref().unwrap().clone()
    }

    pub async fn get_msg_handle(&mut self) -> Arc<Mutex<GateHubMsgHandle>> {
        let _conn_mgr = self.conn_mgr.as_ref().lock().await;
        _conn_mgr.get_hub_msg_handle()
    }

    pub fn get_conn_mgr(&mut self) -> Arc<Mutex<ConnManager>> {
        self.conn_mgr.clone()
    }

    pub async fn send_hub_msg(&mut self, msg: HubService) -> bool {
        let t = TBufferChannel::with_capacity(0, 16384);
        let (rd, wr) = match t.split() {
            Ok(_t) => (_t.0, _t.1),
            Err(_e) => {
                error!("do_get_guid t.split error {}", _e);
                return false;
            }
        };
        let mut o_prot = TCompactOutputProtocol::new(wr);
        let _ = HubService::write_to_out_protocol(&msg, &mut o_prot);
        let mut p_send = self.wr.as_ref().lock().await;
        p_send.send(&rd.write_bytes()).await
    }
}

pub struct DelayHubMsg {
    pub hubproxy: Arc<Mutex<HubProxy>>,
    pub ev: HubService
}

impl DelayHubMsg {
    pub fn new(proxy: Arc<Mutex<HubProxy>>, _ev: HubService) -> DelayHubMsg {
        DelayHubMsg {
            hubproxy: proxy,
            ev: _ev
        }
    }
}

pub struct HubReaderCallback {
    hubproxy: Arc<Mutex<HubProxy>>
}

#[async_trait]
impl NetReaderCallback for HubReaderCallback {
    async fn cb(&mut self, data:Vec<u8>) {
        GateHubMsgHandle::on_event(self.hubproxy.clone(), data).await
    }
}

impl HubReaderCallback {
    pub fn new(_hubproxy: Arc<Mutex<HubProxy>>) -> HubReaderCallback {
        HubReaderCallback {
            hubproxy: _hubproxy
        }
    }

}

pub struct HubProxyManager {
    conn_mgr: Arc<Mutex<ConnManager>>,
    close_handle: Arc<Mutex<CloseHandle>>
}

#[async_trait]
impl TcpListenCallback for HubProxyManager {
    async fn cb(&mut self, rd: TcpReader, wr: TcpWriter){
        let _wr_arc: Arc<Mutex<Box<dyn NetWriter + Send + 'static>>> = Arc::new(Mutex::new(Box::new(wr)));
        let _wr_arc_clone = _wr_arc.clone();
        let _conn_mgr_clone = self.conn_mgr.clone();
        let _hubproxy = Arc::new(Mutex::new(HubProxy::new(_wr_arc, self.conn_mgr.clone())));
        let _hub = _hubproxy.clone();
        let _hub_clone = _hubproxy.clone();
        let _ = rd.start(Arc::new(Mutex::new(Box::new(HubReaderCallback::new(_hub_clone)))), self.close_handle.clone());
    }
}

impl HubProxyManager {
    pub fn new(_conn_mgr: Arc<Mutex<ConnManager>>, _close: Arc<Mutex<CloseHandle>>) -> Arc<Mutex<Box<dyn TcpListenCallback + Send + 'static>>> {
        Arc::new(Mutex::new(Box::new(HubProxyManager {
            conn_mgr: _conn_mgr,
            close_handle: _close
        })))
    }
}