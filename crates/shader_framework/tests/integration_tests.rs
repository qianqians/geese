//! Integration tests for the shader_framework crate.
//!
//! These tests exercise the full pipeline: module creation → composition →
//! WGSL generation → naga validation.

use shader_framework::compose::*;
use shader_framework::core::*;
use shader_framework::effect::*;
use shader_framework::generator::*;
use shader_framework::stream::presets;
use shader_framework::variant::*;

// ─── Helpers ────────────────────────────────────────────────────────────────

/// Build a PBR-like base module inspired by the engine's forward_plus.wgsl.
fn make_pbr_base_module() -> ShaderModule {
    ShaderModuleBuilder::new("PBRBase")
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
        .overridable_function(
            "compute_lighting",
            vec![
                ("n".into(), WgslType::Vec3(WgslScalarType::F32)),
                ("albedo".into(), WgslType::Vec3(WgslScalarType::F32)),
            ],
            Some(WgslType::Vec3(WgslScalarType::F32)),
            "let ndotl = max(dot(n, vec3<f32>(0.0, 1.0, 0.0)), 0.0);\nreturn albedo * ndotl;",
        )
        .function(FunctionDef {
            name: "sample_material".into(),
            parameters: vec![("uv".into(), WgslType::Vec2(WgslScalarType::F32))],
            return_type: Some(WgslType::Vec3(WgslScalarType::F32)),
            body: WgslFragment::new(
                "let tex_color = textureSample(albedo_tex, pbr_sampler, uv).rgb;\nreturn tex_color;",
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
             let color = compute_lighting(input.normal, albedo);\n\
             return vec4<f32>(color, 1.0);",
        )
        .build()
}

/// Build a skinning mixin module inspired by forward_plus.wgsl skin_matrix.
fn make_skinning_mixin() -> ShaderModule {
    ShaderModuleBuilder::new("Skinning")
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
        .build()
}

/// Build a normal-map mixin.
fn make_normal_map_mixin() -> ShaderModule {
    ShaderModuleBuilder::new("NormalMap")
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
            name: "sample_normal".into(),
            parameters: vec![("uv".into(), WgslType::Vec2(WgslScalarType::F32))],
            return_type: Some(WgslType::Vec3(WgslScalarType::F32)),
            body: WgslFragment::new(
                "let n = textureSample(normal_tex, normal_sampler, uv).rgb;\n\
                 return normalize(n * 2.0 - vec3<f32>(1.0));",
            ),
            overridable: false,
        })
        .build()
}

/// Build an emissive mixin.
fn make_emissive_mixin() -> ShaderModule {
    ShaderModuleBuilder::new("Emissive")
        .binding(ShaderBinding {
            group: 1,
            binding: 2,
            name: "emissive_tex".into(),
            resource_type: BindingResourceType::Texture {
                dimension: TextureDimension::D2,
                sample_type: TextureSampleType::Float,
            },
            visibility: wgpu::ShaderStages::FRAGMENT,
        })
        .function(FunctionDef {
            name: "sample_emissive".into(),
            parameters: vec![("uv".into(), WgslType::Vec2(WgslScalarType::F32))],
            return_type: Some(WgslType::Vec3(WgslScalarType::F32)),
            body: WgslFragment::new(
                "return textureSample(emissive_tex, pbr_sampler, uv).rgb;",
            ),
            overridable: false,
        })
        .build()
}

/// Build a simple depth-only shadow module inspired by shadow_depth.wgsl.
fn make_shadow_depth_module() -> ShaderModule {
    ShaderModuleBuilder::new("ShadowDepth")
        .stream(presets::position())
        .vertex_body("output.clip_position = vec4<f32>(input.position, 1.0);")
        .fragment_body("return vec4<f32>(1.0);")
        .build()
}

// ─── Test: PBR + Skinning end-to-end ────────────────────────────────────────

