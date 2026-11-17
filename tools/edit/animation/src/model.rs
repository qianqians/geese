//! 模型加载、每帧动画推进，以及子网格替换的状态。
//!
//! 一个 glTF 文件经 `scene::import_scene` 变成完整的 [`Scene`]（网格、蒙皮、材质、
//! 节点层级、动画剪辑一次到位），本模块负责：
//! - 把 `animations` 摘成 UI 用的剪辑列表；
//! - 用 [`AnimationPlayer`] 每帧推进时间并调用 `Scene::update_animation`
//!   （它会采样剪辑到节点上，再重算世界矩阵与 `joint_matrices`）；
//! - 子网格替换的**状态**：源文件容器、每个目标的原网格、以及所有被换下来的网格。
//!
//! 替换本身的算法在 [`crate::swap`]，这里只保存状态并保证「离开渲染集的网格不释放」。

use std::path::{Path, PathBuf};

use render::ModelMesh;
use scene::{AnimationPlayer, Scene};

use crate::swap::{self, SubMeshView};

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

/// 源文件：只当网格 / 材质 / 蒙皮的容器，不参与渲染与动画。
pub struct SourceModel {
    pub name: String,
    pub path: PathBuf,
    pub scene: Scene,
    /// 源材质库整库 `append_library` 进目标材质库时拿到的偏移。
    pub material_offset: usize,
}

/// 一次替换的记录：目标对象 + 被换下来的原网格。
pub struct SwapRecord {
    pub target: usize,
    /// 原网格。恢复时换回去，同时它也让旧顶点数组的地址不会被复用。
    original: ModelMesh,
    /// 源文件名，UI 上显示「源 xxx.glb」。
    pub source_name: String,
    /// 源网格的标题（`节点 / 网格`）。
    pub source_mesh: String,
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
    /// 当前选作替换源的模型文件。
    source: Option<SourceModel>,
    /// 每个被替换的子网格一条记录（同一目标重复替换时只保留一条）。
    swaps: Vec<SwapRecord>,
    /// 所有离开渲染集的网格都在这里，**永不释放**。
    ///
    /// render 的 mesh 缓冲缓存键是「顶点指针 + 长度」，只插不淘汰；若这些顶点数组被释放、
    /// 之后新分配的数组复用了同一段地址，就会命中旧键渲染出上一套几何。代价是一点内存。
    retired_meshes: Vec<ModelMesh>,
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
            source: None,
            swaps: Vec::new(),
            retired_meshes: Vec::new(),
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

    // ── 子网格替换 ──

    /// 当前模型的子网格列表（含每个对象的展示信息）。
    pub fn submeshes(&self) -> Vec<SubMeshView> {
        (0..self.scene.objects.len())
            .filter_map(|index| swap::submesh_view(&self.scene, index))
            .collect()
    }

    /// 源文件的子网格列表；还没选源文件时为空。
    pub fn source_submeshes(&self) -> Vec<SubMeshView> {
        let Some(source) = &self.source else {
            return Vec::new();
        };
        (0..source.scene.objects.len())
            .filter_map(|index| swap::submesh_view(&source.scene, index))
            .collect()
    }

    pub fn source_name(&self) -> Option<&str> {
        self.source.as_ref().map(|source| source.name.as_str())
    }

    /// 载入替换源文件；同一路径重复载入时复用已有容器，避免材质库反复膨胀。
    pub fn load_source(&mut self, path: &Path) -> Result<(), String> {
        if !path.is_file() {
            return Err(format!("文件不存在: {}", path.display()));
        }
        if self
            .source
            .as_ref()
            .is_some_and(|source| source.path == path)
        {
            return Ok(());
        }

        let source_scene = scene::import_scene(path.display().to_string(), MAX_OBJECTS, MAX_DEPTH)
            .map_err(|e| format!("{e}"))?;
        // 整库搬一次：纹理句柄由 append_library 重映射，源网格的材质句柄只需加偏移。
        // 源场景保留自己那份材质库，好让源网格列表也能显示材质名。
        let material_offset = self
            .scene
            .materials
            .append_library(source_scene.materials.clone());

        self.source = Some(SourceModel {
            name: path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| path.display().to_string()),
            path: path.to_path_buf(),
            scene: source_scene,
            material_offset,
        });
        Ok(())
    }

    /// 目标子网格是否已被替换。
    pub fn is_swapped(&self, target: usize) -> bool {
        self.swaps.iter().any(|record| record.target == target)
    }

    /// 本次会话是否有任何替换（决定「全部恢复」是否可用）。
    pub fn has_swaps(&self) -> bool {
        !self.swaps.is_empty()
    }

    /// 该目标替换自哪个源网格（`源文件 · 节点 / 网格`），未替换时为 `None`。
    pub fn swap_origin(&self, target: usize) -> Option<String> {
        self.swaps
            .iter()
            .find(|record| record.target == target)
            .map(|record| format!("源 {} · {}", record.source_name, record.source_mesh))
    }

    /// 把 `target` 换成源文件里 `source_object` 的网格，动画继续照旧播。
    ///
    /// 同一目标重复替换时先恢复再换，所以每个目标最多一条记录。
    pub fn apply_swap(&mut self, target: usize, source_object: usize) -> Result<(), String> {
        if self.source.is_none() {
            return Err("还没有选择源文件".to_string());
        }
        if self.is_swapped(target) {
            self.restore(target);
        }

        let source = self.source.as_ref().expect("上面已检查");
        let material_offset = source.material_offset;
        let source_mesh = swap::submesh_view(&source.scene, source_object)
            .map(|view| view.title())
            .unwrap_or_else(|| format!("网格 #{source_object}"));
        let source_name = source.name.clone();

        let original = swap::apply(
            &mut self.scene,
            target,
            &source.scene,
            source_object,
            material_offset,
        )
        .map_err(|err| err.to_string())?;

        self.swaps.push(SwapRecord {
            target,
            original,
            source_name,
            source_mesh,
        });
        Ok(())
    }

    /// 恢复某个目标子网格；返回是否确实恢复了。
    pub fn restore(&mut self, target: usize) -> bool {
        let Some(position) = self.swaps.iter().position(|record| record.target == target) else {
            return false;
        };
        let record = self.swaps.remove(position);

        match swap::restore(&mut self.scene, target, record.original) {
            Some(swapped_out) => {
                self.retired_meshes.push(swapped_out);
                true
            }
            None => false,
        }
    }

    /// 恢复本次会话的所有替换，返回恢复的数量。
    pub fn restore_all(&mut self) -> usize {
        let targets: Vec<usize> = self.swaps.iter().map(|record| record.target).collect();
        targets.into_iter().filter(|target| self.restore(*target)).count()
    }
}
