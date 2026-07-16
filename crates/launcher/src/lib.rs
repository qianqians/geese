//! Geese Launcher - 项目模板选择与工程生成。
//!
//! 入口流程：
//! 1. 首页展示可用模板卡片（FPS / 第三人称轨道 / 俯视角）
//! 2. 选择模板后配置项目名称和路径
//! 3. 点击"创建工程"生成完整可运行的 Rust 项目
//! 4. 成功后可打开编辑器

pub mod templates;
mod history;

use std::io::Write;

use crate::templates::ProjectTemplate;
use crate::history::ProjectHistory;

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
    /// 用户请求打开的项目路径（由外部读取后重置）
    open_requested: Option<String>,
    /// 项目历史记录
    project_history: ProjectHistory,
}

impl Launcher {
    pub fn new() -> Self {
        let templates = templates::all_templates();
        let mut history = ProjectHistory::load();
        history.validate_projects();  // 清理无效路径
        
        Self {
            page: LauncherPage::Home,
            templates,
            selected_index: None,
            project_name: String::from("MyGame"),
            project_path: String::from("./projects"),
            status_message: None,
            is_error: false,
            open_requested: None,
            project_history: history,
        }
    }

    /// 取出用户请求打开的项目路径（如果有），读取后重置为 None。
    pub fn take_open_request(&mut self) -> Option<String> {
        self.open_requested.take()
    }

    /// 设置底部状态栏消息。
    pub fn set_status(&mut self, msg: String, is_error: bool) {
        self.status_message = Some(msg);
        self.is_error = is_error;
    }

    /// 重置到首页。
    pub fn reset_to_home(&mut self) {
        self.page = LauncherPage::Home;
        self.selected_index = None;
        self.status_message = None;
        self.is_error = false;
    }

