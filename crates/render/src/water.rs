//! 水面/流体渲染 — 平面网格 + 三层正弦波顶点动画 + 菲涅尔反射/折射混合。
//!
//! 使用方式：
//! 1. 配置 `WaterSettings`
//! 2. 由 `WaterRenderer` 生成水面网格（XZ 平面 quad 细分）
//! 3. 在 `water.wgsl` 中：
//!    - 顶点 shader：3 层正弦波叠加位移顶点
//!    - 片段 shader：法线扰动 + 菲涅尔 + 反射/折射 + 高光 + 深度着色
//!
//! 波浪位移在 shader 中计算，CPU 只生成静态平面网格。

use bytemuck::{Pod, Zeroable};

/// 水面 CPU 端参数配置。
#[derive(Clone, Debug)]
pub struct WaterSettings {
    /// 水面高度（世界空间 y 坐标）
    pub water_level: f32,
    /// 波浪振幅（米）
    pub wave_amplitude: f32,
    /// 波浪频率（周期/米）
    pub wave_frequency: f32,
    /// 波浪速度（米/秒）
    pub wave_speed: f32,
    /// 水体颜色（线性 RGB，0-1）
    pub water_color: [f32; 3],
    /// 高光指数（specular power，越大高光越集中）
    pub specular_power: f32,
    /// 菲涅尔指数（越大掠射角反射越强）
    pub fresnel_power: f32,
    /// 折射强度（0 = 无折射，1 = 完全折射）
    pub refraction_strength: f32,
    /// 反射强度（0 = 无反射，1 = 完全反射）
    pub reflection_strength: f32,
    /// 水面网格细分段数（X/Z 方向各多少段）
    pub subdivisions: u32,
    /// 水面网格边长（米，正方形）
    pub extent: f32,
}

impl Default for WaterSettings {
    fn default() -> Self {
        Self {
            water_level: 0.0,
            wave_amplitude: 0.15,
            wave_frequency: 0.8,
            wave_speed: 1.2,
            water_color: [0.05, 0.2, 0.4],
            specular_power: 64.0,
            fresnel_power: 5.0,
            refraction_strength: 0.3,
            reflection_strength: 0.6,
            subdivisions: 64,
            extent: 100.0,
        }
    }
}

/// GPU 上传的水面参数 uniform（8 vec4 = 128 bytes）。
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct WaterUniform {
    /// [water_level, wave_amplitude, wave_frequency, wave_speed]
    pub wave: [f32; 4],
    /// [water_color_r, water_color_g, water_color_b, specular_power]
    pub color_specular: [f32; 4],
    /// [fresnel_power, refraction_strength, reflection_strength, time]
    pub fresnel_time: [f32; 4],
    /// [extent, subdivisions_f32, _pad, _pad]
    pub mesh_params: [f32; 4],
}

impl Default for WaterUniform {
    fn default() -> Self {
        let s = WaterSettings::default();
        Self::from_settings(&s, 0.0)
    }
}

impl WaterUniform {
    /// 从 `WaterSettings` 编码，`time` 用于波浪动画。
    pub fn from_settings(settings: &WaterSettings, time: f32) -> Self {
        Self {
            wave: [
                settings.water_level,
                settings.wave_amplitude,
                settings.wave_frequency,
                settings.wave_speed,
            ],
            color_specular: [
                settings.water_color[0],
                settings.water_color[1],
                settings.water_color[2],
                settings.specular_power,
            ],
            fresnel_time: [
                settings.fresnel_power,
                settings.refraction_strength,
                settings.reflection_strength,
                time,
            ],
            mesh_params: [
                settings.extent,
                settings.subdivisions as f32,
                0.0,
                0.0,
            ],
        }
    }
}

/// 水面顶点格式（与 water.wgsl 顶点输入对齐）。
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct WaterVertex {
    pub position: [f32; 3],
    pub uv: [f32; 2],
}

impl WaterVertex {
    pub fn layout<'a>() -> wgpu::VertexBufferLayout<'a> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<WaterVertex>() as wgpu::BufferAddress,
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
                    format: wgpu::VertexFormat::Float32x2,
                },
            ],
        }
    }
}

/// 水面渲染器 — 管理水面网格和参数。
pub struct WaterRenderer {
    pub settings: WaterSettings,
}

impl WaterRenderer {
    pub fn new(settings: WaterSettings) -> Self {
        Self { settings }
    }

