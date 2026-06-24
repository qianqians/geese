use wgpu::util::DeviceExt;

use crate::cluster::{ClusterUniform, TOTAL_CLUSTERS};
use crate::common::{
    create_sampler, identity_matrix, joint_uniforms, maybe_upload, CameraUniform,
    DefaultTextures, GpuVertex, MaterialUniform, ObjectUniform, WgpuRenderCommand, WgpuRenderQueue,
};
use crate::light::{Light, LightStorage};
use crate::pipeline::{RenderingPath, ScenePipeline, ScenePipelineDescriptor};
use crate::{MaterialLibrary, RenderQueue};

const PBR_COMMON: &str = include_str!("../shaders/pbr_common.wgsl");
const FORWARD_PLUS_WGSL: &str = include_str!("../shaders/forward_plus.wgsl");
const CLUSTER_CULLING_WGSL: &str = include_str!("../shaders/cluster_culling.wgsl");

const CLUSTER_BITMASK_SIZE: u64 = (TOTAL_CLUSTERS as u64) * 4;
const CULLING_WORKGROUP_SIZE: u32 = 64;

pub struct ForwardPlusPipeline {
    color_format: wgpu::TextureFormat,
    depth_format: wgpu::TextureFormat,
    sample_count: u32,

    camera_buffer: wgpu::Buffer,
    lights_buffer: wgpu::Buffer,
    cluster_buffer: wgpu::Buffer,
    #[allow(dead_code)] // 仅作为 bind group 引用的资源持有，shader 端不直接访问
    cluster_bitmask_buffer: wgpu::Buffer,

    frame_bind_group: wgpu::BindGroup,
    culling_bind_group: wgpu::BindGroup,

    material_bind_group_layout: wgpu::BindGroupLayout,
    object_bind_group_layout: wgpu::BindGroupLayout,

    forward_pipeline: wgpu::RenderPipeline,
    culling_pipeline: wgpu::ComputePipeline,

    default_textures: DefaultTextures,
    prepared: WgpuRenderQueue,
}

impl ForwardPlusPipeline {
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        descriptor: &ScenePipelineDescriptor,
    ) -> Self {
        debug_assert_eq!(descriptor.rendering_path, RenderingPath::ForwardPlus);

        // ---- shader 拼接 ----
        let forward_src = format!("{PBR_COMMON}\n{FORWARD_PLUS_WGSL}");
        let culling_src = format!("{PBR_COMMON}\n{CLUSTER_CULLING_WGSL}");

        let forward_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("forward_plus shader"),
            source: wgpu::ShaderSource::Wgsl(forward_src.into()),
        });
        let culling_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("cluster_culling shader"),
            source: wgpu::ShaderSource::Wgsl(culling_src.into()),
        });

        // ---- frame bind group layout (group 0) ----
        let frame_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("forward+ frame bind group layout"),
                entries: &[
                    uniform_entry(0, wgpu::ShaderStages::VERTEX_FRAGMENT),
                    uniform_entry(1, wgpu::ShaderStages::FRAGMENT),
                    uniform_entry(2, wgpu::ShaderStages::FRAGMENT),
                    storage_read_entry(3, wgpu::ShaderStages::FRAGMENT),
                ],
            });

        // ---- material bind group layout (group 1) ----
        let material_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("forward+ material bind group layout"),
                entries: &material_layout_entries(),
            });

        // ---- object bind group layout (group 2) ----
        let object_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("forward+ object bind group layout"),
                entries: &[uniform_entry(0, wgpu::ShaderStages::VERTEX_FRAGMENT)],
            });

        // ---- 渲染 pipeline ----
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("forward+ pipeline layout"),
            bind_group_layouts: &[
                &frame_bind_group_layout,
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

        let forward_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("forward+ pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &forward_shader,
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
                module: &forward_shader,
                entry_point: "fs_main",
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &color_targets,
            }),
            multiview: None,
            cache: None,
        });

        // ---- compute pipeline (cluster culling) ----
        let culling_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("cluster culling bind group layout"),
            entries: &[
                uniform_entry(0, wgpu::ShaderStages::COMPUTE),
                uniform_entry(1, wgpu::ShaderStages::COMPUTE),
                storage_rw_entry(2, wgpu::ShaderStages::COMPUTE),
            ],
        });
        let culling_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("cluster culling pipeline layout"),
            bind_group_layouts: &[&culling_layout],
            push_constant_ranges: &[],
        });
        let culling_pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("cluster culling pipeline"),
            layout: Some(&culling_pipeline_layout),
            module: &culling_shader,
            entry_point: "cs_main",
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            cache: None,
        });

        // ---- buffers ----
        let camera_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("forward+ camera buffer"),
            contents: bytemuck::bytes_of(&CameraUniform::placeholder()),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let lights_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("forward+ lights buffer"),
            contents: bytemuck::bytes_of(&LightStorage::empty()),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let cluster_uniform = ClusterUniform::new(
            descriptor.width.max(1),
            descriptor.height.max(1),
            0.1,
            1000.0,
        );
        let cluster_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("forward+ cluster uniform buffer"),
            contents: bytemuck::bytes_of(&cluster_uniform),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let cluster_bitmask_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("forward+ cluster bitmask buffer"),
            size: CLUSTER_BITMASK_SIZE,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // ---- bind groups ----
        let frame_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("forward+ frame bind group"),
            layout: &frame_bind_group_layout,
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
            label: Some("cluster culling bind group"),
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

        let default_textures = DefaultTextures::new(device, queue);

        Self {
            color_format: descriptor.color_format,
            depth_format: descriptor.depth_format,
            sample_count: descriptor.sample_count,
            camera_buffer,
            lights_buffer,
            cluster_buffer,
            cluster_bitmask_buffer,
            frame_bind_group,
            culling_bind_group,
            material_bind_group_layout,
            object_bind_group_layout,
            forward_pipeline,
            culling_pipeline,
            default_textures,
            prepared: WgpuRenderQueue::default(),
        }
    }

    pub fn color_format(&self) -> wgpu::TextureFormat {
        self.color_format
    }

    pub fn depth_format(&self) -> wgpu::TextureFormat {
        self.depth_format
    }

    pub fn sample_count(&self) -> u32 {
        self.sample_count
    }
}

