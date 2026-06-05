//! 渲染器顶层 facade。
//!
//! 内部根据 [`RenderingPath`] 选择 [`ForwardPlusPipeline`] 或
//! [`DeferredPlusPipeline`] 作为实际后端，对外暴露统一的
//! `update_camera / update_lights / prepare / render / resize` 生命周期。
//!
//! 调用方无需关心管线细节，只在 `WgpuSceneRendererDescriptor` 中声明
//! 所选 `RenderingPath` 即可在两条管线之间切换。

use crate::deferred_plus::DeferredPlusPipeline;
use crate::forward_plus::ForwardPlusPipeline;
use crate::pipeline::{RenderingPath, ScenePipeline, ScenePipelineDescriptor};
use crate::{Light, MaterialLibrary, RenderQueue};

/// 顶层渲染器构造参数。`rendering_path` 决定选用的具体管线实现。
pub struct WgpuSceneRendererDescriptor {
    pub rendering_path: RenderingPath,
    pub color_format: wgpu::TextureFormat,
    pub depth_format: wgpu::TextureFormat,
    pub sample_count: u32,
    pub width: u32,
    pub height: u32,
}

impl WgpuSceneRendererDescriptor {
    pub fn forward_plus(color_format: wgpu::TextureFormat, width: u32, height: u32) -> Self {
        Self {
            rendering_path: RenderingPath::ForwardPlus,
            color_format,
            depth_format: wgpu::TextureFormat::Depth32Float,
            sample_count: 1,
            width,
            height,
        }
    }

    pub fn deferred_plus(color_format: wgpu::TextureFormat, width: u32, height: u32) -> Self {
        Self {
            rendering_path: RenderingPath::DeferredPlus,
            color_format,
            depth_format: wgpu::TextureFormat::Depth32Float,
            sample_count: 1,
            width,
            height,
        }
    }

    fn into_pipeline_descriptor(&self) -> ScenePipelineDescriptor {
        ScenePipelineDescriptor {
            rendering_path: self.rendering_path,
            color_format: self.color_format,
            depth_format: self.depth_format,
            sample_count: self.sample_count,
            width: self.width,
            height: self.height,
        }
    }
}

/// 渲染器 facade。封装 `Box<dyn ScenePipeline>`，按 `RenderingPath` 选择 Forward+
/// 或 Deferred+ 实现，并保持调用方接口不变。
pub struct WgpuSceneRenderer {
    pipeline: Box<dyn ScenePipeline>,
}

impl WgpuSceneRenderer {
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        descriptor: WgpuSceneRendererDescriptor,
    ) -> Self {
        let pipeline_desc = descriptor.into_pipeline_descriptor();
        let pipeline: Box<dyn ScenePipeline> = match descriptor.rendering_path {
            RenderingPath::ForwardPlus => {
                Box::new(ForwardPlusPipeline::new(device, queue, &pipeline_desc))
            }
            RenderingPath::DeferredPlus => {
                Box::new(DeferredPlusPipeline::new(device, queue, &pipeline_desc))
            }
        };
        Self { pipeline }
    }

    /// 当前实际使用的渲染路径。
    pub fn rendering_path(&self) -> RenderingPath {
        self.pipeline.path()
    }

    pub fn resize(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        width: u32,
        height: u32,
        z_near: f32,
        z_far: f32,
    ) {
        self.pipeline
            .resize(device, queue, width, height, z_near, z_far);
    }

    pub fn update_camera(
        &mut self,
        queue: &wgpu::Queue,
        view_projection: [[f32; 4]; 4],
        camera_position: [f32; 3],
    ) {
        self.pipeline
            .update_camera(queue, view_projection, camera_position);
    }

    pub fn update_lights(&mut self, queue: &wgpu::Queue, ambient: [f32; 3], lights: &[Light]) {
        self.pipeline.update_lights(queue, ambient, lights);
    }

    pub fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        materials: &MaterialLibrary,
        render_queue: &RenderQueue<'_>,
    ) {
        self.pipeline.prepare(device, queue, materials, render_queue);
    }

    pub fn render(
        &self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        color_target: &wgpu::TextureView,
        depth_target: Option<&wgpu::TextureView>,
    ) {
        self.pipeline
            .render(device, encoder, color_target, depth_target);
    }
}

// `WgpuRenderQueue` 已迁移至 `crate::common`;外部通过 `crate::WgpuRenderQueue` 访问。
