# 编辑器打包游戏功能（Windows + Android）

## 核心策略

以最小侵入方式在编辑器中添加打包功能，分三层实现：
1. **BuildPanel 浮动窗口** — 仿 BundlePanel 模式，平台选择 + 构建按钮 + 日志区
2. **异步构建管道** — std::thread + std::sync::mpsc，非阻塞 UI，每帧 try_recv 轮询进度
3. **game_runtime 双入口改造** — 拆分 lib.rs + main.rs + android_main，支持 Android cdylib

不引入新 crate 依赖，使用 std::thread 替代 tokio::process（避免修改 Cargo.toml）。

---

## Task 1: 扩展 EditorAction 枚举

**文件**: `crates/editor/src/panels.rs` L63 附近

在 `ExportGameWindows` 后添加：
```rust
/// 导出游戏（Android）
ExportGameAndroid,
/// 打开构建面板
OpenBuildPanel,
```

**风险**: 无 — 仅添加枚举变体

---

## Task 2: 创建 BuildPanel 模块

**新文件**: `crates/editor/src/build_panel.rs`

参考 `bundle_panel.rs` 结构，设计 `BuildPanel` struct：

```rust
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::thread;
use std::process::{Command, Stdio};
use std::io::{BufRead, BufReader};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BuildTarget { Windows, Android }

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BuildPhase { Idle, Checking, Compiling, Packaging, Done, Failed }

#[derive(Debug, Clone)]
pub enum BuildEvent {
    PhaseChanged(BuildPhase),
    LogLine(String),
    Finished(Result<String, String>),
}

pub struct BuildPanel {
    pub visible: bool,
    selected_target: BuildTarget,
    build_phase: BuildPhase,
    log_lines: Vec<String>,
    status_message: Option<String>,
    is_success: bool,
    /// 异步构建的事件接收器
    event_rx: Option<Receiver<BuildEvent>>,
}
```

关键方法：
- `new()` — 初始化默认状态
- `show_panel(&mut self, ui, project_path)` — 渲染 UI：平台 radio + Build 按钮 + 滚动日志区
- `start_build(&mut self, project_path)` — 启动 std::thread 异步构建
- `poll(&mut self)` — 每帧调用，try_recv 拉取事件更新状态

UI 布局（仿 BundlePanel L33-106）：
- 顶部：平台选择 `Windows` / `Android`（radio buttons）
- 中部：Build 按钮（构建中禁用）+ Cancel 按钮
- 底部：滚动日志区（最近 200 行）+ 状态消息（绿/红色，仿 BundlePanel L98-105）

---

## Task 3: 实现异步构建管道

**文件**: `crates/editor/src/build_panel.rs`

使用 `std::thread::spawn` + `std::sync::mpsc::channel` 实现：

```rust
fn start_build(&mut self, project_path: String) {
    let target = self.selected_target;
    let (tx, rx) = mpsc::channel::<BuildEvent>();
    self.event_rx = Some(rx);
    self.log_lines.clear();
    self.build_phase = BuildPhase::Checking;

    thread::spawn(move || {
        // 1. 定位 game_runtime Cargo.toml
        let manifest = format!("{}/crates/game_runtime/Cargo.toml",
            env!("CARGO_MANIFEST_DIR").rsplit_once("/crates").unwrap().0);

        // 2. 构建命令
        let mut cmd = match target {
            BuildTarget::Windows => {
                let mut c = Command::new("cargo");
                c.args(["build", "--release", "-p", "game_runtime",
                        "--manifest-path", &manifest]);
                c
            }
            BuildTarget::Android => {
                // 检查 cargo-ndk
                let _ = tx.send(BuildEvent::LogLine(
                    "Checking cargo-ndk...".into()));
                let check = Command::new("cargo")
                    .args(["ndk", "--version"])
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped())
                    .output();
                if check.is_err() {
                    let _ = tx.send(BuildEvent::Finished(Err(
                        "cargo-ndk not installed. Run: cargo install cargo-ndk".into())));
                    return;
                }
                let mut c = Command::new("cargo");
                c.args(["ndk", "-t", "aarch64-linux-android",
                        "build", "--release", "-p", "game_runtime",
                        "--manifest-path", &manifest]);
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
                // 逐行读取 stderr（cargo 输出在 stderr）
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
                        // 打包产物（复制 assets/config 等）
                        let result = package_output(&project_path, target);
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
                        let _ = tx.send(BuildEvent::Finished(Err("Build failed".into())));
                    }
                }
            }
            Err(e) => {
                let _ = tx.send(BuildEvent::Finished(Err(
                    format!("Failed to start cargo: {e}"))));
            }
        }
    });
}
```

