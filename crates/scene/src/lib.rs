mod material;
pub mod octree;
mod scene;
pub mod scene_object;

pub use octree::Octree;
pub use scene::Scene;
pub use scene_object::SceneObject;

use asset::load;
use cgmath::{
    Point3, Vector2,
    Vector3, /* , Matrix4, InnerSpace, EuclideanSpace, Rad, Deg, PerspectiveFov */
};
use gltf::mesh::Mesh;
use gltf::mesh::Primitive;
use gltf::mesh::util::ReadIndices;
use material::load_material_library;
use math::AABB;
use render::{MaterialHandle, ModelMesh, Vertex};
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

    let positions: Vec<_> = reader.read_positions().unwrap().collect();
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

    for i in 0..positions.len() {
        let position = positions[i];
        let normal = normals[i];
        let uv = uvs[i];

        out.vertices.push(Vertex {
            position: Point3::new(position[0], position[1], position[2]),
            normal: Vector3::new(normal[0], normal[1], normal[2]),
            uv: Vector2::new(uv[0], uv[1]),
        });
    }

    let mut indices = load_indices(prim, buffers);
    if indices.is_empty() {
        indices.extend(0..positions.len() as u32);
    }

    out.indices.extend(indices);
    out.flags.has_normals = has_normals;
    out.flags.has_uv0 = has_uv0;
    out.material = prim.material().index().map(MaterialHandle);
}

fn load_gltf_mesh(mesh: &Mesh, buffers: &[gltf::buffer::Data], oct: &mut Octree) {
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
        oct.insert(SceneObject {
            entity_id: Uuid::new_v4().to_string(),
            aabb: AABB { min, max },
            center: center,
            mesh: model_mesh,
        });
    }
}

fn load_node(node: &gltf::Node, buffers: &[gltf::buffer::Data], oct: &mut Octree) {
    // 如果这个节点有 Mesh → 加载
    if let Some(mesh) = node.mesh() {
        load_gltf_mesh(&mesh, buffers, oct);
    }

    // 递归加载子节点
    for child in node.children() {
        load_node(&child, buffers, oct);
    }
}

pub fn import_scene(
    path: String,
    max_objects: usize,
    max_depth: usize,
) -> Result<Scene, Box<dyn std::error::Error>> {
    let (gltf, buffers, images) = load(path)?;
    let materials = load_material_library(&gltf, &images);

    let mut bounds: AABB = AABB::new(
        Point3::new(-100.0, -100.0, -100.0),
        Point3::new(100.0, 100.0, 100.0),
    );
    for scene in gltf.scenes() {
        for node in scene.nodes() {
            import_document_aabb(&node, &mut bounds);
        }
    }
    let mut tree = Octree::new(bounds, max_objects, max_depth);

    for scene in gltf.scenes() {
        for node in scene.nodes() {
            load_node(&node, &buffers, &mut tree);
        }
    }

    Ok(Scene {
        octree: tree,
        materials,
    })
}
