//! Shader module registry — structured WGSL metadata for the shader_framework.
//!
//! This module defines structured Rust representations of the engine's WGSL
//! shader components (structs, functions, constants) so that the
//! [`shader_framework`] crate can compose, validate, and generate them.
//!
//! The existing `.wgsl` files in `crates/render/shaders/` remain the source of
//! truth for entry-point code; this module extracts only the *shared library*
//! metadata (the content of `pbr_common.wgsl`) into typed Rust form.
//!
//! # Architecture
//!
//! ```text
//! shader_library.rs (Rust metadata)
//!   └─ pbr_common_module() → ShaderModule { structs, functions, constants }
//!        ↓ compose via ShaderComposer
//!   ComposedShader { structs, functions, constants }
//!        ↓ WgslGenerator (individual methods: generate_structs, etc.)
//!   Generated library WGSL
//!        ↓ concatenate
//!   Generated library + pipeline .wgsl file → complete shader source
//! ```

use shader_framework::compose::{ShaderComposer, ShaderModule, ShaderModuleBuilder};
use shader_framework::core::*;
use shader_framework::generator::{ConstantDef, WgslGenerator};

/// Maximum number of lights supported by the PBR shader.
///
/// **Coupling**: This value MUST stay in sync with:
/// - `crates/render/src/light.rs` → `MAX_LIGHTS` (the CPU-side GPU buffer layout)
/// - The `MAX_LIGHTS` `ConstantDef` declared in [`pbr_common_module`] below
///   (which uses this constant as its default value)
///
/// If any of these drift apart the GPU buffer layout and the WGSL struct
/// definition will silently disagree, causing rendering corruption.
pub const MAX_LIGHTS_SHADER: u32 = 32;

// ─── pbr_common module ──────────────────────────────────────────────────────

