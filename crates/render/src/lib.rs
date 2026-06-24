pub mod cluster;
pub mod common;
pub mod deferred_plus;
pub mod forward_plus;
pub mod ibl;
pub mod light;
pub mod lines;
pub mod material;
pub mod mesh;
pub mod pipeline;
pub mod post;
pub mod scene;
pub mod shadow;
pub mod skinning;
pub mod wgpu_renderer;

pub use cluster::{
    ClusterUniform, CLUSTER_DEPTH_SLICES, CLUSTER_TILES_X, CLUSTER_TILES_Y, TOTAL_CLUSTERS,
};
pub use common::{
    CameraUniform, DefaultTextures, GpuVertex, MaterialUniform, ObjectUniform, WgpuRenderCommand,
    WgpuRenderQueue, MAX_JOINTS, TEX_BIT_BASE_COLOR, TEX_BIT_EMISSIVE,
    TEX_BIT_METALLIC_ROUGHNESS, TEX_BIT_NORMAL, TEX_BIT_OCCLUSION,
};
pub use deferred_plus::DeferredPlusPipeline;
pub use forward_plus::ForwardPlusPipeline;
pub use ibl::{IblBaker, IblConfig, IblResources, IblUniform, SkyboxKind, StubIblBaker};
pub use light::{encode_light, GpuLight, Light, LightStorage, MAX_LIGHTS};
pub use material::{
    AlphaMode, FilterMode, Material, MaterialHandle, MaterialLibrary, Sampler, Texture,
    TextureFormat, TextureHandle, WrapMode,
};
pub use mesh::{MeshFlags, ModelMesh, SkinHandle, Vertex};
pub use pipeline::{
    PreparedFrameKind, RenderingPath, ScenePipeline, ScenePipelineDescriptor,
};
pub use post::{
    aces_tonemap, build_post_uniform, halton_2_3, EffectMask, PostChain, PostEffect, PostUniform,
};
pub use scene::{RenderCommand, RenderObject, RenderQueue, RenderStats, SceneRenderer};
pub use shadow::{
    compute_atlas_layout, compute_cascade_splits, AtlasRect, CascadeConfig, CascadeUniform,
    CsmUniform, DirectionalShadowCaster, NullShadowAtlas, ShadowAtlas, MAX_CASCADES,
};
pub use skinning::{
    compute_joint_matrices, GpuJointMatrix, JointPalette, MorphWeights, NullSkinningUploader,
    SkinningMode, SkinningUploader, MAX_MORPH_TARGETS,
};
pub use wgpu_renderer::{WgpuSceneRenderer, WgpuSceneRendererDescriptor};

pub use lines::{LineRenderer, LineVertex};

/// Re-export wgpu so downstream crates (editor, etc.) share the same version.
pub use wgpu;
