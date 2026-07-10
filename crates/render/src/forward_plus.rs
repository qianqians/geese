use wgpu::util::DeviceExt;

use crate::cluster::{ClusterUniform, TOTAL_CLUSTERS};
use crate::common::{
    compute_mesh_key, create_sampler, identity_matrix, joint_uniforms, upload_texture,
    CameraUniform, CachedEntityResources, CachedMeshBuffers, CachedTexture, DefaultTextures,
    GpuResourceCache, GpuVertex, MaterialUniform, ObjectUniform, WgpuRenderCommand,
    WgpuRenderQueue,
};
use crate::light::{Light, LightStorage};
use crate::pipeline::{RenderingPath, ScenePipeline, ScenePipelineDescriptor};
use crate::shadow::{CascadeConfig, CsmUniform};
use crate::shadow_pass::{ShadowPass, WgpuShadowAtlas};
use crate::{MaterialLibrary, RenderQueue};

#[cfg(feature = "profiling")]
use crate::profiler::GpuProfiler;

#[cfg(feature = "instancing")]
use crate::common::InstanceData;

const PBR_COMMON: &str = include_str!("../shaders/pbr_common.wgsl");
const FORWARD_PLUS_WGSL: &str = include_str!("../shaders/forward_plus.wgsl");
const CLUSTER_CULLING_WGSL: &str = include_str!("../shaders/cluster_culling.wgsl");

