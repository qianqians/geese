//! GPU 地形渲染器：per-tile vertex/index buffer cache + draw calls。
//!
//! `GpuTerrainRenderer` 按 `TerrainStreamer` 的 active tiles 创建/更新 GPU buffers，
//! 逐 tile 执行 draw call。

use std::collections::HashMap;
use std::num::NonZeroU64;
use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;
use crate::mesh_builder::TerrainMesher;
use crate::{TileCoord, TerrainTile, TerrainStreamer, TileLoader};

const TERRAIN_SHADER_WGSL: &str = r#"
struct CameraUniform {
    view_projection: mat4x4f,
    inverse_view_projection: mat4x4f,
    camera_position: vec4f,
};

struct TileUniform {
    world_offset: vec2f,  // x, z world-space offset
    _pad: vec2f,
};

@group(0) @binding(0) var<uniform> u_camera: CameraUniform;
@group(1) @binding(0) var<uniform> u_tile: TileUniform;

struct VertexOutput {
    @builtin(position) clip_pos: vec4f,
    @location(0) world_pos: vec3f,
    @location(1) normal: vec3f,
    @location(2) uv: vec2f,
};

@vertex
fn vs_main(
    @location(0) in_pos: vec3f,
    @location(1) in_normal: vec3f,
    @location(2) in_uv: vec2f,
) -> VertexOutput {
    let world_pos = vec3f(
        in_pos.x + u_tile.world_offset.x,
        in_pos.y,
        in_pos.z + u_tile.world_offset.y,
    );
    var out: VertexOutput;
    out.clip_pos = u_camera.view_projection * vec4f(world_pos, 1.0);
    out.world_pos = world_pos;
    out.normal = in_normal;
    out.uv = in_uv;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4f {
    // Simple terrain shading: lit by directional light from above
    let light_dir = normalize(vec3f(0.3, 1.0, 0.2));
    let ndotl = max(dot(normalize(in.normal), light_dir), 0.0);
    let ambient = 0.3;
    let diffuse = ndotl * 0.7;

    // Height-based color: low = green, high = rock
    let height = in.world_pos.y;
    let low_color = vec3f(0.2, 0.5, 0.15);
    let high_color = vec3f(0.4, 0.35, 0.3);
    let color = mix(low_color, high_color, smoothstep(2.0, 15.0, height));

    let lit = color * (ambient + diffuse);
    return vec4f(lit, 1.0);
}
"#;

/// 相机 uniform（与 render crate 的 CameraUniform 布局一致）。
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct TerrainCameraUniform {
    pub view_projection: [[f32; 4]; 4],
    pub inverse_view_projection: [[f32; 4]; 4],
    pub camera_position: [f32; 4],
}

/// Per-tile uniform。
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct TileUniformData {
    pub world_offset: [f32; 2],
    pub _pad: [f32; 2],
}

struct TileGpuResources {
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    index_count: u32,
    tile_uniform: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
}

/// GPU 地形渲染器。
pub struct GpuTerrainRenderer {
    camera_buffer: wgpu::Buffer,
    frame_bind_group_layout: wgpu::BindGroupLayout,
    tile_bind_group_layout: wgpu::BindGroupLayout,
    frame_bind_group: wgpu::BindGroup,
    pipeline: wgpu::RenderPipeline,
    tile_resources: HashMap<TileCoord, TileGpuResources>,
}

impl GpuTerrainRenderer {
    pub fn new(
        device: &wgpu::Device,
        color_format: wgpu::TextureFormat,
        depth_format: wgpu::TextureFormat,
    ) -> Self {
        // ---- camera uniform buffer ----
        let camera_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("terrain camera buffer"),
            contents: bytemuck::bytes_of(&TerrainCameraUniform {
                view_projection: identity_matrix(),
                inverse_view_projection: identity_matrix(),
                camera_position: [0.0, 0.0, 0.0, 1.0],
            }),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        // ---- frame bind group layout (group 0) ----
        let frame_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("terrain frame bind group layout"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: NonZeroU64::new(std::mem::size_of::<TerrainCameraUniform>() as u64),
                    },
                    count: None,
                }],
            });

        // ---- tile bind group layout (group 1) ----
        let tile_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("terrain tile bind group layout"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: NonZeroU64::new(std::mem::size_of::<TileUniformData>() as u64),
                    },
                    count: None,
                }],
            });

        let frame_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("terrain frame bind group"),
            layout: &frame_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: camera_buffer.as_entire_binding(),
            }],
        });

        // ---- shader + pipeline ----
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("terrain shader"),
            source: wgpu::ShaderSource::Wgsl(TERRAIN_SHADER_WGSL.into()),
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("terrain pipeline layout"),
            bind_group_layouts: &[&frame_bind_group_layout, &tile_bind_group_layout],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("terrain render pipeline"),
            layout: Some(&pipeline_layout),
            cache: None,
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "vs_main",
                buffers: &[TerrainMesher::vertex_layout()],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: "fs_main",
                targets: &[Some(wgpu::ColorTargetState {
                    format: color_format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                cull_mode: Some(wgpu::Face::Back),
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: depth_format,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::Less,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
        });

        Self {
            camera_buffer,
            frame_bind_group_layout,
            tile_bind_group_layout,
            frame_bind_group,
            pipeline,
            tile_resources: HashMap::new(),
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
        let data = TerrainCameraUniform {
            view_projection,
            inverse_view_projection,
            camera_position: [camera_position[0], camera_position[1], camera_position[2], 1.0],
        };
        queue.write_buffer(&self.camera_buffer, 0, bytemuck::bytes_of(&data));
    }

    /// 按 `TerrainStreamer` 的 active tiles 创建/更新 GPU buffers。
    ///
    /// 调用方需提供 `TileLoader` 来获取每个 active tile 的数据。
    pub fn update<L: TileLoader>(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        streamer: &TerrainStreamer,
        loader: &mut L,
        cell_size: f32,
    ) {
        // 收集当前 active tiles
        let active_coords: Vec<TileCoord> = streamer.active_tiles().map(|(c, _)| *c).collect();
        let active_set: std::collections::HashSet<TileCoord> = active_coords.iter().cloned().collect();

        // 移除不再 active 的 tile 资源
        let to_remove: Vec<TileCoord> = self.tile_resources.keys()
            .filter(|c| !active_set.contains(c))
            .cloned()
            .collect();
        for coord in to_remove {
            self.tile_resources.remove(&coord);
        }

        // 为新出现的 active tiles 创建 GPU 资源
        for (coord, &lod) in streamer.active_tiles() {
            if self.tile_resources.contains_key(coord) {
                continue;
            }

            // 从 loader 加载 tile
            if let Some(tile) = loader.load(*coord) {
                self.create_tile_resources(device, queue, &tile, cell_size, lod);
            }
        }
    }

    fn create_tile_resources(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        tile: &TerrainTile,
        cell_size: f32,
        lod: u8,
    ) {
        let (vertices, indices) = TerrainMesher::generate_mesh(&tile.heightmap, cell_size, lod);

        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("terrain vertex buffer"),
            contents: bytemuck::cast_slice(&vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("terrain index buffer"),
            contents: bytemuck::cast_slice(&indices),
            usage: wgpu::BufferUsages::INDEX,
        });

        let tile_uniform = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("terrain tile uniform"),
            contents: bytemuck::bytes_of(&TileUniformData {
                world_offset: tile.world_origin,
                _pad: [0.0; 2],
            }),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("terrain tile bind group"),
            layout: &self.tile_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: tile_uniform.as_entire_binding(),
            }],
        });

        let index_count = indices.len() as u32;

        self.tile_resources.insert(tile.coord, TileGpuResources {
            vertex_buffer,
            index_buffer,
            index_count,
            tile_uniform,
            bind_group,
        });

        let _ = queue;
    }

    /// 逐 tile draw call 渲染地形。
    pub fn render(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        color_target: &wgpu::TextureView,
        depth_target: &wgpu::TextureView,
    ) {
        if self.tile_resources.is_empty() {
            return;
        }

        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("terrain render pass"),
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
        pass.set_bind_group(0, &self.frame_bind_group, &[]);

        for (_, res) in &self.tile_resources {
            pass.set_bind_group(1, &res.bind_group, &[]);
            pass.set_vertex_buffer(0, res.vertex_buffer.slice(..));
            pass.set_index_buffer(res.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
            pass.draw_indexed(0..res.index_count, 0, 0..1);
        }
    }

    /// 当前已加载的 tile 数量。
    pub fn tile_count(&self) -> usize {
        self.tile_resources.len()
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
    fn camera_uniform_size() {
        assert_eq!(std::mem::size_of::<TerrainCameraUniform>(), 192);
    }

    #[test]
    fn tile_uniform_size() {
        assert_eq!(std::mem::size_of::<TileUniformData>(), 16);
    }
}
