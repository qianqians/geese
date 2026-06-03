//! Geese Launcher - 项目模板选择与工程生成。
//!
//! 入口流程：
//! 1. 首页展示可用模板卡片（FPS / 第三人称轨道 / 俯视角）
//! 2. 选择模板后配置项目名称和路径
//! 3. 点击"创建工程"生成完整可运行的 Rust 项目
//! 4. 成功后可打开编辑器

pub mod templates;

use std::io::Write;

use crate::templates::ProjectTemplate;

/// Launcher 页面状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LauncherPage {
    /// 首页：模板选择
    Home,
    /// 项目配置：名称、路径
    Config,
    /// 生成成功
    Success,
}

/// Launcher 主结构体。
pub struct Launcher {
    /// 当前页面
    page: LauncherPage,
    /// 所有可用模板
    templates: Vec<ProjectTemplate>,
    /// 当前选中的模板索引
    selected_index: Option<usize>,
    /// 项目名称输入
    project_name: String,
    /// 项目目标路径输入
    project_path: String,
    /// 生成状态消息
    status_message: Option<String>,
    /// 是否为错误消息
    is_error: bool,
}

impl Launcher {
    pub fn new() -> Self {
        let templates = templates::all_templates();
        Self {
            page: LauncherPage::Home,
            templates,
            selected_index: None,
            project_name: String::from("MyGame"),
            project_path: String::from("./projects"),
            status_message: None,
            is_error: false,
        }
    }

    /// 每帧调用，渲染 Launcher UI。
    /// 返回 `true` 表示 Launcher 已完成（用户关闭或进入编辑器）。
    pub fn show(&mut self, ctx: &egui::Context) -> bool {
        egui::CentralPanel::default().show(ctx, |ui| {
            match self.page {
                LauncherPage::Home => self.show_home(ui),
                LauncherPage::Config => self.show_config(ui),
                LauncherPage::Success => {
                    self.show_success(ui);
                    // 用户点击"打开编辑器"后返回 done
                }
            }
        });

        // 底部状态栏
        if let Some(ref msg) = self.status_message {
            egui::TopBottomPanel::bottom("status_bar").show(ctx, |ui| {
                ui.horizontal(|ui| {
                    if self.is_error {
                        ui.colored_label(egui::Color32::RED, "✗");
                    } else {
                        ui.colored_label(egui::Color32::GREEN, "✓");
                    }
                    ui.label(msg);
                });
            });
        }

        false
    }

    // -----------------------------------------------------------------------
    // 首页：模板选择
    // -----------------------------------------------------------------------

    fn show_home(&mut self, ui: &mut egui::Ui) {
        ui.vertical_centered(|ui| {
            ui.add_space(40.0);

            // Logo / 标题
            ui.heading(
                egui::RichText::new("🪿 Geese Engine Launcher")
                    .size(28.0)
                    .color(egui::Color32::from_rgb(100, 180, 255)),
            );
            ui.add_space(4.0);
            ui.label(
                egui::RichText::new("选择游戏模板，快速开始你的项目")
                    .size(14.0)
                    .color(egui::Color32::GRAY),
            );
            ui.add_space(30.0);

            // 模板卡片区域
            let template_count = self.templates.len();
            ui.horizontal(|ui| {
                ui.add_space(20.0);
                for i in 0..template_count {
                    let selected = self.selected_index == Some(i);
                    self.show_template_card(ui, i, selected);
                    ui.add_space(20.0);
                }
            });

            ui.add_space(30.0);

            // 选择后显示"下一步"按钮
            if let Some(idx) = self.selected_index {
                ui.add_space(10.0);
                if ui
                    .add_sized(
                        [200.0, 40.0],
                        egui::Button::new(
                            egui::RichText::new("下一步：配置项目 →").size(16.0),
                        ),
                    )
                    .clicked()
                {
                    let template = &self.templates[idx];
                    self.project_name = format!("My{}", self.sanitize_name(&template.name));
                    self.status_message = None;
                    self.page = LauncherPage::Config;
                }
            }
        });
    }

