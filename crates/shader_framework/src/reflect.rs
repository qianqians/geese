//! WGSL reflection module — extracts structured metadata from WGSL source using naga AST.

use crate::core::*;
use crate::generator::{AddressSpace, ConstantDef, GlobalVarDef};
use crate::stream::*;

// ─── ReflectedModule ────────────────────────────────────────────────────────

/// All metadata extracted from a single WGSL source module via naga reflection.
#[derive(Debug, Clone)]
pub struct ReflectedModule {
    pub name: String,
    pub entry_points: Vec<EntryPointInfo>,
    pub bindings: Vec<ShaderBinding>,
    pub structs: Vec<StructDef>,
    pub functions: Vec<FunctionDef>,
    pub constants: Vec<ConstantDef>,
    pub global_vars: Vec<GlobalVarDef>,
    pub vertex_attributes: Vec<VertexAttributeInfo>,
    pub vertex_body: Option<WgslFragment>,
    pub fragment_body: Option<WgslFragment>,
    pub compute_body: Option<WgslFragment>,
}

// ─── Public API ─────────────────────────────────────────────────────────────

/// Parse WGSL source and extract structured metadata using naga AST + text scanning.
pub fn reflect_module(name: &str, source: &str) -> ShaderResult<ReflectedModule> {
    let naga_module = naga::front::wgsl::parse_str(source)
        .map_err(|e| ShaderError::Reflect { message: format!("naga parse: {e}") })?;

    let entry_points = extract_entry_points(&naga_module);
    let bindings = extract_bindings(&naga_module);
    let structs = extract_structs(&naga_module);
    let functions = extract_functions(&naga_module);
    let constants = extract_constants(&naga_module);
    let global_vars = extract_global_vars(&naga_module);
    let vertex_attributes = extract_vertex_attributes(&naga_module);

    let vertex_body = extract_entry_point_body(source, &entry_points, ShaderStage::Vertex);
    let fragment_body = extract_entry_point_body(source, &entry_points, ShaderStage::Fragment);
    let compute_body = extract_entry_point_body(source, &entry_points, ShaderStage::Compute);

    Ok(ReflectedModule {
        name: name.to_string(),
        entry_points,
        bindings,
        structs,
        functions,
        constants,
        global_vars,
        vertex_attributes,
        vertex_body,
        fragment_body,
        compute_body,
    })
}

// ─── Type Resolution ────────────────────────────────────────────────────────

fn resolve_type_handle(
    handle: naga::Handle<naga::Type>,
    types: &naga::UniqueArena<naga::Type>,
) -> WgslType {
    let ty = &types[handle];
    match &ty.inner {
        naga::TypeInner::Scalar(scalar) => scalar_to_wgsl(scalar),
        naga::TypeInner::Vector { size, scalar } => {
            let st = scalar_kind_to_wgsl(scalar);
            match size {
                naga::VectorSize::Bi => WgslType::Vec2(st),
                naga::VectorSize::Tri => WgslType::Vec3(st),
                naga::VectorSize::Quad => WgslType::Vec4(st),
            }
        }
        naga::TypeInner::Matrix { columns, rows, scalar } => {
            let st = scalar_kind_to_wgsl(scalar);
            match (columns, rows) {
                (naga::VectorSize::Bi, naga::VectorSize::Bi) => WgslType::Mat2x2(st),
                (naga::VectorSize::Tri, naga::VectorSize::Tri) => WgslType::Mat3x3(st),
                (naga::VectorSize::Quad, naga::VectorSize::Quad) => WgslType::Mat4x4(st),
                _ => WgslType::Custom(format!(
                    "mat{}x{}", *columns as u8, *rows as u8
                )),
            }
        }
        naga::TypeInner::Array { base, size, .. } => {
            let elem = resolve_type_handle(*base, types);
            let n = match size {
                naga::ArraySize::Constant(nz) => Some(nz.get()),
                naga::ArraySize::Dynamic => None,
            };
            WgslType::Array(Box::new(elem), n)
        }
        naga::TypeInner::Struct { .. } => {
            WgslType::Struct(ty.name.clone().unwrap_or_default())
        }
        naga::TypeInner::Image { dim, class, .. } => match (dim, class) {
            (naga::ImageDimension::D2, naga::ImageClass::Sampled { .. }) => WgslType::Texture2d,
            (naga::ImageDimension::Cube, naga::ImageClass::Sampled { .. }) => WgslType::TextureCube,
            (naga::ImageDimension::D2, naga::ImageClass::Depth { .. }) => {
                WgslType::Custom("texture_depth_2d".to_string())
            }
            _ => WgslType::Custom(format!("texture_{:?}", dim)),
        },
        naga::TypeInner::Sampler { .. } => WgslType::Sampler,
        _ => WgslType::Custom(ty.name.clone().unwrap_or_else(|| format!("{:?}", ty.inner))),
    }
}

