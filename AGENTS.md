# AGENTS.md

This file provides guidance to Qoder (qoder.com) when working with code in this repository.

## 项目概述

Geese 是一个 3D 游戏引擎/编辑器，核心为 Rust + wgpu 渲染管线，通过 PyO3 暴露 Python 脚本层。项目没有 Cargo workspace——`desktop/`、`server/`、`client/` 分别是独立的 Cargo 入口项目，通过 `path = "../crates/xxx"` 引用共享 crate。

## 构建与测试

### 构建编辑器（desktop 应用）

```bash
# Debug 构建
cd desktop && cargo build

# Release 构建
cd desktop && cargo build --release
```

运行编辑器：
```
python run_editor.py           # 直接打开项目编辑器
python test_desktop.py          # 启动 Launcher → Editor 流程
```

### 构建 server（游戏服务器端）

```bash
cd server && cargo build
```

Server 输出三个二进制：`dbproxy`、`gate` 和一个 `cdylib` Python 扩展模块 `pyhub`（Hub 节点通过 Python 脚本扩展游戏逻辑）。

### 构建 client（客户端 SDK）

```bash
cd client && cargo build
```

输出 `pyclient` cdylib 扩展模块，作为 Python 客户端的底层 Socket/RPC 层。

### 运行测试

各 crate 均支持标准 Cargo test：

```bash
# 运行单个 crate 的测试
cd crates/scene && cargo test

# 运行所有 crate 的测试（需要逐个执行，因为没有 workspace）
cd crates/render && cargo test
cd crates/physics && cargo test

# 运行特定测试
cd crates/scene && cargo test animation_graph_tests
```

### 构建配置

- `.cargo/config.toml`：仅为 macOS 交叉编译 PyO3 cdylib 设置 linker 参数，Windows/Linux 无需关注
- rust-analyzer 配置了三个 linked projects（见 `.vscode/settings.json`）：`client/Cargo.toml`、`server/Cargo.toml`、`desktop/Cargo.toml`。添加新 crate 后如需补全支持，需更新此列表
- 各 crate 的 `edition` 不统一：客户端/编辑器侧多用 `2024`，服务端侧多用 `2021`

## 架构概览

### 三层结构

```
desktop/          ← 编辑器 GUI 层 (egui + wgpu + PyO3)
  ↓
crates/           ← 共享引擎 crate（33 个，核心资产）
  ↓
server/ & client/ ← 游戏运行时（网络层）
```

### 核心 crate 依赖关系

**渲染管线**（GPU 侧，无平台抽象，直接依赖 wgpu）：
- `render` — wgpu 渲染器核心，同时提供 Forward+ 和 Deferred+ 两条管线，CLI 聚类光照、CSM 阴影、IBL、后处理链、骨骼蒙皮
- `grid` — 动态世界空间网格渲染（用于编辑器视口辅助）
- `lines` — 3D 线段渲染（禁止近平面裁剪，保证长线段可见）
- `material` — PBR 材质、纹理句柄、采样器

**场景与资源**（数据侧，不依赖 GPU）：
- `asset` — 统一资源管线：`Handle<T>` 引用计数、`AssetLoader<T>`、`AssetCache` 按 `(TypeId, path)` 去重。注意：当前为同步加载骨架，后续计划异步（tokio）+ 热重载（notify）
- `scene` — 场景图核心：`Scene`、`SceneObject`、`Octree`、`Prefab` 嵌套引用（含循环检测）、glTF 导入（网格/蒙皮/动画）、动画状态机（`AnimationStateMachine` + `BlendTree`）
- `avatar` — 角色骨骼/蒙皮/动画数据结构（纯数学 crate，无外部依赖）

**物理**：
- `physics` — 物理引擎后端，支持 `pyo3` 和 `scene-builder` 两个 feature
- `physics_client` — 远程物理的 TCP 客户端
- `physics_manager` — 统一物理后端管理层：整合本地/远程两种模式，独立于编辑器和服务端

**编辑器**（`crates/editor/`）：
- 模块化面板架构：`viewport`、`hierarchy`、`inspector`、`asset_browser`、`bundle_panel`、`gizmo`、`physics_debug`
- `CommandHistory` 撤销/重做系统，`SceneSerializer` 序列化
- `PanelLayer` / `PanelLayerManager` 面板层级管理
- `PlayMode` — 编辑器内运行模式切换

**UI 框架**：
- `ui` — egui 通用 UI 组件封装
- `launcher` — 项目模板生成器，启动时选择模板 → 生成工程 → 打开 Editor

**服务端架构**（`server/lib/`）：
- `hub` — 游戏逻辑节点（Python 可扩展，通过 pyo3-async-runtimes 在 tokio 中运行 Python 协程）
- `gate` — 网关节点（管理客户端 WebSocket/TCP 连接）
- `dbproxy` — 数据库代理
- 三层通过 `wss`（WebSocket）、`tcp`、`net`、`queue` 通信，服务发现使用 Consul

**服务端/客户端共享基础设施**（`crates/`）：
- `proto` — Thrift 协议定义 + 生成的 Rust 代码
- `wss` — WebSocket 服务器/客户端实现
- `tcp` — TCP 连接管理
- `net` — 网络消息编解码
- `redis_service` — Redis 缓存封装
- `mongo` — MongoDB 封装
- `consul` — Consul 服务发现客户端
- `aoi` — Area of Interest 空间广播
- `sync` — 实体内插状态同步
- `queue` — 消息队列抽象
- `time` — 游戏时间系统
- `config` — 配置加载
- `log` — 日志系统
- `health` — 健康检查
- `close_handle` — 优雅关闭

**RPC 系统**（`rpc/`）：
- Python 脚本工具链，从 Thrift/自定义 IDL 生成 Python/TypeScript 的 RPC stub 和数据类
- `rpc/gen/` — 代码生成器：`client_call_hub`、`hub_call_client`、`common`，每个目录下分别有 `python/` 和 `ts/` 目标

### 关键设计约定

**Prefab 嵌套引用**：`PrefabNodeDef` 中 `mesh` 与 `prefab_ref` 互斥；循环检测使用 `visited + recursion_stack` 双集合 DFS；实例化递归有 `max_depth` 限制。

**坐标系**：右手坐标系（cgmath），Y 轴朝上。编辑器视口中 `Y=0` 与窗口底部对齐。球坐标转笛卡尔时注意 pitch 符号。

**渲染管线选择**：`ScenePipelineDescriptor` 中 `RenderingPath` 枚举决定使用 `ForwardPlusPipeline` 或 `DeferredPlusPipeline`。编辑器默认使用 Forward+。

**资源路径**：所有资源加载通过 `AssetLoader`，使用 UUID + 文件路径映射，资源存储在项目 `config/` 目录中。

**Python 脚本调用约定**：所有涉及 Python 的入口点（desktop、server hub）使用 PyO3 `extension-module` 模式，编译为 `cdylib`，通过 `python` 命令加载 `.dll`（Windows）或 `.dylib`（macOS）。在 Windows 上必须使用 `python` 而非 `python3` 命令。