    /// 每帧调用，渲染 Launcher UI。
    /// 项目打开请求通过 [`take_open_request`] 取出。
    pub fn show(&mut self, ctx: &egui::Context) {
        // 全局视觉风格
        let mut style = (*ctx.style()).clone();
        style.visuals.panel_fill = egui::Color32::from_rgb(20, 22, 30);
        style.visuals.window_fill = egui::Color32::from_rgb(20, 22, 30);
        style.visuals.faint_bg_color = egui::Color32::from_rgb(28, 30, 40);
        style.visuals.widgets.inactive.bg_fill = egui::Color32::from_rgb(38, 42, 55);
        style.visuals.widgets.inactive.weak_bg_fill = egui::Color32::from_rgb(32, 36, 48);
        style.visuals.widgets.hovered.bg_fill = egui::Color32::from_rgb(50, 55, 70);
        style.visuals.widgets.active.bg_fill = egui::Color32::from_rgb(40, 90, 170);
        style.visuals.selection.bg_fill = egui::Color32::from_rgb(60, 130, 220);
        ctx.set_style(style);

        egui::CentralPanel::default().show(ctx, |ui| {
            match self.page {
                LauncherPage::Home => self.show_home(ui),
                LauncherPage::Config => self.show_config(ui),
                LauncherPage::Success => {
                    self.show_success(ui);
                }
            }
        });

        // 底部状态栏
        if let Some(ref msg) = self.status_message {
            egui::TopBottomPanel::bottom("status_bar")
                .min_height(28.0)
                .show_separator_line(true)
                .show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        ui.add_space(12.0);
                        let (icon, color) = if self.is_error {
                            ("✗", egui::Color32::from_rgb(255, 80, 80))
                        } else {
                            ("✓", egui::Color32::from_rgb(80, 200, 120))
                        };
                        ui.colored_label(color, icon);
                        ui.add_space(6.0);
                        ui.label(
                            egui::RichText::new(msg)
                                .size(12.0)
                                .color(egui::Color32::from_rgb(180, 188, 200)),
                        );
                    });
                });
        }
    }

    // -----------------------------------------------------------------------
    // 首页：模板选择
    // -----------------------------------------------------------------------

    fn show_home(&mut self, ui: &mut egui::Ui) {
        ui.vertical_centered(|ui| {
            ui.add_space(30.0);

            // Logo / 标题
            ui.heading(
                egui::RichText::new("🪿 Geese Engine")
                    .size(28.0)
                    .color(egui::Color32::from_rgb(110, 180, 255)),
            );
            ui.add_space(2.0);
            ui.label(
                egui::RichText::new("选择游戏模板，快速开始你的项目")
                    .size(13.0)
                    .color(egui::Color32::from_rgb(140, 148, 168)),
            );
            ui.add_space(20.0);

            // 左右分栏
            let total_width = ui.available_width();
            let left_width = 440.0;
            let right_width = 210.0;
            let side_gap = (total_width - left_width - right_width - 40.0) * 0.5 - 50.0;

            ui.horizontal(|ui| {
                ui.add_space(side_gap);

                // 左侧：模板卡片
                ui.allocate_ui(egui::vec2(left_width, ui.available_height()), |ui| {
                    ui.vertical_centered(|ui| {
                        ui.label(
                            egui::RichText::new("🎮 选择模板")
                                .size(14.0)
                                .color(egui::Color32::from_rgb(160, 180, 215)),
                        );
                        ui.add_space(8.0);
                        for i in 0..self.templates.len() {
                            self.show_template_card(ui, i);
                            ui.add_space(8.0);
                        }
                    });
                });

                ui.add_space(85.0);

                // 右侧：历史项目
                ui.allocate_ui(egui::vec2(right_width, ui.available_height()), |ui| {
                    ui.vertical_centered(|ui| {
                        if !self.project_history.projects.is_empty() {
                            ui.label(
                                egui::RichText::new("📂 最近项目")
                                    .size(14.0)
                                    .color(egui::Color32::from_rgb(160, 180, 215)),
                            );
                            ui.add_space(8.0);
                            egui::ScrollArea::vertical()
                                .max_height(320.0)
                                .show(ui, |ui| {
                                    let count = self.project_history.projects.len();
                                    for i in 0..count {
                                        let project = self.project_history.projects[i].clone();
                                        self.show_history_entry(ui, &project);
                                    }
                                });
                        }
                    });
                });

                ui.add_space(side_gap);
            });
        });
    }

    /// 渲染单个模板卡片（点击直接跳转配置页）。
    fn show_template_card(&mut self, ui: &mut egui::Ui, index: usize) {
        let template = &self.templates[index];

        let fill = egui::Color32::from_rgb(36, 40, 52);
        let stroke_color = if ui.rect_contains_pointer(ui.available_rect_before_wrap()) {
            egui::Color32::from_rgb(90, 160, 255)
        } else {
            egui::Color32::from_rgb(50, 54, 68)
        };
        let stroke = egui::Stroke::new(1.5, stroke_color);

        let card_rect = egui::Frame::none()
            .fill(fill)
            .stroke(stroke)
            .rounding(egui::Rounding::same(10.0))
            .inner_margin(egui::Margin::symmetric(22.0, 16.0))
            .outer_margin(egui::Margin::same(0.0))
            .show(ui, |ui| {
                ui.set_min_width(440.0);
                ui.set_max_width(440.0);
                ui.horizontal(|ui| {
                    // 图标圆形背景
                    let icon_bg = egui::Color32::from_rgb(48, 54, 72);
                    let (icon, tag_label, tag_color) = match template.id.as_str() {
                        "empty" => ("📦", "自由", egui::Color32::from_rgb(140, 160, 190)),
                        "fps" => ("🔫", "FPS", egui::Color32::from_rgb(230, 120, 80)),
                        "third_person" => ("🎮", "第三人称", egui::Color32::from_rgb(100, 190, 140)),
                        "topdown" => ("🛰", "俯视角", egui::Color32::from_rgb(180, 150, 80)),
                        _ => ("📄", "", egui::Color32::WHITE),
                    };
                    // 图标区域
                    egui::Frame::none()
                        .fill(icon_bg)
                        .rounding(egui::Rounding::same(8.0))
                        .inner_margin(egui::Margin::symmetric(10.0, 8.0))
                        .show(ui, |ui| {
                            ui.label(egui::RichText::new(icon).size(28.0));
                        });
                    ui.add_space(14.0);

                    ui.vertical(|ui| {
                        ui.horizontal(|ui| {
                            ui.label(
                                egui::RichText::new(&template.name)
                                    .size(16.0)
                                    .strong()
                                    .color(egui::Color32::from_rgb(220, 226, 240)),
                            );
                            ui.add_space(8.0);
                            if !tag_label.is_empty() {
                                egui::Frame::none()
                                    .fill(tag_color.gamma_multiply(0.15))
                                    .rounding(egui::Rounding::same(4.0))
                                    .inner_margin(egui::Margin::symmetric(6.0, 1.0))
                                    .show(ui, |ui| {
                                        ui.label(
                                            egui::RichText::new(tag_label)
                                                .size(10.0)
                                                .color(tag_color),
                                        );
                                    });
                            }
                        });
                        ui.add_space(3.0);
                        ui.label(
                            egui::RichText::new(&template.description)
                                .size(12.0)
                                .color(egui::Color32::from_rgb(140, 148, 168)),
                        );
                    });
                });
            });

        // 让卡片可交互
        let response = ui.interact(card_rect.response.rect, ui.next_auto_id(), egui::Sense::click());
        if response.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        if response.clicked() {
            self.selected_index = Some(index);
            let template = &self.templates[index];
            self.project_name = format!("My{}", self.sanitize_name(&template.name));
            self.status_message = None;
            self.page = LauncherPage::Config;
        }
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
            ui.add_space(36.0);
            ui.heading(
                egui::RichText::new(format!("配置项目 - {}", template_name))
                    .size(24.0)
                    .color(egui::Color32::from_rgb(210, 220, 240)),
            );
            ui.add_space(24.0);
        });

        // 居中表单
        ui.vertical_centered(|ui| {
            egui::Frame::none()
                .fill(egui::Color32::from_rgb(30, 34, 48))
                .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(50, 54, 68)))
                .rounding(egui::Rounding::same(10.0))
                .inner_margin(egui::Margin::symmetric(32.0, 24.0))
                .show(ui, |ui| {
                    ui.set_width(420.0);

                    // 项目名称
                    ui.label(
                        egui::RichText::new("项目名称")
                            .size(13.0)
                            .strong()
                            .color(egui::Color32::from_rgb(190, 198, 215)),
                    );
                    ui.add_space(6.0);
                    ui.add(
                        egui::TextEdit::singleline(&mut self.project_name)
                            .hint_text("输入项目名称...")
                            .desired_width(f32::INFINITY),
                    );
                    ui.add_space(18.0);

                    // 项目路径
                    ui.label(
                        egui::RichText::new("保存路径")
                            .size(13.0)
                            .strong()
                            .color(egui::Color32::from_rgb(190, 198, 215)),
                    );
                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        ui.add(
                            egui::TextEdit::singleline(&mut self.project_path)
                                .hint_text("./projects")
                                .desired_width(f32::INFINITY),
                        );
                        if ui
                            .add_sized([60.0, 20.0], egui::Button::new("浏览..."))
                            .clicked()
                        {
                            // TODO: 后续接入 native file dialog
                        }
                    });
                    ui.add_space(6.0);
                    ui.label(
                        egui::RichText::new(format!(
                            "项目将创建在: {}/{}",
                            self.project_path.trim_end_matches('/'),
                            self.project_name
                        ))
                        .size(11.0)
                        .color(egui::Color32::from_rgb(130, 138, 158)),
                    );

                    ui.add_space(22.0);

                    // 模板信息摘要
                    ui.separator();
                    ui.add_space(10.0);
                    ui.label(
                        egui::RichText::new("📋 模板配置摘要")
                            .size(13.0)
                            .strong()
                            .color(egui::Color32::from_rgb(190, 198, 215)),
                    );
                    ui.add_space(8.0);
                    let (cam_icon, cam_type) = match camera_type {
                        crate::templates::CameraType::Empty => ("🎯", "自由模式"),
                        crate::templates::CameraType::FirstPerson => ("🔫", "第一人称 (FPS)"),
                        crate::templates::CameraType::ThirdPerson => ("🎮", "第三人称 (轨道)"),
                        crate::templates::CameraType::TopDown => ("🛰", "俯视角 (Top-Down)"),
                    };
                    ui.horizontal(|ui| {
                        ui.label(format!("{} 摄像机: {}", cam_icon, cam_type));
                    });
                    ui.add_space(3.0);
                    ui.label(format!("   FOV: {}°  |  移动速度: {} m/s  |  物体数: {}", fov, move_speed, object_count));
                });

            ui.add_space(24.0);

            // 按钮行
            ui.horizontal(|ui| {
                if ui
                    .add_sized([120.0, 34.0], egui::Button::new(
                        egui::RichText::new("← 返回").size(14.0),
                    ))
                    .clicked()
                {
                    self.page = LauncherPage::Home;
                    self.status_message = None;
                }

                ui.add_space(60.0);

                let can_create = !self.project_name.trim().is_empty()
                    && !self.project_path.trim().is_empty();

                let create_btn = egui::Button::new(
                    egui::RichText::new("🚀 创建工程").size(15.0),
                )
                .fill(egui::Color32::from_rgb(50, 150, 80))
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

            // 成功图标
            egui::Frame::none()
                .fill(egui::Color32::from_rgb(50, 150, 80).gamma_multiply(0.12))
                .rounding(egui::Rounding::same(40.0))
                .inner_margin(egui::Margin::symmetric(24.0, 18.0))
                .show(ui, |ui| {
                    ui.label(
                        egui::RichText::new("✅").size(40.0),
                    );
                });
            ui.add_space(18.0);

            ui.heading(
                egui::RichText::new("工程创建成功！")
                    .size(24.0)
                    .color(egui::Color32::from_rgb(220, 226, 240)),
            );
            ui.add_space(10.0);

            let full_path = format!(
                "{}/{}",
                self.project_path.trim_end_matches('/'),
                self.project_name
            );
            ui.label(
                egui::RichText::new(format!("📁 {}", full_path))
                    .size(13.0)
                    .color(egui::Color32::from_rgb(150, 160, 180)),
            );
            ui.add_space(8.0);
            ui.label(
                egui::RichText::new("项目已生成，包含场景、摄像机、角色控制器和物理配置。")
                    .size(12.0)
                    .color(egui::Color32::from_rgb(120, 128, 148)),
            );

            ui.add_space(36.0);

            ui.horizontal(|ui| {
                if ui
                    .add_sized([160.0, 38.0], egui::Button::new(
                        egui::RichText::new("← 返回首页").size(14.0),
                    ))
                    .clicked()
                {
                    self.page = LauncherPage::Home;
                    self.selected_index = None;
                    self.status_message = None;
                }

                ui.add_space(24.0);

                if ui
                    .add_sized(
                        [200.0, 38.0],
                        egui::Button::new(
                            egui::RichText::new("🎮 打开编辑器 →").size(15.0),
                        )
                        .fill(egui::Color32::from_rgb(50, 120, 210)),
                    )
                    .clicked()
                {
                    let full_path = format!(
                        "{}/{}",
                        self.project_path.trim_end_matches('/'),
                        self.project_name
                    );
                    self.open_requested = Some(full_path);
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

        if let Err(e) = validate_project_name(name) {
            self.status_message = Some(e);
            self.is_error = true;
            return;
        }

        let full_path = format!("{}/{}", base_path, name);

        match self.generate_project(template, name, &full_path) {
            Ok(()) => {
                // 保存历史记录
                self.project_history.add_project(
                    name.to_string(),
                    full_path.clone(),
                    template.id.clone(),
                );
                if let Err(e) = self.project_history.save() {
                    // 非关键错误，仅记录
                    eprintln!("保存项目历史失败: {}", e);
                }

                // 设置打开请求，由外部处理（隐藏 launcher 打开 editor）
                self.open_requested = Some(full_path.clone());
            }
            Err(e) => {
                self.status_message = Some(format!("生成失败: {}", e));
                self.is_error = true;
            }
        }
    }

    /// 执行工程生成（暂存-提交模式，失败时自动回滚）。
    fn generate_project(
        &self,
        template: &ProjectTemplate,
        name: &str,
        full_path: &str,
    ) -> Result<(), String> {
        use std::fs;

        let target = std::path::Path::new(full_path);
        if target.exists() {
            return Err(format!("目录已存在: {}", full_path));
        }

        // 1. 在同父目录下创建暂存目录
        let parent = target.parent().unwrap_or(std::path::Path::new("."));
        let staging = parent.join(format!(".{}.tmp", name));
        if staging.exists() {
            let _ = fs::remove_dir_all(&staging);
        }

        // 2. 所有文件写入暂存目录
        let write_all = |staging_path: &std::path::Path| -> Result<(), String> {
            let staging_str = staging_path.display().to_string();

            // 创建目录结构
            let dirs = ["", "/src", "/assets", "/assets/scenes", "/config"];
            for d in &dirs {
                fs::create_dir_all(format!("{}{}", staging_str, d))
                    .map_err(|e| format!("创建目录失败: {}", e))?;
            }

            // 替换变量的辅助函数
            let replace_vars = |content: &str| -> String {
                content
                    .replace("{{project_name}}", name)
                    .replace("{{camera_fov}}", &template.camera_config.fov.to_string())
                    .replace("{{player_height}}", &template.player_config.capsule_height.to_string())
            };

            // 生成 Cargo.toml
            let cargo_path = format!("{}/Cargo.toml", staging_str);
            let cargo_content = templates::cargo_toml_content(name);
            Self::write_file_static(&cargo_path, &replace_vars(&cargo_content))?;

            // 生成 main.rs
            let main_path = format!("{}/src/main.rs", staging_str);
            let main_content = templates::main_rs_content(template, name);
            Self::write_file_static(&main_path, &replace_vars(&main_content))?;

            // 生成 project.toml 配置
            let config_path = format!("{}/config/project.toml", staging_str);
            let config_content = templates::project_config_content(template, name);
            Self::write_file_static(&config_path, &replace_vars(&config_content))?;

            // 生成模板特定文件
            for file in &template.files {
                let file_path = format!("{}/{}", staging_str, file.relative_path);
                if let Some(parent) = std::path::Path::new(&file_path).parent() {
                    fs::create_dir_all(parent)
                        .map_err(|e| format!("创建目录失败 {:?}: {}", parent, e))?;
                }
                Self::write_file_static(&file_path, &replace_vars(&file.content))?;
            }

            Ok(())
        };

        // 3. 写入暂存目录
        if let Err(e) = write_all(&staging) {
            let _ = fs::remove_dir_all(&staging);
            return Err(e);
        }

        // 4. 原子 rename：暂存目录 → 目标目录（失败时回退到递归拷贝）
        match fs::rename(&staging, target) {
            Ok(()) => Ok(()),
            Err(_) => {
                // rename 可能因跨卷/路径过长/AV锁定失败，回退到递归拷贝
                let result = Self::move_dir_recursive(&staging, target);
                let _ = fs::remove_dir_all(&staging);
                result.map_err(|e| format!("提交失败(rename回退拷贝): {}", e))
            }
        }
    }

    /// 静态文件写入（用于 generate_project 闭包内部）。
    fn write_file_static(path: &str, content: &str) -> Result<(), String> {
        let mut file =
            std::fs::File::create(path).map_err(|e| format!("创建文件失败 {}: {}", path, e))?;
        file.write_all(content.as_bytes())
            .map_err(|e| format!("写入文件失败 {}: {}", path, e))?;
        Ok(())
    }

    /// 递归移动目录（rename 失败时的回退方案：逐文件拷贝后删除源）。
    fn move_dir_recursive(src: &std::path::Path, dst: &std::path::Path) -> Result<(), String> {
        use std::fs;
        if !src.exists() {
            return Err(format!("源目录不存在: {}", src.display()));
        }
        fs::create_dir_all(dst).map_err(|e| format!("创建目标目录失败: {}", e))?;
        for entry in fs::read_dir(src).map_err(|e| format!("读取源目录失败: {}", e))? {
            let entry = entry.map_err(|e| format!("读取目录项失败: {}", e))?;
            let path = entry.path();
            let dst_path = dst.join(entry.file_name());
            if path.is_dir() {
                Self::move_dir_recursive(&path, &dst_path)?;
            } else {
                fs::copy(&path, &dst_path)
                    .map_err(|e| format!("拷贝文件失败 {:?}: {}", path, e))?;
            }
        }
        Ok(())
    }

    /// 将模板名称转为合法的项目名片段。
    fn sanitize_name(&self, name: &str) -> String {
        name.chars()
            .filter(|c| c.is_ascii_alphanumeric())
            .collect()
    }

    /// 渲染单个历史项目条目
    fn show_history_entry(&mut self, ui: &mut egui::Ui, entry: &crate::history::RecentProject) {
        egui::Frame::none()
            .fill(egui::Color32::from_rgb(38, 42, 55))
            .rounding(egui::Rounding::same(6.0))
            .inner_margin(egui::Margin::symmetric(10.0, 6.0))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.label(
                            egui::RichText::new(&entry.name)
                                .size(13.0)
                                .strong()
                                .color(egui::Color32::from_rgb(210, 216, 230)),
                        );
                        ui.label(
                            egui::RichText::new(&entry.path)
                                .size(11.0)
                                .color(egui::Color32::from_rgb(130, 138, 158)),
                        );
                    });

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let open_btn = egui::Button::new(
                            egui::RichText::new("打开").size(11.0),
                        )
                        .fill(egui::Color32::from_rgb(50, 120, 210))
                        .min_size(egui::vec2(48.0, 22.0))
                        .rounding(egui::Rounding::same(4.0));
                        if ui.add(open_btn).clicked() {
                            self.open_requested = Some(entry.path.clone());
                        }
                    });
                });
            });
        ui.add_space(3.0);
    }
}