fn scalar_to_wgsl(scalar: &naga::Scalar) -> WgslType {
    match (scalar.kind, scalar.width) {
        (naga::ScalarKind::Float, 4) => WgslType::F32,
        (naga::ScalarKind::Sint, 4) => WgslType::I32,
        (naga::ScalarKind::Uint, 4) => WgslType::U32,
        (naga::ScalarKind::Bool, 1) | (naga::ScalarKind::Bool, _) => WgslType::Bool,
        _ => WgslType::Custom(format!("scalar_{:?}_{}", scalar.kind, scalar.width)),
    }
}

fn scalar_kind_to_wgsl(scalar: &naga::Scalar) -> WgslScalarType {
    match scalar.kind {
        naga::ScalarKind::Float => WgslScalarType::F32,
        naga::ScalarKind::Sint => WgslScalarType::I32,
        naga::ScalarKind::Uint => WgslScalarType::U32,
        naga::ScalarKind::Bool => WgslScalarType::Bool,
        _ => WgslScalarType::F32,
    }
}

// ─── Entry Points ───────────────────────────────────────────────────────────

fn extract_entry_points(module: &naga::Module) -> Vec<EntryPointInfo> {
    module
        .entry_points
        .iter()
        .map(|ep| {
            let stage = match ep.stage {
                naga::ShaderStage::Vertex => ShaderStage::Vertex,
                naga::ShaderStage::Fragment => ShaderStage::Fragment,
                naga::ShaderStage::Compute => ShaderStage::Compute,
            };
            let workgroup_size = if ep.stage == naga::ShaderStage::Compute {
                Some(ep.workgroup_size)
            } else {
                None
            };
            let parameters = ep
                .function
                .arguments
                .iter()
                .map(|arg| ParameterInfo {
                    name: arg.name.clone().unwrap_or_default(),
                    ty: resolve_type_handle(arg.ty, &module.types),
                    binding: arg.binding.as_ref().map(|b| match b {
                        naga::Binding::BuiltIn(bi) => {
                            ParameterBinding::Builtin(builtin_to_string(bi))
                        }
                        naga::Binding::Location { location, .. } => {
                            ParameterBinding::Location(*location)
                        }
                    }),
                })
                .collect();
            EntryPointInfo {
                name: ep.name.clone(),
                stage,
                workgroup_size,
                parameters,
            }
        })
        .collect()
}

// ─── Vertex Attributes ──────────────────────────────────────────────────────

