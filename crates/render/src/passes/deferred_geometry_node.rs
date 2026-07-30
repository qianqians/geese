//! Deferred geometry pass 渲染图节点。

use crate::common::{GpuResourceCache, WgpuRenderCommand};
use crate::graph::{RenderGraphContext, RenderPassNode, ResourceId};

/// G-Buffer 纹理视图引用集合。
pub struct GBufferViews<'a> {
    pub base: &'a wgpu::TextureView,
    pub normal: &'a wgpu::TextureView,
    pub emissive: &'a wgpu::TextureView,
    pub depth: &'a wgpu::TextureView,
}

/// Deferred geometry pass 节点。
///
/// 将场景 mesh 渲染到 G-Buffer（base color + normal + emissive + depth）。
pub struct DeferredGeometryNode<'a> {
    pub geometry_pipeline: &'a wgpu::RenderPipeline,
    pub geometry_frame_bind_group: &'a wgpu::BindGroup,
    pub commands: &'a [WgpuRenderCommand],
    pub cache: &'a GpuResourceCache,
    pub gbuffer_views: GBufferViews<'a>,
}

impl<'a> RenderPassNode<'a> for DeferredGeometryNode<'a> {
    fn name(&self) -> &str {
        "deferred_geometry"
    }

    fn outputs(&self) -> Vec<ResourceId> {
        vec![
            ResourceId::new("gbuffer_base"),
            ResourceId::new("gbuffer_normal"),
            ResourceId::new("gbuffer_emissive"),
            ResourceId::new("gbuffer_depth"),
        ]
    }

    fn execute(&self, _ctx: &RenderGraphContext<'_>, encoder: &mut wgpu::CommandEncoder) {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("deferred+ geometry pass"),
            color_attachments: &[
                Some(wgpu::RenderPassColorAttachment {
                    view: self.gbuffer_views.base,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                }),
                Some(wgpu::RenderPassColorAttachment {
                    view: self.gbuffer_views.normal,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.5,
                            g: 0.5,
                            b: 1.0,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                }),
                Some(wgpu::RenderPassColorAttachment {
                    view: self.gbuffer_views.emissive,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                }),
            ],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: self.gbuffer_views.depth,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
        });

        pass.set_pipeline(self.geometry_pipeline);
        pass.set_bind_group(0, self.geometry_frame_bind_group, &[]);

        for command in self.commands {
            let mesh = match self.cache.mesh_buffers.get(&command.mesh_key) {
                Some(m) => m,
                None => continue,
            };
            pass.set_bind_group(1, &command.material_bind_group, &[]);
            pass.set_bind_group(2, &command.object_bind_group, &[]);
            pass.set_vertex_buffer(0, mesh.vertex_buffer.slice(..));
            pass.set_index_buffer(mesh.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
            pass.draw_indexed(0..command.index_count, 0, 0..1);
        }
    }
}
