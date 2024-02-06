use std::collections::{BTreeSet, BTreeMap};
use std::sync::Arc;

use rand::Rng;
use tokio::task::JoinHandle;
use tokio::sync::Mutex;
use uuid::Uuid;
use async_trait::async_trait;

use thrift::protocol::{TCompactOutputProtocol, TSerializable};
use thrift::transport::{TIoChannel, TBufferChannel};

use net::{NetReaderCallback, NetReader, NetWriter};
use tcp::tcp_socket::{TcpReader, TcpWriter};
use tcp::tcp_connect::TcpConnect;
use tcp::tcp_server::TcpListenCallback;
use wss::wss_socket::{WSSReader, WSSWriter};
use wss::wss_server::WSSListenCallback;
use redis_service::redis_service::create_lock_key;
use close_handle::CloseHandle;
use time::utc_unix_time;
use tracing::{trace, info, error};

use crate::conn_manager::ConnManager;
use crate::client_msg_handle::GateClientMsgHandle;
use crate::hub_proxy_manager::{HubProxy, HubReaderCallback};

use proto::hub::{
    HubService,
    ClientRequestLogin,
    ClientRequestReconnect,
    ClientDisconnnect,
    TransferMsgEnd
};

use proto::client::{
    ClientService,
    NtfConnId
};

pub async fn request_login(
    _hubproxy: Arc<Mutex<HubProxy>>,
    _gate_name: String,
    _gate_host: String,
    _conn_id: String,
    _sdk_uuid: String) -> bool
{
    trace!("request_login begin!");

    let mut _hub = _hubproxy.as_ref().lock().await;
    trace!("request_login _hubproxy lock!");
    _hub.send_hub_msg(HubService::ClientRequestLogin(
        ClientRequestLogin::new(_gate_name, _gate_host, _conn_id, _sdk_uuid))).await

}

pub async fn request_reconnect(
    _hubproxy: Arc<Mutex<HubProxy>>,
    _gate_name: String,
    _gate_host: String,
    _conn_id: String,
    _account_id: String,
    _token: String) -> bool
{
    let mut _hub = _hubproxy.as_ref().lock().await;
    _hub.send_hub_msg(HubService::ClientRequestReconnect(
        ClientRequestReconnect::new(_gate_name, _gate_host, _conn_id, _account_id, _token))).await
}

pub async fn entry_hub_service(_conn_mgr: Arc<Mutex<ConnManager>>, _service_name:String) -> Option<Arc<Mutex<HubProxy>>>
{
    trace!("entry_hub_service begin!");

    let mut _conn_mgr_handle = _conn_mgr.as_ref().lock().await;
    let _close = _conn_mgr_handle.get_close_handle();
    let _gate_name = _conn_mgr_handle.get_gate_name();
    let _entry_service = _service_name.clone();

    let _consul_impl = _conn_mgr_handle.get_consul_impl(); 
    let mut _impl = _consul_impl.as_ref().lock().await;
    let mut services = match _impl.services(_entry_service.clone()).await {
        None => {
            info!("consul services _entry_service:{} None!", _entry_service.clone());
            return None
        }
        Some(s) => {
            if s.len() <= 0 {
                info!("consul services _entry_service:{} empty!", _entry_service.clone());
                return None
            }
            s
        }
    };
    let mut rng = rand::thread_rng();
    loop {
        let index = rng.gen_range(0..services.len());
        let service = match services.get(index) {
            None => {
                info!("consul services index:{} None!", index);
                return None
            },
            Some(s) => s
        };
        if let Some(_hubproxy) = _conn_mgr_handle.get_hub_proxy(&service.id) {
            return Some(_hubproxy.clone())
        }
        else {
            let _redis_service = _conn_mgr_handle.get_redis_service();
            let mut _service = _redis_service.as_ref().lock().await;
            let lock_key = create_lock_key(_conn_mgr_handle.get_gate_name(), service.id.clone());
            if let Ok(value) = _service.acquire_lock(lock_key.clone(), 3).await {
                if let Some(_hubproxy) = _conn_mgr_handle.get_hub_proxy(&service.id) {
                    let _ = _service.release_lock(lock_key.clone(), value.clone()).await;
                    return Some(_hubproxy.clone())
                }

                _conn_mgr_handle.add_lock(lock_key, value);

                if let Ok((rd, wr)) = TcpConnect::connect(format!("{}:{}", service.addr, service.port)).await {
                    let _wr_arc: Arc<Mutex<Box<dyn NetWriter + Send + 'static>>> = Arc::new(Mutex::new(Box::new(wr)));
                    let _conn_mgr_clone = _conn_mgr.clone();
                    let _hubproxy = Arc::new(Mutex::new(HubProxy::new(
                        _wr_arc, _conn_mgr_clone)));
                    let _ = rd.start(
                        Arc::new(Mutex::new(Box::new(
                            HubReaderCallback::new(_hubproxy.clone(), _conn_mgr_handle.get_hub_msg_handle())))),
                        _close.clone());
                    _conn_mgr_handle.add_hub_proxy(service.id.to_string(), _hubproxy.clone()).await;
                    let _h_clone = _hubproxy.clone();
                    let mut _h = _hubproxy.as_ref().lock().await;
                    _h.name = Some(service.id.to_string());
                    return Some(_h_clone)
                }
                else {
                    error!("entry_hub_service tcp connect faild!");
                }
            }
        }
        services.remove(index);
        if services.len() <= 0 {
            error!("entry_hub_service faild:login!");
            return None
        }
    }
}

