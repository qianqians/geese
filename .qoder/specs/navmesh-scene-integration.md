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
- 为 `Vec2`、`NavTri`、`NavMesh` 添加 `#[derive(Serialize, Deserialize)]`
- `Serialize`/`Deserialize` 通过 feature flag 控制（`#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]`），避免强制引入 serde
- **依赖**：无
- **验证**：`cd crates/navmesh && cargo build`，测试全部通过

### 任务 2：Scene Crate 添加 navmesh 依赖与 feature flag
**文件**：`d:/Personal/lib/geese/crates/scene/Cargo.toml`

- 添加依赖：`navmesh = { path = "../navmesh", version = "0.1.0", optional = true }`
- 添加 feature：`navmesh = ["dep:navmesh"]`
- 更新 default features：`default = ["physics", "navmesh"]`
- **依赖**：任务 1
- **验证**：`cd crates/scene && cargo check`，`cargo check --no-default-features` 均通过

### 任务 3：manifest.rs 添加 NavMeshComponentDef
**文件**：`d:/Personal/lib/geese/crates/scene/src/manifest.rs`

在 `PhysicsComponentDef` 定义附近（约 L153 之后）添加：
```rust
/// 实体 NavMesh 组件定义。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NavMeshComponentDef {
    /// 服务器是否启用导航网格
    #[serde(default = "default_true")]
    pub server_enabled: bool,
    /// 客户端是否启用导航网格
    #[serde(default = "default_true")]
    pub client_enabled: bool,
    /// 导航代理半径
    #[serde(default = "default_agent_radius")]
    pub agent_radius: f32,
}

fn default_agent_radius() -> f32 { 0.5 }
```

- `ModelRef` 添加 `navmesh: Option<NavMeshComponentDef>` 字段（参考 L62 physics 字段）
- `SceneObjectDef` 添加 `navmesh: Option<NavMeshComponentDef>` 字段（参考 L176 physics 字段）
- **依赖**：任务 2
- **验证**：`cd crates/scene && cargo check`

### 任务 4：prefab_manifest.rs 添加 navmesh 支持
**文件**：`d:/Personal/lib/geese/crates/scene/src/prefab_manifest.rs`

- `PrefabNodeDef` 添加 `navmesh: Option<NavMeshComponentDef>` 字段（参考 L67-L69 physics）
- **不添加** `effective_navmesh()` 方法（NavMesh 不需要旧格式兼容迁移）
- 更新测试中的 `PrefabNodeDef` 构造，添加 `navmesh: None`
- **依赖**：任务 3
- **验证**：`cd crates/scene && cargo test prefab_manifest`

### 任务 5：EditorState 添加 NavMesh 缓存 + EditorAction 变体
**文件**：`d:/Personal/lib/geese/crates/editor/src/panels.rs`

- `EditorAction` 枚举添加变体（参考 L47-L51）：
  ```rust
  SetNavMeshComponent {
      node_id: String,
      component: Option<scene::manifest::NavMeshComponentDef>,
  },
  ```
- `EditorState` 添加缓存字段（参考 L121）：
  ```rust
  pub navmesh_component_cache: HashMap<String, scene::manifest::NavMeshComponentDef>,
  ```
- `EditorState::new()` 中初始化 `navmesh_component_cache: HashMap::new()`
- **依赖**：任务 3
- **验证**：`cd crates/editor && cargo check`

### 任务 6：SceneNodeData 添加 navmesh 字段
**文件**：`d:/Personal/lib/geese/crates/editor/src/hierarchy.rs`

- `SceneNodeData` 添加字段（参考 L34 physics）：
  ```rust
  pub navmesh: Option<scene::manifest::NavMeshComponentDef>,
  ```
- 更新 `new()` 中所有示例节点构造（L176-L259，共 7 处），每处加 `navmesh: None,`
- **依赖**：任务 3
- **验证**：`cd crates/editor && cargo check`

### 任务 7：editor.rs 处理 NavMesh Action + 场景导入 + Prefab 操作
**文件**：`d:/Personal/lib/geese/crates/editor/src/editor.rs`

这是变更最多的文件，涉及 4 个修改点：

1. **`process_prefab_actions()`**（L764-L819）：添加 `EditorAction::SetNavMeshComponent` 处理分支（参考 L785-L795 的 SetPhysicsComponent 处理），更新 `navmesh_component_cache`

2. **`walk_gltf_node` 函数**（L319-L368）：添加 `navmesh_cache: &mut HashMap<String, NavMeshComponentDef>` 参数，导入时从 manifest 读取并填充缓存。调用处（L372）传递 `&mut self.state.navmesh_component_cache`

3. **`handle_save_as_prefab`**（L916-L998）：`PrefabNodeDef` 构造时从 `navmesh_component_cache` 读取 navmesh（L989-L998），同时修复 **bug**：`physics: None` 改为从缓存读取

4. **`handle_instantiate_prefab`**（L1056-L1156）：`SceneNodeData` 构造时添加 `navmesh: node_def.navmesh.clone()`（参考 L1132 physics 字段）