impl ScenePipeline for ForwardPlusPipeline {
    fn path(&self) -> RenderingPath {
        RenderingPath::ForwardPlus
    }

    fn resize(
        &mut self,
        _device: &wgpu::Device,
        queue: &wgpu::Queue,
        width: u32,
        height: u32,
        z_near: f32,
        z_far: f32,
    ) {
        let cluster = ClusterUniform::new(width, height, z_near, z_far);
        queue.write_buffer(&self.cluster_buffer, 0, bytemuck::bytes_of(&cluster));
    }

    fn update_camera(
        &mut self,
        queue: &wgpu::Queue,
        view_projection: [[f32; 4]; 4],
        camera_position: [f32; 3],
    ) {
        let camera = CameraUniform::new(view_projection, camera_position);
        queue.write_buffer(&self.camera_buffer, 0, bytemuck::bytes_of(&camera));
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
        depth_target: Option<&wgpu::TextureView>,
    ) {
        // ---- compute: cluster culling ----
        {
            let mut cpass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("forward+ cluster culling"),
                timestamp_writes: None,
            });
            cpass.set_pipeline(&self.culling_pipeline);
            cpass.set_bind_group(0, &self.culling_bind_group, &[]);
            let groups = (TOTAL_CLUSTERS + CULLING_WORKGROUP_SIZE - 1) / CULLING_WORKGROUP_SIZE;
            cpass.dispatch_workgroups(groups, 1, 1);
        }

        // ---- forward render pass ----
        let depth = depth_target.expect("forward+ requires depth target");
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

        pass.set_pipeline(&self.forward_pipeline);
        pass.set_bind_group(0, &self.frame_bind_group, &[]);
        for command in &self.prepared.commands {
            pass.set_bind_group(1, &command.material_bind_group, &[]);
            pass.set_bind_group(2, &command.object_bind_group, &[]);
            pass.set_vertex_buffer(0, command.vertex_buffer.slice(..));
            pass.set_index_buffer(command.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
            pass.draw_indexed(0..command.index_count, 0, 0..1);
        }
    }
}

// -------- helper: bind group layout entries --------

pub(crate) fn uniform_entry(binding: u32, vis: wgpu::ShaderStages) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: vis,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Uniform,
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}

pub(crate) fn storage_read_entry(binding: u32, vis: wgpu::ShaderStages) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: vis,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Storage { read_only: true },
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}

pub(crate) fn storage_rw_entry(binding: u32, vis: wgpu::ShaderStages) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: vis,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Storage { read_only: false },
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}