fn extract_vertex_attributes(module: &naga::Module) -> Vec<VertexAttributeInfo> {
    let Some(vs_ep) = module
        .entry_points
        .iter()
        .find(|ep| ep.stage == naga::ShaderStage::Vertex)
    else {
        return Vec::new();
    };

    let mut attrs = Vec::new();
    let mut offset: u64 = 0;

    for arg in &vs_ep.function.arguments {
        match &arg.binding {
            // Mode B: inline @location
            Some(naga::Binding::Location { location, .. }) => {
                let ty = resolve_type_handle(arg.ty, &module.types);
                let name = arg.name.clone().unwrap_or_default();
                let format = ty.to_vertex_format().unwrap_or(wgpu::VertexFormat::Float32x4);
                let attr_size = ty.byte_size();
                attrs.push(VertexAttributeInfo {
                    name: name.clone(),
                    location: *location,
                    ty,
                    format,
                    offset,
                    semantic: infer_semantic(&name),
                    step_mode: wgpu::VertexStepMode::Vertex,
                });
                offset += attr_size;
            }
            // Mode C: @builtin — skip
            Some(naga::Binding::BuiltIn(_)) => {}
            // Mode A: struct parameter — expand members
            None => {
                let ty_inner = &module.types[arg.ty].inner;
                if let naga::TypeInner::Struct { members, .. } = ty_inner {
                    for member in members {
                        match &member.binding {
                            Some(naga::Binding::Location { location, .. }) => {
                                let field_name = member.name.clone().unwrap_or_default();
                                let field_ty =
                                    resolve_type_handle(member.ty, &module.types);
                                let format = field_ty
                                    .to_vertex_format()
                                    .unwrap_or(wgpu::VertexFormat::Float32x4);
                                let attr_size = field_ty.byte_size();
                                attrs.push(VertexAttributeInfo {
                                    name: field_name.clone(),
                                    location: *location,
                                    ty: field_ty,
                                    format,
                                    offset,
                                    semantic: infer_semantic(&field_name),
                                    step_mode: wgpu::VertexStepMode::Vertex,
                                });
                                offset += attr_size;
                            }
                            _ => {}
                        }
                    }
                }
            }
        }
    }

    attrs
}

// ─── Semantic Inference ─────────────────────────────────────────────────────

fn infer_semantic(name: &str) -> StreamSemantic {
    let lower = name.to_lowercase();
    match lower.as_str() {
        "position" | "pos" | "in_pos" => StreamSemantic::Position,
        "normal" | "nrm" | "in_normal" => StreamSemantic::Normal,
        "tangent" | "tan" | "in_tangent" => StreamSemantic::Tangent,
        "joints" | "bone_indices" | "bone_ids" => StreamSemantic::BoneIndices,
        "weights" | "bone_weights" => StreamSemantic::BoneWeights,
        _ if lower.starts_with("uv") || lower.starts_with("texcoord") => {
            let idx: u32 = lower
                .chars()
                .filter(|c| c.is_ascii_digit())
                .collect::<String>()
                .parse()
                .unwrap_or(0);
            StreamSemantic::UV(idx)
        }
        _ if lower.starts_with("color") || lower.starts_with("col") => {
            let idx: u32 = lower
                .chars()
                .filter(|c| c.is_ascii_digit())
                .collect::<String>()
                .parse()
                .unwrap_or(0);
            StreamSemantic::Color(idx)
        }
        _ => StreamSemantic::Custom(name.to_string()),
    }
}

// ─── Bindings ───────────────────────────────────────────────────────────────

fn extract_bindings(module: &naga::Module) -> Vec<ShaderBinding> {
    module
        .global_variables
        .iter()
        .filter_map(|(_, var)| {
            let rb = var.binding.as_ref()?;
            Some(ShaderBinding {
                group: rb.group,
                binding: rb.binding,
                name: var.name.clone().unwrap_or_default(),
                resource_type: classify_resource(var, module),
                visibility: wgpu::ShaderStages::all(),
            })
        })
        .collect()
}

