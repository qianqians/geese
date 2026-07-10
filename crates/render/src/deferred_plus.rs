use wgpu::util::DeviceExt;

use crate::cluster::{ClusterUniform, TOTAL_CLUSTERS};
use crate::common::{
    CameraUniform, DefaultTextures, GpuResourceCache, GpuVertex, WgpuRenderQueue,
};
use crate::forward_plus::{
    build_command, material_layout_entries, storage_read_entry, storage_rw_entry, uniform_entry,
};
use crate::light::LightStorage;
use crate::pipeline::{RenderingPath, ScenePipeline, ScenePipelineDescriptor};
use crate::{Light, MaterialLibrary, RenderQueue};

const PBR_COMMON: &str = include_str!("../shaders/pbr_common.wgsl");
const DEFERRED_GEOMETRY_WGSL: &str = include_str!("../shaders/deferred_geometry.wgsl");
const DEFERRED_LIGHTING_WGSL: &str = include_str!("../shaders/deferred_lighting.wgsl");
const CLUSTER_CULLING_WGSL: &str = include_str!("../shaders/cluster_culling.wgsl");

const CLUSTER_BITMASK_SIZE: u64 = (TOTAL_CLUSTERS as u64) * 4;
const CULLING_WORKGROUP_SIZE: u32 = 64;

const GBUFFER_BASE_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
const GBUFFER_NORMAL_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
const GBUFFER_EMISSIVE_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;

/// Deferred+ 渲染管线。
///
/// - geometry pass：把 mesh 写入 3 张 G-Buffer + depth
/// - cluster culling：复用与 Forward+ 相同的 compute shader 写 bitmask
/// - lighting pass：全屏 quad 从 G-Buffer + depth 还原信息并按 cluster 着色
pub struct DeferredPlusPipeline {
    final_color_format: wgpu::TextureFormat,
    depth_format: wgpu::TextureFormat,
    sample_count: u32,
    width: u32,
    height: u32,
    z_near: f32,
    z_far: f32,

    // ---- 帧级 buffer ----
    camera_buffer: wgpu::Buffer,
    lights_buffer: wgpu::Buffer,
    cluster_buffer: wgpu::Buffer,
    #[allow(dead_code)]
    cluster_bitmask_buffer: wgpu::Buffer,

    // ---- layouts ----
    #[allow(dead_code)]
    geometry_frame_layout: wgpu::BindGroupLayout,
    #[allow(dead_code)]
    lighting_frame_layout: wgpu::BindGroupLayout,
    #[allow(dead_code)]
    culling_layout: wgpu::BindGroupLayout,
    material_bind_group_layout: wgpu::BindGroupLayout,
    object_bind_group_layout: wgpu::BindGroupLayout,
    gbuffer_bind_group_layout: wgpu::BindGroupLayout,

    // ---- bind groups ----
    geometry_frame_bind_group: wgpu::BindGroup,
    lighting_frame_bind_group: wgpu::BindGroup,
    culling_bind_group: wgpu::BindGroup,
    /// G-Buffer bind group 在 resize 时随 G-Buffer 一起重建
    gbuffer_bind_group: wgpu::BindGroup,

    // ---- pipelines ----
    geometry_pipeline: wgpu::RenderPipeline,
    lighting_pipeline: wgpu::RenderPipeline,
    culling_pipeline: wgpu::ComputePipeline,

    // ---- G-Buffer 资源（resize 重建）----
    gbuffer_base: GBufferTexture,
    gbuffer_normal: GBufferTexture,
    gbuffer_emissive: GBufferTexture,
    gbuffer_depth: GBufferTexture,
    gbuffer_sampler: wgpu::Sampler,

    default_textures: DefaultTextures,
    prepared: WgpuRenderQueue,
    cache: GpuResourceCache,
}

struct GBufferTexture {
    #[allow(dead_code)]
    texture: wgpu::Texture,
    view: wgpu::TextureView,
}

