//! 资源 Cooking 管线 — 纹理处理 + 格式转换。
//!
//! Feature gate: `cooking`（默认禁用）。
//!
//! 纹理处理：图像解码 → RGBA8 转换 → Mipmap 生成 → wgpu 友好格式编码。
//! 全部基于纯 CPU 算法，不依赖 GPU 或平台特定压缩库（如 basis-universal），
//! 确保在 Windows / Linux / macOS 上均可编译运行。

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// 纹理配置 & 输出类型
// ---------------------------------------------------------------------------

/// 纹理 cooking 配置。
#[derive(Clone, Debug)]
pub struct TextureCookConfig {
    /// 目标平台（影响默认格式选择）
    pub target: CookTarget,
    /// 压缩质量（0-100），当前影响 mipmap 滤波质量
    pub quality: u32,
    /// 是否生成 mipmap 链
    pub generate_mipmaps: bool,
    /// 目标像素格式
    pub target_format: TextureOutputFormat,
}

/// 纹理输出像素格式（wgpu 友好）。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum TextureOutputFormat {
    /// 标准 8-bit RGBA，对应 wgpu `Rgba8Unorm`
    Rgba8Unorm,
    /// sRGB 空间的 8-bit RGBA，对应 wgpu `Rgba8UnormSrgb`
    Rgba8UnormSrgb,
    /// 预乘 alpha 的 8-bit RGBA
    Rgba8UnormPremultiplied,
    /// 8-bit RGB（无 alpha），对应 wgpu `Rgb8Unorm`
    Rgb8Unorm,
}

/// 纹理 cook 输出。
#[derive(Clone, Debug)]
pub struct TextureOutput {
    /// 所有 mip 级别的像素数据（按顺序拼接）
    pub data: Vec<u8>,
    /// 各级 mipmap 的字节偏移
    pub mip_offsets: Vec<usize>,
    /// 各级 mipmap 的 (width, height)
    pub mip_sizes: Vec<(u32, u32)>,
    /// 输出像素格式
    pub format: TextureOutputFormat,
}

/// Cooking 目标平台。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum CookTarget {
    Desktop,
    Mobile,
    Android,
}

impl Default for TextureCookConfig {
    fn default() -> Self {
        Self {
            target: CookTarget::Desktop,
            quality: 80,
            generate_mipmaps: true,
            target_format: TextureOutputFormat::Rgba8Unorm,
        }
    }
}

// ---------------------------------------------------------------------------
// TextureCooker
// ---------------------------------------------------------------------------

/// Texture cooker: 将原始像素/图像数据处理为 GPU 友好格式。
///
/// 处理管线：
/// 1. 图像解码（PNG/JPEG → RGBA8）— 需要 `image` crate
/// 2. 格式转换（RGBA8 → 目标格式）
/// 3. Mipmap 链生成（box-filter 降采样）
/// 4. 编码输出
pub struct TextureCooker;

impl TextureCooker {
    /// 处理 RGBA8 原始像素数据（向后兼容的简化接口）。
    ///
    /// 输入 `pixels` 必须为 `width * height * 4` 字节的 RGBA8 数据。
    /// 返回处理后的像素数据（含 mipmap 链，如已启用）。
    pub fn cook(
        pixels: &[u8],
        width: u32,
        height: u32,
        config: &TextureCookConfig,
    ) -> Vec<u8> {
        Self::cook_full(pixels, width, height, config).data
    }

    /// 完整纹理处理管线，返回结构化输出。
    ///
    /// 输入 `pixels` 必须为 `width * height * 4` 字节的 RGBA8 数据。
    pub fn cook_full(
        pixels: &[u8],
        width: u32,
        height: u32,
        config: &TextureCookConfig,
    ) -> TextureOutput {
        let expected = (width as usize) * (height as usize) * 4;
        assert!(
            pixels.len() >= expected,
            "TextureCooker: pixel data too small (expected >= {} bytes for {}x{} RGBA8, got {})",
            expected,
            width,
            height,
            pixels.len()
        );

        // 1. 裁剪到精确尺寸（输入可能含多余字节）
        let rgba = &pixels[..expected];

        // 2. 生成 mipmap 链或仅保留原始级别
        let mip_chain: Vec<Vec<u8>> = if config.generate_mipmaps {
            Self::generate_mipmaps(rgba, width, height, config.quality)
        } else {
            vec![rgba.to_vec()]
        };

        // 3. 编码为目标格式
        Self::encode_mip_chain(&mip_chain, width, height, config.target_format)
    }

