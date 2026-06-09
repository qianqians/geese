# 编辑器双端开发支持 + 场景物理世界方案

## Context

当前编辑器 (`crates/editor`) 是纯 Rust/egui 桌面应用,通过 `desktop` crate 的 pyo3 绑定暴露给 Python。用户需要在编辑器中同时支持两种开发模式:

- **Server 模式**: 编辑器连接 Python 物理服务器,物理模拟在服务端运行 (通过 pyhub → physics crate),服务于多人在线游戏后端开发
- **Client 模式**: 编辑器直接内嵌 `physics` crate,物理模拟在 Rust 本地运行,服务于单机/客户端游戏开发

两种模式共享相同的场景数据 (`.scene.json`)、相同的碰撞体加载逻辑 (来自 `scene_physics.py`)、相同的调试渲染方式,但物理引擎运行环境和步进方式不同。

## 架构设计

### 统一物理抽象层

```
┌─────────────────────────────────────────────────────────────┐
│  Editor (Rust/egui)                                         │
│                                                             │
│  ┌─────────────────────────────────────────────────────┐   │
│  │  DevelopmentTarget                                   │   │
│  │  - Server → server_physics_backend                   │   │
│  │  - Client → local_physics_backend                    │   │
│  └────────────────────┬────────────────────────────────┘   │
│                       │                                     │
│  ┌────────────────────▼────────────────────────────────┐   │
│  │  PhysicsBackend trait (统一接口)                     │   │
│  │  - init(gravity)                                     │   │
│  │  - load_scene(manifest_path)                         │   │
│  │  - step(dt)                                          │   │
│  │  - get_bodies() → Vec<BodyData>                      │   │
│  │  - reset()                                           │   │
│  └──────┬──────────────────────────────┬────────────────┘   │
│         │                              │                     │
│  ┌──────▼──────────┐          ┌───────▼────────────────┐   │
│  │ ServerBackend   │          │  LocalBackend          │   │
│  │ (HTTP → Python) │          │  (physics crate 直接)   │   │
│  └──────┬──────────┘          └───────┬────────────────┘   │
│         │                              │                     │
└─────────┼──────────────────────────────┼─────────────────────┘
          │                              │
          ▼                              ▼
┌─────────────────────┐    ┌─────────────────────────────────┐
│ Python Physics      │    │  physics crate                   │
│ Server (子进程)      │    │  - PhysicsWorld                  │
│ - FastAPI + uvicorn │    │  - PhysicsScene                  │
│ - pyhub 绑定         │    │  - scene_builder (GLTF 提取)     │
│ - scene_physics.py  │    │  - add_static_trimeshes()         │
└─────────────────────┘    └─────────────────────────────────┘
```

### 模式切换

- 编辑器工具栏添加 **开发目标切换按钮**: `Server` / `Client`
- 当前模式显示在状态栏
- 切换模式时重置物理世界(销毁 + 重建)

## Implementation Steps

### 阶段 0: 基础设施 — 物理后端抽象层

**目标**: 创建 `PhysicsBackend` trait 和两个实现,统一双端接口

#### Step 0.1: 修改 editor Cargo.toml — 添加 physics 原生支持
- **文件**: `/Users/qianqians/Documents/geese/crates/editor/Cargo.toml`
- **修改**:
  ```toml
  [dependencies]
  # ... 现有依赖 ...
  physics = { path = "../physics", version = "0.1.0", features = ["scene-builder"] }
  reqwest = { version = "0.11", features = ["json", "blocking"] }
  ```

#### Step 0.2: 创建 DevelopmentTarget 枚举
- **文件**: `/Users/qianqians/Documents/geese/crates/editor/src/development_target.rs` (新建)
- **内容**:
  ```rust
  /// 编辑器开发目标。
  #[derive(Debug, Clone, Copy, PartialEq, Eq)]
  pub enum DevelopmentTarget {
      /// 服务器端开发：物理引擎通过 Python 子进程运行
      Server,
      /// 客户端开发：物理引擎直接在 Rust 中运行
      Client,
  }
  
  impl DevelopmentTarget {
      pub fn label(&self) -> &str {
          match self {
              Self::Server => "🖥 Server",
              Self::Client => "🎮 Client",
          }
      }
  
      pub fn toggle(&self) -> Self {
          match self {
              Self::Server => Self::Client,
              Self::Client => Self::Server,
          }
      }
  }
  
  impl Default for DevelopmentTarget {
      fn default() -> Self {
          Self::Client // 默认客户端模式
      }
  }
  ```

