//! Prefab 加载器。
//!
//! 从 `PrefabManifest`（`.prefab.json`）实例化 Prefab：
//! - 支持 GLTF 模型引用（通过 `import_scene` 加载）
//! - 支持程序化网格（plane/cube）
//! - 支持嵌套 Prefab 引用（递归实例化，含深度限制）
//! - 实例化结果合并到目标 `Scene`

use asset::database::AssetDatabase;
use cgmath::{Euler, InnerSpace, Matrix3, Matrix4, Point3, Quaternion, Rad, SquareMatrix, Vector3};
use cgmath::Matrix;
use math::AABB;
use uuid::Uuid;

use crate::manifest::TransformDef;
use crate::prefab_manifest::{PrefabManifest, PrefabMeshDef, PrefabNodeDef, PrefabOverrides};
use crate::scene::Scene;
use crate::scene_object::{DirtyFlags, SceneObject};
use crate::{import_scene, loader};
use avatar::{SceneNode, Transform};

/// 从 `.prefab.json` 文件路径加载 Prefab 清单。
pub fn load_prefab_manifest(path: &str) -> Result<PrefabManifest, Box<dyn std::error::Error>> {
    let content = std::fs::read_to_string(path)?;
    let manifest: PrefabManifest = serde_json::from_str(&content)?;
    Ok(manifest)
}

/// 将 Prefab 实例化到目标场景中。
///
/// # 参数
/// - `scene`: 目标场景
/// - `manifest`: Prefab 清单
/// - `world_transform`: 应用到根节点的世界变换
/// - `database`: 资源数据库（用于 UUID→路径 解析）
/// - `max_depth`: 递归深度上限（到达后停止展开嵌套 Prefab）
///
/// # 返回
/// 新创建的 `entity_id` 列表（仅根节点对应的实体，不含子孙）。
pub fn instantiate_prefab(
    scene: &mut Scene,
    manifest: &PrefabManifest,
    world_transform: &TransformDef,
    database: &AssetDatabase,
    max_depth: u32,
) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    if max_depth == 0 {
        log::warn!(
            "[prefab] max_depth reached for '{}', stopping recursion",
            manifest.name
        );
        return Ok(vec![]);
    }

    // 循环依赖检测：检查此 Prefab 的依赖链是否存在环路
    let prefab_metas = database.all_metas();
    let cycles = asset::dependency_scanner::check_prefab_cycle(&prefab_metas);
    if !cycles.is_empty() {
        let cycle_desc: Vec<String> = cycles.iter()
            .map(|c| c.join(" → "))
            .collect();
        return Err(format!(
            "[prefab] cycle dependency detected, refusing to instantiate '{}': {}",
            manifest.name,
            cycle_desc.join("; ")
        ).into());
    }

    // 记录实例化前的对象数量，用于准确标记 prefab_source
    let object_offset_before = scene.objects.len();

    let mut created_root_ids: Vec<String> = Vec::new();

    for &root_idx in &manifest.root_nodes {
        if root_idx >= manifest.nodes.len() {
            log::warn!(
                "[prefab] invalid root node index {} in '{}'",
                root_idx, manifest.name
            );
            continue;
        }
        let node_def = &manifest.nodes[root_idx];
        let ids = instantiate_node_recursive(
            scene,
            manifest,
            node_def,
            None, // parent node index
            world_transform,
            database,
            max_depth,
            &manifest.name,
        )?;
        // 顶层根节点对应的 entity_id 收集到返回列表中
        if let Some(first_id) = ids.first() {
            created_root_ids.push(first_id.clone());
        }
    }

    // 标记所有新创建的对象来源于此 Prefab
    for obj in &mut scene.objects[object_offset_before..] {
        obj.prefab_source = Some(manifest.name.clone());
    }

    // 对所有节点更新世界变换（包含新创建的节点）+ 重建 octree
    scene.update_world_transforms();
    scene.rebuild_octree();

    Ok(created_root_ids)
}

