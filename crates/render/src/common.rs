use std::collections::HashMap;

use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;

use crate::{FilterMode, Material, MaterialLibrary, Texture, TextureFormat, Vertex, WrapMode};

/// GPU uniform 中关节矩阵的上限。当前设为 32 以减小 ObjectUniform 体积（~2 KB）。
/// 若后续需要支持更多骨骼，可增大此值或改用独立的 storage buffer。
pub const MAX_JOINTS: usize = 32;

/// 共享给 forward+ 与 deferred+ 两条管线的 GPU 顶点格式。
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct GpuVertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub uv: [f32; 2],
    pub tangent: [f32; 4],
    pub joints: [u32; 4],
    pub weights: [f32; 4],
}

impl GpuVertex {
    pub fn layout<'a>() -> wgpu::VertexBufferLayout<'a> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<GpuVertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[
                wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 0,
                    format: wgpu::VertexFormat::Float32x3,
                },
                wgpu::VertexAttribute {
                    offset: std::mem::size_of::<[f32; 3]>() as wgpu::BufferAddress,
                    shader_location: 1,
                    format: wgpu::VertexFormat::Float32x3,
                },
                wgpu::VertexAttribute {
                    offset: std::mem::size_of::<[f32; 6]>() as wgpu::BufferAddress,
                    shader_location: 2,
                    format: wgpu::VertexFormat::Float32x2,
                },
                wgpu::VertexAttribute {
                    offset: std::mem::size_of::<[f32; 8]>() as wgpu::BufferAddress,
                    shader_location: 3,
                    format: wgpu::VertexFormat::Float32x4,
                },
                wgpu::VertexAttribute {
                    offset: (std::mem::size_of::<[f32; 8]>() + std::mem::size_of::<[f32; 4]>())
                        as wgpu::BufferAddress,
                    shader_location: 4,
                    format: wgpu::VertexFormat::Uint32x4,
                },
                wgpu::VertexAttribute {
                    offset: (std::mem::size_of::<[f32; 8]>()
                        + std::mem::size_of::<[f32; 4]>()
                        + std::mem::size_of::<[u32; 4]>())
                        as wgpu::BufferAddress,
                    shader_location: 5,
                    format: wgpu::VertexFormat::Float32x4,
                },
            ],
        }
    }
}

