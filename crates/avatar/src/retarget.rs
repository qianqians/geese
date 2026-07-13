//! 动画重定向 — 将动画从一个骨架迁移到另一个骨架。
//!
//! 通过骨骼名称映射和缩放因子，将源动画剪辑的通道重新映射到目标骨架。

use std::collections::HashMap;

use crate::animation::{
    AnimatedProperty, AnimationChannel, AnimationClip, AnimationOutputs,
};

/// 骨骼映射配置：描述源骨架与目标骨架之间的对应关系。
#[derive(Clone, Debug)]
pub struct BoneMapping {
    /// 源骨骼名 → 目标骨骼名的映射。
    pub source_to_target: HashMap<String, String>,
    /// 整体缩放因子（应用于所有平移数据）。
    pub scale_factor: f32,
    /// 特定骨骼的缩放覆盖（骨骼名 → 缩放因子）。
    /// 此处的骨骼名为**目标**骨骼名。
    pub bone_scale_overrides: HashMap<String, f32>,
}

impl Default for BoneMapping {
    fn default() -> Self {
        Self {
            source_to_target: HashMap::new(),
            scale_factor: 1.0,
            bone_scale_overrides: HashMap::new(),
        }
    }
}

impl BoneMapping {
    /// 创建空的骨骼映射。
    pub fn new() -> Self {
        Self::default()
    }

    /// 添加一条骨骼名称映射。
    pub fn add_mapping(&mut self, source_bone: &str, target_bone: &str) {
        self.source_to_target
            .insert(source_bone.to_string(), target_bone.to_string());
    }

    /// 为特定目标骨骼设置缩放覆盖。
    pub fn set_bone_scale(&mut self, target_bone: &str, scale: f32) {
        self.bone_scale_overrides
            .insert(target_bone.to_string(), scale);
    }
}

/// 目标骨架描述：骨骼名称列表（索引 = 骨骼 ID）。
#[derive(Clone, Debug)]
pub struct TargetSkeleton {
    /// 目标骨架的骨骼名称，索引即骨骼 ID。
    pub bone_names: Vec<String>,
}

impl TargetSkeleton {
    pub fn new(bone_names: Vec<String>) -> Self {
        Self { bone_names }
    }

    /// 按名称查找骨骼索引。
    pub fn find_bone(&self, name: &str) -> Option<usize> {
        self.bone_names.iter().position(|n| n == name)
    }
}

/// 对动画剪辑执行重定向。
///
/// # 参数
/// - `source_clip`: 源动画剪辑
/// - `mapping`: 骨骼名称映射和缩放配置
/// - `source_bone_names`: 源骨架的骨骼名称列表（索引 = 骨骼 ID）
/// - `target_skeleton`: 目标骨架描述
///
/// # 返回
/// 重定向后的新 `AnimationClip`。无法映射的通道将被丢弃。
///
/// # 规则
/// - 遍历源动画的每个通道
/// - 根据 `source_to_target` 映射到目标骨骼索引
/// - 对平移数据应用缩放（整体 `scale_factor` × 骨骼级覆盖）
/// - 旋转数据直接传递（假设骨骼局部坐标系方向一致）
/// - 缩放（Scale）属性直接传递
pub fn retarget_clip(
    source_clip: &AnimationClip,
    mapping: &BoneMapping,
    source_bone_names: &[String],
    target_skeleton: &TargetSkeleton,
) -> AnimationClip {
    // 构建 source_index → target_index 的快速查找表
    let mut index_remap: HashMap<usize, usize> = HashMap::new();
    for (src_idx, src_name) in source_bone_names.iter().enumerate() {
        if let Some(target_name) = mapping.source_to_target.get(src_name) {
            if let Some(tgt_idx) = target_skeleton.find_bone(target_name) {
                index_remap.insert(src_idx, tgt_idx);
            }
        }
    }

    let mut new_channels = Vec::with_capacity(source_clip.channels.len());

    for channel in &source_clip.channels {
        let target_node = match index_remap.get(&channel.target_node) {
            Some(&idx) => idx,
            None => continue, // 无法映射的通道直接丢弃
        };

        // 确定目标骨骼名称（用于查找缩放覆盖）
        let target_bone_name = &target_skeleton.bone_names[target_node];
        let bone_scale = mapping
            .bone_scale_overrides
            .get(target_bone_name)
            .copied()
            .unwrap_or(1.0);
        let total_scale = mapping.scale_factor * bone_scale;

        let new_outputs = match (&channel.property, &channel.outputs) {
            // 平移需要应用缩放
            (AnimatedProperty::Translation, AnimationOutputs::Translations(verts)) => {
                AnimationOutputs::Translations(
                    verts.iter().map(|v| v * total_scale).collect(),
                )
            }
            // 旋转直接传递
            (AnimatedProperty::Rotation, AnimationOutputs::Rotations(_)) => {
                channel.outputs.clone()
            }
            // 缩放属性直接传递（不按骨骼缩放来乘，否则会双重缩放）
            (AnimatedProperty::Scale, AnimationOutputs::Scales(_)) => {
                channel.outputs.clone()
            }
            _ => channel.outputs.clone(),
        };

        new_channels.push(AnimationChannel {
            target_node,
            property: channel.property,
            interpolation: channel.interpolation,
            inputs: channel.inputs.clone(),
            outputs: new_outputs,
        });
    }

    AnimationClip {
        name: source_clip.name.clone(),
        duration: source_clip.duration,
        channels: new_channels,
        markers: source_clip.markers.clone(),
    }
}

