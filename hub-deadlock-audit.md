# Hub 层死锁专项审计

> 审计范围：`server/lib/hub/` 全部 11 个源文件，以及它直接压住的 `crates/net`、`crates/tcp`、`crates/queue`、`crates/redis_service`（`brpop` 循环）、`crates/health`。
> 方法：枚举全部 113 处 `.lock()` / `try_lock`，按「持 A 求 B」建图找环，并核对每个环的**可触发条件**（能否真的有多线程/多任务同时在场）。
> 结论：**3 条真实环路 + 2 条「等价于卡死」的阻塞链 + 3 条会伪装成卡死的隐患**。环路 1 在正常流量下就可能踩中。

---

## 1. 结论速览

| 编号 | 环 / 链 | 触发概率 | 后果 | 根因 |
|---|---|---|---|---|
| **D1** | `Q` ⇄ `CP` | **高（有并发流量就有机会）** | 整个 hub 静默卡死 | `poll` 持队列锁做 `block_on`；`on_event` 持 `ConnProxy` 抢队列锁 |
| **D2** | `Q → CM → CP_out` ⊗ `CP_out → Q` | 中（多 hub/多 gate + 同时在重连时） | 卡死 | `add_*_proxy` 在持 `ConnManager` 时锁 `ConnProxy` |
| **D3** | `R` ⇄ `CM` | 低（当前被全局 `HubServer` 锁串行化掩盖） | 卡死 —— **一旦按性能建议去掉全局锁就立刻激活** | `entry_gate_service` 是 `R→CM`，`entry_direct_hub_server` 是 `CM→R` |
| **D4** | `Q → R → rconn(1s)` | **必然**（每次重连都踩） | 全机冻结 0.5~1s（持 GIL） | `brpop(1.0)` 在 async 任务里同步阻塞，且与所有 redis 命令共用一条连接锁 |
| **D5** | `CP → W → write_all` | 中（对端不读时） | 该连接及其发送方全部挂起；在 D1 的放大下变成全局卡死 | 写锁跨网络写；`ConnManager`/全局锁又被跨层持有 |
| D6 | GIL | 目前无环 | 任何 Rust 侧阻塞 = 所有 Python 线程一起停 | `with_server!`/`poll` 都持 GIL 做 `block_on` |
| D7 | poison / panic | 中 | 从卡死变崩溃，或反复 panic | `tx_buf` 用 `.unwrap()`，thrift 有 panic 点；`blocking_lock()` 在 async 上下文 panic |
| D8 | `try_lock` 500ms | 中（被 D4 放大） | 启动直接失败 | `HubConnMsgPump::new` 用忙等 `try_lock` |

**一句话**：锁的层次是有的，但**没有一条是被强制执行的**，而 `poll`（Python 主线程）和 reader 任务恰好用了相反的两条路径，于是形成环。

---

## 2. 锁清单

| 记号 | 实际类型 | 位置 |
|---|---|---|
| `S` | `Arc<tokio::Mutex<HubServer>>` | `lib.rs:117` |
| `CM` | `Arc<tokio::Mutex<ConnManager>>` | `hub_server.rs:32` |
| `CP` | `Arc<tokio::Mutex<ConnProxy>>`（每连接） | `hub_service_manager.rs:100-105` |
| `HP`/`GP`/`DP` | `Arc<tokio::Mutex<HubProxy/GateProxy/DBProxyProxy>>` | 各 proxy 文件 |
| `Q` | **`Arc<std::sync::Mutex<ConnCallbackMsgHandle>>`**（含无界队列） | `hub_service_manager.rs:130-139` |
| `Qdb` | `Arc<std::sync::Mutex<DBCallbackMsgHandle>>` | `dbproxy_msg_handle.rs:27-29` |
| `HM`/`GM` | `Arc<std::sync::Mutex<…>>`（**结构体是空的**） | `hub_msg_handle.rs:27`、`gate_msg_handle.rs:24` |
| `W` | `Arc<tokio::Mutex<Box<dyn NetWriter>>>` | 各 proxy 的 `wr` |
| `R` | `Arc<tokio::Mutex<RedisService>>` | `hub_server.rs:30` |
| `rconn` | `Arc<tokio::Mutex<redis::Connection>>`（`R` 内部） | `redis_service.rs:74` |
| `CU` | `Arc<tokio::Mutex<ConsulImpl>>` | `hub_server.rs:35` |
| `TB` | `Arc<std::sync::Mutex<Vec<u8>>>`（序列化缓冲） | `vec_transport.rs:19` |
| `CB` | `Arc<tokio::Mutex<Box<dyn …Callback>>>`（accept/MQ 回调对象） | `tcp_server.rs:26`、`redis_service.rs:69` |
| `CH`/`CL` | `HealthHandle` / `CloseHandle` | 叶子锁，不参与环 |

