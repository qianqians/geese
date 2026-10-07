# Geese 引擎 · 开发 3D MMORPG 需要增加的内容

**本文定位**：`geese-3d-mmorpg-gap-analysis.md`（下称"原文档"）的**落实篇**。原文档回答"缺什么、为什么"，本文回答"**要加什么、加在哪、按什么顺序、怎么验收**"。

**目标场景**：3D 即时战斗 MMORPG，**手机优先（Android）**，团队 5–9 人。
**复核方法**：直接读 `client/lib/client/Cargo.toml`、`server/lib/hub/Cargo.toml`、`server/engine/*.py`、`crates/*`，并全仓 grep 依赖与调用点交叉核对。凡影响决策的结论均标注 `file:line`，可逐条复核。
**与原文档的关系**：原文档 §10 的十条判断中 **7 条成立、3 条失真**，另有 3 项它未看到 —— 见 §1。**本文不修改原文档**，勘误全部写在这里。

---

## 0. 一句话结论

**要加的东西分三类，优先级依次是：**

1. **接线（成本最低、收益最大）**：把已经写好却没接上的能力接进主链路 —— 客户端宿主与帧循环、`sync` 插值、AOI、`input`、HUD、音频、VFX、存档、服务端物理同步、编辑器视口。**约 10–15 人周**就能让 M0 里程碑跑起来。
2. **新增（真正的团队工作量）**：客户端预测/服务器和解/回滚、服务端权威移动与反外挂、战斗与数值、背包/任务/社交/交易/匹配、怪物 AI、移动端出包。
3. **改造（让既有能力可被信任）**：gate 的 56 处 `unwrap()`、Mongo 事务、Redis 连接池与越界、队列无界、Forward+ 阴影、HUD 文本光栅化、feature 启用/删除决策、Cargo workspace 与 CI。

---

## 1. 复核结论：原文档「总结」中必须修正的 6 处

| # | 原文档说法 | 复核结果 | 对方案的影响 |
|---|---|---|---|
| 1 | §2/P0-1「`pyclient` 依赖里**没有** render / scene」 | **错误**。`client/lib/client/Cargo.toml:14-24` 已依赖 `net`/`wss`/`tcp`/`proto`/`queue`/`asset`/`camera`/`render`(instancing)/`scene`/`avatar` —— 原文档只看了外层 `client/Cargo.toml`。真正缺的是：**pyclient 不依赖 winit/wgpu，没有窗口与帧循环**；导出的 pyclass 只有 `ClientContext`/`ClientPump`（`client/lib/client/src/lib.rs:28,97`）+ `Transform`/`Scene`/`SceneObject`/`SceneNode`/`AABB`/`Plane`/`Frustum`（`client/lib/client/src/py/*`），**没有任何渲染器绑定** | 「合并两个客户端工程」降级为「**给一条宿主加窗口 + 渲染帧循环**」，工作量与风险显著下降；仍必须做，它决定后续一切 |
| 2 | P0-2/P0-12「服务端权威物理**完全不存在**」 | **部分错误**。服务端 Python 层已有：`server/engine/physics.py`(278 行，World/Body/Shape/raycast/collision)、`scene_physics.py`(368 行，解析 `.scene.json` 并从 GLTF 提三角网建 Fixed 刚体)、`physics_component.py`(100)、`physics_sync.py`(174，`PhysicsSyncManager`)，由 `app.py:136-163 build_scene_physics` + `app.py:287-296` 主循环物理 tick + `flush_after_step` 挂载；`crates/physics/proto/{client_call_hub/physics.juggle,hub_call_client/physics_sync.juggle}` 定义了 `cast_ray` 与 `sync_bodies`/`sync_contacts`。**但两处断线**：① `PhysicsSyncManager.attach()` 无生产调用（仅 `server/engine/tests/test_physics.py:63`）；② `physics_sync.py:112` 把 `conn_client_gate`（`entity.py:20`/`player.py:28` 中为 `list[str]`）当 `(gate_name, conn_id)` 二元组解包 → **推送路径永不触发** | 缺的不是世界/形状/静态碰撞，而是「**玩家胶囊的权威移动命令路径 + 校验 + 回滚**」；属接线 + 中等新增，不是从零造物理 |
| 3 | P1「战斗/技能/背包/任务全缺失」 | **方向对，需细化**。仓库**存在** MMO 协议与数据模型的 codegen 产物：`server/engine/common_svr.py`(737 行，含 `error_code`、`battle_info`/`battle_entity`/`bb`/`item`/`skill_info`/`attribute`/`task_info`)、`{battle,npc,equip,player,scene,bi_log}_svr.py`（各含 `*_module` 与 `on_use_skill`/`on_accept_task` 式回调列表）。**但它们无法导入**：这些文件 `from .engine import *`，而 `server/engine/engine/` 不存在；`save.py:7` 甚至 `from engine.engine.app import app`。仓库内也没有对应 `.juggle` 源（只有 `sample/proto/proto/**` 与 `crates/physics/proto/**`）→ 它们是**孤儿产物** | 战斗/属性/任务**逻辑**确为零，但存在可复刻的数据模型与 RPC 命名；必须先把 IDL 源补回仓库，这些文件才可能变成资产 |
| 4 | P0-9「AOI 有实现没调用者，生产走全场景广播」 | **成立**。`AoiManager`（`server/engine/aoi.py:41`）与 `AoiEntityMixin`（`:210`）只在自身与 `server/engine/tests/aoi_manager_smoke.py` 出现 | 保持 P0，纯接线 |
| 5 | P0-10「gate 有 63 处 `unwrap()`」 | **成立，数字略偏**：实测 `server/lib/gate/src` **56 处**，其中 `hub_msg_handle.rs` **41 处**（`client_msg_handle.rs` 12、`conn_manager.rs:83`、`entity_manager.rs:124`、`hub_proxy_manager.rs:50`）。且 `Queue::new()` 全站无界、`enque` 结果被 `let _ =` 丢弃 | 保持 P0 |
| 6 | P0-3/P0-4/P0-5/P0-6/P0-11（input / UI / 事件总线 / 编辑器视口 / 存档） | **逐条复核成立**：`WinitBackend::push_event` 全仓唯一出现点就是它自己的定义（`crates/input/src/winit_backend.rs:35`）；`game_ui` 输出的 `DrawList` 无消费者、全仓无字体 crate；`rendered_texture` 仅声明 + 置 `None`（`crates/editor/src/viewport.rs:316,339`）；`set_dirty` 仅 `server/engine/save.py:31` 自身 | 保持 P0 |

