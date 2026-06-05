//! 场景加载器。
//!
//! 从 `SceneManifest`（`.scene.json`）加载完整的场景：
//! - 合并多个 GLTF 模型（索引导出、变换应用）
//! - 程序化生成内联对象（plane/cube）
//! - 构建最终的 `Scene`

use std::io::Read;
use std::path::Path;

use cgmath::{Euler, Matrix4, Point3, Rad, SquareMatrix, Vector2, Vector3};
use cgmath::Matrix;
use math::AABB;
use render::{Material, MaterialLibrary, MeshFlags, ModelMesh, Vertex, AlphaMode};
use uuid::Uuid;

use crate::manifest::{SceneManifest, TransformDef};
use crate::scene::Scene;
use crate::scene_object::SceneObject;
use crate::scene_object::DirtyFlags;
use crate::import_scene;
use avatar::{SceneNode, Transform};

/// 从 `.scene.json` 文件路径加载场景。
///
/// `scene_path` 是 `.scene.json` 文件的完整路径。
/// GLTF 模型路径相对于 `scene_path` 的父目录解析。
pub fn load_scene_from_file(scene_path: &str, max_objects: usize, max_depth: usize) -> Result<Scene, Box<dyn std::error::Error>> {
    let path = Path::new(scene_path);
    let base_dir = path.parent()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|| ".".to_string());

    let mut file = std::fs::File::open(scene_path)?;
    let mut content = String::new();
    file.read_to_string(&mut content)?;
    let manifest: SceneManifest = serde_json::from_str(&content)?;

    load_scene_from_manifest(&manifest, &base_dir, max_objects, max_depth)
}

/// 从已解析的 `SceneManifest` 构建场景。
pub fn load_scene_from_manifest(
    manifest: &SceneManifest,
    base_path: &str,
    max_objects: usize,
    max_depth: usize,
) -> Result<Scene, Box<dyn std::error::Error>> {
    let mut all_nodes: Vec<SceneNode> = vec![];
    let mut all_objects: Vec<SceneObject> = vec![];
    let mut all_materials = MaterialLibrary::default();
    let mut all_animations: Vec<avatar::AnimationClip> = vec![];
    let mut all_skins: Vec<avatar::Skin> = vec![];
    let mut global_bounds = AABB::new(
        Point3::new(std::f32::MAX, std::f32::MAX, std::f32::MAX),
        Point3::new(std::f32::MIN, std::f32::MIN, std::f32::MIN),
    );

    // 1. 加载每个 GLTF 模型
    for model in &manifest.models {
        let gltf_path = format!("{}/{}", base_path, model.path);
        let mut scene = import_scene(gltf_path, max_objects, max_depth)?;

        let node_offset = all_nodes.len();
        let object_offset = all_objects.len();
        let skin_offset = all_skins.len();
        let material_offset = all_materials.materials.len();

        let transform_matrix = build_transform_matrix(&model.transform);

        // 对根节点应用变换 + 偏移所有索引引用
        for node in &mut scene.nodes {
            if node.parent.is_none() {
                apply_transform_to_root(node, &model.transform);
            }
            offset_node_indices(node, node_offset, object_offset, skin_offset, material_offset);
        }

        // 偏移 SceneObject 中的 node/skin/material 引用
        for obj in &mut scene.objects {
            obj.node += node_offset;
            if let Some(ref mut skin_h) = obj.mesh.skin {
                skin_h.0 += skin_offset;
            }
            if let Some(ref mut mat_h) = obj.mesh.material {
                mat_h.0 += material_offset;
            }
        }

        // 偏移 skin 中的 joint 引用
        for skin in &mut scene.skins {
            for joint in &mut skin.joints {
                *joint += node_offset;
            }
        }

        // 偏移 animation channel 中的 target_node
        for anim in &mut scene.animations {
            for channel in &mut anim.channels {
                channel.target_node += node_offset;
            }
        }

        // 合并 AABB
        let transformed_bounds = transform_aabb_merge(scene.bounds, transform_matrix);
        global_bounds = merge_aabb(global_bounds, transformed_bounds);

        // 追加到全局列表
        all_nodes.append(&mut scene.nodes);
        all_objects.append(&mut scene.objects);
        all_materials.materials.append(&mut scene.materials.materials);
        all_materials.textures.append(&mut scene.materials.textures);
        all_animations.append(&mut scene.animations);
        all_skins.append(&mut scene.skins);
    }

    // 2. 处理程序化内联对象
    for obj_def in &manifest.objects {
        let (mesh, color) = match obj_def.object_type.as_str() {
            "plane" => (
                create_plane_mesh_procedural(obj_def.scale[0], obj_def.scale[2]),
                obj_def.color,
            ),
            "cube" => (
                create_cube_mesh_procedural(obj_def.scale[0], obj_def.scale[1], obj_def.scale[2]),
                obj_def.color,
            ),
            _ => continue,
        };

        let material_idx = all_materials.materials.len();
        all_materials.materials.push(create_pbr_material(&obj_def.object_type, color));

        let mut obj_mesh = mesh;
        obj_mesh.material = Some(render::MaterialHandle(material_idx));

        add_procedural_object(
            &mut all_nodes,
            &mut all_objects,
            obj_mesh,
            obj_def.position,
            obj_def.rotation_euler.unwrap_or([0.0, 0.0, 0.0]),
            obj_def.tag.clone(),
        );

        // 更新全局 AABB
        let pos = Point3::new(obj_def.position[0], obj_def.position[1], obj_def.position[2]);
        let half = Vector3::new(
            obj_def.scale[0] * 0.5,
            obj_def.scale[1] * 0.5,
            obj_def.scale[2] * 0.5,
        );
        global_bounds = merge_aabb(
            global_bounds,
            AABB::new(pos - half, pos + half),
        );
    }

    // 3. 处理环境光照
    setup_environment(&manifest.environment, &mut all_nodes, &mut all_objects);

    // 4. 如果没有任何内容，使用默认 bounds
    if global_bounds.min.x == std::f32::MAX {
        global_bounds = AABB::new(
            Point3::new(-100.0, -100.0, -100.0),
            Point3::new(100.0, 100.0, 100.0),
        );
    }

    // 5. 添加方向光作为 tagged objects（从 environment 提取）
    Ok(Scene::new(
        all_nodes,
        all_objects,
        all_materials,
        all_animations,
        all_skins,
        global_bounds,
        max_objects,
        max_depth,
    ))
}

