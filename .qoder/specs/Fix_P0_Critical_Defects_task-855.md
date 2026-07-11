
# 修复 P0 级别严重缺陷

## 缺陷清单

| 编号 | 缺陷 | 严重性 | 工作量 | 类型 |
|:---:|------|:------:|:-----:|------|
| C1 | EventBus `unsafe impl Sync` + `RefCell` = UB | 严重 | 小 | unsafe/UB |
| C2+W3 | 分布式锁获取失败静默绕过 + 释放错误丢弃 | 严重 | 小 | 逻辑错误 |
| C3 | DBEvent `unsafe impl Send` 缺少类型约束 | 严重 | 小 | unsafe/UB |
| C4 | Deferred+ 管线完全缺少阴影集成 | 严重 | 小(P0) | 功能缺失 |
| C5 | `Scene::build_navmesh()` 始终返回 None | 严重 | 中 | 功能桩 |
| C6 | `Scene::evaluate_event_components()` 空函数 | 严重 | 中 | 功能桩 |

---

## 任务 1: C1 — EventBus 线程安全（RwLock 迁移）

**目标**：消除 `unsafe impl Send/Sync` + `RefCell` 导致的未定义行为风险。

**文件**：`crates/event/src/bus.rs`

**当前问题**：
- L96-L98: `channels: RefCell<HashMap<TypeId, Box<dyn AnyChannel>>>`
- L102-L103: `unsafe impl Send/Sync for EventBus` — 自相矛盾的 SAFETY 注释
- `publish(&self)` 调用 `borrow_mut()`，跨线程并发 = UB

**为什么不能简单删除 unsafe impl**：
- `EventBusBridge` trait（L206）要求 `Send + Sync`
- `Scene` 通过 `Option<Box<dyn EventBusBridge>>` 持有（scene.rs L88）
- 删除 Sync 会破坏下游使用

**修改**：
1. L30: 将 `use std::cell::RefCell` 替换为 `use std::sync::RwLock`
2. L97: `channels: RefCell<...>` → `channels: RwLock<...>`
3. L38-L45: 为 `AnyChannel` trait 添加 `Send + Sync` bound
4. L48: `EventChannel<E>` 的 `queue: RefCell<Vec<E>>` → `RwLock<Vec<E>>`
5. L61-L63: `publish()` 中 `borrow_mut()` → `write().unwrap()`
6. L73: `AnyChannel::flush()` 中 `borrow_mut()` → `write().unwrap()`
7. L116-L126: `EventBus::publish()` 中 `borrow_mut()` → `write().unwrap()`
8. L131-L141: `EventBus::subscribe()` 保持 `&mut self`，改用 `write().unwrap()`
9. L146-L151: `EventBus::flush_all()` 中 `borrow_mut()` → `write().unwrap()`
10. L154-L155: `EventBus::clear()` 中 `borrow_mut()` → `write().unwrap()`
11. L159-L161: `EventBus::event_type_count()` 中 `borrow()` → `read().unwrap()`
12. L100-L103: **删除** `unsafe impl Send/Sync for EventBus`

**并发语义验证**：
- `publish()`: 写锁 → 安全并发入队
- `subscribe()`: 写锁（通过 `&mut self`）→ 独占访问
- `flush_all()`: 写锁（通过 `&mut self`）→ 独占分发
- `event_type_count()`: 读锁 → 共享读取

**验证**：`cd crates/event && cargo test`

---

## 任务 2: C3 — DBEvent Send 类型约束

**目标**：将 `unsafe impl Send for DBEvent` 替换为编译期 trait 约束。

**文件**：`server/lib/dbproxy/src/db.rs`