#### Step 0.3: 创建 PhysicsBackend trait
- **文件**: `/Users/qianqians/Documents/geese/crates/editor/src/physics_backend.rs` (新建)
- **内容**:
  ```rust
  //! 统一物理后端抽象。
  
  use cgmath::{Quaternion, Vector3};
  
  /// 碰撞体变换快照,用于调试渲染。
  #[derive(Debug, Clone)]
  pub struct BodySnapshot {
      pub id: String,
      pub position: [f32; 3],
      pub rotation: [f32; 4], // quaternion (x, y, z, w)
  }
  
  /// 物理后端统一接口。
  pub trait PhysicsBackend: Send {
      /// 初始化物理世界。Play 模式首次进入时调用。
      fn init(&mut self, gravity: [f32; 3]) -> Result<(), String>;
  
      /// 从 .scene.json 加载场景碰撞体。
      fn load_scene(&mut self, manifest_path: &str) -> Result<usize, String>;
  
      /// 步进物理模拟。dt 单位: 秒。
      fn step(&mut self, dt: f32) -> Result<(), String>;
  
      /// 获取所有碰撞体变换快照 (用于调试渲染)。
      fn get_bodies(&self) -> Result<Vec<BodySnapshot>, String>;
  
      /// 重置物理世界。Stop 模式时调用。
      fn reset(&mut self) -> Result<(), String>;
  
      /// 后端类型标识。
      fn backend_type(&self) -> &str;
  }
  ```

#### Step 0.4: 实现 ServerPhysicsBackend (HTTP)
- **文件**: `/Users/qianqians/Documents/geese/crates/editor/src/server_physics_backend.rs` (新建)
- **内容**: 包装之前的 `PhysicsClient` + `PhysicsServerManager`,实现 `PhysicsBackend` trait
- 复用之前设计的 HTTP 通信协议和 Python 服务器

#### Step 0.5: 实现 LocalPhysicsBackend (直接 physics crate)
- **文件**: `/Users/qianqians/Documents/geese/crates/editor/src/local_physics_backend.rs` (新建)
- **内容**:
  ```rust
  //! 本地物理后端: 直接使用 physics crate。
  
  use physics::{PhysicsWorld, PhysicsScene, BodyDesc, BodyKind, ShapeDesc};
  use physics::scene_builder::extract_gltf_trimeshes;
  use crate::physics_backend::{BodySnapshot, PhysicsBackend};
  
  pub struct LocalPhysicsBackend {
      world: Option<PhysicsWorld>,
      scene_id: Option<physics::SceneId>,
  }
  
  impl LocalPhysicsBackend {
      pub fn new() -> Self {
          Self { world: None, scene_id: None }
      }
  }
  
  impl PhysicsBackend for LocalPhysicsBackend {
      fn init(&mut self, gravity: [f32; 3]) -> Result<(), String> {
          let mut world = PhysicsWorld::new();
          let g = physics::Vec3::new(gravity[0], gravity[1], gravity[2]);
          let scene_id = world.create_scene(g);
          self.world = Some(world);
          self.scene_id = Some(scene_id);
          Ok(())
      }
  
      fn load_scene(&mut self, manifest_path: &str) -> Result<usize, String> {
          let world = self.world.as_mut().ok_or("not initialized")?;
          let scene_id = self.scene_id.ok_or("not initialized")?;
  
          // 复用 physics crate 的 scene_builder 提取几何
          let scene = world.scene_mut(scene_id).ok_or("scene not found")?;
  
          // 解析 .scene.json 清单
          let content = std::fs::read_to_string(manifest_path)
              .map_err(|e| format!("read manifest: {e}"))?;
          let manifest: scene::manifest::SceneManifest = serde_json::from_str(&content)
              .map_err(|e| format!("parse manifest: {e}"))?;
  
          let base_dir = std::path::Path::new(manifest_path)
              .parent()
              .map(|p| p.to_string_lossy().to_string())
              .unwrap_or_default();
  
          let mut count = 0;
          for model in &manifest.models {
              if !model.collision_enabled {
                  continue;
              }
              let gltf_path = format!("{}/{}", base_dir, model.path);
              let meshes = extract_gltf_trimeshes(&gltf_path)
                  .map_err(|e| format!("extract trimesh: {e}"))?;
  
              let transform = physics::Iso3::translation(
                  model.transform.translation[0],
                  model.transform.translation[1],
                  model.transform.translation[2],
              );
  
              count += scene.add_static_trimeshes(&meshes, transform, 0.5, 0.0)
                  .map_err(|e| format!("add trimeshes: {e}"))?
                  .len();
          }
  
          Ok(count)
      }
  
      fn step(&mut self, dt: f32) -> Result<(), String> {
          let world = self.world.as_mut().ok_or("not initialized")?;
          let scene_id = self.scene_id.ok_or("not initialized")?;
          let scene = world.scene_mut(scene_id).ok_or("scene not found")?;
          scene.step(dt);
          Ok(())
      }
  
      fn get_bodies(&self) -> Result<Vec<BodySnapshot>, String> {
          // 本地模式暂时返回空列表
          // TODO: 遍历场景获取所有 body 变换
          Ok(Vec::new())
      }
  
      fn reset(&mut self) -> Result<(), String> {
          self.world = None;
          self.scene_id = None;
          Ok(())
      }
  
      fn backend_type(&self) -> &str {
          "local"
      }
  }
  ```

