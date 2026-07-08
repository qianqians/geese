//! 游戏打包面板。
//!
//! 提供编辑器内的游戏打包功能，支持 Windows 和 Android 平台。
//! 使用 std::thread + std::sync::mpsc 实现非阻塞异步构建，
//! 构建进度通过 channel 实时推送到 UI 线程。

use std::io::{BufRead, BufReader};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::thread;

// ---------------------------------------------------------------------------
// 枚举定义
// ---------------------------------------------------------------------------

/// 构建目标平台。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuildTarget {
    Windows,
    Android,
}

/// 构建阶段。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuildPhase {
    Idle,
    Checking,
    Compiling,
    Packaging,
    Done,
    Failed,
}

impl BuildPhase {
    fn label(&self) -> &str {
        match self {
            Self::Idle => "Idle",
            Self::Checking => "Checking toolchain...",
            Self::Compiling => "Compiling...",
            Self::Packaging => "Packaging...",
            Self::Done => "Done",
            Self::Failed => "Failed",
        }
    }

    fn is_active(&self) -> bool {
        matches!(self, Self::Checking | Self::Compiling | Self::Packaging)
    }
}

/// 异步构建事件（通过 mpsc channel 传递）。
#[derive(Debug, Clone)]
enum BuildEvent {
    PhaseChanged(BuildPhase),
    LogLine(String),
    Finished(Result<String, String>),
}

// ---------------------------------------------------------------------------
// BuildPanel
// ---------------------------------------------------------------------------

/// 游戏打包浮动面板。
pub struct BuildPanel {
    /// 面板是否可见
    pub visible: bool,
    /// 选中的目标平台
    selected_target: BuildTarget,
    /// 当前构建阶段
    build_phase: BuildPhase,
    /// 构建日志行（最近 200 行）
    log_lines: Vec<String>,
    /// 状态消息
    status_message: Option<String>,
    /// 上次构建是否成功
    is_success: bool,
    /// 异步构建事件接收器
    event_rx: Option<Receiver<BuildEvent>>,
}

impl BuildPanel {
    pub fn new() -> Self {
        Self {
            visible: false,
            selected_target: BuildTarget::Windows,
            build_phase: BuildPhase::Idle,
            log_lines: Vec::new(),
            status_message: None,
            is_success: false,
            event_rx: None,
        }
    }

    /// 设置构建目标平台并打开面板。
    pub fn open_for_target(&mut self, target: BuildTarget) {
        self.selected_target = target;
        self.visible = true;
    }

