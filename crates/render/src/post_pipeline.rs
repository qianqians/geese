//! 4.7 后处理 GPU 管线：ACES Tonemap + Bloom + SSAO + SSR 全屏 pass。
//!
//! `PostProcessPipeline` 不侵入 `ScenePipeline` trait。调用方在 `pipeline.render()`
//! 之后手动调用 `post.process()`。
//!
//! 消费 `post.rs` 的 `PostChain` / `PostUniform` / `EffectMask` / `build_post_uniform`。

use crate::post::{EffectMask, PostUniform};
use bytemuck::{Pod, Zeroable};
use std::num::NonZeroU64;
use wgpu::util::DeviceExt;

const POST_TONEMAP_WGSL: &str = include_str!("../shaders/post_tonemap.wgsl");
const POST_BLOOM_WGSL: &str = include_str!("../shaders/post_bloom.wgsl");
const POST_SSAO_WGSL: &str = include_str!("../shaders/post_ssao.wgsl");
const POST_SSR_WGSL: &str = include_str!("../shaders/post_ssr.wgsl");
const POST_DOF_WGSL: &str = include_str!("../shaders/post_dof.wgsl");
const POST_MOTION_BLUR_WGSL: &str = include_str!("../shaders/post_motion_blur.wgsl");

/// 后处理管线：持 HDR 中间纹理 + ping-pong 纹理 + uniform buffer + render pipeline。
///
/// 调用方在 `pipeline.render()` 之后手动调用 `process()`。
pub struct PostProcessPipeline {
    color_format: wgpu::TextureFormat,
    width: u32,
    height: u32,

    uniform_buffer: wgpu::Buffer,
    bind_group_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,

    // Bloom ping-pong textures (half-res)
    bloom_a: wgpu::TextureView,
    bloom_b: wgpu::TextureView,
    bloom_a_width: u32,
    bloom_a_height: u32,
    bloom_b_width: u32,
    bloom_b_height: u32,

    // Bind groups for bloom passes (rebuilt on resize)
    bloom_downsample_bg: wgpu::BindGroup,
    bloom_upsample_bg: wgpu::BindGroup,

    tonemap_pipeline: wgpu::RenderPipeline,
    bloom_downsample_pipeline: wgpu::RenderPipeline,
    bloom_upsample_pipeline: wgpu::RenderPipeline,

    // SSAO
    ssao_pipeline: wgpu::RenderPipeline,
    ssao_output: wgpu::TextureView,

    // SSR
    ssr_pipeline: wgpu::RenderPipeline,
    ssr_output: wgpu::TextureView,

    // DoF
    dof_pipeline: wgpu::RenderPipeline,
    dof_output: wgpu::TextureView,

    // Motion Blur
    motion_blur_pipeline: wgpu::RenderPipeline,
    motion_blur_output: wgpu::TextureView,
}

/// Uniform data matching WGSL `PostUniformData` (std140-compatible).
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
struct PostUniformData {
    params: [f32; 4],
    frame: [f32; 4],
    extra: [f32; 4],
}

impl From<PostUniform> for PostUniformData {
    fn from(u: PostUniform) -> Self {
        Self {
            params: u.params,
            frame: u.frame,
            extra: u.extra,
        }
    }
}