`package_output()` 函数：
- Windows：创建 `export/{project_name}/windows/`，复制 `geese_game.exe` + assets + config（复用 `copy_dir_recursive`）
- Android：创建 `export/{project_name}/android/`，复制 `libgeese_game.so` + assets + config

**先例参考**: `crates/physics_manager/src/manager.rs` L358-363 已有 `Command::new(python_path).spawn()` 模式

---

## Task 4: 集成 BuildPanel 到 Editor

**文件**: `crates/editor/src/editor.rs`

### 4a. 添加 import 和字段
- L12 附近添加：`use crate::build_panel::BuildPanel;`
- L64 附近（Editor struct）添加：`build_panel: BuildPanel,`
- L133 附近（`new()`）添加：`build_panel: BuildPanel::new(),`

### 4b. 扩展 Build 菜单
L470-475 修改为：
```rust
ui.menu_button("Build", |ui| {
    if ui.button("Package Game...").clicked() {
        self.build_panel.visible = true;
        ui.close_menu();
    }
    ui.separator();
    if ui.button("Quick Export (Windows)").clicked() {
        self.state.pending_actions.push(EditorAction::ExportGameWindows);
        ui.close_menu();
    }
});
```

### 4c. 在 update() 中添加 BuildPanel 渲染和轮询
L275 附近（BundlePanel 渲染之后）添加：
```rust
// Build 面板轮询
self.build_panel.poll();

// Build 面板渲染（浮动窗口）
if self.build_panel.visible {
    egui::Window::new("Build Game")
        .open(&mut self.build_panel.visible)
        .default_pos([200.0, 100.0])
        .default_width(400.0)
        .default_height(300.0)
        .show(ctx, |ui| {
            self.build_panel.show_panel(ui, &self.state.project_path);
        });
}
```

### 4d. 处理 EditorAction
L851-853 附近添加：
```rust
EditorAction::ExportGameAndroid => {
    self.build_panel.visible = true;
    // 直接启动 Android 构建
}
EditorAction::OpenBuildPanel => {
    self.build_panel.visible = true;
}
```

**文件**: `crates/editor/src/lib.rs` L10 附近添加：`pub mod build_panel;`

---

## Task 5: 改进 handle_export_windows()

**文件**: `crates/editor/src/editor.rs` L859-908

当前问题：仅复制预编译 exe，不实际构建。

改进：在复制前调用 `cargo build --release -p game_runtime`：
```rust
fn handle_export_windows(&mut self) {
    let project_name = ...;  // 现有逻辑
    let export_dir = ...;    // 现有逻辑

    self.state.status_message = Some("Building game_runtime (release)...".into());

    // 构建命令
    let manifest = format!("{}/crates/game_runtime/Cargo.toml",
        env!("CARGO_MANIFEST_DIR").rsplit_once("/crates").unwrap().0);
    
    let status = Command::new("cargo")
        .args(["build", "--release", "-p", "game_runtime",
               "--manifest-path", &manifest])
        .status();

    match status {
        Ok(s) if s.success() => {
            // 继续现有的复制逻辑（L868-907）
            // ... 复制 exe + assets + config
        }
        _ => {
            self.state.status_message = Some("Build failed. Check console for details.".into());
        }
    }
}
```

需要添加 `use std::process::Command;` 到 editor.rs。

**风险**: 中 — 构建会阻塞 UI（同步 .status()），但这是改进现有代码的第一步，后续 Task 3 的异步管道会替代它。

