# 修复 18 项 Critical 级别问题

## Context

代码库全面审计发现 18 项 Critical 级别问题，涉及服务端 tokio Runtime 管理、Redis 分布式锁、渲染管线性能、场景图热路径、编辑器阻塞、网络安全等多个模块。这些问题可导致死锁、OOM、崩溃、严重性能下降等后果，需优先修复。

---

## Task 1: 服务端 tokio Runtime 整合（Issues #1, #5）

**涉及文件**：
- `server/lib/hub/src/lib.rs`（1070 行）
- `server/lib/hub/src/hub_service_manager.rs`（943 行）

**修复方案**：

### lib.rs — 38 个 pymethod 复用已有 Runtime
- HubContext 已持有 `_listen_rt: tokio::runtime::Runtime`（行 111）
- 所有 pymethod 中 `tokio::runtime::Runtime::new().unwrap()` 替换为 `self._listen_rt.handle().block_on(...)`
- 涉及行号：206-210, 217-221, 226-231, 245-267 等共 38 处
- 提取辅助方法封装复用

### lib.rs — blocking_lock 修复（行 1036, 1058）
- `HubConnMsgPump::new` 和 `HubDBMsgPump::new` 中 `blocking_lock()` 改为 `try_lock()` + 重试 + 超时

### hub_service_manager.rs — poll() 接受 Runtime Handle
- `ConnCallbackMsgHandle` 结构体添加 `rt_handle: tokio::runtime::Handle` 字段
- 在创建时传入 `_listen_rt.handle().clone()`
- poll() 内 19 个 match 分支的 `Runtime::new().unwrap() + block_on` 替换为 `self.rt_handle.block_on(...)`
- 提取辅助方法 `get_hub_name` / `get_gate_name` 消除重复

**验证**：`cd server && cargo build`

---

## Task 2: Redis 服务修复（Issues #2, #3, #4）

**涉及文件**：`crates/redis_service/src/redis_service.rs`（304 行）

**修复方案**：

### 分布式锁原子化（行 181-248）
- `acquire_lock`：用 `SET key value EX seconds NX` 原子命令替换 `set_nx` + `expire` 两步
- `release_lock`：用 Lua 脚本替换 `get + compare + del` 三步操作

### 添加超时（6 个方法，行 181-303）
- 所有 `loop {}` 添加最大重试次数参数（默认 100 次 × 5ms = 500ms）
- 超时后返回 `Result::Err`

### 同步连接改 spawn_blocking
- 所有 `client.blocking_lock().get_connection()` + 同步命令包装在 `tokio::task::spawn_blocking` 中

**验证**：`cd server && cargo build`（redis_service 被 server 引用）

---

## Task 3: 渲染管线 GPU 资源缓存（Issue #6）

**涉及文件**：
- `crates/render/src/forward_plus.rs`
- `crates/render/src/deferred_plus.rs`
- `crates/render/src/common.rs`

**修复方案**：

### 引入 GPU 资源缓存
- 在 `RenderCommand` 或上层添加 GPU 缓存 HashMap（key = mesh/texture ID）
- `build_command` 先查缓存：命中则 `write_buffer` 更新；未命中则创建并缓存
- 使用 dirty flag 标记需更新的 buffer
- 纹理按路径/hash 缓存 `wgpu::Texture` + `wgpu::TextureView`

### depth_target.expect 修复
- `forward_plus.rs` 行 354、`deferred_plus.rs` 行 420：提供默认 depth texture 或返回错误

**验证**：`cd desktop && cargo build`

---

## Task 4: 场景图热路径优化（Issue #7）

**涉及文件**：`crates/scene/src/scene.rs`

**修复方案**：
- `update_node_world`（行 498-526）：消除 `children.clone()` 和 `objects.clone()`，使用索引迭代
- `remove_object_internal`（行 574-589）：可选添加 HashMap 索引优化

**验证**：`cd crates/scene && cargo test`

---

## Task 5: 粒子系统 O(1) 分配（Issue #8）

