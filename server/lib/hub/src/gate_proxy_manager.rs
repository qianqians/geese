use std::sync::Arc;

use tokio::sync::Mutex;
use tracing::error;

use thrift::protocol::{TCompactOutputProtocol, TSerializable};
use thrift::transport::{TIoChannel, TBufferChannel};

use net::NetWriter;

use proto::gate::GateHubService;

pub struct GateProxy {
    pub gate_name: Option<String>,
    pub gate_host: Option<String>,
    pub wr: Arc<Mutex<Box<dyn NetWriter + Send + 'static>>>
}

impl GateProxy {
    pub fn new(_wr: Arc<Mutex<Box<dyn NetWriter + Send + 'static>>>) -> GateProxy 
    {
        GateProxy {
            gate_name: None,
            gate_host: None,
            wr: _wr
        }
    }

    pub async fn send_gate_msg(&mut self, msg: GateHubService) -> bool {
        let t = TBufferChannel::with_capacity(0, 16384);
        let (rd, wr) = match t.split() {
            Ok(_t) => (_t.0, _t.1),
            Err(_e) => {
                error!("do_get_guid t.split error {}", _e);
                return false;
            }
        };
        let mut o_prot = TCompactOutputProtocol::new(wr);
        let _ = GateHubService::write_to_out_protocol(&msg, &mut o_prot);
        let wr = self.wr.clone();
        let mut p_send = wr.as_ref().lock().await;
        p_send.send(&rd.write_bytes()).await
    }
}
