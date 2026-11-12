# Hub 层性能分析与优化方案

> 分析对象：`server/lib/hub/`（含其依赖的 `crates/net`、`crates/tcp`、`crates/queue`、`crates/proto`，以及 Python 侧的 `server/engine/app.py`、`aoi.py`）。
> 方法：通读源码 + 逐路径记账（每条结论都给了 `file:line`，可复核）。
> 说明：本文所有耗时数字都是按代码路径做的**量级估算**，标注为「估算」。第 6 节给了把它变成实测数字的最小方案。
>
> **状态更新（2026-11-12）**：第 5 节的 A1/A2/A3、死锁审计里的 D1/D2/D3/D4/D7/D8，以及「D5 写锁跨 write_all / 持 CM 跨 TcpConnect / 空结构体 mutex / 锁等待看门狗」这四项也**已全部修复**（`cargo check` 零 error 零 warning），详见 `hub-deadlock-audit.md` §9。
>
> 其中按本文编号已落地的有：**② 每 peer 写任务 + 合并发包**（`peer_writer.rs`）、**④ 的一部分**（`VecWrite` 让序列化不再逐 write 加锁；`NetPack` 的 O(N²) 仍在）、**⑥ 的一部分**（锁结构 + 分段持锁）、**① 的一部分**（批量泵、去掉分发路径里的 `CM` 间接 `block_on`）。**仍未做**：①(a) 缓存 peer 名字以彻底去掉 `get_*_name_async` 的 `block_on`、③ 广播批量化、④ `NetPack` 游标化、⑤ 编解码零拷贝、⑦ tick 预算 + 唤醒。

---

## 1. TL;DR

Hub 现在的形态是：**Python 主线程用「一次一条消息」的方式跨 FFI 来拉队列；每条消息都要 `rt.block_on` + 抢 4~6 把锁；发送侧每条消息都要过一把全局 `HubServer` 锁 + 一次 `block_on` + 一次 `write` 系统调用。**

按「收益 / 改动风险」排序，我建议这么打：

| 优先级 | 优化 | 主要收益（估算） | 改动面 |
|---|---|---|---|
| P0 | ① 泵（`poll`）批量化 + 去掉每条消息的 `block_on` | 单消息 Rust 侧开销 −60%~70%，泵吞吐 ×3~5 | 只动 `hub_service_manager.rs` 的泵 + proxy 锁类型 |
| P0 | ② 出站改「每 peer 一条 mpsc + 写任务」，去掉全局 `Mutex<HubServer>` | 单次发送 −50%~70%，系统调用 ÷N，**消除一个慢 peer 拖死整个 hub** | `hub_server.rs` + 三个 proxy |
| P1 | ③ 广播类 API 批量化（序列化/发包各一次） | AOI 扇出场景流量与 CPU 随观察者数从 O(N) 降到 O(1)+O(N) 小拷贝 | 新增 API，Python 侧改一处 `aoi.py` |
| P1 | ④ 收包零拷贝：修掉 `NetPack` 的 `drain` O(N²) | 高负载（一次 read 里几十~几百个包）时收包 CPU 可降一个数量级 | `crates/net`（gate 同收益） |
| P2 | ⑤ 序列化器去无谓分配（`TFieldIdentifier` 每字段一次 String） | 发送侧每条消息 −10~20 次堆分配 | 生成器或 `proto` 一行级改动 |
| P2 | ⑥ 锁结构整治（`RwLock`+`HashMap`、去掉无状态 mutex、缩短临界区） | 每消息 −3~5 次锁；去掉 3 处「持锁跨网络 I/O」 | 分散，逐个小改 |
| P3 | ⑦ tick 内按时间预算泵消息 + 数据到达唤醒 | 收包延迟从「最多 33ms」降到亚毫秒；避免突发把 tick 撑爆 | `app.py` + 泵 API |
| P3 | ⑧ 健康/时间接口去 `block_on` | 每 tick 省 2 次 runtime 往返（小） | `lib.rs` 小改 |

**另外有 3 个会伪装成「性能问题」的正确性缺陷（P0，必须先修）**：`poll` 持队列锁跨 `block_on` 和 Python 回调导致的 ABBA 死锁（整个 hub 卡死，看起来像「主循环慢」）、`ConnManager`↔`ConnProxy` 锁序反转、无界队列无背压（OOM）。见第 5 节。

