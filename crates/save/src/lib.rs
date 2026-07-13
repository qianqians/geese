//! 游戏存档系统：序列化/反序列化存档，支持多槽位、自动保存和手动保存。
//!
//! 提供：
//! - `SaveData`：存档数据（metadata + entities + game_state）
//! - `EntitySave`：单个实体的组件数据
//! - `SaveSlot`：一个存档槽位（含 id、名称、时间戳、数据）
//! - `SaveSlotInfo`：存档槽位摘要（用于列表显示，不含完整数据）
//! - `SaveManager`：管理存档的读写，支持 save/load/list_slots/delete

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// 核心类型
// ---------------------------------------------------------------------------

/// 单个实体的存档数据。
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EntitySave {
    pub entity_id: String,
    pub components: HashMap<String, serde_json::Value>,
}

/// 存档数据主体。
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SaveData {
    pub metadata: HashMap<String, String>,
    pub entities: Vec<EntitySave>,
    pub game_state: HashMap<String, serde_json::Value>,
}

/// 存档槽位（含完整数据）。
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SaveSlot {
    pub id: u32,
    pub name: String,
    /// 存储为 UNIX_EPOCH 以来的秒数，便于序列化。
    pub timestamp_secs: u64,
    pub data: SaveData,
}

impl SaveSlot {
    /// 返回存档时间（SystemTime）。
    pub fn timestamp(&self) -> SystemTime {
        UNIX_EPOCH + std::time::Duration::from_secs(self.timestamp_secs)
    }
}

/// 存档槽位摘要（用于列表，不含完整 data）。
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SaveSlotInfo {
    pub id: u32,
    pub name: String,
    pub timestamp_secs: u64,
    pub entity_count: usize,
}

// ---------------------------------------------------------------------------
// 存档文件内部包装
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize)]
struct SaveFile {
    version: u32,
    slot: SaveSlot,
}

const SAVE_VERSION: u32 = 1;

// ---------------------------------------------------------------------------
// SaveManager
// ---------------------------------------------------------------------------

/// 存档管理器：负责在指定目录下读写存档文件。
///
/// 每个槽位对应一个 `{save_dir}/slot_{id}.json` 文件。
pub struct SaveManager {
    save_dir: PathBuf,
    /// 自动保存槽位 id（如果启用）。
    auto_save_slot: Option<u32>,
}

impl SaveManager {
    /// 创建一个新的 SaveManager，存档存储在 `save_dir` 目录下。
    /// 目录不存在时会自动创建。
    pub fn new(save_dir: impl Into<PathBuf>) -> Result<Self, SaveError> {
        let save_dir = save_dir.into();
        if !save_dir.exists() {
            fs::create_dir_all(&save_dir)
                .map_err(|e| SaveError::Io(format!("创建存档目录失败: {e}")))?;
        }
        Ok(Self {
            save_dir,
            auto_save_slot: None,
        })
    }

    /// 启用自动保存：设置自动保存使用的槽位 id。
    pub fn enable_auto_save(&mut self, slot_id: u32) {
        self.auto_save_slot = Some(slot_id);
    }

    /// 禁用自动保存。
    pub fn disable_auto_save(&mut self) {
        self.auto_save_slot = None;
    }

