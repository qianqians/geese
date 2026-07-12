# 修复 P2/P3 级缺陷实施计划

## 关键发现（来自三视角研究）

经过代码验证，以下发现修正了审查报告中的部分描述：

1. **`scene/src/ragdoll.rs` 是死代码**：`scene/src/lib.rs` 中无 `mod ragdoll;` 声明，scene 通过 `pub use gameplay_physics::{...}` 重导出 ragdoll 类型（L24-25）。321 行文件未被编译，应直接删除而非提取新 crate。
2. **音频 3D 衰减已部分实现**：`rodio_backend.rs` L55-61 有 `compute_attenuation()`（反距离模型），`set_position()` 会自动触发衰减重算（L170-177）。真正的缺口是：`update_listener()` 未暴露在 `AudioSystem` 层、衰减参数不可配置、无 `max_distance` 截止。
3. **`game_runtime` 未依赖 `config` crate**：需要新增依赖才能读取 `ConfigRenderingPath`。

---

## Task 1: 删除 scene 死代码 ragdoll.rs [P2-W11]

**风险**: 极低 | **工作量**: 微小

- 直接删除 `crates/scene/src/ragdoll.rs`（321行）
- `scene/src/lib.rs` 未声明 `mod ragdoll;`，删除不影响任何编译
- 验证: `cd crates/scene && cargo build`

---

## Task 2: 修正 render feature 默认策略 [P2-W13]

**风险**: 低 | **工作量**: 微小

- 修改 `crates/render/Cargo.toml` L14: `default = ["instancing"]` → `default = []`
- **同步修复下游依赖**（保持现有行为不变）:
  - `crates/scene/Cargo.toml` L17: `render = { path = "../render", version = "0.1.0" }` → 添加 `features = ["instancing"]`
  - `crates/game_runtime/Cargo.toml` L30: `render = { path = "../render" }` → 添加 `features = ["instancing"]`
  - `desktop/Cargo.toml`: 检查 render 依赖，按需添加 `features = ["instancing"]`
- 验证: 全量 `cargo build` + 确认 instancing 代码路径仍被编译

---

## Task 3: db.rs 重复代码提取辅助方法 [P3-S9]

**风险**: 低 | **工作量**: 小

文件: `server/lib/dbproxy/src/db.rs`（455行，7 个 do_* 方法）

提取私有辅助方法 `serialize_and_send`，封装公共的 "TBufferChannel → split → TCompactOutputProtocol → write_to_out_protocol → upgrade send_proxy → send" 模式（每处 15-20 行重复代码）：

```rust
async fn serialize_and_send(&self, cb: &DbCallback, buffer_cap: usize, context: &str) {
    let t = TBufferChannel::with_capacity(0, buffer_cap);
    let (rd, wr) = match t.split() {
        Ok(v) => v,
        Err(e) => { error!("{} t.split error {}", context, e); return; }
    };
    let mut o_prot = TCompactOutputProtocol::new(wr);
    let _ = DbCallback::write_to_out_protocol(cb, &mut o_prot);
    if let Some(p) = self.send_proxy.upgrade() {
        let mut p_send = p.as_ref().lock().await;
        let _ = p_send.send(&rd.write_bytes()).await;
    } else {
        error!("{} send_proxy is destroy!", context);
    }
}
```

每个 `do_*` 方法简化为：下转型事件数据 → 调用 mongo_proxy → 构建 DbCallback → `self.serialize_and_send(&cb, 16384, "do_xxx").await`

`do_get_object_info`（L331-411）的分批发送逻辑额外提取为 `send_batched_docs` 辅助。

预计净减少约 120 行重复代码。

---

## Task 4: Launcher 模板 main.rs 填充 [P2-W8]

**风险**: 低 | **工作量**: 小

文件: `crates/launcher/src/templates.rs` L693-L738

将 `main_rs_content()` 中 L729-L733 的两行 TODO 注释替换为调用 `game_runtime` 入口的可运行代码。模板目录下已有 `fps_camera.rs.txt`、`scene_builder.rs.txt` 等成熟片段可引用。

