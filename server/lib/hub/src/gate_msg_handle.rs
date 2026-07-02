use std::sync::Arc;

use pyo3::prelude::*;
use pyo3::types::PyBytes;
use tracing::{trace, warn, error};

// gate msg
use proto::hub::{
    ClientRequestLogin,
    ClientRequestReconnect,
    TransferMsgEnd,
    TransferEntityControl,
    ClientRequestService,
    KickOffClient,
    ClientDisconnnect,
    ClientCallRpc,
    ClientCallRsp,
    ClientCallErr,
    ClientCallNtf,
};

use crate::hub_service_manager::StdMutex;

pub struct GateCallbackMsgHandle {}

impl GateCallbackMsgHandle {
    pub fn new() -> Arc<StdMutex<GateCallbackMsgHandle>> {
        Arc::new(StdMutex::new(GateCallbackMsgHandle{}))
    }

    pub fn do_client_request_login(&mut self, py: Python<'_>, py_handle: Py<PyAny>, ev: ClientRequestLogin) {
        trace!("do_client_request_login begin py_handle:{}!", py_handle);

        let gate_name = match ev.gate_name {
            Some(v) => v,
            None => {
                warn!("Missing required field 'gate_name' in ClientRequestLogin, skipping");
                return;
            }
        };
        let conn_id = match ev.conn_id {
            Some(v) => v,
            None => {
                warn!("Missing required field 'conn_id' in ClientRequestLogin, skipping");
                return;
            }
        };
        let sdk_uuid = match ev.sdk_uuid {
            Some(v) => v,
            None => {
                warn!("Missing required field 'sdk_uuid' in ClientRequestLogin, skipping");
                return;
            }
        };
        let argvs = ev.argvs.unwrap_or_default();

        let argvs = (
            gate_name,
            conn_id,
            sdk_uuid,
            PyBytes::new(py, &argvs),
        );

        if let Err(e) = py_handle.call_method1(py, "on_client_request_login", argvs) {
            error!("do_client_request_login failed: {}", e);
        }
    }

    pub fn do_client_request_reconnect(&mut self, py: Python<'_>, py_handle: Py<PyAny>, ev: ClientRequestReconnect) {
        trace!("do_client_request_reconnect begin!");

        let gate_name = match ev.gate_name {
            Some(v) => v,
            None => {
                warn!("Missing required field 'gate_name' in ClientRequestReconnect, skipping");
                return;
            }
        };
        let conn_id = match ev.conn_id {
            Some(v) => v,
            None => {
                warn!("Missing required field 'conn_id' in ClientRequestReconnect, skipping");
                return;
            }
        };
        let account_id = match ev.account_id {
            Some(v) => v,
            None => {
                warn!("Missing required field 'account_id' in ClientRequestReconnect, skipping");
                return;
            }
        };
        let argvs = ev.argvs.unwrap_or_default();

        let argvs = (
            gate_name,
            conn_id,
            account_id,
            argvs,
        );

        if let Err(e) = py_handle.call_method1(py, "on_client_request_reconnect", argvs) {
            error!("do_client_request_reconnect failed: {}", e);
        }
    }

    pub fn do_transfer_msg_end(&mut self, py: Python<'_>, py_handle: Py<PyAny>, ev: TransferMsgEnd) {
        trace!("do_transfer_msg_end begin!");

        let conn_id = match ev.conn_id {
            Some(v) => v,
            None => {
                warn!("Missing required field 'conn_id' in TransferMsgEnd, skipping");
                return;
            }
        };
        let is_kick_off = ev.is_kick_off.unwrap_or_default();

        let argvs = (
            conn_id,
            is_kick_off,
        );

        if let Err(e) = py_handle.call_method1(py, "do_transfer_msg_end", argvs) {
            error!("do_transfer_msg_end failed: {}", e);
        }
    }

    pub fn do_transfer_entity_control(&mut self, py: Python<'_>, py_handle: Py<PyAny>, ev: TransferEntityControl) {
        trace!("do_transfer_entity_control begin!");

        let entity_id = match ev.entity_id {
            Some(v) => v,
            None => {
                warn!("Missing required field 'entity_id' in TransferEntityControl, skipping");
                return;
            }
        };
        let is_main = ev.is_main.unwrap_or_default();
        let is_reconnect = ev.is_reconnect.unwrap_or_default();
        let gate_name = match ev.gate_name {
            Some(v) => v,
            None => {
                warn!("Missing required field 'gate_name' in TransferEntityControl, skipping");
                return;
            }
        };
        let conn_id = match ev.conn_id {
            Some(v) => v,
            None => {
                warn!("Missing required field 'conn_id' in TransferEntityControl, skipping");
                return;
            }
        };

        let argvs = (
            entity_id,
            is_main,
            is_reconnect,
            gate_name,
            conn_id,
        );

        if let Err(e) = py_handle.call_method1(py, "on_transfer_entity_control", argvs) {
            error!("do_transfer_entity_control failed: {}", e);
        }
    }

    pub fn do_client_kick_off(&mut self, py: Python<'_>, py_handle: Py<PyAny>, gate_name: String, ev: KickOffClient) {
        trace!("do_client_kick_off begin!");

        let conn_id = match ev.conn_id {
            Some(v) => v,
            None => {
                warn!("Missing required field 'conn_id' in KickOffClient, skipping");
                return;
            }
        };

        let argvs = (
            gate_name,
            conn_id,
        );

        if let Err(e) = py_handle.call_method1(py, "on_client_kick_off", argvs) {
            error!("do_client_kick_off failed: {}", e);
        }
    }

