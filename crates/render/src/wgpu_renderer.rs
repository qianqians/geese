use wgpu::util::DeviceExt;

use crate::{RenderQueue, Vertex};

#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct GpuVertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub uv: [f32; 2],
}

impl GpuVertex {
    pub fn layout<'a>() -> wgpu::VertexBufferLayout<'a> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<GpuVertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[
                wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 0,
                    format: wgpu::VertexFormat::Float32x3,
                },
                wgpu::VertexAttribute {
                    offset: std::mem::size_of::<[f32; 3]>() as wgpu::BufferAddress,
                    shader_location: 1,
                    format: wgpu::VertexFormat::Float32x3,
                },
                wgpu::VertexAttribute {
                    offset: std::mem::size_of::<[f32; 6]>() as wgpu::BufferAddress,
                    shader_location: 2,
                    format: wgpu::VertexFormat::Float32x2,
                },
            ],
        }
    }
}

impl From<&Vertex> for GpuVertex {
    fn from(vertex: &Vertex) -> Self {
        Self {
            position: [vertex.position.x, vertex.position.y, vertex.position.z],
            normal: [vertex.normal.x, vertex.normal.y, vertex.normal.z],
            uv: [vertex.uv.x, vertex.uv.y],
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct CameraUniform {
    pub view_projection: [[f32; 4]; 4],
}

impl CameraUniform {
    pub fn new(view_projection: [[f32; 4]; 4]) -> Self {
        Self { view_projection }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct MaterialUniform {
    pub base_color_factor: [f32; 4],
}

pub struct WgpuSceneRendererDescriptor {
    pub color_format: wgpu::TextureFormat,
    pub depth_format: Option<wgpu::TextureFormat>,
    pub sample_count: u32,
}

pub struct WgpuRenderCommand {
    pub entity_id: String,
    pub vertex_buffer: wgpu::Buffer,
    pub index_buffer: wgpu::Buffer,
    pub index_count: u32,
    material_buffer: wgpu::Buffer,
    material_bind_group: wgpu::BindGroup,
}

pub struct WgpuRenderQueue {
    pub commands: Vec<WgpuRenderCommand>,
}

pub struct WgpuSceneRenderer {
    pipeline: wgpu::RenderPipeline,
    camera_buffer: wgpu::Buffer,
    camera_bind_group: wgpu::BindGroup,
    material_bind_group_layout: wgpu::BindGroupLayout,
}

impl WgpuSceneRenderer {
    pub fn new(device: &wgpu::Device, descriptor: WgpuSceneRendererDescriptor) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("scene mesh shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("../shaders/mesh.wgsl").into()),
        });

        let camera_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("scene camera bind group layout"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
            });

        let material_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("scene material bind group layout"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
            });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("scene pipeline layout"),
            bind_group_layouts: &[&camera_bind_group_layout, &material_bind_group_layout],
            push_constant_ranges: &[],
        });

        let vertex_buffers = [GpuVertex::layout()];
        let color_targets = [Some(wgpu::ColorTargetState {
            format: descriptor.color_format,
            blend: Some(wgpu::BlendState::ALPHA_BLENDING),
            write_mask: wgpu::ColorWrites::ALL,
        })];

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("scene mesh pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "vs_main",
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &vertex_buffers,
            },
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: descriptor
                .depth_format
                .map(|format| wgpu::DepthStencilState {
                    format,
                    depth_write_enabled: true,
                    depth_compare: wgpu::CompareFunction::Less,
                    stencil: wgpu::StencilState::default(),
                    bias: wgpu::DepthBiasState::default(),
                }),
            multisample: wgpu::MultisampleState {
                count: descriptor.sample_count,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: "fs_main",
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &color_targets,
            }),
            multiview: None,
        });

        let camera_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("scene camera buffer"),
            contents: bytemuck::bytes_of(&CameraUniform::new(identity_matrix())),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let camera_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("scene camera bind group"),
            layout: &camera_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: camera_buffer.as_entire_binding(),
            }],
        });

        Self {
            pipeline,
            camera_buffer,
            camera_bind_group,
            material_bind_group_layout,
        }
    }

    pub fn update_camera(&self, queue: &wgpu::Queue, view_projection: [[f32; 4]; 4]) {
        queue.write_buffer(
            &self.camera_buffer,
            0,
            bytemuck::bytes_of(&CameraUniform::new(view_projection)),
        );
    }

    pub fn prepare(&self, device: &wgpu::Device, queue: &RenderQueue<'_>) -> WgpuRenderQueue {
        let commands = queue
            .commands
            .iter()
            .map(|command| {
                let vertices: Vec<_> = command.mesh.vertices.iter().map(GpuVertex::from).collect();

                let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("scene vertex buffer"),
                    contents: bytemuck::cast_slice(&vertices),
                    usage: wgpu::BufferUsages::VERTEX,
                });

                let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("scene index buffer"),
                    contents: bytemuck::cast_slice(&command.mesh.indices),
                    usage: wgpu::BufferUsages::INDEX,
                });

                let material_buffer =
                    device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("scene material buffer"),
                        contents: bytemuck::bytes_of(&MaterialUniform {
                            base_color_factor: command.material.base_color_factor,
                        }),
                        usage: wgpu::BufferUsages::UNIFORM,
                    });

                let material_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("scene material bind group"),
                    layout: &self.material_bind_group_layout,
                    entries: &[wgpu::BindGroupEntry {
                        binding: 0,
                        resource: material_buffer.as_entire_binding(),
                    }],
                });

                WgpuRenderCommand {
                    entity_id: command.entity_id.to_string(),
                    vertex_buffer,
                    index_buffer,
                    index_count: command.mesh.indices.len() as u32,
                    material_buffer,
                    material_bind_group,
                }
            })
            .collect();

        WgpuRenderQueue { commands }
    }

    pub fn draw_prepared<'pass>(
        &'pass self,
        pass: &mut wgpu::RenderPass<'pass>,
        queue: &'pass WgpuRenderQueue,
    ) {
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.camera_bind_group, &[]);

        for command in &queue.commands {
            let _keep_material_buffer_alive = &command.material_buffer;
            pass.set_bind_group(1, &command.material_bind_group, &[]);
            pass.set_vertex_buffer(0, command.vertex_buffer.slice(..));
            pass.set_index_buffer(command.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
            pass.draw_indexed(0..command.index_count, 0, 0..1);
        }
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
