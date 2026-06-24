//! 绗?3 绾挎潯娓叉煋妯″潡锛堢紪杈戝櫒缃戞牸 / Debug 绾挎绛夛級銆?
//!
//! 浣跨敤 `wgpu::PrimitiveTopology::LineList` 锛屽湪 GPU 涓婄粯鍒剁嚎娈碉紝
//! 鍚?3D 鍦烘櫙鍏变韩鍚屼竴涓?render pass 涓旀繁搴︽祴璇曘€?
//! 鐩告姌浜?editor 灞?egui painter 鎵嬪姩鎶曞奖瀵艰嚧鐨勭綉鏍兼柇瑁傞棶棰樸€?

use bytemuck::{Pod, Zeroable};

const LINES_WGSL: &str = include_str!("../shaders/lines.wgsl");

/// 绾挎潯椤剁偣锛堜綅缃?+ RGBA 棰滆壊锛屾灉閬撻涓婅壊锛夈€?
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct LineVertex {
    pub position: [f32; 3],
    pub color: [f32; 4],
}

impl LineVertex {
    fn layout<'a>() -> wgpu::VertexBufferLayout<'a> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<LineVertex>() as wgpu::BufferAddress,
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
                    format: wgpu::VertexFormat::Float32x4,
                },
            ],
        }
    }
}

/// 绾挎潯娓叉煋鍣ㄣ€傝嚜鍖呭惈 pipeline 涓?camera uniform锛屽彲鍦ㄤ换鎰?render pass 涓粯鍒躲€?
pub struct LineRenderer {
    pipeline: wgpu::RenderPipeline,
    camera_buffer: wgpu::Buffer,
    camera_bind_group: wgpu::BindGroup,
    vertex_buffer: wgpu::Buffer,
    /// 褰撳墠缂撳瓨鐨勯《鐐规暟锛堟瘡 2 涓?1 鏉＄嚎娈碉級銆?
    vertex_count: u32,
    /// 缂撳啿鍖哄瓙閲忥紙鍗曚綅锛氶《鐐癸級銆?
    capacity: u32,
}

impl LineRenderer {
    pub fn new(
        device: &wgpu::Device,
        color_format: wgpu::TextureFormat,
        depth_format: wgpu::TextureFormat,
        sample_count: u32,
    ) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("lines shader"),
            source: wgpu::ShaderSource::Wgsl(LINES_WGSL.into()),
        });

        let camera_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("lines camera uniform"),
            size: std::mem::size_of::<crate::CameraUniform>() as wgpu::BufferAddress,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let camera_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("lines camera layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });

        let camera_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("lines camera bind group"),
            layout: &camera_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: camera_buffer.as_entire_binding(),
            }],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("lines pipeline layout"),
            bind_group_layouts: &[&camera_layout],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("lines pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "vs_main",
                compilation_options: Default::default(),
                buffers: &[LineVertex::layout()],
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
                topology: wgpu::PrimitiveTopology::LineList,
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

        // 鍒濆鑳藉沖噺锛?4096 涓ラ《鐐?锛?2048 鏉＄嚎娈碉級
        let initial_capacity: u32 = 4096;
        let vertex_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("lines vertex buffer"),
            size: (initial_capacity as wgpu::BufferAddress)
                * std::mem::size_of::<LineVertex>() as wgpu::BufferAddress,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        Self {
            pipeline,
            camera_buffer,
            camera_bind_group,
            vertex_buffer,
            vertex_count: 0,
            capacity: initial_capacity,
        }
    }

    /// 鏇存柊鐩告満 uniform锛堜笌涓荤鐞跨嚎鍏变韩鍚屼竴濂楁姇褰辩煩闃碉級銆?
    pub fn update_camera(
        &self,
        queue: &wgpu::Queue,
        view_projection: [[f32; 4]; 4],
        camera_position: [f32; 3],
    ) {
        let uniform = crate::CameraUniform::new(view_projection, camera_position);
        queue.write_buffer(&self.camera_buffer, 0, bytemuck::bytes_of(&uniform));
    }

    /// 涓婁紶绾挎绠椾釜椤剁偣銆傚繀瑕佹椂鑷姁鎵╁缂插啿鍖恒€?
    pub fn upload(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, vertices: &[LineVertex]) {
        self.vertex_count = vertices.len() as u32;

        if vertices.is_empty() {
            return;
        }

        if self.vertex_count > self.capacity {
            let new_capacity = self.vertex_count.next_power_of_two().max(self.capacity * 2);
            self.vertex_buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("lines vertex buffer"),
                size: (new_capacity as wgpu::BufferAddress)
                    * std::mem::size_of::<LineVertex>() as wgpu::BufferAddress,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            self.capacity = new_capacity;
        }

        queue.write_buffer(&self.vertex_buffer, 0, bytemuck::cast_slice(vertices));
    }

    /// 鍦ㄥ凡寮€濮嬬殑 render pass 涓粯鍒剁嚎鏉°€傝皟鐢ㄨ€呴』鍦ㄥ悓涓€涓?pass 鍐咃紝鍦ㄤ富鍦烘櫙缁樺埗涔嬪悗璋冪敤锛屼互渚跨綉鏍艰鐪熷疄鍑犱綍瑕嗙洊銆?
    pub fn draw<'a>(&'a self, pass: &mut wgpu::RenderPass<'a>) {
        if self.vertex_count == 0 {
            return;
        }
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.camera_bind_group, &[]);
        pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
        pass.draw(0..self.vertex_count, 0..1);
    }

    /// 褰撳墠缂撳瓨鐨勭嚎娈垫暟锛堥《鐐规暟 / 2锛夈€?
    pub fn line_count(&self) -> u32 {
        self.vertex_count / 2
    }
}