#[cfg(feature = "instancing")]
const FORWARD_PLUS_INSTANCED_WGSL: &str =
    include_str!("../shaders/forward_plus_instanced.wgsl");

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

    /// Instanced 渲染管线（feature = "instancing" 时启用）
    #[cfg(feature = "instancing")]
    instanced_pipeline: wgpu::RenderPipeline,
    /// 实例数据 buffer，每帧重建
    #[cfg(feature = "instancing")]
    instance_buffer: wgpu::Buffer,
    /// Instance bind group（绑定 instance buffer 到 group 2）
    #[cfg(feature = "instancing")]
    instance_bind_group: wgpu::BindGroup,
    /// Instance bind group layout
    #[cfg(feature = "instancing")]
    #[allow(dead_code)]
    instance_bind_group_layout: wgpu::BindGroupLayout,

    default_textures: DefaultTextures,
    prepared: WgpuRenderQueue,
    cache: GpuResourceCache,

    shadow_pass: Option<ShadowPass>,
    shadow_atlas: Option<WgpuShadowAtlas>,

    /// GPU profiler（feature = "profiling" 时启用）
    #[cfg(feature = "profiling")]
    profiler: GpuProfiler,

    /// 缓存 viewport 尺寸，供 update_camera 同步 cluster inverse VP 使用
    cluster_width: u32,
    cluster_height: u32,
    cluster_z_near: f32,
    cluster_z_far: f32,
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

        // ---- instanced 渲染 pipeline ----
        #[cfg(feature = "instancing")]
        let (instanced_pipeline, instance_bind_group_layout, instance_buffer, instance_bind_group) = {
            let instanced_src = format!("{PBR_COMMON}\n{FORWARD_PLUS_INSTANCED_WGSL}");
            let instanced_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("forward+ instanced shader"),
                source: wgpu::ShaderSource::Wgsl(instanced_src.into()),
            });

            // Group 2 for instances: storage buffer (read-only)
            let instance_layout =
                device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: Some("forward+ instance bind group layout"),
                    entries: &[wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::VERTEX,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Storage { read_only: true },
                            has_dynamic_offset: false,
                            min_binding_size: std::num::NonZeroU64::new(
                                std::mem::size_of::<InstanceData>() as u64,
                            ),
                        },
                        count: None,
                    }],
                });

            let instanced_pipeline_layout =
                device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    label: Some("forward+ instanced pipeline layout"),
                    bind_group_layouts: &[
                        &frame_bind_group_layout,
                        &material_bind_group_layout,
                        &instance_layout,
                    ],
                    push_constant_ranges: &[],
                });

            let instanced_pipeline =
                device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                    label: Some("forward+ instanced pipeline"),
                    layout: Some(&instanced_pipeline_layout),
                    vertex: wgpu::VertexState {
                        module: &instanced_shader,
                        entry_point: "vs_main_instanced",
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
                        module: &instanced_shader,
                        entry_point: "fs_main",
                        compilation_options: wgpu::PipelineCompilationOptions::default(),
                        targets: &color_targets,
                    }),
                    multiview: None,
                    cache: None,
                });

            // 预分配 instance buffer（可容纳 1024 个实例）
            let max_instances: u64 = 1024;
            let instance_buf = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("forward+ instance buffer"),
                size: max_instances * std::mem::size_of::<InstanceData>() as u64,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            let instance_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("forward+ instance bind group"),
                layout: &instance_layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: instance_buf.as_entire_binding(),
                }],
            });

            (instanced_pipeline, instance_layout, instance_buf, instance_bg)
        };

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
            crate::common::identity_matrix(),
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
            cache: GpuResourceCache::new(),
            shadow_pass: None,
            shadow_atlas: None,

            #[cfg(feature = "profiling")]
            profiler: GpuProfiler::new(device, 16),

            #[cfg(feature = "instancing")]
            instanced_pipeline,
            #[cfg(feature = "instancing")]
            instance_buffer,
            #[cfg(feature = "instancing")]
            instance_bind_group,
            #[cfg(feature = "instancing")]
            instance_bind_group_layout,

            cluster_width: descriptor.width.max(1),
            cluster_height: descriptor.height.max(1),
            cluster_z_near: 0.1,
            cluster_z_far: 1000.0,
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

    /// Enable CSM shadow mapping. Creates the shadow atlas and depth-only render pipeline.
    pub fn enable_shadows(
        &mut self,
        device: &wgpu::Device,
        config: &CascadeConfig,
    ) {
        let shadow_atlas = WgpuShadowAtlas::new(device, config);
        let shadow_pass = ShadowPass::new(
            device,
            config,
            &self.object_bind_group_layout,
        );
        self.shadow_atlas = Some(shadow_atlas);
        self.shadow_pass = Some(shadow_pass);
    }

    /// Write CSM uniform and per-cascade VP matrices to GPU buffers.
    /// Must be called before [`render`] if shadows are enabled.
    pub fn update_shadows(
        &self,
        queue: &wgpu::Queue,
        cascade_vps: &[[[f32; 4]; 4]],
        csm_uniform: &CsmUniform,
    ) {
        if let (Some(sp), Some(atlas)) = (&self.shadow_pass, &self.shadow_atlas) {
            sp.update_cascade_vps(queue, cascade_vps);
            atlas.write_csm_uniform(queue, csm_uniform);
        }
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
        self.cluster_width = width.max(1);
        self.cluster_height = height.max(1);
        self.cluster_z_near = z_near;
        self.cluster_z_far = z_far;
        let cluster = ClusterUniform::new(
            self.cluster_width,
            self.cluster_height,
            z_near,
            z_far,
            crate::common::identity_matrix(),
        );
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
        // 同步 cluster uniform 的 inverse VP，供 cluster culling compute shader 使用
        let mut cluster = ClusterUniform::new(
            self.cluster_width,
            self.cluster_height,
            self.cluster_z_near,
            self.cluster_z_far,
            camera.inverse_view_projection,
        );
        // 保留 depth slice 参数兼容
        cluster.flags[0] = 1.0;
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
        let mut commands: Vec<WgpuRenderCommand> = render_queue
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

        // ---- GPU Instancing: 按 mesh_key 分组连续的相同 mesh 命令 ----
        #[cfg(feature = "instancing")]
        {
            commands = group_instanced_commands(commands, &self.instance_buffer, queue);
        }

        self.prepared = WgpuRenderQueue { commands };
    }

    fn render(
        &self,
        _device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        color_target: &wgpu::TextureView,
        depth_target: Option<&wgpu::TextureView>,
    ) {
        // ---- begin frame profiling ----
        #[cfg(feature = "profiling")]
        self.profiler.begin_frame();

        // ---- shadow pass (optional) ----
        if let (Some(sp), Some(atlas)) = (&self.shadow_pass, &self.shadow_atlas) {
            #[cfg(feature = "profiling")]
            let shadow_tw = self.profiler.render_pass_writes("shadow");
            #[cfg(not(feature = "profiling"))]
            let shadow_tw: Option<wgpu::RenderPassTimestampWrites> = None;
            sp.render(encoder, &atlas.view, &self.cache, &self.prepared.commands, shadow_tw);
        }

        // ---- compute: cluster culling ----
        {
            #[cfg(feature = "profiling")]
            let culling_tw = self.profiler.compute_pass_writes("cluster_culling");
            #[cfg(not(feature = "profiling"))]
            let culling_tw: Option<wgpu::ComputePassTimestampWrites> = None;

            let mut cpass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("forward+ cluster culling"),
                timestamp_writes: culling_tw,
            });
            cpass.set_pipeline(&self.culling_pipeline);
            cpass.set_bind_group(0, &self.culling_bind_group, &[]);
            let groups = (TOTAL_CLUSTERS + CULLING_WORKGROUP_SIZE - 1) / CULLING_WORKGROUP_SIZE;
            cpass.dispatch_workgroups(groups, 1, 1);
        }

        // ---- forward render pass ----
        let depth = match depth_target {
            Some(t) => t,
            None => {
                log::error!("[render] Missing depth target, skipping forward+ render pass");
                return;
            }
        };

        #[cfg(feature = "profiling")]
        let forward_tw = self.profiler.render_pass_writes("forward+");
        #[cfg(not(feature = "profiling"))]
        let forward_tw: Option<wgpu::RenderPassTimestampWrites> = None;

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
            timestamp_writes: forward_tw,
            occlusion_query_set: None,
        });

        pass.set_pipeline(&self.forward_pipeline);
        pass.set_bind_group(0, &self.frame_bind_group, &[]);
        for command in &self.prepared.commands {
            let mesh = match self.cache.mesh_buffers.get(&command.mesh_key) {
                Some(m) => m,
                None => continue,
            };
            pass.set_bind_group(1, &command.material_bind_group, &[]);
            pass.set_vertex_buffer(0, mesh.vertex_buffer.slice(..));
            pass.set_index_buffer(mesh.index_buffer.slice(..), wgpu::IndexFormat::Uint32);

            // Instanced path: 当 instance_count > 1 时使用 instanced pipeline
            #[cfg(feature = "instancing")]
            {
                if command.instance_count > 1 {
                    pass.set_pipeline(&self.instanced_pipeline);
                    pass.set_bind_group(2, &self.instance_bind_group, &[]);
                    pass.draw_indexed(0..command.index_count, 0, 0..command.instance_count);
                    pass.set_pipeline(&self.forward_pipeline);
                    continue;
                }
            }

            // Regular path: 单实例，使用 Object uniform
            pass.set_bind_group(2, &command.object_bind_group, &[]);
            pass.draw_indexed(0..command.index_count, 0, 0..1);
        }

        // ---- end frame profiling ----
        #[cfg(feature = "profiling")]
        self.profiler.end_frame(encoder, _device);
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
    cache: &mut GpuResourceCache,
) -> WgpuRenderCommand {
    let mesh_key = compute_mesh_key(command.mesh);
    let index_count = command.lod_index_count.unwrap_or(command.mesh.indices.len() as u32);
    let entity_id = command.entity_id.to_string();

    // ---- 1. Mesh buffers (vertex + index) ----
    if !cache.mesh_buffers.contains_key(&mesh_key) {
        let vertices: Vec<_> = command.mesh.vertices.iter().map(GpuVertex::from).collect();
        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("cached vertex buffer"),
            contents: bytemuck::cast_slice(&vertices),
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        });
        let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("cached index buffer"),
            contents: bytemuck::cast_slice(&command.mesh.indices),
            usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
        });
        cache.mesh_buffers.insert(
            mesh_key,
            CachedMeshBuffers {
                vertex_buffer,
                index_buffer,
            },
        );
    } else {
        let mesh_buf = cache.mesh_buffers.get(&mesh_key).unwrap();
        let vertices: Vec<_> = command.mesh.vertices.iter().map(GpuVertex::from).collect();
        queue.write_buffer(&mesh_buf.vertex_buffer, 0, bytemuck::cast_slice(&vertices));
        queue.write_buffer(
            &mesh_buf.index_buffer,
            0,
            bytemuck::cast_slice(&command.mesh.indices),
        );
    }

    // ---- 2. Textures ----
    let has_tangent_uv = command.mesh.flags.has_tangents && command.mesh.flags.has_uv0;
    let normal_handle = if has_tangent_uv {
        command.material.normal_texture
    } else {
        None
    };
    let tex_handles: [Option<crate::TextureHandle>; 5] = [
        command.material.base_color_texture,
        command.material.metallic_roughness_texture,
        normal_handle,
        command.material.occlusion_texture,
        command.material.emissive_texture,
    ];

    // 确保所有需要的纹理都在缓存中
    for handle in tex_handles.iter().copied().flatten() {
        let idx = handle.0;
        if !cache.textures.contains_key(&idx) {
            if let Some(texture) = materials.texture(handle) {
                let uploaded = upload_texture(device, queue, texture);
                let view = uploaded.create_view(&wgpu::TextureViewDescriptor::default());
                let sampler = create_sampler(device, texture);
                cache.textures.insert(
                    idx,
                    CachedTexture {
                        texture: uploaded,
                        view,
                        sampler,
                    },
                );
            }
        }
    }

    // 从缓存获取纹理 view 和 sampler
    macro_rules! get_tex_view {
        ($handle:expr, $default:expr) => {
            $handle
                .and_then(|h| cache.textures.get(&h.0))
                .map(|t| &t.view)
                .unwrap_or($default)
        };
    }
    let base_view = get_tex_view!(tex_handles[0], &defaults.white_view);
    let mr_view = get_tex_view!(tex_handles[1], &defaults.metallic_roughness_view);
    let normal_view = get_tex_view!(tex_handles[2], &defaults.normal_view);
    let occlusion_view = get_tex_view!(tex_handles[3], &defaults.occlusion_view);
    let emissive_view = get_tex_view!(tex_handles[4], &defaults.black_view);

    let sampler_ref = tex_handles[0]
        .and_then(|h| cache.textures.get(&h.0))
        .map(|t| &t.sampler)
        .unwrap_or(&defaults.sampler);

    // ---- 3. Material & Object uniform ----
    let mat_uniform = MaterialUniform::from_material(command.material, has_tangent_uv);
    let obj_uniform = ObjectUniform {
        model: command.model_matrix,
        normal: command.normal_matrix,
        skin: [command.mesh.flags.has_skin as u32, 0, 0, 0],
        joints: joint_uniforms(command.joint_matrices),
    };

    // ---- 4. Entity resources (material/object buffer + bind groups) ----
    let tex_handle_indices: [Option<usize>; 5] = [
        tex_handles[0].map(|h| h.0),
        tex_handles[1].map(|h| h.0),
        tex_handles[2].map(|h| h.0),
        tex_handles[3].map(|h| h.0),
        tex_handles[4].map(|h| h.0),
    ];

    let needs_material_bg_rebuild = cache
        .entity_resources
        .get(&entity_id)
        .map(|r| r.tex_handles != tex_handle_indices || r.has_tangent_uv != has_tangent_uv)
        .unwrap_or(true);

    if !cache.entity_resources.contains_key(&entity_id) {
        // 首次创建 entity 资源
        let material_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("cached material buffer"),
            contents: bytemuck::bytes_of(&mat_uniform),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let object_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("cached object buffer"),
            contents: bytemuck::bytes_of(&obj_uniform),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let material_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("cached material bind group"),
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
        let object_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("cached object bind group"),
            layout: object_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: object_buffer.as_entire_binding(),
            }],
        });
        cache.entity_resources.insert(
            entity_id.clone(),
            CachedEntityResources {
                material_buffer,
                object_buffer,
                material_bind_group,
                object_bind_group,
                tex_handles: tex_handle_indices,
                has_tangent_uv,
            },
        );
    } else {
        // 更新已有 entity 资源
        let res = cache.entity_resources.get(&entity_id).unwrap();
        queue.write_buffer(&res.material_buffer, 0, bytemuck::bytes_of(&mat_uniform));
        queue.write_buffer(&res.object_buffer, 0, bytemuck::bytes_of(&obj_uniform));

        if needs_material_bg_rebuild {
            let material_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("cached material bind group"),
                layout: material_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: res.material_buffer.as_entire_binding(),
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
            let res = cache.entity_resources.get_mut(&entity_id).unwrap();
            res.material_bind_group = material_bind_group;
            res.tex_handles = tex_handle_indices;
            res.has_tangent_uv = has_tangent_uv;
        }
    }

    // ---- 5. 构建 bind group 引用 ----
    let res = cache.entity_resources.get(&entity_id).unwrap();

    // Bind group 是 wgpu 内部引用计数资源，可以通过 create_bind_group 的返回值安全地
    // 独立存在，不受缓存借用生命周期的约束。但为了简单起见，这里每帧重建 bind group。
    let material_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("frame material bind group"),
        layout: material_layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: res.material_buffer.as_entire_binding(),
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
    let object_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("frame object bind group"),
        layout: object_layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: res.object_buffer.as_entire_binding(),
        }],
    });

    // 抑制未用变量警告
    let _ = identity_matrix;

    WgpuRenderCommand {
        entity_id,
        mesh_key,
        index_count,
        material_bind_group,
        object_bind_group,
        model_matrix: command.model_matrix,
        #[cfg(feature = "instancing")]
        instance_count: 1,
        #[cfg(feature = "instancing")]
        instance_models: vec![command.model_matrix],
    }
}

