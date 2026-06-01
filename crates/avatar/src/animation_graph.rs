use std::collections::HashMap;

use crate::animation::{AnimationClip, AnimationPlayer};

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
    Bool(String, bool),
}

#[derive(Clone, Debug)]
pub struct Transition {
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

pub struct AnimationStateMachine {
    states: HashMap<String, AnimationState>,
    transitions: HashMap<String, Vec<Transition>>,
    current_state: String,
    current_state_time: f32,
    transition_target: Option<String>,
    transition_progress: f32,
    transition_duration: f32,
    parameters: HashMap<String, Parameter>,
    players: HashMap<String, HashMap<usize, AnimationPlayer>>,
}

impl AnimationStateMachine {
    pub fn new(initial_state: String) -> Self {
        let mut players = HashMap::new();
        players.insert(initial_state.clone(), HashMap::new());
        Self {
            states: HashMap::new(),
            transitions: HashMap::new(),
            current_state: initial_state,
            current_state_time: 0.0,
            transition_target: None,
            transition_progress: 0.0,
            transition_duration: 0.0,
            parameters: HashMap::new(),
            players,
        }
    }

    pub fn add_state(&mut self, state: AnimationState) {
        let mut state_players = HashMap::new();
        for clip in state.tree.clips() {
            state_players
                .entry(clip)
                .or_insert_with(|| AnimationPlayer::new(clip));
        }
        self.players.insert(state.name.clone(), state_players);
        self.states.insert(state.name.clone(), state);
    }

    pub fn add_transition(&mut self, from: &str, transition: Transition) {
        self.transitions
            .entry(from.to_string())
            .or_default()
            .push(transition);
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
        &self.current_state
    }

    pub fn current_state_time(&self) -> f32 {
        self.current_state_time
    }

    pub fn is_transitioning(&self) -> bool {
        self.transition_target.is_some()
    }

    pub fn update(&mut self, dt: f32, clips: &[AnimationClip]) -> Vec<ActiveAnimation> {
        self.current_state_time += dt;

        // 驱动当前状态
        if let Some(state) = self.states.get(&self.current_state) {
            if let Some(players) = self.players.get_mut(&self.current_state) {
                for (clip_idx, player) in players.iter_mut() {
                    if let Some(clip) = clips.get(*clip_idx) {
                        player.advance(dt * state.speed, clip.duration);
                    }
                }
            }
        }

        // 检查 transition
        if self.transition_target.is_none() {
            if let Some(transitions) = self.transitions.get(&self.current_state) {
                for transition in transitions {
                    if Self::check_condition(&transition.condition, &self.parameters) {
                        self.transition_target = Some(transition.target_state.clone());
                        self.transition_duration = transition.duration;
                        self.transition_progress = 0.0;

                        if let TransitionCondition::Trigger(ref name) = transition.condition {
                            self.parameters.remove(name);
                        }
                        break;
                    }
                }
            }
        }

        let mut result = Vec::new();

        if let Some(ref target_name) = self.transition_target {
            // 驱动目标状态
            if let Some(target_state) = self.states.get(target_name) {
                if let Some(players) = self.players.get_mut(target_name) {
                    for (clip_idx, player) in players.iter_mut() {
                        if let Some(clip) = clips.get(*clip_idx) {
                            player.advance(dt * target_state.speed, clip.duration);
                        }
                    }
                }
            }

            self.transition_progress += dt / self.transition_duration.max(f32::EPSILON);
            let t = self.transition_progress.clamp(0.0, 1.0);
            let from_weight = 1.0 - t;
            let to_weight = t;

            // 当前状态 evaluate
            if let Some(state) = self.states.get(&self.current_state) {
                if let Some(players) = self.players.get(&self.current_state) {
                    for (clip, local_weight) in state.tree.evaluate(&self.parameters) {
                        if let Some(player) = players.get(&clip) {
                            result.push(ActiveAnimation {
                                clip,
                                weight: local_weight * from_weight,
                                time: player.time,
                            });
                        }
                    }
                }
            }

            // 目标状态 evaluate
            if let Some(state) = self.states.get(target_name) {
                if let Some(players) = self.players.get(target_name) {
                    for (clip, local_weight) in state.tree.evaluate(&self.parameters) {
                        if let Some(player) = players.get(&clip) {
                            result.push(ActiveAnimation {
                                clip,
                                weight: local_weight * to_weight,
                                time: player.time,
                            });
                        }
                    }
                }
            }

            if self.transition_progress >= 1.0 {
                self.current_state = target_name.clone();
                self.current_state_time = 0.0;
                self.transition_target = None;
                self.transition_progress = 0.0;
                self.transition_duration = 0.0;
            }
        } else {
            if let Some(state) = self.states.get(&self.current_state) {
                if let Some(players) = self.players.get(&self.current_state) {
                    for (clip, local_weight) in state.tree.evaluate(&self.parameters) {
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
            TransitionCondition::Bool(name, expected) => params
                .get(name)
                .is_some_and(|p| matches!(p, Parameter::Bool(v) if *v == *expected)),
        }
    }
}
