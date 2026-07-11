//! 场景清单数据结构。
//!
//! 定义 `.scene.json` 文件的完整格式，支持：
//! - 多个 GLTF 模型引用
//! - 程序化内联对象（plane/cube）
//! - 环境光照配置
//! - 玩家出生点

use serde::{Deserialize, Serialize};

/// 场景清单——`.scene.json` 文件的顶级结构。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SceneManifest {
    /// 格式版本号，当前为 "1.0"
    pub version: String,
    /// 场景名称
    pub name: String,
    /// GLTF 模型引用列表（可多个）
    #[serde(default)]
    pub models: Vec<ModelRef>,
    /// 环境光照配置
    #[serde(default)]
    pub environment: Environment,
    /// 玩家出生点列表
    #[serde(default)]
    pub spawn_points: Vec<SpawnPoint>,
    /// 程序化内联对象（不依赖外部 GLTF 文件）
    #[serde(default)]
    pub objects: Vec<SceneObjectDef>,
    /// Prefab 实例引用列表（场景加载时自动实例化）
    #[serde(default)]
    pub prefab_instances: Vec<PrefabInstanceDef>,
}

impl SceneManifest {
    /// 创建一个最小的空场景清单。
    pub fn empty(name: &str) -> Self {
        Self {
            version: "1.0".into(),
            name: name.into(),
            models: vec![],
            environment: Environment::default(),
            spawn_points: vec![],
            objects: vec![],
            prefab_instances: vec![],
        }
    }
}

/// GLTF 模型引用。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelRef {
    /// 场景内唯一标识
    pub id: String,
    /// 相对于项目根目录的 GLTF 文件路径
    pub path: String,
    /// 应用到模型根节点的变换
    #[serde(default)]
    pub transform: TransformDef,
    /// 物理组件定义。None 表示无物理。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub physics: Option<PhysicsComponentDef>,
    /// NavMesh 组件定义。None 表示不参与导航网格构建。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub navmesh: Option<NavMeshComponentDef>,
    /// 角色控制器组件定义。None 表示无角色控制器。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub character_controller: Option<CharacterControllerDef>,
    // ── 以下为旧格式兼容字段（仅反序列化，不序列化输出）──
    /// [deprecated] 旧格式 collision_enabled —— 自动迁移到 physics.collision_enabled
    #[serde(default, skip_serializing, alias = "collision_enabled")]
    pub _collision_enabled: Option<bool>,
    /// [deprecated]
    #[serde(default, skip_serializing, alias = "body_kind")]
    pub _body_kind: Option<BodyKindDef>,
}

impl ModelRef {
    /// 获取有效的物理组件定义（兼容旧格式自动迁移）。
    pub fn effective_physics(&self) -> Option<PhysicsComponentDef> {
        self.physics.clone().or_else(|| {
            // 从旧格式字段构造
            let collision = self._collision_enabled.unwrap_or(false);
            let body_kind = self._body_kind.unwrap_or_else(default_body_kind);
            if collision || self._body_kind.is_some() {
                Some(PhysicsComponentDef {
                    collision_enabled: collision,
                    body_kind,
                    ..Default::default()
                })
            } else {
                None
            }
        })
    }
}

/// 3D 变换定义。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransformDef {
    /// 位移 (x, y, z)
    #[serde(default)]
    pub translation: [f32; 3],
    /// 欧拉角旋转（度），顺序 (yaw, pitch, roll)
    #[serde(default)]
    pub rotation: [f32; 3],
    /// 缩放 (x, y, z)
    #[serde(default = "default_scale")]
    pub scale: [f32; 3],
}

fn default_scale() -> [f32; 3] {
    [1.0, 1.0, 1.0]
}

impl Default for TransformDef {
    fn default() -> Self {
        Self {
            translation: [0.0, 0.0, 0.0],
            rotation: [0.0, 0.0, 0.0],
            scale: [1.0, 1.0, 1.0],
        }
    }
}

// ---------------------------------------------------------------------------
// Physics Component
// ---------------------------------------------------------------------------