    /// 渲染单个模板卡片。
    fn show_template_card(&mut self, ui: &mut egui::Ui, index: usize, is_selected: bool) {
        let template = &self.templates[index];

        let (fill, stroke) = if is_selected {
            (
                egui::Color32::from_rgb(40, 60, 100),
                egui::Stroke::new(2.0, egui::Color32::from_rgb(100, 160, 255)),
            )
        } else {
            (
                egui::Color32::from_rgb(30, 30, 40),
                egui::Stroke::new(1.0, egui::Color32::from_rgb(60, 60, 80)),
            )
        };

        egui::Frame::none()
            .fill(fill)
            .stroke(stroke)
            .rounding(egui::Rounding::same(8.0))
            .inner_margin(egui::Margin::same(16.0))
            .show(ui, |ui| {
                ui.set_width(240.0);

                // 图标
                let icon_text = match template.id.as_str() {
                    "empty" => "📦",
                    "fps" => "🔫",
                    "third_person" => "🎮",
                    "topdown" => "🛰",
                    _ => "📄",
                };
                ui.label(egui::RichText::new(icon_text).size(40.0));
                ui.add_space(8.0);

                // 模板名称
                ui.label(
                    egui::RichText::new(&template.name)
                        .size(18.0)
                        .strong(),
                );
                ui.add_space(6.0);

                // 描述
                ui.label(
                    egui::RichText::new(&template.description)
                        .size(12.0)
                        .color(egui::Color32::from_rgb(180, 180, 190)),
                );

                ui.add_space(12.0);

                // 选中按钮
                if ui.button("选择此模板").clicked() {
                    self.selected_index = Some(index);
                }

                if is_selected {
                    ui.add_space(4.0);
                    ui.colored_label(
                        egui::Color32::from_rgb(100, 200, 100),
                        "✓ 已选择",
                    );
                }
            });
    }

    // -----------------------------------------------------------------------
    // 配置页：项目名称与路径
    // -----------------------------------------------------------------------

