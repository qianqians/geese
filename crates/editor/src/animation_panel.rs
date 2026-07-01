//! 动画面板。
//!
//! 提供动画剪辑预览、时间轴标记管理和标记事件可视化。
//! 遵循 [`EditorPanel`] trait 模式,与 InspectorPanel 保持一致的设计风格。

use crate::panels::{EditorAction, EditorPanel, EditorState};
use egui::{Color32, Pos2, Rect, Stroke};

/// 动画面板。
pub struct AnimationPanel {
    /// 当前选中的动画剪辑索引
    pub selected_clip: Option<usize>,
    /// 预览播放器状态
    pub preview_time: f32,
    pub preview_playing: bool,
    pub preview_speed: f32,
    pub preview_looping: bool,
    /// 上次选中实体（检测变化）
    last_selected: Option<String>,
    /// 新标记名称输入
    new_marker_name: String,
    /// 触发的标记事件（预览中显示闪烁提示）
    pub fired_markers: Vec<String>,
    /// 事件闪烁计时器（秒）
    fired_flash_timer: f32,
    /// 正在编辑的标记（索引,用于内联编辑）
    editing_marker_idx: Option<usize>,
    editing_marker_name: String,
}

impl AnimationPanel {
    pub fn new() -> Self {
        Self {
            selected_clip: None,
            preview_time: 0.0,
            preview_playing: false,
            preview_speed: 1.0,
            preview_looping: true,
            last_selected: None,
            new_marker_name: String::new(),
            fired_markers: Vec::new(),
            fired_flash_timer: 0.0,
            editing_marker_idx: None,
            editing_marker_name: String::new(),
        }
    }

    /// 标记触发回调（由 Editor 在预览播放时调用）
    pub fn on_markers_fired(&mut self, events: &[scene::MarkerEvent]) {
        for e in events {
            self.fired_markers.push(e.marker_name.clone());
        }
        self.fired_flash_timer = 1.5; // 闪烁持续 1.5 秒
    }

    /// 更新内部计时器
    pub fn update_timer(&mut self, dt: f32) {
        if self.fired_flash_timer > 0.0 {
            self.fired_flash_timer = (self.fired_flash_timer - dt).max(0.0);
            if self.fired_flash_timer == 0.0 {
                self.fired_markers.clear();
            }
        }
    }

    /// 获取当前剪辑的标记列表（从 EditorState 读取）
    fn current_markers<'a>(&self, state: &'a EditorState) -> &'a [(f32, String)] {
        if let Some(idx) = self.selected_clip {
            if let Some(markers) = state.animation_markers.get(idx) {
                return markers.as_slice();
            }
        }
        &[]
    }

    /// 获取当前剪辑的时长
    fn current_duration(&self, state: &EditorState) -> f32 {
        if let Some(idx) = self.selected_clip {
            if let Some(clip) = state.animation_clips.iter().find(|c| c.2 == idx) {
                return clip.1;
            }
        }
        0.0
    }
}

impl EditorPanel for AnimationPanel {
    fn title(&self) -> &str {
        "Animation"
    }

    fn show(&mut self, ui: &mut egui::Ui, state: &mut EditorState) {
        ui.strong("Animation");

        // ── 剪辑选择器 ──
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Clip:");

            let mut selected_name = String::new();
            if let Some(idx) = self.selected_clip {
                if let Some(clip) = state.animation_clips.iter().find(|c| c.2 == idx) {
                    selected_name = format!("{} ({:.1}s)", clip.0, clip.1);
                }
            }

            egui::ComboBox::from_id_salt("animation_clip_selector")
                .selected_text(&selected_name)
                .show_ui(ui, |ui| {
                    for (name, dur, idx) in &state.animation_clips {
                        let label = format!("{} ({:.1}s)", name, dur);
                        if ui.selectable_label(false, &label).clicked() {
                            self.selected_clip = Some(*idx);
                            self.preview_time = 0.0;
                            self.preview_playing = false;
                            self.fired_markers.clear();
                            self.fired_flash_timer = 0.0;
                        }
                    }
                });
        });

        // ── 播放控制 ──
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            // 播放/暂停按钮
            let play_label = if self.preview_playing { "⏸ Pause" } else { "▶ Play" };
            if ui.button(play_label).clicked() {
                self.preview_playing = !self.preview_playing;
            }

            // 停止按钮
            if ui.button("⏹ Stop").clicked() {
                self.preview_playing = false;
                self.preview_time = 0.0;
            }

            ui.separator();

            // 循环开关
            ui.checkbox(&mut self.preview_looping, "🔁 Loop");

            // 速度控制
            ui.label("Speed:");
            ui.add(egui::Slider::new(&mut self.preview_speed, 0.1..=3.0).step_by(0.1));
            ui.label(format!("{:.1}x", self.preview_speed));
        });

        ui.add_space(4.0);

        // ── 时间轴 ──
        let duration = self.current_duration(state);
        self.render_timeline(ui, state, duration);

        ui.add_space(4.0);

        // ── 标记列表 ──
        self.render_marker_list(ui, state);

        // ── 状态栏 ──
        ui.add_space(4.0);
        ui.separator();
        ui.horizontal(|ui| {
            ui.label(format!(
                "Time: {:.2}s / {:.2}s",
                self.preview_time, duration
            ));

            if self.fired_flash_timer > 0.0 && !self.fired_markers.is_empty() {
                let flash = (self.fired_flash_timer * 4.0) as u32 % 2 == 0;
                let color = if flash {
                    Color32::from_rgb(255, 200, 0)
                } else {
                    Color32::from_rgb(255, 100, 0)
                };
                ui.label(
                    egui::RichText::new(format!(
                        "⚡ Fired: {}",
                        self.fired_markers.join(", ")
                    ))
                    .color(color),
                );
            }
        });
    }
}

