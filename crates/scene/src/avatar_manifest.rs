//! Avatar 资源清单。
//!
//! 定义 `.avatar.json` 文件结构，描述一个带骨骼动画的角色模型：
//! - 引用的 GLTF 文件路径
//! - 动画列表（名称 + 时长）
//! - 骨骼信息（关节数 + 根关节名）

use serde::{Deserialize, Serialize};

use crate::import_scene;

/// Avatar 资源清单——`.avatar.json` 文件的顶级结构。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AvatarManifest {
    /// 格式版本号，当前为 "1.0"
    pub version: String,
    /// Avatar 名称
    pub name: String,
    /// GLTF 文件路径（相对于清单文件）
    pub gltf_path: String,
    /// 动画列表
    #[serde(default)]
    pub animations: Vec<AvatarAnimationRef>,
    /// 骨骼信息
    #[serde(default)]
    pub skeleton: AvatarSkeletonRef,
}

/// 动画引用——从 GLTF 中预提取的名称与时长。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AvatarAnimationRef {
    /// 动画名称
    pub name: String,
    /// 动画时长（秒）
    pub duration: f32,
}

/// 骨骼引用——关节数量与根关节名。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AvatarSkeletonRef {
    /// 关节数量
    #[serde(default)]
    pub joint_count: usize,
    /// 根关节名称（可选）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub root_joint: Option<String>,
}

impl Default for AvatarSkeletonRef {
    fn default() -> Self {
        Self {
            joint_count: 0,
            root_joint: None,
        }
    }
}

impl AvatarManifest {
    /// 从 GLTF 文件路径解析并构建 Avatar 清单。
    ///
    /// 遍历 GLTF 文档提取：
    /// - 所有动画名称与时长
    /// - 骨骼蒙皮信息（关节数量、根关节名）
    pub fn from_gltf(name: &str, gltf_path: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let scene = import_scene(gltf_path.to_string(), 8, 6)?;

        // 提取动画信息
        let animations: Vec<AvatarAnimationRef> = scene
            .animations
            .iter()
            .map(|clip| AvatarAnimationRef {
                name: clip
                    .name
                    .clone()
                    .unwrap_or_else(|| "unnamed".to_string()),
                duration: clip.duration,
            })
            .collect();

        // 提取骨骼信息：取第一个 skin
        let skeleton = if let Some(skin) = scene.skins.first() {
            AvatarSkeletonRef {
                joint_count: skin.joints.len(),
                root_joint: None, // SceneNode 无 name 字段
            }
        } else {
            AvatarSkeletonRef::default()
        };

        Ok(Self {
            version: "1.0".into(),
            name: name.to_string(),
            gltf_path: gltf_path.to_string(),
            animations,
            skeleton,
        })
    }

    /// 序列化为 JSON 字符串。
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    /// 从 JSON 字符串反序列化。
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serialize_avatar_manifest() {
        let manifest = AvatarManifest {
            version: "1.0".into(),
            name: "Player".into(),
            gltf_path: "models/player.glb".into(),
            animations: vec![
                AvatarAnimationRef {
                    name: "Idle".into(),
                    duration: 2.5,
                },
                AvatarAnimationRef {
                    name: "Run".into(),
                    duration: 0.8,
                },
            ],
            skeleton: AvatarSkeletonRef {
                joint_count: 42,
                root_joint: Some("Hips".into()),
            },
        };

        let json = serde_json::to_string_pretty(&manifest).unwrap();
        let decoded: AvatarManifest = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.name, "Player");
        assert_eq!(decoded.animations.len(), 2);
        assert_eq!(decoded.skeleton.joint_count, 42);
    }

    #[test]
    fn deserialize_minimal_avatar() {
        let json = r#"{
            "version": "1.0",
            "name": "Simple",
            "gltf_path": "models/simple.glb"
        }"#;
        let manifest: AvatarManifest = serde_json::from_str(json).unwrap();
        assert_eq!(manifest.name, "Simple");
        assert!(manifest.animations.is_empty());
        assert_eq!(manifest.skeleton.joint_count, 0);
    }
}