// ---------------------------------------------------------------------------
// 辅助函数
// ---------------------------------------------------------------------------

/// 构建 4x4 变换矩阵，用于 AABB 变换。
fn build_transform_matrix(tf: &TransformDef) -> Matrix4<f32> {
    use cgmath::Rotation3;
    let translation = Matrix4::from_translation(Vector3::new(
        tf.translation[0],
        tf.translation[1],
        tf.translation[2],
    ));
    let rotation = cgmath::Quaternion::from_angle_y(Rad(tf.rotation[1].to_radians()))
        * cgmath::Quaternion::from_angle_x(Rad(tf.rotation[0].to_radians()))
        * cgmath::Quaternion::from_angle_z(Rad(tf.rotation[2].to_radians()));
    let scale = Matrix4::from_nonuniform_scale(tf.scale[0], tf.scale[1], tf.scale[2]);
    translation * Matrix4::from(rotation) * scale
}

/// 对 GLTF 根节点应用外部变换。
fn apply_transform_to_root(node: &mut SceneNode, tf: &TransformDef) {
    use cgmath::Rotation3;
    let rot = cgmath::Quaternion::from_angle_y(Rad(tf.rotation[1].to_radians()))
        * cgmath::Quaternion::from_angle_x(Rad(tf.rotation[0].to_radians()))
        * cgmath::Quaternion::from_angle_z(Rad(tf.rotation[2].to_radians()));
    node.local_transform.translation =
        Vector3::new(tf.translation[0], tf.translation[1], tf.translation[2]);
    node.local_transform.rotation = rot;
    node.local_transform.scale = Vector3::new(tf.scale[0], tf.scale[1], tf.scale[2]);
}

