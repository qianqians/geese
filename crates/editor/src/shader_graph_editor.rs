//! Shader Graph 编辑器 — 节点式 shader graph 编辑面板。
//!
//! 功能：节点拖拽、连线编辑、实时 WGSL 预览。
//!
//! Feature gate: 绑定到 `render::shader_graph::ShaderGraph`，
//! 通过 `Material::custom_shader` 关联。

use egui::{Color32, Pos2, Rect, ScrollArea, Sense, Stroke, Vec2};
use render::shader_graph::{ShaderGraph, ShaderNode};

/// Shader Graph 编辑器面板状态。
#[derive(Default)]
pub struct ShaderGraphEditorPanel {
    /// 当前编辑的 shader graph
    graph: ShaderGraph,
    /// 生成的 WGSL 代码预览
    wgsl_preview: String,
    /// 是否需要刷新预览
    dirty: bool,
    /// canvas 缩放
    zoom: f32,
    /// canvas 平移偏移
    pan: Vec2,
}

impl ShaderGraphEditorPanel {
    pub fn new() -> Self {
        Self {
            zoom: 1.0,
            pan: Vec2::ZERO,
            ..Default::default()
        }
    }

    /// 加载 shader graph 进行编辑。
    pub fn load_graph(&mut self, graph: &ShaderGraph) {
        self.graph = graph.clone();
        self.dirty = true;
    }

    /// 绘制 shader graph 编辑器 UI。
    pub fn show(&mut self, ctx: &egui::Context, _open: &mut bool) {
        egui::Window::new("Shader Graph Editor")
            .default_size([800.0, 600.0])
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    if ui.button("Add Color Node").clicked() {
                        self.graph.add_node(
                            ShaderNode::ConstantColor([1.0, 1.0, 1.0, 1.0]),
                            "Color",
                            [100.0, 100.0],
                        );
                        self.dirty = true;
                    }
                    if ui.button("Add Multiply Node").clicked() {
                        self.graph.add_node(ShaderNode::Multiply, "Multiply", [200.0, 200.0]);
                        self.dirty = true;
                    }
                    if ui.button("Add Output Node").clicked() {
                        self.graph.add_node(ShaderNode::Output, "Output", [300.0, 300.0]);
                        self.dirty = true;
                    }
                });

                ui.separator();

                // Canvas 区域：显示节点
                ScrollArea::both()
                    .max_height(300.0)
                    .show(ui, |ui| {
                        let canvas_size = ui.available_size();
                        let (response, painter) =
                            ui.allocate_painter(canvas_size, Sense::click_and_drag());

                        // 绘制节点
                        for node in &self.graph.nodes {
                            let pos = Pos2::new(
                                node.position[0] * self.zoom + self.pan.x,
                                node.position[1] * self.zoom + self.pan.y,
                            );
                            let rect = Rect::from_center_size(pos, Vec2::new(120.0, 40.0));
                            painter.rect_filled(rect, 4.0, Color32::from_gray(60));
                            painter.text(
                                rect.center(),
                                egui::Align2::CENTER_CENTER,
                                &node.label,
                                egui::FontId::proportional(12.0),
                                Color32::WHITE,
                            );
                        }

                        // 绘制边
                        for &(from_idx, _, to_idx, _) in &self.graph.edges {
                            if let (Some(from), Some(to)) =
                                (self.graph.nodes.get(from_idx), self.graph.nodes.get(to_idx))
                            {
                                let from_pos = Pos2::new(
                                    from.position[0] * self.zoom + self.pan.x,
                                    from.position[1] * self.zoom + self.pan.y,
                                );
                                let to_pos = Pos2::new(
                                    to.position[0] * self.zoom + self.pan.x,
                                    to.position[1] * self.zoom + self.pan.y,
                                );
                                painter.line_segment(
                                    [from_pos, to_pos],
                                    Stroke::new(2.0, Color32::from_rgb(100, 200, 100)),
                                );
                            }
                        }
                    });

                ui.separator();

                // WGSL 预览
                if self.dirty {
                    self.wgsl_preview = self.graph.generate_wgsl();
                    self.dirty = false;
                }

                ui.collapsing("WGSL Preview", |ui| {
                    ui.add(
                        egui::TextEdit::multiline(&mut self.wgsl_preview.as_str())
                            .font(egui::TextStyle::Monospace)
                            .desired_width(f32::INFINITY)
                            .desired_rows(10),
                    );
                });
            });
    }
}
