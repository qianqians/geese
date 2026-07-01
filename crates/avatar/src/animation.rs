use cgmath::{InnerSpace, Matrix4, Quaternion, Vector3};

#[derive(Clone, Copy, Debug)]
pub struct Transform {
    pub translation: Vector3<f32>,
    pub rotation: Quaternion<f32>,
    pub scale: Vector3<f32>,
}

impl Transform {
    pub fn from_gltf(translation: [f32; 3], rotation: [f32; 4], scale: [f32; 3]) -> Self {
        Self {
            translation: Vector3::new(translation[0], translation[1], translation[2]),
            rotation: Quaternion::new(rotation[3], rotation[0], rotation[1], rotation[2]),
            scale: Vector3::new(scale[0], scale[1], scale[2]),
        }
    }

    pub fn matrix(&self) -> Matrix4<f32> {
        Matrix4::from_translation(self.translation)
            * Matrix4::from(self.rotation)
            * Matrix4::from_nonuniform_scale(self.scale.x, self.scale.y, self.scale.z)
    }
}

impl Default for Transform {
    fn default() -> Self {
        Self {
            translation: Vector3::new(0.0, 0.0, 0.0),
            rotation: Quaternion::new(1.0, 0.0, 0.0, 0.0),
            scale: Vector3::new(1.0, 1.0, 1.0),
        }
    }
}

#[derive(Clone, Debug)]
pub struct SceneNode {
    pub id: usize,
    pub parent: Option<usize>,
    pub children: Vec<usize>,
    pub objects: Vec<usize>,
    pub base_transform: Transform,
    pub local_transform: Transform,
    pub world_transform: Matrix4<f32>,
}

#[derive(Clone, Debug)]
pub struct Skin {
    pub joints: Vec<usize>,
    pub inverse_bind_matrices: Vec<Matrix4<f32>>,
}

impl SceneNode {
    pub fn new(id: usize, parent: Option<usize>, transform: Transform) -> Self {
        Self {
            id,
            parent,
            children: Vec::new(),
            objects: Vec::new(),
            base_transform: transform,
            local_transform: transform,
            world_transform: Matrix4::from_scale(1.0),
        }
    }
}

/// 动画时间轴上的命名标记点。
/// 当动画播放跨越此时间点时，触发同名事件。
#[derive(Clone, Debug)]
pub struct AnimationMarker {
    pub time: f32,
    pub name: String,
}

#[derive(Clone, Debug)]
pub struct AnimationClip {
    pub name: Option<String>,
    pub duration: f32,
    pub channels: Vec<AnimationChannel>,
    pub markers: Vec<AnimationMarker>,
}

