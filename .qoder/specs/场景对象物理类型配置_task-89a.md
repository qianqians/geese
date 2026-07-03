# 场景对象物理类型配置

## 需求

场景中每个对象可配置物理类型：
- **Static**（`BodyKind::Fixed`）：固定位置，不受物理影响（地面、墙壁）
- **Dynamic**（`BodyKind::Dynamic`）：受重力影响，掉落直到落在固定物体上（石块、箱子）

## 核心架构发现

**双重数据流独立**——编辑器层级流和物理加载流各自解析 `.scene.json`：
- 编辑器侧：`Editor::load_imported_scene` → 解析为 `SceneManifest`（类型化结构体）
- 物理侧：`PhysicsManager::load_scene` → 解析为 `serde_json::Value`（原始 JSON）

两者必须独立更新。此外 Python 远程物理服务器和产品服务器的加载逻辑也需要同步更新。

**Trimesh 约束**：Rapier 中 trimesh 碰撞体只能附加到 `Fixed`/`Kinematic` 刚体，不能用于 `Dynamic`。GLTF 模型使用 trimesh 碰撞时，Dynamic 需要降级处理。

---

## 任务 1：定义 `BodyKindDef` 枚举并添加到 SceneManifest

**文件**: `d:\Personal\geese\crates\scene\src\manifest.rs`

在 `TransformDef` 之后（约第 91 行附近）新增枚举和默认值函数：

```rust
/// 场景清单中的物理刚体类型定义。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BodyKindDef {
    Fixed,
    Dynamic,
}

fn default_body_kind() -> BodyKindDef {
    BodyKindDef::Fixed
}
```

给 `ModelRef`（第 62 行 `collision_enabled` 之后）增加字段：

```rust
#[serde(default = "default_body_kind")]
pub body_kind: BodyKindDef,
```

给 `SceneObjectDef`（第 111 行 `tag` 之后）增加字段：

```rust
#[serde(default = "default_body_kind")]
pub body_kind: BodyKindDef,
```

添加转换方法（放在 impl 块中）：

```rust
impl BodyKindDef {
    pub fn to_physics_kind(&self) -> physics::world::BodyKind {
        match self {
            Self::Fixed => physics::world::BodyKind::Fixed,
            Self::Dynamic => physics::world::BodyKind::Dynamic,
        }
    }
}
```

更新 `deserialize_full_manifest` 测试（~246 行）加入 `"body_kind": "fixed"`，新增一个 missing-body-kind 反序列化测试验证默认值。

**依赖**: 无（第一个任务）

---

## 任务 2：给 `SceneNodeData` 和 `PrefabNodeDef` 增加 `body_kind`

**文件 1**: `d:\Personal\geese\crates\editor\src\hierarchy.rs`

给 `SceneNodeData` 结构体（第 32 行 `prefab_ref_uuid` 之后）增加：

```rust
pub body_kind: scene::manifest::BodyKindDef,
```

更新 `walk_gltf_node` 中的构造代码（editor.rs 约第 400 行）来填充此字段。

**文件 2**: `d:\Personal\geese\crates\scene\src\prefab_manifest.rs`

给 `PrefabNodeDef`（第 66 行 `overrides` 之后）增加：

```rust
#[serde(default = "default_body_kind")]
pub body_kind: BodyKindDef,
```

添加 `fn default_body_kind() -> BodyKindDef`（或复用 manifest.rs 中的）。

更新 roundtrip 测试加入 `body_kind` 字段。

**依赖**: 任务 1

---

## 任务 3：给 `EditorState` 增加 `body_kind_cache` 和 `EditorAction::SetBodyKind`

**文件**: `d:\Personal\geese\crates\editor\src\panels.rs`

在 `EditorState` 结构体（第 98 行 `transform_cache` 之后）增加：

```rust
/// 实体物理类型缓存 (entity_id → body_kind)
pub body_kind_cache: HashMap<String, scene::manifest::BodyKindDef>,
```

在 `EditorAction` 枚举（第 46 行 `}` 之前）增加变体：

```rust
/// 切换实体的物理刚体类型
SetBodyKind {
    node_id: String,
    body_kind: scene::manifest::BodyKindDef,
},
```

在 `EditorState::new()` 中初始化 `body_kind_cache: HashMap::new()`。

**依赖**: 任务 1

---

## 任务 4：参数化 `add_static_trimeshes` → `add_trimeshes`

**文件**: `d:\Personal\geese\crates\physics\src\scene.rs`

将 `add_static_trimeshes`（第 176 行）重命名/重构为 `add_trimeshes`，接受 `body_kind: BodyKind` 参数：

```rust
pub fn add_trimeshes(
    &mut self,
    meshes: &[crate::scene_builder::TrimeshData],
    transform: Iso3,
    body_kind: BodyKind,
    friction: f32,
    restitution: f32,
) -> Result<Vec<(BodyHandle, ColliderHandle)>, String>
```

将硬编码的 `BodyKind::Fixed`（第 198 行）替换为参数 `body_kind`。

保留 `add_static_trimeshes` 作为向后兼容的薄封装：

