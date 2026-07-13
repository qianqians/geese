//! 2D Sprite rendering module.
//!
//! Provides batch sprite rendering with orthographic projection,
//! alpha blending, and per-sprite properties (rotation, flip, color tint).

use bytemuck::{Pod, Zeroable};

use crate::material::TextureHandle;

const SPRITE_WGSL: &str = include_str!("../shaders/sprite.wgsl");

// ---------------------------------------------------------------------------
// Vertex
// ---------------------------------------------------------------------------

/// Sprite vertex: 2D position + texture coordinate + RGBA color.
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct SpriteVertex {
    pub position: [f32; 2],
    pub uv: [f32; 2],
    pub color: [f32; 4],
}

impl SpriteVertex {
    fn layout<'a>() -> wgpu::VertexBufferLayout<'a> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<SpriteVertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[
                wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 0,
                    format: wgpu::VertexFormat::Float32x2,
                },
                wgpu::VertexAttribute {
                    offset: std::mem::size_of::<[f32; 2]>() as wgpu::BufferAddress,
                    shader_location: 1,
                    format: wgpu::VertexFormat::Float32x2,
                },
                wgpu::VertexAttribute {
                    offset: std::mem::size_of::<[f32; 4]>() as wgpu::BufferAddress,
                    shader_location: 2,
                    format: wgpu::VertexFormat::Float32x4,
                },
            ],
        }
    }
}

// ---------------------------------------------------------------------------
// Sprite
// ---------------------------------------------------------------------------

/// A single 2D sprite with transform and visual properties.
#[derive(Clone, Debug)]
pub struct Sprite {
    /// World-space position (x, y, z).
    pub position: [f32; 3],
    /// Width and height in world units.
    pub size: [f32; 2],
    /// Texture coordinate rectangle (x, y, w, h) in normalized [0,1] space.
    pub uv_rect: [f32; 4],
    /// Tint color with alpha (RGBA).
    pub color: [f32; 4],
    /// 2D rotation in radians (around the sprite center).
    pub rotation: f32,
    /// Depth value used for draw-order sorting (lower = drawn first).
    pub z_order: f32,
    /// Horizontal flip.
    pub flip_x: bool,
    /// Vertical flip.
    pub flip_y: bool,
    /// Reference to the texture used by this sprite.
    pub texture_handle: TextureHandle,
}

impl Default for Sprite {
    fn default() -> Self {
        Self {
            position: [0.0; 3],
            size: [1.0, 1.0],
            uv_rect: [0.0, 0.0, 1.0, 1.0],
            color: [1.0, 1.0, 1.0, 1.0],
            rotation: 0.0,
            z_order: 0.0,
            flip_x: false,
            flip_y: false,
            texture_handle: TextureHandle(0),
        }
    }
}

// ---------------------------------------------------------------------------
// SpriteBatch
// ---------------------------------------------------------------------------

/// A batch of sprites sharing the same texture that will be rendered in a
/// single draw call. Sprites are sorted by `z_order` before building the
/// vertex buffer.
#[derive(Clone, Debug, Default)]
pub struct SpriteBatch {
    sprites: Vec<Sprite>,
}

impl SpriteBatch {
    pub fn new() -> Self {
        Self {
            sprites: Vec::new(),
        }
    }

    /// Add a sprite to the batch.
    pub fn add_sprite(&mut self, sprite: Sprite) {
        self.sprites.push(sprite);
    }

    /// Clear all sprites from the batch.
    pub fn clear(&mut self) {
        self.sprites.clear();
    }

    /// Number of sprites currently in the batch.
    pub fn len(&self) -> usize {
        self.sprites.len()
    }

    /// Returns `true` if the batch contains no sprites.
    pub fn is_empty(&self) -> bool {
        self.sprites.is_empty()
    }

    /// Sort sprites by `z_order` (ascending). Must be called before
    /// `build_vertex_buffer` to ensure correct draw order.
    pub fn sort_by_z(&mut self) {
        self.sprites
            .sort_by(|a, b| a.z_order.partial_cmp(&b.z_order).unwrap_or(std::cmp::Ordering::Equal));
    }

