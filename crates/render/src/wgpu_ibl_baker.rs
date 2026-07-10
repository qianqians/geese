//! GPU IBL 烘焙器：通过 wgpu compute shader 生成 IBL 资源。
//!
//! `WgpuIblBaker` 实现 [`crate::ibl::IblBaker`] trait，`StubIblBaker` 不受影响。
//!
//! 支持的 `SkyboxKind`:
//! - `SolidColor` → 创建纯色 cubemap texture
//! - `Procedural` → compute shader 大气散射（简化版）
//! - `Hdri` → 首版不实现（需 `image`/`hdr-rs` crate），返回错误
//!
//! BRDF LUT 通过 CPU 预计算后上传 GPU，避免 compute shader 依赖。
//! Irradiance 通过 compute shader 生成。

use std::collections::HashMap;
use std::num::NonZeroU64;

use wgpu::util::DeviceExt;

use crate::ibl::{IblBaker, IblConfig, IblResources, SkyboxKind};

const IBL_BRDF_LUT_WGSL: &str = include_str!("../shaders/ibl_brdf_lut.wgsl");
const IBL_IRRADIANCE_WGSL: &str = include_str!("../shaders/ibl_irradiance.wgsl");

/// 烘焙得到的 GPU 纹理资源。
pub struct BakedIblTextures {
    pub irradiance: wgpu::TextureView,
    pub prefiltered: wgpu::TextureView,
    pub brdf_lut: Option<wgpu::TextureView>,
    pub mip_levels: u32,
}

/// GPU IBL 烘焙器。
///
/// 持有对 `wgpu::Device` 和 `wgpu::Queue` 的引用，通过 compute shader 或
/// CPU 预计算生成 IBL 纹理。
pub struct WgpuIblBaker<'a> {
    device: &'a wgpu::Device,
    queue: &'a wgpu::Queue,
    next_id: u32,
    baked: HashMap<u32, BakedIblTextures>,
    brdf_lut_view: Option<wgpu::TextureView>,
    irradiance_pipeline: wgpu::ComputePipeline,
    brdf_lut_pipeline: wgpu::ComputePipeline,
    irradiance_bind_group_layout: wgpu::BindGroupLayout,
    brdf_lut_bind_group_layout: wgpu::BindGroupLayout,
}

/// IBL 烘焙错误。
#[derive(Debug)]
pub enum IblBakeError {
    Unsupported(String),
    GpuError(String),
}

impl std::fmt::Display for IblBakeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            IblBakeError::Unsupported(msg) => write!(f, "IBL bake unsupported: {msg}"),
            IblBakeError::GpuError(msg) => write!(f, "IBL bake GPU error: {msg}"),
        }
    }
}

impl std::error::Error for IblBakeError {}

