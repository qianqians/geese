mod material;
pub mod octree;
mod scene;
pub mod scene_object;
pub mod character_animation;
pub mod manifest;
pub mod prefab_manifest;
pub mod prefab_loader;
pub mod avatar_manifest;
pub mod loader;
pub mod primitives;
pub mod net;
pub mod script;

pub use avatar::{AnimationClip, AnimationPlayer, SceneNode, Skin, Transform};
pub use avatar::{
    ActiveAnimation, AnimationEvent, AnimationState, AnimationStateMachine, Blend1DEntry, BlendTree, Parameter,
    Transition, TransitionCondition,
};
pub use octree::Octree;
pub use scene::Scene;
pub use scene::{MarkerEvent, SceneAnimationEvent, EntityAnimationGraph};
pub use scene_object::SceneObject;
pub use scene_object::DirtyFlags;
#[cfg(feature = "physics")]
pub use gameplay_physics::{CapsuleController, CharacterControllerType, CharacterPhysics, JointTypeStrategy, RagdollBuilder, RagdollConfig, RagdollInstance};
#[cfg(feature = "navmesh")]
pub use navmesh;
pub use character_animation::{CharacterAnimationGraph, SpeedThresholds};
pub use primitives::{PrimitiveKind, create_primitive_mesh};
pub use script::ScriptComponent;

use asset::load;
use std::collections::HashMap;

use avatar::{AnimatedProperty, AnimationChannel, AnimationOutputs, Interpolation};
use cgmath::{
    InnerSpace, Matrix4, Point3, Quaternion, Vector2,
    Vector3, /* , InnerSpace, EuclideanSpace, Rad, Deg, PerspectiveFov */
};
use gltf::animation::util::ReadOutputs;
use gltf::mesh::Mesh;
use gltf::mesh::Primitive;
use gltf::mesh::util::ReadIndices;
use material::load_material_library;
use math::AABB;
use render::{MaterialHandle, ModelMesh, SkinHandle, Vertex};
use uuid::Uuid;

fn import_document_aabb(node: &gltf::Node, bounds: &mut AABB) {
    if let Some(mesh) = node.mesh() {
        for primitive in mesh.primitives() {
            let bbox = primitive.bounding_box();
            let min = Point3::new(bbox.min[0], bbox.min[1], bbox.min[2]);
            let max = Point3::new(bbox.max[0], bbox.max[1], bbox.max[2]);
            if bounds.min.x > min.x {
                bounds.min.x = min.x;
            }
            if bounds.min.y > min.y {
                bounds.min.y = min.y;
            }
            if bounds.min.z > min.z {
                bounds.min.z = min.z;
            }
            if bounds.max.x < max.x {
                bounds.max.x = max.x;
            }
            if bounds.max.y < max.y {
                bounds.max.y = max.y;
            }
            if bounds.max.z < max.z {
                bounds.max.z = max.z;
            }
        }
    }

    for child in node.children() {
        import_document_aabb(&child, bounds);
    }
}

fn load_indices(prim: &Primitive, buffers: &[gltf::buffer::Data]) -> Vec<u32> {
    let reader = prim.reader(|buffer| Some(buffers[buffer.index()].0.as_slice()));
    let mut indices = Vec::new();

    match reader.read_indices() {
        Some(ReadIndices::U8(iter)) => indices.extend(iter.map(u32::from)),
        Some(ReadIndices::U16(iter)) => indices.extend(iter.map(u32::from)),
        Some(ReadIndices::U32(iter)) => indices.extend(iter),
        None => {}
    }

    indices
}

