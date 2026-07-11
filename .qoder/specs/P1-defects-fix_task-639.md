
# P1 缺陷修复计划

## 缺陷概览（按变更风险从低到高排列）

| 序号 | 编号 | 问题 | 风险 |
|:----:|------|------|:----:|
| 1 | W9 | ConfigRenderingPath ↔ RenderingPath 无转换 | 低 |
| 2 | C7 | Inspector Transform 同步空函数 + 缓存污染 | 低 |
| 3 | W10 | version_handshake 未接入 service union | 低 |
| 4 | W4 | PyO3 构造函数 panic!() 跨 FFI | 低 |
| 5 | W5 | Thrift Option 字段 .unwrap()（14+ 处）| 中 |
| 6 | W6 | Mutex::lock().unwrap() 级联 panic（16 处）| 中 |
| 7 | W7 | TextureCooker/MeshCooker stub 无警告 | 低 |
| 8 | W1 | SSAO/SSR/DoF/MotionBlur 静默跳过 | 低 |
| 9 | W2 | Bloom 合成丢失原始场景颜色 | 中 |

---

## 任务 1: W9 — 添加 ConfigRenderingPath → RenderingPath 的 From 转换

**文件**:
- `crates/render/Cargo.toml` — 添加 `config` 依赖
- `crates/render/src/pipeline.rs` — 添加 `From` impl

**操作**:
1. 在 `crates/render/Cargo.toml` 的 `[dependencies]` 中添加:
   ```toml
   config = { path = "../config" }
   ```
2. 在 `pipeline.rs` 的 `RenderingPath` 定义之后添加:
   ```rust
   impl From<config::ConfigRenderingPath> for RenderingPath {
       fn from(c: config::ConfigRenderingPath) -> Self {
           match c {
               config::ConfigRenderingPath::ForwardPlus => Self::ForwardPlus,
               config::ConfigRenderingPath::DeferredPlus => Self::DeferredPlus,
           }
       }
   }
   ```

**验证**: `cd crates/render && cargo check`

---

## 任务 2: C7 — 修复 InspectorPanel Transform 缓存污染

**文件**: `crates/editor/src/inspector.rs` (L95-97, L163-168)

**问题**: `sync_transform()` 是空函数，且在 `show()` L164-165 中 cache miss 时会将错误默认值 `[0,0,0]` 写入缓存，导致后续读取永远拿到错误数据。

**操作**:
1. 实现 `sync_transform()` (L94-97): 从缓存的 `EditorState` 同步 Transform。由于当前架构中 `sync_transform` 无法直接访问 `EditorState`，保留其作为公共 API 桩但改为从传入的缓存参数读取:
   ```rust
   pub fn sync_transform(&mut self, entity_id: &str, transform_cache: &HashMap<String, ([f32;3],[f32;3],[f32;3])>) {
       if let Some(&(pos, rot, scl)) = transform_cache.get(entity_id) {
           self.position = pos;
           self.rotation = rot;
           self.scale = scl;
       }
   }
   ```
2. **修复缓存污染** — 在 `show()` L163-168 的 else 分支中: 移除 `state.transform_cache.insert(entity_id.clone(), defaults)` 行（这会污染缓存），仅保留本地赋值。添加 `log::warn!` 提示数据未就绪:
   ```rust
   } else {
       log::warn!("Inspector: transform cache miss for entity '{}'", entity_id);
       let defaults = ([0.0, 0.0, 0.0], [0.0, 0.0, 0.0], [1.0, 1.0, 1.0]);
       self.position = defaults.0;
       self.rotation = defaults.1;
       self.scale = defaults.2;
   }
   ```

**验证**: `cd crates/editor && cargo check`

---

## 任务 3: W10 — 将 version_handshake 添加到 gate_client_service union

**文件**: `crates/proto/proto/gate.thrift` (L224-233)

**操作**:
1. 在 `gate_client_service` union 中添加第 9 个字段（field ID 9，因为 1-8 已被占用）:
   ```thrift
   union gate_client_service {
       1:client_request_hub_login login,
       2:client_request_hub_reconnect reconnect,
       3:client_request_hub_service request_hub_service, 
       4:client_call_hub_rpc call_rpc,
       5:client_call_hub_rsp call_rsp,
       6:client_call_hub_err call_err,
       7:client_call_hub_ntf call_ntf,
       8:client_call_gate_heartbeats heartbeats,
       9:version_handshake handshake,
   }
   ```
2. 运行 Thrift 代码生成器重新生成 Rust 代码: `cd crates/proto && gen_proto.bat`
3. 检查生成的 `crates/proto/src/gate.rs` 编译通过，并在 gate 服务器消息处理循环中添加 `GateClientService::Handshake` 分支处理（预留处理逻辑，至少记录 `debug!` 日志）