---

### 阶段 1: Python 物理服务器 (Server 模式后端)

**目标**: 创建 Python 物理服务器,与之前方案一致

#### Step 1.1: 创建 Python 服务器模块
- **文件**: `/Users/qianqians/Documents/geese/server/engine/physics_server.py` (新建)
- **内容**: 复用之前设计,通过 FastAPI 暴露 HTTP API
- 端点: `/physics/init`, `/physics/load_scene`, `/physics/step`, `/physics/bodies`, `/physics/reset`

#### Step 1.2: Python 依赖
- 添加 `fastapi` 和 `uvicorn` 到 server 依赖

---

### 阶段 2: 进程管理 (仅 Server 模式)

#### Step 2.1: 创建 Python 进程管理器
- **文件**: `/Users/qianqians/Documents/geese/crates/editor/src/server_physics_backend.rs` (与 Step 0.4 合并)
- 自动端口探测、子进程启动/停止、健康检查

---

### 阶段 3: 编辑器集成双端切换

**目标**: 编辑器工具栏添加 Server/Client 切换,统一管理物理后端

#### Step 3.1: 修改 EditorState
- **文件**: `/Users/qianqians/Documents/geese/crates/editor/src/panels.rs`
- **修改**: 添加字段
  ```rust
  use crate::development_target::DevelopmentTarget;
  use crate::physics_backend::PhysicsBackend;
  
  pub struct EditorState {
      // ... 现有字段 ...
      pub dev_target: DevelopmentTarget,
      pub physics_backend: Option<Box<dyn PhysicsBackend>>,
      pub physics_debug_enabled: bool,
  }
  
  impl EditorState {
      pub fn new(project_path: String) -> Self {
          Self {
              // ... 现有字段 ...
              dev_target: DevelopmentTarget::default(),
              physics_backend: None,
              physics_debug_enabled: false,
          }
      }
  }
  ```

#### Step 3.2: 修改编辑器工具栏 — 添加开发目标切换按钮
- **文件**: `/Users/qianqians/Documents/geese/crates/editor/src/editor.rs`
- **修改**: 在 `show_toolbar()` 中添加
  ```rust
  // 开发目标切换
  if ui.button(self.state.dev_target.label()).clicked() {
      // 重置当前后端
      if let Some(backend) = self.state.physics_backend.as_mut() {
          let _ = backend.reset();
      }
      self.state.physics_backend = None;
      self.state.dev_target = self.state.dev_target.toggle();
  }
  
  // Physics Debug 开关
  ui.toggle_value(&mut self.state.physics_debug_enabled, "🔍 Debug");
  ```

