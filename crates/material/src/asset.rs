//! `.material.json` 材质补丁与引擎内置默认材质。
//!
//! # 参数继承与覆盖（优先级）
//!
//! 1. **模型 glTF**：网格引用模型的材质时，参数全部继承自 glTF 文件。
//! 2. **`.material.json`**：按 `name` 命中同名材质时**只覆盖声明了的变量**，其余继续继承；
//!    没有同名材质时作为一个新材质加入（基值取 [`Material::default()`]）。
//! 3. **引擎内置**：只在库中没有同名材质时补齐（兜底），不覆盖模型材质。
//!
//! 也就是说 `.material.json` 是**补充修改**，不是重新定义：想改哪个变量就只写哪个。
//!
//! ```json
//! {
//!   "version": 1,
//!   "name": "default",
//!   "roughness": 0.35
//! }
//! ```
//!
//! 上面这份文件只把 `default` 材质的 roughness 改成 0.35，其余参数（base_color、
//! metallic、alpha…）保持原样——可能来自 glTF，也可能来自引擎内置默认值。
//!
//! # 加载规则
//!
//! - `assets/materials/` **不存在**时，首次运行写入全部内置默认材质；之后完全由
//!   项目控制（删文件、加文件都不会被覆盖）。
//! - 文件按文件名排序，保证句柄分配稳定。
//! - 文件名必须等于 `name`（`default.material.json` ↔ `"name": "default"`），避免
//!   "改了文件却没生效"。
//! - 按名字匹配：glTF 里没有名字的材质（`Material.name == None`）无法被补丁命中，
//!   补丁会退化成"新增同名材质"。导入时请确保模型材质有名字。
//! - 未知变量、格式版本不符都会报错（`deny_unknown_fields`），避免拼写错误被静默忽略。
//! - v1 只支持标量参数；贴图引用留给后续纹理管线（见设计文档 §9.5/§9.7）。

use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::{AlphaMode, Material, MaterialLibrary};
use serde::{Deserialize, Serialize};

/// 材质文件扩展名（与 `asset::meta::AssetTypeKind::Material` 的识别规则一致）。
pub const MATERIAL_EXTENSION: &str = ".material.json";

/// 项目内的材质目录（相对项目根目录）。
pub const MATERIALS_DIR: &str = "assets/materials";

/// 引擎缺省材质名：网格没有材质时回退到它。
pub const DEFAULT_MATERIAL_NAME: &str = "default";

/// 当前支持的 `.material.json` 格式版本。
pub const MATERIAL_FORMAT_VERSION: u32 = 1;

/// `.material.json` 的根对象。
///
/// 除 `version` / `name` 外全部可选：缺省的变量沿用 [`Material::default()`]，
/// 因此一个文件可以只写想改的那几个变量。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MaterialAsset {
    /// 格式版本，当前必须为 [`MATERIAL_FORMAT_VERSION`]。
    pub version: u32,
    /// 材质名，必须与文件名（去掉 `.material.json`）一致。
    pub name: String,
    /// RGBA，线性空间。
    #[serde(default)]
    pub base_color: Option<[f32; 4]>,
    #[serde(default)]
    pub metallic: Option<f32>,
    #[serde(default)]
    pub roughness: Option<f32>,
    /// RGB 自发光。
    #[serde(default)]
    pub emissive: Option<[f32; 3]>,
    /// 法线贴图强度（glTF `normalTexture.scale`）。
    #[serde(default)]
    pub normal_scale: Option<f32>,
    /// 环境光遮蔽强度（glTF `occlusionTexture.strength`）。
    #[serde(default)]
    pub occlusion_strength: Option<f32>,
    #[serde(default)]
    pub alpha: Option<AlphaSettings>,
    #[serde(default)]
    pub double_sided: Option<bool>,
}

/// 透明度设置。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AlphaSettings {
    pub mode: AlphaModeName,
    /// 仅 `mask` 使用。
    #[serde(default)]
    pub cutoff: Option<f32>,
}

/// JSON 中的 alpha 模式写法（小写）。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AlphaModeName {
    Opaque,
    Mask,
    Blend,
}

