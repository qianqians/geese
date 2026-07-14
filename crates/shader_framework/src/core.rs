//! Core types for the shader composition framework.

use std::hash::{Hash, Hasher};

// ─── Scalar Type ─────────────────────────────────────────────────────────────

/// Scalar base types used in WGSL vectors and matrices.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WgslScalarType {
    F32,
    I32,
    U32,
    Bool,
}

impl WgslScalarType {
    /// WGSL keyword for this scalar type.
    pub fn to_wgsl(self) -> &'static str {
        match self {
            Self::F32 => "f32",
            Self::I32 => "i32",
            Self::U32 => "u32",
            Self::Bool => "bool",
        }
    }

    /// Byte size of a single scalar value.
    pub fn byte_size(self) -> u64 {
        match self {
            Self::F32 | Self::I32 | Self::U32 => 4,
            Self::Bool => 4, // padded to 4 bytes in WGSL
        }
    }
}

// ─── WgslType ─────────────────────────────────────────────────────────────────

/// Supported WGSL scalar / vector / matrix / composite types.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum WgslType {
    Bool,
    I32,
    U32,
    F32,
    Vec2(WgslScalarType),
    Vec3(WgslScalarType),
    Vec4(WgslScalarType),
    Mat2x2(WgslScalarType),
    Mat3x3(WgslScalarType),
    Mat4x4(WgslScalarType),
    /// `Array(element_type, optional_fixed_size)`
    Array(Box<WgslType>, Option<u32>),
    /// Reference to a named struct definition.
    Struct(String),
    Sampler,
    Texture2d,
    TextureCube,
    /// Arbitrary user-defined type name (emitted verbatim).
    Custom(String),
}

impl WgslType {
    /// Returns the canonical WGSL source representation of this type.
    pub fn to_wgsl(&self) -> String {
        match self {
            Self::Bool => "bool".to_string(),
            Self::I32 => "i32".to_string(),
            Self::U32 => "u32".to_string(),
            Self::F32 => "f32".to_string(),
            Self::Vec2(s) => format!("vec2<{}>", s.to_wgsl()),
            Self::Vec3(s) => format!("vec3<{}>", s.to_wgsl()),
            Self::Vec4(s) => format!("vec4<{}>", s.to_wgsl()),
            Self::Mat2x2(s) => format!("mat2x2<{}>", s.to_wgsl()),
            Self::Mat3x3(s) => format!("mat3x3<{}>", s.to_wgsl()),
            Self::Mat4x4(s) => format!("mat4x4<{}>", s.to_wgsl()),
            Self::Array(elem, Some(n)) => format!("array<{}, {}>", elem.to_wgsl(), n),
            Self::Array(elem, None) => format!("array<{}>", elem.to_wgsl()),
            Self::Struct(name) => name.clone(),
            Self::Sampler => "sampler".to_string(),
            Self::Texture2d => "texture_2d<f32>".to_string(),
            Self::TextureCube => "texture_cube<f32>".to_string(),
            Self::Custom(name) => name.clone(),
        }
    }

    /// Returns the byte size of this type (useful for vertex layout calculation).
    ///
    /// For unsized arrays, struct references, samplers and textures the result is `0`
    /// because their size cannot be statically determined here.
    pub fn byte_size(&self) -> u64 {
        match self {
            Self::Bool => 4,
            Self::I32 | Self::U32 | Self::F32 => 4,
            Self::Vec2(s) => 2 * s.byte_size(),
            Self::Vec3(s) => 3 * s.byte_size(),
            Self::Vec4(s) => 4 * s.byte_size(),
            Self::Mat2x2(s) => 2 * 2 * s.byte_size(),
            Self::Mat3x3(s) => 3 * 3 * s.byte_size(),
            Self::Mat4x4(s) => 4 * 4 * s.byte_size(),
            Self::Array(elem, Some(n)) => elem.byte_size() * (*n as u64),
            Self::Array(_, None) => 0,
            Self::Struct(_) | Self::Sampler | Self::Texture2d | Self::TextureCube => 0,
            Self::Custom(_) => 0,
        }
    }

