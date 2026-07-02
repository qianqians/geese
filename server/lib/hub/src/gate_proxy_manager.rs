use std::sync::Arc;

use tokio::sync::Mutex;
use tracing::error;

use thrift::protocol::{TCompactOutputProtocol, TSerializable};
use thrift::transport::{TIoChannel, TBufferChannel};

use net::NetWriter;

use proto::gate::GateHubService;

/// Thrift 序列化缓冲区容量（1MB）
const THRIFT_BUFFER_CAPACITY: usize = 1_048_576;

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
        let t = TBufferChannel::with_capacity(0, THRIFT_BUFFER_CAPACITY);
        let (rd, wr) = match t.split() {
            Ok(_t) => (_t.0, _t.1),
            Err(_e) => {
                error!("send_gate_msg t.split error {}", _e);
                return false;
            }
        };
        let mut o_prot = TCompactOutputProtocol::new(wr);
        if let Err(e) = GateHubService::write_to_out_protocol(&msg, &mut o_prot) {
            error!("Failed to serialize Thrift message in send_gate_msg: {}", e);
            return false;
        }
        let wr = self.wr.clone();
        let mut p_send = wr.as_ref().lock().await;
        p_send.send(&rd.write_bytes()).await
    }
}
