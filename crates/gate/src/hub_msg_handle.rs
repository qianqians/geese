use std::sync::{Arc, Weak};

use tokio::sync::Mutex;
use tracing::{trace, info, error};

use thrift::protocol::{TCompactInputProtocol, TSerializable};
use thrift::transport::TBufferChannel;

use redis_service::redis_service::create_lock_key;

use proto::common::{RegServer, RegServerCallback};

use proto::gate::{
    GateHubService, 
    HubCallClientCreateRemoteEntity, 
    HubCallClientDeleteRemoteEntity,
    HubCallClientRefreshEntity,
    HubCallClientRpc, 
    HubCallClientRsp, 
    HubCallClientErr, 
    HubCallClientNtf,
    HubCallClientGlobal,
    HubCallKickOffClient,
    HubCallTransferClientComplete,
    HubCallKickOffClientComplete
};

use proto::client::{
    ClientService,
    CreateRemoteEntity,
    DeleteRemoteEntity,
    RefreshEntity,
    TransferComplete,
    KickOff,
    CallRpc,
    CallRsp,
    CallErr,
    CallNtf,
    CallGlobal
};

use proto::hub::{
    HubService,
    TransferMsgEnd,
    TransferEntityControl
};

use queue::Queue;

use crate::entity_manager::Entity;
use crate::conn_manager::ConnManager;
use crate::hub_proxy_manager::HubProxy;

struct HubEvent {
    proxy: Weak<Mutex<HubProxy>>,
    ev: GateHubService
}

pub struct GateHubMsgHandle {
    queue: Queue<Box<HubEvent>>
}

fn deserialize(data: Vec<u8>) -> Result<GateHubService, Box<dyn std::error::Error>> {
    trace!("deserialize begin!");
    let mut t = TBufferChannel::with_capacity(data.len(), 0);
    let _ = t.set_readable_bytes(&data);
    let mut i_prot = TCompactInputProtocol::new(t);
    let ev_data = GateHubService::read_from_in_protocol(&mut i_prot)?;
    Ok(ev_data)
}

async fn get_mut_entity(_conn_mgr_arc: Arc<Mutex<ConnManager>>, entity_id: String) -> Option<Entity> {
    let mut _conn_mgr = _conn_mgr_arc.as_ref().lock().await;
    let mut _entity = _conn_mgr.get_entity_mut(&entity_id);
    return _entity.cloned()
}

impl GateHubMsgHandle {
    pub fn new() -> Arc<Mutex<GateHubMsgHandle>> {
        Arc::new(Mutex::new(GateHubMsgHandle {
            queue: Queue::new(), 
        }))
    }

    fn enque_event(&mut self, ev: HubEvent) {
        self.queue.enque(Box::new(ev))
    }

    pub async fn on_event(&mut self, _proxy: Arc<Mutex<HubProxy>>, data: Vec<u8>) {
        trace!("do_hub_event begin!");

        let _ev: GateHubService;
        {
            _ev = match deserialize(data) {
                Err(e) => {
                    error!("GateHubMsgHandle do_event err:{}", e);
                    return;
                }
                Ok(d) => d
            };
        }

        self.enque_event(HubEvent {
            proxy: Arc::downgrade(&_proxy.clone()),
            ev: _ev
        });

        trace!("do_hub_event end!");
    }

