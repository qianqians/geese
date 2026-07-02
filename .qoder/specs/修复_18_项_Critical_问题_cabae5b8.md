# 修复 18 项 Critical 级别问题

## Context

代码库审计发现 18 项 Critical 问题，可导致死锁/OOM/崩溃/严重性能下降。分解为 11 个独立任务并行修复。

## Task 1: 服务端 tokio Runtime 整合（Issues #1, #5）
- `server/lib/hub/src/lib.rs`：38 个 pymethod 复用已有 `_listen_rt` 的 Handle，不再每次创建新 Runtime
- `server/lib/hub/src/hub_service_manager.rs`：poll() 19 个分支改用传入的 `Handle`；提取 `get_hub_name`/`get_gate_name` 辅助函数
- `lib.rs` 行 1036, 1058：`blocking_lock()` 改为 `try_lock()` + 超时

## Task 2: Redis 服务修复（Issues #2, #3, #4）
- `crates/redis_service/src/redis_service.rs`：
  - acquire_lock 用 `SET NX EX` 原子命令；release_lock 用 Lua 脚本
  - 6 个方法添加最大重试次数/超时
  - 同步连接操作包装 `spawn_blocking`

## Task 3: 渲染管线 GPU 资源缓存（Issue #6）
- `crates/render/src/{forward_plus,deferred_plus,common}.rs`：
  - 引入 GPU buffer/texture 缓存，build_command 先查缓存
  - depth_target.expect 改为安全处理

## Task 4: 场景图热路径优化（Issue #7）
- `crates/scene/src/scene.rs`：update_node_world 消除 clone，使用索引迭代

## Task 5: 粒子系统 O(1) 分配（Issue #8）
- `crates/vfx/src/lib.rs`：引入 free_indices free list

## Task 6: 物理客户端条件变量（Issue #9）
- `crates/physics_client/src/lib.rs`：忙等待改为 Condvar

## Task 7: 地形溢出 + 动画索引（Issues #10, #17）
- `crates/terrain/src/lib.rs`：f32→i32 加 clamp
- `crates/avatar/src/animation.rs`：CubicSpline 加边界检查

## Task 8: 编辑器 Critical 修复（Issues #11, #14, #15, #16）
- `crates/editor/src/editor.rs`：Play 模式 block_on 改非阻塞
- `crates/editor/src/viewport.rs`：射线拾取求逆失败返回 None
- `desktop/src/desktop_app.rs`：关闭拦截后继续渲染

## Task 9: 网络安全加固（Issue #13）
- `crates/net/src/lib.rs`：try_get_pack 添加 16MB 上限
- `crates/tcp/src/tcp_server.rs`：添加基础握手

## Task 10: Thrift 反序列化安全化（Issue #18）
- `server/lib/hub/src/{gate,hub,dbproxy}_msg_handle.rs`：99 处 unwrap 替换为安全访问

## Task 11: test_launcher.py 修复（Issue #12）
- 修正 ImportError，调整脚本逻辑

## 执行策略
- 第一批并行：Task 1, 2, 4, 5, 6, 7, 11（互不依赖）
- 第二批并行：Task 3, 8, 9, 10
- 统一验证：desktop/server/client 三个入口项目完整构建 + 已有测试