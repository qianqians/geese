# ForwardPlus RenderGraph 重构计划

## 背景

当前 `ForwardPlusPipeline::render()`（[forward_plus.rs#L547-L656](file:///d:/Personal/lib/geese/crates/render/src/forward_plus.rs#L547-L656)）是一个单体方法，包含 3 个顺序 GPU 阶段：
1. **Shadow Pass**（L559-565）：可选，depth-only 渲染到 `WgpuShadowAtlas`
2. **Cluster Culling**（L568-582）：compute pass，写 `cluster_bitmask_buffer`
3. **Forward Render**（L584-656）：主 color+depth pass，读取 bitmask + shadow atlas

目标：将这三个阶段提取为独立 `RenderPassNode` 实现，由 `RenderGraphBuilder` 编排执行，`ScenePipeline` trait 签名不变。

## 关键技术约束

| 约束 | 来源 | 影响 |
|------|------|------|
| `RenderPassNode: Send + Sync` | [graph.rs#L62](file:///d:/Personal/lib/geese/crates/render/src/graph.rs#L62) | Node 不能持有 `&ForwardPlusPipeline`（含非 Sync 的 `GpuProfiler`） |
| `execute(&self, ctx, encoder)` | [graph.rs#L77](file:///d:/Personal/lib/geese/crates/render/src/graph.rs#L77) | 执行时不可变借用，profiler 需外部处理 |
| `render(&self, ...)` | [pipeline.rs#L111](file:///d:/Personal/lib/geese/crates/render/src/pipeline.rs#L111) | 图在 `&self` 上下文中构建+执行 |
| wgpu 自动 barrier | WebGPU 规范 | `insert_barriers_if_needed` 保持为验证层，无需真正 barrier |
| Feature flags | [Cargo.toml#L17-L24](file:///d:/Personal/lib/geese/crates/render/Cargo.toml#L17-L24) | `profiling`、`instancing` 条件编译需在 Node 中保留 |

## 实现步骤

### Phase 1: 基础设施准备（无行为变更）

**Step 1.1** — 添加 feature flag
- 文件: `crates/render/Cargo.toml`
- 在 `[features]` 中添加 `render-graph = []`

**Step 1.2** — 创建 passes 模块
- 新建: `crates/render/src/passes/mod.rs`
- 新建: `crates/render/src/passes/shadow_node.rs`
- 新建: `crates/render/src/passes/cluster_culling_node.rs`
- 新建: `crates/render/src/passes/forward_node.rs`
- 修改: `crates/render/src/lib.rs` — 添加 `pub mod passes;`

### Phase 2: 定义帧上下文结构体

**Step 2.1** — 在 `passes/mod.rs` 中定义 `ForwardPlusFrameRefs<'a>`

```rust
/// render() 调用期间有效的只读引用集合。
/// 所有字段均为 Send + Sync（wgpu 资源 + 纯数据）。
pub struct ForwardPlusFrameRefs<'a> {
    // 持久 GPU 资源（new() 时创建）
    pub forward_pipeline: &'a wgpu::RenderPipeline,
    pub culling_pipeline: &'a wgpu::ComputePipeline,
    pub frame_bind_group: &'a wgpu::BindGroup,
    pub culling_bind_group: &'a wgpu::BindGroup,
    // 帧级数据（prepare() 后有效）
    pub commands: &'a [WgpuRenderCommand],
    pub cache: &'a GpuResourceCache,
    // Shadow（可选）
    pub shadow_pass: Option<&'a ShadowPass>,
    pub shadow_atlas: Option<&'a WgpuShadowAtlas>,
    // Instancing
    #[cfg(feature = "instancing")]
    pub instanced_pipeline: &'a wgpu::RenderPipeline,
    #[cfg(feature = "instancing")]
    pub instance_bind_group: &'a wgpu::BindGroup,
}
```

**设计理由**：
- 不持有 `&ForwardPlusPipeline` 整体 → 规避 `GpuProfiler` 的 `Cell`/`RefCell` 导致的非 Sync 问题
- 所有 wgpu 资源（`RenderPipeline`、`BindGroup`、`Buffer`）天然 `Send + Sync`
- `GpuResourceCache`（HashMap + wgpu 资源）和 `WgpuRenderCommand`（Vec + wgpu 资源）也是 Sync
- 生命周期 `'a` 绑定到 `render()` 调用期间，无自引用问题

### Phase 3: 实现三个 RenderPassNode

**Step 3.1** — `ShadowPassNode`（`passes/shadow_node.rs`）

```rust
pub struct ShadowPassNode<'a> {
    shadow_pass: &'a ShadowPass,
    shadow_atlas: &'a WgpuShadowAtlas,
    cache: &'a GpuResourceCache,
    commands: &'a [WgpuRenderCommand],
}

impl RenderPassNode for ShadowPassNode<'_> {
    fn name(&self) -> &str { "shadow" }
    fn outputs(&self) -> Vec<ResourceId> { vec![ResourceId::new("shadow_atlas")] }
    fn execute(&self, _ctx: &RenderGraphContext<'_>, encoder: &mut wgpu::CommandEncoder) {
        // 搬运 forward_plus.rs L559-565 逻辑
        // profiling timestamp_writes 通过 ctx 扩展字段或 cfg 条件处理
        self.shadow_pass.render(encoder, &self.shadow_atlas.view, self.cache, self.commands, None);
    }
}
```

**Step 3.2** — `ClusterCullingNode`（`passes/cluster_culling_node.rs`）

```rust
pub struct ClusterCullingNode<'a> {
    culling_pipeline: &'a wgpu::ComputePipeline,
    culling_bind_group: &'a wgpu::BindGroup,
}

impl RenderPassNode for ClusterCullingNode<'_> {
    fn name(&self) -> &str { "cluster_culling" }
    fn inputs(&self) -> Vec<ResourceId> { vec![ResourceId::new("lights_buffer"), ResourceId::new("cluster_uniform")] }
    fn outputs(&self) -> Vec<ResourceId> { vec![ResourceId::new("cluster_bitmask")] }
    fn execute(&self, _ctx: &RenderGraphContext<'_>, encoder: &mut wgpu::CommandEncoder) {
        // 搬运 forward_plus.rs L574-581（6 行核心逻辑）
        let mut cpass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("forward+ cluster culling"),
            timestamp_writes: None,
        });
        cpass.set_pipeline(self.culling_pipeline);
        cpass.set_bind_group(0, self.culling_bind_group, &[]);
        let groups = (TOTAL_CLUSTERS + CULLING_WORKGROUP_SIZE - 1) / CULLING_WORKGROUP_SIZE;
        cpass.dispatch_workgroups(groups, 1, 1);
    }
}
```

**Step 3.3** — `ForwardRenderNode`（`passes/forward_node.rs`）

```rust
pub struct ForwardRenderNode<'a> {
    forward_pipeline: &'a wgpu::RenderPipeline,
    frame_bind_group: &'a wgpu::BindGroup,
    commands: &'a [WgpuRenderCommand],
    cache: &'a GpuResourceCache,
    #[cfg(feature = "instancing")]
    instanced_pipeline: &'a wgpu::RenderPipeline,
    #[cfg(feature = "instancing")]
    instance_bind_group: &'a wgpu::BindGroup,
}

impl RenderPassNode for ForwardRenderNode<'_> {
    fn name(&self) -> &str { "forward_main" }
    fn inputs(&self) -> Vec<ResourceId> {
        vec![ResourceId::new("cluster_bitmask"), ResourceId::new("shadow_atlas")]
    }
    fn outputs(&self) -> Vec<ResourceId> { vec![ResourceId::new("color_target")] }
    fn execute(&self, ctx: &RenderGraphContext<'_>, encoder: &mut wgpu::CommandEncoder) {
        // 搬运 forward_plus.rs L584-656 逻辑
        // 从 ctx.depth_target 获取 depth view，None 时 log + return
        // 从 ctx.color_target 获取 color view
    }
}
```

### Phase 4: 集成到 ScenePipeline::render()

**Step 4.1** — 修改 `forward_plus.rs` 的 `render()` 方法

```rust
fn render(&self, device: &wgpu::Device, encoder: &mut wgpu::CommandEncoder,
          color_target: &wgpu::TextureView, depth_target: Option<&wgpu::TextureView>) {
    #[cfg(feature = "profiling")]
    self.profiler.begin_frame();

    #[cfg(feature = "render-graph")]
    {
        use crate::graph::{RenderGraphBuilder, RenderGraphContext};
        use crate::passes::*;

        let mut builder = RenderGraphBuilder::new();

        // Shadow（可选）
        if let (Some(sp), Some(atlas)) = (&self.shadow_pass, &self.shadow_atlas) {
            builder.add_pass("shadow", Box::new(ShadowPassNode {
                shadow_pass: sp, shadow_atlas: atlas,
                cache: &self.cache, commands: &self.prepared.commands,
            }), &[]);
        }

        // Cluster Culling
        builder.add_pass("cluster_culling", Box::new(ClusterCullingNode {
            culling_pipeline: &self.culling_pipeline,
            culling_bind_group: &self.culling_bind_group,
        }), &[]);

        // Forward Main
        let deps: Vec<&str> = if self.shadow_pass.is_some() {
            vec!["shadow", "cluster_culling"]
        } else {
            vec!["cluster_culling"]
        };
        builder.add_pass("forward_main", Box::new(ForwardRenderNode {
            forward_pipeline: &self.forward_pipeline,
            frame_bind_group: &self.frame_bind_group,
            commands: &self.prepared.commands,
            cache: &self.cache,
            #[cfg(feature = "instancing")]
            instanced_pipeline: &self.instanced_pipeline,
            #[cfg(feature = "instancing")]
            instance_bind_group: &self.instance_bind_group,
        }), &deps);

        let graph = builder.compile().expect("forward+ graph compile");
        let ctx = RenderGraphContext {
            device, queue: /* 需传入或从 self 获取 */,
            color_target: Some(color_target),
            depth_target,
        };
        graph.execute(&ctx, encoder);
    }

    #[cfg(not(feature = "render-graph"))]
    {
        // ... 保留原有 L559-656 代码不变 ...
    }

    #[cfg(feature = "profiling")]
    self.profiler.end_frame(encoder, device);
}
```

**Step 4.2** — 解决 `queue` 传递问题
- 当前 `ScenePipeline::render()` 签名无 `queue` 参数
- 方案 A（推荐）：在 `ForwardPlusPipeline` 中存储 `queue: wgpu::Queue`（wgpu Queue 是 Send+Sync 且廉价 clone）
- 方案 B：扩展 `RenderGraphContext` 使 `queue` 为 `Option`（shadow/culling 不需要 queue）
- 选择方案 A：在 `new()` 时 `self.queue = queue.clone()`

### Phase 5: Profiling 集成

**Step 5.1** — 在 graph 路径中保留 profiling
- `begin_frame()` / `end_frame()` 保持在 `render()` 顶层（graph 外）
- 各 Node 的 `timestamp_writes`：
  - 短期：Node 内 `timestamp_writes: None`（profiling 精度略降）
  - 中期：扩展 `RenderGraphContext` 添加 `profiler: Option<&GpuProfiler>` 字段，Node 从 ctx 获取
  - 注意：`GpuProfiler` 方法接受 `&self`（内部可变性），但 `&GpuProfiler` 不是 Sync → ctx 中用 `Option<*const GpuProfiler>` + unsafe 或改为在 execute 循环中由 `CompiledGraph` 统一注入

**推荐短期方案**：graph 路径下暂不传 timestamp_writes 给各 Node，profiling 仅统计整帧时间。后续再细化 per-pass 计时。

### Phase 6: 测试

**Step 6.1** — 单元测试（`passes/mod.rs` 底部 `#[cfg(test)]`）
- 验证各 Node 的 `name()`、`inputs()`、`outputs()` 声明正确
- 构建完整 forward+ 图（3 节点），验证 `compile()` 成功 + 拓扑序正确
- 构建无 shadow 图（2 节点），验证 compile 成功

**Step 6.2** — 编译验证
```bash
cd crates/render && cargo test                           # 默认（无 render-graph）
cd crates/render && cargo test --features render-graph   # 新路径
cd crates/render && cargo test --features "render-graph,profiling,instancing"  # 全组合
```

**Step 6.3** — 集成验证
```bash
cd desktop && cargo build --features "render/render-graph"  # 编辑器构建
```

### Phase 7: 清理（验证通过后）

**Step 7.1** — 移除 `#[cfg(not(feature = "render-graph"))]` 旧路径代码
**Step 7.2** — 将 `render-graph` 加入 `default = ["render-graph"]`
**Step 7.3** — 为 `DeferredPlusPipeline` 预留 TODO 注释（后续 PR 实施）

## 依赖关系

```
Step 1.1 → Step 1.2 → Step 2.1
Step 2.1 → Step 3.1, 3.2, 3.3（并行）
Step 3.1, 3.2, 3.3 → Step 4.1
Step 4.1 → Step 4.2 → Step 5.1
Step 5.1 → Step 6.1 → Step 6.2 → Step 6.3
Step 6.3 通过 → Step 7.1 → 7.2 → 7.3
```

## 风险与缓解

| 风险 | 严重度 | 缓解 |
|------|--------|------|
| `RenderPassNode: Send + Sync` vs 生命周期节点 | 高 | Node 仅持有 wgpu 资源引用（天然 Sync），不持有整个 pipeline |
| `render()` 签名无 queue 参数 | 中 | Pipeline 内部 clone 存储 queue（wgpu::Queue 是 Arc 包装，clone 廉价） |
| 每帧 graph 构建开销（3×Box + 拓扑排序） | 低 | 3 节点 O(V+E) 排序 ~100ns；后续可缓存 CompiledGraph |
| profiling 在 graph 路径下精度降低 | 低 | 短期接受整帧计时；中期通过 ctx 扩展恢复 per-pass |
| feature flag 组合测试矩阵 | 中 | CI 覆盖 3 个关键组合即可 |
| DeferredPlusPipeline 共享 helper 函数 | 低 | `build_command` 等 helper 保持 `pub(crate)` 不动 |

## 被拒绝的替代方案

| 方案 | 拒绝理由 |
|------|---------|
| 修改 `RenderPassNode` trait 返回 `&[ResourceId]` 替代 `Vec` | 破坏性 API 变更，影响已有测试；3 节点场景下 Vec 分配开销可忽略 |
| 添加 `QueueType` 枚举支持 async compute | 过早优化，wgpu 当前不暴露多队列；预留接口增加无用复杂度 |
| Node 持有 `Arc<ForwardPlusFrameData>`（含持久资源） | 混淆持久资源与帧级数据；每帧 Arc 分配不必要 |
| 永久保留双路径（feature flag 不删除） | 长期维护成本翻倍；验证通过后应统一为 graph 路径 |
| 将 `GpuProfiler` 改为 `Mutex` 内部实现 | 侵入性修改 profiler 模块，影响 DeferredPlus；用 ctx 传递更解耦 |

## 关键文件清单

1. `crates/render/src/forward_plus.rs` — 主要重构目标（render() 分解）
2. `crates/render/src/graph.rs` — RenderPassNode trait + RenderGraphBuilder（可能扩展 Context）
3. `crates/render/src/passes/` (新建目录) — 三个 Node 实现
4. `crates/render/src/pipeline.rs` — ScenePipeline trait（保持不变，验证契约）
5. `crates/render/Cargo.toml` — 添加 render-graph feature
