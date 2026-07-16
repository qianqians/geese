//! 场景构建器 - 程序化生成跳一跳游戏的几何体和材质。
//!
//! 无需外部 glTF 文件，通过代码生成顶点数据。

use cgmath::{Point3, Vector2, Vector3};
use render::{Material, MaterialHandle, MaterialLibrary, MeshFlags, ModelMesh, Vertex};

/// 生成一个平面网格（XZ 平面，法线朝上）。
#[allow(dead_code)]
pub fn create_plane_mesh(size_x: f32, size_z: f32) -> ModelMesh {
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

/// 生成一个立方体网格。
pub fn create_cube_mesh(sx: f32, sy: f32, sz: f32, material_index: usize) -> ModelMesh {
    let hx = sx * 0.5;
    let hy = sy * 0.5;
    let hz = sz * 0.5;

    #[rustfmt::skip]
    let positions = [
        // 前面 (+Z)
        [-hx, -hy,  hz], [ hx, -hy,  hz], [ hx,  hy,  hz], [-hx,  hy,  hz],
        // 后面 (-Z)
        [ hx, -hy, -hz], [-hx, -hy, -hz], [-hx,  hy, -hz], [ hx,  hy, -hz],
        // 右面 (+X)
        [ hx, -hy,  hz], [ hx, -hy, -hz], [ hx,  hy, -hz], [ hx,  hy,  hz],
        // 左面 (-X)
        [-hx, -hy, -hz], [-hx, -hy,  hz], [-hx,  hy,  hz], [-hx,  hy, -hz],
        // 顶面 (+Y)
        [-hx,  hy,  hz], [ hx,  hy,  hz], [ hx,  hy, -hz], [-hx,  hy, -hz],
        // 底面 (-Y)
        [-hx, -hy, -hz], [ hx, -hy, -hz], [ hx, -hy,  hz], [-hx, -hy,  hz],
    ];

    #[rustfmt::skip]
    let normals = [
        [0.0, 0.0, 1.0], [0.0, 0.0, 1.0], [0.0, 0.0, 1.0], [0.0, 0.0, 1.0],
        [0.0, 0.0, -1.0], [0.0, 0.0, -1.0], [0.0, 0.0, -1.0], [0.0, 0.0, -1.0],
        [1.0, 0.0, 0.0], [1.0, 0.0, 0.0], [1.0, 0.0, 0.0], [1.0, 0.0, 0.0],
        [-1.0, 0.0, 0.0], [-1.0, 0.0, 0.0], [-1.0, 0.0, 0.0], [-1.0, 0.0, 0.0],
        [0.0, 1.0, 0.0], [0.0, 1.0, 0.0], [0.0, 1.0, 0.0], [0.0, 1.0, 0.0],
        [0.0, -1.0, 0.0], [0.0, -1.0, 0.0], [0.0, -1.0, 0.0], [0.0, -1.0, 0.0],
    ];

    #[rustfmt::skip]
    let uvs = [
        [0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0],
        [0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0],
        [0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0],
        [0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0],
        [0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0],
        [0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0],
    ];

    #[rustfmt::skip]
    let tangents: [[f32; 4]; 24] = [
        [1.0, 0.0, 0.0, 1.0], [1.0, 0.0, 0.0, 1.0], [1.0, 0.0, 0.0, 1.0], [1.0, 0.0, 0.0, 1.0],
        [-1.0, 0.0, 0.0, 1.0], [-1.0, 0.0, 0.0, 1.0], [-1.0, 0.0, 0.0, 1.0], [-1.0, 0.0, 0.0, 1.0],
        [0.0, 0.0, -1.0, 1.0], [0.0, 0.0, -1.0, 1.0], [0.0, 0.0, -1.0, 1.0], [0.0, 0.0, -1.0, 1.0],
        [0.0, 0.0, 1.0, 1.0], [0.0, 0.0, 1.0, 1.0], [0.0, 0.0, 1.0, 1.0], [0.0, 0.0, 1.0, 1.0],
        [1.0, 0.0, 0.0, 1.0], [1.0, 0.0, 0.0, 1.0], [1.0, 0.0, 0.0, 1.0], [1.0, 0.0, 0.0, 1.0],
        [1.0, 0.0, 0.0, 1.0], [1.0, 0.0, 0.0, 1.0], [1.0, 0.0, 0.0, 1.0], [1.0, 0.0, 0.0, 1.0],
    ];

    let vertices: Vec<Vertex> = (0..24)
        .map(|i| Vertex {
            position: Point3::new(positions[i][0], positions[i][1], positions[i][2]),
            normal: Vector3::new(normals[i][0], normals[i][1], normals[i][2]),
            uv: Vector2::new(uvs[i][0], uvs[i][1]),
            tangent: tangents[i],
            joints: [0, 0, 0, 0],
            weights: [1.0, 0.0, 0.0, 0.0],
        })
        .collect();

    #[rustfmt::skip]
    let indices = vec![
        0,1,2, 0,2,3,
        4,5,6, 4,6,7,
        8,9,10, 8,10,11,
        12,13,14, 12,14,15,
        16,17,18, 16,18,19,
        20,21,22, 20,22,23,
    ];

    let mut mesh = ModelMesh::new();
    mesh.vertices = vertices;
    mesh.indices = indices;
    mesh.material = Some(MaterialHandle(material_index));
    mesh.flags = MeshFlags {
        has_normals: true,
        has_uv0: true,
        has_tangents: true,
        has_skin: false,
    };
    mesh
}

/// 创建默认 PBR 材质。
pub fn create_default_material(name: &str, color: (f32, f32, f32)) -> Material {
    Material {
        name: Some(name.to_string()),
        base_color_factor: [color.0, color.1, color.2, 1.0],
        metallic_factor: 0.0,
        roughness_factor: 0.8,
        emissive_factor: [0.0, 0.0, 0.0],
        alpha_mode: render::AlphaMode::Opaque,
        alpha_cutoff: 0.5,
        base_color_texture: None,
        normal_texture: None,
        metallic_roughness_texture: None,
        occlusion_texture: None,
        emissive_texture: None,
        double_sided: false,
        custom_shader: None,
    }
}

/// 构建跳一跳游戏的默认材质库。
/// 索引: 0=地面灰, 1=玩家黄, 2=平台蓝, 3=平台红, 4=平台绿, 5=平台紫, 6=平台橙
pub fn create_game_materials() -> MaterialLibrary {
    MaterialLibrary {
        materials: vec![
            create_default_material("ground", (0.35, 0.35, 0.38)),        // 0: 深灰 - 地面
            create_default_material("player", (1.0, 0.85, 0.1)),           // 1: 金黄 - 玩家
            create_default_material("platform_blue", (0.2, 0.5, 0.9)),     // 2: 蓝色平台
            create_default_material("platform_red", (0.9, 0.25, 0.25)),    // 3: 红色平台
            create_default_material("platform_green", (0.25, 0.7, 0.35)),  // 4: 绿色平台
            create_default_material("platform_purple", (0.6, 0.3, 0.8)),   // 5: 紫色平台
            create_default_material("platform_orange", (1.0, 0.55, 0.1)),  // 6: 橙色平台
        ],
        textures: vec![],
    }
}
