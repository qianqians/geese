use render::{
    AlphaMode, FilterMode, Material, MaterialLibrary, Sampler, Texture, TextureFormat,
    TextureHandle, WrapMode,
};

pub fn load_material_library(
    document: &gltf::Document,
    images: &[gltf::image::Data],
) -> MaterialLibrary {
    let textures = document
        .textures()
        .map(|texture| {
            let source = texture.source();
            let image = &images[source.index()];

            Texture {
                name: texture.name().map(str::to_string),
                width: image.width,
                height: image.height,
                format: convert_texture_format(image.format),
                pixels: image.pixels.clone(),
                sampler: convert_sampler(texture.sampler()),
            }
        })
        .collect();

    let mut materials: Vec<_> = document
        .materials()
        .map(|material| {
            let pbr = material.pbr_metallic_roughness();

            Material {
                name: material.name().map(str::to_string),
                base_color_factor: pbr.base_color_factor(),
                base_color_texture: pbr
                    .base_color_texture()
                    .map(|info| TextureHandle(info.texture().index())),
                metallic_factor: pbr.metallic_factor(),
                roughness_factor: pbr.roughness_factor(),
                metallic_roughness_texture: pbr
                    .metallic_roughness_texture()
                    .map(|info| TextureHandle(info.texture().index())),
                normal_texture: material
                    .normal_texture()
                    .map(|info| TextureHandle(info.texture().index())),
                occlusion_texture: material
                    .occlusion_texture()
                    .map(|info| TextureHandle(info.texture().index())),
                emissive_factor: material.emissive_factor(),
                emissive_texture: material
                    .emissive_texture()
                    .map(|info| TextureHandle(info.texture().index())),
                alpha_mode: convert_alpha_mode(material.alpha_mode()),
                alpha_cutoff: material.alpha_cutoff().unwrap_or(0.5),
                double_sided: material.double_sided(),
            }
        })
        .collect();

    if materials.is_empty() {
        materials.push(Material::default());
    }

    MaterialLibrary {
        materials,
        textures,
    }
}

fn convert_texture_format(format: gltf::image::Format) -> TextureFormat {
    match format {
        gltf::image::Format::R8 => TextureFormat::R8,
        gltf::image::Format::R8G8 => TextureFormat::R8G8,
        gltf::image::Format::R8G8B8 => TextureFormat::R8G8B8,
        gltf::image::Format::R8G8B8A8 => TextureFormat::R8G8B8A8,
        gltf::image::Format::R16 => TextureFormat::R16,
        gltf::image::Format::R16G16 => TextureFormat::R16G16,
        gltf::image::Format::R16G16B16 => TextureFormat::R16G16B16,
        gltf::image::Format::R16G16B16A16 => TextureFormat::R16G16B16A16,
        gltf::image::Format::R32G32B32FLOAT => TextureFormat::R32G32B32Float,
        gltf::image::Format::R32G32B32A32FLOAT => TextureFormat::R32G32B32A32Float,
    }
}

fn convert_sampler(sampler: gltf::texture::Sampler) -> Sampler {
    Sampler {
        mag_filter: sampler
            .mag_filter()
            .map(convert_mag_filter)
            .unwrap_or(FilterMode::Linear),
        min_filter: sampler
            .min_filter()
            .map(convert_min_filter)
            .unwrap_or(FilterMode::Linear),
        wrap_s: convert_wrap_mode(sampler.wrap_s()),
        wrap_t: convert_wrap_mode(sampler.wrap_t()),
    }
}

fn convert_mag_filter(filter: gltf::texture::MagFilter) -> FilterMode {
    match filter {
        gltf::texture::MagFilter::Nearest => FilterMode::Nearest,
        gltf::texture::MagFilter::Linear => FilterMode::Linear,
    }
}

fn convert_min_filter(filter: gltf::texture::MinFilter) -> FilterMode {
    match filter {
        gltf::texture::MinFilter::Nearest => FilterMode::Nearest,
        gltf::texture::MinFilter::Linear => FilterMode::Linear,
        gltf::texture::MinFilter::NearestMipmapNearest => FilterMode::NearestMipmapNearest,
        gltf::texture::MinFilter::LinearMipmapNearest => FilterMode::LinearMipmapNearest,
        gltf::texture::MinFilter::NearestMipmapLinear => FilterMode::NearestMipmapLinear,
        gltf::texture::MinFilter::LinearMipmapLinear => FilterMode::LinearMipmapLinear,
    }
}

fn convert_wrap_mode(mode: gltf::texture::WrappingMode) -> WrapMode {
    match mode {
        gltf::texture::WrappingMode::ClampToEdge => WrapMode::ClampToEdge,
        gltf::texture::WrappingMode::Repeat => WrapMode::Repeat,
        gltf::texture::WrappingMode::MirroredRepeat => WrapMode::MirroredRepeat,
    }
}

fn convert_alpha_mode(mode: gltf::material::AlphaMode) -> AlphaMode {
    match mode {
        gltf::material::AlphaMode::Opaque => AlphaMode::Opaque,
        gltf::material::AlphaMode::Mask => AlphaMode::Mask,
        gltf::material::AlphaMode::Blend => AlphaMode::Blend,
    }
}
