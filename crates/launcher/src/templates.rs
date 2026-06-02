//! 项目模板定义。
//!
//! 每个模板包含：
//! - 基本信息（id, name, description）
//! - 摄像机配置（FPS 或 第三人称固定）
//! - 输入映射
//! - 物理配置
//! - 场景定义
//! - 生成文件列表

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// 模板数据结构
// ---------------------------------------------------------------------------

/// 摄像机类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CameraType {
    /// 空项目：自由配置，无预设摄像机。
    Empty,
    /// 第一人称：绑定在角色眼睛位置，鼠标控制旋转。
    FirstPerson,
    /// 第三人称固定：固定在角色后方上方，平滑跟随。
    ThirdPersonFixed,
    /// 俯视角：摄像机垂直俯视，2D 平面移动。
    TopDown,
}

/// 摄像机配置。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CameraConfig {
    pub camera_type: CameraType,
    /// 视场角（度）
    pub fov: f32,
    /// 近裁剪面
    pub z_near: f32,
    /// 远裁剪面
    pub z_far: f32,
    /// 角色眼睛高度偏移（仅 FPS）
    pub eye_height: f32,
    /// 摄像机相对角色偏移（仅 ThirdPersonFixed）
    pub follow_offset: (f32, f32, f32),
    /// 跟随平滑系数（仅 ThirdPersonFixed）
    pub follow_smooth: f32,
}

impl Default for CameraConfig {
    fn default() -> Self {
        Self {
            camera_type: CameraType::ThirdPersonFixed,
            fov: 60.0,
            z_near: 0.1,
            z_far: 1000.0,
            eye_height: 1.7,
            follow_offset: (0.0, 5.0, 8.0),
            follow_smooth: 0.1,
        }
    }
}

/// 角色控制器配置。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerConfig {
    /// 移动速度 (m/s)
    pub move_speed: f32,
    /// 跳跃冲量
    pub jump_impulse: f32,
    /// 胶囊体半径
    pub capsule_radius: f32,
    /// 胶囊体高度
    pub capsule_height: f32,
    /// 鼠标灵敏度（仅 FPS）
    pub mouse_sensitivity: f32,
}

impl Default for PlayerConfig {
    fn default() -> Self {
        Self {
            move_speed: 5.0,
            jump_impulse: 8.0,
            capsule_radius: 0.3,
            capsule_height: 1.7,
            mouse_sensitivity: 0.002,
        }
    }
}

/// 输入动作绑定。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InputMapping {
    pub action: String,
    pub key: String,
}

/// 场景对象描述。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SceneObjectDesc {
    pub object_type: String, // "plane", "cube", "light", "player_spawn"
    pub position: (f32, f32, f32),
    pub scale: (f32, f32, f32),
    pub color: Option<(f32, f32, f32)>,
    pub rotation_euler: Option<(f32, f32, f32)>,
}

/// 模板文件：相对路径 + 文本内容（支持 `{{variable}}` 占位符）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemplateFile {
    pub relative_path: String,
    pub content: String,
}

/// 项目模板定义。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectTemplate {
    pub id: String,
    pub name: String,
    pub description: String,
    pub camera_config: CameraConfig,
    pub player_config: PlayerConfig,
    pub input_mappings: Vec<InputMapping>,
    pub objects: Vec<SceneObjectDesc>,
    /// 生成到目标工程的文件列表
    pub files: Vec<TemplateFile>,
}

// ---------------------------------------------------------------------------
// 预置模板
// ---------------------------------------------------------------------------

fn empty_objects() -> Vec<SceneObjectDesc> {
    vec![
        // 基础地面
        SceneObjectDesc {
            object_type: "plane".into(),
            position: (0.0, 0.0, 0.0),
            scale: (10.0, 1.0, 10.0),
            color: Some((0.5, 0.5, 0.5)),
            rotation_euler: None,
        },
        // 方向光
        SceneObjectDesc {
            object_type: "directional_light".into(),
            position: (5.0, 10.0, 5.0),
            scale: (1.0, 1.0, 1.0),
            color: Some((1.0, 1.0, 0.9)),
            rotation_euler: Some((-0.6, 0.4, 0.0)),
        },
    ]
}

