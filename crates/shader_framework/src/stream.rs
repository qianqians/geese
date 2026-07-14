//! Stream type system — vertex data flowing through shader stages.
//!
//! A "stream" represents a typed channel of data (position, normal, UV, etc.)
//! that passes from the vertex stage through to the fragment stage. The
//! [`StreamRouter`] manages location assignment and generates matching
//! VS-output / FS-input WGSL structs automatically.

use crate::core::*;

// ─── Stream Semantic ──────────────────────────────────────────────────────────

/// Semantic meaning of a vertex stream.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum StreamSemantic {
    Position,
    Normal,
    Tangent,
    /// UV channel with index (e.g. UV(0), UV(1)).
    UV(u32),
    /// Vertex color channel with index.
    Color(u32),
    BoneWeights,
    BoneIndices,
    /// Application-defined semantic.
    Custom(String),
}

// ─── Stream Stage ─────────────────────────────────────────────────────────────

/// Which shader stages consume this stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StreamStage {
    /// Only used in the vertex stage (not passed to fragment).
    VertexOnly,
    /// Written by VS and read by FS (interpolated).
    VertexToFragment,
    /// Only used in the fragment stage.
    FragmentOnly,
    /// Available in all stages.
    AllStages,
}

// ─── Stream Declaration ───────────────────────────────────────────────────────

/// A single stream declaration describing one channel of vertex/fragment data.
#[derive(Debug, Clone)]
pub struct StreamDeclaration {
    /// The semantic meaning (Position, Normal, UV(n), etc.).
    pub semantic: StreamSemantic,
    /// The WGSL type carried by this stream.
    pub wgsl_type: WgslType,
    /// Location index for the `@location(N)` attribute.
    /// If `None`, [`StreamRouter::add_stream`] will auto-assign one.
    pub location: Option<u32>,
    /// Variable name used in shader code (e.g. `"position"`, `"world_normal"`).
    pub name: String,
    /// Which shader stages use this stream.
    pub stage: StreamStage,
}

// ─── Stream Router ────────────────────────────────────────────────────────────

/// Manages stream routing: location assignment and VS↔FS matching.
///
/// The router assigns unique `@location(N)` indices to each stream,
/// ensures no duplicates or conflicts, and can emit the VS input,
/// VS output, and FS input WGSL structs that wire everything together.
#[derive(Debug, Clone)]
pub struct StreamRouter {
    streams: Vec<StreamDeclaration>,
    next_location: u32,
}

impl StreamRouter {
    /// Create a new empty router.
    pub fn new() -> Self {
        Self {
            streams: Vec::new(),
            next_location: 0,
        }
    }

    /// Add a stream declaration, auto-assigning a location if `location` is `None`.
    ///
    /// Returns the assigned location.
    ///
    /// # Errors
    /// Returns [`ShaderError::Composition`] if a stream with the same semantic
    /// is already registered.
    pub fn add_stream(&mut self, mut decl: StreamDeclaration) -> ShaderResult<u32> {
        // Reject duplicate semantics.
        if self.streams.iter().any(|s| s.semantic == decl.semantic) {
            return Err(ShaderError::Composition {
                message: format!(
                    "Duplicate stream semantic: {:?}",
                    decl.semantic
                ),
            });
        }

        let loc = match decl.location {
            Some(l) => {
                // Validate no location conflict with existing streams.
                if self.streams.iter().any(|s| s.location == Some(l)) {
                    return Err(ShaderError::Composition {
                        message: format!("Location {} is already in use", l),
                    });
                }
                // Advance next_location past any manually-assigned locations.
                if l >= self.next_location {
                    self.next_location = l + 1;
                }
                l
            }
            None => {
                let l = self.next_location;
                self.next_location += 1;
                decl.location = Some(l);
                l
            }
        };

        self.streams.push(decl);
        Ok(loc)
    }

    /// Add multiple streams at once, returning the assigned locations.
    pub fn add_streams(&mut self, decls: Vec<StreamDeclaration>) -> ShaderResult<Vec<u32>> {
        decls.into_iter().map(|d| self.add_stream(d)).collect()
    }

    /// All registered streams.
    pub fn streams(&self) -> &[StreamDeclaration] {
        &self.streams
    }

