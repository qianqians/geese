# 事件组件系统实现方案

## Context

项目需要为场景对象添加事件组件（Event Component），允许在编辑器中为任意实体配置触发函数和响应函数。触发函数 `fn() -> bool` 每帧调用以判断是否触发，响应函数 `fn()` 在触发时执行。本项目已有成熟的 **Physics/NavMesh 组件 5 层集成闭环**和 **Scene 级 drain 事件模式**，事件系统将完全复用这些模式。

## 推荐方案

完全复用 Physics/NavMesh 的 5 层集成闭环：manifest 定义数据结构 → panels.rs EditorAction + EditorState 缓存 → hierarchy.rs SceneNodeData 字段 → editor.rs 数据流串联 → inspector.rs UI。事件条目以字符串对存储（trigger_name → response_name），运行时通过 Scene tick() 评估触发函数并消费响应事件。~160 行代码，7 个文件，零新文件。

## 任务分解

### 任务 1：manifest.rs 定义数据结构
**文件**：`d:/Personal/lib/geese/crates/scene/src/manifest.rs`

在 `NavMeshComponentDef` 之后（约 L189）添加：
- `EventEntryDef`：
  ```rust
  #[derive(Debug, Clone, Serialize, Deserialize)]
  pub struct EventEntryDef {
      pub trigger: String,   // 触发函数名 fn() -> bool
      pub response: String,  // 响应函数名 fn()
  }
  ```
- `EventComponentDef`：
  ```rust
  #[derive(Debug, Clone, Serialize, Deserialize)]
  pub struct EventComponentDef {
      #[serde(default = "default_true")]
      pub server_enabled: bool,
      #[serde(default = "default_true")]
      pub client_enabled: bool,
      #[serde(default)]
      pub entries: Vec<EventEntryDef>,
  }
  ```
- `Default` impl（server/client enabled，entries 空）

**ModelRef**（L62-L65）和 **SceneObjectDef**（L212-L215）各添加：
```rust
#[serde(default, skip_serializing_if = "Option::is_none")]
pub event: Option<EventComponentDef>,
```
- **依赖**：无
- **验证**：`cd crates/scene && cargo check`

### 任务 2：prefab_manifest.rs 添加 event 字段
**文件**：`d:/Personal/lib/geese/crates/scene/src/prefab_manifest.rs`

- 导入 `EventComponentDef`（更新 L11 use 语句）
- `PrefabNodeDef`（L69-L73 附近）添加：
  ```rust
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub event: Option<EventComponentDef>,
  ```
- 更新测试 `roundtrip_prefab_manifest`（L228-L241）添加 `event: None`
- **依赖**：任务 1
- **验证**：`cd crates/scene && cargo test prefab_manifest`

### 任务 3：panels.rs 添加 EditorAction + EditorState 缓存
**文件**：`d:/Personal/lib/geese/crates/editor/src/panels.rs`

- L56 附近：新增 `EditorAction::SetEventComponent { node_id, component: Option<scene::manifest::EventComponentDef> }`
- L128 附近：新增 `EditorState.event_component_cache: HashMap<String, scene::manifest::EventComponentDef>`
- `EditorState::new()` 中初始化为 `HashMap::new()`
- **依赖**：任务 1
- **验证**：`cd crates/editor && cargo check`

### 任务 4：hierarchy.rs 添加 SceneNodeData.event 字段
**文件**：`d:/Personal/lib/geese/crates/editor/src/hierarchy.rs`

- L36 附近：新增 `pub event: Option<scene::manifest::EventComponentDef>`
- 更新 `new()` 中 7 处示例节点构造 + `show()` 中 "Create Empty" 构造（共 8 处），每处加 `event: None,`
- **依赖**：任务 1
- **验证**：`cd crates/editor && cargo check`

### 任务 5：editor.rs 数据流串联（5 处修改）
**文件**：`d:/Personal/lib/geese/crates/editor/src/editor.rs`