#### Step 3.3: 修改 Play 模式 — 按开发目标创建后端
- **文件**: `/Users/qianqians/Documents/geese/crates/editor/src/play_mode.rs`
- **修改**: 在 `play()` 中
  ```rust
  // 根据开发目标创建/获取物理后端
  if state.physics_backend.is_none() {
      let backend: Box<dyn PhysicsBackend> = match state.dev_target {
          DevelopmentTarget::Client => {
              Box::new(LocalPhysicsBackend::new())
          }
          DevelopmentTarget::Server => {
              let mut mgr = ServerPhysicsBackend::new();
              if let Err(e) = mgr.start("python3", "server/engine/physics_server.py") {
                  eprintln!("server backend failed: {e}");
                  // 降级为本地模式
                  Box::new(LocalPhysicsBackend::new())
              } else {
                  Box::new(mgr)
              }
          }
      };
      let _ = backend.init([0.0, -9.81, 0.0]);
      state.physics_backend = Some(backend);
  }
  
  // 加载场景碰撞体
  if let (Some(backend), Some(scene_path)) = (
      state.physics_backend.as_mut(),
      state.current_scene_path.as_ref()
  ) {
      let _ = backend.load_scene(scene_path);
  }
  ```
  
  在 `stop()` 中:
  ```rust
  if let Some(backend) = state.physics_backend.as_mut() {
      let _ = backend.reset();
  }
  ```

#### Step 3.4: 编辑器主循环 — 物理步进
- **文件**: `/Users/qianqians/Documents/geese/crates/editor/src/editor.rs`
- **修改**: 在 `update()` 方法中
  ```rust
  // 物理步进
  if self.play_mode.is_playing && self.state.dev_target == DevelopmentTarget::Server {
      if let Some(backend) = self.state.physics_backend.as_mut() {
          let _ = backend.step(ctx.input(|i| i.unstable_dt.min(0.05)));
      }
  }
  
  // Client 模式下,物理步进频率由 physics crate 内部控制
  // 这里只负责触发步进
  if self.play_mode.is_playing && self.state.dev_target == DevelopmentTarget::Client {
      if let Some(backend) = self.state.physics_backend.as_mut() {
          let _ = backend.step(ctx.input(|i| i.unstable_dt.min(0.05)));
      }
  }
  ```

---

### 阶段 4: GLTF 导入时标记碰撞体

- **文件**: `/Users/qianqians/Documents/geese/crates/editor/src/gltf_import_dialog.rs`
- 添加碰撞体选项,生成 .scene.json 时设置 `collision_enabled: true`

---

### 阶段 5: 视口碰撞体调试渲染

#### Step 5.1: 调试渲染器
- **文件**: `/Users/qianqians/Documents/geese/crates/editor/src/physics_debug.rs` (新建)
- 从 `PhysicsBackend::get_bodies()` 获取数据,渲染线框

#### Step 5.2: 集成到视口
- **文件**: `/Users/qianqians/Documents/geese/crates/editor/src/viewport.rs`
- 在渲染管线中调用调试渲染

---

## Dependencies

```
阶段 0 (PhysicsBackend trait + 双实现)
  ├─ 依赖 physics crate (已存在,需启用 scene-builder feature)
  └─ 依赖阶段 1 (Server 后端需要 Python 服务器)

阶段 1 (Python 服务器)
  └─ 无外部依赖,可独立开发测试

阶段 2 (进程管理)
  └─ 依赖阶段 1

阶段 3 (编辑器集成双端切换)
  ├─ 依赖阶段 0 (PhysicsBackend trait)
  ├─ 依赖阶段 2 (Server 后端)
  └─ 无依赖 (Client 后端直接可用)

阶段 4 (GLTF 导入)
  └─ 独立

阶段 5 (调试渲染)
  └─ 依赖阶段 0 (PhysicsBackend trait - get_bodies)
```

