use std::sync::{Arc, Weak};

use tokio::sync::Mutex;
use tracing::{error, info, trace, warn};

use thrift::protocol::{TCompactInputProtocol, TSerializable};
use thrift::transport::TBufferChannel;

use proto::gate::{
    GateClientService, 
    ClientRequestHubLogin,
    ClientRequestHubReconnect,
    ClientRequestHubService,
    ClientCallHubRpc,
    ClientCallHubRsp, 
    ClientCallHubErr,
    ClientCallHubNtf,
    ClientCallGateHeartbeats
};

use proto::hub::{
    HubService,
    ClientRequestService,
    ClientCallRpc,
    ClientCallRsp,
	ClientCallErr,
	ClientCallNtf
};

use proto::client::{
    ClientService,
    GateCallHeartbeats
};

use queue::Queue;
use time::OffsetTime;

use crate::client_proxy_manager::{ClientProxy, request_login, request_reconnect};
use crate::client_proxy_manager::entry_hub_service;
use crate::conn_manager::ConnManager;
use crate::entity_manager::CacheMigrateMsg;

struct ClientEvent {
    proxy: Weak<Mutex<ClientProxy>>,
    ev: GateClientService
}

pub struct GateClientMsgHandle {
    queue: Queue<Box<ClientEvent>>,
    offset_time: Arc<Mutex<OffsetTime>>
}

fn deserialize(data: Vec<u8>) -> Result<GateClientService, Box<dyn std::error::Error>> {
    trace!("deserialize begin!");
    let mut t = TBufferChannel::with_capacity(data.len(), 0);
    let _ = t.set_readable_bytes(&data);
    let mut i_prot = TCompactInputProtocol::new(t);
    let ev_data = GateClientService::read_from_in_protocol(&mut i_prot)?;
    Ok(ev_data)
}

impl GateClientMsgHandle {
    pub fn new(offset_time: Arc<Mutex<OffsetTime>>) -> Arc<Mutex<GateClientMsgHandle>> {
        Arc::new(Mutex::new(GateClientMsgHandle {
            queue: Queue::new(), 
            offset_time: offset_time
        }))
    }

    fn enque_event(&mut self, ev: ClientEvent) {
        self.queue.enque(Box::new(ev))
    }

    pub async fn on_event(&mut self, _proxy: Arc<Mutex<ClientProxy>>, data: Vec<u8>) {
        trace!("do_client_event begin data:{:?}!", data);

        let _ev = match deserialize(data) {
            Err(e) => {
                error!("GateClientMsgHandle do_event err:{}", e);
                return;
            }
            Ok(d) => d
        };

        self.enque_event(ClientEvent {
            proxy: Arc::downgrade(&_proxy.clone()),
            ev: _ev
        });

        trace!("do_client_event end!");
    }

    pub async fn poll(&mut self) {
        loop {
            let opt_ev_data: Option<Box<ClientEvent>>;
            {
                opt_ev_data = self.queue.deque();
            }
            let mut_ev_data = match opt_ev_data {
                None => break,
                Some(ev_data) => ev_data
            };
            trace!("GateClientMsgHandle poll begin!");
            let proxy = mut_ev_data.proxy.clone();
            match mut_ev_data.ev {
                GateClientService::Login(ev) => GateClientMsgHandle::do_client_request_hub_login(proxy, ev).await,
                GateClientService::Reconnect(ev) => GateClientMsgHandle::do_client_request_hub_reconnect(proxy, ev).await,
                GateClientService::RequestHubService(ev) => GateClientMsgHandle::do_client_request_hub_service(proxy, ev).await,
                GateClientService::CallRpc(ev) => GateClientMsgHandle::do_call_hub_rpc(proxy, ev).await,
                GateClientService::CallRsp(ev) => GateClientMsgHandle::do_call_hub_rsp(proxy, ev).await,
                GateClientService::CallErr(ev) => GateClientMsgHandle::do_call_hub_err(proxy, ev).await,
                GateClientService::CallNtf(ev) => GateClientMsgHandle::do_call_hub_ntf(proxy, ev).await,
                GateClientService::Heartbeats(ev) => GateClientMsgHandle::do_call_gate_heartbeats(proxy, self.offset_time.clone(), ev).await
            }
        }
    }