fn classify_resource(var: &naga::GlobalVariable, module: &naga::Module) -> BindingResourceType {
    match &var.space {
        naga::AddressSpace::Uniform => BindingResourceType::UniformBuffer,
        naga::AddressSpace::Storage { access } => BindingResourceType::StorageBuffer {
            read_only: !access.contains(naga::StorageAccess::STORE),
        },
        naga::AddressSpace::Handle => {
            let ty = &module.types[var.ty];
            match &ty.inner {
                naga::TypeInner::Image { dim, class, .. } => match class {
                    naga::ImageClass::Sampled { kind, .. } => {
                        let dimension = match dim {
                            naga::ImageDimension::D2 => TextureDimension::D2,
                            naga::ImageDimension::Cube => TextureDimension::Cube,
                            _ => TextureDimension::D2,
                        };
                        let sample_type = match kind {
                            naga::ScalarKind::Float => TextureSampleType::Float,
                            naga::ScalarKind::Sint => TextureSampleType::Sint,
                            naga::ScalarKind::Uint => TextureSampleType::Uint,
                            _ => TextureSampleType::Float,
                        };
                        BindingResourceType::Texture { dimension, sample_type }
                    }
                    naga::ImageClass::Depth { .. } => BindingResourceType::Texture {
                        dimension: TextureDimension::D2,
                        sample_type: TextureSampleType::Depth,
                    },
                    naga::ImageClass::Storage { format, access } => {
                        let dimension = match dim {
                            naga::ImageDimension::D2 => TextureDimension::D2,
                            naga::ImageDimension::Cube => TextureDimension::Cube,
                            _ => TextureDimension::D2,
                        };
                        let access_str = if access.contains(naga::StorageAccess::STORE) {
                            "write"
                        } else {
                            "read"
                        };
                        BindingResourceType::StorageTexture {
                            dimension,
                            format: format!("{:?}", format),
                            access: access_str.to_string(),
                        }
                    }
                },
                naga::TypeInner::Sampler { comparison } => {
                    if *comparison {
                        BindingResourceType::Sampler(SamplerType::Comparison)
                    } else {
                        BindingResourceType::Sampler(SamplerType::Filtering)
                    }
                }
                _ => BindingResourceType::UniformBuffer,
            }
        }
        _ => BindingResourceType::UniformBuffer,
    }
}

// ─── Structs ────────────────────────────────────────────────────────────────

fn extract_structs(module: &naga::Module) -> Vec<StructDef> {
    module
        .types
        .iter()
        .filter_map(|(_, ty)| {
            let name = ty.name.as_ref()?;
            if let naga::TypeInner::Struct { members, .. } = &ty.inner {
                Some(StructDef {
                    name: name.clone(),
                    fields: members
                        .iter()
                        .map(|m| StructField {
                            name: m.name.clone().unwrap_or_default(),
                            ty: resolve_type_handle(m.ty, &module.types),
                            attributes: extract_member_attributes(&m.binding),
                        })
                        .collect(),
                })
            } else {
                None
            }
        })
        .collect()
}

fn extract_member_attributes(binding: &Option<naga::Binding>) -> Vec<StructFieldAttribute> {
    let mut attrs = Vec::new();
    if let Some(b) = binding {
        match b {
            naga::Binding::Location { location, .. } => {
                attrs.push(StructFieldAttribute::Location(*location));
            }
            naga::Binding::BuiltIn(bi) => {
                attrs.push(StructFieldAttribute::Builtin(builtin_to_string(bi)));
            }
        }
    }
    attrs
}

// ─── Functions ──────────────────────────────────────────────────────────────

fn extract_functions(module: &naga::Module) -> Vec<FunctionDef> {
    module
        .functions
        .iter()
        .filter_map(|(_, func)| {
            let name = func.name.as_ref()?.clone();
            let parameters = func
                .arguments
                .iter()
                .map(|arg| {
                    (
                        arg.name.clone().unwrap_or_default(),
                        resolve_type_handle(arg.ty, &module.types),
                    )
                })
                .collect();
            let return_type = func
                .result
                .as_ref()
                .map(|r| resolve_type_handle(r.ty, &module.types));
            Some(FunctionDef {
                name,
                parameters,
                return_type,
                body: WgslFragment::empty(),
                overridable: false,
            })
        })
        .collect()
}