/// Build a PBR shader from scratch, compose with skinning, generate WGSL,
/// validate with naga.
#[test]
fn test_pbr_skinning_end_to_end() {
    let mut composer = ShaderComposer::new();
    composer.register_module(make_pbr_base_module()).unwrap();
    composer.register_module(make_skinning_mixin()).unwrap();

    // Compose: PBR + Skinning
    let composed = composer
        .mixin("PBRBase", "Skinning", "CharacterShader")
        .unwrap();

    // Verify streams: position, normal, uv0, bone_weights, bone_indices
    assert_eq!(composed.streams.streams().len(), 5);
    assert!(composed.streams.get_by_semantic(&StreamSemantic::Position).is_some());
    assert!(composed.streams.get_by_semantic(&StreamSemantic::Normal).is_some());
    assert!(composed.streams.get_by_semantic(&StreamSemantic::UV(0)).is_some());
    assert!(composed.streams.get_by_semantic(&StreamSemantic::BoneWeights).is_some());
    assert!(composed.streams.get_by_semantic(&StreamSemantic::BoneIndices).is_some());

    // Verify functions: compute_lighting, sample_material, skin_matrix
    let fn_names: Vec<&str> = composed.functions.iter().map(|f| f.name.as_str()).collect();
    assert!(fn_names.contains(&"compute_lighting"));
    assert!(fn_names.contains(&"sample_material"));
    assert!(fn_names.contains(&"skin_matrix"));

    // Verify bindings: albedo_tex, pbr_sampler
    assert_eq!(composed.bindings.len(), 2);

    // Generate WGSL
    let generator = WgslGenerator::new();
    let wgsl = generator.generate(&composed).unwrap();

    // Print for manual inspection
    println!("=== PBR + Skinning WGSL ===\n{wgsl}");

    // Verify WGSL content
    assert!(wgsl.contains("fn skin_matrix"), "Missing skin_matrix function");
    assert!(wgsl.contains("fn compute_lighting"), "Missing compute_lighting");
    assert!(wgsl.contains("fn sample_material"), "Missing sample_material");
    assert!(wgsl.contains("bone_weights"), "Missing bone-related streams");
    assert!(wgsl.contains("albedo_tex"), "Missing albedo texture binding");
    assert!(wgsl.contains("@vertex"), "Missing vertex entry point");
    assert!(wgsl.contains("@fragment"), "Missing fragment entry point");

    // Validate with naga
    let validated = generator.generate_and_validate(&composed);
    assert!(validated.is_ok(), "Naga validation failed: {:?}", validated.err());
}

// ─── Test: Deferred rendering effect ────────────────────────────────────────

/// Build a deferred rendering effect from TOML, compile all passes.
#[test]
fn test_deferred_rendering_effect() {
    let mut composer = ShaderComposer::new();
    composer.register_module(make_pbr_base_module()).unwrap();
    composer.register_module(make_shadow_depth_module()).unwrap();

    // Create a deferred lighting module
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

    let toml_str = r#"
[effect]
name = "DeferredPipeline"
features = ["Lighting", "Shadows"]

[[effect.passes]]
name = "GeometryPass"
pass_type = "Geometry"
shader = "PBRBase"
enabled = true

[[effect.passes]]
name = "LightingPass"
pass_type = "Lighting"
shader = "DeferredLighting"

[[effect.passes]]
name = "ShadowPass"
pass_type = "Shadow"
shader = "ShadowDepth"
"#;

    // Load effect
    let effect = EffectLoader::load_from_str(toml_str).unwrap();
    assert_eq!(effect.name, "DeferredPipeline");
    assert_eq!(effect.passes.len(), 3);
    assert_eq!(effect.features.len(), 2);

    // Validate effect
    let compiler = EffectCompiler::new();
    compiler.validate_effect(&effect, &composer).unwrap();

    // Compile effect
    let compiled = compiler.compile(&effect, &composer).unwrap();
    assert_eq!(compiled.passes.len(), 3);

    // Verify geometry pass
    assert_eq!(compiled.passes[0].name, "GeometryPass");
    assert_eq!(compiled.passes[0].pass_type, PassType::Geometry);
    assert!(compiled.passes[0].wgsl_source.contains("@vertex"));
    assert!(compiled.passes[0].wgsl_source.contains("@fragment"));
    assert!(compiled.passes[0].wgsl_source.contains("fn compute_lighting"));
    assert!(compiled.passes[0].enabled);

    // Verify lighting pass
    assert_eq!(compiled.passes[1].name, "LightingPass");
    assert_eq!(compiled.passes[1].pass_type, PassType::Lighting);
    assert!(compiled.passes[1].wgsl_source.contains("gbuffer_albedo"));

    // Verify shadow pass
    assert_eq!(compiled.passes[2].name, "ShadowPass");
    assert_eq!(compiled.passes[2].pass_type, PassType::Shadow);

    // Print all passes
    for pass in &compiled.passes {
        println!("\n=== {} ({:?}) ===\n{}", pass.name, pass.pass_type, &pass.wgsl_source);
    }
}

