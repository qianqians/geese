use cgmath::{Point3, Vector2, Vector3};

use crate::MaterialHandle;

#[derive(Clone)]
pub struct Vertex {
    pub position: Point3<f32>,
    pub normal: Vector3<f32>,
    pub uv: Vector2<f32>,
    pub tangent: [f32; 4],
    pub joints: [u16; 4],
    pub weights: [f32; 4],
}

#[derive(Clone)]
pub struct ModelMesh {
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u32>,
    pub material: Option<MaterialHandle>,
    pub skin: Option<SkinHandle>,
    pub flags: MeshFlags,
}

impl ModelMesh {
    pub fn new() -> Self {
        ModelMesh {
            vertices: Vec::new(),
            indices: Vec::new(),
            material: None,
            skin: None,
            flags: MeshFlags::default(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SkinHandle(pub usize);

#[derive(Clone, Copy, Debug, Default)]
pub struct MeshFlags {
    pub has_normals: bool,
    pub has_uv0: bool,
    pub has_tangents: bool,
    pub has_skin: bool,
}