同时修复模板 Cargo.toml 中 wgpu 版本（当前 `"0.20"` 应改为 `"22.1"` 以匹配引擎版本）。

验证: 运行 launcher 生成新项目，`cargo build` 通过。

---

## Task 5: 运行时管线路径切换 [P2-S6]

**风险**: 低 | **工作量**: 小

文件: `crates/game_runtime/src/lib.rs` L174, `crates/game_runtime/Cargo.toml`

1. `Cargo.toml` 添加依赖: `config = { path = "../config" }`
2. 新增 `GameState::new_with_config()` 构造器，接受 `EngineConfig` 参数，从中读取 `rendering_path` 选择 `forward_plus()` 或 `deferred_plus()`
3. **保留原 `new()` 不变**（内部调用 `new_with_config` + 默认配置），确保向后兼容
4. 可选: 添加 `switch_rendering_path()` 方法支持运行时重建渲染管线

注意: `config` crate 中已有 `ConfigRenderingPath` 枚举（ForwardPlus / DeferredPlus），且 render crate 的 Cargo.toml 已依赖 config（L11），但 game_runtime 未依赖 config，需新增。

---

## Task 6: 音频 3D 空间衰减增强 [P2-S3]

**风险**: 低 | **工作量**: 小

文件: `crates/audio/src/lib.rs`, `crates/audio/src/rodio_backend.rs`

3D 衰减核心逻辑已存在于 `rodio_backend.rs`（`compute_attenuation` L55-61）。需要补齐的缺口：

1. 在 `AudioSystem`（lib.rs）添加 `update_listener()` 方法，将调用转发到底层后端（当前 `update_listener` 仅在 `RodioBackend` 上可用，`AudioSystem` 层无法调用）
2. 将硬编码的 `ROLLOFF_FACTOR`（L18）改为可配置参数（通过 `AudioConfig` 或直接暴露 setter）
3. 添加 `max_distance` 截止: 距离超过阈值时 gain 直接归零，避免无效计算
4. 补充衰减相关的单元测试

---

## Task 7: PhysicsManager 状态迁移 [P2-S4]

**风险**: 中低 | **工作量**: 小

文件: `crates/physics_manager/src/manager.rs`

当前 `PhysicsManager` 仅持有单个 `PhysicsWorld` + `SceneId`，无场景切换或状态迁移 API。最小变更方案：

1. 定义 `BodySnapshot` 结构体: `{ position: Vec3, rotation: Quat, linear_velocity: Vec3, angular_velocity: Vec3, body_kind: RigidBodyType }`
2. 添加 `export_snapshots(&self) -> Vec<BodySnapshot>` — 从当前 scene 读取所有刚体状态
3. 添加 `switch_scene(&mut self, gravity: [f32; 3]) -> (SceneId, Vec<BodySnapshot>)` — 导出旧快照、创建新 scene、切换 scene_id
4. 添加 `import_snapshots(&mut self, snapshots: &[BodySnapshot])` — 在新 scene 中恢复刚体

仅新增方法，不修改已有签名。后续如需"后端切换"可基于此 API 扩展。

---

## Task 8: 编辑器角色控制器集成 [P2-W12]

**风险**: 低 | **工作量**: 小

文件: `crates/editor/src/editor.rs` L843-849

Editor 当前不直接持有 Scene/Physics 引用（TODO 注释已说明）。最小可行方案是参照 `physics_component_cache`（L857-863）和 `navmesh_component_cache` 的现有模式：

1. 在 editor state 中新增 `character_controller_cache: HashMap<String, CharacterControllerConfig>`
2. 将 `ToggleCharacterController` 分支的 `eprintln!` 替换为 cache insert/remove + `log::info!`
3. 定义 `CharacterControllerConfig { move_speed, jump_impulse, air_control, half_height, radius }` 结构体

后续由 scene 加载器消费此缓存，与现有 physics/navmesh 组件缓存模式完全一致。

---

## Task 9: Edition 统一为 2024 [P3-S2]

**风险**: 中 | **工作量**: 中