impl From<AlphaModeName> for AlphaMode {
    fn from(mode: AlphaModeName) -> Self {
        match mode {
            AlphaModeName::Opaque => AlphaMode::Opaque,
            AlphaModeName::Mask => AlphaMode::Mask,
            AlphaModeName::Blend => AlphaMode::Blend,
        }
    }
}

impl MaterialAsset {
    /// 解析并校验格式版本。未知字段会被拒绝。
    pub fn from_json(json: &str) -> Result<Self, MaterialAssetError> {
        let asset: Self = serde_json::from_str(json).map_err(|source| MaterialAssetError::Parse {
            path: None,
            message: source.to_string(),
        })?;
        if asset.version != MATERIAL_FORMAT_VERSION {
            return Err(MaterialAssetError::UnsupportedVersion {
                path: None,
                version: asset.version,
            });
        }
        Ok(asset)
    }

    /// 转成渲染用的 [`Material`]，缺省变量取 [`Material::default()`]。
    pub fn to_material(&self) -> Material {
        let mut material = Material::default();
        self.apply_to(&mut material);
        material
    }

    /// 把本文件声明的变量写入 `material`；未声明的字段保持不变。
    pub fn apply_to(&self, material: &mut Material) {
        material.name = Some(self.name.clone());

        if let Some(color) = self.base_color {
            material.base_color_factor = [
                color[0],
                color[1],
                color[2],
                self.clamp_unit("base_color[3]", color[3]),
            ];
        }
        if let Some(metallic) = self.metallic {
            material.metallic_factor = self.clamp_unit("metallic", metallic);
        }
        if let Some(roughness) = self.roughness {
            material.roughness_factor = self.clamp_unit("roughness", roughness);
        }
        if let Some(emissive) = self.emissive {
            material.emissive_factor = emissive;
        }
        if let Some(scale) = self.normal_scale {
            material.normal_scale = scale.max(0.0);
        }
        if let Some(strength) = self.occlusion_strength {
            material.occlusion_strength = self.clamp_unit("occlusion_strength", strength);
        }
        if let Some(alpha) = &self.alpha {
            material.alpha_mode = alpha.mode.into();
            if let Some(cutoff) = alpha.cutoff {
                material.alpha_cutoff = self.clamp_unit("alpha.cutoff", cutoff);
            }
        }
        if let Some(double_sided) = self.double_sided {
            material.double_sided = double_sided;
        }
    }

    fn clamp_unit(&self, field: &str, value: f32) -> f32 {
        if !value.is_finite() {
            log::warn!(
                "[material] 材质 `{}` 的 {} 不是有限数（{value}），按 0 处理",
                self.name,
                field
            );
            return 0.0;
        }
        let clamped = value.clamp(0.0, 1.0);
        if clamped != value {
            log::warn!(
                "[material] 材质 `{}` 的 {} = {value} 超出 [0,1]，已截断为 {clamped}",
                self.name,
                field
            );
        }
        clamped
    }
}

/// 引擎内置默认材质。
pub struct BuiltinMaterial {
    /// 材质名（= 文件名去掉 `.material.json`）。
    pub name: &'static str,
    pub file_name: &'static str,
    /// 编译期嵌入的 JSON 原文，用于首次写入项目。
    pub json: &'static str,
}

/// 引擎自带的默认材质集合。
pub const BUILTIN_MATERIALS: &[BuiltinMaterial] = &[
    BuiltinMaterial {
        name: "default",
        file_name: "default.material.json",
        json: include_str!("../assets/default_materials/default.material.json"),
    },
    BuiltinMaterial {
        name: "default_white",
        file_name: "default_white.material.json",
        json: include_str!("../assets/default_materials/default_white.material.json"),
    },
    BuiltinMaterial {
        name: "default_black",
        file_name: "default_black.material.json",
        json: include_str!("../assets/default_materials/default_black.material.json"),
    },
    BuiltinMaterial {
        name: "default_glass",
        file_name: "default_glass.material.json",
        json: include_str!("../assets/default_materials/default_glass.material.json"),
    },
    BuiltinMaterial {
        name: "default_emissive",
        file_name: "default_emissive.material.json",
        json: include_str!("../assets/default_materials/default_emissive.material.json"),
    },
];