fn fps_objects() -> Vec<SceneObjectDesc> {
    vec![
        // 地板（大平面）
        SceneObjectDesc {
            object_type: "plane".into(),
            position: (0.0, 0.0, 0.0),
            scale: (20.0, 1.0, 20.0),
            color: Some((0.4, 0.4, 0.4)),
            rotation_euler: None,
        },
        // 四面墙
        SceneObjectDesc {
            object_type: "cube".into(),
            position: (0.0, 2.5, -10.0),
            scale: (20.0, 5.0, 0.5),
            color: Some((0.6, 0.6, 0.65)),
            rotation_euler: None,
        },
        SceneObjectDesc {
            object_type: "cube".into(),
            position: (0.0, 2.5, 10.0),
            scale: (20.0, 5.0, 0.5),
            color: Some((0.6, 0.6, 0.65)),
            rotation_euler: None,
        },
        SceneObjectDesc {
            object_type: "cube".into(),
            position: (-10.0, 2.5, 0.0),
            scale: (0.5, 5.0, 20.0),
            color: Some((0.55, 0.55, 0.6)),
            rotation_euler: None,
        },
        SceneObjectDesc {
            object_type: "cube".into(),
            position: (10.0, 2.5, 0.0),
            scale: (0.5, 5.0, 20.0),
            color: Some((0.55, 0.55, 0.6)),
            rotation_euler: None,
        },
        // 天花板
        SceneObjectDesc {
            object_type: "cube".into(),
            position: (0.0, 5.0, 0.0),
            scale: (20.0, 0.2, 20.0),
            color: Some((0.7, 0.7, 0.7)),
            rotation_euler: None,
        },
        // 室内道具 - 若干立方体
        SceneObjectDesc {
            object_type: "cube".into(),
            position: (-3.0, 0.5, -5.0),
            scale: (1.0, 1.0, 1.0),
            color: Some((0.8, 0.3, 0.3)),
            rotation_euler: Some((0.0, 30.0_f32.to_radians(), 0.0)),
        },
        SceneObjectDesc {
            object_type: "cube".into(),
            position: (4.0, 1.0, -3.0),
            scale: (2.0, 2.0, 2.0),
            color: Some((0.3, 0.6, 0.3)),
            rotation_euler: None,
        },
        SceneObjectDesc {
            object_type: "cube".into(),
            position: (-5.0, 0.25, 6.0),
            scale: (0.5, 0.5, 0.5),
            color: Some((0.3, 0.3, 0.8)),
            rotation_euler: None,
        },
        // 方向光
        SceneObjectDesc {
            object_type: "directional_light".into(),
            position: (5.0, 10.0, 5.0),
            scale: (1.0, 1.0, 1.0),
            color: Some((1.0, 0.95, 0.85)),
            rotation_euler: Some((-0.8, 0.6, 0.0)),
        },
        // 玩家出生点
        SceneObjectDesc {
            object_type: "player_spawn".into(),
            position: (0.0, 1.0, 0.0),
            scale: (1.0, 1.0, 1.0),
            color: None,
            rotation_euler: Some((0.0, 0.0, 0.0)),
        },
    ]
}

