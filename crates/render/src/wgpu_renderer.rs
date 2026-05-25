use wgpu::util::DeviceExt;

use crate::{FilterMode, MaterialLibrary, RenderQueue, Texture, TextureFormat, Vertex, WrapMode};

#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct GpuVertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub uv: [f32; 2],
    pub tangent: [f32; 4],
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
                wgpu::VertexAttribute {
                    offset: std::mem::size_of::<[f32; 8]>() as wgpu::BufferAddress,
                    shader_location: 3,
                    format: wgpu::VertexFormat::Float32x4,
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
            tangent: vertex.tangent,
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct CameraUniform {
    pub view_projection: [[f32; 4]; 4],
    pub camera_position: [f32; 4],
}

impl CameraUniform {
    pub fn new(view_projection: [[f32; 4]; 4], camera_position: [f32; 3]) -> Self {
        Self {
            view_projection,
            camera_position: [
                camera_position[0],
                camera_position[1],
                camera_position[2],
                1.0,
            ],
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct MaterialUniform {
    pub base_color_factor: [f32; 4],
    pub params: [f32; 4],
}

impl MaterialUniform {
    pub fn new(base_color_factor: [f32; 4], normal_map_enabled: bool, shininess: f32) -> Self {
        Self {
            base_color_factor,
            params: [
                normal_map_enabled as u32 as f32,
                shininess.max(1.0),
                0.0,
                0.0,
            ],
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct ObjectUniform {
    pub model: [[f32; 4]; 4],
    pub normal: [[f32; 4]; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct LightUniform {
    pub direction: [f32; 4],
    pub color: [f32; 4],
    pub ambient: [f32; 4],
}

impl LightUniform {
    pub fn directional(direction: [f32; 3], color: [f32; 3], ambient: [f32; 3]) -> Self {
        Self {
            direction: [direction[0], direction[1], direction[2], 0.0],
            color: [color[0], color[1], color[2], 1.0],
            ambient: [ambient[0], ambient[1], ambient[2], 1.0],
        }
    }
}

impl Default for LightUniform {
    fn default() -> Self {
        Self::directional([0.4, -0.8, 0.4], [1.0, 1.0, 1.0], [0.08, 0.08, 0.08])
    }
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
    object_buffer: wgpu::Buffer,
    object_bind_group: wgpu::BindGroup,
    normal_texture: Option<wgpu::Texture>,
    normal_texture_view: Option<wgpu::TextureView>,
    normal_sampler: Option<wgpu::Sampler>,
}

pub struct WgpuRenderQueue {
    pub commands: Vec<WgpuRenderCommand>,
}

pub struct WgpuSceneRenderer {
    pipeline: wgpu::RenderPipeline,
    camera_buffer: wgpu::Buffer,
    camera_bind_group: wgpu::BindGroup,
    light_buffer: wgpu::Buffer,
    material_bind_group_layout: wgpu::BindGroupLayout,
    object_bind_group_layout: wgpu::BindGroupLayout,
    default_normal_texture: wgpu::Texture,
    default_normal_texture_view: wgpu::TextureView,
    default_sampler: wgpu::Sampler,
}

impl WgpuSceneRenderer {
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        descriptor: WgpuSceneRendererDescriptor,
    ) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("scene mesh shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("../shaders/mesh.wgsl").into()),
        });

        let camera_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("scene camera bind group layout"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                ],
            });

        let material_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("scene material bind group layout"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 2,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                ],
            });

        let object_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("scene object bind group layout"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
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
            bind_group_layouts: &[
                &camera_bind_group_layout,
                &material_bind_group_layout,
                &object_bind_group_layout,
            ],
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
            contents: bytemuck::bytes_of(&CameraUniform::new(identity_matrix(), [0.0, 0.0, 1.0])),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let light_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("scene light buffer"),
            contents: bytemuck::bytes_of(&LightUniform::default()),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let camera_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("scene camera bind group"),
            layout: &camera_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: camera_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: light_buffer.as_entire_binding(),
                },
            ],
        });

        let default_normal_texture = create_rgba_texture(
            device,
            queue,
            "default normal texture",
            1,
            1,
            &[128, 128, 255, 255],
        );
        let default_normal_texture_view =
            default_normal_texture.create_view(&wgpu::TextureViewDescriptor::default());
        let default_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("default scene sampler"),
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            address_mode_w: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        Self {
            pipeline,
            camera_buffer,
            camera_bind_group,
            light_buffer,
            material_bind_group_layout,
            object_bind_group_layout,
            default_normal_texture,
            default_normal_texture_view,
            default_sampler,
        }
    }

    pub fn update_camera(
        &self,
        queue: &wgpu::Queue,
        view_projection: [[f32; 4]; 4],
        camera_position: [f32; 3],
    ) {
        queue.write_buffer(
            &self.camera_buffer,
            0,
            bytemuck::bytes_of(&CameraUniform::new(view_projection, camera_position)),
        );
    }

    pub fn update_light(&self, queue: &wgpu::Queue, light: LightUniform) {
        queue.write_buffer(&self.light_buffer, 0, bytemuck::bytes_of(&light));
    }

    pub fn prepare(
        &self,
        device: &wgpu::Device,
        gpu_queue: &wgpu::Queue,
        materials: &MaterialLibrary,
        render_queue: &RenderQueue<'_>,
    ) -> WgpuRenderQueue {
        self.prepare_with_shininess(device, gpu_queue, materials, render_queue, 32.0)
    }

    pub fn prepare_with_shininess(
        &self,
        device: &wgpu::Device,
        gpu_queue: &wgpu::Queue,
        materials: &MaterialLibrary,
        render_queue: &RenderQueue<'_>,
        shininess: f32,
    ) -> WgpuRenderQueue {
        let commands = render_queue
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

                let normal_map_enabled = command.material.normal_texture.is_some()
                    && command.mesh.flags.has_tangents
                    && command.mesh.flags.has_uv0;

                let material_buffer =
                    device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("scene material buffer"),
                        contents: bytemuck::bytes_of(&MaterialUniform::new(
                            command.material.base_color_factor,
                            normal_map_enabled,
                            shininess,
                        )),
                        usage: wgpu::BufferUsages::UNIFORM,
                    });

                let normal_source = normal_map_enabled
                    .then_some(command.material.normal_texture)
                    .flatten()
                    .and_then(|handle| materials.texture(handle));
                let normal_texture =
                    normal_source.map(|texture| upload_texture(device, gpu_queue, texture));
                let normal_texture_view = normal_texture
                    .as_ref()
                    .map(|texture| texture.create_view(&wgpu::TextureViewDescriptor::default()));
                let normal_sampler = normal_source.map(|texture| create_sampler(device, texture));

                let normal_view = normal_texture_view
                    .as_ref()
                    .unwrap_or(&self.default_normal_texture_view);
                let normal_sampler_ref = normal_sampler.as_ref().unwrap_or(&self.default_sampler);

                let material_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("scene material bind group"),
                    layout: &self.material_bind_group_layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: material_buffer.as_entire_binding(),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::TextureView(normal_view),
                        },
                        wgpu::BindGroupEntry {
                            binding: 2,
                            resource: wgpu::BindingResource::Sampler(normal_sampler_ref),
                        },
                    ],
                });

                let object_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("scene object buffer"),
                    contents: bytemuck::bytes_of(&ObjectUniform {
                        model: command.model_matrix,
                        normal: command.normal_matrix,
                    }),
                    usage: wgpu::BufferUsages::UNIFORM,
                });

                let object_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("scene object bind group"),
                    layout: &self.object_bind_group_layout,
                    entries: &[wgpu::BindGroupEntry {
                        binding: 0,
                        resource: object_buffer.as_entire_binding(),
                    }],
                });

                WgpuRenderCommand {
                    entity_id: command.entity_id.to_string(),
                    vertex_buffer,
                    index_buffer,
                    index_count: command.mesh.indices.len() as u32,
                    material_buffer,
                    material_bind_group,
                    object_buffer,
                    object_bind_group,
                    normal_texture,
                    normal_texture_view,
                    normal_sampler,
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
            let _keep_object_buffer_alive = &command.object_buffer;
            let _keep_normal_texture_alive = &command.normal_texture;
            let _keep_normal_texture_view_alive = &command.normal_texture_view;
            let _keep_normal_sampler_alive = &command.normal_sampler;
            let _keep_default_normal_texture_alive = &self.default_normal_texture;
            pass.set_bind_group(1, &command.material_bind_group, &[]);
            pass.set_bind_group(2, &command.object_bind_group, &[]);
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

fn upload_texture(device: &wgpu::Device, queue: &wgpu::Queue, texture: &Texture) -> wgpu::Texture {
    let pixels = to_rgba8(texture);
    create_rgba_texture(
        device,
        queue,
        texture.name.as_deref().unwrap_or("scene texture"),
        texture.width,
        texture.height,
        &pixels,
    )
}

fn create_rgba_texture(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    label: &str,
    width: u32,
    height: u32,
    pixels: &[u8],
) -> wgpu::Texture {
    device.create_texture_with_data(
        queue,
        &wgpu::TextureDescriptor {
            label: Some(label),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        },
        wgpu::util::TextureDataOrder::LayerMajor,
        pixels,
    )
}

fn to_rgba8(texture: &Texture) -> Vec<u8> {
    match texture.format {
        TextureFormat::R8 => texture
            .pixels
            .iter()
            .flat_map(|r| [*r, *r, *r, 255])
            .collect(),
        TextureFormat::R8G8 => texture
            .pixels
            .chunks_exact(2)
            .flat_map(|px| [px[0], px[1], 0, 255])
            .collect(),
        TextureFormat::R8G8B8 => texture
            .pixels
            .chunks_exact(3)
            .flat_map(|px| [px[0], px[1], px[2], 255])
            .collect(),
        TextureFormat::R8G8B8A8 => texture.pixels.clone(),
        _ => vec![128, 128, 255, 255],
    }
}

fn create_sampler(device: &wgpu::Device, texture: &Texture) -> wgpu::Sampler {
    device.create_sampler(&wgpu::SamplerDescriptor {
        label: texture.name.as_deref(),
        address_mode_u: convert_wrap_mode(texture.sampler.wrap_s),
        address_mode_v: convert_wrap_mode(texture.sampler.wrap_t),
        address_mode_w: wgpu::AddressMode::Repeat,
        mag_filter: convert_filter_mode(texture.sampler.mag_filter),
        min_filter: convert_filter_mode(texture.sampler.min_filter),
        mipmap_filter: convert_filter_mode(texture.sampler.min_filter),
        ..Default::default()
    })
}

fn convert_filter_mode(mode: FilterMode) -> wgpu::FilterMode {
    match mode {
        FilterMode::Nearest
        | FilterMode::NearestMipmapNearest
        | FilterMode::NearestMipmapLinear => wgpu::FilterMode::Nearest,
        FilterMode::Linear | FilterMode::LinearMipmapNearest | FilterMode::LinearMipmapLinear => {
            wgpu::FilterMode::Linear
        }
    }
}

fn convert_wrap_mode(mode: WrapMode) -> wgpu::AddressMode {
    match mode {
        WrapMode::ClampToEdge => wgpu::AddressMode::ClampToEdge,
        WrapMode::Repeat => wgpu::AddressMode::Repeat,
        WrapMode::MirroredRepeat => wgpu::AddressMode::MirrorRepeat,
    }
}
