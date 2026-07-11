# Geese 3D 游戏引擎 — 功能清单与代码审查报告

> **生成日期**：2026-07-11  
> **审查范围**：全代码库（38 个共享 crate + desktop / server / client 三个入口项目）  
> **审查方法**：完整性 · 正确性 · 影响分析三视角并行审查 + 代码质量扫描，结果去重合并

---

## 第一部分：已有功能清单

### 1. 渲染管线（`crates/render/`）

| 功能 | 状态 | 说明 |
|------|:----:|------|
| Forward+ 管线 | ✅ | Cluster 光照剔除（8×8×16 = 1024 clusters）、实例化渲染、完整可用 |
| Deferred+ 管线 | ⚠️ | G-Buffer（3 张）+ Cluster Culling + Lighting 完整，**缺少阴影集成** |
| CSM 级联阴影 | ✅ | 最多 4 级联、PSSM lambda 划分、atlas 布局、PCF 采样（仅 Forward+ 可用） |
| IBL 环境光照 | ✅ | HDRI / 程序化 / 纯色三模式 + Irradiance / Prefiltered / BRDF LUT 三件套 |
| ACES Tonemap | ✅ | 后处理链首环节 |
| Bloom | ⚠️ | GPU pass 已实现，但合成时**丢失原始场景颜色** |
| TAA 时序抗锯齿 | ✅ | Halton(2,3) jitter |
| SSAO / SSR / DoF / MotionBlur | ❌ | 枚举和 uniform 位已定义，**无 GPU shader 实现**，推入 PostChain 后静默跳过 |
| 骨骼蒙皮 | ✅ | 5 种模式：CPU / Uniform / SSBO / VertexPulling / Morph，MAX_JOINTS = 256 |
| GPU 粒子 | ✅ | compute shader + 实例化 + indirect draw（feature-gated，默认关闭） |
| Hi-Z 遮挡剔除 | ✅ | 深度金字塔 + AABB 投影测试（feature-gated，默认关闭） |
| LOD | ✅ | camera_distance 选择 |
| 渲染图 DAG | ✅ | 声明式 RenderGraphBuilder + Kahn 拓扑排序 + 自动 barrier |
| Shader Graph | ✅ | 节点式编辑器 + WGSL 代码生成 |

### 2. 场景与资源

| 功能 | 状态 | 说明 |
|------|:----:|------|
| Scene 场景图 | ✅ | Scene（1416 行）、SceneObject、DirtyFlags |
| Octree 八叉树 | ✅ | 视锥体裁剪查询，解耦 id 索引 |
| Prefab 系统 | ✅ | JSON manifest、嵌套引用 + DFS 循环检测 + max_depth 限制 |
| glTF 导入 | ✅ | 网格 / 蒙皮 / 动画 / 材质 / 切线 / 形态目标，两遍加载 |
| 动画状态机 | ✅ | AnimationStateMachine + BlendTree + Transition + Marker 事件 |
| 角色动画图 | ✅ | CharacterAnimationGraph（状态机驱动） |
| 场景序列化 | ✅ | manifest / prefab_manifest / avatar_manifest（JSON） |
| Python 脚本组件 | ✅ | ScriptComponent + ScriptSystem（feature-gated PyO3） |
| 场景网络复制 | ✅ | SceneObjectNetMsg + msgpack |
| NavMesh 寻路 | ⚠️ | A* + 漏斗平滑已实现，**自动构建为桩实现** |
| 事件组件系统 | ❌ | `evaluate_event_components()` 为空函数 |
| Handle / AssetLoader / AssetCache | ✅ | Arc 引用计数、(TypeId, path) 去重 |
| 异步加载 | ✅ | AsyncAssetCache（tokio） |
| 热重载 | ✅ | notify feature |
| VFS 虚拟文件系统 | ✅ | MountPoint + 最长前缀匹配 |
| 资源数据库 / 依赖扫描 / Bundle | ✅ | UUID + 路径映射 |
| 纹理 Cooker | ❌ | stub，未集成 basis-universal |
| 网格 Cooker | ❌ | stub，未集成 meshopt |

### 3. 物理系统

