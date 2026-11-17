//! 编辑器状态与 egui UI。
//!
//! 左侧材质列表 + 新建/保存，右侧 PBR 标量参数面板；预览球体由 `main.rs` 每帧渲染到
//! 窗口背景，本模块只负责暴露当前编辑材质与轨道相机。
//!
//! 启动时必选一个**工作目录**（`--workdir`），在本工具里它就等于**项目目录**：材质从
//! `<工作目录>/assets/materials/` 读写，记录写 `<工作目录>/.data/material-editor.json`。
//! 项目在运行期间不可切换，因此没有「选择项目」入口。

use std::path::PathBuf;

use camera::{Camera, CameraMode};
use cgmath::Point3;
use edit_prefs::Prefs;
use egui::Color32;
use material::{AlphaMode, Material};
use render::Light;

use crate::material_store::{LoadedMaterial, MaterialStore};

/// 记录文件：`<工作目录>/.data/material-editor.json`。
const PREFS_APP: &str = "material-editor";
/// 记录键：上次选中的材质文件名（路径恒为工作目录）。
const PREFS_KEY: &str = "project";

pub struct EditorApp {
    /// 工作目录打不开时为 `None`，原因在 `status` 里。
    store: Option<MaterialStore>,
    /// 启动参数指定的工作目录——本工具里等于项目目录。
    workdir: PathBuf,
    selected: usize,
    dirty: bool,
    status: String,
    new_name: String,
    camera: Camera,
    prefs: Prefs,
    pub lights: Vec<Light>,
    pub ambient: [f32; 3],
}

impl EditorApp {
    /// 工作目录就是项目目录：构造时直接打开，不再有「选择项目」入口。
    pub fn new(workdir: PathBuf) -> Self {
        let prefs = Prefs::load_in(&workdir, PREFS_APP);
        let camera = Camera::new(
            CameraMode::Orbit {
                yaw: -0.7,
                pitch: 0.35,
                focal_point: Point3::new(0.0, 0.0, 0.0),
                distance: 3.2,
                min_distance: 1.2,
                max_distance: 20.0,
            },
            1.0,
        );

        let mut app = Self {
            store: None,
            workdir,
            selected: 0,
            dirty: false,
            status: String::new(),
            new_name: String::new(),
            camera,
            prefs,
            lights: vec![
                Light::directional([-0.5, -1.0, -0.4], [1.0, 0.95, 0.9], 1.5),
                Light::point([2.0, 2.0, 2.0], [0.6, 0.6, 1.0], 8.0, 20.0),
                Light::point([-2.0, 0.5, 1.5], [1.0, 0.7, 0.5], 6.0, 20.0),
            ],
            ambient: [0.08, 0.08, 0.1],
        };
        app.open_project();
        app
    }

    /// 打开工作目录（= 项目目录）：会先在该目录补齐内置默认材质，然后加载全部
    /// `.material.json`，并恢复上次选中的材质（不存在则选第 0 个）。
    fn open_project(&mut self) {
        let dir = self.workdir.clone();
        let wanted = self.prefs.last(PREFS_KEY).map(|entry| entry.name.clone());

        match MaterialStore::open(&dir) {
            Ok(store) => {
                self.selected = wanted
                    .and_then(|name| {
                        store
                            .materials()
                            .iter()
                            .position(|loaded| loaded.file_name == name)
                    })
                    .unwrap_or(0);
                self.store = Some(store);
                self.dirty = false;
                self.status.clear();
                self.sync_prefs();
            }
            Err(err) => {
                self.status = format!("打开项目 {} 失败: {err}", dir.display());
                self.store = None;
            }
        }
    }

    /// 把「项目目录（= 工作目录） + 当前选中的材质文件名」写回记录。
    fn sync_prefs(&mut self) {
        let name = self
            .store
            .as_ref()
            .and_then(|store| store.materials().get(self.selected))
            .map(|loaded| loaded.file_name.clone())
            .unwrap_or_default();
        let workdir = self.workdir.clone();
        self.prefs.set(PREFS_KEY, name, &workdir);
    }