---

## 2. 现状：一条消息的完整链路

### 2.1 收包（gate → hub）

```
TcpReader 读循环 (crates/tcp/src/tcp_socket.rs:32-69)
  └ buf: Vec<u8> 64KB 复用 ; net_pack.input(&buf[..n])           ── 拷贝 ①
  └ loop { net_pack.try_get_pack() }                             ── 拷贝 ② + drain O(N²)
        └ f.lock().await ; cb(data)                              ── 每包一次异步锁
              └ ConnCallbackMsgHandle::on_event                  (hub_service_manager.rs:182)
                   ├ proxy.lock().await                          (186)  ← 持锁做 CPU 活
                   ├ deserialize(data)                           (141-148) ── 零填充 + 拷贝 ③ + 每字段 String/Vec 分配
                   ├ handle.lock()  (std::Mutex)                  (195)
                   └ queue.enque(Box<ConnEvent>)                  (199)  ← 无界
```

### 2.2 泵（Python 每条消息一次 FFI）

`server/engine/app.py:250-253` 是一个 `while True: if not poll_conn_msg(): break` 的 Python 循环，即**一条消息 = 一次 Python→Rust 调用**。而 `HubConnMsgPump::poll_conn_msg`（`server/lib/hub/src/lib.rs:853-856`）转发到 `ConnCallbackMsgHandle::poll`（`hub_service_manager.rs:205-866`），后者：

- 拿队列锁（210）、clone `Arc`（211）、clone `runtime::Handle`（212）、`deque` 一条（213）；
- 按变体分发；**大量分支里有 `rt.block_on(...)`**（226、277、327、429、618、663、727、778），目的只是从 `Arc<Mutex<ConnProxy>>` / `Arc<Mutex<HubProxy>>` 里读一个 `hub_name` / `gate_name` 字符串（`get_hub_name_async`/`get_gate_name_async`，32-50）；
- 然后**再深拷贝一次事件**（闭包里 `ev.clone()`，如 306、318、525）；
- 最后 `call_method1` 调 Python，`PyBytes::new(py, &argvs)` 再拷一份 payload（`hub_msg_handle.rs:109` 等）。

### 2.3 发送（Python → hub → gate/dbproxy）

```
ctx.hub_call_client_ntf(...)                    ── Python 包装层 2 跳 (context.py:148)
 └ HubContext::hub_call_client_ntf              (lib.rs:671-681)
     └ with_server!  = server_arc.clone + rt.block_on        (lib.rs:130-139)   ← 全局 HubServer 锁
         └ HubServer::send_gate_msg            (hub_server.rs:354-377)          ← 持锁跨 await
             ├ conn_mgr.lock().await            (357)  BTreeMap<String,_> 查找
             ├ gate_proxy.lock().await          (359)
             └ GateProxy::send_gate_msg
                  ├ thrift 序列化进复用 buf，**每次 write() 都锁一把 std::Mutex** (vec_transport.rs:36-40)
                  │    且每个字段 `TFieldIdentifier::new("name",..)` 都 new 一个 String（proto/src/hub.rs:1182 等）
                  ├ wr.lock().await            ← 写锁跨 write_all
                  └ TcpWriter::send: vec![0;4] + 整包 extend + write_all  (tcp_socket.rs:86-108) ── 拷贝 ②
```

### 2.4 单消息开销记账（估算，`ClientCallNtf` + ~100B payload）

| 阶段 | 分配次数 | 拷贝次数 | 估算耗时 |
|---|---|---|---|
| socket read（摊薄） | 0 | 1 | ~0.2µs |
| `NetPack` 分包 | 1~2/包 | 2 | 0.1~0.2µs，**负载高时 `drain` 的 memmove 可到 2~5µs/包** |
| thrift 反序列化 | 6~12 | 1 | 0.4~0.8µs |
| 入队（2 把锁） | 1（Box） | 0 | ~0.15µs |
| Python `poll_conn_msg` 边界 | 0 | 0 | 0.1~0.2µs |
| 队列锁 + Arc/Handle clone | 0 | 0 | ~0.1µs |
| `ev.clone()` 深拷贝 | 2~3 | 1 | 0.2~0.4µs |
| `rt.block_on` + 2 把异步锁（取名） | 0 | 1（String） | **0.5~1.5µs** |
| 无状态 handle 的 std mutex | 0 | 0 | ~0.05µs |
| `PyBytes::new` + tuple + `call_method1` | 2 | 1 | 0.5~0.8µs |
| **Rust 侧合计** | **~12~20** | **~7** | **2~4µs（不含 memmove 放大）** |
| Python 业务（entity 查找 + msgpack + 逻辑） | — | — | 5~20µs ← 通常才是真正的天花板 |

