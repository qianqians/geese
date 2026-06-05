use crate::{Light, MaterialLibrary, RenderQueue};

/// 渲染路径选择。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RenderingPath {
    /// 当前默认：Forward+（cluster culling + forward 着色）。
    ForwardPlus,
    /// 备选：Deferred+（G-Buffer + cluster culling + 全屏 lighting）。
    DeferredPlus,
}

impl Default for RenderingPath {
    fn default() -> Self {
        Self::ForwardPlus
    }
}

/// 渲染管线统一构造参数。
pub struct ScenePipelineDescriptor {
    pub rendering_path: RenderingPath,
    pub color_format: wgpu::TextureFormat,
    pub depth_format: wgpu::TextureFormat,
    pub sample_count: u32,
    pub width: u32,
    pub height: u32,
}

impl ScenePipelineDescriptor {
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
}

/// 不同渲染路径在 `prepare` 之后产出的中间帧状态。`render` 阶段消费这一状态。
///
/// 当前 Forward+ / Deferred+ 各自把状态保存在管线对象内部，因此本枚举仅作为
/// 调试/扩展锚点，不直接参与对外 API。
#[derive(Clone, Copy, Debug)]
pub enum PreparedFrameKind {
    ForwardPlus,
    DeferredPlus,
}

/// 渲染管线统一接口。所有路径都遵守 update -> resize -> prepare -> render 的生命周期。
///
/// `render` 接受外部 color/depth 视图，因此管线对象本身不持有 surface texture，
/// 适合接入 winit / xr / 离屏渲染等不同宿主。
pub trait ScenePipeline {
    fn path(&self) -> RenderingPath;

    /// 视口尺寸或近远平面变化时调用。负责重建 cluster uniform 与依赖屏幕尺寸的
    /// 临时纹理（G-Buffer 等）。
    fn resize(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        width: u32,
        height: u32,
        z_near: f32,
        z_far: f32,
    );

    fn update_camera(
        &mut self,
        queue: &wgpu::Queue,
        view_projection: [[f32; 4]; 4],
        camera_position: [f32; 3],
    );

    fn update_lights(&mut self, queue: &wgpu::Queue, ambient: [f32; 3], lights: &[Light]);

    /// 上传本帧绘制命令所需的 GPU 资源（顶点 / 索引 / 材质 / 对象 uniform）。
    fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        materials: &MaterialLibrary,
        render_queue: &RenderQueue<'_>,
    );

    /// 把本帧已 prepare 的内容编码到给定的 encoder，并最终输出到 `color_target`。
    /// `depth_target` 在 Forward+ 必须提供;Deferred+ 内部使用自有 G-Buffer 深度，
    /// 此参数可以忽略，但保持接口一致。
    fn render(
        &self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        color_target: &wgpu::TextureView,
        depth_target: Option<&wgpu::TextureView>,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_path_is_forward_plus() {
        assert_eq!(RenderingPath::default(), RenderingPath::ForwardPlus);
    }

    #[test]
    fn descriptor_constructors_set_path() {
        let f = ScenePipelineDescriptor::forward_plus(wgpu::TextureFormat::Rgba8Unorm, 1280, 720);
        assert_eq!(f.rendering_path, RenderingPath::ForwardPlus);
        let d = ScenePipelineDescriptor::deferred_plus(wgpu::TextureFormat::Rgba8Unorm, 1280, 720);
        assert_eq!(d.rendering_path, RenderingPath::DeferredPlus);
    }
}
