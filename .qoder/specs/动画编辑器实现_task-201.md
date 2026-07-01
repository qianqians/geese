# 动画编辑器实现计划

## 总体架构

```
AnimationClip (avatar crate)           ← 数据层: 添加 AnimationMarker
        ↓
Scene (scene crate)                    ← 检测层: 标记跨越检测 + 事件收集
        ↓
┌───────────────┬──────────────────────┐
│ AnimationPanel (editor crate)        │  Python Bridge (client/server)
│  - 时间轴 UI                          │  - PyO3 暴露 drain_marker_events()
│  - 标记 CRUD                          │  - Python animation_event_handler
│  - 动画预览                           │
└───────────────┴──────────────────────┘
```

---

## Task 1: 添加 `AnimationMarker` 数据结构到 avatar crate

**文件**: `crates/avatar/src/animation.rs`

在 `AnimationClip` (L67-72) 之前添加标记结构体:

```rust
/// 动画时间轴上的命名标记点。
/// 当动画播放跨越此时间点时,触发同名事件。
#[derive(Clone, Debug)]
pub struct AnimationMarker {
    pub time: f32,
    pub name: String,
}
```

为 `AnimationClip` 添加 `markers` 字段:

```rust
#[derive(Clone, Debug)]
pub struct AnimationClip {
    pub name: Option<String>,
    pub duration: f32,
    pub channels: Vec<AnimationChannel>,
    pub markers: Vec<AnimationMarker>,  // NEW
}
```

**兼容性**: 所有现有构造 `AnimationClip` 的代码(如 `make_dummy_clip` in `character_animation.rs` L315-321, `classify_objects` 测试 L809-822) 在结构体字面量中缺少 `markers` 字段会导致编译错误,需要全局批量添加 `markers: vec![]`。

**同时修改 `AnimationClip` 的所有构造函数**:
- `crates/scene/src/character_animation.rs#L315-L321` — `make_dummy_clip`
- `crates/scene/src/scene.rs#L809` — 测试中的 AnimationClip 构造
- `crates/scene/src/lib.rs` — `load_animations()` 中的 AnimationClip 构造
- GLTF 加载代码中所有 AnimationClip 构造

---

## Task 2: 在 avatar crate 中添加标记辅助函数

**文件**: `crates/avatar/src/animation.rs`

在 `AnimationPlayer::advance()` 之后,`sample_clip` 之前,添加标记检测辅助函数:

```rust
/// 检查两个时间点之间跨越的所有标记(处理循环回绕)。
/// prev_time: 上一帧的时间
/// curr_time: 当前帧的时间
/// duration: 动画总时长
/// 返回被跨越的标记索引列表。
pub fn check_markers_crossed(
    markers: &[AnimationMarker],
    prev_time: f32,
    curr_time: f32,
    duration: f32,
) -> Vec<usize> {
    if markers.is_empty() || duration <= 0.0 {
        return vec![];
    }

    let mut result = Vec::new();

    if curr_time >= prev_time {
        // 正常前进(含循环回绕边界内的前进)
        for (i, m) in markers.iter().enumerate() {
            if m.time > prev_time && m.time <= curr_time {
                result.push(i);
            }
        }
    } else {
        // 循环回绕: prev_time → duration + 0 → curr_time
        for (i, m) in markers.iter().enumerate() {
            if m.time > prev_time || m.time <= curr_time {
                result.push(i);
            }
        }
    }

    result
}
```

**同时添加回绕时间检测到 `AnimationPlayer::advance()`**: 在 L129 `self.time += dt * self.speed;` 之前记录 `prev_time`,返回时包含回绕信息。但为了最小侵入性,先不改 `advance()` 签名,而是在调用方记录 `prev_time`。

---

## Task 3: 导出新类型

**文件**: `crates/avatar/src/lib.rs`

在现有 re-export 中添加:
```rust
pub use animation::{
    ..., AnimationMarker, check_markers_crossed,
};
```

**文件**: `crates/scene/src/lib.rs`

确认已从 avatar re-export 相关类型(目前已有 `AnimationClip` 等)。

---

## Task 4: 在 Scene 中添加标记事件收集

**文件**: `crates/scene/src/scene.rs`

### 4a. 添加事件结构体和收集字段

在 `Scene` struct (L23-47) 中添加:

```rust
/// 本帧触发的动画标记事件,由外部消费者 drain。
pub marker_events: Vec<MarkerEvent>,
```

