use cgmath::{Point3, Vector2, Vector3};

use crate::MaterialHandle;

pub struct Vertex {
    pub position: Point3<f32>,
    pub normal: Vector3<f32>,
    pub uv: Vector2<f32>,
}

pub struct ModelMesh {
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u32>,
    pub material: Option<MaterialHandle>,
}

impl ModelMesh {
    pub fn new() -> Self {
        ModelMesh {
            vertices: Vec::new(),
            indices: Vec::new(),
            material: None,
        }
    }
}