    /// 执行自动保存（如果已启用）。返回是否实际执行了保存。
    pub fn auto_save(&self, name: &str, data: SaveData) -> Result<bool, SaveError> {
        if let Some(slot_id) = self.auto_save_slot {
            self.save(slot_id, name, data)?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// 保存存档到指定槽位。
    pub fn save(&self, slot_id: u32, name: &str, data: SaveData) -> Result<(), SaveError> {
        let now_secs = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let slot = SaveSlot {
            id: slot_id,
            name: name.to_string(),
            timestamp_secs: now_secs,
            data,
        };

        let file = SaveFile {
            version: SAVE_VERSION,
            slot,
        };

        let json = serde_json::to_string_pretty(&file)
            .map_err(|e| SaveError::Serialize(e.to_string()))?;

        let path = self.slot_path(slot_id);
        fs::write(&path, json)
            .map_err(|e| SaveError::Io(format!("写入存档失败 ({}): {e}", path.display())))?;

        Ok(())
    }

    /// 从指定槽位加载存档数据。
    pub fn load(&self, slot_id: u32) -> Result<SaveData, SaveError> {
        let slot = self.load_slot(slot_id)?;
        Ok(slot.data)
    }

    /// 从指定槽位加载完整 SaveSlot（含元数据）。
    pub fn load_slot(&self, slot_id: u32) -> Result<SaveSlot, SaveError> {
        let path = self.slot_path(slot_id);
        let json = fs::read_to_string(&path)
            .map_err(|e| SaveError::Io(format!("读取存档失败 ({}): {e}", path.display())))?;

        let file: SaveFile = serde_json::from_str(&json)
            .map_err(|e| SaveError::Deserialize(e.to_string()))?;

        Ok(file.slot)
    }

    /// 列出所有存档槽位摘要。
    pub fn list_slots(&self) -> Vec<SaveSlotInfo> {
        let mut slots = Vec::new();

        let entries = match fs::read_dir(&self.save_dir) {
            Ok(e) => e,
            Err(_) => return slots,
        };

        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            let file_name = match path.file_stem().and_then(|s| s.to_str()) {
                Some(n) => n.to_string(),
                None => continue,
            };
            if !file_name.starts_with("slot_") {
                continue;
            }

            // 尝试读取并解析
            if let Ok(json) = fs::read_to_string(&path) {
                if let Ok(file) = serde_json::from_str::<SaveFile>(&json) {
                    slots.push(SaveSlotInfo {
                        id: file.slot.id,
                        name: file.slot.name,
                        timestamp_secs: file.slot.timestamp_secs,
                        entity_count: file.slot.data.entities.len(),
                    });
                }
            }
        }

        slots.sort_by_key(|s| s.id);
        slots
    }

    /// 删除指定槽位的存档文件。
    pub fn delete(&self, slot_id: u32) -> Result<(), SaveError> {
        let path = self.slot_path(slot_id);
        if path.exists() {
            fs::remove_file(&path)
                .map_err(|e| SaveError::Io(format!("删除存档失败 ({}): {e}", path.display())))?;
        }
        Ok(())
    }

    /// 返回存档目录路径。
    pub fn save_dir(&self) -> &Path {
        &self.save_dir
    }

    /// 检查指定槽位是否有存档。
    pub fn slot_exists(&self, slot_id: u32) -> bool {
        self.slot_path(slot_id).exists()
    }

    fn slot_path(&self, slot_id: u32) -> PathBuf {
        self.save_dir.join(format!("slot_{slot_id}.json"))
    }
}

// ---------------------------------------------------------------------------
// 错误类型
// ---------------------------------------------------------------------------

/// 存档操作错误。
#[derive(Clone, Debug)]
pub enum SaveError {
    Io(String),
    Serialize(String),
    Deserialize(String),
}

impl std::fmt::Display for SaveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SaveError::Io(msg) => write!(f, "IO error: {msg}"),
            SaveError::Serialize(msg) => write!(f, "Serialize error: {msg}"),
            SaveError::Deserialize(msg) => write!(f, "Deserialize error: {msg}"),
        }
    }
}

impl std::error::Error for SaveError {}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_test_data() -> SaveData {
        let mut metadata = HashMap::new();
        metadata.insert("level".into(), "forest".into());

        let entities = vec![
            EntitySave {
                entity_id: "player_1".into(),
                components: {
                    let mut m = HashMap::new();
                    m.insert("position".into(), serde_json::json!({"x": 1.0, "y": 2.0}));
                    m.insert("health".into(), serde_json::json!(100));
                    m
                },
            },
        ];

        let mut game_state = HashMap::new();
        game_state.insert("time_of_day".into(), serde_json::json!(12.5));

        SaveData {
            metadata,
            entities,
            game_state,
        }
    }

    #[test]
    fn save_and_load_roundtrip() {
        let dir = std::env::temp_dir().join("save_test_roundtrip");
        let _ = fs::remove_dir_all(&dir);

        let mgr = SaveManager::new(&dir).unwrap();
        let data = make_test_data();
        mgr.save(1, "Test Save", data.clone()).unwrap();

        let loaded = mgr.load(1).unwrap();
        assert_eq!(loaded.metadata.get("level").unwrap(), "forest");
        assert_eq!(loaded.entities.len(), 1);
        assert_eq!(loaded.entities[0].entity_id, "player_1");

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn list_slots_returns_saved() {
        let dir = std::env::temp_dir().join("save_test_list");
        let _ = fs::remove_dir_all(&dir);

        let mgr = SaveManager::new(&dir).unwrap();
        mgr.save(0, "Slot Zero", make_test_data()).unwrap();
        mgr.save(2, "Slot Two", make_test_data()).unwrap();

        let slots = mgr.list_slots();
        assert_eq!(slots.len(), 2);
        assert_eq!(slots[0].id, 0);
        assert_eq!(slots[1].id, 2);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn delete_removes_slot() {
        let dir = std::env::temp_dir().join("save_test_delete");
        let _ = fs::remove_dir_all(&dir);

        let mgr = SaveManager::new(&dir).unwrap();
        mgr.save(5, "To Delete", make_test_data()).unwrap();
        assert!(mgr.slot_exists(5));

        mgr.delete(5).unwrap();
        assert!(!mgr.slot_exists(5));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_nonexistent_returns_error() {
        let dir = std::env::temp_dir().join("save_test_noload");
        let _ = fs::remove_dir_all(&dir);

        let mgr = SaveManager::new(&dir).unwrap();
        assert!(mgr.load(999).is_err());

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn auto_save_works_when_enabled() {
        let dir = std::env::temp_dir().join("save_test_autosave");
        let _ = fs::remove_dir_all(&dir);

        let mut mgr = SaveManager::new(&dir).unwrap();
        // auto_save disabled by default
        let result = mgr.auto_save("auto", make_test_data()).unwrap();
        assert!(!result);

        mgr.enable_auto_save(99);
        let result = mgr.auto_save("auto", make_test_data()).unwrap();
        assert!(result);
        assert!(mgr.slot_exists(99));

        let _ = fs::remove_dir_all(&dir);
    }
}