- **5a**：`walk_gltf_node` 函数签名（L319-L330）添加 `event_cache: &mut HashMap<String, EventComponentDef>` 参数 + `event: Option<EventComponentDef>` 参数；`SceneNodeData` 构造（L348-L360）添加 `event: event.clone()`；缓存写入添加 `if let Some(ref ev) = event { event_cache.entry(...) }`
- **5b**：`walk_gltf_node` 递归调用（L335）传递新参数；调用点（L378）传递 `&mut self.state.event_component_cache` + `model.event.clone()`
- **5c**：`process_prefab_actions`（L791-L812）添加 `SetEventComponent` 匹配分支——插入/移除缓存
- **5d**：`handle_save_as_prefab`（L1006-L1020）读取 `event_component_cache` 写入 `PrefabNodeDef.event`
- **5e**：`handle_instantiate_prefab`（L1154-L1155）从 `node_def.event.clone()` 写入 `SceneNodeData.event` + 缓存
- **依赖**：任务 3、任务 4
- **验证**：`cd crates/editor && cargo check`

### 任务 6：inspector.rs Event Component UI
**文件**：`d:/Personal/lib/geese/crates/editor/src/inspector.rs`

- InspectorPanel 新增字段（L35-L37）：
  ```rust
  event_enabled: bool,
  event_server_enabled: bool,
  event_client_enabled: bool,
  event_entries: Vec<(String, String)>,  // (trigger, response)
  ```
- `new()` 初始化：`event_enabled: false, event_server_enabled: true, event_client_enabled: true, event_entries: Vec::new()`
- 选择同步（L182-L188）：从 `event_component_cache` 同步 entries 列表
- 新增 `push_event_update()` 辅助方法（参考 `push_navmesh_update`）
- 在 Character Controller 面板之后（L392 后）新增 `▼ Event Component` CollapsingHeader：
  - Add/Remove Component 按钮
  - `server_enabled` / `client_enabled` 复选框
  - entries 列表：每行 `trigger → response` + "✖ Remove" 按钮
  - "➕ Add Entry" 按钮 → 弹出 trigger/response 文本输入
  - 任何变更 push `EditorAction::SetEventComponent`
- `▼ All Components` 列表添加 `if self.event_enabled { ui.label("• Event Component"); }`
- **依赖**：任务 3
- **验证**：`cd crates/editor && cargo check`

### 任务 7：Scene 运行时事件评估
**文件**：`d:/Personal/lib/geese/crates/scene/src/scene.rs`

- Scene 结构体添加字段（L75-L78 附近）：
  ```rust
  /// 本帧触发的自定义事件列表，由外部消费者 drain。
  pub triggered_events: Vec<(String, String)>,  // (entity_id, response_name)
  ```
- `new()` 初始化 `triggered_events: Vec::new()`
- 新增 `drain_triggered_events()` 方法（参考 L818-L826 drain 模式）
- 新增 `evaluate_event_components()` 方法（桩实现，留待 Python 运行时对接）：
  ```rust
  pub fn evaluate_event_components(&mut self) {
      // TODO: 遍历所有标记了 event 组件的实体,
      // 调用其 trigger 函数检查是否触发,
      // 将触发的 response 函数名推入 triggered_events
  }
  ```
- `tick()` 方法末尾添加 `self.evaluate_event_components();` 调用
- **依赖**：任务 1
- **验证**：`cd crates/scene && cargo build`

### 任务 8：端到端编译验证
- 编译 scene → editor → desktop 全链路
- 运行 navmesh + scene 测试套件
- **依赖**：任务 1-7 全部完成
- **验证**：
  ```powershell
  cd crates/scene; cargo build; cargo test
  cd crates/editor; cargo build
  cd desktop; cargo check
  ```

## 验证方式

1. **编译验证**：任务 8 的 3 个编译命令均通过
2. **测试验证**：`cargo test` 所有测试通过
3. **编辑器功能验证**：
   - 运行 `python run_editor.py`
   - 选中场景节点 → Inspector 出现 `▼ Event Component` 面板
   - 点击 "Add Component" 启用
   - 添加事件条目：输入 trigger 函数名和 response 函数名
   - 保存为 Prefab 后重新加载，Event Component 配置应保留

## 拒绝的替代方案

| 方案 | 拒绝原因 |
|------|----------|
| 在 Rust 层持有 Python 对象引用 | 跨 FFI 边界的生命周期管理复杂，增加内存泄漏风险。字符串引用模式更安全 |
| 事件函数使用 trait object（`Box<dyn Fn()>`）| 不可序列化到 JSON manifest，破坏编辑器保存/加载流程 |
| 新建独立 `event` crate | 过度设计。事件组件规模小，放在 scene manifest 中即可 |
| 批量 FFI 优化（Plan B）| 当前阶段事件数量少，单次 GIL 获取开销可接受；批量模式增加复杂度 |