发送侧单次 `hub_call_client_ntf`：`block_on` + 全局锁 + 3 把锁 + 序列化（10~20 次无用 String 分配）+ 整包拷贝 + 1 次 syscall ≈ **4~8µs**。`aoi.py:237-257` 的广播是**每个观察者一次**这种调用，所以一次 50 人的广播 ≈ 200~400µs + 50 次 syscall，且 50 次序列化同一份 payload。

**结论**：不是「Rust 慢」，而是「每条消息的固定开销太重、拷贝太多、把 I/O 和 Python 回调绑在同一个线程的同一次调用里」。这就是优化的主战场。

---

## 3. 优化项详解

### ① 泵批量化 + 去掉逐消息 `block_on`（最高性价比）

**问题**
1. 每条消息一次 Python→Rust 调用，Python 侧 `while True` 循环本身也是成本（约 0.1~0.3µs/条）。
2. 每次调用都重复：`Arc` clone、`runtime::Handle` clone、`expect`、队列锁。
3. 最贵的是 `rt.block_on(...)`（`hub_service_manager.rs:65、226、277…`）：**它唯一的目的就是读一个字符串**。这是每条消息 0.5~1.5µs 的纯浪费，还顺带把 `Handle` 粘进了消息分发逻辑（`poll` 里 `rt` 是 `expect("rt_handle not set")` 出来的，212）。

**改法（分两步，可独立上线）**

(a) 先消灭 `block_on`：把 peer 身份在握手时**写死**到连接上，用同步锁读。`ConnProxy` 的字段全是纯数据（`wr: Arc<…>`、`Option<Arc<Mutex<HubProxy>>>`），没有任何 `await` 需求，把 `tokio::sync::Mutex` 换成 `parking_lot::RwLock`（或 `ArcSwap`）即可在 `poll` 里零等待读取：

```rust
// hub_service_manager.rs
pub struct ConnProxy {
    pub wr: Arc<Mutex<Box<dyn NetWriter + Send + 'static>>>,
    pub peer: Arc<OnceLock<Peer>>,   // 新增：{ kind: Hub|Gate, name: Arc<str>, host: Arc<str> }
    ...
}
// RegServer/ClientRequestLogin 分支里 peer.set(...)，poll 里 peer.get() 直接拿 &Peer
```
顺带把 `HubProxy`/`GateProxy` 的 `hub_name`/`gate_name`/`gate_host`/`tx_buf` 也挪进同一个同步元数据结构（`wr` 用 `Arc` 克隆后丢锁再 `await`），`handle_hub_event`（52-74）里的 `rt.block_on` 就整体消失了。

(b) 再把泵改成批量：Python 侧只调用一次，内部循环到队列空或时间预算用完：

```rust
// lib.rs：替换 poll_conn_msg
pub fn poll_conn_msg(slf: PyRefMut<Self>, py_handle: Py<PyAny>, budget_ms: u64) -> usize {
    let py = slf.py();
    ConnCallbackMsgHandle::poll_batch(slf.conn_msg_handle.clone(), py, py_handle,
                                      std::time::Duration::from_millis(budget_ms))
}
```
`poll_batch` 内部：**先把队列整体换出（`mem::take`/`VecDeque::split_off`）→ 释放队列锁**（这同时修掉第 5 节的死锁，见 A1）→ 逐条分发 → 返回处理条数。Python 侧 `while` 循环保留但改成按预算调用（见 ⑦）。

**预期**：单消息 Rust 开销 2~4µs → 1~1.5µs；泵吞吐 ×3~5；顺带消除 A1 死锁。
**风险**：低。语义不变（仍然在主线程串行处理）。唯一注意点是「一批处理中途收到新消息」的行为——用预算 + 返回条数表达即可。

---

### ② 出站：每 peer 一条 mpsc + 写任务，去掉全局 `Mutex<HubServer>`

