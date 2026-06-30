//! Undo/Redo 命令系统。
//!
//! 提供：
//! - [`EditorCommand`] trait：可执行/可撤销的编辑命令
//! - [`CommandHistory`]：双栈结构的命令历史管理
//! - 具体命令：TransformCommand、DeleteEntity、CreateEntity、ReparentCommand
//!
//! 快捷键：Ctrl+Z（撤销）、Ctrl+Shift+Z（重做）

use cgmath::{Point3, Vector3};

// ---------------------------------------------------------------------------
// EditorCommand trait
// ---------------------------------------------------------------------------

/// 可撤销的编辑器命令。
///
/// 每个命令负责保存执行前状态，以便 `undo()` 恢复。
pub trait EditorCommand {
    /// 执行命令，返回是否成功。
    fn execute(&mut self) -> bool;

    /// 撤销命令，返回是否成功。
    fn undo(&mut self) -> bool;

    /// 命令描述（用于 UI 显示）。
    fn description(&self) -> &str;
}

// ---------------------------------------------------------------------------
// 具体命令实现
// ---------------------------------------------------------------------------

/// 变换命令：记录实体的位置/旋转/缩放变更。
pub struct TransformCommand {
    /// 受影响的实体 ID
    pub entity_id: String,
    /// 变更前的位置
    pub old_position: Point3<f32>,
    /// 变更前（应用后）
    pub new_position: Point3<f32>,
    /// 变更前的旋转（欧拉角，度）
    pub old_rotation_euler: Vector3<f32>,
    /// 变更后的旋转（欧拉角，度）
    pub new_rotation_euler: Vector3<f32>,
    /// 变更前的缩放
    pub old_scale: Vector3<f32>,
    /// 变更后的缩放
    pub new_scale: Vector3<f32>,
    /// 外部回调：应用变换到目标实体
    pub apply: Option<Box<dyn Fn(&str, Point3<f32>, Vector3<f32>, Vector3<f32>)>>,
    executed: bool,
}

impl TransformCommand {
    pub fn new(
        entity_id: String,
        old_position: Point3<f32>,
        new_position: Point3<f32>,
        old_rotation_euler: Vector3<f32>,
        new_rotation_euler: Vector3<f32>,
        old_scale: Vector3<f32>,
        new_scale: Vector3<f32>,
    ) -> Self {
        Self {
            entity_id,
            old_position,
            new_position,
            old_rotation_euler,
            new_rotation_euler,
            old_scale,
            new_scale,
            apply: None,
            executed: false,
        }
    }

    /// 设置应用变换的回调。
    pub fn with_apply<F>(mut self, f: F) -> Self
    where
        F: Fn(&str, Point3<f32>, Vector3<f32>, Vector3<f32>) + 'static,
    {
        self.apply = Some(Box::new(f));
        self
    }
}

impl EditorCommand for TransformCommand {
    fn execute(&mut self) -> bool {
        if let Some(ref apply) = self.apply {
            apply(&self.entity_id, self.new_position, self.new_rotation_euler, self.new_scale);
        }
        self.executed = true;
        true
    }

    fn undo(&mut self) -> bool {
        if !self.executed {
            return false;
        }
        if let Some(ref apply) = self.apply {
            apply(&self.entity_id, self.old_position, self.old_rotation_euler, self.old_scale);
        }
        self.executed = false;
        true
    }

    fn description(&self) -> &str {
        "Transform"
    }
}

/// 删除实体命令。
pub struct DeleteEntityCommand {
    /// 被删除的实体 ID
    pub entity_id: String,
    /// 父实体 ID
    pub parent_id: Option<String>,
    /// 删除前的位置
    pub position: Point3<f32>,
    /// 删除前的旋转（欧拉角）
    pub rotation_euler: Vector3<f32>,
    /// 删除前的缩放
    pub scale: Vector3<f32>,
    /// 删除回调
    pub on_delete: Option<Box<dyn Fn(&str)>>,
    /// 创建回调（用于 undo）
    pub on_create: Option<Box<dyn Fn(&str, Point3<f32>, Vector3<f32>, Vector3<f32>)>>,
    executed: bool,
}

impl DeleteEntityCommand {
    pub fn new(entity_id: String, position: Point3<f32>, rotation_euler: Vector3<f32>, scale: Vector3<f32>) -> Self {
        Self {
            entity_id,
            parent_id: None,
            position,
            rotation_euler,
            scale,
            on_delete: None,
            on_create: None,
            executed: false,
        }
    }
}

impl EditorCommand for DeleteEntityCommand {
    fn execute(&mut self) -> bool {
        if let Some(ref on_delete) = self.on_delete {
            on_delete(&self.entity_id);
        }
        self.executed = true;
        true
    }

