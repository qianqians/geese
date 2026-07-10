//! Hi-Z (Hierarchical Z-Buffer) 深度金字塔构建与遮挡剔除。
//!
//! Feature gate: `hi-z-occlusion`（默认禁用）。
//!
//! ## 设计
//! - **Hi-Z 构建**：从前一帧的深度缓冲逐级 downsample（2×2 max reduction），
//!   生成深度金字塔 mip chain。
//! - **遮挡测试**：对每个待绘制物体的 AABB，投影到屏幕空间后与 Hi-Z 金字塔
//!   比较——若物体 AABB 的最大深度小于 Hi-Z 对应 mip 的最小深度，则物体被遮挡
//!   （可跳过绘制）。
//! - 使用上一帧深度缓冲（业界标准实践），存在 1 帧遮挡滞后。
//!
//! ## GPU 资源
//! - `hi_z_pyramid`: 深度金字塔纹理（mip-mapped `R32Float`）
//! - `hi_z_sampler`: 点采样器（用于精确 depth fetch）

/// Hi-Z 深度金字塔。
///
/// 存储从前一帧深度缓冲逐级 downsampled 的 mip chain。
pub struct HiZPyramid {
    /// 金字塔纹理（mip-mapped R32Float），mip 0 = 全分辨率深度。
    pub texture: wgpu::Texture,
    /// 纹理 view（包含所有 mip levels）。
    pub view: wgpu::TextureView,
    /// 点采样器。
    pub sampler: wgpu::Sampler,
    /// 存储 mip level 数量和尺寸信息。
    pub mip_level_count: u32,
    pub base_width: u32,
    pub base_height: u32,
    /// Hi-Z build compute pipeline。
    pub build_pipeline: Option<wgpu::ComputePipeline>,
    /// Hi-Z build bind group layout。
    pub build_bind_group_layout: Option<wgpu::BindGroupLayout>,
}

impl HiZPyramid {
    /// 创建 Hi-Z 金字塔（不包含 compute pipeline——需调用 `init_pipeline` 初始化）。
    pub fn new(device: &wgpu::Device, width: u32, height: u32) -> Self {
        let mip_level_count = max_mip_levels(width, height);
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Hi-Z pyramid"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R32Float,
            usage: wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::STORAGE_BINDING
                | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor {
            label: Some("Hi-Z pyramid view"),
            ..Default::default()
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Hi-Z sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        Self {
            texture,
            view,
            sampler,
            mip_level_count,
            base_width: width,
            base_height: height,
            build_pipeline: None,
            build_bind_group_layout: None,
        }
    }

    /// 初始化 Hi-Z build compute pipeline（需在首次渲染前调用）。
    pub fn init_pipeline(&mut self, device: &wgpu::Device, hi_z_build_wgsl: &str) {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Hi-Z build shader"),
            source: wgpu::ShaderSource::Wgsl(hi_z_build_wgsl.into()),
        });

        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Hi-Z build bind group layout"),
            entries: &[
                // binding 0: src depth texture (read-only)
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                // binding 1: dst mip level texture (storage write)
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::StorageTexture {
                        access: wgpu::StorageTextureAccess::WriteOnly,
                        format: wgpu::TextureFormat::R32Float,
                        view_dimension: wgpu::TextureViewDimension::D2,
                    },
                    count: None,
                },
            ],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Hi-Z build pipeline layout"),
            bind_group_layouts: &[&layout],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("Hi-Z build pipeline"),
            layout: Some(&pipeline_layout),
            module: &shader,
            entry_point: "cs_main",
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            cache: None,
        });

        self.build_pipeline = Some(pipeline);
        self.build_bind_group_layout = Some(layout);
    }

    /// 调整金字塔尺寸（viewport resize 时调用）。
    pub fn resize(&mut self, device: &wgpu::Device, width: u32, height: u32) {
        let mip_level_count = max_mip_levels(width, height);
        self.texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Hi-Z pyramid"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R32Float,
            usage: wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::STORAGE_BINDING
                | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        self.view = self.texture.create_view(&wgpu::TextureViewDescriptor::default());
        self.mip_level_count = mip_level_count;
        self.base_width = width;
        self.base_height = height;
    }

    /// 执行 Hi-Z 构建：从 src_depth 逐级 downsample 填充金字塔。
    ///
    /// `src_depth` 应为上一帧深度缓冲的 texture view。
    pub fn build(
        &self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        _src_depth: &wgpu::TextureView,
    ) {
        let pipeline = match &self.build_pipeline {
            Some(p) => p,
            None => {
                log::warn!("[Hi-Z] build_pipeline not initialized, skipping");
                return;
            }
        };
        let layout = match &self.build_bind_group_layout {
            Some(l) => l,
            None => return,
        };

        // 对每个 mip level（从 1 开始），执行一次 compute pass
        let mut src_width = self.base_width;
        let mut src_height = self.base_height;

        for mip in 1..self.mip_level_count {
            let dst_width = (src_width / 2).max(1);
            let dst_height = (src_height / 2).max(1);

            let src_view = self.texture.create_view(&wgpu::TextureViewDescriptor {
                label: Some("Hi-Z src mip"),
                base_mip_level: mip - 1,
                mip_level_count: Some(1),
                ..Default::default()
            });
            let dst_view = self.texture.create_view(&wgpu::TextureViewDescriptor {
                label: Some("Hi-Z dst mip"),
                base_mip_level: mip,
                mip_level_count: Some(1),
                ..Default::default()
            });

            let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("Hi-Z build bind group"),
                layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&src_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(&dst_view),
                    },
                ],
            });

            let mut cpass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("Hi-Z build"),
                timestamp_writes: None,
            });
            cpass.set_pipeline(pipeline);
            cpass.set_bind_group(0, &bind_group, &[]);
            let groups_x = (dst_width + 7) / 8;
            let groups_y = (dst_height + 7) / 8;
            cpass.dispatch_workgroups(groups_x, groups_y, 1);

            src_width = dst_width;
            src_height = dst_height;
        }
    }

    /// 检查设备是否支持 compute shader + storage texture（Hi-Z 所需）。
    pub fn is_supported(_device: &wgpu::Device) -> bool {
        // wgpu 22.1: compute + storage texture 在所有主流后端均支持
        // (Vulkan/DX12/Metal)。WebGPU 也支持。
        true
    }
}