    pub fn do_client_disconnnect(&mut self, py: Python<'_>, py_handle: Py<PyAny>, gate_name: String, ev: ClientDisconnnect) {
        trace!("do_client_disconnnect begin!");

        let conn_id = match ev.conn_id {
            Some(v) => v,
            None => {
                warn!("Missing required field 'conn_id' in ClientDisconnnect, skipping");
                return;
            }
        };

        let argvs = (
            gate_name,
            conn_id,
        );

        if let Err(e) = py_handle.call_method1(py, "on_client_disconnnect", argvs) {
            error!("do_client_disconnnect failed: {}", e);
        }
    }

    pub fn do_client_request_service(&mut self, py: Python<'_>, py_handle: Py<PyAny>, gate_name: String, ev: ClientRequestService) {
        trace!("do_client_request_service begin!");

        let service_name = match ev.service_name {
            Some(v) => v,
            None => {
                warn!("Missing required field 'service_name' in ClientRequestService, skipping");
                return;
            }
        };
        let conn_id = match ev.conn_id {
            Some(v) => v,
            None => {
                warn!("Missing required field 'conn_id' in ClientRequestService, skipping");
                return;
            }
        };
        let argvs = ev.argvs.unwrap_or_default();

        let argvs = (
            service_name,
            gate_name,
            conn_id,
            argvs,
        );

        if let Err(e) = py_handle.call_method1(py, "on_client_request_service", argvs) {
            error!("do_client_request_service failed: {}", e);
        }
    }

    pub fn do_client_call_rpc(&mut self, py: Python<'_>, py_handle: Py<PyAny>, gate_name: String, ev: ClientCallRpc) {
        trace!("do_client_call_rpc begin!");

        let conn_id = match ev.conn_id {
            Some(v) => v,
            None => {
                warn!("Missing required field 'conn_id' in ClientCallRpc, skipping");
                return;
            }
        };
        let entity_id = match ev.entity_id {
            Some(v) => v,
            None => {
                warn!("Missing required field 'entity_id' in ClientCallRpc, skipping");
                return;
            }
        };
        let msg_cb_id = ev.msg_cb_id.unwrap_or_default();
        let msg = match ev.message {
            Some(v) => v,
            None => {
                warn!("Missing required field 'message' in ClientCallRpc, skipping");
                return;
            }
        };
        let method = match msg.method {
            Some(v) => v,
            None => {
                warn!("Missing required field 'method' in Msg, skipping");
                return;
            }
        };
        let msg_argvs = msg.argvs.unwrap_or_default();

        let argvs = (
            gate_name,
            conn_id,
            entity_id,
            msg_cb_id,
            method,
            PyBytes::new(py, &msg_argvs),
        );

        if let Err(e) = py_handle.call_method1(py, "on_client_call_rpc", argvs) {
            error!("do_client_call_rpc failed: {}", e);
        }
    }

    pub fn do_client_call_rsp(&mut self, py: Python<'_>, py_handle: Py<PyAny>, gate_name: String, ev: ClientCallRsp) {
        trace!("do_client_call_rsp begin!");

        let msg = match ev.rsp {
            Some(v) => v,
            None => {
                warn!("Missing required field 'rsp' in ClientCallRsp, skipping");
                return;
            }
        };
        let entity_id = match msg.entity_id {
            Some(v) => v,
            None => {
                warn!("Missing required field 'entity_id' in RpcRsp, skipping");
                return;
            }
        };
        let msg_cb_id = msg.msg_cb_id.unwrap_or_default();
        let msg_argvs = msg.argvs.unwrap_or_default();

        let argvs = (
            gate_name,
            entity_id,
            msg_cb_id,
            PyBytes::new(py, &msg_argvs),
        );

        if let Err(e) = py_handle.call_method1(py, "on_client_call_rsp", argvs) {
            error!("do_client_call_rsp failed: {}", e);
        }
    }

    pub fn do_client_call_err(&mut self, py: Python<'_>, py_handle: Py<PyAny>, gate_name: String, ev: ClientCallErr) {
        trace!("do_client_call_err begin!");

        let msg = match ev.err {
            Some(v) => v,
            None => {
                warn!("Missing required field 'err' in ClientCallErr, skipping");
                return;
            }
        };
        let entity_id = match msg.entity_id {
            Some(v) => v,
            None => {
                warn!("Missing required field 'entity_id' in RpcErr, skipping");
                return;
            }
        };
        let msg_cb_id = msg.msg_cb_id.unwrap_or_default();
        let msg_argvs = msg.argvs.unwrap_or_default();

        let argvs = (
            gate_name,
            entity_id,
            msg_cb_id,
            PyBytes::new(py, &msg_argvs),
        );

        if let Err(e) = py_handle.call_method1(py, "on_client_call_err", argvs) {
            error!("do_client_call_err failed: {}", e);
        }
    }

    pub fn do_client_call_ntf(&mut self, py: Python<'_>, py_handle: Py<PyAny>, gate_name: String, ev: ClientCallNtf) {
        trace!("do_client_call_ntf begin!");

        let entity_id = match ev.entity_id {
            Some(v) => v,
            None => {
                warn!("Missing required field 'entity_id' in ClientCallNtf, skipping");
                return;
            }
        };
        let msg = match ev.message {
            Some(v) => v,
            None => {
                warn!("Missing required field 'message' in ClientCallNtf, skipping");
                return;
            }
        };
        let method = match msg.method {
            Some(v) => v,
            None => {
                warn!("Missing required field 'method' in Msg, skipping");
                return;
            }
        };
        let msg_argvs = msg.argvs.unwrap_or_default();

        let argvs = (
            gate_name,
            entity_id,
            method,
            PyBytes::new(py, &msg_argvs),
        );

        if let Err(e) = py_handle.call_method1(py, "on_client_call_ntf", argvs) {
            error!("do_client_call_ntf failed: {}", e);
        }
    }
}