    /// Maps this type to the equivalent `wgpu::VertexFormat` for vertex input attributes.
    ///
    /// Returns `None` for types that have no direct vertex format equivalent
    /// (matrices, structs, textures, samplers, etc.).
    pub fn to_vertex_format(&self) -> Option<wgpu::VertexFormat> {
        match self {
            Self::F32 => Some(wgpu::VertexFormat::Float32),
            Self::I32 => Some(wgpu::VertexFormat::Sint32),
            Self::U32 => Some(wgpu::VertexFormat::Uint32),
            Self::Vec2(WgslScalarType::F32) => Some(wgpu::VertexFormat::Float32x2),
            Self::Vec3(WgslScalarType::F32) => Some(wgpu::VertexFormat::Float32x3),
            Self::Vec4(WgslScalarType::F32) => Some(wgpu::VertexFormat::Float32x4),
            Self::Vec2(WgslScalarType::I32) => Some(wgpu::VertexFormat::Sint32x2),
            Self::Vec3(WgslScalarType::I32) => Some(wgpu::VertexFormat::Sint32x3),
            Self::Vec4(WgslScalarType::I32) => Some(wgpu::VertexFormat::Sint32x4),
            Self::Vec2(WgslScalarType::U32) => Some(wgpu::VertexFormat::Uint32x2),
            Self::Vec3(WgslScalarType::U32) => Some(wgpu::VertexFormat::Uint32x3),
            Self::Vec4(WgslScalarType::U32) => Some(wgpu::VertexFormat::Uint32x4),
            _ => None,
        }
    }
}

// ─── Shader Stage ─────────────────────────────────────────────────────────────

/// A programmable shader stage.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ShaderStage {
    Vertex,
    Fragment,
    Compute,
}

// ─── Binding Types ────────────────────────────────────────────────────────────

/// Texture view dimension for binding declarations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TextureDimension {
    D2,
    Cube,
    D2Array,
}

/// How a texture is sampled.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TextureSampleType {
    Float,
    Sint,
    Uint,
    Depth,
}

/// Sampler binding type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SamplerType {
    Filtering,
    NonFiltering,
    Comparison,
}

/// The kind of resource bound to a shader.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum BindingResourceType {
    UniformBuffer,
    StorageBuffer { read_only: bool },
    Texture {
        dimension: TextureDimension,
        sample_type: TextureSampleType,
    },
    Sampler(SamplerType),
}

/// A single resource binding descriptor (one `@group @binding` slot).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShaderBinding {
    pub group: u32,
    pub binding: u32,
    pub name: String,
    pub resource_type: BindingResourceType,
    pub visibility: wgpu::ShaderStages,
}

// Manual Hash: wgpu::ShaderStages may not implement Hash in all wgpu versions.
impl Hash for ShaderBinding {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.group.hash(state);
        self.binding.hash(state);
        self.name.hash(state);
        self.resource_type.hash(state);
        // ShaderStages is repr(transparent) over a u32 bitfield in wgpu-types.
        // Hash via its Debug representation which is stable.
        format!("{:?}", self.visibility).hash(state);
    }
}

impl ShaderBinding {
    /// Generate the full WGSL `@group(N) @binding(N) var<...> name : type;` declaration.
    pub fn to_wgsl(&self) -> String {
        let prefix = format!("@group({}) @binding({})", self.group, self.binding);
        match &self.resource_type {
            BindingResourceType::UniformBuffer => {
                format!("{} var<uniform> {}: {}", prefix, self.name, self.struct_name_for_buffer())
            }
            BindingResourceType::StorageBuffer { read_only } => {
                let access = if *read_only { "read" } else { "read_write" };
                format!(
                    "{} var<storage, {}> {}: {}",
                    prefix,
                    access,
                    self.name,
                    self.struct_name_for_buffer()
                )
            }
            BindingResourceType::Texture { dimension, sample_type } => {
                let dim = match dimension {
                    TextureDimension::D2 => "texture_2d",
                    TextureDimension::Cube => "texture_cube",
                    TextureDimension::D2Array => "texture_2d_array",
                };
                let st = match sample_type {
                    TextureSampleType::Float => "f32",
                    TextureSampleType::Sint => "i32",
                    TextureSampleType::Uint => "u32",
                    TextureSampleType::Depth => "depth",
                };
                if *sample_type == TextureSampleType::Depth {
                    format!("{} var {}: texture_depth_2d;", prefix, self.name)
                } else {
                    format!("{} var {}: {}<{}>;", prefix, self.name, dim, st)
                }
            }
            BindingResourceType::Sampler(st) => {
                let st_str = match st {
                    SamplerType::Filtering => "sampler",
                    SamplerType::NonFiltering => "sampler",
                    SamplerType::Comparison => "sampler_comparison",
                };
                format!("{} var {}: {};", prefix, self.name, st_str)
            }
        }
    }

