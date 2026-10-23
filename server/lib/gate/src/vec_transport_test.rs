//! TVecTransport 的 thrift 层回归测试。
//! 放在 gate 是因为 net 不依赖 thrift，而这里要验证"序列化 → 反序列化"的完整性。

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

use net::vec_transport::TVecTransport;
    use thrift::protocol::{TCompactInputProtocol, TCompactOutputProtocol, TSerializable};
    use thrift::transport::TBufferChannel;

    use proto::client::{CallNtf, ClientService};
    use proto::common::Msg;

    /// 大包回归：TBufferChannel 会静默截断，TVecTransport 必须完整写出并能反序列化回来。
    #[test]
    fn big_message_is_not_truncated() {
        for size in [64usize, 256, 1024, 8000, 200_000] {
            let big: Vec<u8> = (0..size).map(|i| (i % 251) as u8).collect();
            let msg = ClientService::CallNtf(CallNtf::new(
                "hub_scene0".to_string(),
                "entity-0001".to_string(),
                Msg::new("move".to_string(), big.clone()),
            ));

            let buf = Arc::new(Mutex::new(Vec::new()));
            {
                let mut o_prot = TCompactOutputProtocol::new(TVecTransport::new(buf.clone()));
                ClientService::write_to_out_protocol(&msg, &mut o_prot).unwrap();
            }
            let bytes = std::mem::take(&mut *buf.lock().unwrap());

            assert!(
                bytes.len() > size,
                "载荷 {} 字节，序列化后只有 {} 字节，明显被截断了",
                size, bytes.len()
            );

            let mut ch = TBufferChannel::with_capacity(bytes.len(), 0);
            ch.set_readable_bytes(&bytes);
            let mut i_prot = TCompactInputProtocol::new(ch);
            let back = ClientService::read_from_in_protocol(&mut i_prot).unwrap();
            match back {
                ClientService::CallNtf(n) => {
                    let argvs = n.message.unwrap().argvs.unwrap();
                    assert_eq!(argvs.len(), size, "反序列化后长度不对（size={}）", size);
                    assert_eq!(argvs, big, "反序列化后内容不对（size={}）", size);
                }
                other => panic!("类型不对: {:?}", other),
            }
        }
    }

    /// 对照：同一个包用"小容量 TBufferChannel"确实会被截断（证明这个改动是必要的）
    #[test]
    fn tbufferchannel_truncates_but_vectransport_does_not() {
        let big: Vec<u8> = (0..4096).map(|i| (i % 251) as u8).collect();
        let msg = ClientService::CallNtf(CallNtf::new(
            "hub".to_string(),
            "e".to_string(),
            Msg::new("move".to_string(), big),
        ));

        // 小容量缓冲：写不进去，bytes 被截断
        let small = TBufferChannel::with_capacity(0, 256);
        let mut o_prot = TCompactOutputProtocol::new(small.clone());
        let _ = ClientService::write_to_out_protocol(&msg, &mut o_prot);
        let truncated = small.write_bytes();

        // 我们的传输：完整
        let buf = Arc::new(Mutex::new(Vec::new()));
        {
            let mut o_prot = TCompactOutputProtocol::new(TVecTransport::new(buf.clone()));
            ClientService::write_to_out_protocol(&msg, &mut o_prot).unwrap();
        }
        let full = std::mem::take(&mut *buf.lock().unwrap());

        assert!(truncated.len() <= 256, "TBufferChannel 应该被截断到 256 以内，实际 {}", truncated.len());
        assert!(full.len() > 4096, "TVecTransport 不应该被截断，实际 {}", full.len());
    }
}
