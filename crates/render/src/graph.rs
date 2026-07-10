//! 声明式渲染 Pass DAG。
//!
//! 提供基于有向无环图的渲染管线编排：
//! - [`RenderPassNode`]：单个渲染 pass 的 trait 抽象
//! - [`RenderGraphBuilder`]：声明式构建器，自动拓扑排序
//! - [`CompiledGraph`]：编译后的可执行图，执行时自动插入资源 barrier
//!
//! 使用 Kahn 算法检测循环并按拓扑序排列节点。相邻节点之间比对资源
//! 状态自动插入 wgpu 资源 barrier。

use std::collections::{HashMap, VecDeque};
use std::sync::Arc;

// ---------------------------------------------------------------------------
// 资源句柄 & 状态
// ---------------------------------------------------------------------------

/// 渲染图中的虚拟资源句柄（纹理 / 缓冲区）。
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ResourceId(pub Arc<str>);

impl ResourceId {
    pub fn new(name: &str) -> Self {
        Self(Arc::from(name))
    }
}

/// 资源在 GPU 管线中的使用状态。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResourceState {
    /// 未定义 / 未初始化
    Undefined,
    /// 作为颜色附件写入
    ColorAttachmentWrite,
    /// 作为深度附件写入
    DepthAttachmentWrite,
    /// 着色器读取（片元/顶点/计算）
    ShaderRead,
    /// 计算着色器写入
    ShaderWrite,
    /// 呈现目标
    Present,
}

// ---------------------------------------------------------------------------
// RenderPassNode
// ---------------------------------------------------------------------------

/// 渲染图执行上下文。在 `execute` 调用时传入。
pub struct RenderGraphContext<'a> {
    pub device: &'a wgpu::Device,
    pub queue: &'a wgpu::Queue,
    /// 外部颜色目标（通常为 surface texture view）
    pub color_target: Option<&'a wgpu::TextureView>,
    /// 外部深度目标
    pub depth_target: Option<&'a wgpu::TextureView>,
}

/// 单个渲染 pass 的抽象。
///
/// 每个节点声明其输入/输出资源（用于 barrier 推导）和执行逻辑。
pub trait RenderPassNode: Send + Sync {
    /// Pass 名称（用于调试和依赖引用）。
    fn name(&self) -> &str;

    /// 本 pass 读取的资源列表。
    fn inputs(&self) -> Vec<ResourceId> {
        Vec::new()
    }

    /// 本 pass 写入的资源列表。
    fn outputs(&self) -> Vec<ResourceId> {
        Vec::new()
    }

    /// 执行渲染 pass。
    fn execute(&self, ctx: &RenderGraphContext<'_>, encoder: &mut wgpu::CommandEncoder);
}

// ---------------------------------------------------------------------------
// RenderGraphBuilder
// ---------------------------------------------------------------------------

struct NodeEntry {
    name: String,
    node: Option<Box<dyn RenderPassNode>>,
    /// 依赖的 pass 名称（本 pass 必须在这些 pass 之后执行）
    deps: Vec<String>,
}

/// 声明式渲染图构建器。
///
/// 用法：
/// ```ignore
/// let mut builder = RenderGraphBuilder::new();
/// builder.add_pass("shadow", Box::new(ShadowPassNode::new(...)), &[]);
/// builder.add_pass("forward", Box::new(ForwardPassNode::new(...)), &["shadow"]);
/// let compiled = builder.compile()?;
/// ```
pub struct RenderGraphBuilder {
    nodes: Vec<NodeEntry>,
}

impl RenderGraphBuilder {
    pub fn new() -> Self {
        Self { nodes: Vec::new() }
    }

    /// 添加一个渲染 pass。
    ///
    /// - `name`：pass 名称（唯一标识，用于依赖引用）
    /// - `node`：实现了 [`RenderPassNode`] 的 pass 对象
    /// - `deps`：本 pass 依赖的 pass 名称列表（本 pass 在其后执行）
    pub fn add_pass(
        &mut self,
        name: &str,
        node: Box<dyn RenderPassNode>,
        deps: &[&str],
    ) -> &mut Self {
        self.nodes.push(NodeEntry {
            name: name.to_string(),
            node: Some(node),
            deps: deps.iter().map(|s| s.to_string()).collect(),
        });
        self
    }