    /// Placeholder struct name used in `to_wgsl()` for buffer bindings.
    /// Real code generation will replace this with the actual struct name.
    fn struct_name_for_buffer(&self) -> String {
        // Capitalise first letter of the binding name as a conventional struct name.
        let mut s = self.name.clone();
        if let Some(first) = s.get_mut(0..1) {
            first.make_ascii_uppercase();
        }
        format!("{}Data", s)
    }

    /// Generate the corresponding `wgpu::BindGroupLayoutEntry`.
    pub fn to_bind_group_layout_entry(&self) -> wgpu::BindGroupLayoutEntry {
        let ty = match &self.resource_type {
            BindingResourceType::UniformBuffer => wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            BindingResourceType::StorageBuffer { read_only } => wgpu::BindingType::Buffer {
                ty: if *read_only {
                    wgpu::BufferBindingType::Storage { read_only: true }
                } else {
                    wgpu::BufferBindingType::Storage { read_only: false }
                },
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            BindingResourceType::Texture { dimension, sample_type } => {
                let view_dimension = match dimension {
                    TextureDimension::D2 => wgpu::TextureViewDimension::D2,
                    TextureDimension::Cube => wgpu::TextureViewDimension::Cube,
                    TextureDimension::D2Array => wgpu::TextureViewDimension::D2Array,
                };
                let sample_type = match sample_type {
                    TextureSampleType::Float => {
                        wgpu::TextureSampleType::Float { filterable: true }
                    }
                    TextureSampleType::Sint => wgpu::TextureSampleType::Sint,
                    TextureSampleType::Uint => wgpu::TextureSampleType::Uint,
                    TextureSampleType::Depth => wgpu::TextureSampleType::Depth,
                };
                wgpu::BindingType::Texture {
                    sample_type,
                    view_dimension,
                    multisampled: false,
                }
            }
            BindingResourceType::Sampler(st) => {
                let binding_type = match st {
                    SamplerType::Filtering => wgpu::SamplerBindingType::Filtering,
                    SamplerType::NonFiltering => wgpu::SamplerBindingType::NonFiltering,
                    SamplerType::Comparison => wgpu::SamplerBindingType::Comparison,
                };
                wgpu::BindingType::Sampler(binding_type)
            }
        };

        wgpu::BindGroupLayoutEntry {
            binding: self.binding,
            visibility: self.visibility,
            ty,
            count: None,
        }
    }
}

// ─── WgslFragment ─────────────────────────────────────────────────────────────

/// A fragment of WGSL source code with optional metadata for error reporting.
#[derive(Debug, Clone)]
pub struct WgslFragment {
    pub source: String,
    pub label: Option<String>,
}

impl WgslFragment {
    /// Create an unlabeled fragment.
    pub fn new(source: impl Into<String>) -> Self {
        Self {
            source: source.into(),
            label: None,
        }
    }

    /// Create a labeled fragment (label appears in error messages).
    pub fn labeled(label: impl Into<String>, source: impl Into<String>) -> Self {
        Self {
            source: source.into(),
            label: Some(label.into()),
        }
    }

    /// An empty fragment (no source code).
    pub fn empty() -> Self {
        Self {
            source: String::new(),
            label: None,
        }
    }

    /// Concatenate two fragments with a newline separator.
    pub fn concat(&self, other: &WgslFragment) -> WgslFragment {
        if self.source.is_empty() {
            return other.clone();
        }
        if other.source.is_empty() {
            return self.clone();
        }
        WgslFragment {
            source: format!("{}\n{}", self.source, other.source),
            label: self.label.clone().or_else(|| other.label.clone()),
        }
    }