/// 校验项目名称是否符合 Cargo 命名规范。
fn validate_project_name(name: &str) -> Result<(), String> {
    if name.is_empty() {
        return Err("项目名称不能为空".into());
    }
    if name.len() > 64 {
        return Err("项目名称不能超过 64 字符".into());
    }
    // 必须 ASCII 字母开头
    let first = name.chars().next().unwrap();
    if !first.is_ascii_alphabetic() {
        return Err("项目名称必须以字母开头".into());
    }
    // 仅允许 ASCII 字母数字 + _ + -
    if name.chars().any(|c| !c.is_ascii_alphanumeric() && c != '_' && c != '-') {
        return Err("项目名称只能包含字母、数字、下划线和连字符".into());
    }
    Ok(())
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
        // 试图覆盖现有目录应返回错误：temp_dir 始终存在
        let tmp = std::env::temp_dir();
        let result = launcher.generate_project(template, "test", tmp.to_str().unwrap());
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
        assert!(empty.files.len() == 4); // camera.rs, player.rs, scene_builder.rs, scene.json
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
        assert!(td.files.len() >= 3);
    }

    // -----------------------------------------------------------------------
    // 新增测试
    // -----------------------------------------------------------------------

    #[test]
    fn scene_builder_mod_in_main_rs() {
        let fps = templates::fps_template();
        let main_rs = templates::main_rs_content(&fps, "TestGame");
        assert!(
            main_rs.contains("// mod scene_builder;"),
            "FPS main.rs should contain commented scene_builder module"
        );

        let tp = templates::third_person_template();
        let main_rs = templates::main_rs_content(&tp, "TestGame");
        assert!(
            main_rs.contains("// mod scene_builder;"),
            "ThirdPerson main.rs should contain commented scene_builder module"
        );
    }

    #[test]
    fn validate_project_name_rejects_invalid() {
        assert!(validate_project_name("").is_err());
        assert!(validate_project_name("123game").is_err()); // 数字开头
        assert!(validate_project_name("测试项目").is_err()); // 含中文
        assert!(validate_project_name("my game").is_err()); // 含空格
        assert!(validate_project_name("a".repeat(65).as_str()).is_err()); // 超长
    }

    #[test]
    fn validate_project_name_accepts_valid() {
        assert!(validate_project_name("MyGame").is_ok());
        assert!(validate_project_name("my-game").is_ok());
        assert!(validate_project_name("game_01").is_ok());
        assert!(validate_project_name("A").is_ok());
    }

    #[test]
    fn topdown_has_scene_builder() {
        let td = templates::topdown_template();
        let has_sb = td.files.iter().any(|f| f.relative_path == "src/scene_builder.rs");
        assert!(has_sb, "TopDown template should include scene_builder.rs");
    }

    #[test]
    fn empty_has_all_modules() {
        let empty = templates::empty_template();
        let paths: Vec<&str> = empty.files.iter().map(|f| f.relative_path.as_str()).collect();
        assert!(paths.contains(&"src/camera.rs"), "empty template should have camera.rs");
        assert!(paths.contains(&"src/player.rs"), "empty template should have player.rs");
        assert!(paths.contains(&"src/scene_builder.rs"), "empty template should have scene_builder.rs");
        assert!(paths.contains(&"assets/scenes/default.scene.json"), "empty template should have scene.json");
        assert!(empty.files.len() >= 4);
    }

    #[test]
    fn generate_project_rollback_on_error() {
        // 使用一个已存在的目录 → 应返回错误，且不留下暂存目录
        let launcher = Launcher::new();
        let template = &launcher.templates[0]; // empty

        let tmp = std::env::temp_dir();
        let result = launcher.generate_project(template, "test_rollback", tmp.to_str().unwrap());
        assert!(result.is_err());

        // 验证暂存目录不存在
        let staging = tmp.join(".test_rollback.tmp");
        assert!(!staging.exists(), "Staging directory should not exist after rollback");
    }
}
