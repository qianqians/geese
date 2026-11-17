//! 预览窗口的状态与 egui UI。
//!
//! 左侧是剪辑列表、底部是控制条与状态栏；中央留给 `main.rs` 每帧渲染的模型画面，
//! 这里只画 UI 与空状态提示。相机交互（拖动旋转 / 滚轮缩放）由 `main.rs` 转发。

use std::path::Path;
use std::time::Instant;

use camera::{Camera, CameraMode};
use cgmath::Point3;
use edit_prefs::Prefs;
use egui::Color32;
use render::Light;

use crate::model::LoadedModel;

/// 记录文件：`%APPDATA%\geese\animation-preview.json`。
const PREFS_APP: &str = "animation-preview";
/// 记录键：上次打开的模型。
const PREFS_KEY: &str = "model";

/// UI 里触发、需要在面板闭包外执行的动作（避免与 `self` 的可变借用打架）。
enum Action {
    OpenDialog,
    SelectClip(usize),
    Seek(f32),
}

pub struct PreviewApp {
    model: Option<LoadedModel>,
    camera: Camera,
    aspect: f32,
    playing: bool,
    /// 拖动时间轴之前的播放状态，松手后恢复。
    resume_after_scrub: Option<bool>,
    /// 加载失败的说明，显示在状态栏。
    error: Option<String>,
    prefs: Prefs,
    last_frame: Instant,
    pub lights: Vec<Light>,
    pub ambient: [f32; 3],
}

impl PreviewApp {
    pub fn new() -> Self {
        Self {
            model: None,
            camera: Camera::new(
                CameraMode::Orbit {
                    yaw: -0.7,
                    pitch: 0.25,
                    focal_point: Point3::new(0.0, 0.0, 0.0),
                    distance: 3.0,
                    min_distance: 0.2,
                    max_distance: 200.0,
                },
                1.0,
            ),
            aspect: 1.0,
            playing: true,
            resume_after_scrub: None,
            error: None,
            prefs: Prefs::load(PREFS_APP),
            last_frame: Instant::now(),
            lights: vec![
                Light::directional([-0.5, -1.0, -0.4], [1.0, 0.95, 0.9], 1.5),
                Light::point([2.0, 2.0, 2.0], [0.6, 0.6, 1.0], 8.0, 20.0),
                Light::point([-2.0, 0.5, 1.5], [1.0, 0.7, 0.5], 6.0, 20.0),
            ],
            ambient: [0.12, 0.12, 0.15],
        }
    }

    /// 启动流程：命令行参数 → 上次记录的模型 → 都没有就弹原生对话框。
    pub fn startup(&mut self, cli_model: Option<String>) {
        if let Some(arg) = cli_model {
            self.open_model(Path::new(&arg));
            return;
        }

        let recorded = self
            .prefs
            .last_existing(PREFS_KEY)
            .map(|entry| entry.to_path_buf());

        match recorded {
            Some(path) => self.open_model(&path),
            None => self.open_model_dialog(),
        }
    }

    /// 弹原生文件对话框选模型；取消则维持当前状态。
    pub fn open_model_dialog(&mut self) {
        let picked = rfd::FileDialog::new()
            .add_filter("glTF 模型", &["gltf", "glb"])
            .pick_file();
        if let Some(path) = picked {
            self.open_model(&path);
        }
    }

    /// 加载指定模型；成功则记到 prefs 并重新取景，失败只更新状态栏。
    pub fn open_model(&mut self, path: &Path) {
        log::info!("加载模型 {}", path.display());
        match LoadedModel::load(path) {
            Ok(model) => {
                self.frame_camera(&model);
                self.playing = true;
                self.resume_after_scrub = None;
                self.error = None;
                let name = model.name.clone();
                self.model = Some(model);
                self.prefs.set(PREFS_KEY, name, path);
            }
            Err(err) => {
                // 记录不覆盖：下次启动还能试同一个路径，或由用户重新选择。
                self.error = Some(format!("加载 {} 失败：{err}", path.display()));
                self.model = None;
            }
        }
    }

