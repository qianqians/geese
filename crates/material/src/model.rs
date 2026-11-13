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
    /// glTF `normalTexture.scale`，默认 1.0。
    pub normal_scale: f32,
    pub occlusion_texture: Option<TextureHandle>,
    /// glTF `occlusionTexture.strength`，默认 1.0。
    pub occlusion_strength: f32,
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
            normal_scale: 1.0,
            occlusion_texture: None,
            occlusion_strength: 1.0,
            emissive_factor: [0.0, 0.0, 0.0],
            emissive_texture: None,
            alpha_mode: AlphaMode::Opaque,
            alpha_cutoff: 0.5,
            double_sided: false,
            custom_shader: None,
        }
    }
}

impl Material {
    /// 把所有纹理句柄整体平移 `offset`。合并两个材质库时使用，
    /// 避免第二个库的纹理索引覆盖第一个库的贴图。
    pub fn remap_texture_handles(&mut self, offset: usize) {
        if offset == 0 {
            return;
        }
        for slot in [
            &mut self.base_color_texture,
            &mut self.metallic_roughness_texture,
            &mut self.normal_texture,
            &mut self.occlusion_texture,
            &mut self.emissive_texture,
        ] {
            if let Some(handle) = slot {
                handle.0 += offset;
            }
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

    /// 可变访问材质，用于按名覆盖 glTF 材质的参数（`apply_to` 只改声明的变量）。
    pub fn material_mut(&mut self, handle: MaterialHandle) -> Option<&mut Material> {
        self.materials.get_mut(handle.0)
    }

    pub fn texture(&self, handle: TextureHandle) -> Option<&Texture> {
        self.textures.get(handle.0)
    }

    /// 按名称查找材质。引擎内置默认材质以固定名称注册（如 `default`）。
    pub fn find_by_name(&self, name: &str) -> Option<MaterialHandle> {
        self.materials
            .iter()
            .position(|material| material.name.as_deref() == Some(name))
            .map(MaterialHandle)
    }

    /// 追加一个材质，返回其句柄。
    pub fn insert(&mut self, material: Material) -> MaterialHandle {
        let handle = MaterialHandle(self.materials.len());
        self.materials.push(material);
        handle
    }

    /// 把 `other` 追加到本库末尾，并重映射 `other` 中所有纹理句柄。
    ///
    /// 返回**材质偏移量**：`other` 中第 `i` 个材质在本库中的句柄是
    /// `MaterialHandle(material_offset + i)`。既有句柄不受影响。
    pub fn append_library(&mut self, mut other: MaterialLibrary) -> usize {
        let texture_offset = self.textures.len();
        let material_offset = self.materials.len();

        if texture_offset > 0 {
            for material in &mut other.materials {
                material.remap_texture_handles(texture_offset);
            }
        }

        self.materials.append(&mut other.materials);
        self.textures.append(&mut other.textures);
        material_offset
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn solid_texture() -> Texture {
        Texture {
            name: None,
            width: 1,
            height: 1,
            format: TextureFormat::R8G8B8A8,
            pixels: vec![255, 255, 255, 255],
            sampler: Sampler::default(),
        }
    }

    #[test]
    fn find_by_name_matches_named_material() {
        let mut library = MaterialLibrary::default();
        library.insert(Material::default()); // 匿名
        let mut named = Material::default();
        named.name = Some("default".into());
        let handle = library.insert(named);

        assert_eq!(library.find_by_name("default"), Some(handle));
        assert_eq!(library.find_by_name("missing"), None);
    }

    #[test]
    fn insert_returns_sequential_handles() {
        let mut library = MaterialLibrary::default();
        assert_eq!(library.insert(Material::default()), MaterialHandle(0));
        assert_eq!(library.insert(Material::default()), MaterialHandle(1));
        assert_eq!(library.materials.len(), 2);
    }

    #[test]
    fn append_library_remaps_texture_handles() {
        let mut base = MaterialLibrary::default();
        base.textures.push(solid_texture());
        base.materials.push(Material::default());

        let mut other = MaterialLibrary::default();
        other.textures.push(solid_texture());
        let mut material = Material::default();
        material.base_color_texture = Some(TextureHandle(0));
        material.emissive_texture = Some(TextureHandle(0));
        other.materials.push(material);

        let material_offset = base.append_library(other);

        assert_eq!(material_offset, 1);
        let appended = &base.materials[1];
        assert_eq!(appended.base_color_texture, Some(TextureHandle(1)));
        assert_eq!(appended.emissive_texture, Some(TextureHandle(1)));
        assert_eq!(appended.normal_texture, None);
        assert_eq!(base.textures.len(), 2);
    }

    #[test]
    fn append_library_to_empty_base_keeps_original_indices() {
        let mut base = MaterialLibrary::default();
        let mut other = MaterialLibrary::default();
        other.textures.push(solid_texture());
        let mut material = Material::default();
        material.occlusion_texture = Some(TextureHandle(0));
        other.materials.push(material);

        let material_offset = base.append_library(other);

        assert_eq!(material_offset, 0);
        assert_eq!(base.materials[0].occlusion_texture, Some(TextureHandle(0)));
    }

    #[test]
    fn remap_texture_handles_is_noop_for_zero_offset() {
        let mut material = Material::default();
        material.base_color_texture = Some(TextureHandle(3));
        material.remap_texture_handles(0);
        assert_eq!(material.base_color_texture, Some(TextureHandle(3)));
    }
}
