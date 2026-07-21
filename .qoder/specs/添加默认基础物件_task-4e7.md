# 添加默认基础物件（球体、立方体、平面、圆柱体）

## 概述

在编辑器中添加创建基础几何体（Primitive）的能力，支持：Cube（立方体）、Sphere（球体）、Plane（平面）、Cylinder（圆柱体）。通过新建 `primitives.rs` 模块集中管理网格生成逻辑，编辑器通过 "GameObject" 菜单和 Hierarchy 右键菜单触发创建。

## 实现步骤

### 1. Scene Crate — 新建 primitives 模块

**文件**: `crates/scene/src/primitives.rs`（新建）

- 定义 `PrimitiveKind` 枚举：`Cube`, `Sphere`, `Plane`, `Cylinder`
- 从 `loader.rs` 迁移 `create_plane_mesh_procedural()` 和 `create_cube_mesh_procedural()`（改为 `pub`）
- 新增 `create_sphere_mesh_procedural(radius: f32, segments: u32, rings: u32) -> ModelMesh`
  - UV Sphere 算法：rings × segments 顶点，球面坐标计算 position/normal
  - 默认参数：radius=0.5, segments=32, rings=16
  - 正确生成法线（归一化 position）、UV (theta/2π, phi/π)、切线
- 新增 `create_cylinder_mesh_procedural(radius: f32, height: f32, segments: u32) -> ModelMesh`
  - 侧面 + 顶盖 + 底盖，各部分独立法线
  - 默认参数：radius=0.5, height=1.0, segments=32
- 新增调度函数：
  ```rust
  pub fn create_primitive_mesh(kind: PrimitiveKind) -> ModelMesh
  pub fn primitive_kind_from_str(s: &str) -> Option<PrimitiveKind>
  ```
- 迁移 `create_pbr_material()` 为 `pub`（或保留在 loader.rs 中，primitives 模块仅负责网格）
- 所有网格统一设置 `MeshFlags { has_normals: true, has_uv0: true, has_tangents: true, has_skin: false }`

**依赖**: 无前置依赖

### 2. Scene Crate — 模块注册与 loader 重构

**文件**: `crates/scene/src/lib.rs`
- 添加 `pub mod primitives;`
- 添加 re-export: `pub use primitives::{PrimitiveKind, create_primitive_mesh};`

**文件**: `crates/scene/src/loader.rs`
- 删除 `create_plane_mesh_procedural` 和 `create_cube_mesh_procedural` 函数体
- 第 174 行的 `match obj_def.object_type.as_str()` 扩展：
  ```rust
  "sphere" => (primitives::create_sphere_mesh_procedural(0.5, 32, 16), obj_def.color),
  "cylinder" => (primitives::create_cylinder_mesh_procedural(0.5, 1.0, 32), obj_def.color),
  ```
- 原有 `"plane"` / `"cube"` 分支改为调用 `primitives::create_primitive_mesh()`
- `create_pbr_material` 保持 `pub(crate)` 或改为 `pub`（供 editor 使用）

**依赖**: 步骤 1

### 3. Manifest 序列化支持

**文件**: `crates/scene/src/manifest.rs`
- `SceneObjectDef.object_type` 文档注释更新：`"plane" | "cube" | "sphere" | "cylinder"`

**文件**: `crates/scene/src/prefab_manifest.rs`
- `PrefabMeshDef::Procedural.object_type` 同步更新支持列表

**依赖**: 步骤 1（仅文档更新，无逻辑依赖）

### 4. Editor — EditorAction 扩展

**文件**: `crates/editor/src/panels.rs`
- 在 `EditorAction` 枚举末尾（`OpenBuildPanel` 之后）新增：
  ```rust
  /// 创建基础几何体
  CreatePrimitive {
      kind: String,
      position: [f32; 3],
      parent_node_id: Option<String>,
  },
  ```

**依赖**: 无前置依赖

### 5. Editor — "GameObject" 菜单

**文件**: `crates/editor/src/editor.rs`（`show_menu_bar` 方法，约 line 502）
- 在 "Edit" 和 "View" 菜单之间插入 "GameObject" 菜单：
  ```rust
  ui.menu_button("GameObject", |ui| {
      ui.menu_button("3D Object", |ui| {
          if ui.button("Cube").clicked() { /* push CreatePrimitive kind="cube" */ }
          if ui.button("Sphere").clicked() { /* push CreatePrimitive kind="sphere" */ }
          if ui.button("Plane").clicked() { /* push CreatePrimitive kind="plane" */ }
          if ui.button("Cylinder").clicked() { /* push CreatePrimitive kind="cylinder" */ }
      });
  });
  ```
