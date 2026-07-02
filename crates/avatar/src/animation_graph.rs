use std::collections::HashMap;

use crate::animation::{AnimationClip, AnimationPlayer};

/// 状态 ID——`AnimationStateMachine` 内部使用 `usize` 索引代替 `String` 查找，
/// 避免每帧 hash + 字符串比较的开销。
pub type StateId = usize;

#[derive(Clone, Debug)]
pub enum Parameter {
    Float(f32),
    Bool(bool),
    Trigger,
}

#[derive(Clone, Debug)]
pub struct Blend1DEntry {
    pub threshold: f32,
    pub clip: usize,
}

#[derive(Clone, Debug)]
pub enum BlendTree {
    Single(usize),
    Blend1D {
        parameter: String,
        entries: Vec<Blend1DEntry>,
    },
}

impl BlendTree {
    pub fn clips(&self) -> Vec<usize> {
        match self {
            BlendTree::Single(c) => vec![*c],
            BlendTree::Blend1D { entries, .. } => entries.iter().map(|e| e.clip).collect(),
        }
    }

    pub fn evaluate(&self, params: &HashMap<String, Parameter>) -> Vec<(usize, f32)> {
        match self {
            BlendTree::Single(clip) => vec![(*clip, 1.0)],
            BlendTree::Blend1D { parameter, entries } => {
                let value = match params.get(parameter) {
                    Some(Parameter::Float(v)) => *v,
                    _ => 0.0,
                };
                if entries.is_empty() {
                    return Vec::new();
                }
                if entries.len() == 1 || value <= entries[0].threshold {
                    return vec![(entries[0].clip, 1.0)];
                }
                if value >= entries.last().unwrap().threshold {
                    return vec![(entries.last().unwrap().clip, 1.0)];
                }
                for i in 0..entries.len() - 1 {
                    let a = &entries[i];
                    let b = &entries[i + 1];
                    if value >= a.threshold && value <= b.threshold {
                        let t = if b.threshold > a.threshold {
                            (value - a.threshold) / (b.threshold - a.threshold)
                        } else {
                            0.0
                        };
                        return vec![(a.clip, 1.0 - t), (b.clip, t)];
                    }
                }
                vec![(entries.last().unwrap().clip, 1.0)]
            }
        }
    }
}

#[derive(Clone, Debug)]
pub struct AnimationState {
    pub name: String,
    pub tree: BlendTree,
    pub speed: f32,
}

#[derive(Clone, Debug)]
pub enum TransitionCondition {
    Always,
    Trigger(String),
    FloatGreater(String, f32),
    FloatLess(String, f32),
    FloatInRange(String, f32, f32),
    Bool(String, bool),
}

#[derive(Clone, Debug)]
pub struct Transition {
    /// 目标状态名称（公共 API 保持字符串，内部解析为 `StateId`）。
    pub target_state: String,
    pub duration: f32,
    pub condition: TransitionCondition,
}

#[derive(Clone, Debug)]
pub struct ActiveAnimation {
    pub clip: usize,
    pub weight: f32,
    pub time: f32,
}

/// 动画系统内部事件。
#[derive(Clone, Debug)]
pub enum AnimationEvent {
    /// 状态机完成状态切换。
    StateChanged { from: String, to: String },
    /// 循环动画完成一次循环（时间回绕）。
    AnimationLoop { clip_index: usize },
    /// 非循环动画播放结束。
    AnimationEnd { clip_index: usize },
}

/// 内部使用的已解析 transition（`target_state` 已转为 `StateId`）。
#[derive(Clone, Debug)]
struct ResolvedTransition {
    target_state_id: StateId,
    duration: f32,
    condition: TransitionCondition,
}

/// 动画状态机。
///
/// 内部使用 `StateId`（`usize`）索引代替 `String` 查找，
/// 将 per-frame 的 HashMap hash 操作替换为 Vec 直接索引。
pub struct AnimationStateMachine {
    /// ID → name 映射
    state_names: Vec<String>,
    /// name → ID 快速查找
    state_map: HashMap<String, StateId>,
    /// ID → state（使用 `Option` 以支持稀疏占位）
    states: Vec<Option<AnimationState>>,
    /// ID → 已解析 transitions
    transitions: Vec<Vec<ResolvedTransition>>,
    current_state: StateId,
    current_state_time: f32,
    transition_target: Option<StateId>,
    transition_progress: f32,
    transition_duration: f32,
    parameters: HashMap<String, Parameter>,
    /// ID → per-state animation players
    players: Vec<HashMap<usize, AnimationPlayer>>,
    events: Vec<AnimationEvent>,
    /// 每个 player 的上一次时间（用于检测循环回绕）。
    player_prev_times: HashMap<usize, f32>,
}

