//! Shader composition engine — mixin, override, and compose operations.
//!
//! Provides a high-level composition API inspired by Stride SDSL that enables
//! shader mixing: mixin (inject modules), override (replace functions), and
//! compose (namespace sub-modules).

use crate::core::*;
use crate::generator::*;
use crate::stream::*;
use std::collections::HashMap;

// ─── Shader Module ────────────────────────────────────────────────────────────

/// A concrete shader module that can participate in composition.
#[derive(Debug, Clone)]
pub struct ShaderModule {
    pub name: String,
    pub input_streams: Vec<StreamDeclaration>,
    pub output_streams: Vec<StreamDeclaration>,
    pub bindings: Vec<ShaderBinding>,
    pub structs: Vec<StructDef>,
    pub functions: Vec<FunctionDef>,
    pub vertex_body: Option<WgslFragment>,
    pub fragment_body: Option<WgslFragment>,
    pub compute_body: Option<WgslFragment>,
    pub constants: Vec<ConstantDef>,
    pub global_vars: Vec<GlobalVarDef>,
    pub dependencies: Vec<String>,
}

impl ShaderModuleDef for ShaderModule {
    fn name(&self) -> &str {
        &self.name
    }

    fn input_streams(&self) -> Vec<StreamDeclaration> {
        self.input_streams.clone()
    }

    fn output_streams(&self) -> Vec<StreamDeclaration> {
        self.output_streams.clone()
    }

    fn bindings(&self) -> Vec<ShaderBinding> {
        self.bindings.clone()
    }

    fn structs(&self) -> Vec<StructDef> {
        self.structs.clone()
    }

    fn functions(&self) -> Vec<FunctionDef> {
        self.functions.clone()
    }

    fn vertex_body(&self) -> Option<WgslFragment> {
        self.vertex_body.clone()
    }

    fn fragment_body(&self) -> Option<WgslFragment> {
        self.fragment_body.clone()
    }

    fn compute_body(&self) -> Option<WgslFragment> {
        self.compute_body.clone()
    }

    fn dependencies(&self) -> Vec<String> {
        self.dependencies.clone()
    }
}

impl ShaderModule {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            input_streams: Vec::new(),
            output_streams: Vec::new(),
            bindings: Vec::new(),
            structs: Vec::new(),
            functions: Vec::new(),
            vertex_body: None,
            fragment_body: None,
            compute_body: None,
            constants: Vec::new(),
            global_vars: Vec::new(),
            dependencies: Vec::new(),
        }
    }

    pub fn with_stream(mut self, decl: StreamDeclaration) -> Self {
        self.input_streams.push(decl);
        self
    }

    pub fn with_streams(mut self, decls: Vec<StreamDeclaration>) -> Self {
        self.input_streams.extend(decls);
        self
    }

    pub fn with_binding(mut self, binding: ShaderBinding) -> Self {
        self.bindings.push(binding);
        self
    }

    pub fn with_struct(mut self, s: StructDef) -> Self {
        self.structs.push(s);
        self
    }

    pub fn with_function(mut self, f: FunctionDef) -> Self {
        self.functions.push(f);
        self
    }

    pub fn with_vertex_body(mut self, body: impl Into<String>) -> Self {
        self.vertex_body = Some(WgslFragment::new(body));
        self
    }

    pub fn with_fragment_body(mut self, body: impl Into<String>) -> Self {
        self.fragment_body = Some(WgslFragment::new(body));
        self
    }

    pub fn with_compute_body(mut self, body: impl Into<String>) -> Self {
        self.compute_body = Some(WgslFragment::new(body));
        self
    }

    pub fn with_constant(mut self, c: ConstantDef) -> Self {
        self.constants.push(c);
        self
    }

    pub fn with_global_var(mut self, v: GlobalVarDef) -> Self {
        self.global_vars.push(v);
        self
    }
}

// ─── Composition Operation ────────────────────────────────────────────────────

/// Composition operation descriptor.
#[derive(Debug, Clone)]
pub enum CompositionOp {
    /// Mixin: inject module's code into the base, overriding same-named functions.
    Mixin(String),
    /// Override: replace a specific function by name.
    Override {
        target_fn: String,
        replacement: FunctionDef,
    },
    /// Compose: add module as namespaced sub-component.
    Compose {
        name: String,
        module: String,
    },
}

// ─── Shader Composer ──────────────────────────────────────────────────────────

/// The main composition engine.
pub struct ShaderComposer {
    /// Registry of available modules by name.
    modules: HashMap<String, ShaderModule>,
}

impl ShaderComposer {
    pub fn new() -> Self {
        Self {
            modules: HashMap::new(),
        }
    }

    /// Register a shader module for use in compositions.
    ///
    /// Returns an error if a module with the same name is already registered.
    pub fn register_module(&mut self, module: ShaderModule) -> ShaderResult<()> {
        if self.modules.contains_key(&module.name) {
            return Err(ShaderError::Composition {
                message: format!("Module '{}' is already registered", module.name),
            });
        }
        self.modules.insert(module.name.clone(), module);
        Ok(())
    }

    /// Get a registered module by name.
    pub fn get_module(&self, name: &str) -> Option<&ShaderModule> {
        self.modules.get(name)
    }

    /// List all registered module names.
    pub fn module_names(&self) -> Vec<&str> {
        self.modules.keys().map(|k| k.as_str()).collect()
    }

