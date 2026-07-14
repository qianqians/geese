//! Effect system (SDFX equivalent) — declarative render pass composition.
//!
//! An "effect" binds composed shaders to render-pass definitions, producing
//! compiled WGSL for every pass. Effects can be loaded from TOML/JSON files
//! or built programmatically via [`EffectBuilder`].

use crate::compose::*;
use crate::core::*;
use crate::generator::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

// ─── Pass Type ───────────────────────────────────────────────────────────────

/// Type of render pass.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PassType {
    /// Geometry pass (writes G-buffer or renders forward).
    Geometry,
    /// Lighting pass (computes lighting from G-buffer or forward).
    Lighting,
    /// Shadow depth pass.
    Shadow,
    /// Post-processing fullscreen pass.
    PostProcess,
    /// Compute shader pass.
    Compute,
    /// Custom pass type.
    Custom(String),
}

// ─── Render Feature ──────────────────────────────────────────────────────────

/// A render feature that can be included in an effect.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RenderFeature {
    Lighting,
    Shadows,
    PostProcess,
    SkinRendering,
    Instancing,
    Terrain,
    Particles,
    Water,
    Fog,
    Custom(String),
}

// ─── Pass Definition ─────────────────────────────────────────────────────────

/// A single render pass within an effect.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PassDef {
    pub name: String,
    pub pass_type: PassType,
    /// Name of the shader module or composed shader to use.
    pub shader: String,
    /// Optional composition operations to apply to the shader.
    #[serde(default)]
    pub compositions: Vec<CompositionDef>,
    /// Variant features to enable for this pass.
    #[serde(default)]
    pub features: HashMap<String, String>,
    /// Whether this pass is enabled.
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn default_true() -> bool {
    true
}

// ─── Composition Definition ──────────────────────────────────────────────────

/// Composition operation in effect config.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompositionDef {
    /// Operation kind: `"mixin"`, `"override"`, or `"compose"`.
    pub op: String,
    /// Module name referenced by this operation.
    pub module: String,
    /// Namespace name (for `"compose"` op).
    #[serde(default)]
    pub name: Option<String>,
    /// Function to replace (for `"override"` op).
    #[serde(default)]
    pub target_fn: Option<String>,
}

// ─── Effect Parameter ────────────────────────────────────────────────────────

/// An effect parameter (for conditional composition).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EffectParameter {
    pub name: String,
    pub value: ParameterValue,
}

/// Typed parameter value.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ParameterValue {
    Bool(bool),
    Int(i64),
    Float(f64),
    String(String),
    ShaderRef(String),
}

// ─── Render Effect ───────────────────────────────────────────────────────────

/// A complete render effect definition (SDFX equivalent).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenderEffect {
    pub name: String,
    /// Ordered list of render passes.
    #[serde(default)]
    pub passes: Vec<PassDef>,
    /// Render features included in this effect.
    #[serde(default)]
    pub features: Vec<RenderFeature>,
    /// Effect parameters for conditional composition.
    #[serde(default)]
    pub parameters: Vec<EffectParameter>,
}

/// TOML / JSON file wrapper for loading effects.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EffectFile {
    pub effect: RenderEffect,
}

// ─── Compiled Types ──────────────────────────────────────────────────────────

/// A compiled pass with resolved shader code.
#[derive(Debug, Clone)]
pub struct CompiledPass {
    pub name: String,
    pub pass_type: PassType,
    pub shader_name: String,
    pub wgsl_source: String,
    pub features: HashMap<String, String>,
    pub enabled: bool,
}

/// A compiled effect with all passes resolved to WGSL.
#[derive(Debug, Clone)]
pub struct CompiledEffect {
    pub name: String,
    pub passes: Vec<CompiledPass>,
    pub features: Vec<RenderFeature>,
}

// ─── Effect Loader ───────────────────────────────────────────────────────────

/// Loads and deserializes effect definitions from files or strings.
pub struct EffectLoader;

impl EffectLoader {
    /// Load an effect from a TOML file on disk.
    pub fn load_from_file(path: &Path) -> ShaderResult<RenderEffect> {
        let content = std::fs::read_to_string(path)?;
        Self::load_from_str(&content)
    }

    /// Load an effect from a TOML string.
    pub fn load_from_str(toml_str: &str) -> ShaderResult<RenderEffect> {
        let file: EffectFile = toml::from_str(toml_str)
            .map_err(|e| ShaderError::Effect { message: format!("TOML parse error: {e}") })?;
        Ok(file.effect)
    }

    /// Load an effect from a JSON string.
    pub fn load_from_json(json_str: &str) -> ShaderResult<RenderEffect> {
        let file: EffectFile = serde_json::from_str(json_str)
            .map_err(|e| ShaderError::Effect { message: format!("JSON parse error: {e}") })?;
        Ok(file.effect)
    }
}

