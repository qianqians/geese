use std::sync::{Arc, Weak};

use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use pyo3::prelude::*;
use async_trait::async_trait;
use tracing::{trace, warn, error};

use thrift::protocol::{TCompactInputProtocol, TSerializable};
use thrift::transport::TBufferChannel;

use net::{NetReaderCallback, NetReader, NetWriter};
use tcp::tcp_socket::{TcpReader, TcpWriter};
use tcp::tcp_server::TcpListenCallback;
use redis_service::redis_mq_channel::RedisMQReader;
use redis_service::redis_service::{create_lock_key, RedisService, RedisMQListenCallback};
use close_handle::CloseHandle;
use queue::Queue;

use proto::common::{RegServer, RegServerCallback};
use proto::gate::{GateHubService, HubCallTransferEntityComplete};
use proto::hub::HubService;

pub type StdMutex<T> = std::sync::Mutex<T>;

use crate::hub_proxy_manager::HubProxy;
use crate::hub_msg_handle::HubCallbackMsgHandle;
use crate::gate_proxy_manager::GateProxy;
use crate::gate_msg_handle::GateCallbackMsgHandle;
use crate::conn_manager::ConnManager;

pub struct ConnProxy {
    pub wr: Arc<Mutex<Box<dyn NetWriter + Send + 'static>>>,
    pub hubproxy: Option<Arc<Mutex<HubProxy>>>,
    pub gateproxy: Option<Arc<Mutex<GateProxy>>>,
    msg_handle: Arc<StdMutex<ConnCallbackMsgHandle>>
}

impl ConnProxy {
    pub fn new(
        _wr: Arc<Mutex<Box<dyn NetWriter + Send + 'static>>>,
        _msg_handle: Arc<StdMutex<ConnCallbackMsgHandle>>) -> ConnProxy
    {
        ConnProxy {
            wr: _wr,
            hubproxy: None,
            gateproxy: None,
            msg_handle: _msg_handle
        }
    }

    pub fn get_msg_handle(&mut self) -> Arc<StdMutex<ConnCallbackMsgHandle>> {
        self.msg_handle.clone()
    }
}

pub struct ConnEvent {
    connproxy: Weak<Mutex<ConnProxy>>,
    ev: HubService,
}

pub struct ConnCallbackMsgHandle {
    pub redis_service: Option<Arc<Mutex<RedisService>>>,
    hub_name: String,
    hub_msg_handle: Arc<StdMutex<HubCallbackMsgHandle>>,
    gate_msg_handle: Arc<StdMutex<GateCallbackMsgHandle>>,
    conn_mgr: Arc<Mutex<ConnManager>>,
    close: Arc<Mutex<CloseHandle>>,
    queue: Queue<Box<ConnEvent>>
}

fn deserialize(data: Vec<u8>) -> Result<HubService, Box<dyn std::error::Error>> {
    trace!("deserialize begin!");
    let mut t = TBufferChannel::with_capacity(data.len(), 0);
    let _ = t.set_readable_bytes(&data);
    let mut i_prot = TCompactInputProtocol::new(t);
    let ev_data = HubService::read_from_in_protocol(&mut i_prot)?;
    Ok(ev_data)
}

impl ConnCallbackMsgHandle {
    pub fn new(
        _hub_name: String,
        _hub_msg_handle: Arc<StdMutex<HubCallbackMsgHandle>>,
        _gate_msg_handle: Arc<StdMutex<GateCallbackMsgHandle>>,
        _conn_mgr: Arc<Mutex<ConnManager>>,
        _close: Arc<Mutex<CloseHandle>>) -> Arc<StdMutex<ConnCallbackMsgHandle>> 
    {
        Arc::new(StdMutex::new(ConnCallbackMsgHandle {
            hub_name: _hub_name,
            hub_msg_handle: _hub_msg_handle,
            gate_msg_handle: _gate_msg_handle,
            redis_service: None,
            conn_mgr: _conn_mgr,
            close: _close,
            queue: Queue::new()
        }))
    }

    fn enque_event(&mut self, ev: ConnEvent) {
        self.queue.enque(Box::new(ev))
    }

    pub async fn on_event(_proxy: Arc<Mutex<ConnProxy>>, data: Vec<u8>) {
        trace!("do_client_event begin!");

        let _proxy_clone = _proxy.clone();
        let mut _p = _proxy.as_ref().lock().await;
        let _ev = match deserialize(data) {
            Err(e) => {
                error!("GateClientMsgHandle do_event err:{}", e);
                return;
            }
            Ok(d) => d
        };
        let _handle_arc = _p.get_msg_handle();
        let mut _handle = _handle_arc.as_ref().lock().unwrap();
        _handle.enque_event(ConnEvent{
            connproxy: Arc::downgrade(&_proxy_clone),
            ev: _ev
        })
    }