### 1.1 原文档未提到的 3 项（本次新增）

| # | 事实 | 证据 | 意义 |
|---|---|---|---|
| N1 | `crates/terrain`（高度图 / tile 流式 / LOD / splatting / mesh builder / gpu_renderer）与 `crates/vfs`（挂载点式虚拟路径）**存在但零依赖**；`vfs` 仅被 `asset` 的可选 `vfs-integration` feature 引用，而该 feature 全仓零启用 | `crates/terrain/src/lib.rs:1-11`（自述「真实磁盘 IO / 网络流式留待接入」）、`crates/asset/Cargo.toml:14,21` | 做 MMORPG 大地图时是「接线 + 补流式 IO」，不是从零；它同时是**包体 / 热更**方案的落点 |
| N2 | 仓库有**两套协议体系**：`crates/proto/proto/*.thrift`（生成 Rust，节点间 gate/hub/dbproxy 传输）+ `rpc/` 的 `*.juggle` 自定义 IDL（生成 Python/TS 的 client↔hub / hub↔client / hub↔hub 服务与数据类）。且 Thrift 生成器版本(0.19.0) 与依赖声明(thrift 0.17.0) 不一致 | `crates/proto/gen_proto.bat:2-6`、`crates/proto/src/gate.rs:1`、`rpc/parser/jparser.py:27-65`、`rpc/gen/{genc2h,genh2c,genh2h}.py` | 新增任何玩法协议必须**先决定放哪一套**，否则会像 `*_svr.py` 一样变成孤儿 |
| N3 | 服务端 Rust 侧**没有任何 tick / 实体进场 / 登录钩子**，主循环在 Python：`app.poll`（33ms 目标，`server/engine/app.py:270-331`），物理 tick 通过 `register_physics_tick`（`app.py:127`）注入；Rust→Python 仅 `on_*` 回调名 | `server/lib/hub/src/hub_service_manager.rs:205-866`、`server/lib/hub/src/{hub,gate}_msg_handle.rs` | 所有服务端玩法逻辑的落点是 Python `server/engine/`，Rust 侧只做泵与校验 —— 方案要顺着这个分层，而不是在 hub 里写玩法 |

---

## 2. 增量清单

**类型**：接线 / 改造 / 新增。**规模**：S ≤1 人周、M 2–4、L 5–10、XL >10（误差 ±50%，不含美术与内容）。

### A. 运行时与帧链路（关键路径，先做）