impl PostProcessPipeline {
    /// 创建后处理管线。
    ///
    /// `color_format` 是最终输出格式（通常是 `Rgba8Unorm` 或 `Bgra8Unorm`）。
    /// HDR 中间纹理使用 `Rgba16Float`。
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        color_format: wgpu::TextureFormat,
        width: u32,
        height: u32,
    ) -> Self {
        let hdr_format = wgpu::TextureFormat::Rgba16Float;

        // ---- uniform buffer ----
        let uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("post uniform buffer"),
            contents: bytemuck::bytes_of(&PostUniformData::from(crate::post::PostUniform::default())),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        // ---- bind group layout ----
        let bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("post bind group layout"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: NonZeroU64::new(std::mem::size_of::<PostUniformData>() as u64),
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
                    wgpu::BindGroupLayoutEntry {
                        binding: 3,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                ],
            });

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("post sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        // ---- bloom ping-pong textures (half-res) ----
        let half_w = (width / 2).max(1);
        let half_h = (height / 2).max(1);
        let bloom_a = create_hdr_texture_view(device, hdr_format, half_w, half_h);
        let bloom_b = create_hdr_texture_view(device, hdr_format, half_w, half_h);

        // ---- SSAO output texture (full-res) ----
        let ssao_output = create_hdr_texture_view(device, hdr_format, width, height);

        // ---- SSR output texture (full-res) ----
        let ssr_output = create_hdr_texture_view(device, hdr_format, width, height);

        // ---- DoF output texture (full-res) ----
        let dof_output = create_hdr_texture_view(device, hdr_format, width, height);

        // ---- Motion Blur output texture (full-res) ----
        let motion_blur_output = create_hdr_texture_view(device, hdr_format, width, height);

        // ---- pipelines ----
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("post pipeline layout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        let tonemap_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("post tonemap shader"),
            source: wgpu::ShaderSource::Wgsl(POST_TONEMAP_WGSL.into()),
        });
        let bloom_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("post bloom shader"),
            source: wgpu::ShaderSource::Wgsl(POST_BLOOM_WGSL.into()),
        });
        let ssao_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("post ssao shader"),
            source: wgpu::ShaderSource::Wgsl(POST_SSAO_WGSL.into()),
        });
        let ssr_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("post ssr shader"),
            source: wgpu::ShaderSource::Wgsl(POST_SSR_WGSL.into()),
        });
        let dof_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("post dof shader"),
            source: wgpu::ShaderSource::Wgsl(POST_DOF_WGSL.into()),
        });
        let motion_blur_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("post motion blur shader"),
            source: wgpu::ShaderSource::Wgsl(POST_MOTION_BLUR_WGSL.into()),
        });

        let tonemap_pipeline = create_post_pipeline(
            device,
            &pipeline_layout,
            &tonemap_shader,
            "fs_tonemap",
            color_format,
        );
        let bloom_downsample_pipeline = create_post_pipeline(
            device,
            &pipeline_layout,
            &bloom_shader,
            "fs_bloom_downsample",
            hdr_format,
        );
        let bloom_upsample_pipeline = create_post_pipeline(
            device,
            &pipeline_layout,
            &bloom_shader,
            "fs_bloom_upsample",
            hdr_format,
        );
        let ssao_pipeline = create_post_pipeline(
            device,
            &pipeline_layout,
            &ssao_shader,
            "fs_ssao",
            hdr_format,
        );
        let ssr_pipeline = create_post_pipeline(
            device,
            &pipeline_layout,
            &ssr_shader,
            "fs_ssr",
            hdr_format,
        );
        let dof_pipeline = create_post_pipeline(
            device,
            &pipeline_layout,
            &dof_shader,
            "fs_dof",
            hdr_format,
        );
        let motion_blur_pipeline = create_post_pipeline(
            device,
            &pipeline_layout,
            &motion_blur_shader,
            "fs_motion_blur",
            hdr_format,
        );

        // ---- initial bind groups ----
        let bloom_downsample_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("post bloom downsample bind group"),
            layout: &bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&bloom_a),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(&bloom_a),
                },
            ],
        });
        let bloom_upsample_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("post bloom upsample bind group"),
            layout: &bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&bloom_b),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
        });

        let _ = queue; // queue not needed at creation time (no data upload)

        Self {
            color_format,
            width,
            height,
            uniform_buffer,
            bind_group_layout,
            sampler,
            bloom_a,
            bloom_b,
            bloom_a_width: half_w,
            bloom_a_height: half_h,
            bloom_b_width: half_w,
            bloom_b_height: half_h,
            bloom_downsample_bg,
            bloom_upsample_bg,
            tonemap_pipeline,
            bloom_downsample_pipeline,
            bloom_upsample_pipeline,
            ssao_pipeline,
            ssao_output,
            ssr_pipeline,
            ssr_output,
            dof_pipeline,
            dof_output,
            motion_blur_pipeline,
            motion_blur_output,
        }
    }

    /// 重建中间纹理（视口尺寸变化时调用）。
    pub fn resize(&mut self, device: &wgpu::Device, width: u32, height: u32) {
        self.width = width;
        self.height = height;
        let hdr_format = wgpu::TextureFormat::Rgba16Float;
        let half_w = (width / 2).max(1);
        let half_h = (height / 2).max(1);

        self.bloom_a = create_hdr_texture_view(device, hdr_format, half_w, half_h);
        self.bloom_b = create_hdr_texture_view(device, hdr_format, half_w, half_h);
        self.bloom_a_width = half_w;
        self.bloom_a_height = half_h;
        self.bloom_b_width = half_w;
        self.bloom_b_height = half_h;
        self.ssao_output = create_hdr_texture_view(device, hdr_format, width, height);
        self.ssr_output = create_hdr_texture_view(device, hdr_format, width, height);
        self.dof_output = create_hdr_texture_view(device, hdr_format, width, height);
        self.motion_blur_output = create_hdr_texture_view(device, hdr_format, width, height);

        self.bloom_downsample_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("post bloom downsample bind group (resized)"),
            layout: &self.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: self.uniform_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&self.bloom_a),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(&self.bloom_a),
                },
            ],
        });
        self.bloom_upsample_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("post bloom upsample bind group (resized)"),
            layout: &self.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: self.uniform_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&self.bloom_b),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(&self.bloom_b),
                },
            ],
        });
    }

    /// 更新 uniform buffer。
    pub fn update_uniform(&self, queue: &wgpu::Queue, uniform: &PostUniform) {
        let data = PostUniformData::from(*uniform);
        queue.write_buffer(&self.uniform_buffer, 0, bytemuck::bytes_of(&data));
    }

    /// 执行后处理效果链。
    ///
    /// `input_view` 是主渲染管线输出的 HDR/SDR 颜色纹理视图。
    /// `output_view` 是最终输出目标（通常是 surface texture view）。
    ///
    /// 效果链顺序: SSAO → SSR → DoF → MotionBlur → Bloom → Tonemap
    pub fn process(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        input_view: &wgpu::TextureView,
        output_view: &wgpu::TextureView,
        uniform: &PostUniform,
    ) {
        self.update_uniform(queue, uniform);

        let mask = EffectMask::from_bits_truncate(uniform.frame[3].to_bits());

        // ---- 0. SSAO: input_view → ssao_output ----
        // 当 SSAO 启用时，先对场景颜色做环境光遮蔽处理，后续 Bloom/Tonemap 读取 ssao_output。
        let effective_input: &wgpu::TextureView = if mask.contains(EffectMask::SSAO) {
            let ssao_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("post ssao bg"),
                layout: &self.bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: self.uniform_buffer.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(input_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::Sampler(&self.sampler),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: wgpu::BindingResource::TextureView(input_view),
                    },
                ],
            });

            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("post ssao pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &self.ssao_output,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                });
                pass.set_pipeline(&self.ssao_pipeline);
                pass.set_bind_group(0, &ssao_bg, &[]);
                pass.draw(0..3, 0..1);
            }
            &self.ssao_output
        } else {
            input_view
        };

        // ---- 0.5 SSR: effective_input → ssr_output ----
        // 当 SSR 启用时，在 SSAO 之后、Bloom 之前做屏幕空间反射。
        let effective_input: &wgpu::TextureView = if mask.contains(EffectMask::SSR) {
            let ssr_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("post ssr bg"),
                layout: &self.bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: self.uniform_buffer.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(effective_input),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::Sampler(&self.sampler),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: wgpu::BindingResource::TextureView(effective_input),
                    },
                ],
            });

            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("post ssr pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &self.ssr_output,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                });
                pass.set_pipeline(&self.ssr_pipeline);
                pass.set_bind_group(0, &ssr_bg, &[]);
                pass.draw(0..3, 0..1);
            }
            &self.ssr_output
        } else {
            effective_input
        };

        // ---- 0.6 DoF: effective_input → dof_output ----
        // 当 DoF 启用时，基于颜色梯度估算散焦，对场景做景深模糊。
        let effective_input: &wgpu::TextureView = if mask.contains(EffectMask::DOF) {
            let dof_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("post dof bg"),
                layout: &self.bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: self.uniform_buffer.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(effective_input),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::Sampler(&self.sampler),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: wgpu::BindingResource::TextureView(effective_input),
                    },
                ],
            });

            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("post dof pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &self.dof_output,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                });
                pass.set_pipeline(&self.dof_pipeline);
                pass.set_bind_group(0, &dof_bg, &[]);
                pass.draw(0..3, 0..1);
            }
            &self.dof_output
        } else {
            effective_input
        };

        // ---- 0.7 MotionBlur: effective_input → motion_blur_output ----
        // 当 MotionBlur 启用时，基于亮度梯度估算运动方向并做运动模糊。
        let effective_input: &wgpu::TextureView = if mask.contains(EffectMask::MOTION_BLUR) {
            let mb_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("post motion blur bg"),
                layout: &self.bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: self.uniform_buffer.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(effective_input),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::Sampler(&self.sampler),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: wgpu::BindingResource::TextureView(effective_input),
                    },
                ],
            });

            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("post motion blur pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &self.motion_blur_output,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                });
                pass.set_pipeline(&self.motion_blur_pipeline);
                pass.set_bind_group(0, &mb_bg, &[]);
                pass.draw(0..3, 0..1);
            }
            &self.motion_blur_output
        } else {
            effective_input
        };

        // ---- 1. Bloom downsample: effective_input → bloom_b ----
        if mask.contains(EffectMask::BLOOM) {
            // Create a temporary bind group for effective_input → bloom_b
            let downsample_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("post bloom downsample input bg"),
                layout: &self.bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: self.uniform_buffer.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(effective_input),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::Sampler(&self.sampler),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: wgpu::BindingResource::TextureView(effective_input),
                    },
                ],
            });

            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("post bloom downsample pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &self.bloom_b,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                });
                pass.set_pipeline(&self.bloom_downsample_pipeline);
                pass.set_bind_group(0, &downsample_bg, &[]);
                pass.draw(0..3, 0..1);
            }

            // ---- 2. Bloom upsample: bloom_b → bloom_a ----
            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("post bloom upsample pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &self.bloom_a,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                });
                pass.set_pipeline(&self.bloom_upsample_pipeline);
                pass.set_bind_group(0, &self.bloom_upsample_bg, &[]);
                pass.draw(0..3, 0..1);
            }
        }

        // ---- 3. Tonemap: (effective_input + bloom) → output ----
        // effective_input is bound to binding 1; bloom_a is bound to binding 3.
        // The tonemap shader composites input + bloom when BLOOM mask bit is set.
        let tonemap_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("post tonemap bg"),
            layout: &self.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: self.uniform_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(effective_input),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(if mask.contains(EffectMask::BLOOM) {
                        &self.bloom_a
                    } else {
                        effective_input
                    }),
                },
            ],
        });

        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("post tonemap pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: output_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(&self.tonemap_pipeline);
            pass.set_bind_group(0, &tonemap_bg, &[]);
            pass.draw(0..3, 0..1);
        }
    }

    /// 返回输出颜色格式。
    pub fn color_format(&self) -> wgpu::TextureFormat {
        self.color_format
    }
}

fn create_hdr_texture_view(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
    width: u32,
    height: u32,
) -> wgpu::TextureView {
    let tex = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("post hdr intermediate"),
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
    tex.create_view(&wgpu::TextureViewDescriptor::default())
}

fn create_post_pipeline(
    device: &wgpu::Device,
    layout: &wgpu::PipelineLayout,
    shader: &wgpu::ShaderModule,
    fs_entry: &str,
    color_format: wgpu::TextureFormat,
) -> wgpu::RenderPipeline {
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("post pipeline"),
        layout: Some(layout),
        cache: None,
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: "vs_fullscreen",
            buffers: &[],
            compilation_options: wgpu::PipelineCompilationOptions::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: fs_entry,
            targets: &[Some(wgpu::ColorTargetState {
                format: color_format,
                blend: Some(wgpu::BlendState::REPLACE),
                write_mask: wgpu::ColorWrites::ALL,
            })],
            compilation_options: wgpu::PipelineCompilationOptions::default(),
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
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        multiview: None,
    })
}