    pub fn poll(_handle: Arc<StdMutex<ConnCallbackMsgHandle>>, py: Python<'_>, py_handle: Py<PyAny>) -> bool {
        let mut _self = _handle.as_ref().lock().unwrap();
        let _handle_clone = _handle.clone();
        let opt_ev_data = _self.queue.deque();
        let ev_data = match opt_ev_data {
            None => return false,
            Some(ev_data) => ev_data
        };
        
        match (*ev_data).ev {
            // hub msg handle
            HubService::RegServer(ev) => {
                if let Some(conn_proxy) = ev_data.connproxy.upgrade() {
                    let mut _hub_msg_handle_c = _self.hub_msg_handle.clone();
                    let ev_tmp = ev.clone();
                    let rt: tokio::runtime::Runtime = tokio::runtime::Runtime::new().unwrap();
                    rt.block_on(async move {
                        {
                            let mut _conn_proxy = conn_proxy.as_ref().lock().await;

                            let svr_name = ev.name.clone().unwrap();
                            let svr_type = ev.type_.clone().unwrap();
                            
                            let cb_msg = RegServerCallback::new(_self.hub_name.clone());

                            if svr_type == "hub" {
                                let _proxy = Arc::new(Mutex::new(
                                    HubProxy::new(_conn_proxy.wr.clone())
                                ));
    
                                let mut _proxy_tmp = _proxy.as_ref().lock().await;
                                _proxy_tmp.hub_name = Some(svr_name.clone());
                                
                                let mut _conn_mgr = _self.conn_mgr.as_ref().lock().await;
                                _conn_mgr.add_hub_proxy(svr_name.clone(), _proxy.clone()).await;
                                _conn_proxy.hubproxy = Some(_proxy.clone());
    
                                _proxy_tmp.send_hub_msg(HubService::RegServerCallback(cb_msg)).await;
                            } else if svr_type == "gate" {
                                let _gate_name_tmp = svr_name.clone();
                                let mut _gate_tmp = GateProxy::new(_conn_proxy.wr.clone());
                            
                                _gate_tmp.gate_name = Some(svr_name.clone());
                                _gate_tmp.send_gate_msg(GateHubService::RegServerCallback(cb_msg)).await;
                
                                let _gateproxy = Arc::new(Mutex::new(_gate_tmp));
                                let mut _conn_mgr = _self.conn_mgr.as_ref().lock().await;
                                _conn_mgr.add_gate_proxy(_gate_name_tmp, _gateproxy).await;
                            }
                        }
                    });
                    let mut _hub_msg_handle = _hub_msg_handle_c.as_ref().lock().unwrap();
                    _hub_msg_handle.do_reg_hub(py, py_handle, ev_tmp);
                }
                else {
                    error!("hub reg hub conn_proxy is destory!");
                }
            },
            HubService::RegServerCallback(ev) => {
                let rt: tokio::runtime::Runtime = tokio::runtime::Runtime::new().unwrap();
                rt.block_on(async move {
                    let mut _conn_mgr = _self.conn_mgr.as_ref().lock().await;
                    let lock_key = create_lock_key(_self.hub_name.clone(), ev.name.clone().unwrap());
                    let value = _conn_mgr.remove_lock(lock_key.clone());
                    let _redis_service = _self.redis_service.clone().unwrap();
                    let mut _service = _redis_service.as_ref().lock().await;
                    let _ = _service.release_lock(lock_key, value);
                });
            },
            HubService::QueryEntity(ev) => {
                if let Some(conn_proxy) = ev_data.connproxy.upgrade() {
                    let rt: tokio::runtime::Runtime = tokio::runtime::Runtime::new().unwrap();
                    let hub_name = rt.block_on(async move {
                        {
                            let mut _conn_proxy = conn_proxy.as_ref().lock().await;
                            
                            if let Some(_hub_proxy) = _conn_proxy.hubproxy.clone() {
                                let _proxy_tmp = _hub_proxy.as_ref().lock().await;
                                return _proxy_tmp.hub_name.clone().unwrap();
                            }
                            else {
                                error!("HubService::QueryEntity! wrong msg handle!");
                            }
                        }
                        return "".to_string();
                    });

                    let mut _hub_msg_handle = _self.hub_msg_handle.as_ref().lock().unwrap();
                    _hub_msg_handle.do_query_service_entity(py, py_handle, hub_name, ev)
                }
                else {
                    error!("hub query entity conn_proxy is destory!");
                }
            },
            HubService::CreateServiceEntity(ev) => {
                if let Some(conn_proxy) = ev_data.connproxy.upgrade() {
                    let rt: tokio::runtime::Runtime = tokio::runtime::Runtime::new().unwrap();
                    let hub_name = rt.block_on(async move {
                        {
                            let mut _conn_proxy = conn_proxy.as_ref().lock().await;

                            if let Some(_hub_proxy) = _conn_proxy.hubproxy.clone() {
                                let _proxy_tmp = _hub_proxy.as_ref().lock().await;
                                return _proxy_tmp.hub_name.clone().unwrap();
                            }
                            else {
                                error!("HubService::CreateServiceEntity! wrong msg handle!");
                            }
                        }
                        return "".to_string();
                    });

                    let mut _hub_msg_handle = _self.hub_msg_handle.as_ref().lock().unwrap();
                    _hub_msg_handle.do_create_service_entity(py, py_handle, hub_name, ev)
                }
                else {
                    error!("hub create service entity conn_proxy is destory!");
                }
            },
            HubService::HubForwardClientRequestService(ev) => {
                if let Some(conn_proxy) = ev_data.connproxy.upgrade() {
                    let mut _hub_msg_handle_c = _self.hub_msg_handle.clone();
                    let ev_tmp = ev.clone();
                    let rt: tokio::runtime::Runtime = tokio::runtime::Runtime::new().unwrap();
                    let hub_name = rt.block_on(async move {
                        let mut hub_name: String = "".to_string();
                        {
                            let mut _conn_proxy = conn_proxy.as_ref().lock().await;

                            if let Some(_hub_proxy) = _conn_proxy.hubproxy.clone() {
                                let _proxy_tmp = _hub_proxy.as_ref().lock().await;
                                hub_name = _proxy_tmp.hub_name.clone().unwrap();

                                let _gate_name = ev.gate_name.clone().unwrap();
                                let _gate_host = ev.gate_host.clone().unwrap(); 

                                let mut _conn_mgr = _self.conn_mgr.as_ref().lock().await;
                                let _redis_service = _self.redis_service.clone().unwrap();
                                let mut _service = _redis_service.as_ref().lock().await;
                                let _lock_key = create_lock_key(_gate_name.clone(), _conn_mgr.get_hub_name());

                                let _close = _self.close.clone();

                                let value = _service.acquire_lock(_lock_key.clone(), 3).await;
                                if _conn_mgr.get_gate_proxy(&_gate_name).is_none() {
                                    _conn_mgr.add_lock(_lock_key, value);

                                    if let Some(_wr_arc) = _conn_mgr.direct_connect_server(
                                        _gate_name.clone(), 
                                        _gate_host.clone(), 
                                        _handle_clone.clone(), 
                                        _close).await
                                    {
                                        let _wr_arc_clone = _wr_arc.clone();
                                        
                                        let _gate_name_tmp = _gate_name.clone();
                                        let mut _gate_tmp = GateProxy::new(_wr_arc_clone);
                                        _gate_tmp.send_gate_msg(GateHubService::RegServer(RegServer::new(_conn_mgr.get_hub_name(), "hub".to_string()))).await;
                                    
                                        _gate_tmp.gate_name = Some(_gate_name);
                                        _gate_tmp.gate_host = Some(_gate_host);

                                        let _gateproxy = Arc::new(Mutex::new(_gate_tmp));
                                        _conn_mgr.add_gate_proxy(_gate_name_tmp, _gateproxy.clone()).await;
                                        _conn_proxy.gateproxy = Some(_gateproxy.clone());
                                    }
                                }
                                else {
                                    let _ = _service.release_lock(_lock_key, value).await;
                                }
                            }
                            else {
                                error!("HubService::HubForwardClientRequestService! wrong msg handle!");
                            }
                        }
                        return hub_name;
                    });

                    let mut _hub_msg_handle = _hub_msg_handle_c.as_ref().lock().unwrap();
                    _hub_msg_handle.do_forward_client_request_service(py, py_handle, hub_name, ev_tmp);
                }
                else {
                    error!("hub forward client request service conn_proxy is destory!");
                }
            },
            HubService::HubForwardClientRequestServiceExt(ev) => {
                if let Some(conn_proxy) = ev_data.connproxy.upgrade() {
                    let mut _hub_msg_handle_c = _self.hub_msg_handle.clone();
                    let ev_tmp = ev.clone();
                    let rt: tokio::runtime::Runtime = tokio::runtime::Runtime::new().unwrap();
                    let hub_name = rt.block_on(async move {
                        let mut hub_name: String = "".to_string();
                        {
                            let mut _conn_proxy = conn_proxy.as_ref().lock().await;

                            if let Some(_hub_proxy) = _conn_proxy.hubproxy.clone() {
                                let _proxy_tmp = _hub_proxy.as_ref().lock().await;
                                hub_name = _proxy_tmp.hub_name.clone().unwrap();
                                
                                for info in ev.request_infos.unwrap() {
                                    let _gate_name = info.gate_name.clone().unwrap();
                                    let _gate_host = info.gate_host.clone().unwrap(); 

                                    let mut _conn_mgr = _self.conn_mgr.as_ref().lock().await;
                                    let _redis_service = _self.redis_service.clone().unwrap();
                                    let mut _service = _redis_service.as_ref().lock().await;
                                    let _lock_key = create_lock_key(_gate_name.clone(), _conn_mgr.get_hub_name());

                                    let _close = _self.close.clone();

                                    let value = _service.acquire_lock(_lock_key.clone(), 3).await;
                                    if _conn_mgr.get_gate_proxy(&_gate_name).is_none() {
                                        _conn_mgr.add_lock(_lock_key, value);

                                        if let Some(_wr_arc) = _conn_mgr.direct_connect_server(
                                            _gate_name.clone(), 
                                            _gate_host.clone(), 
                                            _handle_clone.clone(), 
                                            _close).await
                                        {
                                            let _wr_arc_clone = _wr_arc.clone();
                                            
                                            let _gate_name_tmp = _gate_name.clone();
                                            let mut _gate_tmp = GateProxy::new(_wr_arc_clone);
                                            _gate_tmp.send_gate_msg(GateHubService::RegServer(RegServer::new(_conn_mgr.get_hub_name(), "hub".to_string()))).await;
                                        
                                            _gate_tmp.gate_name = Some(_gate_name);
                                            _gate_tmp.gate_host = Some(_gate_host);

                                            let _gateproxy = Arc::new(Mutex::new(_gate_tmp));
                                            _conn_mgr.add_gate_proxy(_gate_name_tmp, _gateproxy.clone()).await;
                                            _conn_proxy.gateproxy = Some(_gateproxy.clone());
                                        }
                                    }
                                    else {
                                        let _ = _service.release_lock(_lock_key, value).await;
                                    }
                                }
                            }
                            else {
                                error!("HubService::HubForwardClientRequestService! wrong msg handle!");
                            }
                        }
                        return hub_name;
                    });

                    let mut _hub_msg_handle = _hub_msg_handle_c.as_ref().lock().unwrap();
                    _hub_msg_handle.do_forward_client_request_service_ext(py, py_handle, hub_name, ev_tmp);
                }
                else {
                    error!("hub forward client request service conn_proxy is destory!");
                }
            },
            HubService::HubCallRpc(ev) => {
                if let Some(conn_proxy) = ev_data.connproxy.upgrade() {
                    let rt: tokio::runtime::Runtime = tokio::runtime::Runtime::new().unwrap();
                    let hub_name = rt.block_on(async move {
                        let mut hub_name: String = "".to_string();
                        {
                            let mut _conn_proxy = conn_proxy.as_ref().lock().await;

                            if let Some(_hub_proxy) = _conn_proxy.hubproxy.clone() {
                                let _proxy_tmp = _hub_proxy.as_ref().lock().await;
                                hub_name = _proxy_tmp.hub_name.clone().unwrap();
                            }
                            else {
                                error!("HubService::HubCallHubRpc! wrong msg handle!");
                            }
                        }
                        return hub_name;
                    });

                    let mut _hub_msg_handle = _self.hub_msg_handle.as_ref().lock().unwrap();
                    _hub_msg_handle.do_call_hub_rpc(py, py_handle, hub_name, ev)
                }
                else {
                    error!("hub call rpc conn_proxy is destory!");
                }
            },
            HubService::HubCallRsp(ev) => {
                if let Some(conn_proxy) = ev_data.connproxy.upgrade() {
                    let rt: tokio::runtime::Runtime = tokio::runtime::Runtime::new().unwrap();
                    let hub_name = rt.block_on(async move {
                        let mut hub_name: String = "".to_string();
                        {
                            let mut _conn_proxy = conn_proxy.as_ref().lock().await;

                            if let Some(_hub_proxy) = _conn_proxy.hubproxy.clone() {
                                let _proxy_tmp = _hub_proxy.as_ref().lock().await;
                                hub_name = _proxy_tmp.hub_name.clone().unwrap();
                            }
                            else {
                                error!("HubService::HubCallRsp! wrong msg handle!");
                            }
                        }
                        return hub_name;
                    });

                    let mut _hub_msg_handle = _self.hub_msg_handle.as_ref().lock().unwrap();
                    _hub_msg_handle.do_call_hub_rsp(py, py_handle, hub_name, ev)
                }
                else {
                    error!("hub call rsp conn_proxy is destory!");
                }
            },
            HubService::HubCallErr(ev) => {
                if let Some(conn_proxy) = ev_data.connproxy.upgrade() {
                    let rt: tokio::runtime::Runtime = tokio::runtime::Runtime::new().unwrap();
                    let hub_name = rt.block_on(async move {
                        let mut hub_name: String = "".to_string();
                        {
                            let mut _conn_proxy = conn_proxy.as_ref().lock().await;
                            
                            if let Some(_hub_proxy) = _conn_proxy.hubproxy.clone() {
                                let _proxy_tmp = _hub_proxy.as_ref().lock().await;
                                hub_name = _proxy_tmp.hub_name.clone().unwrap();
                            }
                            else {
                                error!("HubService::HubCallErr! wrong msg handle!");
                            }
                        }
                        return hub_name;
                    });
                    
                    let mut _hub_msg_handle = _self.hub_msg_handle.as_ref().lock().unwrap();
                    _hub_msg_handle.do_call_hub_err(py, py_handle, hub_name, ev)
                }
                else {
                    error!("hub call err conn_proxy is destory!");
                }
            } 
            HubService::HubCallNtf(ev) => {
                if let Some(conn_proxy) = ev_data.connproxy.upgrade() {
                    let rt: tokio::runtime::Runtime = tokio::runtime::Runtime::new().unwrap();
                    let hub_name = rt.block_on(async move {
                        let mut hub_name: String = "".to_string();
                        {
                            let mut _conn_proxy = conn_proxy.as_ref().lock().await;

                            if let Some(_hub_proxy) = _conn_proxy.hubproxy.clone() {
                                let _proxy_tmp = _hub_proxy.as_ref().lock().await;
                                hub_name = _proxy_tmp.hub_name.clone().unwrap();
                            }
                            else {
                                error!("HubService::HubCallNtf! wrong msg handle!");
                            }
                        }
                        return hub_name;
                    });
                    
                    let mut _hub_msg_handle = _self.hub_msg_handle.as_ref().lock().unwrap();
                    _hub_msg_handle.do_call_hub_ntf(py, py_handle, hub_name, ev)
                }
                else {
                    error!("hub call ntf conn_proxy is destory!");
                }
            },
            HubService::WaitMigrateEntity(ev) => {
                if let Some(conn_proxy) = ev_data.connproxy.upgrade() {
                    let rt: tokio::runtime::Runtime = tokio::runtime::Runtime::new().unwrap();
                    let hub_name = rt.block_on(async move {
                        let mut hub_name: String = "".to_string();
                        {
                            let mut _conn_proxy = conn_proxy.as_ref().lock().await;

                            if let Some(_hub_proxy) = _conn_proxy.hubproxy.clone() {
                                let _proxy_tmp = _hub_proxy.as_ref().lock().await;
                                hub_name = _proxy_tmp.hub_name.clone().unwrap();
                            }
                            else {
                                error!("HubService::WaitMigrateEntity! wrong msg handle!");
                            }
                        }
                        return hub_name;
                    });
                    
                    let mut _hub_msg_handle = _self.hub_msg_handle.as_ref().lock().unwrap();
                    _hub_msg_handle.do_wait_migrate_entity(py, py_handle, hub_name, ev)
                }
                else {
                    error!("wait migrate entity conn_proxy is destory!");
                }
            },
            HubService::MigrateEntity(ev) => {
                if let Some(conn_proxy) = ev_data.connproxy.upgrade() {
                    let rt: tokio::runtime::Runtime = tokio::runtime::Runtime::new().unwrap();
                    let hub_name = rt.block_on(async move {
                        let mut hub_name: String = "".to_string();
                        {
                            let mut _conn_proxy = conn_proxy.as_ref().lock().await;

                            if let Some(_hub_proxy) = _conn_proxy.hubproxy.clone() {
                                let _proxy_tmp = _hub_proxy.as_ref().lock().await;
                                hub_name = _proxy_tmp.hub_name.clone().unwrap();
                            }
                            else {
                                error!("HubService::WaitMigrateEntity! wrong msg handle!");
                            }
                        }
                        return hub_name;
                    });
                    
                    let mut _hub_msg_handle = _self.hub_msg_handle.as_ref().lock().unwrap();
                    _hub_msg_handle.do_migrate_entity(py, py_handle, hub_name, ev)
                }
                else {
                    error!("migrate entity conn_proxy is destory!");
                }
            },
            HubService::CreateMigrateEntity(ev) => {
                if let Some(conn_proxy) = ev_data.connproxy.upgrade() {
                    let mut _hub_msg_handle = _self.hub_msg_handle.as_ref().lock().unwrap();
                    let rt: tokio::runtime::Runtime = tokio::runtime::Runtime::new().unwrap();
                    rt.block_on(async move {
                        let mut _conn_proxy = conn_proxy.as_ref().lock().await;

                        if let Some(_hub_proxy) = _conn_proxy.hubproxy.clone() {
                            let _proxy_tmp = _hub_proxy.as_ref().lock().await;
                            let hub_name = _proxy_tmp.hub_name.clone().unwrap();
                            _hub_msg_handle.do_create_migrate_entity(py, py_handle, hub_name, ev)
                        }
                        else {
                            error!("HubService::WaitMigrateEntity! wrong msg handle!");
                        }
                    });
                }
                else {
                    error!("migrate entity complete conn_proxy is destory!");
                }
            },
            HubService::MigrateEntityComplete(ev) => {
                if let Some(conn_proxy) = ev_data.connproxy.upgrade() {
                    let rt: tokio::runtime::Runtime = tokio::runtime::Runtime::new().unwrap();
                    let hub_name = rt.block_on(async move {
                        let mut hub_name: String = "".to_string();
                        {
                            let mut _conn_proxy = conn_proxy.as_ref().lock().await;

                            if let Some(_hub_proxy) = _conn_proxy.hubproxy.clone() {
                                let _proxy_tmp = _hub_proxy.as_ref().lock().await;
                                hub_name = _proxy_tmp.hub_name.clone().unwrap();
                            }
                            else {
                                error!("HubService::WaitMigrateEntity! wrong msg handle!");
                            }
                        }
                        return hub_name;
                    });
                    
                    let mut _hub_msg_handle = _self.hub_msg_handle.as_ref().lock().unwrap();
                    _hub_msg_handle.do_migrate_entity_complete(py, py_handle, hub_name, ev)
                }
                else {
                    error!("migrate entity complete conn_proxy is destory!");
                }
            },

            // gate msg handle
            HubService::ClientRequestLogin(ev) => {
                if let Some(conn_proxy) = ev_data.connproxy.upgrade() {
                    let _gate_msg_handle_c = _self.gate_msg_handle.clone();
                    let ev_tmp = ev.clone();
                    let rt: tokio::runtime::Runtime = tokio::runtime::Runtime::new().unwrap();
                    rt.block_on(async move {
                        {
                            let mut _conn_proxy = conn_proxy.as_ref().lock().await;

                            let _gate_name = ev.gate_name.clone().unwrap();
                            let _proxy = Arc::new(Mutex::new(
                                GateProxy::new(_conn_proxy.wr.clone())
                            ));

                            let mut _conn_mgr = _self.conn_mgr.as_ref().lock().await;
                            _conn_mgr.add_gate_proxy(_gate_name.clone(), _proxy.clone()).await;
                            _conn_proxy.gateproxy = Some(_proxy.clone());

                            let mut _proxy_tmp = _proxy.as_ref().lock().await;
                            _proxy_tmp.gate_name = Some(_gate_name.clone());
                            _proxy_tmp.gate_host = Some(ev.clone().gate_host.unwrap());

                            let cb_msg = RegServerCallback::new(_self.hub_name.clone());
                            let msg = GateHubService::RegServerCallback(cb_msg);
                            _proxy_tmp.send_gate_msg(msg).await;
                        }
                    });

                    let mut _gate_msg_handle = _gate_msg_handle_c.as_ref().lock().unwrap();
                    _gate_msg_handle.do_client_request_login(py, py_handle, ev_tmp);
                }
                else {
                    error!("gate client request login conn_proxy is destory!");
                }
            },
            HubService::ClientRequestReconnect(ev) => {
                if let Some(conn_proxy) = ev_data.connproxy.upgrade() {
                    let _gate_msg_handle_c = _self.gate_msg_handle.clone();
                    let _ev_tmp = ev.clone();
                    let rt: tokio::runtime::Runtime = tokio::runtime::Runtime::new().unwrap();
                    rt.block_on(async move {
                        {
                            let mut _conn_proxy = conn_proxy.as_ref().lock().await;

                            let _gate_name = ev.gate_name.clone().unwrap();

                            let _proxy = Arc::new(Mutex::new(
                                GateProxy::new(_conn_proxy.wr.clone())
                            ));
                            let mut _proxy_tmp = _proxy.as_ref().lock().await;

                            _proxy_tmp.gate_name = Some(_gate_name.clone());
                            _proxy_tmp.gate_host = ev.gate_host;

                            let mut _conn_mgr = _self.conn_mgr.as_ref().lock().await;
                            _conn_mgr.add_gate_proxy(_gate_name.clone(), _proxy.clone()).await;
                            _conn_proxy.gateproxy = Some(_proxy.clone());

                            let cb_msg = RegServerCallback::new(_self.hub_name.clone());
                            let msg = GateHubService::RegServerCallback(cb_msg);
                            _proxy_tmp.send_gate_msg(msg).await;
                        }
                    });

                    let mut _gate_msg_handle = _gate_msg_handle_c.as_ref().lock().unwrap();
                    _gate_msg_handle.do_client_request_reconnect(py, py_handle, _ev_tmp);
                }
                else {
                    error!("gate client request reconnect conn_proxy is destory!");
                }
            },
            HubService::TransferMsgEnd(ev) => {
                let mut _gate_msg_handle_c = _self.gate_msg_handle.as_ref().lock().unwrap();
                _gate_msg_handle_c.do_transfer_msg_end(py, py_handle, ev);

            },
            HubService::TransferEntityControl(ev) => {
                let conn_id = ev.conn_id.clone();
                let entity_id = ev.entity_id.clone();

                let mut _gate_msg_handle = _self.gate_msg_handle.as_ref().lock().unwrap();
                _gate_msg_handle.do_transfer_entity_control(py, py_handle, ev);

                if let Some(conn_proxy) = ev_data.connproxy.upgrade() {
                    let rt: tokio::runtime::Runtime = tokio::runtime::Runtime::new().unwrap();
                    _ = rt.block_on(async move {
                        let mut _conn_proxy = conn_proxy.as_ref().lock().await;

                        if let Some(_gate_proxy) = &_conn_proxy.gateproxy {
                            let mut _proxy_tmp = _gate_proxy.as_ref().lock().await;
                            _proxy_tmp.send_gate_msg(GateHubService::TransferComplete(HubCallTransferEntityComplete::new(conn_id.unwrap(), entity_id.unwrap()))).await;
                        }
                        else {
                            error!("HubService::TransferEntityControl! wrong msg handle!");
                        }
                    });
                }
                else {
                    error!("gate client Transfer Entity Control conn_proxy is destory!");
                }
            }
            HubService::KickOffClient(ev) => {
                if let Some(conn_proxy) = ev_data.connproxy.upgrade() {
                    let rt: tokio::runtime::Runtime = tokio::runtime::Runtime::new().unwrap();
                    let gate_name = rt.block_on(async move {
                        let mut gate_name: String = "".to_string();
                        {
                            let mut _conn_proxy = conn_proxy.as_ref().lock().await;

                            if let Some(_gate_proxy) = _conn_proxy.gateproxy.clone() {
                                let _proxy_tmp = _gate_proxy.as_ref().lock().await;
                                gate_name = _proxy_tmp.gate_name.clone().unwrap();
                            }
                            else {
                                error!("HubService::KickOffClient! wrong msg handle!");
                            }
                        }
                        return gate_name;
                    });
                    let mut _gate_msg_handle = _self.gate_msg_handle.as_ref().lock().unwrap();
                    _gate_msg_handle.do_client_kick_off(py, py_handle, gate_name, ev);
                }
                else {
                    error!("gate client kick off conn_proxy is destory!");
                }
            },
            HubService::ClientDisconnnect(ev) => {
                if let Some(conn_proxy) = ev_data.connproxy.upgrade() {
                    let rt: tokio::runtime::Runtime = tokio::runtime::Runtime::new().unwrap();
                    let gate_name = rt.block_on(async move {
                        let mut gate_name: String = "".to_string();
                        {
                            let mut _conn_proxy = conn_proxy.as_ref().lock().await;

                            if let Some(_gate_proxy) = _conn_proxy.gateproxy.clone() {
                                let _proxy_tmp = _gate_proxy.as_ref().lock().await;
                                gate_name = _proxy_tmp.gate_name.clone().unwrap();
                            }
                            else {
                                error!("HubService::ClientDisconnnect! wrong msg handle!");
                            }
                        }
                        return gate_name;
                    });
                    let mut _gate_msg_handle = _self.gate_msg_handle.as_ref().lock().unwrap();
                    _gate_msg_handle.do_client_disconnnect(py, py_handle, gate_name, ev);
                }
                else {
                    error!("gate client disconnect conn_proxy is destory!");
                }
            },
            HubService::ClientRequestService(ev) => {
                if let Some(conn_proxy) = ev_data.connproxy.upgrade() {
                    let _gate_msg_handle_c = _self.gate_msg_handle.clone();
                    let _ev_tmp = ev.clone();
                    let rt: tokio::runtime::Runtime = tokio::runtime::Runtime::new().unwrap();
                    let gate_name = rt.block_on(async move {
                        let gate_name: String;
                        {
                            let mut _conn_proxy = conn_proxy.as_ref().lock().await;

                            if let Some(_gate_proxy) = _conn_proxy.gateproxy.clone() {
                                let _proxy_tmp = _gate_proxy.as_ref().lock().await;
                                gate_name = _proxy_tmp.gate_name.clone().unwrap();
                            }
                            else {
                                gate_name = ev.gate_name.unwrap();
                                let _proxy = Arc::new(Mutex::new(
                                    GateProxy::new(_conn_proxy.wr.clone())
                                ));
                                let mut _proxy_tmp = _proxy.as_ref().lock().await;
    
                                _proxy_tmp.gate_name = Some(gate_name.clone());
                                _proxy_tmp.gate_host = ev.gate_host;
    
                                let mut _conn_mgr = _self.conn_mgr.as_ref().lock().await;
                                _conn_mgr.add_gate_proxy(gate_name.clone(), _proxy.clone()).await;
                                _conn_proxy.gateproxy = Some(_proxy.clone());
    
                                let cb_msg = RegServerCallback::new(_self.hub_name.clone());
                                let msg = GateHubService::RegServerCallback(cb_msg);
                                _proxy_tmp.send_gate_msg(msg).await;

                                warn!("HubService::ClientRequestService! wrong msg handle!");
                            }
                        }
                        return gate_name;
                    });
                    let mut _gate_msg_handle = _gate_msg_handle_c.as_ref().lock().unwrap();
                    _gate_msg_handle.do_client_request_service(py, py_handle, gate_name, _ev_tmp);
                }
                else {
                    error!("gate client request service conn_proxy is destory!");
                }
            }
            HubService::ClientCallRpc(ev) => {
                if let Some(conn_proxy) = ev_data.connproxy.upgrade() {
                    let rt: tokio::runtime::Runtime = tokio::runtime::Runtime::new().unwrap();
                    let gate_name = rt.block_on(async move {
                        let mut gate_name: String = "".to_string();
                        {
                            let mut _conn_proxy = conn_proxy.as_ref().lock().await;

                            if let Some(_gate_proxy) = _conn_proxy.gateproxy.clone() {
                                let _proxy_tmp = _gate_proxy.as_ref().lock().await;
                                gate_name = _proxy_tmp.gate_name.clone().unwrap();
                            }
                            else {
                                error!("HubService::ClientCallRpc! wrong msg handle!")
                            }
                        }
                        return gate_name;
                    });
                    let mut _gate_msg_handle = _self.gate_msg_handle.as_ref().lock().unwrap();
                    _gate_msg_handle.do_client_call_rpc(py, py_handle, gate_name, ev)
                }
                else {
                    error!("gate client call rpc conn_proxy is destory!");
                }
            },
            HubService::ClientCallRsp(ev) => {
                if let Some(conn_proxy) = ev_data.connproxy.upgrade() {
                    let rt: tokio::runtime::Runtime = tokio::runtime::Runtime::new().unwrap();
                    let gate_name = rt.block_on(async move {
                        let mut gate_name: String = "".to_string();
                        {
                            let mut _conn_proxy = conn_proxy.as_ref().lock().await;

                            if let Some(_gate_proxy) = _conn_proxy.gateproxy.clone() {
                                let _proxy_tmp = _gate_proxy.as_ref().lock().await;
                                gate_name = _proxy_tmp.gate_name.clone().unwrap();
                            }
                            else {
                                error!("HubService::ClientCallRsp! wrong msg handle!")
                            }
                        }
                        return gate_name;
                    });
                    let mut _gate_msg_handle = _self.gate_msg_handle.as_ref().lock().unwrap();
                    _gate_msg_handle.do_client_call_rsp(py, py_handle, gate_name, ev)
                }
                else {
                    error!("gate client call rsp conn_proxy is destory!");
                }
            },
            HubService::ClientCallErr(ev) => {
                if let Some(conn_proxy) = ev_data.connproxy.upgrade() {
                    let rt: tokio::runtime::Runtime = tokio::runtime::Runtime::new().unwrap();
                    let gate_name = rt.block_on(async move {
                        let mut gate_name: String = "".to_string();
                        {
                            let mut _conn_proxy = conn_proxy.as_ref().lock().await;

                            if let Some(_gate_proxy) = _conn_proxy.gateproxy.clone() {
                                let _proxy_tmp = _gate_proxy.as_ref().lock().await;
                                gate_name = _proxy_tmp.gate_name.clone().unwrap();
                            }
                            else {
                                error!("HubService::ClientCallErr! wrong msg handle!")
                            }
                        }
                        return gate_name;
                    });
                    let mut _gate_msg_handle = _self.gate_msg_handle.as_ref().lock().unwrap();
                    _gate_msg_handle.do_client_call_err(py, py_handle, gate_name, ev)
                }
                else {
                    error!("gate client call err conn_proxy is destory!");
                }
            },
            HubService::ClientCallNtf(ev) => {
                if let Some(conn_proxy) = ev_data.connproxy.upgrade() {
                    let rt: tokio::runtime::Runtime = tokio::runtime::Runtime::new().unwrap();
                    let gate_name = rt.block_on(async move {
                        let mut gate_name: String = "".to_string();
                        {
                            let mut _conn_proxy = conn_proxy.as_ref().lock().await;

                            if let Some(_gate_proxy) = _conn_proxy.gateproxy.clone() {
                                let _proxy_tmp = _gate_proxy.as_ref().lock().await;
                                gate_name = _proxy_tmp.gate_name.clone().unwrap();
                            }
                            else {
                                error!("HubService::ClientCallNtf! wrong msg handle!")
                            }
                        }
                        return gate_name;
                    });
                    let mut _gate_msg_handle = _self.gate_msg_handle.as_ref().lock().unwrap();
                    _gate_msg_handle.do_client_call_ntf(py, py_handle, gate_name, ev)
                }
                else {
                    error!("gate client call ntf conn_proxy is destory!");
                }
            },
        };
        
        return true;
    }
}