**修改**：
1. L1: 添加 `use std::any::Any`（已有，确认存在）
2. L136-L143 之后: 添加 trait 定义：
```rust
/// DBEvent 事件数据的类型安全 trait。
/// 要求 Send 以确保可跨线程传递。
pub trait DBEventData: Any + Send {
    fn as_any(&self) -> &dyn Any;
}
impl<T: Any + Send> DBEventData for T {
    fn as_any(&self) -> &dyn Any { self }
}
```
3. L142: `ev_data: Box<dyn Any>` → `ev_data: Box<dyn DBEventData>`
4. L145: **删除** `unsafe impl Send for DBEvent {}`
5. L148: `DBEvent::new()` 参数 `_ev_data: Box<dyn Any>` → `_ev_data: Box<dyn DBEventData>`
6. L185, L216, L247, L292, L324, L405: 将 `self.ev_data.downcast_ref::<DBEvXxx>()` 改为 `self.ev_data.as_any().downcast_ref::<DBEvXxx>()`

**向后兼容**：所有已有 `DBEv*` 结构体仅包含 `Vec<u8>`/`bool`/`u32`，天然满足 `Send`。所有调用处创建 DBEvent 时使用 `Box::new(DBEvXxx::new(...))`，编译器会自动推断 `DBEventData` 实现。

**验证**：`cd server && cargo check -p dbproxy`

---

## 任务 3: C2+W3 — 分布式锁错误处理

**目标**：修复锁获取失败静默绕过 + 释放错误静默丢弃。

**涉及文件**：
- `server/lib/hub/src/hub_service_manager.rs`（L289, L355 + W3 释放点）
- `server/lib/hub/src/hub_server.rs`（L188 + W3 释放点 L212）
- 可能还有 `hub_proxy_manager.rs` 中的 W3 释放点

### 3a: 锁获取 — 替换 `unwrap_or_default()`

**hub_service_manager.rs L289**（`HubForwardClientRequestService`）：
```rust
// 原代码:
let value = _service.acquire_lock(_lock_key.clone(), 3, None).await.unwrap_or_default();

// 修复为:
let value = match _service.acquire_lock(_lock_key.clone(), 3, None).await {
    Ok(v) => v,
    Err(e) => {
        error!("Failed to acquire lock for gate '{}': {}", _gate_name, e);
        // 锁获取失败，不应继续连接 gate
        return hub_name;
    }
};
```
注意：由于这是在 `async move` 闭包内，`return hub_name` 从闭包返回（不影响外层），后续代码不会执行。

**hub_service_manager.rs L355**（`HubForwardClientRequestServiceExt`）：
同样的修复模式，传入 `_gate_name` 用于错误日志。

**hub_server.rs L188**（`entry_gate_service`）：
```rust
// 原代码:
let value = _service.acquire_lock(lock_key.clone(), 3, None).await.unwrap_or_default();

// 修复为:
let value = match _service.acquire_lock(lock_key.clone(), 3, None).await {
    Ok(v) => v,
    Err(e) => {
        error!("Failed to acquire lock for gate '{}': {}", _gate_name, e);
        return; // 提前返回，不连接 gate
    }
};
```

### 3b: 锁释放 — 替换 `let _ =`

需要搜索所有 `let _ = ...release_lock(...)` 位置：

**hub_service_manager.rs L257**（`RegServerCallback`）：
```rust
// 原代码: let _ = _service.release_lock(lock_key, value, None).await;
// 修复为:
if let Err(e) = _service.release_lock(lock_key, value, None).await {
    error!("Failed to release lock '{}': {}", lock_key, e);
}
```

**hub_service_manager.rs L314**（`HubForwardClientRequestService` else 分支）：
同上修复模式。

**hub_service_manager.rs L380**（`HubForwardClientRequestServiceExt` else 分支）：
同上修复模式。

**hub_server.rs L212**（`entry_gate_service` else 分支）：
```rust
// 原代码: let _ = _service.release_lock(lock_key.clone(), value, None).await;
// 修复为:
if let Err(e) = _service.release_lock(lock_key.clone(), value, None).await {
    error!("Failed to release lock for gate '{}': {}", _gate_name, e);
}
```

**验证**：执行 `grep` 搜索确认不再有 `let _ = ...release_lock` 或 `unwrap_or_default()` 在锁操作中。

