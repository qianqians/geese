//! Shader variant management — compile-time permutation system.
//!
//! Variants are compile-time permutations of a shader (e.g. with/without
//! skinning, with/without shadow casting). This module provides a
//! variant selector, cache, and WGSL override-based code generation,
//! similar to Unity/Unreal shader permutation systems.

use crate::compose::*;
use crate::core::*;
use crate::generator::*;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};

// ─── Variant Value ───────────────────────────────────────────────────────────

/// A variant feature value.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum VariantValue {
    Bool(bool),
    Int(i32),
    Enum(String),
}

impl VariantValue {
    /// Convert to a WGSL-compatible constant expression.
    pub fn to_wgsl_expr(&self) -> String {
        match self {
            VariantValue::Bool(b) => b.to_string(),
            VariantValue::Int(i) => i.to_string(),
            VariantValue::Enum(s) => format!("\"{s}\""),
        }
    }

    /// Convert to a WGSL `override` default literal for the given type.
    fn to_wgsl_override_default(&self) -> String {
        match self {
            VariantValue::Bool(b) => b.to_string(),
            VariantValue::Int(i) => format!("{i}"),
            VariantValue::Enum(s) => {
                // Enums are represented as u32 indices; caller maps index.
                // Fallback: emit 0.
                let _ = s;
                "0u".to_string()
            }
        }
    }

}

impl std::fmt::Display for VariantValue {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            VariantValue::Bool(b) => write!(f, "{b}"),
            VariantValue::Int(i) => write!(f, "{i}"),
            VariantValue::Enum(s) => write!(f, "{s}"),
        }
    }
}

// ─── Variant Key ─────────────────────────────────────────────────────────────

/// An ordered set of feature flags that uniquely identifies a variant.
///
/// Uses `BTreeMap` internally so that iteration order is deterministic
/// regardless of insertion order, ensuring stable hashing.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct VariantKey {
    features: BTreeMap<String, VariantValue>,
}

impl VariantKey {
    pub fn new() -> Self {
        Self {
            features: BTreeMap::new(),
        }
    }

    pub fn set(mut self, name: impl Into<String>, value: VariantValue) -> Self {
        self.features.insert(name.into(), value);
        self
    }

    pub fn get(&self, name: &str) -> Option<&VariantValue> {
        self.features.get(name)
    }

    pub fn is_enabled(&self, name: &str) -> bool {
        matches!(self.features.get(name), Some(VariantValue::Bool(true)))
    }

    pub fn features(&self) -> &BTreeMap<String, VariantValue> {
        &self.features
    }

    pub fn is_empty(&self) -> bool {
        self.features.is_empty()
    }
}

impl Default for VariantKey {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for VariantKey {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let pairs: Vec<String> = self
            .features
            .iter()
            .map(|(k, v)| format!("{k}={v}"))
            .collect();
        write!(f, "{}", pairs.join("+"))
    }
}

// ─── Feature Definitions ─────────────────────────────────────────────────────

/// Definition of a single feature flag.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeatureDef {
    pub name: String,
    /// All possible values this feature can take.
    pub possible_values: Vec<VariantValue>,
    /// Default value when not specified.
    pub default_value: VariantValue,
    /// Features this feature depends on (must be present and have specific values).
    #[serde(default)]
    pub dependencies: Vec<FeatureDependency>,
    /// Features that are mutually exclusive with this one.
    #[serde(default)]
    pub exclusive_with: Vec<String>,
}

/// A dependency: feature X requires feature Y to have a specific value.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeatureDependency {
    pub feature: String,
    pub required_value: VariantValue,
}

// ─── Shader Variant ──────────────────────────────────────────────────────────

/// A generated shader variant.
#[derive(Debug, Clone)]
pub struct ShaderVariant {
    pub base_shader: String,
    pub key: VariantKey,
    pub wgsl_source: String,
}

// ─── Cache Stats ─────────────────────────────────────────────────────────────

/// Cache statistics.
#[derive(Debug, Clone)]
pub struct CacheStats {
    pub hits: u64,
    pub misses: u64,
    pub total_cached: usize,
}

impl std::fmt::Display for CacheStats {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let total = self.hits + self.misses;
        let rate = if total > 0 {
            self.hits as f64 / total as f64 * 100.0
        } else {
            0.0
        };
        write!(
            f,
            "Cache: {} hits, {} misses, {:.1}% hit rate, {} cached",
            self.hits, self.misses, rate, self.total_cached
        )
    }
}

// ─── Variant Manager ─────────────────────────────────────────────────────────

/// Manages shader variant generation and caching.
pub struct VariantManager {
    /// Feature definitions per base shader.
    feature_defs: HashMap<String, Vec<FeatureDef>>,
    /// Cache of generated variants: (base_shader, variant_key) -> ShaderVariant
    cache: HashMap<(String, VariantKey), ShaderVariant>,
    /// Cache statistics.
    hits: u64,
    misses: u64,
}

impl VariantManager {
    pub fn new() -> Self {
        Self {
            feature_defs: HashMap::new(),
            cache: HashMap::new(),
            hits: 0,
            misses: 0,
        }
    }