impl DeferredPlusPipeline {
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        descriptor: &ScenePipelineDescriptor,
    ) -> Self {
        debug_assert_eq!(descriptor.rendering_path, RenderingPath::DeferredPlus);

        // ---- shaders ----
        let geometry_src = format!("{PBR_COMMON}\n{DEFERRED_GEOMETRY_WGSL}");
        let lighting_src = format!("{PBR_COMMON}\n{DEFERRED_LIGHTING_WGSL}");
        let culling_src = format!("{PBR_COMMON}\n{CLUSTER_CULLING_WGSL}");

        let geometry_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("deferred+ geometry shader"),
            source: wgpu::ShaderSource::Wgsl(geometry_src.into()),
        });
        let lighting_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("deferred+ lighting shader"),
            source: wgpu::ShaderSource::Wgsl(lighting_src.into()),
        });
        let culling_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("deferred+ cluster culling shader"),
            source: wgpu::ShaderSource::Wgsl(culling_src.into()),
        });

        // ---- layouts ----
        let geometry_frame_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("deferred+ geometry frame layout"),
                entries: &[uniform_entry(0, wgpu::ShaderStages::VERTEX_FRAGMENT)],
            });
        let lighting_frame_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("deferred+ lighting frame layout"),
                entries: &[
                    uniform_entry(0, wgpu::ShaderStages::VERTEX_FRAGMENT),
                    uniform_entry(1, wgpu::ShaderStages::FRAGMENT),
                    uniform_entry(2, wgpu::ShaderStages::FRAGMENT),
                    storage_read_entry(3, wgpu::ShaderStages::FRAGMENT),
                ],
            });
        let culling_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("deferred+ cluster culling layout"),
            entries: &[
                uniform_entry(0, wgpu::ShaderStages::COMPUTE),
                uniform_entry(1, wgpu::ShaderStages::COMPUTE),
                storage_rw_entry(2, wgpu::ShaderStages::COMPUTE),
            ],
        });
        let material_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("deferred+ material layout"),
                entries: &material_layout_entries(),
            });
        let object_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("deferred+ object layout"),
                entries: &[uniform_entry(0, wgpu::ShaderStages::VERTEX_FRAGMENT)],
            });
        let gbuffer_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("deferred+ G-Buffer layout"),
                entries: &gbuffer_layout_entries(),
            });

        // ---- pipelines ----
        let geometry_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("deferred+ geometry pipeline layout"),
                bind_group_layouts: &[
                    &geometry_frame_layout,
                    &material_bind_group_layout,
                    &object_bind_group_layout,
                ],
                push_constant_ranges: &[],
            });
        let vertex_buffers = [GpuVertex::layout()];
        let geometry_targets = [
            Some(wgpu::ColorTargetState {
                format: GBUFFER_BASE_FORMAT,
                blend: None,
                write_mask: wgpu::ColorWrites::ALL,
            }),
            Some(wgpu::ColorTargetState {
                format: GBUFFER_NORMAL_FORMAT,
                blend: None,
                write_mask: wgpu::ColorWrites::ALL,
            }),
            Some(wgpu::ColorTargetState {
                format: GBUFFER_EMISSIVE_FORMAT,
                blend: None,
                write_mask: wgpu::ColorWrites::ALL,
            }),
        ];
        let geometry_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("deferred+ geometry pipeline"),
            layout: Some(&geometry_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &geometry_shader,
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
            depth_stencil: Some(wgpu::DepthStencilState {
                format: descriptor.depth_format,
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
                module: &geometry_shader,
                entry_point: "fs_main",
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &geometry_targets,
            }),
            multiview: None,
            cache: None,
        });

        let lighting_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("deferred+ lighting pipeline layout"),
                bind_group_layouts: &[&lighting_frame_layout, &gbuffer_bind_group_layout],
                push_constant_ranges: &[],
            });
        let lighting_targets = [Some(wgpu::ColorTargetState {
            format: descriptor.color_format,
            blend: None,
            write_mask: wgpu::ColorWrites::ALL,
        })];
        let lighting_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("deferred+ lighting pipeline"),
            layout: Some(&lighting_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &lighting_shader,
                entry_point: "vs_main",
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[],
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
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &lighting_shader,
                entry_point: "fs_main",
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &lighting_targets,
            }),
            multiview: None,
            cache: None,
        });

        let culling_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("deferred+ cluster culling pipeline layout"),
                bind_group_layouts: &[&culling_layout],
                push_constant_ranges: &[],
            });
        let culling_pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("deferred+ cluster culling pipeline"),
            layout: Some(&culling_pipeline_layout),
            module: &culling_shader,
            entry_point: "cs_main",
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            cache: None,
        });

        // ---- buffers ----
        let camera_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("deferred+ camera buffer"),
            contents: bytemuck::bytes_of(&CameraUniform::placeholder()),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let lights_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("deferred+ lights buffer"),
            contents: bytemuck::bytes_of(&LightStorage::empty()),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let width = descriptor.width.max(1);
        let height = descriptor.height.max(1);
        let cluster_uniform = ClusterUniform::new(width, height, 0.1, 1000.0, crate::common::identity_matrix());
        let cluster_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("deferred+ cluster buffer"),
            contents: bytemuck::bytes_of(&cluster_uniform),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let cluster_bitmask_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("deferred+ cluster bitmask buffer"),
            size: CLUSTER_BITMASK_SIZE,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // ---- 静态 bind groups ----
        let geometry_frame_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("deferred+ geometry frame bind group"),
            layout: &geometry_frame_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: camera_buffer.as_entire_binding(),
            }],
        });
        let lighting_frame_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("deferred+ lighting frame bind group"),
            layout: &lighting_frame_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: camera_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: lights_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: cluster_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: cluster_bitmask_buffer.as_entire_binding(),
                },
            ],
        });
        let culling_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("deferred+ cluster culling bind group"),
            layout: &culling_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: cluster_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: lights_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: cluster_bitmask_buffer.as_entire_binding(),
                },
            ],
        });

        // ---- G-Buffer ----
        let gbuffer_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("deferred+ G-Buffer sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });
        let gbuffer_base = create_gbuffer_color(device, "deferred+ G-Buffer base", width, height, GBUFFER_BASE_FORMAT);
        let gbuffer_normal = create_gbuffer_color(device, "deferred+ G-Buffer normal", width, height, GBUFFER_NORMAL_FORMAT);
        let gbuffer_emissive = create_gbuffer_color(device, "deferred+ G-Buffer emissive", width, height, GBUFFER_EMISSIVE_FORMAT);
        let gbuffer_depth = create_gbuffer_depth(device, "deferred+ G-Buffer depth", width, height, descriptor.depth_format);
        let gbuffer_bind_group = create_gbuffer_bind_group(
            device,
            &gbuffer_bind_group_layout,
            &gbuffer_base,
            &gbuffer_normal,
            &gbuffer_emissive,
            &gbuffer_depth,
            &gbuffer_sampler,
        );

        let default_textures = DefaultTextures::new(device, queue);

        Self {
            final_color_format: descriptor.color_format,
            depth_format: descriptor.depth_format,
            sample_count: descriptor.sample_count,
            width,
            height,
            z_near: 0.1,
            z_far: 1000.0,
            camera_buffer,
            lights_buffer,
            cluster_buffer,
            cluster_bitmask_buffer,
            geometry_frame_layout,
            lighting_frame_layout,
            culling_layout,
            material_bind_group_layout,
            object_bind_group_layout,
            gbuffer_bind_group_layout,
            geometry_frame_bind_group,
            lighting_frame_bind_group,
            culling_bind_group,
            gbuffer_bind_group,
            geometry_pipeline,
            lighting_pipeline,
            culling_pipeline,
            gbuffer_base,
            gbuffer_normal,
            gbuffer_emissive,
            gbuffer_depth,
            gbuffer_sampler,
            default_textures,
            prepared: WgpuRenderQueue::default(),
            cache: GpuResourceCache::new(),
        }
    }

    pub fn final_color_format(&self) -> wgpu::TextureFormat {
        self.final_color_format
    }

    pub fn depth_format(&self) -> wgpu::TextureFormat {
        self.depth_format
    }

    pub fn sample_count(&self) -> u32 {
        self.sample_count
    }
}