| ID | 内容 | 类型 | 落点 | 依赖 | 验收 | 规模 |
|---|---|---|---|---|---|---|
| A1 | **确立唯一客户端运行时**：以 `crates/game_runtime` 为主机（winit 事件循环 + wgpu + render + input + ui + 网络），Python 只写玩法逻辑（复用已有 `python-runtime`/`py_engine` 通道）；`pyclient` 保留为逻辑/工具用途并逐步收敛 | 改造 | `crates/game_runtime/{lib.rs,Cargo.toml}`、`client/lib/client/src/lib.rs` | — | 同一进程能连 gate、收到远端实体并渲染出来 | L |
| A2 | 建立完整帧管线：输入 → 网络轮询 → 实体增删 → 插值 → 物理/动画 → 场景渲染 → HUD → 呈现；把 `update()`（当前 3 行）扩为真实帧 | 改造 | `crates/game_runtime/src/lib.rs:331-387` | A1 | 单帧内可观测到五个阶段都有产出 | L |
| A3 | 光照由硬编码改为场景驱动（平行光 + 环境光来自场景/配置） | 改造 | `crates/game_runtime/src/lib.rs:345-349` | A2 | 改场景光照配置，画面随之变化 | S |
| A4 | 资源加载统一到 `AssetDatabase`/loader + VFS 挂载点；手机分包/热更所需的虚拟路径先接上 | 接线 | `crates/asset`、`crates/vfs`（启用 `vfs-integration`）、`crates/scene/src/loader.rs:61` | A1 | 同一份资源凭虚拟路径在桌面/Android 均可加载 | M |
| A5 | 客户端性能与内存预算：帧率统计（编辑器 FPS 硬编码 `--`，`crates/editor/src/viewport.rs:999`）、DrawCall/内存打点 | 新增+改造 | `render::profiler`（需先启用 `profiling`）、`game_runtime` | A2 | 有可读的帧耗时与显存/内存统计 | M |

### B. 网络同步与服务端权威

| ID | 内容 | 类型 | 落点 | 依赖 | 验收 | 规模 |
|---|---|---|---|---|---|---|
| B1 | 把 `sync` 接进客户端：用 `SnapshotBuffer::sample`/`interpolate`（`crates/sync/src/lib.rs:70,109`）驱动远端实体位姿 | 接线 | `crates/game_runtime`（新增 `sync` 依赖）、`crates/sync` | A2 | 远端实体移动平滑无瞬移 | M |
| B2 | 快照结构补齐：现 `Snapshot` **只有位置**（`crates/sync/src/lib.rs:18-33`），需加朝向/速度/动画态/血量等 | 改造 | `crates/sync` | B1 | 转向与动画过渡同样插值 | M |
| B3 | 广播频率 / 优先级 / 压缩与带宽预算（手机 4G 目标） | 新增 | `crates/{net,sync}`、`server/engine/group.py` | B1,B2 | 100 人同屏下行带宽达标（自定基线） | M |
| B4 | **AOI 真接线**：服务端实例化 `AoiManager`（`server/engine/aoi.py:41`）替掉 `group.py:18-23` 的全场景广播；客户端消费 Enter/Leave 做实体增删；每格加实体数上限（当前 `HashSet` 无界） | 接线+改造 | `server/engine/{app.py,group.py,aoi.py}`、`crates/aoi`、客户端 | A2,B1 | 500 人场景远端实体创建数从 O(N²) 降为半径内 | L |
| B5 | **客户端预测 + 输入回放 + 服务器和解 + 回滚**（`sync` 现在完全没有：无 acked-input、无状态回卷） | 新增 | `crates/sync`、客户端移动层 | B1,B4 | 200ms 延迟下手感可接受、位置收敛无漂移 | XL |
| B6 | 命中盒历史 / 战斗延迟补偿（服务端按时间回溯判定） | 新增 | 服务端玩法层（Python）+ 客户端表现 | B5,C3 | 高延迟下击中判定与视觉一致 | L |
| B7 | 断线重连、序列号、at-least-once、退避（传输层现状无） | 新增 | `crates/{tcp,wss,net}`、`client` | A1 | 断网 30s 重连不丢关键消息 | L |

### C. 服务端玩法骨架（真正的团队工作量）