    /// Register feature definitions for a base shader.
    pub fn register(
        &mut self,
        base_shader: impl Into<String>,
        features: Vec<FeatureDef>,
    ) -> ShaderResult<()> {
        let name = base_shader.into();

        // No duplicate feature names.
        let mut seen = std::collections::HashSet::new();
        for f in &features {
            if !seen.insert(&f.name) {
                return Err(ShaderError::Variant {
                    message: format!("Duplicate feature name '{}' in shader '{}'", f.name, name),
                });
            }
        }

        let feature_names: std::collections::HashSet<&str> =
            features.iter().map(|f| f.name.as_str()).collect();

        for f in &features {
            // Default value must be in possible_values.
            if !f.possible_values.contains(&f.default_value) {
                return Err(ShaderError::Variant {
                    message: format!(
                        "Feature '{}' default value '{}' not in possible_values",
                        f.name, f.default_value
                    ),
                });
            }

            // Dependency references must exist.
            for dep in &f.dependencies {
                if !feature_names.contains(dep.feature.as_str()) {
                    return Err(ShaderError::Variant {
                        message: format!(
                            "Feature '{}' depends on '{}' which is not defined",
                            f.name, dep.feature
                        ),
                    });
                }
            }

            // Exclusive_with references must exist.
            for excl in &f.exclusive_with {
                if !feature_names.contains(excl.as_str()) {
                    return Err(ShaderError::Variant {
                        message: format!(
                            "Feature '{}' is exclusive with '{}' which is not defined",
                            f.name, excl
                        ),
                    });
                }
            }
        }

        self.feature_defs.insert(name, features);
        Ok(())
    }

    /// Get or generate a specific variant using the base module directly.
    ///
    /// Looks up the base module from the composer, composes it, injects
    /// variant features as WGSL `override` constants, and caches the result.
    pub fn get_or_create(
        &mut self,
        base_shader: &str,
        key: &VariantKey,
        composer: &ShaderComposer,
    ) -> ShaderResult<&ShaderVariant> {
        self.validate_key(base_shader, key)?;

        let cache_key = (base_shader.to_string(), key.clone());
        if self.cache.contains_key(&cache_key) {
            self.hits += 1;
            return Ok(self.cache.get(&cache_key).unwrap());
        }

        self.misses += 1;

        // Resolve the effective key (fill in defaults for missing features).
        let effective_key = self.resolve_key(base_shader, key)?;

        let base_module = composer.get_module(base_shader).ok_or_else(|| {
            ShaderError::Variant {
                message: format!("Base shader module '{}' not found in composer", base_shader),
            }
        })?;

        let wgsl = self.generate_variant_wgsl(base_module, &effective_key)?;

        let variant = ShaderVariant {
            base_shader: base_shader.to_string(),
            key: effective_key.clone(),
            wgsl_source: wgsl,
        };

        self.cache.insert(cache_key.clone(), variant);
        Ok(self.cache.get(&cache_key).unwrap())
    }

    /// Get or generate a variant using an already-composed shader as the base.
    ///
    /// This is useful when the shader has been composed from multiple modules
    /// via mixin/compose/override operations, and the caller wants to generate
    /// variants on top of the composed result.
    pub fn get_or_create_with_composition(
        &mut self,
        base_name: &str,
        composed: &ComposedShader,
        key: &VariantKey,
    ) -> ShaderResult<&ShaderVariant> {
        self.validate_key(base_name, key)?;

        let cache_key = (base_name.to_string(), key.clone());
        if self.cache.contains_key(&cache_key) {
            self.hits += 1;
            return Ok(self.cache.get(&cache_key).unwrap());
        }

        self.misses += 1;

        let effective_key = self.resolve_key(base_name, key)?;

        // Build variant constants from features and inject into the composed shader.
        let mut variant_shader = composed.clone();
        let variant_constants = self.build_variant_constants(&effective_key);

        // Assign IDs starting after any existing constants.
        let base_id = variant_shader
            .constants
            .iter()
            .map(|c| c.id)
            .max()
            .map(|m| m + 1)
            .unwrap_or(0);

        for (i, mut vc) in variant_constants.into_iter().enumerate() {
            vc.id = base_id + i as u32;
            variant_shader.constants.push(vc);
        }

        let generator = WgslGenerator::new();
        let wgsl = generator.generate(&variant_shader)?;

        let variant = ShaderVariant {
            base_shader: base_name.to_string(),
            key: effective_key.clone(),
            wgsl_source: wgsl,
        };

        self.cache.insert(cache_key.clone(), variant);
        Ok(self.cache.get(&cache_key).unwrap())
    }

    /// Enumerate all valid variant keys for a base shader.
    pub fn enumerate(&self, base_shader: &str) -> ShaderResult<Vec<VariantKey>> {
        let features = self.feature_defs.get(base_shader).ok_or_else(|| {
            ShaderError::Variant {
                message: format!("No features registered for shader '{}'", base_shader),
            }
        })?;

        if features.is_empty() {
            return Ok(vec![VariantKey::new()]);
        }

        let mut results = Vec::new();
        let mut current: Vec<(String, VariantValue)> = Vec::new();
        self.enumerate_recursive(features, 0, &mut current, &mut results);
        Ok(results)
    }