/// 递归实例化单个 Prefab 节点（及其子节点）。
///
/// 返回该节点及其所有子节点创建的 entity_id 列表。
fn instantiate_node_recursive(
    scene: &mut Scene,
    manifest: &PrefabManifest,
    node_def: &PrefabNodeDef,
    parent_node_idx: Option<usize>,
    world_transform: &TransformDef,
    database: &AssetDatabase,
    max_depth: u32,
    prefab_name: &str,
) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let mut created_entity_ids: Vec<String> = Vec::new();

    // 计算该节点的最终变换（world_transform × 节点局部变换）
    let local_tf = combine_transforms(world_transform, &node_def.transform);

    if let Some(ref mesh_def) = node_def.mesh {
        // ── 网格节点：加载 GLTF 或创建程序化网格 ──
        let ids = instantiate_mesh_node(scene, node_def, mesh_def, &local_tf, parent_node_idx, database)?;
        created_entity_ids.extend(ids);
    } else if let Some(ref nested_prefab_uuid) = node_def.prefab_ref {
        // ── 嵌套 Prefab 节点：递归实例化 ──
        let ids = instantiate_nested_prefab(
            scene,
            nested_prefab_uuid,
            &local_tf,
            node_def.overrides.as_ref(),
            database,
            max_depth,
        )?;
        created_entity_ids.extend(ids);
    } else {
        // ── 纯变换组节点（无 mesh 也无 prefab_ref）──
        let entity_id = create_empty_node(scene, node_def, &local_tf, parent_node_idx);
        created_entity_ids.push(entity_id);
    }

    // 递归处理子节点
    // 子节点共享父节点的世界变换
    // 注意：第一个创建的 entity 对应的 node 是该节点的"代表"
    let representative_node_idx = if let Some(first_id) = created_entity_ids.first() {
        scene.objects.iter().position(|o| &o.entity_id == first_id).map(|obj_idx| scene.objects[obj_idx].node)
    } else {
        None
    };

    for &child_idx in &node_def.children {
        if child_idx >= manifest.nodes.len() {
            log::warn!(
                "[prefab] invalid child node index {} in '{}'",
                child_idx, prefab_name
            );
            continue;
        }
        let child_def = &manifest.nodes[child_idx];
        let child_ids = instantiate_node_recursive(
            scene,
            manifest,
            child_def,
            representative_node_idx.or(parent_node_idx),
            &local_tf, // 子节点继承父节点的累积变换
            database,
            max_depth,
            prefab_name,
        )?;
        // 子节点的 entity_id 不加入当前节点的返回列表
        let _ = child_ids;
    }

    Ok(created_entity_ids)
}

/// 实例化网格节点：加载 GLTF 或创建程序化网格。
fn instantiate_mesh_node(
    scene: &mut Scene,
    node_def: &PrefabNodeDef,
    mesh_def: &PrefabMeshDef,
    transform: &TransformDef,
    parent_node_idx: Option<usize>,
    database: &AssetDatabase,
) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    match mesh_def {
        PrefabMeshDef::ModelRef { model_uuid, mesh_name: _ } => {
            // 通过 UUID 解析 GLTF 文件路径
            let entry = database
                .entry_by_uuid(model_uuid)
                .ok_or_else(|| format!("[prefab] model UUID '{}' not found in database", model_uuid))?;
            let gltf_path = database
                .project_root()
                .join(&entry.path)
                .to_string_lossy()
                .to_string();

            // 使用 import_scene 加载 GLTF → 合并到当前场景
            instantiate_gltf_model(scene, &gltf_path, transform, parent_node_idx, node_def)
        }
        PrefabMeshDef::Procedural { object_type, color, dimensions } => {
            let entity_id = instantiate_procedural_mesh(scene, node_def, object_type, color, dimensions, transform, parent_node_idx);
            Ok(vec![entity_id])
        }
    }
}

