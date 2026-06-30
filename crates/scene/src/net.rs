//! 场景对象网络复制消息格式。
//!
//! 利用现有 Thrift 协议中的 `create_remote_entity` / `delete_remote_entity`
//! 消息，通过 `entity_type` 区分场景对象类型，`argvs` 承载具体数据。
//!
//! ## Entity Type 常量
//!
//! - `"scene_object_static"`  — 静态场景对象（加载场景时批量创建）
//! - `"scene_object_dynamic"` — 动态场景对象（运行时创建/销毁）
//!
//! ## argvs 格式
//!
//! argvs 为 msgpack 序列化的 `SceneObjectNetMsg`，包含：
//! - `entity_id`: 场景对象唯一 ID
//! - `type`: 对象类型 ("mesh_ref" | "plane" | "cube")
//! - `transform`: 世界变换
//! - `mesh_ref`: 指向 GLTF 文件中某个 mesh 的引用（仅 type="mesh_ref"）
//! - `color`: RGB 颜色（仅程序化对象）
//! - `dimensions`: 尺寸（仅程序化对象）

use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// 静态场景对象的 entity_type 常量。
pub const ENTITY_TYPE_STATIC: &str = "scene_object_static";

/// 动态场景对象的 entity_type 常量。
pub const ENTITY_TYPE_DYNAMIC: &str = "scene_object_dynamic";

/// 场景对象网络消息——填充在 `create_remote_entity.argvs` 中。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SceneObjectNetMsg {
    /// 场景对象唯一 ID
    pub entity_id: String,
    /// 对象类型
    #[serde(rename = "type")]
    pub object_type: SceneObjectNetType,
    /// 世界变换
    pub transform: NetTransform,
    /// 网格引用（仅 "mesh_ref" 类型）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mesh_ref: Option<MeshRef>,
    /// 程序化对象的颜色 (0-1)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<[f32; 3]>,
    /// 程序化对象的尺寸
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dimensions: Option<[f32; 3]>,
    /// 来源 Prefab UUID（Prefab 实例化对象携带）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prefab_uuid: Option<String>,
}

/// 场景对象的网络类型。
#[derive(Debug, Clone)]
pub enum SceneObjectNetType {
    /// 引用 GLTF 模型中的 mesh
    MeshRef,
    /// 程序化平面
    Plane,
    /// 程序化立方体
    Cube,
    /// Prefab 实例（携带 prefab_uuid 元数据）
    PrefabInstance,
}

impl Serialize for SceneObjectNetType {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::MeshRef => serializer.serialize_str("mesh_ref"),
            Self::Plane => serializer.serialize_str("plane"),
            Self::Cube => serializer.serialize_str("cube"),
            Self::PrefabInstance => serializer.serialize_str("prefab_instance"),
        }
    }
}

impl<'de> Deserialize<'de> for SceneObjectNetType {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        match s.as_str() {
            "mesh_ref" => Ok(Self::MeshRef),
            "plane" => Ok(Self::Plane),
            "cube" => Ok(Self::Cube),
            "prefab_instance" => Ok(Self::PrefabInstance),
            _ => Err(serde::de::Error::unknown_variant(&s, &["mesh_ref", "plane", "cube", "prefab_instance"])),
        }
    }
}

/// 网格引用——指向 GLTF 文件中的特定 mesh。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeshRef {
    /// GLTF 文件相对路径
    pub gltf_path: String,
    /// GLTF 场景中的 mesh 名称（可选，默认加载全部）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mesh_name: Option<String>,
}

/// 网络变换。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetTransform {
    /// 位移 (x, y, z)
    pub translation: [f32; 3],
    /// 欧拉角旋转（度）(yaw, pitch, roll)
    pub rotation: [f32; 3],
    /// 缩放 (x, y, z)
    #[serde(default = "default_scale")]
    pub scale: [f32; 3],
}