    /// Generate `wgpu::VertexAttribute` descriptors for vertex-input streams.
    ///
    /// Only streams with `VertexOnly`, `VertexToFragment`, or `AllStages` are
    /// included (i.e. anything the vertex stage reads).  Fragment-only streams
    /// are skipped.
    ///
    /// `offset` is computed cumulatively based on each attribute's byte size.
    pub fn vertex_buffer_layout(&self) -> Vec<wgpu::VertexAttribute> {
        let mut attrs = Vec::new();
        let mut offset: u64 = 0;
        for decl in &self.streams {
            if decl.stage == StreamStage::FragmentOnly {
                continue;
            }
            if let (Some(loc), Some(fmt)) = (decl.location, decl.wgsl_type.to_vertex_format()) {
                attrs.push(wgpu::VertexAttribute {
                    format: fmt,
                    offset,
                    shader_location: loc,
                });
                offset += decl.wgsl_type.byte_size();
            }
        }
        attrs
    }

    /// Total byte stride of all vertex-input attributes.
    pub fn vertex_stride(&self) -> u64 {
        self.streams
            .iter()
            .filter(|d| d.stage != StreamStage::FragmentOnly)
            .map(|d| d.wgsl_type.byte_size())
            .sum()
    }

    /// Generate the WGSL VS input struct.
    ///
    /// Includes all streams that the vertex stage reads
    /// (`VertexOnly`, `VertexToFragment`, `AllStages`).
    pub fn generate_vs_input_struct(&self, name: &str) -> String {
        let fields: Vec<_> = self
            .streams
            .iter()
            .filter(|d| d.stage != StreamStage::FragmentOnly)
            .collect();
        self.generate_struct_wgsl(name, &fields, false)
    }

    /// Generate the WGSL VS output struct.
    ///
    /// Includes `VertexToFragment` and `AllStages` streams as `@location(N)` fields.
    /// Also prepends the built-in `@builtin(position) clip_position: vec4<f32>` field.
    pub fn generate_vs_output_struct(&self, name: &str) -> String {
        let fields: Vec<_> = self
            .streams
            .iter()
            .filter(|d| matches!(d.stage, StreamStage::VertexToFragment | StreamStage::AllStages))
            .collect();
        self.generate_struct_wgsl(name, &fields, true)
    }

    /// Generate the WGSL FS input struct.
    ///
    /// Should match the VS output exactly (same fields, same locations).
    pub fn generate_fs_input_struct(&self, name: &str) -> String {
        // Identical fields to VS output.
        self.generate_vs_output_struct(name)
    }

    /// Validate that all streams are consistent:
    /// - No location conflicts
    /// - No duplicate semantics
    pub fn validate(&self) -> ShaderResult<()> {
        let mut seen_semantics = std::collections::HashSet::new();
        let mut seen_locations = std::collections::HashMap::new();

        for decl in &self.streams {
            // Semantic uniqueness.
            if !seen_semantics.insert(decl.semantic.clone()) {
                return Err(ShaderError::Composition {
                    message: format!("Duplicate stream semantic: {:?}", decl.semantic),
                });
            }
            // Location uniqueness (only among streams that actually have a location).
            if let Some(loc) = decl.location {
                if let Some(prev_name) = seen_locations.insert(loc, &decl.name) {
                    return Err(ShaderError::Composition {
                        message: format!(
                            "Location {} conflict: '{}' and '{}'",
                            loc, prev_name, decl.name
                        ),
                    });
                }
            }
        }
        Ok(())
    }

    /// Merge streams from another router into this one.
    ///
    /// Streams with a semantic already present in `self` are skipped.
    /// Returns the newly assigned locations for the streams that were added.
    pub fn merge(&mut self, other: &StreamRouter) -> ShaderResult<Vec<u32>> {
        let mut locations = Vec::new();
        for decl in &other.streams {
            if self.streams.iter().any(|s| s.semantic == decl.semantic) {
                // Already present — skip.
                continue;
            }
            let mut cloned = decl.clone();
            cloned.location = None; // force re-assignment to avoid conflicts
            let loc = self.add_stream(cloned)?;
            locations.push(loc);
        }
        Ok(locations)
    }

    /// Look up a stream by its semantic.
    pub fn get_by_semantic(&self, semantic: &StreamSemantic) -> Option<&StreamDeclaration> {
        self.streams.iter().find(|s| &s.semantic == semantic)
    }

    // ─── private helpers ──────────────────────────────────────────────────────

