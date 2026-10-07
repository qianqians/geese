# Geese 引擎 · 3D MMORPG 差距分析

**目标场景**：3D 即时战斗 MMORPG，**手机优先（Android）**，团队 5–9 人。
**分析对象**：`D:\Personal\lib\geese`（Rust + wgpu 渲染，PyO3 暴露 Python 脚本层）
**分析日期**：2026-10-07
**基线版本**：git `091e2a6`（最后一次提交 2026-10-05），391 次提交，工作区干净

---

## 0. 阅读须知：项目自带的差距文档已过期

`geese/.qoder/specs/引擎功能差距与路线图_c975aac1.md`（2026-07-15 写）**已严重过期**，直接照它排期会做错事。三处硬性矛盾：

| 该文档的结论 | 源码实际情况 | 证据 |
|---|---|---|
| 触屏输入 **P2**"InputEvent 无 Touch/Pointer 事件，移动端受限" | **已实现**：`TouchPoint` + `TouchStart/TouchMove/TouchEnd/TouchCancel` 完整 | `crates/input/src/lib.rs:91-125` |
| 游戏内 UI **P0**"无 HUD、血条、菜单、按钮" | **已有骨架**：`Widget` trait、`HudOverlay`、`DrawList`、`GameInput`、`layout`、`Label`，带单测 | `crates/ui/src/game_ui/`（5 文件 38KB） |
| Sprite/2D **P0**"全项目无任何 2D 渲染支持" | **已有实现**：有独立渲染管线 | `crates/render/src/sprite.rs`（20.2KB，1 条 RenderPipeline） |
| 贴花/反射探针/水面/体积雾 **P1 全缺失** | **文件存在，但只是骨架**（见 P2-1） | `render/src/decal.rs`、`reflection_probe.rs`、`water.rs`、`fog.rs` 均为 0 条管线 |

**结论：不要再基于那份文档做决策。** 本文件所有结论均来自源码与实测。

---

## 1. 实测基线（工程健康度）

| 项目 | 结果 |
|---|---|
| Rust 工具链 | 1.96.0，就绪 |
| `server` 全量类型检查 | ✅ **通过**（1m55s，仅 2 个 warning） |
| `game_runtime` 类型检查 | ✅ **通过**（22s，产出 `geese_game` 可执行文件） |
| Android 交叉编译配置 | ✅ 已配置（`.cargo/config.toml` 有 `aarch64-linux-android` / `armv7` linker） |
| 开发活跃度 | ✅ 健康：7 月 70 次提交，最近提交 2 天前 |
| 测试/CI | ⚠️ 见 P3-3 |
| 服务端产物 | `dbproxy`、`gate` 两个 bin + `pyhub` cdylib（`server/Cargo.toml`） |

**这是一次重要的正面结论**：引擎不是半成品垃圾，是一套**能编译、在持续开发、模块划分清晰**的自研引擎。问题不在代码质量，在**架构还没接成一条游戏链路**。

---

## 2. 最关键的架构结论：客户端被劈成了两个互不相干的工程

这是本次分析**最重要的发现**，它比任何单个功能缺失都更致命。

```
geese_client  (client/Cargo.toml)          game_runtime  (crates/game_runtime)
  └─ cdylib = pyclient                        └─ bin = geese_game
  依赖: physics, navmesh,                      依赖: config, render, scene,
        gameplay_physics, client/*                   physics, camera, winit
  依赖里【没有】render / scene                 依赖里【没有】net / proto /
  依赖里【没有】net / sync / aoi                     client / sync / aoi
       ↓                                            ↓
  Python 逻辑层 client/engine/ (31KB)          3D 渲染技术演示
  有网络、有实体收发、无渲染                      有渲染、无网络
```

- `client/Cargo.toml` 的依赖是 `physics` / `navmesh` / `gameplay_physics` / `client`——**没有任何渲染依赖**。
- `crates/game_runtime/Cargo.toml` 的依赖是 `config` / `render` / `scene` / `physics` / `camera` / `winit`——**没有任何网络依赖**。

**也就是说：目前不存在一个"既能联网又能渲染 3D 场景"的客户端可执行程序。** 做 3D MMORPG 必须先把这两半合并，这是所有后续工作的前提。

---

## 3. P0 — 不解决则"MMORPG"不成立

### P0-1 客户端状态同步与 AOI 完全没有接线

| 事实 | 证据 |
|---|---|
| `sync` crate **没有任何 crate 依赖它** | 全仓库 `Cargo.toml` 搜索 `sync`，仅命中自身 `name = "sync"` 与 tokio 的 `features=["sync"]` |
| 文档却把状态同步评为 **A 级**（Source Engine 插值 / SnapshotBuffer / Lagged+Extrapolated） | `引擎功能差距与路线图` A 表 |
| `geese_client` 依赖里**没有 `sync`、没有 `aoi`** | `client/Cargo.toml` |
| `aoi` 只被服务端使用 | `server/Cargo.toml:29`、`server/lib/hub/Cargo.toml:33` |
| Python 客户端引擎共 **31KB**，**零插值代码** | `client/engine/*.py`；唯一的 "interpolat" 命中是 `scene.py:25` 的一句 debug `print` |

**含义**：服务端有 AOI（决定"该让谁看见谁"），但**客户端既不做实体插值、也不消费 AOI 的 Enter/Leave 事件**。结果就是别人在你屏幕上会瞬移、闪现、抖动。即时战斗 MMO 的生命线恰恰是这一环。

**要补**：把 `sync` 接进客户端主循环；客户端消费 AOI 增量事件做实体增删；实现快照缓冲 + 插值 + 外推 + 服务器和解（reconciliation）。

### P0-2 服务端权威物理完全不存在

| 事实 | 证据 |
|---|---|
| `hub` 的 11 个源文件里 **`physics` 出现 0 次** | `server/lib/hub/src/*.rs` 全文统计 |
| `gameplay_physics`（胶囊控制器 / 角色物理 / 布娃娃）**只被客户端依赖** | `client/Cargo.toml` 依赖 `gameplay_physics`；`server/Cargo.toml` 不含它 |
| `hub` 对 `aoi` 的使用仅限一个 Python 绑定文件 | `server/lib/hub/src/aoi_py.rs`（aoi 命中 1 次） |

**含义**：移动、碰撞、击退、卡位全部由客户端说了算。**没有移动速度校验、没有瞬移检测、没有服务端碰撞判定**——这在 MMORPG 里等于没有反外挂。

**要补**：服务端加载物理场景（rapier3d 已在依赖里）+ 权威胶囊控制器 + 移动合法性校验（速度/加速度上限、穿墙检测）+ 必要时回滚客户端位置。

### P0-3 `crates/input` 事实上未被使用，触屏零生产者