## 对比: Server vs Client 模式

| 特性 | Server 模式 | Client 模式 |
|---|---|---|
| 物理引擎位置 | Python 子进程 (pyhub) | Rust 内嵌 (physics crate) |
| 碰撞体加载 | `scene_physics.py` | `scene_builder::extract_gltf_trimeshes` |
| 通信方式 | HTTP (本地回环) | 直接函数调用 |
| 适用场景 | 多人游戏服务端开发 | 单机/客户端游戏开发 |
| 启动依赖 | 需要 Python 环境 | 纯 Rust,无外部依赖 |
| 性能 | 网络延迟 ~1ms | 零额外开销 |
| 调试 | 可通过 Python 工具链调试 | 通过 Rust 工具链调试 |
| 崩溃隔离 | 子进程崩溃不影响编辑器 | 崩溃影响编辑器 |

## Risks and Mitigations

### 风险 1: 双后端维护成本
- **影响**: 两个后端实现需要保持一致的行为
- **缓解**:
  - 共享同一个 trait 定义
  - 共享测试用例
  - Client 模式优先,Server 模式复用现有 server 代码

### 风险 2: LocalBackend 缺少碰撞体查询
- **影响**: 调试渲染在 Local 模式暂时不可用
- **缓解**:
  - physics crate 的 `PhysicsScene` 已有 body 查询接口
  - 阶段 5 实现时同步补齐

### 风险 3: 模式切换时状态丢失
- **影响**: 用户切换 Server/Client 时物理状态丢失
- **缓解**:
  - 明确提示用户模式切换会重置物理世界
  - 未来可支持状态序列化/迁移

### 风险 4: physics crate scene-builder 与编辑器代码重复
- **影响**: `load_scene` 逻辑在两个后端重复
- **缓解**:
  - 提取共享的 manifest 解析逻辑到 scene crate
  - `SceneManifest` 已在 scene crate 中定义,可直接复用

## Critical Files

1. **`/Users/qianqians/Documents/geese/crates/editor/src/physics_backend.rs`** (新建)
   - `PhysicsBackend` trait 定义

2. **`/Users/qianqians/Documents/geese/crates/editor/src/local_physics_backend.rs`** (新建)
   - 客户端模式的物理后端

3. **`/Users/qianqians/Documents/geese/crates/editor/src/server_physics_backend.rs`** (新建)
   - 服务端模式的物理后端 (HTTP + 进程管理)

4. **`/Users/qianqians/Documents/geese/crates/editor/src/development_target.rs`** (新建)
   - 开发目标枚举

5. **`/Users/qianqians/Documents/geese/crates/editor/src/panels.rs`** (修改)
   - 添加 `dev_target`, `physics_backend` 字段

6. **`/Users/qianqians/Documents/geese/crates/editor/src/play_mode.rs`** (修改)
   - Play/Stop 时创建/销毁物理后端

7. **`/Users/qianqians/Documents/geese/crates/editor/Cargo.toml`** (修改)
   - 添加 physics crate 的 scene-builder feature

## Verification

### 端到端测试流程

1. **启动编辑器** - 验证默认 Client 模式
2. **导入 GLTF** - 勾选 Enable Collision
3. **Client 模式 Play** - 验证本地物理引擎运行
4. **切换 Server 模式** - 验证 Python 服务器启动
5. **Server 模式 Play** - 验证远程物理引擎运行
6. **Physics Debug** - 验证碰撞体线框显示
7. **Stop** - 验证物理世界重置
8. **关闭编辑器** - 验证 Python 进程清理

## Rejected Alternatives

### 替代方案 1: 仅 Server 模式
- **为什么拒绝**: 用户明确要求双端支持,Client 模式对单机开发更友好

### 替代方案 2: 仅 Client 模式
- **为什么拒绝**: 无法验证服务端物理逻辑,多人游戏开发需要 Server 模式

### 替代方案 3: 编辑器启动时选择模式 (命令行参数)
- **为什么拒绝**: 不够灵活,运行时切换更方便
