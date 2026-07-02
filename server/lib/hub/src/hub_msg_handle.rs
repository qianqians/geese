use std::sync::Arc;

use pyo3::prelude::*;
use pyo3::types::PyBytes;
use tracing::{trace, warn, error};

use proto::common::RegServer;

// hub msg
use proto::hub::{
    CreateServiceEntity, 
    HubCallHubCreateMigrateEntity, 
    HubCallHubErr, 
    HubCallHubMigrateEntity, 
    HubCallHubMigrateEntityComplete, 
    HubCallHubNtf, 
    HubCallHubRpc, 
    HubCallHubRsp, 
    HubCallHubWaitMigrateEntity, 
    HubForwardClientRequestService, 
    HubForwardClientRequestServiceExt, 
    QueryServiceEntity
};

use crate::hub_service_manager::StdMutex;

pub struct HubCallbackMsgHandle {
}

impl HubCallbackMsgHandle {
    pub fn new() -> Arc<StdMutex<HubCallbackMsgHandle>> {
        Arc::new(StdMutex::new(HubCallbackMsgHandle {}))
    }

    pub fn do_reg_hub(&mut self, py: Python<'_>, py_handle: Py<PyAny>, ev: RegServer) {
        trace!("do_rge_hub begin!");

        let name = match ev.name {
            Some(v) => v,
            None => {
                warn!("Missing required field 'name' in RegServer, skipping");
                return;
            }
        };

        let argvs = (
            name,
        );

        if let Err(e) = py_handle.call_method1(py, "on_rge_hub", argvs) {
            error!("do_rge_hub python callback error:{}", e)
        }
    }

    pub fn do_query_service_entity(&mut self, py: Python<'_>, py_handle: Py<PyAny>, hub_name: String, ev: QueryServiceEntity) {
        trace!("do_query_service_entity begin!");

        let service_name = match ev.service_name {
            Some(v) => v,
            None => {
                warn!("Missing required field 'service_name' in QueryServiceEntity, skipping");
                return;
            }
        };

        let argvs = (
            hub_name, 
            service_name,
        );

        if let Err(e) = py_handle.call_method1(py, "on_query_service_entity", argvs) {
            error!("do_query_service_entity python callback error:{}", e)
        }
    }

    pub fn do_create_service_entity(&mut self, py: Python<'_>, py_handle: Py<PyAny>, hub_name: String, ev: CreateServiceEntity) {
        trace!("do_create_service_entity begin!");

        let is_migrate = ev.is_migrate.unwrap_or_default();
        let service_name = match ev.service_name {
            Some(v) => v,
            None => {
                warn!("Missing required field 'service_name' in CreateServiceEntity, skipping");
                return;
            }
        };
        let entity_id = match ev.entity_id {
            Some(v) => v,
            None => {
                warn!("Missing required field 'entity_id' in CreateServiceEntity, skipping");
                return;
            }
        };
        let entity_type = match ev.entity_type {
            Some(v) => v,
            None => {
                warn!("Missing required field 'entity_type' in CreateServiceEntity, skipping");
                return;
            }
        };
        let argvs = ev.argvs.unwrap_or_default();

        let argvs = (
            hub_name, 
            is_migrate,
            service_name, 
            entity_id, 
            entity_type, 
            PyBytes::new(py, &argvs)
        );

        if let Err(e) = py_handle.call_method1(py, "on_create_service_entity", argvs) {
            error!("do_create_service_entity python callback error:{}", e)
        }
    }

    pub fn do_forward_client_request_service(&mut self, py: Python<'_>, py_handle: Py<PyAny>, hub_name: String, ev: HubForwardClientRequestService) {
        trace!("do_forward_client_request_service begin!");

        let service_name = match ev.service_name {
            Some(v) => v,
            None => {
                warn!("Missing required field 'service_name' in HubForwardClientRequestService, skipping");
                return;
            }
        };
        let gate_name = match ev.gate_name {
            Some(v) => v,
            None => {
                warn!("Missing required field 'gate_name' in HubForwardClientRequestService, skipping");
                return;
            }
        };
        let conn_id = match ev.conn_id {
            Some(v) => v,
            None => {
                warn!("Missing required field 'conn_id' in HubForwardClientRequestService, skipping");
                return;
            }
        };
        let argvs = ev.argvs.unwrap_or_default();

        let argvs = (
            hub_name,
            service_name, 
            gate_name, 
            conn_id,
            argvs
        );

        if let Err(e) = py_handle.call_method1(py, "on_forward_client_request_service", argvs) {
            error!("do_forward_client_request_service python callback error:{}", e)
        }
    }