    pub async fn poll(_handle: Arc<Mutex<GateHubMsgHandle>>) {
        loop {
            let mut_ev_data: Box<HubEvent>;
            {
                let mut _self = _handle.as_ref().lock().await;
                let opt_ev_data = _self.queue.deque();
                mut_ev_data = match opt_ev_data {
                    None => break,
                    Some(ev_data) => ev_data
                };
            }
            trace!("GateHubMsgHandle poll begin!");
            let proxy = mut_ev_data.proxy.clone();
            match mut_ev_data.ev {
                GateHubService::RegServer(ev) => GateHubMsgHandle::do_reg_hub(proxy, ev).await,
                GateHubService::RegServerCallback(ev) => GateHubMsgHandle::do_reg_server_callback(proxy, ev).await,
                GateHubService::CreateRemoteEntity(ev) => GateHubMsgHandle::do_create_remote_entity(proxy, ev).await,
                GateHubService::DeleteRemoteEntity(ev) => GateHubMsgHandle::do_delete_remote_entity(proxy, ev).await,
                GateHubService::RefreshEntity(ev) => GateHubMsgHandle::do_refresh_entity(proxy, ev).await,
                GateHubService::CallRpc(ev) => GateHubMsgHandle::do_call_client_rpc(proxy, ev).await,
                GateHubService::CallRsp(ev) => GateHubMsgHandle::do_call_client_rsp(proxy, ev).await,
                GateHubService::CallErr(ev) => GateHubMsgHandle::do_call_client_err(proxy, ev).await,
                GateHubService::CallNtf(ev) => GateHubMsgHandle::do_call_client_ntf(proxy, ev).await,
                GateHubService::CallGlobal(ev) => GateHubMsgHandle::do_call_client_global(proxy, ev).await,
                GateHubService::KickOff(ev) => GateHubMsgHandle::do_kick_off_client(proxy, ev).await,
                GateHubService::TransferComplete(ev) => GateHubMsgHandle::do_transfer_client_complete(proxy, ev).await,
                GateHubService::KickOffComplete(ev) => GateHubMsgHandle::do_kick_off_client_complete(proxy, ev).await
            }
        }
    }

    pub async fn do_reg_hub(_proxy: Weak<Mutex<HubProxy>>, ev: RegServer) {
        trace!("do_hub_event reg_hub begin!");

        if let Some(_proxy_handle) = _proxy.upgrade() {
            let name = ev.name.unwrap();
            HubProxy::set_hub_info(_proxy_handle.clone(), name).await;

            let _conn_mgr_arc: Arc<Mutex<ConnManager>>;
            {
                let mut _hub = _proxy_handle.as_ref().lock().await;
                _conn_mgr_arc = _hub.get_conn_mgr();
            }

            let gate_name: String;
            {
                let _conn_mgr = _conn_mgr_arc.as_ref().lock().await;
                gate_name = _conn_mgr.get_gate_name();
            }

            let cb_msg = RegServerCallback::new(gate_name);
            let msg = HubService::RegServerCallback(cb_msg);
            {
                let mut _hub = _proxy_handle.as_ref().lock().await;
                _hub.send_hub_msg(msg).await;
            }
        }
        else {
            error!("do_reg_hub HubProxy is destory!");
        }

        trace!("do_hub_event reg_hub end!");
    }

    pub async fn do_reg_server_callback(_proxy: Weak<Mutex<HubProxy>>, ev: RegServerCallback) {
        trace!("do_hub_event do_reg_server_callback begin!");

        if let Some(_proxy_handle) = _proxy.upgrade() {
            let name = ev.name.unwrap();

            let _conn_mgr_arc: Arc<Mutex<ConnManager>>;
            {
                let mut _hub = _proxy_handle.as_ref().lock().await;
                _conn_mgr_arc = _hub.get_conn_mgr();
            }

            let mut _conn_mgr = _conn_mgr_arc.as_ref().lock().await;
            let lock_key = create_lock_key(name, _conn_mgr.get_gate_name());
            if let Some(value) = _conn_mgr.remove_lock(lock_key.clone()) {
                let _service = _conn_mgr.get_redis_service();
                let mut _s = _service.as_ref().lock().await;
                let _ = _s.release_lock(lock_key, value).await;
            }
        }
        else {
            error!("do_reg_hub HubProxy is destory!");
        }

        trace!("do_hub_event do_reg_server_callback end!");
    }

