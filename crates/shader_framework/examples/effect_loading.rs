//! Effect loading example.
//!
//! Demonstrates the SDFX-equivalent effect system:
//! - Define render modules
//! - Load an effect from TOML
//! - Compile the effect into WGSL passes
//! - Use the EffectBuilder API for programmatic construction

use shader_framework::compose::*;
use shader_framework::core::*;
use shader_framework::effect::*;
use shader_framework::stream::presets;

fn main() {
    println!("=== Effect Loading Example ===\n");

    // ── Set up composer with modules ──────────────────────────────────────
    let mut composer = ShaderComposer::new();

    // Forward+ PBR shader (inspired by forward_plus.wgsl)
    let forward_plus = ShaderModuleBuilder::new("ForwardPlus")
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
            name: "pbr_sampler".into(),
            resource_type: BindingResourceType::Sampler(SamplerType::Filtering),
            visibility: wgpu::ShaderStages::FRAGMENT,
        })
        .function(FunctionDef {
            name: "compute_pbr_lighting".into(),
            parameters: vec![
                ("n".into(), WgslType::Vec3(WgslScalarType::F32)),
                ("albedo".into(), WgslType::Vec3(WgslScalarType::F32)),
            ],
            return_type: Some(WgslType::Vec3(WgslScalarType::F32)),
            body: WgslFragment::new(
                "let ndotl = max(dot(n, vec3<f32>(0.0, 1.0, 0.0)), 0.0);\n\
                 return albedo * ndotl;",
            ),
            overridable: false,
        })
        .vertex_body(
            "output.clip_position = vec4<f32>(input.position, 1.0);\n\
             output.normal = input.normal;\n\
             output.uv0 = input.uv0;",
        )
        .fragment_body(
            "let albedo = textureSample(albedo_tex, pbr_sampler, input.uv0).rgb;\n\
             let color = compute_pbr_lighting(input.normal, albedo);\n\
             return vec4<f32>(color, 1.0);",
        )
        .build();
    composer.register_module(forward_plus).unwrap();

    // Shadow depth shader (inspired by shadow_depth.wgsl)
    let shadow_depth = ShaderModuleBuilder::new("ShadowDepth")
        .stream(presets::position())
        .vertex_body("output.clip_position = vec4<f32>(input.position, 1.0);")
        .fragment_body("return vec4<f32>(1.0);")
        .build();
    composer.register_module(shadow_depth).unwrap();

    // Deferred lighting fullscreen pass
    let deferred_lighting = ShaderModuleBuilder::new("DeferredLighting")
        .stream(presets::position())
        .stream(presets::uv(0))
        .binding(ShaderBinding {
            group: 0,
            binding: 0,
            name: "gbuffer_albedo".into(),
            resource_type: BindingResourceType::Texture {
                dimension: TextureDimension::D2,
                sample_type: TextureSampleType::Float,
            },
            visibility: wgpu::ShaderStages::FRAGMENT,
        })
        .binding(ShaderBinding {
            group: 0,
            binding: 1,
            name: "gbuffer_normal".into(),
            resource_type: BindingResourceType::Texture {
                dimension: TextureDimension::D2,
                sample_type: TextureSampleType::Float,
            },
            visibility: wgpu::ShaderStages::FRAGMENT,
        })
        .binding(ShaderBinding {
            group: 0,
            binding: 2,
            name: "gbuffer_sampler".into(),
            resource_type: BindingResourceType::Sampler(SamplerType::Filtering),
            visibility: wgpu::ShaderStages::FRAGMENT,
        })
        .vertex_body(
            "output.clip_position = vec4<f32>(input.position, 1.0);\n\
             output.uv0 = input.uv0;",
        )
        .fragment_body(
            "let albedo = textureSample(gbuffer_albedo, gbuffer_sampler, input.uv0).rgb;\n\
             let normal = textureSample(gbuffer_normal, gbuffer_sampler, input.uv0).rgb;\n\
             let lit = albedo * max(dot(normal, vec3<f32>(0.0, 1.0, 0.0)), 0.0);\n\
             return vec4<f32>(lit, 1.0);",
        )
        .build();
    composer.register_module(deferred_lighting).unwrap();

    // ── Part 1: Load effect from TOML ─────────────────────────────────────
    println!("--- Part 1: Loading effect from TOML ---\n");

    let toml_str = r#"