    /// Get the total number of valid variants for a base shader.
    pub fn variant_count(&self, base_shader: &str) -> ShaderResult<usize> {
        Ok(self.enumerate(base_shader)?.len())
    }

    /// Validate a variant key against the feature definitions.
    pub fn validate_key(&self, base_shader: &str, key: &VariantKey) -> ShaderResult<()> {
        let features = self.feature_defs.get(base_shader).ok_or_else(|| {
            ShaderError::Variant {
                message: format!("No features registered for shader '{}'", base_shader),
            }
        })?;

        let feature_map: HashMap<&str, &FeatureDef> =
            features.iter().map(|f| (f.name.as_str(), f)).collect();

        // Check all features in key are defined and values are valid.
        for (name, value) in key.features() {
            let feat = feature_map.get(name.as_str()).ok_or_else(|| {
                ShaderError::Variant {
                    message: format!("Feature '{}' is not defined for shader '{}'", name, base_shader),
                }
            })?;

            if !feat.possible_values.contains(value) {
                return Err(ShaderError::Variant {
                    message: format!(
                        "Feature '{}' value '{}' is not in possible_values",
                        name, value
                    ),
                });
            }
        }

        // Build effective assignment (key values + defaults for missing).
        let mut assignment: HashMap<String, VariantValue> = HashMap::new();
        for feat in features {
            if let Some(v) = key.get(&feat.name) {
                assignment.insert(feat.name.clone(), v.clone());
            } else {
                assignment.insert(feat.name.clone(), feat.default_value.clone());
            }
        }

        // Check dependencies.
        for feat in features {
            let feat_val = assignment.get(&feat.name).unwrap();
            if *feat_val != feat.default_value {
                for dep in &feat.dependencies {
                    let dep_val = assignment.get(&dep.feature).unwrap();
                    if *dep_val != dep.required_value {
                        return Err(ShaderError::Variant {
                            message: format!(
                                "Feature '{}' = '{}' requires '{}' = '{}', but got '{}'",
                                feat.name, feat_val, dep.feature, dep.required_value, dep_val
                            ),
                        });
                    }
                }
            }
        }

        // Check mutual exclusions.
        for feat in features {
            let feat_val = assignment.get(&feat.name).unwrap();
            if *feat_val != feat.default_value {
                for excl_name in &feat.exclusive_with {
                    let excl_feat = feature_map.get(excl_name.as_str()).unwrap();
                    let excl_val = assignment.get(excl_name).unwrap();
                    if *excl_val != excl_feat.default_value {
                        return Err(ShaderError::Variant {
                            message: format!(
                                "Feature '{}' = '{}' is mutually exclusive with '{}' = '{}'",
                                feat.name, feat_val, excl_name, excl_val
                            ),
                        });
                    }
                }
            }
        }

        Ok(())
    }

    /// Get cache statistics.
    pub fn cache_stats(&self) -> CacheStats {
        CacheStats {
            hits: self.hits,
            misses: self.misses,
            total_cached: self.cache.len(),
        }
    }

    /// Clear the variant cache.
    pub fn clear_cache(&mut self) {
        self.cache.clear();
        self.hits = 0;
        self.misses = 0;
    }

    /// Get all registered base shader names.
    pub fn registered_shaders(&self) -> Vec<&str> {
        self.feature_defs.keys().map(|k| k.as_str()).collect()
    }

    /// Get feature definitions for a base shader.
    pub fn get_features(&self, base_shader: &str) -> Option<&[FeatureDef]> {
        self.feature_defs.get(base_shader).map(|v| v.as_slice())
    }

    // ─── Private helpers ─────────────────────────────────────────────────────

