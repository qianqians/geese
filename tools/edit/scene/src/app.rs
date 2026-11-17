//! 三栏 UI、拖放添加模型、场景重建与自动保存。
//!
//! 中央的 3D 画面由 `main.rs` 每帧先渲染到这里（面板只是叠在它上面），本模块只负责
//! UI、输入落点和「什么时候重建场景」。

use std::path::PathBuf;
use std::time::Instant;

use camera::{Camera, CameraMode};
use cgmath::Point3;
use edit_prefs::Prefs;
use egui::{Align, Color32, Layout, RichText};
use render::Light;
use scene::Scene;
use scene::manifest::Environment;

use crate::doc::{self, DocError, SceneDoc};
use crate::models::{self, ModelEntry, ModelGroup};
use crate::pick;

/// 记录文件：`<工作目录>/.data/scene.json`。
const PREFS_APP: &str = "scene";
/// 记录键：上次编辑的场景文件。
const PREFS_KEY: &str = "scene";

/// 单个模型导入上限（节点数 / 递归深度），与其它工具同档。
const MAX_OBJECTS: usize = 64;
const MAX_DEPTH: usize = 8;

const WARN: Color32 = Color32::from_rgb(0xff, 0xb4, 0x54);
const ERR: Color32 = Color32::from_rgb(0xff, 0x6b, 0x6b);
const DIM: Color32 = Color32::from_rgb(0x8b, 0x8b, 0x98);
const ACCENT: Color32 = Color32::from_rgb(0x4f, 0x8c, 0xff);

/// 一份已构建好的场景，连同渲染它需要的队列构造器与状态栏计数。
pub struct BuiltScene {
    pub scene: Scene,
    pub scene_renderer: render::SceneRenderer,
    pub model_count: usize,
    pub object_count: usize,
    pub prefab_count: usize,
    pub triangle_count: usize,
    /// 可见内容的包围盒（由对象自己的 aabb 并出来）：`Scene::bounds` 是加载器的全局 AABB，
    /// 空场景/只有微小程序时会给出 ±100 的默认盒，拿它取景会把相机推到几百单位外。
    pub bounds: Option<math::AABB>,
}

/// UI 里触发、需要跳出闭包再执行的动作（避免与 `self` 的可变借用打架）。
enum Action {
    New,
    Open,
    Save,
}

pub struct SceneApp {
    /// 启动参数指定的工作目录；记录、模型列表、文件对话框都围绕它。
    workdir: PathBuf,
    prefs: Prefs,
    doc: SceneDoc,
    built: Option<BuiltScene>,
    /// 最近一次应用失败的原因。
    error: Option<DocError>,
    /// 左栏模型（按目录分组）。
    groups: Vec<ModelGroup>,
    /// 一次性提示（加模型成功、保存成功等）。
    status: Option<String>,
    camera: Camera,
    aspect: f32,
    /// 下一个场景建好后是否重新取景（打开/新建场景时为真，改 JSON 时为假，免得画面乱跳）。
    frame_on_next_build: bool,
    /// 已经为哪份文本弹过「另存为」，避免用户取消后每次应用都再弹一次。
    prompted_text: Option<String>,
    pub lights: Vec<Light>,
    pub ambient: [f32; 3],
}

impl SceneApp {
    pub fn new(workdir: PathBuf) -> Self {
        let prefs = Prefs::load_in(&workdir, PREFS_APP);
        let aspect = 16.0 / 9.0;
        Self {
            groups: models::scan_models(&workdir),
            doc: SceneDoc::blank(),
            built: None,
            error: None,
            status: None,
            camera: Camera::new(
                CameraMode::Orbit {
                    yaw: -0.7,
                    pitch: 0.35,
                    focal_point: Point3::new(0.0, 0.0, 0.0),
                    distance: 8.0,
                    min_distance: 0.2,
                    max_distance: 2000.0,
                },
                aspect,
            ),
            aspect,
            frame_on_next_build: true,
            prompted_text: None,
            prefs,
            lights: default_lights(),
            ambient: [0.12, 0.12, 0.15],
            workdir,
        }
    }

