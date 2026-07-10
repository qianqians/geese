//! 轻量级 ECS（Entity-Component-System）注册表。
//!
//! 提供：
//! - [`Component`] trait：标记类型为 ECS 组件
//! - [`World`]：实体注册表 + 类型化组件存储
//! - [`EcsBridge`] trait：最小桥接接口，供 Scene 通过 trait object 持有
//!
//! # 示例
//!
//! ```
//! use ecs::World;
//!
//! // Component trait 有 blanket impl，任何 Clone + Send + Sync + 'static 类型自动实现
//! #[derive(Debug, Clone)]
//! struct Health(f32);
//!
//! #[derive(Debug, Clone)]
//! struct Position { x: f32, y: f32, z: f32 }
//!
//! let mut world = World::new();
//! world
//!     .spawn("entity_1")
//!     .with(Health(100.0))
//!     .with(Position { x: 1.0, y: 2.0, z: 3.0 });
//!
//! assert!(world.has::<Health>("entity_1"));
//! assert_eq!(world.get::<Health>("entity_1").unwrap().0, 100.0);
//! ```

pub mod storage;

use std::any::TypeId;
use std::collections::HashMap;
use storage::{AnyStorage, ComponentStorage};

// ---------------------------------------------------------------------------
// Component trait
// ---------------------------------------------------------------------------

/// 标记 trait：实现此 trait 的类型可作为 ECS 组件存储于 [`World`] 中。
///
/// 同时要求 `Clone + Send + Sync + 'static` 以确保存储安全。
pub trait Component: Clone + Send + Sync + 'static {}

// Blanket impl for types satisfying the bounds
impl<T: Clone + Send + Sync + 'static> Component for T {}

// ---------------------------------------------------------------------------
// EntityBuilder
// ---------------------------------------------------------------------------

/// 实体构造器（链式 API）。
///
/// 由 [`World::spawn()`] 返回，通过 `.with::<T>(component)` 添加组件。
pub struct EntityBuilder<'w> {
    world: &'w mut World,
    entity_id: String,
}

impl<'w> EntityBuilder<'w> {
    /// 添加组件。
    pub fn with<T: Component>(self, component: T) -> Self {
        self.world.insert(self.entity_id.clone(), component);
        self
    }

    /// 获取实体 ID（用于链式操作过程中的引用）。
    pub fn id(&self) -> &str {
        &self.entity_id
    }
}

// ---------------------------------------------------------------------------
// World
// ---------------------------------------------------------------------------

/// ECS 世界：实体注册表 + 组件存储。
///
/// 内部为每个组件类型维护一个 `ComponentStorage<T>`。
/// 使用 `TypeId` 进行类型索引。
pub struct World {
    /// 所有已注册的实体 ID（有序）。
    entities: Vec<String>,
    /// 按组件类型索引的存储。
    storages: HashMap<TypeId, Box<dyn AnyStorage>>,
}

impl World {
    pub fn new() -> Self {
        Self {
            entities: Vec::new(),
            storages: HashMap::new(),
        }
    }

    // -- 实体生命周期 --

