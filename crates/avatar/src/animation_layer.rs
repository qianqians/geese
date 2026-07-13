//! 动画层系统 — 支持 Override / Additive / AdditiveScaled 混合模式。
//!
//! 允许多个动画在不同层上同时播放，并按从底到顶的顺序混合为最终骨骼姿态。

use cgmath::{InnerSpace, Quaternion, Vector3};

use crate::animation::{
    sample_indices, sample_quat, sample_vec3, AnimationClip, AnimationOutputs,
    AnimationPlayer, Transform,
};

/// 动画层混合模式。
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum BlendMode {
    /// 完全覆盖下层动画（按权重 lerp）。
    Override,
    /// 将（层输出 − 参考姿态）的差异叠加到下层。
    Additive,
    /// 按额外权重缩放后叠加差异。
    AdditiveScaled(f32),
}

/// 单个动画层。
#[derive(Clone, Debug)]
pub struct AnimationLayer {
    /// 层名（如 "Base", "Upper Body", "IK"）。
    pub name: String,
    /// 层权重（0.0 = 无影响，1.0 = 完全影响）。
    pub weight: f32,
    /// 混合模式。
    pub blend_mode: BlendMode,
    /// 骨骼遮罩（`None` = 影响所有骨骼，`Some` = 仅影响指定骨骼索引）。
    pub mask: Option<Vec<usize>>,
    /// 该层的动画播放器（复用现有 `AnimationPlayer`）。
    pub player: AnimationPlayer,
}

/// 多层动画控制器。
///
/// 管理从底到顶的动画层栈，每帧更新后通过 [`evaluate`](Self::evaluate)
/// 计算最终骨骼变换列表。
pub struct LayeredAnimationController {
    /// 从底到顶的动画层。
    pub layers: Vec<AnimationLayer>,
    /// 参考姿态（通常为 T-Pose / Bind-Pose），Additive 层据此计算差异。
    pub reference_pose: Vec<Transform>,
}

impl LayeredAnimationController {
    /// 创建空的层控制器，reference_pose 初始为空。
    pub fn new() -> Self {
        Self {
            layers: Vec::new(),
            reference_pose: Vec::new(),
        }
    }

    /// 设置参考姿态（Additive 混合所必需）。
    pub fn set_reference_pose(&mut self, pose: Vec<Transform>) {
        self.reference_pose = pose;
    }

    /// 添加一个动画层到栈顶。
    pub fn add_layer(
        &mut self,
        name: &str,
        blend_mode: BlendMode,
        weight: f32,
        mask: Option<Vec<usize>>,
        clip_index: usize,
    ) {
        self.layers.push(AnimationLayer {
            name: name.to_string(),
            weight,
            blend_mode,
            mask,
            player: AnimationPlayer::new(clip_index),
        });
    }

    /// 按名称移除动画层。
    pub fn remove_layer(&mut self, name: &str) {
        self.layers.retain(|l| l.name != name);
    }

    /// 调整指定层的权重。
    pub fn set_layer_weight(&mut self, name: &str, weight: f32) {
        if let Some(layer) = self.layers.iter_mut().find(|l| l.name == name) {
            layer.weight = weight;
        }
    }

    /// 在指定层上播放新的动画剪辑。
    pub fn play_on_layer(&mut self, name: &str, clip_index: usize) {
        if let Some(layer) = self.layers.iter_mut().find(|l| l.name == name) {
            layer.player = AnimationPlayer::new(clip_index);
        }
    }

    /// 获取指定层的不可变引用。
    pub fn get_layer(&self, name: &str) -> Option<&AnimationLayer> {
        self.layers.iter().find(|l| l.name == name)
    }

    /// 获取指定层的可变引用。
    pub fn get_layer_mut(&mut self, name: &str) -> Option<&mut AnimationLayer> {
        self.layers.iter_mut().find(|l| l.name == name)
    }

    /// 推进所有层的动画播放器。
    pub fn update(&mut self, dt: f32, clips: &[AnimationClip]) {
        for layer in &mut self.layers {
            if let Some(clip) = clips.get(layer.player.clip) {
                layer.player.advance(dt, clip.duration);
            }
        }
    }