    /// Indent every line by `spaces` spaces.
    pub fn indent(&self, spaces: usize) -> WgslFragment {
        let prefix: String = " ".repeat(spaces);
        let indented = self
            .source
            .lines()
            .map(|line| {
                if line.is_empty() {
                    String::new()
                } else {
                    format!("{}{}", prefix, line)
                }
            })
            .collect::<Vec<_>>()
            .join("\n");
        WgslFragment {
            source: indented,
            label: self.label.clone(),
        }
    }

    /// Returns `true` if this fragment contains no source code.
    pub fn is_empty(&self) -> bool {
        self.source.trim().is_empty()
    }
}

impl std::fmt::Display for WgslFragment {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", self.source)
    }
}

// ─── Stream types used by ShaderModuleDef ─────────────────────────────────────

// Forward-declared: full definition lives in `stream.rs`.
// Re-exported here so trait signatures compile without importing `stream`.
pub use crate::stream::{StreamDeclaration, StreamSemantic, StreamStage};

// ─── ShaderModuleDef trait ────────────────────────────────────────────────────

/// Core trait: a shader module that can participate in composition.
///
/// Implement this trait for each logical piece of shader code (lighting model,
/// skinning, material, etc.). The compositor reads these methods to produce a
/// single unified WGSL shader.
pub trait ShaderModuleDef: Send + Sync {
    /// Unique name of this module.
    fn name(&self) -> &str;

    /// Stream declarations this module requires as input.
    fn input_streams(&self) -> Vec<StreamDeclaration> {
        vec![]
    }

    /// Stream declarations this module produces as output.
    fn output_streams(&self) -> Vec<StreamDeclaration> {
        vec![]
    }

    /// Resource bindings this module requires.
    fn bindings(&self) -> Vec<ShaderBinding> {
        vec![]
    }

    /// Struct definitions this module contributes.
    fn structs(&self) -> Vec<StructDef> {
        vec![]
    }

    /// Function definitions this module contributes.
    fn functions(&self) -> Vec<FunctionDef> {
        vec![]
    }

    /// Vertex stage body code (without the entry-point wrapper).
    fn vertex_body(&self) -> Option<WgslFragment> {
        None
    }

    /// Fragment stage body code (without the entry-point wrapper).
    fn fragment_body(&self) -> Option<WgslFragment> {
        None
    }

    /// Compute stage body code.
    fn compute_body(&self) -> Option<WgslFragment> {
        None
    }

    /// Module names this module depends on (used for topological ordering).
    fn dependencies(&self) -> Vec<String> {
        vec![]
    }
}

// ─── StructDef ────────────────────────────────────────────────────────────────

/// A WGSL `struct` definition.
#[derive(Debug, Clone)]
pub struct StructDef {
    pub name: String,
    pub fields: Vec<StructField>,
}

/// A single field inside a WGSL struct.
#[derive(Debug, Clone)]
pub struct StructField {
    pub name: String,
    pub ty: WgslType,
    pub attributes: Vec<StructFieldAttribute>,
}

/// An attribute applied to a struct field (`@location`, `@builtin`, etc.).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StructFieldAttribute {
    /// `@location(N)`
    Location(u32),
    /// `@builtin(name)` — e.g. `"position"`, `"vertex_index"`
    Builtin(String),
    /// `@align(N)`
    Align(u32),
    /// `@size(N)`
    Size(u32),
}

impl StructFieldAttribute {
    /// Emit the WGSL attribute string (e.g. `@location(0)`).
    pub fn to_wgsl(&self) -> String {
        match self {
            Self::Location(n) => format!("@location({})", n),
            Self::Builtin(name) => format!("@builtin({})", name),
            Self::Align(n) => format!("@align({})", n),
            Self::Size(n) => format!("@size({})", n),
        }
    }
}

