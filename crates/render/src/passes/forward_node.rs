//! Forward render pass 渲染图节点。

use crate::common::{GpuResourceCache, WgpuRenderCommand};
use crate::graph::{RenderGraphContext, RenderPassNode, ResourceId};

/// Forward+ 主渲染节点。
///
/// 执行最终的 color + depth render pass：绑定帧级 uniform、遍历绘制命令、
/// 支持 GPU Instancing 路径。从 `RenderGraphContext` 获取 color/depth target。
pub struct ForwardRenderNode<'a> {
    pub forward_pipeline: &'a wgpu::RenderPipeline,
    pub frame_bind_group: &'a wgpu::BindGroup,
    pub commands: &'a [WgpuRenderCommand],
    pub cache: &'a GpuResourceCache,
    #[cfg(feature = "instancing")]
    pub instanced_pipeline: &'a wgpu::RenderPipeline,
    #[cfg(feature = "instancing")]
    pub instance_bind_group: &'a wgpu::BindGroup,
}

impl<'a> RenderPassNode<'a> for ForwardRenderNode<'a> {
    fn name(&self) -> &str {
        "forward_main"
    }

    fn inputs(&self) -> Vec<ResourceId> {
        vec![
            ResourceId::new("cluster_bitmask"),
            ResourceId::new("shadow_atlas"),
        ]
    }

    fn outputs(&self) -> Vec<ResourceId> {
        vec![ResourceId::new("color_target")]
    }

    fn execute(&self, ctx: &RenderGraphContext<'_>, encoder: &mut wgpu::CommandEncoder) {
        let color_target = match ctx.color_target {
            Some(t) => t,
            None => {
                log::error!("[forward_main] Missing color target, skipping forward+ render pass");
                return;
            }
        };
        let depth = match ctx.depth_target {
            Some(t) => t,
            None => {
                log::error!("[forward_main] Missing depth target, skipping forward+ render pass");
                return;
            }
        };

        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("forward+ render pass"),
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
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: depth,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
        });

        pass.set_pipeline(self.forward_pipeline);
        pass.set_bind_group(0, self.frame_bind_group, &[]);

        for command in self.commands {
            let mesh = match self.cache.mesh_buffers.get(&command.mesh_key) {
                Some(m) => m,
                None => continue,
            };
            pass.set_bind_group(1, &command.material_bind_group, &[]);
            pass.set_vertex_buffer(0, mesh.vertex_buffer.slice(..));
            pass.set_index_buffer(mesh.index_buffer.slice(..), wgpu::IndexFormat::Uint32);

            // Instanced path: 当 instance_count > 1 时使用 instanced pipeline
            #[cfg(feature = "instancing")]
            {
                if command.instance_count > 1 {
                    pass.set_pipeline(self.instanced_pipeline);
                    pass.set_bind_group(2, self.instance_bind_group, &[]);
                    pass.draw_indexed(0..command.index_count, 0, 0..command.instance_count);
                    pass.set_pipeline(self.forward_pipeline);
                    continue;
                }
            }

            // Regular path: 单实例，使用 Object uniform
            pass.set_bind_group(2, &command.object_bind_group, &[]);
            pass.draw_indexed(0..command.index_count, 0, 0..1);
        }
    }
}