    /// 每帧构建全部 UI。
    pub fn ui(&mut self, ctx: &egui::Context) {
        let mut clicked_material: Option<usize> = None;

        {
            let app = &mut *self;

            // ── 左：工作目录 / 材质列表 / 新建 / 保存 ──
            egui::SidePanel::left("materials")
                .default_width(220.0)
                .show(ctx, |ui| {
                    ui.heading("材质");
                    ui.label(
                        egui::RichText::new(app.workdir.display().to_string())
                            .weak()
                            .small(),
                    );
                    ui.add_space(4.0);

                    match &app.store {
                        None => {
                            ui.label(egui::RichText::new("项目未能打开，见下方错误").weak());
                        }
                        Some(store) => {
                            egui::ScrollArea::vertical()
                                .max_height((ui.available_height() - 96.0).max(60.0))
                                .show(ui, |ui| {
                                    for (i, loaded) in store.materials().iter().enumerate() {
                                        let name = loaded
                                            .material
                                            .name
                                            .clone()
                                            .unwrap_or_else(|| loaded.file_name.clone());
                                        if ui.selectable_label(app.selected == i, name).clicked() {
                                            clicked_material = Some(i);
                                        }
                                    }
                                });

                            ui.separator();
                            ui.horizontal(|ui| {
                                ui.label("新建");
                                ui.text_edit_singleline(&mut app.new_name);
                                if ui.button("创建").clicked() {
                                    app.create_material();
                                }
                            });
                            if ui.button("保存").clicked() {
                                app.save();
                            }
                        }
                    }

                    ui.separator();
                    ui.label(&app.status);
                });

            // ── 右：参数面板 ──
            let idx = app.selected;
            let has_store = app.store.is_some();
            let mut params_changed = false;
            egui::SidePanel::right("params")
                .default_width(300.0)
                .show(ctx, |ui| {
                    ui.heading("参数");
                    ui.add_space(4.0);
                    match app.store.as_mut().and_then(|store| store.material_mut(idx)) {
                        Some(m) => params_changed |= params_ui(ui, m),
                        None => {
                            if has_store {
                                ui.label("没有选中材质");
                            } else {
                                ui.label("工作目录打不开，无法编辑材质");
                            }
                        }
                    }
                });
            self.dirty |= params_changed;
        }

        if let Some(index) = clicked_material {
            if index != self.selected {
                self.selected = index;
                self.status.clear();
                self.sync_prefs();
            }
        }
    }

    /// 当前选中材质（供预览渲染）。
    pub fn preview_material(&self) -> Material {
        self.store
            .as_ref()
            .and_then(|store| store.materials().get(self.selected))
            .map(|loaded| loaded.material.clone())
            .unwrap_or_default()
    }

    pub fn view_projection(&self) -> [[f32; 4]; 4] {
        self.camera.view_projection_raw()
    }

    pub fn camera_position(&self) -> [f32; 3] {
        self.camera.camera_position_raw()
    }

    pub fn update_aspect(&mut self, width: u32, height: u32) {
        self.camera.update_aspect(width, height);
    }

    pub fn orbit(&mut self, dx: f32, dy: f32) {
        self.camera.orbit(dx, dy);
    }

    pub fn zoom(&mut self, delta: f32) {
        self.camera.zoom(delta);
    }

    fn create_material(&mut self) {
        let name = self.new_name.trim().to_string();
        if name.is_empty() {
            self.status = "请输入材质名".to_string();
            return;
        }
        if name.contains('/') || name.contains('\\') {
            self.status = "材质名不能包含路径分隔符".to_string();
            return;
        }

        let Some(store) = self.store.as_mut() else {
            self.status = "先选择一个项目目录".to_string();
            return;
        };

        let mut material = Material::default();
        material.name = Some(name.clone());
        store.push(LoadedMaterial {
            file_name: format!("{name}.material.json"),
            material,
        });
        self.selected = store.materials().len() - 1;
        self.dirty = true;
        self.new_name.clear();
        self.status = format!("已创建 {name}（未保存）");
    }

    fn save(&mut self) {
        let Some(store) = self.store.as_ref() else {
            self.status = "先选择一个项目目录".to_string();
            return;
        };

        let idx = self.selected;
        let result = store
            .materials()
            .get(idx)
            .map(|loaded| store.save(&loaded.material));

        match result {
            Some(Ok(())) => {
                self.dirty = false;
                let name = store
                    .materials()
                    .get(idx)
                    .map(|l| l.file_name.clone())
                    .unwrap_or_default();
                self.status = format!("已保存 {name}");
            }
            Some(Err(err)) => self.status = format!("保存失败: {err}"),
            None => self.status = "没有选中材质".to_string(),
        }
    }
}