/// 实体物理组件定义。
/// `None` 表示该实体没有物理组件，`Some` 表示实体参与物理模拟。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PhysicsComponentDef {
    /// 服务器是否运行物理模拟
    #[serde(default = "default_true")]
    pub server_enabled: bool,
    /// 客户端是否运行物理模拟
    #[serde(default = "default_true")]
    pub client_enabled: bool,
    /// 碰撞体开关
    #[serde(default = "default_true")]
    pub collision_enabled: bool,
    /// 物理刚体类型
    #[serde(default = "default_body_kind")]
    pub body_kind: BodyKindDef,
}

fn default_true() -> bool { true }

impl Default for PhysicsComponentDef {
    fn default() -> Self {
        Self {
            server_enabled: true,
            client_enabled: true,
            collision_enabled: true,
            body_kind: BodyKindDef::Fixed,
        }
    }
}

// ---------------------------------------------------------------------------
// NavMesh Component
// ---------------------------------------------------------------------------

/// 实体 NavMesh 组件定义。
/// `None` 表示该实体不参与导航网格构建，`Some` 表示实体提供导航数据。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NavMeshComponentDef {
    /// 服务器是否启用导航网格
    #[serde(default = "default_true")]
    pub server_enabled: bool,
    /// 客户端是否启用导航网格
    #[serde(default = "default_true")]
    pub client_enabled: bool,
    /// 导航代理半径（用于寻路时的碰撞检测）
    #[serde(default = "default_agent_radius")]
    pub agent_radius: f32,
}

fn default_agent_radius() -> f32 {
    0.5
}

impl Default for NavMeshComponentDef {
    fn default() -> Self {
        Self {
            server_enabled: true,
            client_enabled: true,
            agent_radius: 0.5,
        }
    }
}

// ---------------------------------------------------------------------------
// Character Controller Component
// ---------------------------------------------------------------------------

/// 角色控制器组件定义。
///
/// 序列化到 RON 格式示例：
/// ```ron
/// character_controller: Some((
///     move_speed: 5.0,
///     jump_impulse: 8.0,
///     air_control: 0.3,
///     gravity: [0.0, -9.81, 0.0],
///     half_height: 1.0,
///     radius: 0.5,
/// )),
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CharacterControllerDef {
    /// 水平移动速度 (m/s)
    #[serde(default = "default_move_speed")]
    pub move_speed: f32,
    /// 跳跃冲量大小
    #[serde(default = "default_jump_impulse")]
    pub jump_impulse: f32,
    /// 空中移动控制系数 (0-1)
    #[serde(default = "default_air_control")]
    pub air_control: f32,
    /// 重力向量
    #[serde(default = "default_gravity")]
    pub gravity: [f32; 3],
    /// 胶囊体半高（不含半球帽）
    #[serde(default = "default_half_height")]
    pub half_height: f32,
    /// 胶囊体半径
    #[serde(default = "default_radius")]
    pub radius: f32,
}

fn default_move_speed() -> f32 { 5.0 }
fn default_jump_impulse() -> f32 { 8.0 }
fn default_air_control() -> f32 { 0.3 }
fn default_gravity() -> [f32; 3] { [0.0, -9.81, 0.0] }
fn default_half_height() -> f32 { 1.0 }
fn default_radius() -> f32 { 0.5 }

impl Default for CharacterControllerDef {
    fn default() -> Self {
        Self {
            move_speed: default_move_speed(),
            jump_impulse: default_jump_impulse(),
            air_control: default_air_control(),
            gravity: default_gravity(),
            half_height: default_half_height(),
            radius: default_radius(),
        }
    }
}

impl CharacterControllerDef {
    /// 序列化为 RON 格式字符串（作为实体组件字段输出）。
    ///
    /// 输出格式：
    /// ```ron
    /// character_controller: Some((
    ///     move_speed: 5.0,
    ///     jump_impulse: 8.0,
    ///     air_control: 0.3,
    ///     gravity: [0.0, -9.81, 0.0],
    ///     half_height: 1.0,
    ///     radius: 0.5,
    /// )),
    /// ```
    pub fn to_ron_string(&self) -> String {
        let pretty = ron::ser::PrettyConfig::new()
            .depth_limit(2)
            .separate_tuple_members(true)
            .indentor("    ".to_string());
        let inner = ron::ser::to_string_pretty(self, pretty)
            .unwrap_or_else(|_| format!(
                "(move_speed: {}, jump_impulse: {}, air_control: {}, gravity: [{}, {}, {}], half_height: {}, radius: {})",
                self.move_speed, self.jump_impulse, self.air_control,
                self.gravity[0], self.gravity[1], self.gravity[2],
                self.half_height, self.radius,
            ));
        format!("character_controller: Some({}),", inner)
    }