| 事实 | 证据 |
|---|---|
| `game_runtime` 的 `use` 列表里**没有 `input`** | `crates/game_runtime/src/lib.rs:1-35` |
| 它直接 `use winit::event::{DeviceEvent, Event, KeyEvent, WindowEvent}` 自己处理输入 | 同上 32 行；自己维护 `keys_pressed: HashSet<winit::KeyCode>`（`lib.rs:453`） |
| 相机移动**硬编码 WASD** | `lib.rs:510` `if keys_pressed.contains(&KeyCode::KeyW) { ... move_forward ... }` |
| 事件处理**只认 4 类，其余 `_ => {}` 丢弃** | `lib.rs:462-491`：仅 `CloseRequested` / `Resized` / `KeyboardInput` / `CursorEntered`；`python_runtime.rs:316-359` 同样只处理 `KeyboardInput`，`:359` `_ => {}` |
| `input` 其实**有**触屏翻译器 | `input/src/winit_backend.rs:99-118`：`WindowEvent::Touch` → 4 种 Touch 事件（**仓库中唯一非测试的构造点**） |
| 但 `WinitBackend::push_event` **全仓库零调用者** | grep 命中只有该文件自身、`lib.rs:21` 的 re-export，以及 `#[cfg(test)]` 内（测试甚至直接改私有字段 `pending_events` 造事件，`winit_backend.rs:316-322`） |
| `input` 只被**默认关闭的 feature** 引用 | `game_runtime/Cargo.toml:37` 声明，但实际只在 `#[cfg(feature="python-runtime")]` 下用（`python_runtime.rs:22,351-356`），而 `default = []`（`Cargo.toml:19`）；`editor/Cargo.toml:29` 是**死依赖**（editor 代码 0 处 `input::`） |
| **无任何虚拟摇杆/移动端控件** | 全仓 grep `joystick` / `virtual_stick` 在 `.rs`/`.py` 中 0 命中 |
| Python 侧也拿不到触摸 | `py_engine` 只暴露 `input_key_pressed` / `input_key_released`（`py_engine/src/lib.rs:224,229`），无 mouse/touch/gamepad API |

**含义**：触屏是"**有类型、有状态机、有翻译器、但零生产者**"的完美孤岛。手机游戏最核心的**触屏 + 虚拟摇杆 + 技能按钮**既没实现也没接线路径。

**要补**：把 winit（含 Android）事件统一喂进 `input`，填上唯一的调用点；实现虚拟摇杆与可点击技能区；UI 命中测试与战斗输入分发。

### P0-4 游戏内 UI 未接线，且**文本渲染是假的**

| 事实 | 证据 |
|---|---|
| `game_runtime` 的 `Cargo.toml` **没有 `ui`** | 依赖表 |
| 唯一声明 `ui` 的是 `editor/Cargo.toml:19`，但 editor 代码 **0 处 `ui::`**（它直接用 `egui::`，见 `editor/src/panels.rs:268-313`） | 全仓核对 → **`ui` 是死依赖** |
| `game_ui` 的 widget 是真的（5 种 + 接口） | `Label:163`、`Button:220`、`ProgressBar:310`、`Image:376`、`Panel:436`；`trait Widget:138`；`Button::update:265-279` 实现了 hover + 按下并在内部释放才算 click 的完整状态机（带测试 `:519`） |
| **文本渲染是假的** | `Label::draw:209` 只往 `DrawList` 推一条 `DrawCmd::Text` 命令；**全仓库零字体/光栅化依赖**（grep `ab_glyph`/`fontdue`/`cosmic-text`/`rusttype`/`glyph_brush`/`font8x8`/`swash` 在所有 `Cargo.toml` 中 **0 命中**） |
| **没有任何消费者处理 `DrawList`/`DrawCmd`** | grep `DrawCmd`/`DrawList`/`GameUI`/`HudOverlay` 的命中**全在 `crates/ui/src/` 内部**（含测试）；`render` crate 里没有 game_ui 桥接 |

**含义**：HUD 的绘制输出只是**内存里的命令列表**，即使有 `Text` 命令也**永远不会被光栅化**。MMORPG 的界面（血条、目标框、背包、聊天、技能栏、小地图）是玩家感知的全部，现在等于零。

**要补**：把 `game_ui` 的输出接到一个真实的 2D 渲染通道（`sprite` 管线已有基础，但需先修绑定 bug，见 5.3）；引入字体光栅化（`.ttf`/`.otf` → 字形图集）；列表/拖拽等复杂控件。

### P0-5 事件总线安全但整条链路是死的

| 事实 | 证据 |
|---|---|
| 总线**本身真实且安全**：`std::sync::RwLock`，crate 内 `unsafe` 出现 **0** 次、`RefCell`/`Cell` **0** 次 | `event/src/bus.rs:30,48,96`；`EventBus:95`、`AnyChannel:37`、`flush_all:140` |
| 但 `Scene::event_bus` 永远是 `None` | `scene/src/scene.rs:96` 声明，`:159` 初始化 `None`——**全仓唯一两处出现**，`flush()` 从未被调用 |
| `Scene::add_event_component` **零调用者** | `scene.rs:910` → `has_event_components` 恒 `false` → `Scene::tick` 里的 `evaluate_event_components()`（`:978`→`:933`）在 `:935` 立即 return |
| `drain_triggered_events` 也**零调用者** | `scene.rs:902` |
| 触发器条件逻辑本身也没实现 | `scene.rs:944-946` 自带 `TODO: 接入 Python 脚本运行时后，调用 trigger 函数求值` → 当前**所有条目无条件触发** |
| `Scene::tick` 本身零外部调用者 | 全仓 `\.tick(` 只命中 vfx 测试与 server 的 tokio interval |
| 另有一处真 `unsafe`（在别的 crate） | `py_engine/src/lib.rs:113-114` 的 `unsafe impl Sync`（`EngineBridge` 裸指针）——文档 C 表提到的 unsafe 问题在这里仍存在 |

**含义**：想做"踩到触发区就开副本/播剧情/发奖励"这类 MMO 基础玩法时，事件链路**从场景到总线到条件求值全是断的**。

### P0-6 编辑器视口不渲染场景几何体（内容生产的硬阻塞）

**这是我复核过的最严重的问题**，独立验证如下：

| 检查 | 结果 |
|---|---|
| `rendered_texture` 字段 | 仅两处：声明（`crates/editor/src/viewport.rs:316`）+ 初始化为 `None`（`:339`）。**全 crate 从未赋值、从未读取** |
| 编辑器是否引用 `WgpuSceneRenderer` / `ForwardPlusPipeline` / `DeferredPlus` / `.prepare(` / `SceneRenderer` | **零命中**（整个 `crates/editor/src`） |
| 编辑器实际用到 `render` crate 的什么 | 只有 `LineVertex` / `LineRenderer`（`viewport.rs:23,1103`）、`MaterialLibrary`、`MaterialHandle`、`AlphaMode`、`ShaderGraph` |
| `crates/editor/Cargo.toml` | 依赖了 `render`（含 `instancing`）与 `scene`，**但没有 `game_runtime`** → 无法复用其场景渲染路径 |

视口实际渲染的是：**离屏 `Rgba8UnormSrgb` 目标 + `GpuGridRenderer`（网格线）+ painter 叠加层（gizmo/坐标轴/碰撞体线框）**。**没有任何代码绘制场景对象的网格。**

**后果**：编辑器里能搭层级、能改 Transform、能导入 glTF、能做动画时间轴——**但看不到模型**。对 3D 内容生产来说这是致命的：美术无法在编辑器内验证任何视觉效果。

