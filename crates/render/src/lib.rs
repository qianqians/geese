pub mod cluster;
pub mod common;
pub mod deferred_plus;
pub mod forward_plus;
pub mod light;
pub mod material;
pub mod mesh;
pub mod pipeline;
pub mod scene;
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
pub use light::{encode_light, GpuLight, Light, LightStorage, MAX_LIGHTS};
pub use material::{
    AlphaMode, FilterMode, Material, MaterialHandle, MaterialLibrary, Sampler, Texture,
    TextureFormat, TextureHandle, WrapMode,
};
pub use mesh::{MeshFlags, ModelMesh, SkinHandle, Vertex};
pub use pipeline::{
    PreparedFrameKind, RenderingPath, ScenePipeline, ScenePipelineDescriptor,
};
pub use scene::{RenderCommand, RenderObject, RenderQueue, RenderStats, SceneRenderer};
pub use wgpu_renderer::{WgpuSceneRenderer, WgpuSceneRendererDescriptor};