**问题**（这是最危险的架构点，既是性能也是可用性）
- 每个 `hub_call_client_*` 都经 `with_server!`（`lib.rs:130-139`）`block_on` 抢**唯一一把** `Mutex<HubServer>`；
- `HubServer::send_gate_msg`（`hub_server.rs:354-377`）在**持有这把全局锁的情况下** `await` 到 `GateProxy` 的写锁、再 `await` `write_all`；
- `write_all` 在对端不读（窗口满）时会长时间挂起 → **一个慢 gate 会卡住整个 hub 的所有发送**，而 Python 主线程正 `block_on` 等这把锁，于是整个 tick 一起卡住。
- 每条消息 = 1 次 syscall（`tcp_socket.rs:100`）。游戏里典型是几十~几百字节的小包，`TCP_NODELAY` 已开（`tcp_server.rs:53`），所以既没有 Nagle 合并，也没有用户态合并。

**改法**

```rust
struct PeerWriter { tx: mpsc::Sender<Vec<u8>> }   // 有界，例如 4096

// 每个 peer 一个写任务
tokio::spawn(async move {
    let mut out = Vec::with_capacity(64 * 1024);
    while let Some(first) = rx.recv().await {
        out.clear(); out.extend_from_slice(&first);
        while let Ok(b) = rx.try_recv() {
            out.extend_from_slice(&b);
            if out.len() >= 64 * 1024 { break }
        }
        if wr.lock().await.send(&out).await == false { break }
    }
});
```
- 发送入口变成：`HashMap<name, Arc<PeerWriter>>` 查找（`RwLock` 读锁）+ `try_send`，**没有 `block_on`、没有全局锁、没有 I/O**，返回耗时降到亚微秒。
- 背压策略要显式化：`NTF` 用 `try_send` + 丢弃计数（可观测），`RPC/RSP/实体生命周期` 用 `blocking_send`（保序可靠）——这需要在 Python API 上区分，或统一按消息类型定策略。
- 写合并后系统调用可以按「一批」计。

**预期**：单次发送 4~8µs → 1~2µs；syscall 数取决于合并率（保守 ÷4~÷10）；**消除跨 peer 的头部阻塞**。
**风险**：中。要改 `HubProxy`/`GateProxy`/`DBProxyProxy` 三个 `send_*` 的签名（`&mut self` → `&self`），并决定背压语义。建议保留 `send_*` 的 `bool` 语义 = 「是否入队成功」。

---

### ③ 广播类 API 批量化

**问题**：`aoi.py:237-257` 把同一条广播对 N 个观察者**逐个**调 `hub_call_client_ntf`：N 次序列化 + N 次 syscall + N 次加锁。收包方向已经有 `HubForwardClientRequestServiceExt`（`hub_service_manager.rs:424-515`）这种「一次带多个 request_info」的批量形态，发送方向缺一个对称的。

**改法**：
- 新增 `hub_call_client_ntf_multi(gate, [(conn_id, entity_id)], method, argvs)`：**序列化一次**（或新增一个 thrift `NTF 批量` 消息：method/argvs 各一份 + N 个 (conn, entity)），按 gate 分组后每个 gate 发一个包。
- Python 侧把 `call_observers_notify` 改成先按 `gate_name` 分组再批量调用。

**预期**：50 人广播 200~400µs → 10~20µs；包体大小从 N×完整帧 降到 1×帧 + N×小元组。
**风险**：低-中（新增消息类型，gate 侧要解包；跨版本需兼容）。

---

### ④ 收包零拷贝：`NetPack` 的 O(N²) 与多余拷贝

**问题**（`crates/net/src/lib.rs:60-102`）
```rust
let mut buf = vec![0u8; new_pack_len];
buf.copy_from_slice(&self.buf[4..packet_end]);   // 每个包一次分配 + 拷贝
if total > packet_end { self.buf.drain(0..packet_end); }  // 每个包一次 memmove 剩余全部
```
一次 `read` 拿到 64KB（约 100~600 个小包）时，`drain` 的总搬运量是 `O(N × 剩余长度)`：600 个包 → 平均每次搬 ~32KB → **约 20MB memmove / 64KB 数据**，放大 300 倍。这是「连接一忙 CPU 就飙」的典型来源，gate（`crates/tcp` 共用）也有同样问题。