| 功能 | 状态 | 说明 |
|------|:----:|------|
| rapier3d 多场景世界 | ✅ | PhysicsWorld → PhysicsScene |
| 刚体 / 碰撞体 / 关节 | ✅ | Static / Dynamic / Kinematic + 多种形状 + Fixed / Revolute / Spherical |
| 射线检测 / 碰撞事件 | ✅ | RayHit / CollisionEvent / ContactForceEvent |
| 胶囊体控制器 | ✅ | CapsuleController |
| 角色物理桥接 | ✅ | CharacterPhysics |
| Ragdoll | ✅ | RagdollBuilder / Config / Instance（两份重复副本） |
| 远程物理 TCP | ✅ | physics_client（MessagePack） |
| 统一物理管理器 | ✅ | PhysicsManager（本地 / 远程切换） |
| Python 绑定 | ✅ | pyo3 feature |
| GLTF 场景构建器 | ✅ | scene-builder feature |

### 4. 编辑器（`crates/editor/`）

| 功能 | 状态 | 说明 |
|------|:----:|------|
| 3D 视口 | ✅ | OrbitCamera / Gizmo / 射线-AABB 拾取 / 物理调试叠加 |
| 场景层级树 | ✅ | 节点类型区分、拖拽、搜索 |
| 属性面板 (Inspector) | ⚠️ | Transform 同步为空函数，Mesh Renderer 显示占位文本 |
| 资源浏览器 | ✅ | 目录遍历、缩略图、拖入场景 |
| 动画预览 | ✅ | Marker 编辑 |
| GLTF 导入向导 | ✅ | 选择节点 / 动画 / 蒙皮 |
| 材质编辑器 | ✅ | — |
| Shader Graph 编辑器 | ✅ | 可视化节点编辑 |
| Bundle / Build 面板 | ✅ | — |
| 物理调试渲染 | ✅ | 碰撞体线框 |
| 撤销 / 重做 | ✅ | CommandHistory + SceneSerializer |
| PlayMode | ✅ | 编辑 / 运行模式切换 |
| 角色控制器集成 | ❌ | 仅打日志，无物理集成 |

### 5. 网络 / 多人

| 功能 | 状态 | 说明 |
|------|:----:|------|
| Hub / Gate / DBProxy 三节点 | ✅ | TCP + Redis MQ + Thrift |
| Consul 服务发现 | ✅ | 注册 / 发现 + 健康检查 |
| TCP / WS / WSS 接入 | ✅ | 三协议 + TLS |
| 双向 RPC + 通知 | ✅ | Hub-Client / Hub-Hub / Hub-DBProxy |
| 实体迁移 | ✅ | MigrateEntity / Complete |
| AOI 兴趣管理 | ✅ | GridAoi 九宫格 + Enter / Leave 事件 |
| 状态同步插值 | ✅ | SnapshotBuffer + Lagged / Extrapolated + 速度外推 |
| MongoDB 持久化 | ✅ | DBProxy CRUD via Redis MQ |
| RPC 代码生成 | ✅ | Python / TypeScript stub |
| 版本协商 | ❌ | Thrift 定义了 `version_handshake` 但未加入 service union |

### 6. 其他模块

| 功能 | 状态 | 说明 |
|------|:----:|------|
| 轻量 ECS | ✅ | Component trait + World + EntityBuilder |
| 输入系统 | ✅ | 键盘 / 鼠标 / 手柄 + ActionMap |
| 音频系统 | ⚠️ | 多通道混音 + rodio 后端，3D 空间衰减未实现 |
| 地形系统 | ✅ | Heightmap + 流式加载 + geo-clipmap LOD |
| 配置加载 | ⚠️ | JSON / TOML 可加载，渲染路径配置**未桥接到运行时** |
| 分布式日志 | ✅ | tracing + OpenTelemetry / Jaeger |
| 健康检查 / 优雅关闭 | ✅ | axum HTTP + signal-hook |
| Launcher 模板生成 | ⚠️ | 流程可用，生成代码为空壳 |
| 游戏运行时 | ⚠️ | winit + wgpu 事件循环，**硬编码 Forward+** |