    /// 启动流程：上次记录的场景（文件仍在）→ 文件对话框 → 空状态（空白场景）。
    pub fn startup(&mut self) {
        let recorded = self
            .prefs
            .last_existing(PREFS_KEY)
            .map(|entry| entry.path.clone());
        match recorded {
            Some(path) => self.open_scene(path),
            None => self.open_dialog(),
        }
    }

    // ── 由 `main.rs` 每帧调用的对外接口 ──

    pub fn update_aspect(&mut self, width: u32, height: u32) {
        self.camera.update_aspect(width, height);
        self.aspect = self.camera.aspect;
    }

    pub fn orbit(&mut self, dx: f32, dy: f32) {
        self.camera.orbit(dx, dy);
    }

    pub fn pan(&mut self, dx: f32, dy: f32) {
        self.camera.pan(dx, dy);
    }

    pub fn zoom(&mut self, delta: f32) {
        self.camera.zoom(delta);
    }

    pub fn view_projection(&self) -> [[f32; 4]; 4] {
        self.camera.view_projection_raw()
    }

    pub fn camera_position(&self) -> [f32; 3] {
        self.camera.camera_position_raw()
    }

    pub fn built(&self) -> Option<&BuiltScene> {
        self.built.as_ref()
    }

    /// 该解析并重建场景了吗（防抖到点，或新建/打开/拖入模型后的立刻应用）。
    pub fn rebuild_due(&self, now: Instant) -> bool {
        self.doc.due(now)
    }

    /// 解析当前文本并重建场景；成功则按需自动保存。调用方负责先清 GPU 资源缓存。
    pub fn rebuild(&mut self) {
        let text = self.doc.take_attempt();
        let base_dir = self.doc.base_dir(&self.workdir);

        let manifest = match doc::parse_manifest(&text) {
            Ok(manifest) => manifest,
            Err(e) => {
                log::warn!("场景 JSON 解析失败：{e}");
                self.error = Some(e);
                return;
            }
        };

        let base = match base_dir.to_str() {
            Some(base) => base.to_string(),
            None => {
                self.error = Some(DocError {
                    message: format!("场景目录不是合法 UTF-8：{}", base_dir.display()),
                    line: None,
                });
                return;
            }
        };

        match scene::loader::load_scene_from_manifest(&manifest, &base, MAX_OBJECTS, MAX_DEPTH, None) {
            Ok(scene) => {
                let triangle_count = scene
                    .objects
                    .iter()
                    .map(|object| object.mesh.indices.len() / 3)
                    .sum();
                let bounds = union_bounds(&scene);
                self.built = Some(BuiltScene {
                    scene,
                    scene_renderer: render::SceneRenderer::new(render::Material::default()),
                    model_count: manifest.models.len(),
                    object_count: manifest.objects.len(),
                    prefab_count: manifest.prefab_instances.len(),
                    triangle_count,
                    bounds,
                });
                self.error = None;
                self.apply_environment(&manifest.environment);
                log::info!(
                    "场景重建成功（{}）：模型 {} 个，程序化物体 {} 个，三角面 {}",
                    self.doc.file_name(),
                    manifest.models.len(),
                    manifest.objects.len(),
                    triangle_count
                );
                if self.frame_on_next_build {
                    self.frame_scene();
                    self.frame_on_next_build = false;
                }
                self.autosave();
            }
            Err(e) => {
                log::warn!("加载场景失败：{e}");
                self.error = Some(DocError {
                    message: format!("加载场景失败：{e}"),
                    line: None,
                });
            }
        }
    }

    // ── 文件操作 ──

    fn new_scene(&mut self) {
        self.doc = SceneDoc::blank();
        self.frame_on_next_build = true;
        self.prompted_text = None;
        self.status = Some("新建空白场景".to_string());
    }