    /// 计算最终骨骼变换。
    ///
    /// 1. 底层作为基础姿态
    /// 2. 遍历上层：根据 `blend_mode` 和 `mask` 混合
    ///    - **Override**: `result = lerp(base, layer_output, weight)`
    ///    - **Additive**: `result = base + (layer_output − reference_pose) * weight`
    ///    - **AdditiveScaled(s)**: `result = base + (layer_output − reference_pose) * weight * s`
    /// 3. 返回最终骨骼变换列表
    pub fn evaluate(&self, bone_count: usize, clips: &[AnimationClip]) -> Vec<Transform> {
        // 用 reference_pose 或 default 初始化结果
        let mut result: Vec<Transform> = if self.reference_pose.len() == bone_count {
            self.reference_pose.clone()
        } else {
            vec![Transform::default(); bone_count]
        };

        for (layer_idx, layer) in self.layers.iter().enumerate() {
            if layer.weight <= 0.0 {
                continue;
            }
            let clip = match clips.get(layer.player.clip) {
                Some(c) => c,
                None => continue,
            };

            let layer_transforms = sample_clip_to_transforms(clip, layer.player.time, bone_count);

            if layer_idx == 0 {
                // 底层始终覆盖（按权重缩放，若 weight<1 则与 reference_pose 混合）
                for i in 0..bone_count {
                    if should_apply(&layer.mask, i) {
                        result[i] = blend_override(result[i], layer_transforms[i], layer.weight);
                    }
                }
            } else {
                match layer.blend_mode {
                    BlendMode::Override => {
                        for i in 0..bone_count {
                            if should_apply(&layer.mask, i) {
                                result[i] =
                                    blend_override(result[i], layer_transforms[i], layer.weight);
                            }
                        }
                    }
                    BlendMode::Additive => {
                        for i in 0..bone_count {
                            if should_apply(&layer.mask, i) {
                                let ref_pose = get_ref_pose(&self.reference_pose, i);
                                result[i] =
                                    blend_additive(result[i], layer_transforms[i], ref_pose, layer.weight);
                            }
                        }
                    }
                    BlendMode::AdditiveScaled(scale) => {
                        for i in 0..bone_count {
                            if should_apply(&layer.mask, i) {
                                let ref_pose = get_ref_pose(&self.reference_pose, i);
                                result[i] = blend_additive(
                                    result[i],
                                    layer_transforms[i],
                                    ref_pose,
                                    layer.weight * scale,
                                );
                            }
                        }
                    }
                }
            }
        }

        result
    }
}

