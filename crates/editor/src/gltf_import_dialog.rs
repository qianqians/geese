//! GLTF 导入对话框。
//!
//! 用户选择资源类型（Scene / Avatar），选取 GLTF/GLB 文件，
//! 点击 Import 后：
//! 1. 将 GLTF 文件复制到项目 `assets/models/` 目录
//! 2. 生成对应的 `.scene.json` 或 `.avatar.json` 清单文件

use crate::panels::{EditorPanel, EditorState};
use scene::avatar_manifest::AvatarManifest;
use scene::manifest::{ModelRef, SceneManifest, TransformDef};

/// 资源类型选择。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportResourceType {
    Scene,
    Avatar,
}

/// GLTF 导入对话框状态。
pub struct GltfImportDialog {
    /// 是否显示对话框
    pub visible: bool,
    /// 资源类型
    resource_type: ImportResourceType,
    /// 选中的 GLTF 文件路径（绝对路径）
    source_path: String,
    /// 资源名称
    resource_name: String,
    /// 状态消息
    status_message: Option<String>,
    /// 是否成功
    pub import_success: bool,
    /// 是否启用碰撞体（仅 Scene 类型时有效）
    pub collision_enabled: bool,
}

impl Default for GltfImportDialog {
    fn default() -> Self {
        Self {
            visible: false,
            resource_type: ImportResourceType::Scene,
            source_path: String::new(),
            resource_name: String::new(),
            status_message: None,
            import_success: false,
            collision_enabled: false,
        }
    }
}

impl GltfImportDialog {
    pub fn new() -> Self {
        Self::default()
    }

    /// 打开对话框。
    pub fn open(&mut self) {
        self.visible = true;
        self.source_path.clear();
        self.resource_name.clear();
        self.status_message = None;
        self.import_success = false;
    }

    /// 弹出原生文件选择对话框，选取 GLTF/GLB 文件。
    fn pick_file(&mut self) {
        let file = rfd::FileDialog::new()
            .add_filter("GLTF files", &["gltf", "glb"])
            .set_title("Select GLTF / GLB file")
            .pick_file();
        if let Some(path) = file {
            let path_str = path.display().to_string();
            // 自动从文件名提取资源名
            if self.resource_name.is_empty() {
                if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                    self.resource_name = stem.to_string();
                }
            }
            self.source_path = path_str;
        }
    }

    /// 执行导入操作。
    fn do_import(&mut self, project_path: &str) {
        if self.source_path.is_empty() || self.resource_name.is_empty() {
            self.status_message = Some("Please select a file and enter a name.".into());
            self.import_success = false;
            return;
        }

        // 目标目录
        let models_dir = format!("{}/assets/models", project_path);
        if let Err(e) = std::fs::create_dir_all(&models_dir) {
            self.status_message = Some(format!("Failed to create directory: {}", e));
            self.import_success = false;
            return;
        }

        // 提取文件名并复制
        let file_name = match std::path::Path::new(&self.source_path)
            .file_name()
            .and_then(|n| n.to_str())
        {
            Some(name) => name.to_string(),
            None => {
                self.status_message = Some("Invalid source file path.".into());
                self.import_success = false;
                return;
            }
        };

        let dest_path = format!("{}/{}", models_dir, file_name);
        if self.source_path != dest_path {
            if let Err(e) = std::fs::copy(&self.source_path, &dest_path) {
                self.status_message = Some(format!("Failed to copy file: {}", e));
                self.import_success = false;
                return;
            }
        }

        let relative_path = format!("assets/models/{}", file_name);

        // 根据资源类型生成清单文件
        let result = match self.resource_type {
            ImportResourceType::Scene => {
                self.generate_scene_manifest(project_path, &self.resource_name, &relative_path)
            }
            ImportResourceType::Avatar => {
                self.generate_avatar_manifest(project_path, &self.resource_name, &relative_path)
            }
        };

        match result {
            Ok(manifest_path) => {
                self.status_message =
                    Some(format!("Imported successfully: {}", manifest_path));
                self.import_success = true;
            }
            Err(e) => {
                self.status_message = Some(format!("Import failed: {}", e));
                self.import_success = false;
            }
        }
    }

    /// 生成 `.scene.json` 清单文件。
    fn generate_scene_manifest(
        &self,
        project_path: &str,
        name: &str,
        gltf_relative_path: &str,
    ) -> Result<String, Box<dyn std::error::Error>> {
        let manifest = SceneManifest {
            version: "1.0".into(),
            name: name.to_string(),
            models: vec![ModelRef {
                id: name.to_string(),
                path: gltf_relative_path.to_string(),
                transform: TransformDef::default(),
                collision_enabled: self.collision_enabled,
            }],
            environment: Default::default(),
            spawn_points: vec![],
            objects: vec![],
        };

        let json = serde_json::to_string_pretty(&manifest)?;
        let manifest_path = format!("{}/assets/{}.scene.json", project_path, name);
        std::fs::write(&manifest_path, json)?;
        Ok(manifest_path)
    }

    /// 生成 `.avatar.json` 清单文件。
    fn generate_avatar_manifest(
        &self,
        project_path: &str,
        name: &str,
        gltf_relative_path: &str,
    ) -> Result<String, Box<dyn std::error::Error>> {
        // 尝试从 GLTF 解析动画/骨骼信息；若失败则使用空信息
        let abs_gltf = format!("{}/{}", project_path, gltf_relative_path);
        let manifest = match AvatarManifest::from_gltf(name, &abs_gltf) {
            Ok(m) => m,
            Err(_) => AvatarManifest {
                version: "1.0".into(),
                name: name.to_string(),
                gltf_path: gltf_relative_path.to_string(),
                animations: vec![],
                skeleton: Default::default(),
            },
        };

        let json = manifest.to_json()?;
        let manifest_path = format!("{}/assets/{}.avatar.json", project_path, name);
        std::fs::write(&manifest_path, json)?;
        Ok(manifest_path)
    }
}