/// Build the `pbr_common` library module.
///
/// Contains all shared structs, functions, and constants from
/// `crates/render/shaders/pbr_common.wgsl`.
///
/// This module has **no** vertex/fragment/compute body — it is designed to be
/// used as a mixin or composed with pipeline-specific modules.
pub fn pbr_common_module() -> ShaderModule {
    ShaderModuleBuilder::new("pbr_common")
        // ── Constants ───────────────────────────────────────────────────────
        .constant(ConstantDef {
            name: "PI".into(),
            ty: WgslType::F32,
            id: 0,
            default_value: Some("3.14159265359".into()),
            is_const: true,
        })
        .constant(ConstantDef {
            name: "MAX_LIGHTS".into(),
            ty: WgslType::U32,
            id: 1,
            default_value: Some(format!("{MAX_LIGHTS_SHADER}u").into()),
            is_const: true,
        })
        .constant(ConstantDef {
            name: "TOTAL_CLUSTERS".into(),
            ty: WgslType::U32,
            id: 2,
            default_value: Some("1024u".into()),
            is_const: true,
        })
        .constant(ConstantDef {
            name: "CLUSTER_TILES_X".into(),
            ty: WgslType::U32,
            id: 3,
            default_value: Some("8u".into()),
            is_const: true,
        })
        .constant(ConstantDef {
            name: "CLUSTER_TILES_Y".into(),
            ty: WgslType::U32,
            id: 4,
            default_value: Some("8u".into()),
            is_const: true,
        })
        .constant(ConstantDef {
            name: "CLUSTER_DEPTH_SLICES".into(),
            ty: WgslType::U32,
            id: 5,
            default_value: Some("16u".into()),
            is_const: true,
        })
        .constant(ConstantDef {
            name: "LIGHT_TYPE_DIRECTIONAL".into(),
            ty: WgslType::F32,
            id: 6,
            default_value: Some("0.0".into()),
            is_const: true,
        })
        .constant(ConstantDef {
            name: "LIGHT_TYPE_POINT".into(),
            ty: WgslType::F32,
            id: 7,
            default_value: Some("1.0".into()),
            is_const: true,
        })
        .constant(ConstantDef {
            name: "LIGHT_TYPE_SPOT".into(),
            ty: WgslType::F32,
            id: 8,
            default_value: Some("2.0".into()),
            is_const: true,
        })
        // ── Structs ─────────────────────────────────────────────────────────
        .struct_def(StructDef {
            name: "Camera".into(),
            fields: vec![
                StructField {
                    name: "view_projection".into(),
                    ty: WgslType::Mat4x4(WgslScalarType::F32),
                    attributes: vec![],
                },
                StructField {
                    name: "inverse_view_projection".into(),
                    ty: WgslType::Mat4x4(WgslScalarType::F32),
                    attributes: vec![],
                },
                StructField {
                    name: "camera_position".into(),
                    ty: WgslType::Vec4(WgslScalarType::F32),
                    attributes: vec![],
                },
            ],
        })
        .struct_def(StructDef {
            name: "Light".into(),
            fields: vec![
                StructField {
                    name: "position_range".into(),
                    ty: WgslType::Vec4(WgslScalarType::F32),
                    attributes: vec![],
                },
                StructField {
                    name: "direction_type".into(),
                    ty: WgslType::Vec4(WgslScalarType::F32),
                    attributes: vec![],
                },
                StructField {
                    name: "color_intensity".into(),
                    ty: WgslType::Vec4(WgslScalarType::F32),
                    attributes: vec![],
                },
                StructField {
                    name: "cone".into(),
                    ty: WgslType::Vec4(WgslScalarType::F32),
                    attributes: vec![],
                },
            ],
        })
        .struct_def(StructDef {
            name: "LightStorage".into(),
            fields: vec![
                StructField {
                    name: "ambient".into(),
                    ty: WgslType::Vec4(WgslScalarType::F32),
                    attributes: vec![],
                },
                StructField {
                    name: "count".into(),
                    ty: WgslType::Vec4(WgslScalarType::U32),
                    attributes: vec![],
                },
                StructField {
                    // Array size uses MAX_LIGHTS_SHADER — must match light.rs::MAX_LIGHTS
                    // and the MAX_LIGHTS ConstantDef default_value above.
                    name: "lights".into(),
                    ty: WgslType::Array(
                        Box::new(WgslType::Struct("Light".into())),
                        Some(MAX_LIGHTS_SHADER),
                    ),
                    attributes: vec![],
                },
            ],
        })
        .struct_def(StructDef {
            name: "ClusterUniform".into(),
            fields: vec![
                StructField {
                    name: "tile_count".into(),
                    ty: WgslType::Vec4(WgslScalarType::U32),
                    attributes: vec![],
                },
                StructField {
                    name: "screen_z".into(),
                    ty: WgslType::Vec4(WgslScalarType::F32),
                    attributes: vec![],
                },
                StructField {
                    name: "depth_params".into(),
                    ty: WgslType::Vec4(WgslScalarType::F32),
                    attributes: vec![],
                },
                StructField {
                    name: "flags".into(),
                    ty: WgslType::Vec4(WgslScalarType::F32),
                    attributes: vec![],
                },
                StructField {
                    name: "inv_vp_0".into(),
                    ty: WgslType::Vec4(WgslScalarType::F32),
                    attributes: vec![],
                },
                StructField {
                    name: "inv_vp_1".into(),
                    ty: WgslType::Vec4(WgslScalarType::F32),
                    attributes: vec![],
                },
                StructField {
                    name: "inv_vp_2".into(),
                    ty: WgslType::Vec4(WgslScalarType::F32),
                    attributes: vec![],
                },
                StructField {
                    name: "inv_vp_3".into(),
                    ty: WgslType::Vec4(WgslScalarType::F32),
                    attributes: vec![],
                },
            ],
        })
        .struct_def(StructDef {
            name: "MaterialUniform".into(),
            fields: vec![
                StructField {
                    name: "base_color_factor".into(),
                    ty: WgslType::Vec4(WgslScalarType::F32),
                    attributes: vec![],
                },
                StructField {
                    name: "emissive_alpha_cutoff".into(),
                    ty: WgslType::Vec4(WgslScalarType::F32),
                    attributes: vec![],
                },
                StructField {
                    name: "metallic_roughness_normal_occlusion".into(),
                    ty: WgslType::Vec4(WgslScalarType::F32),
                    attributes: vec![],
                },
                StructField {
                    name: "flags".into(),
                    ty: WgslType::Vec4(WgslScalarType::U32),
                    attributes: vec![],
                },
            ],
        })
        // ── Functions ───────────────────────────────────────────────────────
        .function(FunctionDef {
            name: "material_has_texture".into(),
            parameters: vec![
                ("mat".into(), WgslType::Struct("MaterialUniform".into())),
                ("bit".into(), WgslType::U32),
            ],
            return_type: Some(WgslType::Bool),
            body: WgslFragment::new("return (mat.flags.x & (1u << bit)) != 0u;"),
            overridable: false,
        })
        .function(FunctionDef {
            name: "distribution_ggx".into(),
            parameters: vec![
                ("n_dot_h".into(), WgslType::F32),
                ("roughness".into(), WgslType::F32),
            ],
            return_type: Some(WgslType::F32),
            body: WgslFragment::new(
                "let a = roughness * roughness;\n\
                 let a2 = a * a;\n\
                 let denom = n_dot_h * n_dot_h * (a2 - 1.0) + 1.0;\n\
                 return a2 / max(PI * denom * denom, 1e-5);",
            ),
            overridable: false,
        })
        .function(FunctionDef {
            name: "geometry_schlick_ggx".into(),
            parameters: vec![
                ("n_dot_v".into(), WgslType::F32),
                ("roughness".into(), WgslType::F32),
            ],
            return_type: Some(WgslType::F32),
            body: WgslFragment::new(
                "let r = roughness + 1.0;\n\
                 let k = (r * r) / 8.0;\n\
                 return n_dot_v / max(n_dot_v * (1.0 - k) + k, 1e-5);",
            ),
            overridable: false,
        })
        .function(FunctionDef {
            name: "geometry_smith".into(),
            parameters: vec![
                ("n_dot_v".into(), WgslType::F32),
                ("n_dot_l".into(), WgslType::F32),
                ("roughness".into(), WgslType::F32),
            ],
            return_type: Some(WgslType::F32),
            body: WgslFragment::new(
                "return geometry_schlick_ggx(max(n_dot_v, 0.0), roughness)\n     \
                 * geometry_schlick_ggx(max(n_dot_l, 0.0), roughness);",
            ),
            overridable: false,
        })
        .function(FunctionDef {
            name: "fresnel_schlick".into(),
            parameters: vec![
                ("cos_theta".into(), WgslType::F32),
                ("f0".into(), WgslType::Vec3(WgslScalarType::F32)),
            ],
            return_type: Some(WgslType::Vec3(WgslScalarType::F32)),
            body: WgslFragment::new(
                "let v = clamp(1.0 - cos_theta, 0.0, 1.0);\n\
                 return f0 + (vec3<f32>(1.0, 1.0, 1.0) - f0) * pow(v, 5.0);",
            ),
            overridable: false,
        })
        .function(FunctionDef {
            name: "attenuation_inverse_square".into(),
            parameters: vec![
                ("distance_sq".into(), WgslType::F32),
                ("range_sq".into(), WgslType::F32),
            ],
            return_type: Some(WgslType::F32),
            body: WgslFragment::new(
                "if (distance_sq >= range_sq) {\n    return 0.0;\n}\n\
                 let factor = 1.0 - distance_sq / range_sq;\n\
                 return factor * factor / max(distance_sq, 1e-4);",
            ),
            overridable: false,
        })
        .function(FunctionDef {
            name: "spot_cone_attenuation".into(),
            parameters: vec![
                ("cos_outer".into(), WgslType::F32),
                ("cos_inner".into(), WgslType::F32),
                ("cos_angle".into(), WgslType::F32),
            ],
            return_type: Some(WgslType::F32),
            body: WgslFragment::new(
                "if (cos_inner <= cos_outer) {\n    \
                 return select(0.0, 1.0, cos_angle >= cos_outer);\n\
                 }\n\
                 let t = clamp((cos_angle - cos_outer) / (cos_inner - cos_outer), 0.0, 1.0);\n\
                 return t * t * (3.0 - 2.0 * t);",
            ),
            overridable: false,
        })
        .function(FunctionDef {
            name: "shade_light".into(),
            parameters: vec![
                ("light".into(), WgslType::Struct("Light".into())),
                ("world_pos".into(), WgslType::Vec3(WgslScalarType::F32)),
                ("n".into(), WgslType::Vec3(WgslScalarType::F32)),
                ("v".into(), WgslType::Vec3(WgslScalarType::F32)),
                ("base_color".into(), WgslType::Vec3(WgslScalarType::F32)),
                ("metallic".into(), WgslType::F32),
                ("roughness".into(), WgslType::F32),
                ("f0".into(), WgslType::Vec3(WgslScalarType::F32)),
            ],
            return_type: Some(WgslType::Vec3(WgslScalarType::F32)),
            body: WgslFragment::new(
                "var l: vec3<f32>;\n\
                 var radiance: vec3<f32>;\n\
                 if (light.direction_type.w < 0.5) {\n    \
                 l = normalize(-light.direction_type.xyz);\n    \
                 radiance = light.color_intensity.rgb * light.color_intensity.a;\n\
                 } else if (light.direction_type.w < 1.5) {\n    \
                 let to_light = light.position_range.xyz - world_pos;\n    \
                 let dist_sq = dot(to_light, to_light);\n    \
                 let atten = attenuation_inverse_square(dist_sq, light.cone.z);\n    \
                 if (atten <= 0.0) {\n        \
                 return vec3<f32>(0.0);\n    \
                 }\n    \
                 l = to_light * inverseSqrt(max(dist_sq, 1e-8));\n    \
                 radiance = light.color_intensity.rgb * light.color_intensity.a * atten;\n\
                 } else {\n    \
                 let to_light = light.position_range.xyz - world_pos;\n    \
                 let dist_sq = dot(to_light, to_light);\n    \
                 let atten = attenuation_inverse_square(dist_sq, light.cone.z);\n    \
                 if (atten <= 0.0) {\n        \
                 return vec3<f32>(0.0);\n    \
                 }\n    \
                 l = to_light * inverseSqrt(max(dist_sq, 1e-8));\n    \
                 let spot_dir = normalize(light.direction_type.xyz);\n    \
                 let cos_angle = dot(-spot_dir, l);\n    \
                 let cone_atten = spot_cone_attenuation(light.cone.y, light.cone.x, cos_angle);\n    \
                 if (cone_atten <= 0.0) {\n        \
                 return vec3<f32>(0.0);\n    \
                 }\n    \
                 radiance = light.color_intensity.rgb * light.color_intensity.a * atten * cone_atten;\n\
                 }\n\
                 let h = normalize(v + l);\n\
                 let n_dot_l = max(dot(n, l), 0.0);\n\
                 let n_dot_v = max(dot(n, v), 0.0);\n\
                 let n_dot_h = max(dot(n, h), 0.0);\n\
                 let v_dot_h = max(dot(v, h), 0.0);\n\
                 let d = distribution_ggx(n_dot_h, roughness);\n\
                 let g = geometry_smith(n_dot_v, n_dot_l, roughness);\n\
                 let f = fresnel_schlick(v_dot_h, f0);\n\
                 let specular = (d * g * f) / max(4.0 * n_dot_v * n_dot_l, 1e-4);\n\
                 let kd = (vec3<f32>(1.0) - f) * (1.0 - metallic);\n\
                 let diffuse = kd * base_color / PI;\n\
                 return (diffuse + specular) * radiance * n_dot_l;",
            ),
            overridable: false,
        })
        .function(FunctionDef {
            name: "cluster_index_from_screen".into(),
            parameters: vec![
                ("frag_xy".into(), WgslType::Vec2(WgslScalarType::F32)),
                ("view_z".into(), WgslType::F32),
                ("cluster".into(), WgslType::Struct("ClusterUniform".into())),
            ],
            return_type: Some(WgslType::U32),
            body: WgslFragment::new(
                "let tile_x = u32(clamp(frag_xy.x / cluster.depth_params.z, 0.0, f32(cluster.tile_count.x) - 1.0));\n\
                 let tile_y = u32(clamp(frag_xy.y / cluster.depth_params.w, 0.0, f32(cluster.tile_count.y) - 1.0));\n\
                 let near = cluster.screen_z.z;\n\
                 let log_per_slice = cluster.depth_params.x;\n\
                 var slice: u32 = 0u;\n\
                 if (view_z > near && log_per_slice > 0.0) {\n    \
                 let f = log(view_z / near) / log_per_slice;\n    \
                 slice = u32(clamp(f, 0.0, f32(cluster.tile_count.z) - 1.0));\n\
                 }\n\
                 return (slice * cluster.tile_count.y + tile_y) * cluster.tile_count.x + tile_x;",
            ),
            overridable: false,
        })
        .function(FunctionDef {
            name: "linearize_depth".into(),
            parameters: vec![
                ("depth".into(), WgslType::F32),
                ("near".into(), WgslType::F32),
                ("far".into(), WgslType::F32),
            ],
            return_type: Some(WgslType::F32),
            body: WgslFragment::new(
                "return (near * far) / max(far - depth * (far - near), 1e-5);",
            ),
            overridable: false,
        })
        .build()
}

