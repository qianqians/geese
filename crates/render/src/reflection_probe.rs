//! 反射探针系统 — 捕获环境并在附近物体上渲染反射。
//!
//! 探针从世界某点渲染 6 面 cubemap，供延迟/前向渲染 lighting pass 采样。
//! 当前版本为骨架实现，GPU 渲染部分标记为 TODO。

use bytemuck::{Pod, Zeroable};

/// cubemap 纹理句柄（实际 GPU 资源索引由 backend 维护）。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct CubemapHandle {
    /// 纹理数组中的索引或句柄
    pub id: u32,
}

/// 单个反射探针。
#[derive(Clone, Debug)]
pub struct ReflectionProbe {
    /// 探针世界坐标位置
    pub position: [f32; 3],
    /// 影响半径（米）
    pub influence_radius: f32,
    /// cubemap 分辨率（正方形面，如 256）
    pub capture_size: u32,
    /// 探针间过渡距离（米）：从 influence_radius 向外延伸的衰减宽度
    pub blend_distance: f32,
    /// 捕获的环境贴图（None 表示尚未捕获）
    pub cubemap: Option<CubemapHandle>,
    /// 是否启用
    pub enabled: bool,
}

impl Default for ReflectionProbe {
    fn default() -> Self {
        Self {
            position: [0.0, 0.0, 0.0],
            influence_radius: 5.0,
            capture_size: 256,
            blend_distance: 1.0,
            cubemap: None,
            enabled: true,
        }
    }
}

/// 探针影响权重结果。
#[derive(Clone, Copy, Debug)]
pub struct ProbeInfluence {
    /// 探针索引
    pub probe_index: usize,
    /// 影响权重（0 = 无影响，1 = 完全影响）
    pub weight: f32,
}

/// 反射探针系统 — 管理多个探针及其影响计算。
#[derive(Clone, Debug, Default)]
pub struct ReflectionProbeSystem {
    probes: Vec<ReflectionProbe>,
}

impl ReflectionProbeSystem {
    pub fn new() -> Self {
        Self { probes: Vec::new() }
    }

    /// 添加探针，返回其索引。
    pub fn add_probe(&mut self, probe: ReflectionProbe) -> usize {
        let idx = self.probes.len();
        self.probes.push(probe);
        idx
    }

    /// 移除探针（将其 enabled 设为 false，避免索引重排）。
    pub fn remove_probe(&mut self, index: usize) {
        if let Some(p) = self.probes.get_mut(index) {
            p.enabled = false;
        }
    }

    /// 更新探针参数。
    pub fn update_probe<F: FnOnce(&mut ReflectionProbe)>(&mut self, index: usize, f: F) {
        if let Some(p) = self.probes.get_mut(index) {
            f(p);
        }
    }

    /// 从探针位置渲染 6 面 cubemap。
    ///
    /// TODO: 实际 GPU 渲染需绑定 wgpu 渲染管线，从 probe.position 渲染 6 个面。
    /// 当前仅在探针已有 cubemap 句柄时记录调用。
    pub fn capture_environment(&mut self, index: usize, handle: CubemapHandle) {
        if let Some(p) = self.probes.get_mut(index) {
            p.cubemap = Some(handle);
            // TODO: 实际渲染 6 面 cubemap 并写入 handle 对应的 GPU 纹理
        }
    }

    /// 计算某世界位置受哪些探针影响，返回各探针权重（按距离排序，最多 max_results 个）。
    ///
    /// 权重计算：在 [influence_radius, influence_radius + blend_distance] 区间线性衰减。
    pub fn get_influence_at(&self, point: [f32; 3], max_results: usize) -> Vec<ProbeInfluence> {
        let mut influences: Vec<ProbeInfluence> = self
            .probes
            .iter()
            .enumerate()
            .filter(|(_, p)| p.enabled && p.cubemap.is_some())
            .filter_map(|(i, p)| {
                let dx = point[0] - p.position[0];
                let dy = point[1] - p.position[1];
                let dz = point[2] - p.position[2];
                let dist = (dx * dx + dy * dy + dz * dz).sqrt();

                if dist > p.influence_radius + p.blend_distance {
                    return None;
                }

                let weight = if dist <= p.influence_radius {
                    1.0
                } else {
                    let t = (dist - p.influence_radius) / p.blend_distance.max(1e-6);
                    (1.0 - t).clamp(0.0, 1.0)
                };

                Some(ProbeInfluence { probe_index: i, weight })
            })
            .collect();

        // 按权重降序排列
        influences.sort_by(|a, b| b.weight.partial_cmp(&a.weight).unwrap_or(std::cmp::Ordering::Equal));
        influences.truncate(max_results);
        influences
    }

    /// 获取所有探针（只读）。
    pub fn probes(&self) -> &[ReflectionProbe] {
        &self.probes
    }

    /// 获取活跃（enabled + 有 cubemap）探针数量。
    pub fn active_count(&self) -> usize {
        self.probes.iter().filter(|p| p.enabled && p.cubemap.is_some()).count()
    }