在文件顶部添加事件结构体:
```rust
/// 动画标记触发事件
#[derive(Clone, Debug)]
pub struct MarkerEvent {
    pub marker_name: String,
    pub clip_index: usize,
    pub clip_name: Option<String>,
    pub entity_id: Option<String>,
}
```

初始化: 在 `Scene::new()` (L67) 中添加 `marker_events: Vec::new()`。

### 4b. 在 `update_animation()` 中检测标记

修改 `update_animation()` (L154-164):

```rust
pub fn update_animation(&mut self, player: &mut AnimationPlayer, dt: f32) {
    let Some(clip) = self.animations.get(player.clip) else {
        return;
    };

    let prev_time = player.time;  // NEW
    player.advance(dt, clip.duration);
    
    // 检测标记跨越 (NEW)
    if !clip.markers.is_empty() {
        let crossed = check_markers_crossed(
            &clip.markers, prev_time, player.time, clip.duration,
        );
        for idx in crossed {
            let m = &clip.markers[idx];
            self.marker_events.push(MarkerEvent {
                marker_name: m.name.clone(),
                clip_index: player.clip,
                clip_name: clip.name.clone(),
                entity_id: None,
            });
        }
    }

    sample_clip(clip, player.time, &mut self.nodes);
    self.update_world_transforms();
}
```

### 4c. 在 `update_animation_graph()` 中检测标记

修改 `update_animation_graph()` (L166-254):
- 在 `active = graph.update(dt, &self.animations)` 之后遍历 `active` 动画
- 对每个活跃动画,检测其时间区间是否跨越标记:
  - 问题: AnimationStateMachine 的 `update()` 直接返回合并后的 `ActiveAnimation`,不暴露 `prev_time`
  - 解决: 在 Scene 中维护 `graph_prev_times: HashMap<(graph_id, clip_idx), f32>` 来跟踪每个动画图+剪辑的上次时间
  - 按 `ActiveAnimation.weight` 区分:仅主动画(weight > 0.5)触发事件

```rust
// 在 update_animation_graph 方法内,active 计算之后:
for anim in &active {
    if anim.weight > 0.5 {
        if let Some(clip) = self.animations.get(anim.clip) {
            if !clip.markers.is_empty() {
                let key = (anim.clip, 0); // 简化:单图用 clip_index 做 key
                let prev = self.graph_prev_times.get(&key).copied().unwrap_or(0.0);
                let crossed = check_markers_crossed(
                    &clip.markers, prev, anim.time, clip.duration,
                );
                for idx in crossed {
                    let m = &clip.markers[idx];
                    self.marker_events.push(MarkerEvent {
                        marker_name: m.name.clone(),
                        clip_index: anim.clip,
                        clip_name: clip.name.clone(),
                        entity_id: None,
                    });
                }
            }
        }
    }
    self.graph_prev_times.insert(key, anim.time);
}
```

### 4d. 添加 drain 方法

```rust
/// 消费本帧所有已触发的标记事件。
pub fn drain_marker_events(&mut self) -> Vec<MarkerEvent> {
    std::mem::take(&mut self.marker_events)
}
```

---

## Task 5: 添加 `PanelLayer::Animation` 变体

**文件**: `crates/editor/src/panel_layer.rs`

在 `PanelLayer` enum (L12-23) 中添加:
```rust
Animation = 5,
```

在 `PanelLayerManager::default()` (L37-43) 中添加:
```rust
visibility.insert(PanelLayer::Animation, true);
```

---

## Task 6: 创建 `AnimationPanel` 编辑器面板

**新文件**: `crates/editor/src/animation_panel.rs`

### 6a. 结构体定义

```rust
use crate::panels::{EditorAction, EditorPanel, EditorState};

pub struct AnimationPanel {
    /// 当前选中的动画剪辑索引
    selected_clip: Option<usize>,
    /// 预览播放器状态
    preview_time: f32,
    preview_playing: bool,
    preview_speed: f32,
    preview_looping: bool,
    /// 新标记编辑
    new_marker_name: String,
    /// 上次选中实体(检测变化)
    last_selected: Option<String>,
    /// 可用剪辑名称缓存
    clip_names: Vec<String>,
    clip_durations: Vec<f32>,
    /// 触发的标记事件(预览中显示)
    fired_markers: Vec<String>,
    fired_timer: f32,
}
```

### 6b. UI 布局(实现 `EditorPanel` trait)

