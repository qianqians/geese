# 编辑器物理方案

## 当前架构 (2026-07)

编辑器支持**本地物理模式**，使用 `physics_manager` crate 管理本地 Rapier3D 物理世界。
服务器物理开发完全独立于编辑器，通过 `server/` 项目独立构建运行。

### 架构分层

```
crates/physics/            ← 纯物理引擎 (Rapier3D, 零网络, 零 pyo3)
crates/navmesh/            ← 寻路系统 (A* + Funnel, 零依赖)
crates/gameplay_physics/   ← 游戏玩法物理 (胶囊体, 布娃娃, 角色物理)
                            客户端和服务器共用
crates/physics_manager/    ← 本地物理管理 (编辑器使用)
crates/physics_client/     ← TCP 物理客户端 (保留, 供未来远程调试)
```

### 物理组件系统

实体物理行为通过 `PhysicsComponentDef` 定义：

```rust
pub struct PhysicsComponentDef {
    pub server_enabled: bool,    // 服务器是否运行物理模拟
    pub client_enabled: bool,    // 客户端是否运行物理模拟
    pub collision_enabled: bool, // 碰撞体开关
    pub body_kind: BodyKindDef,  // 刚体类型 (Static/Dynamic)
}
```

- 编辑器中在 Inspector 面板添加/移除物理组件
- 物理组件的 Server/Client 开关控制实体在哪一端运行物理模拟
- 配置序列化到 `.scene.json` 的 `physics` 字段中

### JSON 格式

**新格式**:
```json
{ "physics": { "server_enabled": true, "client_enabled": true, "body_kind": "fixed" } }
```

**向后兼容**: 旧格式的 `collision_enabled` + `body_kind` 扁平字段自动迁移。

### 编辑器物理路径

编辑器 → `physics_manager` → `physics` crate → Rapier3D (本地进程内)

### 服务器物理路径

服务器 Hub → `physics` (pyo3) + `gameplay_physics` → Python 游戏逻辑 (独立运行)

服务器开发通过 `server/` 项目独立进行，与编辑器零耦合。

### 客户端 SDK 物理路径

客户端 SDK → `physics` + `navmesh` + `gameplay_physics` (为客户端预测做准备)

## 历史方案 (已废弃)

此前 `physics_manager` 支持 Server/Client/ClientAndServer 三种模式，
允许编辑器启动 Python 子进程作为远程物理服务器。该方案已被废弃，
原因：
- 编辑器不应涉及服务器开发
- 服务器应独立构建和部署
- 与客户端通过 RPC (Thrift/WebSocket) 对接
