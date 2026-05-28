//! 4.10 AOI（Area of Interest）兴趣管理骨架（服务端）。
//!
//! 提供两种实现：
//! - `GridAoi`：九宫格分桶（最经典的 MMO 实现）
//! - `Aoi` trait：上层接口，便于后续切换 Octree/动态格
//!
//! 关键 API：`insert / update / remove / observers(entity)` + 帧末 `take_events()`
//! 拉取 Enter/Leave 事件列表，由 server 派发给 player。

use std::collections::{HashMap, HashSet};

#[cfg(feature = "pyo3")]
pub mod py;

pub type EntityId = u64;

/// 进出兴趣范围的事件。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AoiEvent {
    /// `observer` 开始关注 `target`。
    Enter { observer: EntityId, target: EntityId },
    /// `observer` 停止关注 `target`。
    Leave { observer: EntityId, target: EntityId },
}

/// AOI 通用接口。
pub trait Aoi {
    fn insert(&mut self, id: EntityId, pos: [f32; 3], radius: f32);
    fn update(&mut self, id: EntityId, pos: [f32; 3]);
    fn remove(&mut self, id: EntityId);
    fn observers(&self, target: EntityId) -> Vec<EntityId>;
    fn take_events(&mut self) -> Vec<AoiEvent>;
}

#[derive(Clone, Copy, Debug)]
struct EntityRecord {
    pos: [f32; 3],
    radius: f32,
    cell: (i32, i32),
}

/// 九宫格 AOI：把 XZ 平面划分为固定大小的桶，邻 3×3 = 9 桶为视野范围。
pub struct GridAoi {
    cell_size: f32,
    cells: HashMap<(i32, i32), HashSet<EntityId>>,
    entities: HashMap<EntityId, EntityRecord>,
    /// 上一帧的 (observer -> targets) 关系，用于 diff 出 Enter/Leave。
    last_visible: HashMap<EntityId, HashSet<EntityId>>,
    pending_events: Vec<AoiEvent>,
}

impl GridAoi {
    pub fn new(cell_size: f32) -> Self {
        Self {
            cell_size: cell_size.max(1e-3),
            cells: HashMap::new(),
            entities: HashMap::new(),
            last_visible: HashMap::new(),
            pending_events: Vec::new(),
        }
    }

    fn pos_to_cell(&self, pos: [f32; 3]) -> (i32, i32) {
        ((pos[0] / self.cell_size).floor() as i32, (pos[2] / self.cell_size).floor() as i32)
    }

    /// 查询 entity 当前可见的所有目标（9 桶并集 - 自身）。
    fn compute_visible(&self, id: EntityId) -> HashSet<EntityId> {
        let mut visible = HashSet::new();
        let Some(rec) = self.entities.get(&id) else { return visible; };
        let (cx, cz) = rec.cell;
        // 半径换算为格数（至少 1）
        let cell_radius = ((rec.radius / self.cell_size).ceil() as i32).max(1);
        for dx in -cell_radius..=cell_radius {
            for dz in -cell_radius..=cell_radius {
                if let Some(set) = self.cells.get(&(cx + dx, cz + dz)) {
                    for &other in set {
                        if other != id {
                            visible.insert(other);
                        }
                    }
                }
            }
        }
        visible
    }

    /// 重算 entity 的可见集并 diff 出事件。
    fn recompute_and_diff(&mut self, id: EntityId) {
        let now = self.compute_visible(id);
        let prev = self.last_visible.remove(&id).unwrap_or_default();
        for &t in now.difference(&prev) {
            self.pending_events.push(AoiEvent::Enter { observer: id, target: t });
        }
        for &t in prev.difference(&now) {
            self.pending_events.push(AoiEvent::Leave { observer: id, target: t });
        }
        self.last_visible.insert(id, now);
    }

    pub fn entity_count(&self) -> usize { self.entities.len() }
}

impl Aoi for GridAoi {
    fn insert(&mut self, id: EntityId, pos: [f32; 3], radius: f32) {
        if self.entities.contains_key(&id) {
            self.update(id, pos);
            return;
        }
        let cell = self.pos_to_cell(pos);
        self.cells.entry(cell).or_default().insert(id);
        self.entities.insert(id, EntityRecord { pos, radius: radius.max(0.0), cell });
        self.recompute_and_diff(id);
        // 同时让旧居民重算
        let neighbors: Vec<EntityId> = self.cells.get(&cell).cloned().unwrap_or_default()
            .into_iter().filter(|&n| n != id).collect();
        for n in neighbors {
            self.recompute_and_diff(n);
        }
    }

