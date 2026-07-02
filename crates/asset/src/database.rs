//! 资源数据库——扫描 assets 目录，管理 .meta 文件，维护 UUID 索引。

use crate::dependency_scanner;
use crate::meta::{self, AssetTypeKind};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

/// 资源数据库：扫描 assets/ 目录，管理 .meta 文件，维护 UUID 索引。
pub struct AssetDatabase {
    /// 项目根目录（绝对路径）
    project_root: PathBuf,
    /// 资产根目录名（默认 "assets"）
    assets_dir: String,
    /// UUID → DatabaseEntry 的内存索引
    entries: HashMap<String, DatabaseEntry>,
    /// 相对路径 → UUID 的反向索引
    path_to_uuid: HashMap<String, String>,
}

/// 内存中的资产条目。
#[derive(Debug, Clone)]
pub struct DatabaseEntry {
    pub uuid: String,
    /// 相对于项目根目录的路径，如 "assets/models/hero.glb"
    pub path: String,
    pub asset_type: AssetTypeKind,
    /// 依赖的其他资源 UUID 列表
    pub dependencies: Vec<String>,
    pub file_size: u64,
}

/// 扫描报告。
#[derive(Debug, Default)]
pub struct ScanReport {
    pub new_assets: usize,
    pub updated: usize,
    pub removed: usize,
    pub errors: Vec<String>,
}

/// 数据库错误。
#[derive(Debug)]
pub enum DatabaseError {
    Io(std::io::Error),
    Meta(meta::MetaError),
}

impl std::fmt::Display for DatabaseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DatabaseError::Io(e) => write!(f, "database IO error: {e}"),
            DatabaseError::Meta(e) => write!(f, "database meta error: {e}"),
        }
    }
}

impl std::error::Error for DatabaseError {}

impl From<std::io::Error> for DatabaseError {
    fn from(e: std::io::Error) -> Self {
        DatabaseError::Io(e)
    }
}

impl From<meta::MetaError> for DatabaseError {
    fn from(e: meta::MetaError) -> Self {
        DatabaseError::Meta(e)
    }
}

impl AssetDatabase {
    /// 创建空的数据库（不扫描）。
    pub fn new_empty(project_root: &str) -> Self {
        Self {
            project_root: PathBuf::from(project_root),
            assets_dir: "assets".into(),
            entries: HashMap::new(),
            path_to_uuid: HashMap::new(),
        }
    }

    /// 创建并扫描，自动生成缺失的 .meta 文件。
    pub fn open(project_root: &str) -> Result<Self, DatabaseError> {
        let mut db = Self::new_empty(project_root);
        db.scan()?;
        Ok(db)
    }

    /// 返回 assets 目录的绝对路径。
    fn assets_path(&self) -> PathBuf {
        self.project_root.join(&self.assets_dir)
    }

    /// 全量扫描：walkdir 遍历 assets/，对每个无 .meta 的文件自动生成。
    pub fn scan(&mut self) -> Result<ScanReport, DatabaseError> {
        let mut report = ScanReport::default();

        // 保存旧路径集合用于检测删除
        let old_paths: HashSet<String> = self.path_to_uuid.keys().cloned().collect();

        // 清空现有索引
        self.entries.clear();
        self.path_to_uuid.clear();

        let assets_path = self.assets_path();
        if !assets_path.exists() {
            // assets 目录不存在，创建它
            if let Err(e) = std::fs::create_dir_all(&assets_path) {
                report.errors.push(format!("Cannot create assets dir: {e}"));
                return Ok(report);
            }
        }

        // 收集磁盘上所有资产文件（非 .meta、非隐藏）
        let mut disk_files: HashSet<String> = HashSet::new();

        for entry in WalkDir::new(&assets_path)
            .into_iter()
            .filter_entry(|e| {
                // 跳过隐藏目录（如 .git）
                let name = e.file_name().to_str().unwrap_or("");
                !name.starts_with('.')
            })
        {
            let entry = match entry {
                Ok(e) => e,
                Err(e) => {
                    report.errors.push(format!("WalkDir error: {e}"));
                    continue;
                }
            };

            let path = entry.path();

            // 跳过 .meta 文件
            if path.extension().is_some_and(|ext| ext == "meta") {
                continue;
            }

            // 跳过目录本身（只处理文件）
            if path.is_dir() {
                continue;
            }

            // 计算相对路径（如 "assets/models/hero.glb"）
            let rel_path = match path.strip_prefix(&self.project_root) {
                Ok(p) => p.to_string_lossy().replace('\\', "/"),
                Err(_) => continue,
            };

            disk_files.insert(rel_path.clone());

            // 检查是否有对应的 .meta 文件
            let meta_path = meta::meta_path_for(path);
            let asset_meta = if meta_path.exists() {
                // 从磁盘读取已有的 .meta 文件
                match meta::read_meta(&meta_path) {
                    Ok(m) => m,
                    Err(e) => {
                        report.errors.push(format!("Cannot read meta for {rel_path}: {e}"));
                        continue;
                    }
                }
            } else {
                // 生成新的 .meta 文件并直接使用返回值（避免重复读盘）
                match meta::create_meta_for(path) {
                    Ok(m) => {
                        report.new_assets += 1;
                        m
                    }
                    Err(e) => {
                        report.errors.push(format!("Cannot create meta for {rel_path}: {e}"));
                        continue;
                    }
                }
            };

            let file_size = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
            let db_entry = DatabaseEntry {
                uuid: asset_meta.uuid.clone(),
                path: rel_path.clone(),
                asset_type: asset_meta.asset_type,
                dependencies: asset_meta.dependencies.clone(),
                file_size,
            };
            self.path_to_uuid.insert(rel_path.clone(), asset_meta.uuid.clone());
            self.entries.insert(asset_meta.uuid.clone(), db_entry);
        }

        // 扫描依赖（需要所有条目已注册）
        self.scan_all_dependencies();

        // 检测已删除的文件（在旧索引中但不在磁盘上）
        for old_path in &old_paths {
            if !disk_files.contains(old_path.as_str()) {
                // 清理孤立的 .meta 文件
                let abs_path = self.project_root.join(old_path);
                let meta_path = meta::meta_path_for(&abs_path);
                if meta_path.exists() {
                    let _ = std::fs::remove_file(&meta_path);
                }
                report.removed += 1;
            }
        }

        Ok(report)
    }