    /// Compose a shader from a base module and a list of operations.
    pub fn compose(
        &self,
        base_name: &str,
        operations: &[CompositionOp],
        result_name: &str,
    ) -> ShaderResult<ComposedShader> {
        let base = self.modules.get(base_name).ok_or_else(|| {
            ShaderError::Composition {
                message: format!("Base module '{}' not found", base_name),
            }
        })?;

        let mut input_streams = base.input_streams.clone();
        let mut output_streams = base.output_streams.clone();
        let mut bindings = base.bindings.clone();
        let mut structs = base.structs.clone();
        let mut functions = base.functions.clone();
        let mut vertex_body = base.vertex_body.clone();
        let mut fragment_body = base.fragment_body.clone();
        let mut compute_body = base.compute_body.clone();
        let mut constants = base.constants.clone();
        let mut global_vars = base.global_vars.clone();

        for op in operations {
            match op {
                CompositionOp::Mixin(mixin_name) => {
                    let mixin = self.modules.get(mixin_name.as_str()).ok_or_else(|| {
                        ShaderError::Composition {
                            message: format!("Mixin module '{}' not found", mixin_name),
                        }
                    })?;

                    // 1. Merge streams by semantic (skip duplicates)
                    Self::merge_streams(&mut input_streams, &mixin.input_streams);
                    Self::merge_streams(&mut output_streams, &mixin.output_streams);

                    // 2. Merge bindings (conflict on same group:binding, different name)
                    Self::merge_bindings(&mut bindings, &mixin.bindings, base_name, mixin_name)?;

                    // 3. Merge structs (mixin overrides same-named)
                    Self::merge_structs(&mut structs, &mixin.structs);

                    // 4. Merge functions (mixin overrides same-named overridable functions)
                    Self::merge_functions(
                        &mut functions,
                        &mixin.functions,
                        base_name,
                        mixin_name,
                    )?;

                    // 5. Merge constants (mixin overrides same-named, same-id)
                    Self::merge_constants(&mut constants, &mixin.constants);

                    // 6. Merge global vars (mixin overrides same-named)
                    Self::merge_global_vars(&mut global_vars, &mixin.global_vars);

                    // 7. Entry point bodies with base() token support
                    if let Some(ref mixin_vs) = mixin.vertex_body {
                        vertex_body =
                            Some(Self::merge_body(vertex_body.as_ref(), mixin_vs));
                    }
                    if let Some(ref mixin_fs) = mixin.fragment_body {
                        fragment_body =
                            Some(Self::merge_body(fragment_body.as_ref(), mixin_fs));
                    }
                    if let Some(ref mixin_cs) = mixin.compute_body {
                        compute_body =
                            Some(Self::merge_body(compute_body.as_ref(), mixin_cs));
                    }
                }

                CompositionOp::Override {
                    target_fn,
                    replacement,
                } => {
                    let idx = functions.iter().position(|f| f.name == *target_fn);
                    match idx {
                        Some(i) if functions[i].overridable => {
                            functions[i] = replacement.clone();
                        }
                        Some(_) => {
                            return Err(ShaderError::Composition {
                                message: format!("Function '{}' is not overridable", target_fn),
                            });
                        }
                        None => {
                            return Err(ShaderError::Composition {
                                message: format!("Function '{}' not found", target_fn),
                            });
                        }
                    }
                }

                CompositionOp::Compose {
                    name,
                    module: module_name,
                } => {
                    let sub_module = self.modules.get(module_name.as_str()).ok_or_else(|| {
                        ShaderError::Composition {
                            message: format!("Compose module '{}' not found", module_name),
                        }
                    })?;

                    let prefix = format!("{}_", name);

                    // Namespace-prefix the sub-module's structs
                    for s in &sub_module.structs {
                        let mut ns = s.clone();
                        ns.name = format!("{}{}", prefix, s.name);
                        structs.push(ns);
                    }

                    // Namespace-prefix the sub-module's functions
                    for f in &sub_module.functions {
                        let mut nf = f.clone();
                        nf.name = format!("{}{}", prefix, f.name);
                        functions.push(nf);
                    }

                    // Merge streams and bindings normally (no prefixing)
                    Self::merge_streams(&mut input_streams, &sub_module.input_streams);
                    Self::merge_streams(&mut output_streams, &sub_module.output_streams);
                    Self::merge_bindings(
                        &mut bindings,
                        &sub_module.bindings,
                        base_name,
                        module_name,
                    )?;

                    // Merge constants and global vars
                    Self::merge_constants(&mut constants, &sub_module.constants);
                    Self::merge_global_vars(&mut global_vars, &sub_module.global_vars);
                }
            }
        }

        // Build StreamRouter from all merged input streams
        let mut router = StreamRouter::new();
        for stream in input_streams {
            router.add_stream(stream)?;
        }

        Ok(ComposedShader {
            name: result_name.to_string(),
            streams: router,
            bindings,
            structs,
            functions,
            vertex_entry: vertex_body.map(|body| EntryPointDef {
                name: "vs_main".to_string(),
                body,
                local_vars: Vec::new(),
            }),
            fragment_entry: fragment_body.map(|body| EntryPointDef {
                name: "fs_main".to_string(),
                body,
                local_vars: Vec::new(),
            }),
            compute_entry: compute_body.map(|body| ComputeEntryPointDef {
                name: "cs_main".to_string(),
                workgroup_size: [1, 1, 1],
                body,
                local_vars: Vec::new(),
            }),
            constants,
            global_vars,
        })
    }

    /// Convenience: compose with a single mixin.
    pub fn mixin(
        &self,
        base_name: &str,
        mixin_name: &str,
        result_name: &str,
    ) -> ShaderResult<ComposedShader> {
        self.compose(
            base_name,
            &[CompositionOp::Mixin(mixin_name.to_string())],
            result_name,
        )
    }

    /// Convenience: compose with a function override.
    pub fn override_function(
        &self,
        base_name: &str,
        target_fn: &str,
        replacement: FunctionDef,
        result_name: &str,
    ) -> ShaderResult<ComposedShader> {
        self.compose(
            base_name,
            &[CompositionOp::Override {
                target_fn: target_fn.to_string(),
                replacement,
            }],
            result_name,
        )
    }