// ---------------------------------------------------------------------------
// GPU uniform structs
// ---------------------------------------------------------------------------

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct BrdfLutParams {
    sample_count: u32,
    resolution: u32,
    _pad: u32,
    _pad2: u32,
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct IrradianceParams {
    face_size: u32,
    sample_count: u32,
    face_index: u32,
    _pad: u32,
    sun_direction: [f32; 4],
    ground_albedo: [f32; 4],
    sky_color: [f32; 4],
    horizon_color: [f32; 4],
    ground_color: [f32; 4],
}

impl<'a> WgpuIblBaker<'a> {
    /// 创建一个新的 GPU IBL 烘焙器。
    pub fn new(device: &'a wgpu::Device, queue: &'a wgpu::Queue) -> Self {
        // ---- BRDF LUT compute pipeline ----
        let brdf_lut_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("ibl brdf lut bind group layout"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::COMPUTE,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: NonZeroU64::new(std::mem::size_of::<BrdfLutParams>() as u64),
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::COMPUTE,
                        ty: wgpu::BindingType::StorageTexture {
                            access: wgpu::StorageTextureAccess::WriteOnly,
                            format: wgpu::TextureFormat::Rgba16Float,
                            view_dimension: wgpu::TextureViewDimension::D2,
                        },
                        count: None,
                    },
                ],
            });

        let brdf_lut_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("ibl brdf lut shader"),
            source: wgpu::ShaderSource::Wgsl(IBL_BRDF_LUT_WGSL.into()),
        });

        let brdf_lut_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("ibl brdf lut pipeline layout"),
                bind_group_layouts: &[&brdf_lut_bind_group_layout],
                push_constant_ranges: &[],
            });

        let brdf_lut_pipeline =
            device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some("ibl brdf lut pipeline"),
                layout: Some(&brdf_lut_pipeline_layout),
                module: &brdf_lut_shader,
                entry_point: "cs_main",
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                cache: None,
            });

        // ---- Irradiance compute pipeline ----
        let irradiance_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("ibl irradiance bind group layout"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::COMPUTE,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: NonZeroU64::new(std::mem::size_of::<IrradianceParams>() as u64),
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::COMPUTE,
                        ty: wgpu::BindingType::StorageTexture {
                            access: wgpu::StorageTextureAccess::WriteOnly,
                            format: wgpu::TextureFormat::Rgba16Float,
                            view_dimension: wgpu::TextureViewDimension::D2,
                        },
                        count: None,
                    },
                ],
            });

        let irradiance_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("ibl irradiance shader"),
            source: wgpu::ShaderSource::Wgsl(IBL_IRRADIANCE_WGSL.into()),
        });

        let irradiance_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("ibl irradiance pipeline layout"),
                bind_group_layouts: &[&irradiance_bind_group_layout],
                push_constant_ranges: &[],
            });

        let irradiance_pipeline =
            device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some("ibl irradiance pipeline"),
                layout: Some(&irradiance_pipeline_layout),
                module: &irradiance_shader,
                entry_point: "cs_main",
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                cache: None,
            });

        Self {
            device,
            queue,
            next_id: 1,
            baked: HashMap::new(),
            brdf_lut_view: None,
            irradiance_pipeline,
            brdf_lut_pipeline,
            irradiance_bind_group_layout,
            brdf_lut_bind_group_layout,
        }
    }

    /// 按 ID 获取烘焙好的 GPU 纹理。
    pub fn get_textures(&self, id: u32) -> Option<&BakedIblTextures> {
        self.baked.get(&id)
    }

    /// 获取 BRDF LUT 纹理视图（全局共享）。
    pub fn get_brdf_lut(&self) -> Option<&wgpu::TextureView> {
        self.brdf_lut_view.as_ref()
    }

    fn issue_id(&mut self) -> u32 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    /// 创建 BRDF LUT 纹理（GPU compute shader）。
    fn bake_brdf_lut(&self, cfg: &IblConfig) -> (wgpu::TextureView, wgpu::BindGroup, wgpu::Buffer) {
        let size = cfg.brdf_lut_size;
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("ibl brdf lut"),
            size: wgpu::Extent3d { width: size, height: size, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba16Float,
            usage: wgpu::TextureUsages::STORAGE_BINDING
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });

        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());

        let params = BrdfLutParams {
            sample_count: cfg.sample_count.min(8192),
            resolution: size,
            _pad: 0,
            _pad2: 0,
        };

        let param_buffer = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("ibl brdf lut params"),
            contents: bytemuck::bytes_of(&params),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("ibl brdf lut bind group"),
            layout: &self.brdf_lut_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: param_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
            ],
        });

        (view, bind_group, param_buffer)
    }

    /// 执行 BRDF LUT compute pass。
    fn run_brdf_lut(&self, cfg: &IblConfig) -> wgpu::TextureView {
        let (view, bind_group, _param_buffer) = self.bake_brdf_lut(cfg);

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("ibl brdf lut encoder"),
            });

        {
            let mut cpass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("ibl brdf lut compute"),
                timestamp_writes: None,
            });
            cpass.set_pipeline(&self.brdf_lut_pipeline);
            cpass.set_bind_group(0, &bind_group, &[]);
            let groups = (cfg.brdf_lut_size + 7) / 8;
            cpass.dispatch_workgroups(groups, groups, 1);
        }

        self.queue.submit(std::iter::once(encoder.finish()));
        view
    }

    /// 创建纯色 cubemap 纹理。
    fn create_solid_color_cubemap(&self, color: [f32; 3]) -> wgpu::TextureView {
        let size: u32 = 32;
        let tex = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("ibl solid color cubemap"),
            size: wgpu::Extent3d {
                width: size,
                height: size,
                depth_or_array_layers: 6,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba16Float,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });

        // Fill each face with the solid color
        let pixel_size = 8; // Rgba16Float = 4 * 16-bit = 8 bytes
        let row_size = pixel_size * size as usize;
        let face_size = row_size * size as usize;
        let mut data = vec![0u8; face_size * 6];

        for face in 0..6 {
            let offset = face * face_size;
            for i in 0..(size as usize * size as usize) {
                let px_offset = offset + i * pixel_size;
                // Rgba16Float: each component is f16
                let r = f32_to_f16(color[0]);
                let g = f32_to_f16(color[1]);
                let b = f32_to_f16(color[2]);
                let a = f32_to_f16(1.0);
                let bytes = [r, g, b, a];
                for (j, &byte_val) in bytes.iter().enumerate() {
                    data[px_offset + j * 2] = (byte_val & 0xFF) as u8;
                    data[px_offset + j * 2 + 1] = ((byte_val >> 8) & 0xFF) as u8;
                }
            }
        }

        self.queue.write_texture(
            wgpu::ImageCopyTexture {
                texture: &tex,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &data,
            wgpu::ImageDataLayout {
                offset: 0,
                bytes_per_row: Some(row_size as u32),
                rows_per_image: Some(size),
            },
            wgpu::Extent3d {
                width: size,
                height: size,
                depth_or_array_layers: 6,
            },
        );

        tex.create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(wgpu::TextureViewDimension::Cube),
            ..Default::default()
        })
    }

    /// 创建程序化天空 irradiance cubemap（GPU compute shader）。
    fn bake_procedural_irradiance(&self, cfg: &IblConfig, sun: [f32; 3], ground_albedo: [f32; 3]) -> wgpu::TextureView {
        let face_size = cfg.irradiance_size;
        let tex = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("ibl procedural irradiance"),
            size: wgpu::Extent3d {
                width: face_size,
                height: face_size,
                depth_or_array_layers: 6,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba16Float,
            usage: wgpu::TextureUsages::STORAGE_BINDING
                | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });

        // Process each face
        for face in 0..6u32 {
            let face_view = tex.create_view(&wgpu::TextureViewDescriptor {
                dimension: Some(wgpu::TextureViewDimension::D2),
                base_array_layer: face,
                array_layer_count: Some(1),
                ..Default::default()
            });

            let params = IrradianceParams {
                face_size,
                sample_count: cfg.sample_count.min(1024),
                face_index: face,
                _pad: 0,
                sun_direction: [sun[0], sun[1], sun[2], 0.0],
                ground_albedo: [ground_albedo[0], ground_albedo[1], ground_albedo[2], 0.0],
                sky_color: [0.3, 0.5, 0.8, 1.0],
                horizon_color: [0.5, 0.6, 0.7, 1.0],
                ground_color: [ground_albedo[0] * 0.5, ground_albedo[1] * 0.5, ground_albedo[2] * 0.5, 1.0],
            };

            let param_buffer = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("ibl irradiance params"),
                contents: bytemuck::bytes_of(&params),
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            });

            let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("ibl irradiance bind group"),
                layout: &self.irradiance_bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: param_buffer.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(&face_view),
                    },
                ],
            });

            let mut encoder = self
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("ibl irradiance encoder"),
                });

            {
                let mut cpass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                    label: Some("ibl irradiance compute"),
                    timestamp_writes: None,
                });
                cpass.set_pipeline(&self.irradiance_pipeline);
                cpass.set_bind_group(0, &bind_group, &[]);
                let groups = (face_size + 7) / 8;
                cpass.dispatch_workgroups(groups, groups, 1);
            }

            self.queue.submit(std::iter::once(encoder.finish()));
        }

        tex.create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(wgpu::TextureViewDimension::Cube),
            ..Default::default()
        })
    }

    /// 创建预滤波环境贴图（首版简化：复用 irradiance，带 mip）。
    fn bake_prefiltered(&self, irradiance_view: &wgpu::TextureView, cfg: &IblConfig) -> wgpu::TextureView {
        // 首版简化：创建带 mipmap 的空纹理，复制 irradiance 到 mip 0
        let size = cfg.prefilter_size;
        let mip_levels = cfg.prefilter_mip_levels.min(8);
        let tex = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("ibl prefiltered"),
            size: wgpu::Extent3d {
                width: size,
                height: size,
                depth_or_array_layers: 6,
            },
            mip_level_count: mip_levels,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba16Float,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });

        // 首版不实际做 prefilter 卷积，只创建纹理。真实 prefilter 需要按 roughness 做
        // importance sampling 卷积，这里留作后续扩展。
        let _ = irradiance_view;

        tex.create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(wgpu::TextureViewDimension::Cube),
            ..Default::default()
        })
    }

    /// 实际烘焙逻辑（不通过 trait 调用时使用，可以返回错误）。
    pub fn bake_gpu(&mut self, sky: &SkyboxKind, cfg: &IblConfig) -> Result<IblResources, IblBakeError> {
        // BRDF LUT（只烘焙一次，全局共享）
        if self.brdf_lut_view.is_none() {
            self.brdf_lut_view = Some(self.run_brdf_lut(cfg));
        }
        let brdf_lut_id = 0u32; // BRDF LUT 通过 get_brdf_lut() 获取，不用 ID

        // Irradiance + Prefiltered
        let (irradiance_view, prefiltered_view) = match sky {
            SkyboxKind::SolidColor(color) => {
                let irr = self.create_solid_color_cubemap(*color);
                let pre = self.create_solid_color_cubemap(*color);
                (irr, pre)
            }
            SkyboxKind::Procedural { sun_direction, ground_albedo, .. } => {
                let irr = self.bake_procedural_irradiance(cfg, *sun_direction, *ground_albedo);
                let pre = self.bake_prefiltered(&irr, cfg);
                (irr, pre)
            }
            SkyboxKind::Hdri { .. } => {
                return Err(IblBakeError::Unsupported(
                    "HDRI decoding not yet implemented (requires image/hdr-rs crate)".into(),
                ));
            }
        };

        let id = self.issue_id();
        self.baked.insert(
            id,
            BakedIblTextures {
                irradiance: irradiance_view,
                prefiltered: prefiltered_view,
                brdf_lut: None,
                mip_levels: cfg.prefilter_mip_levels,
            },
        );

        Ok(IblResources {
            irradiance: id,
            prefiltered: id,
            brdf_lut: brdf_lut_id,
            mip_levels: cfg.prefilter_mip_levels,
        })
    }
}

