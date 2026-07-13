//! 地形纹理 splatting：基于高度和坡度的多层纹理混合。
//!
//! `TerrainSplatting` 管理最多 4 个 `SplatLayer`，每个层定义高度范围、坡度范围及
//! 可选权重图。`TerrainMesher` 在生成顶点时调用 `compute_weights()` 得到 4 路混合权重，
//! 供 fragment shader 混合多层纹理颜色。

/// 单个 splat 层。
#[derive(Clone, Debug)]
pub struct SplatLayer {
    /// 纹理句柄（由上层渲染系统赋值，此处仅存标识字符串）。
    pub texture_handle: String,
    /// 高度范围：仅在此区间内的顶点才受该层影响。
    pub height_min: f32,
    pub height_max: f32,
    /// 坡度范围（normal.y 的值域，1.0 = 完全平坦，0.0 = 垂直）。
    pub slope_min: f32,
    pub slope_max: f32,
    /// 可选的 CPU 侧权重图（width×height，与 heightmap 同分辨率），值域 [0,1]。
    /// 若为 None，则仅按高度/坡度计算权重。
    pub weight_map: Option<Vec<f32>>,
}

impl SplatLayer {
    /// 创建一个新的 splat 层。
    pub fn new(
        texture_handle: impl Into<String>,
        height_range: (f32, f32),
        slope_range: (f32, f32),
    ) -> Self {
        Self {
            texture_handle: texture_handle.into(),
            height_min: height_range.0,
            height_max: height_range.1,
            slope_min: slope_range.0,
            slope_max: slope_range.1,
            weight_map: None,
        }
    }

    /// 附加 CPU 侧权重图。
    pub fn with_weight_map(mut self, map: Vec<f32>) -> Self {
        self.weight_map = Some(map);
        self
    }

    /// 根据高度和坡度计算该层的原始权重（未归一化）。
    ///
    /// - `height`: 世界空间高度
    /// - `slope`: 坡度值 = normal.y，范围 [0,1]，1=平坦
    pub fn raw_weight(&self, height: f32, slope: f32) -> f32 {
        // 高度衰减：在 [height_min, height_max] 内为 1.0，边界外 smoothstep 衰减
        let h_range = self.height_max - self.height_min;
        let h_fade = if h_range <= 0.0 {
            // 单层覆盖全高度
            1.0
        } else {
            let mid = (self.height_min + self.height_max) * 0.5;
            let half = h_range * 0.5;
            let d = (height - mid).abs() / half;
            // smoothstep 衰减：在 [0,1] 内为 1.0，超过 1 平滑到 0
            if d <= 1.0 {
                1.0 - d * d * (3.0 - 2.0 * d)
            } else {
                0.0
            }
        };

        // 坡度衰减：在 [slope_min, slope_max] 内为 1.0，边界外 smoothstep 衰减
        let s_range = self.slope_max - self.slope_min;
        let s_fade = if s_range <= 0.0 {
            1.0
        } else {
            let mid = (self.slope_min + self.slope_max) * 0.5;
            let half = s_range * 0.5;
            let d = if half > 0.0 { (slope - mid).abs() / half } else { 0.0 };
            if d <= 1.0 {
                1.0 - d * d * (3.0 - 2.0 * d)
            } else {
                0.0
            }
        };

        h_fade * s_fade
    }
}

/// 地形纹理 splatting 配置：最多 4 层。
#[derive(Clone, Debug)]
pub struct TerrainSplatting {
    pub layers: Vec<SplatLayer>,
}

impl TerrainSplatting {
    /// 创建空的 splatting 配置。
    pub fn new() -> Self {
        Self { layers: Vec::new() }
    }

    /// 添加一个层（最多 4 层，超出将被忽略）。
    pub fn add_layer(&mut self, layer: SplatLayer) {
        if self.layers.len() < 4 {
            self.layers.push(layer);
        }
    }

    /// 根据高度和坡度计算 4 路混合权重（归一化，总和 = 1.0）。
    ///
    /// 未配置层的通道权重为 0.0；若所有层原始权重均为 0，则回退到 channel 0 = 1.0。
    pub fn compute_weights(&self, height: f32, slope: f32) -> [f32; 4] {
        let mut weights = [0.0f32; 4];
        let mut total = 0.0;

        for (i, layer) in self.layers.iter().enumerate().take(4) {
            let w = layer.raw_weight(height, slope);
            weights[i] = w;
            total += w;
        }

        // 归一化
        if total > 1e-6 {
            for w in &mut weights {
                *w /= total;
            }
        } else {
            // 回退：第 0 层全权
            weights[0] = 1.0;
        }

        weights
    }

    /// 当前配置的层数。
    pub fn layer_count(&self) -> usize {
        self.layers.len()
    }
}

impl Default for TerrainSplatting {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_splatting() -> TerrainSplatting {
        let mut s = TerrainSplatting::new();
        // 层 0：草地，低海拔，平坦
        s.add_layer(SplatLayer::new("grass", (0.0, 20.0), (0.7, 1.0)));
        // 层 1：岩石，高海拔，任意坡度
        s.add_layer(SplatLayer::new("rock", (15.0, 100.0), (0.0, 1.0)));
        // 层 2：雪，高海拔，平坦
        s.add_layer(SplatLayer::new("snow", (50.0, 200.0), (0.6, 1.0)));
        // 层 3：峭壁，任意高度，陡峭
        s.add_layer(SplatLayer::new("cliff", (0.0, 200.0), (0.0, 0.4)));
        s
    }

    #[test]
    fn weights_sum_to_one() {
        let s = make_splatting();
        let w = s.compute_weights(10.0, 0.9);
        let sum: f32 = w.iter().sum();
        assert!((sum - 1.0).abs() < 1e-5, "weights = {w:?}, sum = {sum}");
    }

    #[test]
    fn low_flat_terrain_favors_grass() {
        let s = make_splatting();
        let w = s.compute_weights(5.0, 0.95);
        // 低海拔 + 平坦 → grass (layer 0) 权重最大
        assert!(w[0] > w[1], "grass should dominate: {w:?}");
    }

    #[test]
    fn steep_slope_favors_cliff() {
        let s = make_splatting();
        let w = s.compute_weights(10.0, 0.1);
        // 陡峭 → cliff (layer 3) 应有显著权重
        assert!(w[3] > 0.0, "cliff should have weight: {w:?}");
    }

    #[test]
    fn empty_splatting_falls_back() {
        let s = TerrainSplatting::new();
        let w = s.compute_weights(50.0, 0.5);
        assert_eq!(w, [1.0, 0.0, 0.0, 0.0]);
    }

    #[test]
    fn max_four_layers() {
        let mut s = TerrainSplatting::new();
        for i in 0..6 {
            s.add_layer(SplatLayer::new(format!("layer_{i}"), (0.0, 100.0), (0.0, 1.0)));
        }
        assert_eq!(s.layer_count(), 4);
    }
}
