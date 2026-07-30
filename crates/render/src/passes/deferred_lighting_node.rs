//! Deferred lighting pass 渲染图节点。

use crate::graph::{RenderGraphContext, RenderPassNode, ResourceId};

/// Deferred lighting pass 节点。
///
/// 全屏三角形从 G-Buffer + cluster bitmask 还原光照，输出到 final color target。
pub struct DeferredLightingNode<'a> {
    pub lighting_pipeline: &'a wgpu::RenderPipeline,
    pub lighting_frame_bind_group: &'a wgpu::BindGroup,
    pub gbuffer_bind_group: &'a wgpu::BindGroup,
}

impl<'a> RenderPassNode<'a> for DeferredLightingNode<'a> {
    fn name(&self) -> &str {
        "deferred_lighting"
    }

    fn inputs(&self) -> Vec<ResourceId> {
        vec![
            ResourceId::new("gbuffer_base"),
            ResourceId::new("gbuffer_normal"),
            ResourceId::new("gbuffer_emissive"),
            ResourceId::new("gbuffer_depth"),
            ResourceId::new("cluster_bitmask"),
        ]
    }

    fn outputs(&self) -> Vec<ResourceId> {
        vec![ResourceId::new("color_target")]
    }

    fn execute(&self, ctx: &RenderGraphContext<'_>, encoder: &mut wgpu::CommandEncoder) {
        let color_target = match ctx.color_target {
            Some(t) => t,
            None => {
                log::error!("[deferred_lighting] Missing color target, skipping lighting pass");
                return;
            }
        };

        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("deferred+ lighting pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: color_target,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: 0.02,
                        g: 0.02,
                        b: 0.03,
                        a: 1.0,
                    }),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });

        pass.set_pipeline(self.lighting_pipeline);
        pass.set_bind_group(0, self.lighting_frame_bind_group, &[]);
        pass.set_bind_group(1, self.gbuffer_bind_group, &[]);
        pass.draw(0..3, 0..1);
    }
}