impl StructDef {
    /// Generate valid WGSL source for this struct.
    ///
    /// ```wgsl
    /// struct VertexInput {
    ///     @location(0) position: vec3<f32>,
    ///     @location(1) normal: vec3<f32>,
    /// }
    /// ```
    pub fn to_wgsl(&self) -> String {
        let mut out = format!("struct {} {{\n", self.name);
        for field in &self.fields {
            let attrs = if field.attributes.is_empty() {
                String::new()
            } else {
                let parts: Vec<String> =
                    field.attributes.iter().map(|a| a.to_wgsl()).collect();
                format!("{} ", parts.join(" "))
            };
            out.push_str(&format!(
                "    {}{}: {},\n",
                attrs,
                field.name,
                field.ty.to_wgsl()
            ));
        }
        out.push_str("}\n");
        out
    }
}

// ─── FunctionDef ──────────────────────────────────────────────────────────────

/// A WGSL function definition.
#[derive(Debug, Clone)]
pub struct FunctionDef {
    pub name: String,
    pub parameters: Vec<(String, WgslType)>,
    pub return_type: Option<WgslType>,
    pub body: WgslFragment,
    /// If `true`, this function can be overridden by another module during composition.
    pub overridable: bool,
}

impl FunctionDef {
    /// Generate valid WGSL source for this function.
    pub fn to_wgsl(&self) -> String {
        let params: Vec<String> = self
            .parameters
            .iter()
            .map(|(name, ty)| format!("{}: {}", name, ty.to_wgsl()))
            .collect();
        let ret = match &self.return_type {
            Some(ty) => format!(" -> {}", ty.to_wgsl()),
            None => String::new(),
        };
        format!(
            "fn {}({}){} {{\n{}\n}}\n",
            self.name,
            params.join(", "),
            ret,
            self.body.indent(4)
        )
    }
}

// ─── Errors ───────────────────────────────────────────────────────────────────

/// Errors that can occur during shader framework operations.
#[derive(Debug, thiserror::Error)]
pub enum ShaderError {
    #[error("Binding conflict: {group}:{binding} used by both '{existing}' and '{new}'")]
    BindingConflict {
        group: u32,
        binding: u32,
        existing: String,
        new: String,
    },

    #[error("Stream mismatch: output stream '{name}' has no matching input")]
    StreamMismatch { name: String },

    #[error("Function conflict: '{name}' defined in both '{module_a}' and '{module_b}'")]
    FunctionConflict {
        name: String,
        module_a: String,
        module_b: String,
    },

    #[error("Missing dependency: module '{module}' requires '{dependency}' but it was not found")]
    MissingDependency {
        module: String,
        dependency: String,
    },

    #[error("Composition error: {message}")]
    Composition { message: String },

    #[error("WGSL validation error: {message}")]
    WgslValidation { message: String },

    #[error("Variant error: {message}")]
    Variant { message: String },

    #[error("Effect error: {message}")]
    Effect { message: String },

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

/// Convenience result type for shader operations.
pub type ShaderResult<T> = Result<T, ShaderError>;

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wgsl_scalar_type_to_wgsl() {
        assert_eq!(WgslScalarType::F32.to_wgsl(), "f32");
        assert_eq!(WgslScalarType::I32.to_wgsl(), "i32");
        assert_eq!(WgslScalarType::U32.to_wgsl(), "u32");
        assert_eq!(WgslScalarType::Bool.to_wgsl(), "bool");
    }

    #[test]
    fn test_wgsl_type_to_wgsl_scalars() {
        assert_eq!(WgslType::Bool.to_wgsl(), "bool");
        assert_eq!(WgslType::I32.to_wgsl(), "i32");
        assert_eq!(WgslType::U32.to_wgsl(), "u32");
        assert_eq!(WgslType::F32.to_wgsl(), "f32");
    }

    #[test]
    fn test_wgsl_type_to_wgsl_vectors() {
        assert_eq!(WgslType::Vec2(WgslScalarType::F32).to_wgsl(), "vec2<f32>");
        assert_eq!(WgslType::Vec3(WgslScalarType::F32).to_wgsl(), "vec3<f32>");
        assert_eq!(WgslType::Vec4(WgslScalarType::F32).to_wgsl(), "vec4<f32>");
        assert_eq!(WgslType::Vec4(WgslScalarType::I32).to_wgsl(), "vec4<i32>");
        assert_eq!(WgslType::Vec2(WgslScalarType::U32).to_wgsl(), "vec2<u32>");
    }