    /// 推进动画（`main.rs` 每帧调用一次）。
    pub fn advance(&mut self) {
        let now = Instant::now();
        let dt = (now - self.last_frame).as_secs_f32().min(0.1);
        self.last_frame = now;

        if let Some(model) = &mut self.model {
            model.tick(dt, self.playing);
        }
    }

    pub fn ui(&mut self, ctx: &egui::Context) {
        let mut actions: Vec<Action> = Vec::new();

        {
            let app = &mut *self;
            egui::TopBottomPanel::bottom("status")
                .show(ctx, |ui| app.status_bar(ui));
            egui::TopBottomPanel::bottom("controls")
                .show(ctx, |ui| app.controls(ui, &mut actions));
            egui::SidePanel::left("clips")
                .default_width(220.0)
                .show(ctx, |ui| app.clip_panel(ui, &mut actions));
            // 中央不画背景，让 main.rs 渲染的 3D 画面透出来。
            egui::CentralPanel::default()
                .frame(egui::Frame::none())
                .show(ctx, |ui| app.empty_hint(ui, &mut actions));
        }

        for action in actions {
            match action {
                Action::OpenDialog => self.open_model_dialog(),
                Action::SelectClip(index) => {
                    if let Some(model) = &mut self.model {
                        model.select_clip(index);
                    }
                    self.playing = true;
                }
                Action::Seek(time) => {
                    if let Some(model) = &mut self.model {
                        model.seek(time);
                    }
                }
            }
        }
    }

    // ── 渲染侧需要的东西 ──

    pub fn model(&self) -> Option<&LoadedModel> {
        self.model.as_ref()
    }

    pub fn view_projection(&self) -> [[f32; 4]; 4] {
        self.camera.view_projection_raw()
    }

    pub fn camera_position(&self) -> [f32; 3] {
        self.camera.camera_position_raw()
    }

    pub fn update_aspect(&mut self, width: u32, height: u32) {
        self.camera.update_aspect(width, height);
        self.aspect = self.camera.aspect;
    }

    pub fn orbit(&mut self, dx: f32, dy: f32) {
        self.camera.orbit(dx, dy);
    }

    pub fn zoom(&mut self, delta: f32) {
        self.camera.zoom(delta);
    }

    // ── 内部 ──

    /// 按模型包围盒取景：中心为焦点，距离约为半径的 2.6 倍。
    fn frame_camera(&mut self, model: &LoadedModel) {
        let bounds = &model.scene.bounds;
        let center = bounds.center();
        let size = bounds.size();
        let radius = (size.x.max(size.y).max(size.z)) * 0.5;
        let radius = if radius.is_finite() && radius > 0.01 {
            radius
        } else {
            1.0
        };

        self.camera = Camera::new(
            CameraMode::Orbit {
                yaw: -0.7,
                pitch: 0.25,
                focal_point: center,
                distance: radius * 2.6,
                min_distance: radius * 0.2,
                max_distance: radius * 20.0,
            },
            self.aspect,
        );
    }

    fn status_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            match &self.model {
                Some(model) => {
                    ui.label(egui::RichText::new(&model.name).strong());
                    ui.label(
                        egui::RichText::new(model.path.display().to_string())
                            .weak()
                            .small(),
                    );
                    ui.separator();

                    if model.joint_count > render::MAX_JOINTS {
                        ui.label(
                            egui::RichText::new(format!(
                                "⚠ 关节 {} / {}",
                                model.joint_count,
                                render::MAX_JOINTS
                            ))
                            .color(Color32::from_rgb(0xe2, 0xb0, 0x4a)),
                        );
                        ui.label(
                            egui::RichText::new("超出的关节不参与蒙皮，模型会扭曲")
                                .weak()
                                .small(),
                        );
                    } else {
                        ui.label(format!("关节 {} / {}", model.joint_count, render::MAX_JOINTS));
                    }

                    ui.separator();
                    ui.label(format!("剪辑 {}", model.clips.len()));
                }
                None => {
                    ui.label(egui::RichText::new("未加载模型").weak());
                }
            }