**1. 顶部控制栏**:
- 剪辑选择器: `egui::ComboBox` 列出场景中所有动画剪辑
- 播放/暂停按钮: ▶️/⏸️
- 循环开关
- 速度滑块: 0.1x ~ 3.0x

**2. 时间轴区域**(核心):
- **时间刻度**: 使用 `ui.painter()` 绘制水平标尺,带时间刻度
- **播放头**: 红色竖线标记当前 `preview_time`,可拖拽
- **标记显示**: 每个标记渲染为彩色菱形 + 标签文字
- **点击添加**: 在时间轴上点击空白区域 → 添加标记(使用当前时间)
- **右键删除**: 右键点击标记 → 删除
- **编辑标记名**: 双击标记标签 → 进入文本编辑

绘制逻辑:
```rust
// 在 ScrollArea 内使用 Painter 绘制
let rect = ui.max_rect();
let pixels_per_second = 100.0; // 缩放因子
let timeline_width = duration * pixels_per_second;

// 绘制时间刻度线
for t in (0..=duration as u32).step_by(tick_interval) {
    let x = timeline_rect.left() + t as f32 * pixels_per_second;
    ui.painter().line_segment([pos2(x, top), pos2(x, bottom)], stroke);
}

// 绘制标记菱形
for marker in markers {
    let x = timeline_rect.left() + marker.time * pixels_per_second;
    // 绘制彩色菱形
    let diamond = [/* 四个顶点 */];
    ui.painter().add(egui::Shape::convex_polygon(diamond, color, stroke));
    // 绘制标签
    ui.painter().text(pos2(x, label_y), anchor, marker.name, font_id, text_color);
}
```

**3. 标记列表**:
- 表格显示: 时间 | 名称 | 操作(删除按钮)
- 支持内联编辑标记名称

**4. 底部状态栏**:
- 显示当前时间 / 总时长
- 显示触发事件闪烁提示

### 6c. 标记持久化

标记数据通过 `EditorAction::ModifyAnimationMarker` 写回 `Scene.animations[clip_idx].markers`,随场景序列化保存。

---

## Task 7: 添加 `EditorAction` 新变体

**文件**: `crates/editor/src/panels.rs`

在 `EditorAction` enum (L20-40) 中添加:

```rust
/// 修改动画标记
ModifyAnimationMarker {
    clip_index: usize,
    time: f32,
    name: String,
    remove: bool,
},
```

---

## Task 8: 在 Editor 中集成动画数据和标记处理

**文件**: `crates/editor/src/editor.rs`

### 8a. 添加 `AnimationPanel` 字段

在 `Editor` struct (L32-76) 中添加:
```rust
animation_panel: AnimationPanel,
```

在 `Editor::open()` 初始化:
```rust
animation_panel: AnimationPanel::new(),
```

### 8b. 同步动画数据到 EditorState

在 `Editor::update()` 中同步动画剪辑信息:
```rust
// 同步动画剪辑信息供面板使用
self.state.animation_clips.clear();
for (i, clip) in self.scene.animations.iter().enumerate() {
    self.state.animation_clips.push((
        clip.name.clone().unwrap_or_else(|| format!("Clip {}", i)),
        clip.duration,
        i,
    ));
}
// 同步标记数据
self.state.animation_markers.clear();
for clip in &self.scene.animations {
    let markers: Vec<(f32, String)> = clip.markers.iter()
        .map(|m| (m.time, m.name.clone()))
        .collect();
    self.state.animation_markers.push(markers);
}
```

### 8c. 预览播放驱动

在 `Editor::update()` 中添加:
```rust
// 动画预览更新
if self.animation_panel.preview_playing {
    if let Some(clip_idx) = self.animation_panel.selected_clip {
        if let Some(clip) = self.scene.animations.get(clip_idx) {
            let mut player = AnimationPlayer::new(clip_idx);
            player.time = self.animation_panel.preview_time;
            player.playing = self.animation_panel.preview_playing;
            player.speed = self.animation_panel.preview_speed;
            player.looping = self.animation_panel.preview_looping;
            self.scene.update_animation(&mut player, dt);
            self.animation_panel.preview_time = player.time;
            // 收集触发事件
            let events = self.scene.drain_marker_events();
            if !events.is_empty() {
                self.animation_panel.on_markers_fired(&events);
            }
        }
    }
}
```

### 8d. 处理标记操作

