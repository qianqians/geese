use cgmath::{Point3, Vector2, Vector3};

use crate::MaterialHandle;

/// 单个 LOD 级别的描述。
#[derive(Clone, Debug)]
pub struct LodLevel {
    /// 切换到该级别的相机距离阈值（世界空间单位）
    pub distance: f32,
    /// 该级别使用的变体索引（0 = 原始 full mesh, 1+ = 简化变体）
    pub variant_index: usize,
    /// 该级别的索引数量（用于 draw 调用）
    pub index_count: u32,
}

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
    /// LOD 级别列表（距离阈值降序排列）。空 = 单级 LOD（默认）。
    /// Feature gate: `lod`（默认禁用）。
    pub lod_levels: Vec<LodLevel>,
}

impl ModelMesh {
    pub fn new() -> Self {
        ModelMesh {
            vertices: Vec::new(),
            indices: Vec::new(),
            material: None,
            skin: None,
            flags: MeshFlags::default(),
            lod_levels: Vec::new(),
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
