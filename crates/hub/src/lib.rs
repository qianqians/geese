use std::sync::Arc;

use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use uuid::Uuid;
use consulrs::api::check::common::AgentServiceCheckBuilder;
use consulrs::api::service::requests::RegisterServiceRequest;
use pyo3::prelude::*;
use pyo3::exceptions::PyValueError;
use serde::{Deserialize, Serialize};
use tracing_appender::non_blocking::WorkerGuard;
use tracing::{info, error, trace};

use health::HealthHandle;
use consul::ConsulImpl;
use log;
use config::{load_data_from_file, load_cfg_from_data};
use local_ip::get_local_ip;

use proto::common::{
    Msg,
    RpcRsp,
    RpcErr,
    RegServer
};

use proto::dbproxy::{
    DbEvent,
    GetGuidEvent,
    CreateObjectEvent,
    UpdateObjectEvent,
    FindAndModifyEvent,
    RemoveObjectEvent,
    GetObjectInfoEvent,
    GetObjectCountEvent,
};

use proto::hub::{
    HubService,
    QueryServiceEntity,
    CreateServiceEntity,
    HubForwardClientRequestService,
    HubCallHubRpc,
    HubCallHubRsp,
    HubCallHubErr,
    HubCallHubNtf,
};

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

mod dbproxy_manager;
mod dbproxy_msg_handle;
mod hub_proxy_manager;
mod hub_msg_handle;
mod gate_proxy_manager;
mod gate_msg_handle;
mod hub_service_manager;
mod conn_manager;
mod hub_server;

use crate::hub_server::{HubServer, StdMutex};
use crate::dbproxy_msg_handle::DBCallbackMsgHandle;
use crate::hub_service_manager::ConnCallbackMsgHandle;

#[derive(Deserialize, Serialize, Debug)]
struct HubCfg {
    consul_url: String,
    health_port: u16,
    redis_url: String,
    service_port: u16,
    jaeger_url: Option<String>,
    log_level: String,
    log_file: String,
    log_dir: String
}

#[pyclass]
pub struct HubContext {
    hub_name: String,
    service_port: u16,
    health_port: u16,
    _guard: WorkerGuard, 
    _join_health: JoinHandle<()>,
    _listen_rt: tokio::runtime::Runtime,
    _health_rt: tokio::runtime::Runtime,
    health_handle: Arc<Mutex<HealthHandle>>,
    consul_impl: Arc<Mutex<ConsulImpl>>,
    server: Arc<StdMutex<HubServer>>
}

#[pymethods]
impl HubContext {
    #[new]
    pub fn new(cfg_file: String) -> PyResult<Self> {
        info!("hub start!");

        let _name = format!("hub_{}", Uuid::new_v4());

        let cfg_data = match load_data_from_file(cfg_file.to_string()) {
            Err(e) => {
                error!("hub load_data_from_file faild {}, {}!", cfg_file, e);
                return Err(PyValueError::new_err("hub load_data_from_file faild!"));
            },
            Ok(_cfg_data) => _cfg_data
        };
        let cfg = match load_cfg_from_data::<HubCfg>(&cfg_data) {
            Err(e) => {
                error!("hub load_cfg_from_data faild {}, {}!", cfg_data, e);
                return Err(PyValueError::new_err("hub load_cfg_from_data faild!"));
            },
            Ok(_cfg) => _cfg
        };

        let (_, _guard) = log::init(cfg.log_level, cfg.log_dir, cfg.log_file, cfg.jaeger_url, Some(_name.clone()));
    
        let _health_port = cfg.health_port;
        let _health_host = format!("0.0.0.0:{}", _health_port);
        let _health_handle = HealthHandle::new(_health_host.clone());
    
        let consul_impl = ConsulImpl::new(cfg.consul_url);
        let _consul_impl_arc = Arc::new(Mutex::new(consul_impl));
        let _consul_impl_clone = _consul_impl_arc.clone();
    
        let _hub_host = format!("0.0.0.0:{}", cfg.service_port);
        let server = match HubServer::new(_name.clone(), cfg.redis_url, _hub_host, _consul_impl_arc) {
            Err(e) => {
                error!("Hub HubServer new faild {}!", e);
                return Err(PyValueError::new_err("Hub HubServer new faild!"));
            },
            Ok(_s) => Arc::new(StdMutex::new(_s))
        };

        let rt = tokio::runtime::Runtime::new().unwrap();
        let _s = server.clone();
        rt.block_on(async move {
            let mut _s_handle = _s.as_ref().lock().unwrap();
            _s_handle.listen_hub_service().await;
        });
        
        let rt_join_health = tokio::runtime::Runtime::new().unwrap();
        let _join_health = rt_join_health.spawn(HealthHandle::start_health_service(_health_host.clone(), _health_handle.clone()));

        Ok(HubContext {
            hub_name: _name,
            service_port: cfg.service_port,
            health_port: _health_port,
            health_handle: _health_handle,
            _guard: _guard,
            _join_health: _join_health,
            _listen_rt: rt,
            _health_rt: rt_join_health,
            consul_impl: _consul_impl_clone,
            server: server.clone()
        })
    }

