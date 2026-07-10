//! 组件存储（SoA 风格列式存储）。
//!
//! `ComponentStorage<T>` 按 entity_id → 组件映射存储，O(n) 查询。
//! 后续可升级为 archetype 或 sparse set 以提升大实体量下的性能。

use std::any::TypeId;

/// 类型擦除的列存储 trait。World 通过此 trait 操作异构组件。
pub trait AnyStorage: Send + Sync {
    fn type_id(&self) -> TypeId;
    fn type_name(&self) -> &'static str;
    fn entity_count(&self) -> usize;
    fn has_entity(&self, entity_id: &str) -> bool;
    fn remove_entity(&mut self, entity_id: &str) -> bool;
    /// 用于类型擦除后向下转型为 `ComponentStorage<T>`。
    fn as_any(&self) -> &dyn std::any::Any;
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any;
}

/// 单类组件的具型存储。
///
/// 使用 `Vec<(String, T)>` 存储，查询为 O(n)。适用场景：实体 < 10000。
pub struct ComponentStorage<T: Send + Sync + 'static> {
    entries: Vec<(String, T)>,
}

impl<T: Send + Sync + 'static> ComponentStorage<T> {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    /// 插入或更新组件的值。
    pub fn insert(&mut self, entity_id: impl Into<String>, component: T) {
        let id: String = entity_id.into();
        // 覆盖已存在的条目
        for (existing_id, existing_component) in self.entries.iter_mut() {
            if *existing_id == id {
                *existing_component = component;
                return;
            }
        }
        self.entries.push((id, component));
    }

    /// 获取不可变引用。
    pub fn get(&self, entity_id: &str) -> Option<&T> {
        self.entries
            .iter()
            .find(|(id, _)| id == entity_id)
            .map(|(_, c)| c)
    }

    /// 获取可变引用。
    pub fn get_mut(&mut self, entity_id: &str) -> Option<&mut T> {
        self.entries
            .iter_mut()
            .find(|(id, _)| id == entity_id)
            .map(|(_, c)| c)
    }

    /// 检查实体是否有此组件。
    pub fn has(&self, entity_id: &str) -> bool {
        self.entries.iter().any(|(id, _)| id == entity_id)
    }

    /// 移除实体的组件。返回是否移除成功。
    pub fn remove(&mut self, entity_id: &str) -> bool {
        let len_before = self.entries.len();
        self.entries.retain(|(id, _)| id != entity_id);
        self.entries.len() < len_before
    }

    /// 获取所有持有此组件的实体 ID 列表。
    pub fn entity_ids(&self) -> Vec<&str> {
        self.entries.iter().map(|(id, _)| id.as_str()).collect()
    }

    /// 遍历所有 (entity_id, &T) 对。
    pub fn iter(&self) -> impl Iterator<Item = (&str, &T)> {
        self.entries.iter().map(|(id, c)| (id.as_str(), c))
    }

    /// 可变遍历所有 (entity_id, &mut T) 对。
    pub fn iter_mut(&mut self) -> impl Iterator<Item = (&str, &mut T)> {
        self.entries.iter_mut().map(|(id, c)| (id.as_str(), c))
    }

    /// 条目数量。
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

impl<T: Send + Sync + 'static> AnyStorage for ComponentStorage<T> {
    fn type_id(&self) -> TypeId {
        TypeId::of::<T>()
    }

    fn type_name(&self) -> &'static str {
        std::any::type_name::<T>()
    }

    fn entity_count(&self) -> usize {
        self.entries.len()
    }

    fn has_entity(&self, entity_id: &str) -> bool {
        self.has(entity_id)
    }

    fn remove_entity(&mut self, entity_id: &str) -> bool {
        self.remove(entity_id)
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

impl<T: Send + Sync + 'static> Default for ComponentStorage<T> {
    fn default() -> Self {
        Self::new()
    }
}