    // ─── Merge Helpers ────────────────────────────────────────────────────────

    /// Merge streams by semantic — skip duplicates (same semantic already present).
    fn merge_streams(base: &mut Vec<StreamDeclaration>, mixin: &[StreamDeclaration]) {
        for s in mixin {
            if !base.iter().any(|existing| existing.semantic == s.semantic) {
                base.push(s.clone());
            }
        }
    }

    /// Merge bindings — conflict on same (group, binding) with different name.
    fn merge_bindings(
        base: &mut Vec<ShaderBinding>,
        mixin: &[ShaderBinding],
        _base_name: &str,
        _mixin_name: &str,
    ) -> ShaderResult<()> {
        for b in mixin {
            if let Some(existing) = base.iter().find(|e| e.group == b.group && e.binding == b.binding) {
                if existing.name != b.name {
                    return Err(ShaderError::BindingConflict {
                        group: b.group,
                        binding: b.binding,
                        existing: existing.name.clone(),
                        new: b.name.clone(),
                    });
                }
                // Same (group, binding, name) — skip duplicate.
                continue;
            }
            base.push(b.clone());
        }
        Ok(())
    }

    /// Merge structs by name — mixin overrides same-named structs.
    fn merge_structs(base: &mut Vec<StructDef>, mixin: &[StructDef]) {
        for s in mixin {
            if let Some(idx) = base.iter().position(|existing| existing.name == s.name) {
                base[idx] = s.clone();
            } else {
                base.push(s.clone());
            }
        }
    }

    /// Merge functions by name — mixin overrides same-named overridable functions.
    fn merge_functions(
        base: &mut Vec<FunctionDef>,
        mixin: &[FunctionDef],
        base_name: &str,
        mixin_name: &str,
    ) -> ShaderResult<()> {
        for f in mixin {
            if let Some(idx) = base.iter().position(|existing| existing.name == f.name) {
                if base[idx].overridable {
                    base[idx] = f.clone();
                } else {
                    return Err(ShaderError::FunctionConflict {
                        name: f.name.clone(),
                        module_a: base_name.to_string(),
                        module_b: mixin_name.to_string(),
                    });
                }
            } else {
                base.push(f.clone());
            }
        }
        Ok(())
    }

    /// Merge constants — mixin overrides same-named constants.
    fn merge_constants(base: &mut Vec<ConstantDef>, mixin: &[ConstantDef]) {
        for c in mixin {
            if let Some(idx) = base.iter().position(|existing| existing.name == c.name) {
                base[idx] = c.clone();
            } else {
                base.push(c.clone());
            }
        }
    }

    /// Merge global vars — mixin overrides same-named vars.
    fn merge_global_vars(base: &mut Vec<GlobalVarDef>, mixin: &[GlobalVarDef]) {
        for v in mixin {
            if let Some(idx) = base.iter().position(|existing| existing.name == v.name) {
                base[idx] = v.clone();
            } else {
                base.push(v.clone());
            }
        }
    }

    /// Merge entry point bodies. If the mixin body contains the `base()` token,
    /// it is replaced with the base body wrapped in a block scope.
    ///
    /// Example:
    /// - Base body:   `output.position = vec4<f32>(input.position, 1.0);`
    /// - Mixin body:  `base()\nlet scaled = output.position * 2.0;`
    /// - Result:      `{ var output = output; output.position = vec4<f32>(input.position, 1.0); return output; }\nlet scaled = output.position * 2.0;`
    fn merge_body(base_body: Option<&WgslFragment>, mixin_body: &WgslFragment) -> WgslFragment {
        if mixin_body.source.contains("base()") {
            if let Some(base) = base_body {
                let block = format!(
                    "{{ var output = output; {}; return output; }}",
                    base.source.trim().trim_end_matches(';')
                );
                let merged = mixin_body.source.replace("base()", &block);
                return WgslFragment::labeled(
                    mixin_body.label.clone().unwrap_or_default(),
                    merged,
                );
            } else {
                // No base body — strip the base() token
                let stripped = mixin_body.source.replace("base()", "");
                return WgslFragment::labeled(
                    mixin_body.label.clone().unwrap_or_default(),
                    stripped,
                );
            }
        }
        // Mixin replaces base entirely
        mixin_body.clone()
    }
}

impl Default for ShaderComposer {
    fn default() -> Self {
        Self::new()
    }
}

// ─── Composition Builder ─────────────────────────────────────────────────────

/// Fluent builder API for composing shaders.
pub struct CompositionBuilder {
    base_name: String,
    operations: Vec<CompositionOp>,
    result_name: String,
}

impl CompositionBuilder {
    pub fn new(base_name: impl Into<String>) -> Self {
        let name = base_name.into();
        Self {
            base_name: name.clone(),
            operations: Vec::new(),
            result_name: name,
        }
    }

    pub fn result_name(mut self, name: impl Into<String>) -> Self {
        self.result_name = name.into();
        self
    }

    /// Add a mixin module.
    pub fn mixin(mut self, module_name: impl Into<String>) -> Self {
        self.operations.push(CompositionOp::Mixin(module_name.into()));
        self
    }

    /// Override a specific function.
    pub fn override_fn(mut self, target_fn: impl Into<String>, replacement: FunctionDef) -> Self {
        self.operations.push(CompositionOp::Override {
            target_fn: target_fn.into(),
            replacement,
        });
        self
    }

    /// Add a composed sub-module.
    pub fn compose(mut self, name: impl Into<String>, module_name: impl Into<String>) -> Self {
        self.operations.push(CompositionOp::Compose {
            name: name.into(),
            module: module_name.into(),
        });
        self
    }

