//! GPU Shadow Pass：CSM（Cascaded Shadow Map）wgpu 实现。
//!
//! - [`WgpuShadowAtlas`]：实现 [`crate::shadow::ShadowAtlas`] trait，持有 depth atlas 纹理 + CSM uniform buffer。
//! - [`ShadowPass`]：depth-only render pipeline，逐 cascade 渲染 shadow casters 到 atlas 各区域。
//!
//! 复用 `compute_cascade_splits()` + `compute_atlas_layout()`（shadow.rs 已有）。
//! Shadow atlas 使用独立 bind group（group 0），不修改 ForwardPlusPipeline 的 bind group layout。

use std::num::NonZeroU64;

use bytemuck::{Pod, Zeroable};

use crate::common::{GpuResourceCache, GpuVertex, WgpuRenderCommand};
use crate::shadow::{
    compute_atlas_layout, CsmUniform, ShadowAtlas, MAX_CASCADES, CascadeConfig,
};

const SHADOW_DEPTH_WGSL: &str = include_str!("../shaders/shadow_depth.wgsl");

/// 单 cascade 的 VP 矩阵（std140 对齐：64 bytes）。
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct CascadeVp {
    pub view_proj: [[f32; 4]; 4],
}

// ---------------------------------------------------------------------------
// WgpuShadowAtlas
// ---------------------------------------------------------------------------

/// GPU 阴影 atlas：持有 depth 纹理 + CSM uniform buffer。
///
/// 实现 [`ShadowAtlas`] trait，替代 [`crate::shadow::NullShadowAtlas`]。
pub struct WgpuShadowAtlas {
    /// Depth atlas 纹理（Depth32Float）
    pub texture: wgpu::Texture,
    /// Atlas 纹理视图（用于 shadow pass 的 depth attachment）
    pub view: wgpu::TextureView,
    /// CSM uniform buffer（存储 cascade VPs + params）
    pub csm_uniform_buffer: wgpu::Buffer,
    /// 级联配置
    pub config: CascadeConfig,
    /// 上传次数（用于测试/诊断）
    pub upload_count: usize,
    /// Atlas 布局（各 cascade 区域）
    pub atlas_layout: Vec<crate::shadow::AtlasRect>,
}

impl WgpuShadowAtlas {
    /// 创建 shadow atlas。
    pub fn new(
        device: &wgpu::Device,
        config: &CascadeConfig,
    ) -> Self {
        let resolution = config.atlas_resolution;
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("shadow atlas depth"),
            size: wgpu::Extent3d {
                width: resolution * 2,
                height: resolution * 2,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth32Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });

        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());

        let csm_uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("shadow csm uniform buffer"),
            size: std::mem::size_of::<CsmUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let atlas_layout = compute_atlas_layout(
            config.count.clamp(1, MAX_CASCADES),
            resolution,
        );

        Self {
            texture,
            view,
            csm_uniform_buffer,
            config: config.clone(),
            upload_count: 0,
            atlas_layout,
        }
    }

    /// 返回 shadow atlas 纹理视图（用于 forward pass 采样阴影）。
    pub fn view(&self) -> &wgpu::TextureView {
        &self.view
    }

    /// 返回 atlas 分辨率。
    pub fn resolution(&self) -> u32 {
        self.config.atlas_resolution
    }
}

impl ShadowAtlas for WgpuShadowAtlas {
    fn cascade_count(&self) -> usize {
        self.config.count.clamp(1, MAX_CASCADES)
    }

    fn upload(&mut self, _uniform: &CsmUniform) {
        self.upload_count += 1;
        // 实际 GPU 上传在 queue.write_buffer 中完成，这里只记录。
        // 调用方通过 write_csm_uniform() 写入 GPU buffer。
    }
}

impl WgpuShadowAtlas {
    /// 将 CSM uniform 写入 GPU buffer。
    pub fn write_csm_uniform(&self, queue: &wgpu::Queue, uniform: &CsmUniform) {
        queue.write_buffer(
            &self.csm_uniform_buffer,
            0,
            bytemuck::bytes_of(uniform),
        );
    }
}

// ---------------------------------------------------------------------------
// ShadowPass
// ---------------------------------------------------------------------------

/// GPU Shadow Pass：depth-only render pipeline。
///
/// 逐 cascade 设置 viewport + VP uniform，渲染所有 shadow casters 到 atlas 对应区域。
/// 复用 ForwardPlusPipeline 的 object bind group（group 1）和 mesh buffers。
pub struct ShadowPass {
    pipeline: wgpu::RenderPipeline,
    /// Per-cascade VP uniform buffer（动态偏移，64 bytes × MAX_CASCADES）
    vp_buffer: wgpu::Buffer,
    /// Shadow VP bind group（group 0，动态偏移）
    bind_group: wgpu::BindGroup,
    bind_group_layout: wgpu::BindGroupLayout,
    config: CascadeConfig,
    atlas_layout: Vec<crate::shadow::AtlasRect>,
}

