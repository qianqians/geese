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
        // extend_from_slice 会自动扩容，永远不会短写。
        // 用 into_inner() 兜住 poison：thrift 序列化有 panic 点，一旦在持锁时 panic，
        // `.unwrap()` 会让之后**每一次发送**都接着 panic（hub-deadlock-audit.md D7）。
        self.buf
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .extend_from_slice(buf);
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

/// 直接写进 `&mut Vec<u8>` 的 thrift 写传输：**全程无锁**。
///
/// `TVecTransport` 每次 `write()` 都要锁一次 `Arc<StdMutex<Vec<u8>>>`，
/// 而 compact 协议一条消息会发起几十次 `write()`；给「序列化成一整包、
/// 再交给写任务」的路径用这个，就没有这把锁了。
pub struct VecWrite<'a>(pub &'a mut Vec<u8>);

impl<'a> Write for VecWrite<'a> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// 序列化并加 4 字节小端长度帧头，返回**可直接 `write_all`** 的整包。
///
/// 帧头是 `NetPack` 的分包格式（`[u32 len][payload]`），所以多个这样的整包
/// 可以首尾拼在一起一次写出，对端仍能正确分包 —— 这是"一次系统调用发多条消息"的基础。
///
/// 每次调用分配一个新 `Vec`（旧实现是 `mem::take` 复用缓冲，但那样没法把
/// 所有权交给写任务）；相比旧路径仍然少一次整包 memcpy。
pub fn serialize_framed<F>(serialize: F) -> Option<Vec<u8>>
where
    F: FnOnce(&mut Vec<u8>) -> bool,
{
    let mut bytes = Vec::with_capacity(256);
    bytes.extend_from_slice(&[0u8; 4]);
    if !serialize(&mut bytes) {
        return None;
    }
    let len = (bytes.len() - 4) as u32;
    bytes[0..4].copy_from_slice(&len.to_le_bytes());
    Some(bytes)
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
    let bytes = std::mem::take(&mut *buf.lock().unwrap_or_else(|e| e.into_inner()));
    if bytes.is_empty() {
        restore(buf, bytes);
        return None;    // 序列化失败（协议不完整）就不要发半个包
    }
    Some(bytes)
}

fn restore(buf: &Arc<StdMutex<Vec<u8>>>, bytes: Vec<u8>) {
    let mut g = buf.lock().unwrap_or_else(|e| e.into_inner());
    if g.capacity() < bytes.capacity() {
        *g = bytes;
    }
}