| ID | 内容 | 类型 | 落点 | 依赖 | 验收 | 规模 |
|---|---|---|---|---|---|---|
| C1 | 物理接线修断：让 `PhysicsSyncManager.attach()` 有生产调用、修 `physics_sync.py:112` 的 `conn_client_gate` 解包 bug，使「物理 → 实体 → 客户端推送」真正生效 | 接线+改造 | `server/engine/{app.py,physics_sync.py,entity.py,player.py}` | — | 刚体变换能出现在客户端 | M |
| C2 | **权威移动**：玩家胶囊由服务端物理推进（`crates/physics` 已在 `server/Cargo.toml:28` 以 `pyo3+scene-builder` 启用），客户端只发输入意图；速度/加速度上限、瞬移与穿墙检测、必要时回滚 | 新增 | 服务端 Python 玩法层（复用 `gameplay_physics::CapsuleController`，`crates/gameplay_physics/src/capsule_controller.rs:187,192`；hub 已声明该依赖 `server/lib/hub/Cargo.toml:35` 但 Rust 侧零使用） | C1,B5 | 改客户端坐标/瞬移会被服务端拒绝并纠正 | L |
| C3 | 战斗框架：技能表/配置、CD、伤害结算、属性管线（基础→加成→最终）、buff/debuff、效果管道、目标选择与仇恨 | 新增 | `server/engine`（可复刻 `common_svr.py` 的 `battle_info`/`attribute`/`skill_info` 结构）、新 `*.juggle` IDL | C2,B6 | PvE 打怪全链路可跑、数值可配 | XL |
| C4 | 背包/道具/装备/掉落：容器模型、堆叠、绑定、掉落表 | 新增 | `server/engine` + Mongo | C3,E3 | 拾取/穿戴/卸下双向一致、可回滚 | L |
| C5 | 任务/成就/活动：触发器-条件-进度模型（须先打通事件链路：`Scene::event_bus` 恒 `None`、`add_event_component` 零调用者） | 新增 | `server/engine` + `crates/{scene,event}` | E6 | 「进区域 → 接任务 → 达成 → 发奖」可跑 | L |
| C6 | 怪物 AI / 刷怪：行为树（`external/behavior3py` 已在仓库）+ **navmesh 寻路**（`crates/scene/src/scene.rs:74,641-686` 已能从 NavMesh 组件构建 `Scene::navmesh`，`crates/scene` 默认开 `navmesh` feature） | 新增 | `server/engine` + `crates/navmesh` | C2 | NPC 能绕障追击/回位，服务端权威 | L |
| C7 | 社交：好友/组队/公会/权限；聊天频道（世界/地图/队伍/私聊）。现有 `group` 只是同场景广播 | 新增 | `server/engine` + 新 IDL | C3,E3 | 组队同屏可见、频道收发正常 | XL |
| C8 | 交易/邮件/拍卖：事务性或幂等写、防刷、审计 | 新增 | `server/engine` + `crates/mongo` | E3,E4 | 双人交易原子完成，掉线可恢复 | L |
| C9 | 匹配/排队/副本场景分配 | 新增 | 新服务或 hub 扩展 | C3 | 5 人可匹配进同一副本实例 | L |
| C10 | 排行榜/统计（可复用 `server/engine/rank.py` 的 Redis zset 实现） | 接线 | `server/engine/rank.py` | E4 | 榜单实时正确 | S |

### D. 客户端表现（手机优先）