    #[test]
    fn test_wgsl_type_to_wgsl_matrices() {
        assert_eq!(WgslType::Mat2x2(WgslScalarType::F32).to_wgsl(), "mat2x2<f32>");
        assert_eq!(WgslType::Mat3x3(WgslScalarType::F32).to_wgsl(), "mat3x3<f32>");
        assert_eq!(WgslType::Mat4x4(WgslScalarType::F32).to_wgsl(), "mat4x4<f32>");
    }

    #[test]
    fn test_wgsl_type_to_wgsl_arrays() {
        let arr = WgslType::Array(Box::new(WgslType::Vec4(WgslScalarType::F32)), Some(16));
        assert_eq!(arr.to_wgsl(), "array<vec4<f32>, 16>");

        let dyn_arr = WgslType::Array(Box::new(WgslType::F32), None);
        assert_eq!(dyn_arr.to_wgsl(), "array<f32>");
    }

    #[test]
    fn test_wgsl_type_to_wgsl_special() {
        assert_eq!(WgslType::Struct("MyStruct".into()).to_wgsl(), "MyStruct");
        assert_eq!(WgslType::Sampler.to_wgsl(), "sampler");
        assert_eq!(WgslType::Texture2d.to_wgsl(), "texture_2d<f32>");
        assert_eq!(WgslType::TextureCube.to_wgsl(), "texture_cube<f32>");
        assert_eq!(WgslType::Custom("my_type".into()).to_wgsl(), "my_type");
    }

    #[test]
    fn test_wgsl_type_byte_size() {
        assert_eq!(WgslType::F32.byte_size(), 4);
        assert_eq!(WgslType::I32.byte_size(), 4);
        assert_eq!(WgslType::Bool.byte_size(), 4);
        assert_eq!(WgslType::Vec2(WgslScalarType::F32).byte_size(), 8);
        assert_eq!(WgslType::Vec3(WgslScalarType::F32).byte_size(), 12);
        assert_eq!(WgslType::Vec4(WgslScalarType::F32).byte_size(), 16);
        assert_eq!(WgslType::Mat4x4(WgslScalarType::F32).byte_size(), 64);
        assert_eq!(WgslType::Mat3x3(WgslScalarType::F32).byte_size(), 36);
        assert_eq!(WgslType::Mat2x2(WgslScalarType::F32).byte_size(), 16);
        assert_eq!(
            WgslType::Array(Box::new(WgslType::F32), Some(10)).byte_size(),
            40
        );
        assert_eq!(WgslType::Array(Box::new(WgslType::F32), None).byte_size(), 0);
        assert_eq!(WgslType::Struct("Foo".into()).byte_size(), 0);
        assert_eq!(WgslType::Sampler.byte_size(), 0);
    }

    #[test]
    fn test_wgsl_type_to_vertex_format() {
        assert_eq!(
            WgslType::F32.to_vertex_format(),
            Some(wgpu::VertexFormat::Float32)
        );
        assert_eq!(
            WgslType::Vec3(WgslScalarType::F32).to_vertex_format(),
            Some(wgpu::VertexFormat::Float32x3)
        );
        assert_eq!(
            WgslType::Vec4(WgslScalarType::U32).to_vertex_format(),
            Some(wgpu::VertexFormat::Uint32x4)
        );
        assert_eq!(
            WgslType::Vec2(WgslScalarType::I32).to_vertex_format(),
            Some(wgpu::VertexFormat::Sint32x2)
        );
        // Types without vertex format equivalents
        assert!(WgslType::Mat4x4(WgslScalarType::F32).to_vertex_format().is_none());
        assert!(WgslType::Struct("Foo".into()).to_vertex_format().is_none());
        assert!(WgslType::Sampler.to_vertex_format().is_none());
        assert!(WgslType::Texture2d.to_vertex_format().is_none());
        assert!(WgslType::Bool.to_vertex_format().is_none());
    }

    #[test]
    fn test_wgsl_fragment_new_and_display() {
        let f = WgslFragment::new("let x = 1.0;");
        assert_eq!(f.to_string(), "let x = 1.0;");
        assert!(f.label.is_none());
        assert!(!f.is_empty());
    }

