use cgmath::{Point3, Vector3, Vector2/*Matrix4, InnerSpace, EuclideanSpace, Rad, Deg, PerspectiveFov */};

pub struct Vertex {
    pub position: Point3<f32>,
    pub normal: Vector3<f32>,
    pub uv: Vector2<f32>,
}

pub struct ModelMesh {
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u32>, 
    pub material_index: usize,
}

impl ModelMesh {
    pub fn new() -> Self {
        ModelMesh {
            vertices: Vec::new(),
            indices: Vec::new(),
            material_index: 0,
        }
    }
}