| ID | 内容 | 类型 | 落点 | 依赖 | 验收 | 规模 |
|---|---|---|---|---|---|---|
| D1 | 输入统一：winit（含 Android）事件 → `input` crate（填上 `WinitBackend::push_event` 唯一调用点 `crates/input/src/winit_backend.rs:35`），产出 UI 命中与战斗输入 | 接线 | `crates/game_runtime/{lib.rs,python_runtime.rs}`、`crates/input` | A2 | 键盘/鼠标/触摸事件都进同一抽象 | M |
| D2 | 虚拟摇杆 + 技能按钮 + 可点击 HUD 区域（全仓 `joystick`/`virtual_stick` 零命中） | 新增 | `crates/{input,ui}` | D1,D3 | 手机可移动 + 释放技能 | M |
| D3 | **HUD 渲染通道**：写 `DrawList`/`DrawCmd` 的真实消费者（`game_ui` 自身不依赖 egui，但 `crates/ui` 因 `UiContext` 依赖 egui 0.29，需拆 crate 或 feature-gate 以免客户端被迫拉 egui）；血条/目标框/技能栏 | 新增 | `crates/render`（新 `ui_pass`，可复用并修 `sprite`：`sprite.wgsl:28-29` 要 `@group(1)` 而 `sprite.rs:310-314` 只建了 camera group，且全仓无调用点）、`crates/ui` | A2 | 血条随血量变化、按钮可点 | L |
| D4 | **字体光栅化**：`.ttf/.otf` → 字形图集（当前全仓零字体依赖，`Label::draw` 只推一条 `DrawCmd::Text`，永不光栅化） | 新增 | `crates/ui`（或新 `crates/text`） | D3 | 中英文正常显示、可缩放 | L |
| D5 | 音频接线：`crates/audio` 零依赖、`AudioSystem::try_with_rodio` 零调用者；ITD 算完即弃、左右增益被求平均（单声道）、无多普勒 | 接线+改造 | `crates/audio`、`game_runtime` | A2 | 位置音可辨方向 | M |
| D6 | VFX 粒子接线（`crates/vfx` 零依赖；另需补 `end_color`/`end_size` 插值、`BillboardKind` 三种语义） | 接线+改造 | `crates/vfx`、`game_runtime` | A2 | 技能特效可播、参数生效 | M |
| D7 | 阴影修复（Forward+ 路径）：`enable_shadows`/`update_shadows` 零调用点、公共门面 `WgpuSceneRenderer` 无阴影接口、29 个 `.wgsl` 无一采样阴影贴图 → 加采样 + 让接口可达 | 改造 | `crates/render/{forward_plus.rs,wgpu_renderer.rs,pipeline.rs,shaders/*}` | A2 | 角色/建筑有投影 | L |
| D8 | 动画补强：CCD `weight` 的「迭代后整体 slerp 回原值」（`crates/avatar/src/ik.rs:227-236`）、retarget 无坐标轴/rest-pose 补偿（`crates/avatar/src/retarget.rs:127-130`） | 改造 | `crates/avatar` | — | 非等价骨架重定向不穿帮 | M |
| D9 | 战斗手感：锁定目标、摇杆转向、技能前摇/后摇与动画事件对齐（`client/engine/animation_events.py` 已有钩子） | 新增 | `game_runtime` + 玩法层 | C3,D1 | 手感评审通过 | M |

### E. 数据与可靠性

| ID | 内容 | 类型 | 落点 | 依赖 | 验收 | 规模 |
|---|---|---|---|---|---|---|
| E1 | **存档链路**：`save` 是抽象基类无子类（`server/engine/save.py:19,70`）、`set_dirty` 零调用者、落盘在 `while` 循环**之后**（`app.py:332`，仅 SIGTERM 才写）→ 写子类 + 脏标记 + 周期/下线落盘 | 新增+改造 | `server/engine/{save.py,app.py,player.py,entity.py}` | C1 | `kill -9` 后重启，进度丢失 ≤ 一个周期 | M |
| E2 | `context.py:24-28` 缺 `return`，导致 `save_time_interval()`/`migrate_time_interval()` 返回 `None` 并被喂给 `threading.Timer`（`player.py:36`、`entity.py:27`、`save.py:35`） | 改造 | `server/engine/context.py` | — | 定时器间隔为配置值 | S |
| E3 | Mongo 硬化：可失败方法返回 `Result` 而非 `bool`/`-1`（`crates/mongo/src/lib.rs`），加事务/幂等写/重试/写关注 —— 经济系统必须安全 | 改造 | `crates/mongo`、`server/engine/dbproxy.py` | — | DB 故障可区分并重试，不产生双花 | M |
| E4 | Redis 硬化：`redis_service.rs:110` 越界 panic（只检查 `len()<=0` 却取 `vec_data[1]`）、`:204..305` 在 async fn 内 `blocking_lock()`、`Arc<Mutex<Connection>>` 无连接池把 MQ 串行化 | 改造 | `crates/redis_service` | — | 压测下无 panic、吞吐达标 | M |
| E5 | 队列有界：全站 `Queue::new()` 无界 + `enque` 错误被丢弃 → 每连接上限与背压 | 改造 | `crates/queue`、`server/lib/{gate,hub,dbproxy}` | — | 恶意刷消息不导致内存无限增长 | M |
| E6 | 事件链路打通：`Scene::event_bus` 恒 `None`（`crates/scene/src/scene.rs:96,159`）、`add_event_component`/`drain_triggered_events` 零调用者、触发器条件未实现（`scene.rs:944-946` TODO） | 接线+改造 | `crates/scene`、`crates/event` | — | 触发器按条件触发并驱动玩法回调 | M |
| E7 | `crates/net/src/lib.rs:93` 每消息 `buf.drain(0..packet_end)` → 一次读多包时 O(n²) 搬移 | 改造 | `crates/net` | — | 万包/秒吞吐测试通过 | S |
| E8 | 实体迁移触发改为负载驱动（当前 20% 骰子 + 非空闲） | 改造 | `server/engine/{entity.py,app.py}` | — | 负载均衡后热点 hub 分流 | S |

