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
    /// 是否为模型生成碰撞体
    #[serde(default)]
    pub collision_enabled: bool,
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
                    "collision_enabled": true
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
                { "object_type": "plane", "position": [0,0,0], "scale": [20,1,20], "color": [0.4,0.4,0.4] }
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
    }
}