**改法**：改成游标式缓冲，**只在必要时压缩**，并且不复制 payload：
```rust
pub struct NetPack { buf: Vec<u8>, read_pos: usize }
// 读取：返回 (start, end) 区间，交给反序列化直接在 &buf[start..end] 上读
// 压缩：read_pos > buf.len()/2 && read_pos > 8KB 时 buf.drain(..read_pos) 一次
// 扩容：buf.extend 前先 reserve，避免反复 realloc
```
若允许引依赖，`bytes::BytesMut::advance` 是现成方案。
配套把 `NetReaderCallback::cb(&mut self, data: Vec<u8>)` 改成 `cb(&mut self, data: &[u8])`（或 `Bytes`），省掉「每包一次 Vec 分配 + 拷贝」。

**预期**：高负载下收包 CPU 降一个数量级；稳态每包分配从 2 次降到 0 次。
**风险**：低（接口小改，`tcp`/`wss` 两处调用点）。

---

### ⑤ thrift 编解码的无谓开销

1. **反序列化的第 3 次拷贝**：`deserialize`（`hub_service_manager.rs:141-148`）用 `TBufferChannel::with_capacity(len, 0)`，thrift 0.17 的实现是 `vec![0; len]` 零填充 + `set_readable_bytes` 再拷一遍（registry `thrift-0.17.0/src/transport/mem.rs:58-104`），而且 `Read` 每次调用都要锁一次内部 `Arc<Mutex<ReadData>>`（`mem.rs:165-172`）——compact 协议的 varint 是逐字节读的，一条消息几十次 `read` = 几十次锁。
   **改法**：自己实现一个 `struct SliceTransport<'a>(&'a [u8])`（只实现 `Read`）直接喂给 `TCompactInputProtocol`，零填充和拷贝都没了，读也不再上锁。
2. **`ev.clone()`**：`poll` 里 `match` 是 `ref` 匹配，然后闭包里再 `ev.clone()`（306/318/525/553/573/585/597/609/760/771/827…）。改成**移动语义**：`match ev_data.ev { HubService::ClientCallNtf(ev) => { … ev … } }`，一次深拷贝直接消失。
3. **发送侧的 `TFieldIdentifier`**：`proto/src/hub.rs:1182` 这类 `TFieldIdentifier::new("gate_name", TType::String, 2)`，thrift 0.17 的签名是 `N: Into<Option<S>>, S: Into<String>`（`protocol/mod.rs:662-679`），于是**每个字段都 new 一个 String**；而 compact 协议**根本不用 name**（`protocol/compact.rs:456-475` 只用 `field_type` 和 `id`）。
   **改法**：生成器（`rpc/gen/`）里把 `new("name", …)` 换成 `TFieldIdentifier { name: None, field_type, id: Some(n) }`，或加一个本地 `fn fld(t: TType, id: i16)` 辅助函数。每条消息省 6~20 次堆分配，全服务端受益。
4. **`TVecTransport::write` 每次 write 都锁一把 std Mutex**（`vec_transport.rs:36-40`）：一条消息几十次 `write()` = 几十次锁。改成把 `&mut Vec<u8>` 交给闭包（thrift 只需要 `Write`）：
   ```rust
   pub fn send_via_buffer<F: FnOnce(&mut Vec<u8>) -> bool>(...) { /* 全程无锁 */ }
   ```
5. **`TcpWriter::send`**（`tcp_socket.rs:86-108`）：`vec![0;4]` + `extend_from_slice(buf)` = 把整包再拷一遍。序列化时**先预留 4 字节头**，写完再回填长度，就能直接 `write_all(&buf)` 零额外拷贝（或 `write_vectored` 两段）。

**预期**：收包分配 ~12→~5 次、拷贝 7→3 次；发包分配 −10~20 次、拷贝 −1。合计单消息 CPU 再降 20%~30%。

---

### ⑥ 锁结构与临界区

