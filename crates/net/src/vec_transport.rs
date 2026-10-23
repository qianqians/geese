use std::io::{self, Read, Write};
use std::sync::{Arc, Mutex as StdMutex};

use tokio::sync::Mutex;

use crate::NetWriter;

/// 一个"写进 Vec<u8>"的 thrift 写传输：**空间不够自动扩容**。
///
/// 为什么需要它（踩过的坑）：
///   thrift 0.17 的 `TBufferChannel::write()` 写满是**短写** ——
///   `let nwrite = min(buf.len(), cap - pos)`，而 `TCompactOutputProtocol` 不会重试，
///   所以**超过容量的消息会被静默截断**（对端解析失败 → "发不出去/客户端卡住"）。
///   而各服务原来都是 `TBufferChannel::with_capacity(0, 容量)`：
///     · gate → 客户端        16 KB
///     · hub  → dbproxy       16 MB  ← 每条消息一次 16MB 分配
///     · hub  → gate、redis MQ  1 MB
///   既浪费分配，又有"超过容量就截断"的隐患。这个实现按需扩容，没有长度上限。
pub struct TVecTransport {
    buf: Arc<StdMutex<Vec<u8>>>,
}

impl TVecTransport {
    pub fn new(buf: Arc<StdMutex<Vec<u8>>>) -> TVecTransport {
        TVecTransport { buf }
    }
}

impl Read for TVecTransport {
    fn read(&mut self, _buf: &mut [u8]) -> io::Result<usize> {
        Ok(0) // 只写不读
    }
}

impl Write for TVecTransport {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        // extend_from_slice 会自动扩容，永远不会短写
        self.buf.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// 建一块新缓冲（每个 writer 一块即可）
pub fn new_buf() -> Arc<StdMutex<Vec<u8>>> {
    Arc::new(StdMutex::new(Vec::with_capacity(256)))
}

/// 用一块**可复用**的缓冲做"序列化 + 发送"。
///
/// * `serialize` 里用拿到的 `TVecTransport` 构造 thrift 协议把消息写进去
///   （net 不依赖 thrift，所以协议在调用方构造）
/// * 缓冲自动扩容 → 任意大小的消息都不会被截断
/// * 序列化完 `mem::take` 取出 Vec（只搬指针，零拷贝），发完把容量还回去，下一条不再分配
pub async fn send_via_buffer<F>(
    wr: &Arc<Mutex<Box<dyn NetWriter + Send + 'static>>>,
    buf: &Arc<StdMutex<Vec<u8>>>,
    serialize: F,
) -> bool
where
    F: FnOnce(TVecTransport) -> bool,
{
    let mut bytes = match serialize_to_bytes(buf, serialize) {
        Some(v) => v,
        None => return false,
    };

    let mut p_send = wr.as_ref().lock().await;
    let ok = p_send.send(&bytes).await;
    drop(p_send);

    bytes.clear();
    restore(buf, bytes);
    ok
}

/// 只序列化（不发），把字节交给调用方。
///
/// 给"需要 owned Vec 才能继续"的场景用（例如 redis MQ 要把 Vec 交给 lpush）。
pub fn serialize_to_bytes<F>(buf: &Arc<StdMutex<Vec<u8>>>, serialize: F) -> Option<Vec<u8>>
where
    F: FnOnce(TVecTransport) -> bool,
{
    if !serialize(TVecTransport::new(buf.clone())) {
        return None;
    }
    let bytes = std::mem::take(&mut *buf.lock().unwrap());
    if bytes.is_empty() {
        restore(buf, bytes);
        return None;    // 序列化失败（协议不完整）就不要发半个包
    }
    Some(bytes)
}

fn restore(buf: &Arc<StdMutex<Vec<u8>>>, bytes: Vec<u8>) {
    let mut g = buf.lock().unwrap();
    if g.capacity() < bytes.capacity() {
        *g = bytes;
    }
}
