use std::sync::Arc;

use pyo3::prelude::*;
use pyo3::types::PyBytes;
use tokio::sync::Mutex;
use tracing::{trace, error};

use thrift::protocol::{TCompactInputProtocol, TSerializable};
use thrift::transport::TBufferChannel;

use proto::hub::{
    DbCallback, 
    AckGetGuid, 
    AckCreateObject, 
    AckUpdataObject, 
    AckFindAndModify, 
    AckRemoveObject, 
    AckGetObjectCount, 
    AckGetObjectInfo, 
    AckGetObjectInfoEnd
};
use queue::Queue;

use crate::hub_service_manager::StdMutex;
use crate::dbproxy_manager::DBProxyProxy;

pub struct DBCallbackMsgHandle {
    queue: Queue<Box<DbCallback>>
}

fn deserialize(data: Vec<u8>) -> Result<DbCallback, Box<dyn std::error::Error>> {
    trace!("deserialize begin!");
    let mut t = TBufferChannel::with_capacity(data.len(), 0);
    let _ = t.set_readable_bytes(&data);
    let mut i_prot = TCompactInputProtocol::new(t);
    let ev_data = DbCallback::read_from_in_protocol(&mut i_prot)?;
    Ok(ev_data)
}

impl DBCallbackMsgHandle {
    pub fn new() -> Arc<StdMutex<DBCallbackMsgHandle>> {
        Arc::new(StdMutex::new(DBCallbackMsgHandle {
            queue: Queue::new()
        }))
    }

    fn enque_event(&mut self, ev: DbCallback) {
        self.queue.enque(Box::new(ev))
    }

    pub async fn on_event(_proxy: Arc<Mutex<DBProxyProxy>>, data: Vec<u8>) {
        trace!("DBCallbackMsgHandle on_event begin!");

        let _ev: DbCallback;
        let _handle_arc: Arc<StdMutex<DBCallbackMsgHandle>>;
        {
            let mut _p = _proxy.as_ref().lock().await;
            _ev = match deserialize(data) {
                Err(e) => {
                    error!("GateClientMsgHandle do_event err:{}", e);
                    return;
                }
                Ok(d) => d
            };
            _handle_arc = _p.get_msg_handle();
        }
        let mut _handle = _handle_arc.as_ref().lock().unwrap();
        _handle.enque_event(_ev);

        trace!("DBCallbackMsgHandle on_event end!")
    }

    pub fn poll(_handle: Arc<StdMutex<DBCallbackMsgHandle>>, py: Python<'_>, py_handle: Py<PyAny>) -> bool {
        let mut _self = _handle.as_ref().lock().unwrap();
        let opt_ev_data = _self.queue.deque();
        let ev = match opt_ev_data {
            None => return false,
            Some(ev_data) => ev_data
        };
        trace!("DBCallbackMsgHandle poll event!");
        match *ev {
            DbCallback::GetGuid(ev) => _self.do_ack_get_guid(py, py_handle, ev),
            DbCallback::CreateObject(ev) => _self.do_ack_create_object(py, py_handle, ev),
            DbCallback::UpdataObject(ev) => _self.do_ack_updata_object(py, py_handle, ev),
            DbCallback::FindAndModify(ev) => _self.do_ack_find_and_modify(py, py_handle, ev),
            DbCallback::RemoveObject(ev) => _self.do_ack_remove_object(py, py_handle, ev),
            DbCallback::GetObjectCount(ev) => _self.do_ack_get_object_count(py, py_handle, ev),
            DbCallback::GetObjectInfo(ev) => _self.do_ack_get_object_info(py, py_handle, ev),
            DbCallback::GetObjectInfoEnd(ev) => _self.do_ack_get_object_info_end(py, py_handle, ev)
        };
        trace!("DBCallbackMsgHandle poll event end!");
        return true;
    }

    pub fn do_ack_get_guid(&mut self, py: Python<'_>, py_handle: Py<PyAny>, ev: AckGetGuid) {
        trace!("do_ack_get_guid begin!");

        let args = (ev.callback_id.unwrap(), ev.guid.unwrap());
        if let Err(e) = py_handle.call_method1(py, "on_ack_get_guid", args) {
            error!("do_ack_create_object python callback error:{}", e)
        }
    }

    pub fn do_ack_create_object(&mut self, py: Python<'_>, py_handle: Py<PyAny>, ev: AckCreateObject) {
        trace!("do_ack_create_object begin!");

        let args = (ev.callback_id.unwrap(), ev.result.unwrap());
        if let Err(e) = py_handle.call_method1(py, "on_ack_create_object", args) {
            error!("do_ack_create_object python callback error:{}", e)
        }
    }

    pub fn do_ack_updata_object(&mut self, py: Python<'_>, py_handle: Py<PyAny>, ev: AckUpdataObject) {
        trace!("do_ack_updata_object begin!");

        let args = (ev.callback_id.unwrap(), ev.result.unwrap());
        if let Err(e) = py_handle.call_method1(py, "on_ack_updata_object", args) {
            error!("do_ack_updata_object python callback error:{}", e)
        }
    }

    pub fn do_ack_find_and_modify(&mut self, py: Python<'_>, py_handle: Py<PyAny>, ev: AckFindAndModify) {
        trace!("do_ack_find_and_modify begin!");

        let args = (ev.callback_id.unwrap(), PyBytes::new(py, &ev.object_info.unwrap()));
        if let Err(e) = py_handle.call_method1(py, "on_ack_find_and_modify", args) {
            error!("do_ack_find_and_modify python callback error:{}", e)
        }
    }

    pub fn do_ack_remove_object(&mut self, py: Python<'_>, py_handle: Py<PyAny>, ev: AckRemoveObject) {
        trace!("do_ack_remove_object begin!");

        let args = (ev.callback_id.unwrap(), ev.result.unwrap());
        if let Err(e) = py_handle.call_method1(py, "on_ack_remove_object", args) {
            error!("do_ack_remove_object python callback error:{}", e)
        }
    }
    
    pub fn do_ack_get_object_count(&mut self, py: Python<'_>, py_handle: Py<PyAny>, ev: AckGetObjectCount) {
        trace!("do_ack_get_object_count begin!");

        let args = (ev.callback_id.unwrap(), ev.count.unwrap());
        if let Err(e) = py_handle.call_method1(py, "on_ack_get_object_count", args) {
            error!("do_ack_get_object_count python callback error:{}", e)
        }
    }
    
    pub fn do_ack_get_object_info(&mut self, py: Python<'_>, py_handle: Py<PyAny>, ev: AckGetObjectInfo) {
        trace!("do_ack_get_object_info begin!");

        let callback_id = ev.callback_id.unwrap();
        let data = ev.object_info.unwrap();

        let args = (callback_id.clone(), PyBytes::new(py, &data));
        if let Err(e) = py_handle.call_method1(py, "on_ack_get_object_info", args) {
            error!("do_ack_get_object_info python callback error:{}", e)
        }
    }
    
    pub fn do_ack_get_object_info_end(&mut self, py: Python<'_>, py_handle: Py<PyAny>, ev: AckGetObjectInfoEnd) {
        trace!("do_ack_get_object_info_end begin!");

        let callback_id = ev.callback_id.unwrap();

        let args = (callback_id.clone(), );
        if let Err(e) = py_handle.call_method1(py, "on_ack_get_object_info_end", args) {
            error!("do_ack_get_object_info_end python callback error:{}", e)
        }
    }
    
}

