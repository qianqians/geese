//! Mesh LOD (Level of Detail) 系统。
//!
//! Feature gate: `lod`（默认禁用）。
//!
//! ## 设计
//! - CPU 端 LOD 选择：根据相机到物体中心的距离，从 `ModelMesh::lod_levels` 中选择
//!   合适的 LOD 级别。
//! - 距离计算：世界空间欧氏距离（camera_position ↔ model_matrix 平移分量）。
//! - 空 `lod_levels` = 单级 LOD，始终使用完整网格。

use crate::mesh::LodLevel;

/// LOD 选择结果：返回最适合当前距离的 LOD 级别。
///
/// # 参数
/// - `lod_levels`: 距离阈值降序排列的 LOD 列表（`distance` 单位：世界空间）
/// - `camera_distance`: 相机到物体中心的世界空间距离
///
/// # 返回
/// - 匹配的 `LodLevel` 引用
/// - 若 `lod_levels` 为空（无 LOD），返回 `None`（使用完整网格）
pub fn select_lod<'a>(
    lod_levels: &'a [LodLevel],
    camera_distance: f32,
) -> Option<&'a LodLevel> {
    if lod_levels.is_empty() {
        return None;
    }
    // lod_levels 按 distance 降序排列；找到第一个 distance <= camera_distance 的级别。
    // 若距离小于所有阈值，使用最后一个（最高细节）。
    for level in lod_levels.iter() {
        if camera_distance >= level.distance {
            return Some(level);
        }
    }
    // 距离比所有阈值都小 → 使用最高细节（最后一个）
    lod_levels.last()
}

/// 从 4×4 模型矩阵提取世界空间平移分量。
#[inline]
pub fn extract_translation(model: &[[f32; 4]; 4]) -> [f32; 3] {
    [model[0][3], model[1][3], model[2][3]]
}

/// 计算物体到相机的世界空间距离。
#[inline]
pub fn camera_distance(object_pos: [f32; 3], camera_pos: [f32; 3]) -> f32 {
    let dx = object_pos[0] - camera_pos[0];
    let dy = object_pos[1] - camera_pos[1];
    let dz = object_pos[2] - camera_pos[2];
    (dx * dx + dy * dy + dz * dz).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn select_lod_empty_returns_none() {
        assert!(select_lod(&[], 100.0).is_none());
    }

    #[test]
    fn select_lod_near_camera_uses_highest_detail() {
        let levels = vec![
            LodLevel { distance: 50.0, variant_index: 1, index_count: 100 },
            LodLevel { distance: 20.0, variant_index: 0, index_count: 1000 },
        ];
        // 距离 5 < 所有阈值 → 最高细节（最后一个）
        let selected = select_lod(&levels, 5.0).unwrap();
        assert_eq!(selected.variant_index, 0);
        assert_eq!(selected.index_count, 1000);
    }

    #[test]
    fn select_lod_mid_range_uses_medium() {
        let levels = vec![
            LodLevel { distance: 50.0, variant_index: 2, index_count: 10 },
            LodLevel { distance: 20.0, variant_index: 1, index_count: 100 },
            LodLevel { distance: 10.0, variant_index: 0, index_count: 1000 },
        ];
        // 距离 25: >= 20 → variant_index 1
        let selected = select_lod(&levels, 25.0).unwrap();
        assert_eq!(selected.variant_index, 1);
    }

    #[test]
    fn select_lod_far_uses_lowest_detail() {
        let levels = vec![
            LodLevel { distance: 50.0, variant_index: 2, index_count: 10 },
            LodLevel { distance: 20.0, variant_index: 1, index_count: 100 },
        ];
        let selected = select_lod(&levels, 100.0).unwrap();
        assert_eq!(selected.variant_index, 2);
    }

    #[test]
    fn extract_translation_correct() {
        let model = [
            [1.0, 0.0, 0.0, 5.0],
            [0.0, 1.0, 0.0, 10.0],
            [0.0, 0.0, 1.0, 15.0],
            [0.0, 0.0, 0.0, 1.0],
        ];
        assert_eq!(extract_translation(&model), [5.0, 10.0, 15.0]);
    }

    #[test]
    fn camera_distance_correct() {
        let d = camera_distance([0.0, 0.0, 0.0], [3.0, 4.0, 0.0]);
        assert!((d - 5.0).abs() < 0.001);
    }
}