    /// 构建 GPU uniform 数据（最多 MAX_REFLECTION_PROBES 个探针）。
    pub fn build_uniform(&self) -> ReflectionProbeUniform {
        let mut uni = ReflectionProbeUniform::default();
        uni.count = self
            .probes
            .iter()
            .filter(|p| p.enabled && p.cubemap.is_some())
            .count()
            .min(MAX_REFLECTION_PROBES) as u32;

        let mut idx = 0usize;
        for p in self.probes.iter().filter(|p| p.enabled && p.cubemap.is_some()) {
            if idx >= MAX_REFLECTION_PROBES {
                break;
            }
            uni.positions[idx] = [
                p.position[0],
                p.position[1],
                p.position[2],
                p.influence_radius,
            ];
            uni.params[idx] = [
                p.blend_distance,
                if p.cubemap.is_some() { 1.0 } else { 0.0 },
                0.0,
                0.0,
            ];
            idx += 1;
        }
        uni
    }
}

/// GPU 可上传的反射探针最大数量。
pub const MAX_REFLECTION_PROBES: usize = 8;

/// GPU uniform 数据 — 反射探针位置/半径/过渡距离。
///
/// 布局（std140 兼容）：
/// - count: u32（vec4 对齐）
/// - positions[MAX_REFLECTION_PROBES]: vec4（xyz=位置, w=influence_radius）
/// - params[MAX_REFLECTION_PROBES]: vec4（x=blend_distance, y=has_cubemap, zw=pad）
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct ReflectionProbeUniform {
    /// [count, 0, 0, 0]
    pub count: u32,
    pub _pad0: [u32; 3],
    /// xyz = 探针位置, w = influence_radius
    pub positions: [[f32; 4]; MAX_REFLECTION_PROBES],
    /// x = blend_distance, y = has_cubemap (0/1), zw = pad
    pub params: [[f32; 4]; MAX_REFLECTION_PROBES],
}

impl Default for ReflectionProbeUniform {
    fn default() -> Self {
        Self {
            count: 0,
            _pad0: [0; 3],
            positions: [[0.0; 4]; MAX_REFLECTION_PROBES],
            params: [[0.0; 4]; MAX_REFLECTION_PROBES],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_and_count() {
        let mut sys = ReflectionProbeSystem::new();
        let h = CubemapHandle { id: 1 };
        let mut p = ReflectionProbe::default();
        p.cubemap = Some(h);
        let idx = sys.add_probe(p);
        assert_eq!(sys.active_count(), 1);
        assert_eq!(idx, 0);
    }

    #[test]
    fn remove_disables_probe() {
        let mut sys = ReflectionProbeSystem::new();
        let h = CubemapHandle { id: 1 };
        let mut p = ReflectionProbe::default();
        p.cubemap = Some(h);
        let idx = sys.add_probe(p);
        sys.remove_probe(idx);
        assert_eq!(sys.active_count(), 0);
    }

    #[test]
    fn influence_inside_radius_is_one() {
        let mut sys = ReflectionProbeSystem::new();
        let mut p = ReflectionProbe::default();
        p.cubemap = Some(CubemapHandle { id: 1 });
        p.influence_radius = 10.0;
        p.blend_distance = 2.0;
        sys.add_probe(p);
        let infl = sys.get_influence_at([0.0, 0.0, 0.0], 4);
        assert_eq!(infl.len(), 1);
        assert!((infl[0].weight - 1.0).abs() < 1e-6);
    }

    #[test]
    fn influence_decays_in_blend_zone() {
        let mut sys = ReflectionProbeSystem::new();
        let mut p = ReflectionProbe::default();
        p.cubemap = Some(CubemapHandle { id: 1 });
        p.influence_radius = 5.0;
        p.blend_distance = 5.0;
        sys.add_probe(p);
        // 距离 = 7.5（在 blend zone 中点）
        let infl = sys.get_influence_at([7.5, 0.0, 0.0], 4);
        assert_eq!(infl.len(), 1);
        assert!((infl[0].weight - 0.5).abs() < 0.05);
    }

    #[test]
    fn influence_beyond_range_is_empty() {
        let mut sys = ReflectionProbeSystem::new();
        let mut p = ReflectionProbe::default();
        p.cubemap = Some(CubemapHandle { id: 1 });
        p.influence_radius = 5.0;
        p.blend_distance = 2.0;
        sys.add_probe(p);
        let infl = sys.get_influence_at([100.0, 0.0, 0.0], 4);
        assert!(infl.is_empty());
    }

    #[test]
    fn build_uniform_sets_count() {
        let mut sys = ReflectionProbeSystem::new();
        let mut p = ReflectionProbe::default();
        p.cubemap = Some(CubemapHandle { id: 1 });
        sys.add_probe(p);
        let uni = sys.build_uniform();
        assert_eq!(uni.count, 1);
    }
}