    fn show_config(&mut self, ui: &mut egui::Ui) {
        // 提前提取所需数据，避免借用冲突
        let template_name = self.templates[self.selected_index.unwrap()].name.clone();
        let camera_type = self.templates[self.selected_index.unwrap()].camera_config.camera_type;
        let fov = self.templates[self.selected_index.unwrap()].camera_config.fov;
        let move_speed = self.templates[self.selected_index.unwrap()].player_config.move_speed;
        let object_count = self.templates[self.selected_index.unwrap()].objects.len();

        ui.vertical_centered(|ui| {
            ui.add_space(30.0);
            ui.heading(
                egui::RichText::new(format!("配置项目 - {}", template_name))
                    .size(22.0),
            );
            ui.add_space(20.0);
        });

        // 居中表单
        ui.vertical_centered(|ui| {
            egui::Frame::none()
                .fill(egui::Color32::from_rgb(25, 25, 35))
                .rounding(egui::Rounding::same(8.0))
                .inner_margin(egui::Margin::symmetric(30.0, 20.0))
                .show(ui, |ui| {
                    ui.set_width(400.0);

                    // 项目名称
                    ui.label(
                        egui::RichText::new("项目名称").size(14.0).strong(),
                    );
                    ui.add_space(4.0);
                    ui.add(
                        egui::TextEdit::singleline(&mut self.project_name)
                            .hint_text("输入项目名称...")
                            .desired_width(340.0),
                    );
                    ui.add_space(16.0);

                    // 项目路径
                    ui.label(
                        egui::RichText::new("保存路径").size(14.0).strong(),
                    );
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        ui.add(
                            egui::TextEdit::singleline(&mut self.project_path)
                                .hint_text("./projects")
                                .desired_width(280.0),
                        );
                        if ui.button("浏览...").clicked() {
                            // TODO: 后续接入 native file dialog
                        }
                    });
                    ui.add_space(8.0);
                    ui.label(
                        egui::RichText::new(format!(
                            "项目将创建在: {}/{}",
                            self.project_path.trim_end_matches('/'),
                            self.project_name
                        ))
                        .size(11.0)
                        .color(egui::Color32::GRAY),
                    );

                    ui.add_space(20.0);

                    // 模板信息摘要
                    ui.separator();
                    ui.add_space(8.0);
                    ui.label(
                        egui::RichText::new("模板配置摘要")
                            .size(13.0)
                            .strong(),
                    );
                    ui.add_space(4.0);
                    let cam_type = match camera_type {
                        crate::templates::CameraType::Empty => "自由模式",
                        crate::templates::CameraType::FirstPerson => "第一人称 (FPS)",
                        crate::templates::CameraType::ThirdPerson => "第三人称 (轨道)",
                        crate::templates::CameraType::TopDown => "俯视角 (Top-Down)",
                    };
                    ui.label(format!("摄像机类型: {}", cam_type));
                    ui.label(format!("FOV: {}°", fov));
                    ui.label(format!(
                        "移动速度: {} m/s",
                        move_speed
                    ));
                    ui.label(format!("场景物体数: {}", object_count));
                });

            ui.add_space(20.0);

            // 按钮行
            ui.horizontal(|ui| {
                if ui
                    .add_sized([140.0, 36.0], egui::Button::new("← 返回"))
                    .clicked()
                {
                    self.page = LauncherPage::Home;
                    self.status_message = None;
                }

                ui.add_space(40.0);

                let can_create = !self.project_name.trim().is_empty()
                    && !self.project_path.trim().is_empty();

                let create_btn = egui::Button::new(
                    egui::RichText::new("🚀 创建工程").size(15.0),
                )
                .fill(egui::Color32::from_rgb(40, 120, 40))
                .min_size(egui::vec2(180.0, 36.0));

                if ui
                    .add_enabled(can_create, create_btn)
                    .clicked()
                {
                    self.create_project();
                }
            });
        });
    }

    // -----------------------------------------------------------------------
    // 成功页
    // -----------------------------------------------------------------------

    fn show_success(&mut self, ui: &mut egui::Ui) {
        ui.vertical_centered(|ui| {
            ui.add_space(60.0);
            ui.label(egui::RichText::new("✅").size(48.0));
            ui.add_space(16.0);
            ui.heading(
                egui::RichText::new("工程创建成功！").size(24.0),
            );
            ui.add_space(8.0);

            let full_path = format!(
                "{}/{}",
                self.project_path.trim_end_matches('/'),
                self.project_name
            );
            ui.label(format!("📁 {}", full_path));
            ui.add_space(12.0);
            ui.label(
                egui::RichText::new("项目已生成，包含场景、摄像机、角色控制器和物理配置。")
                    .size(13.0)
                    .color(egui::Color32::GRAY),
            );

            ui.add_space(30.0);

            ui.horizontal(|ui| {
                if ui
                    .add_sized([180.0, 40.0], egui::Button::new("← 返回首页"))
                    .clicked()
                {
                    self.page = LauncherPage::Home;
                    self.selected_index = None;
                    self.status_message = None;
                }

                ui.add_space(20.0);

                if ui
                    .add_sized(
                        [200.0, 40.0],
                        egui::Button::new(
                            egui::RichText::new("打开编辑器 →").size(16.0),
                        )
                        .fill(egui::Color32::from_rgb(40, 100, 180)),
                    )
                    .clicked()
                {
                    // TODO: 阶段二实现 - 启动编辑器
                    self.status_message = Some("编辑器功能将在下一阶段实现".into());
                    self.is_error = false;
                }
            });
        });
    }

    // -----------------------------------------------------------------------
    // 工程生成
    // -----------------------------------------------------------------------

    fn create_project(&mut self) {
        let template_idx = self.selected_index.unwrap();
        let template = &self.templates[template_idx];
        let name = self.project_name.trim();
        let base_path = self.project_path.trim_end_matches('/');

        if name.is_empty() || base_path.is_empty() {
            self.status_message = Some("项目名称和路径不能为空".into());
            self.is_error = true;
            return;
        }

        let full_path = format!("{}/{}", base_path, name);

        match self.generate_project(template, name, &full_path) {
            Ok(()) => {
                self.page = LauncherPage::Success;
                self.status_message = Some(format!("工程已生成: {}", full_path));
                self.is_error = false;
            }
            Err(e) => {
                self.status_message = Some(format!("生成失败: {}", e));
                self.is_error = true;
            }
        }
    }

    /// 执行工程生成。
    fn generate_project(
        &self,
        template: &ProjectTemplate,
        name: &str,
        full_path: &str,
    ) -> Result<(), String> {
        use std::fs;

        // 检查目标目录不存在
        let target = std::path::Path::new(full_path);
        if target.exists() {
            return Err(format!("目录已存在: {}", full_path));
        }

        // 创建目录结构
        let dirs = [
            format!("{full_path}"),
            format!("{full_path}/src"),
            format!("{full_path}/assets"),
            format!("{full_path}/assets/scenes"),
            format!("{full_path}/config"),
        ];

        for dir in &dirs {
            fs::create_dir_all(dir).map_err(|e| format!("创建目录失败 {}: {}", dir, e))?;
        }

        // 替换变量的辅助函数
        let replace_vars = |content: &str| -> String {
            content
                .replace("{{project_name}}", name)
                .replace("{{camera_fov}}", &template.camera_config.fov.to_string())
                .replace("{{player_height}}", &template.player_config.capsule_height.to_string())
        };

        // 生成 Cargo.toml
        let cargo_path = format!("{full_path}/Cargo.toml");
        let cargo_content = templates::cargo_toml_content(name);
        self.write_file(&cargo_path, &replace_vars(&cargo_content))?;

        // 生成 main.rs
        let main_path = format!("{full_path}/src/main.rs");
        let main_content = templates::main_rs_content(template);
        self.write_file(&main_path, &replace_vars(&main_content))?;

        // 生成 project.toml 配置
        let config_path = format!("{full_path}/config/project.toml");
        let config_content = templates::project_config_content(template);
        self.write_file(&config_path, &replace_vars(&config_content))?;

        // 生成模板特定文件（camera.rs, player.rs）
        for file in &template.files {
            let file_path = format!("{full_path}/{}", file.relative_path);
            // 确保父目录存在
            if let Some(parent) = std::path::Path::new(&file_path).parent() {
                fs::create_dir_all(parent)
                    .map_err(|e| format!("创建目录失败 {:?}: {}", parent, e))?;
            }
            self.write_file(&file_path, &replace_vars(&file.content))?;
        }

        Ok(())
    }

    fn write_file(&self, path: &str, content: &str) -> Result<(), String> {
        let mut file =
            std::fs::File::create(path).map_err(|e| format!("创建文件失败 {}: {}", path, e))?;
        file.write_all(content.as_bytes())
            .map_err(|e| format!("写入文件失败 {}: {}", path, e))?;
        Ok(())
    }

    /// 将模板名称转为合法的项目名片段。
    fn sanitize_name(&self, name: &str) -> String {
        name.chars()
            .filter(|c| c.is_ascii_alphanumeric())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn launcher_initial_state() {
        let launcher = Launcher::new();
        assert_eq!(launcher.page, LauncherPage::Home);
        assert_eq!(launcher.templates.len(), 4);
        assert_eq!(launcher.templates[0].id, "empty");
        assert_eq!(launcher.templates[1].id, "fps");
        assert_eq!(launcher.templates[2].id, "third_person");
        assert_eq!(launcher.templates[3].id, "topdown");
        assert!(launcher.selected_index.is_none());
    }

    #[test]
    fn sanitize_name_removes_special_chars() {
        let launcher = Launcher::new();
        assert_eq!(launcher.sanitize_name("第一人称视角 (FPS)"), "FPS");
        assert_eq!(launcher.sanitize_name("Hello World!"), "HelloWorld");
    }

    #[test]
    fn generate_project_rejects_existing_dir() {
        let launcher = Launcher::new();
        let template = &launcher.templates[0];
        // 试图覆盖现有目录应返回错误
        let result = launcher.generate_project(template, "test", "/tmp");
        // /tmp 总是存在，所以应返回错误
        assert!(result.is_err());
    }

    #[test]
    fn templates_have_valid_ids() {
        let templates = templates::all_templates();
        for t in &templates {
            assert!(!t.id.is_empty());
            assert!(!t.name.is_empty());
            assert!(!t.description.is_empty());
            assert!(!t.objects.is_empty());
        }
    }

    #[test]
    fn fps_template_has_first_person_camera() {
        let fps = templates::fps_template();
        assert_eq!(
            fps.camera_config.camera_type,
            crate::templates::CameraType::FirstPerson
        );
        assert!(fps.camera_config.fov > 0.0);
        assert!(fps.player_config.capsule_height > 0.0);
    }

    #[test]
    fn third_person_template_has_orbit_camera() {
        let tp = templates::third_person_template();
        assert_eq!(
            tp.camera_config.camera_type,
            crate::templates::CameraType::ThirdPerson
        );
        let (_ox, oy, oz) = tp.camera_config.follow_offset;
        assert!(oy > 0.0, "camera should be above player");
        assert!(oz > 0.0, "camera should be behind player");
        assert!(tp.player_config.mouse_sensitivity > 0.0, "orbit camera needs mouse sensitivity");
    }

    #[test]
    fn empty_template_has_free_camera() {
        let empty = templates::empty_template();
        assert_eq!(
            empty.camera_config.camera_type,
            crate::templates::CameraType::Empty
        );
        assert!(empty.files.len() == 1); // only scene.json
        assert!(empty.input_mappings.is_empty());
    }

    #[test]
    fn topdown_template_has_topdown_camera() {
        let td = templates::topdown_template();
        assert_eq!(
            td.camera_config.camera_type,
            crate::templates::CameraType::TopDown
        );
        assert!(td.camera_config.follow_offset.1 > 10.0, "camera should be above");
        assert!(td.camera_config.follow_offset.2 > 10.0, "camera should be behind (isometric)");
        assert_eq!(td.player_config.jump_impulse, 0.0, "top-down has no jump");
        assert!(td.files.len() >= 2);
    }
}