/// 把编译期内置默认材质解析成材质库（不触碰磁盘）。
pub fn builtin_materials() -> Result<MaterialLibrary, MaterialAssetError> {
    let mut library = MaterialLibrary::default();
    for builtin in BUILTIN_MATERIALS {
        let asset = MaterialAsset::from_json(builtin.json)?;
        validate_file_name(&asset, Path::new(builtin.file_name))?;
        library.insert(asset.to_material());
    }
    Ok(library)
}

/// 只把库里**尚不存在**的同名内置材质追加进来，返回追加数量。
///
/// 已有同名材质（通常来自 glTF 导入）时跳过，保证"模型材质优先、内置只兜底"。
pub fn append_missing_builtins(
    library: &mut MaterialLibrary,
) -> Result<usize, MaterialAssetError> {
    let mut added = 0;
    for builtin in BUILTIN_MATERIALS {
        if library.find_by_name(builtin.name).is_some() {
            continue;
        }
        let asset = MaterialAsset::from_json(builtin.json)?;
        library.insert(asset.to_material());
        added += 1;
    }
    Ok(added)
}

/// 首次运行时把内置默认材质写入 `<project>/assets/materials/`。
///
/// 只在目录**不存在**时写入，返回写入的文件路径；目录已存在则返回空（尊重用户的
/// 删除/修改，不覆盖）。
pub fn ensure_default_material_files(project_dir: &Path) -> Result<Vec<PathBuf>, MaterialAssetError> {
    let dir = project_dir.join(MATERIALS_DIR);
    if dir.exists() {
        return Ok(Vec::new());
    }

    fs::create_dir_all(&dir).map_err(|source| MaterialAssetError::Io {
        path: dir.clone(),
        source,
    })?;

    let mut written = Vec::with_capacity(BUILTIN_MATERIALS.len());
    for builtin in BUILTIN_MATERIALS {
        let path = dir.join(builtin.file_name);
        fs::write(&path, builtin.json).map_err(|source| MaterialAssetError::Io {
            path: path.clone(),
            source,
        })?;
        written.push(path);
    }

    log::info!(
        "[material] 已生成 {} 个引擎默认材质到 {}（改里面的变量即可调整默认外观）",
        written.len(),
        dir.display()
    );
    Ok(written)
}

/// 读取项目里所有 `.material.json`，作为**增量补丁**返回（不转成材质）。
///
/// 文件名排序保证结果稳定；损坏/名字不符的文件记录错误后跳过。
pub fn load_project_material_assets(
    project_dir: &Path,
) -> Result<Vec<MaterialAsset>, MaterialAssetError> {
    let dir = project_dir.join(MATERIALS_DIR);
    ensure_default_material_files(project_dir)?;

    let mut files = collect_material_files(&dir);
    files.sort();
    if files.is_empty() {
        log::warn!(
            "[material] {} 下没有 .material.json 覆盖，全部使用模型/内置材质参数",
            dir.display()
        );
        return Ok(Vec::new());
    }

    let mut assets = Vec::with_capacity(files.len());
    for path in files {
        match load_material_asset_file(&path) {
            Ok(asset) => assets.push(asset),
            Err(err) => log::error!("[material] 跳过 {}: {err}", path.display()),
        }
    }
    Ok(assets)
}

/// 把补丁按名字应用到材质库：命中已有材质（glTF 或内置）则**只覆盖声明的变量**，
/// 未声明的参数继续沿用原值（继承）；没有同名材质时作为新材质追加。
pub fn apply_material_assets(
    library: &mut MaterialLibrary,
    assets: &[MaterialAsset],
) -> MaterialOverrideReport {
    let mut report = MaterialOverrideReport::default();

    for asset in assets {
        match library.find_by_name(&asset.name) {
            Some(handle) => {
                if let Some(material) = library.material_mut(handle) {
                    asset.apply_to(material);
                    report.patched += 1;
                }
            }
            None => {
                log::info!(
                    "[material] `{}` 没有同名材质，作为新材质追加（需要被显式按名引用才会用到）",
                    asset.name
                );
                library.insert(asset.to_material());
                report.added += 1;
            }
        }
    }

    report
}