    /// Build a WGSL struct string from a list of stream declarations.
    ///
    /// If `include_clip_position` is true, a `@builtin(position)` field is
    /// prepended (used for VS output / FS input structs).
    fn generate_struct_wgsl(
        &self,
        name: &str,
        fields: &[&StreamDeclaration],
        include_clip_position: bool,
    ) -> String {
        let mut out = format!("struct {} {{\n", name);
        if include_clip_position {
            out.push_str("    @builtin(position) clip_position: vec4<f32>,\n");
        }
        for decl in fields {
            if let Some(loc) = decl.location {
                out.push_str(&format!(
                    "    @location({}) {}: {},\n",
                    loc,
                    decl.name,
                    decl.wgsl_type.to_wgsl()
                ));
            }
        }
        out.push_str("}\n");
        out
    }
}

impl Default for StreamRouter {
    fn default() -> Self {
        Self::new()
    }
}

// ─── Presets ──────────────────────────────────────────────────────────────────

/// Helper functions for creating common stream declarations.
pub mod presets {
    use super::*;

    /// Object-space position (vec3<f32>).
    pub fn position() -> StreamDeclaration {
        StreamDeclaration {
            semantic: StreamSemantic::Position,
            wgsl_type: WgslType::Vec3(WgslScalarType::F32),
            location: None,
            name: "position".to_string(),
            stage: StreamStage::VertexToFragment,
        }
    }

    /// Object-space normal (vec3<f32>).
    pub fn normal() -> StreamDeclaration {
        StreamDeclaration {
            semantic: StreamSemantic::Normal,
            wgsl_type: WgslType::Vec3(WgslScalarType::F32),
            location: None,
            name: "normal".to_string(),
            stage: StreamStage::VertexToFragment,
        }
    }

    /// Tangent vector with handedness in W (vec4<f32>).
    pub fn tangent() -> StreamDeclaration {
        StreamDeclaration {
            semantic: StreamSemantic::Tangent,
            wgsl_type: WgslType::Vec4(WgslScalarType::F32),
            location: None,
            name: "tangent".to_string(),
            stage: StreamStage::VertexToFragment,
        }
    }

    /// Texture coordinate channel `n` (vec2<f32>).
    pub fn uv(channel: u32) -> StreamDeclaration {
        StreamDeclaration {
            semantic: StreamSemantic::UV(channel),
            wgsl_type: WgslType::Vec2(WgslScalarType::F32),
            location: None,
            name: format!("uv{}", channel),
            stage: StreamStage::VertexToFragment,
        }
    }

    /// Vertex color channel `n` (vec4<f32>).
    pub fn color(channel: u32) -> StreamDeclaration {
        StreamDeclaration {
            semantic: StreamSemantic::Color(channel),
            wgsl_type: WgslType::Vec4(WgslScalarType::F32),
            location: None,
            name: format!("color{}", channel),
            stage: StreamStage::VertexToFragment,
        }
    }

    /// Bone weights for skeletal animation (vec4<f32>).
    pub fn bone_weights() -> StreamDeclaration {
        StreamDeclaration {
            semantic: StreamSemantic::BoneWeights,
            wgsl_type: WgslType::Vec4(WgslScalarType::F32),
            location: None,
            name: "bone_weights".to_string(),
            stage: StreamStage::VertexOnly,
        }
    }

