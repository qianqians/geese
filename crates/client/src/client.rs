use std::sync::{Arc, Weak};

pub type StdMutex<T> = std::sync::Mutex<T>;

use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use pyo3::prelude::*;
use pyo3::types::PyBytes;
use async_trait::async_trait;

use thrift::protocol::{TCompactOutputProtocol, TCompactInputProtocol, TSerializable};
use thrift::transport::{TIoChannel, TBufferChannel};

use net::{NetReaderCallback, NetReader, NetWriter};
use tcp::tcp_connect::TcpConnect;
use wss::wss_connect::WSSConnect;
use queue::Queue;

use proto::gate::GateClientService;
use proto::client::ClientService;

pub struct GateProxy {
    pub wr: Arc<Mutex<Box<dyn NetWriter + Send + 'static>>>,
    pub msg_handle: Arc<StdMutex<GateMsgHandle>>,
    pub conn_id: String,
    pub join: Option<JoinHandle<()>>
}

impl GateProxy {
    pub fn new(
        _wr: Arc<Mutex<Box<dyn NetWriter + Send + 'static>>>,
        _msg_handle: Arc<StdMutex<GateMsgHandle>>) -> Arc<StdMutex<GateProxy>> 
    {
        Arc::new(StdMutex::new(GateProxy { wr: _wr, msg_handle: _msg_handle, conn_id: "".to_string(), join: None }))
    }

    pub async fn send_msg(&mut self, msg: GateClientService) -> bool {
        let t = TBufferChannel::with_capacity(0, 16384);
        let (rd, wr) = match t.split() {
            Ok(_t) => (_t.0, _t.1),
            Err(_e) => {
                println!("do_get_guid t.split error {}", _e);
                return false;
            }
        };
        let mut o_prot = TCompactOutputProtocol::new(wr);
        let _ = GateClientService::write_to_out_protocol(&msg, &mut o_prot);
        let mut p_send = self.wr.as_ref().lock().await;
        p_send.send(&rd.write_bytes()).await
    }
}

pub struct GateProxyReaderCallback {
    gate_proxy: Arc<StdMutex<GateProxy>>
}

#[async_trait]
impl NetReaderCallback for GateProxyReaderCallback {
    async fn cb(&mut self, data:Vec<u8>) {
        GateMsgHandle::on_event(self.gate_proxy.clone(), data)
    }
}

impl GateProxyReaderCallback {
    pub fn new(_gate_proxy: Arc<StdMutex<GateProxy>>) -> GateProxyReaderCallback {
        GateProxyReaderCallback {
            gate_proxy: _gate_proxy
        }
    }
}

fn deserialize(data: Vec<u8>) -> Result<ClientService, Box<dyn std::error::Error>> {
    let mut t = TBufferChannel::with_capacity(data.len(), 0);
    let _ = t.set_readable_bytes(&data);
    let mut i_prot = TCompactInputProtocol::new(t);
    let ev_data = ClientService::read_from_in_protocol(&mut i_prot)?;
    Ok(ev_data)
}

pub struct ConnEvent {
    gate_proxy: Weak<StdMutex<GateProxy>>,
    ev: ClientService,
}

pub struct GateMsgHandle {
    queue: Queue<Box<ConnEvent>>
}

impl GateMsgHandle {
    pub fn new() -> Arc<StdMutex<GateMsgHandle>> {
        Arc::new(StdMutex::new(GateMsgHandle{
            queue: Queue::new()
        }))
    }

    fn enque_event(&mut self, ev: ConnEvent) {
        self.queue.enque(Box::new(ev))
    }

    pub fn on_event(_proxy: Arc<StdMutex<GateProxy>>, data: Vec<u8>) {
        let _ev = match deserialize(data) {
            Err(e) => {
                println!("GateClientMsgHandle do_event err:{}", e);
                return;
            }
            Ok(d) => d
        };

        let _proxy_clone = _proxy.clone();

        let _msg_handle:Arc<StdMutex<GateMsgHandle>>;
        {
            let mut _p = _proxy.as_ref().lock().unwrap();
            _msg_handle = _p.msg_handle.clone();
        }

        let mut _handle = _msg_handle.as_ref().lock().unwrap();
        _handle.enque_event(ConnEvent{
            gate_proxy: Arc::downgrade(&_proxy_clone),
            ev: _ev
        })
    }
    