    /// Recursive backtracking enumeration of all valid feature combinations.
    fn enumerate_recursive(
        &self,
        features: &[FeatureDef],
        idx: usize,
        current: &mut Vec<(String, VariantValue)>,
        results: &mut Vec<VariantKey>,
    ) {
        if idx == features.len() {
            let mut key = VariantKey::new();
            for (name, val) in current.iter() {
                key = key.set(name.clone(), val.clone());
            }
            results.push(key);
            return;
        }

        let feat = &features[idx];

        // Collect valid values for this feature, using a scoped block
        // so that borrows of `current` are released before recursion.
        let valid_values: Vec<VariantValue> = {
            let assignment: HashMap<&str, &VariantValue> = current
                .iter()
                .map(|(k, v)| (k.as_str(), v))
                .collect();

            feat.possible_values
                .iter()
                .filter(|val| {
                    let is_non_default = **val != feat.default_value;

                    // Check dependencies: non-default value requires deps to be met.
                    if is_non_default {
                        for dep in &feat.dependencies {
                            match assignment.get(dep.feature.as_str()) {
                                Some(dep_val) => {
                                    if **dep_val != dep.required_value {
                                        return false;
                                    }
                                }
                                None => return false,
                            }
                        }
                    }

                    // Check forward exclusions: non-default value requires excluded
                    // features to still be at default.
                    if is_non_default {
                        for excl_name in &feat.exclusive_with {
                            if let Some(excl_val) = assignment.get(excl_name.as_str()) {
                                let excl_feat =
                                    features.iter().find(|f| f.name == *excl_name).unwrap();
                                if **excl_val != excl_feat.default_value {
                                    return false;
                                }
                            }
                        }
                    }

                    // Check reverse exclusions: a previously assigned feature excludes this one.
                    for (prev_name, prev_val) in current.iter() {
                        let prev_feat =
                            features.iter().find(|f| f.name == *prev_name).unwrap();
                        if prev_feat.exclusive_with.contains(&feat.name)
                            && *prev_val != prev_feat.default_value
                            && is_non_default
                        {
                            return false;
                        }
                    }

                    // Check reverse dependencies: a previously assigned feature depends on this one.
                    for (prev_name, prev_val) in current.iter() {
                        let prev_feat =
                            features.iter().find(|f| f.name == *prev_name).unwrap();
                        if *prev_val != prev_feat.default_value {
                            for dep in &prev_feat.dependencies {
                                if dep.feature == feat.name
                                    && *val != &dep.required_value
                                {
                                    return false;
                                }
                            }
                        }
                    }

                    true
                })
                .cloned()
                .collect()
        }; // assignment dropped here

        for val in valid_values {
            current.push((feat.name.clone(), val));
            self.enumerate_recursive(features, idx + 1, current, results);
            current.pop();
        }
    }

    /// Resolve a key by filling in defaults for unspecified features.
    fn resolve_key(&self, base_shader: &str, key: &VariantKey) -> ShaderResult<VariantKey> {
        let features = self.feature_defs.get(base_shader).ok_or_else(|| {
            ShaderError::Variant {
                message: format!("No features registered for shader '{}'", base_shader),
            }
        })?;

        let mut resolved = key.clone();
        for feat in features {
            if !key.features().contains_key(&feat.name) {
                resolved = resolved.set(feat.name.clone(), feat.default_value.clone());
            }
        }
        Ok(resolved)
    }

    /// Build WGSL override constant definitions from a variant key.
    fn build_variant_constants(&self, key: &VariantKey) -> Vec<ConstantDef> {
        key.features()
            .iter()
            .enumerate()
            .map(|(i, (name, value))| {
                let ty = match value {
                    VariantValue::Bool(_) => WgslType::Bool,
                    VariantValue::Int(_) => WgslType::I32,
                    VariantValue::Enum(_) => WgslType::U32,
                };
                ConstantDef {
                    name: name.clone(),
                    ty,
                    id: i as u32,
                    default_value: Some(value.to_wgsl_override_default()),
                }
            })
            .collect()
    }

    /// Generate variant WGSL by injecting feature constants into the base module.
    fn generate_variant_wgsl(
        &self,
        base_module: &ShaderModule,
        key: &VariantKey,
    ) -> ShaderResult<String> {
        // Build a ComposedShader from the base module.
        let mut router = crate::stream::StreamRouter::new();
        for stream in &base_module.input_streams {
            router.add_stream(stream.clone())?;
        }

        let mut constants = base_module.constants.clone();
        let variant_constants = self.build_variant_constants(key);
        let base_id = constants
            .iter()
            .map(|c| c.id)
            .max()
            .map(|m| m + 1)
            .unwrap_or(0);
        for (i, mut vc) in variant_constants.into_iter().enumerate() {
            vc.id = base_id + i as u32;
            constants.push(vc);
        }

        let composed = ComposedShader {
            name: format!("{}_variant_{}", base_module.name, key),
            streams: router,
            bindings: base_module.bindings.clone(),
            structs: base_module.structs.clone(),
            functions: base_module.functions.clone(),
            vertex_entry: base_module.vertex_body.as_ref().map(|body| EntryPointDef {
                name: "vs_main".to_string(),
                body: body.clone(),
                local_vars: Vec::new(),
            }),
            fragment_entry: base_module.fragment_body.as_ref().map(|body| EntryPointDef {
                name: "fs_main".to_string(),
                body: body.clone(),
                local_vars: Vec::new(),
            }),
            compute_entry: base_module.compute_body.as_ref().map(|body| ComputeEntryPointDef {
                name: "cs_main".to_string(),
                workgroup_size: [1, 1, 1],
                body: body.clone(),
                local_vars: Vec::new(),
            }),
            constants,
            global_vars: base_module.global_vars.clone(),
        };

        let generator = WgslGenerator::new();
        generator.generate(&composed)
    }
}

impl Default for VariantManager {
    fn default() -> Self {
        Self::new()
    }
}

// ─── Feature Set Builder ───────────────────────────────────────────────────────

/// Builder for constructing FeatureDef lists.
pub struct FeatureSetBuilder {
    features: Vec<FeatureDef>,
}

impl FeatureSetBuilder {
    pub fn new() -> Self {
        Self {
            features: Vec::new(),
        }
    }

