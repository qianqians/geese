//! 虚拟文件系统（VFS）。
//!
//! 提供基于挂载点的路径抽象：
//! - [`Vfs`]：管理多个挂载点，将虚拟路径解析为物理路径
//! - [`MountPoint`]：虚拟前缀 → 物理根目录的映射
//!
//! 最长前缀匹配：虚拟路径 `/assets/textures/hero.png`
//! 匹配挂载点 `/assets/` → `./game_data/`，解析为 `./game_data/textures/hero.png`。

use std::path::{Path, PathBuf};

// ---------------------------------------------------------------------------
// MountPoint
// ---------------------------------------------------------------------------

/// 虚拟路径挂载点。
///
/// 将虚拟路径前缀（如 `/assets/`）映射到物理文件系统路径（如 `./game_data/`）。
#[derive(Clone, Debug)]
pub struct MountPoint {
    /// 虚拟路径前缀（例如 `/assets/`），以 `/` 开头和结尾。
    pub prefix: String,
    /// 物理文件系统根目录（例如 `./game_data/`）。
    pub physical_root: PathBuf,
    /// 是否允许写入操作。
    pub writable: bool,
}

impl MountPoint {
    /// 创建新的挂载点。
    ///
    /// `prefix` 会自动规范化为以 `/` 开头和结尾。
    pub fn new(prefix: impl Into<String>, physical_root: impl Into<PathBuf>, writable: bool) -> Self {
        let mut prefix: String = prefix.into();
        // 规范化：确保以 / 开头
        if !prefix.starts_with('/') {
            prefix.insert(0, '/');
        }
        // 规范化：确保以 / 结尾
        if !prefix.ends_with('/') {
            prefix.push('/');
        }
        Self {
            prefix,
            physical_root: physical_root.into(),
            writable,
        }
    }

    /// 检查虚拟路径是否匹配此前缀。
    fn matches(&self, virtual_path: &str) -> bool {
        virtual_path.starts_with(&self.prefix)
    }

    /// 前缀长度（用于最长前缀匹配排序）。
    fn prefix_len(&self) -> usize {
        self.prefix.len()
    }
}

// ---------------------------------------------------------------------------
// Vfs
// ---------------------------------------------------------------------------

/// 虚拟文件系统。
///
/// 持有按优先级排序的挂载点列表（后挂载的优先级更高）。
/// 路径解析采用最长前缀匹配。
#[derive(Clone, Debug, Default)]
pub struct Vfs {
    mounts: Vec<MountPoint>,
}

impl Vfs {
    /// 创建空的虚拟文件系统。
    pub fn new() -> Self {
        Self::default()
    }

    /// 添加挂载点。
    ///
    /// 后添加的挂载点在解析时优先级更高（列表尾部优先）。
    pub fn mount(&mut self, mount: MountPoint) {
        self.mounts.push(mount);
    }

    /// 移除指定前缀的挂载点。返回是否成功移除。
    pub fn unmount(&mut self, prefix: &str) -> bool {
        let len_before = self.mounts.len();
        self.mounts.retain(|m| m.prefix != prefix);
        self.mounts.len() < len_before
    }

    /// 获取挂载点数量。
    pub fn mount_count(&self) -> usize {
        self.mounts.len()
    }

    /// 将虚拟路径解析为物理路径。
    ///
    /// 使用最长前缀匹配：在匹配的挂载点中选择前缀最长的。
    /// 返回 `None` 表示没有匹配的挂载点。
    ///
    /// # 示例
    ///
    /// ```
    /// use std::path::Path;
    /// use vfs::{Vfs, MountPoint};
    ///
    /// let mut vfs = Vfs::new();
    /// vfs.mount(MountPoint::new("/assets/", "./game_data/", false));
    /// vfs.mount(MountPoint::new("/assets/textures/", "./hd_textures/", false));
    ///
    /// // 最长前缀匹配：/assets/textures/ 优先于 /assets/
    /// assert_eq!(
    ///     vfs.resolve("/assets/textures/hero.png"),
    ///     Some(Path::new("./hd_textures/hero.png").to_path_buf())
    /// );
    /// assert_eq!(
    ///     vfs.resolve("/assets/models/hero.glb"),
    ///     Some(Path::new("./game_data/models/hero.glb").to_path_buf())
    /// );
    /// ```
    pub fn resolve(&self, virtual_path: &str) -> Option<PathBuf> {
        // 从后往前遍历（后挂载的优先级更高），找最长前缀匹配
        let mut best: Option<&MountPoint> = None;
        let mut best_len = 0;

        for mount in self.mounts.iter().rev() {
            if mount.matches(virtual_path) {
                let len = mount.prefix_len();
                if len > best_len {
                    best = Some(mount);
                    best_len = len;
                }
            }
        }

        best.map(|mount| {
            let relative = &virtual_path[mount.prefix.len()..];
            mount.physical_root.join(relative)
        })
    }

