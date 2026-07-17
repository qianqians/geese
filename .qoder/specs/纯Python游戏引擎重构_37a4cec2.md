# 纯 Python 游戏引擎重构

## 现状分析

当前 jump_jump 是 **Rust+Python 混合架构**：

| 层 | 文件 | 行数 | 职责 |
|---|---|---|---|
| Rust cdylib | `projects/jump_jump/src/lib.rs` | 254 | wgpu 初始化、窗口、渲染管线、后处理、事件循环、输入采集 |
| Rust 场景构建 | `projects/jump_jump/src/scene_builder.rs` | 185 | 程序化网格/材质生成 |
| Python 游戏逻辑 | `projects/jump_jump/game/jump_game.py` | 379 | 蓄力跳跃、平台生成、着陆检测、计分 |
| Python 入口 | `projects/jump_jump/run.py` | 49 | 加载两个 .dll 并启动 |

**核心问题**：每个游戏项目都需要编译独立的 Rust cdylib（`lib.rs` 中 254 行几乎全是可以通用的引擎启动代码），Python 只负责游戏逻辑。这导致：
1. 新建游戏项目需要写/编译 Rust 代码
2. 引擎启动代码在 `jump_jump/src/lib.rs` 和 `crates/game_runtime/src/lib.rs` 之间大量重复
3. `py_engine` 只暴露了 `build_cube` 一种网格构建方式，场景构建能力受限

## 目标

**让 jump_jump 可以纯 Python 实现**：游戏逻辑 + 场景构建 + 材质定义 + 灯光配置全在 Python 中完成，引擎提供通用运行时，无需项目级 Rust cdylib。

## 实现方案

### 第一步：扩展 py_engine PyO3 绑定

**文件**：`crates/py_engine/src/lib.rs`

在现有 `EngineBridge` 上新增以下方法：

1. **网格基元构建**（新增静态方法）：
   - `build_plane(sx, sz, material_index)` — XZ 平面，4 顶点，复用 `scene_builder.rs` 中已有的 `create_plane_mesh` 逻辑
   - `build_sphere(radius, segments, material_index)` — UV 球体
   - `build_cylinder(radius, height, segments, material_index)` — 圆柱体

2. **材质管理**：
   - `build_material(name, r, g, b, metallic, roughness)` — 替代现有的 `make_material` 返回 PyDict 的方式，直接在引擎侧注册材质并返回索引
   - `build_material_full(name, base_color, metallic, roughness, emissive)` — 完整版

3. **灯光配置**：
   - `light_add_directional(dir, color, intensity)` — 添加方向光
   - `light_clear()` — 清空灯光
   - `light_set_ambient(r, g, b)` — 设置环境光

4. **渲染控制**：
   - `set_game_over_visual(bool)` — 控制灯光切换（当前在 Rust 硬编码 L214-218）

5. **扩展输入键映射**（`parse_key_code` 函数 L109-131）：
   - 增加：`"F"`, `"G"`, `"H"`, `"1"`-`"9"`, `"0"`, `"Space"` 等常用按键
   - 增加鼠标按钮支持：`"MouseLeft"`, `"MouseRight"`

6. **重力配置**：
   - `set_gravity(x, y, z)` — 物理世界重力

7. **摄像机增强**：
   - `camera_set_orbit(yaw, pitch, focal_x, focal_y, focal_z, distance)` — 设置轨道摄像机参数
   - `camera_set_fov(fov_degrees)` — 设置 FOV

### 第二步：创建通用 Python 游戏运行时

**新建文件**：`crates/game_runtime/src/python_runtime.rs`

将 `projects/jump_jump/src/lib.rs` 中的引擎启动逻辑抽取为通用运行时：

```
通用 Python 游戏运行时 (python_runtime.rs)
├── 接收 Python 模块名作为参数
├── wgpu + winit 初始化（通用）
├── 渲染管线 + 后处理链（通用）
├── 场景 + 物理世界（通用）
├── 事件循环（通用）
└── 每帧创建 EngineBridge → 调用 Python game.update(bridge, dt)
```

**关键设计**：
- 在 `game_runtime` crate 中新增 `python-runtime` feature flag
- 当 feature 启用时，依赖 `pyo3` 和 `py_engine`
- 暴露 `#[pyfunction] fn run_game(project_dir, game_module, window_title, width, height)` 
- 灯光、后处理等从 Python 侧通过 EngineBridge 配置，不再在 Rust 硬编码

**文件变更**：
- `crates/game_runtime/Cargo.toml` — 添加 `python-runtime` feature，引入 pyo3/py_engine 依赖
- `crates/game_runtime/src/python_runtime.rs` — 新建，通用运行时
- `crates/game_runtime/src/lib.rs` — 导出 python_runtime 模块

### 第三步：构建 geese_game Python 启动器

**新建文件**：`crates/game_runtime/run_game.py`

```python
# 纯 Python 游戏启动器
# 用法: python run_game.py <project_dir> <game_module>
# 示例: python run_game.py projects/jump_jump jump_game

import sys, os, importlib.machinery, importlib.util

# 1. 加载 py_engine.dll
# 2. 加载 geese_game.dll (通用运行时)
# 3. 将项目的 game/ 目录加入 sys.path
# 4. 调用 geese_game.run_game(project_dir, game_module, ...)
```

这将替代每个项目独立的 `run.py` + `lib.rs` 组合。

### 第四步：重构 jump_jump 为纯 Python 实现

**文件**：`projects/jump_jump/game/jump_game.py`

重构现有 379 行代码，利用新增的 py_engine API：