    fn open_dialog(&mut self) {
        let start = self.scenes_dir();
        let mut dialog = rfd::FileDialog::new().add_filter("场景 JSON", &["json"]);
        if start.is_dir() {
            dialog = dialog.set_directory(&start);
        } else {
            dialog = dialog.set_directory(&self.workdir);
        }
        if let Some(path) = dialog.pick_file() {
            self.open_scene(path);
        }
    }

    fn open_scene(&mut self, path: PathBuf) {
        match SceneDoc::open(path.clone()) {
            Ok(doc) => {
                self.doc = doc;
                self.frame_on_next_build = true;
                self.prompted_text = None;
                self.status = None;
                self.error = None;
                let name = path
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default();
                self.prefs.set(PREFS_KEY, name, &path);
            }
            Err(e) => self.error = Some(DocError { message: e, line: None }),
        }
    }

    fn save_now(&mut self) {
        if self.doc.path.is_none() {
            self.save_as_dialog();
            return;
        }
        match self.doc.save() {
            Ok(()) => {
                self.status = Some(format!("已保存 {}", self.doc.file_name()));
                if let Some(path) = self.doc.path.clone() {
                    self.prefs.set(PREFS_KEY, self.doc.file_name(), &path);
                }
            }
            Err(e) => self.error = Some(DocError { message: e, line: None }),
        }
    }

    fn save_as_dialog(&mut self) {
        let dir = self.scenes_dir();
        let mut dialog = rfd::FileDialog::new()
            .add_filter("场景 JSON", &["json"])
            .set_file_name(self.doc.file_name());
        dialog = if dir.is_dir() {
            dialog.set_directory(&dir)
        } else {
            dialog.set_directory(&self.workdir)
        };
        let Some(path) = dialog.save_file() else {
            return;
        };
        match self.doc.write_to(path.clone()) {
            Ok(()) => {
                self.status = Some(format!("已保存 {}", self.doc.file_name()));
                self.prefs.set(PREFS_KEY, self.doc.file_name(), &path);
            }
            Err(e) => self.error = Some(DocError { message: e, line: None }),
        }
    }

    fn scenes_dir(&self) -> PathBuf {
        self.workdir.join("assets/scenes")
    }

    /// 成功应用后才自动写回：有路径就写，没有就弹一次「另存为」（用户取消则不再纠缠）。
    fn autosave(&mut self) {
        if !self.doc.is_dirty() {
            return;
        }
        if self.doc.path.is_some() {
            if let Err(e) = self.doc.save() {
                self.error = Some(DocError { message: e, line: None });
            } else if let Some(path) = self.doc.path.clone() {
                self.prefs.set(PREFS_KEY, self.doc.file_name(), &path);
                self.status = Some(format!("已自动保存 {}", self.doc.file_name()));
            }
            return;
        }
        if self.prompted_text.as_deref() != Some(self.doc.text.as_str()) {
            self.prompted_text = Some(self.doc.text.clone());
            self.save_as_dialog();
        }
    }

    // ── 加模型 / 相机 ──

    /// 拖放落点处加一条 `models` 记录，并立刻应用（不等防抖）。
    fn place_model(&mut self, entry: &ModelEntry, point: [f32; 3]) {
        let base_dir = self.doc.base_dir(&self.workdir);
        match doc::insert_model(&self.doc.text, &base_dir, &entry.path, point) {
            Ok(text) => {
                self.doc.set_text(text, Instant::now());
                self.doc.request_apply();
                self.camera
                    .set_mode_focal_point(Point3::new(point[0], 0.0, point[2]));
                self.status = Some(format!("已加入 {}（{}）", entry.file_name(), point_summary(point)));
                self.prompted_text = None;
            }
            Err(e) => {
                self.error = Some(DocError {
                    message: format!("加入模型失败：{}（先把 JSON 改对）", e),
                    line: e.line,
                });
            }
        }
    }

    fn apply_environment(&mut self, environment: &Environment) {
        self.ambient = environment.ambient;
        self.lights.clear();
        for light in &environment.directional_lights {
            self.lights.push(Light::directional(
                light.direction,
                light.color,
                light.intensity,
            ));
        }
        if self.lights.is_empty() {
            // 场景没写光照时给一盏默认方向光，否则预览一片黑。
            self.lights = default_lights();
        }
    }