---

## Task 6: game_runtime 双入口改造

**文件**: `crates/game_runtime/Cargo.toml`

添加 lib 节和 Android 条件依赖：
```toml
[lib]
name = "geese_game"
crate-type = ["rlib", "cdylib"]

[target.'cfg(target_os = "android")'.dependencies]
winit = { version = "0.30", features = ["android-native-activity"] }
android_logger = "0.14"
log = "0.4"

[package.metadata.android]
package_name = "com.geese.game"
label = "Geese Game"
icon = ""
sdk_version = 24
target_sdk_version = 34
```

**新文件**: `crates/game_runtime/src/lib.rs`

从 main.rs 提取核心逻辑：
- `pub struct GameState` — 移到 lib.rs（L73-291）
- `pub fn run_event_loop(event_loop, window, project_dir, scene_file)` — 平台无关的事件循环
- 所有 `impl GameState` 方法移到 lib.rs

**文件**: `crates/game_runtime/src/main.rs`（精简）

保留桌面入口：
```rust
use geese_game::run_event_loop;
// ... 精简为仅 EventLoop 创建 + 调用 run_event_loop
fn main() {
    env_logger::init();
    let project_dir = std::env::args().nth(1).unwrap_or_else(|| ".".into());
    let scene_file = std::env::args().nth(2)
        .unwrap_or_else(|| "assets/scenes/default.scene.json".into());
    let event_loop = EventLoop::new().unwrap();
    let window = event_loop.create_window(
        WindowAttributes::default()
            .with_title("Geese Game")
            .with_inner_size(winit::dpi::LogicalSize::new(1280, 720))
    ).unwrap();
    run_event_loop(event_loop, window, &project_dir, &scene_file);
}
```

**文件**: `crates/game_runtime/src/lib.rs`（Android 入口）

```rust
#[cfg(target_os = "android")]
use winit::platform::android::EventLoopBuilderExtAndroid;

#[cfg(target_os = "android")]
#[no_mangle]
fn android_main(app: winit::platform::android::activity::AndroidApp) {
    use winit::event_loop::EventLoopBuilder;
    android_logger::init_once(
        android_logger::Config::default().with_min_level(log::Level::Info));
    
    let mut event_loop_builder = EventLoopBuilder::new();
    event_loop_builder.with_android_app(app);
    let event_loop = event_loop_builder.build().unwrap();
    let window = event_loop.create_window(
        WindowAttributes::default().with_title("Geese Game")
    ).unwrap();
    
    // Android 上资源路径
    let project_dir = std::env::args().nth(1).unwrap_or_else(|| ".".into());
    let scene_file = "assets/scenes/default.scene.json".to_string();
    
    run_event_loop(event_loop, window, &project_dir, &scene_file);
}
```

**关键**: GameState 的 `new()` 方法（L91-217）中 wgpu 初始化使用 `Backends::PRIMARY`（L101-103），在 Android 上自动选择 Vulkan，无需修改。

---

## Task 7: Android 构建配置

**文件**: `.cargo/config.toml`（追加）

```toml
# Android NDK linker（用户需设置 ANDROID_NDK_HOME 环境变量）
[target.aarch64-linux-android]
linker = "aarch64-linux-android24-clang"

[target.armv7-linux-androideabi]
linker = "armv7a-linux-androideabi24-clang"
```

**说明**: 如果使用 `cargo-ndk`，它会自动设置正确的 linker，此配置作为后备。

---

## Task 8: 完善 BuildPanel 的 Android 打包逻辑

**文件**: `crates/editor/src/build_panel.rs`

`package_output()` 中 Android 分支：
1. 查找 `.so` 产物：`crates/game_runtime/target/aarch64-linux-android/release/libgeese_game.so`
2. 创建 `export/{project_name}/android/` 目录
3. 复制 `.so` 到 `lib/arm64-v8a/`
4. 复制 assets 到 `assets/`（Android APK 约定路径）
5. 复制 config 到 `config/`
6. 设置状态消息：`"Android build: export/{project_name}/android/ — Use Android Studio or aapt2 to assemble APK"`