---

## 3. 锁图（持 A → 求 B）

下面每行都是「在持有 A 的前提下，代码去拿 B」，出处可逐条核对。

### 3.1 从 `poll` 出发（**持有 `Q`**，`hub_service_manager.rs:210`）

```
Q → HM / GM           66, 90, 266, 414, 508, 652, 695, 706, 719, 722, 812
Q → CP                65, 89（handle_*_event 里的 rt.block_on）
                      228, 620, 665, 728, 781
Q → CM                242, 261, 278, 348, 448, 634, 683, 795
Q → R                 291, 357, 456
Q → HP / GP           335, 435, 731, 784, 790
Q → 新建 GP → W       239, 790
Q → （Python 回调）→ S   见 D6
```

再从这些点继续：

```
CP → CM               228→242（RegServer）、620→634（Login）、665→683（Reconnect）
CM → CP               242→conn_manager.rs:106、261→118、634→106、683→118、795→118
CM → R                348→350/357、278→291
R  → CM               entry_gate_service：hub_server.rs:216→225
CP → GP → W           728→731、784
```

### 3.2 从 reader 任务出发（`on_event`）

```
CP → Q                 hub_service_manager.rs:186（拿 CP）… 195（拿 Q）
CP → W → CM            on_close：914 → 930 → 935
```

### 3.3 从「连接/服务建立」出发（都在 `S` 之下）

```
CM → CP                conn_manager.rs:106、118（add_hub_proxy / add_gate_proxy）
CM → R → rconn         hub_proxy_manager.rs:34→35
R  → CM                hub_server.rs:216→225（entry_gate_service）
CM → HP/GP → W         hub_proxy_manager.rs:64-65、hub_server.rs:239-240
CU → CM → CP           hub_proxy_manager.rs:79（CU 拿到后全程不放）→104→106
```

### 3.4 redis 后台循环

```
CL → rconn（阻塞 brpop 1.0s）→ rds → rmqrd → CB
```

---

## 4. 环路详述

### D1：`Q` ⇄ `CP`（最危险，最可能先炸）

**证据**
- `poll` 在 `hub_service_manager.rs:210` 拿住 `Q`（`std::sync::Mutex`），**一直持有到函数返回**（866）；中间 `handle_hub_event` / `handle_gate_event` 会 `rt.block_on(get_*_name_async(...))`（65 / 89），而它第一步就是 `conn_proxy.lock().await`（33 / 43）。
- reader 任务的反方向：`on_event` 先 `conn_proxy.lock().await`（186），反序列化完**还持着** `CP`，再去抢 `Q`（195）。

**精确交错**（`P` = Python 主线程持 GIL，`W` = tokio worker 跑 reader）

