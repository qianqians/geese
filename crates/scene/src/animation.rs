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

#[derive(Clone, Debug)]
pub struct AnimationClip {
    pub name: Option<String>,
    pub duration: f32,
    pub channels: Vec<AnimationChannel>,
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

pub fn sample_clip(clip: &AnimationClip, time: f32, nodes: &mut [SceneNode]) {
    for node in nodes.iter_mut() {
        node.local_transform = node.base_transform;
    }

    for channel in &clip.channels {
        if channel.inputs.is_empty() || channel.target_node >= nodes.len() {
            continue;
        }

        let (left, right, factor) = sample_indices(&channel.inputs, time, channel.interpolation);
        let transform = &mut nodes[channel.target_node].local_transform;

        match (&channel.property, &channel.outputs) {
            (AnimatedProperty::Translation, AnimationOutputs::Translations(values)) => {
                transform.translation =
                    sample_vec3(values, left, right, factor, channel.interpolation);
            }
            (AnimatedProperty::Rotation, AnimationOutputs::Rotations(values)) => {
                transform.rotation =
                    sample_quat(values, left, right, factor, channel.interpolation);
            }
            (AnimatedProperty::Scale, AnimationOutputs::Scales(values)) => {
                transform.scale = sample_vec3(values, left, right, factor, channel.interpolation);
            }
            _ => {}
        }
    }
}

fn sample_indices(inputs: &[f32], time: f32, interpolation: Interpolation) -> (usize, usize, f32) {
    if inputs.len() == 1 || time <= inputs[0] {
        return (0, 0, 0.0);
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
            return (i, i + 1, factor);
        }
    }

    let last = inputs.len() - 1;
    (last, last, 0.0)
}

fn output_index(index: usize, interpolation: Interpolation) -> usize {
    match interpolation {
        Interpolation::CubicSpline => index * 3 + 1,
        _ => index,
    }
}

fn sample_vec3(
    values: &[Vector3<f32>],
    left: usize,
    right: usize,
    factor: f32,
    interpolation: Interpolation,
) -> Vector3<f32> {
    let a = values[output_index(left, interpolation)];
    let b = values[output_index(right, interpolation)];
    a + (b - a) * factor
}

fn sample_quat(
    values: &[Quaternion<f32>],
    left: usize,
    right: usize,
    factor: f32,
    interpolation: Interpolation,
) -> Quaternion<f32> {
    let a = values[output_index(left, interpolation)];
    let b = values[output_index(right, interpolation)];
    if interpolation == Interpolation::Step {
        a
    } else if a.v.magnitude2() <= f32::EPSILON || b.v.magnitude2() <= f32::EPSILON {
        a
    } else {
        a.slerp(b, factor)
    }
}