    pub async fn do_create_remote_entity(_proxy: Weak<Mutex<HubProxy>>, ev: HubCallClientCreateRemoteEntity) {
        trace!("do_hub_event create_remote_entity begin!");

        if let Some(_proxy_handle) = _proxy.upgrade() {
            let _proxy_clone = _proxy_handle.clone();

            let _conn_mgr_arc: Arc<Mutex<ConnManager>>;
            let _source_hub_name: String;
            {
                let mut _p = _proxy_handle.as_ref().lock().await;
                _conn_mgr_arc = _p.get_conn_mgr();
                _source_hub_name = _p.get_hub_name();
            }

            let entity_id = ev.entity_id.unwrap();
            let entity_type = ev.entity_type.unwrap();
            let argvs = ev.argvs.unwrap();

            let mut main_send_ret = false;
            if let Some(main_conn_id) = ev.main_conn_id.clone() {
                let mut _conn_mgr = _conn_mgr_arc.as_ref().lock().await;
                if let Some(_client_arc) = _conn_mgr.get_client_proxy(&main_conn_id.clone()) {
                    let mut _client = _client_arc.as_ref().lock().await;
                    if _client.send_client_msg(
                        ClientService::CreateRemoteEntity(
                            CreateRemoteEntity::new(entity_id.clone(), entity_type.clone(), true, argvs.clone()))).await 
                    {
                        main_send_ret = true;
                        _client.entities.insert(entity_id.clone());
                    }
                }
            }
            if main_send_ret {
                let mut _conn_mgr = _conn_mgr_arc.as_ref().lock().await;
                let _entity = match _conn_mgr.get_entity_mut(&entity_id) {
                    None => {
                        let e = Entity::new(entity_id.clone(), _source_hub_name.clone());
                        _conn_mgr.update_entity(e);
                        _conn_mgr.get_entity_mut(&entity_id.clone()).unwrap()
                    }
                    Some(e) => e
                };
                if let Some(main_conn_id) = ev.main_conn_id.clone() {
                    _entity.set_main_conn_id(Some(main_conn_id.clone()));
                    if let Some(_client_arc) = _conn_mgr.get_client_proxy(&main_conn_id.clone()) {
                        let mut _client = _client_arc.as_ref().lock().await;
                        _client.hub_proxies.insert(_source_hub_name.clone(), _proxy_clone.clone());
                    }
                }
            }
            else {
                if let Some(main_conn_id) = ev.main_conn_id.clone() {
                    let mut _conn_mgr = _conn_mgr_arc.as_ref().lock().await;
                    _conn_mgr.delete_client_proxy(&main_conn_id);
                    
                    info!("do_create_remote_entity main_conn_id delete_client_proxy!");
                }
            }

            if let Some(conn_dis) = ev.conn_id {
                let mut _conn_mgr = _conn_mgr_arc.as_ref().lock().await;
                for id in conn_dis.iter() {
                    let mut send_ret = false;
                    {
                        if let Some(_client_arc) = _conn_mgr.get_client_proxy(id) {
                            let mut _client = _client_arc.as_ref().lock().await;
                            if _client.send_client_msg(ClientService::CreateRemoteEntity(CreateRemoteEntity::new(entity_id.clone(), entity_type.clone(), false, argvs.clone()))).await {
                                send_ret = true;
                                _client.entities.insert(entity_id.clone());
                            }
                        }
                    }
                    if send_ret {
                        let _entity = match _conn_mgr.get_entity_mut(&entity_id) {
                            None => {
                                let e = Entity::new(entity_id.clone(), _source_hub_name.clone());
                                _conn_mgr.update_entity(e);
                                _conn_mgr.get_entity_mut(&entity_id.clone()).unwrap()
                            }
                            Some(e) => e
                        };
                        _entity.add_conn_id(id.to_string());
                        if let Some(_client_arc) = _conn_mgr.get_client_proxy(id) {
                            let mut _client = _client_arc.as_ref().lock().await;
                            _client.hub_proxies.insert(_source_hub_name.clone(), _proxy_clone.clone());
                        }
                    }
                    else {
                        _conn_mgr.delete_client_proxy(id);
                        
                        info!("do_create_remote_entity conn_dis delete_client_proxy!");
                    }
                }
            }
        }
        else {
            error!("do_create_remote_entity HubProxy is destory!");
        }

        trace!("do_hub_event create_remote_entity end!");
    }