**旁证**：仓库根目录的 `editor_crash.txt` 记录了一次真实的 wgpu 校验失败：
```
PanicException: Error in Queue::submit: Validation Error
Caused by: Texture with 'egui_texid_Managed(2)' label has been destroyed
```
这是 egui 纹理生命周期问题，与视口离屏纹理的非受管注册（`viewport.rs:1183`）相符。

**要补**：把 `WgpuSceneRenderer` 接进编辑器视口（或让编辑器依赖 `game_runtime` 复用其渲染路径），把渲染结果注册为 egui 纹理并真正使用 `rendered_texture`。

### P0-7 编辑器其他关键桩位与死代码

| 位置 | 问题 |
|---|---|
| `inspector.rs:287-293` | Mesh Renderer 区**输出三行硬编码字面量**：`"Vertices: (from GLTF)"` / `"Triangles: (from GLTF)"` / `"Material: (from GLTF)"`，且该区块确实会显示（`mesh_entities` 有填充） |
| `inspector.rs:434` | `if ui.button("+ Add Component").clicked() {}` —— **空处理器** |
| `inspector.rs:307-313` | Physics 的 Add Component 硬编码 `server_enabled: true, client_enabled: true, body_kind: Fixed`，忽略面板当前勾选状态 |
| `inspector.rs:112-119` | `sync_transform` 缓存未命中时**凭空写入默认值** `[0,0,0]/[0,0,0]/[1,1,1]`（函数本身是实现了，但未命中路径是占位） |
| `hierarchy.rs:331-334` | 双击"聚焦"是占位，注释自认，只重设选中项、不做相机聚焦 |
| `shader_graph_editor.rs:72-73` | 注释宣称"节点拖拽、连线编辑"，实际 `response` 绑定后从未使用 → **拖拽与连线未实现**，只能加节点 |
| `MaterialEditorPanel` / `ShaderGraphEditorPanel` | **死代码**：两者都是 `pub mod` 能编译，但 `Editor` 结构体没有它们的字段、editor.rs 也没 import → **运行中的编辑器从不实例化它们** |
| `editor.rs:1164-1179,1306` | 编辑器内建的球/圆柱/平面在保存 Prefab 时因 `asset_source_uuid: None` 走 fallback，**被写成 1×1×1 灰色立方体**（注释自认） |
| `viewport.rs:999` | 状态栏 FPS 是**硬编码 `--`**，从未计算 |
| `physics_debug.rs` | 模块注释称在视口绘制碰撞体线框，但该模块**没有绘制函数**，只存快照；实际绘制在 `viewport.rs:671` |
| `launcher/src/lib.rs:365-370` | "浏览..." 按钮是空 `// TODO` |

编辑器整体评价要修正：**面板/命令/撤销重做/层级/gizmo/资产浏览器/glTF 导入/动画时间轴是真实实现的**（`todo!`/`unimplemented!`/`panic!` 在 `crates/editor/src` **零命中**），比文档描述的好；但**视口不画几何体 + 关键面板是死代码**这两点使它还不能承担 3D 内容生产。

### P0-8 运行时是个技术演示，不是游戏运行时

| 子系统 | 是否接线 |
|---|---|
| render | ✅ `self.renderer.render(...)`（lib.rs:367 / 387） |
| physics | ✅ `scene.step(dt)`（lib.rs:331-333） |
| 后处理 | ✅ `post_pipeline.process(...)`（lib.rs:376，条件开关） |
| **光照** | ❌ **硬编码**：一盏平行光 + 固定环境光 `[0.05,0.05,0.08]`（lib.rs:345-349） |
| audio | ❌ 出现 0 次 |
| ui / HudOverlay | ❌ 出现 0 次 |
| net / proto / queue | ❌ 出现 0 次 |
| aoi | ❌ 出现 0 次 |
| 输入抽象 | ❌ 绕过，直连 winit |

`update()` 整个函数体只有 3 行——只步进物理：

```rust
pub fn update(&mut self, dt: f32) {
    if let Some(scene) = self.physics.scene_mut(self.physics_scene_id) {
        scene.step(dt);
    }
}
```

**含义**：不存在"场景 → 网络 → 同步 → 渲染 → UI"的完整帧。这是所有 P0 的汇总表现。

---

## 4. P0 补充 — 服务端安全、存档与可靠性

这一批是子代理深读 `server/` 与 `crates/{aoi,sync,net,tcp,wss,proto,queue,consul,redis_service,mongo,save}` 的结果，我逐条复核了关键项。

### P0-9 AOI 写了，但运行中的服务器没用它 —— 实际是 O(N²) 广播

| 事实 | 证据 |
|---|---|
| `aoi` 算法**真实且完整** | `GridAoi`（`aoi/src/lib.rs:40`）用 XZ 网格桶；可见性按**半径驱动**（`cell_radius = ceil(radius/cell_size)`，扫 `(2r+1)²` 个桶，`:70-81`），**observer 移动有处理**（`update()` 取"原可见 ∪ 所有可能看到它的 observer ∪ 新可见"再逐个 diff，`:119-150`）；Enter/Leave 由 `take_events()` 拉取（`:177`）；7 个测试 |
| **但文档说的"九宫格 3×3"是错的** | 生产配置 `cell_size=32.0`、`radius=64.0`（`server/engine/aoi.py:54`）→ `cell_radius=2` → **5×5 = 25 个桶**。代码注释（`aoi/src/lib.rs:4,39`）自称九宫格，与实际配置不符 |
| **每个桶无实体上限** | 每格 `HashSet` 无界；500 个玩家挤在同一个 32m 格子里时，`compute_visible` 与每次 `update` 都是 O(格子人数) |
| **`AoiManager` 从未被实例化** | 全仓 grep `AoiManager`/`aoi_register`/`aoi_update`/`AoiEntityMixin`，命中**只在 `server/engine/aoi.py` 自身与其测试**内。没有任何游戏代码、场景或服务调用它 |
| **实际可见性 = 全场景广播** | `group.join` 把场景里**所有**实体和玩家都发给新加入的客户端（`server/engine/group.py:18-23`）；`create_remote_player` 也把新玩家发给现存的每个客户端（`:90-108`）→ **O(N²)** |
| Enter 路径按设计就是空操作 | `server/engine/aoi.py:14-16` 自述 Enter 是 no-op，只有 Leave 驱动 `group.remove_entity/remove_player`（`:141-189`） |

**含义**：AOI crate 存在、有测试、被 Rust 侧正确导出——**但没有调用者**。它要解决的问题（O(N²) 远端实体创建）正在生产路径上真实发生。**这是接线问题，不是新代码问题。**

### P0-10 网关是最大的崩溃面，而鉴权与限流为零