// -------- GPU Instancing: 分组连续的相同 mesh 命令，合并实例数据 --------

/// 按 `mesh_key` 分组连续的命令，将同 mesh 的多个绘制合并为一次 instanced draw。
/// 仅当 `instancing` feature 启用时编译。
#[cfg(feature = "instancing")]
fn group_instanced_commands(
    mut commands: Vec<WgpuRenderCommand>,
    instance_buffer: &wgpu::Buffer,
    queue: &wgpu::Queue,
) -> Vec<WgpuRenderCommand> {
    if commands.is_empty() {
        return commands;
    }

    let mut grouped: Vec<WgpuRenderCommand> = Vec::with_capacity(commands.len());
    let mut batch_start: usize = 0;

    for i in 1..=commands.len() {
        let flush = i == commands.len() || commands[i].mesh_key != commands[batch_start].mesh_key;
        if flush {
            let batch_size = i - batch_start;
            if batch_size == 1 {
                // 单实例：保持原样
                grouped.push(commands.swap_remove(batch_start));
            } else {
                // 多实例：合并为一条 instanced 命令
                let mut base = commands.swap_remove(batch_start);
                // 收集 batch 中剩余命令的 model 矩阵
                let mut models = base.instance_models;
                for _ in 1..batch_size {
                    let cmd = commands.swap_remove(batch_start);
                    models.extend(cmd.instance_models);
                }
                base.instance_count = models.len() as u32;
                base.instance_models = models;
                grouped.push(base);
            }
            batch_start = i;
        }
    }

    // 将所有实例数据写入 instance buffer
    let mut all_instances: Vec<InstanceData> = Vec::new();
    for cmd in &grouped {
        if cmd.instance_count > 1 {
            for model in &cmd.instance_models {
                all_instances.push(InstanceData { model: *model });
            }
        }
    }
    if !all_instances.is_empty() {
        queue.write_buffer(
            instance_buffer,
            0,
            bytemuck::cast_slice(&all_instances),
        );
    }

    grouped
}