---

## 任务 4: C4 — Deferred+ 阴影缺失警告

**目标**：当用户选择 Deferred+ 渲染路径时，明确警告阴影不可用。

**文件**：`crates/render/src/deferred_plus.rs`

**背景分析**：
- Forward+ 管线有完整的 `ShadowPass` + `WgpuShadowAtlas` 集成（forward_plus.rs L71-L72, L437-L465, L567-L573）
- Deferred+ 管线缺少所有阴影相关字段（deferred_plus.rs L31-L80）
- 完整阴影集成需要：添加 `shadow_pass`/`shadow_atlas` 字段、创建 `enable_shadows()`/`update_shadows()` 方法、修改 `render()` 添加 shadow pass、修改 shader 采样 shadow map——这是"大"工作量

**P0 最小修复**：
1. 在 `DeferredPlusPipeline` 结构体添加字段：`shadow_warned: bool`
2. 在 `DeferredPlusPipeline::new()` 中初始化为 `false`
3. 添加 `pub fn enable_shadows(&mut self, ...)` 方法：
```rust
/// Warns that CSM shadows are not yet implemented for the Deferred+ path.
/// Use Forward+ for shadow mapping.
pub fn enable_shadows(&mut self, _device: &wgpu::Device, _config: &CascadeConfig) {
    if !self.shadow_warned {
        log::warn!(
            "DeferredPlusPipeline: CSM shadow mapping is not yet implemented. \
             Shadows will not appear. Use RenderingPath::ForwardPlus for shadows."
        );
        self.shadow_warned = true;
    }
}
```
4. 添加 `pub fn update_shadows(&self, ...)` 空方法以保持 API 兼容：
```rust
pub fn update_shadows(&self, _queue: &wgpu::Queue, _cascade_vps: &[[[f32; 4]; 4]], _csm_uniform: &CsmUniform) {
    // Shadows not implemented for Deferred+; use Forward+ for CSM.
}
```

**注意**：如果 `ScenePipeline` trait 不要求这些方法，则只需在 `DeferredPlusPipeline` 上添加即可。调用方通常在外部判断渲染路径后再调用阴影相关方法。

**验证**：`cd crates/render && cargo check`

---

## 任务 5: C5 — NavMesh 自动构建

**目标**：实现 `build_navmesh()` 从场景对象提取三角形数据构建导航网格。

**文件**：`crates/scene/src/scene.rs`

**当前状态**：
- L633-L639: 桩实现，仅设置 `self.navmesh = None`
- NavMesh crate 已有完整的 `from_triangles(vertices: Vec<Vec2>, triangles: Vec<NavTri>)` API
- Scene 有 `self.objects`（`Vec<SceneObject>`）和 `self.object_index`（`HashMap<String, usize>`）