    pub async fn do_delete_remote_entity(_proxy: Weak<Mutex<HubProxy>>, ev: HubCallClientDeleteRemoteEntity) {
        trace!("do_hub_event delete_remote_entity begin!");

        if let Some(_proxy_handle) = _proxy.upgrade() {
            let _conn_mgr_arc: Arc<Mutex<ConnManager>>;
            {
                let mut _p = _proxy_handle.as_ref().lock().await;
                _conn_mgr_arc = _p.get_conn_mgr();
            }
            let mut _conn_mgr = _conn_mgr_arc.as_ref().lock().await;

            let entity_id = ev.entity_id.unwrap();
            if let Some(_entity) = _conn_mgr.delete_entity(&entity_id) {
                if let Some(main_conn_id) = _entity.get_main_conn_id() {
                    let mut main_send_ret = false;
                    {
                        if let Some(_client_arc) = _conn_mgr.get_client_proxy(&main_conn_id) {
                            let mut _client = _client_arc.as_ref().lock().await;
                            if _client.send_client_msg(ClientService::DeleteRemoteEntity(DeleteRemoteEntity::new(entity_id.clone()))).await {
                                main_send_ret = true;
                            }
                            _client.entities.remove(&entity_id);
                        }
                    }
                    if !main_send_ret {
                        _conn_mgr.delete_client_proxy(&main_conn_id);
                        
                        info!("do_delete_remote_entity main_conn_id delete_client_proxy!");
                    }
                }
                for id in _entity.get_conn_ids().iter() {
                    let mut send_ret = false;
                    {
                        if let Some(_client_arc) = _conn_mgr.get_client_proxy(id) {
                            let mut _client = _client_arc.as_ref().lock().await;
                            if _client.send_client_msg(ClientService::DeleteRemoteEntity(DeleteRemoteEntity::new(entity_id.clone()))).await {
                                send_ret = true;
                            }
                            _client.entities.remove(&entity_id);
                        }
                    }
                    if !send_ret {
                        _conn_mgr.delete_client_proxy(id);
                        
                        info!("do_delete_remote_entity get_conn_ids delete_client_proxy!");
                    }
                }
            }
        }
        else {
            error!("do_delete_remote_entity HubProxy is destory!");
        }

        trace!("do_hub_event delete_remote_entity end!");
    }

    pub async fn do_refresh_entity(_proxy: Weak<Mutex<HubProxy>>, ev: HubCallClientRefreshEntity) {
        trace!("do_hub_event do_refresh_entity begin!");

        if let Some(_proxy_handle) = _proxy.upgrade() {
            let _proxy_clone = _proxy_handle.clone();

            let _conn_mgr_arc: Arc<Mutex<ConnManager>>;
            let _source_hub_name: String;
            {
                let mut _p = _proxy_handle.as_ref().lock().await;
                _conn_mgr_arc = _p.get_conn_mgr();
                _source_hub_name = _p.get_hub_name();
            }
            
            let is_main = ev.is_main.unwrap();
            let conn_id = ev.conn_id.unwrap();
            let entity_id = ev.entity_id.unwrap();
            let entity_type = ev.entity_type.unwrap();
            let argvs = ev.argvs.unwrap();

            let mut send_ret = false;
            {
                let mut _conn_mgr = _conn_mgr_arc.as_ref().lock().await;
                if let Some(_client_arc) = _conn_mgr.get_client_proxy(&conn_id.clone()) {
                    let mut _client = _client_arc.as_ref().lock().await;
                    if _client.send_client_msg(ClientService::RefreshEntity(RefreshEntity::new(entity_id.clone(), entity_type.clone(), is_main.clone(), argvs.clone()))).await {
                        send_ret = true;
                        _client.entities.insert(entity_id.clone());
                    }
                }
            }
            if send_ret {
                let mut _conn_mgr = _conn_mgr_arc.as_ref().lock().await;
                let _entity = match _conn_mgr.get_entity_mut(&entity_id) {
                    None => {
                        let e = Entity::new(entity_id.clone(), _source_hub_name.clone());
                        _conn_mgr.update_entity(e);
                        _conn_mgr.get_entity_mut(&entity_id.clone()).unwrap()
                    }
                    Some(e) => e
                };
                if is_main {
                    _entity.set_main_conn_id(Some(conn_id.clone()));
                    if let Some(_client_arc) = _conn_mgr.get_client_proxy(&conn_id.clone()) {
                        let mut _client = _client_arc.as_ref().lock().await;
                        _client.hub_proxies.insert(_source_hub_name.clone(), _proxy_clone.clone());
                    }
                }
                else {
                    _entity.add_conn_id(conn_id.clone());
                }
            }
            else {
                let mut _conn_mgr = _conn_mgr_arc.as_ref().lock().await;
                _conn_mgr.delete_client_proxy(&conn_id);
                        
                info!("do_refresh_entity conn_id delete_client_proxy!");
            }
        }
        else {
            error!("do_refresh_entity HubProxy is destory!");
        }

        trace!("do_hub_event do_refresh_entity end!");
    }