    /// Build the vertex and index data for all sprites in the batch.
    ///
    /// Each sprite produces 4 vertices and 6 indices (2 triangles).
    /// Returns `(vertices, indices)`.
    pub fn build_vertex_buffer(&self) -> (Vec<SpriteVertex>, Vec<u32>) {
        let sprite_count = self.sprites.len();
        let mut vertices = Vec::with_capacity(sprite_count * 4);
        let mut indices = Vec::with_capacity(sprite_count * 6);

        for (i, sprite) in self.sprites.iter().enumerate() {
            let base = (i * 4) as u32;

            let half_w = sprite.size[0] * 0.5;
            let half_h = sprite.size[1] * 0.5;
            let cx = sprite.position[0];
            let cy = sprite.position[1];
            let cos_r = sprite.rotation.cos();
            let sin_r = sprite.rotation.sin();

            // Four corners relative to center, before rotation.
            let corners: [[f32; 2]; 4] = [
                [-half_w, -half_h], // top-left
                [half_w, -half_h],  // top-right
                [half_w, half_h],   // bottom-right
                [-half_w, half_h],  // bottom-left
            ];

            for corner in &corners {
                let rx = corner[0] * cos_r - corner[1] * sin_r + cx;
                let ry = corner[0] * sin_r + corner[1] * cos_r + cy;
                vertices.push(SpriteVertex {
                    position: [rx, ry],
                    uv: [0.0, 0.0], // filled below
                    color: sprite.color,
                });
            }

            // UV coordinates with flip support.
            let [ux, uy, uw, uh] = sprite.uv_rect;
            let (u0, u1) = if sprite.flip_x {
                (ux + uw, ux)
            } else {
                (ux, ux + uw)
            };
            let (v0, v1) = if sprite.flip_y {
                (uy + uh, uy)
            } else {
                (uy, uy + uh)
            };

            let uvs: [[f32; 2]; 4] = [
                [u0, v0], // top-left
                [u1, v0], // top-right
                [u1, v1], // bottom-right
                [u0, v1], // bottom-left
            ];

            // Patch UVs into vertices (they were zeroed above).
            for (j, uv) in uvs.iter().enumerate() {
                vertices[base as usize + j].uv = *uv;
            }

            // Two triangles: 0-1-2 and 0-2-3.
            indices.push(base);
            indices.push(base + 1);
            indices.push(base + 2);
            indices.push(base);
            indices.push(base + 2);
            indices.push(base + 3);
        }

        (vertices, indices)
    }
}

// ---------------------------------------------------------------------------
// Orthographic camera uniform for 2D rendering
// ---------------------------------------------------------------------------

/// Simple orthographic projection uniform used by the sprite renderer.
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct SpriteCameraUniform {
    pub projection: [[f32; 4]; 4],
}

impl SpriteCameraUniform {
    /// Build an orthographic projection matrix.
    pub fn orthographic(left: f32, right: f32, bottom: f32, top: f32, near: f32, far: f32) -> Self {
        let rml = right - left;
        let rpl = right + left;
        let tmb = top - bottom;
        let tpb = top + bottom;
        let fmn = far - near;
        let fpn = far + near;
        Self {
            projection: [
                [2.0 / rml, 0.0, 0.0, 0.0],
                [0.0, 2.0 / tmb, 0.0, 0.0],
                [0.0, 0.0, -2.0 / fmn, 0.0],
                [-(rpl / rml), -(tpb / tmb), -(fpn / fmn), 1.0],
            ],
        }
    }
}

// ---------------------------------------------------------------------------
// SpriteRenderer
// ---------------------------------------------------------------------------

/// GPU sprite renderer. Manages the render pipeline, vertex/index buffers,
/// and camera uniform for 2D sprite drawing.
pub struct SpriteRenderer {
    pipeline: wgpu::RenderPipeline,
    camera_buffer: wgpu::Buffer,
    camera_bind_group: wgpu::BindGroup,
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    index_count: u32,
    vertex_capacity: u32,
    index_capacity: u32,
}

