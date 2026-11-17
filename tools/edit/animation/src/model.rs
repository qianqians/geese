//! 模型加载与每帧动画推进。
//!
//! 一个 glTF 文件经 `scene::import_scene` 变成完整的 [`Scene`]（网格、蒙皮、材质、
//! 节点层级、动画剪辑一次到位），本模块只负责：
//! - 把 `animations` 摘成 UI 用的剪辑列表；
//! - 用 [`AnimationPlayer`] 每帧推进时间并调用 `Scene::update_animation`
//!   （它会采样剪辑到节点上，再重算世界矩阵与 `joint_matrices`）。

use std::path::{Path, PathBuf};

use scene::{AnimationPlayer, Scene};

/// 导入上限：与 `scene::AvatarManifest::from_gltf` 保持一致。
const MAX_OBJECTS: usize = 8;
const MAX_DEPTH: usize = 6;

/// 一个动画剪辑的展示信息。
pub struct ClipInfo {
    /// 在 `Scene::animations` 中的下标。
    pub index: usize,
    pub name: String,
    /// 时长（秒），来自最后一个关键帧。
    pub duration: f32,
}

/// 已加载的模型。
pub struct LoadedModel {
    /// 显示用文件名。
    pub name: String,
    pub path: PathBuf,
    pub scene: Scene,
    /// `Scene::render_queue` 需要的队列构造器；兜底材质取自模型自己的材质库。
    pub scene_renderer: render::SceneRenderer,
    pub clips: Vec<ClipInfo>,
    /// 单个蒙皮里最多的关节数——状态栏拿它和 `render::MAX_JOINTS` 比。
    pub joint_count: usize,
    player: AnimationPlayer,
}

impl LoadedModel {
    /// 加载一个 glTF/GLB 文件，并采样到第 0 帧。
    pub fn load(path: &Path) -> Result<Self, String> {
        if !path.is_file() {
            return Err(format!("文件不存在: {}", path.display()));
        }

        let scene = scene::import_scene(path.display().to_string(), MAX_OBJECTS, MAX_DEPTH)
            .map_err(|e| format!("{e}"))?;

        let clips = scene
            .animations
            .iter()
            .enumerate()
            .map(|(index, clip)| ClipInfo {
                index,
                name: clip
                    .name
                    .clone()
                    .unwrap_or_else(|| format!("clip_{index}")),
                duration: clip.duration,
            })
            .collect::<Vec<_>>();

        let joint_count = scene
            .skins
            .iter()
            .map(|skin| skin.joints.len())
            .max()
            .unwrap_or(0);

        let scene_renderer =
            render::SceneRenderer::new(scene::material_asset::default_material(&scene.materials));

        let mut model = Self {
            name: path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| path.display().to_string()),
            path: path.to_path_buf(),
            scene,
            scene_renderer,
            clips,
            joint_count,
            // 剪辑可能在下面被立刻替换；无剪辑时 `update_animation` 会直接返回。
            player: AnimationPlayer::new(0),
        };

        // 先采样第 0 帧，加载完就能看到绑定姿势（甚至第一帧动画）。
        model.tick(0.0, false);
        Ok(model)
    }

    /// 当前剪辑下标。
    pub fn current_clip(&self) -> usize {
        self.player.clip
    }

    /// 当前剪辑时长；没有剪辑时为 0。
    pub fn duration(&self) -> f32 {
        self.clips
            .get(self.player.clip)
            .map(|clip| clip.duration)
            .unwrap_or(0.0)
    }

    /// 播放位置（秒）。
    pub fn time(&self) -> f32 {
        self.player.time
    }

    /// 推进一帧：`playing` 为假时传 `dt = 0`，只重新采样当前时间。
    pub fn tick(&mut self, dt: f32, playing: bool) {
        self.player.playing = playing;
        self.scene
            .update_animation(&mut self.player, if playing { dt } else { 0.0 });
    }

    /// 定位到指定时间（时间轴拖动）。
    pub fn seek(&mut self, time: f32) {
        self.player.time = time.clamp(0.0, self.duration());
        self.scene.update_animation(&mut self.player, 0.0);
    }

    /// 切换剪辑并回到 0 秒。
    pub fn select_clip(&mut self, index: usize) {
        if index >= self.clips.len() {
            return;
        }
        self.player = AnimationPlayer::new(index);
        self.scene.update_animation(&mut self.player, 0.0);
    }
}