| 事实 | 证据 |
|---|---|
| **`server/lib/gate` 有 63 处 `unwrap()`**，其中 `hub_msg_handle.rs` 独占 **41 处** | 且大量作用在**对端提供的 Option 与 map 查找**上：`ev.name.unwrap()`（`:152,185`）、`ev.entity_type.unwrap()`（`:224,421`）、`get_entity_mut(...).unwrap()`（`:249,289,441`）、`ev.conn_id.unwrap()`（`:737,756,847`） |
| 其余 panic 点 | `conn_manager.rs:83` `self.redis_service.clone().unwrap()`；`hub_proxy_manager.rs:50` `self.name.as_ref().unwrap()`；`dbproxy/handle.rs:62` `_data.hub_name.unwrap()` |
| **鉴权完全不存在** | 登录把 `sdk_uuid` + 不透明 `argvs` 直接转给 hub（`client_msg_handle.rs:138-149`），网关只校验字段存在；无 token/签名/会话校验、无 IP 白名单；`login_event_handle` 在本仓库是 `ABC` 抽象类**没有任何实现**（`server/engine/login.py:16-22`） |
| **版本握手被丢弃** | `client_msg_handle.rs:114-116`：`debug!("Client version handshake received; version negotiation not yet implemented.")` → `crates/proto/src/version.rs` 的 `check_version`/`negotiated_version`（有 5 个测试）**零调用者**，**无最低客户端版本强制** |
| **限流完全不存在** | 网关/hub/dbproxy 全无每连接或每账号的消息速率、字节速率上限，**每连接队列无界**（`Queue::new()` 无容量） |
| 全仓 grep 佐证 | `auth\|token\|hmac\|signature\|rate_limit\|anti_cheat\|checksum\|speed_hack` 在 `server/engine/**` 与 `server/lib/**` 中**只命中 vendored 的 pymongo/redis 库代码**，从不命中引擎代码 |

**含义**：客户端是明文的（内网链路也全明文），没有限流，而网关有 63 个可被**畸形帧或对端恶意输入**触发的 panic。这三者叠加是最尖锐的可用性风险——**一个恶意客户端即可打死作为接入层的网关进程**。

### P0-11 玩家存档链路是死的，且只在优雅退出时落盘

| 事实 | 证据 |
|---|---|
| `save` 是**抽象基类，无任何具体子类** | `server/engine/save.py:19` `class save(ABC, base_dbproxy_handle)`，`store()` 是 `@abstractmethod`（`:70-71`） |
| **`set_dirty()` 零调用者** | 全仓 grep `set_dirty`/`class ...(save)`/`def store`/`SaveDBDescribe` 命中**只在 `save.py` 自身** |
| 且基类**从不设置 `__query__`** | `save_entity` 读 `self.__query__`（`:53`），基类未赋值；装饰器把 `__db__` 写在**子类**上而 `save_entity` 读实例属性 → 即便有子类也会 `AttributeError` |
| **落盘在循环之外** | `app.py:332` 的 `save_mgr.for_each_entity(lambda entt: entt.save_entity())` 位于 `while self.__is_run__` **之后** → 只有优雅 `SIGTERM` 才存档；**SIGKILL / 崩溃 / 断电 = 全丢**（`poll()` 里没有周期性存档） |
| 底层写入路径本身是真的 | `dbproxy.updata_object`（`server/engine/dbproxy.py:39-42`）→ `context.update_object`（`context.py:178`）→ `pyhub` → `mongo.update(upsert=True)`（`db.rs:249`、`mongo/src/lib.rs:82-116`） |
| 另有独立的 `crates/save`（Rust/本地 JSON）**服务端完全不用** | 仅 `desktop/Cargo.toml:22` 依赖它；`server/` 无任何引用 |

**含义**：MMORPG 的存档是**数据资产安全**问题。现状是"写得了但没人调用，且只在优雅退出时写"。必须补：脏标记驱动的周期落盘 + 下线落盘 + 崩溃可恢复性。

### P0-12 服务端没有权威物理 / 反外挂（复核确认）

| 事实 | 证据 |
|---|---|
| `hub` 的 11 个源文件里 **`physics` 出现 0 次** | 我亲自统计 |
| 服务端**全无移动校验** | 全仓 grep 无反作弊相关命中（见 P0-10 表） |
| `gameplay_physics`（胶囊控制器/角色物理/布娃娃）**只被客户端依赖** | `client/Cargo.toml`；`server/Cargo.toml` 不含它 |
| **纯 Rust 服务端是空想** | `server/src/native_server.rs` 里是显式 stub：`// 主循环: 当前为 stub 实现` / `// TODO: tick entity/physics/service loop`（`:57,67`），且该文件**未被声明为任何 `mod`/target → 从不参与编译** |

### P0-13 其他已确认的服务端缺陷（按影响排序）

| 缺陷 | 证据 |
|---|---|
| **`context.py:24-28` 缺 `return`** → `save_time_interval()` / `migrate_time_interval()` 返回 `None`，而消费者把它喂给 `threading.Timer(...)`：`player.py:36`、`entity.py:27`（迁移定时器）、`save.py:35`（存盘定时器）→ **迁移与存盘间隔未定义** | 契约级 bug，在热路径上 |
| **`net/src/lib.rs:93` 每条消息 `self.buf.drain(0..packet_end)`** | 一次读取里若含多个小包，整体搬移剩余缓冲 → **O(n²)** |
| `redis_service.rs:110` `vec_data[1]` 只检查了 `len()<=0` | 返回 1 元素的 BRPOP 回复即**越界 panic**；另 `:124,129,142` 有 `unwrap()` |
| `redis_service.rs:204,233,258,280,305` 在 **async fn 里调 `blocking_lock()`** | 在工作线程上会 panic（潜在 panic，取决于执行器） |
| **Redis 无连接池** | 单个 `Arc<Mutex<Connection>>` 把全部 MQ 流量串行化；`brpop` 每轮 1s 超时 |
| `mongo` 每个可失败方法返回 `bool`/`-1` 而非 `Result` | 调用方**无法区分"false"与"数据库挂了"**；无事务/无 session/无重试/无写关注配置 |
| `consul` 三个方法全部把错误吞成日志 | `register/deregister/services`（`consul/src/lib.rs:32,43,54`）；注册靠 10s `Timer` 刷新来绕过健康检查抖动（`context.py:69-86`） |
| `dbproxy` 队列冗余 | `handle.rs` 既 `enque` 又**内联执行** `do_*`，而 `poll` 每个 `run` 迭代都会再跑一遍 → **重复执行** |
| `wss` 是唯一做对的传输 | 有 `HANDSHAKE_TIMEOUT=5s`、`MAX_PENDING_HANDSHAKES=256`（`wss_server.rs:24-25`），关闭路径用 watch 通道 + `Drop` 释放 + 每次退出都 `notify_close`，5 个测试覆盖对端关闭/猛断/超长/本地关闭/中止；**TCP 完全没有这些** |
| `close_handle` **只注册 SIGTERM** | `close_handle/src/lib.rs:11` → **Ctrl-C/SIGINT 不会触发优雅关闭** |
| `health` 只有存活探针 | `/health` 返回固定内容；无就绪探针、无 Prometheus 指标、无子系统健康 |
| `log` 静默降级 | Jaeger 安装失败返回 `(false, guard)`，**不打日志**（`log/src/lib.rs:19-21`） |
| 传输层缺三样 | **无批处理、无压缩、无 TLS**（除显式配置的 WSS）：`gate` 的客户端 TCP 端口明文（`gate/src/lib.rs:104-108`），**所有服务间链路明文** |

## 5. P1 — MMORPG 玩法系统：几乎为零

引擎提供了**底座**（渲染 / 物理 / 场景 / 网络 / AOI），但**游戏玩法层没有任何实现**。以下全部缺失：