```rust
pub fn add_static_trimeshes(
    &mut self,
    meshes: &[crate::scene_builder::TrimeshData],
    transform: Iso3,
    friction: f32,
    restitution: f32,
) -> Result<Vec<(BodyHandle, ColliderHandle)>, String> {
    self.add_trimeshes(meshes, transform, BodyKind::Fixed, friction, restitution)
}
```

**依赖**: 无（可并行）

---

## 任务 5：更新 `PhysicsManager::load_scene` 读取 `body_kind`

**文件**: `d:\Personal\geese\crates\physics_manager\src\manager.rs`

**GLTF 模型部分**（第 112-151 行）:

在第 144 行的 `add_static_trimeshes` 调用处改为：

```rust
let body_kind = match model["body_kind"].as_str().unwrap_or("fixed") {
    "dynamic" => {
        eprintln!("[PhysicsManager] WARNING: model '{}' has body_kind=dynamic but uses trimesh collision; trimesh only supports Fixed. Falling back to Fixed.", model["id"]);
        BodyKind::Fixed
    }
    _ => BodyKind::Fixed,
};
scene.add_trimeshes(&meshes, iso, body_kind, 0.5, 0.0);
```

> **Trimesh 降级说明**：Rapier 不支持 Dynamic + trimesh 组合，现阶段 GLTF 模型保持 Fixed。后续可通过 convex decomposition（V-HACD）生成 Dynamic 碰撞体。

**程序化对象部分**（第 155-197 行）:

在第 188 行的 `BodyDesc` 构造前读取 body_kind：

```rust
let body_kind = match obj["body_kind"].as_str().unwrap_or("fixed") {
    "dynamic" => BodyKind::Dynamic,
    _ => BodyKind::Fixed,
};
let desc = BodyDesc {
    kind: body_kind,
    position: iso,
    ..Default::default()
};
```

**依赖**: 任务 1, 任务 4

---

## 任务 6：更新 Python 物理加载脚本

**文件 1**: `d:\Personal\geese\crates\editor\scripts\physics_editor_server.py`

`_load_scene_collision` 函数（第 132-176 行）：

GLTF 模型（第 145-158 行）——保持 `add_fixed` 不变，添加注释说明 trimesh 限制。

程序化对象（第 161-174 行）——将第 173 行的 `add_fixed` 替换为：

```python
body_kind = obj_def.get("body_kind", "fixed")
if body_kind == "dynamic":
    body = PhysicsBody.add_dynamic(
        _world, _scene_id, shape, position=pos, rotation=rot
    )
else:
    body = PhysicsBody.add_fixed(
        _world, _scene_id, shape, position=pos, rotation=rot
    )
```

**文件 2**: `d:\Personal\geese\server\engine\scene_physics.py`

`load_scene_collision_from_manifest` 函数（第 42-117 行）——对程序化对象（第 96-115 行）进行同样的 `body_kind` 检查和条件分支（`add_dynamic` vs `add_fixed`）。

**依赖**: 任务 1（可并行）

---

## 任务 7：在 `Editor::load_imported_scene` 中传播 `body_kind`

**文件**: `d:\Personal\geese\crates\editor\src\editor.rs`

在 `load_imported_scene` 函数中（约第 336-416 行）：

1. 读取每个 `ModelRef` 的 `body_kind` 值
2. 在 `walk_gltf_node` 闭包中，创建 `SceneNodeData` 时填充 `body_kind` 字段
3. 在 `transform_cache` 插入位置（第 400-404 行）同步填充 `state.body_kind_cache`：

```rust
state.body_kind_cache.insert(eid.clone(), model.body_kind);
```

对于程序化对象（若也在编辑器中展示），同样填充 body_kind_cache。

**依赖**: 任务 2, 任务 3

---

## 任务 8：Inspector 新增 Physics Body 组件

**文件**: `d:\Personal\geese\crates\editor\src\inspector.rs`

在 `InspectorPanel` 结构体（第 26 行 `cc_enabled` 之后）增加字段：

```rust
body_kind_idx: usize, // 0=Static, 1=Dynamic
```

在 `show()` 方法中，在 "Components" 折叠头之后（约第 213 行）、Character Controller 之前，新增 "Physics Body" 折叠头：

```rust
egui::CollapsingHeader::new("▼ Physics Body")
    .default_open(false)
    .show(ui, |ui| {
        if let Some(ref entity_id) = state.selected_entity {
            if let Some(kind) = state.body_kind_cache.get(entity_id).copied() {
                let mut idx = match kind {
                    scene::manifest::BodyKindDef::Fixed => 0,
                    scene::manifest::BodyKindDef::Dynamic => 1,
                };
                let old_idx = idx;
                ui.horizontal(|ui| {
                    ui.label("Type:");
                    ui.selectable_value(&mut idx, 0, "Static");
                    ui.selectable_value(&mut idx, 1, "Dynamic");
                });
                if idx != old_idx {
                    let new_kind = if idx == 0 {
                        scene::manifest::BodyKindDef::Fixed
                    } else {
                        scene::manifest::BodyKindDef::Dynamic
                    };
                    state.body_kind_cache.insert(entity_id.clone(), new_kind);
                    state.pending_actions.push(EditorAction::SetBodyKind {
                        node_id: entity_id.clone(),
                        body_kind: new_kind,
                    });
                }
            }
        }
    });
```