fn third_person_objects() -> Vec<SceneObjectDesc> {
    vec![
        // 开阔地面
        SceneObjectDesc {
            object_type: "plane".into(),
            position: (0.0, 0.0, 0.0),
            scale: (50.0, 1.0, 50.0),
            color: Some((0.35, 0.5, 0.25)),
            rotation_euler: None,
        },
        // 散落的立方体道具
        SceneObjectDesc {
            object_type: "cube".into(),
            position: (-5.0, 0.5, -5.0),
            scale: (1.0, 1.0, 1.0),
            color: Some((0.7, 0.5, 0.3)),
            rotation_euler: Some((0.0, 25.0_f32.to_radians(), 0.0)),
        },
        SceneObjectDesc {
            object_type: "cube".into(),
            position: (8.0, 1.0, -3.0),
            scale: (2.0, 2.0, 2.0),
            color: Some((0.5, 0.5, 0.5)),
            rotation_euler: None,
        },
        SceneObjectDesc {
            object_type: "cube".into(),
            position: (-8.0, 0.75, 7.0),
            scale: (1.5, 1.5, 1.5),
            color: Some((0.4, 0.3, 0.7)),
            rotation_euler: None,
        },
        SceneObjectDesc {
            object_type: "cube".into(),
            position: (3.0, 0.25, 6.0),
            scale: (0.5, 0.5, 0.5),
            color: Some((0.9, 0.2, 0.2)),
            rotation_euler: None,
        },
        SceneObjectDesc {
            object_type: "cube".into(),
            position: (-2.0, 2.0, -8.0),
            scale: (3.0, 4.0, 3.0),
            color: Some((0.6, 0.6, 0.7)),
            rotation_euler: None,
        },
        // 方向光（产生阴影）
        SceneObjectDesc {
            object_type: "directional_light".into(),
            position: (10.0, 15.0, 10.0),
            scale: (1.0, 1.0, 1.0),
            color: Some((1.0, 0.95, 0.8)),
            rotation_euler: Some((-0.7, 0.5, 0.0)),
        },
        // 玩家出生点
        SceneObjectDesc {
            object_type: "player_spawn".into(),
            position: (0.0, 1.0, 0.0),
            scale: (1.0, 1.0, 1.0),
            color: None,
            rotation_euler: Some((0.0, 0.0, 0.0)),
        },
    ]
}

fn topdown_objects() -> Vec<SceneObjectDesc> {
    vec![
        // 开阔地面（绿色草地）
        SceneObjectDesc {
            object_type: "plane".into(),
            position: (0.0, 0.0, 0.0),
            scale: (40.0, 1.0, 40.0),
            color: Some((0.3, 0.5, 0.25)),
            rotation_euler: None,
        },
        // 障碍物 — 方形柱子
        SceneObjectDesc {
            object_type: "cube".into(),
            position: (-8.0, 1.0, -8.0),
            scale: (2.0, 2.0, 2.0),
            color: Some((0.5, 0.4, 0.3)),
            rotation_euler: None,
        },
        SceneObjectDesc {
            object_type: "cube".into(),
            position: (8.0, 1.0, 8.0),
            scale: (2.0, 2.0, 2.0),
            color: Some((0.5, 0.4, 0.3)),
            rotation_euler: None,
        },
        SceneObjectDesc {
            object_type: "cube".into(),
            position: (-8.0, 1.0, 8.0),
            scale: (2.0, 2.0, 2.0),
            color: Some((0.5, 0.4, 0.3)),
            rotation_euler: None,
        },
        SceneObjectDesc {
            object_type: "cube".into(),
            position: (8.0, 1.0, -8.0),
            scale: (2.0, 2.0, 2.0),
            color: Some((0.5, 0.4, 0.3)),
            rotation_euler: None,
        },
        // 中心区域矮墙
        SceneObjectDesc {
            object_type: "cube".into(),
            position: (0.0, 0.5, -6.0),
            scale: (4.0, 1.0, 0.5),
            color: Some((0.6, 0.55, 0.5)),
            rotation_euler: None,
        },
        SceneObjectDesc {
            object_type: "cube".into(),
            position: (0.0, 0.5, 6.0),
            scale: (4.0, 1.0, 0.5),
            color: Some((0.6, 0.55, 0.5)),
            rotation_euler: None,
        },
        SceneObjectDesc {
            object_type: "cube".into(),
            position: (-6.0, 0.5, 0.0),
            scale: (0.5, 1.0, 4.0),
            color: Some((0.6, 0.55, 0.5)),
            rotation_euler: None,
        },
        SceneObjectDesc {
            object_type: "cube".into(),
            position: (6.0, 0.5, 0.0),
            scale: (0.5, 1.0, 4.0),
            color: Some((0.6, 0.55, 0.5)),
            rotation_euler: None,
        },
        // 方向光
        SceneObjectDesc {
            object_type: "directional_light".into(),
            position: (0.0, 20.0, 0.0),
            scale: (1.0, 1.0, 1.0),
            color: Some((1.0, 0.95, 0.8)),
            rotation_euler: Some((-1.2, 0.0, 0.0)),
        },
        // 玩家出生点
        SceneObjectDesc {
            object_type: "player_spawn".into(),
            position: (0.0, 1.0, 0.0),
            scale: (1.0, 1.0, 1.0),
            color: None,
            rotation_euler: Some((0.0, 0.0, 0.0)),
        },
    ]
}