    /// 按场景包围盒重新取景。
    fn frame_scene(&mut self) {
        let Some(built) = &self.built else {
            return;
        };
        let (center, radius) = match built.bounds {
            Some(bounds) => {
                let size = bounds.size();
                let radius = size.x.max(size.y).max(size.z) * 0.5;
                (bounds.center(), radius)
            }
            None => (Point3::new(0.0, 0.0, 0.0), 3.0),
        };
        let radius = if radius.is_finite() && radius > 0.01 {
            radius.min(500.0)
        } else {
            3.0
        };
        // 很小的场景（一个小方块）按比例算出来的距离会把相机贴到物体脸上，给个下限。
        let distance = (radius * 2.6).max(2.5);
        self.camera = Camera::new(
            CameraMode::Orbit {
                yaw: -0.7,
                pitch: 0.35,
                focal_point: center,
                distance,
                min_distance: 0.1,
                max_distance: 2000.0,
            },
            self.aspect,
        );
    }

    fn refresh_models(&mut self) {
        self.groups = models::scan_models(&self.workdir);
        self.status = Some(format!("模型 {} 个", models::count_entries(&self.groups)));
    }

    // ── UI ──

    pub fn ui(&mut self, ctx: &egui::Context) {
        let save_shortcut = ctx.input_mut(|input| {
            input.consume_shortcut(&egui::KeyboardShortcut::new(
                egui::Modifiers::COMMAND,
                egui::Key::S,
            ))
        });

        egui::TopBottomPanel::top("scene_toolbar").show(ctx, |ui| self.toolbar_ui(ui));
        egui::TopBottomPanel::bottom("scene_status").show(ctx, |ui| self.status_ui(ui));
        egui::SidePanel::left("scene_models")
            .resizable(true)
            .default_width(230.0)
            .width_range(160.0..=460.0)
            .show(ctx, |ui| self.models_ui(ui));
        egui::SidePanel::right("scene_json")
            .resizable(true)
            .default_width(420.0)
            .width_range(240.0..=1000.0)
            .show(ctx, |ui| self.json_ui(ui));
        egui::CentralPanel::default()
            .frame(egui::Frame::none())
            .show(ctx, |ui| self.preview_ui(ui));

        if save_shortcut {
            self.save_now();
        }
    }