**修改**：
1. 添加 dirty 追踪变量。在 Scene struct 中添加字段（L94 附近）：
```rust
/// build_navmesh 是否需要重新构建
#[cfg(feature = "navmesh")]
navmesh_dirty: bool,
```
2. 在 `Scene::new()` 中初始化为 `true`
3. 在添加/移除对象的操作中设置 `navmesh_dirty = true`
4. 重写 `build_navmesh()` 方法（L633-L639）：
```rust
#[cfg(feature = "navmesh")]
pub fn build_navmesh(&mut self) {
    // 从标记了 navmesh 组件的 SceneObject 提取三角形数据构建 NavMesh
    let mut vertices: Vec<navmesh::Vec2> = Vec::new();
    let mut triangles: Vec<navmesh::NavTri> = Vec::new();

    for obj in &self.objects {
        // 检查对象是否有 navmesh 组件（通过 scripts HashMap 中的 navmesh 标记）
        // 初期实现：遍历所有静态对象，提取其 mesh 三角形
        if let Some(node_idx) = obj.node {
            if node_idx < self.nodes.len() {
                // 从 ModelMesh 提取三角形顶点
                let mesh = &obj.mesh;
                let world_matrix = self.nodes[node_idx].world_transform;
                let base_idx = vertices.len() as u32;

                // 提取 position 数据并应用世界变换
                for pos in &mesh.positions {
                    let world_pos = world_matrix * cgmath::Vector4::new(pos[0], pos[1], pos[2], 1.0);
                    vertices.push(navmesh::Vec2::new(world_pos.x, world_pos.z));
                }

                // 提取三角形索引
                for i in (0..mesh.indices.len()).step_by(3) {
                    if i + 2 < mesh.indices.len() {
                        triangles.push(navmesh::NavTri::new(
                            base_idx + mesh.indices[i] as u32,
                            base_idx + mesh.indices[i + 1] as u32,
                            base_idx + mesh.indices[i + 2] as u32,
                        ));
                    }
                }
            }
        }
    }

    if triangles.is_empty() {
        log::warn!("[Scene] build_navmesh: no triangles found in scene");
        self.navmesh = None;
    } else {
        self.navmesh = Some(navmesh::NavMesh::from_triangles(vertices, triangles));
    }
    self.navmesh_dirty = false;
}
```
5. 添加惰性构建方法：
```rust
#[cfg(feature = "navmesh")]
pub fn ensure_navmesh(&mut self) {
    if self.navmesh_dirty {
        self.build_navmesh();
    }
}
```
6. 在 `Scene::new()` 中初始化 `navmesh_dirty: true`

**注意**：需要验证 `render::ModelMesh` 的字段结构（`positions`、`indices`）。如果字段名不同，需要调整。

**验证**：`cd crates/scene && cargo check --features navmesh`

---

## 任务 6: C6 — 事件组件评估

**目标**：实现 `evaluate_event_components()` 以触发事件组件的游戏逻辑。

**文件**：`crates/scene/src/scene.rs`

**当前状态**：
- L852-L854: 空方法体，每帧从 `tick()`（L880）调用
- `triggered_events: Vec<(String, String)>` 永远不被填充
- `scripts: HashMap<String, crate::ScriptComponent>`（L82）已存在

**修改**：
1. 添加快速路径标志。在 Scene struct 中添加：
```rust
/// 是否有注册了事件处理器的脚本组件
has_event_components: bool,
```
2. 在 `Scene::new()` 中初始化为 `false`
3. 在脚本添加/移除时更新标志
4. 实现 `evaluate_event_components()`（L852-L854）：
```rust
/// 评估所有实体的事件组件，将触发的 response 推入 triggered_events。
pub fn evaluate_event_components(&mut self) {
    if !self.has_event_components {
        return; // 快速路径：无事件组件，跳过
    }

    // 检查 scripts 中是否有触发事件
    let mut has_any = false;
    for (entity_id, script) in &self.scripts {
        // Python 脚本通过 ScriptComponent 的 event_triggers 字典检查
        // 初期实现：检查 script 中是否有注册的 event handler
        if script.has_event_handler() {
            let triggered = script.evaluate_triggers();
            for response_name in triggered {
                self.triggered_events.push((entity_id.clone(), response_name));
            }
            has_any = true;
        }
    }
    self.has_event_components = has_any;
}
```
5. 如果 `ScriptComponent` 当前没有 `has_event_handler()` 和 `evaluate_triggers()` 方法，添加最小存根：
```rust
// 在 ScriptComponent 中添加:
pub fn has_event_handler(&self) -> bool { false }  // 默认无事件处理器
pub fn evaluate_triggers(&self) -> Vec<String> { Vec::new() }  // 默认无触发
```
6. 在脚本注册时设置 `has_event_components = true`

**验证**：`cd crates/scene && cargo check`

---

## 依赖关系