| 系统 | 现状 | 说明 |
|---|---|---|
| 战斗与技能 | **缺失** | 无技能表、无 CD、无伤害结算、无 buff/debuff、无效果管线。`gameplay_physics` 只提供胶囊控制器/角色物理/布娃娃 |
| 属性与数值 | **缺失** | 无属性管线（基础→加成→最终）、无公式配置 |
| 背包 / 道具 / 装备 | **缺失** | 无容器模型、堆叠、绑定、掉落物 |
| 任务 / 成就 / 活动 | **缺失** | 无触发器-条件模型、无进度追踪（且事件链路是死的，见 P0-5） |
| 社交（好友/组队/公会） | **缺失** | 无任何组织架构与权限。现有 `group` 只是"同场景广播"，不是组队系统 |
| 聊天频道 | **缺失** | 无世界/地图/队伍/私聊 |
| 交易 / 邮件 / 拍卖行 | **缺失** | 需要事务性操作与防刷；而 `mongo` **无事务**、错误被压成 `bool`（见 P0-13） |
| 匹配 / 排队 | **缺失** | 无撮合服务 |
| 怪物 AI / 刷怪 | **缺失** | Rust 侧无行为树（文档称仅有 Python 侧 behavior3py） |
| 掉落 / 随机 | **缺失** | 无掉落表 |
| 排行榜 / 统计 | **缺失** | 无快照机制 |
| 存档 / 读档 | **缺失且链路是死的** | 见 P0-11：抽象基类无子类、`set_dirty()` 零调用者、只在优雅退出时写 |
| **客户端预测 / 服务器和解 / 回滚 / 战斗延迟补偿** | **缺失** | `sync` 只有插值 + 外推；**无输入回放、无 acked-input 缓冲、无状态回卷、无命中盒历史**（P0-1） |
| **可靠消息 / 断线续传** | **缺失** | 传输层无重连/退避，无序列号，无 at-least-once 语义 |

### 5.1 三个"看起来能用、实际未接线"的子系统

这几项在文档里会被误认为已有能力，务必分清：

| 子系统 | 实体实现 | 接线状况 |
|---|---|---|
| **音频**（`crates/audio`） | ✅ 真实：rodio 后端能播 wav/vorbis（`rodio_backend.rs:61,167`）、反距离衰减（`compute_attenuation:44`）、ILD/ITD 数学（`lib.rs:56-120`，含 `HEAD_RADIUS_M`、Woodworth 简化模型） | ❌ **零个 Cargo.toml 依赖它**，全仓无 `audio::`；`AudioSystem::try_with_rodio`（`lib.rs:382`）**0 调用者**。且 **ITD 算完就丢**（`left_delay_s`/`right_delay_s` 从未被读取），左右增益被**求平均**后塞给单个 `rodio::Sink::set_volume`（`:188-194`）→ 单声道，**不是 panning、更不是 HRTF**；`SoundPosition.velocity`/`Listener.velocity` 是**死字段**（全仓无读取）→ **无多普勒**。源码注释自认这是占位（`rodio_backend.rs:8-9`） |
| **VFX 粒子**（`crates/vfx`） | ✅ CPU 粒子模拟（`lib.rs:175`）+ 真实 wgpu 管线（`gpu_renderer.rs:47,227`，9 个测试） | ❌ **零个 Cargo.toml 依赖它**，全仓无 `vfx::`。另有内部空洞：`Emitter::end_color`/`end_size` 是**只写不读的死字段**，而 `tick()` 的文档注释宣称做"颜色尺寸插值"、实现里**没有这一步**；`BillboardKind` 的 `AxisLocked`/`VelocityAligned`/`Mesh` 三种语义**无任何实现**（只做 view-aligned） |
| **i18n**（`crates/i18n`） | ✅ 功能完整（`LocalizationManager:35`、`tr:118`、`tr_format:145`，9 个测试） | ❌ 仅 `desktop/Cargo.toml:23` 声明，而 desktop 源码 **0 处** `i18n`/`Locale` → 死依赖 |

### 5.2 真正被多目标消费的少数模块（可作为基础设施复用）

`crates/avatar` 是本轮审计中**最实、且真被接线**的模块：采样含 CubicSpline 切线插值与四元数 slerp（含对径点处理）、状态机 + BlendTree(1D) 阈值插值、`BlendMode::{Override, Additive, AdditiveScaled}` 分层混合 + 骨骼 mask、FABRIK 与 CCD 两套 IK、`retarget_clip` 骨骼名映射，共 **25 个测试**。它被 `scene` / `editor` / `client` / `py_engine` / `gameplay_physics` 真实引用，而 `scene` 又被 `game_runtime`/`editor`/`client` 使用。

**但有两处粗糙**：CCD 的 `weight` 是"迭代结束后整体 slerp 回原值"（`ik.rs:227-236`，注释自称"骨架版"），且链序判定用 `j != joint_idx` 过滤而非真正按父子序；重定向的旋转是**直接传递**（`retarget.rs:127-130`），**无坐标轴/rest-pose 补偿** → 非等价骨架会穿帮。

**判断**：对 5–9 人团队，**玩法系统才是真正的工作量所在**。引擎侧再打磨也换不来这些系统；而上面这些"写了没接"的模块，接线成本远低于重写。

---

## 6. P2 — 渲染：高级效果是"骨架"，不是"缺失"

文档说这些"完全缺失"，实际是**只有数据结构、没有接入渲染**。我用"是否创建 GPU 管线"判定：

| 模块 | 大小 | 自建 RenderPipeline | 判定 |
|---|---|---|---|
| `forward_plus.rs` | 45.9KB | ✅ | **真实** |
| `deferred_plus.rs` | 34.7KB | ✅ | 真实，但文档称缺阴影集成 |
| `sprite.rs` | 20.2KB | ✅ 1 条 | **真实**（2D 能力存在） |
| `shadow_pass.rs` | 13.1KB | ✅ 1 条 | **真实** |
| `post_pipeline.rs` | 32.6KB | ✅ 1 条 + 18 处着色器引用 | **真实**，且已被主循环调用 |
| `lines.rs` | 8.2KB | ✅ 1 条 | 真实 |
| **`decal.rs`** | 10.4KB | ❌ **0** | **骨架**（有 uniform/数据结构，无 pass） |
| **`water.rs`** | 9.1KB | ❌ **0** | **骨架**（2 处着色器引用，无管线） |
| **`fog.rs`** | 7.0KB | ❌ **0** | **骨架** |
| **`reflection_probe.rs`** | 9.2KB | ❌ **0** | **骨架** |
| **`particle.rs`** | 4.7KB | ❌ **0** | **骨架** |
| `hiz.rs` | 12.4KB | ❌ 0（3 处着色器） | 疑似 GPU 遮挡剔除，需确认是否接入 |
| `shader_library.rs` | 39.8KB | — | 104 处着色器引用，着色器资产库 |
| `profiler.rs` | 11.9KB | ❌ 0 | 只有统计，无 GPU 计时确认 |

### 6.1 更严重的问题：一半代码靠 feature gate 关闭，从未被编译

`crates/render/Cargo.toml:18` 是 `default = []`。全仓库核对启用点后的结果：