fn load_primitive(prim: &Primitive, buffers: &[gltf::buffer::Data], out: &mut ModelMesh) {
    let reader = prim.reader(|buffer| Some(buffers[buffer.index()].0.as_slice()));

    // Skip primitives without positions (invalid GLTF)
    let positions: Vec<_> = match reader.read_positions() {
        Some(iter) => iter.collect(),
        None => return,
    };
    let normals = reader.read_normals();
    let has_normals = normals.is_some();
    let normals: Vec<_> = normals
        .map(Iterator::collect)
        .unwrap_or_else(|| vec![[0.0, 1.0, 0.0]; positions.len()]);

    let tex_coords = reader.read_tex_coords(0);
    let has_uv0 = tex_coords.is_some();
    let uvs: Vec<_> = tex_coords
        .map(|tex_coords| tex_coords.into_f32().collect())
        .unwrap_or_else(|| vec![[0.0, 0.0]; positions.len()]);

    let mut indices = load_indices(prim, buffers);
    if indices.is_empty() {
        indices.extend(0..positions.len() as u32);
    }

    let tangents = reader.read_tangents();
    let has_gltf_tangents = tangents.is_some();
    let generated_tangents;
    let tangents: Vec<_> = if let Some(tangents) = tangents {
        tangents.collect()
    } else if has_uv0 {
        generated_tangents = generate_tangents(&positions, &uvs, &indices);
        generated_tangents
    } else {
        vec![[1.0, 0.0, 0.0, 1.0]; positions.len()]
    };
    let joints: Vec<_> = reader
        .read_joints(0)
        .map(|joints| joints.into_u16().collect())
        .unwrap_or_else(|| vec![[0, 0, 0, 0]; positions.len()]);
    let weights: Vec<_> = reader
        .read_weights(0)
        .map(|weights| weights.into_f32().collect())
        .unwrap_or_else(|| vec![[1.0, 0.0, 0.0, 0.0]; positions.len()]);

    for i in 0..positions.len() {
        let position = positions[i];
        let normal = normals[i];
        let uv = uvs[i];

        out.vertices.push(Vertex {
            position: Point3::new(position[0], position[1], position[2]),
            normal: Vector3::new(normal[0], normal[1], normal[2]),
            uv: Vector2::new(uv[0], uv[1]),
            tangent: tangents[i],
            joints: joints[i],
            weights: weights[i],
        });
    }

    out.indices.extend(indices);
    out.flags.has_normals = has_normals;
    out.flags.has_uv0 = has_uv0;
    out.flags.has_tangents = has_gltf_tangents || has_uv0;
    out.flags.has_skin = reader.read_joints(0).is_some() && reader.read_weights(0).is_some();
    out.material = prim.material().index().map(MaterialHandle);
}

fn generate_tangents(positions: &[[f32; 3]], uvs: &[[f32; 2]], indices: &[u32]) -> Vec<[f32; 4]> {
    let mut tangents = vec![Vector3::new(0.0, 0.0, 0.0); positions.len()];

    for triangle in indices.chunks_exact(3) {
        let i0 = triangle[0] as usize;
        let i1 = triangle[1] as usize;
        let i2 = triangle[2] as usize;

        let p0 = Vector3::new(positions[i0][0], positions[i0][1], positions[i0][2]);
        let p1 = Vector3::new(positions[i1][0], positions[i1][1], positions[i1][2]);
        let p2 = Vector3::new(positions[i2][0], positions[i2][1], positions[i2][2]);
        let uv0 = Vector2::new(uvs[i0][0], uvs[i0][1]);
        let uv1 = Vector2::new(uvs[i1][0], uvs[i1][1]);
        let uv2 = Vector2::new(uvs[i2][0], uvs[i2][1]);

        let edge1 = p1 - p0;
        let edge2 = p2 - p0;
        let delta_uv1 = uv1 - uv0;
        let delta_uv2 = uv2 - uv0;
        let det = delta_uv1.x * delta_uv2.y - delta_uv2.x * delta_uv1.y;

        if det.abs() <= f32::EPSILON {
            continue;
        }

        let tangent = (edge1 * delta_uv2.y - edge2 * delta_uv1.y) / det;
        tangents[i0] += tangent;
        tangents[i1] += tangent;
        tangents[i2] += tangent;
    }

    tangents
        .into_iter()
        .map(|tangent| {
            if tangent.magnitude2() > f32::EPSILON {
                let tangent = tangent.normalize();
                [tangent.x, tangent.y, tangent.z, 1.0]
            } else {
                [1.0, 0.0, 0.0, 1.0]
            }
        })
        .collect()
}