impl SpriteRenderer {
    /// Create a new sprite renderer.
    ///
    /// `color_format` and `depth_format` must match the render target.
    /// `sample_count` controls MSAA (use 1 for no multisampling).
    pub fn new(
        device: &wgpu::Device,
        color_format: wgpu::TextureFormat,
        depth_format: wgpu::TextureFormat,
        sample_count: u32,
    ) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("sprite shader"),
            source: wgpu::ShaderSource::Wgsl(SPRITE_WGSL.into()),
        });

        // Camera uniform
        let camera_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("sprite camera uniform"),
            size: std::mem::size_of::<SpriteCameraUniform>() as wgpu::BufferAddress,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let camera_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("sprite camera layout"),
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

        let camera_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("sprite camera bind group"),
            layout: &camera_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: camera_buffer.as_entire_binding(),
            }],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("sprite pipeline layout"),
            bind_group_layouts: &[&camera_layout],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("sprite pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "vs_main",
                compilation_options: Default::default(),
                buffers: &[SpriteVertex::layout()],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: "fs_main",
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: color_format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                unclipped_depth: false,
                polygon_mode: wgpu::PolygonMode::Fill,
                conservative: false,
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: depth_format,
                depth_write_enabled: false,
                depth_compare: wgpu::CompareFunction::LessEqual,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState {
                count: sample_count,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview: None,
            cache: None,
        });

        // Initial buffer capacity (1024 sprites = 4096 vertices, 6144 indices).
        let vertex_capacity: u32 = 4096;
        let index_capacity: u32 = 6144;

        let vertex_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("sprite vertex buffer"),
            size: (vertex_capacity as wgpu::BufferAddress)
                * std::mem::size_of::<SpriteVertex>() as wgpu::BufferAddress,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let index_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("sprite index buffer"),
            size: (index_capacity as wgpu::BufferAddress) * std::mem::size_of::<u32>() as wgpu::BufferAddress,
            usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        Self {
            pipeline,
            camera_buffer,
            camera_bind_group,
            vertex_buffer,
            index_buffer,
            index_count: 0,
            vertex_capacity,
            index_capacity,
        }
    }

    /// Update the orthographic projection uniform.
    pub fn update_camera(&self, queue: &wgpu::Queue, uniform: &SpriteCameraUniform) {
        queue.write_buffer(&self.camera_buffer, 0, bytemuck::bytes_of(uniform));
    }

    /// Upload sprite batch vertex/index data to the GPU.
    /// Automatically resizes buffers when the data exceeds current capacity.
    pub fn upload(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        batch: &SpriteBatch,
    ) {
        let (vertices, indices) = batch.build_vertex_buffer();
        self.index_count = indices.len() as u32;

        if vertices.is_empty() {
            return;
        }

        let vcount = vertices.len() as u32;
        let icount = indices.len() as u32;

        // Resize vertex buffer if needed.
        if vcount > self.vertex_capacity {
            let new_cap = vcount.next_power_of_two().max(self.vertex_capacity * 2);
            self.vertex_buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("sprite vertex buffer"),
                size: (new_cap as wgpu::BufferAddress)
                    * std::mem::size_of::<SpriteVertex>() as wgpu::BufferAddress,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            self.vertex_capacity = new_cap;
        }

        // Resize index buffer if needed.
        if icount > self.index_capacity {
            let new_cap = icount.next_power_of_two().max(self.index_capacity * 2);
            self.index_buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("sprite index buffer"),
                size: (new_cap as wgpu::BufferAddress)
                    * std::mem::size_of::<u32>() as wgpu::BufferAddress,
                usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            self.index_capacity = new_cap;
        }

        queue.write_buffer(&self.vertex_buffer, 0, bytemuck::cast_slice(&vertices));
        queue.write_buffer(&self.index_buffer, 0, bytemuck::cast_slice(&indices));
    }

    /// Draw the uploaded sprite batch in an already-started render pass.
    pub fn draw<'a>(&'a self, pass: &mut wgpu::RenderPass<'a>) {
        if self.index_count == 0 {
            return;
        }
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.camera_bind_group, &[]);
        pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
        pass.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
        pass.draw_indexed(0..self.index_count, 0, 0..1);
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_sprite(z: f32) -> Sprite {
        Sprite {
            z_order: z,
            position: [z, z, 0.0],
            ..Sprite::default()
        }
    }

    #[test]
    fn test_batch_sort_by_z() {
        let mut batch = SpriteBatch::new();
        batch.add_sprite(make_sprite(3.0));
        batch.add_sprite(make_sprite(1.0));
        batch.add_sprite(make_sprite(2.0));

        batch.sort_by_z();

        let z_orders: Vec<f32> = batch.sprites.iter().map(|s| s.z_order).collect();
        assert_eq!(z_orders, vec![1.0, 2.0, 3.0]);
    }

    #[test]
    fn test_batch_build_vertex_buffer_single_sprite() {
        let mut batch = SpriteBatch::new();
        batch.add_sprite(Sprite {
            position: [0.0, 0.0, 0.0],
            size: [2.0, 2.0],
            uv_rect: [0.0, 0.0, 1.0, 1.0],
            color: [1.0; 4],
            rotation: 0.0,
            z_order: 0.0,
            flip_x: false,
            flip_y: false,
            texture_handle: TextureHandle(0),
        });

        let (vertices, indices) = batch.build_vertex_buffer();
        assert_eq!(vertices.len(), 4);
        assert_eq!(indices.len(), 6);

        // Check that corners are at (-1,-1), (1,-1), (1,1), (-1,1)
        assert!((vertices[0].position[0] - (-1.0)).abs() < 1e-6);
        assert!((vertices[0].position[1] - (-1.0)).abs() < 1e-6);
        assert!((vertices[2].position[0] - 1.0).abs() < 1e-6);
        assert!((vertices[2].position[1] - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_batch_build_vertex_buffer_flip_x() {
        let mut batch = SpriteBatch::new();
        batch.add_sprite(Sprite {
            uv_rect: [0.0, 0.0, 1.0, 1.0],
            flip_x: true,
            ..Sprite::default()
        });

        let (vertices, _indices) = batch.build_vertex_buffer();
        // flip_x swaps u0 and u1: top-left should have u=1.0
        assert!((vertices[0].uv[0] - 1.0).abs() < 1e-6);
        assert!((vertices[1].uv[0] - 0.0).abs() < 1e-6);
    }

    #[test]
    fn test_batch_empty() {
        let batch = SpriteBatch::new();
        assert!(batch.is_empty());
        assert_eq!(batch.len(), 0);
        let (v, i) = batch.build_vertex_buffer();
        assert!(v.is_empty());
        assert!(i.is_empty());
    }

    #[test]
    fn test_batch_clear() {
        let mut batch = SpriteBatch::new();
        batch.add_sprite(make_sprite(0.0));
        batch.add_sprite(make_sprite(1.0));
        assert_eq!(batch.len(), 2);
        batch.clear();
        assert!(batch.is_empty());
    }

    #[test]
    fn test_orthographic_projection() {
        let uniform = SpriteCameraUniform::orthographic(0.0, 800.0, 0.0, 600.0, -1.0, 1.0);
        // projection[0][0] = 2 / (right - left) = 2/800
        assert!((uniform.projection[0][0] - 2.0 / 800.0).abs() < 1e-6);
        // projection[1][1] = 2 / (top - bottom) = 2/600
        assert!((uniform.projection[1][1] - 2.0 / 600.0).abs() < 1e-6);
    }

    #[test]
    fn test_batch_multiple_sprites_indices() {
        let mut batch = SpriteBatch::new();
        batch.add_sprite(make_sprite(0.0));
        batch.add_sprite(make_sprite(1.0));
        batch.add_sprite(make_sprite(2.0));

        let (vertices, indices) = batch.build_vertex_buffer();
        assert_eq!(vertices.len(), 12); // 3 sprites * 4 vertices
        assert_eq!(indices.len(), 18);  // 3 sprites * 6 indices

        // Second sprite indices should be offset by 4
        assert_eq!(indices[6], 4);
        assert_eq!(indices[7], 5);
        assert_eq!(indices[8], 6);
    }
}