/// 加载完整的项目材质库（内置兜底 + `.material.json` 覆盖），不涉及 glTF。
///
/// 运行时应当走"场景库 + [`append_missing_builtins`] + [`apply_material_assets`]"
/// 以保留 glTF 参数；本函数适合工具/测试等没有场景的场合。
pub fn load_project_materials(project_dir: &Path) -> Result<MaterialLibrary, MaterialAssetError> {
    let mut library = MaterialLibrary::default();
    append_missing_builtins(&mut library)?;
    let assets = load_project_material_assets(project_dir)?;
    apply_material_assets(&mut library, &assets);
    Ok(library)
}

/// 读取单个 `.material.json` 为补丁对象。
pub fn load_material_asset_file(path: &Path) -> Result<MaterialAsset, MaterialAssetError> {
    let json = fs::read_to_string(path).map_err(|source| MaterialAssetError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let asset = MaterialAsset::from_json(&json).map_err(|err| err.with_path(path))?;
    validate_file_name(&asset, path)?;
    Ok(asset)
}

/// 读取单个 `.material.json` 并物化为材质（以 [`Material::default()`] 为基）。
pub fn load_material_file(path: &Path) -> Result<Material, MaterialAssetError> {
    Ok(load_material_asset_file(path)?.to_material())
}

/// 补丁应用结果。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MaterialOverrideReport {
    /// 命中已有材质、只覆盖声明变量的数量（glTF/内置材质被补充修改）。
    pub patched: usize,
    /// 库中没有同名材质、作为新材质追加的数量。
    pub added: usize,
}

/// 取名为 `default` 的材质；不存在时回退到 [`Material::default()`]。
pub fn default_material(library: &MaterialLibrary) -> Material {
    library
        .find_by_name(DEFAULT_MATERIAL_NAME)
        .and_then(|handle| library.material(handle))
        .cloned()
        .unwrap_or_default()
}

fn collect_material_files(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_file())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.ends_with(MATERIAL_EXTENSION))
        })
        .collect()
}

fn validate_file_name(asset: &MaterialAsset, path: &Path) -> Result<(), MaterialAssetError> {
    let stem = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or_default();
    let expected = stem.strip_suffix(".material").unwrap_or(stem);
    if asset.name == expected {
        return Ok(());
    }
    Err(MaterialAssetError::NameMismatch {
        path: path.to_path_buf(),
        declared: asset.name.clone(),
        expected: expected.to_string(),
    })
}

/// 材质资产加载错误。
#[derive(Debug)]
pub enum MaterialAssetError {
    Io {
        path: PathBuf,
        source: io::Error,
    },
    Parse {
        path: Option<PathBuf>,
        message: String,
    },
    UnsupportedVersion {
        path: Option<PathBuf>,
        version: u32,
    },
    NameMismatch {
        path: PathBuf,
        declared: String,
        expected: String,
    },
}

impl MaterialAssetError {
    fn with_path(self, path: &Path) -> Self {
        match self {
            MaterialAssetError::Parse { message, .. } => MaterialAssetError::Parse {
                path: Some(path.to_path_buf()),
                message,
            },
            MaterialAssetError::UnsupportedVersion { version, .. } => {
                MaterialAssetError::UnsupportedVersion {
                    path: Some(path.to_path_buf()),
                    version,
                }
            }
            other => other,
        }
    }
}