    /// 增量刷新（重新扫描）。
    pub fn refresh(&mut self) -> ScanReport {
        match self.scan() {
            Ok(report) => report,
            Err(e) => {
                let mut report = ScanReport::default();
                report.errors.push(format!("Refresh failed: {e}"));
                report
            }
        }
    }

    /// 扫描所有资产的依赖关系。
    fn scan_all_dependencies(&mut self) {
        // 收集所有路径和 UUID
        let entries_snapshot: Vec<(String, String, PathBuf)> = self
            .entries
            .values()
            .map(|e| (e.uuid.clone(), e.path.clone(), self.project_root.join(&e.path)))
            .collect();

        for (uuid, _path, abs_path) in &entries_snapshot {
            let deps = dependency_scanner::scan_dependencies(
                abs_path,
                &self.project_root,
                &self.path_to_uuid,
            );
            if let Some(entry) = self.entries.get_mut(uuid) {
                entry.dependencies = deps;
            }
        }

        // 收集需要同步到磁盘的依赖更新，放入后台线程写入。
        // 内存索引已经更新完毕，磁盘写入不阻塞调用方。
        let mut pending_writes: Vec<(PathBuf, crate::meta::AssetMeta)> = Vec::new();
        for entry in self.entries.values() {
            let abs_path = self.project_root.join(&entry.path);
            let meta_path = meta::meta_path_for(&abs_path);
            if let Ok(mut asset_meta) = meta::read_meta(&meta_path) {
                if asset_meta.dependencies != entry.dependencies {
                    asset_meta.dependencies = entry.dependencies.clone();
                    pending_writes.push((meta_path, asset_meta));
                }
            }
        }
        if !pending_writes.is_empty() {
            Self::spawn_dependency_meta_writes(pending_writes);
        }

        // 循环依赖检测：扫描完成后检查 Prefab 之间是否存在环路
        let all_metas = self.all_metas();
        let cycles = dependency_scanner::check_prefab_cycle(&all_metas);
        if !cycles.is_empty() {
            for cycle in &cycles {
                let chain = cycle.join(" → ");
                log::error!("[asset] cycle dependency detected: {}", chain);
            }
        }
    }

    /// 在后台线程中写入已更新的依赖 .meta 文件。
    ///
    /// 内存索引已同步更新，磁盘写入不阻塞调用方；
    /// 线程结束后自动 join（`JoinHandle` drop 时不等待）。
    fn spawn_dependency_meta_writes(
        writes: Vec<(PathBuf, crate::meta::AssetMeta)>,
    ) {
        std::thread::spawn(move || {
            for (meta_path, asset_meta) in writes {
                let _ = meta::write_meta(&meta_path, &asset_meta);
            }
        });
    }

    /// 按 UUID 查询条目。
    pub fn entry_by_uuid(&self, uuid: &str) -> Option<&DatabaseEntry> {
        self.entries.get(uuid)
    }

