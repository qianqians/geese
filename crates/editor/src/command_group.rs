//! CommandGroup — 批量命令支持。
//!
//! [`CommandGroup`] 将多个编辑命令打包为一个原子操作：
//! - `execute()` 顺序执行所有子命令
//! - `undo()` 逆序撤销所有子命令
//! - 推入 [`CommandHistory`] 后作为一个整体撤销/重做
//!
//! 典型用途：批量删除、Gizmo 拖拽（多实体同时变换）、复制粘贴等。

use crate::commands::EditorCommand;

// ---------------------------------------------------------------------------
// CommandGroup
// ---------------------------------------------------------------------------

/// 批量命令组。将多个 [`EditorCommand`] 打包为一个原子操作。
///
/// # 示例
///
/// ```ignore
/// use editor::command_group::CommandGroup;
/// use editor::commands::EditorCommand;
///
/// let group = CommandGroup::new("Delete multiple entities")
///     .with(delete_cmd_1)
///     .with(delete_cmd_2)
///     .with(delete_cmd_3);
///
/// history.execute_group(group);
/// // Ctrl+Z 一次撤销全部三个删除
/// ```
pub struct CommandGroup {
    /// 子命令列表
    commands: Vec<Box<dyn EditorCommand>>,
    /// 操作描述（显示在 UI 中）
    description: String,
    /// 执行状态
    executed: bool,
}

impl CommandGroup {
    /// 创建一个空的命令组。
    pub fn new(description: &str) -> Self {
        Self {
            commands: Vec::new(),
            description: description.to_string(),
            executed: false,
        }
    }

    /// 添加一个子命令。
    pub fn with(mut self, command: Box<dyn EditorCommand>) -> Self {
        self.commands.push(command);
        self
    }

    /// 批量添加子命令。
    pub fn with_all(mut self, commands: Vec<Box<dyn EditorCommand>>) -> Self {
        self.commands.extend(commands);
        self
    }

    /// 子命令数量。
    pub fn len(&self) -> usize {
        self.commands.len()
    }

    /// 是否为空。
    pub fn is_empty(&self) -> bool {
        self.commands.is_empty()
    }
}

impl EditorCommand for CommandGroup {
    fn execute(&mut self) -> bool {
        // 顺序执行所有子命令
        for cmd in &mut self.commands {
            if !cmd.execute() {
                // 执行失败：回滚已执行的命令
                self.rollback_executed();
                return false;
            }
        }
        self.executed = true;
        true
    }

    fn undo(&mut self) -> bool {
        if !self.executed {
            return false;
        }
        // 逆序撤销所有子命令
        for cmd in self.commands.iter_mut().rev() {
            if !cmd.undo() {
                return false;
            }
        }
        self.executed = false;
        true
    }

    fn description(&self) -> &str {
        &self.description
    }
}

impl CommandGroup {
    /// 回滚已执行的子命令（从最后成功执行的命令开始逆序撤销）。
    fn rollback_executed(&mut self) {
        for cmd in self.commands.iter_mut().rev() {
            // 尝试撤销；忽略失败（最佳努力回滚）
            let _ = cmd.undo();
        }
    }
}

// ---------------------------------------------------------------------------
// 测试
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// 测试用简单命令
    struct CounterCmd {
        desc: String,
        counter: std::sync::Arc<std::sync::atomic::AtomicI32>,
        executed: bool,
    }

    impl CounterCmd {
        fn new(desc: &str, counter: std::sync::Arc<std::sync::atomic::AtomicI32>) -> Self {
            Self {
                desc: desc.to_string(),
                counter,
                executed: false,
            }
        }
    }

    impl EditorCommand for CounterCmd {
        fn execute(&mut self) -> bool {
            self.counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            self.executed = true;
            true
        }
        fn undo(&mut self) -> bool {
            if !self.executed {
                return false;
            }
            self.counter.fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
            self.executed = false;
            true
        }
        fn description(&self) -> &str {
            &self.desc
        }
    }

    #[test]
    fn command_group_execute_and_undo() {
        let counter = std::sync::Arc::new(std::sync::atomic::AtomicI32::new(0));

        let mut group = CommandGroup::new("Test Group")
            .with(Box::new(CounterCmd::new("A", counter.clone())))
            .with(Box::new(CounterCmd::new("B", counter.clone())))
            .with(Box::new(CounterCmd::new("C", counter.clone())));

        assert_eq!(group.len(), 3);

        // Execute
        let ok = group.execute();
        assert!(ok);
        assert_eq!(counter.load(std::sync::atomic::Ordering::SeqCst), 3);

        // Undo (reverse order)
        let ok = group.undo();
        assert!(ok);
        assert_eq!(counter.load(std::sync::atomic::Ordering::SeqCst), 0);

        // Double undo should fail
        assert!(!group.undo());
    }

    #[test]
    fn empty_command_group() {
        let group = CommandGroup::new("Empty");
        assert!(group.is_empty());
        assert_eq!(group.len(), 0);

        let mut group = group;
        assert!(group.execute());
        assert!(group.undo());
    }

    #[test]
    fn command_group_description() {
        let group = CommandGroup::new("Batch Delete (3 entities)");
        assert_eq!(group.description(), "Batch Delete (3 entities)");
    }

    #[test]
    fn command_group_with_all() {
        let counter = std::sync::Arc::new(std::sync::atomic::AtomicI32::new(0));
        let cmds: Vec<Box<dyn EditorCommand>> = vec![
            Box::new(CounterCmd::new("X", counter.clone())),
            Box::new(CounterCmd::new("Y", counter.clone())),
        ];

        let mut group = CommandGroup::new("With All").with_all(cmds);
        assert_eq!(group.len(), 2);
        assert!(group.execute());
        assert_eq!(counter.load(std::sync::atomic::Ordering::SeqCst), 2);
        assert!(group.undo());
        assert_eq!(counter.load(std::sync::atomic::Ordering::SeqCst), 0);
    }
}