| feature | 启用情况 | 后果 |
|---|---|---|
| `instancing` | ✅ 被 `scene` / `game_runtime` / `editor` 启用 | 实际生效 |
| `render-graph` | ⚠️ 仅 `projects/jump_jump/Cargo.toml:16` 启用 | 只有那个示例走渲染图，主路径走手写通道 |
| `profiling` | ❌ **全仓库零启用** | `profiler.rs`（11.9KB）**在任何构建里都是死代码** |
| `hi-z-occlusion` | ❌ **零启用** | `hiz.rs`（12.4KB）**从不参与编译** |
| `particles` | ❌ **零启用** | `particle.rs` 从不参与编译 |
| `lod` | ❌ **零启用** | `lod.rs` 从不参与编译 |
| `use-shader-framework` | ❌ **零启用** | `shader_framework` crate 从不参与编译 |
| `ecs_bridge`（scene） | ❌ **零启用** | `Scene::ecs` **永远是 `None`**（`scene.rs:93-94`）→ 整个 `crates/ecs` 事实上死了 |
| `hot-reload` / `cooking`（asset） | ❌ **零启用** | 热重载与 cooker 从不参与编译 |

**含义**：`ECS`、`GPU Profiler`、`Hi-Z 遮挡剔除`、`热重载`、`资源 cooker` 这些在文档里被当作"已有能力"的东西，**在当前任何构建里都不存在**。这不是"缺功能"，是"写了但没接"。

### 6.2 阴影：渲染是真的，但**永远看不见**

三项独立复核（我亲自验证）：

1. `forward_plus.rs:430,447` 与 `deferred_plus.rs:433,444` 的 `enable_shadows` / `update_shadows` **全仓库零调用点**；
2. 公共门面 `WgpuSceneRenderer`（`wgpu_renderer.rs:89-135`）与 `ScenePipeline` trait（`pipeline.rs:75-118`）**都没有阴影接口** → 就算想开也调不到；
3. **29 个 `.wgsl` 里没有任何一个采样阴影贴图**。`shadow_depth.wgsl` 只负责写深度（第 1 行注释自述 "writes depth only"），而 `forward_plus.wgsl` 的 bind group 只有 camera/lights/cluster/bitmask/material/object，光照循环里**没有 shadow 项**。

结论：**阴影深度通道（`shadow_pass.rs`）是真实实现的，但它是孤岛——没有任何着色器读它。** 阴影在 Forward+ 下也不会出现。

### 6.3 其他已复核的具体缺陷

| 缺陷 | 证据 |
|---|---|
| **HDR 中间纹理其实是 SDR** | `game_runtime/src/lib.rs:258` 用 `surface_format` 创建，而管线内部按 `Rgba16Float` 处理（`post_pipeline.rs:96`）；`post_processing_enabled` 默认 `true`（`lib.rs:278`）→ Bloom/ACES 跑在 8bit sRGB 上 |
| **sprite 绑定必然校验失败** | `sprite.wgsl:28-29` 要求 `@group(1)` 的纹理+采样器，但管线布局只建了 camera group（`sprite.rs:310-314`），且全仓库无调用点 |
| **SSAO/SSR/DoF/MotionBlur 不会执行** | 着色器与通道真实存在（`post_pipeline.rs:15-18,429-621`），但由 `EffectMask` 控制（`:425`），而两个调用方只推 ACES + Bloom（`lib.rs:245-248`、`python_runtime.rs:270-273`） |
| **TAA 是死标志位** | `PostEffect::Taa` 会置位（`post.rs:129-135`）且文档宣称支持，但 `post_pipeline.rs` 从不检测该位，也不存在 `taa.wgsl` |
| **11 个 wgsl 是死资源** | `dof` / `motion_blur` / `ssao` / `ssr` / `fog` / `water` / `reflection_probe` / `lod_select` / `particle_sim` / `particle_render` / `hiz_build` 均无 `include_str!` 引用 |
| **graph.rs 的屏障是空的** | `insert_barriers_if_needed` 函数体为空（`graph.rs:264-276`），但注释宣称有屏障 |
| **Hi-Z 是坏的** | `build()` 忽略 `_src_depth`（`hiz.rs:170`），mip0 永不填充；`init_pipeline` 无调用者 |
| **profiler 永远返回 0** | `collect()` 返回 `duration_ms: 0.0`（`profiler.rs:229-237`），staging buffer 创建后即被 drop |
| **IBL 预滤波是空 mip** | `wgpu_ibl_baker.rs:452-480`；HDRI 解码直接返回 `Unsupported`（`:502-506`） |
| **骨骼上限不一致** | `skinning.rs:15` 写 256，而 `common.rs:10` 与 `forward_plus.wgsl:5` 是 **32**；5 种模式里只有 uniform palette 真正实现 |

### 6.4 确实是"真实可用"的部分（应予以认可）

- **Forward+ 管线**（多管线 + compute 剔除 + 实例化 + 阴影深度通道）、**Deferred+ 几何/光照**（仅缺阴影）
- **后处理管线本体**：6 个着色器、7 条管线，已接入主循环
- **场景图**：父子层级世界变换传播（`scene.rs:702-748`，含深度与自环保护）、**八叉树真实且被用于视锥剔除**（`octree.rs` → `scene.rs:210-226`）、Prefab 嵌套（含循环检测与 `max_depth`）、**glTF 真实导入**（`gltf` crate，含蒙皮/动画/材质）
- **动画系统**：状态机 + BlendTree(1D) + 图层混合（override/additive）+ 四元数 slerp 采样，**真实实现**（在 `crates/avatar`）
- **AssetDatabase / .meta / 依赖扫描**：真实且被编辑器与 Prefab 使用

### 6.5 对手机 3D MMO 的取舍建议

- **Deferred+ 缺阴影：可以不管**。手机 GPU 带宽吃紧，应走 Forward+。文档把它列 P1 是错的方向。
- **但 Forward+ 的阴影必须修**（5.2）——否则角色/建筑没有投影，画面会"飘"。这是观感基线，不是特效。
- **删掉死 feature 或启用它们**：`ecs_bridge` 与 `profiling` 至少应启用（ECS 是性能刚需，profiler 是优化前提）；`lod` 对手机价值高。
- **贴花（弹痕/血迹）、水面、雾**：对氛围有价值，但**不阻塞"能不能玩"**，建议推迟。
- **`hiz.rs`（GPU 遮挡剔除）**：手机同屏剔除收益直接，但当前实现是坏的（5.3），需先修再用。
- **SSAO/SSR/DoF/MB**：手机上本就不该全开；当前"默认不执行"其实符合手机需求，只是要让它**可显式开启而非静默无效**。

---

## 7. P3 — 平台、工程与内容管线