**验证**: `cd crates/proto && cargo check`

---

## 任务 4: W4 — 用 PyResult::Err 替换 PyO3 构造函数中的 panic!

**文件**: `server/lib/hub/src/lib.rs` (L799, L833)

**操作**:
1. L799: 将 `Err(_) => panic!("Failed to acquire server lock after 500ms")` 替换为:
   ```rust
   Err(_) => {
       error!("Failed to acquire server lock after 500ms");
       return Err(PyValueError::new_err("Failed to acquire server lock after 500ms"));
   }
   ```
2. L833: 对 `HubDBMsgPump::new` 做同样修改

**注意**: `PyValueError` 已在 L9 导入 (`use pyo3::exceptions::PyValueError;`)，无需额外导入。

**验证**: `cd server && cargo check`

---

## 任务 5: W5 — 将 Thrift Option 字段 .unwrap() 替换为安全处理

**文件**: 
- `server/lib/hub/src/hub_service_manager.rs` (14+ 处)
- `server/lib/hub/src/hub_server.rs` (7 处)

**策略**: 遵循项目中 `hub_msg_handle.rs`/`gate_msg_handle.rs` 已有的模式——对于非关键字段使用 `.unwrap_or_default()`，对于关键字段（如 gate_name）使用 `match` + `error!()` + 提前返回。

**操作**:

### hub_service_manager.rs:
1. **`name.clone().unwrap()`** (L212, L253) → `.unwrap_or_default()`
2. **`type_.clone().unwrap()`** (L213) → `.unwrap_or_default()`
3. **`hub_name.clone().unwrap()`** (L279, L352) → `.unwrap_or_default()`
4. **`gate_name.clone().unwrap()`** (L281, L355, L450, L484) → match + `error!()` + return:
   ```rust
   let gate_name = match ev.gate_name.clone() {
       Some(n) => n,
       None => {
           error!("Missing required field 'gate_name' in event");
           return;
       }
   };
   ```
5. **`gate_host.clone().unwrap()`** (L282, L356) → `.unwrap_or_default()`
6. **`gate_host.unwrap()`** (L461) → `.unwrap_or_default()`
7. **`redis_service.clone().unwrap()`** (L255, L285, L359) → match + `error!()` + return
8. **`request_infos.unwrap()`** (L354) → `.unwrap_or_default()` (与 hub_msg_handle.rs L168 一致)
9. **`conn_id.unwrap()`** / **`entity_id.unwrap()`** (L529) → match + `error!()` + return
10. **`gate_name.unwrap()`** (L557, L560) → match + `error!()` + return

### hub_server.rs:
1. **`hub_redis_service.as_mut().unwrap()`** (L128, L247) → match + `error!()` + return
2. **`redis_mq_service.unwrap()`** (L142, L160, L171) → match + `error!()` + continue/return
3. **`hub_redis_service.clone().unwrap()`** (L185) → match + `error!()` + return
4. **`gate_host.clone().unwrap()`** (L241) → `.unwrap_or_default()`

**验证**: `cd server && cargo check`

---

## 任务 6: W6 — 将 Mutex::lock().unwrap() 替换为 unwrap_or_else

**文件**:
- `server/lib/hub/src/hub_service_manager.rs` (12 处)
- `server/lib/hub/src/hub_server.rs` (2 处)
- `server/lib/hub/src/dbproxy_msg_handle.rs` (2 处)

**操作**: 将所有 `StdMutex::lock().unwrap()` 替换为 `lock().unwrap_or_else(|e| e.into_inner())`。

受影响的调用点:
- `hub_service_manager.rs`: L66, L87, L185, L193, L243, L334, L409, L469, L504, L512, L520, L582
- `hub_server.rs`: L111, L233
- `dbproxy_msg_handle.rs`: L67, L74

**注意**: 只修改 `std::sync::Mutex` 的 `.lock()`，不修改 `tokio::sync::Mutex` 的 `.lock().await`。

**验证**: `cd server && cargo check`

---

## 任务 7: W7 — 为 TextureCooker/MeshCooker stub 添加 log::warn!

**文件**: `crates/asset/src/texture_cooker.rs`

**操作**:
1. 在文件头部添加 `use log;`（log crate 已在 asset 的依赖中可用）
2. 在 `TextureCooker::cook()` (L63) 开头添加:
   ```rust
   log::warn!("TextureCooker::cook is a stub — returning raw RGBA8 data. Integrate basis-universal for BC7/ASTC/ETC2 compression.");
   ```