    pub fn poll(_handle: Arc<StdMutex<GateMsgHandle>>, py: Python<'_>, py_handle: Py<PyAny>) -> bool {
        let ev_data: Box<ConnEvent>;
        {
            let mut _self = _handle.as_ref().lock().unwrap();
            let opt_ev_data = _self.queue.deque();
            ev_data = match opt_ev_data {
                None => return false,
                Some(ev_data) => ev_data
            };
        }
        let _proxy = match ev_data.gate_proxy.upgrade() {
            None => return false,
            Some(_p) => _p
        };
        let _ev = ev_data.ev;
        println!("GateMsgHandle poll event begin!");
        
        match _ev {
            ClientService::ConnId(ev) => {
                println!("GateMsgHandle ClientService ConnId begin!");

                let conn_id = ev.conn_id.unwrap();
                {
                    let mut _p_handle = _proxy.as_ref().lock().unwrap();
                    _p_handle.conn_id = conn_id.clone();
                }

                let argvs = (conn_id.clone(), );
                if let Err(e) = py_handle.call_method1(py, "on_conn_id", argvs) {
                    println!("on_conn_id python callback error:{}", e)
                }
            },
            ClientService::CreateRemoteEntity(ev) => {
                println!("GateMsgHandle ClientService CreateRemoteEntity begin!");
                
                let argvs = (ev.entity_type.unwrap(), ev.entity_id.unwrap(), PyBytes::new(py, &ev.argvs.unwrap()));
                if let Err(e) = py_handle.call_method1(py, "on_create_remote_entity", argvs) {
                    println!("on_create_remote_entity python callback error:{}", e)
                }
            },
            ClientService::DeleteRemoteEntity(ev) => {
                let argvs = (ev.entity_id.unwrap(),);
                if let Err(e) = py_handle.call_method1(py, "on_delete_remote_entity", argvs) {
                        println!("on_delete_remote_entity python callback error:{}", e)
                }
            },
            ClientService::RefreshEntity(ev) => {
                let argvs = (ev.entity_type.unwrap(), ev.entity_id.unwrap(), PyBytes::new(py, &ev.argvs.unwrap()));
                if let Err(e) = py_handle.call_method1(py, "on_refresh_entity", argvs) {
                    println!("on_create_remote_entity python callback error:{}", e)
                }
            },
            ClientService::KickOff(ev) => {
                let argvs = (ev.prompt_info.unwrap(),);
                if let Err(e) = py_handle.call_method1(py, "on_kick_off", argvs) {
                    println!("on_kick_off python callback error:{}", e)
                }
            },
            ClientService::TransferComplete(_) => {
                let argvs = ();
                if let Err(e) = py_handle.call_method1(py, "on_transfer_complete", argvs) {
                    println!("on_transfer_complete python callback error:{}", e)
                }
            }
            ClientService::CallRpc(ev) => {
                let msg = ev.message.unwrap();
                    let argvs = (
                        ev.hub_name.unwrap(),
                        ev.entity_id.unwrap(),
                        ev.msg_cb_id.unwrap(),
                        msg.method.unwrap(),
                        PyBytes::new(py, &msg.argvs.unwrap()));

                if let Err(e) = py_handle.call_method1(py, "on_call_rpc", argvs) {
                    println!("on_call_rpc python callback error:{}", e)
                }
            },
            ClientService::CallRsp(ev) => {
                let rsp = ev.rsp.unwrap();
                let argvs = (
                    rsp.entity_id.unwrap(),
                    rsp.msg_cb_id.unwrap(),
                    PyBytes::new(py, &rsp.argvs.unwrap()));

                if let Err(e) = py_handle.call_method1(py, "on_call_rsp", argvs) {
                    println!("on_call_rsp python callback error:{}", e)
                }    
            },
            ClientService::CallErr(ev) => {
                let err = ev.err.unwrap();
                let argvs = (
                    err.entity_id.unwrap(),
                    err.msg_cb_id.unwrap(),
                    PyBytes::new(py, &err.argvs.unwrap()));

                if let Err(e) = py_handle.call_method1(py, "on_call_err", argvs) {
                    println!("on_call_err python callback error:{}", e)
                }
            },
            ClientService::CallNtf(ev) => {
                let msg = ev.message.unwrap();
                let argvs = (
                    ev.hub_name.unwrap(),
                    ev.entity_id.unwrap(),
                    msg.method.unwrap(),
                    PyBytes::new(py, &msg.argvs.unwrap()));

                if let Err(e) = py_handle.call_method1(py, "on_call_ntf", argvs) {
                    println!("on_call_ntf python callback error:{}", e)
                }    
            },
            ClientService::CallGlobal(ev) => {
                let msg = ev.message.unwrap();
                let argvs = (
                    msg.method.unwrap(),
                    PyBytes::new(py, &msg.argvs.unwrap()));

                if let Err(e) = py_handle.call_method1(py, "on_call_global", argvs) {
                    println!("on_call_global python callback error:{}", e)
                }
            },
            ClientService::Heartbeats(_) => {
                let mut _p_handle = _proxy.as_ref().lock().unwrap();
                println!("ClientService::Heartbeats, conn_id:{}", _p_handle.conn_id);
            },
        }
        
        return true
    }
}

