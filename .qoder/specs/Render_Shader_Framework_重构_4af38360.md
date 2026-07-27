# Render Shader Framework 重构

## Summary

当前 render crate 使用 `include_str!` + `format!("{pbr_common}\n{pipeline}")` 手动拼接 shader，并通过 18 处 `#[cfg(feature = "use-shader-framework")]` 维护双路径。本次重构将统一为 shader_framework compose 路径，用 Mixin 替代手动拼接，消除所有 cfg 双分支，并创建首个 StageChain 实际用例（instancing 变体）。

## 重构范围

**需要改动的文件（4 个）:**
- `crates/render/src/shader_library.rs` — 扩展为完整的 shader 编译工厂
- `crates/render/src/forward_plus.rs` — 替换 format! 拼接，消除 cfg 双分支
- `crates/render/src/deferred_plus.rs` — 同上
- `crates/render/Cargo.toml` — 调整 feature gate 配置

**不需要改动的文件:**
- `post_pipeline.rs` — 6 个独立 shader，不依赖 pbr_common，无收益
- `sprite.rs` / `lines.rs` / `shadow_pass.rs` / `wgpu_ibl_baker.rs` / `hiz.rs` — 独立 shader
- `shader_graph.rs` — 独立的节点式 shader 生成，留作后续独立任务
- `terrain/` / `vfx/` — 外部 crate，不在本次范围
- 所有 `.wgsl` 文件 — 保持不变，仍是 entry point 的 source of truth
- `compose.rs` / `effect.rs` — StageChain 系统已完善，无需修改

## 关键设计决策

### Mixin 而非 StageChain 作为主要组合操作
当前 `format!("{pbr_common}\n{pipeline}")` 是 library prepend 语义——pbr_common 提供 structs/functions/constants 库，管线 .wgsl 提供完整 entry point。这直接映射到 `CompositionOp::Mixin`，而非 StageChain。

### StageChain 用于 instancing 变体
`forward_plus_instanced.wgsl` 的 vertex body 可以作为 StageChain 追加到 base forward_plus 的 vertex body 之后，实现 `[["vs", "fs"], ["instancing.vs"]]` 的配置模式。

### 保留 `use-shader-framework` feature gate 但使其成为推荐路径
不强制所有构建切换到 shader_framework，但简化新路径的代码量，使旧路径成为 fallback。

---

## Task 1: 扩展 shader_library.rs 为统一编译工厂

**文件**: `crates/render/src/shader_library.rs`

### 1.1 新增管线 shader 编译辅助函数

在 `generate_pbr_common_wgsl()` 之后，新增以下函数：

```rust
/// Compose a pipeline shader by mixing pbr_common into a pipeline module.
///
/// `pipeline_body_vs`: vertex entry point body from the pipeline .wgsl file
/// `pipeline_body_fs`: fragment entry point body (optional)
/// `pipeline_body_cs`: compute entry point body (optional)
/// `vs_streams`: vertex input streams for the pipeline
/// `bindings`: GPU bindings used by the pipeline
pub fn compose_pipeline_shader(
    name: &str,
    pipeline_body_vs: Option<&str>,
    pipeline_body_fs: Option<&str>,
    pipeline_body_cs: Option<&str>,
    vs_streams: Vec<StreamDef>,
    bindings: Vec<BindingDef>,
) -> String { ... }
```

内部逻辑：
1. `create_composer()` 获取含 pbr_common 的 ShaderComposer
2. 用 `ShaderModuleBuilder` 创建 pipeline 模块（body + streams + bindings）
3. `composer.register_module(pipeline_module)`
4. `composer.compose(name, &[CompositionOp::Mixin("pbr_common".into())], name)`
5. `WgslGenerator::generate()` 输出完整 WGSL

### 1.2 新增便捷函数

为每个管线 shader 创建专用函数（保留 `include_str!` 读取 .wgsl body）：

```rust
/// Generate complete Forward+ shader (pbr_common + forward_plus entry points)
pub fn generate_forward_plus_wgsl() -> String { ... }

/// Generate complete cluster culling compute shader
pub fn generate_cluster_culling_wgsl() -> String { ... }

/// Generate complete Deferred+ geometry shader
pub fn generate_deferred_geometry_wgsl() -> String { ... }

/// Generate complete Deferred+ lighting shader
pub fn generate_deferred_lighting_wgsl() -> String { ... }

/// Generate complete Forward+ instanced shader (base + instancing StageChain)
pub fn generate_forward_plus_instanced_wgsl() -> String { ... }
```

### 1.3 instancing StageChain 实现

`generate_forward_plus_instanced_wgsl()` 使用 StageChain 组合：
1. 注册 `forward_plus_base` 模块（包含 base vertex_body + fragment_body）
2. 注册 `forward_instancing` 模块（仅包含 instancing vertex_body）
3. compose 时同时使用 Mixin + StageChain：
   ```rust
   composer.compose("forward_instanced", &[
       CompositionOp::Mixin("pbr_common".into()),
       CompositionOp::StageChain {
           stage: ShaderStage::Vertex,
           modules: vec!["forward_instancing".into()],
       },
   ], "forward_instanced")
   ```

### 1.4 新增测试

- 每个 `generate_*_wgsl()` 函数的 naga 验证测试
- 对比测试：生成 WGSL vs 原 format! 拼接的语义等价性（复用 L660-759 的交叉验证模式）
- StageChain instancing 测试：验证 base body + instancing body 链式执行

---

## Task 2: 重构 forward_plus.rs — 消除 cfg 双分支

**文件**: `crates/render/src/forward_plus.rs`

### 2.1 替换 shader 加载逻辑 (L94-115)