impl EditorPanel for GltfImportDialog {
    fn title(&self) -> &str {
        "Import GLTF"
    }

    fn show(&mut self, ui: &mut egui::Ui, state: &mut EditorState) {
        self.show_dialog(ui.ctx(), state);
    }
}

impl GltfImportDialog {
    /// 渲染对话框（接受 `&egui::Context`，便于在任意位置调用）。
    pub fn show_dialog(&mut self, ctx: &egui::Context, state: &mut EditorState) {
        if !self.visible {
            return;
        }

        let mut close = false;
        let mut do_import = false;

        egui::Window::new("Import GLTF Resource")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .show(ctx, |ui| {
                ui.set_min_width(420.0);

                // 资源类型选择
                ui.horizontal(|ui| {
                    ui.label("Resource Type:");
                    ui.selectable_value(
                        &mut self.resource_type,
                        ImportResourceType::Scene,
                        "🎬 Scene",
                    );
                    ui.selectable_value(
                        &mut self.resource_type,
                        ImportResourceType::Avatar,
                        "🧑 Avatar",
                    );
                });

                ui.add_space(8.0);

                // 文件路径
                ui.horizontal(|ui| {
                    ui.label("GLTF File:");
                    let display_path = if self.source_path.is_empty() {
                        "(none)"
                    } else {
                        &self.source_path
                    };
                    ui.label(
                        egui::RichText::new(display_path)
                            .monospace()
                            .color(egui::Color32::GRAY),
                    );
                    if ui.button("Browse...").clicked() {
                        self.pick_file();
                    }
                });

                ui.add_space(4.0);

                // 资源名称
                ui.horizontal(|ui| {
                    ui.label("Name:");
                    ui.text_edit_singleline(&mut self.resource_name);
                });

                ui.add_space(4.0);

                // 碰撞体开关（仅 Scene 类型时显示）
                if self.resource_type == ImportResourceType::Scene {
                    ui.horizontal(|ui| {
                        ui.checkbox(&mut self.collision_enabled, "Enable Collision (TriMesh)");
                        if self.collision_enabled {
                            ui.label(
                                egui::RichText::new("⚠ Physics collision will be generated")
                                    .size(11.0)
                                    .color(egui::Color32::YELLOW),
                            );
                        }
                    });
                } else {
                    self.collision_enabled = false;
                }

                ui.add_space(8.0);

                // 状态消息
                if let Some(ref msg) = self.status_message {
                    let color = if self.import_success {
                        egui::Color32::from_rgb(80, 200, 120)
                    } else {
                        egui::Color32::from_rgb(220, 80, 80)
                    };
                    ui.label(egui::RichText::new(msg).color(color));
                }

                ui.add_space(8.0);
                ui.separator();

                // 按钮
                ui.horizontal(|ui| {
                    let can_import =
                        !self.source_path.is_empty() && !self.resource_name.is_empty();
                    if ui.add_enabled(can_import, egui::Button::new("Import")).clicked() {
                        do_import = true;
                    }
                    if ui.button("Cancel").clicked() {
                        close = true;
                    }
                    if self.import_success && ui.button("Close").clicked() {
                        close = true;
                    }
                });
            });

        if do_import {
            self.do_import(&state.project_path);
        }
        if close {
            self.visible = false;
        }
    }
}