    /// 使用默认参数创建。
    pub fn default_water() -> Self {
        Self {
            settings: WaterSettings::default(),
        }
    }

    /// 构建当前帧的 GPU uniform。
    pub fn build_uniform(&self, time: f32) -> WaterUniform {
        WaterUniform::from_settings(&self.settings, time)
    }

    /// 生成水面网格顶点和索引（XZ 平面，y = water_level）。
    ///
    /// 波浪位移在 GPU 端 shader 中计算，此处只生成静态平面。
    pub fn generate_mesh(&self) -> (Vec<WaterVertex>, Vec<u32>) {
        let n = self.settings.subdivisions as usize;
        let half = self.settings.extent * 0.5;
        let step = self.settings.extent / n as f32;
        let y = self.settings.water_level;

        let mut vertices = Vec::with_capacity((n + 1) * (n + 1));
        let mut indices = Vec::with_capacity(n * n * 6);

        for iz in 0..=n {
            for ix in 0..=n {
                let x = -half + ix as f32 * step;
                let z = -half + iz as f32 * step;
                let u = ix as f32 / n as f32;
                let v = iz as f32 / n as f32;
                vertices.push(WaterVertex {
                    position: [x, y, z],
                    uv: [u, v],
                });
            }
        }

        for iz in 0..n {
            for ix in 0..n {
                let i00 = (iz * (n + 1) + ix) as u32;
                let i10 = (iz * (n + 1) + ix + 1) as u32;
                let i01 = ((iz + 1) * (n + 1) + ix) as u32;
                let i11 = ((iz + 1) * (n + 1) + ix + 1) as u32;
                // Two triangles per quad, CCW winding.
                indices.push(i00);
                indices.push(i01);
                indices.push(i10);
                indices.push(i10);
                indices.push(i01);
                indices.push(i11);
            }
        }

        (vertices, indices)
    }

    /// CPU 侧波浪位移（用于测试/预览，与 shader 一致）。
    ///
    /// 返回 y 方向位移量（叠加到 water_level 上）。
    pub fn compute_wave_displacement(&self, x: f32, z: f32, time: f32) -> f32 {
        let amp = self.settings.wave_amplitude;
        let freq = self.settings.wave_frequency;
        let speed = self.settings.wave_speed;

        // 3 层正弦波，不同方向和频率。
        let w1 = (x * freq + time * speed).sin() * amp;
        let w2 = (z * freq * 0.7 + time * speed * 1.3).sin() * amp * 0.5;
        let w3 = ((x + z) * freq * 0.5 + time * speed * 0.8).sin() * amp * 0.25;

        w1 + w2 + w3
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn water_uniform_is_64_bytes() {
        // 4 vec4 = 64 bytes
        assert_eq!(std::mem::size_of::<WaterUniform>(), 64);
    }

    #[test]
    fn water_vertex_is_20_bytes() {
        assert_eq!(std::mem::size_of::<WaterVertex>(), 20);
    }

    #[test]
    fn mesh_generation_correct_counts() {
        let s = WaterSettings {
            subdivisions: 4,
            extent: 10.0,
            ..WaterSettings::default()
        };
        let r = WaterRenderer::new(s);
        let (verts, indices) = r.generate_mesh();
        // (4+1)^2 = 25 vertices
        assert_eq!(verts.len(), 25);
        // 4*4*6 = 96 indices
        assert_eq!(indices.len(), 96);
    }

    #[test]
    fn wave_displacement_bounded() {
        let r = WaterRenderer::default_water();
        let max_amp = r.settings.wave_amplitude * 1.75; // 1.0 + 0.5 + 0.25
        for x in (-10..=10).step_by(3) {
            for z in (-10..=10).step_by(3) {
                let d = r.compute_wave_displacement(x as f32, z as f32, 0.5);
                assert!(d.abs() <= max_amp + 0.01, "d={d} at ({x},{z})");
            }
        }
    }

    #[test]
    fn mesh_vertices_within_extent() {
        let s = WaterSettings {
            subdivisions: 8,
            extent: 20.0,
            water_level: 5.0,
            ..WaterSettings::default()
        };
        let r = WaterRenderer::new(s);
        let (verts, _) = r.generate_mesh();
        for v in &verts {
            assert!(v.position[0] >= -10.01 && v.position[0] <= 10.01);
            assert!((v.position[1] - 5.0).abs() < 0.01);
            assert!(v.position[2] >= -10.01 && v.position[2] <= 10.01);
        }
    }
}