选择同步逻辑：在 `selection_changed` 块中（第 100-117 行），从 `state.body_kind_cache` 更新 `self.body_kind_idx`。

**依赖**: 任务 3, 任务 7

---

## 任务 9：Editor 中处理 `SetBodyKind` 动作

**文件**: `d:\Personal\geese\crates\editor\src\editor.rs`

在 `process_prefab_actions` 方法（约第 797 行）中增加 `EditorAction::SetBodyKind` 的处理分支：

```rust
EditorAction::SetBodyKind { node_id, body_kind } => {
    state.body_kind_cache.insert(node_id.clone(), body_kind);
    // 标记场景为脏，触发重新保存
    // TODO: 运行时动态更新物理类型（需要 BodyHandle 映射）
}
```

> **已知限制**：Inspector 修改 `body_kind` 后，运行时物理体不会立即切换类型——需要重新加载场景（Play→Stop→Play）才能生效。这是因为当前没有维护 entity_id → BodyHandle 的映射表。后续可增加此映射以实现即时切换。

**依赖**: 任务 3, 任务 5

---

## 任务 10：更新测试

1. `d:\Personal\geese\crates\scene\src\manifest.rs` — 添加 `body_kind` 的序列化/反序列化/默认值测试
2. `d:\Personal\geese\crates\scene\src\prefab_manifest.rs` — 更新 roundtrip 测试包含 `body_kind`

**依赖**: 任务 1, 任务 2

执行测试命令：
```bash
cd crates/scene && cargo test
```

---

## 依赖关系图

```
任务1: BodyKindDef + manifest 字段
  ├─ 任务2: SceneNodeData + PrefabNodeDef ← 依赖任务1
  ├─ 任务3: EditorState cache + EditorAction ← 依赖任务1
  ├─ 任务4: add_trimeshes 参数化 ← 独立
  └─ 任务6: Python 加载脚本 ← 依赖任务1 (可并行)

任务2 + 任务3 → 任务7: Editor 场景加载传播

任务1 + 任务4 → 任务5: PhysicsManager 加载

任务3 + 任务7 → 任务8: Inspector UI

任务3 + 任务5 → 任务9: Editor SetBodyKind 处理

任务1 + 任务2 → 任务10: 测试更新
```

**推荐实现顺序**: 1 → (2,3,4,6 并行) → (5,7 并行) → 8 → 9 → 10

---

## 风险与对策

| 风险 | 对策 |
|------|------|
| GLTF 模型 Dynamic + trimesh 不兼容 | 任务5 中已实施降级策略：记录警告日志，强制回退为 Fixed。后续可引入 convex decomposition |
| 现有 `.scene.json` 无 `body_kind` 字段 | `#[serde(default)]` + `unwrap_or("fixed")` 保证向后兼容 |
| Inspector 修改后物理体未立即切换 | 已知限制，文档化。后续通过 BodyHandle 映射实现运行时切换 |
| Python 脚本与 Rust 加载逻辑不同步 | 两个 Python 文件（editor/server 端）均在同一任务中更新 |

## 影响范围

| 文件 | 变更类型 | 预计行数 |
|------|---------|---------|
| `crates/scene/src/manifest.rs` | 新增枚举 + 两个 struct 字段 + 测试 | ~35 行 |
| `crates/editor/src/panels.rs` | 新增 cache + 枚举变体 | ~8 行 |
| `crates/editor/src/hierarchy.rs` | 新增字段 | ~1 行 |
| `crates/physics/src/scene.rs` | 参数化方法 + 转发封装 | ~25 行 |
| `crates/physics_manager/src/manager.rs` | 条件分支读取 body_kind | ~15 行 |
| `crates/editor/src/inspector.rs` | Physics Body 组件 UI | ~30 行 |
| `crates/editor/src/editor.rs` | 缓存填充 + SetBodyKind 处理 | ~15 行 |
| `crates/editor/scripts/physics_editor_server.py` | 条件 add_dynamic/Fixed | ~10 行 |
| `server/engine/scene_physics.py` | 条件 add_dynamic/Fixed | ~10 行 |
| **合计** | | **~150 行** |

## 被拒绝的替代方案

### 方案1：仅 `SceneObjectDef` 不支持 `ModelRef`
被拒原因：用户需要在 GLTF 模型上也配置物理类型（即使当前有 trimesh 限制）。添加字段 + 运行时降级是正确的分层方式。

### 方案2：使用 String 而非枚举
被拒原因：`"fxied"` 这种拼写错误在运行时才能发现。`BodyKindDef` 枚举 + `serde(rename_all)` 在反序列化阶段就能报错。

### 方案3：Inspector 不缓存，实时从文件读取
被拒原因：每帧读取文件开销大，且与现有 `transform_cache` 模式不一致。缓存模式与现有架构一致。
