//! 角色动画蓝图。
//!
//! [`CharacterAnimationGraph`] 封装速度驱动的动画状态机，
//! 根据角色移动速度和着地状态在 Idle/Walk/Run/Jump/Fall 之间切换。

use avatar::{
    ActiveAnimation, AnimationClip, AnimationState, AnimationStateMachine,
    BlendTree, Transition, TransitionCondition,
};

/// 预设的角色动画状态名称。
pub mod state_names {
    pub const IDLE: &str = "Idle";
    pub const WALK: &str = "Walk";
    pub const RUN: &str = "Run";
    pub const JUMP: &str = "Jump";
    pub const FALL: &str = "Fall";
}

/// 速度阈值配置。
#[derive(Debug, Clone)]
pub struct SpeedThresholds {
    /// 速度低于此值进入 Idle
    pub idle_max: f32,
    /// 速度在此区间为 Walk
    pub walk_max: f32,
    /// 超过此值为 Run
    pub run_min: f32,
}

impl Default for SpeedThresholds {
    fn default() -> Self {
        Self {
            idle_max: 0.5,
            walk_max: 2.5,
            run_min: 2.5,
        }
    }
}

/// 角色动画蓝图：封装速度驱动的状态机。
pub struct CharacterAnimationGraph {
    /// 动画状态机
    pub machine: AnimationStateMachine,
    /// 速度参数名
    velocity_param: String,
    /// 着地参数名
    grounded_param: String,
}

impl CharacterAnimationGraph {
    /// 创建新的角色动画蓝图。
    ///
    /// # Arguments
    /// * `idle_clip` - Idle 动画 clip 索引
    /// * `walk_clip` - Walk 动画 clip 索引
    /// * `run_clip` - Run 动画 clip 索引
    /// * `jump_clip` - Jump 动画 clip 索引（可选）
    /// * `fall_clip` - Fall 动画 clip 索引（可选）
    /// * `thresholds` - 速度阈值
    pub fn new(
        idle_clip: usize,
        walk_clip: usize,
        run_clip: usize,
        jump_clip: Option<usize>,
        fall_clip: Option<usize>,
        thresholds: SpeedThresholds,
    ) -> Self {
        let velocity_param = "physics_velocity".to_string();
        let grounded_param = "physics_grounded".to_string();

        let mut machine = AnimationStateMachine::new(state_names::IDLE.to_string());

        // 设置速度参数（初始为 0）
        machine.set_float(&velocity_param, 0.0);
        machine.set_bool(&grounded_param, true);

        // --- Idle 状态 ---
        let idle_state = AnimationState {
            name: state_names::IDLE.to_string(),
            tree: BlendTree::Single(idle_clip),
            speed: 1.0,
        };
        machine.add_state(idle_state);

        // --- Walk 状态 ---
        let walk_state = AnimationState {
            name: state_names::WALK.to_string(),
            tree: BlendTree::Single(walk_clip),
            speed: 1.0,
        };
        machine.add_state(walk_state);

        // --- Run 状态 ---
        let run_state = AnimationState {
            name: state_names::RUN.to_string(),
            tree: BlendTree::Single(run_clip),
            speed: 1.0,
        };
        machine.add_state(run_state);

        // --- Idle → Walk: 速度在 walk 区间 ---
        machine.add_transition(
            state_names::IDLE,
            Transition {
                target_state: state_names::WALK.to_string(),
                duration: 0.2,
                condition: TransitionCondition::FloatInRange(
                    velocity_param.clone(),
                    thresholds.idle_max,
                    thresholds.walk_max,
                ),
            },
        );

        // --- Idle → Run: 速度在 run 区间 ---
        machine.add_transition(
            state_names::IDLE,
            Transition {
                target_state: state_names::RUN.to_string(),
                duration: 0.2,
                condition: TransitionCondition::FloatGreater(
                    velocity_param.clone(),
                    thresholds.run_min,
                ),
            },
        );

        // --- Walk → Idle: 速度低于 idle 阈值 ---
        machine.add_transition(
            state_names::WALK,
            Transition {
                target_state: state_names::IDLE.to_string(),
                duration: 0.3,
                condition: TransitionCondition::FloatLess(
                    velocity_param.clone(),
                    thresholds.idle_max,
                ),
            },
        );

        // --- Walk → Run: 速度进入 run 区间 ---
        machine.add_transition(
            state_names::WALK,
            Transition {
                target_state: state_names::RUN.to_string(),
                duration: 0.2,
                condition: TransitionCondition::FloatGreater(
                    velocity_param.clone(),
                    thresholds.run_min,
                ),
            },
        );

        // --- Run → Walk: 速度降回 walk 区间 ---
        machine.add_transition(
            state_names::RUN,
            Transition {
                target_state: state_names::WALK.to_string(),
                duration: 0.2,
                condition: TransitionCondition::FloatInRange(
                    velocity_param.clone(),
                    thresholds.idle_max,
                    thresholds.walk_max,
                ),
            },
        );

        // --- Run → Idle: 速度降到 idle ---
        machine.add_transition(
            state_names::RUN,
            Transition {
                target_state: state_names::IDLE.to_string(),
                duration: 0.25,
                condition: TransitionCondition::FloatLess(
                    velocity_param.clone(),
                    thresholds.idle_max,
                ),
            },
        );

        // --- Jump / Fall 状态 ---
        if let Some(jump_clip) = jump_clip {
            let jump_state = AnimationState {
                name: state_names::JUMP.to_string(),
                tree: BlendTree::Single(jump_clip),
                speed: 1.0,
            };
            machine.add_state(jump_state);

            // 脱离地面 + 向上速度 → Jump
            machine.add_transition(
                state_names::IDLE,
                Transition {
                    target_state: state_names::JUMP.to_string(),
                    duration: 0.1,
                    condition: TransitionCondition::Bool(grounded_param.clone(), false),
                },
            );
            machine.add_transition(
                state_names::WALK,
                Transition {
                    target_state: state_names::JUMP.to_string(),
                    duration: 0.1,
                    condition: TransitionCondition::Bool(grounded_param.clone(), false),
                },
            );
            machine.add_transition(
                state_names::RUN,
                Transition {
                    target_state: state_names::JUMP.to_string(),
                    duration: 0.1,
                    condition: TransitionCondition::Bool(grounded_param.clone(), false),
                },
            );

            // 着地 → Idle
            machine.add_transition(
                state_names::JUMP,
                Transition {
                    target_state: state_names::IDLE.to_string(),
                    duration: 0.15,
                    condition: TransitionCondition::Bool(grounded_param.clone(), true),
                },
            );
        }

        if let Some(fall_clip) = fall_clip {
            let fall_state = AnimationState {
                name: state_names::FALL.to_string(),
                tree: BlendTree::Single(fall_clip),
                speed: 1.0,
            };
            machine.add_state(fall_state);

            // Jump → Fall: 待扩展（基于速度 y 分量）
        }

        Self {
            machine,
            velocity_param,
            grounded_param,
        }
    }