// ─── Effect Compiler ─────────────────────────────────────────────────────────

/// Compiles a [`RenderEffect`] into a [`CompiledEffect`] by resolving every
/// shader reference through a [`ShaderComposer`] and generating WGSL.
pub struct EffectCompiler {
    generator: WgslGenerator,
}

impl EffectCompiler {
    pub fn new() -> Self {
        Self {
            generator: WgslGenerator::new(),
        }
    }

    /// Compile a render effect, resolving all shader references through the composer.
    pub fn compile(
        &self,
        effect: &RenderEffect,
        composer: &ShaderComposer,
    ) -> ShaderResult<CompiledEffect> {
        let mut passes = Vec::with_capacity(effect.passes.len());
        for pass in &effect.passes {
            passes.push(self.compile_pass(pass, composer)?);
        }
        Ok(CompiledEffect {
            name: effect.name.clone(),
            passes,
            features: effect.features.clone(),
        })
    }

    /// Compile a single pass.
    pub fn compile_pass(
        &self,
        pass: &PassDef,
        composer: &ShaderComposer,
    ) -> ShaderResult<CompiledPass> {
        let composed = if pass.compositions.is_empty() {
            // No compositions — build a ComposedShader directly from the module.
            let module = composer.get_module(&pass.shader).ok_or_else(|| {
                ShaderError::Effect {
                    message: format!("Shader module '{}' not found in composer", pass.shader),
                }
            })?;
            self.module_to_composed(module, &pass.shader)
        } else {
            // Convert CompositionDef → CompositionOp and compose.
            let ops = self.resolve_compositions(&pass.compositions, composer)?;
            let result_name = format!("{}_{}", pass.shader, pass.name);
            composer.compose(&pass.shader, &ops, &result_name)?
        };

        let wgsl_source = self.generator.generate(&composed)?;

        Ok(CompiledPass {
            name: pass.name.clone(),
            pass_type: pass.pass_type.clone(),
            shader_name: pass.shader.clone(),
            wgsl_source,
            features: pass.features.clone(),
            enabled: pass.enabled,
        })
    }

    /// Validate an effect definition without compiling.
    ///
    /// Checks:
    /// - All shader references exist in the composer.
    /// - No duplicate pass names.
    /// - All composition module references exist.
    pub fn validate_effect(
        &self,
        effect: &RenderEffect,
        composer: &ShaderComposer,
    ) -> ShaderResult<()> {
        // Check duplicate pass names.
        let mut seen_names: HashMap<&str, ()> = HashMap::new();
        for pass in &effect.passes {
            if seen_names.insert(pass.name.as_str(), ()).is_some() {
                return Err(ShaderError::Effect {
                    message: format!("Duplicate pass name: '{}'", pass.name),
                });
            }
        }

        // Check shader references and composition modules.
        for pass in &effect.passes {
            if composer.get_module(&pass.shader).is_none() {
                return Err(ShaderError::Effect {
                    message: format!(
                        "Pass '{}' references shader '{}' which is not registered",
                        pass.name, pass.shader
                    ),
                });
            }
            for comp in &pass.compositions {
                if composer.get_module(&comp.module).is_none() {
                    return Err(ShaderError::Effect {
                        message: format!(
                            "Pass '{}' composition references module '{}' which is not registered",
                            pass.name, comp.module
                        ),
                    });
                }
            }
        }

        Ok(())
    }

    // ─── Private helpers ─────────────────────────────────────────────────────

    /// Convert a [`ShaderModule`] into a [`ComposedShader`] (no composition ops).
    fn module_to_composed(&self, module: &ShaderModule, name: &str) -> ComposedShader {
        let mut router = crate::stream::StreamRouter::new();
        for s in &module.input_streams {
            // add_stream returns Result; unwrap is safe for well-formed modules.
            let _ = router.add_stream(s.clone());
        }

        ComposedShader {
            name: name.to_string(),
            streams: router,
            bindings: module.bindings.clone(),
            structs: module.structs.clone(),
            functions: module.functions.clone(),
            vertex_entry: module.vertex_body.as_ref().map(|body| EntryPointDef {
                name: "vs_main".to_string(),
                body: body.clone(),
                local_vars: Vec::new(),
            }),
            fragment_entry: module.fragment_body.as_ref().map(|body| EntryPointDef {
                name: "fs_main".to_string(),
                body: body.clone(),
                local_vars: Vec::new(),
            }),
            compute_entry: module.compute_body.as_ref().map(|body| ComputeEntryPointDef {
                name: "cs_main".to_string(),
                workgroup_size: [1, 1, 1],
                body: body.clone(),
                local_vars: Vec::new(),
            }),
            constants: module.constants.clone(),
            global_vars: module.global_vars.clone(),
        }
    }