1. **材质定义移入 Python**（原来在 `scene_builder.rs` 的 `create_game_materials()`）：
   ```python
   materials = [
       bridge.build_material("ground", 0.35, 0.35, 0.38, 0.0, 0.8),
       bridge.build_material("player", 1.0, 0.85, 0.1, 0.0, 0.6),
       bridge.build_material("platform_blue", 0.2, 0.5, 0.9, 0.0, 0.7),
       # ...
   ]
   ```

2. **灯光配置移入 Python**（原来在 `lib.rs` L214-218 硬编码）：
   ```python
   def _setup_lighting(self, bridge, game_over):
       if game_over:
           bridge.light_set_ambient(0.18, 0.06, 0.06)
           bridge.light_add_directional([-0.3, -1.0, -0.5], [0.7, 0.3, 0.3], 0.9)
       else:
           bridge.light_set_ambient(0.12, 0.12, 0.15)
           bridge.light_add_directional([-0.3, -1.0, -0.5], [1.0, 0.95, 0.85], 1.2)
   ```

3. **保留现有游戏逻辑**：蓄力跳跃、平台生成、着陆检测、计分、连击系统全部保留

**可删除文件**：
- `projects/jump_jump/src/lib.rs` — 由通用运行时替代
- `projects/jump_jump/src/scene_builder.rs` — 材质/网格构建移入 Python
- `projects/jump_jump/Cargo.toml` — 不再需要项目级 Rust 编译
- `projects/jump_jump/run.py` — 由通用 `run_game.py` 替代

### 第五步：验证与测试

1. **编译验证**：`cd crates/game_runtime && cargo build --features python-runtime`
2. **运行验证**：`python crates/game_runtime/run_game.py projects/jump_jump jump_game`
3. **功能验证**：
   - 蓄力 → 跳跃 → 着陆检测 → 计分
   - 平台生成与清理
   - 游戏结束与重启
   - 摄像机平滑跟随
   - 灯光切换（正常/game_over）
   - 蓄力压缩动画

## 依赖关系

```
步骤1（扩展 py_engine）
    ↓
步骤2（通用运行时） ← 依赖步骤1的新 API
    ↓
步骤3（Python 启动器） ← 依赖步骤2编译出的 .dll
    ↓
步骤4（重构 jump_jump） ← 依赖步骤1-3完成
    ↓
步骤5（验证测试） ← 依赖步骤4完成
```

## 风险与缓解

| 风险 | 缓解策略 |
|------|---------|
| game_runtime 添加 pyo3 依赖可能引起编译冲突 | 使用 feature flag `python-runtime` 隔离，默认不启用 |
| 材质从 Rust 移入 Python 后，MaterialLibrary 的生命周期管理变化 | EngineBridge 持有 `*mut Scene`，材质存储在 Scene 中，生命周期不变 |
| 新增灯光 API 需要 Renderer 每帧从 Scene 读取灯光配置 | 在 Scene 上添加 `lights: Vec<Light>` 和 `ambient: [f32;3]` 字段 |
| Python 启动器 DLL 路径解析在不同环境下可能不同 | 支持环境变量 `GEESE_ENGINE_PATH` 覆盖默认路径 |
| 删除 jump_jump Cargo.toml 后，cargo 项目结构变化 | 保留 Cargo.toml 但改为纯数据配置（无 lib 目标），作为迁移过渡 |

## 被拒绝的方案

1. **仅重构 jump_game.py 而不改引擎**：虽然现有游戏逻辑已是纯 Python，但仍需编译项目级 Rust cdylib（lib.rs 254 行），无法满足"纯 Python 实现游戏"的目标。

2. **将 py_engine 改为独立可执行文件**：让 py_engine 自带 main 函数和事件循环。但这会让 py_engine 从库变为应用，破坏其作为 rlib/cdylib 双模态的设计。

3. **完全重写 Python API 层（Game/Entity/Component 框架）**：参考 Agent A 的建议创建完整的 Python 游戏框架。虽然架构更优雅，但改动量大（需要新增 500+ 行 Python 框架代码），且与现有 EngineBridge API 不兼容，会导致 jump_game.py 需要大幅重写。当前方案选择在已有 API 上增量扩展，风险更低。

4. **性能优先的批量更新模式**：参考 Agent B 的建议引入脏标记批量同步。虽然性能更优，但 jump_jump 是轻量级游戏（<50 实体），当前逐帧同步模式已足够，过早优化增加复杂度。

## 关键文件清单

| 文件 | 操作 | 说明 |
|------|------|------|
| `crates/py_engine/src/lib.rs` | 修改 | 扩展 EngineBridge API（网格/材质/灯光/输入/摄像机） |
| `crates/py_engine/Cargo.toml` | 修改 | 无需变更（已有 render/scene/physics 依赖） |
| `crates/game_runtime/src/python_runtime.rs` | 新建 | 通用 Python 游戏运行时 |
| `crates/game_runtime/Cargo.toml` | 修改 | 添加 python-runtime feature + pyo3/py_engine 依赖 |
| `crates/game_runtime/src/lib.rs` | 修改 | 导出 python_runtime 模块 |
| `crates/game_runtime/run_game.py` | 新建 | 通用 Python 启动器 |
| `projects/jump_jump/game/jump_game.py` | 修改 | 使用新 API 重构，材质/灯光移入 Python |
| `projects/jump_jump/src/lib.rs` | 删除/废弃 | 由通用运行时替代 |
| `projects/jump_jump/src/scene_builder.rs` | 删除/废弃 | 移入 Python |