    /// 每帧更新：根据角色速度与着地状态驱动状态机。
    ///
    /// # Arguments
    /// * `velocity` - 水平速度大小 (m/s)
    /// * `grounded` - 是否着地
    /// * `dt` - 帧时间
    /// * `clips` - 动画剪辑数组
    ///
    /// 返回当前活跃动画列表，供 Scene::update_animation_graph 使用。
    pub fn update(
        &mut self,
        velocity: f32,
        grounded: bool,
        dt: f32,
        clips: &[AnimationClip],
    ) -> Vec<ActiveAnimation> {
        self.machine.set_float(&self.velocity_param, velocity);
        self.machine.set_bool(&self.grounded_param, grounded);
        self.machine.update(dt, clips)
    }

    /// 获取内部状态机引用。
    pub fn machine(&self) -> &AnimationStateMachine {
        &self.machine
    }

    /// 获取内部状态机可变引用。
    pub fn machine_mut(&mut self) -> &mut AnimationStateMachine {
        &mut self.machine
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use avatar::AnimatedProperty;
    use avatar::AnimationChannel;
    use avatar::AnimationOutputs;
    use avatar::Interpolation;

    fn make_dummy_clip(name: &str, duration: f32) -> AnimationClip {
        AnimationClip {
            name: Some(name.to_string()),
            duration,
            channels: vec![],
        }
    }

    #[test]
    fn test_idle_to_walk_transition() {
        let mut graph = CharacterAnimationGraph::new(
            0, 1, 2, None, None, SpeedThresholds::default(),
        );

        let clips = vec![
            make_dummy_clip("idle", 1.0),
            make_dummy_clip("walk", 1.0),
            make_dummy_clip("run", 1.0),
        ];

        // 初始速度 0 → Idle
        let active = graph.update(0.0, true, 0.1, &clips);
        assert_eq!(graph.machine.current_state(), state_names::IDLE);

        // 速度进入 Walk 区间
        let active = graph.update(1.0, true, 0.3, &clips);
        // 应该正在切换或已切换到 Walk
        let state = graph.machine.current_state();
        assert!(
            state == state_names::WALK || graph.machine.is_transitioning(),
            "expected Walk or transitioning, got {}",
            state
        );
    }

    #[test]
    fn test_grounded_triggers_jump() {
        let mut graph = CharacterAnimationGraph::new(
            0, 1, 2, Some(3), None, SpeedThresholds::default(),
        );

        let clips = vec![
            make_dummy_clip("idle", 1.0),
            make_dummy_clip("walk", 1.0),
            make_dummy_clip("run", 1.0),
            make_dummy_clip("jump", 1.0),
        ];

        graph.update(0.0, true, 0.1, &clips);
        assert_eq!(graph.machine.current_state(), state_names::IDLE);

        graph.update(0.0, false, 0.3, &clips);
        let state = graph.machine.current_state();
        assert!(
            state == state_names::JUMP || graph.machine.is_transitioning(),
            "expected Jump or transitioning, got {}",
            state
        );
    }
}