    /// 从 RON 字符串反序列化。
    pub fn from_ron_str(s: &str) -> Result<Self, ron::de::SpannedError> {
        ron::from_str(s)
    }
}

/// 程序化内联对象定义（不依赖外部 glTF 文件）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SceneObjectDef {
    /// 对象类型: "plane" | "cube"
    pub object_type: String,
    /// 世界位置
    pub position: [f32; 3],
    /// 缩放
    #[serde(default = "default_scale")]
    pub scale: [f32; 3],
    /// RGB 颜色 (0.0–1.0)
    #[serde(default = "default_color")]
    pub color: [f32; 3],
    /// 欧拉角旋转（度），可选
    #[serde(default)]
    pub rotation_euler: Option<[f32; 3]>,
    /// 网格类型标识（用于特殊标识，如 "player_spawn"、"directional_light"）
    #[serde(default)]
    pub tag: Option<String>,
    /// 物理组件定义。None 表示无物理。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub physics: Option<PhysicsComponentDef>,
    /// NavMesh 组件定义。None 表示不参与导航网格构建。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub navmesh: Option<NavMeshComponentDef>,
    /// 角色控制器组件定义。None 表示无角色控制器。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub character_controller: Option<CharacterControllerDef>,
    // ── 旧格式兼容字段（仅反序列化）──
    /// [deprecated] 旧格式 body_kind —— 自动迁移到 physics.body_kind
    #[serde(default, skip_serializing, alias = "body_kind")]
    pub _body_kind: Option<BodyKindDef>,
}

impl SceneObjectDef {
    /// 获取有效的物理组件定义（兼容旧格式自动迁移）。
    pub fn effective_physics(&self) -> Option<PhysicsComponentDef> {
        self.physics.clone().or_else(|| {
            self._body_kind.map(|body_kind| PhysicsComponentDef {
                body_kind,
                ..Default::default()
            })
        })
    }
}

/// 场景清单中的物理刚体类型定义。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BodyKindDef {
    /// 固定位置，不受物理影响（地面、墙壁）
    Fixed,
    /// 受重力影响，会下落（石块、箱子）
    Dynamic,
}

pub fn default_body_kind() -> BodyKindDef {
    BodyKindDef::Fixed
}

impl BodyKindDef {
    /// 转换为 physics crate 的 `BodyKind`。
    pub fn to_physics_kind(&self) -> physics::world::BodyKind {
        match self {
            Self::Fixed => physics::world::BodyKind::Fixed,
            Self::Dynamic => physics::world::BodyKind::Dynamic,
        }
    }
}

fn default_color() -> [f32; 3] {
    [0.5, 0.5, 0.5]
}

/// 环境光照配置。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Environment {
    /// 环境光颜色 RGB
    #[serde(default = "default_ambient")]
    pub ambient: [f32; 3],
    /// 方向光列表
    #[serde(default)]
    pub directional_lights: Vec<DirectionalLightDef>,
}

fn default_ambient() -> [f32; 3] {
    [0.1, 0.1, 0.1]
}

impl Default for Environment {
    fn default() -> Self {
        Self {
            ambient: default_ambient(),
            directional_lights: vec![],
        }
    }
}

/// 方向光定义。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DirectionalLightDef {
    /// 光照方向向量 (x, y, z)
    pub direction: [f32; 3],
    /// RGB 颜色
    #[serde(default = "default_light_color")]
    pub color: [f32; 3],
    /// 强度倍率
    #[serde(default = "default_intensity")]
    pub intensity: f32,
}

fn default_light_color() -> [f32; 3] {
    [1.0, 0.95, 0.85]
}

fn default_intensity() -> f32 {
    1.0
}

/// 玩家出生点。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpawnPoint {
    /// 出生点名称
    pub name: String,
    /// 世界位置
    pub position: [f32; 3],
    /// 朝向欧拉角（度），(yaw, pitch, roll)
    pub rotation: [f32; 3],
}