| 步 | 线程 P | 线程 W |
|---|---|---|
| 1 | `poll()`：`Q.lock()` ✔ | |
| 2 | `deque()` 取到 conn C 的 msg#1，进 Python 回调 | `on_event()`：`CP_C.lock()` ✔ |
| 3 | | `deserialize()`，然后 `Q.lock()` → **阻塞**（Q 在 P 手上） |
| 4 | 处理完 msg#1，返回 Python，**释放 Q** | 仍在阻塞（尚未被唤醒/尚未抢到） |
| 5 | Python `while` 立刻再进 `poll()`：`Q.lock()` ✔（抢赢） | |
| 6 | `deque()` 取到 **同一连接 C** 的 msg#2 | |
| 7 | `rt.block_on(...)` → `CP_C.lock().await` → **永久等待** | 持 `CP_C`，等 `Q` → **永久等待** |

**触发条件**：① 同一连接上至少排队 2 条消息；② 第 5 步 Python 抢到 `Q` 快于 W 恢复执行。
**概率**：`poll` 每条消息都持 `Q` 5~20µs（含 Python 回调），W 抢 `Q` 只在 `poll` 返回后那一瞬间；只要该连接有后续报文，逐消息都在掷骰子。**持续压测下几乎必然踩中**，这也是「跑一段时间就卡、重启就好」的典型形态。
**放大**：若 `num_cpus=1`，或者 runtime worker 恰好都被 `brpop`（D4）占住，W 连被调度的机会都没有，环会更快锁死。
**对照**：**DB 侧同样的结构是正确的** —— `dbproxy_msg_handle.rs:57-68` 先把 `DbCallback` 反序列化出来、**释放 `DP`**、再拿 `Qdb` 入队，所以没有环。这就是 hub 侧该照抄的写法。

### D2：`Q → CM → CP_out` ⊗ `CP_out → Q`

**证据**
- `poll` 持 `Q`，`RegServer` 分支（228→242）里持 `CP_in` 再拿 `CM`；`ClientRequestLogin`（620→634）、`ClientRequestService`（781→795）同型。
- `CM::add_hub_proxy` / `add_gate_proxy`（`conn_manager.rs:103-121`）在**持 `CM`** 时去锁 `connproxys[name]`（106 / 118），即 `CP_out`。
- `entry_gate_service`（`hub_server.rs:225-245`）持 `CM` 调 `add_gate_proxy` → 锁 `CP_out`；`entry_direct_hub_server`（`hub_proxy_manager.rs:67`）同。
- `on_event` 在 `CP_out` 上持锁等 `Q`。

**环**：`Q`（P）→ `CM` → `CP_out`（被 W 持有）→ `Q`（P）。三方各持一把，谁都走不了。
**触发条件**：hub 与同一对端存在**两条连接**（出站 `direct_connect_server` 登记进 `connproxys`，入站不登记），并且「某连接正在入队」与「另一线程正在 `add_*_proxy` 建/回填」同时发生。多 hub 互连 + 断线重连时最常见。

### D3：`R` ⇄ `CM`（现在被 `S` 掩盖，去掉全局锁就活）

- `entry_gate_service`（`hub_server.rs:216` 拿 `R` → 225 拿 `CM`）是 **R → CM**。
- `entry_direct_hub_server`（`hub_proxy_manager.rs:34` 拿 `CM` → 35 拿 `R`）是 **CM → R**。

两者互为逆序。现在不出事，**唯一原因**是它们都只在 `HubServer` 的那把全局锁 `S` 之下被调用，天然串行化了。而性能优化方案 ② 的核心就是**去掉这把 `S`** —— 一旦去掉，这两条路径就能并发，环立刻成立。
**所以：D3 是「性能改造的前置条件」，不是「可以以后再说」。**

### D4：`rconn` 被阻塞 `brpop` 占用 1s（必然发生，且持 GIL）

