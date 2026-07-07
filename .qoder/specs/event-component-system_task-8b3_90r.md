# 事件组件系统实现方案（独立模块）

## Context

用户要求事件系统作为独立 crate 模块（`crates/event/`），与 `crates/navmesh/` 同级。触发函数 `fn() -> bool` 和响应函数 `fn()` 以字符串形式存储。编辑器通过 5 层集成闭环添加 Event Component UI。

## 推荐方案

新建 `crates/event/` 独立 crate，定义 `EventEntryDef` 和 `EventComponentDef` 核心类型。scene 通过 optional feature flag 依赖 event，editor 直接依赖。ModelRef/SceneObjectDef/PrefabNodeDef 各添加 `event` 字段。~220 行，13 个文件（2 新建 + 11 修改）。

## 任务分解

### 任务 1：创建 crates/event/ 独立模块
**新建文件**：`d:/Personal/lib/geese/crates/event/Cargo.toml`, `d:/Personal/lib/geese/crates/event/src/lib.rs`

Cargo.toml 参考 navmesh 模式：serde optional dep + default=["serde"]
lib.rs：`EventEntryDef{trigger, response}` + `EventComponentDef{server_enabled, client_enabled, entries: Vec<EventEntryDef>}` + Default impl
- **依赖**：无
- **验证**：`cd crates/event && cargo build && cargo test`

### 任务 2：scene crate 添加 event 依赖 + feature flag
**文件**：`d:/Personal/lib/geese/crates/scene/Cargo.toml`

- `event = { path = "../event", optional = true }`，feature `event = ["dep:event"]`，default 添加 `"event"`
- **依赖**：任务 1
- **验证**：`cd crates/scene && cargo check`

### 任务 3：manifest.rs 引用 event 类型 + ModelRef/SceneObjectDef 添加字段
**文件**：`d:/Personal/lib/geese/crates/scene/src/manifest.rs`

- 顶部 `#[cfg(feature = "event")] use event::{EventComponentDef, EventEntryDef};`
- ModelRef 添加 `pub event: Option<EventComponentDef>`
- SceneObjectDef 添加 `pub event: Option<EventComponentDef>`
- **依赖**：任务 2
- **验证**：`cd crates/scene && cargo check`

### 任务 4：prefab_manifest.rs 添加 event 字段
**文件**：`d:/Personal/lib/geese/crates/scene/src/prefab_manifest.rs`

- 导入 EventComponentDef，PrefabNodeDef 添加 `event` 字段，更新测试构造
- **依赖**：任务 2
- **验证**：`cd crates/scene && cargo test prefab_manifest`

### 任务 5：loader.rs 更新 SceneObjectDef 构造
**文件**：`d:/Personal/lib/geese/crates/scene/src/loader.rs`

- 两处 SceneObjectDef 构造添加 `event: None`
- **依赖**：任务 3

### 任务 6：editor crate 添加 event 依赖
**文件**：`d:/Personal/lib/geese/crates/editor/Cargo.toml`

- `event = { path = "../event" }`
- **依赖**：任务 1

### 任务 7：panels.rs 添加 EditorAction + cache
**文件**：`d:/Personal/lib/geese/crates/editor/src/panels.rs`

- `EditorAction::SetEventComponent { node_id, component }` + `EditorState.event_component_cache`
- **依赖**：任务 6

### 任务 8：hierarchy.rs 添加字段
**文件**：`d:/Personal/lib/geese/crates/editor/src/hierarchy.rs`

- SceneNodeData 添加 `event` 字段（8 处构造更新）
- **依赖**：任务 6

### 任务 9：editor.rs 5 处数据流串联
**文件**：`d:/Personal/lib/geese/crates/editor/src/editor.rs`

- walk_gltf_node + 调用点 + process_prefab_actions + handle_save_as_prefab + handle_instantiate_prefab
- **依赖**：任务 7、任务 8

### 任务 10：gltf_import_dialog.rs 更新
**文件**：`d:/Personal/lib/geese/crates/editor/src/gltf_import_dialog.rs`

- ModelRef 构造添加 `event: None`
- **依赖**：任务 3

### 任务 11：inspector.rs Event Component UI
**文件**：`d:/Personal/lib/geese/crates/editor/src/inspector.rs`

- 新增字段 + 选择同步 + push_event_update() + CollapsingHeader UI（Add/Remove Component、server/client 复选框、entries 列表增删）
- **依赖**：任务 7

### 任务 12：Scene 运行时事件评估
**文件**：`d:/Personal/lib/geese/crates/scene/src/scene.rs`

- triggered_events 字段 + drain_triggered_events() + evaluate_event_components() 桩 + tick() 调用
- **依赖**：任务 2

### 任务 13：端到端编译验证
- event → scene → editor → desktop 全链路编译 + 测试

## 拒绝的替代方案

| 方案 | 拒绝原因 |
|------|----------|
| 嵌入 scene manifest 而非独立 crate | 用户明确要求独立模块 |
| 在 Rust 层持有 Python 对象引用 | 跨 FFI 生命周期管理复杂 |
| 事件函数使用 trait object | 不可序列化到 JSON |