impl AnimationStateMachine {
    pub fn new(initial_state: String) -> Self {
        Self {
            state_names: vec![initial_state.clone()],
            state_map: {
                let mut m = HashMap::new();
                m.insert(initial_state, 0);
                m
            },
            states: vec![None],
            transitions: vec![Vec::new()],
            current_state: 0,
            current_state_time: 0.0,
            transition_target: None,
            transition_progress: 0.0,
            transition_duration: 0.0,
            parameters: HashMap::new(),
            players: vec![HashMap::new()],
            events: Vec::new(),
            player_prev_times: HashMap::new(),
        }
    }

    /// 解析状态名称到 `StateId`。
    ///
    /// 如果状态尚未通过 `add_state` 注册（例如构造函数传入的初始状态名），
    /// 会自动为其分配 ID（`state` 槽位为 `None`，待 `add_state` 填充）。
    fn resolve_state_id(&mut self, name: &str) -> StateId {
        if let Some(&id) = self.state_map.get(name) {
            return id;
        }
        let id = self.state_names.len();
        self.state_names.push(name.to_string());
        self.state_map.insert(name.to_string(), id);
        self.states.push(None);
        self.transitions.push(Vec::new());
        self.players.push(HashMap::new());
        id
    }

    pub fn add_state(&mut self, state: AnimationState) {
        let id = self.resolve_state_id(&state.name);

        let mut state_players = HashMap::new();
        for clip in state.tree.clips() {
            state_players
                .entry(clip)
                .or_insert_with(|| AnimationPlayer::new(clip));
        }
        self.players[id] = state_players;
        self.states[id] = Some(state);
    }

    pub fn add_transition(&mut self, from: &str, transition: Transition) {
        let from_id = self.resolve_state_id(from);
        let target_id = self.resolve_state_id(&transition.target_state);
        self.transitions[from_id].push(ResolvedTransition {
            target_state_id: target_id,
            duration: transition.duration,
            condition: transition.condition,
        });
    }

    pub fn set_parameter(&mut self, name: &str, param: Parameter) {
        self.parameters.insert(name.to_string(), param);
    }

    pub fn set_float(&mut self, name: &str, value: f32) {
        self.parameters
            .insert(name.to_string(), Parameter::Float(value));
    }

    pub fn set_bool(&mut self, name: &str, value: bool) {
        self.parameters
            .insert(name.to_string(), Parameter::Bool(value));
    }

    pub fn trigger(&mut self, name: &str) {
        self.parameters
            .insert(name.to_string(), Parameter::Trigger);
    }

    pub fn current_state(&self) -> &str {
        &self.state_names[self.current_state]
    }

    pub fn current_state_time(&self) -> f32 {
        self.current_state_time
    }

    pub fn is_transitioning(&self) -> bool {
        self.transition_target.is_some()
    }

    /// 消费并返回本帧积累的动画事件。
    pub fn drain_events(&mut self) -> Vec<AnimationEvent> {
        std::mem::take(&mut self.events)
    }