pub struct ConnProxyReaderCallback {
    connproxy: Arc<Mutex<ConnProxy>>
}

#[async_trait]
impl NetReaderCallback for ConnProxyReaderCallback {
    async fn cb(&mut self, data:Vec<u8>) {
        ConnCallbackMsgHandle::on_event(self.connproxy.clone(), data).await
    }
}

impl ConnProxyReaderCallback {
    pub fn new(_connproxy: Arc<Mutex<ConnProxy>>) -> ConnProxyReaderCallback {
        ConnProxyReaderCallback {
            connproxy: _connproxy
        }
    }
}

pub struct ConnProxyManager {
    conn_msg_handle: Arc<StdMutex<ConnCallbackMsgHandle>>, 
    join_list: Vec<JoinHandle<()>>
}

#[async_trait]
impl RedisMQListenCallback for ConnProxyManager {
    async fn redis_mq_cb(&mut self, rd: Arc<Mutex<RedisMQReader>>, wr: Arc<Mutex<Box<dyn NetWriter + Send + 'static>>>){
        let _connproxy = Arc::new(Mutex::new(ConnProxy::new(wr, self.conn_msg_handle.clone())));
        let mut _rd_ref = rd.as_ref().lock().await;
        self.join_list.push(_rd_ref.start(Arc::new(Mutex::new(Box::new(ConnProxyReaderCallback::new(_connproxy))))));
    }
}