impl Default for LayeredAnimationController {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// 内部辅助函数
// ---------------------------------------------------------------------------

/// 判断骨骼索引是否应被该层遮罩影响。
fn should_apply(mask: &Option<Vec<usize>>, bone_index: usize) -> bool {
    match mask {
        None => true,
        Some(indices) => indices.contains(&bone_index),
    }
}

/// 获取参考姿态中指定骨骼的变换（越界返回默认）。
fn get_ref_pose(reference_pose: &[Transform], index: usize) -> Transform {
    reference_pose.get(index).copied().unwrap_or_default()
}

/// Override 混合：`lerp(base, layer, weight)`。
fn blend_override(base: Transform, layer: Transform, weight: f32) -> Transform {
    let w = weight.clamp(0.0, 1.0);
    Transform {
        translation: base.translation + (layer.translation - base.translation) * w,
        rotation: slerp_quat(base.rotation, layer.rotation, w),
        scale: base.scale + (layer.scale - base.scale) * w,
    }
}

/// Additive 混合：`base + (layer - reference) * weight`。
fn blend_additive(base: Transform, layer: Transform, reference: Transform, weight: f32) -> Transform {
    let w = weight.clamp(0.0, 1.0);
    // 平移/缩放差异叠加
    let delta_translation = (layer.translation - reference.translation) * w;
    let delta_scale = (layer.scale - reference.scale) * w;
    // 旋转差异：delta_rot = layer * inverse(reference)，然后按 weight slerp
    let ref_inv = quat_inverse(reference.rotation);
    let delta_rot = layer.rotation * ref_inv;
    let identity = Quaternion::new(1.0, 0.0, 0.0, 0.0);
    let applied_rot = slerp_quat(identity, delta_rot, w) * base.rotation;

    Transform {
        translation: base.translation + delta_translation,
        rotation: applied_rot.normalize(),
        scale: Vector3::new(
            (base.scale.x + delta_scale.x).max(0.0),
            (base.scale.y + delta_scale.y).max(0.0),
            (base.scale.z + delta_scale.z).max(0.0),
        ),
    }
}

/// 四元数共轭（对单位四元数即逆）。
fn quat_inverse(q: Quaternion<f32>) -> Quaternion<f32> {
    Quaternion::new(q.s, -q.v.x, -q.v.y, -q.v.z)
}

/// 安全的四元数 slerp（处理对径点和退化情况）。
fn slerp_quat(a: Quaternion<f32>, mut b: Quaternion<f32>, t: f32) -> Quaternion<f32> {
    let mut dot = a.s * b.s + a.v.dot(b.v);
    if dot < 0.0 {
        b = Quaternion::new(-b.s, -b.v.x, -b.v.y, -b.v.z);
        dot = -dot;
    }
    if dot > 0.9995 {
        let result = Quaternion::new(
            a.s + t * (b.s - a.s),
            a.v.x + t * (b.v.x - a.v.x),
            a.v.y + t * (b.v.y - a.v.y),
            a.v.z + t * (b.v.z - a.v.z),
        );
        return result.normalize();
    }
    let theta_0 = dot.acos();
    let theta = theta_0 * t;
    let sin_theta = theta.sin();
    let sin_theta_0 = theta_0.sin();
    let s0 = ((1.0 - t) * theta_0).sin() / sin_theta_0;
    let s1 = sin_theta / sin_theta_0;
    Quaternion::new(
        s0 * a.s + s1 * b.s,
        s0 * a.v.x + s1 * b.v.x,
        s0 * a.v.y + s1 * b.v.y,
        s0 * a.v.z + s1 * b.v.z,
    )
}

/// 将动画剪辑在指定时刻采样为一组骨骼变换（不影响 `SceneNode`）。
pub fn sample_clip_to_transforms(
    clip: &AnimationClip,
    time: f32,
    bone_count: usize,
) -> Vec<Transform> {
    let mut transforms = vec![Transform::default(); bone_count];
    for channel in &clip.channels {
        if channel.inputs.is_empty() || channel.target_node >= bone_count {
            continue;
        }
        let outputs = &channel.outputs;
        let (left, right, factor, interval) =
            sample_indices(&channel.inputs, time, channel.interpolation);
        let transform = &mut transforms[channel.target_node];

        match (&channel.property, &channel.outputs) {
            (crate::animation::AnimatedProperty::Translation, AnimationOutputs::Translations(_)) => {
                transform.translation =
                    sample_vec3(outputs, left, right, factor, interval, channel.interpolation);
            }
            (crate::animation::AnimatedProperty::Rotation, AnimationOutputs::Rotations(_)) => {
                transform.rotation =
                    sample_quat(outputs, left, right, factor, interval, channel.interpolation);
            }
            (crate::animation::AnimatedProperty::Scale, AnimationOutputs::Scales(_)) => {
                transform.scale =
                    sample_vec3(outputs, left, right, factor, interval, channel.interpolation);
            }
            _ => {}
        }
    }
    transforms
}

// ---------------------------------------------------------------------------
// 单元测试
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::animation::{
        AnimatedProperty, AnimationChannel, AnimationClip, AnimationOutputs,
        Interpolation, Transform,
    };
    use cgmath::Vector3;

    /// 创建一个简单的平移动画剪辑：bone 0 从 origin 移到 target。
    fn make_translation_clip(
        name: &str,
        bone: usize,
        origin: Vector3<f32>,
        target: Vector3<f32>,
        duration: f32,
    ) -> AnimationClip {
        AnimationClip {
            name: Some(name.to_string()),
            duration,
            channels: vec![AnimationChannel {
                target_node: bone,
                property: AnimatedProperty::Translation,
                interpolation: Interpolation::Linear,
                inputs: vec![0.0, duration],
                outputs: AnimationOutputs::Translations(vec![origin, target]),
            }],
            markers: vec![],
        }
    }