    pub async fn do_call_client_rpc(_proxy: Weak<Mutex<HubProxy>>, ev: HubCallClientRpc) {
        trace!("do_hub_event call_client_rpc begin!");

        if let Some(_proxy_handle) = _proxy.upgrade() {
            let _conn_mgr_arc: Arc<Mutex<ConnManager>>;
            let hub_name: String;
            {
                let mut _p = _proxy_handle.as_ref().lock().await;
                _conn_mgr_arc = _p.get_conn_mgr();
                hub_name = _p.get_hub_name();
            }
            
            let event = ev.message.unwrap();
            let entity_id = ev.entity_id.unwrap();
            let msg_cb_id = ev.msg_cb_id.unwrap();
            if let Some(_entity) = get_mut_entity(_conn_mgr_arc.clone(), entity_id.clone()).await {
                if let Some(main_conn_id) = _entity.get_main_conn_id() {
                    let mut main_send_ret = false;
                    {
                        let mut _conn_mgr = _conn_mgr_arc.as_ref().lock().await;
                        if let Some(_client_arc) = _conn_mgr.get_client_proxy(&main_conn_id) {
                            let mut _client = _client_arc.as_ref().lock().await;
                            if _client.send_client_msg(ClientService::CallRpc(CallRpc::new(hub_name, entity_id, msg_cb_id, event))).await {
                                main_send_ret = true;
                            }
                        }
                    }
                    if !main_send_ret {
                        let mut _entity_mut = _entity;
                        _entity_mut.set_main_conn_id(None);
                        let mut _conn_mgr = _conn_mgr_arc.as_ref().lock().await;
                        _conn_mgr.delete_client_proxy(&main_conn_id);
                        
                        info!("do_call_client_rpc main_conn_id delete_client_proxy!");
                    }
                }
            }
        }
        else {
            error!("do_call_client_rpc HubProxy is destory!");
        }

        trace!("do_hub_event call_client_rpc end!");
    }

    pub async fn do_call_client_rsp(_proxy: Weak<Mutex<HubProxy>>, ev: HubCallClientRsp) {
        trace!("do_hub_event call_client_rpc begin!");

        if let Some(_proxy_handle) = _proxy.upgrade() {
            let _conn_mgr_arc: Arc<Mutex<ConnManager>>;
            {
                let mut _p = _proxy_handle.as_ref().lock().await;
                _conn_mgr_arc = _p.get_conn_mgr();
            }
            
            let event = ev.rsp.unwrap();
            let conn_id = ev.conn_id.unwrap();
            let entity_id = event.clone().entity_id.unwrap();
            if let Some(_entity) = get_mut_entity(_conn_mgr_arc.clone(), entity_id.clone()).await {
                let mut send_ret = false;
                {
                    let mut _conn_mgr_tmp = _conn_mgr_arc.as_ref().lock().await;
                    if let Some(_client_arc) = _conn_mgr_tmp.get_client_proxy(&conn_id) {
                        let mut _client = _client_arc.as_ref().lock().await;
                        if _client.send_client_msg(ClientService::CallRsp(CallRsp::new(event.clone()))).await {
                            send_ret = true;
                        }
                    }
                }
                if !send_ret {
                    let mut _entity_mut = _entity;
                    _entity_mut.set_main_conn_id(None);
                    let mut _conn_mgr_tmp = _conn_mgr_arc.as_ref().lock().await;
                    _conn_mgr_tmp.delete_client_proxy(&conn_id);
                        
                    info!("do_call_client_rsp conn_id delete_client_proxy!");
                }
            }
        }
        else {
            error!("do_call_client_rsp HubProxy is destory!");
        }

        trace!("do_hub_event call_client_rpc end!");
    }

