---
id: tools-edit
type: module-design
status: draft
title: tools/edit — 开发者单用途小工具
tags: [tools, editor, animation, material]
---

## 是什么

`tools/edit/` 放**单用途的开发者窗口工具**：一个目录一个工具，各自是独立的 Cargo bin 包（`cargo build` 直接在该目录里跑，不接入 `build.ps1`，也不进 `.vscode/settings.json` 的 linkedProjects）。它们不参与游戏运行时，只把引擎的某个数据侧能力单独拿出来看/改。

大而全的编辑器（原 `crates/editor/`）已整体移除——新增编辑能力时**优先加一个这样的小工具**，而不是重建一个多面板编辑器。

统一的窗口栈：`winit 0.30` + `wgpu 22.1` + `egui / egui-winit / egui-wgpu 0.29`，3D 画面渲染到窗口背景，egui 叠在上面；相机的轨道交互只在指针不落在 UI 上时生效（`egui_ctx.wants_pointer_input()`）。

## 组成

| 目录 | 包 | 作用 |
| --- | --- | --- |
| `prefs/` | `edit-prefs`（lib） | 共用：「上次打开的东西」记录 |
| `material/` | `material-editor` | 编辑项目下 `assets/materials/*.material.json`，球体实时预览 |
| `animation/` | `animation-preview` | 加载 glTF/GLB，播放/拖动它的骨骼动画剪辑，看蒙皮网格 |

## 共用约定：上次打开的东西（`edit-prefs`）

- 记录落在系统配置目录：`<config_dir>/geese/<app>.json`（Windows 即 `%APPDATA%\geese\`），内容 `{ "entries": { "<key>": { "name", "path" } } }`。
- 启动优先级统一为：**命令行参数 → 记录（且路径仍存在）→ 原生对话框（`rfd`）→ 空状态**。用户在对话框里取消，就停在空状态，工具不会自己挑一个目录/文件。
- 只有**成功**打开的资源才写记录；打开失败只报错、不覆盖记录，下一次启动还能重试同一路径。
- 记录读写失败、坏 JSON、取不到配置目录都退化为空记录并打日志——工具本身绝不能因为记录文件而启动失败。
- 内容未变时不落盘（`Prefs::set` 内部短路），切换选中项导致的写盘不会成为高频 IO。

## 工具契约

### `animation-preview`

用法 `animation-preview [model.gltf|model.glb]`，记录键 `model`，记录的是模型文件的完整路径 + 文件名。

- **加载走引擎真实路径**：`scene::import_scene(path, 8, 6)` 一次拿到网格 / 蒙皮 / `MaterialLibrary` / 节点层级 / `AnimationClip`，因此 `scene` 的默认 features（`physics` + `navmesh`）也被拖进来——`manifest.rs` 用了 `physics::` 而没有任何 `cfg` 门禁，关掉 feature 无法编译。这是接受的编译成本，不是遗漏。
- **每帧**：`Scene::update_animation(&mut player, dt)`；播放时 `dt` 为真实帧间隔，暂停/拖动时间轴时传 `0.0` 原地重采样。`avatar::sample_clip` 每次调用都会先把所有节点重置回 `base_transform` 再采样，所以 `dt = 0` 是定位到任意时间的合法手段，不需要第二套采样代码。
- **渲染**：`Scene::render_queue(&scene_renderer, None)` —— `None` 表示不做视锥/octree 剔除，被动画驱动的对象一律参与（`update_animation` 不重建 octree，动画物体也只走 `dynamic_indices`）。
- UI：左栏剪辑列表（点选即切剪辑并从头播）、底部 ComboBox + 播放/暂停/重置 + 可拖动时间轴（拖动期间暂停、松手恢复原播放状态）、底部状态栏显示文件名/路径/关节数/剪辑数。

### `material-editor`

用法 `material-editor [project_dir]`，记录键 `project`，记录的是项目目录完整路径 + 上次选中的材质文件名。

- 项目目录是**可选**的：只有用户明确指定才调用 `MaterialStore::open`（其内部会 `ensure_default_material_files` 补齐内置材质）。启动时不再用当前工作目录兜底，避免在错误目录里凭空产生材质文件。这是有意改变的行为。
- 记录里的目录与目标目录一致时，才用记录中的材质文件名定位选中项；否则落到第 0 个。切换选中材质即写回记录。

## 已知限制

- `render::MAX_JOINTS = 32`：单个蒙皮超过 32 个关节时，多出的关节被静默丢弃、网格扭曲。`animation-preview` 因此在状态栏常驻 `关节 N / 32` 并对超限标黄——让人能区分「模型/引擎上限」与「工具坏了」。修上限属于渲染器层的另一个任务。
- `shadow_depth.wgsl` 不处理 skin，蒙皮网格投出的阴影是绑定姿势。预览工具的灯光不特意规避。
- 预览工具按目录独立构建，没有共享的构建脚本或 CI 目标；没有 glTF 测试资源，蒙皮效果靠本地实跑验证。

## 非目标

- 不做骨骼线框/关节可视化、地面网格与坐标轴。
- 不做速度倍率、循环开关、动画 marker 事件提示（原 `crates/editor/src/animation_panel.rs` 有过）。
- 不支持把文件拖进窗口加载（模型/项目一律走「参数 → 记录 → 对话框」）。