/// 渲染右侧参数面板；返回是否有参数发生变化。
fn params_ui(ui: &mut egui::Ui, m: &mut Material) -> bool {
    let mut changed = false;

    ui.label("基础颜色（线性 RGBA）");
    let mut srgb = Color32::from_rgba_unmultiplied(
        f32_to_u8(linear_to_srgb(m.base_color_factor[0])),
        f32_to_u8(linear_to_srgb(m.base_color_factor[1])),
        f32_to_u8(linear_to_srgb(m.base_color_factor[2])),
        f32_to_u8(m.base_color_factor[3]),
    );
    if ui.color_edit_button_srgba(&mut srgb).changed() {
        m.base_color_factor[0] = srgb_to_linear(u8_to_f32(srgb.r()));
        m.base_color_factor[1] = srgb_to_linear(u8_to_f32(srgb.g()));
        m.base_color_factor[2] = srgb_to_linear(u8_to_f32(srgb.b()));
        m.base_color_factor[3] = u8_to_f32(srgb.a());
        changed = true;
    }

    ui.separator();
    changed |= ui
        .add(egui::Slider::new(&mut m.metallic_factor, 0.0..=1.0).text("金属度"))
        .changed();
    changed |= ui
        .add(egui::Slider::new(&mut m.roughness_factor, 0.0..=1.0).text("粗糙度"))
        .changed();
    changed |= ui
        .add(egui::Slider::new(&mut m.normal_scale, 0.0..=2.0).text("法线强度"))
        .changed();
    changed |= ui
        .add(egui::Slider::new(&mut m.occlusion_strength, 0.0..=1.0).text("AO 强度"))
        .changed();

    ui.separator();
    ui.label("自发光（线性 RGB）");
    let mut e_srgb = [
        f32_to_u8(linear_to_srgb(m.emissive_factor[0])),
        f32_to_u8(linear_to_srgb(m.emissive_factor[1])),
        f32_to_u8(linear_to_srgb(m.emissive_factor[2])),
    ];
    if ui.color_edit_button_srgb(&mut e_srgb).changed() {
        m.emissive_factor = [
            srgb_to_linear(u8_to_f32(e_srgb[0])),
            srgb_to_linear(u8_to_f32(e_srgb[1])),
            srgb_to_linear(u8_to_f32(e_srgb[2])),
        ];
        changed = true;
    }

    ui.separator();
    egui::ComboBox::from_label("透明度模式")
        .selected_text(alpha_mode_label(m.alpha_mode))
        .show_ui(ui, |ui| {
            ui.selectable_value(&mut m.alpha_mode, AlphaMode::Opaque, "不透明");
            ui.selectable_value(&mut m.alpha_mode, AlphaMode::Mask, "镂空");
            ui.selectable_value(&mut m.alpha_mode, AlphaMode::Blend, "混合");
        });
    if m.alpha_mode == AlphaMode::Mask {
        changed |= ui
            .add(egui::Slider::new(&mut m.alpha_cutoff, 0.0..=1.0).text("镂空阈值"))
            .changed();
    }
    changed |= ui.checkbox(&mut m.double_sided, "双面渲染").changed();

    ui.separator();
    if ui.button("恢复默认").clicked() {
        let name = m.name.clone();
        *m = Material::default();
        m.name = name;
        changed = true;
    }

    changed
}

fn alpha_mode_label(mode: AlphaMode) -> &'static str {
    match mode {
        AlphaMode::Opaque => "不透明",
        AlphaMode::Mask => "镂空",
        AlphaMode::Blend => "混合",
    }
}

// ── 颜色空间转换：.material.json 存线性，egui 编辑用 sRGB ──

fn linear_to_srgb(c: f32) -> f32 {
    if c <= 0.003_130_8 {
        c * 12.92
    } else {
        1.055 * c.powf(1.0 / 2.4) - 0.055
    }
}

fn srgb_to_linear(c: f32) -> f32 {
    if c <= 0.040_45 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

fn f32_to_u8(v: f32) -> u8 {
    (v.clamp(0.0, 1.0) * 255.0).round() as u8
}

fn u8_to_f32(v: u8) -> f32 {
    v as f32 / 255.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn srgb_linear_roundtrip() {
        for c in [0.0, 0.1, 0.25, 0.5, 0.75, 1.0] {
            let round = srgb_to_linear(linear_to_srgb(c));
            assert!((round - c).abs() < 1e-4, "roundtrip failed for {c}: {round}");
        }
    }

    #[test]
    fn srgb_mid_gray() {
        // 线性 0.5 对应 sRGB 约 0.735
        assert!((linear_to_srgb(0.5) - 0.735).abs() < 0.005);
    }
}
