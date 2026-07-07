# NavMesh 场景对象集成方案

## Context

当前 `crates/navmesh/` 已有完整的 NavMesh 寻路骨架（A* + 漏斗平滑），但完全孤立——零依赖、未集成到任何 crate。用户需要在编辑器中为场景对象添加 NavMesh 组件，实现场景内导航寻路。

项目已有成熟的 **Physics Component 集成模式**（manifest 定义 → EditorState 缓存 → Inspector UI → EditorAction 处理），NavMesh 组件将完全复用此模式。

## 推荐方案

完全遵循 Physics Component 的 5 层集成闭环，通过 feature flag 隔离，最小化风险。核心变更：在 scene crate 定义 `NavMeshComponentDef`，在 editor 添加对应的 Inspector UI 和 EditorAction 处理，在 Scene 运行时持有统一的 `NavMesh` 实例。

## 任务分解

### 任务 1：NavMesh Crate 添加 serde 支持
**文件**：`d:/Personal/lib/geese/crates/navmesh/Cargo.toml`、`d:/Personal/lib/geese/crates/navmesh/src/lib.rs`

- Cargo.toml 添加 `serde = { version = "1.0", features = ["derive"] }`
- 为 `Vec2`、`NavTri`、`NavMesh` 添加 `#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]`
- **依赖**：无
- **验证**：`cd crates/navmesh && cargo build`，测试全部通过

### 任务 2：Scene Crate 添加 navmesh 依赖与 feature flag
**文件**：`d:/Personal/lib/geese/crates/scene/Cargo.toml`

- 添加依赖：`navmesh = { path = "../navmesh", version = "0.1.0", optional = true }`
- 添加 feature：`navmesh = ["dep:navmesh"]`
- 更新 default features：`default = ["physics", "navmesh"]`
- **依赖**：任务 1
- **验证**：`cd crates/scene && cargo check` 同时测试 `--no-default-features`

### 任务 3：manifest.rs 添加 NavMeshComponentDef
**文件**：`d:/Personal/lib/geese/crates/scene/src/manifest.rs`

- 在 `PhysicsComponentDef` 附近添加 `NavMeshComponentDef`（含 server_enabled、client_enabled、agent_radius）
- `ModelRef` 添加 `navmesh: Option<NavMeshComponentDef>` 字段
- `SceneObjectDef` 添加 `navmesh: Option<NavMeshComponentDef>` 字段
- **依赖**：任务 2
- **验证**：`cd crates/scene && cargo check`

### 任务 4：prefab_manifest.rs 添加 navmesh 支持
**文件**：`d:/Personal/lib/geese/crates/scene/src/prefab_manifest.rs`

- `PrefabNodeDef` 添加 `navmesh: Option<NavMeshComponentDef>` 字段
- 更新测试中的 `PrefabNodeDef` 构造，添加 `navmesh: None`
- **依赖**：任务 3
- **验证**：`cd crates/scene && cargo test prefab_manifest`

### 任务 5：EditorState 添加 NavMesh 缓存 + EditorAction 变体
**文件**：`d:/Personal/lib/geese/crates/editor/src/panels.rs`

- `EditorAction` 添加 `SetNavMeshComponent { node_id, component }` 变体
- `EditorState` 添加 `navmesh_component_cache: HashMap<String, NavMeshComponentDef>`
- `EditorState::new()` 初始化该缓存
- **依赖**：任务 3
- **验证**：`cd crates/editor && cargo check`

### 任务 6：SceneNodeData 添加 navmesh 字段
**文件**：`d:/Personal/lib/geese/crates/editor/src/hierarchy.rs`

- `SceneNodeData` 添加 `pub navmesh: Option<scene::manifest::NavMeshComponentDef>`
- 更新 `new()` 中所有示例节点构造（约 7 处），每处加 `navmesh: None,`
- **依赖**：任务 3
- **验证**：`cd crates/editor && cargo check`

### 任务 7：editor.rs 处理 NavMesh Action + 场景导入 + Prefab 操作
**文件**：`d:/Personal/lib/geese/crates/editor/src/editor.rs`

四个修改点：
1. `process_prefab_actions()` 添加 SetNavMeshComponent 处理分支（参考 SetPhysicsComponent）
2. `walk_gltf_node` 添加 navmesh_cache 参数，导入时填充缓存
3. `handle_save_as_prefab` 中 PrefabNodeDef 构造时从缓存读取 navmesh，同时修复 `physics: None` 硬编码 bug
4. `handle_instantiate_prefab` 中 SceneNodeData 构造时添加 `navmesh: node_def.navmesh.clone()`
- **依赖**：任务 5、任务 6
- **验证**：`cd crates/editor && cargo check`

### 任务 8：Inspector 添加 NavMesh Component UI
**文件**：`d:/Personal/lib/geese/crates/editor/src/inspector.rs`

- `InspectorPanel` 添加 `navmesh_enabled: bool`、`navmesh_agent_radius: f32` 字段
- 选择实体变化时同步 navmesh 缓存
- 添加 `push_navmesh_update()` 辅助方法
- 添加 `▼ NavMesh Component` 折叠面板（参考 Physics Component UI 模式）
- 含 Add/Remove 按钮 + Agent Radius slider（0.1..=2.0）
- **依赖**：任务 5
- **验证**：`cd crates/editor && cargo check`

### 任务 9：Editor Cargo.toml 添加 navmesh 依赖
**文件**：`d:/Personal/lib/geese/crates/editor/Cargo.toml`

- 添加：`navmesh = { path = "../navmesh", version = "0.1.0" }`
- **依赖**：任务 2
- **验证**：`cd crates/editor && cargo check`

### 任务 10：Scene 结构体添加运行时 NavMesh
**文件**：`d:/Personal/lib/geese/crates/scene/src/scene.rs`、`lib.rs`

- Scene 添加 `#[cfg(feature = "navmesh")] pub navmesh: Option<navmesh::NavMesh>` 字段
- Scene::new() 中初始化 `navmesh: None`
- 添加 `build_navmesh()` 桩方法
- lib.rs 中添加 navmesh re-export
- **依赖**：任务 2
- **验证**：`cd crates/scene && cargo build`

### 任务 11：端到端编译验证
- 按序编译 navmesh → scene → editor crate
- 运行 `cargo test` 确保所有测试通过
- 运行编辑器验证 NavMesh Component UI 可用
- **依赖**：任务 1-10 全部完成

## 拒绝的替代方案

| 方案 | 拒绝原因 |
|------|----------|
| Recast 体素生成集成 | 过度设计。当前阶段不需要自动从 mesh 生成导航网格，留待未来扩展 |
| UniformGrid 空间索引优化 | 当前场景规模下 O(N) locate() 足够，优化应在性能瓶颈确认后进行 |
| 增量 NavMesh 重建 | 现阶段全量重建足够，增量方案复杂度高 |
| Per-Object NavMesh 存储 | 增加内存开销和维护复杂度，Scene 级统一 NavMesh 更简洁 |

## 附注

在 `handle_save_as_prefab`（`editor.rs` L989-L998）中发现 `physics: None` 硬编码 bug——无论实体是否有 Physics Component，保存 Prefab 时均丢失。任务 7 将顺便修复此 bug。