在 `process_prefab_actions()` 中添加:
```rust
EditorAction::ModifyAnimationMarker { clip_index, time, name, remove } => {
    if let Some(clip) = self.scene.animations.get_mut(clip_index) {
        if remove {
            clip.markers.retain(|m| m.time != time || m.name != name);
        } else {
            // 避免重复
            if !clip.markers.iter().any(|m| m.time == time && m.name == name) {
                clip.markers.push(AnimationMarker { time, name });
                // 保持按时间排序
                clip.markers.sort_by(|a, b| a.time.partial_cmp(&b.time).unwrap());
            }
        }
    }
}
```

### 8e. 渲染 AnimationPanel

在 `show_editor_layout()` 或 `update()` 中添加浮动窗口:
```rust
if self.state.panel_layer.is_visible(&PanelLayer::Animation) {
    egui::Window::new("Animation")
        .default_pos([400.0, 500.0])
        .default_size([700.0, 250.0])
        .show(ctx, |ui| {
            self.animation_panel.show(ui, &mut self.state);
        });
}
```

### 8f. View 菜单添加快捷键

在 `show_menu_bar()` 的 View 菜单中添加:
```rust
let mut anim_vis = self.state.panel_layer.is_visible(&PanelLayer::Animation);
if ui.checkbox(&mut anim_vis, "Animation").clicked() {
    self.state.panel_layer.set_visible(PanelLayer::Animation, anim_vis);
    ui.close_menu();
}
```

---

## Task 9: 更新 EditorState 支持动画数据

**文件**: `crates/editor/src/panels.rs`

在 `EditorState` (L77-106) 中添加:
```rust
/// 动画剪辑信息: (name, duration, index)
pub animation_clips: Vec<(String, f32, usize)>,
/// 每个剪辑的标记列表: Vec<(time, name)>
pub animation_markers: Vec<Vec<(f32, String)>>,
```

在 `EditorState::new()` 中初始化:
```rust
animation_clips: Vec::new(),
animation_markers: Vec::new(),
```

---

## Task 10: 模块注册

**文件**: `crates/editor/src/lib.rs`

添加:
```rust
pub mod animation_panel;
```

---

## Task 11: Python 事件响应接口

### 11a. Python 事件处理器

**新文件**: `client/engine/animation_events.py`

```python
from collections.abc import Callable

class animation_event_handler:
    """动画标记事件处理器。
    
    用法:
        handler = animation_event_handler()
        handler.register("footstep", lambda name, clip, time: print(f"Footstep at {time}"))
    """
    def __init__(self):
        self._handlers: dict[str, list[Callable[[str, str, float], None]]] = {}

    def register(self, marker_name: str, callback: Callable[[str, str, float], None]):
        """注册标记事件回调。
        
        Args:
            marker_name: 标记名称
            callback: 回调函数, 参数为 (marker_name, clip_name, time)
        """
        if marker_name not in self._handlers:
            self._handlers[marker_name] = []
        self._handlers[marker_name].append(callback)

    def unregister(self, marker_name: str, callback: Callable):
        """取消注册回调。"""
        if marker_name in self._handlers:
            try:
                self._handlers[marker_name].remove(callback)
            except ValueError:
                pass

    def fire(self, marker_name: str, clip_name: str, time: float):
        """触发标记事件(内部调用)。"""
        for cb in self._handlers.get(marker_name, []):
            cb(marker_name, clip_name, time)

    def fire_all(self, events: list[tuple[str, str, float]]):
        """批量触发事件。"""
        for name, clip, time in events:
            self.fire(name, clip, time)
```

### 11b. Rust 侧的 PyO3 桥接

在 `client/lib/client/src/` 中,为 `ClientContext` 添加:
```rust
/// 获取并清空动画标记事件队列
#[pyo3(name = "drain_marker_events")]
fn drain_marker_events(&mut self) -> Vec<(String, String, f32)> {
    // 从 Scene 获取标记事件并转换为 Python 可读格式
    let events = self.scene.drain_marker_events();
    events.into_iter().map(|e| {
        (e.marker_name, e.clip_name.unwrap_or_default(), 0.0f32)
    }).collect()
}
```

### 11c. 集成到 app.py

**文件**: `client/engine/app.py`

在 `app` 类中添加:
```python
from .animation_events import animation_event_handler

class app:
    def __init__(self):
        # ... existing code ...
        self.animation_events = animation_event_handler()

    def poll(self):
        while self.__is_run__:
            start = time.time()
            self.poll_conn_msg()
            # 处理动画标记事件
            events = self.__pump__.drain_marker_events()
            if events:
                self.animation_events.fire_all(events)
            tick = time.time() - start
            if tick < 0.033:
                time.sleep(0.033 - tick)
```