### F. 安全与运维

| ID | 内容 | 类型 | 落点 | 依赖 | 验收 | 规模 |
|---|---|---|---|---|---|---|
| F1 | 登录鉴权：现在 `sdk_uuid` + 不透明 `argvs` 直接转 hub（`server/lib/gate/src/client_msg_handle.rs:138-149`），`server/engine/login.py:16-22` 是**无实现的 ABC**；补 token/签名/会话校验、IP 白名单 | 新增 | `server/lib/gate`、`server/engine/login.py` | — | 伪造 uuid 无法登入 | L |
| F2 | 版本协商接线：`client_msg_handle.rs:114-116` 直接丢弃 handshake，`crates/proto/src/version.rs`（`check_version`/`negotiated_version`，5 个测试）零调用者 | 接线 | `server/lib/gate`、`crates/proto` | — | 低版本客户端被拒并给出提示 | S |
| F3 | 限流：消息速率/字节速率/每连接队列上限（当前全无） | 新增 | `server/lib/{gate,hub}` | E5 | 单连接刷消息不影响其他玩家 | M |
| F4 | 清 `gate` 的 56 处 `unwrap()`（`hub_msg_handle.rs` 41 处在对端 Option 与 map 查找上，`client_msg_handle.rs` 12 处）→ 改 `Result`/`?` + 告警 | 改造 | `server/lib/gate/src/*` | — | 畸形帧/缺字段帧模糊测试不 panic | M |
| F5 | 传输加固：无批处理/无压缩/无 TLS（客户端 TCP 明文，服务间全明文） | 新增 | `crates/{net,tcp,wss}` | — | 客户端链路启用 WSS；服务间 TLS 有排期 | L |
| F6 | 可观测性：`health` 仅存活探针（固定内容）、无就绪探针/指标；`log` 的 Jaeger 安装失败静默（`crates/log/src/lib.rs:19-21`）；`close_handle` 只注册 SIGTERM（`crates/close_handle/src/lib.rs:11`）→ 补 SIGINT | 改造+新增 | `crates/{health,log,close_handle}` | — | Ctrl-C 优雅关闭、有就绪探针与关键指标 | M |
| F7 | 反外挂：服务端权威判定 + 异常行为日志/审计 + 客户端完整性（与 C2 配套） | 新增 | 服务端玩法层 | C2,C3 | 常见加速/瞬移/穿墙被拒并记录 | L |

### G. 内容管线与编辑器

| ID | 内容 | 类型 | 落点 | 依赖 | 验收 | 规模 |
|---|---|---|---|---|---|---|
| G1 | **编辑器视口渲染场景几何体**：`rendered_texture` 从未赋值/读取（`crates/editor/src/viewport.rs:316,339`），编辑器引用 `WgpuSceneRenderer`/`ForwardPlusPipeline` 零命中，只画网格线 + gizmo → 接 `WgpuSceneRenderer` 或让编辑器复用 `game_runtime` 渲染路径，并把离屏纹理按 egui 受管方式注册（`editor_crash.txt` 记录的 `egui_texid_Managed` 校验失败即源于此） | 改造 | `crates/editor/src/viewport.rs`、`crates/editor/Cargo.toml` | A2 | 编辑器内可见并预览模型/动画 | L |
| G2 | 编辑器内 HUD/字体预览（与 D3/D4 共用通道） | 新增 | `crates/editor` | D3,D4 | 面板内可预览血条文本 | M |
| G3 | **Android 出包链路**：现在只把 `libgeese_game.so` 复制到 `export/<项目>/android/lib/arm64-v8a/`（`crates/editor/src/build_panel.rs:457-492`），APK 组装（java 层/AndroidManifest/gradle）不在仓库 | 新增 | 新 `android/` 工程 + `build_panel.rs` | A1 | 一键产出可安装 APK | L |
| G4 | 编辑器桩位修复：Mesh Renderer 三行硬编码字面量（`inspector.rs:287-293`）、`+ Add Component` 空处理器（`inspector.rs:434`）、Physics 组件忽略面板勾选（`inspector.rs:307-313`）、`sync_transform` 未命中写入默认值（`inspector.rs:112-119`）、双击聚焦占位（`hierarchy.rs:331-334`）、ShaderGraph 拖拽连线未实现（`shader_graph_editor.rs:72-73`）、`MaterialEditorPanel`/`ShaderGraphEditorPanel` 是**死代码**从未实例化、编辑器内建球/柱/平面存 Prefab 变 1×1×1 灰立方体（`editor.rs:1164-1179,1306`）、FPS 硬编码（`viewport.rs:999`） | 改造 | `crates/editor/src/*` | G1 | 逐项通过人工验收清单 | L |
| G5 | 手机包体与资源 cooking：启用 `asset` 的 `cooking`/`hot-reload` feature（当前零启用、`AssetCache` 无引擎调用点），接纹理压缩/mesh 优化 | 新增 | `crates/asset`、构建流程 | A4 | 安装包与运行内存达标 | L |
| G6 | 大地图/地形：把 `crates/terrain` 接进渲染与场景，实现磁盘/网络流式 tile 加载 | 接线+新增 | `crates/terrain`、`crates/render`、`game_runtime` | A4 | 无缝大世界可跑、内存有界 | XL |