`crates/redis_service/src/redis_service.rs:84-107`：
```rust
let _join = tokio::spawn(async move {
    loop {
        { let _c = conn.as_ref().lock().await; _c.brpop(pop_lname, 1.0) }   // 96-97：同步阻塞调用！
```
- `brpop` 是 **同步** 的 redis 客户端调用，超时 1.0s，在 async 任务里直接阻塞住一个 runtime worker；
- 而 `acquire_lock` / `release_lock` / `set` / `get`（181 / 217 / 246 / 271）用的是**同一个** `conn`（`self.conn`），所以**每次 redis 操作都要排在 `brpop` 后面，最坏等 1s**；
- 这些调用出现在 `entry_gate_service`（持 `CM`）、`release_lock`（`hub_service_manager.rs:292`，持 `Q`）、`flush_hub_host_cache`、`listen_hub_service` —— 也就是说 **`Q` 和 `CM` 会被一起按住 1s**，而入口还是 Python 主线程持 GIL 在 `block_on`。D1 的窗口被拉长 5 个数量级。

这不是环，但**效果等同于卡死**，而且会给 D1/D2 制造出巨大的时间窗口。

### D5：写锁跨网络写

`vec_transport.rs:58-78` 里 `wr.lock().await` 持有到 `write_all` 返回；`TcpWriter::send`（`tcp_socket.rs:86-108`）在 socket 发送窗口满时会长时间挂起。`send_gate_msg` 又是**持 `CM` + 持 `S`** 走进去的（`hub_server.rs:357-360`），`with_server!` 外面还套着 GIL。对端一个不读的 gate 就能把整个 hub 拖住——叠加 D1 时，这会直接从「某条连接慢」升级为「全 hub 卡死」。

---

## 5. 其它会伪装成死锁的隐患

### D6：GIL 不构成环，但是放大器

我核对了 `pyo3-async-runtimes 0.26` 的完成路径（`generic.rs:599-649`）：future 完成后，**worker 线程会 `Python::attach` 去 `set_result`**。好消息是 `set_result` 之前 async 块内的 guard 已经 drop，所以「持锁等 GIL」不成立，**目前没有 GIL 环**。
但：`poll`（`hub_service_manager.rs:205`）和 `with_server!`（`lib.rs:130-139`）都是**持 GIL 做 `block_on`**。这意味着任何 Rust 侧等待都会冻结**所有** Python 线程（主循环 + asyncio 协程 + db 泵）。所以 D1/D4/D5 一旦发生，`py-spy` 看到的会是「Python 全停」。
⚠ 后续若在 runtime 任务里持有 `S`/`CM` 时调用 Python 回调（例如把发送改成 `future_into_py`），就会变成真正的 GIL 死锁。

### D7：panic → poison → 从卡死变崩溃

- `vec_transport.rs:38, 90, 99` 对 `TB` 用的是 `.unwrap()`。而 thrift 序列化存在 panic 点：`compact.rs:460-465`（pending bool 重入 panic）、`471`（`expect("non-stop field should have field id")`）。一旦在持锁期间 panic，`TB` 被毒化 → 之后**每次发送都 panic**。
- `redis_service.rs:204/233/258/280/305` 在 async fn 里调 `self.client.blocking_lock()`（tokio Mutex 的阻塞 API）——**在 async 上下文里会 panic**。它位于 redis 命令失败的恢复分支上，即「redis 一抖，hub 就 panic」。

### D8：`try_lock` 忙等 500ms

`lib.rs:833-845`、`869-881`：`HubConnMsgPump::new` / `HubDBMsgPump::new` 用 `try_lock` + `sleep(10ms)` 轮询 50 次，超时返回 `PyValueError`。而 `S` 一旦被 D4 那种 1s 的 redis 等待占住（`S` 是在 `with_server!` 里被占的），**启动阶段就会直接失败**。

---

## 6. 修复方案（按顺序）

### 第 1 步（必做，20 行以内）：拆掉 D1 的两条边