---

## 第二部分：代码审查发现

### 严重问题（MUST FIX） — 7 项

#### C1. EventBus `unsafe impl Sync` + `RefCell` = 未定义行为

**文件**：[`crates/event/src/bus.rs#L96-L126`](file:///c:/Users/theDa/Documents/workspace/library/geese/crates/event/src/bus.rs#L96-L126)

`EventBus` 使用 `RefCell<HashMap>` 实现内部可变性，却通过 `unsafe impl Send/Sync` 声称线程安全。`publish(&self)` 调用 `borrow_mut()`，跨线程并发调用会导致双重可变借用——**未定义行为 (UB)**。SAFETY 注释自相矛盾。

```rust
// 修复方案 A：线程安全
use std::sync::RwLock;
pub struct EventBus {
    channels: RwLock<HashMap<TypeId, Box<dyn AnyChannel + Send + Sync>>>,
}

// 修复方案 B：单线程 — 删除 unsafe impl Sync，保留 RefCell
```

#### C2. 分布式锁获取失败被 `unwrap_or_default()` 静默绕过

**文件**：[`hub_service_manager.rs#L289`](file:///c:/Users/theDa/Documents/workspace/library/geese/server/lib/hub/src/hub_service_manager.rs#L289)、[`L355`](file:///c:/Users/theDa/Documents/workspace/library/geese/server/lib/hub/src/hub_service_manager.rs#L355)、[`hub_server.rs#L188`](file:///c:/Users/theDa/Documents/workspace/library/geese/server/lib/hub/src/hub_server.rs#L188)

`acquire_lock` 失败时返回空字符串，后续代码误以为获锁成功，多个 hub 同时连接同一 gate——破坏分布式锁互斥设计。

```rust
let value = match _service.acquire_lock(_lock_key.clone(), 3, None).await {
    Ok(v) => v,
    Err(e) => {
        error!("Failed to acquire lock for gate '{}': {}", _gate_name, e);
        return hub_name;
    }
};
```

#### C3. `DBEvent` 的 `unsafe impl Send` 缺少类型级约束

**文件**：[`server/lib/dbproxy/src/db.rs#L136-L145`](file:///c:/Users/theDa/Documents/workspace/library/geese/server/lib/dbproxy/src/db.rs#L136-L145)

`ev_data: Box<dyn Any>` 无 `Send` 超约束，安全性完全依赖人工纪律。

```rust
pub trait DBEventData: Any + Send {}
impl<T: Any + Send> DBEventData for T {}
// 替换 Box<dyn Any> → Box<dyn DBEventData>，删除 unsafe impl
```

#### C4. Deferred+ 管线完全缺少阴影集成

**文件**：[`crates/render/src/deferred_plus.rs#L31-L80`](file:///c:/Users/theDa/Documents/workspace/library/geese/crates/render/src/deferred_plus.rs#L31-L80)

无 `shadow_pass` / `shadow_atlas` / `CsmUniform`，`deferred_lighting.wgsl` 无 shadow map 采样。选择 Deferred 路径时阴影**静默丢失**，无编译错误或运行时警告。

**修复**：参照 `ForwardPlusPipeline` 添加 ShadowPass + WgpuShadowAtlas。短期至少添加 `log::warn!`。

#### C5. `Scene::build_navmesh()` 始终返回 None

**文件**：[`crates/scene/src/scene.rs#L633-L639`](file:///c:/Users/theDa/Documents/workspace/library/geese/crates/scene/src/scene.rs#L633-L639)

桩实现 `self.navmesh = None`，`navmesh` feature 默认启用但功能不可用。

**修复**：从标记了 navmesh 组件的场景对象提取三角形数据，调用 `NavMesh::from_triangles()` 构建。

#### C6. `Scene::evaluate_event_components()` 是空函数

**文件**：[`crates/scene/src/scene.rs#L852-L854`](file:///c:/Users/theDa/Documents/workspace/library/geese/crates/scene/src/scene.rs#L852-L854)

方法体为空，每帧在 `tick()` 中调用。`triggered_events` 永远不被填充，事件组件游戏逻辑无法触发。

#### C7. `InspectorPanel::sync_transform()` 是空函数

**文件**：[`crates/editor/src/inspector.rs#L95-L97`](file:///c:/Users/theDa/Documents/workspace/library/geese/crates/editor/src/inspector.rs#L95-L97)

Inspector 缓存未命中时回退到 `[0,0,0]`，显示错误数据。

---

### 警告（SHOULD FIX） — 13 项

| 编号 | 问题 | 位置 | 修复方向 |
|:----:|------|------|----------|
| W1 | SSAO/SSR/DoF/MotionBlur 无 GPU 实现，静默跳过 | `render/post.rs` `post_pipeline.rs` | 添加 shader 或 `log::warn!` 提示 |
| W2 | Bloom 合成丢失原始场景颜色 | `render/post_pipeline.rs#L375-L384` | tonemap shader 叠加 input + bloom |
| W3 | 分布式锁释放 `let _ =` 静默丢弃（8 处） | hub_service_manager / hub_proxy_manager / hub_server 等 | `if let Err(e) = ... { warn!(...) }` |
| W4 | PyO3 构造函数 `panic!()` 跨 FFI 崩溃 | `hub/lib.rs#L799` `L833` | 返回 `PyResult::Err` |
| W5 | Thrift Option 字段 `.unwrap()`（14+ 处） | `hub_service_manager.rs` 多处 | match None + 日志 |
| W6 | `Mutex::lock().unwrap()` 级联 panic（16 处） | hub_service_manager / hub_server / dbproxy_msg_handle | `unwrap_or_else(\|e\| e.into_inner())` |
| W7 | TextureCooker / MeshCooker 是直通 stub | `asset/texture_cooker.rs` | 集成 basis-universal + meshopt |
| W8 | Launcher 模板生成空壳代码 | `launcher/templates.rs#L729-L733` | 生成可运行 boilerplate |
| W9 | ConfigRenderingPath ↔ RenderingPath 无转换 | `config/engine_config.rs` `render/pipeline.rs` | 添加 `From` impl |
| W10 | `version_handshake` 未接入 service union | `proto/gate.thrift#L219-L233` | 添加到 `gate_client_service` |
| W11 | Ragdoll 两份 320 行重复代码 | `scene/ragdoll.rs` `gameplay_physics/ragdoll.rs` | 提取独立 crate |
| W12 | 编辑器 CharacterController 仅打日志 | `editor/editor.rs#L843-L849` | 接入 PhysicsManager |
| W13 | `instancing` feature 默认开启 | `render/Cargo.toml#L13` | 改为 `default = []` |

---

### 建议（CONSIDER） — 9 项

| 编号 | 问题 | 位置 |
|:----:|------|------|
| S1 | Inspector Mesh Renderer 显示占位文本 "(from GLTF)" | `inspector.rs#L267` |
| S2 | Edition 不统一（2021 vs 2024） | 整个项目 |
| S3 | 音频 3D 空间衰减模型未实现 | `audio/spatial.rs` |
| S4 | PhysicsManager 后端切换时状态丢失 | `physics_manager/lib.rs` |
| S5 | 10 个核心 crate 缺少测试（camera / math / time 优先） | 多处 |
| S6 | `game_runtime` 硬编码 Forward+ | `game_runtime/lib.rs` |
| S7 | 无 Cargo workspace，测试需逐个执行 | 项目根 |
| S8 | Deferred+ 无阴影也无运行时警告 | `deferred_plus.rs` |
| S9 | `dbproxy/db.rs` 7 个 do_* 方法 80% 重复 | `db.rs` |

---

## 第三部分：缺陷统计

| 严重程度 | 数量 | 关键类别 |
|----------|:----:|----------|
| **严重 (MUST FIX)** | 7 | unsafe UB (2)、分布式锁绕过 (1)、功能空桩 (4) |
| **警告 (SHOULD FIX)** | 13 | 静默错误丢弃 (3)、panic 风险 (3)、功能不完整 (4)、死代码/重复 (3) |
| **建议 (CONSIDER)** | 9 | 占位显示 (1)、配置一致性 (3)、测试覆盖 (1)、代码组织 (4) |

---

## 第四部分：后续功能路线图

### P0 — 立即修复（安全 / 核心功能缺口）

| 功能 | 当前状态 | 工作量 | 关联编号 |
|------|----------|:------:|:--------:|
| EventBus 线程安全 | UB 风险 | 小 | C1 |
| DBEvent Send 类型约束 | unsafe 依赖人工纪律 | 小 | C3 |
| Deferred+ 阴影集成 | 完全缺失 | 大 | C4 |
| NavMesh 自动构建 | 桩实现 | 中 | C5 |
| 事件组件运行时 | 空函数 | 中 | C6 |
| 分布式锁错误处理 | 静默绕过/丢弃 | 小 | C2, W3 |

### P1 — 短期补齐（功能缺口 / 稳定性）

| 功能 | 当前状态 | 工作量 | 关联编号 |
|------|----------|:------:|:--------:|
| SSAO / SSR / DoF / MotionBlur 后处理 | 仅枚举定义 | 大 | W1 |
| Bloom + 原场景合成 | 合成丢失 | 小 | W2 |
| Inspector Transform 同步 | 空函数 | 小 | C7 |
| 纹理压缩 Cooker | stub | 中 | W7 |
| 网格优化 Cooker | stub | 中 | W7 |
| 服务器 panic 消除（30+ 处） | unwrap / panic | 中 | W4, W5, W6 |
| 版本协商接入 RPC | 死代码 | 小 | W10 |
| 配置→渲染路径桥接 | 缺失转换 | 小 | W9 |

### P2 — 中期完善（用户体验）

| 功能 | 当前状态 | 工作量 | 关联编号 |
|------|----------|:------:|:--------:|
| Launcher 模板完善 | 空壳代码 | 中 | W8 |
| GPU 粒子 / Hi-Z 默认启用策略 | feature-gated | 小 | W13 |
| Ragdoll 代码去重 | 两份副本 | 小 | W11 |
| 编辑器角色控制器集成 | 仅打日志 | 中 | W12 |
| 音频 3D 空间衰减 | 逻辑缺失 | 中 | S3 |
| 运行时管线路径切换 | 硬编码 | 小 | S6 |
| PhysicsManager 状态迁移 | 切换丢失 | 中 | S4 |

### P3 — 长期改进（工程质量）

| 功能 | 当前状态 | 工作量 | 关联编号 |
|------|----------|:------:|:--------:|
| 基础 crate 测试覆盖 | 10 个 crate 无测试 | 中 | S5 |
| Edition 统一 | 2021 / 2024 混用 | 小 | S2 |
| 原生 Rust 服务器入口 | TODO | 大 | — |
| db.rs 重复代码重构 | 80% 重复 | 小 | S9 |

---

## 第五部分：模块成熟度评价

| 模块 | 成熟度 | 评价 |
|------|:------:|------|
| 渲染管线（Forward+） | ★★★★★ | 最完备模块：CSM / IBL / 后处理 / 蒙皮 / 渲染图 / Shader Graph 均完整 |
| 服务端三节点架构 | ★★★★☆ | Hub / Gate / DBProxy 功能完整，错误处理和安全性需加强 |
| 场景系统 | ★★★★☆ | 动画 / Prefab / glTF / 序列化扎实，NavMesh 和事件系统需补齐 |
| 物理系统 | ★★★★☆ | rapier3d 封装完整，角色控制器 / Ragdoll / 远程物理可用 |
| 编辑器 | ★★★★☆ | 10+ 面板功能丰富，Inspector 同步和物理集成需完善 |
| 网络基础设施 | ★★★★☆ | 多协议 / RPC / 服务发现 / 实体迁移完备，版本协商待接入 |
| 资源管理 | ★★★☆☆ | 核心管线完整（Handle / Cache / 异步 / 热重载），Cooker 需补实现 |
| 音频 | ★★☆☆☆ | 基础框架就绪（混音 / rodio），3D 空间化待实现 |
| Launcher | ★★☆☆☆ | 选择模板 → 生成工程流程可用，生成代码为空壳 |