fn default_scale() -> [f32; 3] {
    [1.0, 1.0, 1.0]
}

impl SceneObjectNetMsg {
    /// 为 GLTF 网格引用创建消息。
    pub fn mesh_ref(
        entity_id: &str,
        gltf_path: &str,
        mesh_name: Option<&str>,
        translation: [f32; 3],
        rotation: [f32; 3],
        scale: [f32; 3],
    ) -> Self {
        Self {
            entity_id: entity_id.to_string(),
            object_type: SceneObjectNetType::MeshRef,
            transform: NetTransform { translation, rotation, scale },
            mesh_ref: Some(MeshRef {
                gltf_path: gltf_path.to_string(),
                mesh_name: mesh_name.map(|s| s.to_string()),
            }),
            color: None,
            dimensions: None,
            prefab_uuid: None,
        }
    }

    /// 为程序化对象创建消息。
    ///
    /// # Panics
    ///
    /// 在 debug 构建中，如果传入 `MeshRef` 类型会触发断言失败。
    pub fn procedural(
        entity_id: &str,
        obj_type: SceneObjectNetType,
        translation: [f32; 3],
        rotation: [f32; 3],
        scale: [f32; 3],
        color: [f32; 3],
        dimensions: [f32; 3],
    ) -> Self {
        debug_assert!(
            !matches!(obj_type, SceneObjectNetType::MeshRef),
            "procedural() called with MeshRef type; use mesh_ref() instead"
        );
        Self {
            entity_id: entity_id.to_string(),
            object_type: obj_type,
            transform: NetTransform { translation, rotation, scale },
            mesh_ref: None,
            color: Some(color),
            dimensions: Some(dimensions),
            prefab_uuid: None,
        }
    }

    /// 序列化为 argvs 二进制（msgpack，struct-as-map 模式兼容 Python msgpack）。
    pub fn to_argvs(&self) -> Result<Vec<u8>, rmp_serde::encode::Error> {
        let mut buf = Vec::new();
        self.serialize(&mut rmp_serde::encode::Serializer::new(&mut buf).with_struct_map())?;
        Ok(buf)
    }

    /// 从 argvs 二进制反序列化。
    pub fn from_argvs(data: &[u8]) -> Result<Self, rmp_serde::decode::Error> {
        rmp_serde::from_slice(data)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serialize_mesh_ref_msg() {
        let msg = SceneObjectNetMsg::mesh_ref(
            "obj-001",
            "assets/models/house.gltf",
            Some("Wall"),
            [10.0, 0.0, 5.0],
            [0.0, 45.0, 0.0],
            [1.0, 1.0, 1.0],
        );
        let argvs = msg.to_argvs().unwrap();
        let decoded: SceneObjectNetMsg = SceneObjectNetMsg::from_argvs(&argvs).unwrap();
        assert_eq!(decoded.entity_id, "obj-001");
        assert!(matches!(decoded.object_type, SceneObjectNetType::MeshRef));
    }

    #[test]
    fn serialize_procedural_msg() {
        let msg = SceneObjectNetMsg::procedural(
            "obj-002",
            SceneObjectNetType::Cube,
            [0.0, 0.5, 0.0],
            [0.0, 0.0, 0.0],
            [2.0, 1.0, 2.0],
            [0.8, 0.3, 0.3],
            [2.0, 1.0, 2.0],
        );
        let argvs = msg.to_argvs().unwrap();
        let decoded = SceneObjectNetMsg::from_argvs(&argvs).unwrap();
        assert_eq!(decoded.entity_id, "obj-002");
        assert_eq!(decoded.color.unwrap(), [0.8, 0.3, 0.3]);
    }

    #[test]
    fn entity_type_constants() {
        assert_eq!(ENTITY_TYPE_STATIC, "scene_object_static");
        assert_eq!(ENTITY_TYPE_DYNAMIC, "scene_object_dynamic");
    }
}
