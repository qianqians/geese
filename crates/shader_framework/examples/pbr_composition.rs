//! PBR + Skinning composition example.
//!
//! Demonstrates the composition system similar to Stride's SDSL:
//! - Define a PBR base shader module
//! - Define a Skinning mixin module
//! - Define a NormalMap mixin module
//! - Compose them together
//! - Generate and validate the final WGSL

use shader_framework::compose::*;
use shader_framework::core::*;
use shader_framework::generator::*;
use shader_framework::stream::presets;

fn main() {
    println!("=== PBR + Skinning Composition Example ===\n");

    let mut composer = ShaderComposer::new();

    // ── Register PBR base module ──────────────────────────────────────────
    // Inspired by the engine's forward_plus.wgsl and pbr_common.wgsl.
    let pbr_base = ShaderModuleBuilder::new("PBRBase")
        .stream(presets::position())
        .stream(presets::normal())
        .stream(presets::uv(0))
        .binding(ShaderBinding {
            group: 0,
            binding: 0,
            name: "albedo_tex".into(),
            resource_type: BindingResourceType::Texture {
                dimension: TextureDimension::D2,
                sample_type: TextureSampleType::Float,
            },
            visibility: wgpu::ShaderStages::FRAGMENT,
        })
        .binding(ShaderBinding {
            group: 0,
            binding: 1,
            name: "metallic_roughness_tex".into(),
            resource_type: BindingResourceType::Texture {
                dimension: TextureDimension::D2,
                sample_type: TextureSampleType::Float,
            },
            visibility: wgpu::ShaderStages::FRAGMENT,
        })
        .binding(ShaderBinding {
            group: 0,
            binding: 2,
            name: "pbr_sampler".into(),
            resource_type: BindingResourceType::Sampler(SamplerType::Filtering),
            visibility: wgpu::ShaderStages::FRAGMENT,
        })
        // Overridable PBR lighting function
        .overridable_function(
            "compute_lighting",
            vec![
                ("n".into(), WgslType::Vec3(WgslScalarType::F32)),
                ("albedo".into(), WgslType::Vec3(WgslScalarType::F32)),
                ("metallic".into(), WgslType::F32),
                ("roughness".into(), WgslType::F32),
            ],
            Some(WgslType::Vec3(WgslScalarType::F32)),
            // Simplified PBR: Lambert diffuse + basic specular
            "let ndotl = max(dot(n, vec3<f32>(0.0, 1.0, 0.0)), 0.0);\n\
             let diffuse = albedo * ndotl;\n\
             let specular = vec3<f32>(pow(ndotl, mix(8.0, 128.0, 1.0 - roughness)));\n\
             return mix(diffuse, specular, metallic);",
        )
        // Material sampling function
        .function(FunctionDef {
            name: "sample_material".into(),
            parameters: vec![("uv".into(), WgslType::Vec2(WgslScalarType::F32))],
            return_type: Some(WgslType::Vec3(WgslScalarType::F32)),
            body: WgslFragment::new(
                "return textureSample(albedo_tex, pbr_sampler, uv).rgb;",
            ),
            overridable: false,
        })
        .function(FunctionDef {
            name: "sample_metallic_roughness".into(),
            parameters: vec![("uv".into(), WgslType::Vec2(WgslScalarType::F32))],
            return_type: Some(WgslType::Vec2(WgslScalarType::F32)),
            body: WgslFragment::new(
                // Blue channel = metallic, Green channel = roughness (glTF convention)
                "let mr = textureSample(metallic_roughness_tex, pbr_sampler, uv);\n\
                 return vec2<f32>(mr.z, mr.y);",
            ),
            overridable: false,
        })
        .vertex_body(
            "output.clip_position = vec4<f32>(input.position, 1.0);\n\
             output.normal = input.normal;\n\
             output.uv0 = input.uv0;",
        )
        .fragment_body(
            "let albedo = sample_material(input.uv0);\n\
             let mr = sample_metallic_roughness(input.uv0);\n\
             let metallic = mr.x;\n\
             let roughness = mr.y;\n\
             let color = compute_lighting(input.normal, albedo, metallic, roughness);\n\
             return vec4<f32>(color, 1.0);",
        )
        .build();
    composer.register_module(pbr_base).unwrap();

    // ── Register Skinning mixin ───────────────────────────────────────────
    // Inspired by forward_plus.wgsl skin_matrix function.
    let skinning = ShaderModuleBuilder::new("Skinning")
        .stream(presets::bone_weights())
        .stream(presets::bone_indices())
        .function(FunctionDef {
            name: "skin_matrix".into(),
            parameters: vec![
                ("weights".into(), WgslType::Vec4(WgslScalarType::F32)),
                ("indices".into(), WgslType::Vec4(WgslScalarType::U32)),
            ],
            return_type: Some(WgslType::Mat4x4(WgslScalarType::F32)),
            body: WgslFragment::new(
                "let identity = mat4x4<f32>(\n    \
                    vec4<f32>(1.0, 0.0, 0.0, 0.0),\n    \
                    vec4<f32>(0.0, 1.0, 0.0, 0.0),\n    \
                    vec4<f32>(0.0, 0.0, 1.0, 0.0),\n    \
                    vec4<f32>(0.0, 0.0, 0.0, 1.0));\n\
                 return identity * weights.x + identity * weights.y \
                 + identity * weights.z + identity * weights.w;",
            ),
            overridable: false,
        })
        .build();
    composer.register_module(skinning).unwrap();

    // ── Register NormalMap mixin ──────────────────────────────────────────
    let normal_map = ShaderModuleBuilder::new("NormalMap")
        .stream(presets::tangent())
        .binding(ShaderBinding {
            group: 1,
            binding: 0,
            name: "normal_tex".into(),
            resource_type: BindingResourceType::Texture {
                dimension: TextureDimension::D2,
                sample_type: TextureSampleType::Float,
            },
            visibility: wgpu::ShaderStages::FRAGMENT,
        })
        .binding(ShaderBinding {
            group: 1,
            binding: 1,
            name: "normal_sampler".into(),
            resource_type: BindingResourceType::Sampler(SamplerType::Filtering),
            visibility: wgpu::ShaderStages::FRAGMENT,
        })
        .function(FunctionDef {
            name: "perturb_normal".into(),
            parameters: vec![
                ("world_normal".into(), WgslType::Vec3(WgslScalarType::F32)),
                ("uv".into(), WgslType::Vec2(WgslScalarType::F32)),
            ],
            return_type: Some(WgslType::Vec3(WgslScalarType::F32)),
            body: WgslFragment::new(
                "let tangent_normal = textureSample(normal_tex, normal_sampler, uv).rgb * 2.0 - vec3<f32>(1.0);\n\
                 return normalize(world_normal + tangent_normal);",
            ),
            overridable: false,
        })
        .build();
    composer.register_module(normal_map).unwrap();

    // ── Compose: PBR + Skinning ───────────────────────────────────────────
    println!("--- Composing: PBR + Skinning ---");

    let character = CompositionBuilder::new("PBRBase")
        .result_name("CharacterShader")
        .mixin("Skinning")
        .build(&composer)
        .expect("Composition failed");

    let generator = WgslGenerator::new();
    let wgsl = generator
        .generate_and_validate(&character)
        .expect("WGSL generation failed");

    println!("=== Character Shader (PBR + Skinning) ===");
    println!("{wgsl}");

    // Print summary
    println!("\n--- Shader Summary ---");
    println!("Name: {}", character.name);
    println!("Streams: {}", character.streams.streams().len());
    for s in character.streams.streams() {
        println!(
            "  {:?}: {} (location {:?})",
            s.semantic,
            s.wgsl_type.to_wgsl(),
            s.location
        );
    }
    println!("Bindings: {}", character.bindings.len());
    for b in &character.bindings {
        println!("  @group({}) @binding({}) {}", b.group, b.binding, b.name);
    }
    println!("Functions: {}", character.functions.len());
    for f in &character.functions {
        println!("  fn {} (overridable: {})", f.name, f.overridable);
    }

    // ── Compose: PBR + Skinning + NormalMap ───────────────────────────────
    println!("\n\n--- Composing: PBR + Skinning + NormalMap ---");

    let full_character = CompositionBuilder::new("PBRBase")
        .result_name("FullCharacterShader")
        .mixin("Skinning")
        .mixin("NormalMap")
        .build(&composer)
        .expect("Full composition failed");

    let wgsl2 = generator
        .generate_and_validate(&full_character)
        .expect("Full WGSL generation failed");

    println!("=== Full Character Shader (PBR + Skinning + NormalMap) ===");
    println!("{wgsl2}");

    println!("\n--- Full Shader Summary ---");
    println!("Streams: {}", full_character.streams.streams().len());
    println!("Bindings: {}", full_character.bindings.len());
    println!("Functions: {}", full_character.functions.len());
}