pub struct Context {
    gate_proxy: Option<Arc<StdMutex<GateProxy>>>,
    msg_handle: Arc<StdMutex<GateMsgHandle>>,
    net_rt: Arc<StdMutex<tokio::runtime::Runtime>>
}

impl Context {
    pub fn new() -> Context {
        Context {
            gate_proxy: None,
            msg_handle: GateMsgHandle::new(),
            net_rt: Arc::new(StdMutex::new(tokio::runtime::Runtime::new().unwrap()))
        }
    }

    pub fn connect_tcp(&mut self, addr: String, port: u16) {
        let rt_clone = self.net_rt.clone();
        let rt = rt_clone.as_ref().lock().unwrap();
        rt.block_on(async move {
            print!("connect_tcp addr:{}, port:{}", addr, port);
            if let Ok((rd, wr)) = TcpConnect::connect(format!("{}:{}", addr, port)).await {
                print!("connect_tcp addr:{}, port:{} success!", addr, port);

                let _wr_arc: Arc<Mutex<Box<dyn NetWriter + Send + 'static>>> = Arc::new(Mutex::new(Box::new(wr)));

                let _gate_proxy = GateProxy::new(_wr_arc.clone(), self.msg_handle.clone());
                self.gate_proxy = Some(_gate_proxy.clone());

                let _ = rd.start(Arc::new(Mutex::new(Box::new(
                    GateProxyReaderCallback::new(_gate_proxy)))));
            }
            else {
                println!("connect_tcp faild! host:{}", format!("{}:{}", addr, port));
            }
        });
    }

    pub fn connect_ws(&mut self, host: String) {
        let rt_clone = self.net_rt.clone();
        let rt = rt_clone.as_ref().lock().unwrap();
        rt.block_on(async move {
            if let Ok((rd, wr)) = WSSConnect::connect(host.clone()).await {
                let _wr_arc: Arc<Mutex<Box<dyn NetWriter + Send + 'static>>> = Arc::new(Mutex::new(Box::new(wr)));

                let _gate_proxy = GateProxy::new(_wr_arc.clone(), self.msg_handle.clone());
                self.gate_proxy = Some(_gate_proxy.clone());

                let _ = rd.start(Arc::new(Mutex::new(Box::new(
                    GateProxyReaderCallback::new(_gate_proxy)))));
            }
            else {
                println!("connect_ws faild! host:{}", host);
            }
        });
    }

    pub fn send_msg(&mut self, msg: GateClientService) -> bool {
        let proxy = match &self.gate_proxy {
            None => return false,
            Some(p) => p,
        };
        let mut p_send = proxy.as_ref().lock().unwrap();
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async move {
            p_send.send_msg(msg).await
        })
    }

    pub fn get_msg_handle(&self) -> Arc<StdMutex<GateMsgHandle>> {
        self.msg_handle.clone()
    }

}