    pub async fn do_call_client_err(_proxy: Weak<Mutex<HubProxy>>, ev: HubCallClientErr) {
        trace!("do_hub_event call_client_rpc begin!");

        if let Some(_proxy_handle) = _proxy.upgrade() {
            let _conn_mgr_arc: Arc<Mutex<ConnManager>>;
            {
                let mut _p = _proxy_handle.as_ref().lock().await;
                _conn_mgr_arc = _p.get_conn_mgr();
            }
            
            let event = ev.err.unwrap();
            let conn_id = ev.conn_id.unwrap();
            let entity_id = event.clone().entity_id.unwrap();
            if let Some(_entity) = get_mut_entity(_conn_mgr_arc.clone(), entity_id.clone()).await {
                let mut send_ret = false;
                {
                    let mut _conn_mgr_tmp = _conn_mgr_arc.as_ref().lock().await;
                    if let Some(_client_arc) = _conn_mgr_tmp.get_client_proxy(&conn_id) {
                        let mut _client = _client_arc.as_ref().lock().await;
                        if _client.send_client_msg(ClientService::CallErr(CallErr::new(event.clone()))).await {
                            send_ret = true;
                        }
                    }
                }
                if !send_ret {
                    let mut _entity_mut = _entity;
                    _entity_mut.set_main_conn_id(None);
                    let mut _conn_mgr_tmp = _conn_mgr_arc.as_ref().lock().await;
                    _conn_mgr_tmp.delete_client_proxy(&conn_id);
                        
                    info!("do_call_client_err conn_id delete_client_proxy!");
                }
            }
        }
        else {
            error!("do_call_client_err HubProxy is destory!");
        }

        trace!("do_hub_event call_client_rpc end!");
    }

    pub async fn do_call_client_ntf(_proxy: Weak<Mutex<HubProxy>>, ev: HubCallClientNtf) {
        trace!("do_hub_event call_client_ntf begin!");

        if let Some(_proxy_handle) = _proxy.upgrade() {
            let _conn_mgr_arc: Arc<Mutex<ConnManager>>;
            let hub_name: String;
            {
                let mut _p = _proxy_handle.as_ref().lock().await;
                _conn_mgr_arc = _p.get_conn_mgr();
                hub_name = _p.get_hub_name();
            }
            
            let entity_id = ev.entity_id.unwrap();
            let entity_id_clone = entity_id.clone();
            let event = ev.message.unwrap();
            if let Some(_entity_tmp) = get_mut_entity(_conn_mgr_arc.clone(), entity_id.clone()).await {
                let mut _entity = _entity_tmp;

                if let Some(conn_id) = ev.conn_id {
                    let mut conn_send_ret = false;
                    {
                        let mut _conn_mgr_tmp = _conn_mgr_arc.as_ref().lock().await;
                        if let Some(_client_arc) = _conn_mgr_tmp.get_client_proxy(&conn_id) {
                            let mut _client = _client_arc.as_ref().lock().await;
                            if _client.send_client_msg(ClientService::CallNtf(CallNtf::new(hub_name.clone(), entity_id.clone(), event.clone()))).await {
                                conn_send_ret = true;
                            }
                        }
                    }
                    if !conn_send_ret {
                        _entity.set_main_conn_id(None);
                        let mut _conn_mgr_tmp = _conn_mgr_arc.as_ref().lock().await;
                        _conn_mgr_tmp.delete_client_proxy(&conn_id);
                        
                        info!("do_call_client_ntf conn_id delete_client_proxy!");
                    }
                }
                else {
                    if let Some(main_conn_id) = _entity.get_main_conn_id() {
                        let mut main_send_ret = false;
                        {
                            let mut _conn_mgr_tmp = _conn_mgr_arc.as_ref().lock().await;
                            if let Some(_client_arc) = _conn_mgr_tmp.get_client_proxy(&main_conn_id) {
                                let mut _client = _client_arc.as_ref().lock().await;
                                if _client.send_client_msg(ClientService::CallNtf(CallNtf::new(hub_name.clone(), entity_id, event.clone()))).await {
                                    main_send_ret = true;
                                }
                            }
                        }
                        if !main_send_ret {
                            _entity.set_main_conn_id(None);
                            let mut _conn_mgr_tmp = _conn_mgr_arc.as_ref().lock().await;
                            _conn_mgr_tmp.delete_client_proxy(&main_conn_id);
                        
                            info!("do_call_client_ntf main_conn_id delete_client_proxy!");
                        }
                    }

                    let mut invalid_ids: Vec<String> = vec![];
                    for id in _entity.get_conn_ids().iter() {
                        let mut send_ret = false;
                        {
                            let mut _conn_mgr_tmp = _conn_mgr_arc.as_ref().lock().await;
                            if let Some(_client_arc) = _conn_mgr_tmp.get_client_proxy(id) {
                                let mut _client = _client_arc.as_ref().lock().await;
                                let _entity_id_tmp = entity_id_clone.clone();
                                if _client.send_client_msg(ClientService::CallNtf(CallNtf::new(hub_name.clone(), _entity_id_tmp, event.clone()))).await {
                                    send_ret = true;
                                }
                            }
                        }
                        if !send_ret {
                            invalid_ids.push(id.to_string());
                            let mut _conn_mgr_tmp = _conn_mgr_arc.as_ref().lock().await;
                            _conn_mgr_tmp.delete_client_proxy(id);
                        
                            info!("do_call_client_ntf get_conn_ids delete_client_proxy!");
                        }
                    }
                    for invalid_id in invalid_ids.iter() {
                        _entity.delete_conn_id(invalid_id);
                    }
                }
            }
        }
        else {
            error!("do_call_client_ntf HubProxy is destory!");
        }

        trace!("do_hub_event call_client_ntf end!");
    }

