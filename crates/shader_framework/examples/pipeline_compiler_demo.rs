//! Demonstrates the pipeline compiler's execution plan feature.
//!
//! Run with:
//! ```sh
//! cargo run --example pipeline_compiler_demo
//! ```

use shader_framework::core::*;
use shader_framework::pipeline_compiler::*;
use shader_framework::reflect::ReflectedModule;
use std::collections::HashMap;

fn main() {
    // Build some mock modules for the demo
    let mut modules = HashMap::new();

    // A compute shader module (e.g. image blur)
    modules.insert(
        "blur".to_string(),
        make_demo_module(
            "blur",
            vec![EntryPointInfo {
                name: "cs_main".to_string(),
                stage: ShaderStage::Compute,
                workgroup_size: Some([16, 16, 1]),
                parameters: Vec::new(),
            }],
            vec![ShaderBinding {
                group: 0,
                binding: 0,
                name: "blur_params".to_string(),
                resource_type: BindingResourceType::UniformBuffer,
                visibility: wgpu::ShaderStages::all(),
            }],
            vec![],
        ),
    );

    // A render shader module (e.g. forward pass)
    modules.insert(
        "forward".to_string(),
        make_demo_module(
            "forward",
            vec![
                EntryPointInfo {
                    name: "vs_main".to_string(),
                    stage: ShaderStage::Vertex,
                    workgroup_size: None,
                    parameters: Vec::new(),
                },
                EntryPointInfo {
                    name: "fs_main".to_string(),
                    stage: ShaderStage::Fragment,
                    workgroup_size: None,
                    parameters: Vec::new(),
                },
            ],
            vec![ShaderBinding {
                group: 0,
                binding: 1,
                name: "camera".to_string(),
                resource_type: BindingResourceType::UniformBuffer,
                visibility: wgpu::ShaderStages::all(),
            }],
            vec![VertexAttributeInfo {
                name: "position".to_string(),
                location: 0,
                ty: WgslType::Vec3(WgslScalarType::F32),
                format: wgpu::VertexFormat::Float32x3,
                offset: 0,
                semantic: StreamSemantic::Position,
                step_mode: wgpu::VertexStepMode::Vertex,
            }],
        ),
    );

    // Declare a stage list: render pass first, compute pass second
    let stage_list = vec![vec![
        "forward.vs".to_string(),
        "forward.fs".to_string(),
        "blur.cs".to_string(),
    ]];

    println!("=== Pipeline Compiler Demo ===\n");

    // Compile the pipeline
    let pipeline = compile_pipeline(&stage_list, &modules).expect("pipeline compilation failed");

    println!("Compiled {} phases:", pipeline.phases.len());
    for (i, phase) in pipeline.phases.iter().enumerate() {
        match phase {
            ExecutionPhase::Compute {
                shader,
                entry_point,
                workgroup_size,
                ..
            } => {
                println!(
                    "  [{}] Compute: {} :: {} (workgroup {:?})",
                    i, shader, entry_point, workgroup_size
                );
            }
            ExecutionPhase::Render {
                vertex_shader,
                fragment_shader,
                ..
            } => {
                println!(
                    "  [{}] Render: vs={:?}, fs={:?}",
                    i, vertex_shader, fragment_shader
                );
            }
        }
    }

    // Build execution plan: ComputeThenRender reorders so compute runs first
    let plan = pipeline
        .build_execution_plan(ExecutionOrder::ComputeThenRender)
        .expect("failed to build execution plan");

    println!("\nExecution order (ComputeThenRender):");
    println!("  Sequence: {:?}", plan.execution_sequence());
    for (step, phase) in plan.ordered_phases().iter().enumerate() {
        match phase {
            ExecutionPhase::Compute { shader, .. } => {
                println!("  Step {}: Compute({})", step, shader);
            }
            ExecutionPhase::Render {
                vertex_shader,
                fragment_shader,
                ..
            } => {
                println!(
                    "  Step {}: Render(vs={:?}, fs={:?})",
                    step, vertex_shader, fragment_shader
                );
            }
        }
    }

    println!("\nMerged bindings: {} total", plan.pipeline.merged_bindings.len());
    for b in &plan.pipeline.merged_bindings {
        println!("  group={} binding={} name={}", b.group, b.binding, b.name);
    }

    if let Some(ref layout) = plan.pipeline.vertex_layout {
        println!(
            "\nUnified vertex layout: {} attributes, stride={}",
            layout.attributes.len(),
            layout.stride
        );
        for attr in &layout.attributes {
            println!(
                "  loc={} name={} ty={:?}",
                attr.location, attr.name, attr.ty
            );
        }
    }

    println!("\nDone.");
}

// ─── Helpers ────────────────────────────────────────────────────────────────

fn make_demo_module(
    name: &str,
    entry_points: Vec<EntryPointInfo>,
    bindings: Vec<ShaderBinding>,
    vertex_attributes: Vec<VertexAttributeInfo>,
) -> ReflectedModule {
    ReflectedModule {
        name: name.to_string(),
        entry_points,
        bindings,
        structs: Vec::new(),
        functions: Vec::new(),
        constants: Vec::new(),
        global_vars: Vec::new(),
        vertex_attributes,
        vertex_body: None,
        fragment_body: None,
        compute_body: None,
    }
}