| # | 缺口 | 证据 / 说明 |
|---|---|---|
| P3-1 | **iOS 无实际接入** | 文档自认仅桌面+Android；若目标含 iOS 是一大块新工作 |
| P3-2 | **Android 只能出 `.so`，不能出 APK** | 已读 `build_panel.rs:457-492`：Android 分支仅复制 `libgeese_game.so` 到 `export/<项目>/android/lib/arm64-v8a/`，返回值里自带说明 "Use Android Studio or aapt2 to assemble APK from this directory." **APK 组装（java 层、AndroidManifest、gradle 工程）不在本仓库**。Windows 分支能真出 exe（`build_panel.rs:431-455`） |
| P3-3 | **`sync_transform()` 指控不成立** | 实测该函数**真实实现**了 transform 缓存查找，未命中时 `log::warn!`（`crates/editor/src/inspector.rs`）。编辑器共 20 个模块、`todo!`/`unimplemented!` **零命中**，比文档描述的好得多 |
| P3-4 | **无 Cargo workspace** | `desktop` / `server` / `client` 三个独立入口，`crates/` 无 workspace 顶层。后果：**无法一键全量编译与测试**，CI 也难做 |
| P3-5 | **ECS 事实上是死代码** | 不只是"性能弱"（String 实体 ID、O(n) 查询、无 System 调度）：`ecs_bridge` feature **全仓库零启用**，`Scene::ecs` **永远是 `None`**（`scene.rs:93-94`）→ 整个 `crates/ecs` 不参与任何构建 |
| P3-6 | **资源管线的高级能力未启用** | `asset` 的 `hot-reload` 与 `cooking` feature **零启用**；`AssetCache` / `AsyncAssetCache` **无任何引擎调用点**（场景加载直接走 `AssetDatabase` + glTF，见 `scene/src/loader.rs:61`）；cooker 也无 meshopt/basis-universal（设计如此） |
| P3-7 | **无 i18n** | 文档自认缺失，发行多语言受影响 |
| P3-8 | **代码质量债** | 文档自列 7 项 MUST FIX（含 2 处 `unsafe` UB、1 处分布式锁绕过、4 处功能空桩）+ 13 项警告；渲染侧实测另有 `graph.rs:264-276` 空屏障函数、`hiz.rs:170` 忽略输入、`profiler.rs:229-237` 恒返回 0 等 |

---

## 8. 与文档结论的差异汇总（避免误判）

| 主题 | 文档说法 | 实测真相 | 对排期的影响 |
|---|---|---|---|
| 触屏输入 | P2 缺失 | ✅ 已实现（仅未接线） | 降级为"接线"工作 |
| 游戏内 UI | P0 完全缺失 | ⚠️ 有骨架，未接线 | 从"从零做"变"接线+补文本" |
| 2D/Sprite | P0 完全缺失 | ⚠️ 管线真实但**绑定组缺失**（`sprite.wgsl:28-29` 要 `@group(1)`，布局只建 camera group），且无调用点 | 从"缺失"变"**修 bug**" |
| 贴花/水面/雾/反射探针 | P1 完全缺失 | ⚠️ 只有数据结构，无管线，着色器文件无人引用 | 从"缺失"变"接入渲染" |
| 状态同步 | **A 级可用** | ❌ **无人依赖，客户端未接** | **必须升级为 P0** |
| Deferred+ 阴影 | P1 优先修 | 手机应走 Forward+ | **可降级/搁置** |
| **Forward+ 阴影** | 文档视为 A 级已完备 | ❌ **渲染真实但无着色器采样、API 不可达 → 永远看不见** | **新增 P0/P1 观感基线** |
| ECS | C+（性能弱） | ❌ feature 零启用，**完全不参与构建** | 从"优化"变"**先启用**" |
| GPU Profiler | A 表未单列 | ❌ `profiling` 零启用 + `collect()` 恒返回 0 | 优化前提，需先修 |
| 资源热重载/Cooker | 列为计划 | ❌ feature 零启用，`AssetCache` 无调用点 | 需先接线 |
| 服务端三节点 | A- 可用 | ✅ 编译通过（hub 149KB / gate 98KB / dbproxy 33KB） | 认可 |
| AOI | A 级可用 | ⚠️ **算法真实（半径驱动、observer 移动有处理）但 `AoiManager` 从未实例化**；生产用 `group` 全场景广播 → **O(N²)** | 从"可用"变"**必须接线**" |
| 服务端鉴权 | 未提及 | ❌ **完全不存在**；handshake 被显式丢弃、`proto/version.rs` 无调用者、限流为零 | **新增 P0** |
| 服务端稳定性 | "无 unwrap/panic 风险" | ❌ **`gate` 有 63 处 `unwrap()`**，多在**对端输入与 map 查找**上 | **新增 P0（可用性风险）** |
| 玩家存档 | 文档自认缺失 | ❌ **比"缺失"更糟**：抽象类无子类、`set_dirty()` 零调用者、**只在优雅 SIGTERM 落盘** | **新增 P0（数据安全）** |
| 实体迁移 | 未单列 | ✅ **服务端实现最好的功能**（全量迁移 + 在途消息缓存重放），但触发是 20% 骰子 + 非空闲 | 认可，但触发策略需改为负载驱动 |
| 音频 | 缺空间化 | ❌ **更底层的问题：零个 Cargo.toml 依赖它**；ITD 算完丢弃、左右增益被求平均 → 单声道 | 从"补空间化"变"**先接线**" |
| VFX 粒子 | B 级已有 | ❌ CPU 模拟 + 真实管线，但**零依赖、无 `vfx::` 引用** | 需先接线 |
| i18n | 自认缺失 | ⚠️ **其实功能完整**（含 9 个测试），只是 desktop 死依赖 | 从"要做"变"**接线**" |
| 传输层 | "三节点完整" | ⚠️ 真实但缺三样：**无批处理、无压缩、无 TLS**（服务间全明文） | 手机端带宽/安全需补 |

### 8.1 一处实测的测试失败，但**不是渲染 bug**（避免误判）

我实跑 `crates/vfx` 的测试：**8 passed / 1 failed**。

```
---- gpu_renderer::tests::camera_uniform_size_is_144_bytes ----
assertion `left == right` failed
  left: 144   right: 192
```

初看像是 GPU 缓冲区与着色器布局不匹配，但逐字核对后**结论相反**：

| | 字段 | 大小 |
|---|---|---|
| Rust `ParticleCameraUniform`（`vfx/src/gpu_renderer.rs`） | `view_projection: [[f32;4];4]` + `inverse_view_projection: [[f32;4];4]` + `camera_position: [f32;4]` | 64+64+16 = **144** |
| WGSL `CameraUniform`（`vfx/shaders/particle_billboard.wgsl:4-7`） | `mat4x4f` + `mat4x4f` + `vec4f` | **144** |

**两边完全一致**，`size_of` 驱动的缓冲区分配（`:85`）也是对的。是**测试本身过期**：函数名写 `..._is_144_bytes`、注释写 "3 x mat4x4 = 3 x 64 = 192"、断言写 `192`，而结构体只有 **2** 个 `mat4`。

**同类脏测试**：`instance_data_size_is_32_bytes`（`:285`）断言 `size_of::<ParticleInstanceData>() == 48`。

**意义**：这说明 geese 的测试套件里存在**断言与实现脱节**的情况——看到失败先核对实现再判断，不要直接当成渲染缺陷。同时 `todo!()`/`unimplemented!()` 为零，意味着**失败测试是发现这类脱节的主要信号**，值得优先跑一遍全量测试。
| iOS | 自认有限 | ❌ 无实际接入；**Android 也只能出 `.so`** | 平台工作量被低估 |
| 编辑器 | Inspector 有空桩 | ✅ `sync_transform` 已实现，`todo!` 零命中 | 编辑器比文档描述的**好** |
| 编辑器视口 | 未提及 | ❌ **不渲染任何场景网格**（`rendered_texture` 从未赋值；只画网格线+gizmo） | **新增 P0（内容生产阻塞）** |
| 全局桩位风格 | 列了"功能空桩" | ⚠️ **全仓库 `todo!()`/`unimplemented!()` 为零**；桩都是"语义性空实现"（空函数体/占位返回/无生产者的数据类型） | **grep 更难发现，需专项清查** |