    pub fn hub_name(slf: PyRefMut<'_, Self>) -> String {
        slf.hub_name.clone()
    }

    pub fn gate_host(slf: PyRefMut<'_, Self>, gate_name:String) -> String {
        trace!("gate_host gate_name:{} begin!", gate_name);

        let _server = slf.server.clone();
        let mut _server_handle = _server.as_ref().lock().unwrap();
        _server_handle.gate_host(gate_name)
    }

    pub fn log(_: PyRefMut<'_, Self>, level: String, content: String) {
        HubServer::log(level, content)
    }

    pub fn register_service(slf: PyRefMut<'_, Self>, service: String) {
        trace!("register_service begin!");

        let _health_port = slf.health_port;
        let _consul_impl_clone = slf.consul_impl.clone();
        let _name = slf.hub_name.clone();
        let _service_port = slf.service_port;

        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async move {
            let _local_ip = get_local_ip();
            let _health_host = format!("http://{_local_ip}:{_health_port}/health");
    
            let mut _consul_impl = _consul_impl_clone.as_ref().lock().await;
            _consul_impl.register(service.clone(), Some(
                RegisterServiceRequest::builder()
                    .name(service.clone())
                    .id(_name)
                    .address(_local_ip)
                    .port(_service_port)
                    .check(AgentServiceCheckBuilder::default()
                        .name("health_check")
                        .interval("10s")
                        .http(_health_host)
                        .status("passing")
                        .build()
                        .unwrap()
                    ),
                ),
            ).await;
        })
    }

    pub fn set_health_state(slf: PyRefMut<'_, Self>, _status: bool) {
        trace!("set_health_state begin!");

        let rt = tokio::runtime::Runtime::new().unwrap();
        let _health_handle = slf.health_handle.clone();
        rt.block_on(async move {
            let mut _handle = _health_handle.as_ref().lock().await;
            _handle.set_health_status(_status);
        });
    }

    pub fn entry_dbproxy_service(slf: PyRefMut<'_, Self>) -> String {
        trace!("entry_dbproxy_service begin!");

        let rt: tokio::runtime::Runtime = tokio::runtime::Runtime::new().unwrap();
        let _server = slf.server.clone();
        rt.block_on(async move {
            let mut _server_handle = _server.as_ref().lock().unwrap();
            let _dbproxy_id =  _server_handle.entry_dbproxy_service().await;
            _dbproxy_id.clone()
        })
    }

    pub fn entry_hub_service(slf: PyRefMut<'_, Self>, service_name: String) -> String {
        trace!("entry_hub_service begin!");

        let rt: tokio::runtime::Runtime = tokio::runtime::Runtime::new().unwrap();
        let _server = slf.server.clone();
        rt.block_on(async move {
            let mut _server_handle = _server.as_ref().lock().unwrap();
            let _hub_id = _server_handle.entry_hub_service(service_name).await;
            _hub_id.clone()
        })
    }

    pub fn entry_direct_hub_server(slf: PyRefMut<'_, Self>, hub_name: String) {
        trace!("entry_direct_hub_server begin!");

        let rt: tokio::runtime::Runtime = tokio::runtime::Runtime::new().unwrap();
        let _server = slf.server.clone();
        rt.block_on(async move {
            let mut _server_handle = _server.as_ref().lock().unwrap();
            _server_handle.entry_direct_hub_server(hub_name).await
        })
    }

    pub fn check_connect_hub_server(slf: PyRefMut<'_, Self>, hub_name: String) -> bool {
        trace!("check_connect_hub_server begin!");

        let _server = slf.server.clone();
        let mut _server_handle = _server.as_ref().lock().unwrap();
        _server_handle.check_connect_hub_server(hub_name)
    }

    pub fn entry_gate_service(slf: PyRefMut<'_, Self>, gate_name: String) {
        trace!("entry_gate_service begin!");

        let rt: tokio::runtime::Runtime = tokio::runtime::Runtime::new().unwrap();
        let _server = slf.server.clone();
        rt.block_on(async move {
            let mut _server_handle = _server.as_ref().lock().unwrap();
            _server_handle.entry_gate_service(gate_name).await
        })
    }

    pub fn flush_hub_host_cache(slf: PyRefMut<'_, Self>) {
        trace!("flush_hub_host_cache begin!");

        let rt: tokio::runtime::Runtime = tokio::runtime::Runtime::new().unwrap();
        let _server = slf.server.clone();
        rt.block_on(async move {
            let mut _server_handle = _server.as_ref().lock().unwrap();
            _server_handle.flush_hub_host_cache().await
        })
    }

    pub fn reg_hub_to_hub(slf: PyRefMut<'_, Self>, hub_name: String) -> bool {
        trace!("reg_hub_to_hub begin!");

        let _server = slf.server.clone();
        let _self_name = slf.hub_name.clone();

        let mut _server_handle = _server.as_ref().lock().unwrap();
        _server_handle.send_hub_msg(hub_name, HubService::RegServer(RegServer::new(_self_name)))
    }

    pub fn query_service(slf: PyRefMut<'_, Self>, hub_name: String, service_name: String) -> bool {
        trace!("query_service begin!");

        let _server = slf.server.clone();
        let mut _server_handle = _server.as_ref().lock().unwrap();
        _server_handle.send_hub_msg(hub_name, HubService::QueryEntity(QueryServiceEntity::new(service_name)))
    }

    pub fn create_service_entity(
        slf: PyRefMut<'_, Self>, 
        hub_name: String, 
        service_name: String, 
        entity_id: String,
        entity_type: String,
        argvs: Vec<u8>) -> bool 
    {
        trace!("create_service_entity begin!");

        let _server = slf.server.clone();
        let mut _server_handle = _server.as_ref().lock().unwrap();
        _server_handle.send_hub_msg(hub_name, HubService::CreateServiceEntity(CreateServiceEntity::new(service_name, entity_id, entity_type, argvs)))
    }

    pub fn forward_client_request_service(
        slf: PyRefMut<'_, Self>, 
        hub_name: String, 
        service_name: String, 
        gate_name: String, 
        gate_host: String, 
        conn_id: String) -> bool 
    {
        trace!("forward_client_request_service begin!");

        let _server = slf.server.clone();
        let mut _server_handle = _server.as_ref().lock().unwrap();
        _server_handle.send_hub_msg(hub_name, HubService::HubForwardClientRequestService(HubForwardClientRequestService::new(service_name, gate_name, gate_host, conn_id)))
    }

    pub fn hub_call_hub_rpc(
        slf: PyRefMut<'_, Self>, 
        hub_name: String, 
        entity_id: String, 
        msg_cb_id: i64,
        method: String,
        argvs: Vec<u8>) -> bool 
    {
        trace!("hub_call_hub_rpc begin!");

        let _server = slf.server.clone();
        let mut _server_handle = _server.as_ref().lock().unwrap();
        _server_handle.send_hub_msg(
            hub_name, 
            HubService::HubCallRpc(
                HubCallHubRpc::new(entity_id, msg_cb_id, Msg::new(method, argvs))
            )
        )
    }

    pub fn hub_call_hub_rsp(
        slf: PyRefMut<'_, Self>, 
        hub_name: String, 
        entity_id: String, 
        msg_cb_id: i64,
        argvs: Vec<u8>) -> bool 
    {
        trace!("hub_call_hub_rsp begin!");

        let _server = slf.server.clone();
        let mut _server_handle = _server.as_ref().lock().unwrap();
        _server_handle.send_hub_msg(
            hub_name, 
            HubService::HubCallRsp(
                HubCallHubRsp::new(RpcRsp::new(entity_id, msg_cb_id, argvs))
            )
        )
    }

    pub fn hub_call_hub_err(
        slf: PyRefMut<'_, Self>, 
        hub_name: String, 
        entity_id: String, 
        msg_cb_id: i64,
        argvs: Vec<u8>) -> bool 
    {
        trace!("hub_call_hub_err begin!");

        let _server = slf.server.clone();
        let mut _server_handle = _server.as_ref().lock().unwrap();
        _server_handle.send_hub_msg(
            hub_name, 
            HubService::HubCallErr(
                HubCallHubErr::new(RpcErr::new(entity_id, msg_cb_id, argvs))
            )
        )
    }

    pub fn hub_call_hub_ntf(
        slf: PyRefMut<'_, Self>, 
        hub_name: String, 
        entity_id: String, 
        method: String,
        argvs: Vec<u8>) -> bool 
    {
        trace!("hub_call_hub_ntf begin!");

        let _server = slf.server.clone();
        let mut _server_handle = _server.as_ref().lock().unwrap();
        _server_handle.send_hub_msg(
            hub_name, 
            HubService::HubCallNtf(
                HubCallHubNtf::new(entity_id, Msg::new(method, argvs))
            )
        )
    }

    pub fn hub_call_client_create_remote_entity(
        slf: PyRefMut<'_, Self>, 
        gate_name: String, 
        conn_id: Vec<String>, 
        main_conn_id: String, 
        entity_id: String, 
        entity_type: String, 
        argvs: Vec<u8>) -> bool 
    {
        trace!("hub_call_client_create_remote_entity begin!");

        let _server = slf.server.clone();
        let _self_name = slf.hub_name.clone();

        let rt: tokio::runtime::Runtime = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async move {
            let mut _server_handle = _server.as_ref().lock().unwrap();
            _server_handle.send_gate_msg(
                gate_name, 
                GateHubService::CreateRemoteEntity(
                    HubCallClientCreateRemoteEntity::new(conn_id, main_conn_id, entity_id, entity_type, argvs)
                )
            ).await
        })
    }

    pub fn hub_call_client_delete_remote_entity(slf: PyRefMut<'_, Self>, gate_name: String, entity_id: String) -> bool {
        trace!("hub_call_client_delete_remote_entity begin!");

        let _server = slf.server.clone();
        let _self_name = slf.hub_name.clone();

        let rt: tokio::runtime::Runtime = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async move {
            let mut _server_handle = _server.as_ref().lock().unwrap();
            _server_handle.send_gate_msg(
                gate_name, 
                GateHubService::DeleteRemoteEntity(
                    HubCallClientDeleteRemoteEntity::new(entity_id)
                )
            ).await
        })
    }

    pub fn hub_call_client_refresh_entity(slf: PyRefMut<'_, Self>, 
        gate_name: String, 
        conn_id: String, 
        is_main: bool, 
        entity_id: String, 
        entity_type: String, 
        argvs: Vec<u8>) -> bool 
    {
        trace!("hub_call_client_refresh_entity begin!");

        let _server = slf.server.clone();
        let _self_name = slf.hub_name.clone();

        let rt: tokio::runtime::Runtime = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async move {
            let mut _server_handle = _server.as_ref().lock().unwrap();
            _server_handle.send_gate_msg(
                gate_name, 
                GateHubService::RefreshEntity(
                    HubCallClientRefreshEntity::new(conn_id, is_main, entity_id, entity_type, argvs)
                )
            ).await
        })
    }

    pub fn hub_call_client_rpc(
        slf: PyRefMut<'_, Self>, 
        gate_name: String, 
        entity_id: String, 
        msg_cb_id: i64, 
        method: String, 
        argvs: Vec<u8>) -> bool 
    {
        trace!("hub_call_client_rpc begin!");

        let _server = slf.server.clone();
        let _self_name = slf.hub_name.clone();

        let rt: tokio::runtime::Runtime = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async move {
            let mut _server_handle = _server.as_ref().lock().unwrap();
            _server_handle.send_gate_msg(
                gate_name, 
                GateHubService::CallRpc(
                    HubCallClientRpc::new(entity_id, msg_cb_id, Msg::new(method, argvs))
                )
            ).await
        })
    }

    pub fn hub_call_client_rsp(
        slf: PyRefMut<'_, Self>, 
        gate_name: String, 
        conn_id: String, 
        entity_id: String, 
        msg_cb_id: i64, 
        argvs: Vec<u8>) -> bool 
    {
        trace!("hub_call_client_rsp begin!");

        let _server = slf.server.clone();
        let _self_name = slf.hub_name.clone();

        let rt: tokio::runtime::Runtime = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async move {
            let mut _server_handle = _server.as_ref().lock().unwrap();
            _server_handle.send_gate_msg(
                gate_name, 
                GateHubService::CallRsp(
                    HubCallClientRsp::new(conn_id, RpcRsp::new(entity_id, msg_cb_id, argvs))
                )
            ).await
        })
    }

    pub fn hub_call_client_err(
        slf: PyRefMut<'_, Self>, 
        gate_name: String, 
        conn_id: String, 
        entity_id: String, 
        msg_cb_id: i64, 
        argvs: Vec<u8>) -> bool 
    {
        trace!("hub_call_client_err begin!");

        let _server = slf.server.clone();
        let _self_name = slf.hub_name.clone();

        let rt: tokio::runtime::Runtime = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async move {
            let mut _server_handle = _server.as_ref().lock().unwrap();
            _server_handle.send_gate_msg(
                gate_name, 
                GateHubService::CallErr(
                    HubCallClientErr::new(conn_id, RpcErr::new(entity_id, msg_cb_id, argvs))
                )
            ).await
        })
    }

    pub fn hub_call_client_ntf(
        slf: PyRefMut<'_, Self>, 
        gate_name: String, 
        conn_id: String, 
        entity_id: String, 
        method: String, 
        argvs: Vec<u8>) -> bool 
    {
        trace!("hub_call_client_ntf begin!");

        let _server = slf.server.clone();
        let _self_name = slf.hub_name.clone();

        let rt: tokio::runtime::Runtime = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async move {
            let mut _server_handle = _server.as_ref().lock().unwrap();
            _server_handle.send_gate_msg(
                gate_name, 
                GateHubService::CallNtf(
                    HubCallClientNtf::new(conn_id, entity_id, Msg::new(method, argvs))
                )
            ).await
        })
    }

    pub fn hub_call_client_global(slf: PyRefMut<'_, Self>, gate_name: String, method: String, argvs: Vec<u8>) -> bool {
        trace!("hub_call_client_global begin!");

        let _server = slf.server.clone();
        let _self_name = slf.hub_name.clone();

        let rt: tokio::runtime::Runtime = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async move {
            let mut _server_handle = _server.as_ref().lock().unwrap();
            _server_handle.send_gate_msg(
                gate_name, 
                GateHubService::CallGlobal(
                    HubCallClientGlobal::new(Msg::new(method, argvs))
                )
            ).await
        })
    }

    pub fn hub_call_kick_off_client(slf: PyRefMut<'_, Self>, old_gate_name: String, old_conn_id: String, new_gate_name: String, new_conn_id: String, is_replace: bool, prompt_info: String) -> bool {
        trace!("hub_call_kick_off_client begin!");

        let _server = slf.server.clone();
        let _self_name = slf.hub_name.clone();

        let rt: tokio::runtime::Runtime = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async move {
            let mut _server_handle = _server.as_ref().lock().unwrap();
            _server_handle.send_gate_msg(
                old_gate_name, 
                GateHubService::KickOff(
                    HubCallKickOffClient::new(old_conn_id, prompt_info, new_gate_name, new_conn_id, is_replace)
                )
            ).await
        })
    }

    pub fn hub_call_transfer_client_complete(slf: PyRefMut<'_, Self>, gate_name: String, conn_id: String) -> bool {
        trace!("hub_call_transfer_client_complete begin!");

        let _server = slf.server.clone();
        let _self_name = slf.hub_name.clone();

        let rt: tokio::runtime::Runtime = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async move {
            let mut _server_handle = _server.as_ref().lock().unwrap();
            _server_handle.send_gate_msg(
                gate_name, 
                GateHubService::TransferComplete(
                    HubCallTransferClientComplete::new(conn_id)
                )
            ).await
        })
    }

    pub fn hub_call_kick_off_client_complete(slf: PyRefMut<'_, Self>, gate_name: String, conn_id: String) -> bool {
        trace!("hub_call_kick_off_client_complete begin!");

        let _server = slf.server.clone();
        let _self_name = slf.hub_name.clone();

        let rt: tokio::runtime::Runtime = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async move {
            let mut _server_handle = _server.as_ref().lock().unwrap();
            _server_handle.send_gate_msg(
                gate_name, 
                GateHubService::KickOffComplete(
                    HubCallKickOffClientComplete::new(conn_id)
                )
            ).await
        })
    }

    pub fn get_guid(
        slf: PyRefMut<'_, Self>, 
        dbproxy_name: String,
        db: String,
        collection: String,
        callback_id: String) -> bool 
    {
        trace!("get_guid begin!");

        let _server = slf.server.clone();
        let mut _server_handle = _server.as_ref().lock().unwrap();
        trace!("get_guid _server_handle lock!");
        let success = _server_handle.send_db_msg(
            dbproxy_name.clone(), 
            DbEvent::GetGuid(
                GetGuidEvent::new(db, collection, callback_id)
            )
        );

        trace!("get_guid end!");
        return success;
    }

    pub fn create_object(
        slf: PyRefMut<'_, Self>, 
        dbproxy_name: String,
        db: String,
        collection: String,
        callback_id: String,
        object_info: Vec<u8>) -> bool 
    {
        trace!("create_object begin!");

        let _server = slf.server.clone();
        let mut _server_handle = _server.as_ref().lock().unwrap();
        let success = _server_handle.send_db_msg(
            dbproxy_name, 
            DbEvent::CreateObject(
                CreateObjectEvent::new(db, collection, callback_id, object_info)
            )
        );

        trace!("create_object end!");
        return success;
    }

    pub fn update_object(
        slf: PyRefMut<'_, Self>, 
        dbproxy_name: String,
        db: String,
        collection: String,
        callback_id: String,
        query_info: Vec<u8>,
        updata_info: Vec<u8>,
        _upsert: bool) -> bool 
    {
        trace!("update_object begin!");

        let _server = slf.server.clone();
        let mut _server_handle = _server.as_ref().lock().unwrap();
        let success = _server_handle.send_db_msg(
            dbproxy_name, 
            DbEvent::UpdateObject(
                UpdateObjectEvent::new(db, collection, callback_id, query_info, updata_info, _upsert)
            )
        );

        trace!("update_object end!");
        return success;
    }

    pub fn find_and_modify(
        slf: PyRefMut<'_, Self>, 
        dbproxy_name: String,
        db: String,
        collection: String,
        callback_id: String,
        query_info: Vec<u8>,
        updata_info: Vec<u8>,
        _new: bool,
        _upsert: bool) -> bool 
    {
        trace!("find_and_modify begin!");

        let _server = slf.server.clone();
        let mut _server_handle = _server.as_ref().lock().unwrap();
        let success = _server_handle.send_db_msg(
            dbproxy_name, 
            DbEvent::FindAndModify(
                FindAndModifyEvent::new(db, collection, callback_id, query_info, updata_info, _new, _upsert)
            )
        );

        trace!("find_and_modify end!");
        return success;
    }

    pub fn remove_object(
        slf: PyRefMut<'_, Self>, 
        dbproxy_name: String,
        db: String,
        collection: String,
        callback_id: String,
        query_info: Vec<u8>) -> bool 
    {
        trace!("remove_object begin!");

        let _server = slf.server.clone();
        let mut _server_handle = _server.as_ref().lock().unwrap();
        let success = _server_handle.send_db_msg(
            dbproxy_name, 
            DbEvent::RemoveObject(
                RemoveObjectEvent::new(db, collection, callback_id, query_info)
            )
        );

        trace!("remove_object end!");
        return success;
    }

    pub fn get_object_info(
        slf: PyRefMut<'_, Self>, 
        dbproxy_name: String,
        db: String,
        collection: String,
        callback_id: String,
        query_info: Vec<u8>,
        skip: i32,
        limit: i32,
        sort: String,
        ascending: bool) -> bool 
    {
        trace!("get_object_info begin!");

        let _server = slf.server.clone();
        let mut _server_handle = _server.as_ref().lock().unwrap();
        let success = _server_handle.send_db_msg(
            dbproxy_name, 
            DbEvent::GetObjectInfo(
                GetObjectInfoEvent::new(db, collection, callback_id, query_info, skip, limit, sort, ascending)
            )
        );

        trace!("get_object_info end!");
        return success;
    }

    pub fn get_object_one(
        slf: PyRefMut<'_, Self>, 
        dbproxy_name: String,
        db: String,
        collection: String,
        callback_id: String,
        query_info: Vec<u8>) -> bool 
    {
        trace!("get_object_one begin!");

        let _server = slf.server.clone();
        let mut _server_handle = _server.as_ref().lock().unwrap();
        let success = _server_handle.send_db_msg(
            dbproxy_name.clone(), 
            DbEvent::GetObjectInfo(
                GetObjectInfoEvent::new(db, collection, callback_id, query_info, 0, 100, "".to_string(), false)
            )
        );

        trace!("get_object_one end!");
        return success;
    }

    pub fn get_object_count(
        slf: PyRefMut<'_, Self>, 
        dbproxy_name: String,
        db: String,
        collection: String,
        callback_id: String,
        query_info: Vec<u8>) -> bool 
    {
        trace!("get_object_count begin!");

        let _server = slf.server.clone();
        let mut _server_handle = _server.as_ref().lock().unwrap();
        let success = _server_handle.send_db_msg(
            dbproxy_name, 
            DbEvent::GetObjectCount(
                GetObjectCountEvent::new(db, collection, callback_id, query_info)
            )
        );

        trace!("get_object_count end!");
        return success;
    }
}

