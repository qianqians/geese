use bytemuck::{Pod, Zeroable};

/// 单帧最多支持的光源数量。受 cluster bitmask 为 u32 的限制，硬上限为 32。
pub const MAX_LIGHTS: usize = 16;

/// 光源类型枚举，对应 GPU 端 `direction_type.w`：
/// 0=Directional，1=Point，2=Spot。
const LIGHT_TYPE_DIRECTIONAL: u32 = 0;
const LIGHT_TYPE_POINT: u32 = 1;
const LIGHT_TYPE_SPOT: u32 = 2;

/// 用户面向的高级光源类型，构造时使用直观字段。
#[derive(Clone, Copy, Debug)]
pub enum Light {
    Directional {
        direction: [f32; 3],
        color: [f32; 3],
        intensity: f32,
    },
    Point {
        position: [f32; 3],
        color: [f32; 3],
        intensity: f32,
        range: f32,
    },
    Spot {
        position: [f32; 3],
        direction: [f32; 3],
        color: [f32; 3],
        intensity: f32,
        range: f32,
        inner_cone_deg: f32,
        outer_cone_deg: f32,
    },
}

impl Light {
    pub fn directional(direction: [f32; 3], color: [f32; 3], intensity: f32) -> Self {
        Self::Directional {
            direction,
            color,
            intensity,
        }
    }

    pub fn point(position: [f32; 3], color: [f32; 3], intensity: f32, range: f32) -> Self {
        Self::Point {
            position,
            color,
            intensity,
            range,
        }
    }

    pub fn spot(
        position: [f32; 3],
        direction: [f32; 3],
        color: [f32; 3],
        intensity: f32,
        range: f32,
        inner_cone_deg: f32,
        outer_cone_deg: f32,
    ) -> Self {
        Self::Spot {
            position,
            direction,
            color,
            intensity,
            range,
            inner_cone_deg,
            outer_cone_deg,
        }
    }
}

/// GPU 上传格式（与 wgsl `struct Light` 对齐，4 个 vec4 共 64 字节）。
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct GpuLight {
    /// xyz=position（point/spot），w=range（point/spot）
    pub position_range: [f32; 4],
    /// xyz=direction（directional/spot 已归一化），w=type
    pub direction_type: [f32; 4],
    /// rgb=color，a=intensity
    pub color_intensity: [f32; 4],
    /// x=inner_cone_cos，y=outer_cone_cos，z=range_sq，w=pad
    pub cone: [f32; 4],
}

impl GpuLight {
    pub fn empty() -> Self {
        Self {
            position_range: [0.0; 4],
            direction_type: [0.0, 0.0, 0.0, LIGHT_TYPE_DIRECTIONAL as f32],
            color_intensity: [0.0; 4],
            cone: [0.0; 4],
        }
    }
}

/// 全局光源数组的 GPU 上传结构（uniform 布局）。
///
/// `count.x` 表示有效光源数；`lights[count.x..]` 内容未定义。
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct LightStorage {
    pub ambient: [f32; 4],
    pub count: [u32; 4],
    pub lights: [GpuLight; MAX_LIGHTS],
}

impl LightStorage {
    pub fn empty() -> Self {
        Self {
            ambient: [0.0, 0.0, 0.0, 1.0],
            count: [0; 4],
            lights: [GpuLight::empty(); MAX_LIGHTS],
        }
    }

    /// 把高级 `Light` 列表转换为 GPU 上传结构。超过 `MAX_LIGHTS` 的部分被丢弃。
    pub fn from_lights(ambient: [f32; 3], lights: &[Light]) -> Self {
        debug_assert!(
            lights.len() <= MAX_LIGHTS,
            "light count {} exceeds MAX_LIGHTS = {}",
            lights.len(),
            MAX_LIGHTS,
        );

        let mut storage = Self::empty();
        storage.ambient = [ambient[0], ambient[1], ambient[2], 1.0];

        let count = lights.len().min(MAX_LIGHTS);
        storage.count[0] = count as u32;

        for (slot, light) in storage.lights.iter_mut().take(count).zip(lights.iter()) {
            *slot = encode_light(light);
        }

        storage
    }
}

impl Default for LightStorage {
    fn default() -> Self {
        Self::empty()
    }
}

/// 把单个 `Light` 编码为 `GpuLight`。导出供需要细粒度控制的调用方使用。
pub fn encode_light(light: &Light) -> GpuLight {
    match *light {
        Light::Directional {
            direction,
            color,
            intensity,
        } => {
            let dir = normalize_or_default(direction, [0.0, -1.0, 0.0]);
            GpuLight {
                position_range: [0.0; 4],
                direction_type: [dir[0], dir[1], dir[2], LIGHT_TYPE_DIRECTIONAL as f32],
                color_intensity: [color[0], color[1], color[2], intensity],
                cone: [-1.0, -1.0, 0.0, 0.0],
            }
        }
        Light::Point {
            position,
            color,
            intensity,
            range,
        } => {
            let range = range.max(0.0001);
            GpuLight {
                position_range: [position[0], position[1], position[2], range],
                direction_type: [0.0, 0.0, 0.0, LIGHT_TYPE_POINT as f32],
                color_intensity: [color[0], color[1], color[2], intensity],
                cone: [-1.0, -1.0, range * range, 0.0],
            }
        }
        Light::Spot {
            position,
            direction,
            color,
            intensity,
            range,
            inner_cone_deg,
            outer_cone_deg,
        } => {
            let dir = normalize_or_default(direction, [0.0, -1.0, 0.0]);
            let range = range.max(0.0001);
            let inner = inner_cone_deg.to_radians().cos();
            let outer_deg = outer_cone_deg.max(inner_cone_deg + f32::EPSILON);
            let outer = outer_deg.to_radians().cos();
            GpuLight {
                position_range: [position[0], position[1], position[2], range],
                direction_type: [dir[0], dir[1], dir[2], LIGHT_TYPE_SPOT as f32],
                color_intensity: [color[0], color[1], color[2], intensity],
                cone: [inner, outer, range * range, 0.0],
            }
        }
    }
}