[effect]
name = "ForwardPipeline"
features = ["Lighting", "Shadows"]

[[effect.passes]]
name = "ForwardPass"
pass_type = "Geometry"
shader = "ForwardPlus"

[[effect.passes]]
name = "ShadowPass"
pass_type = "Shadow"
shader = "ShadowDepth"
"#;

    let effect = EffectLoader::load_from_str(toml_str).unwrap();
    println!("Loaded effect: {}", effect.name);
    println!("Passes: {}", effect.passes.len());
    println!("Features: {:?}", effect.features);

    // Validate and compile
    let compiler = EffectCompiler::new();
    compiler.validate_effect(&effect, &composer).unwrap();
    let compiled = compiler.compile(&effect, &composer).unwrap();

    for pass in &compiled.passes {
        let preview_len = 300.min(pass.wgsl_source.len());
        println!("\n=== {} ({:?}) ===", pass.name, pass.pass_type);
        println!("Enabled: {}", pass.enabled);
        println!("WGSL ({} bytes):", pass.wgsl_source.len());
        println!("{}...", &pass.wgsl_source[..preview_len]);
    }

    // ── Part 2: Build effect programmatically ─────────────────────────────
    println!("\n\n--- Part 2: Building effect programmatically ---\n");

    let effect2 = EffectBuilder::new("DeferredPipeline")
        .add_geometry_pass("GBufferPass", "ForwardPlus")
        .add_lighting_pass("LightingPass", "DeferredLighting")
        .add_shadow_pass("ShadowPass", "ShadowDepth")
        .add_feature(RenderFeature::Lighting)
        .add_feature(RenderFeature::Shadows)
        .add_feature(RenderFeature::PostProcess)
        .add_parameter("shadow_map_size", ParameterValue::Int(2048))
        .add_parameter("use_csm", ParameterValue::Bool(true))
        .build();

    println!("Built effect: {}", effect2.name);
    println!("Passes: {}", effect2.passes.len());
    println!("Features: {:?}", effect2.features);
    println!("Parameters: {}", effect2.parameters.len());

    let compiled2 = compiler.compile(&effect2, &composer).unwrap();
    for pass in &compiled2.passes {
        println!(
            "\n=== {} ({:?}) — {} bytes WGSL ===",
            pass.name,
            pass.pass_type,
            pass.wgsl_source.len()
        );
    }

    // ── Part 3: Serialize effect to TOML ──────────────────────────────────
    println!("\n\n--- Part 3: Serializing effect to TOML ---\n");

    let file = EffectFile { effect: effect2 };
    let serialized = toml::to_string_pretty(&file).unwrap();
    println!("{serialized}");

    // ── Part 4: Effect with compositions ──────────────────────────────────
    println!("--- Part 4: Effect with composition operations ---\n");

    // Register a skinning mixin
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

    let skinned_effect = RenderEffect {
        name: "SkinnedCharacter".into(),
        passes: vec![
            PassDef {
                name: "SkinnedForward".into(),
                pass_type: PassType::Geometry,
                shader: "ForwardPlus".into(),
                compositions: vec![CompositionDef {
                    op: "mixin".into(),
                    module: "Skinning".into(),
                    name: None,
                    target_fn: None,
                }],
                features: std::collections::HashMap::new(),
                enabled: true,
            },
            PassDef {
                name: "SkinnedShadow".into(),
                pass_type: PassType::Shadow,
                shader: "ShadowDepth".into(),
                compositions: vec![CompositionDef {
                    op: "mixin".into(),
                    module: "Skinning".into(),
                    name: None,
                    target_fn: None,
                }],
                features: std::collections::HashMap::new(),
                enabled: true,
            },
        ],
        features: vec![RenderFeature::SkinRendering],
        parameters: Vec::new(),
    };

    let compiled3 = compiler.compile(&skinned_effect, &composer).unwrap();
    for pass in &compiled3.passes {
        println!(
            "\n=== {} ({:?}) — {} bytes WGSL ===",
            pass.name,
            pass.pass_type,
            pass.wgsl_source.len()
        );
        // Show whether skin_matrix function is present
        let has_skin = pass.wgsl_source.contains("fn skin_matrix");
        println!("  Has skin_matrix: {has_skin}");
    }
}