// ─── Constants ──────────────────────────────────────────────────────────────

fn extract_constants(module: &naga::Module) -> Vec<ConstantDef> {
    module
        .constants
        .iter()
        .filter_map(|(_, c)| {
            let name = c.name.as_ref()?.clone();
            let ty = resolve_type_handle(c.ty, &module.types);
            let expr = &module.global_expressions[c.init];
            let default_value = match expr {
                naga::Expression::Literal(naga::Literal::F32(v)) => Some(v.to_string()),
                naga::Expression::Literal(naga::Literal::U32(v)) => Some(format!("{v}u")),
                naga::Expression::Literal(naga::Literal::I32(v)) => Some(format!("{v}i")),
                naga::Expression::Literal(naga::Literal::Bool(v)) => Some(v.to_string()),
                _ => None,
            };
            Some(ConstantDef {
                name,
                ty,
                id: 0,
                default_value,
                is_const: true,
            })
        })
        .collect()
}

// ─── Global Vars ────────────────────────────────────────────────────────────

fn extract_global_vars(module: &naga::Module) -> Vec<GlobalVarDef> {
    module
        .global_variables
        .iter()
        .filter_map(|(_, var)| {
            let space = match var.space {
                naga::AddressSpace::Private => AddressSpace::Private,
                naga::AddressSpace::WorkGroup => AddressSpace::Workgroup,
                _ => return None,
            };
            let name = var.name.as_ref()?.clone();
            let ty = resolve_type_handle(var.ty, &module.types);
            Some(GlobalVarDef {
                name,
                ty,
                address_space: space,
                init: None,
            })
        })
        .collect()
}

// ─── Helpers ────────────────────────────────────────────────────────────────

fn builtin_to_string(bi: &naga::BuiltIn) -> String {
    match bi {
        naga::BuiltIn::Position { .. } => "position",
        naga::BuiltIn::VertexIndex => "vertex_index",
        naga::BuiltIn::InstanceIndex => "instance_index",
        naga::BuiltIn::GlobalInvocationId => "global_invocation_id",
        naga::BuiltIn::LocalInvocationId => "local_invocation_id",
        naga::BuiltIn::LocalInvocationIndex => "local_invocation_index",
        naga::BuiltIn::WorkGroupId => "workgroup_id",
        naga::BuiltIn::FrontFacing => "front_facing",
        naga::BuiltIn::FragDepth => "frag_depth",
        naga::BuiltIn::SampleIndex => "sample_index",
        naga::BuiltIn::SampleMask => "sample_mask",
        naga::BuiltIn::PointSize => "point_size",
        naga::BuiltIn::BaseVertex => "base_vertex",
        naga::BuiltIn::BaseInstance => "base_instance",
        naga::BuiltIn::NumWorkGroups => "num_workgroups",
        naga::BuiltIn::WorkGroupSize => "workgroup_size",
        _ => "unknown",
    }
    .to_string()
}

// ─── Text Body Extraction ───────────────────────────────────────────────────

/// Find the content range of a brace-delimited block starting at `open_pos`.
/// Returns (content_start, content_end) excluding the outermost braces.
fn find_brace_block(source: &str, open_pos: usize) -> Option<(usize, usize)> {
    let bytes = source.as_bytes();
    if open_pos >= bytes.len() || bytes[open_pos] != b'{' {
        return None;
    }

    let mut depth = 1i32;
    let mut i = open_pos + 1;
    let mut in_line_comment = false;
    let mut in_block_comment = false;

    while i < bytes.len() && depth > 0 {
        let ch = bytes[i];

        if in_line_comment {
            if ch == b'\n' {
                in_line_comment = false;
            }
            i += 1;
            continue;
        }

        if in_block_comment {
            if ch == b'*' && i + 1 < bytes.len() && bytes[i + 1] == b'/' {
                in_block_comment = false;
                i += 2;
            } else {
                i += 1;
            }
            continue;
        }

        if ch == b'/' && i + 1 < bytes.len() {
            if bytes[i + 1] == b'/' {
                in_line_comment = true;
                i += 2;
                continue;
            } else if bytes[i + 1] == b'*' {
                in_block_comment = true;
                i += 2;
                continue;
            }
        }

        if ch == b'{' {
            depth += 1;
        } else if ch == b'}' {
            depth -= 1;
            if depth == 0 {
                return Some((open_pos + 1, i));
            }
        }

        i += 1;
    }

    None
}

