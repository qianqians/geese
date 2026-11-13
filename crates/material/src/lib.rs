//! 材质抽象层：数据模型 + `.material.json` 资产 + 引擎默认材质。
//!
//! 本 crate **不依赖 GPU**（没有 wgpu），供模型导入、骨骼动画、场景与渲染层共用：
//!
//! ```text
//! material（本 crate）        ← 材质数据、句柄、材质库、.material.json、默认材质
//!    ▲
//!    ├── render              ← 只负责把材质上传成 GPU 资源（uniform / bind group）
//!    ├── scene               ← glTF/模型导入、场景材质库、骨骼动画
//!    └── game_runtime / py_engine
//! ```
//!
//! # 关键类型
//!
//! | 类型 | 用途 |
//! | --- | --- |
//! | [`Material`] | PBR 参数 + 5 个贴图槽位 |
//! | [`MaterialLibrary`] | 材质/纹理表，按句柄访问，可按名字查找 |
//! | [`Texture`] | CPU 侧纹理数据 + 采样设置 |
//! | [`MaterialAsset`] / [`asset`] | `.material.json` 解析、内置默认材质、增量补丁 |
//! | [`ShaderGraph`] | 自定义着色图（节点 + WGSL 生成） |
//!
//! # 分层约定
//!
//! - **数据侧**（本 crate）只描述"材质是什么"：参数、贴图引用、透明度、双面等。
//! - **渲染侧**（`render`）只描述"怎么上 GPU"：`MaterialUniform`、bind group、
//!   纹理上传/缓存、绘制排序。
//! - 纹理句柄是库内索引；跨库合并请用 [`MaterialLibrary::append_library`]，
//!   它会重映射纹理句柄。

pub mod asset;
pub mod model;
pub mod shader_graph;

pub use asset::{
    append_missing_builtins, apply_material_assets, builtin_materials, default_material,
    ensure_default_material_files, load_material_asset_file, load_material_file,
    load_project_material_assets, load_project_materials, AlphaSettings, AlphaModeName,
    BuiltinMaterial, MaterialAsset, MaterialAssetError, MaterialOverrideReport, BUILTIN_MATERIALS,
    DEFAULT_MATERIAL_NAME, MATERIALS_DIR, MATERIAL_EXTENSION, MATERIAL_FORMAT_VERSION,
};
pub use model::{
    AlphaMode, FilterMode, Material, MaterialHandle, MaterialLibrary, Sampler, Texture,
    TextureFormat, TextureHandle, WrapMode,
};
pub use shader_graph::{ShaderGraph, ShaderGraphNode, ShaderNode};