    /// Convert [`CompositionDef`] descriptors into [`CompositionOp`] values,
    /// validating references along the way.
    fn resolve_compositions(
        &self,
        defs: &[CompositionDef],
        composer: &ShaderComposer,
    ) -> ShaderResult<Vec<CompositionOp>> {
        let mut ops = Vec::with_capacity(defs.len());
        for def in defs {
            // Ensure referenced module exists.
            if composer.get_module(&def.module).is_none() {
                return Err(ShaderError::Effect {
                    message: format!(
                        "Composition references unknown module '{}'",
                        def.module
                    ),
                });
            }
            let op = match def.op.as_str() {
                "mixin" => CompositionOp::Mixin(def.module.clone()),
                "override" => {
                    let target_fn = def.target_fn.clone().ok_or_else(|| {
                        ShaderError::Effect {
                            message: "Override composition requires 'target_fn'".into(),
                        }
                    })?;
                    // Look up the replacement function from the referenced module.
                    let module = composer.get_module(&def.module).unwrap();
                    let replacement = module
                        .functions
                        .iter()
                        .find(|f| f.name == target_fn)
                        .cloned()
                        .ok_or_else(|| ShaderError::Effect {
                            message: format!(
                                "Module '{}' does not contain function '{}' for override",
                                def.module, target_fn
                            ),
                        })?;
                    CompositionOp::Override {
                        target_fn,
                        replacement,
                    }
                }
                "compose" => {
                    let ns_name = def.name.clone().unwrap_or_else(|| def.module.clone());
                    CompositionOp::Compose {
                        name: ns_name,
                        module: def.module.clone(),
                    }
                }
                other => {
                    return Err(ShaderError::Effect {
                        message: format!("Unknown composition op: '{}'", other),
                    });
                }
            };
            ops.push(op);
        }
        Ok(ops)
    }
}

impl Default for EffectCompiler {
    fn default() -> Self {
        Self::new()
    }
}

// ─── Effect Builder ──────────────────────────────────────────────────────────

/// Fluent builder for constructing [`RenderEffect`] in Rust code.
pub struct EffectBuilder {
    name: String,
    passes: Vec<PassDef>,
    features: Vec<RenderFeature>,
    parameters: Vec<EffectParameter>,
}