    /// 生成新实体（或重新激活已有 ID），返回构造器。
    ///
    /// 若 entity_id 已存在，则不清除现有组件（仅返回构造器）。
    pub fn spawn(&mut self, entity_id: impl Into<String>) -> EntityBuilder<'_> {
        let id: String = entity_id.into();
        if !self.entities.contains(&id) {
            self.entities.push(id.clone());
        }
        EntityBuilder {
            world: self,
            entity_id: id,
        }
    }

    /// 销毁实体，移除其所有组件。
    pub fn despawn(&mut self, entity_id: &str) -> bool {
        let idx = self.entities.iter().position(|id| id == entity_id);
        if let Some(pos) = idx {
            self.entities.remove(pos);
            for storage in self.storages.values_mut() {
                storage.remove_entity(entity_id);
            }
            true
        } else {
            false
        }
    }

    /// 检查实体是否已注册。
    pub fn exists(&self, entity_id: &str) -> bool {
        self.entities.iter().any(|id| id == entity_id)
    }

    /// 所有已注册实体 ID 的数量。
    pub fn entity_count(&self) -> usize {
        self.entities.len()
    }

    /// 所有已注册实体 ID 的迭代器。
    pub fn entity_ids(&self) -> impl Iterator<Item = &str> {
        self.entities.iter().map(|s| s.as_str())
    }

    // -- 组件操作 --

    /// 插入或更新组件的值。
    ///
    /// 注意：此方法不会自动注册实体——需先调用 `spawn()`。
    pub fn insert<T: Component>(&mut self, entity_id: String, component: T) {
        let storage = self
            .storages
            .entry(TypeId::of::<T>())
            .or_insert_with(|| Box::new(ComponentStorage::<T>::new()));
        storage
            .as_any_mut()
            .downcast_mut::<ComponentStorage<T>>()
            .expect("TypeId mismatch in ECS storage")
            .insert(entity_id, component);
    }

    /// 获取不可变引用。
    pub fn get<T: Component>(&self, entity_id: &str) -> Option<&T> {
        self.storages
            .get(&TypeId::of::<T>())?
            .as_any()
            .downcast_ref::<ComponentStorage<T>>()?
            .get(entity_id)
    }

    /// 获取可变引用。
    pub fn get_mut<T: Component>(&mut self, entity_id: &str) -> Option<&mut T> {
        self.storages
            .get_mut(&TypeId::of::<T>())?
            .as_any_mut()
            .downcast_mut::<ComponentStorage<T>>()?
            .get_mut(entity_id)
    }

    /// 检查实体是否持有某组件。
    pub fn has<T: Component>(&self, entity_id: &str) -> bool {
        self.storages
            .get(&TypeId::of::<T>())
            .and_then(|s| s.as_any().downcast_ref::<ComponentStorage<T>>())
            .map(|s| s.has(entity_id))
            .unwrap_or(false)
    }

    /// 移除实体的某组件。返回是否移除成功。
    pub fn remove<T: Component>(&mut self, entity_id: &str) -> bool {
        self.storages
            .get_mut(&TypeId::of::<T>())
            .and_then(|s| s.as_any_mut().downcast_mut::<ComponentStorage<T>>())
            .map(|s| s.remove(entity_id))
            .unwrap_or(false)
    }

    /// 获取拥有某组件的实体 ID 列表。
    pub fn entities_with<T: Component>(&self) -> Vec<&str> {
        self.storages
            .get(&TypeId::of::<T>())
            .and_then(|s| s.as_any().downcast_ref::<ComponentStorage<T>>())
            .map(|s| s.entity_ids())
            .unwrap_or_default()
    }

    /// 遍历所有 (entity_id, &T) 对。
    pub fn iter<T: Component>(&self) -> Option<impl Iterator<Item = (&str, &T)>> {
        self.storages
            .get(&TypeId::of::<T>())
            .and_then(|s| s.as_any().downcast_ref::<ComponentStorage<T>>())
            .map(|s| s.iter())
    }

    /// 遍历所有已注册的组件类型名称。
    pub fn component_types(&self) -> Vec<&'static str> {
        self.storages.values().map(|s| s.type_name()).collect()
    }

    /// 组件存储数量（不同组件类型的数量）。
    pub fn storage_count(&self) -> usize {
        self.storages.len()
    }
}

impl Default for World {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// EcsBridge — Scene 通过 trait object 持有的最小接口
// ---------------------------------------------------------------------------

/// ECS 桥接 trait：Scene 通过 `Option<Box<dyn EcsBridge>>` 持有。
///
/// trait 方法不含泛型参数，满足对象安全性。
pub trait EcsBridge: Send + Sync {
    /// 已注册实体数量。
    fn entity_count(&self) -> usize;

    /// 实体是否存在。
    fn exists(&self, entity_id: &str) -> bool;

    /// 是否持有指定组件（按类型名查询，适合 trait object 调用）。
    fn has_component_by_name(&self, entity_id: &str, type_name: &str) -> bool;

    /// 获取所有实体 ID。
    fn entity_ids(&self) -> Vec<String>;

    /// 获取所有已注册的组件类型名称。
    fn component_type_names(&self) -> Vec<String>;
}

/// World 对 EcsBridge 的实现。
///
/// 注意：`has_component_by_name` 使用 `std::any::type_name::<T>()` 返回的名称做匹配。
/// 这是调试级别的匹配，不适合热路径。
impl EcsBridge for World {
    fn entity_count(&self) -> usize {
        self.entity_count()
    }