**a) `poll`：先整体换出队列、释放 `Q`，再逐条处理**
```rust
// hub_service_manager.rs:205
pub fn poll(handle: Arc<StdMutex<ConnCallbackMsgHandle>>, py: Python<'_>, py_handle: Py<PyAny>) -> usize {
    // 1) 只在这段持 Q
    let (events, rt) = {
        let mut h = handle.as_ref().lock().unwrap_or_else(|e| e.into_inner());
        let rt = h.rt_handle.clone().expect("rt_handle not set");
        (h.drain_all(), rt)          // Vec<Box<ConnEvent>>，Queue 加一个 drain
    };                               // ← Q 在这里就释放了
    // 2) 之后的 block_on / Python 回调都不再持 Q
    for ev in events { Self::dispatch_one(ev, &rt, py, py_handle, &handle); }
}
```
顺带把「一次一条消息」变成「一次一批」，正好也是性能方案 ①(b)。
⚠ 注意：`dispatch_one` 需要把 `hub_msg_handle`/`gate_msg_handle` 从 `Q` 里取出来（它们现在是 `Q` 的字段），单独 `Arc` 存一份即可；`HM`/`GM` 是空结构体，建议**直接删掉这层锁**。

**b) `on_event`：照抄 DB 侧的正确写法**（`dbproxy_msg_handle.rs:57-68`）
```rust
// hub_service_manager.rs:182
let ev = { let _p = _proxy.lock().await; deserialize(data)? };   // 先释放 CP
let handle = /* 单独存一份 Arc，不要从 CP 里取 */;
handle.lock().unwrap_or_else(|e| e.into_inner()).enque_event(ConnEvent {
    connproxy: Arc::downgrade(&_proxy),
    ev,
});
```
`msg_handle` 直接在 `ConnProxy::new` 时另存一个 `Arc` 到 `ConnProxyReaderCallback` 里，就不需要「为了拿 handle 而锁 CP」。这样 `CP → Q` 这条边彻底消失。

做完 a+b，D1 双向边都没了，D2 也自动拆掉一半。

### 第 2 步：统一锁序并写进模块文档

建议锁序（从外到内，禁止逆序）：
```
S  →  R  →  CM  →  CP  →  {HP, GP, DP}  →  W
                                  TB 只贴着最内层，且不跨 await
Q / Qdb 是「叶子锁」：只允许在完全没有其它锁、也不跨 await 的短临界区里使用
```
然后逐条消灭逆序：
- `CM::add_hub_proxy` / `add_gate_proxy`（`conn_manager.rs:103-121`）：**不要**在持 `CM` 时锁 `CP`。改成「持 `CM` 只做 `insert`，把回填用的 `Arc` clone 出来，`drop(CM)` 之后再锁 `CP` 回填」。
- `entry_gate_service`（`hub_server.rs:201-253`）现在是 `S → R → CM → CP`，而 `entry_direct_hub_server` 是 `S → CM → R`：统一成 `S → R → CM`（或更彻底：**把 redis/connect 全部挪到锁外**，见第 3 步）。

### 第 3 步：不要在持任何锁时做网络/redis I/O

`hub_proxy_manager.rs:34-68`、`79-152`、`dbproxy_manager.rs:56-78` 都是「持 `CM` 甚至持 `CU` 跨 `connect`/`acquire_lock`」。改成标准的 check–act–recheck：

```
{ 持锁查表 } → miss → drop(锁) → await 连接/redis → 再持锁二次检查 → insert
```
副作用是可能出现「重复建连」，用现有的 `locks` 表 + 幂等检查兜住即可。

### 第 4 步：redis 侧

- `brpop` 换 `tokio::task::spawn_blocking`，或用 `redis::aio::MultiplexedConnection`；
- **读写分离**：`brpop` 循环独占一条连接，`acquire_lock`/`set`/`get` 用另一条，不要和 1s 的阻塞调用共享 `conn`；
- 去掉 `blocking_lock()`（D7）。

### 第 5 步：把「卡死」变成「可观测」

- 所有 `tokio::sync::Mutex` 的获取套一层带名字的看门狗（debug 构建）：
  ```rust
  async fn timed_lock<'a, T>(name: &'static str, m: &'a Mutex<T>) -> MutexGuard<'a, T> {
      tokio::select! {
          g = m.lock() => g,
          _ = tokio::time::sleep(Duration::from_millis(500)) => {
              tracing::error!("lock wait > 500ms: {name}");
              m.lock().await       // 继续等，但日志已经指出是谁
          }
      }
  }
  ```
  卡死时日志最后一行就是被谁挡住的。