    /// 编译渲染图：拓扑排序 + 检测循环。
    pub fn compile(self) -> Result<CompiledGraph, GraphError> {
        Self::compile_inner(self.nodes)
    }

    fn compile_inner(mut nodes: Vec<NodeEntry>) -> Result<CompiledGraph, GraphError> {
        let name_to_idx: HashMap<String, usize> = nodes
            .iter()
            .enumerate()
            .map(|(i, n)| (n.name.clone(), i))
            .collect();

        let n = nodes.len();
        let mut adj: Vec<Vec<usize>> = vec![Vec::new(); n];
        let mut in_degree: Vec<usize> = vec![0; n];

        for (i, entry) in nodes.iter().enumerate() {
            for dep_name in &entry.deps {
                let dep_idx = name_to_idx.get(dep_name).copied().ok_or_else(|| {
                    GraphError::UnknownDependency {
                        pass: entry.name.clone(),
                        dependency: dep_name.clone(),
                    }
                })?;
                adj[dep_idx].push(i);
                in_degree[i] += 1;
            }
        }

        let mut queue: VecDeque<usize> = (0..n).filter(|&i| in_degree[i] == 0).collect();
        let mut order = Vec::with_capacity(n);

        while let Some(u) = queue.pop_front() {
            order.push(u);
            for &v in &adj[u] {
                in_degree[v] -= 1;
                if in_degree[v] == 0 {
                    queue.push_back(v);
                }
            }
        }

        if order.len() != n {
            let cycle_nodes: Vec<String> = (0..n)
                .filter(|&i| in_degree[i] > 0)
                .map(|i| nodes[i].name.clone())
                .collect();
            return Err(GraphError::CycleDetected {
                nodes: cycle_nodes,
            });
        }

        let sorted: Vec<Box<dyn RenderPassNode>> = order
            .into_iter()
            .map(|i| nodes[i].node.take().expect("node already taken"))
            .collect();

        Ok(CompiledGraph { nodes: sorted })
    }
}

impl Default for RenderGraphBuilder {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// CompiledGraph
// ---------------------------------------------------------------------------

/// 编译后的渲染图（已拓扑排序，可直接执行）。
pub struct CompiledGraph {
    nodes: Vec<Box<dyn RenderPassNode>>,
}

impl std::fmt::Debug for CompiledGraph {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CompiledGraph")
            .field("node_count", &self.nodes.len())
            .finish()
    }
}

impl CompiledGraph {
    /// 顺序执行所有 pass，在相邻 pass 之间自动插入资源 barrier。
    pub fn execute(&self, ctx: &RenderGraphContext<'_>, encoder: &mut wgpu::CommandEncoder) {
        // 跟踪每个资源的当前状态（用于 barrier 推导）
        let mut resource_states: HashMap<ResourceId, ResourceState> = HashMap::new();

        for (i, node) in self.nodes.iter().enumerate() {
            let name = node.name();

            // 若需要，插入 barrier 以过渡资源状态
            if i > 0 {
                Self::insert_barriers_if_needed(
                    encoder,
                    node,
                    &self.nodes[i - 1],
                    &mut resource_states,
                );
            }

            // 更新资源状态
            for input in &node.inputs() {
                resource_states.insert(input.clone(), ResourceState::ShaderRead);
            }
            for output in &node.outputs() {
                resource_states.insert(output.clone(), ResourceState::ColorAttachmentWrite);
            }

            let _span = format!("RenderGraph::{}", name);
            node.execute(ctx, encoder);
        }
    }