            if let Some(err) = &self.error {
                ui.separator();
                ui.label(egui::RichText::new(err).color(Color32::from_rgb(0xe0, 0x6c, 0x75)));
            }
        });
    }

    fn controls(&mut self, ui: &mut egui::Ui, actions: &mut Vec<Action>) {
        ui.add_space(4.0);

        let Some(model) = &mut self.model else {
            ui.label(egui::RichText::new("先用左侧「打开模型…」选一个 glTF/GLB").weak());
            ui.add_space(4.0);
            return;
        };

        let duration = model.duration();
        let current = model.current_clip();

        ui.horizontal(|ui| {
            let selected_text = model
                .clips
                .get(current)
                .map(|clip| format!("{} ({:.2}s)", clip.name, clip.duration))
                .unwrap_or_else(|| "没有剪辑".to_string());

            egui::ComboBox::from_id_salt("clip_combo")
                .selected_text(selected_text)
                .show_ui(ui, |ui| {
                    if model.clips.is_empty() {
                        ui.label("没有剪辑");
                    }
                    for clip in &model.clips {
                        if ui
                            .selectable_label(clip.index == current, &clip.name)
                            .clicked()
                        {
                            actions.push(Action::SelectClip(clip.index));
                        }
                    }
                });

            let play_label = if self.playing { "⏸ 暂停" } else { "▶ 播放" };
            if ui.button(play_label).clicked() {
                self.playing = !self.playing;
            }
            if ui.button("⟲ 重置").clicked() {
                actions.push(Action::Seek(0.0));
            }

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(format!("{:.2}s / {:.2}s", model.time(), duration));
            });
        });

        ui.add_space(2.0);
        if duration > 0.0 {
            let mut time = model.time();
            let response = ui.add(
                egui::Slider::new(&mut time, 0.0..=duration)
                    .show_value(false)
                    .trailing_fill(true),
            );

            if response.drag_started() {
                self.resume_after_scrub = Some(self.playing);
                self.playing = false;
            }
            if response.dragged() || response.clicked() {
                actions.push(Action::Seek(time));
            }
            if response.drag_stopped() {
                if self.resume_after_scrub.take() == Some(true) {
                    self.playing = true;
                }
            }
        }
        ui.add_space(4.0);
    }

    fn clip_panel(&mut self, ui: &mut egui::Ui, actions: &mut Vec<Action>) {
        ui.heading("剪辑");
        ui.add_space(4.0);

        match &self.model {
            None => {
                ui.label(egui::RichText::new("未选择模型").weak());
            }
            Some(model) => {
                if model.clips.is_empty() {
                    ui.label(egui::RichText::new("该模型没有动画剪辑").weak());
                }
                let current = model.current_clip();
                for clip in &model.clips {
                    let label = format!("{}  ({:.2}s)", clip.name, clip.duration);
                    if ui.selectable_label(clip.index == current, label).clicked() {
                        actions.push(Action::SelectClip(clip.index));
                    }
                }
            }
        }

        ui.separator();
        if ui.button("打开模型…").clicked() {
            actions.push(Action::OpenDialog);
        }

        ui.add_space(8.0);
        ui.label(
            egui::RichText::new("左键拖动旋转 · 滚轮缩放\n点击剪辑从头播放")
                .weak()
                .small(),
        );
    }

    fn empty_hint(&mut self, ui: &mut egui::Ui, actions: &mut Vec<Action>) {
        if self.model.is_some() || self.error.is_some() {
            return;
        }

        let mut open = false;
        ui.centered_and_justified(|ui| {
            ui.vertical_centered(|ui| {
                ui.label(egui::RichText::new("未选择模型").size(18.0));
                ui.add_space(8.0);
                open = ui.button("打开模型…").clicked();
            });
        });
        if open {
            actions.push(Action::OpenDialog);
        }
    }
}