3. 在 `MeshCooker::cook()` (L88) 开头添加:
   ```rust
   log::warn!("MeshCooker::cook is a stub — returning raw vertex/index data. Integrate meshopt for vertex fetch + overdraw optimization.");
   ```

**验证**: `cd crates/asset && cargo check`

---

## 任务 8: W1 — 为无实现的 GPU 后处理效果添加 log::warn!

**文件**: `crates/render/src/post_pipeline.rs` (L309 之后)

**操作**: 在 `process()` 方法中 L309（mask 提取处）之后，添加对未实现效果的警告。使用 `std::sync::atomic::AtomicBool` 确保每个效果仅警告一次，避免日志泛滥:

```rust
use std::sync::atomic::{AtomicBool, Ordering};

// 在 process() 中 L309 之后:
static WARNED_SSAO: AtomicBool = AtomicBool::new(false);
static WARNED_SSR: AtomicBool = AtomicBool::new(false);
static WARNED_DOF: AtomicBool = AtomicBool::new(false);
static WARNED_MOTION_BLUR: AtomicBool = AtomicBool::new(false);

if mask.contains(EffectMask::SSAO) && !WARNED_SSAO.swap(true, Ordering::Relaxed) {
    log::warn!("SSAO effect is not yet implemented (no GPU shader); effect will be skipped.");
}
if mask.contains(EffectMask::SSR) && !WARNED_SSR.swap(true, Ordering::Relaxed) {
    log::warn!("SSR effect is not yet implemented (no GPU shader); effect will be skipped.");
}
if mask.contains(EffectMask::DOF) && !WARNED_DOF.swap(true, Ordering::Relaxed) {
    log::warn!("Depth of Field effect is not yet implemented (no GPU shader); effect will be skipped.");
}
if mask.contains(EffectMask::MOTION_BLUR) && !WARNED_MOTION_BLUR.swap(true, Ordering::Relaxed) {
    log::warn!("Motion Blur effect is not yet implemented (no GPU shader); effect will be skipped.");
}
```

**验证**: `cd crates/render && cargo check`

---

## 任务 9: W2 — 修复 Bloom 合成：tonemap shader 叠加 input + bloom

**文件**:
- `crates/render/shaders/post_tonemap.wgsl`
- `crates/render/src/post_pipeline.rs`

**问题**: 当前 bloom 启用时，`tonemap_source` 被设为 `&self.bloom_a`（仅 bloom 结果），原始场景颜色完全丢失。Tonemap shader 仅有一个纹理输入。

**修复方案**: 扩展 bind group layout 添加第二个纹理绑定点（bloom），在 shader 中合成 input + bloom。

**操作**:

### 9a. 修改 `post_tonemap.wgsl`:
在现有 binding 2 之后添加 (L14 之后):
```wgsl
@group(0) @binding(3) var t_bloom: texture_2d<f32>;
```

修改 `fs_tonemap` 函数，在 exposure 和 tonemap 之间添加 bloom 合成:
```wgsl
@fragment
fn fs_tonemap(in: VertexOutput) -> @location(0) vec4f {
    let color = textureSample(t_input, s_input, in.uv).rgb;
    let exposure = u_post.params.x;

    var result = color * exposure;

    // Bloom synthesis (bit 1 = bloom enabled)
    if ((enabled_mask() & 2u) != 0u) {
        let bloom = textureSample(t_bloom, s_input, in.uv).rgb;
        result = result + bloom * u_post.params.z;  // z = bloom_intensity
    }

    // ACES tonemap (bit 0)
    if ((enabled_mask() & 1u) != 0u) {
        result = vec3f(
            aces_tonemap(result.r),
            aces_tonemap(result.g),
            aces_tonemap(result.b),
        );
    }

    return vec4f(result, 1.0);
}
```

### 9b. 修改 `post_pipeline.rs`:
1. **扩展 bind group layout** (L84-115): 在现有 3 个 entries 之后添加第 4 个 entry (binding 3):
   ```rust
   wgpu::BindGroupLayoutEntry {
       binding: 3,
       visibility: wgpu::ShaderStages::FRAGMENT,
       ty: wgpu::BindingType::Texture {
           sample_type: wgpu::TextureSampleType::Float { filterable: true },
           view_dimension: wgpu::TextureViewDimension::D2,
           multisampled: false,
       },
       count: None,
   },
   ```

