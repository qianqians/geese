//! 子网格替换：把一个对象的网格换成另一个场景里某个对象的网格。
//!
//! 只吃 `&mut Scene`，不碰 UI 与记录（那些在 `model.rs` / `app.rs`），所以可以直接
//! 用手工构造的场景做单元测试。
//!
//! ## 蒙皮规则
//!
//! 场景节点没有名字，跨文件无法按骨骼名重绑，只能按**关节下标**对应：
//!
//! | 目标 | 源 | 行为 |
//! | --- | --- | --- |
//! | 有 skin | 有 skin，关节数相同 | 新 `Skin` = 目标的 `joints` + 源的 `inverse_bind_matrices` |
//! | 有 skin | 有 skin，关节数不同 | 拒绝（[`SwapError::JointMismatch`]） |
//! | 有 skin | 无 skin | 刚性换上（`skin = None`），跟着目标节点变换 |
//! | 无 skin | 无 skin | 纯几何替换 |
//! | 无 skin | 有 skin | 拒绝（[`SwapError::TargetRigid`]） |
//!
//! ## 被换下来的网格必须继续存活
//!
//! `render` 的 mesh 缓冲缓存键是 `(顶点指针, 顶点数, 索引指针, 索引数)`
//! （`crates/render/src/common.rs` 的 `compute_mesh_key`），且只插不淘汰。若被换下来的
//! `ModelMesh` 被丢弃、之后新分配的顶点数组复用了同一地址与长度，就会命中旧键、渲染出
//! 上一套几何。因此 [`apply`] / [`restore`] 都把换下来的网格**返回给调用方**，由调用方
//! 持有（见 `model.rs` 的 `retired_meshes`）。

use cgmath::Point3;
use math::AABB;
use render::{MaterialHandle, ModelMesh, SkinHandle};
use scene::{DirtyFlags, Scene, Skin};

/// 替换失败的原因。
#[derive(Debug, PartialEq, Eq)]
pub enum SwapError {
    /// 目标或源的对象下标越界。
    OutOfRange,
    /// 目标与源的关节数不一致。
    JointMismatch { target: usize, source: usize },
    /// 目标没有蒙皮，装不了带蒙皮的源网格。
    TargetRigid,
}

impl std::fmt::Display for SwapError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SwapError::OutOfRange => write!(f, "子网格下标越界"),
            SwapError::JointMismatch { target, source } => {
                write!(f, "源网格 {source} 关节 ≠ 目标 {target} 关节，未替换")
            }
            SwapError::TargetRigid => write!(f, "目标子网格没有蒙皮，无法绑定带蒙皮的源网格"),
        }
    }
}

/// 把 `scene` 里 `target` 对象的网格换成 `source` 里 `source_object` 的网格。
///
/// `material_offset` 是调用方把源文件的材质库整库 `append_library` 进 `scene.materials`
/// 时拿到的偏移量——源网格的材质句柄要按它平移。
///
/// 返回被换下来的原网格，**调用方必须继续持有**（见模块文档）。
pub fn apply(
    scene: &mut Scene,
    target: usize,
    source: &Scene,
    source_object: usize,
    material_offset: usize,
) -> Result<ModelMesh, SwapError> {
    let mut new_mesh = source
        .objects
        .get(source_object)
        .ok_or(SwapError::OutOfRange)?
        .mesh
        .clone();

    if scene.objects.get(target).is_none() {
        return Err(SwapError::OutOfRange);
    }

    // 材质句柄按整库偏移平移；偏移越界说明调用方没搬材质库，宁可退回默认材质。
    if let Some(handle) = new_mesh.material {
        let shifted = material_offset + handle.0;
        if shifted < scene.materials.materials.len() {
            new_mesh.material = Some(MaterialHandle(shifted));
        } else {
            log::warn!("源材质句柄 {} 超出材质库（偏移 {}），改用默认材质", handle.0, material_offset);
            new_mesh.material = None;
        }
    }

    // 蒙皮
    match (new_mesh.skin, scene.objects[target].mesh.skin) {
        (Some(source_skin), Some(target_skin)) => {
            let source_joints = source
                .skins
                .get(source_skin.0)
                .map(|skin| skin.inverse_bind_matrices.len())
                .ok_or(SwapError::OutOfRange)?;
            let target_joints = scene
                .skins
                .get(target_skin.0)
                .map(|skin| skin.joints.clone())
                .ok_or(SwapError::OutOfRange)?;

            if source_joints != target_joints.len() {
                return Err(SwapError::JointMismatch {
                    target: target_joints.len(),
                    source: source_joints,
                });
            }

            // 骨骼顺序沿用目标骨架，绑定姿势用源网格自己的逆绑定矩阵。
            let inverse_bind_matrices = source.skins[source_skin.0].inverse_bind_matrices.clone();
            let handle = SkinHandle(scene.skins.len());
            scene.skins.push(Skin {
                joints: target_joints,
                inverse_bind_matrices,
            });

            new_mesh.skin = Some(handle);
            new_mesh.flags.has_skin = true;
        }
        (Some(_), None) => return Err(SwapError::TargetRigid),
        (None, _) => {
            new_mesh.skin = None;
            new_mesh.flags.has_skin = false;
        }
    }

    let original = install(scene, target, new_mesh);
    Ok(original)
}