    pub fn do_forward_client_request_service_ext(&mut self, py: Python<'_>, py_handle: Py<PyAny>, hub_name: String, ev: HubForwardClientRequestServiceExt) {
        trace!("do_forward_client_request_service begin!");

        let service_name = match ev.service_name {
            Some(v) => v,
            None => {
                warn!("Missing required field 'service_name' in HubForwardClientRequestServiceExt, skipping");
                return;
            }
        };

        let mut request_infos: Vec<(String, String, Vec<u8>)> = Vec::new();
        for info in ev.request_infos.unwrap_or_default() {
            let gate_name = match info.gate_name {
                Some(v) => v,
                None => {
                    warn!("Missing required field 'gate_name' in ForwardClientRequestInfo, skipping entry");
                    continue;
                }
            };
            let conn_id = match info.conn_id {
                Some(v) => v,
                None => {
                    warn!("Missing required field 'conn_id' in ForwardClientRequestInfo, skipping entry");
                    continue;
                }
            };
            let argvs = info.argvs.unwrap_or_default();
            request_infos.push((gate_name, conn_id, argvs));
        }

        let argvs = (
            hub_name,
            service_name, 
            request_infos
        );

        if let Err(e) = py_handle.call_method1(py, "on_forward_client_request_service_ext", argvs) {
            error!("do_forward_client_request_service python callback error:{}", e)
        }
    }

