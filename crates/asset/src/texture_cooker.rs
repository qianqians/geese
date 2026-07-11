//! 资源 Cooking 管线 — 纹理压缩 + 网格优化。
//!
//! Feature gate: `cooking`（默认禁用）。
//!
//! 纹理压缩：将 PNG/JPEG 编码为 BC7 (Desktop) / ASTC (Mobile) / ETC2 (Android) 格式。
//! 网格优化：顶点重排（vertex fetch optimization）、索引优化（overdraw optimization）。

/// 纹理 cooking 配置。
#[derive(Clone, Debug)]
pub struct TextureCookConfig {
    /// 目标平台：Desktop = BC7, Mobile = ASTC, Android = ETC2
    pub target: CookTarget,
    /// 压缩质量（0-100）
    pub quality: u32,
}

/// 网格 cooking 配置。
#[derive(Clone, Debug)]
pub struct MeshCookConfig {
    /// 是否优化顶点缓存
    pub optimize_vertex_cache: bool,
    /// 是否优化过度绘制
    pub optimize_overdraw: bool,
    /// overdraw optimization 阈值
    pub overdraw_threshold: f32,
}

/// Cooking 目标平台。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CookTarget {
    Desktop,   // BC7
    Mobile,    // ASTC
    Android,   // ETC2
}

impl Default for TextureCookConfig {
    fn default() -> Self {
        Self {
            target: CookTarget::Desktop,
            quality: 80,
        }
    }
}

impl Default for MeshCookConfig {
    fn default() -> Self {
        Self {
            optimize_vertex_cache: true,
            optimize_overdraw: true,
            overdraw_threshold: 1.05,
        }
    }
}

/// Texture cooker: 将原始像素数据压缩为目标格式。
pub struct TextureCooker;

impl TextureCooker {
    /// 压缩纹理数据。
    ///
    /// 当前为 stub 实现——直接返回原始数据。
    /// 完整实现需要集成 `basis-universal` crate 或调用外部 basisu 工具。
    pub fn cook(pixels: &[u8], width: u32, height: u32, config: &TextureCookConfig) -> Vec<u8> {
        let _ = (width, height, config);
        // Stub: 返回原始 RGBA8 数据
        // TODO: 集成 basis-universal 进行 BC7/ASTC/ETC2 编码
        log::warn!("TextureCooker::cook is a stub — returning raw RGBA8 data. Integrate basis-universal for BC7/ASTC/ETC2 compression.");
        pixels.to_vec()
    }

    /// 返回目标平台的压缩格式名称。
    pub fn format_name(target: CookTarget) -> &'static str {
        match target {
            CookTarget::Desktop => "BC7",
            CookTarget::Mobile => "ASTC",
            CookTarget::Android => "ETC2",
        }
    }
}

/// Mesh cooker: 优化顶点和索引数据。
pub struct MeshCooker;

impl MeshCooker {
    /// 优化网格数据。
    ///
    /// 当前为 stub 实现——直接返回原始数据。
    /// 完整实现需要集成 `meshopt` crate。
    pub fn cook(
        vertices: &[u8],
        indices: &[u32],
        _vertex_stride: usize,
        config: &MeshCookConfig,
    ) -> (Vec<u8>, Vec<u32>) {
        let _ = config;
        // Stub: 返回原始数据
        // TODO: 集成 meshopt 进行 vertex fetch + overdraw optimization
        log::warn!("MeshCooker::cook is a stub — returning raw vertex/index data. Integrate meshopt for vertex fetch + overdraw optimization.");
        (vertices.to_vec(), indices.to_vec())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn texture_cooker_stub_preserves_data() {
        let pixels = vec![255u8; 64];
        let result = TextureCooker::cook(&pixels, 4, 4, &TextureCookConfig::default());
        assert_eq!(result, pixels);
    }

    #[test]
    fn mesh_cooker_stub_preserves_data() {
        let verts = vec![0u8; 128];
        let indices = vec![0u32, 1, 2];
        let (v, i) = MeshCooker::cook(&verts, &indices, 32, &MeshCookConfig::default());
        assert_eq!(v, verts);
        assert_eq!(i, indices);
    }

    #[test]
    fn format_name_returns_correct() {
        assert_eq!(TextureCooker::format_name(CookTarget::Desktop), "BC7");
        assert_eq!(TextureCooker::format_name(CookTarget::Mobile), "ASTC");
        assert_eq!(TextureCooker::format_name(CookTarget::Android), "ETC2");
    }
}
