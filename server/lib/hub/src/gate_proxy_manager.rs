use std::sync::Arc;

use tokio::sync::Mutex;
use tracing::error;

use thrift::protocol::{TCompactOutputProtocol, TSerializable};

use net::NetWriter;

use proto::gate::GateHubService;

/// 对某个 gate 的发送代理。
///
/// 发送路径现在是「序列化 → 入队」两步，**既不 `await`、也不持写锁**：
/// 真正的 socket 写在 [`crate::peer_writer`] 里那条独立任务上完成。
/// 这样上层（`HubServer` / `ConnManager` / proxy 三层锁）永远不会被一个
/// 慢对端的 `write_all` 堵住（见 hub-deadlock-audit.md D5）。
pub struct GateProxy {
    pub gate_name: Option<String>,
    pub gate_host: Option<String>,
    tx: crate::peer_writer::PeerSender,
}

impl GateProxy {
    pub fn new(_wr: Arc<Mutex<Box<dyn NetWriter + Send + 'static>>>) -> GateProxy {
        GateProxy {
            gate_name: None,
            gate_host: None,
            tx: crate::peer_writer::spawn(_wr),
        }
    }

    /// 序列化并入队。返回 `false` 表示**没有发出去**（序列化失败，或发送队列已满）。
    ///
    /// 注意：返回 `true` 只代表"已进入发送队列"，不代表对端已收到。
    pub fn send_gate_msg(&mut self, msg: GateHubService) -> bool {
        match net::vec_transport::serialize_framed(|buf| {
            let mut o_prot = TCompactOutputProtocol::new(net::vec_transport::VecWrite(buf));
            match GateHubService::write_to_out_protocol(&msg, &mut o_prot) {
                Ok(_) => true,
                Err(e) => {
                    error!("Failed to serialize Thrift message in send_gate_msg: {}", e);
                    false
                }
            }
        }) {
            Some(bytes) => self.tx.try_send(bytes),
            None => false,
        }
    }
}