impl ScenePipeline for DeferredPlusPipeline {
    fn path(&self) -> RenderingPath {
        RenderingPath::DeferredPlus
    }

    fn resize(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        width: u32,
        height: u32,
        z_near: f32,
        z_far: f32,
    ) {
        let width = width.max(1);
        let height = height.max(1);
        self.z_near = z_near;
        self.z_far = z_far;
        let cluster = ClusterUniform::new(width, height, z_near, z_far, crate::common::identity_matrix());
        queue.write_buffer(&self.cluster_buffer, 0, bytemuck::bytes_of(&cluster));

        // 仅在尺寸变化时重建 G-Buffer，避免每帧申请显存
        if width != self.width || height != self.height {
            self.gbuffer_base = create_gbuffer_color(
                device,
                "deferred+ G-Buffer base",
                width,
                height,
                GBUFFER_BASE_FORMAT,
            );
            self.gbuffer_normal = create_gbuffer_color(
                device,
                "deferred+ G-Buffer normal",
                width,
                height,
                GBUFFER_NORMAL_FORMAT,
            );
            self.gbuffer_emissive = create_gbuffer_color(
                device,
                "deferred+ G-Buffer emissive",
                width,
                height,
                GBUFFER_EMISSIVE_FORMAT,
            );
            self.gbuffer_depth = create_gbuffer_depth(
                device,
                "deferred+ G-Buffer depth",
                width,
                height,
                self.depth_format,
            );
            self.gbuffer_bind_group = create_gbuffer_bind_group(
                device,
                &self.gbuffer_bind_group_layout,
                &self.gbuffer_base,
                &self.gbuffer_normal,
                &self.gbuffer_emissive,
                &self.gbuffer_depth,
                &self.gbuffer_sampler,
            );
            self.width = width;
            self.height = height;
        }
    }

    fn update_camera(
        &mut self,
        queue: &wgpu::Queue,
        view_projection: [[f32; 4]; 4],
        camera_position: [f32; 3],
    ) {
        let camera = CameraUniform::new(view_projection, camera_position);
        queue.write_buffer(&self.camera_buffer, 0, bytemuck::bytes_of(&camera));
        // 同步 cluster uniform 的 inverse VP，供 cluster culling compute shader 使用
        let cluster = ClusterUniform::new(
            self.width,
            self.height,
            self.z_near,
            self.z_far,
            camera.inverse_view_projection,
        );
        queue.write_buffer(&self.cluster_buffer, 0, bytemuck::bytes_of(&cluster));
    }

