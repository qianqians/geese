//! 面板层级管理器。
//!
//! 管理浮动面板的可见性、透明度和 z-顺序，支持：
//! - Tab 键切换所有非 pinned 面板显隐
//! - Play 模式下自动降低全局透明度
//! - 右键面板标题钉住/取消钉住

use std::collections::{HashMap, HashSet};

/// 面板层级标识，数值越大层级越高（渲染越靠前）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PanelLayer {
    /// 场景视口层（最底层）
    SceneView = 0,
    /// 资源浏览器
    AssetBrowser = 1,
    /// 层级面板
    Hierarchy = 2,
    /// Inspector 面板
    Inspector = 3,
    /// 浮动工具栏（最顶层）
    Toolbar = 4,
    /// 动画面板
    Animation = 5,
}

/// 面板层级管理器。
#[derive(Debug, Clone)]
pub struct PanelLayerManager {
    /// 每个面板的可见性
    pub visibility: HashMap<PanelLayer, bool>,
    /// 全局面板透明度（0.0-1.0）
    pub global_alpha: f32,
    /// 钉住的面板（不随 Tab 切换隐藏）
    pub pinned: HashSet<PanelLayer>,
}

impl Default for PanelLayerManager {
    fn default() -> Self {
        let mut visibility = HashMap::new();
        visibility.insert(PanelLayer::SceneView, true);
        visibility.insert(PanelLayer::AssetBrowser, true);
        visibility.insert(PanelLayer::Hierarchy, true);
        visibility.insert(PanelLayer::Inspector, true);
        visibility.insert(PanelLayer::Toolbar, true);
        visibility.insert(PanelLayer::Animation, false);  // 默认隐藏动画面板

        Self {
            visibility,
            global_alpha: 0.85,
            pinned: HashSet::new(),
        }
    }
}

impl PanelLayerManager {
    /// 切换所有非 pinned 面板的显隐。
    pub fn toggle_all(&mut self) {
        // 检查当前是否有可见的非 pinned 面板
        let any_visible = self.visibility.iter().any(|(layer, &vis)| {
            vis && !self.pinned.contains(layer)
        });

        for (layer, vis) in self.visibility.iter_mut() {
            if !self.pinned.contains(layer) {
                *vis = !any_visible;
            }
        }
    }

    /// 设置面板可见性。
    pub fn set_visible(&mut self, layer: PanelLayer, visible: bool) {
        self.visibility.insert(layer, visible);
    }

    /// 获取面板可见性。
    pub fn is_visible(&self, layer: &PanelLayer) -> bool {
        self.visibility.get(layer).copied().unwrap_or(true)
    }

    /// 切换指定面板的可见性。
    pub fn toggle_visible(&mut self, layer: PanelLayer) {
        let current = self.is_visible(&layer);
        self.set_visible(layer, !current);
    }

    /// 钉住面板。
    pub fn pin(&mut self, layer: PanelLayer) {
        self.pinned.insert(layer);
    }

    /// 取消钉住。
    pub fn unpin(&mut self, layer: &PanelLayer) {
        self.pinned.remove(layer);
    }

    /// 切换钉住状态。
    pub fn toggle_pin(&mut self, layer: PanelLayer) {
        if self.pinned.contains(&layer) {
            self.pinned.remove(&layer);
        } else {
            self.pinned.insert(layer);
        }
    }

    /// 设置 Play 模式透明度。
    pub fn set_play_alpha(&mut self) {
        self.global_alpha = 0.3;
    }

    /// 恢复编辑模式透明度。
    pub fn set_edit_alpha(&mut self) {
        self.global_alpha = 0.85;
    }

    /// 是否有任何可见面板在指定层级之上（用于输入遮挡检测）。
    pub fn has_visible_above(&self, layer: &PanelLayer) -> bool {
        let layer_val = *layer as u8;
        self.visibility.iter().any(|(l, &vis)| {
            vis && (*l as u8) > layer_val
        })
    }
}