#[derive(Clone, Debug)]
pub struct AnimationChannel {
    pub target_node: usize,
    pub property: AnimatedProperty,
    pub interpolation: Interpolation,
    pub inputs: Vec<f32>,
    pub outputs: AnimationOutputs,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AnimatedProperty {
    Translation,
    Rotation,
    Scale,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Interpolation {
    Linear,
    Step,
    CubicSpline,
}

#[derive(Clone, Debug)]
pub enum AnimationOutputs {
    Translations(Vec<Vector3<f32>>),
    Rotations(Vec<Quaternion<f32>>),
    Scales(Vec<Vector3<f32>>),
}

#[derive(Clone, Debug)]
pub struct AnimationPlayer {
    pub clip: usize,
    pub time: f32,
    pub speed: f32,
    pub looping: bool,
    pub playing: bool,
}

impl AnimationPlayer {
    pub fn new(clip: usize) -> Self {
        Self {
            clip,
            time: 0.0,
            speed: 1.0,
            looping: true,
            playing: true,
        }
    }

    pub fn advance(&mut self, dt: f32, duration: f32) {
        if !self.playing {
            return;
        }

        self.time += dt * self.speed;
        if duration <= 0.0 {
            self.time = 0.0;
        } else if self.looping {
            self.time = self.time.rem_euclid(duration);
        } else if self.time > duration {
            self.time = duration;
            self.playing = false;
        }
    }
}

/// 检查两个时间点之间跨越的所有标记（处理循环回绕）。
/// prev_time: 上一帧的时间
/// curr_time: 当前帧的时间
/// duration: 动画总时长
/// 返回被跨越的标记索引列表。
pub fn check_markers_crossed(
    markers: &[AnimationMarker],
    prev_time: f32,
    curr_time: f32,
    duration: f32,
) -> Vec<usize> {
    if markers.is_empty() || duration <= 0.0 {
        return vec![];
    }

    let mut result = Vec::new();

    if curr_time >= prev_time {
        // 正常前进（含循环回绕边界内的前进）
        for (i, m) in markers.iter().enumerate() {
            if m.time > prev_time && m.time <= curr_time {
                result.push(i);
            }
        }
    } else {
        // 循环回绕: prev_time → duration + 0 → curr_time
        for (i, m) in markers.iter().enumerate() {
            if m.time > prev_time || m.time <= curr_time {
                result.push(i);
            }
        }
    }

    result
}

pub fn sample_clip(clip: &AnimationClip, time: f32, nodes: &mut [SceneNode]) {
    for node in nodes.iter_mut() {
        node.local_transform = node.base_transform;
    }

    for channel in &clip.channels {
        if channel.inputs.is_empty() || channel.target_node >= nodes.len() {
            continue;
        }

        let (left, right, factor, interval) = sample_indices(&channel.inputs, time, channel.interpolation);
        let transform = &mut nodes[channel.target_node].local_transform;

        match (&channel.property, &channel.outputs) {
            (AnimatedProperty::Translation, AnimationOutputs::Translations(values)) => {
                transform.translation =
                    sample_vec3(values, left, right, factor, interval, channel.interpolation);
            }
            (AnimatedProperty::Rotation, AnimationOutputs::Rotations(values)) => {
                transform.rotation =
                    sample_quat(values, left, right, factor, interval, channel.interpolation);
            }
            (AnimatedProperty::Scale, AnimationOutputs::Scales(values)) => {
                transform.scale = sample_vec3(values, left, right, factor, interval, channel.interpolation);
            }
            _ => {}
        }
    }
}

pub fn sample_indices(inputs: &[f32], time: f32, interpolation: Interpolation) -> (usize, usize, f32, f32) {
    if inputs.len() == 1 || time <= inputs[0] {
        return (0, 0, 0.0, 0.0);
    }

    for i in 0..inputs.len() - 1 {
        let start = inputs[i];
        let end = inputs[i + 1];
        if time <= end {
            let factor = if interpolation == Interpolation::Step || end <= start {
                0.0
            } else {
                ((time - start) / (end - start)).clamp(0.0, 1.0)
            };
            return (i, i + 1, factor, end - start);
        }
    }

    let last = inputs.len() - 1;
    (last, last, 0.0, 0.0)
}

fn output_index(index: usize, interpolation: Interpolation) -> usize {
    match interpolation {
        Interpolation::CubicSpline => index * 3 + 1,
        _ => index,
    }
}

pub fn sample_vec3(
    values: &[Vector3<f32>],
    left: usize,
    right: usize,
    factor: f32,
    interval: f32,
    interpolation: Interpolation,
) -> Vector3<f32> {
    match interpolation {
        Interpolation::CubicSpline => {
            let p0 = values[left * 3 + 1];
            let m0 = values[left * 3 + 2] * interval;
            let p1 = values[right * 3 + 1];
            let m1 = values[right * 3] * interval;
            let f2 = factor * factor;
            let f3 = f2 * factor;
            p0 * (2.0 * f3 - 3.0 * f2 + 1.0)
                + m0 * (f3 - 2.0 * f2 + factor)
                + p1 * (-2.0 * f3 + 3.0 * f2)
                + m1 * (f3 - f2)
        }
        _ => {
            let a = values[output_index(left, interpolation)];
            let b = values[output_index(right, interpolation)];
            a + (b - a) * factor
        }
    }
}

pub fn sample_quat(
    values: &[Quaternion<f32>],
    left: usize,
    right: usize,
    factor: f32,
    interval: f32,
    interpolation: Interpolation,
) -> Quaternion<f32> {
    match interpolation {
        Interpolation::CubicSpline => {
            let p0 = values[left * 3 + 1];
            let m0 = values[left * 3 + 2];
            let p1 = values[right * 3 + 1];
            let m1 = values[right * 3];

            let p1 = if quat_dot(p0, p1) < 0.0 { -p1 } else { p1 };
            let m0 = m0 * interval;
            let m1 = m1 * interval;

            let f2 = factor * factor;
            let f3 = f2 * factor;

            let result = p0 * (2.0 * f3 - 3.0 * f2 + 1.0)
                + m0 * (f3 - 2.0 * f2 + factor)
                + p1 * (-2.0 * f3 + 3.0 * f2)
                + m1 * (f3 - f2);

            let mag = result.magnitude();
            if mag > f32::EPSILON {
                result / mag
            } else {
                p0
            }
        }
        _ => {
            let a = values[output_index(left, interpolation)];
            let b = values[output_index(right, interpolation)];
            if interpolation == Interpolation::Step {
                a
            } else {
                a.slerp(b, factor)
            }
        }
    }
}

pub fn quat_dot(a: Quaternion<f32>, b: Quaternion<f32>) -> f32 {
    a.s * b.s + a.v.dot(b.v)
}

pub fn quat_log(q: Quaternion<f32>) -> Vector3<f32> {
    let w = q.s.clamp(-1.0, 1.0);
    if w > 0.9999999 {
        Vector3::new(0.0, 0.0, 0.0)
    } else {
        let angle = w.acos() * 2.0;
        let sin_half = (1.0 - w * w).sqrt().max(f32::EPSILON);
        q.v * (angle / sin_half)
    }
}

pub fn quat_exp(v: Vector3<f32>) -> Quaternion<f32> {
    let angle = v.magnitude();
    if angle < 1e-6 {
        Quaternion::new(1.0, 0.0, 0.0, 0.0)
    } else {
        let half = angle * 0.5;
        let sh = half.sin();
        let ch = half.cos();
        let axis = v / angle;
        Quaternion::new(ch, axis.x * sh, axis.y * sh, axis.z * sh)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cgmath::Vector3;

    #[test]
    fn test_animation_player_advance_looping() {
        let mut player = AnimationPlayer::new(0);
        player.advance(1.5, 1.0);
        assert!((player.time - 0.5).abs() < f32::EPSILON);
    }

    #[test]
    fn test_animation_player_advance_non_looping() {
        let mut player = AnimationPlayer::new(0);
        player.looping = false;
        player.advance(1.5, 1.0);
        assert!((player.time - 1.0).abs() < f32::EPSILON);
        assert!(!player.playing);
    }

    #[test]
    fn test_animation_player_advance_speed() {
        let mut player = AnimationPlayer::new(0);
        player.speed = 2.0;
        player.advance(0.5, 1.0);
        // looping defaults to true, so 1.0 wraps to 0.0
        assert!((player.time - 0.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_animation_player_pause() {
        let mut player = AnimationPlayer::new(0);
        player.playing = false;
        player.advance(1.0, 1.0);
        assert!((player.time - 0.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_sample_indices_single() {
        let (l, r, f, i) = sample_indices(&[0.0], 0.5, Interpolation::Linear);
        assert_eq!(l, 0);
        assert_eq!(r, 0);
        assert_eq!(f, 0.0);
        assert_eq!(i, 0.0);
    }

    #[test]
    fn test_sample_indices_boundary() {
        let (l, r, f, i) = sample_indices(&[0.0, 1.0, 2.0], 1.5, Interpolation::Linear);
        assert_eq!(l, 1);
        assert_eq!(r, 2);
        assert!((f - 0.5).abs() < f32::EPSILON);
        assert!((i - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_sample_vec3_linear() {
        let values = vec![Vector3::new(0.0, 0.0, 0.0), Vector3::new(1.0, 2.0, 3.0)];
        let result = sample_vec3(&values, 0, 1, 0.5, 1.0, Interpolation::Linear);
        assert!((result - Vector3::new(0.5, 1.0, 1.5)).magnitude() < 1e-6);
    }

    #[test]
    fn test_sample_vec3_step() {
        let values = vec![Vector3::new(0.0, 0.0, 0.0), Vector3::new(1.0, 2.0, 3.0)];
        let (left, right, factor, interval) = sample_indices(&[0.0, 1.0], 0.5, Interpolation::Step);
        let result = sample_vec3(&values, left, right, factor, interval, Interpolation::Step);
        assert!((result - Vector3::new(0.0, 0.0, 0.0)).magnitude() < 1e-6);
    }

    #[test]
    fn test_sample_vec3_cubic_spline_zero_tangents() {
        let values = vec![
            Vector3::new(0.0, 0.0, 0.0), // in-tangent 0
            Vector3::new(0.0, 0.0, 0.0), // value 0
            Vector3::new(0.0, 0.0, 0.0), // out-tangent 0
            Vector3::new(0.0, 0.0, 0.0), // in-tangent 1
            Vector3::new(1.0, 1.0, 1.0), // value 1
            Vector3::new(0.0, 0.0, 0.0), // out-tangent 1
        ];
        let result = sample_vec3(&values, 0, 1, 0.5, 1.0, Interpolation::CubicSpline);
        assert!((result - Vector3::new(0.5, 0.5, 0.5)).magnitude() < 1e-6);
    }

    #[test]
    fn test_sample_quat_linear() {
        let a = Quaternion::new(1.0, 0.0, 0.0, 0.0);
        let b = Quaternion::new(0.0, 1.0, 0.0, 0.0);
        let values = vec![a, b];
        let result = sample_quat(&values, 0, 1, 0.5, 1.0, Interpolation::Linear);
        let expected = Quaternion::new(0.70710678, 0.70710678, 0.0, 0.0);
        let dot = quat_dot(result, expected);
        assert!((dot - 1.0).abs() < 1e-5, "dot was {}", dot);
    }

    #[test]
    fn test_sample_quat_step() {
        let a = Quaternion::new(1.0, 0.0, 0.0, 0.0);
        let b = Quaternion::new(0.0, 1.0, 0.0, 0.0);
        let values = vec![a, b];
        let result = sample_quat(&values, 0, 1, 0.5, 1.0, Interpolation::Step);
        assert!((quat_dot(result, a) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_sample_quat_cubic_spline_zero_tangents() {
        let a = Quaternion::new(1.0, 0.0, 0.0, 0.0);
        let b = Quaternion::new(0.0, 1.0, 0.0, 0.0);
        let values = vec![
            Quaternion::new(0.0, 0.0, 0.0, 0.0), // in-tangent 0
            a,                                    // value 0
            Quaternion::new(0.0, 0.0, 0.0, 0.0), // out-tangent 0
            Quaternion::new(0.0, 0.0, 0.0, 0.0), // in-tangent 1
            b,                                    // value 1
            Quaternion::new(0.0, 0.0, 0.0, 0.0), // out-tangent 1
        ];
        let result = sample_quat(&values, 0, 1, 0.5, 1.0, Interpolation::CubicSpline);
        let expected = Quaternion::new(0.70710678, 0.70710678, 0.0, 0.0);
        let dot = quat_dot(result, expected);
        assert!((dot - 1.0).abs() < 1e-5, "dot was {}", dot);
    }
}
