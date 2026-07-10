//! 4.7 后处理骨架：ACES Tone Mapping / Bloom / TAA。
//!
//! 本模块用「效果链 + uniform 数据 + 纯函数」的方式承载，方便先做数学单测，
//! 后续在 wgpu pass 中按 effects 顺序应用。

use bytemuck::{Pod, Zeroable};

/// 单个后处理效果定义。
#[derive(Clone, Debug)]
pub enum PostEffect {
    /// ACES filmic tone mapping，参数为曝光。
    ToneMappingAces { exposure: f32 },
    /// Bloom：threshold 高亮门限，intensity 合成强度，iterations 下采样次数。
    Bloom { threshold: f32, intensity: f32, iterations: u32 },
    /// 时序抗锯齿，jitter_strength 通常 1.0。
    Taa { jitter_strength: f32, feedback: f32 },
    /// 屏幕空间环境光遮蔽 (SSAO/HBAO)。
    Ssao { radius: f32, bias: f32 },
    /// 屏幕空间反射 (SSR)。
    Ssr { max_steps: u32, stride: f32 },
    /// 景深 (Depth of Field)。
    DepthOfField { focus_distance: f32, aperture: f32 },
    /// 运动模糊 (Motion Blur)。
    MotionBlur { intensity: f32 },
}

impl PostEffect {
    pub fn aces(exposure: f32) -> Self { Self::ToneMappingAces { exposure: exposure.max(0.0) } }
    pub fn bloom(threshold: f32, intensity: f32) -> Self {
        Self::Bloom { threshold: threshold.max(0.0), intensity: intensity.max(0.0), iterations: 5 }
    }
    pub fn taa() -> Self { Self::Taa { jitter_strength: 1.0, feedback: 0.9 } }
    pub fn ssao(radius: f32, bias: f32) -> Self {
        Self::Ssao { radius: radius.max(0.0), bias }
    }
    pub fn ssr(max_steps: u32, stride: f32) -> Self {
        Self::Ssr { max_steps: max_steps.min(256), stride: stride.max(0.0) }
    }
    pub fn dof(focus_distance: f32, aperture: f32) -> Self {
        Self::DepthOfField { focus_distance: focus_distance.max(0.0), aperture: aperture.max(0.0) }
    }
    pub fn motion_blur(intensity: f32) -> Self {
        Self::MotionBlur { intensity: intensity.clamp(0.0, 1.0) }
    }
}

/// 后处理链。按 push 顺序执行。
#[derive(Clone, Debug, Default)]
pub struct PostChain {
    pub effects: Vec<PostEffect>,
}

impl PostChain {
    pub fn new() -> Self { Self::default() }
    pub fn push(&mut self, effect: PostEffect) -> &mut Self { self.effects.push(effect); self }
    pub fn len(&self) -> usize { self.effects.len() }
    pub fn is_empty(&self) -> bool { self.effects.is_empty() }
    pub fn has_taa(&self) -> bool {
        self.effects.iter().any(|e| matches!(e, PostEffect::Taa { .. }))
    }
}

/// 上传 GPU 的后处理参数 uniform。
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct PostUniform {
    /// x = exposure, y = bloom_threshold, z = bloom_intensity, w = taa_feedback
    pub params: [f32; 4],
    /// x = taa_jitter_x, y = taa_jitter_y, z = frame_index, w = enabled_mask
    pub frame: [f32; 4],
}

impl Default for PostUniform {
    fn default() -> Self {
        Self { params: [1.0, 1.0, 0.0, 0.9], frame: [0.0, 0.0, 0.0, 0.0] }
    }
}

/// 效果位掩码，写入 `PostUniform.frame.w`（f32 编码 u32 bits）。
///
/// 手写位掋代替 bitflags，避免引入新依赖。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct EffectMask(pub u32);

impl EffectMask {
    pub const TONEMAP: Self = Self(1 << 0);
    pub const BLOOM: Self = Self(1 << 1);
    pub const TAA: Self = Self(1 << 2);
    pub const SSAO: Self = Self(1 << 3);
    pub const SSR: Self = Self(1 << 4);
    pub const DOF: Self = Self(1 << 5);
    pub const MOTION_BLUR: Self = Self(1 << 6);

    pub const fn empty() -> Self { Self(0) }
    pub const fn bits(self) -> u32 { self.0 }
    pub const fn from_bits_truncate(bits: u32) -> Self { Self(bits & 0x7F) }
    pub const fn is_empty(self) -> bool { self.0 == 0 }
    pub const fn contains(self, other: Self) -> bool { (self.0 & other.0) == other.0 }
}

impl std::ops::BitOr for EffectMask {
    type Output = Self;
    fn bitor(self, rhs: Self) -> Self { Self(self.0 | rhs.0) }
}
impl std::ops::BitOrAssign for EffectMask {
    fn bitor_assign(&mut self, rhs: Self) { self.0 |= rhs.0; }
}