/// 偏移 SceneNode 中的所有索引引用。
fn offset_node_indices(
    node: &mut SceneNode,
    node_offset: usize,
    object_offset: usize,
    _skin_offset: usize,
    _material_offset: usize,
) {
    node.id += node_offset;
    if let Some(ref mut parent) = node.parent {
        *parent += node_offset;
    }
    for child in &mut node.children {
        *child += node_offset;
    }
    for obj_idx in &mut node.objects {
        *obj_idx += object_offset;
    }
}

/// 合并两个 AABB。
fn merge_aabb(a: AABB, b: AABB) -> AABB {
    AABB {
        min: Point3::new(
            a.min.x.min(b.min.x),
            a.min.y.min(b.min.y),
            a.min.z.min(b.min.z),
        ),
        max: Point3::new(
            a.max.x.max(b.max.x),
            a.max.y.max(b.max.y),
            a.max.z.max(b.max.z),
        ),
    }
}

/// 用变换矩阵变换 AABB 并与目标合并。
fn transform_aabb_merge(mut target: AABB, matrix: Matrix4<f32>) -> AABB {
    use cgmath::Transform;
    let corners = [
        Point3::new(target.min.x, target.min.y, target.min.z),
        Point3::new(target.min.x, target.min.y, target.max.z),
        Point3::new(target.min.x, target.max.y, target.min.z),
        Point3::new(target.min.x, target.max.y, target.max.z),
        Point3::new(target.max.x, target.min.y, target.min.z),
        Point3::new(target.max.x, target.min.y, target.max.z),
        Point3::new(target.max.x, target.max.y, target.min.z),
        Point3::new(target.max.x, target.max.y, target.max.z),
    ];
    target.min = Point3::new(std::f32::MAX, std::f32::MAX, std::f32::MAX);
    target.max = Point3::new(std::f32::MIN, std::f32::MIN, std::f32::MIN);
    for corner in &corners {
        let p = matrix.transform_point(*corner);
        target.min.x = target.min.x.min(p.x);
        target.min.y = target.min.y.min(p.y);
        target.min.z = target.min.z.min(p.z);
        target.max.x = target.max.x.max(p.x);
        target.max.y = target.max.y.max(p.y);
        target.max.z = target.max.z.max(p.z);
    }
    target
}

/// 添加程序化生成的对象。
fn add_procedural_object(
    nodes: &mut Vec<SceneNode>,
    objects: &mut Vec<SceneObject>,
    mesh: ModelMesh,
    position: [f32; 3],
    rotation_euler: [f32; 3],
    _tag: Option<String>,
) {
    let entity_id = Uuid::new_v4().to_string();
    let node_id = nodes.len();

    let translation = Vector3::new(position[0], position[1], position[2]);
    let rotation: cgmath::Quaternion<f32> = Euler::new(
        Rad(rotation_euler[0].to_radians()),
        Rad(rotation_euler[1].to_radians()),
        Rad(rotation_euler[2].to_radians()),
    )
    .into();
    let transform = Transform {
        translation,
        rotation,
        scale: Vector3::new(1.0, 1.0, 1.0),
    };

    let mut node = SceneNode::new(node_id, None, transform);
    node.objects = vec![objects.len()];
    nodes.push(node);

    // local_aabb 在本地坐标系中，相对于节点位置
    let half = Vector3::new(0.5, 0.5, 0.5);
    let local_aabb = AABB::new(
        Point3::new(-half.x, -half.y, -half.z),
        Point3::new(half.x, half.y, half.z),
    );

    let model_matrix = transform.matrix();
    objects.push(SceneObject {
        entity_id,
        node: node_id,
        local_aabb,
        aabb: local_aabb,
        center: Point3::new(0.0, 0.0, 0.0),
        mesh,
        model_matrix: model_matrix.into(),
        normal_matrix: model_matrix.invert().unwrap_or(Matrix4::identity()).transpose().into(),
        joint_matrices: vec![],
        dirty: DirtyFlags::all(),
    });
}