    /// Build the composed shader using the given composer.
    pub fn build(self, composer: &ShaderComposer) -> ShaderResult<ComposedShader> {
        composer.compose(&self.base_name, &self.operations, &self.result_name)
    }
}

// ─── Shader Module Builder ────────────────────────────────────────────────────

/// Fluent builder for creating [`ShaderModule`] instances easily.
pub struct ShaderModuleBuilder {
    module: ShaderModule,
}

impl ShaderModuleBuilder {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            module: ShaderModule::new(name),
        }
    }

    pub fn stream(mut self, decl: StreamDeclaration) -> Self {
        self.module.input_streams.push(decl);
        self
    }

    pub fn streams(mut self, decls: Vec<StreamDeclaration>) -> Self {
        self.module.input_streams.extend(decls);
        self
    }

    pub fn binding(mut self, binding: ShaderBinding) -> Self {
        self.module.bindings.push(binding);
        self
    }

    pub fn struct_def(mut self, s: StructDef) -> Self {
        self.module.structs.push(s);
        self
    }

    pub fn function(mut self, f: FunctionDef) -> Self {
        self.module.functions.push(f);
        self
    }

    /// Add an overridable function (convenience shorthand).
    pub fn overridable_function(
        mut self,
        name: impl Into<String>,
        params: Vec<(String, WgslType)>,
        ret: Option<WgslType>,
        body: impl Into<String>,
    ) -> Self {
        self.module.functions.push(FunctionDef {
            name: name.into(),
            parameters: params,
            return_type: ret,
            body: WgslFragment::new(body),
            overridable: true,
        });
        self
    }

    pub fn vertex_body(mut self, body: impl Into<String>) -> Self {
        self.module.vertex_body = Some(WgslFragment::new(body));
        self
    }

    pub fn fragment_body(mut self, body: impl Into<String>) -> Self {
        self.module.fragment_body = Some(WgslFragment::new(body));
        self
    }

    pub fn compute_body(mut self, body: impl Into<String>) -> Self {
        self.module.compute_body = Some(WgslFragment::new(body));
        self
    }

    pub fn constant(mut self, c: ConstantDef) -> Self {
        self.module.constants.push(c);
        self
    }

    pub fn global_var(mut self, v: GlobalVarDef) -> Self {
        self.module.global_vars.push(v);
        self
    }

    pub fn depends_on(mut self, dep: impl Into<String>) -> Self {
        self.module.dependencies.push(dep.into());
        self
    }

    pub fn build(self) -> ShaderModule {
        self.module
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stream::presets;

    /// Build a minimal base module for testing.
    fn make_base_module() -> ShaderModule {
        ShaderModuleBuilder::new("base")
            .stream(presets::position())
            .vertex_body("output.clip_position = vec4<f32>(input.position, 1.0);")
            .fragment_body("return vec4<f32>(1.0, 0.0, 0.0, 1.0);")
            .build()
    }

    /// Build a mixin module that adds a normal stream and a helper function.
    fn make_normal_mixin() -> ShaderModule {
        ShaderModuleBuilder::new("normal_mixin")
            .stream(presets::normal())
            .function(FunctionDef {
                name: "compute_normal".into(),
                parameters: vec![("n".into(), WgslType::Vec3(WgslScalarType::F32))],
                return_type: Some(WgslType::Vec3(WgslScalarType::F32)),
                body: WgslFragment::new("return normalize(n);"),
                overridable: false,
            })
            .build()
    }

    #[test]
    fn test_register_and_get_module() {
        let mut composer = ShaderComposer::new();
        let module = ShaderModule::new("test_mod");
        composer.register_module(module).unwrap();

        assert!(composer.get_module("test_mod").is_some());
        assert!(composer.get_module("nonexistent").is_none());

        let names = composer.module_names();
        assert_eq!(names.len(), 1);
        assert!(names.contains(&"test_mod"));
    }

    #[test]
    fn test_register_duplicate_module() {
        let mut composer = ShaderComposer::new();
        composer.register_module(ShaderModule::new("dup")).unwrap();
        let err = composer.register_module(ShaderModule::new("dup"));
        assert!(err.is_err());
    }

    #[test]
    fn test_simple_mixin() {
        let mut composer = ShaderComposer::new();
        composer.register_module(make_base_module()).unwrap();
        composer.register_module(make_normal_mixin()).unwrap();

        let result = composer.mixin("base", "normal_mixin", "result").unwrap();

        // Should have both position and normal streams
        assert_eq!(result.streams.streams().len(), 2);
        assert!(result.streams.get_by_semantic(&StreamSemantic::Position).is_some());
        assert!(result.streams.get_by_semantic(&StreamSemantic::Normal).is_some());

        // Should have the helper function from the mixin
        assert!(result.functions.iter().any(|f| f.name == "compute_normal"));

        // Mixin replaces base's bodies
        assert!(result.vertex_entry.is_some());
        assert!(result.fragment_entry.is_some());
    }

    #[test]
    fn test_mixin_function_override() {
        let base = ShaderModuleBuilder::new("base")
            .stream(presets::position())
            .overridable_function(
                "compute_lighting",
                vec![("n".into(), WgslType::Vec3(WgslScalarType::F32))],
                Some(WgslType::F32),
                "return 0.5;",
            )
            .vertex_body("output.clip_position = vec4<f32>(input.position, 1.0);")
            .fragment_body("return vec4<f32>(1.0);")
            .build();

        let mixin = ShaderModuleBuilder::new("lighting_mixin")
            .function(FunctionDef {
                name: "compute_lighting".into(),
                parameters: vec![("n".into(), WgslType::Vec3(WgslScalarType::F32))],
                return_type: Some(WgslType::F32),
                body: WgslFragment::new("return max(dot(n, vec3<f32>(0.0, 1.0, 0.0)), 0.0);"),
                overridable: false,
            })
            .build();

        let mut composer = ShaderComposer::new();
        composer.register_module(base).unwrap();
        composer.register_module(mixin).unwrap();

        let result = composer.mixin("base", "lighting_mixin", "lit").unwrap();

        // The mixin's version of compute_lighting should be used
        let fn_def = result.functions.iter().find(|f| f.name == "compute_lighting").unwrap();
        assert!(fn_def.body.source.contains("dot"));
        assert!(!fn_def.body.source.contains("0.5"));
    }

    #[test]
    fn test_mixin_non_overridable_function_conflict() {
        let base = ShaderModuleBuilder::new("base")
            .stream(presets::position())
            .function(FunctionDef {
                name: "locked_fn".into(),
                parameters: vec![],
                return_type: None,
                body: WgslFragment::new("let x = 1;"),
                overridable: false,
            })
            .vertex_body("output.clip_position = vec4<f32>(input.position, 1.0);")
            .fragment_body("return vec4<f32>(1.0);")
            .build();

        let mixin = ShaderModuleBuilder::new("mixin")
            .function(FunctionDef {
                name: "locked_fn".into(),
                parameters: vec![],
                return_type: None,
                body: WgslFragment::new("let y = 2;"),
                overridable: false,
            })
            .build();

        let mut composer = ShaderComposer::new();
        composer.register_module(base).unwrap();
        composer.register_module(mixin).unwrap();

        let result = composer.mixin("base", "mixin", "result");
        assert!(result.is_err());
        match result.unwrap_err() {
            ShaderError::FunctionConflict { name, .. } => assert_eq!(name, "locked_fn"),
            other => panic!("Expected FunctionConflict, got: {:?}", other),
        }
    }

    #[test]
    fn test_explicit_override() {
        let base = ShaderModuleBuilder::new("base")
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

        let mut composer = ShaderComposer::new();
        composer.register_module(base).unwrap();

        let replacement = FunctionDef {
            name: "get_color".into(),
            parameters: vec![],
            return_type: Some(WgslType::Vec4(WgslScalarType::F32)),
            body: WgslFragment::new("return vec4<f32>(0.0, 1.0, 0.0, 1.0);"),
            overridable: false,
        };

        let result = composer
            .override_function("base", "get_color", replacement, "overridden")
            .unwrap();

        let fn_def = result.functions.iter().find(|f| f.name == "get_color").unwrap();
        assert!(fn_def.body.source.contains("0.0, 1.0, 0.0"));
    }

    #[test]
    fn test_override_nonexistent_function() {
        let mut composer = ShaderComposer::new();
        composer.register_module(make_base_module()).unwrap();

        let replacement = FunctionDef {
            name: "nonexistent".into(),
            parameters: vec![],
            return_type: None,
            body: WgslFragment::new(""),
            overridable: false,
        };

        let result = composer.override_function("base", "nonexistent", replacement, "r");
        assert!(result.is_err());
        match result.unwrap_err() {
            ShaderError::Composition { message } => {
                assert!(message.contains("not found"));
            }
            other => panic!("Expected Composition error, got: {:?}", other),
        }
    }

    #[test]
    fn test_override_non_overridable_function() {
        let base = ShaderModuleBuilder::new("base")
            .stream(presets::position())
            .function(FunctionDef {
                name: "locked".into(),
                parameters: vec![],
                return_type: None,
                body: WgslFragment::new("let x = 1;"),
                overridable: false,
            })
            .vertex_body("output.clip_position = vec4<f32>(input.position, 1.0);")
            .fragment_body("return vec4<f32>(1.0);")
            .build();

        let mut composer = ShaderComposer::new();
        composer.register_module(base).unwrap();

        let replacement = FunctionDef {
            name: "locked".into(),
            parameters: vec![],
            return_type: None,
            body: WgslFragment::new("let y = 2;"),
            overridable: false,
        };

        let result = composer.override_function("base", "locked", replacement, "r");
        assert!(result.is_err());
        match result.unwrap_err() {
            ShaderError::Composition { message } => {
                assert!(message.contains("not overridable"));
            }
            other => panic!("Expected Composition error, got: {:?}", other),
        }
    }

    #[test]
    fn test_compose_namespace() {
        let base = ShaderModuleBuilder::new("base")
            .stream(presets::position())
            .vertex_body("output.clip_position = vec4<f32>(input.position, 1.0);")
            .fragment_body("return vec4<f32>(1.0);")
            .build();

        let lighting = ShaderModuleBuilder::new("lighting_mod")
            .function(FunctionDef {
                name: "compute".into(),
                parameters: vec![("n".into(), WgslType::Vec3(WgslScalarType::F32))],
                return_type: Some(WgslType::F32),
                body: WgslFragment::new("return n.x;"),
                overridable: false,
            })
            .struct_def(StructDef {
                name: "LightData".into(),
                fields: vec![StructField {
                    name: "intensity".into(),
                    ty: WgslType::F32,
                    attributes: vec![],
                }],
            })
            .build();

        let mut composer = ShaderComposer::new();
        composer.register_module(base).unwrap();
        composer.register_module(lighting).unwrap();

        let result = composer
            .compose(
                "base",
                &[CompositionOp::Compose {
                    name: "lighting".into(),
                    module: "lighting_mod".into(),
                }],
                "composed",
            )
            .unwrap();

        // Function should be prefixed
        assert!(result.functions.iter().any(|f| f.name == "lighting_compute"));
        // Struct should be prefixed
        assert!(result.structs.iter().any(|s| s.name == "lighting_LightData"));
    }

    #[test]
    fn test_binding_conflict_detection() {
        let base = ShaderModuleBuilder::new("base")
            .stream(presets::position())
            .binding(ShaderBinding {
                group: 0,
                binding: 0,
                name: "tex_a".into(),
                resource_type: BindingResourceType::Texture {
                    dimension: TextureDimension::D2,
                    sample_type: TextureSampleType::Float,
                },
                visibility: wgpu::ShaderStages::FRAGMENT,
            })
            .vertex_body("output.clip_position = vec4<f32>(input.position, 1.0);")
            .fragment_body("return vec4<f32>(1.0);")
            .build();

        let mixin = ShaderModuleBuilder::new("mixin")
            .binding(ShaderBinding {
                group: 0,
                binding: 0,
                name: "tex_b".into(),
                resource_type: BindingResourceType::Texture {
                    dimension: TextureDimension::D2,
                    sample_type: TextureSampleType::Float,
                },
                visibility: wgpu::ShaderStages::FRAGMENT,
            })
            .build();

        let mut composer = ShaderComposer::new();
        composer.register_module(base).unwrap();
        composer.register_module(mixin).unwrap();

        let result = composer.mixin("base", "mixin", "conflict");
        assert!(result.is_err());
        match result.unwrap_err() {
            ShaderError::BindingConflict {
                group,
                binding,
                existing,
                new,
            } => {
                assert_eq!(group, 0);
                assert_eq!(binding, 0);
                assert_eq!(existing, "tex_a");
                assert_eq!(new, "tex_b");
            }
            other => panic!("Expected BindingConflict, got: {:?}", other),
        }
    }

    #[test]
    fn test_stream_merge_no_duplicates() {
        let base = ShaderModuleBuilder::new("base")
            .stream(presets::position())
            .stream(presets::normal())
            .vertex_body("output.clip_position = vec4<f32>(input.position, 1.0);")
            .fragment_body("return vec4<f32>(1.0);")
            .build();

        // Mixin also declares position stream — should not duplicate
        let mixin = ShaderModuleBuilder::new("mixin")
            .stream(presets::position())
            .stream(presets::uv(0))
            .build();

        let mut composer = ShaderComposer::new();
        composer.register_module(base).unwrap();
        composer.register_module(mixin).unwrap();

        let result = composer.mixin("base", "mixin", "merged").unwrap();

        // position, normal, uv(0) — 3 streams total
        assert_eq!(result.streams.streams().len(), 3);
        assert!(result.streams.get_by_semantic(&StreamSemantic::Position).is_some());
        assert!(result.streams.get_by_semantic(&StreamSemantic::Normal).is_some());
        assert!(result.streams.get_by_semantic(&StreamSemantic::UV(0)).is_some());
    }

    #[test]
    fn test_composition_builder_api() {
        let base = make_base_module();
        let mixin = make_normal_mixin();
        let sub = ShaderModuleBuilder::new("sub_mod")
            .function(FunctionDef {
                name: "helper".into(),
                parameters: vec![],
                return_type: Some(WgslType::F32),
                body: WgslFragment::new("return 1.0;"),
                overridable: false,
            })
            .build();

        let mut composer = ShaderComposer::new();
        composer.register_module(base).unwrap();
        composer.register_module(mixin).unwrap();
        composer.register_module(sub).unwrap();

        let result = CompositionBuilder::new("base")
            .result_name("composed_shader")
            .mixin("normal_mixin")
            .compose("sub", "sub_mod")
            .build(&composer)
            .unwrap();

        assert_eq!(result.name, "composed_shader");
        // position + normal from mixin
        assert_eq!(result.streams.streams().len(), 2);
        // compute_normal from mixin + sub_helper from composed
        assert!(result.functions.iter().any(|f| f.name == "compute_normal"));
        assert!(result.functions.iter().any(|f| f.name == "sub_helper"));
    }

    #[test]
    fn test_base_call_in_mixin() {
        let base = ShaderModuleBuilder::new("base")
            .stream(presets::position())
            .vertex_body("output.clip_position = vec4<f32>(input.position, 1.0);")
            .fragment_body("return vec4<f32>(1.0, 0.0, 0.0, 1.0);")
            .build();

        // Mixin body uses base() token
        let mixin = ShaderModuleBuilder::new("transform_mixin")
            .vertex_body("base()\noutput.clip_position = output.clip_position * 2.0;")
            .fragment_body("base()")
            .build();

        let mut composer = ShaderComposer::new();
        composer.register_module(base).unwrap();
        composer.register_module(mixin).unwrap();

        let result = composer.mixin("base", "transform_mixin", "transformed").unwrap();

        // The vertex body should contain both the base code and the mixin code
        let vs_body = &result.vertex_entry.as_ref().unwrap().body.source;
        assert!(
            vs_body.contains("input.position"),
            "Should contain base body content: {}",
            vs_body
        );
        assert!(
            vs_body.contains("* 2.0"),
            "Should contain mixin body content: {}",
            vs_body
        );

        // Fragment body with just base() should resolve to the base's body
        let fs_body = &result.fragment_entry.as_ref().unwrap().body.source;
        assert!(
            fs_body.contains("1.0, 0.0, 0.0"),
            "Should contain base fragment body: {}",
            fs_body
        );
    }

    #[test]
    fn test_end_to_end_generate_wgsl() {
        // Build a PBR-like base shader
        let base = ShaderModuleBuilder::new("pbr_base")
            .stream(presets::position())
            .stream(presets::normal())
            .stream(presets::uv(0))
            .overridable_function(
                "compute_lighting",
                vec![("n".into(), WgslType::Vec3(WgslScalarType::F32))],
                Some(WgslType::Vec3(WgslScalarType::F32)),
                "return vec3<f32>(0.5);",
            )
            .vertex_body("output.clip_position = vec4<f32>(input.position, 1.0);\noutput.normal = input.normal;\noutput.uv0 = input.uv0;")
            .fragment_body("let color = compute_lighting(input.normal);\nreturn vec4<f32>(color, 1.0);")
            .build();

        // Build a skinning mixin
        let skinning = ShaderModuleBuilder::new("skinning")
            .stream(presets::bone_weights())
            .stream(presets::bone_indices())
            .function(FunctionDef {
                name: "apply_skinning".into(),
                parameters: vec![("pos".into(), WgslType::Vec3(WgslScalarType::F32))],
                return_type: Some(WgslType::Vec3(WgslScalarType::F32)),
                body: WgslFragment::new("return pos;"),
                overridable: false,
            })
            .build();

        let mut composer = ShaderComposer::new();
        composer.register_module(base).unwrap();
        composer.register_module(skinning).unwrap();

        let result = composer.mixin("pbr_base", "skinning", "skinned_pbr").unwrap();

        // Should have all streams merged
        assert_eq!(result.streams.streams().len(), 5);
        assert!(result.streams.get_by_semantic(&StreamSemantic::BoneWeights).is_some());
        assert!(result.streams.get_by_semantic(&StreamSemantic::BoneIndices).is_some());

        // Generate WGSL
        let generator = WgslGenerator::new();
        let wgsl = generator.generate(&result).unwrap();

        // Verify structure
        assert!(wgsl.contains("struct VertexInput {"));
        assert!(wgsl.contains("@vertex"));
        assert!(wgsl.contains("@fragment"));
        assert!(wgsl.contains("fn compute_lighting"));
        assert!(wgsl.contains("fn apply_skinning"));

        // Validate with naga
        let validated = generator.generate_and_validate(&result);
        assert!(validated.is_ok(), "Naga validation failed: {:?}", validated.err());
    }

    #[test]
    fn test_shader_module_builder() {
        let module = ShaderModuleBuilder::new("test_module")
            .stream(presets::position())
            .streams(vec![presets::normal(), presets::uv(0)])
            .binding(ShaderBinding {
                group: 0,
                binding: 0,
                name: "my_tex".into(),
                resource_type: BindingResourceType::Texture {
                    dimension: TextureDimension::D2,
                    sample_type: TextureSampleType::Float,
                },
                visibility: wgpu::ShaderStages::FRAGMENT,
            })
            .struct_def(StructDef {
                name: "MyStruct".into(),
                fields: vec![StructField {
                    name: "val".into(),
                    ty: WgslType::F32,
                    attributes: vec![],
                }],
            })
            .function(FunctionDef {
                name: "my_fn".into(),
                parameters: vec![],
                return_type: None,
                body: WgslFragment::new("let x = 1;"),
                overridable: false,
            })
            .overridable_function(
                "overridable_fn",
                vec![],
                Some(WgslType::F32),
                "return 0.0;",
            )
            .vertex_body("output.clip_position = vec4<f32>(input.position, 1.0);")
            .fragment_body("return vec4<f32>(1.0);")
            .constant(ConstantDef {
                name: "k".into(),
                ty: WgslType::F32,
                id: 0,
                default_value: Some("1.0".into()),
            })
            .global_var(GlobalVarDef {
                name: "temp".into(),
                ty: WgslType::F32,
                address_space: AddressSpace::Private,
                init: None,
            })
            .depends_on("other_module")
            .build();

        assert_eq!(module.name, "test_module");
        assert_eq!(module.input_streams.len(), 3);
        assert_eq!(module.bindings.len(), 1);
        assert_eq!(module.structs.len(), 1);
        assert_eq!(module.functions.len(), 2);
        assert!(module.functions[1].overridable);
        assert!(module.vertex_body.is_some());
        assert!(module.fragment_body.is_some());
        assert_eq!(module.constants.len(), 1);
        assert_eq!(module.global_vars.len(), 1);
        assert_eq!(module.dependencies, vec!["other_module"]);
    }

    #[test]
    fn test_module_with_methods() {
        let module = ShaderModule::new("test")
            .with_stream(presets::position())
            .with_streams(vec![presets::normal()])
            .with_binding(ShaderBinding {
                group: 0,
                binding: 0,
                name: "tex".into(),
                resource_type: BindingResourceType::Texture {
                    dimension: TextureDimension::D2,
                    sample_type: TextureSampleType::Float,
                },
                visibility: wgpu::ShaderStages::FRAGMENT,
            })
            .with_struct(StructDef {
                name: "S".into(),
                fields: vec![],
            })
            .with_function(FunctionDef {
                name: "f".into(),
                parameters: vec![],
                return_type: None,
                body: WgslFragment::new(""),
                overridable: false,
            })
            .with_vertex_body("let x = 1;")
            .with_fragment_body("return vec4<f32>(1.0);")
            .with_constant(ConstantDef {
                name: "c".into(),
                ty: WgslType::F32,
                id: 0,
                default_value: None,
            })
            .with_global_var(GlobalVarDef {
                name: "g".into(),
                ty: WgslType::F32,
                address_space: AddressSpace::Private,
                init: None,
            });

        assert_eq!(module.name, "test");
        assert_eq!(module.input_streams.len(), 2);
        assert_eq!(module.bindings.len(), 1);
        assert_eq!(module.structs.len(), 1);
        assert_eq!(module.functions.len(), 1);
        assert!(module.vertex_body.is_some());
        assert!(module.fragment_body.is_some());
        assert_eq!(module.constants.len(), 1);
        assert_eq!(module.global_vars.len(), 1);
    }

    #[test]
    fn test_compose_module_not_found() {
        let mut composer = ShaderComposer::new();
        composer.register_module(make_base_module()).unwrap();

        let result = composer.compose(
            "base",
            &[CompositionOp::Mixin("nonexistent".into())],
            "r",
        );
        assert!(result.is_err());

        let result = composer.compose(
            "nonexistent",
            &[],
            "r",
        );
        assert!(result.is_err());

        let result = composer.compose(
            "base",
            &[CompositionOp::Compose {
                name: "sub".into(),
                module: "nonexistent".into(),
            }],
            "r",
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_compose_multiple_operations() {
        let base = ShaderModuleBuilder::new("base")
            .stream(presets::position())
            .overridable_function(
                "lighting",
                vec![],
                Some(WgslType::F32),
                "return 0.5;",
            )
            .vertex_body("output.clip_position = vec4<f32>(input.position, 1.0);")
            .fragment_body("return vec4<f32>(lighting());")
            .build();

        let normal_mixin = ShaderModuleBuilder::new("normals")
            .stream(presets::normal())
            .build();

        let utils = ShaderModuleBuilder::new("utils")
            .function(FunctionDef {
                name: "saturate".into(),
                parameters: vec![("x".into(), WgslType::F32)],
                return_type: Some(WgslType::F32),
                body: WgslFragment::new("return clamp(x, 0.0, 1.0);"),
                overridable: false,
            })
            .build();

        let mut composer = ShaderComposer::new();
        composer.register_module(base).unwrap();
        composer.register_module(normal_mixin).unwrap();
        composer.register_module(utils).unwrap();

        let replacement = FunctionDef {
            name: "lighting".into(),
            parameters: vec![],
            return_type: Some(WgslType::F32),
            body: WgslFragment::new("return 1.0;"),
            overridable: false,
        };

        let result = CompositionBuilder::new("base")
            .result_name("multi_op")
            .mixin("normals")
            .compose("util", "utils")
            .override_fn("lighting", replacement)
            .build(&composer)
            .unwrap();

        // normal stream from mixin
        assert_eq!(result.streams.streams().len(), 2);
        // util_saturate from composed module
        assert!(result.functions.iter().any(|f| f.name == "util_saturate"));
        // lighting overridden
        let lighting = result.functions.iter().find(|f| f.name == "lighting").unwrap();
        assert!(lighting.body.source.contains("1.0"));
        assert!(!lighting.body.source.contains("0.5"));
    }

    #[test]
    fn test_binding_same_slot_same_name_ok() {
        // Two modules declaring the exact same binding (same group:binding:name) should not conflict
        let base = ShaderModuleBuilder::new("base")
            .stream(presets::position())
            .binding(ShaderBinding {
                group: 0,
                binding: 0,
                name: "shared_tex".into(),
                resource_type: BindingResourceType::Texture {
                    dimension: TextureDimension::D2,
                    sample_type: TextureSampleType::Float,
                },
                visibility: wgpu::ShaderStages::FRAGMENT,
            })
            .vertex_body("output.clip_position = vec4<f32>(input.position, 1.0);")
            .fragment_body("return vec4<f32>(1.0);")
            .build();

        let mixin = ShaderModuleBuilder::new("mixin")
            .binding(ShaderBinding {
                group: 0,
                binding: 0,
                name: "shared_tex".into(),
                resource_type: BindingResourceType::Texture {
                    dimension: TextureDimension::D2,
                    sample_type: TextureSampleType::Float,
                },
                visibility: wgpu::ShaderStages::FRAGMENT,
            })
            .build();

        let mut composer = ShaderComposer::new();
        composer.register_module(base).unwrap();
        composer.register_module(mixin).unwrap();

        let result = composer.mixin("base", "mixin", "ok").unwrap();
        // Should only have one binding (no duplicate)
        assert_eq!(result.bindings.len(), 1);
    }

    #[test]
    fn test_default_composer() {
        let composer = ShaderComposer::default();
        assert!(composer.module_names().is_empty());
    }

    #[test]
    fn test_shader_module_def_trait() {
        let module = ShaderModuleBuilder::new("trait_test")
            .stream(presets::position())
            .function(FunctionDef {
                name: "f".into(),
                parameters: vec![],
                return_type: None,
                body: WgslFragment::new(""),
                overridable: false,
            })
            .depends_on("dep_a")
            .build();

        // Test trait methods via the ShaderModuleDef trait
        let def: &dyn ShaderModuleDef = &module;
        assert_eq!(def.name(), "trait_test");
        assert_eq!(def.input_streams().len(), 1);
        assert_eq!(def.functions().len(), 1);
        assert_eq!(def.dependencies(), vec!["dep_a"]);
    }

    #[test]
    fn test_end_to_end_with_override_generates_valid_wgsl() {
        let base = ShaderModuleBuilder::new("base")
            .stream(presets::position())
            .overridable_function(
                "get_scale",
                vec![],
                Some(WgslType::F32),
                "return 1.0;",
            )
            .vertex_body("let s = get_scale();\noutput.clip_position = vec4<f32>(input.position * s, 1.0);")
            .fragment_body("return vec4<f32>(1.0);")
            .build();

        let mut composer = ShaderComposer::new();
        composer.register_module(base).unwrap();

        let replacement = FunctionDef {
            name: "get_scale".into(),
            parameters: vec![],
            return_type: Some(WgslType::F32),
            body: WgslFragment::new("return 2.0;"),
            overridable: false,
        };

        let result = composer
            .override_function("base", "get_scale", replacement, "scaled")
            .unwrap();

        let generator = WgslGenerator::new();
        let wgsl = generator.generate_and_validate(&result);
        assert!(wgsl.is_ok(), "Naga validation failed: {:?}", wgsl.err());

        let src = wgsl.unwrap();
        assert!(src.contains("return 2.0;"));
        assert!(!src.contains("return 1.0;"));
    }
}