    /// Add a boolean feature (true/false), default false.
    pub fn bool_feature(mut self, name: impl Into<String>) -> Self {
        self.features.push(FeatureDef {
            name: name.into(),
            possible_values: vec![VariantValue::Bool(false), VariantValue::Bool(true)],
            default_value: VariantValue::Bool(false),
            dependencies: vec![],
            exclusive_with: vec![],
        });
        self
    }

    /// Add a boolean feature with custom default.
    pub fn bool_feature_default(mut self, name: impl Into<String>, default: bool) -> Self {
        self.features.push(FeatureDef {
            name: name.into(),
            possible_values: vec![VariantValue::Bool(false), VariantValue::Bool(true)],
            default_value: VariantValue::Bool(default),
            dependencies: vec![],
            exclusive_with: vec![],
        });
        self
    }

    /// Add an integer feature with a range [min, max].
    pub fn int_feature(mut self, name: impl Into<String>, min: i32, max: i32) -> Self {
        let values: Vec<VariantValue> = (min..=max).map(VariantValue::Int).collect();
        self.features.push(FeatureDef {
            name: name.into(),
            possible_values: values,
            default_value: VariantValue::Int(min),
            dependencies: vec![],
            exclusive_with: vec![],
        });
        self
    }

    /// Add an enum feature with named values.
    pub fn enum_feature(mut self, name: impl Into<String>, values: Vec<impl Into<String>>) -> Self {
        let vals: Vec<String> = values.into_iter().map(|v| v.into()).collect();
        let possible: Vec<VariantValue> = vals.iter().map(|s| VariantValue::Enum(s.clone())).collect();
        let default_val = VariantValue::Enum(vals[0].clone());
        self.features.push(FeatureDef {
            name: name.into(),
            possible_values: possible,
            default_value: default_val,
            dependencies: vec![],
            exclusive_with: vec![],
        });
        self
    }

    /// Add a dependency to the last added feature.
    pub fn depends_on(mut self, feature: impl Into<String>, required: VariantValue) -> Self {
        if let Some(last) = self.features.last_mut() {
            last.dependencies.push(FeatureDependency {
                feature: feature.into(),
                required_value: required,
            });
        }
        self
    }

    /// Add mutual exclusion to the last added feature.
    pub fn exclusive_with(mut self, feature: impl Into<String>) -> Self {
        if let Some(last) = self.features.last_mut() {
            last.exclusive_with.push(feature.into());
        }
        self
    }

    pub fn build(self) -> Vec<FeatureDef> {
        self.features
    }
}

