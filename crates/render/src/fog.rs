//! 体积雾/云渲染 — 基于深度缓冲的距离雾 + 高度雾 + 噪声扰动。
//!
//! 使用方式：
//! 1. 配置 `FogSettings`
//! 2. 由 `FogRenderer` 编码为 `FogUniform` 并上传 GPU
//! 3. 在 `fog.wgsl` 中作为后处理全屏 pass 采样深度、重建世界坐标、计算雾浓度
//!
//! 噪声使用纯数学 hash noise，无需外部纹理。

use bytemuck::{Pod, Zeroable};

/// 体积雾 CPU 端参数配置。
#[derive(Clone, Debug)]
pub struct FogSettings {
    /// 雾颜色（线性 RGB，0-1）
    pub color: [f32; 3],
    /// 雾基础密度（0 = 无雾，1 = 浓雾）
    pub density: f32,
    /// 高度衰减系数：越大雾随高度稀薄越快
    pub height_falloff: f32,
    /// 距离雾起始距离（米）
    pub start_distance: f32,
    /// 距离雾完全遮蔽距离（米）
    pub end_distance: f32,
    /// 噪声缩放（控制体积变化粒度）
    pub noise_scale: f32,
    /// 噪声强度（0 = 纯指数雾，>0 = 加入体积扰动）
    pub noise_strength: f32,
    /// 是否启用雾效
    pub enabled: bool,
}

impl Default for FogSettings {
    fn default() -> Self {
        Self {
            color: [0.6, 0.65, 0.75],
            density: 0.02,
            height_falloff: 0.1,
            start_distance: 10.0,
            end_distance: 200.0,
            noise_scale: 0.05,
            noise_strength: 0.3,
            enabled: false,
        }
    }
}

/// GPU 上传的雾参数 uniform（std140 兼容，6 vec4 = 96 bytes）。
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct FogUniform {
    /// [color_r, color_g, color_b, density]
    pub color_density: [f32; 4],
    /// [height_falloff, start_distance, end_distance, noise_scale]
    pub params: [f32; 4],
    /// [noise_strength, enabled (1.0/0.0), time, _pad]
    pub extra: [f32; 4],
}

impl Default for FogUniform {
    fn default() -> Self {
        let s = FogSettings::default();
        Self::from_settings(&s, 0.0)
    }
}

impl FogUniform {
    /// 从 `FogSettings` 编码，`time` 用于噪声动画。
    pub fn from_settings(settings: &FogSettings, time: f32) -> Self {
        Self {
            color_density: [
                settings.color[0],
                settings.color[1],
                settings.color[2],
                settings.density,
            ],
            params: [
                settings.height_falloff,
                settings.start_distance,
                settings.end_distance,
                settings.noise_scale,
            ],
            extra: [
                settings.noise_strength,
                if settings.enabled { 1.0 } else { 0.0 },
                time,
                0.0,
            ],
        }
    }
}

/// 体积雾渲染器 — 管理雾参数和 uniform 构建。
///
/// 渲染管线集成：作为独立后处理 pass，从深度缓冲重建世界坐标，
/// 计算距离雾 + 高度雾 + 噪声，与场景颜色混合。
pub struct FogRenderer {
    pub settings: FogSettings,
}

impl FogRenderer {
    pub fn new(settings: FogSettings) -> Self {
        Self { settings }
    }

    /// 使用默认参数创建（雾效关闭）。
    pub fn disabled() -> Self {
        Self {
            settings: FogSettings::default(),
        }
    }

    /// 构建当前帧的 GPU uniform。
    pub fn build_uniform(&self, time: f32) -> FogUniform {
        FogUniform::from_settings(&self.settings, time)
    }

    /// CPU 侧雾浓度计算（用于测试/预览，与 shader 逻辑一致）。
    ///
    /// - `pixel_distance`: 像素到相机的距离
    /// - `pixel_height`: 像素世界空间 y 坐标
    /// - `world_pos`: 像素世界坐标 [x, y, z]
    ///
    /// 返回 0.0（无雾）到 1.0（完全遮蔽）的雾浓度。
    pub fn compute_fog_factor(
        &self,
        pixel_distance: f32,
        pixel_height: f32,
        world_pos: [f32; 3],
    ) -> f32 {
        if !self.settings.enabled {
            return 0.0;
        }

        let s = &self.settings;

        // 距离雾：线性插值 [start_distance, end_distance]
        let dist_range = (s.end_distance - s.start_distance).max(0.001);
        let dist_fog = ((pixel_distance - s.start_distance) / dist_range).clamp(0.0, 1.0);

        // 高度雾：y 越高雾越稀薄，exp(-height_falloff * y)
        let height_fog = (-s.height_falloff * pixel_height.max(0.0)).exp();

        // 噪声扰动：简单 3D hash noise
        let noise = if s.noise_strength > 0.0 {
            let p = [
                world_pos[0] * s.noise_scale,
                world_pos[1] * s.noise_scale,
                world_pos[2] * s.noise_scale,
            ];
            let n = hash_noise_3d(p);
            1.0 + (n - 0.5) * s.noise_strength
        } else {
            1.0
        };

        // 综合：距离雾 × 高度雾 × 噪声 × 密度
        let fog = dist_fog * height_fog * noise * s.density;
        fog.clamp(0.0, 1.0)
    }
}

/// 简单 3D hash noise，输出 [0, 1]。无需外部纹理。
fn hash_noise_3d(p: [f32; 3]) -> f32 {
    let mut h = (p[0] * 127.1 + p[1] * 311.7 + p[2] * 74.7).sin() * 43758.5453;
    h = h.fract();
    // 确保非负
    if h < 0.0 { h += 1.0; }
    h
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fog_uniform_is_48_bytes() {
        // 3 vec4 = 48 bytes
        assert_eq!(std::mem::size_of::<FogUniform>(), 48);
    }

    #[test]
    fn disabled_fog_returns_zero() {
        let r = FogRenderer::disabled();
        let f = r.compute_fog_factor(100.0, 5.0, [10.0, 5.0, 20.0]);
        assert!(f.abs() < 1e-6);
    }

    #[test]
    fn fog_increases_with_distance() {
        let mut s = FogSettings::default();
        s.enabled = true;
        s.density = 1.0;
        s.noise_strength = 0.0; // 关闭噪声以获得确定性结果
        let r = FogRenderer::new(s);

        let near = r.compute_fog_factor(20.0, 0.0, [0.0, 0.0, 0.0]);
        let far = r.compute_fog_factor(150.0, 0.0, [0.0, 0.0, 0.0]);
        assert!(far > near, "far={far}, near={near}");
    }

    #[test]
    fn fog_decreases_with_height() {
        let mut s = FogSettings::default();
        s.enabled = true;
        s.density = 1.0;
        s.noise_strength = 0.0;
        let r = FogRenderer::new(s);

        let low = r.compute_fog_factor(100.0, 0.0, [0.0, 0.0, 0.0]);
        let high = r.compute_fog_factor(100.0, 50.0, [0.0, 50.0, 0.0]);
        assert!(low > high, "low={low}, high={high}");
    }

    #[test]
    fn hash_noise_in_unit_range() {
        for x in 0..10 {
            for y in 0..10 {
                for z in 0..10 {
                    let n = hash_noise_3d([x as f32, y as f32, z as f32]);
                    assert!(n >= 0.0 && n <= 1.0, "n={n} at ({x},{y},{z})");
                }
            }
        }
    }

    #[test]
    fn settings_default_has_fog_off() {
        let s = FogSettings::default();
        assert!(!s.enabled);
    }
}