---

## 9. 路线图建议（针对 5–9 人 + 手机 + 3D 即时战斗）

### 阶段 0：打通一条端到端的线（最高优先级）

**目标：一个能联网、能看见别人、能移动、**不丢档**的 3D 客户端。**

1. **合并两个客户端工程**——让 `game_runtime` 拿到 `net/proto/client`，或让 `pyclient` 拿到 `render/scene`。**这是所有工作的前提**（见第 2 节）。
2. 把 `sync` 接进客户端主循环，做实体插值 + 外推；补 `sync = { path = ... }` 依赖（当前零依赖）。
3. **服务端把 `AoiManager` 真正实例化**，客户端消费 Enter/Leave 事件做实体增删；给每格加实体数上限。**这是当前 O(N²) 的直接解药**（见 P0-9）。
4. **补存档链路**（最优先的"数据安全"项）：给 `save` 写真子类、把 `set_dirty()` 接到状态变更上、把落盘从"循环之后"改成周期 + 下线触发；顺手修 `context.py:24-28` 缺 `return` 的 bug（见 P0-11）。
5. **补服务端权威性与基本防护**（见 P0-10/P0-12）：
   - hub 挂物理场景 + 胶囊控制器 + 移动速度/瞬移校验；
   - 网关加消息速率限制 + 每连接队列上限；
   - **先清掉 `gate` 的 63 处 `unwrap()`**（尤其 `hub_msg_handle.rs` 里对端 Option 与 map 查找那些），否则上面两项都挡不住 DoS；
   - 至少给客户端接入链路启用现有 WSS（内网明文可暂缓，但要有计划）。
6. `input` 接线：winit/Android 事件 → `input` crate（填上 `WinitBackend::push_event` 唯一的调用点）→ 虚拟摇杆。
7. 光照从硬编码改为场景驱动（否则任何美术内容都没法看）。
8. 修 `net/src/lib.rs:93` 的 O(n²) 缓冲搬移；`close_handle` 补注册 SIGINT。

### 阶段 1：让内容能做出来

9. **编辑器视口接上场景渲染**（见 P0-6）——把 `WgpuSceneRenderer` 接进 `viewport.rs`，让 `rendered_texture` 真正被赋值与使用。**不做这一步，3D 内容生产无法开始。**
10. 修编辑器其他桩位：Mesh Renderer 三行占位文本、`+ Add Component` 空处理器、ShadeGraph 拖拽连线、把 `MaterialEditorPanel`/`ShaderGraphEditorPanel` 真正挂上；修球/圆柱存 Prefab 变灰立方体的 fallback（P0-7）。
11. **打通 Android 出包链路**——当前只能出 `.so`，APK 组装（java 层 + AndroidManifest + gradle）不在仓库内（P3-2）。
12. 字体/文本光栅化 + HUD 接入，做出血条与技能栏（见 P0-4）。
13. 修 Forward+ 阴影（加阴影贴图采样 + 让 `enable_shadows` 可达）与 HDR 中间纹理格式（见 6.2/6.3）。
14. **启用已写好但被 feature 关掉的模块**：至少 `profiling`（优化前提）、`ecs_bridge`（若要用 ECS）、`hot-reload`/`cooking`（若要用资产管线）；或明确删除它们以免误导（见 6.1）。

### 阶段 2：玩法骨架

15. 战斗框架（技能表、CD、伤害结算、buff 管线）；补**客户端预测 + 服务器和解**，否则即时战斗手感不成立。
16. 属性数值管线 + 配置表热重载。
17. 背包 / 任务 / 掉落；**先给 Mongo 加事务或改用幂等写**（经济/背包变更必须安全）。

### 阶段 3：MMO 系统

18. 社交（好友/组队/公会）、聊天频道（现有 `group` 不是组队系统）。
19. 交易/邮件事务化、匹配撮合服务。
20. 怪物 AI、刷怪、排行榜。
21. 音频接线（含真正的 panning/HRTF 与多普勒）、VFX 接线、i18n 接线。

### 明确推迟

- Deferred+ 阴影、SSAO/SSR/DoF/MotionBlur、贴花、水面、体积雾、反射探针、光照贴图烘焙、iOS。

---

## 10. 一句话总结

**Geese 的引擎底座是真实可用的**——Forward+ 渲染管线、rapier3d 物理、场景图（含被真实使用的八叉树剔除）、glTF 导入、状态机/BlendTree/IK/重定向、Thrift+TCP/WSS/Redis 的传输与消息泵、Consul 发现、三节点服务端、**两侧都能编译通过**、391 次提交且仍在活跃开发。

**但它现在还不是一个能开发 3D MMORPG 的引擎。** 核心问题是**大量能力"写了但没接线"**，而且断在最关键的几处：

1. **客户端被劈成两个互不相干的工程**——一个能联网（`pyclient`）、一个能渲染（`geese_game`），**不存在既联网又渲染 3D 的客户端**；
2. **`sync` 无人依赖**，客户端零插值 → 别人在你屏幕上会瞬移；且**没有预测/和解/回滚/延迟补偿**；
3. **AOI 有实现但没有调用者**，生产路径用全场景广播 → **O(N²)**；
4. **编辑器视口不渲染任何场景网格**（`rendered_texture` 从未赋值）→ 美术看不见自己的模型；
5. **玩家存档链路是死的**，且只在优雅退出时落盘 → 崩溃即丢档；
6. **服务端无权威物理、无鉴权、无限流**，而网关有 **63 处 `unwrap()`** 落在对端输入与 map 查找上 → 一个恶意客户端可打死接入层；
7. **输入（触屏/摇杆）、UI（含文本光栅化）、音频、事件总线**全部未接线；
8. **ECS、GPU Profiler、Hi-Z、热重载、cooker 等 feature 从未被启用**，这些代码不参与任何构建；
9. **阴影即使渲染了也永远看不见**（没有着色器采样阴影贴图）；
10. **玩法系统（战斗/背包/任务/社交/交易/匹配）为零**——这才是 5–9 人团队真正的工作量。

**最该做的两件事**：**① 把两个客户端合并成一条端到端能跑的链路；② 把已写好但未接线的能力接上（AOI、sync、input、UI、音频、存档）**，而不是继续加新引擎特性。在完成这两件之前，其他任何引擎特性的打磨都无法验证价值。

**一句话**：geese 的问题不是"缺功能"，而是"**功能都在，线没接**"——这既意味着补起来比从零写快，也意味着**没有任何一个玩法能端到端跑通**。

---

*分析方法：29 次工具调用逐 crate 读源码 + 全仓库 `Cargo.toml` 依赖图交叉核对 + 关键论断独立复核（阴影采样、sprite 绑定、HDR 格式、编辑器渲染、feature 启用点）+ `cargo check` 实测（`server` 与 `game_runtime` 均通过）+ git 活跃度统计。辅助使用 4 个并行子代理做 crate 级测绘，其结论中凡影响决策者均由我亲自复核。所有结论标注 file:line，可逐条复核。*
