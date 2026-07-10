//! Shader Graph 数据模型 + WGSL 代码生成。
//!
//! ## 设计
//! - 节点式 shader graph：节点（SampleTexture/Multiply/Add/Lerp/Output 等）+ 边
//! - WGSL codegen：从 graph 生成 fragment shader，替换默认 PBR 着色

/// Shader 节点类型。
#[derive(Clone, Debug, PartialEq)]
pub enum ShaderNode {
    /// 纹理采样节点
    SampleTexture {
        texture_index: usize,
        uv_source: usize, // input pin index
    },
    /// 标量乘法
    Multiply,
    /// 加法
    Add,
    /// 线性插值
    Lerp,
    /// 最终输出节点
    Output,
    /// 常量颜色
    ConstantColor([f32; 4]),
    /// 常量标量
    ConstantScalar(f32),
}

/// Shader 图（节点 + 边）。
#[derive(Clone, Debug, Default)]
pub struct ShaderGraph {
    /// 节点列表
    pub nodes: Vec<ShaderGraphNode>,
    /// 边列表：`(source_node_index, source_pin, target_node_index, target_pin)`
    pub edges: Vec<(usize, u32, usize, u32)>,
}

/// 图中的节点实例。
#[derive(Clone, Debug)]
pub struct ShaderGraphNode {
    /// 节点唯一标识
    pub id: u64,
    /// 节点类型
    pub node: ShaderNode,
    /// 显示名称
    pub label: String,
    /// 在 editor canvas 中的位置
    pub position: [f32; 2],
}

impl ShaderGraph {
    /// 创建空的 shader graph。
    pub fn new() -> Self {
        Self::default()
    }

    /// 添加节点。
    pub fn add_node(&mut self, node: ShaderNode, label: impl Into<String>, pos: [f32; 2]) -> u64 {
        let id = self.next_id();
        self.nodes.push(ShaderGraphNode {
            id,
            node,
            label: label.into(),
            position: pos,
        });
        id
    }

    /// 添加边。
    pub fn add_edge(&mut self, from_node: u64, from_pin: u32, to_node: u64, to_pin: u32) {
        let from_idx = self.nodes.iter().position(|n| n.id == from_node);
        let to_idx = self.nodes.iter().position(|n| n.id == to_node);
        if let (Some(fi), Some(ti)) = (from_idx, to_idx) {
            self.edges.push((fi, from_pin, ti, to_pin));
        }
    }

    /// 从 graph 生成 WGSL fragment shader 代码。
    /// 简单实现：按拓扑序遍历节点，为每个节点生成局部变量。
    pub fn generate_wgsl(&self) -> String {
        let mut code = String::new();
        code.push_str("// Auto-generated shader from ShaderGraph\n");
        code.push_str("@fragment\n");
        code.push_str("fn fs_main() -> @location(0) vec4<f32> {\n");

        for (i, node) in self.nodes.iter().enumerate() {
            match &node.node {
                ShaderNode::ConstantColor(color) => {
                    code.push_str(&format!(
                        "    let c{} = vec4<f32>({:?}, {:?}, {:?}, {:?});\n",
                        i, color[0], color[1], color[2], color[3]
                    ));
                }
                ShaderNode::ConstantScalar(s) => {
                    code.push_str(&format!("    let s{} = {:?};\n", i, s));
                }
                ShaderNode::Multiply => {
                    code.push_str(&format!("    let m{} = c{} * s{};\n", i, i - 2, i - 1));
                }
                ShaderNode::Add => {
                    code.push_str(&format!("    let a{} = c{} + c{};\n", i, i - 2, i - 1));
                }
                ShaderNode::Lerp => {
                    code.push_str(&format!("    let l{} = mix(c{}, c{}, s{});\n", i, i - 3, i - 2, i - 1));
                }
                ShaderNode::Output => {
                    code.push_str(&format!("    return c{};\n", i - 1));
                }
                ShaderNode::SampleTexture { texture_index, .. } => {
                    code.push_str(&format!(
                        "    let t{}_color = textureSample(tex_{}, tex_sampler, uv);\n",
                        i, texture_index
                    ));
                }
            }
        }

        code.push_str("}\n");
        code
    }

    /// 计算 shader graph 的哈希值（用于编译缓存 key）。
    pub fn hash(&self) -> u64 {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let mut hasher = DefaultHasher::new();
        for node in &self.nodes {
            std::mem::discriminant(&node.node).hash(&mut hasher);
        }
        self.edges.len().hash(&mut hasher);
        hasher.finish()
    }

    fn next_id(&self) -> u64 {
        self.nodes
            .iter()
            .map(|n| n.id)
            .max()
            .unwrap_or(0)
            + 1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_graph_generates_wgsl() {
        let graph = ShaderGraph::new();
        let wgsl = graph.generate_wgsl();
        assert!(wgsl.contains("@fragment"));
        assert!(wgsl.contains("fn fs_main"));
    }

    #[test]
    fn constant_color_generates_code() {
        let mut graph = ShaderGraph::new();
        graph.add_node(ShaderNode::ConstantColor([1.0, 0.0, 0.0, 1.0]), "Red", [0.0, 0.0]);
        let wgsl = graph.generate_wgsl();
        assert!(wgsl.contains("1.0, 0.0, 0.0, 1.0"));
    }

    #[test]
    fn graph_hash_changes_with_nodes() {
        let mut g1 = ShaderGraph::new();
        g1.add_node(ShaderNode::ConstantColor([1.0; 4]), "c", [0.0; 2]);
        let h1 = g1.hash();

        let mut g2 = ShaderGraph::new();
        g2.add_node(ShaderNode::ConstantScalar(0.5), "s", [0.0; 2]);
        let h2 = g2.hash();

        assert_ne!(h1, h2);
    }
}