- position 默认 `[0.0, 0.0, 0.0]`，parent_node_id 使用当前选中节点

**依赖**: 步骤 4

### 6. Editor — 处理 CreatePrimitive Action

**文件**: `crates/editor/src/editor.rs`（`process_prefab_actions` 方法，约 line 947）
- 新增 match 分支处理 `EditorAction::CreatePrimitive`：
  1. 调用 `scene::primitives::create_primitive_mesh(kind)` 生成网格
  2. 创建默认 PBR 材质（灰色 `[0.8, 0.8, 0.8]`），push 到 `scene.materials.materials`
  3. 设置 `mesh.material = Some(MaterialHandle(material_idx))`
  4. 调用 `scene.add_static_object(mesh, position, rotation, scale)` 获取 entity_id
  5. 创建 `SceneNodeData`（`NodeType::Mesh`），添加到 hierarchy tree
  6. 更新 `state.transform_cache`、`state.name_cache`、`state.mesh_entities`
  7. 选中新实体：`state.selected_entity = Some(entity_id)`
- 参考现有 `handle_instantiate_prefab`（line 1194）的模式

**依赖**: 步骤 1, 2, 4

### 7. Editor — Hierarchy 右键菜单（可选增强）

**文件**: `crates/editor/src/hierarchy.rs`
- 在现有右键上下文菜单中添加 "Create Primitive >" 子菜单
- 包含 Cube / Sphere / Plane / Cylinder 选项
- 点击后 push `EditorAction::CreatePrimitive`，parent 为右键点击的节点

**依赖**: 步骤 4

### 8. 单元测试

**文件**: `crates/scene/src/primitives.rs`（底部 `#[cfg(test)]` 模块）
- 测试每种 primitive 生成非空 vertices/indices
- 验证顶点法线归一化（length ≈ 1.0）
- 验证 UV 坐标在 [0, 1] 范围
- 验证索引值不超出顶点数范围
- 验证顶点数/索引数符合预期公式

**依赖**: 步骤 1

## 依赖关系图

```
步骤 1 (primitives.rs)
  ├──→ 步骤 2 (lib.rs + loader.rs)
  │       └──→ 步骤 6 (editor action handler)
  ├──→ 步骤 3 (manifest docs) [独立]
  └──→ 步骤 8 (tests) [独立]

步骤 4 (EditorAction) [独立]
  ├──→ 步骤 5 (menu)
  ├──→ 步骤 6 (handler)
  └──→ 步骤 7 (hierarchy menu)
```

## 代码约定

- 坐标系：右手系，Y 轴朝上（cgmath）
- 顶点格式：`Vertex { position: Point3<f32>, normal: Vector3<f32>, uv: Vector2<f32>, tangent: [f32;4], joints: [u16;4], weights: [f32;4] }`
- 网格初始化：`ModelMesh::new()` 后填充 vertices/indices/flags
- 编辑器节点 ID 格式：`format!("node_{}", uuid::Uuid::new_v4())`
- 材质索引：`MaterialHandle(scene.materials.materials.len())` push 后获取

## 风险与缓解

| 风险 | 缓解措施 |
|------|----------|
| Sphere 法线/绕序错误导致渲染异常 | 单元测试验证法线归一化 + 索引范围；使用标准 UV Sphere 算法 |
| 现有 scene 文件使用 `"plane"`/`"cube"` 字符串 | 保持字符串序列化格式不变，仅新增 `"sphere"`/`"cylinder"` |
| Editor undo/redo 不支持新创建的 primitive | 首期不实现 undo（与 Create Empty 行为一致），后续可用 `CreateEntityCommand` 包装 |
| `create_pbr_material` 可见性变更 | 改为 `pub` 是安全的（loader 已是 pub mod） |

## 被拒绝的替代方案

1. **独立 `crates/primitives/` crate**（方案 B）：过度设计。当前仅 4 种 primitive，无需独立 crate 增加依赖管理复杂度。scene crate 内的模块足够。
2. **GPU 内容哈希缓存共享**（方案 B）：修改 `ModelMesh` 结构 + `GpuResourceCache` 键机制影响面过大，且当前 primitive 数量少，GPU buffer 重复不是瓶颈。
3. **LOD 链自动生成**（方案 B）：`lod_levels` 字段已预留但 feature gate 默认禁用，当前无需实现。
4. **全部代码留在 loader.rs**（方案 C 原始方案）：loader.rs 已 579 行，新增 ~120 行球体/圆柱体代码后可读性下降。独立 `primitives.rs` 模块更清晰。