    fn update_lights(&mut self, queue: &wgpu::Queue, ambient: [f32; 3], lights: &[Light]) {
        let storage = LightStorage::from_lights(ambient, lights);
        queue.write_buffer(&self.lights_buffer, 0, bytemuck::bytes_of(&storage));
    }

    fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        materials: &MaterialLibrary,
        render_queue: &RenderQueue<'_>,
    ) {
        let commands = render_queue
            .commands
            .iter()
            .map(|command| {
                build_command(
                    device,
                    queue,
                    materials,
                    command,
                    &self.material_bind_group_layout,
                    &self.object_bind_group_layout,
                    &self.default_textures,
                    &mut self.cache,
                )
            })
            .collect();
        self.prepared = WgpuRenderQueue { commands };
    }

    fn render(
        &self,
        _device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        color_target: &wgpu::TextureView,
        _depth_target: Option<&wgpu::TextureView>,
    ) {
        // ---- compute: cluster culling ----
        {
            let mut cpass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("deferred+ cluster culling"),
                timestamp_writes: None,
            });
            cpass.set_pipeline(&self.culling_pipeline);
            cpass.set_bind_group(0, &self.culling_bind_group, &[]);
            let groups = (TOTAL_CLUSTERS + CULLING_WORKGROUP_SIZE - 1) / CULLING_WORKGROUP_SIZE;
            cpass.dispatch_workgroups(groups, 1, 1);
        }

        // ---- geometry pass: 写 G-Buffer ----
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("deferred+ geometry pass"),
                color_attachments: &[
                    Some(wgpu::RenderPassColorAttachment {
                        view: &self.gbuffer_base.view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                            store: wgpu::StoreOp::Store,
                        },
                    }),
                    Some(wgpu::RenderPassColorAttachment {
                        view: &self.gbuffer_normal.view,
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
                        view: &self.gbuffer_emissive.view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                            store: wgpu::StoreOp::Store,
                        },
                    }),
                ],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.gbuffer_depth.view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(&self.geometry_pipeline);
            pass.set_bind_group(0, &self.geometry_frame_bind_group, &[]);
            for command in &self.prepared.commands {
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

        // ---- lighting pass: 全屏 quad → final color ----
        {
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
            pass.set_pipeline(&self.lighting_pipeline);
            pass.set_bind_group(0, &self.lighting_frame_bind_group, &[]);
            pass.set_bind_group(1, &self.gbuffer_bind_group, &[]);
            pass.draw(0..3, 0..1);
        }
    }
}

// -------- helpers --------

fn gbuffer_layout_entries() -> [wgpu::BindGroupLayoutEntry; 5] {
    let color = |binding: u32| wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Texture {
            sample_type: wgpu::TextureSampleType::Float { filterable: true },
            view_dimension: wgpu::TextureViewDimension::D2,
            multisampled: false,
        },
        count: None,
    };
    [
        color(0),
        color(1),
        color(2),
        wgpu::BindGroupLayoutEntry {
            binding: 3,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Depth,
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        },
        wgpu::BindGroupLayoutEntry {
            binding: 4,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
            count: None,
        },
    ]
}

fn create_gbuffer_color(
    device: &wgpu::Device,
    label: &str,
    width: u32,
    height: u32,
    format: wgpu::TextureFormat,
) -> GBufferTexture {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    GBufferTexture { texture, view }
}

fn create_gbuffer_depth(
    device: &wgpu::Device,
    label: &str,
    width: u32,
    height: u32,
    format: wgpu::TextureFormat,
) -> GBufferTexture {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor {
        label: Some("deferred+ G-Buffer depth view"),
        format: Some(format),
        dimension: Some(wgpu::TextureViewDimension::D2),
        aspect: wgpu::TextureAspect::DepthOnly,
        base_mip_level: 0,
        mip_level_count: None,
        base_array_layer: 0,
        array_layer_count: None,
    });
    GBufferTexture { texture, view }
}

fn create_gbuffer_bind_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    base: &GBufferTexture,
    normal: &GBufferTexture,
    emissive: &GBufferTexture,
    depth: &GBufferTexture,
    sampler: &wgpu::Sampler,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("deferred+ G-Buffer bind group"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&base.view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(&normal.view),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::TextureView(&emissive.view),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: wgpu::BindingResource::TextureView(&depth.view),
            },
            wgpu::BindGroupEntry {
                binding: 4,
                resource: wgpu::BindingResource::Sampler(sampler),
            },
        ],
    })
}