impl AnimationPanel {
    /// 渲染时间轴区域（自定义 egui 绘制）
    fn render_timeline(&mut self, ui: &mut egui::Ui, state: &mut EditorState, duration: f32) {
        if duration <= 0.0 {
            ui.label("No animation clip selected or duration is zero.");
            return;
        }

        let desired_size = egui::vec2(ui.available_width(), 48.0);
        let (rect, response) = ui.allocate_exact_size(desired_size, egui::Sense::click_and_drag());

        let pixels_per_second = rect.width() / duration.max(0.01);
        let timeline_top = rect.top() + 4.0;
        let timeline_bottom = rect.bottom() - 4.0;
        let timeline_mid = (timeline_top + timeline_bottom) / 2.0;

        // 背景
        ui.painter().rect_filled(
            rect,
            2.0,
            Color32::from_gray(28),
        );

        // 时间刻度线
        let tick_interval = Self::calc_tick_interval(duration, rect.width());
        let mut t = 0.0;
        while t <= duration + f32::EPSILON {
            let x = rect.left() + t * pixels_per_second;
            let tick_top = timeline_mid - 6.0;
            let tick_bottom = timeline_mid + 6.0;
            ui.painter().line_segment(
                [Pos2::new(x, tick_top), Pos2::new(x, tick_bottom)],
                Stroke::new(1.0, Color32::from_gray(80)),
            );
            t += tick_interval;
        }

        // 主时间线
        ui.painter().line_segment(
            [Pos2::new(rect.left(), timeline_mid), Pos2::new(rect.right(), timeline_mid)],
            Stroke::new(1.5, Color32::from_gray(120)),
        );

        // 标记（菱形 + 标签）
        let markers = self.current_markers(state).to_vec();
        for (_i, (time, name)) in markers.iter().enumerate() {
            let x = rect.left() + time * pixels_per_second;
            if x < rect.left() || x > rect.right() {
                continue;
            }
            let mid = Pos2::new(x, timeline_mid);

            // 菱形
            let diamond = vec![
                Pos2::new(mid.x, mid.y - 5.0),
                Pos2::new(mid.x + 4.0, mid.y),
                Pos2::new(mid.x, mid.y + 5.0),
                Pos2::new(mid.x - 4.0, mid.y),
            ];
            ui.painter().add(egui::Shape::convex_polygon(
                diamond,
                Color32::from_rgb(0, 180, 255),
                Stroke::new(1.0, Color32::WHITE),
            ));

            // 标签
            let label_pos = Pos2::new(mid.x, timeline_top - 2.0);
            ui.painter().text(
                label_pos,
                egui::Align2::CENTER_BOTTOM,
                name,
                egui::FontId::proportional(10.0),
                Color32::LIGHT_GRAY,
            );

            // 右键删除
            let marker_rect = Rect::from_center_size(mid, egui::vec2(12.0, 14.0));
            if ui.rect_contains_pointer(marker_rect) {
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                if ui.input(|i| i.pointer.secondary_clicked()) {
                    state.pending_actions.push(EditorAction::ModifyAnimationMarker {
                        clip_index: self.selected_clip.unwrap(),
                        time: *time,
                        name: name.clone(),
                        remove: true,
                    });
                }
            }
        }

        // 播放头（红色竖线）
        let playhead_x = rect.left() + self.preview_time * pixels_per_second;
        if playhead_x >= rect.left() && playhead_x <= rect.right() {
            ui.painter().line_segment(
                [Pos2::new(playhead_x, timeline_top), Pos2::new(playhead_x, timeline_bottom)],
                Stroke::new(1.5, Color32::RED),
            );
            // 三角形头部
            let tri = vec![
                Pos2::new(playhead_x, timeline_top),
                Pos2::new(playhead_x - 5.0, timeline_top - 6.0),
                Pos2::new(playhead_x + 5.0, timeline_top - 6.0),
            ];
            ui.painter().add(egui::Shape::convex_polygon(
                tri,
                Color32::RED,
                Stroke::new(1.0, Color32::RED),
            ));
        }

        // 点击时间轴 → 添加标记或是拖拽播放头
        if response.clicked() {
            if let Some(mouse_pos) = response.interact_pointer_pos() {
                let clicked_time = ((mouse_pos.x - rect.left()) / pixels_per_second)
                    .clamp(0.0, duration);

                // 如果按住 Shift → 拖拽播放头；否则 → 添加标记
                let shift = ui.input(|i| i.modifiers.shift);
                if shift {
                    self.preview_time = clicked_time;
                } else if let Some(clip_idx) = self.selected_clip {
                    // 添加标记
                    let marker_name = if self.new_marker_name.is_empty() {
                        format!("marker_{:.2}", clicked_time)
                    } else {
                        std::mem::take(&mut self.new_marker_name)
                    };
                    state.pending_actions.push(EditorAction::ModifyAnimationMarker {
                        clip_index: clip_idx,
                        time: clicked_time,
                        name: marker_name,
                        remove: false,
                    });
                }
            }
        }

        // 拖拽播放头
        if response.dragged() {
            if let Some(mouse_pos) = response.interact_pointer_pos() {
                self.preview_time = ((mouse_pos.x - rect.left()) / pixels_per_second)
                    .clamp(0.0, duration);
            }
        }
    }