删除 18 处 cfg 块中的 forward_plus 部分（约 8 处），替换为：

```rust
#[cfg(feature = "use-shader-framework")]
let forward_src = crate::shader_library::generate_forward_plus_wgsl();
#[cfg(not(feature = "use-shader-framework"))]
let forward_src = format!("{PBR_COMMON}\n{FORWARD_PLUS_WGSL}");

#[cfg(feature = "use-shader-framework")]
let culling_src = crate::shader_library::generate_cluster_culling_wgsl();
#[cfg(not(feature = "use-shader-framework"))]
let culling_src = format!("{PBR_COMMON}\n{CLUSTER_CULLING_WGSL}");
```

### 2.2 替换 instanced shader 加载 (L204-211)

```rust
#[cfg(feature = "use-shader-framework")]
let instanced_src = crate::shader_library::generate_forward_plus_instanced_wgsl();
#[cfg(not(feature = "use-shader-framework"))]
let instanced_src = format!("{PBR_COMMON}\n{FORWARD_PLUS_INSTANCED_WGSL}");
```

### 2.3 保留 include_str! 常量

保留 `FORWARD_PLUS_WGSL`、`CLUSTER_CULLING_WGSL`、`FORWARD_PLUS_INSTANCED_WGSL` 常量（旧路径仍需要），但 `PBR_COMMON` 仅在 `#[cfg(not(feature = "use-shader-framework"))]` 下保留。

---

## Task 3: 重构 deferred_plus.rs — 同步迁移

**文件**: `crates/render/src/deferred_plus.rs`

### 3.1 替换 shader 加载逻辑 (L102-132)

与 Task 2 完全对称的模式：

```rust
#[cfg(feature = "use-shader-framework")]
let geometry_src = crate::shader_library::generate_deferred_geometry_wgsl();
#[cfg(not(feature = "use-shader-framework"))]
let geometry_src = format!("{PBR_COMMON}\n{DEFERRED_GEOMETRY_WGSL}");

// lighting_src, culling_src 同理
```

共替换 6 处 cfg 块。

---

## Task 4: 验证与回归测试

### 4.1 编译验证
```bash
# 默认构建（不启用 use-shader-framework）
cd crates/render && cargo check

# shader_framework 路径构建
cd crates/render && cargo check --features use-shader-framework

# 下游 crate 编译
cd desktop && cargo check
cd server && cargo check
```

### 4.2 单元测试
```bash
cd crates/render && cargo test --features use-shader-framework
cd crates/shader_framework && cargo test
```

### 4.3 语义等价性验证
- shader_library.rs 中新增的 naga 验证测试确认生成 WGSL 可被 GPU 编译器接受
- 交叉对比测试确认 `generate_forward_plus_wgsl()` 输出与原 `format!("{pbr_common}\n{forward_plus}")` 语义等价

---

## Dependencies

```
Task 1 (shader_library 扩展)
  ├── Task 2 (forward_plus 重构) ── depends on Task 1
  ├── Task 3 (deferred_plus 重构) ── depends on Task 1, parallel with Task 2
  └── Task 4 (验证) ── depends on Task 2 + Task 3
```

---

## Risks and Mitigations

| 风险 | 严重度 | 缓解措施 |
|------|--------|----------|
| compose_pipeline_shader 生成的 WGSL 与 format! 拼接语义不一致 | 高 | naga 验证 + 交叉对比测试；保留 cfg fallback 路径 |
| ShaderModule 的 streams/bindings 声明与 .wgsl 文件中的 @group/@location 不匹配 | 高 | 从 .wgsl 文件中提取 stream/binding 元数据时严格对照原文件；naga 编译时会捕获不一致 |
| StageChain instancing 中 base body 和 instancing body 的变量传递 | 中 | StageChain 的 `{ }` block scope 可读写外层 `output`/`input`，通过现有单元测试验证 |
| pbr_common 元数据与 .wgsl 文件漂移 | 中 | 已有 `generated_wgsl_semantically_equivalent_to_original` 测试覆盖（L660-759） |
| 下游 crate 行为变化 | 极低 | editor/game_runtime 不启用 use-shader-framework，完全不受影响 |

---

## Rejected Alternatives

### 1. TOML Effect 文件驱动
Alex 提议创建 `effects/forward_plus.effect.toml` 声明式配置。**否决原因**：增加运行时 TOML 解析开销（虽仅初始化时），且当前管线配置较为固定，Rust API 足够。TOML effect 更适合需要热加载或用户可配置的场景，留作后续按需引入。

### 2. BindGroupLayout 自动推导工厂
Sam 提议从 ShaderBinding 自动推导 BindGroupLayout entries。**否决原因**：当前 BindGroupLayout 硬编码在各管线的 `new()` 方法中，自动推导需要大规模重构，风险过高且与 StageChain 重构无直接关系。

### 3. RenderGraph 接管管线编排
Sam 提议将 Forward+/Deferred+ 封装为 RenderPassNode。**否决原因**：RenderGraph 是完全独立的架构改进，与 shader 组合系统正交。混合在一起会大幅增加变更范围和风险。

### 4. 移除 `use-shader-framework` feature gate
Alex 暗示可以统一为始终使用 shader_framework。**否决原因**：当前无下游 crate 启用此 feature，贸然移除会改变默认构建行为。保留 feature gate 作为安全回退是更稳妥的选择。

### 5. 后处理管线迁移
多个方案提及迁移 post_pipeline 的 6 个独立 shader。**否决原因**：这些 shader 不依赖 pbr_common，当前 `include_str!` 直接加载已经足够简洁，迁移到 shader_framework 无实际收益，仅增加代码量。