// ---------------------------------------------------------------------------
// 单元测试
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::animation::{
        AnimatedProperty, AnimationChannel, AnimationClip, AnimationMarker, AnimationOutputs,
        Interpolation,
    };
    use cgmath::{Quaternion, Vector3};

    fn make_source_clip() -> AnimationClip {
        AnimationClip {
            name: Some("walk".to_string()),
            duration: 1.0,
            channels: vec![
                // bone 0 (Hips) 平移
                AnimationChannel {
                    target_node: 0,
                    property: AnimatedProperty::Translation,
                    interpolation: Interpolation::Linear,
                    inputs: vec![0.0, 1.0],
                    outputs: AnimationOutputs::Translations(vec![
                        Vector3::new(0.0, 0.0, 0.0),
                        Vector3::new(1.0, 0.0, 0.0),
                    ]),
                },
                // bone 1 (Spine) 旋转
                AnimationChannel {
                    target_node: 1,
                    property: AnimatedProperty::Rotation,
                    interpolation: Interpolation::Linear,
                    inputs: vec![0.0, 1.0],
                    outputs: AnimationOutputs::Rotations(vec![
                        Quaternion::new(1.0, 0.0, 0.0, 0.0),
                        Quaternion::new(0.7071, 0.0, 0.7071, 0.0),
                    ]),
                },
                // bone 2 (LeftArm) 平移 — 该骨骼在目标中不存在，应被丢弃
                AnimationChannel {
                    target_node: 2,
                    property: AnimatedProperty::Translation,
                    interpolation: Interpolation::Linear,
                    inputs: vec![0.0, 1.0],
                    outputs: AnimationOutputs::Translations(vec![
                        Vector3::new(0.0, 1.0, 0.0),
                        Vector3::new(0.0, 2.0, 0.0),
                    ]),
                },
            ],
            markers: vec![AnimationMarker {
                time: 0.5,
                name: "foot_down".to_string(),
            }],
        }
    }

    #[test]
    fn test_retarget_basic() {
        let source_clip = make_source_clip();

        let source_bones: Vec<String> = vec!["Hips", "Spine", "LeftArm"]
            .into_iter()
            .map(String::from)
            .collect();

        let target_skeleton = TargetSkeleton::new(
            vec!["Root", "Pelvis", "Torso", "Head"]
                .into_iter()
                .map(String::from)
                .collect(),
        );

        let mut mapping = BoneMapping::new();
        mapping.add_mapping("Hips", "Pelvis");
        mapping.add_mapping("Spine", "Torso");
        mapping.scale_factor = 2.0;

        let retargeted = retarget_clip(&source_clip, &mapping, &source_bones, &target_skeleton);

        // LeftArm 通道应被丢弃（无映射）
        assert_eq!(retargeted.channels.len(), 2);
        assert_eq!(retargeted.duration, 1.0);

        // 检查 Hips → Pelvis (index 1) 平移，缩放 2x
        let hips_ch = &retargeted.channels[0];
        assert_eq!(hips_ch.target_node, 1);
        if let AnimationOutputs::Translations(ref verts) = hips_ch.outputs {
            assert!((verts[1].x - 2.0).abs() < 1e-4, "scaled x={}", verts[1].x);
            assert!((verts[0].x - 0.0).abs() < 1e-4);
        } else {
            panic!("expected Translations");
        }

        // 检查 Spine → Torso (index 2) 旋转不变
        let spine_ch = &retargeted.channels[1];
        assert_eq!(spine_ch.target_node, 2);
        assert_eq!(spine_ch.property, AnimatedProperty::Rotation);
    }

    #[test]
    fn test_retarget_bone_scale_override() {
        let source_clip = AnimationClip {
            name: None,
            duration: 1.0,
            channels: vec![AnimationChannel {
                target_node: 0,
                property: AnimatedProperty::Translation,
                interpolation: Interpolation::Linear,
                inputs: vec![0.0, 1.0],
                outputs: AnimationOutputs::Translations(vec![
                    Vector3::new(0.0, 0.0, 0.0),
                    Vector3::new(1.0, 1.0, 1.0),
                ]),
            }],
            markers: vec![],
        };

        let source_bones: Vec<String> = vec!["Hips".to_string()];
        let target_skeleton = TargetSkeleton::new(vec!["Pelvis".to_string()]);

        let mut mapping = BoneMapping::new();
        mapping.add_mapping("Hips", "Pelvis");
        mapping.scale_factor = 2.0;
        // Pelvis 的骨骼级缩放覆盖为 3.0，总缩放 = 2.0 * 3.0 = 6.0
        mapping.set_bone_scale("Pelvis", 3.0);

        let retargeted = retarget_clip(&source_clip, &mapping, &source_bones, &target_skeleton);
        if let AnimationOutputs::Translations(ref verts) = retargeted.channels[0].outputs {
            assert!((verts[1].x - 6.0).abs() < 1e-4, "x={}", verts[1].x);
            assert!((verts[1].y - 6.0).abs() < 1e-4, "y={}", verts[1].y);
        } else {
            panic!("expected Translations");
        }
    }

    #[test]
    fn test_retarget_preserves_markers() {
        let source_clip = make_source_clip();
        let source_bones: Vec<String> = vec!["Hips", "Spine", "LeftArm"]
            .into_iter()
            .map(String::from)
            .collect();
        let target_skeleton = TargetSkeleton::new(
            vec!["Pelvis", "Torso"]
                .into_iter()
                .map(String::from)
                .collect(),
        );

        let mut mapping = BoneMapping::new();
        mapping.add_mapping("Hips", "Pelvis");
        mapping.add_mapping("Spine", "Torso");

        let retargeted = retarget_clip(&source_clip, &mapping, &source_bones, &target_skeleton);
        assert_eq!(retargeted.markers.len(), 1);
        assert_eq!(retargeted.markers[0].name, "foot_down");
    }
}