    fn toolbar_ui(&mut self, ui: &mut egui::Ui) {
        let mut action: Option<Action> = None;
        ui.horizontal(|ui| {
            ui.label(RichText::new("scene").strong().color(ACCENT));
            ui.separator();
            ui.label(RichText::new(self.doc.file_name()).strong());
            if self.doc.is_dirty() {
                ui.label(RichText::new("● 未保存").color(WARN));
            } else {
                ui.label(RichText::new("已保存").color(DIM));
            }

            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui.button("保存").clicked() {
                    action = Some(Action::Save);
                }
                if ui.button("打开…").clicked() {
                    action = Some(Action::Open);
                }
                if ui.button("新建").clicked() {
                    action = Some(Action::New);
                }
                ui.separator();
                ui.label(
                    RichText::new(format!("工作目录 {}", self.workdir.display())).color(DIM),
                );
            });
        });

        match action {
            Some(Action::New) => self.new_scene(),
            Some(Action::Open) => self.open_dialog(),
            Some(Action::Save) => self.save_now(),
            None => {}
        }
    }

    fn status_ui(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            match &self.built {
                Some(built) => {
                    ui.label(
                        RichText::new(format!(
                            "模型 {} · 程序化物体 {} · 三角面 {}",
                            built.model_count, built.object_count, built.triangle_count
                        ))
                        .color(DIM),
                    );
                    if built.prefab_count > 0 {
                        ui.label(
                            RichText::new(format!(
                                "{} 个 prefab 实例未预览",
                                built.prefab_count
                            ))
                            .color(WARN),
                        );
                    }
                }
                None => {
                    ui.label(RichText::new("空场景").color(DIM));
                }
            }

            if let Some(error) = &self.error {
                ui.label(RichText::new(format!("✖ {error}")).color(ERR));
            } else if let Some(status) = &self.status {
                ui.label(RichText::new(status.clone()).color(DIM));
            }

            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.label(RichText::new("左键旋转 · 中键平移 · 滚轮缩放").color(DIM));
            });
        });
    }

    fn models_ui(&mut self, ui: &mut egui::Ui) {
        let mut refresh = false;
        ui.horizontal(|ui| {
            ui.label(RichText::new("可选模型").strong());
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui.small_button("刷新").clicked() {
                    refresh = true;
                }
            });
        });
        ui.separator();

        if self.groups.is_empty() {
            ui.label(
                RichText::new(format!("{} 下没有 glTF/GLB 文件", models::MODELS_DIR)).color(DIM),
            );
            ui.label(
                RichText::new("把 .gltf/.glb 放进去后点「刷新」")
                    .small()
                    .color(DIM),
            );
        } else {
            let groups = &self.groups;
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    for group in groups {
                        egui::CollapsingHeader::new(group.title.clone())
                            .default_open(true)
                            .show(ui, |ui| {
                                for entry in &group.entries {
                                    let id = ui.make_persistent_id(("model", &entry.label));
                                    ui.dnd_drag_source(id, entry.clone(), |ui| {
                                        ui.label(format!("▣ {}", entry.file_name()));
                                    })
                                    .response
                                    .on_hover_text(entry.label.clone());
                                }
                            });
                    }
                });
        }

        ui.separator();
        ui.label(RichText::new("把模型拖到中间画面放置").small().color(DIM));
        if refresh {
            self.refresh_models();
        }
    }

    fn json_ui(&mut self, ui: &mut egui::Ui) {
        let mut format = false;
        ui.horizontal(|ui| {
            ui.label(RichText::new("场景 JSON").strong());
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui.small_button("格式化").clicked() {
                    format = true;
                }
                ui.label(RichText::new("停顿后自动应用").small().color(DIM));
            });
        });
        ui.separator();

        if let Some(error) = &self.error {
            let text = format!("✖ {error}");
            egui::TopBottomPanel::bottom("scene_json_error").show_inside(ui, |ui| {
                ui.add_space(4.0);
                ui.label(RichText::new(text).color(ERR).small());
                ui.add_space(2.0);
                ui.label(
                    RichText::new("画面停在上一版有效场景")
                        .small()
                        .color(DIM),
                );
                ui.add_space(4.0);
            });
        }

        egui::CentralPanel::default()
            .frame(egui::Frame::none())
            .show_inside(ui, |ui| {
                let mut text = std::mem::take(&mut self.doc.text);
                let rows = text.lines().count().max(1);
                let line_height = ui.text_style_height(&egui::TextStyle::Monospace);
                let numbers = (1..=rows)
                    .map(|n| n.to_string())
                    .collect::<Vec<_>>()
                    .join("\n");

                let mut changed = false;
                egui::ScrollArea::both()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.horizontal_top(|ui| {
                            ui.add_space(6.0);
                            ui.label(RichText::new(numbers).monospace().color(DIM));
                            ui.add_space(2.0);
                            let response = ui.add_sized(
                                [
                                    ui.available_width(),
                                    rows as f32 * line_height + line_height,
                                ],
                                egui::TextEdit::multiline(&mut text)
                                    .code_editor()
                                    .desired_width(f32::INFINITY)
                                    .frame(false),
                            );
                            changed = response.changed();
                        });
                    });

                // 顺序要紧：先 `set_text` 记时间戳，否则防抖计时永远不启动。
                if changed {
                    self.doc.set_text(text, Instant::now());
                } else {
                    self.doc.text = text;
                }
            });

        if format {
            match doc::parse_value(&self.doc.text) {
                Ok(value) => {
                    let pretty = doc::pretty(&value);
                    self.doc.set_text(pretty, Instant::now());
                }
                Err(e) => self.error = Some(e),
            }
        }
    }

    fn preview_ui(&mut self, ui: &mut egui::Ui) {
        let rect = ui.max_rect();
        let response = ui.allocate_rect(rect, egui::Sense::click_and_drag());

        // 3D 画面铺满整个窗口，投影按窗口尺寸算；面板只是叠在上面。
        let screen = ui.ctx().screen_rect().size();
        let viewport = [screen.x.max(1.0), screen.y.max(1.0)];
        let view_projection = self.camera.view_projection_matrix();

        let hover_payload = response.dnd_hover_payload::<ModelEntry>();
        let drop_payload = response.dnd_release_payload::<ModelEntry>();
        let pointer = ui.input(|i| i.pointer.hover_pos());
        let point = pointer
            .filter(|_| hover_payload.is_some())
            .and_then(|p| pick::ground_point([p.x, p.y], viewport, &view_projection));

        {
            let painter = ui.painter();
            if let Some(point) = point {
                if let Some(screen_pos) = pick::project_to_screen(point, viewport, &view_projection)
                {
                    let center = egui::pos2(screen_pos[0], screen_pos[1]);
                    let stroke = egui::Stroke::new(1.5, ACCENT);
                    painter.circle_stroke(center, 9.0, stroke);
                    painter.line_segment(
                        [
                            egui::pos2(center.x - 15.0, center.y),
                            egui::pos2(center.x + 15.0, center.y),
                        ],
                        stroke,
                    );
                    painter.line_segment(
                        [
                            egui::pos2(center.x, center.y - 15.0),
                            egui::pos2(center.x, center.y + 15.0),
                        ],
                        stroke,
                    );
                }
                if let (Some(payload), Some(pointer)) = (hover_payload.as_ref(), pointer) {
                    painter.text(
                        pointer + egui::vec2(14.0, 14.0),
                        egui::Align2::LEFT_TOP,
                        format!("松手放置 {}", payload.file_name()),
                        egui::FontId::proportional(12.0),
                        Color32::WHITE,
                    );
                }
            }

            if self.built.is_none() && self.error.is_none() {
                painter.text(
                    rect.center(),
                    egui::Align2::CENTER_CENTER,
                    "新建或打开一个 .scene.json",
                    egui::FontId::proportional(14.0),
                    DIM,
                );
            }
        }

        if let Some(entry) = drop_payload {
            match point {
                Some(point) => self.place_model(&entry, point),
                None => {
                    self.status =
                        Some("视线与地面平行，没算出落点；换个角度再拖".to_string());
                }
            }
        }
    }
}

