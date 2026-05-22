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

pub struct WgpuRenderCommand {
    pub entity_id: String,
    pub vertex_buffer: wgpu::Buffer,
    pub index_buffer: wgpu::Buffer,
    pub index_count: u32,
}

pub struct WgpuRenderQueue {
    pub commands: Vec<WgpuRenderCommand>,
}

#[derive(Default)]
pub struct WgpuSceneRenderer;

impl WgpuSceneRenderer {
    pub fn new() -> Self {
        Self
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

                WgpuRenderCommand {
                    entity_id: command.entity_id.to_string(),
                    vertex_buffer,
                    index_buffer,
                    index_count: command.mesh.indices.len() as u32,
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
        for command in &queue.commands {
            pass.set_vertex_buffer(0, command.vertex_buffer.slice(..));
            pass.set_index_buffer(command.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
            pass.draw_indexed(0..command.index_count, 0, 0..1);
        }
    }
}