impl Default for FeatureSetBuilder {
    fn default() -> Self {
        Self::new()
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stream::presets;

    #[test]
    fn test_variant_key_construction() {
        let key = VariantKey::new()
            .set("SKINNING", VariantValue::Bool(true))
            .set("MAX_LIGHTS", VariantValue::Int(32));
        assert!(key.is_enabled("SKINNING"));
        assert!(!key.is_enabled("NORMAL_MAP")); // not set
    }

    #[test]
    fn test_variant_key_display() {
        let key = VariantKey::new()
            .set("A", VariantValue::Bool(true))
            .set("B", VariantValue::Int(4));
        assert_eq!(key.to_string(), "A=true+B=4");
    }

    #[test]
    fn test_variant_key_deterministic_hash() {
        // Same features in different insertion order should produce same hash.
        let k1 = VariantKey::new()
            .set("A", VariantValue::Bool(true))
            .set("B", VariantValue::Int(1));
        let k2 = VariantKey::new()
            .set("B", VariantValue::Int(1))
            .set("A", VariantValue::Bool(true));
        assert_eq!(k1, k2);

        // Also verify hash equality via HashMap.
        let mut map = HashMap::new();
        map.insert(k1.clone(), 42);
        assert_eq!(map.get(&k2), Some(&42));
    }

    #[test]
    fn test_variant_key_empty() {
        let key = VariantKey::new();
        assert!(key.is_empty());
        assert_eq!(key.to_string(), "");
    }

    #[test]
    fn test_feature_set_builder() {
        let features = FeatureSetBuilder::new()
            .bool_feature("SKINNING")
            .bool_feature_default("NORMAL_MAP", true)
            .int_feature("MAX_LIGHTS", 1, 32)
            .enum_feature("LIGHTING_MODEL", vec!["Lambert", "CookTorrance", "BlinnPhong"])
            .build();
        assert_eq!(features.len(), 4);
        assert_eq!(features[0].name, "SKINNING");
        assert_eq!(features[1].name, "NORMAL_MAP");
        assert_eq!(features[1].default_value, VariantValue::Bool(true));
        assert_eq!(features[2].possible_values.len(), 32);
        assert_eq!(features[3].possible_values.len(), 3);
    }

    #[test]
    fn test_register_features() {
        let mut mgr = VariantManager::new();
        let features = FeatureSetBuilder::new()
            .bool_feature("SKINNING")
            .bool_feature("NORMAL_MAP")
            .build();
        mgr.register("PBRShader", features).unwrap();
        assert!(mgr.get_features("PBRShader").is_some());
    }

    #[test]
    fn test_register_duplicate_feature() {
        let mut mgr = VariantManager::new();
        let features = vec![
            FeatureDef {
                name: "A".into(),
                possible_values: vec![VariantValue::Bool(false), VariantValue::Bool(true)],
                default_value: VariantValue::Bool(false),
                dependencies: vec![],
                exclusive_with: vec![],
            },
            FeatureDef {
                name: "A".into(),
                possible_values: vec![VariantValue::Bool(false), VariantValue::Bool(true)],
                default_value: VariantValue::Bool(false),
                dependencies: vec![],
                exclusive_with: vec![],
            },
        ];
        let result = mgr.register("test", features);
        assert!(result.is_err());
    }

    #[test]
    fn test_register_invalid_default() {
        let mut mgr = VariantManager::new();
        let features = vec![FeatureDef {
            name: "X".into(),
            possible_values: vec![VariantValue::Bool(false)],
            default_value: VariantValue::Bool(true), // not in possible_values
            dependencies: vec![],
            exclusive_with: vec![],
        }];
        assert!(mgr.register("test", features).is_err());
    }

    #[test]
    fn test_register_invalid_dependency_ref() {
        let mut mgr = VariantManager::new();
        let features = vec![FeatureDef {
            name: "X".into(),
            possible_values: vec![VariantValue::Bool(false), VariantValue::Bool(true)],
            default_value: VariantValue::Bool(false),
            dependencies: vec![FeatureDependency {
                feature: "NONEXISTENT".into(),
                required_value: VariantValue::Bool(true),
            }],
            exclusive_with: vec![],
        }];
        assert!(mgr.register("test", features).is_err());
    }

    #[test]
    fn test_register_invalid_exclusive_ref() {
        let mut mgr = VariantManager::new();
        let features = vec![FeatureDef {
            name: "X".into(),
            possible_values: vec![VariantValue::Bool(false), VariantValue::Bool(true)],
            default_value: VariantValue::Bool(false),
            dependencies: vec![],
            exclusive_with: vec!["NONEXISTENT".into()],
        }];
        assert!(mgr.register("test", features).is_err());
    }

    #[test]
    fn test_enumerate_variants() {
        let mut mgr = VariantManager::new();
        let features = FeatureSetBuilder::new()
            .bool_feature("A")
            .bool_feature("B")
            .build();
        mgr.register("test", features).unwrap();
        let variants = mgr.enumerate("test").unwrap();
        // 2 bool features × 2 values each = 4 variants.
        assert_eq!(variants.len(), 4);
    }

    #[test]
    fn test_enumerate_with_exclusion() {
        let mut mgr = VariantManager::new();
        let features = FeatureSetBuilder::new()
            .bool_feature("SKINNING")
            .bool_feature("INSTANCING")
            .exclusive_with("SKINNING") // can't have both true
            .build();
        mgr.register("test", features).unwrap();
        let variants = mgr.enumerate("test").unwrap();
        // SKINNING=true,INSTANCING=true should be excluded.
        // Valid: (F,F), (F,T), (T,F) = 3.
        assert_eq!(variants.len(), 3);

        // Verify the excluded combination is not present.
        for v in &variants {
            assert!(
                !(v.is_enabled("SKINNING") && v.is_enabled("INSTANCING")),
                "Found excluded combination: {}",
                v
            );
        }
    }

    #[test]
    fn test_enumerate_with_dependency() {
        let mut mgr = VariantManager::new();
        let features = FeatureSetBuilder::new()
            .bool_feature("HAS_UV")
            .bool_feature("NORMAL_MAP")
            .depends_on("HAS_UV", VariantValue::Bool(true)) // NORMAL_MAP=true requires HAS_UV=true
            .build();
        mgr.register("test", features).unwrap();
        let variants = mgr.enumerate("test").unwrap();
        // HAS_UV=false, NORMAL_MAP=true is invalid (dependency not met).
        // Valid: (F,F), (T,F), (T,T) = 3.
        assert_eq!(variants.len(), 3);

        // Verify the invalid combination is not present.
        for v in &variants {
            if v.is_enabled("NORMAL_MAP") {
                assert!(
                    v.is_enabled("HAS_UV"),
                    "NORMAL_MAP=true without HAS_UV=true: {}",
                    v
                );
            }
        }
    }

    #[test]
    fn test_validate_key_valid() {
        let mut mgr = VariantManager::new();
        let features = FeatureSetBuilder::new()
            .bool_feature("SKINNING")
            .bool_feature("NORMAL_MAP")
            .build();
        mgr.register("test", features).unwrap();

        let key = VariantKey::new()
            .set("SKINNING", VariantValue::Bool(true))
            .set("NORMAL_MAP", VariantValue::Bool(false));
        assert!(mgr.validate_key("test", &key).is_ok());
    }

    #[test]
    fn test_validate_key_invalid_feature() {
        let mut mgr = VariantManager::new();
        let features = FeatureSetBuilder::new()
            .bool_feature("SKINNING")
            .build();
        mgr.register("test", features).unwrap();

        let key = VariantKey::new().set("NONEXISTENT", VariantValue::Bool(true));
        assert!(mgr.validate_key("test", &key).is_err());
    }

    #[test]
    fn test_validate_key_invalid_value() {
        let mut mgr = VariantManager::new();
        let features = FeatureSetBuilder::new()
            .bool_feature("SKINNING")
            .build();
        mgr.register("test", features).unwrap();

        let key = VariantKey::new().set("SKINNING", VariantValue::Int(99));
        assert!(mgr.validate_key("test", &key).is_err());
    }

    #[test]
    fn test_validate_key_dependency_violated() {
        let mut mgr = VariantManager::new();
        let features = FeatureSetBuilder::new()
            .bool_feature("HAS_UV")
            .bool_feature("NORMAL_MAP")
            .depends_on("HAS_UV", VariantValue::Bool(true))
            .build();
        mgr.register("test", features).unwrap();

        let key = VariantKey::new()
            .set("HAS_UV", VariantValue::Bool(false))
            .set("NORMAL_MAP", VariantValue::Bool(true));
        let result = mgr.validate_key("test", &key);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_key_exclusion_violated() {
        let mut mgr = VariantManager::new();
        let features = FeatureSetBuilder::new()
            .bool_feature("SKINNING")
            .bool_feature("INSTANCING")
            .exclusive_with("SKINNING")
            .build();
        mgr.register("test", features).unwrap();

        let key = VariantKey::new()
            .set("SKINNING", VariantValue::Bool(true))
            .set("INSTANCING", VariantValue::Bool(true));
        let result = mgr.validate_key("test", &key);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_key_unregistered_shader() {
        let mgr = VariantManager::new();
        let key = VariantKey::new();
        assert!(mgr.validate_key("nonexistent", &key).is_err());
    }

    fn make_simple_module() -> ShaderModule {
        ShaderModuleBuilder::new("simple")
            .stream(presets::position())
            .vertex_body("output.clip_position = vec4<f32>(input.position, 1.0);")
            .fragment_body("return vec4<f32>(1.0, 0.0, 0.0, 1.0);")
            .build()
    }

    #[test]
    fn test_get_or_create_variant() {
        let mut mgr = VariantManager::new();
        let features = FeatureSetBuilder::new()
            .bool_feature("SKINNING")
            .bool_feature("NORMAL_MAP")
            .build();
        mgr.register("simple", features).unwrap();

        let mut composer = ShaderComposer::new();
        composer.register_module(make_simple_module()).unwrap();

        let key = VariantKey::new()
            .set("SKINNING", VariantValue::Bool(true))
            .set("NORMAL_MAP", VariantValue::Bool(false));

        // First call: cache miss.
        let variant = mgr.get_or_create("simple", &key, &composer).unwrap();
        assert!(variant.wgsl_source.contains("override"));
        assert!(variant.wgsl_source.contains("SKINNING"));

        // Second call: cache hit.
        let _variant2 = mgr.get_or_create("simple", &key, &composer).unwrap();
        let stats = mgr.cache_stats();
        assert_eq!(stats.misses, 1);
        assert_eq!(stats.hits, 1);
    }

    #[test]
    fn test_get_or_create_with_composition() {
        let mut mgr = VariantManager::new();
        let features = FeatureSetBuilder::new()
            .bool_feature("SKINNING")
            .build();
        mgr.register("composed_base", features).unwrap();

        let mut composer = ShaderComposer::new();
        let base = make_simple_module();
        composer.register_module(base).unwrap();

        // Compose the shader (trivial: no operations).
        let composed = composer.compose("simple", &[], "composed_base").unwrap();

        let key = VariantKey::new().set("SKINNING", VariantValue::Bool(true));
        let variant = mgr
            .get_or_create_with_composition("composed_base", &composed, &key)
            .unwrap();

        assert!(variant.wgsl_source.contains("override"));
        assert!(variant.wgsl_source.contains("SKINNING"));

        // Second call should be a cache hit.
        let _v2 = mgr
            .get_or_create_with_composition("composed_base", &composed, &key)
            .unwrap();
        let stats = mgr.cache_stats();
        assert_eq!(stats.hits, 1);
        assert_eq!(stats.misses, 1);
    }

    #[test]
    fn test_cache_stats() {
        let mut mgr = VariantManager::new();
        let features = FeatureSetBuilder::new().bool_feature("A").build();
        mgr.register("test", features).unwrap();

        let mut composer = ShaderComposer::new();
        composer
            .register_module(
                ShaderModuleBuilder::new("test")
                    .stream(presets::position())
                    .vertex_body("output.clip_position = vec4<f32>(input.position, 1.0);")
                    .fragment_body("return vec4<f32>(1.0);")
                    .build(),
            )
            .unwrap();

        let k1 = VariantKey::new().set("A", VariantValue::Bool(false));
        let k2 = VariantKey::new().set("A", VariantValue::Bool(true));

        mgr.get_or_create("test", &k1, &composer).unwrap();
        mgr.get_or_create("test", &k2, &composer).unwrap();
        mgr.get_or_create("test", &k1, &composer).unwrap(); // hit

        let stats = mgr.cache_stats();
        assert_eq!(stats.misses, 2);
        assert_eq!(stats.hits, 1);
        assert_eq!(stats.total_cached, 2);
    }

    #[test]
    fn test_variant_wgsl_has_overrides() {
        let mut mgr = VariantManager::new();
        let features = FeatureSetBuilder::new()
            .bool_feature("SKINNING")
            .int_feature("MAX_LIGHTS", 1, 4)
            .build();
        mgr.register("simple", features).unwrap();

        let mut composer = ShaderComposer::new();
        composer.register_module(make_simple_module()).unwrap();

        let key = VariantKey::new()
            .set("MAX_LIGHTS", VariantValue::Int(3))
            .set("SKINNING", VariantValue::Bool(true));

        let variant = mgr.get_or_create("simple", &key, &composer).unwrap();
        let wgsl = &variant.wgsl_source;

        assert!(wgsl.contains("@id("), "Missing override declarations in:\n{}", wgsl);
        assert!(wgsl.contains("SKINNING"), "Missing SKINNING constant in:\n{}", wgsl);
        assert!(wgsl.contains("MAX_LIGHTS"), "Missing MAX_LIGHTS constant in:\n{}", wgsl);
    }

    #[test]
    fn test_clear_cache() {
        let mut mgr = VariantManager::new();
        let features = FeatureSetBuilder::new().bool_feature("A").build();
        mgr.register("test", features).unwrap();

        let mut composer = ShaderComposer::new();
        composer
            .register_module(
                ShaderModuleBuilder::new("test")
                    .stream(presets::position())
                    .vertex_body("output.clip_position = vec4<f32>(input.position, 1.0);")
                    .fragment_body("return vec4<f32>(1.0);")
                    .build(),
            )
            .unwrap();

        let k = VariantKey::new().set("A", VariantValue::Bool(true));
        mgr.get_or_create("test", &k, &composer).unwrap();

        assert_eq!(mgr.cache_stats().total_cached, 1);

        mgr.clear_cache();
        let stats = mgr.cache_stats();
        assert_eq!(stats.hits, 0);
        assert_eq!(stats.misses, 0);
        assert_eq!(stats.total_cached, 0);
    }

    #[test]
    fn test_registered_shaders() {
        let mut mgr = VariantManager::new();
        mgr.register("A", FeatureSetBuilder::new().bool_feature("X").build()).unwrap();
        mgr.register("B", FeatureSetBuilder::new().bool_feature("Y").build()).unwrap();

        let shaders = mgr.registered_shaders();
        assert_eq!(shaders.len(), 2);
        assert!(shaders.contains(&"A"));
        assert!(shaders.contains(&"B"));
    }

    #[test]
    fn test_variant_count() {
        let mut mgr = VariantManager::new();
        let features = FeatureSetBuilder::new()
            .bool_feature("A")
            .bool_feature("B")
            .bool_feature("C")
            .build();
        mgr.register("test", features).unwrap();
        assert_eq!(mgr.variant_count("test").unwrap(), 8);
    }

    #[test]
    fn test_enumerate_empty_features() {
        let mut mgr = VariantManager::new();
        mgr.register("test", vec![]).unwrap();
        let variants = mgr.enumerate("test").unwrap();
        assert_eq!(variants.len(), 1);
        assert!(variants[0].is_empty());
    }

    #[test]
    fn test_variant_value_display() {
        assert_eq!(VariantValue::Bool(true).to_string(), "true");
        assert_eq!(VariantValue::Int(42).to_string(), "42");
        assert_eq!(VariantValue::Enum("Lambert".into()).to_string(), "Lambert");
    }

    #[test]
    fn test_variant_value_to_wgsl_expr() {
        assert_eq!(VariantValue::Bool(true).to_wgsl_expr(), "true");
        assert_eq!(VariantValue::Int(32).to_wgsl_expr(), "32");
        assert_eq!(
            VariantValue::Enum("Cook".into()).to_wgsl_expr(),
            "\"Cook\""
        );
    }

    #[test]
    fn test_cache_stats_display() {
        let stats = CacheStats {
            hits: 8,
            misses: 2,
            total_cached: 5,
        };
        let s = stats.to_string();
        assert!(s.contains("8 hits"));
        assert!(s.contains("2 misses"));
        assert!(s.contains("80.0%"));
        assert!(s.contains("5 cached"));
    }

    #[test]
    fn test_get_or_create_unregistered() {
        let mut mgr = VariantManager::new();
        let composer = ShaderComposer::new();
        let key = VariantKey::new();
        assert!(mgr.get_or_create("nonexistent", &key, &composer).is_err());
    }

    #[test]
    fn test_builder_with_dependency_and_exclusive() {
        let features = FeatureSetBuilder::new()
            .bool_feature("A")
            .bool_feature("B")
            .depends_on("A", VariantValue::Bool(true))
            .exclusive_with("A")
            .build();

        assert_eq!(features[1].dependencies.len(), 1);
        assert_eq!(features[1].dependencies[0].feature, "A");
        assert_eq!(features[1].exclusive_with, vec!["A".to_string()]);
    }
}
