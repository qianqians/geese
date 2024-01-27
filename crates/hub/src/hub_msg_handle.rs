use std::sync::Arc;

use pyo3::prelude::*;
use pyo3::types::PyBytes;
use tokio::sync::Mutex;
use tracing::{trace, error};

use proto::common::RegServer;

// hub msg
use proto::hub::{
    QueryServiceEntity,
    CreateServiceEntity,
    HubForwardClientRequestService,
    HubCallHubRpc,
    HubCallHubRsp,
    HubCallHubErr,
    HubCallHubNtf,
};

pub struct HubCallbackMsgHandle {
}

impl HubCallbackMsgHandle {
    pub fn new() -> Arc<Mutex<HubCallbackMsgHandle>> {
        Arc::new(Mutex::new(HubCallbackMsgHandle {}))
    }

    pub fn do_reg_hub(&mut self, py: Python<'_>, py_handle: Py<PyAny>, ev: RegServer) {
        trace!("do_rge_hub begin!");

        let hub_name = ev.name.unwrap();
        let argvs = (hub_name,);
        if let Err(e) = py_handle.call_method1(py, "on_rge_hub", argvs) {
            error!("do_rge_hub python callback error:{}", e)
        }
    }

    pub fn do_query_service_entity(&mut self, py: Python<'_>, py_handle: Py<PyAny>, hub_name: String, ev: QueryServiceEntity) {
        trace!("do_query_service_entity begin!");

        let argvs = (hub_name, ev.service_name.unwrap(),);
        if let Err(e) = py_handle.call_method1(py, "on_query_service_entity", argvs) {
            error!("do_query_service_entity python callback error:{}", e)
        }
    }

    pub fn do_create_service_entity(&mut self, py: Python<'_>, py_handle: Py<PyAny>, hub_name: String, ev: CreateServiceEntity) {
        trace!("do_create_service_entity begin!");

        let argvs = (
            hub_name, 
            ev.service_name.unwrap(), 
            ev.entity_id.unwrap(), 
            ev.entity_type.unwrap(), 
            PyBytes::new(py, &ev.argvs.unwrap()));
        if let Err(e) = py_handle.call_method1(py, "on_create_service_entity", argvs) {
            error!("do_create_service_entity python callback error:{}", e)
        }
    }

    pub fn do_forward_client_request_service(&mut self, py: Python<'_>, py_handle: Py<PyAny>, hub_name: String, ev: HubForwardClientRequestService) {
        trace!("do_forward_client_request_service begin!");

        let argvs = (
            hub_name,
            ev.service_name.unwrap(), 
            ev.gate_name.unwrap(), 
            ev.conn_id.unwrap());
        if let Err(e) = py_handle.call_method1(py, "on_forward_client_request_service", argvs) {
            error!("do_forward_client_request_service python callback error:{}", e)
        }
    }

    pub fn do_call_hub_rpc(&mut self, py: Python<'_>, py_handle: Py<PyAny>, hub_name: String, ev: HubCallHubRpc) {
        trace!("do_call_hub_rpc begin!");

        let msg = ev.message.unwrap();
        let argvs = (
            hub_name,
            ev.entity_id.unwrap(), 
            ev.msg_cb_id.unwrap(), 
            msg.method.unwrap(),
            PyBytes::new(py, &msg.argvs.unwrap()));
        if let Err(e) = py_handle.call_method1(py, "on_call_hub_rpc", argvs) {
            error!("do_call_hub_rpc python callback error:{}", e)
        }
    }

    pub fn do_call_hub_rsp(&mut self, py: Python<'_>, py_handle: Py<PyAny>, hub_name: String, ev: HubCallHubRsp) {
        trace!("do_call_hub_rsp begin!");

        let msg = ev.rsp.unwrap();
        let argvs = (
            hub_name,
            msg.entity_id.unwrap(), 
            msg.msg_cb_id.unwrap(), 
            PyBytes::new(py, &msg.argvs.unwrap()));
        if let Err(e) = py_handle.call_method1(py, "on_call_hub_rsp", argvs) {
            error!("do_call_hub_rsp python callback error:{}", e)
        }
    }
    
    pub fn do_call_hub_err(&mut self, py: Python<'_>, py_handle: Py<PyAny>, hub_name: String, ev: HubCallHubErr) {
        trace!("do_call_hub_err begin!");

        let msg = ev.err.unwrap();
        let argvs = (
            hub_name,
            msg.entity_id.unwrap(), 
            msg.msg_cb_id.unwrap(), 
            PyBytes::new(py, &msg.argvs.unwrap()));
        if let Err(e) = py_handle.call_method1(py, "on_call_hub_err", argvs) {
            error!("do_call_hub_err python callback error:{}", e)
        }
    }
    
    pub fn do_call_hub_ntf(&mut self, py: Python<'_>, py_handle: Py<PyAny>, hub_name: String, ev: HubCallHubNtf) {
        trace!("do_call_hub_ntf begin!");

        let msg = ev.message.unwrap();
        let argvs = (
            hub_name,
            ev.entity_id.unwrap(), 
            msg.method.unwrap(), 
            PyBytes::new(py, &msg.argvs.unwrap()));
        if let Err(e) = py_handle.call_method1(py, "on_call_hub_ntf", argvs) {
            error!("do_call_hub_ntf python callback error:{}", e)
        }
    }
    
}