    /// 渲染面板 UI。
    pub fn show_panel(&mut self, ui: &mut egui::Ui, project_path: &str, engine_root: &str) {
        ui.horizontal(|ui| {
            ui.strong("Build Game");
        });
        ui.add_space(8.0);

        // 平台选择
        ui.horizontal(|ui| {
            ui.label("Target:");
            ui.radio_value(&mut self.selected_target, BuildTarget::Windows, "Windows");
            ui.radio_value(&mut self.selected_target, BuildTarget::Android, "Android");
        });

        ui.add_space(4.0);

        // 构建状态
        let phase_label = self.build_phase.label();
        let phase_color = match self.build_phase {
            BuildPhase::Done => egui::Color32::from_rgb(80, 200, 120),
            BuildPhase::Failed => egui::Color32::from_rgb(220, 80, 80),
            BuildPhase::Idle => egui::Color32::from_rgb(160, 160, 160),
            _ => egui::Color32::from_rgb(200, 180, 80),
        };
        ui.label(egui::RichText::new(format!("Status: {phase_label}")).color(phase_color));

        ui.add_space(8.0);
        ui.separator();
        ui.add_space(4.0);

        // Build / Cancel 按钮
        let is_building = self.build_phase.is_active();
        ui.horizontal(|ui| {
            if is_building {
                ui.add_enabled(false, egui::Button::new("Building..."));
                // Cancel 按钮（丢弃接收器，线程自然结束后被清理）
                if ui.button("Cancel").clicked() {
                    self.build_phase = BuildPhase::Idle;
                    self.event_rx = None;
                    self.status_message = Some("Build cancelled.".into());
                    self.is_success = false;
                }
            } else {
                let build_label = match self.selected_target {
                    BuildTarget::Windows => "Build for Windows",
                    BuildTarget::Android => "Build for Android",
                };
                if ui.button(build_label).clicked() {
                    self.start_build(project_path.to_string(), engine_root.to_string());
                }
            }
        });

        ui.add_space(8.0);

        // 日志区域
        if !self.log_lines.is_empty() {
            ui.label("Build Log:");
            egui::ScrollArea::vertical()
                .id_salt("build_panel_log")
                .max_height(180.0)
                .stick_to_bottom(true)
                .show(ui, |ui| {
                    for line in &self.log_lines {
                        ui.label(line);
                    }
                });
        }

        ui.add_space(4.0);

        // 状态消息
        if let Some(ref msg) = self.status_message {
            let color = if self.is_success {
                egui::Color32::from_rgb(80, 200, 120)
            } else {
                egui::Color32::from_rgb(220, 80, 80)
            };
            ui.label(egui::RichText::new(msg).color(color));
        }

        // 构建完成后的提示
        if self.build_phase == BuildPhase::Done {
            ui.add_space(4.0);
            ui.label(
                egui::RichText::new("Build successful! Check the export directory.")
                    .color(egui::Color32::from_rgb(80, 200, 120)),
            );
        }
    }