// ─── Test: Full variant workflow ────────────────────────────────────────────

/// Test the full variant management workflow: register features, enumerate,
/// generate, validate, check cache.
#[test]
fn test_variant_workflow() {
    let mut composer = ShaderComposer::new();
    composer.register_module(make_pbr_base_module()).unwrap();

    let mut mgr = VariantManager::new();
    let features = FeatureSetBuilder::new()
        .bool_feature("SKINNING")
        .bool_feature("HAS_UV")
        .bool_feature("NORMAL_MAP")
        .depends_on("HAS_UV", VariantValue::Bool(true))
        .enum_feature("LIGHTING_MODEL", vec!["Lambert", "CookTorrance"])
        .build();
    mgr.register("PBRBase", features).unwrap();

    // Enumerate all valid variants
    let variants = mgr.enumerate("PBRBase").unwrap();
    println!("Total variants: {}", variants.len());

    // SKINNING(2) × HAS_UV(2) × NORMAL_MAP(2, dep on HAS_UV) × LIGHTING_MODEL(2)
    // Valid combinations:
    //   HAS_UV=false → NORMAL_MAP must be false: 1 × 1 × 2 = 2 (per SKINNING)
    //   HAS_UV=true → NORMAL_MAP can be false/true: 1 × 2 × 2 = 4 (per SKINNING)
    // Total per SKINNING value: 2 + 4 = 6
    // Total: 2 × 6 = 12
    assert_eq!(variants.len(), 12);

    // Generate each variant and verify WGSL is generated
    for key in &variants {
        let variant = mgr.get_or_create("PBRBase", key, &composer).unwrap();
        assert!(!variant.wgsl_source.is_empty());
        assert!(
            variant.wgsl_source.contains("@id("),
            "Missing override declarations for variant {}",
            key
        );
        println!("Variant {}: {} bytes", key, variant.wgsl_source.len());
    }

    // Check cache stats: all should be misses
    let stats = mgr.cache_stats();
    assert_eq!(stats.misses, variants.len() as u64);
    assert_eq!(stats.hits, 0);

    // Re-generate first variant — should be a cache hit
    let _ = mgr.get_or_create("PBRBase", &variants[0], &composer).unwrap();
    let stats = mgr.cache_stats();
    assert_eq!(stats.hits, 1);

    println!("Cache stats: {}", stats);
}

// ─── Test: Composition chain (multiple mixins) ──────────────────────────────