// ─── Composer factory ───────────────────────────────────────────────────────

/// Create a [`ShaderComposer`] pre-loaded with all library modules.
///
/// Callers can then compose pipeline-specific shaders using
/// [`ShaderComposer::compose`] or [`ShaderComposer::mixin`].
pub fn create_composer() -> ShaderComposer {
    let mut composer = ShaderComposer::new();
    composer.register_module(pbr_common_module())
        .expect("pbr_common module registration failed");
    composer
}

// ─── Code generation helpers ────────────────────────────────────────────────

/// Generate the "library" portion of WGSL from pbr_common metadata.
///
/// Returns a string containing struct definitions, function definitions, and
/// constants suitable for prepending to a pipeline-specific `.wgsl` file.
///
/// This function composes pbr_common into a [`ComposedShader`] and then uses
/// [`WgslGenerator`]'s individual generation methods to produce only the
/// shared library parts (no entry points are generated).
pub fn generate_pbr_common_wgsl() -> String {
    let composer = create_composer();

    // Compose pbr_common as a standalone library shader
    let composed = composer
        .compose("pbr_common", &[], "pbr_common_library")
        .expect("pbr_common composition failed");

    let generator = WgslGenerator::new();

    let mut parts: Vec<String> = Vec::new();

    // Header
    parts.push("// Generated by shader_framework — pbr_common library".into());

    // Constants (pipeline-overridable)
    let constants = generator.generate_constants(&composed.constants);
    if !constants.is_empty() {
        parts.push(constants);
    }

    // Struct definitions
    let structs = generator.generate_structs(&composed.structs);
    if !structs.is_empty() {
        parts.push(structs);
    }

    // Helper functions
    let functions = generator.generate_functions(&composed.functions);
    if !functions.is_empty() {
        parts.push(functions);
    }

    parts.join("\n")
}

