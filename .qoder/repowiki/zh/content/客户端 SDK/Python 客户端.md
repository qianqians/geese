# Python 客户端

<cite>
**本文引用的文件**
- [client/engine/__init__.py](file://client/engine/__init__.py)
- [client/engine/app.py](file://client/engine/app.py)
- [client/engine/context.py](file://client/engine/context.py)
- [client/engine/conn_msg_handle.py](file://client/engine/conn_msg_handle.py)
- [client/engine/player.py](file://client/engine/player.py)
- [client/engine/subentity.py](file://client/engine/subentity.py)
- [client/engine/receiver.py](file://client/engine/receiver.py)
- [client/engine/callback.py](file://client/engine/callback.py)
- [client/engine/base_entity.py](file://client/engine/base_entity.py)
- [client/engine/scene.py](file://client/engine/scene.py)
- [client/engine/camera.py](file://client/engine/camera.py)
- [sample/client/py/engine/login_cli.py](file://sample/client/py/engine/login_cli.py)
- [sample/client/py/engine/get_rank_cli.py](file://sample/client/py/engine/get_rank_cli.py)
- [sample/client/py/engine/heartbeat_cli.py](file://sample/client/py/engine/heartbeat_cli.py)
- [sample/client/py/engine/common_cli.py](file://sample/client/py/engine/common_cli.py)
- [client/Cargo.toml](file://client/Cargo.toml)
- [client/src/lib.rs](file://client/src/lib.rs)
- [client/lib/client/Cargo.toml](file://client/lib/client/Cargo.toml)
- [client/lib/client/src/lib.rs](file://client/lib/client/src/lib.rs)
- [client/lib/client/src/py/mod.rs](file://client/lib/client/src/py/mod.rs)
- [client/lib/client/src/py/scene.rs](file://client/lib/client/src/py/scene.rs)
- [client/lib/client/src/py/scene_object.rs](file://client/lib/client/src/py/scene_object.rs)
- [client/lib/client/src/py/aabb.rs](file://client/lib/client/src/py/aabb.rs)
- [client/lib/client/src/py/transform.rs](file://client/lib/client/src/py/transform.rs)
- [client/lib/client/src/py/camera.rs](file://client/lib/client/src/py/camera.rs)
</cite>

## 目录
1. [简介](#简介)
2. [项目结构](#项目结构)
3. [核心组件](#核心组件)
4. [架构总览](#架构总览)
5. [详细组件分析](#详细组件分析)
6. [依赖分析](#依赖分析)
7. [性能考虑](#性能考虑)
8. [故障排查指南](#故障排查指南)
9. [结论](#结论)
10. [附录：使用示例与最佳实践](#附录使用示例与最佳实践)

## 简介
本指南面向使用 Python 客户端 SDK 的开发者，系统讲解客户端核心架构、上下文与连接管理、异步编程模型（事件循环与协程调度）、登录与重连流程、实体管理 API、消息处理机制（全局方法注册、回调绑定、自定义消息处理），以及错误处理、网络异常恢复与性能优化建议。文档同时提供可直接参考的实际示例路径，帮助快速集成到实际项目中。

**更新** 本版本将渲染相关模块（scene / 八叉树 / GLTF 资源导入 / camera）通过 pyo3 暴露到 Python 层，并将所有客户端 pyo3 注册逻辑集中到 `client/lib/client/src/lib.rs::add_to_module`，顶层 cdylib `pyclient` 仅负责声明 `#[pymodule]` 入口；**注意** 原文档将 server 侧的 `pyhub` 物理系统误归入客户端 SDK，本次已订正——physics 仅在 server 侧使用，客户端不引入。

## 项目结构
客户端 SDK 的核心位于 client/engine 目录，采用"引擎层"组织方式：app 负责应用生命周期与线程/事件循环协调；context 封装底层连接上下文；conn_msg_handle 处理来自网关/Hub 的消息分发；player/subentity/receiver 管理不同类型的实体；callback 提供 RPC 回调与超时控制；基础实体类 base_entity 统一实体标识。

```mermaid
graph TB
subgraph "引擎层"
APP["app.py<br/>应用入口/事件循环/实体管理"]
CTX["context.py<br/>连接上下文封装"]
CMH["conn_msg_handle.py<br/>消息分发处理器"]
PLR["player.py<br/>玩家实体与RPC回调"]
SUB["subentity.py<br/>子实体与通知回调"]
RCV["receiver.py<br/>接收器与通知回调"]
CBK["callback.py<br/>回调与超时"]
BASE["base_entity.py<br/>实体基类"]
end
APP --> CTX
APP --> CMH
APP --> PLR
APP --> SUB
APP --> RCV
PLR --> CBK
SUB --> CBK
RCV --> BASE
PLR --> BASE
SUB --> BASE
```

**图表来源**
- [client/engine/app.py:40-157](file://client/engine/app.py#L40-L157)
- [client/engine/context.py:4-38](file://client/engine/context.py#L4-L38)
- [client/engine/conn_msg_handle.py:6-86](file://client/engine/conn_msg_handle.py#L6-L86)
- [client/engine/player.py:9-108](file://client/engine/player.py#L9-L108)
- [client/engine/subentity.py:9-89](file://client/engine/subentity.py#L9-L89)
- [client/engine/receiver.py:7-48](file://client/engine/receiver.py#L7-L48)
- [client/engine/callback.py:5-23](file://client/engine/callback.py#L5-L23)
- [client/engine/base_entity.py:3-6](file://client/engine/base_entity.py#L3-L6)

**章节来源**
- [client/engine/__init__.py:1-8](file://client/engine/__init__.py#L1-L8)
- [client/engine/app.py:40-157](file://client/engine/app.py#L40-L157)

## 核心组件
- 应用入口与事件循环
  - app 类负责初始化上下文、连接处理器、实体管理器与事件循环，提供连接、登录、重连、请求 Hub 服务等入口。
  - 通过独立线程运行事件循环，保证消息泵与异步任务并发执行。
- 连接上下文
  - context 对底层 ClientContext 做薄封装，统一对外暴露 connect_tcp/connect_ws/login/reconnect/request_hub_service/heartbeats/call_* 等能力。
- 消息分发
  - conn_msg_handle 接收来自底层的消息，按实体类型与消息类型分派给 player/subentity/receiver 或全局方法。
- 实体管理
  - player/subentity/receiver 分别维护各自实体集合，支持创建、刷新、删除与状态更新。
- 回调与超时
  - callback 提供响应回调与错误回调注册，以及基于定时器的超时触发。

**更新** 新增渲染相关模块（scene / 八叉树 / GLTF 资源导入 / camera）的 pyo3 导出，统一封装入 `client/lib/client::add_to_module`。

**章节来源**
- [client/engine/app.py:40-157](file://client/engine/app.py#L40-L157)
- [client/engine/context.py:4-38](file://client/engine/context.py#L4-L38)
- [client/engine/conn_msg_handle.py:6-86](file://client/engine/conn_msg_handle.py#L6-L86)
- [client/engine/player.py:9-108](file://client/engine/player.py#L9-L108)
- [client/engine/subentity.py:9-89](file://client/engine/subentity.py#L9-L89)
- [client/engine/receiver.py:7-48](file://client/engine/receiver.py#L7-L48)
- [client/engine/callback.py:5-23](file://client/engine/callback.py#L5-L23)

## 架构总览
下图展示从应用启动到消息处理的全链路交互：

```mermaid
sequenceDiagram
participant App as "app.py"
participant Ctx as "context.py"
participant Pump as "ClientPump(底层)"
participant CMH as "conn_msg_handle.py"
participant Net as "网关/Hub"
App->>Ctx : 初始化上下文/连接参数
App->>Net : connect_tcp/connect_ws
Net-->>App : on_conn_id(conn_id)
App->>Ctx : login/reconnect/request_hub_service
loop 消息轮询
App->>Pump : poll_conn_msg(handle)
Pump-->>CMH : 分发消息
CMH->>App : create/update/delete/notify/rpc
App->>App : 更新实体管理器
end
```

**图表来源**
- [client/engine/app.py:60-157](file://client/engine/app.py#L60-L157)
- [client/engine/context.py:8-38](file://client/engine/context.py#L8-L38)
- [client/engine/conn_msg_handle.py:7-86](file://client/engine/conn_msg_handle.py#L7-L86)

## 详细组件分析

### app：应用构建、上下文与连接处理
- 构建流程
  - build：初始化 context、连接处理器、实体管理器、消息泵与事件循环，启动心跳定时器。
- 连接与登录
  - connect_tcp/connect_ws：发起连接，设置连接成功回调。
  - login/reconnect/request_hub_service：向 Hub 发起认证与服务请求。
- 消息泵与事件循环
  - poll_conn_msg：持续拉取底层消息并交由 conn_msg_handle 处理。
  - poll：主循环，限制每帧处理时间，避免过载。
  - run/poll_coroutine_thread：在独立线程中运行事件循环，支持 asyncio.run_coroutine_threadsafe 调度协程。
- 全局方法与事件
  - register_global_method/on_call_global：注册与处理全局方法。
  - on_kick_off/on_transfer_complete：处理被踢下线与迁移完成事件。

```mermaid
classDiagram
class app {
+build(handle)
+connect_tcp(addr, port, cb)
+connect_ws(host, cb)
+login(sdk_uuid, argvs)
+reconnect(account_id, argvs)
+request_hub_service(name, argvs)
+register_global_method(method, cb)
+register(entity_type, creator)
+create_entity(type, id, argvs)
+update_entity(type, id, argvs)
+delete_entity(id)
+run_coroutine_async(coro)
+poll()
+poll_coroutine_thread()
}
class context {
+connect_tcp(...)
+connect_ws(...)
+login(...)
+reconnect(...)
+request_hub_service(...)
+heartbeats()
+call_rpc(...)
+call_rsp(...)
+call_err(...)
+call_ntf(...)
+poll_conn_msg(handle)
}
class conn_msg_handle {
+on_conn_id(...)
+on_create_remote_entity(...)
+on_refresh_entity(...)
+on_delete_remote_entity(...)
+on_call_rpc(...)
+on_call_rsp(...)
+on_call_err(...)
+on_call_ntf(...)
+on_call_global(...)
}
app --> context : "持有"
app --> conn_msg_handle : "持有"
```

**图表来源**
- [client/engine/app.py:40-157](file://client/engine/app.py#L40-L157)
- [client/engine/context.py:4-38](file://client/engine/context.py#L4-L38)
- [client/engine/conn_msg_handle.py:6-86](file://client/engine/conn_msg_handle.py#L6-L86)

**章节来源**
- [client/engine/app.py:40-157](file://client/engine/app.py#L40-L157)

### context：连接上下文封装
- 统一封装底层连接与 RPC 能力，屏蔽平台差异。
- 提供 connect_tcp/connect_ws/login/reconnect/request_hub_service/heartbeats/call_* 等方法。

**章节来源**
- [client/engine/context.py:4-38](file://client/engine/context.py#L4-L38)

### conn_msg_handle：消息分发处理器
- 负责将底层消息映射到具体实体或全局方法：
  - on_conn_id：保存连接 ID 并回调上层。
  - on_create_remote_entity/on_refresh_entity/on_delete_remote_entity：驱动实体管理器更新。
  - on_call_rpc/on_call_rsp/on_call_err：分派到 player/subentity 的回调表。
  - on_call_ntf：分派到 player/subentity/receiver 的通知回调。
  - on_call_global：转交 app 的全局方法处理。

```mermaid
flowchart TD
Start(["收到消息"]) --> Type{"消息类型？"}
Type --> |连接ID| CID["保存conn_id并回调"]
Type --> |创建实体| Crt["调用create_entity(type,id,argvs)"]
Type --> |刷新实体| Ref["调用update_entity(type,id,argvs)"]
Type --> |删除实体| Del["调用delete_entity(id)"]
Type --> |RPC请求| Rpc["查找player/subentity并handle_hub_request"]
Type --> |RPC响应| Rsp["查找player/subentity并handle_hub_response"]
Type --> |RPC错误| Err["查找player/subentity并handle_hub_response_error"]
Type --> |通知| Ntf["查找player/subentity/receiver并handle_hub_notify"]
Type --> |全局方法| Glob["调用on_call_global(method, argvs)"]
CID --> End(["结束"])
Crt --> End
Ref --> End
Del --> End
Rpc --> End
Rsp --> End
Err --> End
Ntf --> End
Glob --> End
```

**图表来源**
- [client/engine/conn_msg_handle.py:6-86](file://client/engine/conn_msg_handle.py#L6-L86)

**章节来源**
- [client/engine/conn_msg_handle.py:6-86](file://client/engine/conn_msg_handle.py#L6-L86)

### player/subentity/receiver：实体管理与消息回调
- player
  - 维护 hub_request_callback/hub_notify_callback/hub_callback 表。
  - 支持 call_hub_request/reg_hub_callback/call_hub_response/call_hub_response_error/call_hub_notify。
  - 通过 app 上下文转发 RPC/通知消息。
- subentity
  - 与 player 类似，但不处理请求回调，仅处理通知与响应。
- receiver
  - 仅处理通知，用于被动接收 Hub 的广播或定向通知。

```mermaid
classDiagram
class base_entity {
+entity_type
+entity_id
}
class player {
+hub_request_callback
+hub_notify_callback
+hub_callback
+call_hub_request(...)
+reg_hub_callback(...)
+call_hub_response(...)
+call_hub_response_error(...)
+call_hub_notify(...)
}
class subentity {
+hub_notify_callback
+hub_callback
+call_hub_request(...)
+reg_hub_callback(...)
+call_hub_notify(...)
}
class receiver {
+hub_notify_callback
+handle_hub_notify(...)
}
base_entity <|-- player
base_entity <|-- subentity
base_entity <|-- receiver
```

**图表来源**
- [client/engine/base_entity.py:3-6](file://client/engine/base_entity.py#L3-L6)
- [client/engine/player.py:9-108](file://client/engine/player.py#L9-L108)
- [client/engine/subentity.py:9-89](file://client/engine/subentity.py#L9-L89)
- [client/engine/receiver.py:7-48](file://client/engine/receiver.py#L7-L48)

**章节来源**
- [client/engine/player.py:9-108](file://client/engine/player.py#L9-L108)
- [client/engine/subentity.py:9-89](file://client/engine/subentity.py#L9-L89)
- [client/engine/receiver.py:7-48](file://client/engine/receiver.py#L7-L48)
- [client/engine/base_entity.py:3-6](file://client/engine/base_entity.py#L3-L6)

### callback：回调与超时
- 提供 callback 注册接口与超时触发机制，便于 RPC 请求的异步处理与资源回收。

**章节来源**
- [client/engine/callback.py:5-23](file://client/engine/callback.py#L5-L23)

### 渲染场景：Python 绑定与 API
**新增** 客户端将渲染所需的 scene / 八叉树 / GLTF 资源导入 / camera 模块通过 pyo3 暴露给 Python 层。Python 业务代码可直接构造视锥体、导入 glTF、做可见性裁剪与动画推进。

#### pyo3 导出架构
所有 pyo3 类的注册逻辑集中在 `client/lib/client::add_to_module`，顶层 cdylib（`client/src/lib.rs::pyclient`）仅声明 `#[pymodule]` 入口并调用该函数；与 server 侧 `pyhub` 模式对称。

```mermaid
graph TB
PYMOD["pyclient #[pymodule]<br/>client/src/lib.rs"]
ADD["client::add_to_module<br/>client/lib/client/src/lib.rs"]
NET["网络层<br/>ClientContext / ClientPump"]
SCENE["scene::py::add_to_module<br/>Scene / SceneNode / SceneObject / AABB / Transform"]
CAM["camera::py::add_to_module<br/>Frustum / Plane"]
PYWRAP["engine.scene / engine.camera<br/>Python 薄封装"]
PYMOD --> ADD
ADD --> NET
ADD --> SCENE
ADD --> CAM
PYWRAP --> PYMOD
```

**图表来源**
- [client/src/lib.rs:1-7](file://client/src/lib.rs#L1-L7)
- [client/lib/client/src/lib.rs:118-132](file://client/lib/client/src/lib.rs#L118-L132)
- [client/lib/client/src/py/mod.rs:1-41](file://client/lib/client/src/py/mod.rs#L1-L41)
- [client/lib/client/src/py/camera.rs:46-127](file://client/lib/client/src/py/camera.rs#L46-L127)

#### 核心类与方法

##### Scene（场景容器）
- 静态构造：`Scene.import_gltf(path, max_objects=8, max_depth=6)` 导入 `.gltf` / `.glb`
- 数量查询：`node_count()` / `object_count()` / `animation_count()` / `skin_count()`
- 节点/对象访问：`get_node(idx)` / `get_object(idx)` / `root_nodes()` / `all_objects()`
- 可见性查询（基于内置八叉树）：`visible_objects(frustum)`
- 动画：`animation_index(name)` / `animation_duration(idx)` / `animation_names()` / `update_animation(clip_index, time, dt, ...)`
- 变换：`update_world_transforms()` / `rebuild_octree()` / `bounds()`
- 内部使用 `Arc<Mutex<Scene>>` 串行化访问

##### SceneObject（场景对象，值视图）
- 元数据：`entity_id` / `node` / `material_handle` / `skin_handle`
- 包围盒：`local_aabb` / `aabb` / `center`
- 矩阵：`model_matrix` / `normal_matrix` / `joint_matrices`（行主序 4x4）
- 网格数据：`vertex_count()` / `index_count()` / `positions()` / `normals()` / `uvs()` / `indices()`

##### SceneNode（场景节点，值视图）
- 拓扑：`id` / `parent` / `children` / `objects`
- 变换：`base_transform` / `local_transform` / `world_transform`

##### AABB / Transform
- `AABB.min` / `max` / `center` / `size`，`contains_point` / `intersects`
- `Transform.translation` / `rotation`（四元数 (x,y,z,w)） / `scale` / `matrix()`

##### Frustum（视锥体）
- 静态构造：`Frustum.from_view_projection(matrix)`（行主序 4x4）/ `from_view_projection_column_major(matrix)`
- 包含/相交查询：`contains_point` / `contains_sphere` / `contains_aabb` / `intersects_aabb`
- 平面访问：`planes()` 返回 6 个 `Plane`（左/右/下/上/近/远）

**章节来源**
- [client/lib/client/src/lib.rs:118-132](file://client/lib/client/src/lib.rs#L118-L132)
- [client/lib/client/src/py/scene.rs:38-211](file://client/lib/client/src/py/scene.rs#L38-L211)
- [client/lib/client/src/py/scene_object.rs:14-207](file://client/lib/client/src/py/scene_object.rs#L14-L207)
- [client/lib/client/src/py/aabb.rs:9-66](file://client/lib/client/src/py/aabb.rs#L9-L66)
- [client/lib/client/src/py/transform.rs:9-69](file://client/lib/client/src/py/transform.rs#L9-L69)
- [client/lib/client/src/py/camera.rs:55-127](file://client/lib/client/src/py/camera.rs#L55-L127)

#### 使用示例
以下示例展示渲染场景 API 的典型用法：

```python
from client.engine.scene import Scene
from client.engine.camera import Frustum

# 1. 导入 glTF 场景（构建节点树 + 八叉树）
scene = Scene.import_gltf("assets/level.glb", max_objects=8, max_depth=6)
print(scene)  # Scene(nodes=..., objects=..., animations=..., skins=...)

# 2. 视锥体可见性裁剪
vp = build_view_projection_matrix(...)        # 行主序 4x4 嵌套 list
frustum = Frustum.from_view_projection(vp)
for obj in scene.visible_objects(frustum):
    print(obj.entity_id, obj.center, obj.aabb.min, obj.aabb.max)

# 3. 推进单条动画并刷新世界矩阵 + 八叉树
clip = scene.animation_index("Run") or 0
time = 0.0
for _ in range(60):
    time, _ = scene.update_animation(clip, time=time, dt=1.0 / 60.0)

# 4. 取节点世界变换矩阵
root_ids = scene.root_nodes()
for nid in root_ids:
    node = scene.get_node(nid)
    print(node.id, node.world_transform)
```

**章节来源**
- [client/engine/scene.py:1-35](file://client/engine/scene.py#L1-L35)
- [client/engine/camera.py:1-12](file://client/engine/camera.py#L1-L12)
- [client/lib/client/src/py/scene.rs:38-211](file://client/lib/client/src/py/scene.rs#L38-L211)

## 依赖分析
- app 依赖 context、conn_msg_handle、实体管理器与事件循环。
- player/subentity/receiver 依赖 app 以访问 context 与回调注册。
- conn_msg_handle 依赖 app 的实体管理器与全局方法表。
- callback 作为轻量工具被 player/subentity 使用。
- **更新** 渲染场景模块（`engine.scene` / `engine.camera`）通过 `pyclient` cdylib 加载，其 pyo3 注册实体由 `client/lib/client::add_to_module` 统一处理，进一步依赖 `crates/scene` 与 `crates/camera` 的 `pyo3` feature。

```mermaid
graph LR
APP["app.py"] --> CTX["context.py"]
APP --> CMH["conn_msg_handle.py"]
APP --> PLRM["player_manager"]
APP --> SUBM["subentity_manager"]
APP --> RCRM["receiver_manager"]
CMH --> APP
PLR["player.py"] --> APP
SUB["subentity.py"] --> APP
RCV["receiver.py"] --> APP
PLR --> CBK["callback.py"]
SUB --> CBK
SCN["engine.scene / engine.camera"] --> PYCLIENT["pyclient cdylib"]
PYCLIENT --> ADD["client::add_to_module"]
ADD --> SCENECRATE["crates/scene::py"]
ADD --> CAMCRATE["crates/camera::py"]
```

**图表来源**
- [client/engine/app.py:40-157](file://client/engine/app.py#L40-L157)
- [client/engine/conn_msg_handle.py:6-86](file://client/engine/conn_msg_handle.py#L6-L86)
- [client/engine/player.py:9-108](file://client/engine/player.py#L9-L108)
- [client/engine/subentity.py:9-89](file://client/engine/subentity.py#L9-L89)
- [client/engine/receiver.py:7-48](file://client/engine/receiver.py#L7-L48)
- [client/engine/callback.py:5-23](file://client/engine/callback.py#L5-L23)
- [client/src/lib.rs:1-7](file://client/src/lib.rs#L1-L7)
- [client/lib/client/src/lib.rs:118-132](file://client/lib/client/src/lib.rs#L118-L132)

**章节来源**
- [client/engine/app.py:40-157](file://client/engine/app.py#L40-L157)
- [client/engine/conn_msg_handle.py:6-86](file://client/engine/conn_msg_handle.py#L6-L86)

## 性能考虑
- 主循环节流：poll 中限制单帧处理时长并进行微小休眠，避免 CPU 占用过高。
- 异步调度：通过 asyncio.run_coroutine_threadsafe 在事件循环线程安全地提交协程任务。
- 消息批处理：conn_msg_handle 内部按类型分派，减少不必要的查找成本。
- 实体管理：player/subentity/receiver 使用字典索引，O(1) 查找与更新。
- **更新** 渲染场景模块（scene / 八叉树 / GLTF / camera）采用 Rust 实现，Python API 仅做薄封装；可见性裁剪由 Rust 八叉树完成，避免在 Python 中遍历大场景。
- 建议
  - 合理设置心跳周期与网络超时，避免频繁重连。
  - 控制回调数量与生命周期，及时释放不再使用的回调。
  - 对高频通知进行去抖/合并，降低 UI 或业务层压力。
  - 视锥体每帧只构造一次后复用 `Frustum.from_view_projection`；动画推进与 `update_world_transforms` / `rebuild_octree` 按需调用，避免每帧无谓重建八叉树。

## 故障排查指南
- 连接失败
  - 检查 connect_tcp/connect_ws 参数与网络可达性；确认 on_conn_id 是否回调。
- 登录/重连异常
  - 核对 login/reconnect 的参数编码（应为二进制）；关注 Hub 返回的错误码。
- RPC 无响应
  - 确认已通过 reg_hub_callback 注册回调；检查 msg_cb_id 是否正确传递。
  - 若未收到响应，检查回调是否被提前释放或超时触发。
- 通知未到达
  - 确认已通过 reg_hub_notify_callback 注册对应方法名的通知回调。
- 被踢下线/迁移
  - on_kick_off/on_transfer_complete 会触发关闭与事件回调，请在上层做资源清理与重连策略。
- **更新** 渲染场景相关问题
  - 确认已编译并加载 `pyclient` cdylib（顶层 `client/src/lib.rs`），`engine.scene` / `engine.camera` 应能正常 import。
  - `Scene.import_gltf` 失败时检查文件路径与 glTF/GLB 合法性；启用 `RUST_LOG=info` 可看到 `crates/scene` 内部日志。
  - 视锥体输入矩阵需为行主序 4×4 嵌套 list；如果业务侧使用列主序请改用 `Frustum.from_view_projection_column_major`。
  - 动画推进后必须调用 `scene.update_world_transforms()`（必要时再 `rebuild_octree()`）才能让 `visible_objects` 命中最新世界矩阵。

**章节来源**
- [client/engine/conn_msg_handle.py:27-35](file://client/engine/conn_msg_handle.py#L27-L35)
- [client/engine/player.py:33-54](file://client/engine/player.py#L33-L54)
- [client/engine/subentity.py:25-46](file://client/engine/subentity.py#L25-L46)
- [client/engine/receiver.py:20-26](file://client/engine/receiver.py#L20-L26)
- [client/engine/callback.py:17-23](file://client/engine/callback.py#L17-L23)

## 结论
该 Python 客户端 SDK 通过 app/context/conn_msg_handle 的清晰分层，结合 player/subentity/receiver 的实体模型与 callback 的回调体系，提供了稳定可靠的连接、认证、实体管理与消息处理能力。配合事件循环与线程隔离，既满足异步编程需求，又保持了良好的可维护性与扩展性。

**更新** 新增的渲染场景模块（scene / 八叉树 / GLTF 资源导入 / camera）通过 pyo3 暴露到 Python 层，配合集中式 `client::add_to_module` 注册架构，使 Python 业务代码可以直接驱动 glTF 场景导入、动画推进、视锥体可见性裁剪等高性能渲染能力。

**注意** server 侧 `pyhub` 中存在的物理系统（rapier）**不属于客户端 SDK**，本文档不再描述其 API；如需了解物理仿真，请参阅 `服务器架构` 与 `API 参考` 中的 server 侧文档。

## 附录：使用示例与最佳实践

### 异步编程模式与事件循环
- 在独立线程中运行事件循环，使用 run_coroutine_threadsafe 提交协程任务，确保线程安全。
- 避免在事件循环线程中执行阻塞操作，必要时使用异步 I/O 或线程池。

**章节来源**
- [client/engine/app.py:131-139](file://client/engine/app.py#L131-L139)

### 登录流程（账号认证、重连与 Hub 服务请求）
- 基本步骤
  - 构建 app 并设置事件处理回调。
  - 建立连接（TCP/WS），等待 on_conn_id。
  - 执行 login，等待 Hub 认证结果。
  - 如需重连，使用 reconnect 并携带账户信息。
  - 通过 request_hub_service 请求 Hub 服务。
- 示例参考
  - 登录调用与回调封装：[sample/client/py/engine/login_cli.py:36-46](file://sample/client/py/engine/login_cli.py#L36-L46)
  - 获取排行榜请求与回调封装：[sample/client/py/engine/get_rank_cli.py:62-80](file://sample/client/py/engine/get_rank_cli.py#L62-L80)
  - 心跳请求处理模块：[sample/client/py/engine/heartbeat_cli.py:37-51](file://sample/client/py/engine/heartbeat_cli.py#L37-L51)

```mermaid
sequenceDiagram
participant CLI as "客户端"
participant APP as "app.py"
participant CTX as "context.py"
participant NET as "网关/Hub"
CLI->>APP : build()/connect_tcp/connect_ws
NET-->>APP : on_conn_id
CLI->>APP : login(sdk_uuid, argvs)
NET-->>APP : 认证结果/错误
CLI->>APP : request_hub_service(name, argvs)
NET-->>APP : 服务响应/通知
```

**图表来源**
- [client/engine/app.py:94-112](file://client/engine/app.py#L94-L112)
- [client/engine/context.py:14-21](file://client/engine/context.py#L14-L21)
- [sample/client/py/engine/login_cli.py:40-46](file://sample/client/py/engine/login_cli.py#L40-L46)
- [sample/client/py/engine/get_rank_cli.py:66-79](file://sample/client/py/engine/get_rank_cli.py#L66-L79)

### 实体管理 API 使用
- 注册实体构造器
  - 使用 register(entity_type, creator) 注册实体类型与构造函数。
- 创建/更新/删除
  - create_entity/update_entity/delete_entity：由 app 驱动实体管理器更新。
- 示例参考
  - 实体注册与使用：[client/engine/app.py:113-129](file://client/engine/app.py#L113-L129)

**章节来源**
- [client/engine/app.py:113-129](file://client/engine/app.py#L113-L129)

### 消息处理机制：全局方法、回调绑定与自定义消息
- 全局方法注册
  - register_global_method(method, callback)：注册全局方法处理函数。
- 回调绑定
  - player/subentity：通过 reg_hub_callback/reg_hub_notify_callback 绑定回调。
  - receiver：通过 reg_hub_notify_callback 绑定通知回调。
- 自定义消息处理
  - 在 conn_msg_handle 中扩展 on_call_* 分支，或在实体侧补充 handle_* 方法。

```mermaid
flowchart TD
A["收到RPC/通知"] --> B{"目标实体存在？"}
B --> |是| C["调用实体.handle_* 方法"]
B --> |否| D["尝试全局方法 on_call_global"]
C --> E["执行回调/更新状态"]
D --> E
```

**图表来源**
- [client/engine/conn_msg_handle.py:36-82](file://client/engine/conn_msg_handle.py#L36-L82)
- [client/engine/player.py:26-61](file://client/engine/player.py#L26-L61)
- [client/engine/subentity.py:31-69](file://client/engine/subentity.py#L31-L69)
- [client/engine/receiver.py:20-28](file://client/engine/receiver.py#L20-L28)

### 错误处理策略与网络异常恢复
- 回调错误分支：在 callback 的 error 回调中处理 Hub 返回的错误。
- 超时控制：通过 callback.timeout 设置超时，避免悬挂请求。
- 重连策略：在 on_kick_off/on_transfer_complete 中触发重连逻辑，重建 app 与连接。

**章节来源**
- [client/engine/callback.py:13-23](file://client/engine/callback.py#L13-L23)
- [client/engine/conn_msg_handle.py:27-35](file://client/engine/conn_msg_handle.py#L27-L35)

### 渲染场景使用指南
**新增** 渲染场景模块在 Python 侧的完整使用指南，覆盖资源加载、可见性查询、动画推进与节点矩阵访问：

#### 基础使用：导入 glTF 与查询节点
```python
from client.engine.scene import Scene

# 1. 导入 glTF / GLB 场景；同时构建八叉树用于可见性查询
scene = Scene.import_gltf("assets/level.glb", max_objects=8, max_depth=6)

print("nodes:", scene.node_count())
print("objects:", scene.object_count())
print("animations:", scene.animation_count())

# 2. 遍历根节点 → 子节点 → 对象
for nid in scene.root_nodes():
    node = scene.get_node(nid)
    print(node.id, node.world_transform)  # 行主序 4x4
    for oid in node.objects:
        obj = scene.get_object(oid)
        print("  -", obj.entity_id, obj.aabb.min, obj.aabb.max)
```

#### 视锥体可见性裁剪
```python
from client.engine.camera import Frustum

# vp 必须是行主序 4x4 嵌套 list；如使用列主序请改用 from_view_projection_column_major
vp = build_view_projection_matrix(...)
frustum = Frustum.from_view_projection(vp)

for obj in scene.visible_objects(frustum):
    print(obj.entity_id, obj.center)
```

#### 动画推进与世界矩阵刷新
```python
clip_idx = scene.animation_index("Run") or 0
time = 0.0
for _ in range(60):
    # update_animation 内部会调用 update_world_transforms
    time, _ = scene.update_animation(clip_idx, time=time, dt=1.0 / 60.0)

# 业务直接修改 transform 后，需要手动刷新
scene.update_world_transforms()
scene.rebuild_octree()
```

#### 平面级查询（自定义裁剪/触发）
```python
for plane in frustum.planes():
    print(plane.normal, plane.distance)

# 单点 / AABB 查询
from client.engine.scene import AABB
aabb = scene.get_object(0).aabb
print(frustum.contains_aabb(aabb))
```

**章节来源**
- [client/engine/scene.py:1-35](file://client/engine/scene.py#L1-L35)
- [client/engine/camera.py:1-12](file://client/engine/camera.py#L1-L12)
- [client/lib/client/src/py/scene.rs:38-211](file://client/lib/client/src/py/scene.rs#L38-L211)
- [client/lib/client/src/py/scene_object.rs:14-207](file://client/lib/client/src/py/scene_object.rs#L14-L207)
- [client/lib/client/src/py/camera.rs:55-127](file://client/lib/client/src/py/camera.rs#L55-L127)

### 实际项目集成要点
- 初始化顺序：context → app → 实体管理器 → 事件循环线程。
- 生命周期管理：在应用退出时调用 app.close()，确保资源释放。
- 参数编码：login/reconnect/request_hub_service 的参数需按 SDK 规范序列化为二进制。
- **更新** 渲染场景集成：
  - 顶层 cdylib `pyclient` 由 `client/src/lib.rs` 声明 `#[pymodule]`，全部注册逻辑集中在子 crate `client::add_to_module` 中；要新增 pyclass 请改 `client/lib/client/src/py/` 下的文件并在 `client::py::add_to_module` 里注册，不要在顶层 cdylib 内直接注册。
  - 渲染层 PyClass（`Plane` / `Frustum` / `AABB` / `Transform` / `SceneObject` / `SceneNode` / `Scene`）全部集中在 `client/lib/client/src/py/`；底层 `crates/camera`、`crates/scene` **零 pyo3 依赖**，仅暴露纯 Rust API，可被任意不含 pyo3 的上层复用。
  - server 侧 `pyhub` 不引入 scene/camera；client 侧 `pyclient` 不引入 physics，二者保持完全隔离。
  - 行主序 vs 列主序：所有矩阵在 Python 边界都使用行主序 `[row][col]`；底层 `cgmath` 是列主序，已在 `client/lib/client/src/py/` 中统一转置。
- 示例参考
  - 通用协议编解码与枚举：[sample/client/py/engine/common_cli.py:9-67](file://sample/client/py/engine/common_cli.py#L9-L67)
  - 登录调用与回调封装：[sample/client/py/engine/login_cli.py:12-46](file://sample/client/py/engine/login_cli.py#L12-L46)
  - 排行榜查询与回调封装：[sample/client/py/engine/get_rank_cli.py:12-80](file://sample/client/py/engine/get_rank_cli.py#L12-L80)
  - 心跳请求处理模块：[sample/client/py/engine/heartbeat_cli.py:12-51](file://sample/client/py/engine/heartbeat_cli.py#L12-L51)