/// PBR base → + Skinning mixin → + NormalMap mixin → + Emissive mixin
#[test]
fn test_composition_chain() {
    let mut composer = ShaderComposer::new();
    composer.register_module(make_pbr_base_module()).unwrap();
    composer.register_module(make_skinning_mixin()).unwrap();
    composer.register_module(make_normal_map_mixin()).unwrap();
    composer.register_module(make_emissive_mixin()).unwrap();

    let composed = CompositionBuilder::new("PBRBase")
        .result_name("FullCharacterShader")
        .mixin("Skinning")
        .mixin("NormalMap")
        .mixin("Emissive")
        .build(&composer)
        .unwrap();

    // Verify all streams: position, normal, uv0, bone_weights, bone_indices, tangent
    assert_eq!(composed.streams.streams().len(), 6);
    assert!(composed.streams.get_by_semantic(&StreamSemantic::Tangent).is_some());

    // Verify all functions
    let fn_names: Vec<&str> = composed.functions.iter().map(|f| f.name.as_str()).collect();
    assert!(fn_names.contains(&"compute_lighting"));
    assert!(fn_names.contains(&"sample_material"));
    assert!(fn_names.contains(&"skin_matrix"));
    assert!(fn_names.contains(&"sample_normal"));
    assert!(fn_names.contains(&"sample_emissive"));

    // Verify all bindings: albedo_tex, pbr_sampler, normal_tex, normal_sampler, emissive_tex
    assert_eq!(composed.bindings.len(), 5);

    // Generate WGSL
    let generator = WgslGenerator::new();
    let wgsl = generator.generate(&composed).unwrap();

    assert!(wgsl.contains("fn skin_matrix"));
    assert!(wgsl.contains("fn sample_normal"));
    assert!(wgsl.contains("fn sample_emissive"));
    assert!(wgsl.contains("fn compute_lighting"));
    assert!(wgsl.contains("tangent"));

    // Validate with naga
    let validated = generator.generate_and_validate(&composed);
    assert!(
        validated.is_ok(),
        "Naga validation failed for composition chain: {:?}",
        validated.err()
    );

    println!("=== Composition Chain WGSL ===\n{wgsl}");
}

// ─── Test: Effect with compositions in passes ───────────────────────────────

/// Effect pass that applies a mixin to the base shader.
#[test]
fn test_effect_with_compositions() {
    let mut composer = ShaderComposer::new();
    composer.register_module(make_pbr_base_module()).unwrap();
    composer.register_module(make_skinning_mixin()).unwrap();

    let effect = RenderEffect {
        name: "SkinnedCharacter".into(),
        passes: vec![PassDef {
            name: "ForwardPass".into(),
            pass_type: PassType::Geometry,
            shader: "PBRBase".into(),
            compositions: vec![CompositionDef {
                op: "mixin".into(),
                module: "Skinning".into(),
                name: None,
                target_fn: None,
            }],
            features: std::collections::HashMap::new(),
            enabled: true,
        }],
        features: vec![RenderFeature::SkinRendering],
        parameters: Vec::new(),
    };

    let compiler = EffectCompiler::new();
    let compiled = compiler.compile(&effect, &composer).unwrap();

    assert_eq!(compiled.passes.len(), 1);
    let wgsl = &compiled.passes[0].wgsl_source;

    // Should contain both PBR and skinning code
    assert!(wgsl.contains("fn skin_matrix"), "Missing skin_matrix in composed effect");
    assert!(wgsl.contains("fn compute_lighting"), "Missing PBR lighting");
    assert!(wgsl.contains("bone_weights"), "Missing bone streams");

    println!("=== Effect with Compositions ===\n{wgsl}");
}

// ─── Test: Effect serialization roundtrip ───────────────────────────────────

/// Create effect → serialize to TOML → deserialize → verify equality.
#[test]
fn test_effect_toml_roundtrip() {
    let effect = EffectBuilder::new("RoundtripPipeline")
        .add_geometry_pass("GBuffer", "GBufferShader")
        .add_lighting_pass("DeferredLight", "DeferredLighting")
        .add_shadow_pass("Shadow", "ShadowDepth")
        .add_post_process_pass("Bloom", "BloomShader")
        .add_feature(RenderFeature::Lighting)
        .add_feature(RenderFeature::Shadows)
        .add_feature(RenderFeature::Custom("Volumetrics".into()))
        .add_parameter("quality", ParameterValue::String("high".into()))
        .add_parameter("max_lights", ParameterValue::Int(32))
        .add_parameter("bloom_intensity", ParameterValue::Float(0.5))
        .build();

    // Serialize to TOML
    let file = EffectFile {
        effect: effect.clone(),
    };
    let toml_str = toml::to_string_pretty(&file).unwrap();
    println!("=== Serialized TOML ===\n{toml_str}");

    // Deserialize
    let loaded = EffectLoader::load_from_str(&toml_str).unwrap();

    // Verify equality
    assert_eq!(loaded.name, effect.name);
    assert_eq!(loaded.passes.len(), effect.passes.len());
    assert_eq!(loaded.features.len(), effect.features.len());
    assert_eq!(loaded.parameters.len(), effect.parameters.len());

    for (a, b) in loaded.passes.iter().zip(effect.passes.iter()) {
        assert_eq!(a.name, b.name);
        assert_eq!(a.pass_type, b.pass_type);
        assert_eq!(a.shader, b.shader);
        assert_eq!(a.enabled, b.enabled);
    }

    for (a, b) in loaded.features.iter().zip(effect.features.iter()) {
        assert_eq!(a, b);
    }
}