- `std::sync::Mutex` 全部换 `unwrap_or_else(|e| e.into_inner())`（`vec_transport.rs` 目前是 `.unwrap()`）。

---

## 7. 怎么验证「真的会卡」

修复前后都建议跑这组用例（不需要真实客户端）：

1. **最小复现（针对 D1）**：起一个假 gate，用**同一条** TCP 连接连发 200 条 `ClientCallNtf`（不等待），hub 侧 handler 故意 `time.sleep(0.0002)` 拉长 `Q` 的持有时间。预期：几十毫秒内卡死 —— 修复后应稳定跑完。
2. **D4 复现**：把 redis 的 `brpop` 超时保持 1.0s，同时触发 `entry_gate_service`，用日志量出「`Q` 持有时间 > 500ms」。
3. **卡死取证**：进程挂住时执行 `py-spy dump --pid <pid>`。
   - 若主线程停在 `poll_conn_msg` / `hub_call_client_ntf`（`with_server!`）里 → 基本就是 D1/D4；
   - 若 asyncio 线程停在 `entry_hub_service` → 看 `R`/`CM` 是否被占；
   - 需要原生栈时用 `procdump -ma` + WinDbg `~*kb`，看 worker 是否停在 `Mutex::lock`（std）或 `write_all`（D5）。
4. **长期回归**：把第 5 步的看门狗日志接到告警，> 500ms 的锁等待即视为回归。

---

## 8. 改动清单（可直接派工）

| # | 文件:行 | 改什么 | 解决 |
|---|---|---|---|
| 1 | `hub_service_manager.rs:205-217` | `poll` 先 drain 再处理，全程不持 `Q` | D1a |
| 2 | `hub_service_manager.rs:186-202` | `on_event` 释放 `CP` 后再入队 | D1b |
| 3 | `hub_service_manager.rs:130-139` + `queue` | 提供 `drain()`；`msg_handle` 不再从 `CP` 取 | D1 |
| 4 | `hub_msg_handle.rs:27`、`gate_msg_handle.rs:24` | 删掉两个空结构体的 mutex | 缩短临界区 |
| 5 | `conn_manager.rs:103-121` | `add_*_proxy` 不在持 `CM` 时锁 `CP` | D2 |
| 6 | `hub_server.rs:216-225` vs `hub_proxy_manager.rs:34-35` | 统一 `R`/`CM` 顺序 | D3 |
| 7 | `hub_proxy_manager.rs:34-68, 79-152`、`dbproxy_manager.rs:56-78` | 锁外做 connect/redis | D3/D4 窗口 |
| 8 | `redis_service.rs:96` | `brpop` → `spawn_blocking` / aio 连接；读写分离 | D4 |
| 9 | `redis_service.rs:204/233/258/280/305` | 去掉 `blocking_lock()` | D7 |
| 10 | `vec_transport.rs:38,90,99` | `.unwrap()` → `into_inner()` | D7 |
| 11 | `lib.rs:130-139`、`hub_service_manager.rs:205` | 锁等待看门狗 | D5/验证 |

---

## 9. 修复记录（已落地，`cargo check` 通过）

> 状态以本节为准；上面第 2~8 节保留为分析与方案。改动仅涉及 7 个文件，未改任何对外 API / 协议 / Python 侧调用方式。

### 9.1 已修复