| 位置 | 问题 | 改法 |
|---|---|---|
| `ConnManager`（`conn_manager.rs:17-25`） | 一个 `tokio::Mutex` 保护 6 张 `BTreeMap<String,_>`；每条消息都要过；`get_hub_name()`（90-92）每次 clone 一个 String | 改 `RwLock`（读多写少）+ `HashMap`（或 `HashMap<Arc<str>, _>`）；`hub_name` 用 `Arc<str>` 避免 clone |
| `hub_proxy_manager.rs:34-68`、`dbproxy_manager.rs:56-78` | **持 `conn_mgr` 锁跨 `redis.get` / `acquire_lock` / `TcpConnect` / `connect`**（网络往返，几十 ms） | 只在「查表/插入」时持锁；网络 I/O 在锁外 |
| `hub_proxy_manager.rs:79-152` | 持 consul 锁跨整个探测/连接循环 | 只在 `services()` 期间持有 |
| `hub_msg_handle`/`gate_msg_handle` | 结构体是**空的**（`hub_msg_handle.rs:27-28`、`gate_msg_handle.rs:24`），却为每条消息套一层 `Arc<StdMutex<…>>`（`poll` 里 266/414/650/693…） | 直接删掉这层锁：这些 `do_*` 无状态，改成自由函数或 `&self` 无锁方法 |
| `ConnProxy`/`HubProxy`/`GateProxy` | 元数据用异步锁，但临界区内没有 `await`（除了 `wr`） | 元数据 → 同步 `RwLock`；只有 `wr`（真 I/O）保留异步锁 |
| `send_hub_msg` 的重连逻辑（`hub_server.rs:312-352`） | 在发送路径里同步做 redis 锁 + TCP 连接（`entry_direct_hub_server`）；`connecting_hubs` 也加锁判断 | 拆成「投递到重连队列 + 后台任务」，发送路径只做 `try_send` |

**预期**：每消息 −3~5 次锁；去掉 3 处跨 I/O 持锁（既是吞吐问题也是**死锁/卡死**来源）。
**风险**：低，但要逐个改 + 有回归测试兜底（目前 hub **零测试**，见第 6 节）。

---

### ⑦ tick 内时间预算 + 数据到达唤醒（延迟问题）

**现状**（`app.py:270-331`）：每 tick 先 `poll_conn_msg()` **把队列抽干**，再物理、再 `update`，最后 `time.sleep(0.033 - tick)`。两个后果：

1. **延迟**：消息在 `sleep` 期间到达时要等最多 33ms 才被处理（对动作类游戏偏高）。
2. **突发撑爆 tick**：一批 5000 条消息 × 10µs = 50ms，全在主线程持 GIL 处理，`update`/asyncio 协程/DB 泵全部被推迟；`app.py:281` 的 `>1s` 告警只有在极端情况才会响。

**改法**：
- 泵按**时间预算**处理（例如每 tick 最多 6ms），剩余留到下轮，并把「每轮处理条数/积压深度」打点（`Queue::len()` 已经有了，`crates/queue/src/lib.rs:39`，只是没暴露）。
- 让 reader 侧在「队列从空变非空」时唤醒主循环（eventfd / `tokio::sync::Notify` + `select!`，或把 `sleep(0.033)` 换成「超时 33ms 或队列非空即返回」）。这样延迟从 33ms 降到「到货即处理」，同时保留 tick 节奏。
- 长期方向：把「收消息」从 tick 里彻底解耦（独立线程泵 + 队列交给 asyncio），但那会动玩法语义（每 tick 确定性处理），要业务确认，不建议第一轮就做。

**预期**：收包延迟 p99 从 ~35ms → <2ms；tick 抖动显著下降，`/health` 不再因突发流量被判不健康。
**风险**：中（时序语义变化，需要业务侧确认「消息必须在本 tick 处理完」这个隐含假设是否成立）。

---

### ⑧ 小项（顺手做）

- `set_health_state`（`lib.rs:329-335`）每次调用都 `block_on` + 异步锁只为改一个 bool，而 `app.py:317` **每 tick 都调**。改成 `AtomicU64`（存 `last_heartbeat` 的 `Instant`→单调纳秒）+ `AtomicBool`，同步 API，零 `block_on`。同理 `utc_unix_time_with_offset`（`lib.rs:267-273`）每次 `block_on`，可把 offset 存 `AtomicI64`，读时钟不需要锁。
  - 顺带发现一个 **Python 侧 bug**：`context.py:66-67` 的 `utc_unix_time_with_offset` **忘了 `return`**，永远返回 `None`。
- `HubConnMsgPump::new` / `HubDBMsgPump::new`（`lib.rs:829-851`、`867-889`）用 `try_lock` + `sleep(10ms)` 轮询最多 500ms 去抢 `server` 锁——启动期没必要，直接在 `HubContext::new` 里把两个 handle 存成字段暴露出去。
- 三个 `tokio::runtime::Runtime::new()`（`lib.rs:191、202` + pyo3-async-runtimes 的默认 runtime）：每个多线程 runtime 都会拉起 `num_cpus` 个 worker。对小规格 hub 是纯浪费，建议统一成一个（`_listen_rt`），健康检查用 `spawn` 挂在上面。
- `poll` 里 `_handle_clone = _handle.clone()`（211）每条消息一次 Arc clone，仅 `direct_connect_server` 一处用到，可以只在那个分支 clone。

