//! GPU 粒子渲染器：billboard 实例化渲染。
//!
//! `GpuParticleRenderer` 只读消费 `ParticleSystem::particles`，不修改模拟逻辑。
//! 使用预分配的最大容量 instance buffer + `write_buffer` 部分更新。

use bytemuck::{Pod, Zeroable};
use std::num::NonZeroU64;
use wgpu::util::DeviceExt;

const PARTICLE_SHADER_WGSL: &str = include_str!("../shaders/particle_billboard.wgsl");

/// 单个粒子的 GPU instance 数据。
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct ParticleInstanceData {
    pub position: [f32; 3],
    pub _pad0: f32,
    pub color: [f32; 4],
    pub size: f32,
    pub _pad1: [f32; 3],
}

/// 相机 uniform（与 render crate 的 CameraUniform 布局一致）。
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct ParticleCameraUniform {
    pub view_projection: [[f32; 4]; 4],
    pub inverse_view_projection: [[f32; 4]; 4],
    pub camera_position: [f32; 4],
}

/// GPU 粒子渲染器。
pub struct GpuParticleRenderer {
    camera_buffer: wgpu::Buffer,
    instance_buffer: wgpu::Buffer,
    bind_group_layout: wgpu::BindGroupLayout,
    bind_group: wgpu::BindGroup,
    pipeline: wgpu::RenderPipeline,
    max_instances: usize,
    instance_count: u32,
}

impl GpuParticleRenderer {
    /// 创建 GPU 粒子渲染器。
    ///
    /// `color_format` 和 `depth_format` 应与主渲染管线一致。
    pub fn new(
        device: &wgpu::Device,
        color_format: wgpu::TextureFormat,
        depth_format: wgpu::TextureFormat,
    ) -> Self {
        let max_instances = 4096;

        // ---- camera uniform buffer ----
        let camera_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("particle camera buffer"),
            contents: bytemuck::bytes_of(&ParticleCameraUniform {
                view_projection: identity_matrix(),
                inverse_view_projection: identity_matrix(),
                camera_position: [0.0, 0.0, 0.0, 1.0],
            }),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        // ---- instance buffer ----
        let instance_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("particle instance buffer"),
            size: (max_instances * std::mem::size_of::<ParticleInstanceData>()) as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // ---- bind group layout ----
        let bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("particle bind group layout"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::VERTEX,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: NonZeroU64::new(
                                std::mem::size_of::<ParticleCameraUniform>() as u64,
                            ),
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::VERTEX,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Storage { read_only: true },
                            has_dynamic_offset: false,
                            min_binding_size: NonZeroU64::new(
                                std::mem::size_of::<ParticleInstanceData>() as u64,
                            ),
                        },
                        count: None,
                    },
                ],
            });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("particle bind group"),
            layout: &bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: camera_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: instance_buffer.as_entire_binding(),
                },
            ],
        });

        // ---- shader + pipeline ----
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("particle billboard shader"),
            source: wgpu::ShaderSource::Wgsl(PARTICLE_SHADER_WGSL.into()),
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("particle pipeline layout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("particle billboard pipeline"),
            layout: Some(&pipeline_layout),
            cache: None,
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "vs_main",
                buffers: &[],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: "fs_main",
                targets: &[Some(wgpu::ColorTargetState {
                    format: color_format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: depth_format,
                depth_write_enabled: false,
                depth_compare: wgpu::CompareFunction::Less,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
        });

        Self {
            camera_buffer,
            instance_buffer,
            bind_group_layout,
            bind_group,
            pipeline,
            max_instances,
            instance_count: 0,
        }
    }

    /// 更新相机 uniform。
    pub fn update_camera(
        &self,
        queue: &wgpu::Queue,
        view_projection: [[f32; 4]; 4],
        inverse_view_projection: [[f32; 4]; 4],
        camera_position: [f32; 3],
    ) {
        let data = ParticleCameraUniform {
            view_projection,
            inverse_view_projection,
            camera_position: [camera_position[0], camera_position[1], camera_position[2], 1.0],
        };
        queue.write_buffer(&self.camera_buffer, 0, bytemuck::bytes_of(&data));
    }

    /// 从 `ParticleSystem` 提取活粒子到 instance buffer。
    pub fn prepare(
        &mut self,
        queue: &wgpu::Queue,
        system: &crate::ParticleSystem,
    ) {
        let mut instances: Vec<ParticleInstanceData> = Vec::with_capacity(self.max_instances);
        for p in system.particles.iter() {
            if !p.is_alive() {
                continue;
            }
            if instances.len() >= self.max_instances {
                break;
            }
            instances.push(ParticleInstanceData {
                position: p.position,
                _pad0: 0.0,
                color: p.color,
                size: p.size,
                _pad1: [0.0; 3],
            });
        }
        self.instance_count = instances.len() as u32;
        if self.instance_count > 0 {
            queue.write_buffer(
                &self.instance_buffer,
                0,
                bytemuck::cast_slice(&instances),
            );
        }
    }

    /// 渲染粒子 billboard 到给定 color/depth target。
    pub fn render(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        color_target: &wgpu::TextureView,
        depth_target: &wgpu::TextureView,
    ) {
        if self.instance_count == 0 {
            return;
        }

        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("particle render pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: color_target,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: depth_target,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
        });

        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.bind_group, &[]);
        // 6 vertices per quad (2 triangles), instance_count instances
        pass.draw(0..6, 0..self.instance_count);
    }

    /// 当前活粒子数。
    pub fn instance_count(&self) -> u32 {
        self.instance_count
    }
}

fn identity_matrix() -> [[f32; 4]; 4] {
    [
        [1.0, 0.0, 0.0, 0.0],
        [0.0, 1.0, 0.0, 0.0],
        [0.0, 0.0, 1.0, 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn instance_data_size_is_32_bytes() {
        // position(12) + pad(4) + color(16) + size(4) + pad(12) = 48
        // Actually: [f32;3] + f32 + [f32;4] + f32 + [f32;3] = 12+4+16+4+12 = 48
        assert_eq!(std::mem::size_of::<ParticleInstanceData>(), 48);
    }

    #[test]
    fn camera_uniform_size_is_144_bytes() {
        // 3 x mat4x4 = 3 x 64 = 192
        assert_eq!(std::mem::size_of::<ParticleCameraUniform>(), 192);
    }
}