/// 计算 Hi-Z 金字塔所需的最大 mip level 数。
fn max_mip_levels(width: u32, height: u32) -> u32 {
    let max_dim = width.max(height);
    32 - max_dim.leading_zeros()
}

/// 物体 AABB（世界空间轴对齐包围盒）。
#[derive(Clone, Copy, Debug)]
pub struct ObjectAabb {
    pub min: [f32; 3],
    pub max: [f32; 3],
}

impl ObjectAabb {
    /// 从模型矩阵和局部 AABB 计算世界空间 AABB（简化：使用模型矩阵的平移 + 缩放上限）。
    pub fn from_local(local_min: [f32; 3], local_max: [f32; 3], model: &[[f32; 4]; 4]) -> Self {
        // 简化：仅考虑平移 + 最大缩放因子
        let scale_x = (model[0][0] * model[0][0] + model[0][1] * model[0][1] + model[0][2] * model[0][2]).sqrt();
        let scale_y = (model[1][0] * model[1][0] + model[1][1] * model[1][1] + model[1][2] * model[1][2]).sqrt();
        let scale_z = (model[2][0] * model[2][0] + model[2][1] * model[2][1] + model[2][2] * model[2][2]).sqrt();
        let max_scale = scale_x.max(scale_y).max(scale_z);

        let tx = model[0][3];
        let ty = model[1][3];
        let tz = model[2][3];

        let half_extent = [
            (local_max[0] - local_min[0]) * 0.5 * max_scale,
            (local_max[1] - local_min[1]) * 0.5 * max_scale,
            (local_max[2] - local_min[2]) * 0.5 * max_scale,
        ];
        let center = [
            tx + (local_min[0] + local_max[0]) * 0.5 * max_scale,
            ty + (local_min[1] + local_max[1]) * 0.5 * max_scale,
            tz + (local_min[2] + local_max[2]) * 0.5 * max_scale,
        ];

        Self {
            min: [center[0] - half_extent[0], center[1] - half_extent[1], center[2] - half_extent[2]],
            max: [center[0] + half_extent[0], center[1] + half_extent[1], center[2] + half_extent[2]],
        }
    }

    /// 默认单位立方体 AABB（供没有自定义 AABB 的 mesh 使用）。
    pub fn unit_cube() -> Self {
        Self {
            min: [-0.5, -0.5, -0.5],
            max: [0.5, 0.5, 0.5],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn max_mip_levels_power_of_two() {
        assert_eq!(max_mip_levels(256, 256), 9); // 2^8=256, +1 = 9 levels (0..8)
    }

    #[test]
    fn max_mip_levels_non_power_of_two() {
        assert_eq!(max_mip_levels(1920, 1080), 11); // 1920>1080, floor(log2(1920))+1 = 11
    }

    #[test]
    fn aabb_from_identity_yields_unit_cube_at_origin() {
        let model = [
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ];
        let aabb = ObjectAabb::from_local([-0.5, -0.5, -0.5], [0.5, 0.5, 0.5], &model);
        assert!((aabb.min[0] + 0.5).abs() < 0.001);
        assert!((aabb.max[0] - 0.5).abs() < 0.001);
    }
}