    pub async fn do_client_request_hub_login(_proxy: Weak<Mutex<ClientProxy>>, ev: ClientRequestHubLogin) {
        trace!("do_client_event client_request_hub_login begin!");

        if ev.sdk_uuid.is_none() {
            warn!("do_client_request_hub_login wrong argvs ev.sdk_uuid.is_none");
            return;
        }

        if let Some(_proxy_handle) = _proxy.upgrade() {
            let _conn_mgr: Arc<Mutex<ConnManager>>;
            let _conn_id: String;
            {
                let mut _p = _proxy_handle.as_ref().lock().await;
                _conn_mgr = _p.get_conn_mgr();
                _conn_id = _p.get_conn_id().clone();
            }

            let _hub_proxy = entry_hub_service(_conn_mgr.clone(), "login".to_string()).await;
            if let Some(_hub) = _hub_proxy {
                let mut _conn_mgr_handle = _conn_mgr.as_ref().lock().await;
                trace!("do_client_event client_request_hub_login _conn_mgr_handle lock!");
                let _gate_name = _conn_mgr_handle.get_gate_name();
                let _gate_host = _conn_mgr_handle.get_gate_host();

                if !request_login(_hub.clone(), _gate_name, _gate_host, _conn_id, ev.sdk_uuid.unwrap(), ev.argvs.unwrap()).await {
                    let _hub_handle = _hub.as_ref().lock().await;
                    _conn_mgr_handle.delete_hub_proxy(&_hub_handle.get_hub_name());
                }
            }
        }
        else {
            error!("do_client_request_hub_login ClientProxy is destory!");
        }
    }

    pub async fn do_client_request_hub_reconnect(_proxy: Weak<Mutex<ClientProxy>>, ev: ClientRequestHubReconnect) {
        trace!("do_client_event client_request_hub_reconnect begin!");

        if ev.account_id.is_none() {
            warn!("do_client_request_hub_reconnect wrong argvs ev.account_id.is_none");
            return;
        }

        if ev.argvs.is_none() {
            warn!("do_client_request_hub_reconnect wrong argvs ev.token.is_none");
            return;
        }

        if let Some(_proxy_handle) = _proxy.upgrade() {
            let _conn_mgr: Arc<Mutex<ConnManager>>;
            let _conn_id: String;
            {
                let mut _p = _proxy_handle.as_ref().lock().await;
                _conn_mgr = _p.get_conn_mgr();
                _conn_id = _p.get_conn_id().clone();
            }
            
            let _hub_proxy = entry_hub_service(_conn_mgr.clone(), "login".to_string()).await;
            if let Some(_hub) = _hub_proxy {
                let mut _conn_mgr_handle = _conn_mgr.as_ref().lock().await;
                let _gate_name = _conn_mgr_handle.get_gate_name();
                let _gate_host = _conn_mgr_handle.get_gate_host();

                if !request_reconnect(_hub.clone(), _gate_name, _gate_host, _conn_id, ev.account_id.unwrap(), ev.argvs.unwrap()).await {
                    let _hub_handle = _hub.as_ref().lock().await;
                    _conn_mgr_handle.delete_hub_proxy(&_hub_handle.get_hub_name());
                }
            }
        }
        else {
            error!("do_client_request_hub_reconnect ClientProxy is destory!");
        }
    }