    pub fn do_call_hub_rpc(&mut self, py: Python<'_>, py_handle: Py<PyAny>, hub_name: String, ev: HubCallHubRpc) {
        trace!("do_call_hub_rpc begin!");

        let entity_id = match ev.entity_id {
            Some(v) => v,
            None => {
                warn!("Missing required field 'entity_id' in HubCallHubRpc, skipping");
                return;
            }
        };
        let msg_cb_id = ev.msg_cb_id.unwrap_or_default();
        let msg = match ev.message {
            Some(v) => v,
            None => {
                warn!("Missing required field 'message' in HubCallHubRpc, skipping");
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
            hub_name,
            entity_id, 
            msg_cb_id, 
            method,
            PyBytes::new(py, &msg_argvs)
        );

        if let Err(e) = py_handle.call_method1(py, "on_call_hub_rpc", argvs) {
            error!("do_call_hub_rpc python callback error:{}", e)
        }
    }

    pub fn do_call_hub_rsp(&mut self, py: Python<'_>, py_handle: Py<PyAny>, hub_name: String, ev: HubCallHubRsp) {
        trace!("do_call_hub_rsp begin!");

        let msg = match ev.rsp {
            Some(v) => v,
            None => {
                warn!("Missing required field 'rsp' in HubCallHubRsp, skipping");
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
            hub_name,
            entity_id, 
            msg_cb_id, 
            PyBytes::new(py, &msg_argvs)
        );

        if let Err(e) = py_handle.call_method1(py, "on_call_hub_rsp", argvs) {
            error!("do_call_hub_rsp python callback error:{}", e)
        }
    }
    
    pub fn do_call_hub_err(&mut self, py: Python<'_>, py_handle: Py<PyAny>, hub_name: String, ev: HubCallHubErr) {
        trace!("do_call_hub_err begin!");

        let msg = match ev.err {
            Some(v) => v,
            None => {
                warn!("Missing required field 'err' in HubCallHubErr, skipping");
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
            hub_name,
            entity_id, 
            msg_cb_id, 
            PyBytes::new(py, &msg_argvs)
        );

        if let Err(e) = py_handle.call_method1(py, "on_call_hub_err", argvs) {
            error!("do_call_hub_err python callback error:{}", e)
        }
    }
    
    pub fn do_call_hub_ntf(&mut self, py: Python<'_>, py_handle: Py<PyAny>, hub_name: String, ev: HubCallHubNtf) {
        trace!("do_call_hub_ntf begin!");

        let entity_id = match ev.entity_id {
            Some(v) => v,
            None => {
                warn!("Missing required field 'entity_id' in HubCallHubNtf, skipping");
                return;
            }
        };
        let msg = match ev.message {
            Some(v) => v,
            None => {
                warn!("Missing required field 'message' in HubCallHubNtf, skipping");
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
            hub_name,
            entity_id, 
            method, 
            PyBytes::new(py, &msg_argvs)
        );

        if let Err(e) = py_handle.call_method1(py, "on_call_hub_ntf", argvs) {
            error!("do_call_hub_ntf python callback error:{}", e)
        }
    }

    pub fn do_wait_migrate_entity(&mut self, py: Python<'_>, py_handle: Py<PyAny>, hub_name: String, ev: HubCallHubWaitMigrateEntity) {
        trace!("do_wait_migrate_entity begin!");

        let entity_id = match ev.entity_id {
            Some(v) => v,
            None => {
                warn!("Missing required field 'entity_id' in HubCallHubWaitMigrateEntity, skipping");
                return;
            }
        };

        let argvs = (
            hub_name,
            entity_id
        );

        if let Err(e) = py_handle.call_method1(py, "on_wait_migrate_entity", argvs) {
            error!("do_wait_migrate_entity python callback error:{}", e)
        }
    }

    pub fn do_migrate_entity(&mut self, py: Python<'_>, py_handle: Py<PyAny>, hub_name: String, ev: HubCallHubMigrateEntity) {
        trace!("do_migrate_entity begin!");

        let entity_type = match ev.entity_type {
            Some(v) => v,
            None => {
                warn!("Missing required field 'entity_type' in HubCallHubMigrateEntity, skipping");
                return;
            }
        };
        let entity_id = match ev.entity_id {
            Some(v) => v,
            None => {
                warn!("Missing required field 'entity_id' in HubCallHubMigrateEntity, skipping");
                return;
            }
        };
        let main_gate_name = match ev.main_gate_name {
            Some(v) => v,
            None => {
                warn!("Missing required field 'main_gate_name' in HubCallHubMigrateEntity, skipping");
                return;
            }
        };
        let main_conn_id = match ev.main_conn_id {
            Some(v) => v,
            None => {
                warn!("Missing required field 'main_conn_id' in HubCallHubMigrateEntity, skipping");
                return;
            }
        };
        let gates = ev.gates.unwrap_or_default();
        let hubs = ev.hubs.unwrap_or_default();
        let argvs = ev.argvs.unwrap_or_default();

        let argvs = (
            hub_name,
            entity_type,
            entity_id,
            main_gate_name,
            main_conn_id,
            gates,
            hubs,
            PyBytes::new(py, &argvs)
        );

        if let Err(e) = py_handle.call_method1(py, "on_migrate_entity", argvs) {
            error!("do_migrate_entity python callback error:{}", e)
        }
    }

    pub fn do_create_migrate_entity(&mut self, py: Python<'_>, py_handle: Py<PyAny>, hub_name: String, ev: HubCallHubCreateMigrateEntity) {
        trace!("do_create_migrate_entity begin!");

        let ev_hub_name = match ev.hub_name {
            Some(v) => v,
            None => {
                warn!("Missing required field 'hub_name' in HubCallHubCreateMigrateEntity, skipping");
                return;
            }
        };
        let entity_id = match ev.entity_id {
            Some(v) => v,
            None => {
                warn!("Missing required field 'entity_id' in HubCallHubCreateMigrateEntity, skipping");
                return;
            }
        };

        let argvs = (
            hub_name,
            ev_hub_name,
            entity_id
        );

        if let Err(e) = py_handle.call_method1(py, "on_create_migrate_entity", argvs) {
            error!("do_create_migrate_entity python callback error:{}", e)
        }
    }

    pub fn do_migrate_entity_complete(&mut self, py: Python<'_>, py_handle: Py<PyAny>, hub_name: String, ev: HubCallHubMigrateEntityComplete) {
        trace!("do_migrate_entity_complete begin!");

        let ev_hub_name = match ev.hub_name {
            Some(v) => v,
            None => {
                warn!("Missing required field 'hub_name' in HubCallHubMigrateEntityComplete, skipping");
                return;
            }
        };
        let entity_id = match ev.entity_id {
            Some(v) => v,
            None => {
                warn!("Missing required field 'entity_id' in HubCallHubMigrateEntityComplete, skipping");
                return;
            }
        };

        let argvs = (
            hub_name,
            ev_hub_name,
            entity_id
        );

        if let Err(e) = py_handle.call_method1(py, "on_migrate_entity_complete", argvs) {
            error!("do_migrate_entity_complete python callback error:{}", e)
        }
    }
    
}