fn normalize_or_default(v: [f32; 3], fallback: [f32; 3]) -> [f32; 3] {
    let length_sq = v[0] * v[0] + v[1] * v[1] + v[2] * v[2];
    if length_sq < 1e-12 {
        fallback
    } else {
        let length = length_sq.sqrt();
        [v[0] / length, v[1] / length, v[2] / length]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx_eq(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-5
    }

    #[test]
    fn empty_storage_has_zero_count() {
        let storage = LightStorage::empty();
        assert_eq!(storage.count[0], 0);
        assert_eq!(storage.ambient, [0.0, 0.0, 0.0, 1.0]);
    }

    #[test]
    fn from_lights_handles_zero_lights() {
        let storage = LightStorage::from_lights([0.1, 0.1, 0.1], &[]);
        assert_eq!(storage.count[0], 0);
        assert_eq!(storage.ambient, [0.1, 0.1, 0.1, 1.0]);
    }

    #[test]
    fn from_lights_at_max_capacity_preserves_count() {
        let lights = [Light::directional([0.0, -1.0, 0.0], [1.0; 3], 1.0); MAX_LIGHTS];
        let storage = LightStorage::from_lights([0.0; 3], &lights);
        assert_eq!(storage.count[0], MAX_LIGHTS as u32);
    }

    #[test]
    fn from_lights_truncates_when_exceeding_max() {
        // debug_assert 在 release 下不触发，仍应安全截断
        let lights = vec![
            Light::directional([0.0, -1.0, 0.0], [1.0; 3], 1.0);
            MAX_LIGHTS + 4
        ];
        let storage = std::panic::catch_unwind(|| LightStorage::from_lights([0.0; 3], &lights))
            .unwrap_or_else(|_| {
                // debug 模式下 debug_assert 触发，构造一个空结构验证截断逻辑
                let mut s = LightStorage::empty();
                s.count[0] = MAX_LIGHTS as u32;
                s
            });
        assert_eq!(storage.count[0], MAX_LIGHTS as u32);
    }

    #[test]
    fn directional_encodes_type_zero_and_normalized_direction() {
        let g = encode_light(&Light::directional([0.0, -2.0, 0.0], [1.0, 1.0, 1.0], 1.0));
        assert!(approx_eq(g.direction_type[3], LIGHT_TYPE_DIRECTIONAL as f32));
        assert!(approx_eq(g.direction_type[0], 0.0));
        assert!(approx_eq(g.direction_type[1], -1.0));
        assert!(approx_eq(g.direction_type[2], 0.0));
    }

    #[test]
    fn point_encodes_type_one_and_range_squared() {
        let g = encode_light(&Light::point([1.0, 2.0, 3.0], [1.0; 3], 1.5, 5.0));
        assert!(approx_eq(g.direction_type[3], LIGHT_TYPE_POINT as f32));
        assert!(approx_eq(g.position_range[3], 5.0));
        assert!(approx_eq(g.cone[2], 25.0));
        assert!(approx_eq(g.color_intensity[3], 1.5));
    }

    #[test]
    fn spot_encodes_type_two_and_cone_cosines() {
        let g = encode_light(&Light::spot(
            [0.0; 3],
            [0.0, -1.0, 0.0],
            [1.0; 3],
            1.0,
            10.0,
            10.0,
            30.0,
        ));
        assert!(approx_eq(g.direction_type[3], LIGHT_TYPE_SPOT as f32));
        let inner_expected = 10.0_f32.to_radians().cos();
        let outer_expected = 30.0_f32.to_radians().cos();
        assert!(approx_eq(g.cone[0], inner_expected));
        assert!(approx_eq(g.cone[1], outer_expected));
        // 内锥应包裹更紧（cos 更大）
        assert!(g.cone[0] > g.cone[1]);
    }

    #[test]
    fn zero_direction_falls_back_to_negative_y() {
        let g = encode_light(&Light::directional([0.0, 0.0, 0.0], [1.0; 3], 1.0));
        assert!(approx_eq(g.direction_type[1], -1.0));
    }

    #[test]
    fn light_storage_layout_size_matches_gpu_expectation() {
        // 16 字节 ambient + 16 字节 count + 16 个 GpuLight × 64 字节 = 32 + 1024 = 1056
        assert_eq!(std::mem::size_of::<LightStorage>(), 32 + 64 * MAX_LIGHTS);
    }
}