/// 加载 GLTF 模型并合并到当前场景。
fn instantiate_gltf_model(
    scene: &mut Scene,
    gltf_path: &str,
    transform: &TransformDef,
    _parent_node_idx: Option<usize>,
    _node_def: &PrefabNodeDef,
) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    // 加载 GLTF 到一个临时 Scene
    let mut temp_scene = import_scene(gltf_path.to_string(), scene.objects.capacity(), crate::loader::DEFAULT_MAX_PREFAB_DEPTH as usize)?;

    let node_offset = scene.nodes.len();
    let object_offset = scene.objects.len();
    let material_offset = scene.materials.materials.len();
    let skin_offset = scene.skins.len();
    let _anim_offset = scene.animations.len();

    // 偏移所有索引引用
    for node in &mut temp_scene.nodes {
        if node.parent.is_none() {
            loader::apply_transform_to_root(node, transform);
        }
        loader::offset_node_indices(node, node_offset, object_offset, skin_offset, material_offset);
    }
    for obj in &mut temp_scene.objects {
        obj.node += node_offset;
        if let Some(ref mut skin_h) = obj.mesh.skin {
            skin_h.0 += skin_offset;
        }
        if let Some(ref mut mat_h) = obj.mesh.material {
            mat_h.0 += material_offset;
        }
        obj.prefab_source = None; // GLTF 加载的对象没有 prefab source
    }
    for skin in &mut temp_scene.skins {
        for joint in &mut skin.joints {
            *joint += node_offset;
        }
    }
    for anim in &mut temp_scene.animations {
        for channel in &mut anim.channels {
            channel.target_node += node_offset;
        }
    }

    // 收集根节点的 entity_id
    let root_entity_ids: Vec<String> = temp_scene
        .nodes
        .iter()
        .filter(|n| n.parent.is_none())
        .flat_map(|n| &n.objects)
        .filter_map(|&obj_idx| temp_scene.objects.get(obj_idx))
        .map(|obj| obj.entity_id.clone())
        .collect();

    // 合并到主场景
    scene.nodes.append(&mut temp_scene.nodes);
    scene.objects.append(&mut temp_scene.objects);
    scene.materials.materials.append(&mut temp_scene.materials.materials);
    scene.materials.textures.append(&mut temp_scene.materials.textures);
    scene.animations.append(&mut temp_scene.animations);
    scene.skins.append(&mut temp_scene.skins);

    // 重建 static/dynamic 索引——新对象刚追加，不可能在旧索引中，直接分类
    for obj_idx in object_offset..scene.objects.len() {
        let obj = &scene.objects[obj_idx];
        // 动态判定：有 skin 或被动画 channel 直接驱动的节点
        let is_dynamic = obj.mesh.skin.is_some();
        if is_dynamic {
            scene.dynamic_indices_mut().push(obj_idx);
        } else {
            scene.static_indices_mut().push(obj_idx);
        }
    }

    Ok(root_entity_ids)
}

/// 创建程序化网格节点。
fn instantiate_procedural_mesh(
    scene: &mut Scene,
    _node_def: &PrefabNodeDef,
    object_type: &str,
    color: &[f32; 3],
    dimensions: &[f32; 3],
    transform: &TransformDef,
    parent_node_idx: Option<usize>,
) -> String {
    let mesh = match object_type {
        "plane" => crate::primitives::create_plane_mesh_procedural(dimensions[0], dimensions[2]),
        "cube" => crate::primitives::create_cube_mesh_procedural(dimensions[0], dimensions[1], dimensions[2]),
        "sphere" => crate::primitives::create_sphere_mesh_procedural(dimensions[0] * 0.5, 32, 16),
        "cylinder" => crate::primitives::create_cylinder_mesh_procedural(dimensions[0] * 0.5, dimensions[1], 32),
        _ => {
            log::warn!(
                "[prefab] unknown procedural type '{}', defaulting to cube",
                object_type
            );
            crate::primitives::create_cube_mesh_procedural(dimensions[0], dimensions[1], dimensions[2])
        }
    };

    let entity_id = Uuid::new_v4().to_string();
    let node_id = scene.nodes.len();
    let object_index = scene.objects.len();

    let translation = Vector3::new(transform.translation[0], transform.translation[1], transform.translation[2]);
    let rotation: cgmath::Quaternion<f32> = Euler::new(
        Rad(transform.rotation[0].to_radians()),
        Rad(transform.rotation[1].to_radians()),
        Rad(transform.rotation[2].to_radians()),
    ).into();
    let scale = Vector3::new(transform.scale[0], transform.scale[1], transform.scale[2]);

    let node_transform = Transform { translation, rotation, scale };
    let mut node = SceneNode::new(node_id, parent_node_idx, node_transform);
    node.objects = vec![object_index];
    scene.nodes.push(node);

    let half = Vector3::new(dimensions[0] * 0.5, dimensions[1] * 0.5, dimensions[2] * 0.5);
    let local_aabb = AABB::new(
        Point3::new(-half.x, -half.y, -half.z),
        Point3::new(half.x, half.y, half.z),
    );
    let model_matrix = node_transform.matrix();

    let mut obj_mesh = mesh;
    // 创建默认 PBR 材质
    let material_idx = scene.materials.materials.len();
    scene.materials.materials.push(loader::create_pbr_material(object_type, *color));
    obj_mesh.material = Some(render::MaterialHandle(material_idx));

    scene.objects.push(SceneObject {
        entity_id: entity_id.clone(),
        node: node_id,
        local_aabb,
        aabb: local_aabb,
        center: Point3::new(0.0, 0.0, 0.0),
        mesh: obj_mesh,
        model_matrix: model_matrix.into(),
        normal_matrix: model_matrix
            .invert()
            .unwrap_or(Matrix4::identity())
            .transpose()
            .into(),
        joint_matrices: vec![],
        dirty: DirtyFlags::all(),
        prefab_source: None,
    });

    // 默认标记为 static
    scene.static_indices_mut().push(object_index);
    entity_id
}