impl<'a> IblBaker for WgpuIblBaker<'a> {
    fn bake(&mut self, sky: &SkyboxKind, cfg: &IblConfig) -> IblResources {
        match self.bake_gpu(sky, cfg) {
            Ok(r) => r,
            Err(e) => {
                log::error!("[IBL] bake failed: {e}");
                // 降级：返回无效 ID
                IblResources {
                    irradiance: 0,
                    prefiltered: 0,
                    brdf_lut: 0,
                    mip_levels: cfg.prefilter_mip_levels,
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// f32 → f16 转换（IEEE 754 half-precision）
// ---------------------------------------------------------------------------

fn f32_to_f16(val: f32) -> u16 {
    if val.is_nan() {
        return 0x7E00; // NaN
    }
    let bits = val.to_bits();
    let sign = ((bits >> 16) & 0x8000) as u16;
    let exponent = ((bits >> 23) & 0xFF) as i32;
    let mantissa = bits & 0x7FFFFF;

    if exponent == 0xFF {
        // Inf or NaN
        if mantissa == 0 {
            return sign | 0x7C00; // Inf
        }
        return sign | 0x7E00; // NaN
    }

    let new_exp = exponent - 127 + 15;
    if new_exp >= 0x1F {
        // Overflow → Inf
        return sign | 0x7C00;
    }
    if new_exp <= 0 {
        if new_exp < -10 {
            return sign; // Underflow → 0
        }
        // Subnormal
        let m = (mantissa | 0x800000) >> (14 - new_exp);
        return sign | (m as u16);
    }
    sign | ((new_exp as u16) << 10) | ((mantissa >> 13) as u16)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn f32_to_f16_known_values() {
        assert_eq!(f32_to_f16(0.0), 0);
        assert_eq!(f32_to_f16(1.0), 0x3C00);
        assert_eq!(f32_to_f16(-1.0), 0xBC00);
        assert_eq!(f32_to_f16(0.5), 0x3800);
        assert_eq!(f32_to_f16(2.0), 0x4000);
    }

    #[test]
    fn f32_to_f16_inf() {
        assert_eq!(f32_to_f16(f32::INFINITY), 0x7C00);
        assert_eq!(f32_to_f16(f32::NEG_INFINITY), 0xFC00);
    }
}