// ─── Test: Variant with mutual exclusion ────────────────────────────────────

/// SKINNING and INSTANCING are mutually exclusive.
#[test]
fn test_variant_exclusion() {
    let mut mgr = VariantManager::new();
    let features = FeatureSetBuilder::new()
        .bool_feature("SKINNING")
        .bool_feature("INSTANCING")
        .exclusive_with("SKINNING")
        .bool_feature("NORMAL_MAP")
        .build();
    mgr.register("test_shader", features).unwrap();

    let variants = mgr.enumerate("test_shader").unwrap();

    // SKINNING and INSTANCING cannot both be true.
    // Without exclusion: 2^3 = 8
    // Excluded: SKINNING=true, INSTANCING=true, NORMAL_MAP=false and
    //           SKINNING=true, INSTANCING=true, NORMAL_MAP=true
    // So: 8 - 2 = 6
    assert_eq!(variants.len(), 6);

    for v in &variants {
        let skinning = v.is_enabled("SKINNING");
        let instancing = v.is_enabled("INSTANCING");
        assert!(
            !(skinning && instancing),
            "Found excluded combination SKINNING+INSTANCING in variant: {v}"
        );
    }

    // Verify validation rejects the excluded combination
    let bad_key = VariantKey::new()
        .set("SKINNING", VariantValue::Bool(true))
        .set("INSTANCING", VariantValue::Bool(true));
    let result = mgr.validate_key("test_shader", &bad_key);
    assert!(result.is_err(), "Should reject SKINNING+INSTANCING combination");

    println!("Valid variants:");
    for v in &variants {
        println!("  {v}");
    }
}

// ─── Test: Composition with function override in effect ─────────────────────

/// Effect that uses override composition to replace a function.
#[test]
fn test_effect_with_override() {
    let mut composer = ShaderComposer::new();
    composer.register_module(make_pbr_base_module()).unwrap();

    // Create a module that provides a replacement compute_lighting
    let toon_lighting = ShaderModuleBuilder::new("ToonLighting")
        .function(FunctionDef {
            name: "compute_lighting".into(),
            parameters: vec![
                ("n".into(), WgslType::Vec3(WgslScalarType::F32)),
                ("albedo".into(), WgslType::Vec3(WgslScalarType::F32)),
            ],
            return_type: Some(WgslType::Vec3(WgslScalarType::F32)),
            body: WgslFragment::new(
                "let ndotl = dot(n, vec3<f32>(0.0, 1.0, 0.0));\n\
                 let band = step(0.5, ndotl);\n\
                 return albedo * band;",
            ),
            overridable: false,
        })
        .build();
    composer.register_module(toon_lighting).unwrap();

    let effect = RenderEffect {
        name: "ToonEffect".into(),
        passes: vec![PassDef {
            name: "ToonPass".into(),
            pass_type: PassType::Geometry,
            shader: "PBRBase".into(),
            compositions: vec![CompositionDef {
                op: "override".into(),
                module: "ToonLighting".into(),
                name: None,
                target_fn: Some("compute_lighting".into()),
            }],
            features: std::collections::HashMap::new(),
            enabled: true,
        }],
        features: Vec::new(),
        parameters: Vec::new(),
    };

    let compiler = EffectCompiler::new();
    let compiled = compiler.compile(&effect, &composer).unwrap();
    let wgsl = &compiled.passes[0].wgsl_source;

    // Should contain the toon shading code (step function) instead of PBR
    assert!(wgsl.contains("step(0.5"), "Should contain toon lighting code");
    assert!(!wgsl.contains("max(dot"), "Should not contain original PBR lighting");

    println!("=== Toon Effect ===\n{wgsl}");
}

