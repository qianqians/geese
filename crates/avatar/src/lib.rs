pub mod animation;
pub mod animation_graph;

pub use animation::{
    AnimatedProperty, AnimationChannel, AnimationClip, AnimationOutputs, AnimationPlayer,
    Interpolation, SceneNode, Skin, Transform,
    quat_dot, quat_exp, quat_log, sample_clip, sample_indices, sample_quat, sample_vec3,
};
pub use animation_graph::{
    ActiveAnimation, AnimationState, AnimationStateMachine, Blend1DEntry, BlendTree, Parameter,
    Transition, TransitionCondition,
};
