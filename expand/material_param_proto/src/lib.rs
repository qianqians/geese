//! 材质参数「单一声明源」原型（非生产代码，仅用于验证设计文档 §附录 B 的可行性）。
//!
//! 目标：一次声明同时产出
//!   1. `Material` 结构体字段
//!   2. `Default` 实现
//!   3. `MaterialUniform` 的打包代码（字段 → vec4 槽位 + 分量）
//!   4. `MaterialUniform` 的 WGSL 声明文本
//!
//! 生产版本只需把第 3 步换成 bytemuck 写入 + 第 4 步换成 build-script 落盘，
//! 机制不变。

/// 单值 → uniform 分量的写入策略。
pub trait Pack {
    fn write(&self, slot: &mut [f32; 4], comp: usize);
}

impl Pack for f32 {
    fn write(&self, slot: &mut [f32; 4], comp: usize) {
        slot[comp] = *self;
    }
}

impl Pack for bool {
    fn write(&self, slot: &mut [f32; 4], comp: usize) {
        slot[comp] = if *self { 1.0 } else { 0.0 };
    }
}

impl Pack for [f32; 4] {
    fn write(&self, slot: &mut [f32; 4], _comp: usize) {
        *slot = *self;
    }
}

impl Pack for [f32; 3] {
    fn write(&self, slot: &mut [f32; 4], comp: usize) {
        slot[comp..comp + 3].copy_from_slice(self);
    }
}

/// 材质参数单一声明源。
///
/// ```ignore
/// material_layout! {
///     uniform MaterialUniform {
///         slot 0 => "base_color_factor" => "vec4<f32>";
///         slot 2 => "metallic_roughness_normal_occlusion" => "vec4<f32>";
///     }
///     material Material {
///         metallic_factor: f32 = (1.0) @ (2, x);
///         roughness_factor: f32 = (1.0) @ (2, y);
///     }
/// }
/// ```
///
/// 新增一个参数 = 在 `material` 段加一行；不需要再碰结构体、Default、打包函数、
/// WGSL 结构体四份代码。
#[macro_export]
macro_rules! material_layout {
    (
        uniform $uniform:ident {
            $( slot $slot_idx:literal => $slot_name:literal => $slot_ty:literal; )*
        }
        material $mat:ident {
            $( $field:ident : $ty:ty = ($default:expr) @ ($sidx:literal, $comp:ident) ; )*
        }
    ) => {
        #[derive(Clone, Copy, Debug, PartialEq)]
        pub struct $mat {
            $( pub $field: $ty, )*
        }

        impl Default for $mat {
            fn default() -> Self {
                Self { $( $field: $default, )* }
            }
        }

        impl $mat {
            /// 把材质参数写入 4 个 `vec4<f32>` 槽位。
            ///
            /// 生产版本会改为 `#[repr(C)] struct MaterialUniform` 的 bytemuck 写入，
            /// 但「字段 → (槽位, 分量)」的映射依然只来自上面的声明。
            pub fn pack_into(&self, slots: &mut [[f32; 4]; 4]) {
                $(
                    {
                        let comp: usize = $crate::material_layout!(@comp $comp);
                        <$ty as $crate::Pack>::write(&self.$field, &mut slots[$sidx], comp);
                    }
                )*
            }

            /// 与 Rust 结构同源生成的 WGSL 声明。
            pub const WGSL_STRUCT: &'static str = concat!(
                "struct ", stringify!($uniform), " {\n",
                $(
                    "    ", $slot_name, ": ", $slot_ty, ",\n",
                )*
                "};\n",
            );
        }
    };

    (@comp x) => { 0usize };
    (@comp y) => { 1usize };
    (@comp z) => { 2usize };
    (@comp w) => { 3usize };
    (@comp vec) => { 0usize };
}

// ─────────────────────────────────────────────────────────────────────────────
// 演示：现有 4 槽位布局 + 5 个参数
// ─────────────────────────────────────────────────────────────────────────────