当前 12 个 crate 使用 2024，18 个使用 2021。统一为 2024，分三批次：

- **批次 A（低风险，纯数据 crate）**: `time`, `queue`, `sync`, `vfs`, `ecs`, `config`, `terrain` — 代码量小，直接改 + `cargo build` 验证
- **批次 B（中风险，中等复杂度）**: `gameplay_physics`, `vfx`, `tcp`, `wss`, `proto`, `redis_service`
- **批次 C（高风险，服务端 + 桌面）**: `server`, `server/dbproxy`, `server/hub`, `server/gate`, `desktop`, `health`, `log`, `aoi`

每批次改完后 `cargo build` + `cargo test` 全量验证。重点关注的 2024 edition breaking change：
- `gen` 成为保留关键字
- `unsafe_op_in_unsafe_fn` lint 默认 warn
- `impl Trait` 默认生命周期变化
- tail expression drop order 变化

---

## Task 10: 核心 crate 基础测试覆盖 [P3-S5]

**风险**: 极低 | **工作量**: 中

优先为 camera / math / time 三个 crate 添加 `#[cfg(test)] mod tests`：

- **math** (`crates/math/src/lib.rs`, 46行): AABB 的 `center()`, `size()`, `contains_point()`, `intersects_aabb()` 测试，含边界情况
- **time** (`crates/time/src/lib.rs`, 30行): `OffsetTime::new()`, `set_time_offset()`, `utc_unix_time_with_offset()` 测试
- **camera** (`crates/camera/src/`): Camera 构造、`view_projection_raw()` 返回非零矩阵、`frustum()` 有效平面、模式切换测试

每个 crate 至少 5 个单元测试。

---

## 不纳入本次修复范围的项目

**原生 Rust 服务器入口 (P3)**: `server/src/native_server.rs` 当前是完整 stub（86行，while 循环 + sleep），需要实现 TCP listener、连接管理、实体管理、AOI、服务发现等，属于多周工作量的独立项目，建议作为独立 milestone 推进。

---

## 执行顺序与依赖关系

```
Task 1 (删除死 ragdoll.rs)  ──┐
Task 2 (render feature)     ──┤── Phase 1: 低风险清理（可并行）
Task 3 (db.rs 重构)         ──┤
                              ┘
Task 4 (Launcher 模板)      ── 依赖 Task 2（feature 影响模板编译）
Task 5 (管线路径切换)       ── Phase 2: 功能完善（独立）
Task 6 (音频衰减增强)       ── Phase 2: 功能完善（独立）
Task 7 (PhysicsManager)     ── Phase 2: 功能完善（独立）
Task 8 (编辑器角色控制器)   ── Phase 2: 功能完善（独立）

Task 9 (Edition 统一)       ── Phase 3: 全局性变更（依赖 Phase 1+2 全部完成）
Task 10 (测试覆盖)          ── Phase 4: 在 Edition 统一后添加

推荐并行: {1,2,3} → {4,5,6,7,8} → 9 → 10
```

---

## 被否决的方案

| 方案 | 否决理由 |
|------|----------|
| 提取 ragdoll 为独立 crate（Alex 方案） | Jack 验证发现 `scene/src/ragdoll.rs` 是未被 `mod` 声明的死代码，直接删除即可。提取新 crate 是不必要的复杂度。 |
| 创建 `audio/spatial.rs` 实现全新衰减系统（Alex/Sam 方案） | 3D 衰减核心逻辑已存在于 `rodio_backend.rs`，无需重新实现。缺口仅是 API 暴露和参数可配置化。 |
| 完整 PhysicsManager 后端切换 + trait 抽象（Sam 方案） | 当前只有一个后端（本地 rapier），"后端切换"是前瞻性需求。最小可行的 `switch_scene()` API 足够为后续扩展打基础，避免过度设计。 |
| 原生 Rust 服务器入口实现（三个方案均提及） | 86 行 stub 需要扩展为完整服务端（TCP listener + 连接管理 + AOI + 服务发现），超出 P2/P3 缺陷修复范围。 |