/// Extract the body text of an entry point function from WGSL source.
fn extract_entry_point_body(
    source: &str,
    entry_points: &[EntryPointInfo],
    stage: ShaderStage,
) -> Option<WgslFragment> {
    let ep = entry_points.iter().find(|ep| ep.stage == stage)?;
    let pattern = format!(r"\bfn\s+{}\s*\(", regex::escape(&ep.name));
    let re = regex::Regex::new(&pattern).ok()?;
    let m = re.find(source)?;

    // Find opening brace after `fn name(`
    let search_start = m.end();
    let open_pos = source[search_start..].find('{')?;
    let abs_open = search_start + open_pos;

    let (content_start, content_end) = find_brace_block(source, abs_open)?;
    let body = source[content_start..content_end].trim();

    if body.is_empty() {
        None
    } else {
        Some(WgslFragment::labeled(&ep.name, body))
    }
}

// ─── into_shader_module ─────────────────────────────────────────────────────

impl ReflectedModule {
    /// Convert reflected metadata into a `ShaderModule` for the composition pipeline.
    pub fn into_shader_module(self) -> crate::compose::ShaderModule {
        crate::compose::ShaderModule {
            name: self.name,
            input_streams: Vec::new(), // TODO: generate from vertex_attributes
            output_streams: Vec::new(),
            bindings: self.bindings,
            structs: self.structs,
            functions: self.functions,
            vertex_body: self.vertex_body,
            fragment_body: self.fragment_body,
            compute_body: self.compute_body,
            constants: self.constants,
            global_vars: self.global_vars,
            dependencies: Vec::new(),
            vertex_attributes: self.vertex_attributes,
        }
    }
}