impl ShadowPass {
    /// 创建 ShadowPass。
    ///
    /// `object_bind_group_layout` 应与 ForwardPlusPipeline 的 group 2 layout 一致，
    /// 以便复用已创建的 object bind groups。
    pub fn new(
        device: &wgpu::Device,
        config: &CascadeConfig,
        object_bind_group_layout: &wgpu::BindGroupLayout,
    ) -> Self {
        // ---- shadow VP bind group layout (group 0, dynamic offset) ----
        let bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("shadow vp bind group layout"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: true,
                        min_binding_size: NonZeroU64::new(std::mem::size_of::<CascadeVp>() as u64),
                    },
                    count: None,
                }],
            });

        // ---- VP uniform buffer (MAX_CASCADES × 64 bytes) ----
        let vp_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("shadow vp uniform buffer"),
            size: (MAX_CASCADES * std::mem::size_of::<CascadeVp>()) as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // Initialize with identity matrices
        let init_data = [CascadeVp {
            view_proj: [
                [1.0, 0.0, 0.0, 0.0],
                [0.0, 1.0, 0.0, 0.0],
                [0.0, 0.0, 1.0, 0.0],
                [0.0, 0.0, 0.0, 1.0],
            ],
        }; MAX_CASCADES];
        // Note: buffer initialized via queue.write_buffer in update_cascade_vps().
        // Initial contents are zeroed; first update_cascade_vps() call fills them.
        let _ = init_data;

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("shadow vp bind group"),
            layout: &bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: vp_buffer.as_entire_binding(),
            }],
        });

        // ---- shader + pipeline ----
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("shadow depth shader"),
            source: wgpu::ShaderSource::Wgsl(SHADOW_DEPTH_WGSL.into()),
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("shadow pipeline layout"),
            bind_group_layouts: &[&bind_group_layout, object_bind_group_layout],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("shadow depth pipeline"),
            layout: Some(&pipeline_layout),
            cache: None,
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "vs_main",
                buffers: &[GpuVertex::layout()],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: None, // depth-only, no fragment
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                cull_mode: Some(wgpu::Face::Front), // Reversed culling for shadow maps
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::LessEqual,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState {
                    constant: config.depth_bias as i32,
                    slope_scale: 1.5,
                    clamp: 0.0,
                },
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
        });

        let atlas_layout = compute_atlas_layout(
            config.count.clamp(1, MAX_CASCADES),
            config.atlas_resolution,
        );

        Self {
            pipeline,
            vp_buffer,
            bind_group,
            bind_group_layout,
            config: config.clone(),
            atlas_layout,
        }
    }

    /// 返回 shadow VP bind group layout（用于创建额外 bind groups）。
    pub fn bind_group_layout(&self) -> &wgpu::BindGroupLayout {
        &self.bind_group_layout
    }

    /// 写入各 cascade 的 VP 矩阵到 GPU buffer。
    ///
    /// 在 `render()` 前调用。
    pub fn update_cascade_vps(
        &self,
        queue: &wgpu::Queue,
        cascade_vps: &[[[f32; 4]; 4]],
    ) {
        let mut vps = [CascadeVp {
            view_proj: crate::common::identity_matrix(),
        }; MAX_CASCADES];
        for (i, vp) in cascade_vps.iter().enumerate().take(MAX_CASCADES) {
            vps[i] = CascadeVp { view_proj: *vp };
        }
        queue.write_buffer(&self.vp_buffer, 0, bytemuck::bytes_of(&vps));
    }

    /// 渲染所有 shadow casters 到 shadow atlas。
    ///
    /// 在 ForwardPlusPipeline::render() 的 forward pass 之前调用。
    ///
    /// `timestamp_writes` 可传入 GPU profiler 的 timestamp query 对（启用 profiling 时）。
    pub fn render(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        atlas_view: &wgpu::TextureView,
        cache: &GpuResourceCache,
        commands: &[WgpuRenderCommand],
        timestamp_writes: Option<wgpu::RenderPassTimestampWrites>,
    ) {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("shadow depth render pass"),
            color_attachments: &[],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: atlas_view,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            timestamp_writes,
            occlusion_query_set: None,
        });

        pass.set_pipeline(&self.pipeline);

        let vp_size = std::mem::size_of::<CascadeVp>() as u32;
        for (cascade_idx, rect) in self.atlas_layout.iter().enumerate() {
            pass.set_viewport(
                rect.offset[0] as f32,
                rect.offset[1] as f32,
                rect.extent[0] as f32,
                rect.extent[1] as f32,
                0.0,
                1.0,
            );
            pass.set_bind_group(0, &self.bind_group, &[(cascade_idx as u32) * vp_size]);

            for command in commands {
                let mesh = match cache.mesh_buffers.get(&command.mesh_key) {
                    Some(m) => m,
                    None => continue,
                };
                pass.set_bind_group(1, &command.object_bind_group, &[]);
                pass.set_vertex_buffer(0, mesh.vertex_buffer.slice(..));
                pass.set_index_buffer(
                    mesh.index_buffer.slice(..),
                    wgpu::IndexFormat::Uint32,
                );
                pass.draw_indexed(0..command.index_count, 0, 0..1);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cascade_vp_size_is_64_bytes() {
        assert_eq!(std::mem::size_of::<CascadeVp>(), 64);
    }

    #[test]
    fn atlas_layout_matches_config() {
        let cfg = CascadeConfig::default();
        let layout = compute_atlas_layout(cfg.count, cfg.atlas_resolution);
        assert_eq!(layout.len(), cfg.count.min(MAX_CASCADES));
    }
}