    #[test]
    fn test_override_layer_blending() {
        // 两层：Base 和 Override，各影响 bone 0
        // Base: bone0 从 (0,0,0) → (10,0,0)，duration=1
        // Override: bone0 从 (0,0,0) → (0,10,0)，duration=1
        let clips = vec![
            make_translation_clip("base", 0, Vector3::new(0.0, 0.0, 0.0), Vector3::new(10.0, 0.0, 0.0), 1.0),
            make_translation_clip("override", 0, Vector3::new(0.0, 0.0, 0.0), Vector3::new(0.0, 10.0, 0.0), 1.0),
        ];

        let mut ctrl = LayeredAnimationController::new();
        ctrl.add_layer("Base", BlendMode::Override, 1.0, None, 0);
        ctrl.add_layer("Upper", BlendMode::Override, 0.5, None, 1);

        // 推进到 t=0.5
        ctrl.update(0.5, &clips);
        let result = ctrl.evaluate(1, &clips);

        // Base at t=0.5: (5, 0, 0)
        // Override at t=0.5: (0, 5, 0)
        // Override blend: lerp((5,0,0), (0,5,0), 0.5) = (2.5, 2.5, 0)
        let t = result[0].translation;
        assert!((t.x - 2.5).abs() < 1e-4, "x={}", t.x);
        assert!((t.y - 2.5).abs() < 1e-4, "y={}", t.y);
        assert!((t.z - 0.0).abs() < 1e-4, "z={}", t.z);
    }

    #[test]
    fn test_additive_layer_blending() {
        // 参考姿态：bone0 在 (0,0,0)
        // Base 层：bone0 → (10,0,0) at t=1
        // Additive 层：bone0 从 default(0,0,0) → (0,5,0)，叠加差异
        let clips = vec![
            make_translation_clip("base", 0, Vector3::new(0.0, 0.0, 0.0), Vector3::new(10.0, 0.0, 0.0), 1.0),
            make_translation_clip("additive", 0, Vector3::new(0.0, 0.0, 0.0), Vector3::new(0.0, 5.0, 0.0), 1.0),
        ];

        let ref_pose = vec![Transform {
            translation: Vector3::new(0.0, 0.0, 0.0),
            ..Transform::default()
        }];

        let mut ctrl = LayeredAnimationController::new();
        ctrl.set_reference_pose(ref_pose);
        ctrl.add_layer("Base", BlendMode::Override, 1.0, None, 0);
        ctrl.add_layer("Additive", BlendMode::Additive, 1.0, None, 1);

        ctrl.update(0.5, &clips);
        let result = ctrl.evaluate(1, &clips);

        // Base at t=0.5: (5, 0, 0)
        // Additive at t=0.5: (0, 2.5, 0); ref=(0,0,0); delta=(0,2.5,0)
        // Result: (5,0,0) + (0,2.5,0) = (5, 2.5, 0)
        let t = result[0].translation;
        assert!((t.x - 5.0).abs() < 1e-4, "x={}", t.x);
        assert!((t.y - 2.5).abs() < 1e-4, "y={}", t.y);
    }

    #[test]
    fn test_additive_scaled_blending() {
        let clips = vec![
            make_translation_clip("base", 0, Vector3::new(0.0, 0.0, 0.0), Vector3::new(10.0, 0.0, 0.0), 1.0),
            make_translation_clip("additive", 0, Vector3::new(0.0, 0.0, 0.0), Vector3::new(0.0, 10.0, 0.0), 1.0),
        ];

        let ref_pose = vec![Transform::default()];
        let mut ctrl = LayeredAnimationController::new();
        ctrl.set_reference_pose(ref_pose);
        ctrl.add_layer("Base", BlendMode::Override, 1.0, None, 0);
        // AdditiveScaled(0.5)：叠加差异 * 0.5
        ctrl.add_layer("ScaledAdditive", BlendMode::AdditiveScaled(0.5), 1.0, None, 1);

        // 推进到 t=1（关闭循环，确保停在末尾）
        for layer in &mut ctrl.layers {
            layer.player.looping = false;
        }
        ctrl.update(1.0, &clips);
        let result = ctrl.evaluate(1, &clips);

        // Base at t=1: (10,0,0)
        // Additive at t=1: (0,10,0); ref=(0,0,0); delta=(0,10,0) * weight(1.0) * scale(0.5) = (0,5,0)
        // Result: (10, 5, 0)
        let t = result[0].translation;
        assert!((t.x - 10.0).abs() < 1e-4, "x={}", t.x);
        assert!((t.y - 5.0).abs() < 1e-4, "y={}", t.y);
    }

