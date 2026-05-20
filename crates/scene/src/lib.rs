pub mod octree;
pub mod scene_object;

pub use octree::Octree;
pub use scene_object::SceneObject;

use cgmath::{Point3/* , Matrix4, Vector3, InnerSpace, EuclideanSpace, Rad, Deg, PerspectiveFov */};
use asset::load;
use math::AABB;

fn import_document(node: &gltf::Node, bounds: & mut AABB) {
    if let Some(mesh) = node.mesh() {
        for primitive in mesh.primitives() {
            let bbox = primitive.bounding_box();
            let min = Point3::new(bbox.min[0], bbox.min[1], bbox.min[2]);
            let max = Point3::new(bbox.max[0], bbox.max[1], bbox.max[2]);
            if bounds.min.x > min.x { bounds.min.x = min.x; }
            if bounds.min.y > min.y { bounds.min.y = min.y; }
            if bounds.min.z > min.z { bounds.min.z = min.z; }
            if bounds.max.x < max.x { bounds.max.x = max.x; }
            if bounds.max.y < max.y { bounds.max.y = max.y; }
            if bounds.max.z < max.z { bounds.max.z = max.z; }
        }
    }

    for child in node.children() {
        import_document(&child, bounds);
    }
}

pub fn import_scene(path: String, max_objects: usize, max_depth: usize) -> Result<Octree, Box<dyn std::error::Error>> {
    let gltf = load(path)?;

    let mut bounds: AABB = AABB::new(
        Point3::new(-100.0, -100.0, -100.0),
        Point3::new(100.0, 100.0, 100.0),
    );
    for scene in gltf.scenes() {
        for node in scene.nodes() {
            import_document(&node, &mut bounds);
        }
    }

    let tree = Octree::new(bounds, max_objects, max_depth);
    
    Ok(tree)
}