2. **修改 `process()` 中的 tonemap bind group** (L386-403): 始终绑定 `input_view` 到 binding 1，绑定 `bloom_a` 到 binding 3:
   ```rust
   let tonemap_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
       label: Some("post tonemap bg"),
       layout: &self.bind_group_layout,
       entries: &[
           wgpu::BindGroupEntry {
               binding: 0,
               resource: self.uniform_buffer.as_entire_binding(),
           },
           wgpu::BindGroupEntry {
               binding: 1,
               resource: wgpu::BindingResource::TextureView(input_view),
           },
           wgpu::BindGroupEntry {
               binding: 2,
               resource: wgpu::BindingResource::Sampler(&self.sampler),
           },
           wgpu::BindGroupEntry {
               binding: 3,
               resource: wgpu::BindingResource::TextureView(if mask.contains(EffectMask::BLOOM) {
                   &self.bloom_a
               } else {
                   input_view  // 无 bloom 时复用 input_view（bloom intensity=0，不影响结果）
               }),
           },
       ],
   });
   ```

3. **删除 L375-384** 的 `tonemap_source` 二元选择逻辑（不再需要）。

**验证**: `cd crates/render && cargo check`

---

## 依赖关系

```
任务 1 (W9)  ──── 无依赖
任务 2 (C7)  ──── 无依赖
任务 3 (W10) ──── 依赖 Thrift 代码生成器；独立于其他任务
任务 4 (W4)  ──── 无依赖
任务 5 (W5)  ──── 无依赖（与任务 6 共享文件，建议先执行任务 5 再任务 6）
任务 6 (W6)  ──── 建议在任务 5 之后（同一文件 hub_service_manager.rs）
任务 7 (W7)  ──── 无依赖
任务 8 (W1)  ──── 无依赖
任务 9 (W2)  ──── 建议在任务 8 之后（共享 post_pipeline.rs）；独立于其他任务
```

**推荐执行顺序**: 任务 1、2、3、4、7 可并行执行 → 任务 5 → 任务 6 → 任务 8 → 任务 9

---

## 风险与缓解措施

| 风险 | 缓解措施 |
|------|----------|
| **W2**: bind group layout 增加 binding 3 后，bloom downsample/upsample pass 的 bind group 也需要匹配新 layout | bloom downsample pass (L314-331) 和 upsample pass 都使用 3 个 entries (binding 0,1,2)，需要将它们的 bind group 也扩展为 4 个 entries（binding 3 绑定 input_view 作为占位）——或者为 tonemap 单独创建一个 bind group layout。**选择**: 为 tonemap 创建独立的 BGL 可避免影响 bloom pass，但会引入额外复杂度。更简单的方案是扩展现有 BGL 并在 bloom pass 的 bind group 中添加 binding 3 dummy entry（绑定 bloom_a） |
| **W5**: `.unwrap_or_default()` 可能静默隐藏数据缺失 | 仅对非关键字段使用此模式（name、host 等）。对关键字段（gate_name、redis_service）使用 match + `error!()` + return |
| **W6**: `into_inner()` 在 poisoned Mutex 上可能返回部分更新数据 | `std::sync::Mutex` 的 poison 仅表示持有线程 panic，数据本身不变；恢复数据比级联崩溃更安全 |
| **W10**: Thrift 代码重新生成可能产生大量 diff | 先在独立分支运行 gen_proto，对比 diff 确认仅新增 1 个 union variant；union 默认向后兼容 |
| **W1**: warn! 每帧触发造成日志洪水 | 使用 `AtomicBool` 确保每个效果仅警告一次 |

---

## 被拒绝的方案

1. **W2 替代方案 — 添加中间 HDR 纹理 pass**: 先用一个 pass 将 input + bloom 合成到中间纹理，再对中间纹理做 tonemap。**拒绝原因**: 需要分配额外的全分辨率 HDR 纹理（显存开销大），且增加一个 render pass（GPU 开销）。直接修改 shader 添加第二个纹理绑定是最小开销方案。

2. **C7 替代方案 — 让 sync_transform 访问 EditorState**: 修改函数签名使其接受 `&EditorState` 引用。**拒绝原因**: 当前架构中 `sync_transform` 是 `InspectorPanel` 的方法，调用方不在 `show()` 的内部流程中。改为修改 `show()` 的 cache miss 逻辑更直接。

3. **W5 替代方案 — 全部使用 `.unwrap_or_default()`**: **拒绝原因**: 对 gate_name 等关键字段使用默认值会隐藏真正的协议错误，导致难以排查的运行时异常。关键字段必须使用 match + error log + return。

4. **W9 替代方案 — 在 game_runtime/desktop 中做手动转换**: **拒绝原因**: 这会导致每个使用点都需要重复的 match 代码，违反 DRY 原则。在 render crate 添加 `From` impl 是正确的 Rust 惯用做法。