pub(crate) fn material_layout_entries() -> [wgpu::BindGroupLayoutEntry; 7] {
    let tex = |binding: u32| wgpu::BindGroupLayoutEntry {
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
        uniform_entry(0, wgpu::ShaderStages::FRAGMENT),
        tex(1),
        tex(2),
        tex(3),
        tex(4),
        tex(5),
        wgpu::BindGroupLayoutEntry {
            binding: 6,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
            count: None,
        },
    ]
}

// -------- helper: 单个 RenderCommand 的资源构建 --------

pub(crate) fn build_command(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    materials: &MaterialLibrary,
    command: &crate::RenderCommand<'_>,
    material_layout: &wgpu::BindGroupLayout,
    object_layout: &wgpu::BindGroupLayout,
    defaults: &DefaultTextures,
) -> WgpuRenderCommand {
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

    let has_tangent_uv = command.mesh.flags.has_tangents && command.mesh.flags.has_uv0;
    let mat_uniform = MaterialUniform::from_material(command.material, has_tangent_uv);
    let material_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("scene material buffer"),
        contents: bytemuck::bytes_of(&mat_uniform),
        usage: wgpu::BufferUsages::UNIFORM,
    });

    let base_color = maybe_upload(device, queue, materials, command.material.base_color_texture);
    let mr = maybe_upload(
        device,
        queue,
        materials,
        command.material.metallic_roughness_texture,
    );
    let normal_handle = if has_tangent_uv {
        command.material.normal_texture
    } else {
        None
    };
    let normal = maybe_upload(device, queue, materials, normal_handle);
    let occlusion = maybe_upload(device, queue, materials, command.material.occlusion_texture);
    let emissive = maybe_upload(device, queue, materials, command.material.emissive_texture);

    // 自定义 sampler：取 base_color 的 sampler，缺省回退到 default
    let sampler = command
        .material
        .base_color_texture
        .and_then(|h| materials.texture(h))
        .map(|tex| create_sampler(device, tex));
    let sampler_ref: &wgpu::Sampler = sampler.as_ref().unwrap_or(&defaults.sampler);

    let base_view = base_color
        .as_ref()
        .map(|u| &u.view)
        .unwrap_or(&defaults.white_view);
    let mr_view = mr
        .as_ref()
        .map(|u| &u.view)
        .unwrap_or(&defaults.metallic_roughness_view);
    let normal_view = normal
        .as_ref()
        .map(|u| &u.view)
        .unwrap_or(&defaults.normal_view);
    let occlusion_view = occlusion
        .as_ref()
        .map(|u| &u.view)
        .unwrap_or(&defaults.occlusion_view);
    let emissive_view = emissive
        .as_ref()
        .map(|u| &u.view)
        .unwrap_or(&defaults.black_view);

    let material_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("scene material bind group"),
        layout: material_layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: material_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(base_view),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::TextureView(mr_view),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: wgpu::BindingResource::TextureView(normal_view),
            },
            wgpu::BindGroupEntry {
                binding: 4,
                resource: wgpu::BindingResource::TextureView(occlusion_view),
            },
            wgpu::BindGroupEntry {
                binding: 5,
                resource: wgpu::BindingResource::TextureView(emissive_view),
            },
            wgpu::BindGroupEntry {
                binding: 6,
                resource: wgpu::BindingResource::Sampler(sampler_ref),
            },
        ],
    });

    let object_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("scene object buffer"),
        contents: bytemuck::bytes_of(&ObjectUniform {
            model: command.model_matrix,
            normal: command.normal_matrix,
            skin: [command.mesh.flags.has_skin as u32, 0, 0, 0],
            joints: joint_uniforms(command.joint_matrices),
        }),
        usage: wgpu::BufferUsages::UNIFORM,
    });
    let object_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("scene object bind group"),
        layout: object_layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: object_buffer.as_entire_binding(),
        }],
    });

    // 收集本帧上传的纹理资源以维持生命周期
    let mut uploaded_textures = Vec::new();
    let mut uploaded_views = Vec::new();
    for slot in [base_color, mr, normal, occlusion, emissive].into_iter().flatten() {
        uploaded_textures.push(slot.texture);
        uploaded_views.push(slot.view);
    }

    // 抑制未用变量警告
    let _ = identity_matrix;

    WgpuRenderCommand {
        entity_id: command.entity_id.to_string(),
        vertex_buffer,
        index_buffer,
        index_count: command.mesh.indices.len() as u32,
        material_buffer,
        material_bind_group,
        object_buffer,
        object_bind_group,
        uploaded_textures,
        uploaded_views,
    }
}