**注意**: 完整的 APK 组装（aapt2 + zipalign + apksigner）标注为后续工作。当前步骤产出可用的 `.so` + 资源目录，用户可用 Android Studio 或命令行工具完成最终 APK 打包。

---

## 依赖关系

```
Task 1 (EditorAction 扩展) ─────────────────────┐
Task 2 (BuildPanel 模块) ── 依赖 Task 1 ──────┐  │
Task 3 (异步管道) ── 依赖 Task 2 ────────────┤  │
Task 5 (改进 Windows 导出) ── 独立 ──────────┤  │
Task 6 (game_runtime 改造) ── 独立 ──────────┤  │
Task 7 (Android 配置) ── 依赖 Task 6 ───────┤  │
Task 8 (Android 打包逻辑) ── 依赖 Task 3,7 ─┤  │
Task 4 (集成到 Editor) ── 依赖 1,2,3,5 ─────┘  │
                                                │
推荐顺序: 6 → 7 → 1 → 2 → 3 → 5 → 4 → 8
```

- Task 6 和 7（game_runtime 改造）与 Task 1-5（编辑器端）可并行
- Task 8 是最后步骤，依赖异步管道和 Android 配置都就绪

---

## 风险与缓解

| 风险 | 等级 | 缓解措施 |
|------|------|---------|
| **Android NDK 环境复杂** | 高 | BuildPanel 构建前检测 cargo-ndk 和 ANDROID_NDK_HOME，缺失时在 UI 显示安装指引 |
| **winit 0.30 android_main 入口不确定** | 中 | 参考 winit 官方 android example；先 cargo check --target aarch64-linux-android 验证 |
| **game_runtime 依赖链可能不兼容 Android** | 中 | 先执行 cargo check --target aarch64-linux-android -p game_runtime，不兼容的 crate 用 cfg 排除 |
| **阻塞式构建冻结 UI（Task 5 的 .status()）** | 中 | Task 5 是过渡方案，Task 3 的异步管道完成后替代它 |
| **APK 组装不完整** | 中 | 当前产出 .so + 资源目录，文档说明用 Android Studio 完成 APK |
| **路径计算依赖 CARGO_MANIFEST_DIR** | 低 | 编译期常量，运行时有效；构建后检查文件是否存在 |

---

## 被拒绝的方案

### 方案 A: 使用 tokio::process::Command 异步构建
**拒绝理由**: 需要修改 editor/Cargo.toml 添加 tokio `process`/`io-util`/`fs` features，引入额外依赖变更。std::thread + std::sync::mpsc 同样实现非阻塞，且无需修改 Cargo.toml。

### 方案 B: 使用 cargo-apk 一站式生成 APK
**拒绝理由**: cargo-apk 要求项目结构改造较大（强制 cdylib + package.metadata.android），且对 Windows 开发者不够透明。cargo-ndk 更轻量，只做交叉编译，APK 组装步骤可控。

### 方案 C: 阻塞式构建（Plan C 的 .status() 方案）
**拒绝理由**: cargo build --release 可能耗时数分钟，阻塞 UI 严重影响用户体验。虽然改动量最小，但用户体验不可接受。仅作为 Task 5 的过渡方案。

---

## 关键文件清单

1. `crates/editor/src/build_panel.rs` — 新建，BuildPanel 浮动窗口 + 异步构建管道
2. `crates/editor/src/editor.rs` — 集成 BuildPanel，扩展菜单，改进 handle_export_windows
3. `crates/editor/src/panels.rs` — 扩展 EditorAction 枚举
4. `crates/editor/src/lib.rs` — 注册 build_panel 模块
5. `crates/game_runtime/src/lib.rs` — 新建，从 main.rs 提取核心逻辑 + Android 入口
6. `crates/game_runtime/src/main.rs` — 精简为桌面入口
7. `crates/game_runtime/Cargo.toml` — 添加 [lib] + Android 条件依赖
8. `.cargo/config.toml` — 追加 Android NDK linker 配置
