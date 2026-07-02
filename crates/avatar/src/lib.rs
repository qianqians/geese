pub mod animation;
pub mod animation_graph;

pub use animation::{
    AnimatedProperty, AnimationChannel, AnimationClip, AnimationMarker, AnimationOutputs,
    AnimationPlayer, Interpolation, SceneNode, Skin, Transform,
    check_markers_crossed, quat_dot, quat_exp, quat_log, sample_clip, sample_indices,
    sample_quat, sample_vec3,
};
pub use animation_graph::{
    ActiveAnimation, AnimationEvent, AnimationState, AnimationStateMachine, Blend1DEntry, BlendTree, Parameter,
    StateId, Transition, TransitionCondition,
};