// ─── Test: Variant generation with composed shader ──────────────────────────

/// Test variant generation on top of a composed shader.
#[test]
fn test_variant_with_composition() {
    let mut composer = ShaderComposer::new();
    composer.register_module(make_pbr_base_module()).unwrap();
    composer.register_module(make_skinning_mixin()).unwrap();

    // First compose PBR + Skinning
    let composed = composer
        .mixin("PBRBase", "Skinning", "SkinnedPBR")
        .unwrap();

    // Register variant features
    let mut mgr = VariantManager::new();
    let features = FeatureSetBuilder::new()
        .bool_feature("SHADOW_CASTER")
        .bool_feature("RECEIVE_SHADOWS")
        .build();
    mgr.register("SkinnedPBR", features).unwrap();

    // Generate variant with both features enabled
    let key = VariantKey::new()
        .set("SHADOW_CASTER", VariantValue::Bool(true))
        .set("RECEIVE_SHADOWS", VariantValue::Bool(true));

    let variant = mgr
        .get_or_create_with_composition("SkinnedPBR", &composed, &key)
        .unwrap();

    assert!(variant.wgsl_source.contains("override"));
    assert!(variant.wgsl_source.contains("SHADOW_CASTER"));
    assert!(variant.wgsl_source.contains("RECEIVE_SHADOWS"));
    assert!(variant.wgsl_source.contains("fn skin_matrix"));

    println!("=== Variant with Composition ===\n{}", variant.wgsl_source);
}

// ─── Test: WGSL output structure validation ─────────────────────────────────

/// Verify the structural correctness of generated WGSL for a composed shader.
#[test]
fn test_wgsl_output_structure() {
    let mut composer = ShaderComposer::new();
    composer.register_module(make_pbr_base_module()).unwrap();
    composer.register_module(make_skinning_mixin()).unwrap();

    let composed = composer
        .mixin("PBRBase", "Skinning", "TestShader")
        .unwrap();

    let generator = WgslGenerator::new();
    let wgsl = generator.generate(&composed).unwrap();

    // 1. Header comment
    assert!(wgsl.starts_with("// Generated by shader_framework — TestShader"));

    // 2. Struct definitions section (VertexInput, VertexOutput, FragmentInput)
    assert!(wgsl.contains("struct VertexInput {"));
    assert!(wgsl.contains("struct VertexOutput {"));
    assert!(wgsl.contains("struct FragmentInput {"));

    // 3. Binding declarations
    assert!(wgsl.contains("@group(0) @binding(0)"));
    assert!(wgsl.contains("@group(0) @binding(1)"));

    // 4. Helper functions before entry points
    let fn_pos = wgsl.find("fn compute_lighting").unwrap();
    let vs_pos = wgsl.find("@vertex").unwrap();
    let fs_pos = wgsl.find("@fragment").unwrap();
    assert!(fn_pos < vs_pos, "Helper functions should come before entry points");
    assert!(vs_pos < fs_pos, "Vertex entry should come before fragment entry");

    // 5. VS I/O matching
    assert!(wgsl.contains("@builtin(position) clip_position: vec4<f32>"));

    // 6. Entry point signatures
    assert!(wgsl.contains("fn vs_main(input: VertexInput) -> VertexOutput"));
    assert!(wgsl.contains("fn fs_main(input: FragmentInput) -> @location(0) vec4<f32>"));
}

// ─── Test: Effect builder API comprehensive ─────────────────────────────────