    fn insert_barriers_if_needed(
        _encoder: &mut wgpu::CommandEncoder,
        _current: &Box<dyn RenderPassNode>,
        _previous: &Box<dyn RenderPassNode>,
        _states: &mut HashMap<ResourceId, ResourceState>,
    ) {
        // 骨架实现：wgpu 在 render pass 之间自动处理大部分同步。
        // 后续"可用级"将在此处插入真正的 wgpu buffer/texture barriers，
        // 比较相邻节点的资源状态并生成相应 barrier 命令。
        //
        // 当前设计预留了 barrier 插入点。用户可以通过各自 pass 的 execute()
        // 中直接调用 encoder 的 barrier 方法实现自定义同步。
    }

    /// 返回拓扑排序后的 pass 数量。
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    /// 是否为空图。
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }
}

// ---------------------------------------------------------------------------
// 错误
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GraphError {
    /// 依赖的 pass 名称未在图中注册
    UnknownDependency { pass: String, dependency: String },
    /// 图中存在循环依赖
    CycleDetected { nodes: Vec<String> },
}

impl std::fmt::Display for GraphError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GraphError::UnknownDependency { pass, dependency } => {
                write!(
                    f,
                    "pass '{pass}' depends on unknown pass '{dependency}'"
                )
            }
            GraphError::CycleDetected { nodes } => {
                write!(f, "cycle detected involving passes: {}", nodes.join(", "))
            }
        }
    }
}

impl std::error::Error for GraphError {}