    #[test]
    fn test_wgsl_fragment_labeled() {
        let f = WgslFragment::labeled("lighting", "let ndotl = dot(n, l);");
        assert_eq!(f.label.as_deref(), Some("lighting"));
        assert_eq!(f.source, "let ndotl = dot(n, l);");
    }

    #[test]
    fn test_wgsl_fragment_empty() {
        let f = WgslFragment::empty();
        assert!(f.is_empty());
        assert_eq!(f.source, "");
    }

    #[test]
    fn test_wgsl_fragment_concat() {
        let a = WgslFragment::new("let x = 1.0;");
        let b = WgslFragment::new("let y = 2.0;");
        let c = a.concat(&b);
        assert_eq!(c.source, "let x = 1.0;\nlet y = 2.0;");

        // Concatenating with empty returns the non-empty one.
        let empty = WgslFragment::empty();
        assert_eq!(a.concat(&empty).source, a.source);
        assert_eq!(empty.concat(&a).source, a.source);
    }

    #[test]
    fn test_wgsl_fragment_indent() {
        let f = WgslFragment::new("line1\nline2\n\nline3");
        let indented = f.indent(4);
        assert_eq!(indented.source, "    line1\n    line2\n\n    line3");
    }

    #[test]
    fn test_struct_def_to_wgsl() {
        let s = StructDef {
            name: "VertexInput".into(),
            fields: vec![
                StructField {
                    name: "position".into(),
                    ty: WgslType::Vec3(WgslScalarType::F32),
                    attributes: vec![StructFieldAttribute::Location(0)],
                },
                StructField {
                    name: "normal".into(),
                    ty: WgslType::Vec3(WgslScalarType::F32),
                    attributes: vec![StructFieldAttribute::Location(1)],
                },
            ],
        };
        let wgsl = s.to_wgsl();
        assert!(wgsl.contains("struct VertexInput {"));
        assert!(wgsl.contains("    @location(0) position: vec3<f32>,"));
        assert!(wgsl.contains("    @location(1) normal: vec3<f32>,"));
        assert!(wgsl.trim_end().ends_with('}'));
    }

    #[test]
    fn test_struct_def_with_builtin() {
        let s = StructDef {
            name: "VsOutput".into(),
            fields: vec![
                StructField {
                    name: "clip_position".into(),
                    ty: WgslType::Vec4(WgslScalarType::F32),
                    attributes: vec![StructFieldAttribute::Builtin("position".into())],
                },
                StructField {
                    name: "uv".into(),
                    ty: WgslType::Vec2(WgslScalarType::F32),
                    attributes: vec![StructFieldAttribute::Location(0)],
                },
            ],
        };
        let wgsl = s.to_wgsl();
        assert!(wgsl.contains("@builtin(position) clip_position: vec4<f32>"));
        assert!(wgsl.contains("@location(0) uv: vec2<f32>"));
    }

    #[test]
    fn test_function_def_to_wgsl_no_params() {
        let f = FunctionDef {
            name: "get_color".into(),
            parameters: vec![],
            return_type: Some(WgslType::Vec4(WgslScalarType::F32)),
            body: WgslFragment::new("return vec4<f32>(1.0, 0.0, 0.0, 1.0);"),
            overridable: false,
        };
        let wgsl = f.to_wgsl();
        assert!(wgsl.starts_with("fn get_color()"));
        assert!(wgsl.contains("-> vec4<f32>"));
        assert!(wgsl.contains("    return vec4<f32>(1.0, 0.0, 0.0, 1.0);"));
        assert!(wgsl.trim_end().ends_with('}'));
    }

    #[test]
    fn test_function_def_to_wgsl_with_params() {
        let f = FunctionDef {
            name: "compute_light".into(),
            parameters: vec![
                ("normal".into(), WgslType::Vec3(WgslScalarType::F32)),
                ("light_dir".into(), WgslType::Vec3(WgslScalarType::F32)),
            ],
            return_type: Some(WgslType::F32),
            body: WgslFragment::new("return max(dot(normal, light_dir), 0.0);"),
            overridable: true,
        };
        let wgsl = f.to_wgsl();
        assert!(wgsl.contains("fn compute_light(normal: vec3<f32>, light_dir: vec3<f32>)"));
        assert!(wgsl.contains("-> f32"));
    }