    /// Bone indices for skeletal animation (vec4<u32>).
    pub fn bone_indices() -> StreamDeclaration {
        StreamDeclaration {
            semantic: StreamSemantic::BoneIndices,
            wgsl_type: WgslType::Vec4(WgslScalarType::U32),
            location: None,
            name: "bone_indices".to_string(),
            stage: StreamStage::VertexOnly,
        }
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::presets;
    use super::*;

    #[test]
    fn test_router_add_stream_auto_location() {
        let mut router = StreamRouter::new();
        let loc0 = router.add_stream(presets::position()).unwrap();
        let loc1 = router.add_stream(presets::normal()).unwrap();
        assert_eq!(loc0, 0);
        assert_eq!(loc1, 1);
        assert_eq!(router.streams().len(), 2);
    }

    #[test]
    fn test_router_add_stream_manual_location() {
        let mut router = StreamRouter::new();
        let mut decl = presets::position();
        decl.location = Some(5);
        let loc = router.add_stream(decl).unwrap();
        assert_eq!(loc, 5);
        // Next auto-assigned should be 6.
        let loc2 = router.add_stream(presets::normal()).unwrap();
        assert_eq!(loc2, 6);
    }

    #[test]
    fn test_router_duplicate_semantic_rejected() {
        let mut router = StreamRouter::new();
        router.add_stream(presets::position()).unwrap();
        let err = router.add_stream(presets::position());
        assert!(err.is_err());
    }

    #[test]
    fn test_router_location_conflict_rejected() {
        let mut router = StreamRouter::new();
        let mut d1 = presets::position();
        d1.location = Some(3);
        router.add_stream(d1).unwrap();

        let mut d2 = presets::normal();
        d2.location = Some(3);
        let err = router.add_stream(d2);
        assert!(err.is_err());
    }

    #[test]
    fn test_router_add_streams_batch() {
        let mut router = StreamRouter::new();
        let locs = router
            .add_streams(vec![presets::position(), presets::normal(), presets::uv(0)])
            .unwrap();
        assert_eq!(locs, vec![0, 1, 2]);
        assert_eq!(router.streams().len(), 3);
    }

    #[test]
    fn test_router_vertex_buffer_layout() {
        let mut router = StreamRouter::new();
        router.add_stream(presets::position()).unwrap(); // vec3<f32> → Float32x3
        router.add_stream(presets::uv(0)).unwrap(); // vec2<f32> → Float32x2

        let attrs = router.vertex_buffer_layout();
        assert_eq!(attrs.len(), 2);
        assert_eq!(attrs[0].format, wgpu::VertexFormat::Float32x3);
        assert_eq!(attrs[0].offset, 0);
        assert_eq!(attrs[0].shader_location, 0);
        assert_eq!(attrs[1].format, wgpu::VertexFormat::Float32x2);
        assert_eq!(attrs[1].offset, 12); // 3 * 4 bytes
        assert_eq!(attrs[1].shader_location, 1);
    }

    #[test]
    fn test_router_vertex_stride() {
        let mut router = StreamRouter::new();
        router.add_stream(presets::position()).unwrap(); // 12 bytes
        router.add_stream(presets::normal()).unwrap(); // 12 bytes
        router.add_stream(presets::uv(0)).unwrap(); // 8 bytes
        assert_eq!(router.vertex_stride(), 32);
    }

    #[test]
    fn test_router_vertex_only_streams_in_layout() {
        let mut router = StreamRouter::new();
        router.add_stream(presets::position()).unwrap();
        router.add_stream(presets::bone_weights()).unwrap(); // VertexOnly
        router.add_stream(presets::bone_indices()).unwrap(); // VertexOnly

        let attrs = router.vertex_buffer_layout();
        // All three are in the vertex input (bone data is vertex-only, still a VS input).
        assert_eq!(attrs.len(), 3);
    }

    #[test]
    fn test_router_fragment_only_excluded_from_vertex_layout() {
        let mut router = StreamRouter::new();
        router.add_stream(presets::position()).unwrap();
        // Add a fragment-only stream.
        router
            .add_stream(StreamDeclaration {
                semantic: StreamSemantic::Custom("screen_uv".into()),
                wgsl_type: WgslType::Vec2(WgslScalarType::F32),
                location: None,
                name: "screen_uv".into(),
                stage: StreamStage::FragmentOnly,
            })
            .unwrap();

        let attrs = router.vertex_buffer_layout();
        assert_eq!(attrs.len(), 1); // only position
    }

    #[test]
    fn test_router_generate_vs_input_struct() {
        let mut router = StreamRouter::new();
        router.add_stream(presets::position()).unwrap();
        router.add_stream(presets::normal()).unwrap();

        let wgsl = router.generate_vs_input_struct("VertexInput");
        assert!(wgsl.contains("struct VertexInput {"));
        assert!(wgsl.contains("@location(0) position: vec3<f32>"));
        assert!(wgsl.contains("@location(1) normal: vec3<f32>"));
        // Should NOT contain clip_position (that's only in output).
        assert!(!wgsl.contains("clip_position"));
    }

    #[test]
    fn test_router_generate_vs_output_struct() {
        let mut router = StreamRouter::new();
        router.add_stream(presets::position()).unwrap();
        router.add_stream(presets::normal()).unwrap();
        router.add_stream(presets::bone_weights()).unwrap(); // VertexOnly → excluded

        let wgsl = router.generate_vs_output_struct("VertexOutput");
        assert!(wgsl.contains("struct VertexOutput {"));
        assert!(wgsl.contains("@builtin(position) clip_position: vec4<f32>"));
        assert!(wgsl.contains("@location(0) position: vec3<f32>"));
        assert!(wgsl.contains("@location(1) normal: vec3<f32>"));
        // bone_weights is VertexOnly — should NOT appear in VS output.
        assert!(!wgsl.contains("bone_weights"));
    }

    #[test]
    fn test_router_vs_output_matches_fs_input() {
        let mut router = StreamRouter::new();
        router.add_stream(presets::position()).unwrap();
        router.add_stream(presets::uv(0)).unwrap();

        let vs_out = router.generate_vs_output_struct("Interpolants");
        let fs_in = router.generate_fs_input_struct("Interpolants");
        assert_eq!(vs_out, fs_in);
    }

    #[test]
    fn test_router_validate_ok() {
        let mut router = StreamRouter::new();
        router.add_stream(presets::position()).unwrap();
        router.add_stream(presets::normal()).unwrap();
        assert!(router.validate().is_ok());
    }

    #[test]
    fn test_router_merge() {
        let mut a = StreamRouter::new();
        a.add_stream(presets::position()).unwrap();
        a.add_stream(presets::normal()).unwrap();

        let mut b = StreamRouter::new();
        b.add_stream(presets::position()).unwrap(); // duplicate — should be skipped
        b.add_stream(presets::uv(0)).unwrap(); // new
        b.add_stream(presets::tangent()).unwrap(); // new

        let new_locs = a.merge(&b).unwrap();
        assert_eq!(new_locs.len(), 2); // uv and tangent added
        assert_eq!(a.streams().len(), 4); // position, normal, uv, tangent
    }

    #[test]
    fn test_router_get_by_semantic() {
        let mut router = StreamRouter::new();
        router.add_stream(presets::position()).unwrap();
        router.add_stream(presets::uv(0)).unwrap();

        assert!(router.get_by_semantic(&StreamSemantic::Position).is_some());
        assert!(router.get_by_semantic(&StreamSemantic::UV(0)).is_some());
        assert!(router.get_by_semantic(&StreamSemantic::UV(1)).is_none());
        assert!(router.get_by_semantic(&StreamSemantic::Normal).is_none());
    }

    #[test]
    fn test_presets_types() {
        let p = presets::position();
        assert_eq!(p.semantic, StreamSemantic::Position);
        assert_eq!(p.wgsl_type, WgslType::Vec3(WgslScalarType::F32));
        assert!(p.location.is_none());
        assert_eq!(p.name, "position");
        assert_eq!(p.stage, StreamStage::VertexToFragment);

        let t = presets::tangent();
        assert_eq!(t.wgsl_type, WgslType::Vec4(WgslScalarType::F32));

        let uv1 = presets::uv(1);
        assert_eq!(uv1.semantic, StreamSemantic::UV(1));
        assert_eq!(uv1.name, "uv1");

        let c0 = presets::color(0);
        assert_eq!(c0.semantic, StreamSemantic::Color(0));

        let bw = presets::bone_weights();
        assert_eq!(bw.stage, StreamStage::VertexOnly);

        let bi = presets::bone_indices();
        assert_eq!(bi.wgsl_type, WgslType::Vec4(WgslScalarType::U32));
    }

    #[test]
    fn test_router_with_all_presets() {
        let mut router = StreamRouter::new();
        router.add_stream(presets::position()).unwrap();
        router.add_stream(presets::normal()).unwrap();
        router.add_stream(presets::tangent()).unwrap();
        router.add_stream(presets::uv(0)).unwrap();
        router.add_stream(presets::uv(1)).unwrap();
        router.add_stream(presets::color(0)).unwrap();
        router.add_stream(presets::bone_weights()).unwrap();
        router.add_stream(presets::bone_indices()).unwrap();

        assert!(router.validate().is_ok());
        assert_eq!(router.streams().len(), 8);

        // All locations should be unique and sequential.
        let locs: Vec<u32> = router.streams().iter().map(|s| s.location.unwrap()).collect();
        assert_eq!(locs, vec![0, 1, 2, 3, 4, 5, 6, 7]);
    }

    #[test]
    fn test_empty_router_structs() {
        let router = StreamRouter::new();
        let vs_in = router.generate_vs_input_struct("VsIn");
        let vs_out = router.generate_vs_output_struct("VsOut");
        assert_eq!(vs_in, "struct VsIn {\n}\n");
        // VS output still includes clip_position even with no user streams.
        assert!(vs_out.contains("@builtin(position) clip_position: vec4<f32>"));
    }
}