    pub fn update(&mut self, dt: f32, clips: &[AnimationClip]) -> Vec<ActiveAnimation> {
        self.current_state_time += dt;

        // 记录当前状态用于后面检测切换完成
        let state_before = self.current_state;
        let was_transitioning = self.transition_target.is_some();

        // 驱动当前状态
        if let Some(Some(state)) = self.states.get(self.current_state) {
            let speed = state.speed;
            let players = &mut self.players[self.current_state];
            for (clip_idx, player) in players.iter_mut() {
                if let Some(clip) = clips.get(*clip_idx) {
                    let prev = self.player_prev_times.get(clip_idx).copied().unwrap_or(player.time);
                    player.advance(dt * speed, clip.duration);
                    // 检测循环回绕
                    if player.time < prev {
                        self.events.push(AnimationEvent::AnimationLoop { clip_index: *clip_idx });
                    }
                    // 检测动画结束
                    if player.just_ended {
                        self.events.push(AnimationEvent::AnimationEnd { clip_index: *clip_idx });
                    }
                    self.player_prev_times.insert(*clip_idx, player.time);
                }
            }
        }

        // 检查 transition
        if self.transition_target.is_none() {
            let transitions = &self.transitions[self.current_state];
            for transition in transitions {
                if Self::check_condition(&transition.condition, &self.parameters) {
                    self.transition_target = Some(transition.target_state_id);
                    self.transition_duration = transition.duration;
                    self.transition_progress = 0.0;

                    if let TransitionCondition::Trigger(ref name) = transition.condition {
                        self.parameters.remove(name);
                    }
                    break;
                }
            }
        }

        let mut result = Vec::new();

        if let Some(target_id) = self.transition_target {
            // 驱动目标状态
            if let Some(Some(target_state)) = self.states.get(target_id) {
                let speed = target_state.speed;
                let players = &mut self.players[target_id];
                for (clip_idx, player) in players.iter_mut() {
                    if let Some(clip) = clips.get(*clip_idx) {
                        let prev = self.player_prev_times.get(clip_idx).copied().unwrap_or(player.time);
                        player.advance(dt * speed, clip.duration);
                        if player.time < prev {
                            self.events.push(AnimationEvent::AnimationLoop { clip_index: *clip_idx });
                        }
                        if player.just_ended {
                            self.events.push(AnimationEvent::AnimationEnd { clip_index: *clip_idx });
                        }
                        self.player_prev_times.insert(*clip_idx, player.time);
                    }
                }
            }

            self.transition_progress += dt / self.transition_duration.max(f32::EPSILON);
            let t = self.transition_progress.clamp(0.0, 1.0);
            let from_weight = 1.0 - t;
            let to_weight = t;

            // 当前状态 evaluate
            if let Some(Some(state)) = self.states.get(self.current_state) {
                let evaluated = state.tree.evaluate(&self.parameters);
                let players = &self.players[self.current_state];
                for (clip, local_weight) in evaluated {
                    if let Some(player) = players.get(&clip) {
                        result.push(ActiveAnimation {
                            clip,
                            weight: local_weight * from_weight,
                            time: player.time,
                        });
                    }
                }
            }

            // 目标状态 evaluate
            if let Some(Some(state)) = self.states.get(target_id) {
                let evaluated = state.tree.evaluate(&self.parameters);
                let players = &self.players[target_id];
                for (clip, local_weight) in evaluated {
                    if let Some(player) = players.get(&clip) {
                        result.push(ActiveAnimation {
                            clip,
                            weight: local_weight * to_weight,
                            time: player.time,
                        });
                    }
                }
            }

            if self.transition_progress >= 1.0 {
                self.current_state = target_id;
                self.current_state_time = 0.0;
                self.transition_target = None;
                self.transition_progress = 0.0;
                self.transition_duration = 0.0;
            }
        } else {
            if let Some(Some(state)) = self.states.get(self.current_state) {
                let evaluated = state.tree.evaluate(&self.parameters);
                let players = &self.players[self.current_state];
                for (clip, local_weight) in evaluated {
                    if let Some(player) = players.get(&clip) {
                        result.push(ActiveAnimation {
                            clip,
                            weight: local_weight,
                            time: player.time,
                        });
                    }
                }
            }
        }

        // 检测状态切换完成
        if was_transitioning && self.transition_target.is_none() && self.current_state != state_before {
            self.events.push(AnimationEvent::StateChanged {
                from: self.state_names[state_before].clone(),
                to: self.state_names[self.current_state].clone(),
            });
        }

        // 合并相同 clip
        let mut merged: HashMap<usize, (f32, f32)> = HashMap::new();
        for anim in result {
            let entry = merged.entry(anim.clip).or_insert((0.0, 0.0));
            entry.0 += anim.weight;
            entry.1 += anim.time * anim.weight;
        }

        merged
            .into_iter()
            .filter(|(_, (w, _))| *w > 0.0)
            .map(|(clip, (w, wt))| ActiveAnimation {
                clip,
                weight: w,
                time: wt / w,
            })
            .collect()
    }

    fn check_condition(
        condition: &TransitionCondition,
        params: &HashMap<String, Parameter>,
    ) -> bool {
        match condition {
            TransitionCondition::Always => true,
            TransitionCondition::Trigger(name) => {
                params.get(name).is_some_and(|p| matches!(p, Parameter::Trigger))
            }
            TransitionCondition::FloatGreater(name, value) => params
                .get(name)
                .is_some_and(|p| matches!(p, Parameter::Float(v) if *v > *value)),
            TransitionCondition::FloatLess(name, value) => params
                .get(name)
                .is_some_and(|p| matches!(p, Parameter::Float(v) if *v < *value)),
            TransitionCondition::FloatInRange(name, min, max) => params
                .get(name)
                .is_some_and(|p| matches!(p, Parameter::Float(v) if *v >= *min && *v <= *max)),
            TransitionCondition::Bool(name, expected) => params
                .get(name)
                .is_some_and(|p| matches!(p, Parameter::Bool(v) if *v == *expected)),
        }
    }
}
