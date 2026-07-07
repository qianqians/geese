//! Prefab 清单数据结构。
//!
//! 定义 `.prefab.json` 文件的完整格式，支持：
//! - 节点树定义（递归子节点）
//! - GLTF 模型引用或程序化网格
//! - 嵌套 Prefab 引用（prefab_ref）
//! - 变换覆写（overrides）

use serde::{Deserialize, Serialize};

use crate::manifest::{BodyKindDef, NavMeshComponentDef, PhysicsComponentDef, TransformDef};
#[cfg(feature = "event")]
use event::EventComponentDef;

/// Prefab 清单——`.prefab.json` 文件的顶级结构。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrefabManifest {
    /// 格式版本号，当前为 "1.0"
    pub version: String,
    /// Prefab 名称
    pub name: String,
    /// 节点定义列表（线性数组，通过 children 索引构建树）
    #[serde(default)]
    pub nodes: Vec<PrefabNodeDef>,
    /// 根节点索引列表
    #[serde(default)]
    pub root_nodes: Vec<usize>,
}

impl PrefabManifest {
    /// 创建一个最小的空 Prefab 清单。
    pub fn empty(name: &str) -> Self {
        Self {
            version: "1.0".into(),
            name: name.into(),
            nodes: vec![],
            root_nodes: vec![],
        }
    }
}

/// Prefab 节点定义。
///
/// 每个节点可以是：
/// - 一个模型引用（`mesh` 字段）
/// - 一个嵌套 Prefab 引用（`prefab_ref` 字段）
/// - 一个纯变换组节点（两者都为空）
///
/// 设计约束：`mesh` 和 `prefab_ref` 互斥。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrefabNodeDef {
    /// 节点显示名称
    pub name: String,
    /// 局部变换
    #[serde(default)]
    pub transform: TransformDef,
    /// 子节点索引列表（指向 `PrefabManifest.nodes` 中的其他节点）
    #[serde(default)]
    pub children: Vec<usize>,
    /// 网格定义（GLTF 模型引用或程序化网格）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mesh: Option<PrefabMeshDef>,
    /// 嵌套 Prefab 引用——被引用 Prefab 的 UUID
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prefab_ref: Option<String>,
    /// 对嵌套 Prefab 实例的变换覆写
    #[serde(skip_serializing_if = "Option::is_none")]
    pub overrides: Option<PrefabOverrides>,
    /// 物理组件定义。None 表示无物理。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub physics: Option<PhysicsComponentDef>,
    /// NavMesh 组件定义。None 表示不参与导航网格构建。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub navmesh: Option<NavMeshComponentDef>,
    /// Event 组件定义。None 表示无事件组件。
    #[cfg(feature = "event")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub event: Option<EventComponentDef>,
    // ── 旧格式兼容字段（仅反序列化）──
    /// [deprecated] 旧格式 body_kind —— 自动迁移到 physics.body_kind
    #[serde(default, skip_serializing, alias = "body_kind")]
    pub _body_kind: Option<BodyKindDef>,
}

impl PrefabNodeDef {
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

/// 网格定义——支持 GLTF 模型引用和程序化网格。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum PrefabMeshDef {
    /// GLTF 模型引用
    #[serde(rename = "model_ref")]
    ModelRef {
        /// 模型资源的 UUID（对应 .glb/.gltf 的 .meta UUID）
        model_uuid: String,
        /// 指定 GLTF 中的 mesh 名称（可选，默认加载全部）
        #[serde(default)]
        mesh_name: Option<String>,
    },
    /// 程序化网格（plane / cube）
    #[serde(rename = "procedural")]
    Procedural {
        /// 对象类型："plane" | "cube"
        object_type: String,
        /// RGB 颜色
        #[serde(default = "default_procedural_color")]
        color: [f32; 3],
        /// 尺寸
        #[serde(default = "default_procedural_dimensions")]
        dimensions: [f32; 3],
    },
}

fn default_procedural_color() -> [f32; 3] {
    [0.5, 0.5, 0.5]
}

fn default_procedural_dimensions() -> [f32; 3] {
    [1.0, 1.0, 1.0]
}