    pub async fn do_client_request_hub_service(_proxy: Weak<Mutex<ClientProxy>>, ev: ClientRequestHubService) {
        trace!("do_client_event client_request_hub_service begin!");
        
        if ev.service_name.is_none() {
            warn!("do_client_request_hub_service wrong argvs ev.service_name.is_none");
            return;
        }

        if let Some(_proxy_handle) = _proxy.upgrade() {
            let _conn_mgr: Arc<Mutex<ConnManager>>;
            let _conn_id: String;
            {
                let mut _p = _proxy_handle.as_ref().lock().await;
                _conn_mgr = _p.get_conn_mgr();
                _conn_id = _p.get_conn_id().clone();
            }
            
            let _hub_proxy = entry_hub_service(_conn_mgr.clone(), ev.service_name.clone().unwrap()).await;
            if let Some(_hub) = _hub_proxy {
                let mut _conn_mgr_handle = _conn_mgr.as_ref().lock().await;
                let _gate_name = _conn_mgr_handle.get_gate_name();
                let _gate_host = _conn_mgr_handle.get_gate_host();

                let mut _hub_handle = _hub.as_ref().lock().await;
                if !_hub_handle.send_hub_msg(HubService::ClientRequestService(
                    ClientRequestService::new(ev.service_name.unwrap(), _gate_name, _gate_host, _conn_id, ev.argvs.unwrap()))).await
                {
                    _conn_mgr_handle.delete_hub_proxy(&_hub_handle.get_hub_name());
                }
            }
        }
        else {
            error!("do_client_request_hub_service ClientProxy is destory!");
        }
    }

    pub async fn do_call_hub_rpc(_proxy: Weak<Mutex<ClientProxy>>, ev: ClientCallHubRpc) {
        trace!("do_client_event call_hub_rpc begin!");
        
        if ev.message.is_none() {
            warn!("do_call_hub_rpc wrong argvs ev.message.is_none");
            return;
        }

        if ev.entity_id.is_none() {
            warn!("do_call_hub_rpc wrong argvs ev.entity_id.is_none");
            return;
        }

        if ev.msg_cb_id.is_none() {
            warn!("do_call_hub_rpc wrong argvs ev.msg_cb_id.is_none");
            return;
        }

        if let Some(_proxy_handle) = _proxy.upgrade() {
            
            let _conn_mgr_arc: Arc<Mutex<ConnManager>>;
            let _conn_id: String;
            {
                let mut _p = _proxy_handle.as_ref().lock().await;
                _conn_mgr_arc = _p.get_conn_mgr();
                _conn_id = _p.get_conn_id().clone();
            }

            let event = ev.message.unwrap();
            let entity_id = ev.entity_id.unwrap();
            let _msg = HubService::ClientCallRpc(ClientCallRpc::new(_conn_id, entity_id.clone(), ev.msg_cb_id.unwrap(), event));
            {
                let mut _conn_mgr = _conn_mgr_arc.as_ref().lock().await;
                if let Some(_entity) = _conn_mgr.get_entity_mut(&entity_id) {
                    if _entity.is_migrate(){
                        _entity.cache_migrate_msg(CacheMigrateMsg{conn_mgr: _conn_mgr_arc.clone(), msg: _msg});
                        return;
                    }
                }
            }

            let mut delete_hub_name: Option<String> = None; 
            {
                let _conn_mgr = _conn_mgr_arc.as_ref().lock().await;
                if let Some(_entity) = _conn_mgr.get_entity(&entity_id) {
                    let hub_name = _entity.get_hub_name();
                    if let Some(_hub_arc) = _conn_mgr.get_hub_proxy(hub_name) {
                        let mut _hub = _hub_arc.as_ref().lock().await;
                        if !_hub.send_hub_msg(_msg).await {
                            delete_hub_name = Some(hub_name.clone());
                        }
                    }
                }
            }
            if let Some(name) = delete_hub_name {
                let mut _conn_mgr = _conn_mgr_arc.as_ref().lock().await;
                _conn_mgr.delete_hub_proxy(&name);
            }
        }
        else {
            error!("do_call_hub_rpc ClientProxy is destory!");
        }
    }
    