#[async_trait]
impl TcpListenCallback for ConnProxyManager {
    async fn cb(&mut self, rd: TcpReader, wr: TcpWriter) {
        let _wr_arc: Arc<Mutex<Box<dyn NetWriter + Send + 'static>>> = Arc::new(Mutex::new(Box::new(wr)));
        let _connproxy = Arc::new(Mutex::new(ConnProxy::new(_wr_arc, self.conn_msg_handle.clone())));
        self.join_list.push(rd.start(Arc::new(Mutex::new(Box::new(ConnProxyReaderCallback::new(_connproxy))))));
    }
}

impl ConnProxyManager {
    pub fn new_tcp_callback(_conn_msg_handle: Arc<StdMutex<ConnCallbackMsgHandle>>) 
        -> Arc<Mutex<Box<dyn TcpListenCallback + Send + 'static>>> 
    {
        Arc::new(Mutex::new(Box::new(ConnProxyManager {
            conn_msg_handle: _conn_msg_handle,
            join_list: Vec::new()
        })))
    }

    pub fn new_redis_mq_callback(_conn_msg_handle: Arc<StdMutex<ConnCallbackMsgHandle>>) 
        -> Arc<Mutex<Box<dyn RedisMQListenCallback + Send + 'static>>> 
    {
        Arc::new(Mutex::new(Box::new(ConnProxyManager {
            conn_msg_handle: _conn_msg_handle,
            join_list: Vec::new()
        })))
    }
}