//! Cluster culling compute pass 渲染图节点。

use crate::cluster::TOTAL_CLUSTERS;
use crate::graph::{RenderGraphContext, RenderPassNode, ResourceId};

const CULLING_WORKGROUP_SIZE: u32 = 64;

/// Cluster culling compute 节点。
///
/// 执行 light-vs-cluster 相交测试，将可见光信息写入 cluster bitmask buffer，
/// 供后续 forward render pass 在片元着色器中查询。
pub struct ClusterCullingNode<'a> {
    pub culling_pipeline: &'a wgpu::ComputePipeline,
    pub culling_bind_group: &'a wgpu::BindGroup,
}

impl<'a> RenderPassNode<'a> for ClusterCullingNode<'a> {
    fn name(&self) -> &str {
        "cluster_culling"
    }

    fn inputs(&self) -> Vec<ResourceId> {
        vec![
            ResourceId::new("lights_buffer"),
            ResourceId::new("cluster_uniform"),
        ]
    }

    fn outputs(&self) -> Vec<ResourceId> {
        vec![ResourceId::new("cluster_bitmask")]
    }

    fn execute(&self, _ctx: &RenderGraphContext<'_>, encoder: &mut wgpu::CommandEncoder) {
        let mut cpass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("forward+ cluster culling"),
            timestamp_writes: None,
        });
        cpass.set_pipeline(self.culling_pipeline);
        cpass.set_bind_group(0, self.culling_bind_group, &[]);
        let groups = (TOTAL_CLUSTERS + CULLING_WORKGROUP_SIZE - 1) / CULLING_WORKGROUP_SIZE;
        cpass.dispatch_workgroups(groups, 1, 1);
    }
}