    /// 从压缩图像数据（PNG/JPEG/BMP/TGA 文件字节）解码为 RGBA8 像素。
    ///
    /// 支持格式由 `image` crate 的启用 feature 决定（默认 PNG + JPEG）。
    pub fn decode_image(data: &[u8]) -> Result<(Vec<u8>, u32, u32), String> {
        Self::decode_image_with_image(data)
    }

    /// 返回目标平台的推荐格式名称。
    pub fn format_name(target: CookTarget) -> &'static str {
        match target {
            CookTarget::Desktop => "Rgba8Unorm",
            CookTarget::Mobile => "Rgba8UnormSrgb",
            CookTarget::Android => "Rgba8Unorm",
        }
    }

    /// 返回给定尺寸的 mipmap 级别数（含 base level）。
    pub fn mip_level_count(width: u32, height: u32) -> u32 {
        let max_dim = width.max(height);
        (32 - max_dim.leading_zeros()).max(1)
    }

    // -----------------------------------------------------------------------
    // 内部方法
    // -----------------------------------------------------------------------

    /// 使用 `image` crate 解码图像文件字节为 RGBA8。
    fn decode_image_with_image(data: &[u8]) -> Result<(Vec<u8>, u32, u32), String> {
        let img = image::load_from_memory(data)
            .map_err(|e| format!("Image decode error: {e}"))?;
        let rgba = img.into_rgba8();
        let (w, h) = rgba.dimensions();
        Ok((rgba.into_raw(), w, h))
    }

    /// 生成 mipmap 链（box-filter 降采样）。
    ///
    /// quality 参数影响降采样滤波：
    /// - >= 50: 标准 2x2 box filter（高质量）
    /// - < 50: 最近邻降采样（快速）
    fn generate_mipmaps(rgba: &[u8], width: u32, height: u32, quality: u32) -> Vec<Vec<u8>> {
        let mut mips = vec![rgba.to_vec()];
        let mut w = width;
        let mut h = height;

        while w > 1 || h > 1 {
            let (new_w, new_h) = ((w / 2).max(1), (h / 2).max(1));
            let prev = mips.last().unwrap();
            let mip = if quality >= 50 {
                Self::downsample_box(prev, w, h, new_w, new_h)
            } else {
                Self::downsample_nearest(prev, w, h, new_w, new_h)
            };
            mips.push(mip);
            w = new_w;
            h = new_h;
        }

        mips
    }

    /// 2x2 box-filter 降采样（4 像素取平均）。
    fn downsample_box(
        src: &[u8],
        src_w: u32,
        src_h: u32,
        dst_w: u32,
        dst_h: u32,
    ) -> Vec<u8> {
        let mut dst = vec![0u8; (dst_w * dst_h * 4) as usize];

        for dy in 0..dst_h {
            for dx in 0..dst_w {
                let sx = (dx * 2).min(src_w - 1);
                let sy = (dy * 2).min(src_h - 1);
                let sx1 = (sx + 1).min(src_w - 1);
                let sy1 = (sy + 1).min(src_h - 1);

                let p00 = pixel_at(src, src_w, sx, sy);
                let p10 = pixel_at(src, src_w, sx1, sy);
                let p01 = pixel_at(src, src_w, sx, sy1);
                let p11 = pixel_at(src, src_w, sx1, sy1);

                let di = ((dy * dst_w + dx) * 4) as usize;
                for c in 0..4 {
                    dst[di + c] =
                        ((p00[c] as u32 + p10[c] as u32 + p01[c] as u32 + p11[c] as u32 + 2)
                            / 4) as u8;
                }
            }
        }

        dst
    }

    /// 最近邻降采样（快速但质量较低）。
    fn downsample_nearest(
        src: &[u8],
        src_w: u32,
        src_h: u32,
        dst_w: u32,
        dst_h: u32,
    ) -> Vec<u8> {
        let mut dst = vec![0u8; (dst_w * dst_h * 4) as usize];

        for dy in 0..dst_h {
            for dx in 0..dst_w {
                let sx = (dx * 2).min(src_w - 1);
                let sy = (dy * 2).min(src_h - 1);
                let si = ((sy * src_w + sx) * 4) as usize;
                let di = ((dy * dst_w + dx) * 4) as usize;
                dst[di..di + 4].copy_from_slice(&src[si..si + 4]);
            }
        }

        dst
    }

    /// 将 mipmap 链编码为目标像素格式。
    fn encode_mip_chain(
        mip_chain: &[Vec<u8>],
        base_w: u32,
        base_h: u32,
        format: TextureOutputFormat,
    ) -> TextureOutput {
        let mut data = Vec::new();
        let mut mip_offsets = Vec::new();
        let mut mip_sizes = Vec::new();

        let mut w = base_w;
        let mut h = base_h;

        for mip in mip_chain {
            mip_offsets.push(data.len());
            mip_sizes.push((w, h));

            match format {
                TextureOutputFormat::Rgba8Unorm
                | TextureOutputFormat::Rgba8UnormSrgb
                | TextureOutputFormat::Rgba8UnormPremultiplied => {
                    data.extend_from_slice(mip);
                }
                TextureOutputFormat::Rgb8Unorm => {
                    // RGBA8 → RGB8：丢弃 alpha 通道
                    for chunk in mip.chunks_exact(4) {
                        data.extend_from_slice(&chunk[..3]);
                    }
                }
            }

            w = (w / 2).max(1);
            h = (h / 2).max(1);
        }

        TextureOutput {
            data,
            mip_offsets,
            mip_sizes,
            format,
        }
    }
}