    #[test]
    fn test_bone_mask() {
        // 2 骨骼；Override 层只影响 bone 1
        let clips = vec![
            make_translation_clip("base", 0, Vector3::new(0.0, 0.0, 0.0), Vector3::new(10.0, 0.0, 0.0), 1.0),
            AnimationClip {
                name: Some("masked".to_string()),
                duration: 1.0,
                channels: vec![
                    AnimationChannel {
                        target_node: 0,
                        property: AnimatedProperty::Translation,
                        interpolation: Interpolation::Linear,
                        inputs: vec![0.0, 1.0],
                        outputs: AnimationOutputs::Translations(vec![
                            Vector3::new(0.0, 0.0, 0.0),
                            Vector3::new(99.0, 99.0, 99.0),
                        ]),
                    },
                    AnimationChannel {
                        target_node: 1,
                        property: AnimatedProperty::Translation,
                        interpolation: Interpolation::Linear,
                        inputs: vec![0.0, 1.0],
                        outputs: AnimationOutputs::Translations(vec![
                            Vector3::new(0.0, 0.0, 0.0),
                            Vector3::new(5.0, 5.0, 5.0),
                        ]),
                    },
                ],
                markers: vec![],
            },
        ];

        let mut ctrl = LayeredAnimationController::new();
        ctrl.add_layer("Base", BlendMode::Override, 1.0, None, 0);
        // mask: 只影响 bone 1
        ctrl.add_layer("Masked", BlendMode::Override, 1.0, Some(vec![1]), 1);

        // 关闭循环以确保停在 t=1.0
        for layer in &mut ctrl.layers {
            layer.player.looping = false;
        }
        ctrl.update(1.0, &clips);
        let result = ctrl.evaluate(2, &clips);

        // bone 0：masked 层不影响 → 保持 base 层结果 (10,0,0)
        assert!((result[0].translation.x - 10.0).abs() < 1e-4);
        assert!((result[0].translation.y - 0.0).abs() < 1e-4);
        // bone 1：masked 层完全覆盖 → (5,5,5)
        assert!((result[1].translation.x - 5.0).abs() < 1e-4);
        assert!((result[1].translation.y - 5.0).abs() < 1e-4);
    }

    #[test]
    fn test_layer_management() {
        let mut ctrl = LayeredAnimationController::new();
        ctrl.add_layer("Base", BlendMode::Override, 1.0, None, 0);
        ctrl.add_layer("IK", BlendMode::Additive, 0.8, None, 0);

        assert_eq!(ctrl.layers.len(), 2);
        assert!(ctrl.get_layer("IK").is_some());

        ctrl.set_layer_weight("IK", 0.5);
        assert!((ctrl.get_layer("IK").unwrap().weight - 0.5).abs() < f32::EPSILON);

        ctrl.remove_layer("IK");
        assert_eq!(ctrl.layers.len(), 1);
        assert!(ctrl.get_layer("IK").is_none());
    }

    #[test]
    fn test_zero_weight_no_effect() {
        let clips = vec![
            make_translation_clip("base", 0, Vector3::new(0.0, 0.0, 0.0), Vector3::new(10.0, 0.0, 0.0), 1.0),
            make_translation_clip("override", 0, Vector3::new(0.0, 0.0, 0.0), Vector3::new(0.0, 99.0, 0.0), 1.0),
        ];

        let mut ctrl = LayeredAnimationController::new();
        ctrl.add_layer("Base", BlendMode::Override, 1.0, None, 0);
        // 权重为 0 的 Override 层不应影响结果
        ctrl.add_layer("ZeroWeight", BlendMode::Override, 0.0, None, 1);

        ctrl.update(0.5, &clips);
        let result = ctrl.evaluate(1, &clips);

        // Base at t=0.5: (5,0,0); zero-weight 层不影响
        assert!((result[0].translation.x - 5.0).abs() < 1e-4);
        assert!((result[0].translation.y - 0.0).abs() < 1e-4);
    }
}