| 项 | 修复方式 | 涉及文件 |
|---|---|---|
| **D1** `Q` ⇄ `CP` | `poll` 改成「持 `Q` 一次性 `drain_events(≤1024)` → **释放 `Q`** → 逐条 `dispatch_one`」，分发上下文 `PollState` 是纯 `Arc` 快照；`on_event` 改成「先无锁反序列化 → 短暂持 `CP` 只取 `msg_handle` → 释放 → 再入队」，`get_msg_handle` 改 `&self`。`Q` 自此只在「取空队列」这一瞬持有，成为叶子锁 | `hub_service_manager.rs` |
| **D2** `CM → CP` | `ConnManager::add_hub_proxy` / `add_gate_proxy` 改为**同步纯插入**，不再持 `CM` 去锁 `ConnProxy`。出站连接的对端名本已由 `ConnProxyCloseCallback::name` 记录，原回填是冗余的；顺带消除了「同一条连接自我死锁」这一潜在变体 | `conn_manager.rs` + 7 处调用点 |
| **D2′**（本轮新发现）`{HP,GP} → CM` | `HubForwardClientRequestService(Ext)` 两个分支原来「持 `HubProxy` 再抢 `CM`」，与 `send_hub_msg` 的 `CM → HP` 相反。改成「读名字时就地释放 HP，再拿 CM」；`RegServer` / `ClientRequestReconnect` / `ClientRequestService` 三处「持新 proxy 再拿 CM」也一并拆成「写字段 → 入表 → 发回执」三段，杜绝坏样板 | `hub_service_manager.rs` |
| **D3** `R` ⇄ `CM` | `entry_gate_service` 拆成三段：**只持 R 取锁 → 只持 CM 建连登记 → 只持 R 释放锁**，任何时刻都不同时持有 `R` 和 `CM`（不是靠顺序约定，而是结构上不存在），比原方案更彻底，且缩短了 `CM` 的持有时间 | `hub_server.rs` |
| **D4** `brpop` 阻塞 1s | `RedisService::listen` 起第二条专用连接给 `brpop`（命令连接与阻塞连接分离），并把 `brpop` 丢进 `spawn_blocking` + `lock_owned()`，不再占 runtime worker | `redis_service.rs` |
| **D7** poison / panic | `TVecTransport` 的三处 `.unwrap()` → `unwrap_or_else(into_inner)`；`acquire_lock`/`release_lock`/`set`/`get`/`expire` 五处 `blocking_lock()`（async 上下文会 panic）→ `self.client.lock().await` | `vec_transport.rs`、`redis_service.rs` |
| **D8** 启动期 `try_lock` 忙等 | `HubContext` 直接持有两个消息泵句柄；`HubConnMsgPump::new` / `HubDBMsgPump::new` 不再 `try_lock(server)`，删掉 500ms 超时失败路径 | `lib.rs` |
| 锁序文档化 | `hub_service_manager.rs` 顶部加 `//!` 锁序不变量（含 `Q` 是叶子锁、`CM` 不得锁 `CP`、`R`/`CM` 不同时持有、禁止持锁 await I/O） | `hub_service_manager.rs` |

### 9.2 修复后的锁图（应为无环）

```
S → CU → CP → CM → R → Q′…            （Q/Qdb/GM/HM 均为叶子，不参与环）
                  ↘ {HP, GP, DP} → W → TB
```

逐对核对过的「曾经成环」候选：`Q↔CP`（已无 `Q` 出边）、`CM↔CP`（已无 `CM→CP`）、`CM↔HP/GP`（纯 `CM→proxy`）、`R↔CM`（已不同时持有）、`CP→CM` 是唯一方向。

### 9.3 第二轮：把「等价于卡死」的四项也修掉