crate::material_layout! {
    uniform MaterialUniform {
        slot 0 => "base_color_factor" => "vec4<f32>";
        slot 1 => "emissive_alpha_cutoff" => "vec4<f32>";
        slot 2 => "metallic_roughness_normal_occlusion" => "vec4<f32>";
    }
    material Material {
        base_color_factor: [f32; 4] = ([1.0, 1.0, 1.0, 1.0]) @ (0, vec);
        metallic_factor: f32 = (1.0) @ (2, x);
        roughness_factor: f32 = (1.0) @ (2, y);
        normal_scale: f32 = (1.0) @ (2, z);
        occlusion_strength: f32 = (1.0) @ (2, w);
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// 演示：新增参数（clearcoat）的全部改动 = material 段里两行
// ─────────────────────────────────────────────────────────────────────────────

crate::material_layout! {
    uniform MaterialUniformV2 {
        slot 0 => "base_color_factor" => "vec4<f32>";
        slot 1 => "emissive_alpha_cutoff" => "vec4<f32>";
        slot 2 => "metallic_roughness_normal_occlusion" => "vec4<f32>";
        slot 3 => "clearcoat_clearcoat_roughness_pad" => "vec4<f32>";
    }
    material MaterialV2 {
        base_color_factor: [f32; 4] = ([1.0, 1.0, 1.0, 1.0]) @ (0, vec);
        metallic_factor: f32 = (1.0) @ (2, x);
        roughness_factor: f32 = (1.0) @ (2, y);
        normal_scale: f32 = (1.0) @ (2, z);
        occlusion_strength: f32 = (1.0) @ (2, w);
        // ↓↓↓ 新增参数的全部改动 ↓↓↓
        clearcoat: f32 = (0.0) @ (3, x);
        clearcoat_roughness: f32 = (0.0) @ (3, y);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_pack_into_expected_slots() {
        let mut slots = [[0.0f32; 4]; 4];
        Material::default().pack_into(&mut slots);

        assert_eq!(slots[0], [1.0, 1.0, 1.0, 1.0]); // base color
        assert_eq!(slots[2][0], 1.0); // metallic
        assert_eq!(slots[2][1], 1.0); // roughness
        assert_eq!(slots[2][2], 1.0); // normal_scale
        assert_eq!(slots[2][3], 1.0); // occlusion_strength
    }

    #[test]
    fn overrides_land_on_the_right_component() {
        let mat = Material {
            base_color_factor: [0.2, 0.4, 0.6, 0.8],
            metallic_factor: 0.25,
            roughness_factor: 0.75,
            normal_scale: 0.5,
            occlusion_strength: 0.9,
        };
        let mut slots = [[0.0f32; 4]; 4];
        mat.pack_into(&mut slots);

        assert_eq!(slots[0], [0.2, 0.4, 0.6, 0.8]);
        assert_eq!(slots[2], [0.25, 0.75, 0.5, 0.9]);
        // 未使用的槽位保持 0，不会误写
        assert_eq!(slots[1], [0.0; 4]);
    }

    #[test]
    fn wgsl_is_generated_from_the_same_declaration() {
        let wgsl = Material::WGSL_STRUCT;
        assert!(wgsl.starts_with("struct MaterialUniform {"));
        assert!(wgsl.contains("    base_color_factor: vec4<f32>,\n"));
        assert!(wgsl.contains("    metallic_roughness_normal_occlusion: vec4<f32>,\n"));
        assert!(wgsl.ends_with("};\n"));
    }

    #[test]
    fn adding_a_param_is_one_line() {
        let mut slots = [[0.0f32; 4]; 4];
        let mat = MaterialV2 {
            clearcoat: 1.0,
            clearcoat_roughness: 0.3,
            ..MaterialV2::default()
        };
        mat.pack_into(&mut slots);

        // 新参数只落在第 3 个槽位，原有槽位布局不变
        assert_eq!(slots[3], [1.0, 0.3, 0.0, 0.0]);
        assert_eq!(slots[2], [1.0, 1.0, 1.0, 1.0]);
        assert!(MaterialV2::WGSL_STRUCT.contains(
            "    clearcoat_clearcoat_roughness_pad: vec4<f32>,\n"
        ));
    }

    /// 打印生成产物，便于人工核对（`cargo test -- --nocapture`）。
    #[test]
    fn print_generated_wgsl() {
        println!("{}", Material::WGSL_STRUCT);
        println!("{}", MaterialV2::WGSL_STRUCT);
    }
}