    pub async fn do_call_client_global(_proxy: Weak<Mutex<HubProxy>>, ev: HubCallClientGlobal) {
        trace!("do_hub_event call_client_global begin!");

        if let Some(_proxy_handle) = _proxy.upgrade() {
            let _conn_mgr_arc: Arc<Mutex<ConnManager>>;
            let hub_name: String;
            {
                let mut _p = _proxy_handle.as_ref().lock().await;
                _conn_mgr_arc = _p.get_conn_mgr();
                hub_name = _p.get_hub_name();
            }
            
            let event = ev.message.unwrap();
            let mut _conn_mgr = _conn_mgr_arc.as_ref().lock().await;
            let _clients = _conn_mgr.get_all_client_proxy();
            for _client_arc in _clients.iter() {
                let mut _client = _client_arc.as_ref().lock().await;
                let _event_tmp = event.clone();
                if !_client.send_client_msg(ClientService::CallGlobal(CallGlobal::new(hub_name.clone(), _event_tmp))).await {
                    _conn_mgr.delete_client_proxy(&_client.get_conn_id());
                        
                    info!("do_call_client_global get_conn_ids delete_client_proxy!");
                }
            }
        }
        else {
            error!("do_call_client_global HubProxy is destory!");
        }

        trace!("do_hub_event call_client_global end!");
    }