    /// 按路径查询条目。
    pub fn entry_by_path(&self, path: &str) -> Option<&DatabaseEntry> {
        self.path_to_uuid
            .get(path)
            .and_then(|uuid| self.entries.get(uuid))
    }

    /// 返回所有条目的迭代器。
    pub fn all_entries(&self) -> impl Iterator<Item = &DatabaseEntry> {
        self.entries.values()
    }

    /// 按类型过滤条目。
    pub fn entries_of_type(&self, ty: AssetTypeKind) -> Vec<&DatabaseEntry> {
        self.entries.values().filter(|e| e.asset_type == ty).collect()
    }

    /// 获取指定路径下的直接子条目（用于 AssetBrowser 目录浏览）。
    pub fn entries_in_directory(&self, dir: &str) -> Vec<&DatabaseEntry> {
        let dir_normalized = dir.replace('\\', "/");
        self.entries
            .values()
            .filter(|e| {
                // 条目的父目录匹配指定目录
                if let Some(parent) = Path::new(&e.path).parent() {
                    let parent_str = parent.to_string_lossy().replace('\\', "/");
                    parent_str == dir_normalized
                } else {
                    false
                }
            })
            .collect()
    }

    /// 获取依赖闭包（递归收集所有依赖）。
    pub fn dependency_chain(&self, uuid: &str) -> Vec<&DatabaseEntry> {
        let mut visited = HashSet::new();
        let mut result = Vec::new();
        self.collect_dependencies(uuid, &mut visited, &mut result);
        result
    }

    fn collect_dependencies<'a>(
        &'a self,
        uuid: &str,
        visited: &mut HashSet<String>,
        result: &mut Vec<&'a DatabaseEntry>,
    ) {
        if visited.contains(uuid) {
            return;
        }
        visited.insert(uuid.to_string());

        if let Some(entry) = self.entries.get(uuid) {
            result.push(entry);
            let deps: Vec<String> = entry.dependencies.clone();
            for dep_uuid in deps {
                self.collect_dependencies(&dep_uuid, visited, result);
            }
        }
    }

    /// 返回 path_to_uuid 索引的引用（供依赖扫描器使用）。
    pub fn path_to_uuid(&self) -> &HashMap<String, String> {
        &self.path_to_uuid
    }

    /// 返回项目根目录。
    pub fn project_root(&self) -> &Path {
        &self.project_root
    }

    /// 构建所有资源的 UUID→AssetMeta 映射（用于循环依赖检测等）。
    ///
    /// 从内存中的 `entries` 构建，不重新读取磁盘 .meta 文件。
    pub fn all_metas(&self) -> HashMap<String, crate::meta::AssetMeta> {
        self.entries
            .values()
            .map(|e| {
                (
                    e.uuid.clone(),
                    crate::meta::AssetMeta {
                        version: 1,
                        uuid: e.uuid.clone(),
                        asset_type: e.asset_type,
                        dependencies: e.dependencies.clone(),
                        import_settings: serde_json::Value::Null,
                    },
                )
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn setup_test_project() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let assets = dir.path().join("assets");
        fs::create_dir_all(&assets).unwrap();
        dir
    }

    #[test]
    fn scan_empty_assets_dir() {
        let dir = setup_test_project();
        let mut db = AssetDatabase::new_empty(dir.path().to_str().unwrap());
        let report = db.scan().unwrap();
        assert_eq!(report.new_assets, 0);
        assert_eq!(report.removed, 0);
        assert!(db.all_entries().count() == 0);
    }

    #[test]
    fn scan_generates_meta_for_new_files() {
        let dir = setup_test_project();
        let assets = dir.path().join("assets");
        fs::create_dir_all(assets.join("models")).unwrap();
        fs::write(assets.join("models/hero.glb"), b"fake glb data").unwrap();

        let mut db = AssetDatabase::new_empty(dir.path().to_str().unwrap());
        let report = db.scan().unwrap();

        assert_eq!(report.new_assets, 1);
        assert_eq!(db.all_entries().count(), 1);

        // 验证 .meta 文件已创建
        let meta_path = assets.join("models/hero.glb.meta");
        assert!(meta_path.exists());
    }

    #[test]
    fn scan_detects_removed_files() {
        let dir = setup_test_project();
        let assets = dir.path().join("assets");
        let file_path = assets.join("test.png");
        fs::write(&file_path, b"fake png").unwrap();

        let mut db = AssetDatabase::new_empty(dir.path().to_str().unwrap());
        db.scan().unwrap();
        assert_eq!(db.all_entries().count(), 1);

        // 删除文件后重新扫描
        fs::remove_file(&file_path).unwrap();
        let report = db.scan().unwrap();
        assert_eq!(report.removed, 1);
        assert_eq!(db.all_entries().count(), 0);
    }
}