    #[test]
    fn test_function_def_to_wgsl_void_return() {
        let f = FunctionDef {
            name: "do_stuff".into(),
            parameters: vec![("x".into(), WgslType::F32)],
            return_type: None,
            body: WgslFragment::new("let y = x * 2.0;"),
            overridable: false,
        };
        let wgsl = f.to_wgsl();
        assert!(wgsl.contains("fn do_stuff(x: f32) {"));
        assert!(!wgsl.contains("->"));
    }

    #[test]
    fn test_binding_to_wgsl_uniform() {
        let b = ShaderBinding {
            group: 0,
            binding: 0,
            name: "camera".into(),
            resource_type: BindingResourceType::UniformBuffer,
            visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
        };
        let wgsl = b.to_wgsl();
        assert_eq!(wgsl, "@group(0) @binding(0) var<uniform> camera: CameraData");
    }

    #[test]
    fn test_binding_to_wgsl_storage() {
        let b = ShaderBinding {
            group: 1,
            binding: 2,
            name: "bones".into(),
            resource_type: BindingResourceType::StorageBuffer { read_only: true },
            visibility: wgpu::ShaderStages::VERTEX,
        };
        let wgsl = b.to_wgsl();
        assert_eq!(wgsl, "@group(1) @binding(2) var<storage, read> bones: BonesData");
    }

    #[test]
    fn test_binding_to_wgsl_texture() {
        let b = ShaderBinding {
            group: 1,
            binding: 0,
            name: "albedo_tex".into(),
            resource_type: BindingResourceType::Texture {
                dimension: TextureDimension::D2,
                sample_type: TextureSampleType::Float,
            },
            visibility: wgpu::ShaderStages::FRAGMENT,
        };
        let wgsl = b.to_wgsl();
        assert_eq!(wgsl, "@group(1) @binding(0) var albedo_tex: texture_2d<f32>;");
    }

    #[test]
    fn test_binding_to_wgsl_depth_texture() {
        let b = ShaderBinding {
            group: 0,
            binding: 3,
            name: "shadow_map".into(),
            resource_type: BindingResourceType::Texture {
                dimension: TextureDimension::D2,
                sample_type: TextureSampleType::Depth,
            },
            visibility: wgpu::ShaderStages::FRAGMENT,
        };
        let wgsl = b.to_wgsl();
        assert_eq!(wgsl, "@group(0) @binding(3) var shadow_map: texture_depth_2d;");
    }

    #[test]
    fn test_binding_to_wgsl_sampler() {
        let b = ShaderBinding {
            group: 1,
            binding: 1,
            name: "tex_sampler".into(),
            resource_type: BindingResourceType::Sampler(SamplerType::Filtering),
            visibility: wgpu::ShaderStages::FRAGMENT,
        };
        let wgsl = b.to_wgsl();
        assert_eq!(wgsl, "@group(1) @binding(1) var tex_sampler: sampler;");
    }

    #[test]
    fn test_binding_to_wgsl_comparison_sampler() {
        let b = ShaderBinding {
            group: 0,
            binding: 4,
            name: "shadow_sampler".into(),
            resource_type: BindingResourceType::Sampler(SamplerType::Comparison),
            visibility: wgpu::ShaderStages::FRAGMENT,
        };
        let wgsl = b.to_wgsl();
        assert_eq!(wgsl, "@group(0) @binding(4) var shadow_sampler: sampler_comparison;");
    }

    #[test]
    fn test_struct_field_attribute_to_wgsl() {
        assert_eq!(StructFieldAttribute::Location(3).to_wgsl(), "@location(3)");
        assert_eq!(
            StructFieldAttribute::Builtin("vertex_index".into()).to_wgsl(),
            "@builtin(vertex_index)"
        );
        assert_eq!(StructFieldAttribute::Align(16).to_wgsl(), "@align(16)");
        assert_eq!(StructFieldAttribute::Size(32).to_wgsl(), "@size(32)");
    }
}