    /// 将物理路径反向解析为虚拟路径。
    ///
    /// 找到第一个匹配的挂载点，返回对应的虚拟路径。
    /// 返回 `None` 表示物理路径不在任何挂载点下。
    pub fn to_virtual(&self, physical: &Path) -> Option<String> {
        // 规范化物理路径
        let canonical = physical.canonicalize().ok()?;

        for mount in self.mounts.iter().rev() {
            let root = mount.physical_root.canonicalize().ok()?;
            if let Ok(relative) = canonical.strip_prefix(&root) {
                let relative_str = relative.to_string_lossy().replace('\\', "/");
                return Some(format!("{}{}", mount.prefix, relative_str));
            }
        }
        None
    }

    /// 列出所有已注册的挂载点。
    pub fn list_mounts(&self) -> &[MountPoint] {
        &self.mounts
    }
}

// ---------------------------------------------------------------------------
// 测试
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    #[test]
    fn empty_vfs_resolves_nothing() {
        let vfs = Vfs::new();
        assert_eq!(vfs.resolve("/anything.txt"), None);
        assert_eq!(vfs.mount_count(), 0);
    }

    #[test]
    fn mount_and_resolve() {
        let mut vfs = Vfs::new();
        vfs.mount(MountPoint::new("/assets/", "./data/", false));

        let resolved = vfs.resolve("/assets/models/hero.glb");
        assert_eq!(resolved, Some(PathBuf::from("./data/models/hero.glb")));
    }

    #[test]
    fn longest_prefix_match() {
        let mut vfs = Vfs::new();
        vfs.mount(MountPoint::new("/assets/", "./data/", false));
        vfs.mount(MountPoint::new("/assets/textures/", "./hd_textures/", false));

        // 子路径匹配更长的前缀
        assert_eq!(
            vfs.resolve("/assets/textures/hero.png"),
            Some(PathBuf::from("./hd_textures/hero.png"))
        );
        // 父路径匹配短前缀
        assert_eq!(
            vfs.resolve("/assets/models/hero.glb"),
            Some(PathBuf::from("./data/models/hero.glb"))
        );
    }

    #[test]
    fn later_mount_wins_same_prefix() {
        let mut vfs = Vfs::new();
        vfs.mount(MountPoint::new("/assets/", "./old_data/", false));
        vfs.mount(MountPoint::new("/assets/", "./new_data/", false));

        // 后挂载的同前缀覆盖先挂载的
        assert_eq!(
            vfs.resolve("/assets/file.txt"),
            Some(PathBuf::from("./new_data/file.txt"))
        );
    }

    #[test]
    fn unmount_removes_prefix() {
        let mut vfs = Vfs::new();
        vfs.mount(MountPoint::new("/temp/", "./tmp/", true));
        assert_eq!(vfs.mount_count(), 1);

        assert!(vfs.unmount("/temp/"));
        assert_eq!(vfs.mount_count(), 0);
        assert_eq!(vfs.resolve("/temp/file.txt"), None);
    }

    #[test]
    fn prefix_normalization() {
        let mount = MountPoint::new("assets", "data", false);
        assert_eq!(mount.prefix, "/assets/");
    }

    #[test]
    fn to_virtual_roundtrip() {
        let temp_dir = env::temp_dir().join("vfs_test_roundtrip");
        let _ = std::fs::create_dir_all(&temp_dir);

        let mut vfs = Vfs::new();
        vfs.mount(MountPoint::new("/test/", &temp_dir, false));

        // 在挂载点下写一个文件
        let test_file = temp_dir.join("subdir").join("hello.txt");
        let _ = std::fs::create_dir_all(test_file.parent().unwrap());
        std::fs::write(&test_file, b"hello").unwrap();

        let virtual_path = vfs.to_virtual(&test_file);
        assert!(virtual_path.is_some(), "should resolve to virtual path");
        // 反向解析后再正向解析，应得到同一文件
        let roundtrip = vfs.resolve(&virtual_path.unwrap());
        assert!(roundtrip.is_some());
    }
}
