use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

/// 单个历史项目记录
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecentProject {
    pub name: String,
    pub path: String,
    pub template_id: String,
    pub last_opened: u64,  // Unix timestamp
}

/// 项目历史管理器
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectHistory {
    pub projects: Vec<RecentProject>,
    #[serde(default = "default_max_entries")]
    max_entries: usize,
}

fn default_max_entries() -> usize {
    20
}

impl ProjectHistory {
    pub fn new() -> Self {
        Self {
            projects: Vec::new(),
            max_entries: 20,
        }
    }

    /// 获取历史文件路径 (~/.geese/recent_projects.json)
    fn config_path() -> PathBuf {
        let mut path = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
        path.push(".geese");
        path.push("recent_projects.json");
        path
    }

    /// 从磁盘加载历史
    pub fn load() -> Self {
        let path = Self::config_path();
        if path.exists() {
            if let Ok(content) = fs::read_to_string(&path) {
                if let Ok(history) = serde_json::from_str(&content) {
                    return history;
                }
            }
        }
        Self::new()
    }

    /// 保存历史到磁盘
    pub fn save(&self) -> Result<(), String> {
        let path = Self::config_path();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("创建目录失败: {}", e))?;
        }
        let content = serde_json::to_string_pretty(self)
            .map_err(|e| format!("序列化失败: {}", e))?;
        fs::write(&path, content)
            .map_err(|e| format!("写入失败: {}", e))?;
        Ok(())
    }

    /// 添加或更新项目记录
    pub fn add_project(&mut self, name: String, path: String, template_id: String) {
        // 移除已存在的相同路径项目（移到最前面）
        self.projects.retain(|p| p.path != path);

        // 获取当前时间戳
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        // 插入到列表开头
        self.projects.insert(0, RecentProject {
            name,
            path,
            template_id,
            last_opened: timestamp,
        });

        // 限制最大条目数
        if self.projects.len() > self.max_entries {
            self.projects.truncate(self.max_entries);
        }
    }

    /// 验证项目路径是否仍然存在
    pub fn validate_projects(&mut self) {
        self.projects.retain(|p| PathBuf::from(&p.path).exists());
    }
}