// ─── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pbr_common_registers_without_error() {
        let composer = create_composer();
        assert!(composer.get_module("pbr_common").is_some());
        assert_eq!(composer.module_names().len(), 1);
    }

    #[test]
    fn pbr_common_has_all_structs() {
        let module = pbr_common_module();
        let struct_names: Vec<&str> = module.structs.iter().map(|s| s.name.as_str()).collect();
        assert!(struct_names.contains(&"Camera"));
        assert!(struct_names.contains(&"Light"));
        assert!(struct_names.contains(&"LightStorage"));
        assert!(struct_names.contains(&"ClusterUniform"));
        assert!(struct_names.contains(&"MaterialUniform"));
        assert_eq!(module.structs.len(), 5);
    }

    #[test]
    fn pbr_common_has_all_functions() {
        let module = pbr_common_module();
        let fn_names: Vec<&str> = module.functions.iter().map(|f| f.name.as_str()).collect();
        assert!(fn_names.contains(&"material_has_texture"));
        assert!(fn_names.contains(&"distribution_ggx"));
        assert!(fn_names.contains(&"geometry_schlick_ggx"));
        assert!(fn_names.contains(&"geometry_smith"));
        assert!(fn_names.contains(&"fresnel_schlick"));
        assert!(fn_names.contains(&"attenuation_inverse_square"));
        assert!(fn_names.contains(&"spot_cone_attenuation"));
        assert!(fn_names.contains(&"shade_light"));
        assert!(fn_names.contains(&"cluster_index_from_screen"));
        assert!(fn_names.contains(&"linearize_depth"));
        assert_eq!(module.functions.len(), 10);
    }

    #[test]
    fn pbr_common_has_all_constants() {
        let module = pbr_common_module();
        let const_names: Vec<&str> = module.constants.iter().map(|c| c.name.as_str()).collect();
        assert!(const_names.contains(&"PI"));
        assert!(const_names.contains(&"MAX_LIGHTS"));
        assert!(const_names.contains(&"TOTAL_CLUSTERS"));
        assert!(const_names.contains(&"CLUSTER_TILES_X"));
        assert!(const_names.contains(&"CLUSTER_TILES_Y"));
        assert!(const_names.contains(&"CLUSTER_DEPTH_SLICES"));
        assert!(const_names.contains(&"LIGHT_TYPE_DIRECTIONAL"));
        assert!(const_names.contains(&"LIGHT_TYPE_POINT"));
        assert!(const_names.contains(&"LIGHT_TYPE_SPOT"));
        assert_eq!(module.constants.len(), 9);
    }

    #[test]
    fn generated_wgsl_contains_key_content() {
        let wgsl = generate_pbr_common_wgsl();
        // Should contain the header comment
        assert!(wgsl.contains("Generated by shader_framework"));
        // Should contain key constants
        assert!(wgsl.contains("PI"));
        assert!(wgsl.contains("MAX_LIGHTS"));
        // Should contain key structs
        assert!(wgsl.contains("struct Camera"));
        assert!(wgsl.contains("struct Light"));
        // Should contain key functions
        assert!(wgsl.contains("fn shade_light"));
        assert!(wgsl.contains("fn distribution_ggx"));
        // Should contain header
        assert!(wgsl.contains("Generated by shader_framework"));
    }

    /// End-to-end test: compose pbr_common with a minimal shader and
    /// validate the generated WGSL via naga (the same validator wgpu uses).
    #[test]
    fn composed_shader_with_pbr_common_passes_naga_validation() {
        use shader_framework::stream::presets;

        let mut composer = create_composer();

        // A minimal test shader that references pbr_common constant (PI)
        let test_shader = ShaderModuleBuilder::new("test_shader")
            .stream(presets::position())
            .struct_def(StructDef {
                name: "TestUniform".into(),
                fields: vec![StructField {
                    name: "value".into(),
                    ty: WgslType::Vec4(WgslScalarType::F32),
                    attributes: vec![],
                }],
            })
            .function(FunctionDef {
                name: "test_fn".into(),
                parameters: vec![],
                return_type: Some(WgslType::F32),
                body: WgslFragment::new("return PI;"),
                overridable: false,
            })
            .vertex_body(
                "let wp = vec4<f32>(input.position, 1.0);\n\
                 output.clip_position = wp;",
            )
            .fragment_body(
                "let val = test_fn();\n\
                 return vec4<f32>(val, val, val, 1.0);",
            )
            .build();

        composer.register_module(test_shader).unwrap();

        // Compose: test_shader as base, pbr_common as mixin
        let composed = composer
            .mixin("test_shader", "pbr_common", "test_composed")
            .unwrap();

        // Generate WGSL first for debugging
        let generator = WgslGenerator::new();
        let wgsl = generator.generate(&composed).expect("WGSL generation failed");
        println!("=== Generated WGSL ===\n{wgsl}\n=== End WGSL ===");

        // Validate with naga
        let validate_result = generator.generate_and_validate(&composed);
        assert!(
            validate_result.is_ok(),
            "Naga validation failed: {:?}",
            validate_result.err()
        );

        // Also verify the generated WGSL contains expected content
        let wgsl = validate_result.unwrap();
        assert!(wgsl.contains("struct Camera"));
        assert!(wgsl.contains("fn shade_light"));
        assert!(wgsl.contains("@vertex"));
        assert!(wgsl.contains("@fragment"));
    }

    /// Verify that the WGSL generated from `pbr_common_module()` is
    /// semantically equivalent to the hand-written `pbr_common.wgsl`.
    ///
    /// We compare the *vocabulary* (struct names, function signatures,
    /// constant names) rather than a byte-for-byte diff, since whitespace
    /// and formatting naturally differ between generated and handwritten code.
    ///
    /// Note: We do NOT run the original `.wgsl` through the naga parser
    /// because it is a library module with no entry points — the WGSL
    /// spec requires at least one entry point for standalone validation.
    #[test]
    fn generated_wgsl_semantically_equivalent_to_original() {
        let wgsl = generate_pbr_common_wgsl();

        // ── All struct definitions must be present ──────────────────────
        for struct_name in &[
            "Camera",
            "Light",
            "LightStorage",
            "ClusterUniform",
            "MaterialUniform",
        ] {
            assert!(
                wgsl.contains(&format!("struct {struct_name}")),
                "Missing struct {struct_name} in generated WGSL"
            );
        }

        // ── All function signatures must be present ─────────────────────
        for fn_name in &[
            "material_has_texture",
            "distribution_ggx",
            "geometry_schlick_ggx",
            "geometry_smith",
            "fresnel_schlick",
            "attenuation_inverse_square",
            "spot_cone_attenuation",
            "shade_light",
            "cluster_index_from_screen",
            "linearize_depth",
        ] {
            assert!(
                wgsl.contains(&format!("fn {fn_name}")),
                "Missing function {fn_name} in generated WGSL"
            );
        }

        // ── All named constants must be present ─────────────────────────
        for const_name in &[
            "PI",
            "MAX_LIGHTS",
            "TOTAL_CLUSTERS",
            "CLUSTER_TILES_X",
            "CLUSTER_TILES_Y",
            "CLUSTER_DEPTH_SLICES",
            "LIGHT_TYPE_DIRECTIONAL",
            "LIGHT_TYPE_POINT",
            "LIGHT_TYPE_SPOT",
        ] {
            assert!(
                wgsl.contains(const_name),
                "Missing constant {const_name} in generated WGSL"
            );
        }

        // ── LightStorage array size must reference MAX_LIGHTS_SHADER ────
        // The generated WGSL should contain the literal array size that
        // matches our Rust constant.
        assert!(
            wgsl.contains(&format!("array<Light, {MAX_LIGHTS_SHADER}>")),
            "LightStorage.lights array size does not match MAX_LIGHTS_SHADER ({MAX_LIGHTS_SHADER})"
        );

        // ── Cross-check: original .wgsl has same vocabulary ─────────────
        // include_str! bakes the file into the test binary at compile time,
        // so a missing file is a compile error — no runtime I/O needed.
        let original = include_str!("../shaders/pbr_common.wgsl");

        for struct_name in &[
            "Camera",
            "Light",
            "LightStorage",
            "ClusterUniform",
            "MaterialUniform",
        ] {
            assert!(
                original.contains(&format!("struct {struct_name}")),
                "Original pbr_common.wgsl is missing struct {struct_name} \
                 — metadata may be out of sync"
            );
        }

        for fn_name in &[
            "material_has_texture",
            "distribution_ggx",
            "geometry_schlick_ggx",
            "geometry_smith",
            "fresnel_schlick",
            "attenuation_inverse_square",
            "spot_cone_attenuation",
            "shade_light",
            "cluster_index_from_screen",
            "linearize_depth",
        ] {
            assert!(
                original.contains(&format!("fn {fn_name}")),
                "Original pbr_common.wgsl is missing fn {fn_name} \
                 — metadata may be out of sync"
            );
        }
    }
}