/// Prefab 实例引用——场景加载时自动实例化指定 Prefab。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrefabInstanceDef {
    /// 场景内唯一标识
    pub id: String,
    /// 引用的 Prefab 资源 UUID
    pub prefab_uuid: String,
    /// 应用到 Prefab 实例的世界变换
    #[serde(default)]
    pub transform: TransformDef,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deserialize_minimal_manifest() {
        let json = r#"{
            "version": "1.0",
            "name": "TestScene"
        }"#;
        let manifest: SceneManifest = serde_json::from_str(json).unwrap();
        assert_eq!(manifest.version, "1.0");
        assert_eq!(manifest.name, "TestScene");
        assert!(manifest.models.is_empty());
        assert!(manifest.objects.is_empty());
        assert!(manifest.spawn_points.is_empty());
    }

    #[test]
    fn deserialize_full_manifest() {
        let json = r#"{
            "version": "1.0",
            "name": "FullScene",
            "models": [
                {
                    "id": "house",
                    "path": "assets/models/house.gltf",
                    "transform": { "translation": [10,0,5], "rotation": [0,45,0], "scale": [1,1,1] },
                    "physics": { "body_kind": "fixed", "collision_enabled": true }
                }
            ],
            "environment": {
                "ambient": [0.2, 0.2, 0.25],
                "directional_lights": [
                    { "direction": [-0.5,-1.0,-0.3], "color": [1.0,0.9,0.8], "intensity": 1.2 }
                ]
            },
            "spawn_points": [
                { "name": "player_start", "position": [0,1,0], "rotation": [0,0,0] }
            ],
            "objects": [
                { "object_type": "plane", "position": [0,0,0], "scale": [20,1,20], "color": [0.4,0.4,0.4], "physics": { "body_kind": "fixed" } }
            ]
        }"#;
        let manifest: SceneManifest = serde_json::from_str(json).unwrap();
        assert_eq!(manifest.models.len(), 1);
        assert_eq!(manifest.models[0].id, "house");
        assert_eq!(manifest.objects.len(), 1);
        assert_eq!(manifest.objects[0].object_type, "plane");
        assert_eq!(manifest.spawn_points.len(), 1);
    }

    #[test]
    fn transform_default_scale() {
        let def = TransformDef::default();
        assert_eq!(def.scale, [1.0, 1.0, 1.0]);
        assert_eq!(def.translation, [0.0, 0.0, 0.0]);
    }

    #[test]
    fn scene_object_def_defaults() {
        let json = r#"{"object_type":"cube","position":[0,0,0]}"#;
        let obj: SceneObjectDef = serde_json::from_str(json).unwrap();
        assert_eq!(obj.scale, [1.0, 1.0, 1.0]);
        assert_eq!(obj.color, [0.5, 0.5, 0.5]);
        assert!(obj.rotation_euler.is_none());
        assert!(obj.tag.is_none());
        // 新格式：无 physics 字段表示无物理组件
        assert!(obj.effective_physics().is_none());
    }

    #[test]
    fn backward_compat_old_body_kind() {
        // 旧格式 JSON（body_kind 作为扁平字段）应自动迁移
        let json = r#"{"object_type":"cube","position":[0,0,0],"body_kind":"dynamic"}"#;
        let obj: SceneObjectDef = serde_json::from_str(json).unwrap();
        let phys = obj.effective_physics().unwrap();
        assert_eq!(phys.body_kind, BodyKindDef::Dynamic);
        assert!(phys.server_enabled);
        assert!(phys.client_enabled);
        assert!(phys.collision_enabled);
    }

    #[test]
    fn backward_compat_old_collision() {
        // 旧格式 JSON（collision_enabled + body_kind 作为扁平字段）
        let json = r#"{"id":"test","path":"a.gltf","collision_enabled":true,"body_kind":"fixed"}"#;
        let model: ModelRef = serde_json::from_str(json).unwrap();
        let phys = model.effective_physics().unwrap();
        assert_eq!(phys.body_kind, BodyKindDef::Fixed);
        assert!(phys.collision_enabled);
    }

    #[test]
    fn body_kind_serialization() {
        let def = BodyKindDef::Dynamic;
        let json = serde_json::to_string(&def).unwrap();
        assert_eq!(json, "\"dynamic\"");

        let def = BodyKindDef::Fixed;
        let json = serde_json::to_string(&def).unwrap();
        assert_eq!(json, "\"fixed\"");
    }

    #[test]
    fn body_kind_deserialization() {
        let def: BodyKindDef = serde_json::from_str("\"dynamic\"").unwrap();
        assert_eq!(def, BodyKindDef::Dynamic);

        let def: BodyKindDef = serde_json::from_str("\"fixed\"").unwrap();
        assert_eq!(def, BodyKindDef::Fixed);
    }

    #[test]
    fn to_physics_kind_conversion() {
        assert_eq!(
            BodyKindDef::Fixed.to_physics_kind(),
            physics::world::BodyKind::Fixed
        );
        assert_eq!(
            BodyKindDef::Dynamic.to_physics_kind(),
            physics::world::BodyKind::Dynamic
        );
    }

    // -----------------------------------------------------------------------
    // CharacterControllerDef tests
    // -----------------------------------------------------------------------

    #[test]
    fn character_controller_defaults() {
        let def = CharacterControllerDef::default();
        assert!((def.move_speed - 5.0).abs() < f32::EPSILON);
        assert!((def.jump_impulse - 8.0).abs() < f32::EPSILON);
        assert!((def.air_control - 0.3).abs() < f32::EPSILON);
        assert_eq!(def.gravity, [0.0, -9.81, 0.0]);
        assert!((def.half_height - 1.0).abs() < f32::EPSILON);
        assert!((def.radius - 0.5).abs() < f32::EPSILON);
    }

    #[test]
    fn character_controller_ron_roundtrip() {
        let def = CharacterControllerDef {
            move_speed: 5.0,
            jump_impulse: 8.0,
            air_control: 0.3,
            gravity: [0.0, -9.81, 0.0],
            half_height: 1.0,
            radius: 0.5,
        };
        // 序列化为 RON
        let ron_str = ron::to_string(&def).unwrap();
        // 反序列化回来
        let restored: CharacterControllerDef = ron::from_str(&ron_str).unwrap();
        assert_eq!(def, restored);
    }

    #[test]
    fn character_controller_ron_output_format() {
        let def = CharacterControllerDef::default();
        let output = def.to_ron_string();
        // 验证输出包含关键字段
        assert!(output.contains("character_controller: Some("), "output: {output}");
        assert!(output.contains("move_speed:"), "output: {output}");
        assert!(output.contains("jump_impulse:"), "output: {output}");
        assert!(output.contains("air_control:"), "output: {output}");
        assert!(output.contains("gravity:"), "output: {output}");
        assert!(output.contains("half_height:"), "output: {output}");
        assert!(output.contains("radius:"), "output: {output}");
    }

    #[test]
    fn character_controller_json_roundtrip() {
        let def = CharacterControllerDef {
            move_speed: 7.5,
            jump_impulse: 10.0,
            air_control: 0.5,
            gravity: [0.0, -10.0, 0.0],
            half_height: 1.2,
            radius: 0.4,
        };
        let json = serde_json::to_string(&def).unwrap();
        let restored: CharacterControllerDef = serde_json::from_str(&json).unwrap();
        assert_eq!(def, restored);
    }

    #[test]
    fn character_controller_json_defaults() {
        // 空 JSON 对象应能反序列化为默认值
        let def: CharacterControllerDef = serde_json::from_str("{}").unwrap();
        assert_eq!(def, CharacterControllerDef::default());
    }

    #[test]
    fn manifest_with_character_controller() {
        let json = r#"{
            "version": "1.0",
            "name": "CCScene",
            "objects": [
                {
                    "object_type": "cube",
                    "position": [0,0,0],
                    "character_controller": {
                        "move_speed": 6.0,
                        "jump_impulse": 10.0,
                        "air_control": 0.5,
                        "gravity": [0.0, -9.81, 0.0],
                        "half_height": 1.0,
                        "radius": 0.5
                    }
                }
            ]
        }"#;
        let manifest: SceneManifest = serde_json::from_str(json).unwrap();
        let cc = manifest.objects[0].character_controller.as_ref().unwrap();
        assert!((cc.move_speed - 6.0).abs() < f32::EPSILON);
        assert!((cc.jump_impulse - 10.0).abs() < f32::EPSILON);
    }

    #[test]
    fn manifest_without_character_controller() {
        let json = r#"{"object_type":"cube","position":[0,0,0]}"#;
        let obj: SceneObjectDef = serde_json::from_str(json).unwrap();
        assert!(obj.character_controller.is_none());
    }
}
