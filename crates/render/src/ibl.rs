//! 4.6 IBL / 天空盒骨架。
//!
//! 提供 HDRI/Procedural/SolidColor 三种天空表达，以及 IBL 三件套
//! （Irradiance / Prefiltered / BRDF LUT）的句柄抽象。
//!
//! 真实烘焙（Hammersley + GGX importance sampling）留待接入 wgpu 时实现。

use bytemuck::{Pod, Zeroable};

/// IBL 配置：影响烘焙分辨率与采样数。
#[derive(Clone, Copy, Debug)]
pub struct IblConfig {
    pub irradiance_size: u32,
    pub prefilter_size: u32,
    pub prefilter_mip_levels: u32,
    pub brdf_lut_size: u32,
    pub sample_count: u32,
}

impl Default for IblConfig {
    fn default() -> Self {
        Self {
            irradiance_size: 32,
            prefilter_size: 128,
            prefilter_mip_levels: 5,
            brdf_lut_size: 256,
            sample_count: 1024,
        }
    }
}

/// 天空盒类型。
#[derive(Clone, Debug)]
pub enum SkyboxKind {
    /// HDRI 等距柱状投影贴图路径。
    Hdri { path: String },
    /// 程序化天空（基础大气散射占位参数）。
    Procedural {
        sun_direction: [f32; 3],
        turbidity: f32,
        ground_albedo: [f32; 3],
    },
    /// 纯色（rgb in linear space）。
    SolidColor([f32; 3]),
}

impl SkyboxKind {
    pub fn solid(rgb: [f32; 3]) -> Self { Self::SolidColor(rgb) }
    pub fn hdri<S: Into<String>>(path: S) -> Self { Self::Hdri { path: path.into() } }
    pub fn procedural(sun: [f32; 3]) -> Self {
        Self::Procedural {
            sun_direction: sun,
            turbidity: 2.5,
            ground_albedo: [0.1, 0.1, 0.1],
        }
    }
}

/// 烘焙得到的 IBL 资源句柄（具体 GPU texture 索引由 backend 维护）。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct IblResources {
    pub irradiance: u32,
    pub prefiltered: u32,
    pub brdf_lut: u32,
    pub mip_levels: u32,
}

/// 上传 GPU 的 IBL 标量参数（采样曝光等）。
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct IblUniform {
    /// x = exposure, y = mip_levels, z = enabled (0/1), w = pad
    pub params: [f32; 4],
}

impl Default for IblUniform {
    fn default() -> Self { Self { params: [1.0, 5.0, 0.0, 0.0] } }
}

/// 烘焙器 trait。M2 骨架阶段只验证调用次数和参数。
pub trait IblBaker {
    fn bake(&mut self, sky: &SkyboxKind, cfg: &IblConfig) -> IblResources;
}

/// 占位 baker：用单调递增 id 模拟资源句柄。
pub struct StubIblBaker {
    next_id: u32,
    pub bake_count: usize,
}

impl StubIblBaker {
    pub fn new() -> Self { Self { next_id: 1, bake_count: 0 } }
    fn issue(&mut self) -> u32 { let id = self.next_id; self.next_id += 1; id }
}

impl Default for StubIblBaker {
    fn default() -> Self { Self::new() }
}

impl IblBaker for StubIblBaker {
    fn bake(&mut self, _sky: &SkyboxKind, cfg: &IblConfig) -> IblResources {
        self.bake_count += 1;
        IblResources {
            irradiance: self.issue(),
            prefiltered: self.issue(),
            brdf_lut: self.issue(),
            mip_levels: cfg.prefilter_mip_levels,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_cfg_is_self_consistent() {
        let c = IblConfig::default();
        assert!(c.irradiance_size <= c.prefilter_size);
        assert!(c.prefilter_mip_levels >= 1);
    }

    #[test]
    fn skybox_constructors() {
        match SkyboxKind::solid([0.1, 0.2, 0.3]) {
            SkyboxKind::SolidColor(c) => assert_eq!(c, [0.1, 0.2, 0.3]),
            _ => panic!(),
        }
        match SkyboxKind::hdri("assets/sky.hdr") {
            SkyboxKind::Hdri { path } => assert_eq!(path, "assets/sky.hdr"),
            _ => panic!(),
        }
        match SkyboxKind::procedural([0.0, 1.0, 0.0]) {
            SkyboxKind::Procedural { sun_direction, .. } => {
                assert_eq!(sun_direction, [0.0, 1.0, 0.0]);
            }
            _ => panic!(),
        }
    }

    #[test]
    fn stub_baker_issues_unique_handles() {
        let mut b = StubIblBaker::new();
        let r1 = b.bake(&SkyboxKind::solid([0.0; 3]), &IblConfig::default());
        let r2 = b.bake(&SkyboxKind::solid([0.0; 3]), &IblConfig::default());
        assert_ne!(r1.irradiance, r2.irradiance);
        assert_ne!(r1.prefiltered, r2.prefiltered);
        assert_eq!(b.bake_count, 2);
    }

    #[test]
    fn stub_baker_propagates_mip_levels() {
        let mut b = StubIblBaker::new();
        let cfg = IblConfig { prefilter_mip_levels: 7, ..Default::default() };
        let r = b.bake(&SkyboxKind::solid([0.0; 3]), &cfg);
        assert_eq!(r.mip_levels, 7);
    }

    #[test]
    fn ibl_uniform_size_is_16_bytes() {
        assert_eq!(std::mem::size_of::<IblUniform>(), 16);
    }
}
