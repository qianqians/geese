//! Basic shader creation example.
//!
//! Demonstrates how to use the shader_framework to create a simple shader,
//! generate WGSL code, and validate it with naga.

use shader_framework::core::*;
use shader_framework::generator::*;
use shader_framework::stream::presets;

fn main() {
    println!("=== Basic Shader Creation Example ===\n");

    // ── Example 1: Simple unlit shader ─────────────────────────────────────
    println!("--- Example 1: Simple Unlit Shader ---");

    let mut builder = ComposedShaderBuilder::new("UnlitShader");
    builder
        .add_stream(presets::position())
        .unwrap()
        .add_stream(presets::uv(0))
        .unwrap()
        .add_binding(ShaderBinding {
            group: 0,
            binding: 0,
            name: "color_tex".into(),
            resource_type: BindingResourceType::Texture {
                dimension: TextureDimension::D2,
                sample_type: TextureSampleType::Float,
            },
            visibility: wgpu::ShaderStages::FRAGMENT,
        })
        .add_binding(ShaderBinding {
            group: 0,
            binding: 1,
            name: "tex_sampler".into(),
            resource_type: BindingResourceType::Sampler(SamplerType::Filtering),
            visibility: wgpu::ShaderStages::FRAGMENT,
        })
        .vertex_entry(
            "vs_main",
            WgslFragment::new(
                "output.clip_position = vec4<f32>(input.position, 1.0);\n\
                 output.uv0 = input.uv0;",
            ),
        )
        .fragment_entry(
            "fs_main",
            WgslFragment::new(
                "let color = textureSample(color_tex, tex_sampler, input.uv0);\n\
                 return color;",
            ),
        );

    let shader = builder.build();
    let generator = WgslGenerator::new();

    match generator.generate_and_validate(&shader) {
        Ok(wgsl) => {
            println!("Generated and validated WGSL:\n");
            println!("{wgsl}");
        }
        Err(e) => {
            eprintln!("Failed to generate/validate WGSL: {e}");
        }
    }

    // ── Example 2: Shader with helper function ─────────────────────────────
    println!("\n--- Example 2: Shader with Helper Function ---");

    let mut builder2 = ComposedShaderBuilder::new("LitShader");
    builder2
        .add_stream(presets::position())
        .unwrap()
        .add_stream(presets::normal())
        .unwrap()
        .add_function(FunctionDef {
            name: "compute_lambert".into(),
            parameters: vec![
                ("normal".into(), WgslType::Vec3(WgslScalarType::F32)),
                ("light_dir".into(), WgslType::Vec3(WgslScalarType::F32)),
            ],
            return_type: Some(WgslType::F32),
            body: WgslFragment::new("return max(dot(normal, light_dir), 0.0);"),
            overridable: false,
        })
        .vertex_entry(
            "vs_main",
            WgslFragment::new(
                "output.clip_position = vec4<f32>(input.position, 1.0);\n\
                 output.normal = input.normal;",
            ),
        )
        .fragment_entry(
            "fs_main",
            WgslFragment::new(
                "let light_dir = normalize(vec3<f32>(1.0, 1.0, 1.0));\n\
                 let lambert = compute_lambert(input.normal, light_dir);\n\
                 return vec4<f32>(vec3<f32>(lambert), 1.0);",
            ),
        );

    let shader2 = builder2.build();
    match generator.generate_and_validate(&shader2) {
        Ok(wgsl) => {
            println!("Generated and validated WGSL:\n");
            println!("{wgsl}");
        }
        Err(e) => {
            eprintln!("Failed: {e}");
        }
    }

    // ── Example 3: Compute shader ──────────────────────────────────────────
    println!("\n--- Example 3: Compute Shader ---");

    let mut builder3 = ComposedShaderBuilder::new("ParticleCompute");
    builder3
        .add_global_var(GlobalVarDef {
            name: "particle_count".into(),
            ty: WgslType::U32,
            address_space: AddressSpace::Private,
            init: Some("0u".into()),
        })
        .compute_entry(
            "cs_main",
            [64, 1, 1],
            WgslFragment::new("let idx = global_id.x;\nparticle_count = particle_count + 1u;"),
        );

    let shader3 = builder3.build();
    match generator.generate_and_validate(&shader3) {
        Ok(wgsl) => {
            println!("Generated and validated WGSL:\n");
            println!("{wgsl}");
        }
        Err(e) => {
            eprintln!("Failed: {e}");
        }
    }
}