### H. 工程化（低成本、高杠杆）

| ID | 内容 | 类型 | 落点 | 依赖 | 验收 | 规模 |
|---|---|---|---|---|---|---|
| H1 | **建 Cargo workspace**：现 `desktop`/`server`/`client` 三个独立入口、`crates/` 无顶层 workspace → 无法一键全量编译与测试 | 改造 | 根 `Cargo.toml` | — | `cargo test --workspace` 可跑 | S |
| H2 | CI：全量 `check`/`test`（实测 server 1m55s、game_runtime 22s）+ 依赖图守卫 | 新增 | CI 配置 | H1 | 每个 PR 全绿门禁 | M |
| H3 | 清理断言与实现脱节的测试：`crates/vfx/src/gpu_renderer.rs` 的 `camera_uniform_size_is_144_bytes`（断言 192，实际 144）与 `instance_data_size_is_32_bytes`（断言 48） | 改造 | `crates/vfx` | — | 全量测试通过且断言有意义 | S |
| H4 | feature 决策：`profiling`/`ecs_bridge`/`lod`/`particles`/`hi-z-occlusion`/`hot-reload`/`cooking`/`use-shader-framework` 当前**全仓零启用**，其代码不参与任何构建（`crates/render/Cargo.toml:18` `default = []`）→ **启用**（至少 `profiling` 与手机价值高的 `lod`）或**删除**，二选一，避免误判能力 | 改造 | `crates/{render,scene,asset}/Cargo.toml` 与各启用点 | — | 每个 feature 状态明确（启用且有调用点 / 删除） | M |
| H5 | 增加「未接线检测」守卫：扫描「`Cargo.toml` 有依赖但源码零 `use`」「`pub` 类型零构造点」「feature 零启用」（原文档多数结论都靠人工 grep 得出） | 新增 | `tools/` + CI | H2 | 新增死依赖/死模块会在 CI 报出 | M |

**前置小项（一并做，均 S 级）**：修 `sprite` 绑定组缺失（`sprite.wgsl:28-29` vs `sprite.rs:310-314`）、HDR 中间纹理实际用 `surface_format` 而非 `Rgba16Float`（`crates/game_runtime/src/lib.rs:258` vs `post_pipeline.rs:96`）、`graph.rs` 空屏障函数与 `hiz.rs` 忽略输入、`profiler.collect()` 恒返回 0、`skinning.rs:15` 骨骼上限 256 与 `common.rs:10`/`forward_plus.wgsl:5` 的 32 不一致 —— 这些属于「让既有能力可被信任」。

---

## 3. 阶段与里程碑

每个里程碑给**可验收的句子**，而不是功能清单。

**M0 · 一条端到端能跑的线**（A1–A3、B1、B4、C1–C2 最小版、E1–E2、H1–H2）
验收：两个客户端经 gate/hub 连上同一场景，能互相看见、移动不瞬移；服务端拒绝超速/瞬移并纠正客户端；`kill -9` 服务端后重启，玩家进度不丢；全量 `cargo test --workspace` 可执行。
规模：**约 30–45 人周**（其中纯接线约 10–15 人周，正对应原文档「功能都在、线没接」的判断）。

**M1 · 内容可生产**（G1、G4、D3–D4 基础版、A4、H3–H5）
验收：美术在编辑器里能看见自己的模型与动画；HUD 能显示血条与文字；Android 能装得到包（G3 可稍晚）。
规模：**约 25–40 人周**。

**M2 · 战斗骨架与手感**（C3–C4、B5–B6、D1–D2、D7、D8–D9）
验收：PvE 打怪走完整链路（技能表/CD/伤害/属性/buff），命中判定服务端权威，200ms 延迟下手感可接受。
规模：**约 45–70 人周**。

