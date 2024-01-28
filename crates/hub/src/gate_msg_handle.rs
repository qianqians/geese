use std::sync::Arc;

use pyo3::prelude::*;
use pyo3::types::PyBytes;
use tracing::{trace, error};

// gate msg
use proto::hub::{
    ClientRequestLogin,
    ClientRequestReconnect,
    TransferMsgEnd,
    TransferEntityControl,
    ClientRequestService,
    ClientDisconnnect,
    ClientCallRpc,
    ClientCallRsp,
    ClientCallErr,
    ClientCallNtf,
};

use crate::hub_service_manager::StdMutex;

pub struct GateCallbackMsgHandle {
}

impl GateCallbackMsgHandle {
    pub fn new() -> Arc<StdMutex<GateCallbackMsgHandle>> {
        Arc::new(StdMutex::new(GateCallbackMsgHandle{}))
    }

    pub fn do_client_request_login(&mut self, py: Python<'_>, py_handle: Py<PyAny>, ev: ClientRequestLogin) {
        trace!("do_client_request_login begin py_handle:{}!", py_handle);

        let argvs = (ev.gate_name.unwrap(), ev.conn_id.unwrap(), ev.sdk_uuid.unwrap());
        if let Err(e) = py_handle.call_method1(py, "on_client_request_login", argvs) {
            error!("do_client_request_login python callback error:{}", e)
        }
    }

    pub fn do_client_request_reconnect(&mut self, py: Python<'_>, py_handle: Py<PyAny>, ev: ClientRequestReconnect) {
        trace!("do_client_request_reconnect begin!");

        let argvs = (ev.gate_name.unwrap(), ev.conn_id.unwrap(), ev.account_id.unwrap(), ev.token.unwrap());
        if let Err(e) = py_handle.call_method1(py, "on_client_request_reconnect", argvs) {
            error!("do_client_request_reconnect python callback error:{}", e)
        }    
    }

    pub fn do_transfer_msg_end(&mut self, py: Python<'_>, py_handle: Py<PyAny>, gate_name: String, ev: TransferMsgEnd) {
        trace!("do_transfer_msg_end begin!");

        let argvs = (gate_name, ev.conn_id.unwrap(), ev.is_kick_off.unwrap());
        if let Err(e) = py_handle.call_method1(py, "on_transfer_msg_end", argvs) {
            error!("do_transfer_msg_end python callback error:{}", e)
        }
    }

    pub fn do_transfer_entity_control(&mut self, py: Python<'_>, py_handle: Py<PyAny>, ev: TransferEntityControl) {
        trace!("do_transfer_msg_end begin!");

        let argvs = (ev.entity_id.unwrap(), ev.is_main.unwrap(), ev.is_replace.unwrap(), ev.gate_name.unwrap(), ev.conn_id.unwrap(),);
        if let Err(e) = py_handle.call_method1(py, "on_transfer_entity_control", argvs) {
            error!("do_transfer_msg_end python callback error:{}", e)
        }
    }

    pub fn do_client_disconnnect(&mut self, py: Python<'_>, py_handle: Py<PyAny>, gate_name: String, ev: ClientDisconnnect) {
        trace!("do_client_disconnnect begin!");

        let argvs = (gate_name, ev.conn_id.unwrap(),);
        if let Err(e) = py_handle.call_method1(py, "on_client_disconnnect", argvs) {
            error!("do_client_disconnnect python callback error:{}", e)
        }
    }

    pub fn do_client_request_service(&mut self, py: Python<'_>, py_handle: Py<PyAny>, gate_name: String, ev:ClientRequestService) {
        trace!("do_client_request_service begin!");

        let argvs = (ev.service_name.unwrap(), gate_name, ev.conn_id.unwrap(), );
        if let Err(e) = py_handle.call_method1(py, "on_client_request_service", argvs) {
            error!("do_client_disconnnect python callback error:{}", e)
        }
    }

    pub fn do_client_call_rpc(&mut self, py: Python<'_>, py_handle: Py<PyAny>, gate_name: String, ev: ClientCallRpc) {
        trace!("do_client_call_rpc begin!");

        let msg = ev.message.unwrap();
        let argvs = (
            gate_name,
            ev.conn_id.unwrap(), 
            ev.entity_id.unwrap(), 
            ev.msg_cb_id.unwrap(),
            msg.method.unwrap(),
            PyBytes::new(py, &msg.argvs.unwrap()));
        if let Err(e) = py_handle.call_method1(py, "on_client_call_rpc", argvs) {
            error!("do_client_call_rpc python callback error:{}", e)
        }
    }
    
    pub fn do_client_call_rsp(&mut self, py: Python<'_>, py_handle: Py<PyAny>, gate_name: String, ev: ClientCallRsp) {
        trace!("do_client_call_rsp begin!");

        let msg = ev.rsp.unwrap();
        let argvs = (
            gate_name,
            msg.entity_id.unwrap(),
            msg.msg_cb_id.unwrap(),
            PyBytes::new(py, &msg.argvs.unwrap()));
        if let Err(e) = py_handle.call_method1(py, "on_client_call_rsp", argvs) {
            error!("do_client_call_rsp python callback error:{}", e)
        }
    }

    pub fn do_client_call_err(&mut self, py: Python<'_>, py_handle: Py<PyAny>, gate_name: String, ev: ClientCallErr) {
        trace!("do_client_call_err begin!");

        let msg = ev.err.unwrap();
        let argvs = (
            gate_name,
            msg.entity_id.unwrap(),
            msg.msg_cb_id.unwrap(),
            PyBytes::new(py, &msg.argvs.unwrap()));
        if let Err(e) = py_handle.call_method1(py, "on_client_call_err", argvs) {
            error!("do_client_call_err python callback error:{}", e)
        }
    }
    
    pub fn do_client_call_ntf(&mut self, py: Python<'_>, py_handle: Py<PyAny>, gate_name: String, ev: ClientCallNtf) {
        trace!("do_client_call_ntf begin!");

        let msg = ev.message.unwrap();
        let argvs = (
            gate_name,
            ev.entity_id.unwrap(),
            msg.method.unwrap(),
            PyBytes::new(py, &msg.argvs.unwrap()));
        if let Err(e) = py_handle.call_method1(py, "on_client_call_ntf", argvs) {
            error!("do_client_call_ntf python callback error:{}", e)
        }
    }
}