pub struct ClientProxy {
    pub conn_id: String,
    pub wr: Arc<Mutex<Box<dyn NetWriter + Send + 'static>>>,
    pub hub_proxies: BTreeMap<String, Arc<Mutex<HubProxy>>>,
    pub join: Option<JoinHandle<()>>,
    pub last_heartbeats_timetmp: i64,
    pub entities: BTreeSet<String>,
    originate_kick_off_hub: Option<Arc<Mutex<HubProxy>>>,
    conn_mgr: Arc<Mutex<ConnManager>>
}

impl ClientProxy {
    pub fn new(_conn_id: String, _wr: Arc<Mutex<Box<dyn NetWriter + Send + 'static>>>, _conn_mgr: Arc<Mutex<ConnManager>>) -> ClientProxy {
        ClientProxy {
            conn_id: _conn_id,
            wr: _wr,
            hub_proxies: BTreeMap::new(),
            join: None,
            entities: BTreeSet::new(),
            originate_kick_off_hub: None,
            conn_mgr: _conn_mgr,
            last_heartbeats_timetmp: utc_unix_time()
        }
    }

    pub fn set_join(&mut self, j: JoinHandle<()>) {
        self.join = Some(j)
    }

    pub fn get_conn_id(&self) -> &String {
        &self.conn_id
    }

    pub fn get_conn_mgr(&mut self) -> Arc<Mutex<ConnManager>> {
        self.conn_mgr.clone()
    }

    pub async fn ntf_client_offline(&mut self, _proxy: Arc<Mutex<HubProxy>>) {
        for (_, _hub_proxy) in &self.hub_proxies {
            let mut _hub = _hub_proxy.as_ref().lock().await;
            _hub.send_hub_msg(HubService::ClientDisconnnect(ClientDisconnnect::new(self.conn_id.clone()))).await;
        }
        self.originate_kick_off_hub = Some(_proxy);

        self.check_all_hub_kick_off().await;
    }

    async fn check_all_hub_kick_off(&mut self) {
        if self.hub_proxies.len() <= 0 {
            if let Some(_proxy) = &self.originate_kick_off_hub {
                {
                    let mut _conn_mgr = self.conn_mgr.as_ref().lock().await;
                    let tmp_conn_id = self.conn_id.clone();
                    _conn_mgr.close_client(&tmp_conn_id).await;
                }
                let mut _p = _proxy.as_ref().lock().await;
                _p.send_hub_msg(HubService::TransferMsgEnd(TransferMsgEnd::new(self.conn_id.clone(), false))).await;
            }
        }
    }

    pub async fn check_hub_kick_off(&mut self, hub_name:String) {
        self.hub_proxies.remove(&hub_name);
        self.check_all_hub_kick_off().await;
    }

    pub async fn send_client_msg(&mut self, msg: ClientService) -> bool {
        let t = TBufferChannel::with_capacity(0, 16384);
        let (rd, wr) = match t.split() {
            Ok(_t) => (_t.0, _t.1),
            Err(_e) => {
                error!("do_get_guid t.split error {}", _e);
                return false;
            }
        };
        let mut o_prot = TCompactOutputProtocol::new(wr);
        let _ = ClientService::write_to_out_protocol(&msg, &mut o_prot);
        let mut p_send = self.wr.as_ref().lock().await;
        p_send.send(&rd.write_bytes()).await
    }

    pub fn set_timetmp(&mut self, timetmp: i64) {
        self.last_heartbeats_timetmp = timetmp
    }
}

pub struct ClientReaderCallback {
    clientproxy: Arc<Mutex<ClientProxy>>,
    client_msg_handle: Arc<Mutex<GateClientMsgHandle>>,
}

#[async_trait]
impl NetReaderCallback for ClientReaderCallback {
    async fn cb(&mut self, data:Vec<u8>) {
        trace!("ClientReaderCallback NetReaderCallback cb!");
        let mut _handle = self.client_msg_handle.as_ref().lock().await;
        trace!("ClientReaderCallback cb client_msg_handle get lock!");
        _handle.on_event(self.clientproxy.clone(), data).await;
        trace!("ClientReaderCallback cb client_msg_handle on_event!");
    }
}