impl fmt::Display for MaterialAssetError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MaterialAssetError::Io { path, source } => {
                write!(f, "读取材质文件 {} 失败: {source}", path.display())
            }
            MaterialAssetError::Parse { path, message } => match path {
                Some(path) => write!(f, "解析材质文件 {} 失败: {message}", path.display()),
                None => write!(f, "解析材质失败: {message}"),
            },
            MaterialAssetError::UnsupportedVersion { path, version } => {
                let prefix = path
                    .as_ref()
                    .map(|path| format!("{}: ", path.display()))
                    .unwrap_or_default();
                write!(
                    f,
                    "{prefix}不支持的材质格式版本 {version}（当前支持 {MATERIAL_FORMAT_VERSION}）"
                )
            }
            MaterialAssetError::NameMismatch {
                path,
                declared,
                expected,
            } => write!(
                f,
                "{}: name = `{declared}` 与文件名 `{expected}` 不一致，请改成一致",
                path.display()
            ),
        }
    }
}

impl std::error::Error for MaterialAssetError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            MaterialAssetError::Io { source, .. } => Some(source),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 测试用项目目录。放在 crate 的 `target/` 下：某些受限环境不允许写系统临时目录。
    fn temp_project(tag: &str) -> PathBuf {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join("test-artifacts")
            .join(format!("material_asset_{tag}"));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn builtin_materials_parse_and_are_named() {
        let library = builtin_materials().unwrap();
        assert_eq!(library.materials.len(), BUILTIN_MATERIALS.len());

        for builtin in BUILTIN_MATERIALS {
            let handle = library
                .find_by_name(builtin.name)
                .unwrap_or_else(|| panic!("缺少内置材质 {}", builtin.name));
            let material = library.material(handle).unwrap();
            assert_eq!(material.name.as_deref(), Some(builtin.name));
        }

        // 缺省材质必须是中性灰 PBR，而不是 Material::default() 的全反光白
        let default = default_material(&library);
        assert_eq!(default.base_color_factor, [0.5, 0.5, 0.5, 1.0]);
        assert_eq!(default.metallic_factor, 0.0);
        assert_eq!(default.roughness_factor, 0.5);
    }

    #[test]
    fn partial_json_keeps_defaults_for_omitted_fields() {
        let json = r#"{ "version": 1, "name": "cheap", "metallic": 0.25 }"#;
        let asset = MaterialAsset::from_json(json).unwrap();
        let material = asset.to_material();

        assert_eq!(material.metallic_factor, 0.25);
        // 未声明的变量沿用默认值
        assert_eq!(material.base_color_factor, [1.0, 1.0, 1.0, 1.0]);
        assert_eq!(material.roughness_factor, 1.0);
        assert_eq!(material.alpha_mode, AlphaMode::Opaque);
        assert_eq!(material.alpha_cutoff, 0.5);
        assert!(!material.double_sided);
    }

    #[test]
    fn alpha_and_double_sided_are_applied() {
        let json = r#"{
            "version": 1, "name": "glass",
            "alpha": { "mode": "blend", "cutoff": 0.3 },
            "double_sided": true
        }"#;
        let material = MaterialAsset::from_json(json).unwrap().to_material();
        assert_eq!(material.alpha_mode, AlphaMode::Blend);
        assert!((material.alpha_cutoff - 0.3).abs() < 1e-6);
        assert!(material.double_sided);
    }

    #[test]
    fn normal_scale_and_occlusion_strength_are_applied() {
        let json = r#"{ "version": 1, "name": "bumpy", "normal_scale": 2.5, "occlusion_strength": 0.4 }"#;
        let material = MaterialAsset::from_json(json).unwrap().to_material();
        assert!((material.normal_scale - 2.5).abs() < 1e-6);
        assert!((material.occlusion_strength - 0.4).abs() < 1e-6);
    }

    #[test]
    fn out_of_range_values_are_clamped() {
        let json = r#"{ "version": 1, "name": "clamp", "metallic": 2.0, "roughness": -1.0 }"#;
        let material = MaterialAsset::from_json(json).unwrap().to_material();
        assert_eq!(material.metallic_factor, 1.0);
        assert_eq!(material.roughness_factor, 0.0);
    }

    #[test]
    fn unknown_variable_is_rejected_with_clear_error() {
        let json = r#"{ "version": 1, "name": "typo", "metalic": 0.5 }"#;
        let err = MaterialAsset::from_json(json).unwrap_err();
        let message = err.to_string();
        assert!(
            message.contains("unknown field") && message.contains("metalic"),
            "错误信息应指出拼错的变量名，实际: {message}"
        );
    }

    #[test]
    fn unsupported_version_is_rejected() {
        let json = r#"{ "version": 99, "name": "future" }"#;
        let err = MaterialAsset::from_json(json).unwrap_err();
        assert!(err.to_string().contains("版本 99"), "实际: {err}");
    }

    #[test]
    fn name_must_match_file_name() {
        let project = temp_project("name_mismatch");
        let dir = project.join(MATERIALS_DIR);
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("red.material.json");
        fs::write(&path, r#"{ "version": 1, "name": "blue" }"#).unwrap();

        let err = load_material_file(&path).unwrap_err();
        assert!(err.to_string().contains("不一致"), "实际: {err}");
    }

    #[test]
    fn first_run_seeds_builtin_files() {
        let project = temp_project("seed");
        let written = ensure_default_material_files(&project).unwrap();
        assert_eq!(written.len(), BUILTIN_MATERIALS.len());

        for builtin in BUILTIN_MATERIALS {
            let path = project.join(MATERIALS_DIR).join(builtin.file_name);
            assert!(path.exists(), "缺少 {}", path.display());
        }

        // 已存在时不覆盖
        let written_again = ensure_default_material_files(&project).unwrap();
        assert!(written_again.is_empty());
    }

    #[test]
    fn editing_a_variable_changes_the_default_material() {
        let project = temp_project("edit");
        ensure_default_material_files(&project).unwrap();

        // 用户改默认材质的变量
        let path = project.join(MATERIALS_DIR).join("default.material.json");
        fs::write(
            &path,
            r#"{
                "version": 1,
                "name": "default",
                "base_color": [0.9, 0.1, 0.1, 1.0],
                "metallic": 0.0,
                "roughness": 0.25
            }"#,
        )
        .unwrap();

        let library = load_project_materials(&project).unwrap();
        let material = default_material(&library);
        assert_eq!(material.base_color_factor, [0.9, 0.1, 0.1, 1.0]);
        assert_eq!(material.roughness_factor, 0.25);
        // 其它内置材质仍可用
        assert!(library.find_by_name("default_glass").is_some());
    }

    #[test]
    fn empty_materials_dir_falls_back_to_builtins_without_writing() {
        let project = temp_project("empty");
        fs::create_dir_all(project.join(MATERIALS_DIR)).unwrap();

        let library = load_project_materials(&project).unwrap();
        assert_eq!(library.materials.len(), BUILTIN_MATERIALS.len());

        // 目录已存在，不应再写入文件
        let entries = fs::read_dir(project.join(MATERIALS_DIR)).unwrap().count();
        assert_eq!(entries, 0);
    }

    #[test]
    fn broken_file_is_skipped_but_others_load() {
        let project = temp_project("broken");
        ensure_default_material_files(&project).unwrap();
        fs::write(
            project.join(MATERIALS_DIR).join("broken.material.json"),
            "{ not json",
        )
        .unwrap();

        let library = load_project_materials(&project).unwrap();
        assert!(library.find_by_name("default").is_some());
        assert!(library.find_by_name("broken").is_none());
    }

    // ── 继承 + 增量覆盖 ──────────────────────────────────────────────

    #[test]
    fn patch_inherits_gltf_parameters() {
        // 模拟 glTF 导入的材质
        let mut library = MaterialLibrary::default();
        let mut gltf_material = Material::default();
        gltf_material.name = Some("hero_body".into());
        gltf_material.base_color_factor = [0.2, 0.3, 0.4, 1.0];
        gltf_material.metallic_factor = 0.9;
        gltf_material.roughness_factor = 0.2;
        library.insert(gltf_material);

        // .material.json 只改一个变量
        let asset = MaterialAsset::from_json(
            r#"{ "version": 1, "name": "hero_body", "roughness": 0.75 }"#,
        )
        .unwrap();
        let report = apply_material_assets(&mut library, &[asset]);

        assert_eq!(report.patched, 1);
        assert_eq!(report.added, 0);

        let handle = library.find_by_name("hero_body").unwrap();
        let material = library.material(handle).unwrap();
        assert_eq!(material.roughness_factor, 0.75); // 被覆盖
        assert_eq!(material.base_color_factor, [0.2, 0.3, 0.4, 1.0]); // 继承 glTF
        assert_eq!(material.metallic_factor, 0.9); // 继承 glTF
        assert_eq!(material.name.as_deref(), Some("hero_body"));
    }

    #[test]
    fn patch_without_matching_name_becomes_new_material() {
        let mut library = MaterialLibrary::default();
        let asset = MaterialAsset::from_json(
            r#"{ "version": 1, "name": "custom", "base_color": [0.1, 0.2, 0.3, 1.0] }"#,
        )
        .unwrap();
        let report = apply_material_assets(&mut library, &[asset]);

        assert_eq!(report.patched, 0);
        assert_eq!(report.added, 1);

        let handle = library.find_by_name("custom").unwrap();
        let material = library.material(handle).unwrap();
        assert_eq!(material.base_color_factor, [0.1, 0.2, 0.3, 1.0]);
        // 未声明的变量以 Material::default() 为基
        assert_eq!(material.roughness_factor, 1.0);
    }

    #[test]
    fn builtins_do_not_override_existing_material() {
        let mut library = MaterialLibrary::default();
        let mut custom_default = Material::default();
        custom_default.name = Some(DEFAULT_MATERIAL_NAME.into());
        custom_default.metallic_factor = 0.42;
        library.insert(custom_default);

        let added = append_missing_builtins(&mut library).unwrap();

        assert_eq!(added, BUILTIN_MATERIALS.len() - 1, "同名 `default` 应被跳过");
        let handle = library.find_by_name(DEFAULT_MATERIAL_NAME).unwrap();
        assert_eq!(library.material(handle).unwrap().metallic_factor, 0.42);
        // 其它内置材质补齐
        assert!(library.find_by_name("default_glass").is_some());
    }

    #[test]
    fn runtime_flow_prefers_gltf_then_patches() {
        // 手写目录（不走播种），保证只有两个补丁文件
        let project = temp_project("runtime_flow");
        let materials_dir = project.join(MATERIALS_DIR);
        fs::create_dir_all(&materials_dir).unwrap();
        // 项目覆盖：改模型材质 hero_body 的 roughness，改内置 default 的 metallic
        fs::write(
            materials_dir.join("hero_body.material.json"),
            r#"{ "version": 1, "name": "hero_body", "roughness": 0.75 }"#,
        )
        .unwrap();
        fs::write(
            materials_dir.join("default.material.json"),
            r#"{ "version": 1, "name": "default", "metallic": 0.33 }"#,
        )
        .unwrap();

        // 场景材质（来自 glTF）
        let mut scene_materials = MaterialLibrary::default();
        let mut gltf_material = Material::default();
        gltf_material.name = Some("hero_body".into());
        gltf_material.base_color_factor = [0.2, 0.3, 0.4, 1.0];
        gltf_material.metallic_factor = 0.9;
        scene_materials.insert(gltf_material);

        // 1) 内置兜底（不覆盖 hero_body）
        append_missing_builtins(&mut scene_materials).unwrap();
        // 2) 项目补丁（按名字增量覆盖）
        let assets = load_project_material_assets(&project).unwrap();
        let report = apply_material_assets(&mut scene_materials, &assets);

        assert_eq!(report.patched, 2, "hero_body 与内置 default 都被补丁命中");
        assert_eq!(report.added, 0);

        let hero = scene_materials
            .material(scene_materials.find_by_name("hero_body").unwrap())
            .unwrap();
        assert_eq!(hero.roughness_factor, 0.75);
        assert_eq!(hero.metallic_factor, 0.9, "未声明的参数继承 glTF");
        assert_eq!(hero.base_color_factor, [0.2, 0.3, 0.4, 1.0]);

        let default = default_material(&scene_materials);
        assert_eq!(default.metallic_factor, 0.33);
        assert_eq!(default.base_color_factor, [0.5, 0.5, 0.5, 1.0], "继承内置参数");
    }
}