/// 从 RGBA8 像素缓冲区读取指定坐标的像素。
#[inline]
fn pixel_at(buf: &[u8], width: u32, x: u32, y: u32) -> [u8; 4] {
    let i = ((y * width + x) * 4) as usize;
    [buf[i], buf[i + 1], buf[i + 2], buf[i + 3]]
}

// ---------------------------------------------------------------------------
// 测试
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cook_without_mipmaps_returns_original() {
        let pixels = vec![128u8; 4 * 4 * 4]; // 4x4 RGBA8
        let mut cfg = TextureCookConfig::default();
        cfg.generate_mipmaps = false;
        let out = TextureCooker::cook_full(&pixels, 4, 4, &cfg);
        assert_eq!(out.mip_sizes.len(), 1);
        assert_eq!(out.mip_sizes[0], (4, 4));
        assert_eq!(out.data.len(), 4 * 4 * 4);
    }

    #[test]
    fn cook_with_mipmaps_generates_chain() {
        let pixels = vec![200u8; 8 * 8 * 4]; // 8x8 RGBA8
        let cfg = TextureCookConfig::default(); // mipmaps enabled
        let out = TextureCooker::cook_full(&pixels, 8, 8, &cfg);
        // 8x8 → 4x4 → 2x2 → 1x1 = 4 levels
        assert_eq!(out.mip_sizes.len(), 4);
        assert_eq!(out.mip_sizes[0], (8, 8));
        assert_eq!(out.mip_sizes[1], (4, 4));
        assert_eq!(out.mip_sizes[2], (2, 2));
        assert_eq!(out.mip_sizes[3], (1, 1));
    }

    #[test]
    fn cook_rgb8_drops_alpha() {
        let mut pixels = vec![0u8; 2 * 2 * 4];
        // Set alpha to 0 to verify it's stripped
        for i in 0..4 {
            pixels[i * 4 + 3] = 0; // alpha = 0
            pixels[i * 4] = 255; // R
        }
        let mut cfg = TextureCookConfig::default();
        cfg.generate_mipmaps = false;
        cfg.target_format = TextureOutputFormat::Rgb8Unorm;
        let out = TextureCooker::cook_full(&pixels, 2, 2, &cfg);
        // RGB8: 2*2*3 = 12 bytes
        assert_eq!(out.data.len(), 12);
        assert_eq!(out.format, TextureOutputFormat::Rgb8Unorm);
    }

    #[test]
    fn mip_level_count_correct() {
        assert_eq!(TextureCooker::mip_level_count(1, 1), 1);
        assert_eq!(TextureCooker::mip_level_count(2, 2), 2);
        assert_eq!(TextureCooker::mip_level_count(8, 8), 4);
        assert_eq!(TextureCooker::mip_level_count(256, 256), 9);
    }

    #[test]
    fn format_name_returns_correct() {
        assert_eq!(TextureCooker::format_name(CookTarget::Desktop), "Rgba8Unorm");
        assert_eq!(TextureCooker::format_name(CookTarget::Mobile), "Rgba8UnormSrgb");
        assert_eq!(TextureCooker::format_name(CookTarget::Android), "Rgba8Unorm");
    }

    #[test]
    fn mipmap_preserves_solid_color() {
        // 纯色纹理降采样后仍为纯色
        let color = [100u8, 150, 200, 255];
        let pixels: Vec<u8> = color.into_iter().cycle().take(4 * 4 * 4).collect();
        let cfg = TextureCookConfig::default();
        let out = TextureCooker::cook_full(&pixels, 4, 4, &cfg);
        // 检查最后一级 (1x1) 的像素值
        let last_offset = *out.mip_offsets.last().unwrap();
        let last_pixel = &out.data[last_offset..last_offset + 4];
        // box filter of solid color should be exact
        assert_eq!(last_pixel, &color);
    }
}