    fn undo(&mut self) -> bool {
        if !self.executed {
            return false;
        }
        if let Some(ref on_create) = self.on_create {
            on_create(&self.entity_id, self.position, self.rotation_euler, self.scale);
        }
        self.executed = false;
        true
    }

    fn description(&self) -> &str {
        "Delete Entity"
    }
}

/// 创建实体命令。
pub struct CreateEntityCommand {
    /// 创建的实体 ID
    pub entity_id: String,
    /// 创建位置
    pub position: Point3<f32>,
    /// 创建时旋转
    pub rotation_euler: Vector3<f32>,
    /// 创建时缩放
    pub scale: Vector3<f32>,
    /// 创建回调
    pub on_create: Option<Box<dyn Fn(&str, Point3<f32>, Vector3<f32>, Vector3<f32>)>>,
    /// 删除回调（用于 undo）
    pub on_delete: Option<Box<dyn Fn(&str)>>,
    executed: bool,
}

impl CreateEntityCommand {
    pub fn new(entity_id: String, position: Point3<f32>, rotation_euler: Vector3<f32>, scale: Vector3<f32>) -> Self {
        Self {
            entity_id,
            position,
            rotation_euler,
            scale,
            on_create: None,
            on_delete: None,
            executed: false,
        }
    }
}

impl EditorCommand for CreateEntityCommand {
    fn execute(&mut self) -> bool {
        if let Some(ref on_create) = self.on_create {
            on_create(&self.entity_id, self.position, self.rotation_euler, self.scale);
        }
        self.executed = true;
        true
    }

    fn undo(&mut self) -> bool {
        if !self.executed {
            return false;
        }
        if let Some(ref on_delete) = self.on_delete {
            on_delete(&self.entity_id);
        }
        self.executed = false;
        true
    }

    fn description(&self) -> &str {
        "Create Entity"
    }
}

/// 实例化 Prefab 命令。
///
/// 记录从 Prefab 创建的实体，支持 undo（删除实例化实体）和 redo（重新实例化）。
pub struct InstantiatePrefabCommand {
    /// 被实例化的 Prefab UUID
    pub prefab_uuid: String,
    /// 实例化位置
    pub position: Point3<f32>,
    /// 实例化后创建的实体 ID 列表
    pub created_entity_ids: Vec<String>,
    /// 实例化回调：prefab_uuid, position → 创建的 entity_ids
    pub on_instantiate: Option<Box<dyn Fn(&str, Point3<f32>) -> Vec<String>>>,
    /// 删除回调：entity_id
    pub on_remove: Option<Box<dyn Fn(&str)>>,
    executed: bool,
}

impl InstantiatePrefabCommand {
    pub fn new(prefab_uuid: String, position: Point3<f32>) -> Self {
        Self {
            prefab_uuid,
            position,
            created_entity_ids: Vec::new(),
            on_instantiate: None,
            on_remove: None,
            executed: false,
        }
    }

    /// 设置实例化回调。
    pub fn with_callbacks<F, G>(mut self, on_instantiate: F, on_remove: G) -> Self
    where
        F: Fn(&str, Point3<f32>) -> Vec<String> + 'static,
        G: Fn(&str) + 'static,
    {
        self.on_instantiate = Some(Box::new(on_instantiate));
        self.on_remove = Some(Box::new(on_remove));
        self
    }
}

impl EditorCommand for InstantiatePrefabCommand {
    fn execute(&mut self) -> bool {
        if let Some(ref on_instantiate) = self.on_instantiate {
            self.created_entity_ids = on_instantiate(&self.prefab_uuid, self.position);
        }
        self.executed = true;
        !self.created_entity_ids.is_empty()
    }

    fn undo(&mut self) -> bool {
        if !self.executed {
            return false;
        }
        if let Some(ref on_remove) = self.on_remove {
            for entity_id in &self.created_entity_ids {
                on_remove(entity_id);
            }
        }
        self.executed = false;
        true
    }

    fn description(&self) -> &str {
        "Instantiate Prefab"
    }
}

/// 重父子关系命令。
pub struct ReparentCommand {
    /// 目标实体 ID
    pub entity_id: String,
    /// 原来的父实体
    pub old_parent: Option<String>,
    /// 新的父实体
    pub new_parent: Option<String>,
    /// 重父子回调
    pub on_reparent: Option<Box<dyn Fn(&str, Option<&str>)>>,
    executed: bool,
}

