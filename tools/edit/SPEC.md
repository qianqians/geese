---
id: tools-edit
type: module-design
status: draft
title: tools/edit — 开发者单用途小工具
tags: [tools, editor, animation, material, scene]
references: [render-gpu-cache]
---

## 是什么

`tools/edit/` 放**单用途的开发者窗口工具**：一个目录一个工具，各自是独立的 Cargo bin 包（`cargo build` 直接在该目录里跑，不接入 `build.ps1`，也不进 `.vscode/settings.json` 的 linkedProjects）。它们不参与游戏运行时，只把引擎的某个数据侧能力单独拿出来看/改。

大而全的编辑器（原 `crates/editor/`）已整体移除——新增编辑能力时**优先加一个这样的小工具**，而不是重建一个多面板编辑器。`scene` 是这条规则的边界形态：它确实有三个面板，但职责仍然单一（只把 `.scene.json` 变成看得见的场景），不是原来那种一个窗口管全部资产、模式、打包、物理调试的全能编辑器。

这三个工具和引擎运行时由 `geese` 命令行拉起（见 `tools-cli`）：`geese scene` / `material` / `animation` 找工程根并用 `--workdir` 传进来。

统一的窗口栈：`winit 0.30` + `wgpu 22.1` + `egui / egui-winit / egui-wgpu 0.29`，3D 画面渲染到窗口背景，egui 叠在上面；相机的轨道交互只在指针不落在 UI 上时生效（`egui_ctx.wants_pointer_input()`）。

## 组成

| 目录 | 包 | 作用 |
| --- | --- | --- |
| `prefs/` | `edit-prefs`（lib） | 共用：「上次打开的东西」记录 |
| `material/` | `material-editor` | 编辑工作目录下的 `assets/materials/*.material.json`，球体实时预览 |
| `scene/` | `edit-scene`（bin `scene`） | 编辑工作目录下的 `assets/scenes/*.scene.json`：左栏拖模型、中间预览、右栏改 JSON |
| `animation/` | `animation-preview` | 加载 glTF/GLB，播放/拖动它的骨骼动画剪辑，看蒙皮网格 |

## 工作目录（`--workdir`）

每个工具启动时**必须**指定一个工作目录：`<工具> --workdir <目录>`。

- 缺参数、给出未知参数、`--workdir` 缺值 → 打印用法并以码 2 退出；目录不存在则自动创建。
- 工作目录在本次运行内**不可切换**，换目录就是重启带另一个参数（UI 里没有切换入口）。
- 工具的**运行数据全部落在工作目录内**，不写 `%APPDATA%` 或任何用户级配置目录：记录在 `<工作目录>/.data/<app>.json`。
- 创建 `.data` 时顺手保证 `<工作目录>/.gitignore` 含 `.data/`——工作目录很可能本身就是 git 仓库。文件不存在则创建，已存在则**只在缺失时追加一行，从不覆盖**原有忽略规则。

## 共用约定：上次打开的东西（`edit-prefs`）

