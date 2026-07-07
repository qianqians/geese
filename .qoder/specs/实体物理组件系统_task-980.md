# 实体物理组件系统

## 背景

当前物理参数分散在三处：manifest 的 `body_kind` + `collision_enabled`、Inspector 的 cc_* 参数、EditorState 的 body_kind_cache。没有统一的组件概念，Server/Client 物理开关缺失。

## 设计决策

1. **复用现有模式**: `BodyKindDef` (已存在, [manifest.rs#L121-L142](file://d:/Personal/lib/geese/crates/scene/src/manifest.rs#L121-L142)) 嵌入新的 `PhysicsComponentDef`
2. **Option 包装**: `Option<PhysicsComponentDef>` — `None` 表示无物理组件，`Some` 表示有
3. **Add/Remove 按钮 UX**: 复用 Inspector 中 Character Controller 的 "Add Component"/"Disable" 模式 ([inspector.rs#L254-L268](file://d:/Personal/lib/geese/crates/editor/src/inspector.rs#L254-L268))
4. **JSON 格式**: `"physics": {"server_enabled": true, "client_enabled": true, "body_kind": "fixed"}` — 嵌套结构便于扩展
5. **向后兼容**: 旧 JSON 的 `collision_enabled` + `body_kind` 字段自动迁移到新 `physics` 字段

---

## 任务列表

### 任务 1: 定义 `PhysicsComponentDef` — manifest 层

**文件**: `d:/Personal/lib/geese/crates/scene/src/manifest.rs`

**1.1** 新增 `PhysicsComponentDef` 结构体（放在 `TransformDef` 之后，约 L94）：

```rust
/// 实体物理组件定义。
/// None 表示该实体没有物理组件。Some 表示实体参与物理模拟。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PhysicsComponentDef {
    /// 服务器是否运行物理模拟
    #[serde(default = "default_true")]
    pub server_enabled: bool,
    /// 客户端是否运行物理模拟
    #[serde(default = "default_true")]
    pub client_enabled: bool,
    /// 碰撞体开关
    #[serde(default = "default_true")]
    pub collision_enabled: bool,
    /// 物理刚体类型
    #[serde(default = "default_body_kind")]
    pub body_kind: BodyKindDef,
}

fn default_true() -> bool { true }
```

**1.2** 修改 `ModelRef`: 删除 `collision_enabled` + `body_kind` 两个独立字段，替换为:
```rust
    /// 物理组件定义。None 表示无物理。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub physics: Option<PhysicsComponentDef>,
```

**1.3** 修改 `SceneObjectDef`: 同样替换 `body_kind` 为 `physics: Option<PhysicsComponentDef>`

**1.4** 实现向后兼容反序列化 — 在 `ModelRef` 和 `SceneObjectDef` 添加自定义 `Deserialize` 实现，检测旧格式字段并自动构造 `PhysicsComponentDef`（如果 `physics` 为 None 但 `body_kind`/`collision_enabled` 存在）

**验证**: `cargo test -p scene` 现有测试通过 + 新增兼容性测试

---

### 任务 2: 更新 `PrefabManifest` 使用 `PhysicsComponentDef`

**文件**: `d:/Personal/lib/geese/crates/scene/src/prefab_manifest.rs`

**2.1** `PrefabNodeDef` (L67-L69): 删除 `body_kind: BodyKindDef`，替换为 `physics: Option<PhysicsComponentDef>`

**2.2** 导入 `PhysicsComponentDef` from `crate::manifest`

**验证**: `cargo check -p scene`

---

### 任务 3: 编辑器 `EditorState` 缓存改用 `PhysicsComponentDef`

**文件**: `d:/Personal/lib/geese/crates/editor/src/panels.rs`

**3.1** 替换 `body_kind_cache: HashMap<String, BodyKindDef>` (L120-L121) 为:
```rust
    /// 实体物理组件缓存 (entity_id → PhysicsComponentDef)
    pub physics_component_cache: HashMap<String, scene::manifest::PhysicsComponentDef>,
```

**3.2** 新增 `EditorAction` 变体:
```rust
    /// 设置/移除物理组件
    SetPhysicsComponent {
        node_id: String,
        component: Option<scene::manifest::PhysicsComponentDef>,
    },
```

**3.3** 保留 `SetBodyKind` 变体但标记为 deprecated（逐步迁移）

**验证**: `cargo check -p editor`

---

### 任务 4: 添加 Undo/Redo 命令

**文件**: `d:/Personal/lib/geese/crates/editor/src/commands.rs`

**4.1** 参照 `SetBodyKindCommand` ([commands.rs#L352-L397](file://d:/Personal/lib/geese/crates/editor/src/commands.rs#L352-L397))，新增:

```rust
pub struct SetPhysicsComponentCommand {
    pub entity_id: String,
    pub old_component: Option<scene::manifest::PhysicsComponentDef>,
    pub new_component: Option<scene::manifest::PhysicsComponentDef>,
    pub on_apply: Option<Box<dyn Fn(&str, Option<scene::manifest::PhysicsComponentDef>)>>,
    executed: bool,
}
```

**4.2** 实现 `Command` trait（模式与 SetBodyKindCommand 一致）

**验证**: `cargo check -p editor`

---

### 任务 5: 重构 Inspector — 物理组件面板 UI

**文件**: `d:/Personal/lib/geese/crates/editor/src/inspector.rs`

**5.1** 在 `InspectorPanel` 结构体中替换分散的物理字段:

```rust
    // 移除: body_kind_idx, cc_* 等分散字段
    // 新增:
    physics_enabled: bool,        // 是否有物理组件
    physics_server_enabled: bool, // Server 物理开关
    physics_client_enabled: bool, // Client 物理开关
    physics_body_kind_idx: usize, // 0=Static, 1=Dynamic
```

**5.2** 将 "Physics Body" 面板 (L217-L248) 改造为 "Physics Component" 面板:

```
▼ Physics Component

  [Add Component] / [Remove]  ← 状态切换按钮

  ── 如果有组件 ──
  ☑ Server Physics    (toggle)
  ☑ Client Physics    (toggle)
  ☑ Collision Enabled (toggle)
  Type: [Static] [Dynamic]  (selectable)
  ──────────────────
```

**5.3** 选中实体切换时从 `state.physics_component_cache` 同步状态

**5.4** 修改时会 push `EditorAction::SetPhysicsComponent` 到 `pending_actions`

**验证**: `cargo check -p editor`

---

### 任务 6: Editor 处理 `SetPhysicsComponent` Action

**文件**: `d:/Personal/lib/geese/crates/editor/src/editor.rs`

**6.1** 在 `process_prefab_actions` 方法中添加处理分支（约 L762-L810 附近）:

```rust
EditorAction::SetPhysicsComponent { node_id, component } => {
    match component {
        Some(ref comp) => {
            self.state.physics_component_cache.insert(node_id.clone(), comp.clone());
        }
        None => {
            self.state.physics_component_cache.remove(&node_id);
        }
    }
}
```

**6.2** 在 `handle_save_as_prefab` 中序列化 `physics_component_cache` 内容到 Prefab 定义

**验证**: `cargo check -p editor`

---

### 任务 7: 场景加载适配新字段

**文件**: `d:/Personal/lib/geese/crates/editor/src/editor.rs` 的 `load_imported_scene` (L291-L377)

**7.1** 修改 `walk_gltf_node` 中填充缓存的逻辑:
- 旧: `body_kind_cache.insert(entity_id, body_kind)` (L364)
- 新: `physics_component_cache.insert(entity_id, PhysicsComponentDef { ... })`
- 从 manifest 的 `model.physics` 读取，缺失时构造默认值（server_enabled=true, client_enabled=true）

**验证**: `cargo check -p editor`

---

### 任务 8: 场景保存使用新 JSON 格式

**文件**: `d:/Personal/lib/geese/crates/editor/src/editor.rs` 的场景保存方法

**8.1** 序列化时写出 `physics` 字段（而非独立 `body_kind` + `collision_enabled`）

**验证**: 保存的场景 JSON 包含 `"physics": {...}` 嵌套结构

---

### 任务 9: `PhysicsManager.load_scene` 读取新字段

**文件**: `d:/Personal/lib/geese/crates/physics_manager/src/manager.rs`

**9.1** 修改 `load_scene` 中的 JSON 读取路径 (L64-L227):
- 优先读取 `model["physics"]` 对象（新格式）
- 若无 `physics` 字段，回退读取旧 `collision_enabled` + `body_kind` 字段
- 当 `physics.collision_enabled == false` 时跳过该实体

**验证**: `cargo check -p physics_manager`

---

### 任务 10: 文档更新

**文件**: `d:/Personal/lib/geese/EDITOR_PHYSICS_PLAN.md`

更新文档，描述物理组件系统的工作方式。

**验证**: 文档审查

---

## 依赖关系

```
任务 1 (PhysicsComponentDef) ── 基础定义，无依赖
    ├── 任务 2 (PrefabManifest) ── 依赖 1
    ├── 任务 3 (EditorState) ── 依赖 1
    │       ├── 任务 4 (Command) ── 依赖 3
    │       ├── 任务 5 (Inspector UI) ── 依赖 3
    │       │       └── 任务 6 (Editor action 处理) ── 依赖 3+5
    │       └── 任务 7 (场景加载适配) ── 依赖 3
    ├── 任务 8 (场景保存) ── 依赖 1
    └── 任务 9 (PhysicsManager) ── 依赖 1
                            └── 任务 10 (文档) ── 无强依赖
```

建议执行顺序: **1 → 2 → 3 → 4+5 → 6+7 → 8+9 → 10**

---

## 新旧 JSON 格式对比

**旧格式** (当前 .scene.json):
```json
{ "id": "house", "path": "assets/house.gltf", "collision_enabled": true, "body_kind": "fixed" }
```

**新格式**:
```json
{ "id": "house", "path": "assets/house.gltf",
  "physics": { "server_enabled": true, "client_enabled": true, "collision_enabled": true, "body_kind": "fixed" }
}
```

无物理的实体直接省略 `physics` 字段（等价于旧格式 `collision_enabled: false`）。

---

## 风险与缓解

| 风险 | 缓解 |
|------|------|
| 旧 `.scene.json` 无法加载 | 任务 1.4 实现向后兼容 deserializer，自动检测并迁移旧字段 |
| `PhysicsManager` 裸读 JSON 字段路径断裂 | 任务 9 使用 `.get("physics")` + 回退到读取旧字段 |
| Inspector UI 重构破坏现有功能 | 渐进迁移：先添加新 UI，保留旧路径兼容，逐步废弃 |
| Prefab 格式变更导致已有数据损坏 | 任务 2 同样实现 serde backward compat |
| `body_kind_cache` 被下游代码直接引用 | 全局 grep `body_kind_cache` → 替换为 `physics_component_cache` |

---

## 关键文件

1. **[manifest.rs](file://d:/Personal/lib/geese/crates/scene/src/manifest.rs)** — `PhysicsComponentDef` 定义 + ModelRef/SceneObjectDef 修改
2. **[panels.rs](file://d:/Personal/lib/geese/crates/editor/src/panels.rs)** — EditorState 缓存 + EditorAction 枚举
3. **[inspector.rs](file://d:/Personal/lib/geese/crates/editor/src/inspector.rs)** — Inspector 物理组件面板 UI
4. **[commands.rs](file://d:/Personal/lib/geese/crates/editor/src/commands.rs)** — SetPhysicsComponentCommand (Undo/Redo)
5. **[manager.rs](file://d:/Personal/lib/geese/crates/physics_manager/src/manager.rs)** — load_scene 读取新 JSON schema

---

## 被拒绝的方案

1. **Plan A 的 SoA PhysicsComponentStore 预计算索引**: 过度工程化，初始版本不需要 O(1) 查询优化。先用简单的 HashMap + 线性遍历，后续根据性能分析再优化。

2. **Plan B 的位标志 + 并行 Vec<Option<T>> 存储**: 实现复杂度高，与 Geese 现有的 HashMap 缓存模式不一致。复用现有模式降低维护成本。

3. **保留 `body_kind` 和 `collision_enabled` 为独立字段，单独添加 `server_enabled`/`client_enabled`**: 字段分散增加 JSON 解析复杂度，不如统一收敛到 `physics` 嵌套对象。