/// 空项目模板
pub fn empty_template() -> ProjectTemplate {
    ProjectTemplate {
        id: "empty".into(),
        name: "空项目".into(),
        description: "最小化空白场景，自由搭建。适合从零开始的任何项目类型。".into(),
        camera_config: CameraConfig {
            camera_type: CameraType::Empty,
            fov: 60.0,
            z_near: 0.1,
            z_far: 1000.0,
            eye_height: 1.7,
            follow_offset: (0.0, 5.0, 8.0),
            follow_smooth: 0.1,
        },
        player_config: PlayerConfig::default(),
        input_mappings: vec![],
        objects: empty_objects(),
        files: empty_template_files(),
    }
}

/// FPS（第一人称视角）模板
pub fn fps_template() -> ProjectTemplate {
    ProjectTemplate {
        id: "fps".into(),
        name: "第一人称视角 (FPS)".into(),
        description: "封闭室内场景，WASD 移动 + 鼠标控制视角，胶囊体物理角色。适合射击、探索类游戏。".into(),
        camera_config: CameraConfig {
            camera_type: CameraType::FirstPerson,
            fov: 70.0,
            z_near: 0.1,
            z_far: 1000.0,
            eye_height: 1.7,
            follow_offset: (0.0, 0.0, 0.0),
            follow_smooth: 0.0,
        },
        player_config: PlayerConfig {
            move_speed: 5.0,
            jump_impulse: 8.0,
            capsule_radius: 0.3,
            capsule_height: 1.7,
            mouse_sensitivity: 0.002,
        },
        input_mappings: vec![
            InputMapping { action: "move_forward".into(), key: "W".into() },
            InputMapping { action: "move_backward".into(), key: "S".into() },
            InputMapping { action: "move_left".into(), key: "A".into() },
            InputMapping { action: "move_right".into(), key: "D".into() },
            InputMapping { action: "jump".into(), key: "Space".into() },
            InputMapping { action: "shoot".into(), key: "MouseLeft".into() },
        ],
        objects: fps_objects(),
        files: fps_template_files(),
    }
}