```
任务 1 (C1) ← 无依赖，独立 event crate
任务 2 (C3) ← 无依赖，独立 dbproxy crate  
任务 3 (C2+W3) ← 无依赖，独立 server crate
任务 4 (C4) ← 无依赖，独立 render crate
任务 5 (C5) ← 无依赖，独立 scene crate
任务 6 (C6) ← 无依赖，独立 scene crate

推荐执行顺序: [C1, C2, C3, C4 并行] → [C5 → C6 顺序]
```
C5 和 C6 都修改 `scene.rs`，建议顺序执行以避免合并冲突。

---

## 风险与缓解

| 风险 | 缓解 |
|------|------|
| **C1**: `RwLock` 可能引入死锁（`flush_all` 持写锁时 `publish` 也需写锁） | `publish` 只入队，不调用 `flush_all`；调用方在帧边界调用 `flush_all`，此时不会有并发 `publish`。写锁的持有时间极短（入队操作）。 |
| **C1**: 现有测试使用 `RefCell<Rc<>>` 模式 | 测试使用 `bus.subscribe::<String>(Box::new(...))`——回调是 `Fn(&E)` 而非 `FnMut`，不捕获可变引用。测试应通过编译。若失败，调整测试中 `Rc<RefCell<>>` 的使用方式。 |
| **C3**: `downcast_ref` 路径变更可能影响 `do_*` 方法 | 所有 `do_*` 方法调用 `self.ev_data.downcast_ref::<DBEv*>()`——改为 `self.ev_data.as_any().downcast_ref::<DBEv*>()` 后语义等价。 |
| **C5**: `ModelMesh` 字段名可能与假设不同 | 在修改前先通过 `cargo check` 确认字段名。若结构不同，调整为实际 API。 |
| **C6**: `ScriptComponent` 可能没有所需方法 | 添加最小存根方法，默认返回空结果。不影响现有行为。 |

---

## 被拒绝的替代方案

1. **C1 方案 A（仅删除 unsafe impl）**：会破坏 `EventBusBridge` trait 的 `Send + Sync` 要求，导致 Scene 无法持有 EventBus。**拒绝原因**：破坏现有 API 合约。

2. **C2 方案 B（RAII DistributedLockGuard）**：需要 async Drop，Rust 不支持。改用 spawn 后台释放任务增加复杂性和资源泄漏风险。**拒绝原因**：过度设计，简单的显式错误处理更可靠。

3. **C4 方案 B（完整阴影集成）**：需要修改 `deferred_plus.rs`、`deferred_lighting.wgsl`，添加 ShadowPass、WgpuShadowAtlas、CSM uniform、bind group 布局变更。这是"大"工作量，不适合作为 P0 修复。**拒绝原因**：范围过大，应在 P1/P2 独立实现。

4. **C5 方案 B（异步 tokio 构建）**：在 P0 修复中引入异步复杂性。**拒绝原因**：同步构建已足够；异步优化可后续添加。

---

## 关键文件

1. **[`crates/event/src/bus.rs`](file:///c:/Users/theDa/Documents/workspace/library/geese/crates/event/src/bus.rs)** — C1: EventBus RwLock 迁移
2. **[`server/lib/dbproxy/src/db.rs`](file:///c:/Users/theDa/Documents/workspace/library/geese/server/lib/dbproxy/src/db.rs)** — C3: DBEventData trait
3. **[`server/lib/hub/src/hub_service_manager.rs`](file:///c:/Users/theDa/Documents/workspace/library/geese/server/lib/hub/src/hub_service_manager.rs)** — C2+W3: 分布式锁错误处理（主文件）
4. **[`server/lib/hub/src/hub_server.rs`](file:///c:/Users/theDa/Documents/workspace/library/geese/server/lib/hub/src/hub_server.rs)** — C2+W3: 分布式锁错误处理
5. **[`crates/render/src/deferred_plus.rs`](file:///c:/Users/theDa/Documents/workspace/library/geese/crates/render/src/deferred_plus.rs)** — C4: 阴影缺失警告
6. **[`crates/scene/src/scene.rs`](file:///c:/Users/theDa/Documents/workspace/library/geese/crates/scene/src/scene.rs)** — C5 + C6: NavMesh 构建 + 事件组件评估