    /// 渲染标记列表
    fn render_marker_list(&mut self, ui: &mut egui::Ui, state: &mut EditorState) {
        ui.label("Markers:");

        // 添加标记输入行
        ui.horizontal(|ui| {
            ui.label("Name:");
            ui.text_edit_singleline(&mut self.new_marker_name);
            if ui.button("Add at current time").clicked() {
                if let Some(clip_idx) = self.selected_clip {
                    let name = if self.new_marker_name.is_empty() {
                        format!("marker_{:.2}", self.preview_time)
                    } else {
                        std::mem::take(&mut self.new_marker_name)
                    };
                    state.pending_actions.push(EditorAction::ModifyAnimationMarker {
                        clip_index: clip_idx,
                        time: self.preview_time,
                        name,
                        remove: false,
                    });
                }
            }
        });

        ui.add_space(2.0);

        // 标记表格
        let markers = self.current_markers(state).to_vec();
        if markers.is_empty() {
            ui.label(
                egui::RichText::new("  No markers. Click on timeline or use 'Add at current time' to create one.")
                    .size(12.0)
                    .color(Color32::GRAY),
            );
            return;
        }

        // 表头
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Time").strong());
            ui.add_space(40.0);
            ui.label(egui::RichText::new("Name").strong());
        });
        ui.separator();

        for (i, (time, name)) in markers.iter().enumerate() {
            ui.horizontal(|ui| {
                // 时间显示 + 跳转按钮
                if ui
                    .add_sized([60.0, 18.0], egui::Button::new(format!("{:.2}s", time)))
                    .clicked()
                {
                    self.preview_time = *time;
                }

                // 名称编辑
                if self.editing_marker_idx == Some(i) {
                    if ui.text_edit_singleline(&mut self.editing_marker_name).lost_focus()
                        || ui.input(|inp| inp.key_pressed(egui::Key::Enter))
                    {
                        if !self.editing_marker_name.is_empty()
                            && self.editing_marker_name != *name
                        {
                            if let Some(clip_idx) = self.selected_clip {
                                // 删除旧标记,添加新标记(相当于重命名)
                                state.pending_actions.push(EditorAction::ModifyAnimationMarker {
                                    clip_index: clip_idx,
                                    time: *time,
                                    name: name.clone(),
                                    remove: true,
                                });
                                state.pending_actions.push(EditorAction::ModifyAnimationMarker {
                                    clip_index: clip_idx,
                                    time: *time,
                                    name: self.editing_marker_name.clone(),
                                    remove: false,
                                });
                            }
                        }
                        self.editing_marker_idx = None;
                        self.editing_marker_name.clear();
                    }
                } else {
                    if ui
                        .add_sized(
                            [120.0, 18.0],
                            egui::Label::new(egui::RichText::new(name).size(12.0)).sense(egui::Sense::click()),
                        )
                        .double_clicked()
                    {
                        self.editing_marker_idx = Some(i);
                        self.editing_marker_name = name.clone();
                    }
                }

                // 删除按钮
                if ui.button("✕").clicked() {
                    if let Some(clip_idx) = self.selected_clip {
                        state.pending_actions.push(EditorAction::ModifyAnimationMarker {
                            clip_index: clip_idx,
                            time: *time,
                            name: name.clone(),
                            remove: true,
                        });
                    }
                }
            });
        }
    }

    /// 根据时间轴宽度和时长计算合适的刻度间隔
    fn calc_tick_interval(duration: f32, width: f32) -> f32 {
        let approx_ticks = (width / 80.0).max(2.0);
        let raw_interval = duration / approx_ticks;

        // 选择"好看"的间隔：0.1, 0.25, 0.5, 1, 2, 5, 10...
        let nice_intervals = [0.1, 0.25, 0.5, 1.0, 2.0, 5.0, 10.0, 30.0, 60.0];
        for &ni in nice_intervals.iter() {
            if raw_interval <= ni {
                return ni;
            }
        }
        60.0
    }
}