/// 设置环境光照（创建光源节点和对象）。
fn setup_environment(
    _env: &crate::manifest::Environment,
    _nodes: &mut Vec<SceneNode>,
    _objects: &mut Vec<SceneObject>,
) {
    // 方向光信息已保存在 manifest 中，后续渲染器可从此读取。
    // 当前 Scene 通过 AABB bounds 和 frustum 管理渲染，
    // 光源作为场景元数据，随 manifest 一起传递。
}

// ---------------------------------------------------------------------------
// 程序化网格生成（plane / cube）
// ---------------------------------------------------------------------------

fn create_plane_mesh_procedural(size_x: f32, size_z: f32) -> ModelMesh {
    let hx = size_x * 0.5;
    let hz = size_z * 0.5;

    let vertices = vec![
        Vertex {
            position: Point3::new(-hx, 0.0, -hz),
            normal: Vector3::new(0.0, 1.0, 0.0),
            uv: Vector2::new(0.0, 0.0),
            tangent: [1.0, 0.0, 0.0, 1.0],
            joints: [0, 0, 0, 0],
            weights: [1.0, 0.0, 0.0, 0.0],
        },
        Vertex {
            position: Point3::new(hx, 0.0, -hz),
            normal: Vector3::new(0.0, 1.0, 0.0),
            uv: Vector2::new(size_x, 0.0),
            tangent: [1.0, 0.0, 0.0, 1.0],
            joints: [0, 0, 0, 0],
            weights: [1.0, 0.0, 0.0, 0.0],
        },
        Vertex {
            position: Point3::new(hx, 0.0, hz),
            normal: Vector3::new(0.0, 1.0, 0.0),
            uv: Vector2::new(size_x, size_z),
            tangent: [1.0, 0.0, 0.0, 1.0],
            joints: [0, 0, 0, 0],
            weights: [1.0, 0.0, 0.0, 0.0],
        },
        Vertex {
            position: Point3::new(-hx, 0.0, hz),
            normal: Vector3::new(0.0, 1.0, 0.0),
            uv: Vector2::new(0.0, size_z),
            tangent: [1.0, 0.0, 0.0, 1.0],
            joints: [0, 0, 0, 0],
            weights: [1.0, 0.0, 0.0, 0.0],
        },
    ];

    let indices = vec![0, 1, 2, 0, 2, 3];

    let mut mesh = ModelMesh::new();
    mesh.vertices = vertices;
    mesh.indices = indices;
    mesh.flags = MeshFlags {
        has_normals: true,
        has_uv0: true,
        has_tangents: true,
        has_skin: false,
    };
    mesh
}