/// 把原网格换回 `target`，返回被换下来的替换网格（同样必须由调用方持有）。
pub fn restore(scene: &mut Scene, target: usize, original: ModelMesh) -> Option<ModelMesh> {
    if scene.objects.get(target).is_none() {
        return None;
    }
    Some(install(scene, target, original))
}

/// 换入网格、按新顶点重算包围盒、打脏标记并立刻刷新世界矩阵/关节矩阵。
fn install(scene: &mut Scene, target: usize, mesh: ModelMesh) -> ModelMesh {
    {
        let object = &mut scene.objects[target];
        let previous = std::mem::replace(&mut object.mesh, mesh);

        let (aabb, center) = mesh_bounds(&object.mesh);
        object.local_aabb = aabb;
        object.aabb = aabb;
        object.center = center;
        object.dirty |= DirtyFlags::MESH;

        // 换完当帧就是对的，不必等下一帧的 update_animation。
        scene.update_world_transforms();
        previous
    }
}

/// 按顶点位置算网格包围盒（网格本地坐标系）。
fn mesh_bounds(mesh: &ModelMesh) -> (AABB, Point3<f32>) {
    let Some(first) = mesh.vertices.first() else {
        let zero = Point3::new(0.0, 0.0, 0.0);
        return (AABB::new(zero, zero), zero);
    };

    let mut min = first.position;
    let mut max = first.position;
    for vertex in &mesh.vertices {
        let p = vertex.position;
        min = Point3::new(min.x.min(p.x), min.y.min(p.y), min.z.min(p.z));
        max = Point3::new(max.x.max(p.x), max.y.max(p.y), max.z.max(p.z));
    }

    let aabb = AABB::new(min, max);
    let center = aabb.center();
    (aabb, center)
}

/// 子网格在 UI 列表里的展示信息。
pub struct SubMeshView {
    pub index: usize,
    pub node_name: String,
    pub mesh_name: String,
    /// 材质名（无材质时为「默认材质」）。
    pub material_name: String,
    pub triangles: usize,
    /// 蒙皮关节数；0 表示没有蒙皮（刚性）。
    pub joints: usize,
}

impl SubMeshView {
    /// 列表第一行：`节点名 / 网格名`。
    pub fn title(&self) -> String {
        format!("{} / {}", self.node_name, self.mesh_name)
    }

    /// 列表第二行：`材质 · N 三角 · M 关节`。
    pub fn detail(&self) -> String {
        if self.joints == 0 {
            format!("{} · {} 三角 · 无蒙皮", self.material_name, self.triangles)
        } else {
            format!(
                "{} · {} 三角 · {} 关节",
                self.material_name, self.triangles, self.joints
            )
        }
    }
}

