use std::sync::Arc;

use crate::shader_graph::ShaderGraph;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct MaterialHandle(pub usize);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TextureHandle(pub usize);

#[derive(Clone, Debug)]
pub struct Material {
    pub name: Option<String>,
    pub base_color_factor: [f32; 4],
    pub base_color_texture: Option<TextureHandle>,
    pub metallic_factor: f32,
    pub roughness_factor: f32,
    pub metallic_roughness_texture: Option<TextureHandle>,
    pub normal_texture: Option<TextureHandle>,
    pub occlusion_texture: Option<TextureHandle>,
    pub emissive_factor: [f32; 3],
    pub emissive_texture: Option<TextureHandle>,
    pub alpha_mode: AlphaMode,
    pub alpha_cutoff: f32,
    pub double_sided: bool,
    /// 自定义 shader graph（替换默认 PBR 着色）。`None` = 使用默认 PBR。
    pub custom_shader: Option<Arc<ShaderGraph>>,
}

impl Default for Material {
    fn default() -> Self {
        Self {
            name: None,
            base_color_factor: [1.0, 1.0, 1.0, 1.0],
            base_color_texture: None,
            metallic_factor: 1.0,
            roughness_factor: 1.0,
            metallic_roughness_texture: None,
            normal_texture: None,
            occlusion_texture: None,
            emissive_factor: [0.0, 0.0, 0.0],
            emissive_texture: None,
            alpha_mode: AlphaMode::Opaque,
            alpha_cutoff: 0.5,
            double_sided: false,
            custom_shader: None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AlphaMode {
    Opaque,
    Mask,
    Blend,
}

#[derive(Clone, Debug)]
pub struct Texture {
    pub name: Option<String>,
    pub width: u32,
    pub height: u32,
    pub format: TextureFormat,
    pub pixels: Vec<u8>,
    pub sampler: Sampler,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextureFormat {
    R8,
    R8G8,
    R8G8B8,
    R8G8B8A8,
    R16,
    R16G16,
    R16G16B16,
    R16G16B16A16,
    R32G32B32Float,
    R32G32B32A32Float,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Sampler {
    pub mag_filter: FilterMode,
    pub min_filter: FilterMode,
    pub wrap_s: WrapMode,
    pub wrap_t: WrapMode,
}

impl Default for Sampler {
    fn default() -> Self {
        Self {
            mag_filter: FilterMode::Linear,
            min_filter: FilterMode::Linear,
            wrap_s: WrapMode::Repeat,
            wrap_t: WrapMode::Repeat,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FilterMode {
    Nearest,
    Linear,
    NearestMipmapNearest,
    LinearMipmapNearest,
    NearestMipmapLinear,
    LinearMipmapLinear,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WrapMode {
    ClampToEdge,
    Repeat,
    MirroredRepeat,
}

#[derive(Clone, Debug, Default)]
pub struct MaterialLibrary {
    pub materials: Vec<Material>,
    pub textures: Vec<Texture>,
}

impl MaterialLibrary {
    pub fn material(&self, handle: MaterialHandle) -> Option<&Material> {
        self.materials.get(handle.0)
    }

    pub fn texture(&self, handle: TextureHandle) -> Option<&Texture> {
        self.textures.get(handle.0)
    }
}