fn create_cube_mesh_procedural(sx: f32, sy: f32, sz: f32) -> ModelMesh {
    let hx = sx * 0.5;
    let hy = sy * 0.5;
    let hz = sz * 0.5;

    #[rustfmt::skip]
    let positions = [
        [-hx, -hy,  hz], [ hx, -hy,  hz], [ hx,  hy,  hz], [-hx,  hy,  hz], // +Z
        [ hx, -hy, -hz], [-hx, -hy, -hz], [-hx,  hy, -hz], [ hx,  hy, -hz], // -Z
        [ hx, -hy,  hz], [ hx, -hy, -hz], [ hx,  hy, -hz], [ hx,  hy,  hz], // +X
        [-hx, -hy, -hz], [-hx, -hy,  hz], [-hx,  hy,  hz], [-hx,  hy, -hz], // -X
        [-hx,  hy,  hz], [ hx,  hy,  hz], [ hx,  hy, -hz], [-hx,  hy, -hz], // +Y
        [-hx, -hy, -hz], [ hx, -hy, -hz], [ hx, -hy,  hz], [-hx, -hy,  hz], // -Y
    ];

    #[rustfmt::skip]
    let normals = [
        [0.0,0.0,1.0],[0.0,0.0,1.0],[0.0,0.0,1.0],[0.0,0.0,1.0],
        [0.0,0.0,-1.0],[0.0,0.0,-1.0],[0.0,0.0,-1.0],[0.0,0.0,-1.0],
        [1.0,0.0,0.0],[1.0,0.0,0.0],[1.0,0.0,0.0],[1.0,0.0,0.0],
        [-1.0,0.0,0.0],[-1.0,0.0,0.0],[-1.0,0.0,0.0],[-1.0,0.0,0.0],
        [0.0,1.0,0.0],[0.0,1.0,0.0],[0.0,1.0,0.0],[0.0,1.0,0.0],
        [0.0,-1.0,0.0],[0.0,-1.0,0.0],[0.0,-1.0,0.0],[0.0,-1.0,0.0],
    ];

    let uvs = [
        [0.0,0.0],[1.0,0.0],[1.0,1.0],[0.0,1.0],
        [0.0,0.0],[1.0,0.0],[1.0,1.0],[0.0,1.0],
        [0.0,0.0],[1.0,0.0],[1.0,1.0],[0.0,1.0],
        [0.0,0.0],[1.0,0.0],[1.0,1.0],[0.0,1.0],
        [0.0,0.0],[1.0,0.0],[1.0,1.0],[0.0,1.0],
        [0.0,0.0],[1.0,0.0],[1.0,1.0],[0.0,1.0],
    ];

    let vertices: Vec<Vertex> = (0..24)
        .map(|i| Vertex {
            position: Point3::new(positions[i][0], positions[i][1], positions[i][2]),
            normal: Vector3::new(normals[i][0], normals[i][1], normals[i][2]),
            uv: Vector2::new(uvs[i][0], uvs[i][1]),
            tangent: [1.0, 0.0, 0.0, 1.0],
            joints: [0, 0, 0, 0],
            weights: [1.0, 0.0, 0.0, 0.0],
        })
        .collect();

    #[rustfmt::skip]
    let indices = vec![
        0,1,2, 0,2,3, 4,5,6, 4,6,7,
        8,9,10, 8,10,11, 12,13,14, 12,14,15,
        16,17,18, 16,18,19, 20,21,22, 20,22,23,
    ];

    let mut mesh = ModelMesh::new();
    mesh.vertices = vertices;
    mesh.indices = indices;
    mesh.flags = MeshFlags {
        has_normals: true,
        has_uv0: true,
        has_tangents: true,
        has_skin: false,
    };
    mesh
}

fn create_pbr_material(name: &str, color: [f32; 3]) -> Material {
    Material {
        name: Some(name.to_string()),
        base_color_factor: [color[0], color[1], color[2], 1.0],
        metallic_factor: 0.0,
        roughness_factor: 0.8,
        emissive_factor: [0.0, 0.0, 0.0],
        alpha_mode: AlphaMode::Opaque,
        alpha_cutoff: 0.5,
        base_color_texture: None,
        normal_texture: None,
        metallic_roughness_texture: None,
        occlusion_texture: None,
        emissive_texture: None,
        double_sided: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::{SceneManifest, SceneObjectDef};

    #[test]
    fn load_empty_manifest() {
        let manifest = SceneManifest::empty("EmptyTest");
        let scene = load_scene_from_manifest(&manifest, ".", 100, 4);
        assert!(scene.is_ok());
        let scene = scene.unwrap();
        assert!(scene.objects().is_empty());
    }

    #[test]
    fn load_manifest_with_procedural_objects() {
        let mut manifest = SceneManifest::empty("ProcTest");
        manifest.objects.push(SceneObjectDef {
            object_type: "plane".into(),
            position: [0.0, 0.0, 0.0],
            scale: [10.0, 1.0, 10.0],
            color: [0.5, 0.5, 0.5],
            rotation_euler: None,
            tag: None,
        });
        manifest.objects.push(SceneObjectDef {
            object_type: "cube".into(),
            position: [5.0, 0.5, -3.0],
            scale: [1.0, 1.0, 1.0],
            color: [0.8, 0.3, 0.3],
            rotation_euler: Some([0.0, 45.0, 0.0]),
            tag: None,
        });

        let scene = load_scene_from_manifest(&manifest, ".", 100, 4);
        assert!(scene.is_ok());
        let scene = scene.unwrap();
        assert!(scene.objects().len() >= 2);
    }
}