/// 嵌套 Prefab 实例的变换覆写。
///
/// 所有字段均为可选——未提供的字段使用被引用 Prefab 中定义的原始值。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrefabOverrides {
    /// 位移覆写 (x, y, z)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub translation: Option<[f32; 3]>,
    /// 旋转覆写（欧拉角，度）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rotation: Option<[f32; 3]>,
    /// 缩放覆写 (x, y, z)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scale: Option<[f32; 3]>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deserialize_minimal_prefab() {
        let json = r#"{
            "version": "1.0",
            "name": "EmptyPrefab"
        }"#;
        let manifest: PrefabManifest = serde_json::from_str(json).unwrap();
        assert_eq!(manifest.version, "1.0");
        assert_eq!(manifest.name, "EmptyPrefab");
        assert!(manifest.nodes.is_empty());
        assert!(manifest.root_nodes.is_empty());
    }

    #[test]
    fn deserialize_prefab_with_model_ref() {
        let json = r#"{
            "version": "1.0",
            "name": "BarrelPrefab",
            "nodes": [
                {
                    "name": "Root",
                    "transform": { "translation": [0,0,0], "rotation": [0,0,0], "scale": [1,1,1] },
                    "children": [1],
                    "mesh": { "type": "model_ref", "model_uuid": "abc-123" }
                },
                {
                    "name": "VFX",
                    "transform": { "translation": [0,1,0], "rotation": [0,0,0], "scale": [1,1,1] },
                    "mesh": { "type": "procedural", "object_type": "cube", "color": [1.0,0.3,0.3], "dimensions": [0.5,0.5,0.5] }
                }
            ],
            "root_nodes": [0]
        }"#;
        let manifest: PrefabManifest = serde_json::from_str(json).unwrap();
        assert_eq!(manifest.nodes.len(), 2);
        assert_eq!(manifest.root_nodes, vec![0]);
        assert_eq!(manifest.nodes[0].children, vec![1]);
        match &manifest.nodes[0].mesh {
            Some(PrefabMeshDef::ModelRef { model_uuid, mesh_name }) => {
                assert_eq!(model_uuid, "abc-123");
                assert!(mesh_name.is_none());
            }
            _ => panic!("expected ModelRef"),
        }
    }

    #[test]
    fn deserialize_nested_prefab_ref() {
        let json = r#"{
            "version": "1.0",
            "name": "HouseWithTower",
            "nodes": [
                {
                    "name": "HouseBase",
                    "transform": { "translation": [0,0,0], "rotation": [0,0,0], "scale": [1,1,1] },
                    "mesh": { "type": "model_ref", "model_uuid": "house-gltf-uuid" }
                },
                {
                    "name": "Tower",
                    "transform": { "translation": [5,0,5], "rotation": [0,45,0], "scale": [1,1,1] },
                    "prefab_ref": "watchtower-prefab-uuid",
                    "overrides": { "scale": [1.5, 1.5, 1.5] }
                }
            ],
            "root_nodes": [0, 1]
        }"#;
        let manifest: PrefabManifest = serde_json::from_str(json).unwrap();
        assert_eq!(manifest.nodes.len(), 2);
        assert_eq!(manifest.root_nodes.len(), 2);

        let tower = &manifest.nodes[1];
        assert_eq!(tower.name, "Tower");
        assert!(tower.mesh.is_none());
        assert_eq!(tower.prefab_ref.as_deref(), Some("watchtower-prefab-uuid"));
        let overrides = tower.overrides.as_ref().unwrap();
        assert_eq!(overrides.scale, Some([1.5, 1.5, 1.5]));
        assert!(overrides.translation.is_none());
        assert!(overrides.rotation.is_none());
    }

    #[test]
    fn roundtrip_prefab_manifest() {
        let mut manifest = PrefabManifest::empty("TestRoundtrip");
        manifest.nodes.push(PrefabNodeDef {
            name: "Root".into(),
            transform: TransformDef::default(),
            children: vec![],
            mesh: Some(PrefabMeshDef::ModelRef {
                model_uuid: "test-uuid".into(),
                mesh_name: None,
            }),
            prefab_ref: None,
            overrides: None,
            physics: None,
            navmesh: None,
            event: None,
            _body_kind: None,
        });
        manifest.root_nodes.push(0);

        let json = serde_json::to_string_pretty(&manifest).unwrap();
        let restored: PrefabManifest = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.nodes.len(), 1);
        assert_eq!(restored.root_nodes, vec![0]);
    }

    #[test]
    fn deserialize_prefab_with_overrides() {
        let json = r#"{
            "version": "1.0",
            "name": "OverrideTest",
            "nodes": [
                {
                    "name": "NestedInstance",
                    "transform": { "translation": [10,0,0] },
                    "prefab_ref": "nested-uuid",
                    "overrides": { "translation": [10,2,0], "rotation": [0,90,0] }
                }
            ],
            "root_nodes": [0]
        }"#;
        let manifest: PrefabManifest = serde_json::from_str(json).unwrap();
        let node = &manifest.nodes[0];
        let overrides = node.overrides.as_ref().unwrap();
        assert_eq!(overrides.translation, Some([10.0, 2.0, 0.0]));
        assert_eq!(overrides.rotation, Some([0.0, 90.0, 0.0]));
        assert!(overrides.scale.is_none());
    }
}