---

## Task 12: 全局构造点更新

由于 `AnimationClip` 添加了新字段 `markers`,需要更新所有构造点:

需要修改的文件(通过搜索 `AnimationClip {` 定位):
- `crates/scene/src/character_animation.rs#L315-L321` — `make_dummy_clip`
- `crates/scene/src/scene.rs#L809-L822` — 测试中的 AnimationClip
- `crates/scene/src/lib.rs` — GLTF 加载中的 AnimationClip(搜索 `load_animations`)

所有位置添加 `markers: vec![]`。

---

## 依赖关系

```
Task 1 (AnimationMarker 数据结构)
  └─→ Task 2 (标记检测函数) → Task 3 (导出)
       └─→ Task 4 (Scene 标记检测) → Task 12 (全局构造点更新)
            ├─→ Task 5 (PanelLayer) → Task 8e (渲染窗口)
            ├─→ Task 6 (AnimationPanel) → Task 8 (Editor 集成)
            ├─→ Task 7 (EditorAction) → Task 8d (处理标记操作)
            └─→ Task 9 (EditorState) → Task 6, Task 8b
                                  └─→ Task 10 (模块注册)
Task 11 (Python 接口) — 独立,可最后实现
```

**并行执行机会**: Task 5, Task 7, Task 9 可以并行; Task 6 和 Task 11 可以并行(依赖 Task 1-4 完成后)。

---

## 风险与缓解

| 风险 | 严重度 | 缓解措施 |
|------|--------|----------|
| **编译错误: AnimationClip 构造点缺少 `markers`** | 低 | Task 12 专门处理;所有构造点都是编译期错误,不会遗漏 |
| **循环回绕标记丢失** | 中 | `check_markers_crossed` 专门处理 `curr_time < prev_time` 情况;单元测试覆盖 |
| **动画图过渡期间错误触发** | 中 | 仅 weight > 0.5 的主动画触发标记(过渡时两边权重各 0.5 都不触发);文档说明 |
| **预览帧时间不准确** | 低 | 复用编辑器现有的 dt 计算(已有 `last_update`),保证帧率稳定 |
| **egui 时间轴性能** | 低 | 仅在 `preview_playing` 时重绘;标记数量通常 < 50,绘制开销可忽略 |
| **Python GIL 阻塞** | 低 | 事件收集在 Rust 侧(drain 模式),Python 侧在 poll 循环中处理,不阻塞渲染 |

---

## 被拒绝的替代方案

1. **新增 `animation_event` crate + crossbeam channel** (来自 Plan A/B): 编辑器是单线程 egui 应用,不需要 lock-free channel。增加了不必要的依赖复杂度。

2. **修改 `AnimationPlayer::advance()` 签名为标记检测添加 callback** (来自 Plan A/B): 破坏性 API 变更,需要修改所有调用方。改为在 Scene 层调用方检测更安全。

3. **使用 `Box<dyn Fn>` 回调** (来自 Plan C): 难以序列化,与场景数据模型不匹配。改用 `Vec<MarkerEvent>` drain 模式,消费者拉取而非推送。

4. **标记数据单独文件存储** (来自 Plan A): 增加文件管理复杂度。标记属于动画剪辑的内在属性,与剪辑一起序列化即可。

---

## 关键文件

1. **[`crates/avatar/src/animation.rs`](file://d:/Personal/geese/crates/avatar/src/animation.rs)** — 添加 `AnimationMarker`、`check_markers_crossed`,修改 `AnimationClip`
2. **[`crates/scene/src/scene.rs`](file://d:/Personal/geese/crates/scene/src/scene.rs)** — `MarkerEvent` 定义、标记检测集成、`drain_marker_events()`
3. **[`crates/editor/src/animation_panel.rs`](file://d:/Personal/geese/crates/editor/src/animation_panel.rs)** (新) — 时间轴 UI、标记 CRUD、预览控制
4. **[`crates/editor/src/editor.rs`](file://d:/Personal/geese/crates/editor/src/editor.rs)** — 集成中枢:面板生命周期、数据同步、标记操作处理
5. **[`crates/editor/src/panels.rs`](file://d:/Personal/geese/crates/editor/src/panels.rs)** — `EditorAction` 新变体、`EditorState` 扩展

**预估总改动量**: ~500 行代码,跨 10 个文件(含 2 个新文件)。