/// 第三人称固定视角模板
pub fn third_person_template() -> ProjectTemplate {
    ProjectTemplate {
        id: "third_person".into(),
        name: "第三人称固定视角".into(),
        description: "开阔室外场景，摄像机固定在角色后方上方，WASD 世界坐标移动。适合动作冒险、RPG 类游戏。".into(),
        camera_config: CameraConfig {
            camera_type: CameraType::ThirdPersonFixed,
            fov: 60.0,
            z_near: 0.1,
            z_far: 1000.0,
            eye_height: 1.7,
            follow_offset: (0.0, 6.0, 10.0),
            follow_smooth: 0.08,
        },
        player_config: PlayerConfig {
            move_speed: 5.0,
            jump_impulse: 8.0,
            capsule_radius: 0.3,
            capsule_height: 1.7,
            mouse_sensitivity: 0.0, // 第三人称不用鼠标
        },
        input_mappings: vec![
            InputMapping { action: "move_forward".into(), key: "W".into() },
            InputMapping { action: "move_backward".into(), key: "S".into() },
            InputMapping { action: "move_left".into(), key: "A".into() },
            InputMapping { action: "move_right".into(), key: "D".into() },
            InputMapping { action: "jump".into(), key: "Space".into() },
        ],
        objects: third_person_objects(),
        files: third_person_template_files(),
    }
}

/// 俯视角模板
pub fn topdown_template() -> ProjectTemplate {
    ProjectTemplate {
        id: "topdown".into(),
        name: "俯视角 (Top-Down)".into(),
        description: "摄像机垂直俯视，WASD 平面移动。适合 RTS、MOBA、Roguelike 等游戏。".into(),
        camera_config: CameraConfig {
            camera_type: CameraType::TopDown,
            fov: 50.0,
            z_near: 0.1,
            z_far: 1000.0,
            eye_height: 0.0,
            follow_offset: (0.0, 20.0, 0.0),
            follow_smooth: 0.05,
        },
        player_config: PlayerConfig {
            move_speed: 4.0,
            jump_impulse: 0.0,
            capsule_radius: 0.3,
            capsule_height: 1.0,
            mouse_sensitivity: 0.0,
        },
        input_mappings: vec![
            InputMapping { action: "move_forward".into(), key: "W".into() },
            InputMapping { action: "move_backward".into(), key: "S".into() },
            InputMapping { action: "move_left".into(), key: "A".into() },
            InputMapping { action: "move_right".into(), key: "D".into() },
        ],
        objects: topdown_objects(),
        files: topdown_template_files(),
    }
}

/// 所有可用模板
pub fn all_templates() -> Vec<ProjectTemplate> {
    vec![empty_template(), fps_template(), third_person_template(), topdown_template()]
}

// ---------------------------------------------------------------------------
// 模板文件内容
// ---------------------------------------------------------------------------

fn empty_template_files() -> Vec<TemplateFile> {
    vec![]
}

fn fps_template_files() -> Vec<TemplateFile> {
    vec![
        TemplateFile {
            relative_path: "src/camera.rs".into(),
            content: include_str!("../templates/fps_camera.rs.txt").to_string(),
        },
        TemplateFile {
            relative_path: "src/player.rs".into(),
            content: include_str!("../templates/fps_player.rs.txt").to_string(),
        },
        TemplateFile {
            relative_path: "src/scene_builder.rs".into(),
            content: include_str!("../templates/scene_builder.rs.txt").to_string(),
        },
    ]
}

fn third_person_template_files() -> Vec<TemplateFile> {
    vec![
        TemplateFile {
            relative_path: "src/camera.rs".into(),
            content: include_str!("../templates/tp_camera.rs.txt").to_string(),
        },
        TemplateFile {
            relative_path: "src/player.rs".into(),
            content: include_str!("../templates/tp_player.rs.txt").to_string(),
        },
        TemplateFile {
            relative_path: "src/scene_builder.rs".into(),
            content: include_str!("../templates/scene_builder.rs.txt").to_string(),
        },
    ]
}

fn topdown_template_files() -> Vec<TemplateFile> {
    vec![
        TemplateFile {
            relative_path: "src/camera.rs".into(),
            content: include_str!("../templates/td_camera.rs.txt").to_string(),
        },
        TemplateFile {
            relative_path: "src/player.rs".into(),
            content: include_str!("../templates/td_player.rs.txt").to_string(),
        },
    ]
}

// ---------------------------------------------------------------------------
// 公共模板文件（Cargo.toml, main.rs, project.toml）
// ---------------------------------------------------------------------------