/// 取某个对象的展示信息；名字缺失时退化为下标。
pub fn submesh_view(scene: &Scene, index: usize) -> Option<SubMeshView> {
    let object = scene.objects.get(index)?;

    let node_name = scene
        .nodes
        .get(object.node)
        .and_then(|node| node.name.clone())
        .unwrap_or_else(|| format!("节点 #{}", object.node));

    let mesh_name = object
        .mesh_name
        .clone()
        .unwrap_or_else(|| format!("网格 #{}", index));

    let material_name = object
        .mesh
        .material
        .and_then(|handle| scene.materials.material(handle))
        .and_then(|material| material.name.clone())
        .unwrap_or_else(|| "默认材质".to_string());

    let joints = object
        .mesh
        .skin
        .and_then(|handle| scene.skins.get(handle.0))
        .map(|skin| skin.joints.len())
        .unwrap_or(0);

    Some(SubMeshView {
        index,
        node_name,
        mesh_name,
        material_name,
        triangles: object.mesh.indices.len() / 3,
        joints,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use cgmath::{Matrix4, Vector2, Vector3};
    use material::MaterialLibrary;
    use render::Vertex;
    use scene::{SceneNode, SceneObject, Transform};

    fn unit_aabb() -> AABB {
        AABB::new(Point3::new(-0.5, -0.5, -0.5), Point3::new(0.5, 0.5, 0.5))
    }

    fn triangle(index_offset: u32) -> ModelMesh {
        let vertex = |x: f32| Vertex {
            position: Point3::new(x, 0.0, 0.0),
            normal: Vector3::new(0.0, 1.0, 0.0),
            uv: Vector2::new(0.0, 0.0),
            tangent: [1.0, 0.0, 0.0, 1.0],
            joints: [0; 4],
            weights: [0.0; 4],
        };
        let mut mesh = ModelMesh::new();
        mesh.vertices = vec![vertex(0.0), vertex(1.0), vertex(0.0)];
        mesh.indices = vec![index_offset, index_offset + 1, index_offset + 2];
        mesh
    }

    /// 造一个「根节点 + joint_count 根关节」的场景，含一个对象；
    /// `skinned` 决定对象是否绑定一根关节的蒙皮（关节数取 `joint_count`）。
    fn build_scene(joint_count: usize, skinned: bool) -> Scene {
        let mut nodes = vec![SceneNode::new(0, None, Transform::default())];
        for i in 0..joint_count {
            nodes.push(SceneNode::new(i + 1, Some(0), Transform::default()));
        }

        let mut skins = Vec::new();
        let mut mesh = triangle(0);
        if skinned {
            skins.push(Skin {
                joints: (1..=joint_count).collect(),
                inverse_bind_matrices: vec![Matrix4::from_scale(1.0); joint_count],
            });
            mesh.skin = Some(SkinHandle(0));
            mesh.flags.has_skin = true;
        }

        let object = SceneObject {
            entity_id: "obj".to_string(),
            node: 0,
            mesh_name: Some("mesh".to_string()),
            local_aabb: unit_aabb(),
            aabb: unit_aabb(),
            center: Point3::new(0.0, 0.0, 0.0),
            mesh,
            model_matrix: Matrix4::from_scale(1.0).into(),
            normal_matrix: Matrix4::from_scale(1.0).into(),
            joint_matrices: Vec::new(),
            dirty: DirtyFlags::empty(),
            prefab_source: None,
        };

        Scene::new(
            nodes,
            vec![object],
            MaterialLibrary::default(),
            Vec::new(),
            skins,
            unit_aabb(),
            8,
            6,
        )
    }

    #[test]
    fn matching_joints_rebind_onto_target_skeleton() {
        let mut target = build_scene(2, true);
        let source = build_scene(2, true);

        let original = apply(&mut target, 0, &source, 0, 0).expect("关节数一致应允许替换");

        // 被换下来的原网格带着目标原来的 skin。
        assert_eq!(original.skin, Some(SkinHandle(0)));

        let swapped = &target.objects[0].mesh;
        let new_skin_handle = swapped.skin.expect("应绑上新 skin");
        assert_eq!(new_skin_handle, SkinHandle(1), "新 skin 应追加在末尾");

        let new_skin = &target.skins[new_skin_handle.0];
        assert_eq!(new_skin.joints, vec![1, 2], "joints 沿用目标骨架");
        assert_eq!(new_skin.inverse_bind_matrices.len(), 2);
        assert!(swapped.flags.has_skin);
    }

    #[test]
    fn joint_mismatch_is_rejected() {
        let mut target = build_scene(2, true);
        let source = build_scene(3, true);

        assert_eq!(
            apply(&mut target, 0, &source, 0, 0).err(),
            Some(SwapError::JointMismatch { target: 2, source: 3 })
        );
        // 拒绝时不能留下副作用。
        assert_eq!(target.skins.len(), 1);
        assert_eq!(target.objects[0].mesh.skin, Some(SkinHandle(0)));
    }

    #[test]
    fn rigid_source_is_allowed() {
        let mut target = build_scene(2, true);
        let source = build_scene(0, false);

        apply(&mut target, 0, &source, 0, 0).expect("无蒙皮的源应作为刚性挂件换上");

        let swapped = &target.objects[0].mesh;
        assert_eq!(swapped.skin, None);
        assert!(!swapped.flags.has_skin);
        assert_eq!(target.skins.len(), 1, "不应追加 skin");
    }

    #[test]
    fn skinned_source_into_rigid_target_is_rejected() {
        let mut target = build_scene(2, false);
        let source = build_scene(2, true);

        assert_eq!(
            apply(&mut target, 0, &source, 0, 0).err(),
            Some(SwapError::TargetRigid)
        );
    }

    #[test]
    fn material_handle_is_shifted_by_offset() {
        let mut target = build_scene(0, false);
        let mut source = build_scene(0, false);

        // 目标先有一份材质，源文件有两份材质，源网格用第 2 份（句柄 1）。
        target.materials.insert(material::Material::default());
        source.materials.insert(material::Material::default());
        source.materials.insert(material::Material::default());
        source.objects[0].mesh.material = Some(MaterialHandle(1));

        // 真实流程：整库搬入拿到偏移，再带着偏移替换。
        let offset = target.materials.append_library(source.materials.clone());
        assert_eq!(offset, 1);

        apply(&mut target, 0, &source, 0, offset).expect("替换应成功");
        assert_eq!(target.objects[0].mesh.material, Some(MaterialHandle(2)));
    }

    #[test]
    fn restore_puts_the_original_mesh_back() {
        let mut target = build_scene(2, true);
        let source = build_scene(2, true);

        let original = apply(&mut target, 0, &source, 0, 0).expect("替换应成功");
        let swapped_out = restore(&mut target, 0, original).expect("恢复应成功");

        // 恢复后回到目标自己的 skin；被换下来的替换网格原样返回。
        assert_eq!(target.objects[0].mesh.skin, Some(SkinHandle(0)));
        assert_eq!(swapped_out.skin, Some(SkinHandle(1)));
        assert_eq!(swapped_out.indices, vec![0, 1, 2]);
    }

    #[test]
    fn out_of_range_is_reported() {
        let mut target = build_scene(2, true);
        let source = build_scene(2, true);

        assert_eq!(
            apply(&mut target, 9, &source, 0, 0).err(),
            Some(SwapError::OutOfRange)
        );
        assert_eq!(
            apply(&mut target, 0, &source, 9, 0).err(),
            Some(SwapError::OutOfRange)
        );
        assert!(restore(&mut target, 9, triangle(0)).is_none());
    }

    #[test]
    fn submesh_view_falls_back_to_indices() {
        let mut scene = build_scene(2, true);
        scene.nodes[0].name = Some("Root".to_string());
        scene.materials.insert(material::Material {
            name: Some("skin_body".to_string()),
            ..Default::default()
        });
        scene.objects[0].mesh.material = Some(MaterialHandle(0));

        let view = submesh_view(&scene, 0).expect("应有展示信息");
        assert_eq!(view.node_name, "Root");
        assert_eq!(view.mesh_name, "mesh");
        assert_eq!(view.material_name, "skin_body");
        assert_eq!(view.triangles, 1);
        assert_eq!(view.joints, 2);
        assert_eq!(view.title(), "Root / mesh");
        assert!(view.detail().contains("1 三角"));
    }
}
