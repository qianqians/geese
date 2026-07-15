pub mod cluster;
pub mod common;
pub mod decal;
pub mod deferred_plus;
pub mod fog;
pub mod forward_plus;
pub mod graph;
pub mod grid;
pub mod hiz;
pub mod ibl;
pub mod light;
pub mod lines;
pub mod lod;
pub mod material;
pub mod mesh;
pub mod particle;
pub mod pipeline;
pub mod post;
pub mod post_pipeline;
pub mod profiler;
pub mod scene;
pub mod shader_graph;
pub mod shadow;
pub mod shadow_pass;
pub mod skinning;
pub mod reflection_probe;
pub mod water;
pub mod wgpu_ibl_baker;
pub mod wgpu_renderer;
pub mod sprite;

pub use cluster::{
    ClusterUniform, CLUSTER_DEPTH_SLICES, CLUSTER_TILES_X, CLUSTER_TILES_Y, TOTAL_CLUSTERS,
};
pub use common::{
    CameraUniform, DefaultTextures, GpuResourceCache, GpuVertex, MaterialUniform, ObjectUniform,
    WgpuRenderCommand, WgpuRenderQueue, MAX_JOINTS, TEX_BIT_BASE_COLOR, TEX_BIT_EMISSIVE,
    TEX_BIT_METALLIC_ROUGHNESS, TEX_BIT_NORMAL, TEX_BIT_OCCLUSION,
};
pub use deferred_plus::DeferredPlusPipeline;
pub use forward_plus::ForwardPlusPipeline;
pub use hiz::{HiZPyramid, ObjectAabb};
pub use ibl::{IblBaker, IblConfig, IblResources, IblUniform, SkyboxKind, StubIblBaker};
pub use wgpu_ibl_baker::{WgpuIblBaker, BakedIblTextures, IblBakeError};
pub use light::{encode_light, GpuLight, Light, LightStorage, MAX_LIGHTS};
pub use lod::{camera_distance, extract_translation, select_lod};
pub use material::{
    AlphaMode, FilterMode, Material, MaterialHandle, MaterialLibrary, Sampler, Texture,
    TextureFormat, TextureHandle, WrapMode,
};
pub use mesh::{LodLevel, MeshFlags, ModelMesh, SkinHandle, Vertex};
pub use particle::{GpuParticle, ParticleEmitter, ParticleSimUniform};
pub use pipeline::{
    PreparedFrameKind, RenderingPath, ScenePipeline, ScenePipelineDescriptor,
};
pub use post::{
    aces_tonemap, build_post_uniform, halton_2_3, EffectMask, PostChain, PostEffect, PostUniform,
};
pub use post_pipeline::PostProcessPipeline;
pub use scene::{RenderCommand, RenderObject, RenderQueue, RenderStats, SceneRenderer};
pub use shadow::{
    compute_atlas_layout, compute_cascade_splits, AtlasRect, CascadeConfig, CascadeUniform,
    CsmUniform, DirectionalShadowCaster, NullShadowAtlas, ShadowAtlas, MAX_CASCADES,
};
pub use shadow_pass::{ShadowPass, WgpuShadowAtlas, CascadeVp};
pub use skinning::{
    compute_joint_matrices, GpuJointMatrix, JointPalette, MorphWeights, NullSkinningUploader,
    SkinningMode, SkinningUploader, MAX_MORPH_TARGETS,
};
pub use wgpu_renderer::{WgpuSceneRenderer, WgpuSceneRendererDescriptor};

pub use lines::{LineRenderer, LineVertex};

pub use fog::{FogRenderer, FogSettings, FogUniform};

pub use water::{WaterRenderer, WaterSettings, WaterUniform, WaterVertex};

pub use decal::{
    Decal, DecalProjection, DecalSystem, DepthTestResult, DEFAULT_MAX_DECALS,
    decal_world_matrix, depth_test_decal, project_decal,
};

pub use reflection_probe::{
    CubemapHandle, ProbeInfluence, ReflectionProbe, ReflectionProbeSystem, ReflectionProbeUniform,
    MAX_REFLECTION_PROBES,
};

/// Re-export wgpu so downstream crates (editor, etc.) share the same version.
pub use wgpu;

pub use sprite::{Sprite, SpriteBatch, SpriteCameraUniform, SpriteRenderer, SpriteVertex};

#[cfg(feature = "use-shader-framework")]
pub mod shader_library;