/// 生成 Cargo.toml 模板内容（根据模板类型插入不同的 crate 依赖）
pub fn cargo_toml_content(project_name: &str) -> String {
    format!(
        r#"[package]
name = "{project_name}"
version = "0.1.0"
edition = "2024"

[dependencies]
egui = {{ version = "0.29", default-features = false }}
cgmath = "0.18"
wgpu = "0.20"
winit = "0.30"
serde = {{ version = "1.0", features = ["derive"] }}
serde_json = "1.0"

"#
    )
}

/// 生成 main.rs 模板内容。
pub fn main_rs_content(template: &ProjectTemplate) -> String {
    let camera_type = match template.camera_config.camera_type {
        CameraType::Empty => "Free",
        CameraType::FirstPerson => "FirstPerson",
        CameraType::ThirdPersonFixed => "ThirdPersonFixed",
        CameraType::TopDown => "TopDown",
    };

    let has_camera_player = template.files.iter().any(|f| f.relative_path == "src/camera.rs");
    let mod_decls = if has_camera_player {
        "mod camera;\nmod player;\n"
    } else {
        ""
    };

    format!(
        r#"//! {project_name} - 由 Geese Launcher 自动生成。
//!
//! 模板类型：{template_name}
//! 摄像机：{camera_type}

{mod_decls}
use std::time::Instant;
use winit::{{event_loop::EventLoop, window::WindowAttributes}};

fn main() {{
    env_logger::init();

    let event_loop = EventLoop::new().unwrap();
    let window = winit::window::WindowBuilder::new()
        .with_title("{project_name}")
        .with_inner_size(winit::dpi::LogicalSize::new(1280, 720))
        .build(&event_loop)
        .unwrap();

    // TODO: 初始化 wgpu 设备、渲染器、场景、物理世界
    // TODO: 主循环：输入轮询 → 更新 → 渲染

    println!("🚀 {project_name} 已启动！模板：{template_name}");
}}
"#,
        project_name = "{{project_name}}",
        template_name = template.name,
        camera_type = camera_type,
    )
}

/// 生成 project.toml 配置文件内容。
pub fn project_config_content(template: &ProjectTemplate) -> String {
    let cam = &template.camera_config;
    let player = &template.player_config;
    let cam_type_str = match cam.camera_type {
        CameraType::Empty => "Free",
        CameraType::FirstPerson => "FirstPerson",
        CameraType::ThirdPersonFixed => "ThirdPersonFixed",
        CameraType::TopDown => "TopDown",
    };

    format!(
        r#"# {project_name} 项目配置
# 由 Geese Launcher 自动生成

[project]
name = "{project_name}"
template = "{template_id}"

[camera]
type = "{cam_type}"
fov = {fov}
z_near = {z_near}
z_far = {z_far}
eye_height = {eye_height}
follow_offset = [{follow_x}, {follow_y}, {follow_z}]
follow_smooth = {follow_smooth}

[player]
move_speed = {move_speed}
jump_impulse = {jump_impulse}
capsule_radius = {capsule_radius}
capsule_height = {capsule_height}
mouse_sensitivity = {mouse_sensitivity}

[[input_mappings]]
"#,
        project_name = "{{project_name}}",
        template_id = template.id,
        cam_type = cam_type_str,
        fov = cam.fov,
        z_near = cam.z_near,
        z_far = cam.z_far,
        eye_height = cam.eye_height,
        follow_x = cam.follow_offset.0,
        follow_y = cam.follow_offset.1,
        follow_z = cam.follow_offset.2,
        follow_smooth = cam.follow_smooth,
        move_speed = player.move_speed,
        jump_impulse = player.jump_impulse,
        capsule_radius = player.capsule_radius,
        capsule_height = player.capsule_height,
        mouse_sensitivity = player.mouse_sensitivity,
    )
}