    fn update(&mut self, id: EntityId, pos: [f32; 3]) {
        let Some(rec) = self.entities.get(&id).cloned() else { return; };
        // 收集受影响的邻居：以 id 作为 observer/target 的双向集合
        let mut affected: HashSet<EntityId> = self.last_visible.get(&id).cloned().unwrap_or_default();
        for (obs, set) in &self.last_visible {
            if set.contains(&id) { affected.insert(*obs); }
        }
        affected.remove(&id);

        let new_cell = self.pos_to_cell(pos);
        if new_cell != rec.cell {
            if let Some(set) = self.cells.get_mut(&rec.cell) {
                set.remove(&id);
                if set.is_empty() { self.cells.remove(&rec.cell); }
            }
            self.cells.entry(new_cell).or_default().insert(id);
        }
        if let Some(r) = self.entities.get_mut(&id) {
            r.pos = pos;
            r.cell = new_cell;
        }
        self.recompute_and_diff(id);
        // 新可见集中的邻居也要补进受影响集
        if let Some(now) = self.last_visible.get(&id).cloned() {
            for n in now { affected.insert(n); }
        }
        for n in affected {
            if self.entities.contains_key(&n) {
                self.recompute_and_diff(n);
            }
        }
    }

    fn remove(&mut self, id: EntityId) {
        let Some(rec) = self.entities.remove(&id) else { return; };
        if let Some(set) = self.cells.get_mut(&rec.cell) {
            set.remove(&id);
            if set.is_empty() { self.cells.remove(&rec.cell); }
        }
        // 所有曾经看见自己的 observer 发 Leave
        let observers: Vec<EntityId> = self.last_visible.iter()
            .filter_map(|(obs, set)| if set.contains(&id) { Some(*obs) } else { None })
            .collect();
        for obs in observers {
            if let Some(set) = self.last_visible.get_mut(&obs) {
                set.remove(&id);
            }
            self.pending_events.push(AoiEvent::Leave { observer: obs, target: id });
        }
        self.last_visible.remove(&id);
    }

    fn observers(&self, target: EntityId) -> Vec<EntityId> {
        self.last_visible.iter()
            .filter_map(|(obs, set)| if set.contains(&target) { Some(*obs) } else { None })
            .collect()
    }

    fn take_events(&mut self) -> Vec<AoiEvent> {
        std::mem::take(&mut self.pending_events)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insert_emits_enter_for_close_pair() {
        let mut a = GridAoi::new(10.0);
        a.insert(1, [0.0, 0.0, 0.0], 15.0);
        let _ = a.take_events();
        a.insert(2, [5.0, 0.0, 5.0], 15.0);
        let evs = a.take_events();
        // 2 进入 1 的视野，1 也进入 2 的视野
        assert!(evs.iter().any(|e| *e == AoiEvent::Enter { observer: 1, target: 2 }));
        assert!(evs.iter().any(|e| *e == AoiEvent::Enter { observer: 2, target: 1 }));
    }

    #[test]
    fn far_entities_do_not_see_each_other() {
        let mut a = GridAoi::new(10.0);
        a.insert(1, [0.0, 0.0, 0.0], 10.0);
        a.insert(2, [1000.0, 0.0, 1000.0], 10.0);
        let evs = a.take_events();
        assert!(!evs.iter().any(|e| matches!(e, AoiEvent::Enter { .. })));
    }

    #[test]
    fn moving_out_emits_leave() {
        let mut a = GridAoi::new(10.0);
        a.insert(1, [0.0, 0.0, 0.0], 15.0);
        a.insert(2, [5.0, 0.0, 5.0], 15.0);
        let _ = a.take_events();
        a.update(2, [500.0, 0.0, 500.0]);
        let evs = a.take_events();
        assert!(evs.iter().any(|e| *e == AoiEvent::Leave { observer: 1, target: 2 }));
    }

    #[test]
    fn remove_emits_leave_for_observers() {
        let mut a = GridAoi::new(10.0);
        a.insert(1, [0.0, 0.0, 0.0], 15.0);
        a.insert(2, [5.0, 0.0, 5.0], 15.0);
        let _ = a.take_events();
        a.remove(2);
        let evs = a.take_events();
        assert!(evs.iter().any(|e| *e == AoiEvent::Leave { observer: 1, target: 2 }));
    }

    #[test]
    fn observers_query_after_steady_state() {
        let mut a = GridAoi::new(10.0);
        a.insert(1, [0.0, 0.0, 0.0], 15.0);
        a.insert(2, [5.0, 0.0, 5.0], 15.0);
        a.insert(3, [3.0, 0.0, 3.0], 15.0);
        let _ = a.take_events();
        let obs = a.observers(2);
        assert!(obs.contains(&1) && obs.contains(&3));
    }

    #[test]
    fn entity_count_tracks_inserts_and_removes() {
        let mut a = GridAoi::new(10.0);
        a.insert(1, [0.0, 0.0, 0.0], 10.0);
        a.insert(2, [0.0, 0.0, 0.0], 10.0);
        assert_eq!(a.entity_count(), 2);
        a.remove(1);
        assert_eq!(a.entity_count(), 1);
    }

    #[test]
    fn re_insert_acts_as_update() {
        let mut a = GridAoi::new(10.0);
        a.insert(1, [0.0, 0.0, 0.0], 10.0);
        a.insert(1, [50.0, 0.0, 50.0], 10.0); // 重复 insert 等价于 update
        assert_eq!(a.entity_count(), 1);
    }
}
