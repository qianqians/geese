//! 编辑器状态与 egui UI。
//!
//! 左侧材质列表 + 新建/保存，右侧 PBR 标量参数面板；预览球体由 `main.rs`
//! 每帧渲染到窗口背景，本模块只负责暴露当前编辑材质与轨道相机。

use std::path::Path;

use camera::{Camera, CameraMode};
use cgmath::Point3;
use egui::Color32;
use material::{AlphaMode, Material};
use render::Light;

use crate::material_store::{LoadedMaterial, MaterialStore, StoreError};

pub struct EditorApp {
    store: MaterialStore,
    selected: usize,
    dirty: bool,
    status: String,
    new_name: String,
    camera: Camera,
    pub lights: Vec<Light>,
    pub ambient: [f32; 3],
}

impl EditorApp {
    pub fn new(project_dir: &Path) -> Result<Self, StoreError> {
        let store = MaterialStore::open(project_dir)?;
        let aspect = 1.0;

        let camera = Camera::new(
            CameraMode::Orbit {
                yaw: -0.7,
                pitch: 0.35,
                focal_point: Point3::new(0.0, 0.0, 0.0),
                distance: 3.2,
                min_distance: 1.2,
                max_distance: 20.0,
            },
            aspect,
        );

        Ok(Self {
            store,
            selected: 0,
            dirty: false,
            status: String::new(),
            new_name: String::new(),
            camera,
            lights: vec![
                Light::directional([-0.5, -1.0, -0.4], [1.0, 0.95, 0.9], 1.5),
                Light::point([2.0, 2.0, 2.0], [0.6, 0.6, 1.0], 8.0, 20.0),
                Light::point([-2.0, 0.5, 1.5], [1.0, 0.7, 0.5], 6.0, 20.0),
            ],
            ambient: [0.08, 0.08, 0.1],
        })
    }

    /// 每帧构建全部 UI。
    pub fn ui(&mut self, ctx: &egui::Context) {
        // ── 左：材质列表 / 新建 / 保存 ──
        egui::SidePanel::left("materials")
            .default_width(220.0)
            .show(ctx, |ui| {
                ui.heading("材质");
                ui.add_space(4.0);

                egui::ScrollArea::vertical().show(ui, |ui| {
                    let selected = self.selected;
                    for (i, loaded) in self.store.materials().iter().enumerate() {
                        let name = loaded
                            .material
                            .name
                            .clone()
                            .unwrap_or_else(|| loaded.file_name.clone());
                        if ui.selectable_label(selected == i, name).clicked() {
                            self.selected = i;
                            self.status.clear();
                        }
                    }
                });

                ui.separator();
                ui.horizontal(|ui| {
                    ui.label("新建");
                    ui.text_edit_singleline(&mut self.new_name);
                    if ui.button("创建").clicked() {
                        self.create_material();
                    }
                });
                if ui.button("保存").clicked() {
                    self.save();
                }
                ui.separator();
                ui.label(&self.status);
            });

        // ── 右：参数面板 ──
        let idx = self.selected;
        let mut params_changed = false;
        egui::SidePanel::right("params")
            .default_width(300.0)
            .show(ctx, |ui| {
                ui.heading("参数");
                ui.add_space(4.0);
                if let Some(m) = self.store.material_mut(idx) {
                    params_changed |= params_ui(ui, m);
                } else {
                    ui.label("没有选中材质");
                }
            });
        self.dirty |= params_changed;
    }

    /// 当前选中材质（供预览渲染）。
    pub fn preview_material(&self) -> Material {
        self.store
            .materials()
            .get(self.selected)
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

        let mut material = Material::default();
        material.name = Some(name.clone());
        self.store.push(LoadedMaterial {
            file_name: format!("{name}.material.json"),
            material,
        });
        self.selected = self.store.materials().len() - 1;
        self.dirty = true;
        self.new_name.clear();
        self.status = format!("已创建 {name}（未保存）");
    }

    fn save(&mut self) {
        let idx = self.selected;
        let result = self
            .store
            .materials()
            .get(idx)
            .map(|loaded| self.store.save(&loaded.material));

        match result {
            Some(Ok(())) => {
                self.dirty = false;
                let name = self
                    .store
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