// ─── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    const LINES_WGSL: &str = include_str!("../../render/shaders/lines.wgsl");
    const SHADOW_DEPTH_WGSL: &str = include_str!("../../render/shaders/shadow_depth.wgsl");

    #[test]
    fn test_reflect_lines_entry_points() {
        let result = reflect_module("lines", LINES_WGSL).unwrap();
        assert_eq!(result.entry_points.len(), 2); // vs + fs
        let stages: Vec<ShaderStage> = result.entry_points.iter().map(|ep| ep.stage).collect();
        assert!(stages.contains(&ShaderStage::Vertex));
        assert!(stages.contains(&ShaderStage::Fragment));
    }

    #[test]
    fn test_reflect_lines_vertex_attributes() {
        let result = reflect_module("lines", LINES_WGSL).unwrap();
        // LineVertexInput has @location(0) position and @location(1) color
        assert_eq!(result.vertex_attributes.len(), 2);
        assert_eq!(result.vertex_attributes[0].name, "position");
        assert_eq!(result.vertex_attributes[0].location, 0);
        assert_eq!(result.vertex_attributes[1].name, "color");
        assert_eq!(result.vertex_attributes[1].location, 1);
    }

    #[test]
    fn test_reflect_shadow_depth_inline_vertex() {
        let result = reflect_module("shadow_depth", SHADOW_DEPTH_WGSL).unwrap();
        assert_eq!(result.vertex_attributes.len(), 1); // inline @location(0)
        assert_eq!(result.vertex_attributes[0].location, 0);
        assert_eq!(result.vertex_attributes[0].name, "in_pos");
    }

    #[test]
    fn test_reflect_lines_bindings() {
        let result = reflect_module("lines", LINES_WGSL).unwrap();
        assert_eq!(result.bindings.len(), 1);
        assert_eq!(result.bindings[0].name, "camera");
        assert_eq!(result.bindings[0].group, 0);
        assert_eq!(result.bindings[0].binding, 0);
        assert_eq!(result.bindings[0].resource_type, BindingResourceType::UniformBuffer);
    }

    #[test]
    fn test_reflect_lines_structs() {
        let result = reflect_module("lines", LINES_WGSL).unwrap();
        let struct_names: Vec<&str> = result.structs.iter().map(|s| s.name.as_str()).collect();
        assert!(struct_names.contains(&"Camera"));
        assert!(struct_names.contains(&"LineVertexInput"));
        assert!(struct_names.contains(&"LineVertexOutput"));
    }

    #[test]
    fn test_reflect_lines_bodies() {
        let result = reflect_module("lines", LINES_WGSL).unwrap();
        assert!(result.vertex_body.is_some());
        assert!(result.fragment_body.is_some());
        assert!(result.compute_body.is_none());

        let vs_body = result.vertex_body.as_ref().unwrap();
        assert!(vs_body.source.contains("camera.view_projection"));

        let fs_body = result.fragment_body.as_ref().unwrap();
        assert!(fs_body.source.contains("in.color"));
    }

    #[test]
    fn test_infer_semantic() {
        assert_eq!(infer_semantic("position"), StreamSemantic::Position);
        assert_eq!(infer_semantic("pos"), StreamSemantic::Position);
        assert_eq!(infer_semantic("in_pos"), StreamSemantic::Position);
        assert_eq!(infer_semantic("normal"), StreamSemantic::Normal);
        assert_eq!(infer_semantic("nrm"), StreamSemantic::Normal);
        assert_eq!(infer_semantic("tangent"), StreamSemantic::Tangent);
        assert_eq!(infer_semantic("uv0"), StreamSemantic::UV(0));
        assert_eq!(infer_semantic("uv1"), StreamSemantic::UV(1));
        assert_eq!(infer_semantic("texcoord0"), StreamSemantic::UV(0));
        assert_eq!(infer_semantic("color0"), StreamSemantic::Color(0));
        assert_eq!(infer_semantic("col1"), StreamSemantic::Color(1));
        assert_eq!(infer_semantic("joints"), StreamSemantic::BoneIndices);
        assert_eq!(infer_semantic("weights"), StreamSemantic::BoneWeights);
        assert_eq!(
            infer_semantic("something_else"),
            StreamSemantic::Custom("something_else".to_string())
        );
    }

    #[test]
    fn test_find_brace_block() {
        let src = "{ hello { world } end }";
        let (start, end) = find_brace_block(src, 0).unwrap();
        assert_eq!(&src[start..end], " hello { world } end ");
    }

    #[test]
    fn test_find_brace_block_with_comments() {
        let src = "{ // }\n  /* } */ x }";
        let (start, end) = find_brace_block(src, 0).unwrap();
        assert_eq!(&src[start..end], " // }\n  /* } */ x ");
    }

    #[test]
    fn test_reflect_into_shader_module() {
        let result = reflect_module("lines", LINES_WGSL).unwrap();
        let module = result.into_shader_module();
        assert_eq!(module.name, "lines");
        assert_eq!(module.bindings.len(), 1);
        assert!(module.vertex_body.is_some());
    }

    #[test]
    fn test_reflect_shadow_depth_body() {
        let result = reflect_module("shadow_depth", SHADOW_DEPTH_WGSL).unwrap();
        assert!(result.vertex_body.is_some());
        let body = result.vertex_body.as_ref().unwrap();
        assert!(body.source.contains("u_vp.view_proj"));
        assert!(body.source.contains("u_object.model"));
    }

    #[test]
    fn test_reflect_parse_error() {
        let bad_wgsl = "this is not valid wgsl {{{";
        let result = reflect_module("bad", bad_wgsl);
        assert!(result.is_err());
    }
}