fn load_gltf_mesh(
    mesh: &Mesh,
    node_id: usize,
    skin: Option<SkinHandle>,
    buffers: &[gltf::buffer::Data],
    objects: &mut Vec<SceneObject>,
) -> Vec<usize> {
    let mut object_indices = Vec::new();

    for prim in mesh.primitives() {
        let bbox = prim.bounding_box();
        let min = Point3::new(bbox.min[0], bbox.min[1], bbox.min[2]);
        let max = Point3::new(bbox.max[0], bbox.max[1], bbox.max[2]);
        let center = Point3::new(
            (min.x + max.x) * 0.5,
            (min.y + max.y) * 0.5,
            (min.z + max.z) * 0.5,
        );

        let mut model_mesh = ModelMesh::new();
        load_primitive(&prim, buffers, &mut model_mesh);
        model_mesh.skin = skin;
        model_mesh.flags.has_skin = model_mesh.flags.has_skin && skin.is_some();
        let object_index = objects.len();
        objects.push(SceneObject {
            entity_id: Uuid::new_v4().to_string(),
            node: node_id,
            local_aabb: AABB { min, max },
            aabb: AABB { min, max },
            center: center,
            mesh: model_mesh,
            model_matrix: Matrix4::from_scale(1.0).into(),
            normal_matrix: Matrix4::from_scale(1.0).into(),
            joint_matrices: Vec::new(),
            dirty: DirtyFlags::all(),
            prefab_source: None,
        });
        object_indices.push(object_index);
    }

    object_indices
}

/// First pass: build node tree structure only (lightweight, no mesh data).
fn load_node_structure(
    node: &gltf::Node,
    parent: Option<usize>,
    nodes: &mut Vec<SceneNode>,
    node_map: &mut HashMap<usize, usize>,
) -> usize {
    let (translation, rotation, scale) = node.transform().decomposed();
    let node_id = nodes.len();
    nodes.push(SceneNode::new(
        node_id,
        parent,
        Transform::from_gltf(translation, rotation, scale),
    ));
    node_map.insert(node.index(), node_id);

    for child in node.children() {
        let child_id = load_node_structure(&child, Some(node_id), nodes, node_map);
        nodes[node_id].children.push(child_id);
    }

    node_id
}

/// Second pass: load actual mesh data using the pre-built node_map and skin_map.
fn load_node_meshes(
    node: &gltf::Node,
    buffers: &[gltf::buffer::Data],
    nodes: &mut Vec<SceneNode>,
    objects: &mut Vec<SceneObject>,
    node_map: &HashMap<usize, usize>,
    skin_map: &HashMap<usize, usize>,
) {
    let node_id = node_map[&node.index()];

    if let Some(mesh) = node.mesh() {
        let skin = node
            .skin()
            .and_then(|skin| skin_map.get(&skin.index()).copied())
            .map(SkinHandle);
        nodes[node_id].objects = load_gltf_mesh(&mesh, node_id, skin, buffers, objects);
    }

    for child in node.children() {
        load_node_meshes(&child, buffers, nodes, objects, node_map, skin_map);
    }
}

pub fn import_scene(
    path: String,
    max_objects: usize,
    max_depth: usize,
) -> Result<Scene, Box<dyn std::error::Error>> {
    let (gltf, buffers, images) = load(path)?;
    import_scene_from_data(&gltf, &buffers, &images, max_objects, max_depth)
}

/// 从预加载的 GLTF 数据构建场景。
///
/// 与 [`import_scene`] 功能相同，但接受已解析的 GLTF 数据而非文件路径。
/// 配合 `asset::GltfDataLoader` + `asset::AssetCache` 使用可避免重复解析同一文件。
pub fn import_scene_from_data(
    gltf: &gltf::Document,
    buffers: &[gltf::buffer::Data],
    images: &[gltf::image::Data],
    max_objects: usize,
    max_depth: usize,
) -> Result<Scene, Box<dyn std::error::Error>> {
    let materials = load_material_library(gltf, images);

    let mut bounds: AABB = AABB::new(
        Point3::new(-100.0, -100.0, -100.0),
        Point3::new(100.0, 100.0, 100.0),
    );
    for scene in gltf.scenes() {
        for node in scene.nodes() {
            import_document_aabb(&node, &mut bounds);
        }
    }
    let mut nodes = Vec::new();
    let mut objects = Vec::new();
    let mut node_map = HashMap::new();

    for scene in gltf.scenes() {
        for node in scene.nodes() {
            load_node_structure(
                &node,
                None,
                &mut nodes,
                &mut node_map,
            );
        }
    }
    let (skins, skin_map) = load_skins(gltf, buffers, &node_map);

    for scene in gltf.scenes() {
        for node in scene.nodes() {
            load_node_meshes(
                &node,
                buffers,
                &mut nodes,
                &mut objects,
                &node_map,
                &skin_map,
            );
        }
    }
    let animations = load_animations(gltf, buffers, &node_map);

    Ok(Scene::new(
        nodes,
        objects,
        materials,
        animations,
        skins,
        bounds,
        max_objects,
        max_depth,
    ))
}