/// 把 PostChain 编码为 GPU uniform + 效果掩码。
pub fn build_post_uniform(chain: &PostChain, frame_index: u64) -> PostUniform {
    let mut u = PostUniform::default();
    let mut mask = EffectMask::empty();
    for e in &chain.effects {
        match *e {
            PostEffect::ToneMappingAces { exposure } => {
                u.params[0] = exposure;
                mask |= EffectMask::TONEMAP;
            }
            PostEffect::Bloom { threshold, intensity, .. } => {
                u.params[1] = threshold;
                u.params[2] = intensity;
                mask |= EffectMask::BLOOM;
            }
            PostEffect::Taa { jitter_strength, feedback } => {
                u.params[3] = feedback;
                let (jx, jy) = halton_2_3(frame_index as u32);
                u.frame[0] = (jx - 0.5) * jitter_strength;
                u.frame[1] = (jy - 0.5) * jitter_strength;
                mask |= EffectMask::TAA;
            }
            PostEffect::Ssao { .. } => {
                mask |= EffectMask::SSAO;
            }
            PostEffect::Ssr { .. } => {
                mask |= EffectMask::SSR;
            }
            PostEffect::DepthOfField { .. } => {
                mask |= EffectMask::DOF;
            }
            PostEffect::MotionBlur { .. } => {
                mask |= EffectMask::MOTION_BLUR;
            }
        }
    }
    u.frame[2] = frame_index as f32;
    u.frame[3] = f32::from_bits(mask.bits());
    u
}

/// ACES filmic 色调映射（Krzysztof Narkowicz 简化拟合，与 Unreal/Unity 主流实现一致）。
pub fn aces_tonemap(linear: [f32; 3]) -> [f32; 3] {
    const A: f32 = 2.51;
    const B: f32 = 0.03;
    const C: f32 = 2.43;
    const D: f32 = 0.59;
    const E: f32 = 0.14;
    let map = |x: f32| {
        let num = x * (A * x + B);
        let den = x * (C * x + D) + E;
        (num / den).clamp(0.0, 1.0)
    };
    [map(linear[0]), map(linear[1]), map(linear[2])]
}

/// Halton(2,3) 序列：TAA jitter 的标准选择，范围 (0,1)。
pub fn halton_2_3(frame: u32) -> (f32, f32) {
    fn halton(mut index: u32, base: u32) -> f32 {
        let mut result = 0.0;
        let mut f = 1.0 / base as f32;
        while index > 0 {
            result += f * (index % base) as f32;
            index /= base;
            f /= base as f32;
        }
        result
    }
    (halton(frame + 1, 2), halton(frame + 1, 3))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aces_black_is_black() {
        let o = aces_tonemap([0.0; 3]);
        assert!(o[0] < 1e-4 && o[1] < 1e-4 && o[2] < 1e-4);
    }

    #[test]
    fn aces_clamps_to_one() {
        let o = aces_tonemap([100.0, 100.0, 100.0]);
        for c in o { assert!(c <= 1.0 + 1e-6 && c > 0.5); }
    }

    #[test]
    fn halton_in_unit_range() {
        for i in 0..32 {
            let (x, y) = halton_2_3(i);
            assert!(x > 0.0 && x < 1.0);
            assert!(y > 0.0 && y < 1.0);
        }
    }

    #[test]
    fn chain_push_order_preserved() {
        let mut c = PostChain::new();
        c.push(PostEffect::aces(1.0)).push(PostEffect::bloom(1.0, 0.4)).push(PostEffect::taa());
        assert_eq!(c.len(), 3);
        assert!(c.has_taa());
    }

    #[test]
    fn build_uniform_writes_mask_and_jitter() {
        let mut c = PostChain::new();
        c.push(PostEffect::aces(1.5)).push(PostEffect::bloom(0.8, 0.3)).push(PostEffect::taa());
        let u = build_post_uniform(&c, 7);
        assert!((u.params[0] - 1.5).abs() < 1e-5);
        assert!((u.params[1] - 0.8).abs() < 1e-5);
        let mask = EffectMask::from_bits_truncate(u.frame[3].to_bits());
        assert!(mask.contains(EffectMask::TONEMAP | EffectMask::BLOOM | EffectMask::TAA));
        assert!((u.frame[2] - 7.0).abs() < 1e-5);
        // TAA jitter 应非零
        assert!(u.frame[0].abs() + u.frame[1].abs() > 1e-4);
    }

    #[test]
    fn empty_chain_has_zero_mask() {
        let u = build_post_uniform(&PostChain::new(), 0);
        assert!(EffectMask::from_bits_truncate(u.frame[3].to_bits()).is_empty());
    }
}