- **依赖**：任务 5、任务 6
- **验证**：`cd crates/editor && cargo check`

### 任务 8：Inspector 添加 NavMesh Component UI
**文件**：`d:/Personal/lib/geese/crates/editor/src/inspector.rs`

- `InspectorPanel` 结构体添加字段（L30 附近）：
  ```rust
  navmesh_enabled: bool,
  navmesh_agent_radius: f32,
  ```
- `new()` 中初始化：`navmesh_enabled: false, navmesh_agent_radius: 0.5`
- 选择实体变化时同步 navmesh 缓存（参考 L152-L163 physics 同步）
- 添加 `push_navmesh_update()` 辅助方法（参考 L58-L73 `push_physics_update`）
- 在 Physics Component 面板之后添加 `▼ NavMesh Component` 折叠面板（参考 L249-L301），含：
  - Add/Remove Component 按钮
  - Agent Radius slider（0.1..=2.0）
- 更新 "All Components" 列表（L339-L345），添加 NavMesh 条目
- **依赖**：任务 5
- **验证**：`cd crates/editor && cargo check`

### 任务 9：Editor Cargo.toml 添加 navmesh 依赖
**文件**：`d:/Personal/lib/geese/crates/editor/Cargo.toml`

- 添加：`navmesh = { path = "../navmesh", version = "0.1.0" }`（与 physics 同级，L25 附近）
- **依赖**：任务 2
- **验证**：`cd crates/editor && cargo check`

### 任务 10：Scene 结构体添加运行时 NavMesh
**文件**：`d:/Personal/lib/geese/crates/scene/src/scene.rs`

- `Scene` 结构体添加字段（L68 附近，在 `#[cfg(feature = "physics")]` 块外）：
  ```rust
  #[cfg(feature = "navmesh")]
  pub navmesh: Option<navmesh::NavMesh>,
  ```
- `Scene::new()` 中初始化 `navmesh: None`（约 L123 附近）
- 添加 `build_navmesh()` 方法：
  ```rust
  #[cfg(feature = "navmesh")]
  pub fn build_navmesh(&mut self) {
      // 从标记了 navmesh 组件的对象构建 NavMesh
      // 初期版本：通过外部传入三角形数据构建，暂不自动从 mesh 提取
      // 未来可扩展为从 GLTF 碰撞网格或单独的 .navmesh 文件加载
  }
  ```
- 在 `scene.rs` 顶部添加 `#[cfg(feature = "navmesh")] use navmesh::NavMesh;` 导入
- `lib.rs` 中添加 `#[cfg(feature = "navmesh")] pub use navmesh;` re-export
- **依赖**：任务 2
- **验证**：`cd crates/scene && cargo build`、`cargo build --no-default-features`

### 任务 11：端到端编译验证
- 确保所有 crate 在 default features 下编译通过：
  ```powershell
  cd d:\Personal\lib\geese\crates\navmesh; cargo build
  cd d:\Personal\lib\geese\crates\scene; cargo build
  cd d:\Personal\lib\geese\crates\editor; cargo build
  ```
- 确保 navmesh 相关测试通过：
  ```powershell
  cd d:\Personal\lib\geese\crates\navmesh; cargo test
  cd d:\Personal\lib\geese\crates\scene; cargo test
  ```
- **依赖**：任务 1-10 全部完成

## 验证方式

1. **编译验证**：按任务 11 执行编译命令，确保无错误
2. **测试验证**：运行 navmesh crate 测试（已有 8 个 test），scene crate 测试
3. **编辑器功能验证**：
   - 运行 `python run_editor.py` 打开编辑器
   - 在 Hierarchy 中选择一个 Mesh 节点
   - Inspector 中应出现 `▼ NavMesh Component` 折叠面板
   - 点击 "Add Component" 启用 NavMesh
   - 调整 Agent Radius slider
   - 点击 "Remove" 移除 NavMesh 组件
   - 保存为 Prefab 后重新加载，NavMesh 配置应保留

## 拒绝的替代方案

| 方案 | 拒绝原因 |
|------|----------|
| Plan B 的 Recast 体素生成集成 | 过度设计。当前阶段不需要自动从任意 mesh 生成导航网格；用户可手动指定或从外部工具导入。留待 `oxidized_navigation` 接入时实现 |
| Plan B 的 UniformGrid 空间索引 | 当前场景规模下 O(N) locate() 足够；优化应在性能瓶颈确认后进行 |
| Plan B 的增量 NavMesh 重建 | 现阶段全量重建足够；增量方案复杂度高、收益不明确 |
| 将 NavMesh 数据存储在 SceneObject 上（Per-Object） | 增加内存开销和维护复杂度；Scene 级统一 NavMesh 更简洁 |
| 添加 `effective_navmesh()` 旧格式兼容方法 | NavMesh 是新功能，无旧格式需要兼容 |

## 附注：Physics Prefab 保存 Bug

在 `handle_save_as_prefab`（`editor.rs` L989-L998）中发现 `physics: None` 硬编码问题——无论实体是否有 Physics Component，保存 Prefab 时均丢失。本方案中任务 7 将顺便修复此 bug。