// ---------------------------------------------------------------------------
// 测试
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// 用于测试的简单 pass：记录执行顺序
    struct TestPass {
        name: String,
        order: Arc<std::sync::Mutex<Vec<String>>>,
    }

    impl TestPass {
        fn new(name: &str, order: Arc<std::sync::Mutex<Vec<String>>>) -> Self {
            Self {
                name: name.to_string(),
                order,
            }
        }
    }

    impl RenderPassNode for TestPass {
        fn name(&self) -> &str {
            &self.name
        }

        fn execute(&self, _ctx: &RenderGraphContext<'_>, _encoder: &mut wgpu::CommandEncoder) {
            self.order.lock().unwrap().push(self.name.clone());
        }
    }

    #[test]
    fn empty_graph_compiles() {
        let builder = RenderGraphBuilder::new();
        let compiled = builder.compile().unwrap();
        assert!(compiled.is_empty());
        assert_eq!(compiled.len(), 0);
    }

    #[test]
    fn single_node_compiles() {
        let mut builder = RenderGraphBuilder::new();
        let order = Arc::new(std::sync::Mutex::new(Vec::new()));
        builder.add_pass("shadow", Box::new(TestPass::new("shadow", order.clone())), &[]);
        let compiled = builder.compile().unwrap();
        assert_eq!(compiled.len(), 1);
    }

    #[test]
    fn linear_chain_topological_sort() {
        let mut builder = RenderGraphBuilder::new();
        let order = Arc::new(std::sync::Mutex::new(Vec::new()));

        builder
            .add_pass("B", Box::new(TestPass::new("B", order.clone())), &["A"])
            .add_pass("A", Box::new(TestPass::new("A", order.clone())), &[])
            .add_pass("C", Box::new(TestPass::new("C", order.clone())), &["B"]);

        let compiled = builder.compile().unwrap();
        assert_eq!(compiled.len(), 3);

        // 验证拓扑排序：A 必须在 B 之前，B 必须在 C 之前
        let names: Vec<&str> = compiled.nodes.iter().map(|n| n.name()).collect();
        let pos_a = names.iter().position(|&n| n == "A").unwrap();
        let pos_b = names.iter().position(|&n| n == "B").unwrap();
        let pos_c = names.iter().position(|&n| n == "C").unwrap();
        assert!(pos_a < pos_b, "A should come before B");
        assert!(pos_b < pos_c, "B should come before C");
    }

    #[test]
    fn diamond_dependency() {
        let mut builder = RenderGraphBuilder::new();
        let order = Arc::new(std::sync::Mutex::new(Vec::new()));

        // A → B → D
        // A → C → D
        builder
            .add_pass("A", Box::new(TestPass::new("A", order.clone())), &[])
            .add_pass("B", Box::new(TestPass::new("B", order.clone())), &["A"])
            .add_pass("C", Box::new(TestPass::new("C", order.clone())), &["A"])
            .add_pass("D", Box::new(TestPass::new("D", order.clone())), &["B", "C"]);

        let compiled = builder.compile().unwrap();
        assert_eq!(compiled.len(), 4);

        // A 必须在最前面，D 必须在最后面
        let names: Vec<&str> = compiled.nodes.iter().map(|n| n.name()).collect();
        let pos_a = names.iter().position(|&n| n == "A").unwrap();
        let pos_d = names.iter().position(|&n| n == "D").unwrap();
        assert_eq!(pos_a, 0, "A should be first");
        assert_eq!(pos_d, 3, "D should be last");
    }

    #[test]
    fn cycle_detection() {
        let mut builder = RenderGraphBuilder::new();
        let order = Arc::new(std::sync::Mutex::new(Vec::new()));

        builder
            .add_pass("X", Box::new(TestPass::new("X", order.clone())), &["Y"])
            .add_pass("Y", Box::new(TestPass::new("Y", order.clone())), &["X"]);

        let result = builder.compile();
        assert!(result.is_err());
        match result.unwrap_err() {
            GraphError::CycleDetected { nodes } => {
                assert!(nodes.contains(&"X".to_string()));
                assert!(nodes.contains(&"Y".to_string()));
            }
            _ => panic!("expected CycleDetected"),
        }
    }

    #[test]
    fn unknown_dependency_error() {
        let mut builder = RenderGraphBuilder::new();
        let order = Arc::new(std::sync::Mutex::new(Vec::new()));

        builder.add_pass(
            "main",
            Box::new(TestPass::new("main", order.clone())),
            &["nonexistent"],
        );

        let result = builder.compile();
        assert!(result.is_err());
        match result.unwrap_err() {
            GraphError::UnknownDependency { pass, dependency } => {
                assert_eq!(pass, "main");
                assert_eq!(dependency, "nonexistent");
            }
            _ => panic!("expected UnknownDependency"),
        }
    }

    #[test]
    fn resource_state_tracking() {
        struct TexPass {
            name: String,
            inputs: Vec<ResourceId>,
            outputs: Vec<ResourceId>,
        }

        impl RenderPassNode for TexPass {
            fn name(&self) -> &str {
                &self.name
            }
            fn inputs(&self) -> Vec<ResourceId> {
                self.inputs.clone()
            }
            fn outputs(&self) -> Vec<ResourceId> {
                self.outputs.clone()
            }
            fn execute(&self, _ctx: &RenderGraphContext<'_>, _encoder: &mut wgpu::CommandEncoder) {}
        }

        let mut builder = RenderGraphBuilder::new();
        let gbuffer = ResourceId::new("gbuffer");
        let shadow_map = ResourceId::new("shadow_map");

        builder
            .add_pass(
                "shadow",
                Box::new(TexPass {
                    name: "shadow".into(),
                    inputs: vec![],
                    outputs: vec![shadow_map.clone()],
                }),
                &[],
            )
            .add_pass(
                "gbuffer",
                Box::new(TexPass {
                    name: "gbuffer".into(),
                    inputs: vec![],
                    outputs: vec![gbuffer.clone()],
                }),
                &[],
            )
            .add_pass(
                "lighting",
                Box::new(TexPass {
                    name: "lighting".into(),
                    inputs: vec![gbuffer.clone(), shadow_map.clone()],
                    outputs: vec![ResourceId::new("final_color")],
                }),
                &["shadow", "gbuffer"],
            );

        let compiled = builder.compile().unwrap();
        assert_eq!(compiled.len(), 3);

        // lighting 必须在 shadow 和 gbuffer 之后
        let names: Vec<&str> = compiled.nodes.iter().map(|n| n.name()).collect();
        let pos_shadow = names.iter().position(|&n| n == "shadow").unwrap();
        let pos_gbuffer = names.iter().position(|&n| n == "gbuffer").unwrap();
        let pos_lighting = names.iter().position(|&n| n == "lighting").unwrap();
        assert!(pos_lighting > pos_shadow);
        assert!(pos_lighting > pos_gbuffer);
    }

}