impl EffectBuilder {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            passes: Vec::new(),
            features: Vec::new(),
            parameters: Vec::new(),
        }
    }

    pub fn add_pass(mut self, pass: PassDef) -> Self {
        self.passes.push(pass);
        self
    }

    pub fn add_geometry_pass(
        mut self,
        name: impl Into<String>,
        shader: impl Into<String>,
    ) -> Self {
        self.passes.push(PassDef {
            name: name.into(),
            pass_type: PassType::Geometry,
            shader: shader.into(),
            compositions: Vec::new(),
            features: HashMap::new(),
            enabled: true,
        });
        self
    }

    pub fn add_lighting_pass(
        mut self,
        name: impl Into<String>,
        shader: impl Into<String>,
    ) -> Self {
        self.passes.push(PassDef {
            name: name.into(),
            pass_type: PassType::Lighting,
            shader: shader.into(),
            compositions: Vec::new(),
            features: HashMap::new(),
            enabled: true,
        });
        self
    }

    pub fn add_shadow_pass(
        mut self,
        name: impl Into<String>,
        shader: impl Into<String>,
    ) -> Self {
        self.passes.push(PassDef {
            name: name.into(),
            pass_type: PassType::Shadow,
            shader: shader.into(),
            compositions: Vec::new(),
            features: HashMap::new(),
            enabled: true,
        });
        self
    }

    pub fn add_post_process_pass(
        mut self,
        name: impl Into<String>,
        shader: impl Into<String>,
    ) -> Self {
        self.passes.push(PassDef {
            name: name.into(),
            pass_type: PassType::PostProcess,
            shader: shader.into(),
            compositions: Vec::new(),
            features: HashMap::new(),
            enabled: true,
        });
        self
    }

    pub fn add_compute_pass(
        mut self,
        name: impl Into<String>,
        shader: impl Into<String>,
    ) -> Self {
        self.passes.push(PassDef {
            name: name.into(),
            pass_type: PassType::Compute,
            shader: shader.into(),
            compositions: Vec::new(),
            features: HashMap::new(),
            enabled: true,
        });
        self
    }

    pub fn add_feature(mut self, feature: RenderFeature) -> Self {
        self.features.push(feature);
        self
    }

    pub fn add_parameter(mut self, name: impl Into<String>, value: ParameterValue) -> Self {
        self.parameters.push(EffectParameter {
            name: name.into(),
            value,
        });
        self
    }

    /// Conditionally add a pass based on a boolean condition.
    pub fn add_pass_if(mut self, condition: bool, pass: PassDef) -> Self {
        if condition {
            self.passes.push(pass);
        }
        self
    }

    /// Build the final [`RenderEffect`].
    pub fn build(self) -> RenderEffect {
        RenderEffect {
            name: self.name,
            passes: self.passes,
            features: self.features,
            parameters: self.parameters,
        }
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stream::presets;

    /// Helper: create a simple shader module with vertex + fragment bodies.
    fn make_simple_module(name: &str) -> ShaderModule {
        ShaderModuleBuilder::new(name)
            .stream(presets::position())
            .vertex_body("output.clip_position = vec4<f32>(input.position, 1.0);")
            .fragment_body("return vec4<f32>(1.0, 0.0, 0.0, 1.0);")
            .build()
    }

    /// Helper: create a compute shader module.
    fn make_compute_module(name: &str) -> ShaderModule {
        ShaderModuleBuilder::new(name)
            .compute_body("let idx = global_id.x;")
            .build()
    }

    // ── TOML loading ─────────────────────────────────────────────────────

    #[test]
    fn test_load_effect_from_toml() {
        let toml_str = r#"
[effect]
name = "CharacterEffect"
features = ["Lighting", "Shadows", "SkinRendering"]

[[effect.passes]]
name = "GBufferPass"
pass_type = "Geometry"
shader = "CharacterShader"
enabled = true

[effect.passes.features]
SKINNING = "true"
NORMAL_MAP = "true"

[[effect.passes]]
name = "LightingPass"
pass_type = "Lighting"
shader = "DeferredLighting"

[[effect.passes]]
name = "ShadowPass"
pass_type = "Shadow"
shader = "ShadowDepth"

[[effect.passes]]
name = "SSAO"
pass_type = "PostProcess"
shader = "SSAOShader"

[[effect.parameters]]
name = "enable_skinning"
value = { Bool = true }
"#;

        let effect = EffectLoader::load_from_str(toml_str).unwrap();
        assert_eq!(effect.name, "CharacterEffect");
        assert_eq!(effect.passes.len(), 4);
        assert_eq!(effect.passes[0].name, "GBufferPass");
        assert_eq!(effect.passes[0].pass_type, PassType::Geometry);
        assert_eq!(effect.passes[0].shader, "CharacterShader");
        assert!(effect.passes[0].enabled);
        assert_eq!(
            effect.passes[0].features.get("SKINNING").map(|s| s.as_str()),
            Some("true")
        );
        assert_eq!(effect.passes[1].pass_type, PassType::Lighting);
        assert_eq!(effect.passes[2].pass_type, PassType::Shadow);
        assert_eq!(effect.passes[3].pass_type, PassType::PostProcess);
        assert_eq!(effect.features.len(), 3);
        assert_eq!(effect.features[0], RenderFeature::Lighting);
        assert_eq!(effect.features[1], RenderFeature::Shadows);
        assert_eq!(effect.features[2], RenderFeature::SkinRendering);
        assert_eq!(effect.parameters.len(), 1);
        assert_eq!(effect.parameters[0].name, "enable_skinning");
        assert!(matches!(effect.parameters[0].value, ParameterValue::Bool(true)));
    }

    #[test]
    fn test_load_effect_from_toml_invalid() {
        let result = EffectLoader::load_from_str("this is not valid toml {{{{");
        assert!(result.is_err());
    }

    // ── JSON loading ─────────────────────────────────────────────────────

    #[test]
    fn test_load_effect_from_json() {
        let json_str = r#"{
  "effect": {
    "name": "SimpleEffect",
    "passes": [
      {
        "name": "MainPass",
        "pass_type": "Geometry",
        "shader": "MainShader",
        "enabled": true
      }
    ],
    "features": ["Lighting"],
    "parameters": [
      { "name": "brightness", "value": { "Float": 1.5 } }
    ]
  }
}"#;

        let effect = EffectLoader::load_from_json(json_str).unwrap();
        assert_eq!(effect.name, "SimpleEffect");
        assert_eq!(effect.passes.len(), 1);
        assert_eq!(effect.passes[0].name, "MainPass");
        assert_eq!(effect.passes[0].pass_type, PassType::Geometry);
        assert!(matches!(
            effect.parameters[0].value,
            ParameterValue::Float(v) if (v - 1.5).abs() < f64::EPSILON
        ));
    }

    #[test]
    fn test_load_effect_from_json_invalid() {
        let result = EffectLoader::load_from_json("{broken json");
        assert!(result.is_err());
    }

    // ── Effect Builder ───────────────────────────────────────────────────

    #[test]
    fn test_effect_builder() {
        let effect = EffectBuilder::new("TestEffect")
            .add_geometry_pass("GBuffer", "GBufferShader")
            .add_lighting_pass("Lighting", "DeferredLighting")
            .add_shadow_pass("Shadow", "ShadowDepth")
            .add_post_process_pass("SSAO", "SSAOShader")
            .add_compute_pass("Cull", "CullCompute")
            .add_feature(RenderFeature::Lighting)
            .add_feature(RenderFeature::Shadows)
            .add_parameter("quality", ParameterValue::String("high".into()))
            .build();

        assert_eq!(effect.name, "TestEffect");
        assert_eq!(effect.passes.len(), 5);
        assert_eq!(effect.passes[0].pass_type, PassType::Geometry);
        assert_eq!(effect.passes[1].pass_type, PassType::Lighting);
        assert_eq!(effect.passes[2].pass_type, PassType::Shadow);
        assert_eq!(effect.passes[3].pass_type, PassType::PostProcess);
        assert_eq!(effect.passes[4].pass_type, PassType::Compute);
        assert_eq!(effect.features.len(), 2);
        assert_eq!(effect.parameters.len(), 1);
    }

    #[test]
    fn test_effect_builder_conditional() {
        let has_skinning = true;
        let has_particles = false;

        let skinning_pass = PassDef {
            name: "Skinning".into(),
            pass_type: PassType::Custom("Skinning".into()),
            shader: "SkinningShader".into(),
            compositions: Vec::new(),
            features: HashMap::new(),
            enabled: true,
        };

        let particles_pass = PassDef {
            name: "Particles".into(),
            pass_type: PassType::PostProcess,
            shader: "ParticleShader".into(),
            compositions: Vec::new(),
            features: HashMap::new(),
            enabled: true,
        };

        let effect = EffectBuilder::new("ConditionalEffect")
            .add_geometry_pass("Main", "MainShader")
            .add_pass_if(has_skinning, skinning_pass)
            .add_pass_if(has_particles, particles_pass)
            .build();

        assert_eq!(effect.passes.len(), 2); // Main + Skinning (particles excluded)
        assert_eq!(effect.passes[0].name, "Main");
        assert_eq!(effect.passes[1].name, "Skinning");
    }

    #[test]
    fn test_effect_builder_add_pass() {
        let pass = PassDef {
            name: "Custom".into(),
            pass_type: PassType::Custom("Special".into()),
            shader: "SpecialShader".into(),
            compositions: Vec::new(),
            features: HashMap::new(),
            enabled: true,
        };
        let effect = EffectBuilder::new("test").add_pass(pass).build();
        assert_eq!(effect.passes.len(), 1);
        assert_eq!(effect.passes[0].pass_type, PassType::Custom("Special".into()));
    }

    // ── Compilation ──────────────────────────────────────────────────────

    #[test]
    fn test_compile_effect() {
        let mut composer = ShaderComposer::new();
        composer
            .register_module(make_simple_module("GBufferShader"))
            .unwrap();
        composer
            .register_module(make_simple_module("DeferredLighting"))
            .unwrap();
        composer
            .register_module(make_compute_module("CullShader"))
            .unwrap();

        let effect = EffectBuilder::new("TestEffect")
            .add_geometry_pass("GBuffer", "GBufferShader")
            .add_lighting_pass("Lighting", "DeferredLighting")
            .add_compute_pass("Cull", "CullShader")
            .add_feature(RenderFeature::Lighting)
            .build();

        let compiler = EffectCompiler::new();
        let compiled = compiler.compile(&effect, &composer).unwrap();

        assert_eq!(compiled.name, "TestEffect");
        assert_eq!(compiled.passes.len(), 3);
        assert_eq!(compiled.features.len(), 1);

        // Every pass should have non-empty WGSL.
        for pass in &compiled.passes {
            assert!(!pass.wgsl_source.is_empty(), "Pass '{}' has empty WGSL", pass.name);
            assert!(pass.enabled);
        }

        // Geometry pass should contain vertex entry.
        assert!(compiled.passes[0].wgsl_source.contains("@vertex"));
        assert!(compiled.passes[0].wgsl_source.contains("@fragment"));

        // Compute pass should contain compute entry.
        assert!(compiled.passes[2].wgsl_source.contains("@compute"));
    }

    #[test]
    fn test_compile_effect_missing_shader() {
        let composer = ShaderComposer::new();

        let effect = EffectBuilder::new("Bad")
            .add_geometry_pass("Main", "NonExistentShader")
            .build();

        let compiler = EffectCompiler::new();
        let result = compiler.compile(&effect, &composer);
        assert!(result.is_err());
        let msg = format!("{}", result.unwrap_err());
        assert!(msg.contains("NonExistentShader"));
    }

    #[test]
    fn test_compile_pass_missing_shader() {
        let composer = ShaderComposer::new();
        let compiler = EffectCompiler::new();

        let pass = PassDef {
            name: "Bad".into(),
            pass_type: PassType::Geometry,
            shader: "Missing".into(),
            compositions: Vec::new(),
            features: HashMap::new(),
            enabled: true,
        };

        let result = compiler.compile_pass(&pass, &composer);
        assert!(result.is_err());
    }

    // ── Validation ───────────────────────────────────────────────────────

    #[test]
    fn test_validate_effect() {
        let mut composer = ShaderComposer::new();
        composer
            .register_module(make_simple_module("ShaderA"))
            .unwrap();
        composer
            .register_module(make_simple_module("ShaderB"))
            .unwrap();

        let effect = EffectBuilder::new("Valid")
            .add_geometry_pass("PassA", "ShaderA")
            .add_lighting_pass("PassB", "ShaderB")
            .build();

        let compiler = EffectCompiler::new();
        assert!(compiler.validate_effect(&effect, &composer).is_ok());
    }

    #[test]
    fn test_validate_effect_duplicate_pass_names() {
        let mut composer = ShaderComposer::new();
        composer
            .register_module(make_simple_module("ShaderA"))
            .unwrap();

        let effect = EffectBuilder::new("Dup")
            .add_geometry_pass("Same", "ShaderA")
            .add_lighting_pass("Same", "ShaderA")
            .build();

        let compiler = EffectCompiler::new();
        let result = compiler.validate_effect(&effect, &composer);
        assert!(result.is_err());
        let msg = format!("{}", result.unwrap_err());
        assert!(msg.contains("Duplicate pass name"));
    }

    #[test]
    fn test_validate_effect_missing_shader() {
        let composer = ShaderComposer::new();

        let effect = EffectBuilder::new("Missing")
            .add_geometry_pass("P", "NoShader")
            .build();

        let compiler = EffectCompiler::new();
        let result = compiler.validate_effect(&effect, &composer);
        assert!(result.is_err());
        let msg = format!("{}", result.unwrap_err());
        assert!(msg.contains("NoShader"));
    }

    #[test]
    fn test_validate_effect_missing_composition_module() {
        let mut composer = ShaderComposer::new();
        composer
            .register_module(make_simple_module("Base"))
            .unwrap();

        let effect = RenderEffect {
            name: "test".into(),
            passes: vec![PassDef {
                name: "P".into(),
                pass_type: PassType::Geometry,
                shader: "Base".into(),
                compositions: vec![CompositionDef {
                    op: "mixin".into(),
                    module: "Missing".into(),
                    name: None,
                    target_fn: None,
                }],
                features: HashMap::new(),
                enabled: true,
            }],
            features: Vec::new(),
            parameters: Vec::new(),
        };

        let compiler = EffectCompiler::new();
        let result = compiler.validate_effect(&effect, &composer);
        assert!(result.is_err());
        let msg = format!("{}", result.unwrap_err());
        assert!(msg.contains("Missing"));
    }

    // ── Disabled pass ────────────────────────────────────────────────────

    #[test]
    fn test_pass_disabled() {
        let mut composer = ShaderComposer::new();
        composer
            .register_module(make_simple_module("Shader"))
            .unwrap();

        let effect = RenderEffect {
            name: "test".into(),
            passes: vec![PassDef {
                name: "DisabledPass".into(),
                pass_type: PassType::Geometry,
                shader: "Shader".into(),
                compositions: Vec::new(),
                features: HashMap::new(),
                enabled: false,
            }],
            features: Vec::new(),
            parameters: Vec::new(),
        };

        let compiler = EffectCompiler::new();
        let compiled = compiler.compile(&effect, &composer).unwrap();
        assert!(!compiled.passes[0].enabled);
        // WGSL should still be generated even for disabled passes.
        assert!(!compiled.passes[0].wgsl_source.is_empty());
    }

    // ── Serialization roundtrip ──────────────────────────────────────────

    #[test]
    fn test_effect_serialization_roundtrip() {
        let effect = EffectBuilder::new("RoundtripEffect")
            .add_geometry_pass("GBuffer", "GShader")
            .add_lighting_pass("Light", "LShader")
            .add_feature(RenderFeature::Lighting)
            .add_feature(RenderFeature::Custom("MyFeature".into()))
            .add_parameter("quality", ParameterValue::Int(3))
            .add_parameter("enabled", ParameterValue::Bool(true))
            .build();

        // Serialize to TOML.
        let file = EffectFile {
            effect: effect.clone(),
        };
        let toml_str = toml::to_string_pretty(&file).unwrap();

        // Deserialize back.
        let loaded = EffectLoader::load_from_str(&toml_str).unwrap();
        assert_eq!(loaded.name, effect.name);
        assert_eq!(loaded.passes.len(), effect.passes.len());
        assert_eq!(loaded.features.len(), effect.features.len());
        assert_eq!(loaded.parameters.len(), effect.parameters.len());

        // Verify individual fields.
        for (a, b) in loaded.passes.iter().zip(effect.passes.iter()) {
            assert_eq!(a.name, b.name);
            assert_eq!(a.pass_type, b.pass_type);
            assert_eq!(a.shader, b.shader);
            assert_eq!(a.enabled, b.enabled);
        }
    }

    #[test]
    fn test_effect_json_roundtrip() {
        let effect = EffectBuilder::new("JsonRoundtrip")
            .add_post_process_pass("Bloom", "BloomShader")
            .add_feature(RenderFeature::PostProcess)
            .add_parameter("intensity", ParameterValue::Float(0.8))
            .build();

        let file = EffectFile {
            effect: effect.clone(),
        };
        let json_str = serde_json::to_string_pretty(&file).unwrap();
        let loaded = EffectLoader::load_from_json(&json_str).unwrap();
        assert_eq!(loaded.name, effect.name);
        assert_eq!(loaded.passes.len(), 1);
        assert_eq!(loaded.passes[0].pass_type, PassType::PostProcess);
    }

    // ── Compile with compositions ────────────────────────────────────────

    #[test]
    fn test_compile_with_compositions() {
        let mut composer = ShaderComposer::new();

        let base = ShaderModuleBuilder::new("BaseShader")
            .stream(presets::position())
            .overridable_function(
                "get_color",
                vec![],
                Some(WgslType::Vec4(WgslScalarType::F32)),
                "return vec4<f32>(1.0, 0.0, 0.0, 1.0);",
            )
            .vertex_body("output.clip_position = vec4<f32>(input.position, 1.0);")
            .fragment_body("return get_color();")
            .build();

        let mixin = ShaderModuleBuilder::new("NormalMixin")
            .stream(presets::normal())
            .function(FunctionDef {
                name: "compute_normal".into(),
                parameters: vec![("n".into(), WgslType::Vec3(WgslScalarType::F32))],
                return_type: Some(WgslType::Vec3(WgslScalarType::F32)),
                body: WgslFragment::new("return normalize(n);"),
                overridable: false,
            })
            .build();

        let replacement_module = ShaderModuleBuilder::new("ColorOverride")
            .function(FunctionDef {
                name: "get_color".into(),
                parameters: vec![],
                return_type: Some(WgslType::Vec4(WgslScalarType::F32)),
                body: WgslFragment::new("return vec4<f32>(0.0, 1.0, 0.0, 1.0);"),
                overridable: false,
            })
            .build();

        composer.register_module(base).unwrap();
        composer.register_module(mixin).unwrap();
        composer.register_module(replacement_module).unwrap();

        let effect = RenderEffect {
            name: "ComposedEffect".into(),
            passes: vec![PassDef {
                name: "ComposedPass".into(),
                pass_type: PassType::Geometry,
                shader: "BaseShader".into(),
                compositions: vec![
                    CompositionDef {
                        op: "mixin".into(),
                        module: "NormalMixin".into(),
                        name: None,
                        target_fn: None,
                    },
                    CompositionDef {
                        op: "override".into(),
                        module: "ColorOverride".into(),
                        name: None,
                        target_fn: Some("get_color".into()),
                    },
                ],
                features: HashMap::new(),
                enabled: true,
            }],
            features: Vec::new(),
            parameters: Vec::new(),
        };

        let compiler = EffectCompiler::new();
        let compiled = compiler.compile(&effect, &composer).unwrap();

        assert_eq!(compiled.passes.len(), 1);
        let wgsl = &compiled.passes[0].wgsl_source;

        // Should contain the normal mixin's function.
        assert!(wgsl.contains("fn compute_normal"));
        // Should contain the overridden get_color (green, not red).
        assert!(wgsl.contains("0.0, 1.0, 0.0"));
        // Should have both position and normal streams.
        assert!(wgsl.contains("struct VertexInput"));
    }

    #[test]
    fn test_compile_with_compose_namespace() {
        let mut composer = ShaderComposer::new();

        let base = ShaderModuleBuilder::new("Base")
            .stream(presets::position())
            .vertex_body("output.clip_position = vec4<f32>(input.position, 1.0);")
            .fragment_body("return vec4<f32>(1.0);")
            .build();

        let sub = ShaderModuleBuilder::new("SubModule")
            .function(FunctionDef {
                name: "helper".into(),
                parameters: vec![],
                return_type: Some(WgslType::F32),
                body: WgslFragment::new("return 42.0;"),
                overridable: false,
            })
            .build();

        composer.register_module(base).unwrap();
        composer.register_module(sub).unwrap();

        let effect = RenderEffect {
            name: "test".into(),
            passes: vec![PassDef {
                name: "P".into(),
                pass_type: PassType::Geometry,
                shader: "Base".into(),
                compositions: vec![CompositionDef {
                    op: "compose".into(),
                    module: "SubModule".into(),
                    name: Some("sub".into()),
                    target_fn: None,
                }],
                features: HashMap::new(),
                enabled: true,
            }],
            features: Vec::new(),
            parameters: Vec::new(),
        };

        let compiler = EffectCompiler::new();
        let compiled = compiler.compile(&effect, &composer).unwrap();
        let wgsl = &compiled.passes[0].wgsl_source;

        // Function should be namespace-prefixed.
        assert!(wgsl.contains("fn sub_helper"));
    }

    // ── Default enabled ──────────────────────────────────────────────────

    #[test]
    fn test_pass_default_enabled() {
        let toml_str = r#"
[effect]
name = "test"

[[effect.passes]]
name = "P"
pass_type = "Geometry"
shader = "S"
"#;
        // No `enabled` field — should default to true.
        let effect = EffectLoader::load_from_str(toml_str).unwrap();
        assert!(effect.passes[0].enabled);
    }

    // ── PassType custom variant ──────────────────────────────────────────

    #[test]
    fn test_pass_type_custom() {
        let toml_str = r#"
[effect]
name = "test"

[[effect.passes]]
name = "P"
pass_type = { Custom = "SpecialPass" }
shader = "S"
"#;
        let effect = EffectLoader::load_from_str(toml_str).unwrap();
        assert_eq!(effect.passes[0].pass_type, PassType::Custom("SpecialPass".into()));
    }

    // ── RenderFeature custom variant ─────────────────────────────────────

    #[test]
    fn test_render_feature_custom() {
        let toml_str = r#"
[effect]
name = "test"
features = ["Lighting", { Custom = "Volumetrics" }]
"#;
        let effect = EffectLoader::load_from_str(toml_str).unwrap();
        assert_eq!(effect.features[0], RenderFeature::Lighting);
        assert_eq!(effect.features[1], RenderFeature::Custom("Volumetrics".into()));
    }

    // ── Compiler default ─────────────────────────────────────────────────

    #[test]
    fn test_compiler_default() {
        let compiler = EffectCompiler::default();
        let mut composer = ShaderComposer::new();
        composer.register_module(make_simple_module("S")).unwrap();

        let effect = EffectBuilder::new("test")
            .add_geometry_pass("P", "S")
            .build();

        assert!(compiler.compile(&effect, &composer).is_ok());
    }

    // ── Unknown composition op ───────────────────────────────────────────

    #[test]
    fn test_unknown_composition_op() {
        let mut composer = ShaderComposer::new();
        composer.register_module(make_simple_module("Base")).unwrap();

        let effect = RenderEffect {
            name: "test".into(),
            passes: vec![PassDef {
                name: "P".into(),
                pass_type: PassType::Geometry,
                shader: "Base".into(),
                compositions: vec![CompositionDef {
                    op: "unknown_op".into(),
                    module: "Base".into(),
                    name: None,
                    target_fn: None,
                }],
                features: HashMap::new(),
                enabled: true,
            }],
            features: Vec::new(),
            parameters: Vec::new(),
        };

        let compiler = EffectCompiler::new();
        let result = compiler.compile(&effect, &composer);
        assert!(result.is_err());
        let msg = format!("{}", result.unwrap_err());
        assert!(msg.contains("unknown_op"));
    }

    // ── Override without target_fn ───────────────────────────────────────

    #[test]
    fn test_override_missing_target_fn() {
        let mut composer = ShaderComposer::new();
        composer.register_module(make_simple_module("Base")).unwrap();
        composer.register_module(make_simple_module("Other")).unwrap();

        let effect = RenderEffect {
            name: "test".into(),
            passes: vec![PassDef {
                name: "P".into(),
                pass_type: PassType::Geometry,
                shader: "Base".into(),
                compositions: vec![CompositionDef {
                    op: "override".into(),
                    module: "Other".into(),
                    name: None,
                    target_fn: None, // Missing!
                }],
                features: HashMap::new(),
                enabled: true,
            }],
            features: Vec::new(),
            parameters: Vec::new(),
        };

        let compiler = EffectCompiler::new();
        let result = compiler.compile(&effect, &composer);
        assert!(result.is_err());
        let msg = format!("{}", result.unwrap_err());
        assert!(msg.contains("target_fn"));
    }
}