---

## 4. 预期收益汇总（估算）

以「收 `ClientCallNtf` + 发给 50 个观察者」为主线：

| 指标 | 现在 | ①+②+③ 后（不含 ④⑤） |
|---|---|---|
| 收包单条 Rust 开销 | 2~4µs | 0.8~1.5µs |
| 单次发送开销 | 4~8µs | 1~2µs |
| 50 人广播总开销 | 200~400µs + 50 syscall | 15~30µs + 1~2 syscall |
| 每消息内存分配次数 | ~12~20（收）+ 10~20（发） | ~5（收）+ ~5（发） |
| 慢 peer 影响 | 卡死整个 hub | 只影响该 peer 队列 |
| 收包延迟 p99 | ~33ms | 受 ⑦ 影响，可到 <2ms |

**先说清楚**：如果业务侧 Python handler 本身就占 10~20µs/条，那么 Rust 侧再怎么优化也只能把「每条消息的总成本」从 ~15µs 压到 ~12µs。**所以第一步必须是量测（第 6 节）**，确认瓶颈到底是 Rust 侧、GIL、还是 Python 业务逻辑——不然可能优化错地方。从代码形态看，Rust 侧固定开销占比大概率是显著的那一档（尤其发送侧和广播放大），但我没法在这个环境里用数据确认。

---

## 5. 顺带发现的「伪装成性能问题」的正确性缺陷（P0，建议先修）

### A1. `poll` 持队列锁跨 `block_on` → ABBA 死锁（会让整个 hub 静默卡死）

- `poll` 在 `hub_service_manager.rs:210` 拿住 `ConnCallbackMsgHandle` 的 **`std::sync::Mutex`**，一直持有到函数结束（866）；中间在 226/277/327/618/663/778 等分支里 `rt.block_on(...)`，而里面的第一步是 `conn_proxy.lock().await`（228/65/331…）。
- 同时，reader 任务在 `on_event`（182-203）里先 `conn_proxy.lock().await`（186），再做反序列化，然后去拿**同一把** `std::Mutex`（195）。
- 于是：`poll` 持 A 等 B，reader 持 B 等 A。`std::sync::Mutex::lock()` 在 runtime worker 上是**阻塞线程**的，不会让出，所以不会自愈 —— 表现为「hub 突然不处理消息、但进程还在、`/health` 变 503」，和性能问题极像。
- **修法**：`poll` 先用 `mem::take`/`split_off` 把队列整体换出、**释放锁**，再逐条处理（正好和 ① 的批量化是同一处改动）。

### A2. `ConnManager` ↔ `ConnProxy` 锁序反转

- `ConnManager::add_hub_proxy` / `add_gate_proxy`（`conn_manager.rs:103-121`）在**持有 `conn_mgr` 的情况下**去锁 `ConnProxy`（106/118）。
- `poll` 的 `RegServer` / `ClientRequestLogin` 分支（228-245、615-647）是**先锁 `ConnProxy` 再锁 `conn_mgr`**。
- 现在没炸，只是因为「入站连接没登记进 `connproxys`」（`ConnProxyManager::cb` 没插入，`hub_service_manager.rs:964-994`）导致 `add_*_proxy` 里 `get` 返回 `None`。这是**靠巧合安全**，任何一次改动（比如为了让出站连接可复用而入表）都会踩成死锁。
- **修法**：定一条全局锁序并写进注释；`add_*_proxy` 里把「锁 `conn_proxy` 回填字段」挪到锁 `conn_mgr` **之前**（先 clone Arc 再在锁外回填）。

### A3. 无界队列、无背压、无观测

`Queue` 默认 `capacity: None`（`crates/queue/src/lib.rs:50-58`），`enque` 结果还被 `let _ =` 丢掉（`hub_service_manager.rs:179`）。只要泵的处理速度低于收包速度（一次 GC/存档/物理构建就能发生），内存就单调增长直到 OOM，而且**没有任何指标能看出来**。建议：有界 + 满了的处理策略（断开该连接/丢弃 NTF）+ 暴露积压深度。