#[pyclass]
pub struct HubConnMsgPump {
    conn_msg_handle: Arc<StdMutex<ConnCallbackMsgHandle>>,
}

#[pymethods]
impl HubConnMsgPump {
    #[new]
    pub fn new(ctx: PyRefMut<HubContext>) -> PyResult<Self> {
        let _server_handle = ctx.server.as_ref().lock().unwrap();

        Ok(HubConnMsgPump{
            conn_msg_handle: _server_handle.get_conn_msg_handle()
        })
    }

    pub fn poll_conn_msg<'a>(slf: PyRefMut<'a, Self>, py_handle: &'a PyAny) -> bool {
        let py = slf.py();
        let _py_handle = py_handle.into_py(py);
        ConnCallbackMsgHandle::poll(slf.conn_msg_handle.clone(), py, _py_handle)
    }
}

#[pyclass]
pub struct HubDBMsgPump {
    db_msg_handle: Arc<StdMutex<DBCallbackMsgHandle>>,
}

#[pymethods]
impl HubDBMsgPump {
    #[new]
    pub fn new(ctx: PyRefMut<HubContext>) -> PyResult<Self> {
        let _server_handle = ctx.server.as_ref().lock().unwrap();

        Ok(HubDBMsgPump{
            db_msg_handle: _server_handle.get_db_msg_handle()
        })
    }

    pub fn poll_db_msg<'a>(slf: PyRefMut<'a, Self>, py_handle: &'a PyAny) -> bool {
        let py = slf.py();
        let _py_handle = py_handle.into_py(py);
        DBCallbackMsgHandle::poll(slf.db_msg_handle.clone(), py, _py_handle)
    }
}