impl ReparentCommand {
    pub fn new(
        entity_id: String,
        old_parent: Option<String>,
        new_parent: Option<String>,
    ) -> Self {
        Self {
            entity_id,
            old_parent,
            new_parent,
            on_reparent: None,
            executed: false,
        }
    }
}

impl EditorCommand for ReparentCommand {
    fn execute(&mut self) -> bool {
        if let Some(ref on_reparent) = self.on_reparent {
            on_reparent(&self.entity_id, self.new_parent.as_deref());
        }
        self.executed = true;
        true
    }

    fn undo(&mut self) -> bool {
        if !self.executed {
            return false;
        }
        if let Some(ref on_reparent) = self.on_reparent {
            on_reparent(&self.entity_id, self.old_parent.as_deref());
        }
        self.executed = false;
        true
    }

    fn description(&self) -> &str {
        "Reparent"
    }
}

// ---------------------------------------------------------------------------
// CommandHistory - 双栈命令历史
// ---------------------------------------------------------------------------

/// Undo/Redo 命令历史管理器。
///
/// 使用双栈结构：
/// - `undo_stack`：可撤销的命令
/// - `redo_stack`：可重做的命令（当有新命令执行时清空）
pub struct CommandHistory {
    /// 可撤销的命令栈
    undo_stack: Vec<Box<dyn EditorCommand>>,
    /// 可重做的命令栈
    redo_stack: Vec<Box<dyn EditorCommand>>,
    /// 最大历史深度
    max_depth: usize,
}

impl CommandHistory {
    pub fn new(max_depth: usize) -> Self {
        Self {
            undo_stack: Vec::with_capacity(max_depth),
            redo_stack: Vec::new(),
            max_depth,
        }
    }

    /// 执行命令并推入 undo 栈。
    ///
    /// 这会清空 redo 栈。
    pub fn execute(&mut self, mut command: Box<dyn EditorCommand>) -> bool {
        let success = command.execute();
        if !success {
            return false;
        }

        // 限制栈深度
        if self.undo_stack.len() >= self.max_depth {
            self.undo_stack.remove(0);
        }

        self.undo_stack.push(command);
        self.redo_stack.clear();
        true
    }

    /// 撤销最近的命令。
    ///
    /// 返回描述字符串，若无命令可撤销则返回 None。
    pub fn undo(&mut self) -> Option<String> {
        let mut command = self.undo_stack.pop()?;
        let desc = command.description().to_string();
        let success = command.undo();
        if success {
            self.redo_stack.push(command);
            Some(desc)
        } else {
            // 撤销失败，放回
            self.undo_stack.push(command);
            None
        }
    }

    /// 重做最近撤销的命令。
    ///
    /// 返回描述字符串，若无命令可重做则返回 None。
    pub fn redo(&mut self) -> Option<String> {
        let mut command = self.redo_stack.pop()?;
        let desc = command.description().to_string();
        let success = command.execute();
        if success {
            self.undo_stack.push(command);
            Some(desc)
        } else {
            self.redo_stack.push(command);
            None
        }
    }

    /// 是否可以撤销。
    pub fn can_undo(&self) -> bool {
        !self.undo_stack.is_empty()
    }

    /// 是否可以重做。
    pub fn can_redo(&self) -> bool {
        !self.redo_stack.is_empty()
    }

    /// 撤销栈深度。
    pub fn undo_depth(&self) -> usize {
        self.undo_stack.len()
    }

    /// 重做栈深度。
    pub fn redo_depth(&self) -> usize {
        self.redo_stack.len()
    }

    /// 弹出最近一条 undo 命令（用于合并连续拖拽操作，不触发 undo 逻辑）。
    pub fn pop_last_undo(&mut self) -> Option<Box<dyn EditorCommand>> {
        self.undo_stack.pop()
    }

    /// 最近命令的描述。
    pub fn last_undo_description(&self) -> Option<&str> {
        self.undo_stack.last().map(|c| c.description())
    }

    /// 清空历史。
    pub fn clear(&mut self) {
        self.undo_stack.clear();
        self.redo_stack.clear();
    }
}

impl Default for CommandHistory {
    fn default() -> Self {
        Self::new(256)
    }
}

// ---------------------------------------------------------------------------
// 场景序列化（占位）
// ---------------------------------------------------------------------------

/// 场景序列化器。
///
/// 将场景状态序列化为 JSON 字符串，支持往返。
pub struct SceneSerializer;

impl SceneSerializer {
    /// 序列化场景到 JSON 字符串。
    pub fn serialize_to_json(
        entities: &[SerializedEntity],
    ) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(entities)
    }

    /// 从 JSON 字符串反序列化场景。
    pub fn deserialize_from_json(json: &str) -> Result<Vec<SerializedEntity>, serde_json::Error> {
        serde_json::from_str(json)
    }
}