**M3 · MMO 系统**（C5–C10、E3–E8、F1–F4、G5）
验收：组队/聊天/任务/交易/排行榜端到端可用；DB 故障不改档；伪造与刷包被挡。
规模：**约 55–85 人周**。

**M4 · 上线准备**（F5–F7、D5–D6、G3、G6 视范围、性能与压测）
规模：**约 30–55 人周**。

合计工程量 **≈185–295 人周**（5–9 人团队约 12–24 个月工程侧），**不含**美术/关卡/任务文本/数值（通常是工程量的 1–2 倍）。

---

## 4. 关键决策（实施前拍板；附推荐）

1. **客户端宿主路线（A1）**：推荐 **以 `crates/game_runtime` 为唯一客户端运行时**，玩法逻辑继续用 Python（`python-runtime` 通道已存在）—— 窗口/输入/渲染天然需要 Rust 主循环，而 `pyclient` 是「由 Python 驱动的 cdylib」，加窗口后事件循环会受 GIL 与线程模型牵制。备选（把渲染器 pyclass 加进 pyclient）仅在「必须保留 Python 主导帧循环」时才选。
2. **玩法协议落在哪一套（N2）**：推荐 **玩法 RPC/数据类走 `rpc/` 的 `.juggle`**（生成 Python/TS，符合 `server/engine` 分层），**节点间传输走 `crates/proto` Thrift**；并把 `server/engine` 里孤儿 codegen 的 `battle_info`/`attribute`/`task_info` 结构**复刻成新 `.juggle`**，而不是去修那些无法导入的旧文件。
3. **ECS 决策（H4）**：`ecs_bridge` 零启用使 `crates/ecs` 完全不参与构建。要么在 M1 前启用并迁移热路径，要么明确删除；不要维持现状。
4. **Android 出包方式（G3）**：在仓库内建 gradle 工程，还是接受"导出 `.so` + 外部流水线组装 APK"？前者前期多花约 1–2 人周，后者的成本会转嫁到每次发布的运维上。

---

## 5. 假设与风险

- **假设**：目标仍是手机优先（Android）+ 3D 即时战斗 MMO；单服/单场景同屏百人量级；团队 5–9 人；估算不含美术与内容生产。
- **最大风险**：M2 的**客户端预测/服务器和解（B5）**与**战斗框架（C3）**是全清单里唯一没有「可复用骨架」的两块，其余大多是接线或有蓝本。若这两块低估，整体排期会滑 1–2 个季度。
- **次大风险**：无外挂防护 + gate 可被畸形帧打崩（56 处 `unwrap()` 落在对端输入与 map 查找上）+ 无鉴权/限流，三者叠加使接入层在公网不可用 —— 因此 **F1/F3/F4 必须与 M0 同期，不能推到 M4**。
- **其它**：Mongo 无事务直接影响交易/背包正确性；Android APK 组装与 iOS 不在仓库内，是独立工作量；无 CI 时任何全量结论都只能靠人工实测。

---

## 6. 明确推迟（写下来避免反复讨论）

Deferred+ 阴影、SSAO/SSR/DoF/MotionBlur 默认关闭（手机本就不该全开，只需从「静默无效」改成「可显式开启」）、贴花/水面/体积雾/反射探针（只有数据结构、无管线）、光照贴图烘焙、iOS、跨服玩法、商城/支付。

---

## 7. 一句话总结

**原文档是对的：geese 的问题不是「缺功能」，而是「功能都在、线没接」。** 但它对客户端依赖的一次性误判，把「合并两个工程」这个最大的心理障碍放大了 —— 实际上 `pyclient` 已经链上了 `render`/`scene`，缺的只是**窗口与帧循环**；服务端也不是没有物理，而是**物理与实体的推送路径断在一行解包 bug 上**。

**所以第一件事不是加功能，而是花约 10–15 人周把 M0 那条线接出来**：客户端宿主 + 帧循环 + `sync` 插值 + AOI + 物理同步的断点 + 存档落盘。这条线一旦跑通，「哪些能力真的可用」就变成可测量的事实，后面 185–295 人周的排期才有依据。

---

*复核方法：逐文件读 `client/`、`server/engine/`、`crates/{sync,aoi,input,ui,terrain,vfs,gameplay_physics,physics}`，全仓 `Cargo.toml` 依赖图与调用点交叉核对，并对 `gate` 的 `unwrap()` 与 `server/engine` 的 RPC 模块做了实测计数。所有结论标注 file:line，可逐条复核。*
