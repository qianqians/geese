//! Shadow pass 渲染图节点。

use crate::common::{GpuResourceCache, WgpuRenderCommand};
use crate::graph::{RenderGraphContext, RenderPassNode, ResourceId};
use crate::shadow_pass::{ShadowPass, WgpuShadowAtlas};

/// Shadow depth-only 渲染节点。
///
/// 将场景中的 shadow caster 渲染到 shadow atlas 的各 cascade 区域。
/// 输出 `shadow_atlas` 资源供后续 forward pass 采样。
pub struct ShadowPassNode<'a> {
    pub shadow_pass: &'a ShadowPass,
    pub shadow_atlas: &'a WgpuShadowAtlas,
    pub cache: &'a GpuResourceCache,
    pub commands: &'a [WgpuRenderCommand],
}

impl<'a> RenderPassNode<'a> for ShadowPassNode<'a> {
    fn name(&self) -> &str {
        "shadow"
    }

    fn outputs(&self) -> Vec<ResourceId> {
        vec![ResourceId::new("shadow_atlas")]
    }

    fn execute(&self, _ctx: &RenderGraphContext<'_>, encoder: &mut wgpu::CommandEncoder) {
        // 短期：graph 路径下不传 timestamp_writes，profiling 仅统计整帧
        self.shadow_pass
            .render(encoder, &self.shadow_atlas.view, self.cache, self.commands, None);
    }
}