    pub async fn do_call_hub_rsp(_proxy: Weak<Mutex<ClientProxy>>, ev: ClientCallHubRsp) {
        trace!("do_client_event call_hub_rsp begin!");
        
        if ev.rsp.is_none() {
            warn!("do_call_hub_rsp wrong argvs ev.rsp.is_none");
            return;
        }

        if let Some(_proxy_handle) = _proxy.upgrade() {

            let _conn_mgr_arc: Arc<Mutex<ConnManager>>;
            {
                let mut _p = _proxy_handle.as_ref().lock().await;
                _conn_mgr_arc = _p.get_conn_mgr();
            }

            if let Some(event) = ev.rsp {
                if event.entity_id.is_none() {
                    warn!("do_call_hub_rsp wrong argvs event.entity_id.is_none");
                    return;
                }

                if event.msg_cb_id.is_none() {
                    warn!("do_call_hub_rsp wrong argvs event.msg_cb_id.is_none");
                    return;
                }

                if event.argvs.is_none() {
                    warn!("do_call_hub_rsp wrong argvs event.argvs.is_none");
                    return;
                }

                let event_tmp_main = event.clone();
                let entity_id = event.entity_id.unwrap();
                let _msg = HubService::ClientCallRsp(ClientCallRsp::new(event_tmp_main));
                {
                    let mut _conn_mgr = _conn_mgr_arc.as_ref().lock().await;
                    if let Some(_entity) = _conn_mgr.get_entity_mut(&entity_id) {
                        if _entity.is_migrate(){
                            _entity.cache_migrate_msg(CacheMigrateMsg{conn_mgr: _conn_mgr_arc.clone(), msg: _msg});
                            return;
                        }
                    }
                }

                let mut delete_hub_name: Option<String> = None; 
                {
                    let mut _conn_mgr = _conn_mgr_arc.as_ref().lock().await;
                    if let Some(_entity) = _conn_mgr.get_entity(&entity_id) {
                        let hub_name = _entity.get_hub_name();
                        if let Some(_hub_arc) = _conn_mgr.get_hub_proxy(hub_name) {
                            let mut _hub = _hub_arc.as_ref().lock().await;
                            if !_hub.send_hub_msg(_msg).await {
                                delete_hub_name = Some(hub_name.clone());
                            }
                        }
                    }
                }
                if let Some(name) = delete_hub_name {
                    let mut _conn_mgr = _conn_mgr_arc.as_ref().lock().await;
                    _conn_mgr.delete_hub_proxy(&name);
                }
            }

        }
        else {
            error!("do_call_hub_rsp ClientProxy is destory!");
        }
    }
    
    pub async fn do_call_hub_err(_proxy: Weak<Mutex<ClientProxy>>, ev: ClientCallHubErr) {
        trace!("do_client_event call_hub_err begin!");
        
        if ev.err.is_none() {
            warn!("do_call_hub_err wrong argvs ev.err.is_none");
            return;
        }

        if let Some(_proxy_handle) = _proxy.upgrade() {

            let _conn_mgr_arc: Arc<Mutex<ConnManager>>;
            {
                let mut _p = _proxy_handle.as_ref().lock().await;
                _conn_mgr_arc = _p.get_conn_mgr();
            }

            if let Some(event) = ev.err {
                if event.entity_id.is_none() {
                    warn!("do_call_hub_err wrong argvs event.entity_id.is_none");
                    return;
                }

                if event.msg_cb_id.is_none() {
                    warn!("do_call_hub_err wrong argvs event.msg_cb_id.is_none");
                    return;
                }

                if event.argvs.is_none() {
                    warn!("do_call_hub_err wrong argvs event.argvs.is_none");
                    return;
                }

                let event_tmp_main = event.clone();
                let entity_id = event.entity_id.unwrap();
                let _msg = HubService::ClientCallErr(ClientCallErr::new(event_tmp_main));
                {
                    let mut _conn_mgr = _conn_mgr_arc.as_ref().lock().await;
                    if let Some(_entity) = _conn_mgr.get_entity_mut(&entity_id) {
                        if _entity.is_migrate(){
                            _entity.cache_migrate_msg(CacheMigrateMsg{conn_mgr: _conn_mgr_arc.clone(), msg: _msg});
                            return;
                        }
                    }
                }

                let mut delete_hub_name: Option<String> = None; 
                {
                    let mut _conn_mgr = _conn_mgr_arc.as_ref().lock().await;
                    if let Some(_entity) = _conn_mgr.get_entity(&entity_id) {
                        let hub_name = _entity.get_hub_name();
                        if let Some(_hub_arc) = _conn_mgr.get_hub_proxy(hub_name) {
                            let mut _hub = _hub_arc.as_ref().lock().await;
                            if !_hub.send_hub_msg(_msg).await {
                                delete_hub_name = Some(hub_name.clone());
                            }
                        }
                    }
                }
                if let Some(name) = delete_hub_name {
                    let mut _conn_mgr = _conn_mgr_arc.as_ref().lock().await;
                    _conn_mgr.delete_hub_proxy(&name);
                }
            }
        }
        else {
            error!("do_call_hub_err ClientProxy is destory!");
        }
    }