**涉及文件**：`crates/vfx/src/lib.rs`

**修复方案**：
- 添加 `free_indices: Vec<usize>` free list
- 发射时 `free_indices.pop()` O(1)，死亡时 `free_indices.push(idx)` 回收
- 移除 `.iter().position(|p| !p.alive)` 线性搜索

**验证**：`cd desktop && cargo build`

---

## Task 6: 物理客户端条件变量（Issue #9）

**涉及文件**：`crates/physics_client/src/lib.rs`

**修复方案**：
- 添加 `condvar: Condvar` + `mutex: Mutex<()>` 字段
- `wait` 改用 `condvar.wait_timeout_while`
- 完成时调用 `condvar.notify_all()`

**验证**：`cd crates/physics_client && cargo build`

---

## Task 7: 地形溢出 + 动画索引检查（Issues #10, #17）

**涉及文件**：
- `crates/terrain/src/lib.rs`（行 158-192）
- `crates/avatar/src/animation.rs`（行 254-317）

**修复方案**：

### 地形 f32→i32 安全转换
- 添加 `.clamp(i32::MIN as f32, i32::MAX as f32)` 边界检查

### 动画 CubicSpline 边界检查
- 索引访问添加 `.get()` 检查，越界返回默认值 + warn 日志

**验证**：分别 cargo build

---

## Task 8: 编辑器 Critical 修复（Issues #11, #14, #15, #16）

**涉及文件**：
- `crates/editor/src/editor.rs`（行 1074, 1078）
- `crates/editor/src/viewport.rs`（射线拾取）
- `desktop/src/desktop_app.rs`（关闭处理）

**修复方案**：

### Play 模式 block_on 改为非阻塞（行 1074, 1078）
- 改为 spawn + 回调通知或 try_recv 非阻塞模式

### 射线拾取矩阵求逆回退
- 求逆失败返回 None，跳过拾取

### Undo 栈优化
- O(n) 栈操作改为 O(1)（VecDeque 或指针索引）

### 关闭拦截后继续渲染
- 取消关闭后确保渲染循环正常继续

**验证**：`cd desktop && cargo build`

---

## Task 9: 网络安全加固（Issue #13）

**涉及文件**：
- `crates/net/src/lib.rs`（行 49-59）
- `crates/tcp/src/tcp_server.rs`（行 33-58）

**修复方案**：

### try_get_pack 添加消息大小上限（16MB）
### TCP 连接基础握手验证

**验证**：分别 cargo build

---

## Task 10: Thrift 反序列化安全化（Issue #18）

**涉及文件**：
- `server/lib/hub/src/gate_msg_handle.rs`（41 处 unwrap）
- `server/lib/hub/src/hub_msg_handle.rs`（43 处 unwrap）
- `server/lib/hub/src/dbproxy_msg_handle.rs`（15 处 unwrap）

**修复方案**：
- 99 处 `.unwrap()` 替换为 `.unwrap_or_default()` 或模式匹配
- 关键字段缺失时 warn + return
- 可选字段使用空默认值

**验证**：`cd server && cargo build`

---

## Task 11: test_launcher.py 修复（Issue #12）

**涉及文件**：`test_launcher.py`

**修复方案**：
- 导入不存在的 `launch` 改为导入实际存在的函数
- 调整脚本逻辑

**验证**：`python test_launcher.py`

---

## 执行依赖关系

- Task 1, 2, 4, 5, 6, 7, 11 — 互不依赖，可并行
- Task 3 — 独立但复杂度高
- Task 8 — 独立
- Task 9, 10 — 独立

**推荐批次**：
1. 第一批并行：Task 1, 2, 4, 5, 6, 7, 11
2. 第二批并行：Task 3, 8, 9, 10
3. 全部完成后统一验证三个入口项目构建

## 验证策略

- 每个 Task 完成后独立编译验证
- 全部完成后：`cd desktop && cargo build` + `cd server && cargo build` + `cd client && cargo build`
- 运行已有测试：`cd crates/scene && cargo test`, `cd crates/physics && cargo test`