impl From<&Vertex> for GpuVertex {
    fn from(vertex: &Vertex) -> Self {
        Self {
            position: [vertex.position.x, vertex.position.y, vertex.position.z],
            normal: [vertex.normal.x, vertex.normal.y, vertex.normal.z],
            uv: [vertex.uv.x, vertex.uv.y],
            tangent: vertex.tangent,
            joints: [
                u32::from(vertex.joints[0]),
                u32::from(vertex.joints[1]),
                u32::from(vertex.joints[2]),
                u32::from(vertex.joints[3]),
            ],
            weights: vertex.weights,
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct CameraUniform {
    pub view_projection: [[f32; 4]; 4],
    pub inverse_view_projection: [[f32; 4]; 4],
    pub camera_position: [f32; 4],
}

impl CameraUniform {
    pub fn new(view_projection: [[f32; 4]; 4], camera_position: [f32; 3]) -> Self {
        Self {
            view_projection,
            inverse_view_projection: invert_4x4(view_projection).unwrap_or(identity_matrix()),
            camera_position: [
                camera_position[0],
                camera_position[1],
                camera_position[2],
                1.0,
            ],
        }
    }

    pub fn placeholder() -> Self {
        Self::new(identity_matrix(), [0.0, 0.0, 1.0])
    }
}

/// 与 [pbr_common.wgsl](file:///Users/qianqians/Documents/geese/crates/render/shaders/pbr_common.wgsl)
/// 中 `MaterialUniform` 严格对齐：4 × vec4 = 64 字节。
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct MaterialUniform {
    pub base_color_factor: [f32; 4],
    /// rgb = emissive_factor，a = alpha_cutoff
    pub emissive_alpha_cutoff: [f32; 4],
    /// x = metallic, y = roughness, z = normal_scale, w = occlusion_strength
    pub metallic_roughness_normal_occlusion: [f32; 4],
    /// x = texture_flags（bit0..4 = base_color/mr/normal/occlusion/emissive）
    /// y = alpha_mode（0 Opaque / 1 Mask / 2 Blend）
    pub flags: [u32; 4],
}

pub const TEX_BIT_BASE_COLOR: u32 = 1 << 0;
pub const TEX_BIT_METALLIC_ROUGHNESS: u32 = 1 << 1;
pub const TEX_BIT_NORMAL: u32 = 1 << 2;
pub const TEX_BIT_OCCLUSION: u32 = 1 << 3;
pub const TEX_BIT_EMISSIVE: u32 = 1 << 4;

impl MaterialUniform {
    pub fn from_material(material: &Material, has_tangent_uv: bool) -> Self {
        let mut texture_flags = 0u32;
        if material.base_color_texture.is_some() {
            texture_flags |= TEX_BIT_BASE_COLOR;
        }
        if material.metallic_roughness_texture.is_some() {
            texture_flags |= TEX_BIT_METALLIC_ROUGHNESS;
        }
        if material.normal_texture.is_some() && has_tangent_uv {
            texture_flags |= TEX_BIT_NORMAL;
        }
        if material.occlusion_texture.is_some() {
            texture_flags |= TEX_BIT_OCCLUSION;
        }
        if material.emissive_texture.is_some() {
            texture_flags |= TEX_BIT_EMISSIVE;
        }

        let alpha_mode = match material.alpha_mode {
            crate::AlphaMode::Opaque => 0u32,
            crate::AlphaMode::Mask => 1u32,
            crate::AlphaMode::Blend => 2u32,
        };

        Self {
            base_color_factor: material.base_color_factor,
            emissive_alpha_cutoff: [
                material.emissive_factor[0],
                material.emissive_factor[1],
                material.emissive_factor[2],
                material.alpha_cutoff,
            ],
            metallic_roughness_normal_occlusion: [
                material.metallic_factor,
                material.roughness_factor,
                1.0, // normal_scale（GLTF 默认 1.0;当前 Material 未存值，用默认）
                1.0, // occlusion_strength
            ],
            flags: [texture_flags, alpha_mode, 0, 0],
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct ObjectUniform {
    pub model: [[f32; 4]; 4],
    pub normal: [[f32; 4]; 4],
    pub skin: [u32; 4],
    pub joints: [[[f32; 4]; 4]; MAX_JOINTS],
}

pub fn joint_uniforms(joints: &[[[f32; 4]; 4]]) -> [[[f32; 4]; 4]; MAX_JOINTS] {
    let mut output = [identity_matrix(); MAX_JOINTS];
    for (index, joint) in joints.iter().take(MAX_JOINTS).enumerate() {
        output[index] = *joint;
    }
    output
}

pub fn identity_matrix() -> [[f32; 4]; 4] {
    [
        [1.0, 0.0, 0.0, 0.0],
        [0.0, 1.0, 0.0, 0.0],
        [0.0, 0.0, 1.0, 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ]
}

/// 4×4 矩阵求逆（行主序）。失败时返回 None。
pub fn invert_4x4(m: [[f32; 4]; 4]) -> Option<[[f32; 4]; 4]> {
    let mut a = [0.0f32; 16];
    for r in 0..4 {
        for c in 0..4 {
            a[r * 4 + c] = m[r][c];
        }
    }

    let mut inv = [0.0f32; 16];
    inv[0] = a[5] * a[10] * a[15] - a[5] * a[11] * a[14] - a[9] * a[6] * a[15]
        + a[9] * a[7] * a[14]
        + a[13] * a[6] * a[11]
        - a[13] * a[7] * a[10];
    inv[4] = -a[4] * a[10] * a[15] + a[4] * a[11] * a[14] + a[8] * a[6] * a[15]
        - a[8] * a[7] * a[14]
        - a[12] * a[6] * a[11]
        + a[12] * a[7] * a[10];
    inv[8] = a[4] * a[9] * a[15] - a[4] * a[11] * a[13] - a[8] * a[5] * a[15]
        + a[8] * a[7] * a[13]
        + a[12] * a[5] * a[11]
        - a[12] * a[7] * a[9];
    inv[12] = -a[4] * a[9] * a[14] + a[4] * a[10] * a[13] + a[8] * a[5] * a[14]
        - a[8] * a[6] * a[13]
        - a[12] * a[5] * a[10]
        + a[12] * a[6] * a[9];
    inv[1] = -a[1] * a[10] * a[15] + a[1] * a[11] * a[14] + a[9] * a[2] * a[15]
        - a[9] * a[3] * a[14]
        - a[13] * a[2] * a[11]
        + a[13] * a[3] * a[10];
    inv[5] = a[0] * a[10] * a[15] - a[0] * a[11] * a[14] - a[8] * a[2] * a[15]
        + a[8] * a[3] * a[14]
        + a[12] * a[2] * a[11]
        - a[12] * a[3] * a[10];
    inv[9] = -a[0] * a[9] * a[15] + a[0] * a[11] * a[13] + a[8] * a[1] * a[15]
        - a[8] * a[3] * a[13]
        - a[12] * a[1] * a[11]
        + a[12] * a[3] * a[9];
    inv[13] = a[0] * a[9] * a[14] - a[0] * a[10] * a[13] - a[8] * a[1] * a[14]
        + a[8] * a[2] * a[13]
        + a[12] * a[1] * a[10]
        - a[12] * a[2] * a[9];
    inv[2] = a[1] * a[6] * a[15] - a[1] * a[7] * a[14] - a[5] * a[2] * a[15]
        + a[5] * a[3] * a[14]
        + a[13] * a[2] * a[7]
        - a[13] * a[3] * a[6];
    inv[6] = -a[0] * a[6] * a[15] + a[0] * a[7] * a[14] + a[4] * a[2] * a[15]
        - a[4] * a[3] * a[14]
        - a[12] * a[2] * a[7]
        + a[12] * a[3] * a[6];
    inv[10] = a[0] * a[5] * a[15] - a[0] * a[7] * a[13] - a[4] * a[1] * a[15]
        + a[4] * a[3] * a[13]
        + a[12] * a[1] * a[7]
        - a[12] * a[3] * a[5];
    inv[14] = -a[0] * a[5] * a[14] + a[0] * a[6] * a[13] + a[4] * a[1] * a[14]
        - a[4] * a[2] * a[13]
        - a[12] * a[1] * a[6]
        + a[12] * a[2] * a[5];
    inv[3] = -a[1] * a[6] * a[11] + a[1] * a[7] * a[10] + a[5] * a[2] * a[11]
        - a[5] * a[3] * a[10]
        - a[9] * a[2] * a[7]
        + a[9] * a[3] * a[6];
    inv[7] = a[0] * a[6] * a[11] - a[0] * a[7] * a[10] - a[4] * a[2] * a[11]
        + a[4] * a[3] * a[10]
        + a[8] * a[2] * a[7]
        - a[8] * a[3] * a[6];
    inv[11] = -a[0] * a[5] * a[11] + a[0] * a[7] * a[9] + a[4] * a[1] * a[11]
        - a[4] * a[3] * a[9]
        - a[8] * a[1] * a[7]
        + a[8] * a[3] * a[5];
    inv[15] = a[0] * a[5] * a[10] - a[0] * a[6] * a[9] - a[4] * a[1] * a[10]
        + a[4] * a[2] * a[9]
        + a[8] * a[1] * a[6]
        - a[8] * a[2] * a[5];

    let det = a[0] * inv[0] + a[1] * inv[4] + a[2] * inv[8] + a[3] * inv[12];
    // 使用相对阈值：将行列式与矩阵元素尺度的 4 次方比较
    let max_abs = a.iter().fold(0.0f32, |acc, &v| acc.max(v.abs()));
    let threshold = 1e-8 * max_abs.powi(4);
    if det.abs() < threshold {
        return None;
    }
    let inv_det = 1.0 / det;
    let mut result = [[0.0f32; 4]; 4];
    for r in 0..4 {
        for c in 0..4 {
            result[r][c] = inv[r * 4 + c] * inv_det;
        }
    }
    Some(result)
}

// -------- GPU 资源缓存 --------

/// 网格缓存键：(vertex_ptr, vertex_len, index_ptr, index_len)
pub type MeshCacheKey = (u64, u64, u64, u64);

pub fn compute_mesh_key(mesh: &crate::ModelMesh) -> MeshCacheKey {
    (
        mesh.vertices.as_ptr() as u64,
        mesh.vertices.len() as u64,
        mesh.indices.as_ptr() as u64,
        mesh.indices.len() as u64,
    )
}

pub struct CachedTexture {
    pub texture: wgpu::Texture,
    pub view: wgpu::TextureView,
    pub sampler: wgpu::Sampler,
}

pub struct CachedMeshBuffers {
    pub vertex_buffer: wgpu::Buffer,
    pub index_buffer: wgpu::Buffer,
}

pub struct CachedEntityResources {
    pub material_buffer: wgpu::Buffer,
    pub object_buffer: wgpu::Buffer,
    pub material_bind_group: wgpu::BindGroup,
    pub object_bind_group: wgpu::BindGroup,
    /// 记录用于构建 material bind group 的纹理 handle，用于检测是否需要重建
    pub(crate) tex_handles: [Option<usize>; 5],
    pub(crate) has_tangent_uv: bool,
}

/// GPU 资源缓存，避免每帧重建 buffer 和重新上传纹理。
pub struct GpuResourceCache {
    pub mesh_buffers: HashMap<MeshCacheKey, CachedMeshBuffers>,
    pub textures: HashMap<usize, CachedTexture>,
    pub entity_resources: HashMap<String, CachedEntityResources>,
}

impl GpuResourceCache {
    pub fn new() -> Self {
        Self {
            mesh_buffers: HashMap::new(),
            textures: HashMap::new(),
            entity_resources: HashMap::new(),
        }
    }
}

impl Default for GpuResourceCache {
    fn default() -> Self {
        Self::new()
    }
}

// -------- Render command (轻量级，引用缓存中的 GPU 资源) --------

/// 单条已 prepare 的绘制命令。GPU 资源（buffer / 纹理）保存在 `GpuResourceCache` 中，
/// 此处仅保存查找键和每帧重建的 bind group。
pub struct WgpuRenderCommand {
    pub entity_id: String,
    pub mesh_key: MeshCacheKey,
    pub index_count: u32,
    pub material_bind_group: wgpu::BindGroup,
    pub object_bind_group: wgpu::BindGroup,
}

pub struct WgpuRenderQueue {
    pub commands: Vec<WgpuRenderCommand>,
}

impl Default for WgpuRenderQueue {
    fn default() -> Self {
        Self {
            commands: Vec::new(),
        }
    }
}

/// 创建 RGBA8 纹理并立即上传像素数据。
pub fn create_rgba_texture(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    label: &str,
    width: u32,
    height: u32,
    pixels: &[u8],
) -> wgpu::Texture {
    device.create_texture_with_data(
        queue,
        &wgpu::TextureDescriptor {
            label: Some(label),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        },
        wgpu::util::TextureDataOrder::LayerMajor,
        pixels,
    )
}

pub fn upload_texture(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    texture: &Texture,
) -> wgpu::Texture {
    let pixels = to_rgba8(texture);
    create_rgba_texture(
        device,
        queue,
        texture.name.as_deref().unwrap_or("scene texture"),
        texture.width,
        texture.height,
        &pixels,
    )
}

fn to_rgba8(texture: &Texture) -> Vec<u8> {
    match texture.format {
        TextureFormat::R8 => texture
            .pixels
            .iter()
            .flat_map(|r| [*r, *r, *r, 255])
            .collect(),
        TextureFormat::R8G8 => texture
            .pixels
            .chunks_exact(2)
            .flat_map(|px| [px[0], px[1], 0, 255])
            .collect(),
        TextureFormat::R8G8B8 => texture
            .pixels
            .chunks_exact(3)
            .flat_map(|px| [px[0], px[1], px[2], 255])
            .collect(),
        TextureFormat::R8G8B8A8 => texture.pixels.clone(),
        _ => {
            log::error!("Unsupported texture format: {:?}", texture.format);
            Vec::new()
        }
    }
}

pub fn create_sampler(device: &wgpu::Device, texture: &Texture) -> wgpu::Sampler {
    device.create_sampler(&wgpu::SamplerDescriptor {
        label: texture.name.as_deref(),
        address_mode_u: convert_wrap_mode(texture.sampler.wrap_s),
        address_mode_v: convert_wrap_mode(texture.sampler.wrap_t),
        address_mode_w: wgpu::AddressMode::Repeat,
        mag_filter: convert_filter_mode(texture.sampler.mag_filter),
        min_filter: convert_filter_mode(texture.sampler.min_filter),
        mipmap_filter: convert_filter_mode(texture.sampler.min_filter),
        ..Default::default()
    })
}

pub fn convert_filter_mode(mode: FilterMode) -> wgpu::FilterMode {
    match mode {
        FilterMode::Nearest
        | FilterMode::NearestMipmapNearest
        | FilterMode::NearestMipmapLinear => wgpu::FilterMode::Nearest,
        FilterMode::Linear | FilterMode::LinearMipmapNearest | FilterMode::LinearMipmapLinear => {
            wgpu::FilterMode::Linear
        }
    }
}

pub fn convert_wrap_mode(mode: WrapMode) -> wgpu::AddressMode {
    match mode {
        WrapMode::ClampToEdge => wgpu::AddressMode::ClampToEdge,
        WrapMode::Repeat => wgpu::AddressMode::Repeat,
        WrapMode::MirroredRepeat => wgpu::AddressMode::MirrorRepeat,
    }
}

/// 默认 PBR 占位贴图集合，避免每个材质重复创建。
pub struct DefaultTextures {
    pub white: wgpu::Texture,
    pub white_view: wgpu::TextureView,
    pub metallic_roughness: wgpu::Texture,
    pub metallic_roughness_view: wgpu::TextureView,
    pub normal: wgpu::Texture,
    pub normal_view: wgpu::TextureView,
    pub occlusion: wgpu::Texture,
    pub occlusion_view: wgpu::TextureView,
    pub black: wgpu::Texture,
    pub black_view: wgpu::TextureView,
    pub sampler: wgpu::Sampler,
}

impl DefaultTextures {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Self {
        let white = create_rgba_texture(device, queue, "default white", 1, 1, &[255, 255, 255, 255]);
        let metallic_roughness =
            // GLTF: B = metallic, G = roughness。默认 metallic=0、roughness=1
            create_rgba_texture(device, queue, "default mr", 1, 1, &[0, 255, 0, 255]);
        let normal = create_rgba_texture(device, queue, "default normal", 1, 1, &[128, 128, 255, 255]);
        let occlusion = create_rgba_texture(device, queue, "default occlusion", 1, 1, &[255, 255, 255, 255]);
        let black = create_rgba_texture(device, queue, "default black", 1, 1, &[0, 0, 0, 255]);

        let view_desc = wgpu::TextureViewDescriptor::default();
        let white_view = white.create_view(&view_desc);
        let metallic_roughness_view = metallic_roughness.create_view(&view_desc);
        let normal_view = normal.create_view(&view_desc);
        let occlusion_view = occlusion.create_view(&view_desc);
        let black_view = black.create_view(&view_desc);

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("default pbr sampler"),
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            address_mode_w: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        Self {
            white,
            white_view,
            metallic_roughness,
            metallic_roughness_view,
            normal,
            normal_view,
            occlusion,
            occlusion_view,
            black,
            black_view,
            sampler,
        }
    }
}

/// 解析单个 PBR 槽位：若 Material 携带 handle 则上传贴图并返回新 view;否则返回 None。
pub struct UploadedTexture {
    pub texture: wgpu::Texture,
    pub view: wgpu::TextureView,
}

pub fn maybe_upload(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    materials: &MaterialLibrary,
    handle: Option<crate::TextureHandle>,
) -> Option<UploadedTexture> {
    let texture = materials.texture(handle?)?;
    let uploaded = upload_texture(device, queue, texture);
    let view = uploaded.create_view(&wgpu::TextureViewDescriptor::default());
    Some(UploadedTexture {
        texture: uploaded,
        view,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AlphaMode, Material, TextureHandle};

    #[test]
    fn material_uniform_is_64_bytes() {
        assert_eq!(std::mem::size_of::<MaterialUniform>(), 64);
    }

    #[test]
    fn camera_uniform_is_144_bytes() {
        assert_eq!(std::mem::size_of::<CameraUniform>(), 64 * 2 + 16);
    }

    #[test]
    fn material_flags_bits_set_when_textures_present() {
        let mut m = Material::default();
        m.base_color_texture = Some(TextureHandle(0));
        m.metallic_roughness_texture = Some(TextureHandle(1));
        m.normal_texture = Some(TextureHandle(2));
        m.occlusion_texture = Some(TextureHandle(3));
        m.emissive_texture = Some(TextureHandle(4));
        m.alpha_mode = AlphaMode::Mask;
        m.alpha_cutoff = 0.6;

        let u = MaterialUniform::from_material(&m, true);
        assert_eq!(u.flags[0] & TEX_BIT_BASE_COLOR, TEX_BIT_BASE_COLOR);
        assert_eq!(
            u.flags[0] & TEX_BIT_METALLIC_ROUGHNESS,
            TEX_BIT_METALLIC_ROUGHNESS
        );
        assert_eq!(u.flags[0] & TEX_BIT_NORMAL, TEX_BIT_NORMAL);
        assert_eq!(u.flags[0] & TEX_BIT_OCCLUSION, TEX_BIT_OCCLUSION);
        assert_eq!(u.flags[0] & TEX_BIT_EMISSIVE, TEX_BIT_EMISSIVE);
        assert_eq!(u.flags[1], 1); // Mask
        assert!((u.emissive_alpha_cutoff[3] - 0.6).abs() < 1e-5);
    }

    #[test]
    fn material_normal_bit_requires_tangent_uv() {
        let mut m = Material::default();
        m.normal_texture = Some(TextureHandle(0));
        let u_with = MaterialUniform::from_material(&m, true);
        let u_without = MaterialUniform::from_material(&m, false);
        assert_eq!(u_with.flags[0] & TEX_BIT_NORMAL, TEX_BIT_NORMAL);
        assert_eq!(u_without.flags[0] & TEX_BIT_NORMAL, 0);
    }

    #[test]
    fn alpha_mode_mapping() {
        let mut m = Material::default();
        m.alpha_mode = AlphaMode::Opaque;
        assert_eq!(MaterialUniform::from_material(&m, true).flags[1], 0);
        m.alpha_mode = AlphaMode::Mask;
        assert_eq!(MaterialUniform::from_material(&m, true).flags[1], 1);
        m.alpha_mode = AlphaMode::Blend;
        assert_eq!(MaterialUniform::from_material(&m, true).flags[1], 2);
    }

    #[test]
    fn invert_identity_is_identity() {
        let i = identity_matrix();
        let inv = invert_4x4(i).unwrap();
        for r in 0..4 {
            for c in 0..4 {
                assert!((inv[r][c] - i[r][c]).abs() < 1e-5);
            }
        }
    }

    #[test]
    fn invert_singular_returns_none() {
        let zero = [[0.0; 4]; 4];
        assert!(invert_4x4(zero).is_none());
    }
}