    /// 每帧调用，轮询异步构建事件。
    pub fn poll(&mut self) {
        let Some(rx) = &self.event_rx else { return };

        loop {
            match rx.try_recv() {
                Ok(event) => match event {
                    BuildEvent::PhaseChanged(phase) => {
                        self.build_phase = phase;
                    }
                    BuildEvent::LogLine(line) => {
                        self.log_lines.push(line);
                        // 限制日志行数
                        if self.log_lines.len() > 200 {
                            let excess = self.log_lines.len() - 200;
                            self.log_lines.drain(0..excess);
                        }
                    }
                    BuildEvent::Finished(result) => {
                        self.event_rx = None;
                        match result {
                            Ok(path) => {
                                self.build_phase = BuildPhase::Done;
                                self.status_message = Some(format!("Export: {path}"));
                                self.is_success = true;
                            }
                            Err(err) => {
                                self.build_phase = BuildPhase::Failed;
                                self.status_message = Some(err);
                                self.is_success = false;
                            }
                        }
                        break;
                    }
                },
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    self.event_rx = None;
                    if self.build_phase.is_active() {
                        self.build_phase = BuildPhase::Failed;
                        self.status_message = Some("Build thread terminated unexpectedly.".into());
                        self.is_success = false;
                    }
                    break;
                }
            }
        }
    }

    /// 启动异步构建。
    fn start_build(&mut self, project_path: String, engine_root: String) {
        let target = self.selected_target;
        let (tx, rx) = mpsc::channel::<BuildEvent>();
        self.event_rx = Some(rx);
        self.log_lines.clear();
        self.status_message = None;
        self.build_phase = BuildPhase::Checking;

        thread::spawn(move || {
            let _ = tx.send(BuildEvent::LogLine(format!(
                "Building for {:?}...",
                target
            )));

            // 定位 game_runtime Cargo.toml
            let root = Path::new(&engine_root);
            if !root.exists() {
                let _ = tx.send(BuildEvent::Finished(Err(
                    "Cannot determine engine root (set GEESE_ROOT env var).".into(),
                )));
                return;
            }
            let manifest_path = root.join("crates").join("game_runtime").join("Cargo.toml");
            let game_target = root.join("crates").join("game_runtime").join("target");

            let _ = tx.send(BuildEvent::LogLine(format!(
                "Manifest: {}",
                manifest_path.display()
            )));

            // 构建命令
            let mut cmd = match target {
                BuildTarget::Windows => {
                    let mut c = Command::new("cargo");
                    c.args([
                        "build",
                        "--release",
                        "-p",
                        "game_runtime",
                        "--manifest-path",
                    ]);
                    c.arg(&manifest_path);
                    c
                }
                BuildTarget::Android => {
                    // 检查 cargo-ndk
                    let _ = tx.send(BuildEvent::LogLine(
                        "Checking cargo-ndk...".into(),
                    ));
                    let check = Command::new("cargo")
                        .args(["ndk", "--version"])
                        .stdout(Stdio::piped())
                        .stderr(Stdio::piped())
                        .output();

                    if check.is_err() {
                        let _ = tx.send(BuildEvent::Finished(Err(
                            "cargo-ndk not installed. Run: cargo install cargo-ndk\n\
                             Also ensure Android NDK is installed and ANDROID_NDK_HOME is set."
                                .into(),
                        )));
                        return;
                    }

                    // 检查 aarch64-linux-android target
                    let _ = tx.send(BuildEvent::LogLine(
                        "Checking aarch64-linux-android target...".into(),
                    ));
                    let target_check = Command::new("rustup")
                        .args(["target", "list", "--installed"])
                        .stdout(Stdio::piped())
                        .output();
                    if let Ok(output) = &target_check {
                        let stdout = String::from_utf8_lossy(&output.stdout);
                        if !stdout.contains("aarch64-linux-android") {
                            let _ = tx.send(BuildEvent::Finished(Err(
                                "aarch64-linux-android target not installed.\n\
                                 Run: rustup target add aarch64-linux-android"
                                    .into(),
                            )));
                            return;
                        }
                    }

                    let mut c = Command::new("cargo");
                    c.args([
                        "ndk",
                        "-t",
                        "aarch64-linux-android",
                        "build",
                        "--release",
                        "-p",
                        "game_runtime",
                        "--manifest-path",
                    ]);
                    c.arg(&manifest_path);
                    c
                }
            };

            cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
            let _ = tx.send(BuildEvent::PhaseChanged(BuildPhase::Compiling));

            match cmd.spawn() {
                Ok(mut child) => {
                    // 逐行读取 stdout
                    if let Some(stdout) = child.stdout.take() {
                        let reader = BufReader::new(stdout);
                        for line in reader.lines().flatten() {
                            let _ = tx.send(BuildEvent::LogLine(line));
                        }
                    }
                    // 逐行读取 stderr（cargo 大部分输出在 stderr）
                    if let Some(stderr) = child.stderr.take() {
                        let reader = BufReader::new(stderr);
                        for line in reader.lines().flatten() {
                            let _ = tx.send(BuildEvent::LogLine(line));
                        }
                    }

                    let status = child.wait();
                    match status {
                        Ok(s) if s.success() => {
                            let _ = tx.send(BuildEvent::PhaseChanged(BuildPhase::Packaging));
                            let result = package_output(&project_path, target, &game_target);
                            match result {
                                Ok(path) => {
                                    let _ = tx.send(BuildEvent::PhaseChanged(BuildPhase::Done));
                                    let _ = tx.send(BuildEvent::Finished(Ok(path)));
                                }
                                Err(e) => {
                                    let _ = tx.send(BuildEvent::PhaseChanged(BuildPhase::Failed));
                                    let _ = tx.send(BuildEvent::Finished(Err(e)));
                                }
                            }
                        }
                        _ => {
                            let _ = tx.send(BuildEvent::PhaseChanged(BuildPhase::Failed));
                            let _ = tx.send(BuildEvent::Finished(Err(
                                "Build failed. Check log above for details.".into(),
                            )));
                        }
                    }
                }
                Err(e) => {
                    let _ = tx.send(BuildEvent::Finished(Err(format!(
                        "Failed to start cargo: {e}"
                    ))));
                }
            }
        });
    }
}