- 记录固定写到 `<工作目录>/.data/<app>.json`，内容 `{ "entries": { "<key>": { "name", "path" } } }`。
- **路径编码**：目标在工作目录内 → 存相对路径（分隔符统一为 `/`，整个工作目录可以搬家）；在工作目录外 → 存绝对路径；指向工作目录本身 → 存 `.`。对外暴露的 `LastOpened.path` 始终是已解析的绝对 `PathBuf`，调用方不需要知道存储形式。`.` 解析时会还原成工作目录本身（`workdir.join(".")` 与原路径在 `PathBuf` 上并不相等，这是个已用单测钉住的坑）。
- 启动优先级统一为：**记录（且路径仍存在）→ 原生对话框（`rfd`）→ 空状态**。命令行不再直接指定被打开的资源（工作目录已经占了唯一的启动参数）。用户在对话框里取消，就停在空状态，工具不会自己挑一个文件。
- 只有**成功**打开的资源才写记录；打开失败只报错、不覆盖记录，下一次启动还能重试同一路径。
- 记录读写失败、坏 JSON、`.data` 建不出来都退化为空记录并打日志——工具本身绝不能因为记录而启动失败。
- 内容未变时不落盘（`Prefs::set` 内部短路），切换选中项导致的写盘不会成为高频 IO。
- 旧版本写在 `%APPDATA%\geese\` 的记录**不迁移也不读取**：新工作目录从空记录开始。迁移规则需要长期维护「哪些工作目录已迁移过」，而丢的只是一次「上次打开」的便利。

## 工具契约

### `animation-preview`

用法 `animation-preview --workdir <工作目录>`，记录键 `model`，记录的是模型文件名 + 路径（工作目录内为相对路径）。模型**只**来自记录或文件对话框，对话框初始目录为工作目录。

- **加载走引擎真实路径**：`scene::import_scene(path, 8, 6)` 一次拿到网格 / 蒙皮 / `MaterialLibrary` / 节点层级 / `AnimationClip`，因此 `scene` 的默认 features（`physics` + `navmesh`）也被拖进来——`manifest.rs` 用了 `physics::` 而没有任何 `cfg` 门禁，关掉 feature 无法编译。这是接受的编译成本，不是遗漏。
- **每帧**：`Scene::update_animation(&mut player, dt)`；播放时 `dt` 为真实帧间隔，暂停/拖动时间轴时传 `0.0` 原地重采样。`avatar::sample_clip` 每次调用都会先把所有节点重置回 `base_transform` 再采样，所以 `dt = 0` 是定位到任意时间的合法手段，不需要第二套采样代码。
- **渲染**：`Scene::render_queue(&scene_renderer, None)` —— `None` 表示不做视锥/octree 剔除，被动画驱动的对象一律参与（`update_animation` 不重建 octree，动画物体也只走 `dynamic_indices`）。
- UI：左栏剪辑列表（点选即切剪辑并从头播）、底部 ComboBox + 播放/暂停/重置 + 可拖动时间轴（拖动期间暂停、松手恢复原播放状态）、底部状态栏显示文件名/路径/关节数/剪辑数/工作目录。

### `material-editor`

用法 `material-editor --workdir <工作目录>`，记录键 `project`，记录的是上次选中的材质文件名（路径恒为工作目录，存为 `.`）。

- **工作目录就是项目目录**：材质从 `<工作目录>/assets/materials/` 读写，构造时直接 `MaterialStore::open`（其内部 `ensure_default_material_files` 补齐内置材质）。
- 项目由启动参数唯一决定、运行期间不可切换，所以没有「选择项目/更换项目」入口；打不开时左栏显示错误文本。
- 启动时用记录里的材质文件名定位选中项，不存在则落到第 0 个；切换选中材质即写回记录。

### `scene`

用法 `scene --workdir <工作目录>`（包 `edit-scene`），记录键 `scene`，记录的是上次编辑的场景文件。三栏：左可选模型、中场景预览、右场景 JSON。

- **输入**：左栏递归扫 `<工作目录>/assets/models` 的 `.gltf`/`.glb`（跳过隐藏目录与符号链接、深度上限 8，按子目录分组）；场景文件从记录/本工作目录下的 `assets/scenes` 选，`新建` 从 `SceneManifest::empty` 开一个无路径的文档。
- **预览**：`scene::load_scene_from_manifest(..., database = None)` → `Scene::render_queue(&renderer, None)`，与运行时的加载路径一致；环境光/方向光取自 manifest 的 `environment`，空光照时给一盏默认方向光。相机是轨道相机，取景用**各对象 aabb 的并集**（`Scene::bounds` 在空场景/微小场景下是 ± 100 默认盒或含模型导入的畸形边界，会给出离谱距离），距离下限 2.5。
- **拖入模型**：从左侧按住拖到中间，落点 = 光标射线与 `y = 0` 平面的交点（接近水平时截断在 1000 单位）；松手往 `models` 末尾追加 `{id, path, transform.translation}`——`id` 由文件名净化后去重，`path` 相对场景文件目录（无法相对化时写绝对路径），`translation` 保留两位小数。写入方式是「解析 → 改值 → 2 空格 pretty → 整段替换」，因此光标位置会丢；当前文本解析失败时**拒绝拖入**并提示先修 JSON。落点写完后相机焦点挪到该点。
- **改 JSON**：右栏是可编辑全文（等宽 + 行号 + 「格式化」）。停止输入 400ms 后解析并重建；解析或加载失败时画面停在上一版有效场景，错误写进右栏底部与状态栏，不弹窗。同一份文本只尝试一次（失败后不会每帧重试）。
- **保存**：`保存` 按钮 / `Ctrl+S`；此外每次**成功**应用后也会自动写回当前文件（内容未变不写）；还没路径的场景（新建）在第一次成功应用时弹一次「另存为」，用户取消后不再纠缠、继续只在内存里编辑。打开/保存成功才写记录。
- **重建前必须调 `WgpuSceneRenderer::clear_resource_cache()`**（见 `render-gpu-cache`）：旧网格释放后新顶点数组可能复用同一地址，命中旧指针键会画出上一套几何。
- **egui-wgpu 0.29 的坑**：`Renderer::render` 只读 `update_buffers` 填好的顶点/索引切片，漏调就 `unwrap` 空切片 panic。本工具已在渲染前调用；其它工具出现同样 panic 同理处理。
- **中文字体**：egui 内置字体不含 CJK，启动时按候选列表加载系统字体（`msyh.ttc` / `simhei.ttf` / `PingFang.ttc` / Noto CJK），都没找到就只打日志、退化为默认字体。

`script` 字段（物件的 Python behavior）在这里只被原样保留、**不执行**——运行的脚本见 `game-runtime-behaviors`。

已知限制：每次应用都**整场景重建**（重新导入所有 glTF），大场景下每次停顿都会顿一下——没有增量更新，也没有导入缓存；`prefab_instances` 不实例化、原样保留，状态栏提示「N 个 prefab 实例未预览」；`objects` 里程序化物体的 `scale` 引擎加载器不作用到网格（只参与 bounds 计算），预览里看到的是 1 倍尺寸，这是 loader 现状而非本工具的取舍。

### `animation-preview` 的子网格替换
左栏「子网格」段可以把当前模型的某个对象换成另一个网格（来自同一个文件或另一个 glTF/GLB），动画照旧播放——手工试一次部件换装。实现分三层：`swap.rs`（纯算法）→ `model.rs`（源文件/记录/保留网格）→ `app.rs`（UI）。

**入口**：点列表项选目标（默认第 0 个）→ 「换源文件…」→ 二级窗口「源网格」里点一项即替换。已替换的目标标 `[已替换]` 并显示来源，「恢复选中」/「全部恢复」换回原网格。

**蒙皮规则**（节点没有名字，跨文件只能按关节下标对应）：

| 目标 | 源 | 行为 |
| --- | --- | --- |
| 有 skin | 有 skin，关节数相同 | 新 `Skin` = 目标的 `joints` + 源的 `inverse_bind_matrices` |
| 有 skin | 有 skin，关节数不同 | 拒绝，状态栏红字报两边关节数 |
| 有 skin | 无 skin | 刚性换上（`skin = None`、`has_skin = false`），跟着目标节点变换 |
| 无 skin | 无 skin | 纯几何替换 |
| 无 skin | 有 skin | 拒绝（没有可对应的骨架） |

拒绝路径不留下任何改动（已用单测钉住：skin 数量与句柄不变）。若两边关节**顺序**不同，画面会绑错骨头——这是无法用下标校验的前提，而不是可以容忍的巧合。

**材质**：源文件加载时 `MaterialLibrary::append_library` 整库搬一次拿到偏移（纹理句柄由它重映射），替换时源网格的材质句柄只加偏移；同一路径重复载入复用已有容器。

**被换下来的网格必须继续存活**：`render` 的 mesh 缓冲缓存键是「顶点指针 + 长度」且只插不淘汰（`crates/render/src/common.rs` 的 `compute_mesh_key` / `forward_plus.rs` 的 `cache.mesh_buffers`）。若旧网格被释放后新分配的顶点数组复用了同一地址，就会命中旧键渲染出上一套几何。因此 `swap::apply`/`swap::restore` 都把换下来的网格**返回给调用方**，由 `LoadedModel::retired_meshes` 持有、永不释放；代价只是一点内存。这是本模块最容易被后人「优化掉」的一处。

**网格名字**：`crates/scene` 导入时把 glTF 的 node/mesh 名字记进 `SceneNode.name` / `SceneObject.mesh_name`（新增字段）。它们**只用于显示**，缺失时退化为 `节点 #id` / `网格 #index`，不参与任何查找。

## 已知限制

- `render::MAX_JOINTS = 32`：单个蒙皮超过 32 个关节时，多出的关节被静默丢弃、网格扭曲。`animation-preview` 因此在状态栏常驻 `关节 N / 32` 并对超限标黄——让人能区分「模型/引擎上限」与「工具坏了」。修上限属于渲染器层的另一个任务。
- `shadow_depth.wgsl` 不处理 skin，蒙皮网格投出的阴影是绑定姿势。预览工具的灯光不特意规避。
- 预览工具按目录独立构建，没有共享的构建脚本或 CI 目标；没有 glTF 测试资源，蒙皮效果靠本地实跑验证。

## 非目标

- 不做骨骼线框/关节可视化、地面网格与坐标轴。
- 不做速度倍率、循环开关、动画 marker 事件提示（原 `crates/editor/src/animation_panel.rs` 有过）。
- 不支持把文件拖进窗口加载（模型一律走「记录 → 对话框」）。
- 不做运行中切换工作目录，也不迁移旧 `%APPDATA%\geese\` 记录。
- 不做工作目录内的文件浏览器（选模型仍用系统原生对话框，只是以工作目录为初始目录）。
- 子网格替换不持久化（关闭即丢）、不写回资产文件、不支持按骨骼名重绑。