    pub async fn do_call_hub_ntf(_proxy: Weak<Mutex<ClientProxy>>, ev: ClientCallHubNtf) {
        trace!("do_client_event call_hub_ntf begin!");
        
        if ev.entity_id.is_none() {
            warn!("do_call_hub_ntf wrong argvs ev.entity_id.is_none");
            return;
        }

        if ev.message.is_none() {
            warn!("do_call_hub_ntf wrong argvs ev.message.is_none");
            return;
        }

        if let Some(_proxy_handle) = _proxy.upgrade() {
            let _conn_mgr_arc: Arc<Mutex<ConnManager>>;
            let conn_id: String;
            {
                let mut _p = _proxy_handle.as_ref().lock().await;
                _conn_mgr_arc = _p.get_conn_mgr();
                conn_id = _p.get_conn_id().clone();
            }

            let event = ev.message.unwrap();
            let entity_id = ev.entity_id.unwrap();
            let _msg = HubService::ClientCallNtf(ClientCallNtf::new(entity_id.clone(), event));
            {
                let mut _conn_mgr = _conn_mgr_arc.as_ref().lock().await;
                if let Some(_entity) = _conn_mgr.get_entity_mut(&entity_id) {
                    if _entity.is_migrate(){
                        _entity.cache_migrate_msg(CacheMigrateMsg{conn_mgr: _conn_mgr_arc.clone(), msg: _msg});
                        return;
                    }
                }
            }

            let mut delete_hub_name: Option<String> = None; 
            {
                let mut _conn_mgr = _conn_mgr_arc.as_ref().lock().await;
                if let Some(_entity) = _conn_mgr.get_entity(&entity_id) {
                    let main_conn_id = _entity.get_main_conn_id();
                    if main_conn_id.is_some() && conn_id != main_conn_id.unwrap() {
                        error!("client ntf rpc to hub need main entity or global entity!");
                        return;
                    }

                    let hub_name = _entity.get_hub_name();
                    if let Some(_hub_arc) = _conn_mgr.get_hub_proxy(hub_name) {
                        let mut _hub = _hub_arc.as_ref().lock().await;
                        if !_hub.send_hub_msg(_msg).await {
                            delete_hub_name = Some(hub_name.clone());
                        }
                    }
                }
            }
            if let Some(name) = delete_hub_name {
                let mut _conn_mgr = _conn_mgr_arc.as_ref().lock().await;
                _conn_mgr.delete_hub_proxy(&name);
            }
        }
        else {
            error!("do_call_hub_ntf ClientProxy is destory!");
        }
    }

    pub async fn do_call_gate_heartbeats(_proxy: Weak<Mutex<ClientProxy>>, offset_time: Arc<Mutex<OffsetTime>>, _: ClientCallGateHeartbeats) {
        trace!("do_client_event call_gate_heartbeats begin!");

        if let Some(_proxy_handle) = _proxy.upgrade() {
            let mut _client = _proxy_handle.as_ref().lock().await;
            let _offset_time = offset_time.as_ref().lock().await;
            let _utc_unix_time = _offset_time.utc_unix_time_with_offset();
            _client.set_timetmp(_utc_unix_time);
            if !_client.send_client_msg(ClientService::Heartbeats(GateCallHeartbeats::new(_utc_unix_time))).await {
                let _conn_mgr_arc = _client.get_conn_mgr();
                let mut _conn_mgr_tmp = _conn_mgr_arc.as_ref().lock().await;
                _conn_mgr_tmp.delete_client_proxy(_client.get_conn_id());

                info!("do_call_gate_heartbeats delete_client_proxy!");
            }
        }
        else {
            error!("do_call_gate_heartbeats ClientProxy is destory!");
        }
    }
    
}