// ---------------------------------------------------------------------------
// 打包产物
// ---------------------------------------------------------------------------

/// 将构建产物打包到 export 目录。
fn package_output(
    project_path: &str,
    target: BuildTarget,
    game_target_dir: &Path,
) -> Result<String, String> {
    let project_name = Path::new(project_path)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "game".to_string());

    let export_subdir = match target {
        BuildTarget::Windows => "windows",
        BuildTarget::Android => "android",
    };

    let export_dir = format!("{project_path}/export/{project_name}/{export_subdir}");

    // 创建输出目录
    std::fs::create_dir_all(&export_dir)
        .map_err(|e| format!("Failed to create export dir: {e}"))?;

    match target {
        BuildTarget::Windows => {
            // 复制 exe
            let exe_src = game_target_dir.join("release").join("geese_game.exe");
            let exe_dst = format!("{export_dir}/{project_name}.exe");
            if exe_src.exists() {
                std::fs::copy(&exe_src, &exe_dst)
                    .map_err(|e| format!("Failed to copy exe: {e}"))?;
            } else {
                return Err(format!(
                    "geese_game.exe not found at {}",
                    exe_src.display()
                ));
            }

            // 复制 assets 和 config
            copy_dir_recursive(
                &format!("{project_path}/assets"),
                &format!("{export_dir}/assets"),
            )?;
            copy_dir_recursive(
                &format!("{project_path}/config"),
                &format!("{export_dir}/config"),
            )?;

            Ok(format!("{export_dir}/{project_name}.exe"))
        }
        BuildTarget::Android => {
            // 复制 .so
            let so_src = game_target_dir
                .join("aarch64-linux-android")
                .join("release")
                .join("libgeese_game.so");
            let lib_dir = format!("{export_dir}/lib/arm64-v8a");
            std::fs::create_dir_all(&lib_dir)
                .map_err(|e| format!("Failed to create lib dir: {e}"))?;

            if so_src.exists() {
                std::fs::copy(&so_src, format!("{lib_dir}/libgeese_game.so"))
                    .map_err(|e| format!("Failed to copy .so: {e}"))?;
            } else {
                return Err(format!(
                    "libgeese_game.so not found at {}\n\
                     Ensure game_runtime has [lib] crate-type = [\"cdylib\"]",
                    so_src.display()
                ));
            }

            // 复制 assets 和 config
            copy_dir_recursive(
                &format!("{project_path}/assets"),
                &format!("{export_dir}/assets"),
            )?;
            copy_dir_recursive(
                &format!("{project_path}/config"),
                &format!("{export_dir}/config"),
            )?;

            Ok(format!(
                "{export_dir}/lib/arm64-v8a/libgeese_game.so\n\
                 Use Android Studio or aapt2 to assemble APK from this directory."
            ))
        }
    }
}

/// 递归复制目录。
fn copy_dir_recursive(src: &str, dst: &str) -> Result<(), String> {
    let src_path = Path::new(src);
    if !src_path.is_dir() {
        return Ok(());
    }
    std::fs::create_dir_all(dst).map_err(|e| format!("Failed to create dir {dst}: {e}"))?;
    for entry in std::fs::read_dir(src_path).map_err(|e| format!("Failed to read dir {src}: {e}"))? {
        let entry = entry.map_err(|e| format!("Failed to read entry: {e}"))?;
        let file_type = entry.file_type().map_err(|e| format!("Failed to get file type: {e}"))?;
        let src_file = entry.path();
        let dst_file = Path::new(dst).join(entry.file_name());
        if file_type.is_dir() {
            copy_dir_recursive(
                &src_file.to_string_lossy(),
                &dst_file.to_string_lossy(),
            )?;
        } else {
            std::fs::copy(&src_file, &dst_file)
                .map_err(|e| format!("Failed to copy {:?}: {e}", src_file))?;
        }
    }
    Ok(())
}