fn default_lights() -> Vec<Light> {
    vec![Light::directional(
        [-0.5, -1.0, -0.4],
        [1.0, 0.95, 0.9],
        1.5,
    )]
}

/// 所有对象 aabb 的并集；没有可用对象时为 `None`。
fn union_bounds(scene: &Scene) -> Option<math::AABB> {
    let mut min = Point3::new(f32::MAX, f32::MAX, f32::MAX);
    let mut max = Point3::new(f32::MIN, f32::MIN, f32::MIN);
    let mut any = false;

    for object in &scene.objects {
        let aabb = object.aabb;
        let finite = [aabb.min.x, aabb.min.y, aabb.min.z, aabb.max.x, aabb.max.y, aabb.max.z]
            .iter()
            .all(|v| v.is_finite());
        if !finite {
            continue;
        }
        any = true;
        min.x = min.x.min(aabb.min.x);
        min.y = min.y.min(aabb.min.y);
        min.z = min.z.min(aabb.min.z);
        max.x = max.x.max(aabb.max.x);
        max.y = max.y.max(aabb.max.y);
        max.z = max.z.max(aabb.max.z);
    }

    if !any {
        return None;
    }
    Some(math::AABB::new(min, max))
}

fn point_summary(point: [f32; 3]) -> String {
    format!("{:.2}, {:.2}", point[0], point[2])
}
