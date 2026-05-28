use std::sync::Arc;

use pyo3::prelude::*;

use proto::gate::{
    GateClientService,
    ClientRequestHubLogin,
    ClientRequestHubReconnect,
    ClientRequestHubService,
    ClientCallHubRpc,
    ClientCallHubRsp,
    ClientCallHubErr,
    ClientCallHubNtf,
    ClientCallGateHeartbeats,
};

use proto::common::{
    Msg,
    RpcRsp,
    RpcErr
};

mod client;
pub mod py;

use crate::client::{Context, GateMsgHandle, StdMutex};

#[pyclass]
pub struct ClientContext {
    ctx: Arc<StdMutex<Context>>
}

#[pymethods]
impl ClientContext {
    #[new]
    pub fn new() -> PyResult<Self> {
        Ok(ClientContext { 
            ctx: Arc::new(StdMutex::new(Context::new()))
        })
    }

    pub fn connect_tcp(slf: PyRefMut<'_, Self>, addr: String, port: u16) {
        let mut _ctx_handle = slf.ctx.as_ref().lock().unwrap();
        _ctx_handle.connect_tcp(addr, port);
    }

    pub fn connect_ws(slf: PyRefMut<'_, Self>, host: String) {
        let mut _ctx_handle = slf.ctx.as_ref().lock().unwrap();
        _ctx_handle.connect_ws(host);
    }

    pub fn login(slf: PyRefMut<'_, Self>, sdk_uuid: String, argvs:Vec<u8>) -> bool {
        let mut _ctx_handle = slf.ctx.as_ref().lock().unwrap();
        _ctx_handle.send_msg(GateClientService::Login(ClientRequestHubLogin::new(sdk_uuid, argvs)))
    }

    pub fn reconnect(slf: PyRefMut<'_, Self>, account_id: String, argvs:Vec<u8>) -> bool {
        let mut _ctx_handle = slf.ctx.as_ref().lock().unwrap();
        _ctx_handle.send_msg(GateClientService::Reconnect(ClientRequestHubReconnect::new(account_id, argvs)))
    }

    pub fn request_hub_service(slf: PyRefMut<'_, Self>, service_name: String, argvs:Vec<u8>) -> bool {
        let mut _ctx_handle = slf.ctx.as_ref().lock().unwrap();
        _ctx_handle.send_msg(GateClientService::RequestHubService(ClientRequestHubService::new(service_name, argvs)))
    }

    pub fn call_rpc(slf: PyRefMut<'_, Self>, entity_id:String, msg_cb_id:i64, method:String, argvs:Vec<u8>) -> bool {
        let mut _ctx_handle = slf.ctx.as_ref().lock().unwrap();
        _ctx_handle.send_msg(GateClientService::CallRpc(
            ClientCallHubRpc::new(entity_id, msg_cb_id, Msg::new(method, argvs))))
    }
    
    pub fn call_rsp(slf: PyRefMut<'_, Self>, entity_id:String, msg_cb_id:i64, argvs:Vec<u8>) -> bool {
        let mut _ctx_handle = slf.ctx.as_ref().lock().unwrap();
        _ctx_handle.send_msg(GateClientService::CallRsp(
            ClientCallHubRsp::new(RpcRsp::new(entity_id, msg_cb_id, argvs))))
    }

    pub fn call_err(slf: PyRefMut<'_, Self>, entity_id:String, msg_cb_id:i64, argvs:Vec<u8>) -> bool {
        let mut _ctx_handle = slf.ctx.as_ref().lock().unwrap();
        _ctx_handle.send_msg(GateClientService::CallErr(
            ClientCallHubErr::new(RpcErr::new(entity_id, msg_cb_id, argvs))))
    }

    pub fn call_ntf(slf: PyRefMut<'_, Self>, entity_id:String, method:String, argvs:Vec<u8>) -> bool {
        let mut _ctx_handle = slf.ctx.as_ref().lock().unwrap();
        _ctx_handle.send_msg(GateClientService::CallNtf(
            ClientCallHubNtf::new(entity_id, Msg::new(method, argvs))))
    }

    pub fn heartbeats(slf: PyRefMut<'_, Self>) -> bool {
        let mut _ctx_handle = slf.ctx.as_ref().lock().unwrap();
        _ctx_handle.send_msg(GateClientService::Heartbeats(ClientCallGateHeartbeats::new()))
    }
}

#[pyclass]
pub struct ClientPump {
    msg_handle: Arc<StdMutex<GateMsgHandle>>
}

#[pymethods]
impl ClientPump {
    #[new]
    pub fn new(ctx: PyRefMut<ClientContext>) -> PyResult<Self> {
        let _handle = ctx.ctx.as_ref().lock().unwrap();

        Ok(ClientPump{
            msg_handle: _handle.get_msg_handle()
        })
    }

    pub fn poll_conn_msg<'a>(slf: PyRefMut<'a, Self>, py_handle: Py<PyAny>) -> bool {
        let py = slf.py();
        GateMsgHandle::poll(slf.msg_handle.clone(), py, py_handle)
    }
}

/// 把客户端相关的全部 pyclass / pyfunction 挂到给定 Python 模块。
///
/// 由宿主进程的顶层 pymodule（`client/src/lib.rs` 中的 `pyclient`）调用。
/// 包含：
/// - 网络层：[`ClientContext`] / [`ClientPump`]
/// - 渲染层（相机、场景、GLTF、八叉树、动画）：`py::*`
///
/// 渲染相关的 PyClass 全部定义在本子 crate 的 [`py`] 模块中，底层
/// `crates/camera` / `crates/scene` 保持零 pyo3 依赖。
pub fn add_to_module(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<ClientContext>()?;
    m.add_class::<ClientPump>()?;
    py::add_to_module(m)?;
    Ok(())
}