/// 创建纯变换组节点（无 mesh 的中间节点）。
fn create_empty_node(
    scene: &mut Scene,
    _node_def: &PrefabNodeDef,
    transform: &TransformDef,
    parent_node_idx: Option<usize>,
) -> String {
    let entity_id = Uuid::new_v4().to_string();
    let node_id = scene.nodes.len();

    let translation = Vector3::new(transform.translation[0], transform.translation[1], transform.translation[2]);
    let rotation: cgmath::Quaternion<f32> = Euler::new(
        Rad(transform.rotation[0].to_radians()),
        Rad(transform.rotation[1].to_radians()),
        Rad(transform.rotation[2].to_radians()),
    ).into();
    let scale = Vector3::new(transform.scale[0], transform.scale[1], transform.scale[2]);

    let node_transform = Transform { translation, rotation, scale };
    let node = SceneNode::new(node_id, parent_node_idx, node_transform);
    scene.nodes.push(node);
    entity_id
}

/// 递归实例化嵌套 Prefab。
fn instantiate_nested_prefab(
    scene: &mut Scene,
    prefab_uuid: &str,
    transform: &TransformDef,
    overrides: Option<&PrefabOverrides>,
    database: &AssetDatabase,
    max_depth: u32,
) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    // 通过 UUID 解析 Prefab 文件路径
    let entry = database
        .entry_by_uuid(prefab_uuid)
        .ok_or_else(|| format!("[prefab] nested prefab UUID '{}' not found", prefab_uuid))?;
    let prefab_path = database
        .project_root()
        .join(&entry.path)
        .to_string_lossy()
        .to_string();

    let nested_manifest = load_prefab_manifest(&prefab_path)?;

    // 应用覆写：合并 overrides 到 transform
    let final_transform = if let Some(ov) = overrides {
        TransformDef {
            translation: ov.translation.unwrap_or(transform.translation),
            rotation: ov.rotation.unwrap_or(transform.rotation),
            scale: ov.scale.unwrap_or(transform.scale),
        }
    } else {
        transform.clone()
    };

    // 记录实例化前的对象数量
    let obj_offset = scene.objects.len();
    let ids = instantiate_prefab(
        scene,
        &nested_manifest,
        &final_transform,
        database,
        max_depth - 1,
    )?;

    // 标记所有新创建的对象来源于此嵌套 Prefab
    for obj in &mut scene.objects[obj_offset..] {
        obj.prefab_source = Some(prefab_uuid.to_string());
    }

    Ok(ids)
}