/// 可序列化的实体表示。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SerializedEntity {
    pub id: String,
    pub name: String,
    pub parent: Option<String>,
    pub position: [f32; 3],
    pub rotation_euler: [f32; 3],
    pub scale: [f32; 3],
    pub visible: bool,
    pub locked: bool,
}

impl SerializedEntity {
    pub fn new(
        id: String,
        name: String,
        parent: Option<String>,
        position: Point3<f32>,
        rotation_euler: Vector3<f32>,
        scale: Vector3<f32>,
    ) -> Self {
        Self {
            id,
            name,
            parent,
            position: [position.x, position.y, position.z],
            rotation_euler: [rotation_euler.x, rotation_euler.y, rotation_euler.z],
            scale: [scale.x, scale.y, scale.z],
            visible: true,
            locked: false,
        }
    }
}

// ---------------------------------------------------------------------------
// 测试
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_command_history_undo_redo() {
        let mut history = CommandHistory::new(10);

        // 简单命令：只记录执行状态
        struct TestCommand {
            desc: String,
            executed: bool,
        }
        impl EditorCommand for TestCommand {
            fn execute(&mut self) -> bool {
                self.executed = true;
                true
            }
            fn undo(&mut self) -> bool {
                self.executed = false;
                true
            }
            fn description(&self) -> &str {
                &self.desc
            }
        }

        history.execute(Box::new(TestCommand { desc: "Cmd1".into(), executed: false }));
        history.execute(Box::new(TestCommand { desc: "Cmd2".into(), executed: false }));

        assert_eq!(history.undo_depth(), 2);
        assert!(history.can_undo());

        let desc = history.undo();
        assert_eq!(desc, Some("Cmd2".into()));
        assert_eq!(history.undo_depth(), 1);
        assert_eq!(history.redo_depth(), 1);

        let desc = history.redo();
        assert_eq!(desc, Some("Cmd2".into()));
        assert_eq!(history.undo_depth(), 2);
        assert_eq!(history.redo_depth(), 0);
    }

    #[test]
    fn test_redo_cleared_on_new_command() {
        let mut history = CommandHistory::new(10);

        struct TestCommand {
            desc: String,
            executed: bool,
        }
        impl EditorCommand for TestCommand {
            fn execute(&mut self) -> bool {
                self.executed = true;
                true
            }
            fn undo(&mut self) -> bool {
                self.executed = false;
                true
            }
            fn description(&self) -> &str {
                &self.desc
            }
        }

        history.execute(Box::new(TestCommand { desc: "Cmd1".into(), executed: false }));
        history.undo();
        assert_eq!(history.redo_depth(), 1);

        // 新命令应清空 redo 栈
        history.execute(Box::new(TestCommand { desc: "Cmd2".into(), executed: false }));
        assert_eq!(history.redo_depth(), 0);
    }

    #[test]
    fn test_max_depth() {
        let mut history = CommandHistory::new(3);

        struct TestCommand {
            desc: String,
            executed: bool,
        }
        impl EditorCommand for TestCommand {
            fn execute(&mut self) -> bool {
                self.executed = true;
                true
            }
            fn undo(&mut self) -> bool {
                self.executed = false;
                true
            }
            fn description(&self) -> &str {
                &self.desc
            }
        }

        for i in 0..5 {
            history.execute(Box::new(TestCommand { desc: format!("Cmd{}", i), executed: false }));
        }

        // 最早的两个命令应被移除
        assert_eq!(history.undo_depth(), 3);
    }

    #[test]
    fn test_scene_serialization_roundtrip() {
        let entities = vec![
            SerializedEntity::new(
                "root".into(),
                "Root".into(),
                None,
                Point3::new(0.0, 0.0, 0.0),
                Vector3::new(0.0, 0.0, 0.0),
                Vector3::new(1.0, 1.0, 1.0),
            ),
            SerializedEntity::new(
                "child".into(),
                "Child".into(),
                Some("root".into()),
                Point3::new(1.0, 2.0, 3.0),
                Vector3::new(45.0, 0.0, 0.0),
                Vector3::new(1.0, 1.5, 1.0),
            ),
        ];

        let json = SceneSerializer::serialize_to_json(&entities).unwrap();
        let restored = SceneSerializer::deserialize_from_json(&json).unwrap();

        assert_eq!(restored.len(), 2);
        assert_eq!(restored[0].id, "root");
        assert_eq!(restored[1].parent, Some("root".into()));
        assert_eq!(restored[1].position, [1.0, 2.0, 3.0]);
    }
}