impl ClientReaderCallback {
    pub fn new(_clientproxy: Arc<Mutex<ClientProxy>>, _client_msg_handle: Arc<Mutex<GateClientMsgHandle>>) -> ClientReaderCallback {
        ClientReaderCallback {
            clientproxy: _clientproxy,
            client_msg_handle: _client_msg_handle,
        }
    }
}

pub struct TcpClientProxyManager {
    conn_mgr: Arc<Mutex<ConnManager>>,
    close_handle: Arc<Mutex<CloseHandle>>
}

#[async_trait]
impl TcpListenCallback for TcpClientProxyManager {
    async fn cb(&mut self, rd: TcpReader, wr: TcpWriter){
        trace!("tcp listen callback!");

        let _wr_arc: Arc<Mutex<Box<dyn NetWriter + Send + 'static>>> = Arc::new(Mutex::new(Box::new(wr)));
        let _conn_id = Uuid::new_v4().to_string();
        
        let _conn_mgr_clone = self.conn_mgr.clone();
        let _clientproxy = Arc::new(Mutex::new(ClientProxy::new(_conn_id.clone(), _wr_arc.clone(), _conn_mgr_clone)));
        let _clientproxy_clone = _clientproxy.clone();
        let _client_msg_handle: Arc<Mutex<GateClientMsgHandle>>;
        {
            trace!("tcp listen _conn_mgr lock begin!");
            let mut _conn_mgr = self.conn_mgr.as_ref().lock().await;
            _conn_mgr.add_client_proxy(_clientproxy_clone.clone()).await;
            _client_msg_handle = _conn_mgr.get_client_msg_handle();
            trace!("tcp listen _conn_mgr lock end!");
        }

        let join = rd.start(Arc::new(Mutex::new(Box::new(ClientReaderCallback::new(_clientproxy_clone.clone(), _client_msg_handle)))), self.close_handle.clone());
        {
            trace!("TcpListenCallback cb _clientproxy lock begin!");
            let mut _client_tmp = _clientproxy.as_ref().lock().await;
            trace!("TcpListenCallback cb _clientproxy lock success!");
            _client_tmp.set_join(join);
            _client_tmp.send_client_msg(ClientService::ConnId(NtfConnId::new(_conn_id.clone()))).await;
        }
    }
}

impl TcpClientProxyManager {
    pub fn new(_conn_mgr: Arc<Mutex<ConnManager>>, _close: Arc<Mutex<CloseHandle>>) -> Arc<Mutex<Box<dyn TcpListenCallback + Send + 'static>>> {
        Arc::new(Mutex::new(Box::new(TcpClientProxyManager {
            conn_mgr: _conn_mgr,
            close_handle: _close
        })))
    }
}

pub struct WSSClientProxyManager {
    conn_mgr: Arc<Mutex<ConnManager>>,
    close_handle: Arc<Mutex<CloseHandle>>
}

#[async_trait]
impl WSSListenCallback for WSSClientProxyManager {
    async fn cb(&mut self, rd: WSSReader, wr: WSSWriter){
        trace!("wss listen callback!");

        let _wr_arc: Arc<Mutex<Box<dyn NetWriter + Send + 'static>>> = Arc::new(Mutex::new(Box::new(wr)));
        let _conn_id = Uuid::new_v4().to_string();
        
        let _conn_mgr_clone = self.conn_mgr.clone();
        let _clientproxy = Arc::new(Mutex::new(ClientProxy::new(_conn_id.clone(), _wr_arc.clone(), _conn_mgr_clone)));
        let _clientproxy_clone = _clientproxy.clone();
        let _client_msg_handle: Arc<Mutex<GateClientMsgHandle>>;
        {
            trace!("wss listen _conn_mgr lock begin!");
            let mut _conn_mgr = self.conn_mgr.as_ref().lock().await;
            _conn_mgr.add_client_proxy(_clientproxy_clone.clone()).await;
            _client_msg_handle = _conn_mgr.get_client_msg_handle();
            trace!("wss listen _conn_mgr lock end!");
        }
        
        let join = rd.start(Arc::new(Mutex::new(Box::new(ClientReaderCallback::new(_clientproxy_clone.clone(), _client_msg_handle)))), self.close_handle.clone());
        {
            trace!("WSSListenCallback cb _clientproxy lock begin!");
            let mut _client_tmp = _clientproxy.as_ref().lock().await;
            trace!("WSSListenCallback cb _clientproxy lock success!");
            _client_tmp.set_join(join);
            _client_tmp.send_client_msg(ClientService::ConnId(NtfConnId::new(_conn_id.clone()))).await;
        }
    }
}

impl WSSClientProxyManager {
    pub fn new(_conn_mgr: Arc<Mutex<ConnManager>>, _close: Arc<Mutex<CloseHandle>>) -> Arc<Mutex<Box<dyn WSSListenCallback + Send + 'static>>> {
        Arc::new(Mutex::new(Box::new(WSSClientProxyManager {
            conn_mgr: _conn_mgr,
            close_handle: _close
        })))
    }
}