    pub async fn do_kick_off_client(_proxy: Weak<Mutex<HubProxy>>, ev: HubCallKickOffClient) {
        trace!("do_hub_event kick_off_client begin!");

        if let Some(_proxy_handle) = _proxy.upgrade() {
            let _proxy_clone = _proxy_handle.clone();

            let _conn_mgr_arc: Arc<Mutex<ConnManager>>;
            {
                let mut _p = _proxy_handle.as_ref().lock().await;
                _conn_mgr_arc = _p.get_conn_mgr();
            }

            let conn_id = ev.conn_id.unwrap();
            if ev.new_gate.is_none() || ev.new_conn_id.is_none() {
                let mut _conn_mgr = _conn_mgr_arc.as_ref().lock().await;
                if let Some(_client_arc) = _conn_mgr.get_client_proxy(&conn_id) {
                    let mut _client = _client_arc.as_ref().lock().await;
                    let _ = _client.send_client_msg(ClientService::KickOff(KickOff::new(ev.prompt_info.unwrap()))).await;
                    _client.ntf_client_offline(_proxy_clone).await;
                }
                else {
                    let mut _p = _proxy_handle.as_ref().lock().await;
                    _p.send_hub_msg(HubService::TransferMsgEnd(TransferMsgEnd::new(conn_id, false))).await;
                }
            }
            else if ev.new_gate.is_some() && ev.new_conn_id.is_some() {
                let new_gate_name = ev.new_gate.unwrap();
                let new_conn_id = ev.new_conn_id.unwrap();

                let mut is_kick_off = false;
                {
                    let mut _conn_mgr = _conn_mgr_arc.as_ref().lock().await;
                    if let Some(_client_arc) = _conn_mgr.get_client_proxy(&conn_id) {
                        let mut _client = _client_arc.as_ref().lock().await;
                        let _ = _client.send_client_msg(ClientService::KickOff(KickOff::new(ev.prompt_info.unwrap()))).await;
                        
                        for (_, _hub_proxy) in &_client.hub_proxies {
                            let mut _hub = _hub_proxy.as_ref().lock().await;
                            for _entity_id in &_client.entities {
                                if let Some(entity) = _conn_mgr.get_entity(_entity_id) {
                                    let is_main = entity.get_main_conn_id().unwrap_or_default() == conn_id;
                                    _hub.send_hub_msg(HubService::TransferEntityControl(TransferEntityControl::new(
                                        _entity_id.clone(), is_main, ev.is_replace.unwrap(), new_gate_name.clone(), new_conn_id.clone()))).await;
                                }
                            }
                        }
                        is_kick_off = true;
                    }
                }
                let mut _p = _proxy_handle.as_ref().lock().await;
                _p.send_hub_msg(HubService::TransferMsgEnd(TransferMsgEnd::new(conn_id, is_kick_off))).await;
            }
        }
        else {
            error!("do_kick_off_client HubProxy is destory!");
        }

        trace!("do_hub_event kick_off_client end!");
    }
    
    pub async fn do_transfer_client_complete(_proxy: Weak<Mutex<HubProxy>>, ev: HubCallTransferClientComplete) {
        trace!("do_hub_event transfer_client_complete begin!");

        if let Some(_proxy_handle) = _proxy.upgrade() {
            let _conn_mgr_arc: Arc<Mutex<ConnManager>>;
            {
                let mut _p = _proxy_handle.as_ref().lock().await;
                _conn_mgr_arc = _p.get_conn_mgr();
            }

            let mut _conn_mgr = _conn_mgr_arc.as_ref().lock().await;
            if let Some(_client_arc) = _conn_mgr.get_client_proxy(&ev.conn_id.unwrap()) {
                let mut _client = _client_arc.as_ref().lock().await;
                _client.send_client_msg(ClientService::TransferComplete(TransferComplete::new())).await;
            }
        }
        else {
            error!("do_transfer_client_complete HubProxy is destory!");
        }

        trace!("do_hub_event transfer_client_complete end!");
    }

    pub async fn do_kick_off_client_complete(_proxy: Weak<Mutex<HubProxy>>, ev: HubCallKickOffClientComplete) {
        trace!("do_hub_event kick_off_client_complete begin!");

        if let Some(_proxy_handle) = _proxy.upgrade() {
            let mut _p = _proxy_handle.as_ref().lock().await;
            let _conn_mgr_arc = _p.get_conn_mgr();
            let mut _conn_mgr = _conn_mgr_arc.as_ref().lock().await;

            if let Some(_client_arc) = _conn_mgr.get_client_proxy(&ev.conn_id.unwrap()) {
                let mut _client = _client_arc.as_ref().lock().await;
                _client.check_hub_kick_off(_p.get_hub_name()).await;
            }
        }
        else {
            error!("do_kick_off_client_complete HubProxy is destory!");
        }

        trace!("do_hub_event kick_off_client_complete end!");

    }
    
}