/// 合并两个 TransformDef（world × local）。
///
/// 使用矩阵乘法正确组合变换：
/// `combined = Matrix4::from(world) * Matrix4::from(local)`
/// 然后从结果矩阵分解出 translation / rotation / scale。
fn combine_transforms(world: &TransformDef, local: &TransformDef) -> TransformDef {
    let wm = loader::build_transform_matrix(world);
    let lm = loader::build_transform_matrix(local);
    let combined = wm * lm;

    // 提取平移（矩阵第四列的 xyz）
    let translation = [combined.w.x, combined.w.y, combined.w.z];

    // 提取缩放（各列 xyz 分量的长度）
    let sx = Vector3::new(combined.x.x, combined.x.y, combined.x.z).magnitude();
    let sy = Vector3::new(combined.y.x, combined.y.y, combined.y.z).magnitude();
    let sz = Vector3::new(combined.z.x, combined.z.y, combined.z.z).magnitude();
    let scale = [sx, sy, sz];

    // 提取旋转（归一化列向量→旋转矩阵→四元数→欧拉角）
    // cgmath Matrix3::new 接受列优先参数：(c0r0, c0r1, c0r2, c1r0, c1r1, c1r2, c2r0, c2r1, c2r2)
    let sx_s = if sx.abs() < 1e-8 { 1e-8 } else { sx };
    let sy_s = if sy.abs() < 1e-8 { 1e-8 } else { sy };
    let sz_s = if sz.abs() < 1e-8 { 1e-8 } else { sz };
    let rot_mat = Matrix3::new(
        combined.x.x / sx_s, combined.x.y / sx_s, combined.x.z / sx_s, // column 0 (x column / sx)
        combined.y.x / sy_s, combined.y.y / sy_s, combined.y.z / sy_s, // column 1 (y column / sy)
        combined.z.x / sz_s, combined.z.y / sz_s, combined.z.z / sz_s, // column 2 (z column / sz)
    );
    let quat: Quaternion<f32> = rot_mat.into();
    let euler: Euler<Rad<f32>> = quat.into();

    TransformDef {
        translation,
        rotation: [
            euler.x.0.to_degrees(),
            euler.y.0.to_degrees(),
            euler.z.0.to_degrees(),
        ],
        scale,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn combine_transforms_uses_matrix_multiplication() {
        // 无旋转时，平移直接相加
        let world = TransformDef { translation: [10.0, 0.0, 5.0], rotation: [0.0, 0.0, 0.0], scale: [1.0, 1.0, 1.0] };
        let local = TransformDef { translation: [1.0, 2.0, 3.0], rotation: [0.0, 0.0, 0.0], scale: [1.0, 1.0, 1.0] };
        let combined = combine_transforms(&world, &local);
        assert!((combined.translation[0] - 11.0).abs() < 0.01);
        assert!((combined.translation[1] - 2.0).abs() < 0.01);
        assert!((combined.translation[2] - 8.0).abs() < 0.01);

        // 有旋转时，local 平移被 world 旋转后再加到 world 平移上
        // world = 90° Y 旋转 + 平移 (10, 0, 0)
        // local = 平移 (1, 0, 0)
        // 90° Y 旋转 (1,0,0) → (0, 0, -1)
        // combined translation = (10+0, 0+0, 0+(-1)) = (10, 0, -1)
        let world = TransformDef { translation: [10.0, 0.0, 0.0], rotation: [0.0, 90.0, 0.0], scale: [1.0, 1.0, 1.0] };
        let local = TransformDef { translation: [1.0, 0.0, 0.0], rotation: [0.0, 0.0, 0.0], scale: [1.0, 1.0, 1.0] };
        let combined = combine_transforms(&world, &local);
        assert!((combined.translation[0] - 10.0).abs() < 0.01, "x: {}", combined.translation[0]);
        assert!((combined.translation[2] - (-1.0)).abs() < 0.01, "z: {}", combined.translation[2]);

        // 同轴旋转叠加（使用 < 90° 避免欧拉角 asin 歧义）
        let world = TransformDef { translation: [0.0; 3], rotation: [0.0, 20.0, 0.0], scale: [1.0; 3] };
        let local = TransformDef { translation: [0.0; 3], rotation: [0.0, 30.0, 0.0], scale: [1.0; 3] };
        let combined = combine_transforms(&world, &local);
        assert!((combined.rotation[1] - 50.0).abs() < 0.5, "yaw: {}", combined.rotation[1]);

        // 缩放叠加
        let world = TransformDef { translation: [0.0; 3], rotation: [0.0; 3], scale: [2.0, 3.0, 4.0] };
        let local = TransformDef { translation: [0.0; 3], rotation: [0.0; 3], scale: [1.0, 2.0, 0.5] };
        let combined = combine_transforms(&world, &local);
        assert!((combined.scale[0] - 2.0).abs() < 0.01);
        assert!((combined.scale[1] - 6.0).abs() < 0.01);
        assert!((combined.scale[2] - 2.0).abs() < 0.01);
    }

    #[test]
    fn load_prefab_manifest_from_minimal_json() {
        let tmp = std::env::temp_dir().join("prefab_loader_test");
        let _ = std::fs::create_dir_all(&tmp);
        let path = tmp.join("minimal.prefab.json");
        let json = r#"{"version":"1.0","name":"Minimal","nodes":[],"root_nodes":[]}"#;
        std::fs::write(&path, json).unwrap();

        let manifest = load_prefab_manifest(&path.to_string_lossy()).unwrap();
        assert_eq!(manifest.name, "Minimal");
        assert!(manifest.nodes.is_empty());

        let _ = std::fs::remove_dir_all(&tmp);
    }
}