/// Test the EffectBuilder API with all pass types.
#[test]
fn test_effect_builder_comprehensive() {
    let effect = EffectBuilder::new("FullPipeline")
        .add_geometry_pass("GBuffer", "GBufferShader")
        .add_lighting_pass("Lighting", "DeferredLighting")
        .add_shadow_pass("Shadow", "ShadowDepth")
        .add_post_process_pass("Bloom", "BloomShader")
        .add_compute_pass("Cull", "CullCompute")
        .add_feature(RenderFeature::Lighting)
        .add_feature(RenderFeature::Shadows)
        .add_feature(RenderFeature::PostProcess)
        .add_feature(RenderFeature::Custom("Volumetrics".into()))
        .add_parameter("quality", ParameterValue::String("ultra".into()))
        .add_parameter("shadow_resolution", ParameterValue::Int(2048))
        .build();

    assert_eq!(effect.name, "FullPipeline");
    assert_eq!(effect.passes.len(), 5);
    assert_eq!(effect.passes[0].pass_type, PassType::Geometry);
    assert_eq!(effect.passes[1].pass_type, PassType::Lighting);
    assert_eq!(effect.passes[2].pass_type, PassType::Shadow);
    assert_eq!(effect.passes[3].pass_type, PassType::PostProcess);
    assert_eq!(effect.passes[4].pass_type, PassType::Compute);
    assert_eq!(effect.features.len(), 4);
    assert_eq!(effect.parameters.len(), 2);
}

// ─── Test: JSON effect loading ──────────────────────────────────────────────

/// Test loading an effect from JSON format.
#[test]
fn test_effect_json_loading() {
    let json_str = r#"{
  "effect": {
    "name": "JsonPipeline",
    "passes": [
      {
        "name": "MainPass",
        "pass_type": "Geometry",
        "shader": "MainShader",
        "enabled": true
      },
      {
        "name": "ShadowPass",
        "pass_type": "Shadow",
        "shader": "ShadowShader",
        "enabled": false
      }
    ],
    "features": ["Lighting", "Shadows"],
    "parameters": [
      { "name": "intensity", "value": { "Float": 1.5 } }
    ]
  }
}"#;

    let effect = EffectLoader::load_from_json(json_str).unwrap();
    assert_eq!(effect.name, "JsonPipeline");
    assert_eq!(effect.passes.len(), 2);
    assert!(effect.passes[0].enabled);
    assert!(!effect.passes[1].enabled);
    assert!(matches!(
        effect.parameters[0].value,
        ParameterValue::Float(v) if (v - 1.5).abs() < f64::EPSILON
    ));
}

// ─── Test: Variant with enum features ───────────────────────────────────────

/// Test variant enumeration and generation with enum features.
#[test]
fn test_variant_enum_features() {
    let mut composer = ShaderComposer::new();
    composer.register_module(make_pbr_base_module()).unwrap();

    let mut mgr = VariantManager::new();
    let features = FeatureSetBuilder::new()
        .enum_feature("LIGHTING_MODEL", vec!["Lambert", "CookTorrance", "BlinnPhong"])
        .bool_feature("SKINNING")
        .build();
    mgr.register("PBRBase", features).unwrap();

    let variants = mgr.enumerate("PBRBase").unwrap();
    // 3 lighting models × 2 skinning = 6
    assert_eq!(variants.len(), 6);

    // Generate each variant
    for key in &variants {
        let variant = mgr.get_or_create("PBRBase", key, &composer).unwrap();
        assert!(!variant.wgsl_source.is_empty());
    }

    // Verify specific variant
    let key = VariantKey::new()
        .set("LIGHTING_MODEL", VariantValue::Enum("CookTorrance".into()))
        .set("SKINNING", VariantValue::Bool(true));
    let variant = mgr.get_or_create("PBRBase", &key, &composer).unwrap();
    assert!(variant.wgsl_source.contains("LIGHTING_MODEL"));
    assert!(variant.wgsl_source.contains("SKINNING"));
}

// ─── Test: End-to-end naga validation for all composed shaders ──────────────