fn load_animations(
    document: &gltf::Document,
    buffers: &[gltf::buffer::Data],
    node_map: &HashMap<usize, usize>,
) -> Vec<AnimationClip> {
    document
        .animations()
        .map(|animation| {
            let mut duration = 0.0;
            let mut channels = Vec::new();

            for channel in animation.channels() {
                let Some(&target_node) = node_map.get(&channel.target().node().index()) else {
                    continue;
                };
                let property = match channel.target().property() {
                    gltf::animation::Property::Translation => AnimatedProperty::Translation,
                    gltf::animation::Property::Rotation => AnimatedProperty::Rotation,
                    gltf::animation::Property::Scale => AnimatedProperty::Scale,
                    gltf::animation::Property::MorphTargetWeights => continue,
                };
                let interpolation = match channel.sampler().interpolation() {
                    gltf::animation::Interpolation::Linear => Interpolation::Linear,
                    gltf::animation::Interpolation::Step => Interpolation::Step,
                    gltf::animation::Interpolation::CubicSpline => Interpolation::CubicSpline,
                };

                let reader = channel.reader(|buffer| Some(buffers[buffer.index()].0.as_slice()));
                let Some(inputs) = reader.read_inputs() else {
                    continue;
                };
                let inputs: Vec<_> = inputs.collect();
                if let Some(last) = inputs.last() {
                    duration = f32::max(duration, *last);
                }

                let Some(outputs) = reader.read_outputs() else {
                    continue;
                };
                let outputs = match outputs {
                    ReadOutputs::Translations(values) => AnimationOutputs::Translations(
                        values
                            .map(|value| Vector3::new(value[0], value[1], value[2]))
                            .collect(),
                    ),
                    ReadOutputs::Rotations(values) => AnimationOutputs::Rotations(
                        values
                            .into_f32()
                            .map(|value| Quaternion::new(value[3], value[0], value[1], value[2]))
                            .collect(),
                    ),
                    ReadOutputs::Scales(values) => AnimationOutputs::Scales(
                        values
                            .map(|value| Vector3::new(value[0], value[1], value[2]))
                            .collect(),
                    ),
                    ReadOutputs::MorphTargetWeights(_) => continue,
                };

                channels.push(AnimationChannel {
                    target_node,
                    property,
                    interpolation,
                    inputs,
                    outputs,
                });
            }

            AnimationClip {
                name: animation.name().map(str::to_string),
                duration,
                channels,
                markers: vec![],
            }
        })
        .collect()
}

fn load_skins(
    document: &gltf::Document,
    buffers: &[gltf::buffer::Data],
    node_map: &HashMap<usize, usize>,
) -> (Vec<Skin>, HashMap<usize, usize>) {
    let mut skins = Vec::new();
    let mut skin_map = HashMap::new();

    for skin in document.skins() {
        let joints: Vec<_> = skin
            .joints()
            .filter_map(|joint| node_map.get(&joint.index()).copied())
            .collect();
        let inverse_bind_matrices: Vec<_> = skin
            .reader(|buffer| Some(buffers[buffer.index()].0.as_slice()))
            .read_inverse_bind_matrices()
            .map(|matrices| matrices.map(Matrix4::from).collect())
            .unwrap_or_else(|| vec![Matrix4::from_scale(1.0); joints.len()]);

        skin_map.insert(skin.index(), skins.len());
        skins.push(Skin {
            joints,
            inverse_bind_matrices,
        });
    }

    (skins, skin_map)
}