    fn exists(&self, entity_id: &str) -> bool {
        self.exists(entity_id)
    }

    fn has_component_by_name(&self, entity_id: &str, type_name: &str) -> bool {
        self.storages
            .values()
            .any(|s| s.type_name() == type_name && s.has_entity(entity_id))
    }

    fn entity_ids(&self) -> Vec<String> {
        self.entities.clone()
    }

    fn component_type_names(&self) -> Vec<String> {
        self.storages
            .values()
            .map(|s| s.type_name().to_string())
            .collect()
    }
}

// ---------------------------------------------------------------------------
// 测试
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Clone, PartialEq)]
    struct Position {
        x: f32,
        y: f32,
        z: f32,
    }

    #[derive(Debug, Clone, PartialEq)]
    struct Velocity {
        dx: f32,
        dy: f32,
        dz: f32,
    }

    #[test]
    fn spawn_and_insert() {
        let mut world = World::new();
        world.spawn("e1").with(Position { x: 1.0, y: 2.0, z: 3.0 });

        assert!(world.exists("e1"));
        assert_eq!(world.entity_count(), 1);
        assert_eq!(
            world.get::<Position>("e1"),
            Some(&Position { x: 1.0, y: 2.0, z: 3.0 })
        );
    }

    #[test]
    fn multiple_components() {
        let mut world = World::new();
        world
            .spawn("player")
            .with(Position { x: 0.0, y: 0.0, z: 0.0 })
            .with(Velocity { dx: 1.0, dy: 0.0, dz: 0.0 });

        assert!(world.has::<Position>("player"));
        assert!(world.has::<Velocity>("player"));
        assert!(!world.has::<Velocity>("nonexistent"));
    }

    #[test]
    fn get_mut_and_update() {
        let mut world = World::new();
        world.spawn("e2").with(Position { x: 1.0, y: 2.0, z: 3.0 });

        if let Some(pos) = world.get_mut::<Position>("e2") {
            pos.x = 10.0;
        }

        assert_eq!(world.get::<Position>("e2").unwrap().x, 10.0);
    }

    #[test]
    fn remove_component() {
        let mut world = World::new();
        world.spawn("e3").with(Position { x: 0.0, y: 0.0, z: 0.0 });

        assert!(world.has::<Position>("e3"));
        assert!(world.remove::<Position>("e3"));
        assert!(!world.has::<Position>("e3"));
    }

    #[test]
    fn despawn_entity() {
        let mut world = World::new();
        world.spawn("temp").with(Position { x: 0.0, y: 0.0, z: 0.0 });

        assert!(world.despawn("temp"));
        assert!(!world.exists("temp"));
        assert!(!world.has::<Position>("temp"));
    }

    #[test]
    fn entities_with_component() {
        let mut world = World::new();
        world.spawn("a").with(Position { x: 1.0, y: 0.0, z: 0.0 });
        world.spawn("b").with(Position { x: 2.0, y: 0.0, z: 0.0 });
        world.spawn("c"); // 无 Position

        let entities: Vec<_> = world.entities_with::<Position>();
        assert_eq!(entities.len(), 2);
        assert!(entities.contains(&"a"));
        assert!(entities.contains(&"b"));
    }

    #[test]
    fn ecs_bridge_trait_object() {
        let mut world = World::new();
        world.spawn("e1").with(Position { x: 0.0, y: 0.0, z: 0.0 });

        let bridge: &dyn EcsBridge = &world;
        assert_eq!(bridge.entity_count(), 1);
        assert!(bridge.exists("e1"));
        assert!(!bridge.exists("e2"));
        assert!(bridge.has_component_by_name(
            "e1",
            std::any::type_name::<Position>()
        ));
    }

    #[test]
    fn insert_overwrites_existing() {
        let mut world = World::new();
        world.spawn("e1").with(Position { x: 1.0, y: 2.0, z: 3.0 });

        // 重新 insert 应覆盖
        world.insert(
            "e1".to_string(),
            Position { x: 99.0, y: 99.0, z: 99.0 },
        );

        assert_eq!(
            world.get::<Position>("e1"),
            Some(&Position { x: 99.0, y: 99.0, z: 99.0 })
        );
    }
}