/// Ensure that every composition pattern produces naga-valid WGSL.
#[test]
fn test_all_compositions_produce_valid_wgsl() {
    let mut composer = ShaderComposer::new();
    composer.register_module(make_pbr_base_module()).unwrap();
    composer.register_module(make_skinning_mixin()).unwrap();
    composer.register_module(make_normal_map_mixin()).unwrap();
    composer.register_module(make_emissive_mixin()).unwrap();
    composer.register_module(make_shadow_depth_module()).unwrap();

    let generator = WgslGenerator::new();

    // 1. Base only
    let base = composer.compose("PBRBase", &[], "BaseOnly").unwrap();
    let result = generator.generate_and_validate(&base);
    assert!(result.is_ok(), "Base-only validation failed: {:?}", result.err());

    // 2. Base + Skinning
    let skinned = composer
        .mixin("PBRBase", "Skinning", "Skinned")
        .unwrap();
    let result = generator.generate_and_validate(&skinned);
    assert!(result.is_ok(), "Skinned validation failed: {:?}", result.err());

    // 3. Base + NormalMap
    let normal_mapped = composer
        .mixin("PBRBase", "NormalMap", "NormalMapped")
        .unwrap();
    let result = generator.generate_and_validate(&normal_mapped);
    assert!(result.is_ok(), "NormalMapped validation failed: {:?}", result.err());

    // 4. Base + Skinning + NormalMap + Emissive
    let full = CompositionBuilder::new("PBRBase")
        .result_name("FullShader")
        .mixin("Skinning")
        .mixin("NormalMap")
        .mixin("Emissive")
        .build(&composer)
        .unwrap();
    let result = generator.generate_and_validate(&full);
    assert!(result.is_ok(), "Full composition validation failed: {:?}", result.err());

    // 5. Shadow depth
    let shadow = composer.compose("ShadowDepth", &[], "ShadowOnly").unwrap();
    let result = generator.generate_and_validate(&shadow);
    assert!(result.is_ok(), "Shadow validation failed: {:?}", result.err());
}

// ─── Test: Variant cache statistics ─────────────────────────────────────────

/// Verify cache statistics work correctly across multiple variant generations.
#[test]
fn test_variant_cache_statistics() {
    let mut composer = ShaderComposer::new();
    composer.register_module(make_pbr_base_module()).unwrap();

    let mut mgr = VariantManager::new();
    let features = FeatureSetBuilder::new()
        .bool_feature("A")
        .bool_feature("B")
        .build();
    mgr.register("PBRBase", features).unwrap();

    let k1 = VariantKey::new().set("A", VariantValue::Bool(false)).set("B", VariantValue::Bool(false));
    let k2 = VariantKey::new().set("A", VariantValue::Bool(true)).set("B", VariantValue::Bool(false));
    let k3 = VariantKey::new().set("A", VariantValue::Bool(false)).set("B", VariantValue::Bool(true));

    // Generate 3 unique variants — all misses
    mgr.get_or_create("PBRBase", &k1, &composer).unwrap();
    mgr.get_or_create("PBRBase", &k2, &composer).unwrap();
    mgr.get_or_create("PBRBase", &k3, &composer).unwrap();

    let stats = mgr.cache_stats();
    assert_eq!(stats.misses, 3);
    assert_eq!(stats.hits, 0);
    assert_eq!(stats.total_cached, 3);

    // Re-request k1 and k2 — 2 hits
    mgr.get_or_create("PBRBase", &k1, &composer).unwrap();
    mgr.get_or_create("PBRBase", &k2, &composer).unwrap();

    let stats = mgr.cache_stats();
    assert_eq!(stats.misses, 3);
    assert_eq!(stats.hits, 2);
    assert_eq!(stats.total_cached, 3);

    // Clear cache
    mgr.clear_cache();
    let stats = mgr.cache_stats();
    assert_eq!(stats.misses, 0);
    assert_eq!(stats.hits, 0);
    assert_eq!(stats.total_cached, 0);
}
