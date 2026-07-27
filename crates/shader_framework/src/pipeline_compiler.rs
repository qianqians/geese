//! Declarative pipeline compiler — splits stage lists into execution phases,
//! merges bindings, and builds unified vertex layouts.

use crate::core::*;
use crate::reflect::ReflectedModule;
use std::collections::HashMap;

// ─── Output Types ───────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct CompiledPipeline {
    pub phases: Vec<ExecutionPhase>,
    pub merged_bindings: Vec<ShaderBinding>,
    pub vertex_layout: Option<VertexLayoutDescriptor>,
    pub vertex_data_mapping: Option<VertexDataMapping>,
}

#[derive(Debug, Clone)]
pub enum ExecutionPhase {
    Compute {
        shader: String,
        entry_point: String,
        workgroup_size: [u32; 3],
        bindings: Vec<ShaderBinding>,
    },
    Render {
        vertex_shader: Option<String>,
        fragment_shader: Option<String>,
        vertex_entry: String,
        fragment_entry: String,
        bindings: Vec<ShaderBinding>,
        vertex_location_subset: Vec<u32>,
    },
}

impl ExecutionPhase {
    /// Returns all module names referenced in this phase.
    pub fn module_names(&self) -> Vec<&str> {
        match self {
            ExecutionPhase::Compute { shader, .. } => vec![shader.as_str()],
            ExecutionPhase::Render {
                vertex_shader,
                fragment_shader,
                ..
            } => {
                let mut names = Vec::new();
                if let Some(vs) = vertex_shader {
                    names.push(vs.as_str());
                }
                if let Some(fs) = fragment_shader {
                    names.push(fs.as_str());
                }
                names
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct VertexDataMapping {
    pub unified_layout: Option<VertexLayoutDescriptor>,
    pub function_subsets: HashMap<String, Vec<u32>>,
    pub semantic_sources: HashMap<StreamSemantic, String>,
}

// ─── Execution Plan ─────────────────────────────────────────────────────────

/// Describes the order in which phases should be executed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExecutionOrder {
    /// Execute phases in the order they appear (default).
    Sequential,
    /// Execute all compute phases first, then all render phases.
    ComputeThenRender,
    /// A user-defined permutation of phase indices.
    Custom(Vec<usize>),
}

/// A compiled pipeline together with an explicit execution order for its phases.
#[derive(Debug, Clone)]
pub struct StageExecutionPlan {
    pub pipeline: CompiledPipeline,
    pub order: ExecutionOrder,
}

impl StageExecutionPlan {
    /// Returns phase indices in the order they should be executed.
    pub fn execution_sequence(&self) -> Vec<usize> {
        let n = self.pipeline.phases.len();
        match &self.order {
            ExecutionOrder::Sequential => (0..n).collect(),
            ExecutionOrder::ComputeThenRender => {
                let mut compute_indices = Vec::new();
                let mut render_indices = Vec::new();
                for (i, phase) in self.pipeline.phases.iter().enumerate() {
                    match phase {
                        ExecutionPhase::Compute { .. } => compute_indices.push(i),
                        ExecutionPhase::Render { .. } => render_indices.push(i),
                    }
                }
                compute_indices.extend(render_indices);
                compute_indices
            }
            ExecutionOrder::Custom(indices) => indices.clone(),
        }
    }

    /// Returns an iterator over phases in execution order.
    pub fn ordered_phases(&self) -> Vec<&ExecutionPhase> {
        self.execution_sequence()
            .iter()
            .filter_map(|&i| self.pipeline.phases.get(i))
            .collect()
    }
}

// ─── Public API ─────────────────────────────────────────────────────────────

/// Compile a declarative stage list into an executable pipeline descriptor.
///
/// `stage_list` is a list of phase groups. Each group is a list of "module.ext" references.
/// Extensions: ".vs" = vertex, ".fs" = fragment, ".cs" = compute.
pub fn compile_pipeline(
    stage_list: &[Vec<String>],
    modules: &HashMap<String, ReflectedModule>,
) -> ShaderResult<CompiledPipeline> {
    let mut all_phases: Vec<ExecutionPhase> = Vec::new();

    for phase_group in stage_list {
        let phases = split_phases(phase_group, modules)?;
        all_phases.extend(phases);
    }

    // Collect all bindings from all phases
    let all_bindings: Vec<ShaderBinding> = all_phases
        .iter()
        .flat_map(|p| match p {
            ExecutionPhase::Compute { bindings, .. } => bindings.clone(),
            ExecutionPhase::Render { bindings, .. } => bindings.clone(),
        })
        .collect();
    let merged_bindings = merge_bindings(&all_bindings)?;

    let (vertex_layout, vertex_data_mapping) = build_unified_vertex_layout(&all_phases, modules)?;

    Ok(CompiledPipeline {
        phases: all_phases,
        merged_bindings,
        vertex_layout,
        vertex_data_mapping,
    })
}

impl CompiledPipeline {
    /// Build a `StageExecutionPlan` from this compiled pipeline and a desired order.
    ///
    /// `order` specifies how phases should be sequenced:
    /// - `Sequential` — execute phases in their natural order.
    /// - `ComputeThenRender` — reorder so all compute phases run before render phases.
    /// - `Custom(indices)` — an explicit permutation of phase indices.
    ///
    /// Returns an error if a `Custom` order references out-of-range or duplicate indices.
    pub fn build_execution_plan(
        self,
        order: ExecutionOrder,
    ) -> ShaderResult<StageExecutionPlan> {
        let n = self.phases.len();
        if let ExecutionOrder::Custom(ref indices) = order {
            // Validate: every index in range
            for &idx in indices {
                if idx >= n {
                    return Err(ShaderError::Reflect {
                        message: format!(
                            "Execution plan index {} out of range (have {} phases)",
                            idx, n
                        ),
                    });
                }
            }
            // Validate: no duplicates
            let mut seen = vec![false; n];
            for &idx in indices {
                if seen[idx] {
                    return Err(ShaderError::Reflect {
                        message: format!(
                            "Execution plan contains duplicate index {}",
                            idx
                        ),
                    });
                }
                seen[idx] = true;
            }
        }
        Ok(StageExecutionPlan {
            pipeline: self,
            order,
        })
    }
}

// ─── Phase Splitting ────────────────────────────────────────────────────────

fn split_phases(
    steps: &[String],
    modules: &HashMap<String, ReflectedModule>,
) -> ShaderResult<Vec<ExecutionPhase>> {
    let mut phases = Vec::new();
    let mut current_vs: Option<String> = None;
    let mut current_fs: Option<String> = None;
    let mut current_bindings: Vec<ShaderBinding> = Vec::new();
    let mut current_vs_locations: Vec<u32> = Vec::new();

    for step in steps {
        let (module_name, stage_hint) = parse_step_reference(step);

        let module = modules.get(&module_name).ok_or_else(|| ShaderError::Reflect {
            message: format!("Module '{}' not found", module_name),
        })?;

        let ep_stage = determine_stage(module, &stage_hint)?;

        match ep_stage {
            ShaderStage::Compute => {
                // Flush current render phase if any
                if current_vs.is_some() || current_fs.is_some() {
                    phases.push(make_render_phase(
                        &mut current_vs,
                        &mut current_fs,
                        &mut current_bindings,
                        &mut current_vs_locations,
                    ));
                }
                // Create compute phase
                let wgs = module
                    .entry_points
                    .iter()
                    .find(|e| e.stage == ShaderStage::Compute)
                    .and_then(|e| e.workgroup_size)
                    .unwrap_or([1, 1, 1]);
                let ep_name = module
                    .entry_points
                    .iter()
                    .find(|e| e.stage == ShaderStage::Compute)
                    .map(|e| e.name.clone())
                    .unwrap_or_else(|| "cs_main".to_string());
                phases.push(ExecutionPhase::Compute {
                    shader: module_name.clone(),
                    entry_point: ep_name,
                    workgroup_size: wgs,
                    bindings: module.bindings.clone(),
                });
            }
            ShaderStage::Vertex => {
                // If we already have a VS, flush current render phase
                if current_vs.is_some() {
                    phases.push(make_render_phase(
                        &mut current_vs,
                        &mut current_fs,
                        &mut current_bindings,
                        &mut current_vs_locations,
                    ));
                }
                current_vs = Some(module_name.clone());
                current_bindings.extend(module.bindings.iter().cloned());
                current_vs_locations =
                    module.vertex_attributes.iter().map(|a| a.location).collect();
            }
            ShaderStage::Fragment => {
                current_fs = Some(module_name.clone());
                current_bindings.extend(module.bindings.iter().cloned());
            }
        }
    }

    // Flush final render phase
    if current_vs.is_some() || current_fs.is_some() {
        phases.push(make_render_phase(
            &mut current_vs,
            &mut current_fs,
            &mut current_bindings,
            &mut current_vs_locations,
        ));
    }

    Ok(phases)
}

fn make_render_phase(
    current_vs: &mut Option<String>,
    current_fs: &mut Option<String>,
    current_bindings: &mut Vec<ShaderBinding>,
    current_vs_locations: &mut Vec<u32>,
) -> ExecutionPhase {
    let vs = current_vs.take();
    let fs = current_fs.take();
    let bindings = std::mem::take(current_bindings);
    let locations = std::mem::take(current_vs_locations);

    let vs_entry = vs
        .as_ref()
        .map(|_| "vs_main".to_string())
        .unwrap_or_default();
    let fs_entry = fs
        .as_ref()
        .map(|_| "fs_main".to_string())
        .unwrap_or_default();

    ExecutionPhase::Render {
        vertex_shader: vs,
        fragment_shader: fs,
        vertex_entry: vs_entry,
        fragment_entry: fs_entry,
        bindings,
        vertex_location_subset: locations,
    }
}

fn parse_step_reference(step: &str) -> (String, String) {
    if let Some(dot_pos) = step.rfind('.') {
        let name = &step[..dot_pos];
        let ext = &step[dot_pos + 1..];
        (name.to_string(), ext.to_string())
    } else {
        (step.to_string(), String::new())
    }
}

fn determine_stage(module: &ReflectedModule, stage_hint: &str) -> ShaderResult<ShaderStage> {
    match stage_hint {
        "vs" => Ok(ShaderStage::Vertex),
        "fs" => Ok(ShaderStage::Fragment),
        "cs" => Ok(ShaderStage::Compute),
        _ => {
            // Try to infer from entry points
            if module
                .entry_points
                .iter()
                .any(|e| e.stage == ShaderStage::Compute)
            {
                Ok(ShaderStage::Compute)
            } else if module
                .entry_points
                .iter()
                .any(|e| e.stage == ShaderStage::Vertex)
            {
                Ok(ShaderStage::Vertex)
            } else if module
                .entry_points
                .iter()
                .any(|e| e.stage == ShaderStage::Fragment)
            {
                Ok(ShaderStage::Fragment)
            } else {
                Err(ShaderError::Reflect {
                    message: format!(
                        "Cannot determine stage for module '{}', hint='{}'",
                        module.name, stage_hint
                    ),
                })
            }
        }
    }
}

// ─── Binding Merge ──────────────────────────────────────────────────────────

fn merge_bindings(all_bindings: &[ShaderBinding]) -> ShaderResult<Vec<ShaderBinding>> {
    let mut merged: HashMap<(u32, u32), ShaderBinding> = HashMap::new();
    for b in all_bindings {
        let key = (b.group, b.binding);
        if let Some(existing) = merged.get(&key) {
            if existing.name != b.name && !existing.name.is_empty() && !b.name.is_empty() {
                return Err(ShaderError::BindingConflict {
                    group: b.group,
                    binding: b.binding,
                    existing: existing.name.clone(),
                    new: b.name.clone(),
                });
            }
            // Same name or one is empty — keep existing
        } else {
            merged.insert(key, b.clone());
        }
    }
    let mut result: Vec<ShaderBinding> = merged.into_values().collect();
    result.sort_by_key(|b| (b.group, b.binding));
    Ok(result)
}

// ─── Unified Vertex Layout ───────────────────────────────────────────────────

fn build_unified_vertex_layout(
    phases: &[ExecutionPhase],
    modules: &HashMap<String, ReflectedModule>,
) -> ShaderResult<(Option<VertexLayoutDescriptor>, Option<VertexDataMapping>)> {
    let mut location_map: HashMap<u32, VertexAttributeInfo> = HashMap::new();
    let mut function_subsets: HashMap<String, Vec<u32>> = HashMap::new();

    for phase in phases {
        for name in phase.module_names() {
            if let Some(module) = modules.get(name) {
                let subset: Vec<u32> =
                    module.vertex_attributes.iter().map(|a| a.location).collect();
                function_subsets.insert(name.to_string(), subset);

                for attr in &module.vertex_attributes {
                    if let Some(existing) = location_map.get(&attr.location) {
                        // CONFLICT CHECK: compare by WgslType (has PartialEq), NOT the whole struct
                        if existing.ty != attr.ty {
                            return Err(ShaderError::Reflect {
                                message: format!(
                                    "@location({}) conflict: '{}' ({:?}) vs '{}' ({:?})",
                                    attr.location,
                                    existing.name,
                                    existing.ty,
                                    attr.name,
                                    attr.ty
                                ),
                            });
                        }
                    } else {
                        location_map.insert(attr.location, attr.clone());
                    }
                }
            }
        }
    }

    if location_map.is_empty() {
        return Ok((
            None,
            Some(VertexDataMapping {
                unified_layout: None,
                function_subsets,
                semantic_sources: HashMap::new(),
            }),
        ));
    }

    // Sort by location, recalculate offsets
    let mut sorted_attrs: Vec<VertexAttributeInfo> = location_map.into_values().collect();
    sorted_attrs.sort_by_key(|a| a.location);
    let mut offset: u64 = 0;
    for attr in &mut sorted_attrs {
        attr.offset = offset;
        offset += attr.ty.byte_size();
    }
    let stride = offset;

    let layout = VertexLayoutDescriptor {
        attributes: sorted_attrs,
        stride,
        has_instance_data: false,
    };

    Ok((
        Some(layout.clone()),
        Some(VertexDataMapping {
            unified_layout: Some(layout),
            function_subsets,
            semantic_sources: HashMap::new(),
        }),
    ))
}

// ─── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // Helper: create a minimal ReflectedModule for testing
    fn make_module(
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

    fn make_vertex_attr(name: &str, location: u32, ty: WgslType) -> VertexAttributeInfo {
        let format = ty.to_vertex_format().unwrap_or(wgpu::VertexFormat::Float32x4);
        VertexAttributeInfo {
            name: name.to_string(),
            location,
            ty,
            format,
            offset: 0,
            semantic: StreamSemantic::Custom(name.to_string()),
            step_mode: wgpu::VertexStepMode::Vertex,
        }
    }

    fn make_binding(group: u32, binding: u32, name: &str) -> ShaderBinding {
        ShaderBinding {
            group,
            binding,
            name: name.to_string(),
            resource_type: BindingResourceType::UniformBuffer,
            visibility: wgpu::ShaderStages::all(),
        }
    }

    fn make_compute_ep(name: &str, wgs: [u32; 3]) -> EntryPointInfo {
        EntryPointInfo {
            name: name.to_string(),
            stage: ShaderStage::Compute,
            workgroup_size: Some(wgs),
            parameters: Vec::new(),
        }
    }

    fn make_vertex_ep(name: &str) -> EntryPointInfo {
        EntryPointInfo {
            name: name.to_string(),
            stage: ShaderStage::Vertex,
            workgroup_size: None,
            parameters: Vec::new(),
        }
    }

    fn make_fragment_ep(name: &str) -> EntryPointInfo {
        EntryPointInfo {
            name: name.to_string(),
            stage: ShaderStage::Fragment,
            workgroup_size: None,
            parameters: Vec::new(),
        }
    }

    // ─── Phase Splitting Tests ──────────────────────────────────────────

    #[test]
    fn test_phase_split_compute_only() {
        let mut modules = HashMap::new();
        modules.insert(
            "blur".to_string(),
            make_module(
                "blur",
                vec![make_compute_ep("cs_main", [64, 1, 1])],
                vec![make_binding(0, 0, "params")],
                vec![],
            ),
        );

        let stage_list = vec![vec!["blur.cs".to_string()]];
        let result = compile_pipeline(&stage_list, &modules).unwrap();

        assert_eq!(result.phases.len(), 1);
        match &result.phases[0] {
            ExecutionPhase::Compute {
                shader,
                workgroup_size,
                ..
            } => {
                assert_eq!(shader, "blur");
                assert_eq!(*workgroup_size, [64, 1, 1]);
            }
            _ => panic!("Expected Compute phase"),
        }
    }

    #[test]
    fn test_phase_split_render_only() {
        let mut modules = HashMap::new();
        modules.insert(
            "forward".to_string(),
            make_module(
                "forward",
                vec![make_vertex_ep("vs_main"), make_fragment_ep("fs_main")],
                vec![make_binding(0, 0, "camera")],
                vec![make_vertex_attr(
                    "position",
                    0,
                    WgslType::Vec3(WgslScalarType::F32),
                )],
            ),
        );

        let stage_list = vec![vec![
            "forward.vs".to_string(),
            "forward.fs".to_string(),
        ]];
        let result = compile_pipeline(&stage_list, &modules).unwrap();

        assert_eq!(result.phases.len(), 1);
        match &result.phases[0] {
            ExecutionPhase::Render {
                vertex_shader,
                fragment_shader,
                ..
            } => {
                assert_eq!(vertex_shader.as_deref(), Some("forward"));
                assert_eq!(fragment_shader.as_deref(), Some("forward"));
            }
            _ => panic!("Expected Render phase"),
        }
    }

    #[test]
    fn test_phase_split_mixed() {
        let mut modules = HashMap::new();
        modules.insert(
            "blur".to_string(),
            make_module(
                "blur",
                vec![make_compute_ep("cs_main", [16, 16, 1])],
                vec![make_binding(0, 0, "params")],
                vec![],
            ),
        );
        modules.insert(
            "forward".to_string(),
            make_module(
                "forward",
                vec![make_vertex_ep("vs_main"), make_fragment_ep("fs_main")],
                vec![make_binding(0, 1, "camera")],
                vec![make_vertex_attr(
                    "position",
                    0,
                    WgslType::Vec3(WgslScalarType::F32),
                )],
            ),
        );

        let stage_list = vec![vec![
            "blur.cs".to_string(),
            "forward.vs".to_string(),
            "forward.fs".to_string(),
        ]];
        let result = compile_pipeline(&stage_list, &modules).unwrap();

        assert_eq!(result.phases.len(), 2);
        assert!(matches!(
            &result.phases[0],
            ExecutionPhase::Compute { .. }
        ));
        assert!(matches!(
            &result.phases[1],
            ExecutionPhase::Render { .. }
        ));
    }

    // ─── Binding Merge Tests ────────────────────────────────────────────

    #[test]
    fn test_binding_merge() {
        let mut modules = HashMap::new();
        modules.insert(
            "a".to_string(),
            make_module(
                "a",
                vec![make_vertex_ep("vs_main")],
                vec![make_binding(0, 0, "camera")],
                vec![],
            ),
        );
        modules.insert(
            "b".to_string(),
            make_module(
                "b",
                vec![make_fragment_ep("fs_main")],
                vec![make_binding(0, 1, "lights")],
                vec![],
            ),
        );

        let stage_list = vec![vec!["a.vs".to_string(), "b.fs".to_string()]];
        let result = compile_pipeline(&stage_list, &modules).unwrap();

        assert_eq!(result.merged_bindings.len(), 2);
    }

    #[test]
    fn test_binding_merge_same_slot_same_name() {
        let mut modules = HashMap::new();
        modules.insert(
            "a".to_string(),
            make_module(
                "a",
                vec![make_vertex_ep("vs_main")],
                vec![make_binding(0, 0, "camera")],
                vec![],
            ),
        );
        modules.insert(
            "b".to_string(),
            make_module(
                "b",
                vec![make_fragment_ep("fs_main")],
                vec![make_binding(0, 0, "camera")], // same slot, same name
                vec![],
            ),
        );

        let stage_list = vec![vec!["a.vs".to_string(), "b.fs".to_string()]];
        let result = compile_pipeline(&stage_list, &modules).unwrap();

        // Should dedup to 1 binding
        assert_eq!(result.merged_bindings.len(), 1);
    }

    #[test]
    fn test_binding_conflict_detected() {
        let mut modules = HashMap::new();
        modules.insert(
            "a".to_string(),
            make_module(
                "a",
                vec![make_vertex_ep("vs_main")],
                vec![make_binding(0, 0, "camera")],
                vec![],
            ),
        );
        modules.insert(
            "b".to_string(),
            make_module(
                "b",
                vec![make_fragment_ep("fs_main")],
                vec![make_binding(0, 0, "lights")], // same slot, different name
                vec![],
            ),
        );

        let stage_list = vec![vec!["a.vs".to_string(), "b.fs".to_string()]];
        let result = compile_pipeline(&stage_list, &modules);

        assert!(result.is_err());
    }

    // ─── Unified Vertex Layout Tests ────────────────────────────────────

    #[test]
    fn test_unified_vertex_layout() {
        let mut modules = HashMap::new();
        modules.insert(
            "a".to_string(),
            make_module(
                "a",
                vec![make_vertex_ep("vs_main")],
                vec![],
                vec![
                    make_vertex_attr("position", 0, WgslType::Vec3(WgslScalarType::F32)),
                    make_vertex_attr("normal", 1, WgslType::Vec3(WgslScalarType::F32)),
                ],
            ),
        );
        modules.insert(
            "b".to_string(),
            make_module(
                "b",
                vec![make_vertex_ep("vs_main2")],
                vec![],
                vec![
                    make_vertex_attr("position", 0, WgslType::Vec3(WgslScalarType::F32)), // same loc, same type — OK
                    make_vertex_attr("uv", 2, WgslType::Vec2(WgslScalarType::F32)),        // new location
                ],
            ),
        );

        // Two separate render phases (each has its own vs)
        let stage_list = vec![vec!["a.vs".to_string()], vec!["b.vs".to_string()]];
        let result = compile_pipeline(&stage_list, &modules).unwrap();

        let layout = result.vertex_layout.unwrap();
        assert_eq!(layout.attributes.len(), 3); // loc 0, 1, 2
        assert_eq!(layout.attributes[0].location, 0);
        assert_eq!(layout.attributes[1].location, 1);
        assert_eq!(layout.attributes[2].location, 2);
    }

    #[test]
    fn test_vertex_location_conflict() {
        let mut modules = HashMap::new();
        modules.insert(
            "a".to_string(),
            make_module(
                "a",
                vec![make_vertex_ep("vs_main")],
                vec![],
                vec![make_vertex_attr(
                    "position",
                    0,
                    WgslType::Vec3(WgslScalarType::F32),
                )],
            ),
        );
        modules.insert(
            "b".to_string(),
            make_module(
                "b",
                vec![make_vertex_ep("vs_main2")],
                vec![],
                // Same location 0 but different type — conflict!
                vec![make_vertex_attr(
                    "data",
                    0,
                    WgslType::Vec4(WgslScalarType::F32),
                )],
            ),
        );

        let stage_list = vec![vec!["a.vs".to_string()], vec!["b.vs".to_string()]];
        let result = compile_pipeline(&stage_list, &modules);

        assert!(result.is_err());
    }

    #[test]
    fn test_module_not_found() {
        let modules = HashMap::new();
        let stage_list = vec![vec!["nonexistent.vs".to_string()]];
        let result = compile_pipeline(&stage_list, &modules);
        assert!(result.is_err());
    }

    // ─── Execution Plan Tests ───────────────────────────────────────────

    /// Helper: build a mixed pipeline (compute + render) for execution plan tests.
    fn make_mixed_pipeline() -> CompiledPipeline {
        let mut modules = HashMap::new();
        modules.insert(
            "blur".to_string(),
            make_module(
                "blur",
                vec![make_compute_ep("cs_main", [8, 8, 1])],
                vec![make_binding(0, 0, "params")],
                vec![],
            ),
        );
        modules.insert(
            "forward".to_string(),
            make_module(
                "forward",
                vec![make_vertex_ep("vs_main"), make_fragment_ep("fs_main")],
                vec![make_binding(0, 1, "camera")],
                vec![make_vertex_attr(
                    "position",
                    0,
                    WgslType::Vec3(WgslScalarType::F32),
                )],
            ),
        );
        // phases[0] = Render(forward), phases[1] = Compute(blur)
        let stage_list = vec![vec![
            "forward.vs".to_string(),
            "forward.fs".to_string(),
            "blur.cs".to_string(),
        ]];
        compile_pipeline(&stage_list, &modules).unwrap()
    }

    #[test]
    fn test_execution_plan_sequential() {
        let pipeline = make_mixed_pipeline();
        assert_eq!(pipeline.phases.len(), 2);

        let plan = pipeline
            .build_execution_plan(ExecutionOrder::Sequential)
            .unwrap();

        assert_eq!(plan.execution_sequence(), vec![0, 1]);
        // First in sequence is Render, second is Compute
        assert!(matches!(plan.ordered_phases()[0], ExecutionPhase::Render { .. }));
        assert!(matches!(plan.ordered_phases()[1], ExecutionPhase::Compute { .. }));
    }

    #[test]
    fn test_execution_plan_compute_then_render() {
        let pipeline = make_mixed_pipeline();
        let plan = pipeline
            .build_execution_plan(ExecutionOrder::ComputeThenRender)
            .unwrap();

        // Compute (index 1) should come first, then Render (index 0)
        assert_eq!(plan.execution_sequence(), vec![1, 0]);
        assert!(matches!(plan.ordered_phases()[0], ExecutionPhase::Compute { .. }));
        assert!(matches!(plan.ordered_phases()[1], ExecutionPhase::Render { .. }));
    }

    #[test]
    fn test_execution_plan_custom_order() {
        let pipeline = make_mixed_pipeline();
        // Reverse order: [1, 0]
        let plan = pipeline
            .build_execution_plan(ExecutionOrder::Custom(vec![1, 0]))
            .unwrap();

        assert_eq!(plan.execution_sequence(), vec![1, 0]);
    }

    #[test]
    fn test_execution_plan_custom_out_of_range() {
        let pipeline = make_mixed_pipeline();
        let result = pipeline.build_execution_plan(ExecutionOrder::Custom(vec![0, 5]));
        assert!(result.is_err());
    }

    #[test]
    fn test_execution_plan_custom_duplicate_index() {
        let pipeline = make_mixed_pipeline();
        let result = pipeline.build_execution_plan(ExecutionOrder::Custom(vec![0, 0]));
        assert!(result.is_err());
    }

    #[test]
    fn test_execution_plan_ordered_phases_matches_sequence() {
        let pipeline = make_mixed_pipeline();
        let plan = pipeline
            .build_execution_plan(ExecutionOrder::ComputeThenRender)
            .unwrap();

        let seq = plan.execution_sequence();
        let ordered = plan.ordered_phases();
        assert_eq!(seq.len(), ordered.len());
    }
}
