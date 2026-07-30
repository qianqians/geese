//! Forward+ 渲染管线的 RenderPassNode 实现。
//!
//! 将 `ForwardPlusPipeline::render()` 中的各阶段拆分为独立的图节点，
//! 由 `RenderGraphBuilder` 声明式编排执行。

#[cfg(feature = "render-graph")]
mod cluster_culling_node;
#[cfg(feature = "render-graph")]
mod deferred_geometry_node;
#[cfg(feature = "render-graph")]
mod deferred_lighting_node;
#[cfg(feature = "render-graph")]
mod forward_node;
#[cfg(feature = "render-graph")]
mod shadow_node;

#[cfg(feature = "render-graph")]
pub use cluster_culling_node::ClusterCullingNode;
#[cfg(feature = "render-graph")]
pub use deferred_geometry_node::{DeferredGeometryNode, GBufferViews};
#[cfg(feature = "render-graph")]
pub use deferred_lighting_node::DeferredLightingNode;
#[cfg(feature = "render-graph")]
pub use forward_node::ForwardRenderNode;
#[cfg(feature = "render-graph")]
pub use shadow_node::ShadowPassNode;

#[cfg(feature = "render-graph")]
use crate::common::{GpuResourceCache, WgpuRenderCommand};
#[cfg(feature = "render-graph")]
use crate::shadow_pass::{ShadowPass, WgpuShadowAtlas};

/// render() 调用期间有效的只读引用集合。
///
/// 所有字段均为 Send + Sync（wgpu 资源 + 纯数据），
/// 用于在不持有整个 `ForwardPlusPipeline` 的情况下向各 Node 提供渲染所需数据。
/// 这规避了 `GpuProfiler` 中 `Cell`/`RefCell` 导致的非 Sync 问题。
#[cfg(feature = "render-graph")]
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