| 项 | 修复方式 | 涉及文件 |
|---|---|---|
| **D5** 写锁跨网络写 | 新增 `peer_writer` 模块：每条连接一个写任务 + 有界 `mpsc`（8192）。`GateProxy` / `HubProxy` 的 `send_*` 变成**同步**的「序列化 → `try_send`」，既不 `await`、也不持写锁；写任务把队列里已就绪的整包拼起来，一次 `write_all` 发多条（帧头天然支持拼接）。新增 `NetWriter::send_framed`（TCP 实现直接原样写，不再整包拷贝）与 `net::vec_transport::{VecWrite, serialize_framed}`（序列化全程无锁、自带 4 字节帧头）。写失败时主动 `close`，让读循环走回收路径 | `peer_writer.rs`(新)、`gate_proxy_manager.rs`、`hub_proxy_manager.rs`、`crates/net`、`crates/tcp` |
| **持 `CM` 跨 `TcpConnect`** | 拆掉 `ConnManager::direct_connect_server(&mut self, …)`（它在持锁时 `await` 建连，超时 5s），改为自由函数 `conn_manager::connect_server(&Arc<WatchMutex<ConnManager>>, …)`：**查表（短锁）→ 建连（无锁）→ 登记（短锁，带二次检查）**。上层新增 `ensure_gate_connected` / `ensure_hub_connected` 两个「分段持锁」的建连流程，5 个调用点全部迁移；门/中继/consul 三条建连路径都不再持锁做 I/O | `conn_manager.rs`、`hub_service_manager.rs`、`hub_proxy_manager.rs`、`hub_server.rs` |
| 空结构体 mutex | `HubCallbackMsgHandle` / `GateCallbackMsgHandle` 改为**无锁**（`Arc<T>` + 直接构造 `T {}`），删掉 9 处加锁与 `ConnCallbackMsgHandle.close` 冗余字段。这消除了「Python handler 重入 `poll_conn_msg` → 同一把 std 锁二次加锁」的自我死锁路径 | `hub_msg_handle.rs`、`gate_msg_handle.rs`、`hub_service_manager.rs` |
| 锁等待看门狗 | 新增 `lock_watch::WatchMutex<T>`（`lock()` 返回原生 `MutexGuard`，调用点零改动）：等待超过 500ms 打 warn 并继续等，拿到后再记录总等待时长。已套在 4 把关键锁上：`HubServer`、`ConnManager`、`ConnProxy`、`RedisService` | `lock_watch.rs`(新)、`lib.rs`、`conn_manager.rs`、`hub_server.rs`、`hub_service_manager.rs`、`hub_proxy_manager.rs`、`dbproxy_manager.rs` |

**修完后的锁序（`hub_service_manager.rs` 顶部有完整版）**：
```
S(HubServer) → CU(Consul) → CP(ConnProxy) → CM(ConnManager) → R(RedisService)
             → {HP, GP, DP} → W(wr)
Q / Qdb / GM / HM：叶子锁，绝不跨 await、绝不回调 Python 时持有
```
`S` / `CM` / `CP` / `R` 均为 `WatchMutex`，卡住时会留下「等哪把锁等了多久」的日志。

### 9.4 已知取舍（不是 bug，但需要知情）

1. **队列满会丢消息**：`peer_writer` 的队列上限 8192 条；打满说明该对端已严重滞后（例如不读 socket），此时丢弃并每 1024 条记一次 error，而不是把调用方无限阻塞。若某类消息不允许丢，应改成"队列满即断开该连接"或对 RPC/RSP 单独用阻塞发送。
2. **`send_*` 的返回值语义变了**：`true` 现在表示「已进入发送队列」，不代表对端已收到（原先也只是写进内核缓冲）。
3. **写失败即断连**：写任务失败会主动 `close`，由读循环触发 `ConnProxyCloseCallback` 回收；这与旧行为（返回 false 但连接还挂着）不同，属于有意的改进。

### 9.5 本次验证到什么程度

- ✅ `cargo check`（`server` 全工程：hub / net / redis_service / tcp）**零 error、零 warning**；`client`（依赖 `net`/`tcp`）亦通过。
- ⚠ **没有做运行时验证**：仓库 `server/lib/hub` 零测试（无 `#[test]`），且跑通 hub 需要 consul + redis + 一个 gate。建议按第 7 节的用例补一次压测：同一条连接连发 200 条 `ClientCallNtf` + handler 里 `sleep(0.2ms)`，修复前应卡死、修复后应稳定跑完；再补一个「假 gate 不读 socket」的用例，确认其它 peer 的 RTT 不再受影响（D5 回归）。


