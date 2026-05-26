use scene::animation_graph::{
    AnimationState, AnimationStateMachine, Blend1DEntry, BlendTree, Transition,
    TransitionCondition,
};
use scene::AnimationClip;

#[test]
fn test_state_machine_single_state() {
    let mut sm = AnimationStateMachine::new("idle".to_string());
    sm.add_state(AnimationState {
        name: "idle".to_string(),
        tree: BlendTree::Single(0),
        speed: 1.0,
    });

    let clips = vec![AnimationClip {
        name: Some("idle".to_string()),
        duration: 1.0,
        channels: vec![],
    }];

    let active = sm.update(0.5, &clips);
    assert_eq!(active.len(), 1);
    assert_eq!(active[0].clip, 0);
    assert!((active[0].weight - 1.0).abs() < 1e-6);
    assert!((active[0].time - 0.5).abs() < 1e-6);
}

#[test]
fn test_state_machine_transition() {
    let mut sm = AnimationStateMachine::new("idle".to_string());
    sm.add_state(AnimationState {
        name: "idle".to_string(),
        tree: BlendTree::Single(0),
        speed: 1.0,
    });
    sm.add_state(AnimationState {
        name: "walk".to_string(),
        tree: BlendTree::Single(1),
        speed: 1.0,
    });
    sm.add_transition(
        "idle",
        Transition {
            target_state: "walk".to_string(),
            duration: 0.2,
            condition: TransitionCondition::Trigger("move".to_string()),
        },
    );

    let clips = vec![
        AnimationClip {
            name: Some("idle".to_string()),
            duration: 1.0,
            channels: vec![],
        },
        AnimationClip {
            name: Some("walk".to_string()),
            duration: 2.0,
            channels: vec![],
        },
    ];

    // 初始状态
    let active = sm.update(0.1, &clips);
    assert_eq!(active.len(), 1);
    assert_eq!(active[0].clip, 0);

    // 触发过渡
    sm.trigger("move");
    let active = sm.update(0.05, &clips);
    assert_eq!(active.len(), 2);
    let idle = active.iter().find(|a| a.clip == 0).unwrap();
    let walk = active.iter().find(|a| a.clip == 1).unwrap();
    assert!(idle.weight > walk.weight);

    // 过渡完成
    let active = sm.update(0.2, &clips);
    assert_eq!(active.len(), 1);
    assert_eq!(active[0].clip, 1);
    assert_eq!(sm.current_state(), "walk");
}

#[test]
fn test_blend1d() {
    let mut sm = AnimationStateMachine::new("move".to_string());
    sm.add_state(AnimationState {
        name: "move".to_string(),
        tree: BlendTree::Blend1D {
            parameter: "speed".to_string(),
            entries: vec![
                Blend1DEntry {
                    threshold: 0.0,
                    clip: 0,
                },
                Blend1DEntry {
                    threshold: 5.0,
                    clip: 1,
                },
            ],
        },
        speed: 1.0,
    });

    let clips = vec![
        AnimationClip {
            name: Some("idle".to_string()),
            duration: 1.0,
            channels: vec![],
        },
        AnimationClip {
            name: Some("run".to_string()),
            duration: 1.0,
            channels: vec![],
        },
    ];

    sm.set_float("speed", 0.0);
    let active = sm.update(0.1, &clips);
    assert_eq!(active.len(), 1);
    assert_eq!(active[0].clip, 0);

    sm.set_float("speed", 5.0);
    let active = sm.update(0.1, &clips);
    assert_eq!(active.len(), 1);
    assert_eq!(active[0].clip, 1);

    sm.set_float("speed", 2.5);
    let active = sm.update(0.1, &clips);
    assert_eq!(active.len(), 2);
    let a0 = active.iter().find(|a| a.clip == 0).unwrap();
    let a1 = active.iter().find(|a| a.clip == 1).unwrap();
    assert!((a0.weight - 0.5).abs() < 1e-6);
    assert!((a1.weight - 0.5).abs() < 1e-6);
}

#[test]
fn test_transition_conditions() {
    let mut sm = AnimationStateMachine::new("idle".to_string());
    sm.add_state(AnimationState {
        name: "idle".to_string(),
        tree: BlendTree::Single(0),
        speed: 1.0,
    });
    sm.add_state(AnimationState {
        name: "run".to_string(),
        tree: BlendTree::Single(1),
        speed: 1.0,
    });
    sm.add_state(AnimationState {
        name: "crouch".to_string(),
        tree: BlendTree::Single(2),
        speed: 1.0,
    });
    sm.add_transition(
        "idle",
        Transition {
            target_state: "run".to_string(),
            duration: 0.1,
            condition: TransitionCondition::FloatGreater("speed".to_string(), 0.5),
        },
    );
    sm.add_transition(
        "idle",
        Transition {
            target_state: "crouch".to_string(),
            duration: 0.1,
            condition: TransitionCondition::Bool("crouching".to_string(), true),
        },
    );

    let clips = vec![
        AnimationClip {
            name: Some("idle".to_string()),
            duration: 1.0,
            channels: vec![],
        },
        AnimationClip {
            name: Some("run".to_string()),
            duration: 1.0,
            channels: vec![],
        },
        AnimationClip {
            name: Some("crouch".to_string()),
            duration: 1.0,
            channels: vec![],
        },
    ];

    // FloatGreater 条件
    sm.set_float("speed", 1.0);
    let active = sm.update(0.2, &clips);
    assert_eq!(sm.current_state(), "run");
    assert_eq!(active[0].clip, 1);

    // 回到 idle，然后 Bool 条件
    sm = AnimationStateMachine::new("idle".to_string());
    sm.add_state(AnimationState {
        name: "idle".to_string(),
        tree: BlendTree::Single(0),
        speed: 1.0,
    });
    sm.add_state(AnimationState {
        name: "crouch".to_string(),
        tree: BlendTree::Single(2),
        speed: 1.0,
    });
    sm.add_transition(
        "idle",
        Transition {
            target_state: "crouch".to_string(),
            duration: 0.1,
            condition: TransitionCondition::Bool("crouching".to_string(), true),
        },
    );

    sm.set_bool("crouching", true);
    let active = sm.update(0.2, &clips);
    assert_eq!(sm.current_state(), "crouch");
    assert_eq!(active[0].clip, 2);
}