---

## 6. 度量与压测方案（做优化的前置条件）

目前 hub **零测试、零指标**（`#[test]` 在 `server/lib/hub` 里 0 命中），任何重构都是盲改。建议先补这三样：

1. **打点（成本很低，收益最高）**
   - 队列积压：`ConnCallbackMsgHandle` 暴露 `queue.len()`；每 tick 记录「处理条数 / 积压峰值 / 预算用尽次数」。
   - 泵耗时：每条消息 `Instant::now()` 差值进 HDR 直方图（或简单的 16 桶对数直方图），每 5s 打 p50/p99/最大值。`app.py:281` 已有 `>1s` 告警，但阈值太粗。
   - 发送侧：每 peer「入队成功/失败/丢弃」计数 + 写任务单批字节数/包数/等待时长。
   - 直方图别用 `Vec` 存原始样本，会自己变成瓶颈。

2. **微基准（`criterion`，dev-dependency 加在 `server/lib/hub`）**
   - `deserialize` 单条：100B / 1KB / 64KB 三档，1M 次；
   - `NetPack` 分包：一次投入 64KB 含 1/10/100/1000 个包，测总耗时（这一项会把 ④ 的收益量化出来）；
   - 序列化 + `TcpWriter::send` 到 `Vec`（不真发）；
   - 泵（无 Python）：`enque` 1M 条 → `poll_batch` 到空，测条/秒。
   - 注意：项目没有 workspace，基准要放在 `server/` 或 `server/lib/hub` 里用 `cargo bench -p hub`（当前各 crate 独立构建，`server/Cargo.toml` 是入口）。

3. **端到端压测**
   - 假 gate（Rust 或 Python）以可配置速率发 `ClientCallNtf`，hub 侧挂一个 no-op 的 Python handler → 得到「纯框架吞吐」；
   - 再把真实 handler 打开 → 两者相减就是业务逻辑成本；
   - 验收指标：泵 p99、CPU/万消息、广播扇出 N=10/50/200 的总耗时、慢 peer（人为不读 socket）时其他 peer 的 RTT。

---

## 7. 落地顺序

| 阶段 | 内容 | 验收 |
|---|---|---|
| S0（1~2 天） | 补 A1/A2/A3 的修复 + 打点 + `NetPack`/序列化微基准 | 压测下不死锁不 OOM；拿到第一组 p50/p99 基线 |
| S1（3~5 天） | ①(a) 去 `block_on`（同步元数据锁 + `OnceLock<Peer>`）、①(b) 批量泵 | 泵吞吐 ×2 以上，单消息 p99 下降；A1 具备修复条件 |
| S2（5~7 天） | ② 出站 mpsc + 写任务（含背压策略）、④ 游标式 `NetPack` | syscall 数下降一个量级；慢 peer 不再影响其他 peer |
| S3（3~5 天） | ③ 广播批量化、⑤ 编解码零拷贝、⑥ 锁整治 | AOI 扇出成本随 N 近似线性下降到「1 帧 + N 小元组」 |
| S4（2~3 天） | ⑦ tick 预算 + 唤醒、⑧ 小项 | 收包 p99 < 2ms；tick 抖动可观测且受控 |

**建议**：S0 的 A1/A2 无论如何先修——它们现在是「偶发卡死」的根因，而且修法与 ① 重合，等于顺手完成了一半性能改造。

---

## 8. 附：关键文件索引

| 关注点 | 位置 |
|---|---|
| 消息泵（逐条 + `block_on`） | `server/lib/hub/src/hub_service_manager.rs:205-866` |
| 入队 / 反序列化 | `hub_service_manager.rs:141-203` |
| 发送全局锁 | `server/lib/hub/src/lib.rs:130-139`、`hub_server.rs:312-377` |
| 写路径 | `crates/net/src/vec_transport.rs:35-96`、`crates/tcp/src/tcp_socket.rs:86-108` |
| 分包 | `crates/net/src/lib.rs:60-102` |
| 锁结构 | `server/lib/hub/src/conn_manager.rs:17-133` |
| 序列化字段分配 | `crates/proto/src/hub.rs:1180-1200` |
| Python 主循环 | `server/engine/app.py:245-331` |
| AOI 广播放大 | `server/engine/aoi.py:237-257` |
