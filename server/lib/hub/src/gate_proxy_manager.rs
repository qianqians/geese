use std::sync::Arc;

use tokio::sync::Mutex;
use tracing::error;

use thrift::protocol::{TCompactOutputProtocol, TSerializable};

use net::NetWriter;

use proto::gate::GateHubService;


pub struct GateProxy {
    pub gate_name: Option<String>,
    pub gate_host: Option<String>,
    pub wr: Arc<Mutex<Box<dyn NetWriter + Send + 'static>>>,
    /// 序列化用的可复用缓冲（自动扩容，见 net::vec_transport）
    tx_buf: Arc<std::sync::Mutex<Vec<u8>>>
}

impl GateProxy {
    pub fn new(_wr: Arc<Mutex<Box<dyn NetWriter + Send + 'static>>>) -> GateProxy 
    {
        GateProxy {
            gate_name: None,
            gate_host: None,
            wr: _wr,
            tx_buf: net::vec_transport::new_buf()
        }
    }

    pub async fn send_gate_msg(&mut self, msg: GateHubService) -> bool {
        // 复用缓冲 + 自动扩容（原来每条消息预分配 1MB，且超容量会被静默截断）
        let tx_buf = self.tx_buf.clone();
        net::vec_transport::send_via_buffer(&self.wr, &tx_buf, |transport| {
            let mut o_prot = TCompactOutputProtocol::new(transport);
            match GateHubService::write_to_out_protocol(&msg, &mut o_prot) {
                Ok(_) => true,
                Err(e) => {
                    error!("Failed to serialize Thrift message in send_gate_msg: {}", e);
                    false
                }
            }
        }).await
    }
}
