# Geese 材质系统设计文档

> 状态：现状盘点 + 目标设计（v2 提案）
> 代码基线：当前工作区 `crates/render` / `crates/scene` / `crates/py_engine` / `crates/game_runtime`
> 日期：2026-11-13

---

## 0. 文档说明

### 0.1 目的

1. 把当前材质系统**实际实现**（而不是设想的实现）完整记录下来：数据结构、CPU→GPU 布局、两条渲染管线中的取样逻辑、材质来源链路。
2. 给出**问题清单**，每条都带代码位置与影响面，便于排期。
3. 给出**目标设计 v2** 与分阶段迁移方案，作为后续重构的依据。

### 0.2 范围

| 属于本文范围 | 不属于本文范围 |
| --- | --- |
| `Material` / `Texture` / `Sampler` / `MaterialLibrary` 数据模型 | 后处理链（`post_pipeline.rs`） |
| `MaterialUniform` 与 group(1) bind group 布局 | 粒子（`particle.rs`）、Sprite（`sprite.rs`）、水体（`water.rs`）、Decal（`decal.rs`）各自的私有 uniform |
| Forward+ / Deferred+ 中的材质取样与 alpha 处理 | IBL 烘焙、阴影、Cluster 剔除 |
| glTF / 程序化 / Prefab / Python 四条材质写入路径 | 编辑器 UI 实现细节 |
| 材质资产化（`.material.json`）与热重载（未实现，见 §9.7） | 网络同步、服务器侧材质 |

### 0.3 验证基线

本次盘点在 `crates/render` 上执行 `cargo test`：**113 passed / 0 failed**（另有 2 个 ignored），说明下文所述的缺陷不是编译或测试失败，而是**逻辑/契约层面的问题**——现有测试没有覆盖到它们（见 §8 M-20）。后续落地（默认材质 + 材质 crate 拆分）后的基线：`crates/material` **24 passed**、`crates/render` **110 passed**、`crates/scene` **59 passed**，`game_runtime` / `py_engine` `cargo check` 通过。

### 0.4 结论摘要（TL;DR）

1. 材质系统已抽成**独立 crate `crates/material`**（不依赖 wgpu）：数据模型 + 句柄 + 材质库 + `.material.json` + 内置默认材质，供模型导入、骨骼动画、场景与渲染层共用。`render` 只保留 GPU 侧（`MaterialUniform`、bind group、纹理上传缓存），并通过 `pub use` 保持旧路径兼容。功能完整度仍约等于"能把 glTF 正确显示出来"的下限。
2. 材质数据到 GPU 的转换是**固定 64 字节 uniform + 固定 7 项 bind group**，纹理位通过 `flags.x` 位掩码显式告知 shader。这个设计本身没问题，但**位掩码与 bind group 内容由两处手写代码分别维护**，是后续最容易出错的地方。
3. 存在 **2 个会导致画面错误的高危缺陷**：多模型/Prefab 合并时 `TextureHandle` 未做偏移（§8 M-01），以及 `game_runtime` 主运行时从未把场景材质库接入渲染（§8 M-02）。
4. 存在 **一批"字段存在但未生效"** 的配置：`double_sided`、`custom_shader`、`FilterMode` 的 mipmap 变体。它们给使用者错误的预期。（`normal_scale` / `occlusion_strength` 已在 v1 接通。）
5. 缺少四类必备能力：**颜色空间（sRGB）处理**、**mipmap**、**透明排序**、**材质去重/批处理**。这四项决定了能否从"demo 可用"走到"项目可用"。
6. 新增一个材质参数当前要同步改 **9 处**，其中 5 处是镜像/重复/样板；用"单一声明源 + 共享 WGSL 片段 + 反射式 setter"可收敛到 **2 处**（数据 + 着色使用），已用可编译原型验证（附录 B）。

---

## 1. 系统定位与边界

### 1.1 在工程中的位置

```
crates/material/                     ← 材质抽象层（无 GPU 依赖）
  src/model.rs        ← 数据模型：Material / Texture / Sampler / 句柄 / MaterialLibrary
  src/asset.rs        ← .material.json 解析、内置默认材质、增量补丁
  src/shader_graph.rs ← 自定义着色图（节点 + WGSL 生成）
  assets/default_materials/            ← 引擎默认材质源文件（编译期嵌入）

crates/render/                       ← 只负责"怎么上 GPU"
  src/material.rs     ← 兼容层：pub use material::*（保留 render::Material 路径）
  src/shader_graph.rs ← 兼容层：pub use material::shader_graph::*
  src/common.rs       ← MaterialUniform / DefaultTextures / 上传与缓存
  src/forward_plus.rs ← group(1) 布局 + bind group 构建（两条管线共用 build_command）
  src/deferred_plus.rs← 复用 forward_plus::build_command
  shaders/pbr_common.wgsl      ← MaterialUniform 的 WGSL 镜像 + BRDF
  shaders/forward_plus.wgsl    ← 采样 + 直接光照
  shaders/deferred_geometry.wgsl ← 采样 + G-Buffer 打包
  shaders/deferred_lighting.wgsl ← G-Buffer 解码 + 光照
  src/shader_library.rs        ← 上述 WGSL 的"程序化生成"版本（feature 开关）

crates/scene/
  src/gltf_material.rs ← glTF → material::Material 的转换
  src/loader.rs        ← 场景清单/多模型合并、程序化对象材质
  src/prefab_loader.rs ← Prefab 实例化时的材质合并
  src/lib.rs           ← 再导出 material::{Material, MaterialLibrary, material_asset}

crates/py_engine/   ← Python 侧材质注册表与创建接口
crates/game_runtime/← 两个运行时入口（纯 Rust / Python 脚本）
```

依赖方向：`material` ← `render`、`material` ← `scene` ← `game_runtime`/`py_engine`。
`material` 不依赖 `wgpu`/`render`/`scene`，`avatar`（骨骼/蒙皮/动画）仍保持纯数学、不依赖材质，动画模型通过 `scene` 的 `ModelMesh.material` 关联材质。

### 1.2 上下游数据流

```
                 ┌──────────────────────────┐
   .gltf/.glb ──▶│ scene::material          │──┐
                 │  load_material_library   │  │
                 └──────────────────────────┘  │
                 ┌──────────────────────────┐  │      ┌───────────────────┐
  .scene.json ──▶│ scene::loader            │──┼─────▶│ MaterialLibrary   │
  (程序化对象)    │  create_pbr_material     │  │      │  materials: Vec   │
                 └──────────────────────────┘  │      │  textures:  Vec   │
                 ┌──────────────────────────┐  │      └─────────┬─────────┘
  Prefab ───────▶│ scene::prefab_loader     │──┘                │ &MaterialLibrary
                 └──────────────────────────┘                   │
                 ┌──────────────────────────┐                   ▼
  Python ───────▶│ py_engine MATERIAL_REGISTRY │──(拷贝)──▶ MaterialLibrary
                 └──────────────────────────┘                   │
                                                                ▼
                                          SceneRenderer::build_queue → RenderCommand{ material: &Material }
                                                                │
                                                                ▼
                                     ForwardPlusPipeline::prepare / DeferredPlusPipeline::prepare
                                                                │ build_command()
                                        ┌───────────────────────┴────────────────────────┐
                                        ▼                                                ▼
                              GpuResourceCache.textures                    MaterialUniform (64B)
                              (wgpu::Texture + sampler, 按 handle 索引)     + 5 tex view + 1 sampler
                                        └───────────────────────┬────────────────────────┘
                                                                ▼
                                                    group(1) material bind group
```

---

## 2. 数据结构

### 2.1 句柄

```rust
// crates/material/src/model.rs:5-9
pub struct MaterialHandle(pub usize);
pub struct TextureHandle(pub usize);
```

- 裸 `usize` 索引，**无代（generation）、无类型内嵌校验**。`MaterialLibrary` 中的元素被删除/重排后，旧句柄会静默指向别的资源。
- 目前两条不变量靠约定维持：
  - `MaterialHandle` 由 `ModelMesh::material: Option<MaterialHandle>` 持有（[`mesh.rs`](crates/render/src/mesh.rs#L26-L36)）。
  - `TextureHandle` 在 glTF 导入路径上恰好等于 `document.textures()` 的下标（[`scene/material.rs`](crates/scene/src/material.rs#L12-L45)），在其它路径上没有任何生产者。

### 2.2 `Material`

[`model.rs`](crates/material/src/model.rs#L11-L31)，`Clone + Debug`，**没有 `Serialize/Deserialize`、没有 `PartialEq/Hash`**。

| 字段 | 类型 | 默认值 | 是否进入 GPU | 说明 |
| --- | --- | --- | --- | --- |
| `name` | `Option<String>` | `None` | 否 | 仅调试 |
| `base_color_factor` | `[f32; 4]` | `[1,1,1,1]` | 是 | 线性空间，RGBA |
| `base_color_texture` | `Option<TextureHandle>` | `None` | 是（binding 1） | sRGB 贴图 |
| `metallic_factor` | `f32` | `1.0` | 是 | glTF 默认值即 1.0 |
| `roughness_factor` | `f32` | `1.0` | 是 | |
| `metallic_roughness_texture` | `Option<TextureHandle>` | `None` | 是（binding 2） | G=roughness，B=metallic |
| `normal_texture` | `Option<TextureHandle>` | `None` | 是（binding 3） | **仅在 mesh 有 tangent+uv0 时生效** |
| `normal_scale` | `f32` | `1.0` | 是（MRNO.z） | glTF `normalTexture.scale`，v1 已接入 |
| `occlusion_texture` | `Option<TextureHandle>` | `None` | 是（binding 4） | 取 R 通道 |
| `occlusion_strength` | `f32` | `1.0` | 是（MRNO.w） | glTF `occlusionTexture.strength`，v1 已接入 |
| `emissive_factor` | `[f32; 3]` | `[0,0,0]` | 是 | |
| `emissive_texture` | `Option<TextureHandle>` | `None` | 是（binding 5） | sRGB 贴图 |
| `alpha_mode` | `AlphaMode` | `Opaque` | 是（flags.y） | |
| `alpha_cutoff` | `f32` | `0.5` | 是 | |
| `double_sided` | `bool` | `false` | **否** | 死字段，见 M-08 |
| `custom_shader` | `Option<Arc<ShaderGraph>>` | `None` | **否** | 死字段，见 M-09 |

### 2.3 `Texture` / `Sampler` / 枚举

[`model.rs`](crates/material/src/model.rs#L86-L144)：

```rust
pub struct Texture {
    pub name: Option<String>,
    pub width: u32, pub height: u32,
    pub format: TextureFormat,
    pub pixels: Vec<u8>,      // CPU 像素常驻内存
    pub sampler: Sampler,     // 采样器内嵌在纹理里
}
```

- `TextureFormat` 只有 10 个变体（R8…R32G32B32A32Float 的若干子集），**没有 sRGB 变体**，也没有压缩格式。上传实现 `to_rgba8` 只支持 4 种 8bit 格式，其余打印 error 并返回空 `Vec`（[`common.rs`](crates/render/src/common.rs#L460-L483)）。
- `Sampler` 内嵌在 `Texture` 中，意味着同一张图无法用两种采样方式（例如 UI 用 Clamp、地表用 Repeat），也意味着 sampler 无法跨纹理去重。
- `FilterMode` 有 mipmap 变体，但 `mip_level_count` 恒为 1，mipmap 相关语义全部无效（M-12）。

### 2.4 `MaterialLibrary`

[`model.rs`](crates/material/src/model.rs#L146-L205)：

```rust
#[derive(Default)]
pub struct MaterialLibrary {
    pub materials: Vec<Material>,
    pub textures: Vec<Texture>,
}
impl MaterialLibrary {
    pub fn material(&self, h: MaterialHandle) -> Option<&Material> { self.materials.get(h.0) }
    pub fn texture(&self, h: TextureHandle) -> Option<&Texture> { self.textures.get(h.0) }
}
```

- 只读查询，无插入/删除/去重/校验 API；所有写入点都直接 `push`。
- **多模型合并时没有句柄重映射机制**（这正是 M-01 的根因）。
- `Default` 即空库；`SceneRenderer` 在查不到材质时回退到 `self.default_material` 并累加 `stats.missing_materials`（[`scene.rs`](crates/render/src/scene.rs#L77-L85)）。

---

## 3. CPU → GPU 映射

### 3.1 `MaterialUniform`（64 字节）

[`common.rs`](crates/render/src/common.rs#L116-L177) 与 [`pbr_common.wgsl`](crates/render/shaders/pbr_common.wgsl#L47-L52) 必须逐字段一致：

| 偏移 | Rust 字段 | WGSL | 语义 |
| --- | --- | --- | --- |
| 0 | `base_color_factor: [f32;4]` | `vec4<f32>` | rgb=基色，a=alpha |
| 16 | `emissive_alpha_cutoff: [f32;4]` | `vec4<f32>` | rgb=自发光，a=alpha cutoff |
| 32 | `metallic_roughness_normal_occlusion: [f32;4]` | `vec4<f32>` | x=metallic，y=roughness，z=normal_scale，w=occlusion_strength |
| 48 | `flags: [u32;4]` | `vec4<u32>` | x=纹理位掩码，y=alpha_mode，z/w 保留 |

- `size_of::<MaterialUniform>() == 64` 由单测锁定（[`common.rs`](crates/render/src/common.rs#L602-L605)），且 `#[repr(C)]`。
- **z（normal_scale）与 w（occlusion_strength）曾硬编码为 1.0**；现已改为读取 `Material::normal_scale` / `Material::occlusion_strength`（glTF 导入时取 `normalTexture.scale` / `occlusionTexture.strength`，`.material.json` 亦可配置）。
- `flags` 用 `u32` 数组而非 `[u32;4]` 的语义化结构，可读性靠注释维持。

### 3.2 纹理位掩码

[`common.rs`](crates/render/src/common.rs#L129-L133)：

| 常量 | 位 | 对应 binding |
| --- | --- | --- |
| `TEX_BIT_BASE_COLOR` | `1 << 0` | 1 |
| `TEX_BIT_METALLIC_ROUGHNESS` | `1 << 1` | 2 |
| `TEX_BIT_NORMAL` | `1 << 2` | 3 |
| `TEX_BIT_OCCLUSION` | `1 << 3` | 4 |
| `TEX_BIT_EMISSIVE` | `1 << 4` | 5 |

WGSL 侧读取：`material_has_texture(mat, bit)` → `(mat.flags.x & (1u << bit)) != 0u`（[`pbr_common.wgsl`](crates/render/shaders/pbr_common.wgsl#L54-L56)）。

**规范要求**：位必须与"实际绑定了非缺省贴图"严格同步。当前由 `from_material()` 与 `build_command()` 两处分别计算：

- `from_material()` 依据 `Option::is_some()` 置位，`normal` 额外要求 `has_tangent_uv`；
- `build_command()` 依据同样的条件挑选 view（[`forward_plus.rs`](crates/render/src/forward_plus.rs#L856-L909)）。

两处逻辑重复且没有共享函数，改一处忘另一处就会采样到缺省贴图或反之（M-06）。

### 3.3 group(1) bind group 布局

[`forward_plus.rs`](crates/render/src/forward_plus.rs#L782-L807)，Deferred+ 复用同一函数：

| binding | 类型 | 可见性 | 内容 |
| --- | --- | --- | --- |
| 0 | uniform buffer | FRAGMENT | `MaterialUniform`（64B） |
| 1 | texture_2d | FRAGMENT | base color |
| 2 | texture_2d | FRAGMENT | metallic-roughness |
| 3 | texture_2d | FRAGMENT | normal |
| 4 | texture_2d | FRAGMENT | occlusion |
| 5 | texture_2d | FRAGMENT | emissive |
| 6 | sampler (filtering) | FRAGMENT | **全槽位共用一个 sampler** |

注意：
- 缺槽位也必须绑定，用 `DefaultTextures` 占位图补齐（white / mr / normal / occlusion / black，[`common.rs`](crates/render/src/common.rs#L517-L574)）。
- 共用 sampler 取自 `base_color_texture` 的 sampler，若只有其他槽位有贴图则回退到缺省 sampler（[`forward_plus.rs`](crates/render/src/forward_plus.rs#L906-L909)）。因此 MR/normal 贴图自带的 wrap/filter 设置被忽略（M-15）。
- 该布局**不是材质共享的**：每个 entity 一套 `material_buffer + bind_group`（见 3.5）。

### 3.4 缺省贴图

| 名称 | 像素 | 用途 |
| --- | --- | --- |
| white | `[255,255,255,255]` | base color 占位（乘 factor 不改变结果） |
| mr | `[0,255,0,255]`（G=1,B=0） | roughness=1、metallic=0 |
| normal | `[128,128,255,255]` | 线性法线 (0,0,1) |
| occlusion | `[255,255,255,255]` | 无遮蔽 |
| black | `[0,0,0,255]` | emissive 占位 |

全部为 `Rgba8Unorm`、1×1、无 mipmap（[`common.rs`](crates/render/src/common.rs#L533-L547)）。

### 3.5 上传、缓存与每帧生命周期

```
build_command()                                (每条 RenderCommand / 每帧)
  ├─ mesh 缓存：按 (ptr,len,ptr,len) 命中则整块 write_buffer
  ├─ 纹理：handle 不在 cache.textures 时 upload_texture() + create_sampler()
  ├─ MaterialUniform::from_material()
  ├─ 首次见 entity：建 material/object buffer + 两个 bind group
  ├─ 再次见 entity：write_buffer 更新 uniform；
  │                  tex_handles 或 has_tangent_uv 变化时重建 bind group
  └─ 然后【无论如何】再 create_bind_group 一次，返回给本帧命令
```

对应代码：[`forward_plus.rs:811-1111`](crates/render/src/forward_plus.rs#L811-L1111)。其中：

- 纹理缓存键是 `handle.0`（`HashMap<usize, CachedTexture>`），无代、无淘汰、无引用计数（[`common.rs`](crates/render/src/common.rs#L315-L341)）。
- `CachedEntityResources` 记录 `tex_handles: [Option<usize>;5]` 与 `has_tangent_uv` 用于判断是否需要重建 bind group（[`common.rs`](crates/render/src/common.rs#L326-L334)）。
- 缓存以 **`entity_id: String`** 为键，每帧构造临时 `BindGroup`；同时存在"已缓存 bind group 但本帧仍重建一次"的重复路径（M-10）。
- 上传后 CPU 端 `pixels` 不释放，纹理内存常驻两份（M-19）。

---

## 4. 着色路径

### 4.1 Forward+

[`forward_plus.wgsl`](crates/render/shaders/forward_plus.wgsl)：

- 采样函数各自内联判断位掩码：`sample_base_color` / `sample_metallic_roughness` / `sample_occlusion` / `sample_emissive` / `compute_normal`。
- `compute_normal` 在切线退化时用 `cross(up, n)` 或 `cross(right, n)` 兜底，并乘 `tangent.w` 得到副切线（handedness），逻辑正确（[`forward_plus.wgsl`](crates/render/shaders/forward_plus.wgsl#L118-L139)）。
- 主函数流程（[`forward_plus.wgsl`](crates/render/shaders/forward_plus.wgsl#L141-L186)）：
  1. 采基色 → `alpha_mode == 1 (Mask) && a < cutoff` 则 `discard`；
  2. 采 MR，`roughness = clamp(r, 0.04, 1.0)`；
  3. `f0 = mix(0.04, base.rgb, metallic)`；
  4. Cluster 索引 → 位掩码 → 遍历光源累加 `shade_light`；
  5. 加 `ambient * base * occlusion` 与 emissive；
  6. 输出 alpha：`select(1.0, base.a, alpha_mode == 2)`。
- **pipeline 层固定 `blend: ALPHA_BLENDING` 且 `depth_write_enabled: true`**（[`forward_plus.rs`](crates/render/src/forward_plus.rs#L136-L140), [:162](crates/render/src/forward_plus.rs#L160-L166)），不区分 alpha mode；顶点 `cull_mode: None`（[:155](crates/render/src/forward_plus.rs#L151-L158)）。
- **instanced 变体整段复制了 forward 的 fragment**：[`forward_plus_instanced.wgsl:79-184`](crates/render/shaders/forward_plus_instanced.wgsl#L79-L184) 的 5 个采样函数 + `fs_main` 与 [`forward_plus.wgsl:82-186`](crates/render/shaders/forward_plus.wgsl#L82-L186) 逐字相同，只有 vertex 入口不同（M-21）。

### 4.2 Deferred+

- geometry pass **复制了**同样的采样函数（不是通过片段共享，逐字重复见 M-21），把结果打包进 3 张 `Rgba8`/`Rgba16` G-Buffer（[`deferred_geometry.wgsl`](crates/render/shaders/deferred_geometry.wgsl#L140-L157)）：
  - RT0 = `(base.rgb, metallic)`
  - RT1 = `(n*0.5+0.5, roughness)`
  - RT2 = `(emissive.rgb, occlusion)`
- **G-Buffer 不含 base color 的 alpha**，因此 `Blend` 材质在 Deferred+ 下退化为不透明，只有 `Mask` 的 `discard` 生效（M-07）。
- lighting pass 解码并复用同一 `shade_light`（[`deferred_lighting.wgsl`](crates/render/shaders/deferred_lighting.wgsl#L51-L60)）。
- Deferred+ 目前不支持阴影（[`deferred_plus.rs:626-633`](crates/render/src/deferred_plus.rs#L626-L633) 有一次警告）。

### 4.3 共享 BRDF

两条路径共用 [`pbr_common.wgsl`](crates/render/shaders/pbr_common.wgsl) 的 GGX / Smith / Schlick 与 `shade_light`（Cook-Torrance + Lambertian）。这是当前 Shader 组织里最干净的部分：**材质作为 uniform 输入的约定稳定，着色公式单点维护**。

### 4.4 Shader 生成的双轨制

- 默认：`include_str!` 拼接手写 WGSL。
- `feature = "use-shader-framework"`：由 [`shader_library.rs`](crates/render/src/shader_library.rs#L234-L269) 程序化生成同一套结构体与函数。
- 两条路径靠**测试**保证一致性（`MaterialUniform` 字段名/类型断言，[`shader_library.rs`](crates/render/src/shader_library.rs#L577-L900)）。
- 风险：新增材质字段必须同时改手写 WGSL、`shader_library.rs`、`MaterialUniform`、`from_material` 四处（见附录 B）。

---

## 5. 材质来源链路（写入路径）

### 5.1 glTF 导入（唯一完整路径）

[`scene/material.rs`](crates/scene/src/material.rs#L6-L69)：

- 遍历 `document.textures()` 生成 `Texture`，`TextureHandle(index)` 直接取自 `document.textures()` 下标。
- 遍历 `document.materials()` 映射 PBR 标量与 5 个贴图槽位。
- 材质为空时塞一个 `Material::default()`；**纹理为空时不做任何补齐**。
- 未读取：`normalTexture.scale`、`occlusionTexture.strength`、`KHR_texture_transform`、`KHR_materials_emissive_strength`、`COLOR_0` 顶点色、`unlit` 扩展、`TEXCOORD_1`。

### 5.2 场景清单 / 程序化对象

[`loader.rs`](crates/scene/src/loader.rs#L172-L199) 对 `plane/cube/sphere/cylinder` 各 `push` 一个 `create_pbr_material(name, color)`（metallic=0, roughness=0.8，无贴图）。

### 5.3 Prefab

[`prefab_loader.rs`](crates/scene/src/prefab_loader.rs#L229-L275) 与 §5.4 的合并逻辑同构。

### 5.4 多模型合并（关键缺陷所在）

[`loader.rs`](crates/scene/src/loader.rs#L74-L129)：

```rust
let material_offset = all_materials.materials.len();   // L81
...
if let Some(ref mut mat_h) = obj.mesh.material {       // L99-101
    mat_h.0 += material_offset;                        // 材质句柄做了偏移
}
...
all_materials.materials.append(&mut scene.materials.materials);  // L125
all_materials.textures.append(&mut scene.materials.textures);    // L126 ← 纹理句柄未偏移
```

材质里的 `base_color_texture` 等 `TextureHandle` **没有加上 `texture_offset`**，所以第二个模型开始，所有贴图索引都指回第一个模型的纹理（M-01）。

### 5.5 Python

[`py_engine`](crates/py_engine/src/lib.rs#L50-L66) 维护一个全局 `MATERIAL_REGISTRY: Mutex<Vec<Material>>`：

- `material_create(name, r,g,b, metallic, roughness)`：同时写入注册表与场景材质库，返回索引（[:569-598](crates/py_engine/src/lib.rs#L569-L598)）。
- `material_set_emissive(idx, r,g,b)`：直接改场景库中的 `emissive_factor`（[:364-370](crates/py_engine/src/lib.rs#L364-L370)）。
- `get_material_library()` 返回的库 `textures: vec![]` 恒为空 → **Python 侧无法使用任何贴图**。
- `python_runtime` 用它初始化 `Scene`（[`python_runtime.rs:284`](crates/game_runtime/src/python_runtime.rs#L278-L291)）。

### 5.6 纯 Rust 运行时（v1 已修复）

原实现（[`game_runtime/src/lib.rs:233`](crates/game_runtime/src/lib.rs#L217-L235)）建了一个空库：

```rust
let materials = MaterialLibrary::default();   // 空库
```

场景带材质句柄、库却为空 → `build_queue` 全部回退到默认材质（M-02）。

v1 改为"**场景库为主体 + 内置兜底 + JSON 增量补丁**"，`prepare` 使用 `scene.materials`：

```rust
let mut scene = scene::loader::load_scene_from_file(...)?;          // 材质来自 glTF
let appended = material::asset::append_missing_builtins(&mut scene.materials)?;
let assets = material::asset::load_project_material_assets(project_dir)?;
let report = material::asset::apply_material_assets(&mut scene.materials, &assets);
let scene_renderer = SceneRenderer::new(material::asset::default_material(&scene.materials));
```

- `append_missing_builtins` 只补齐**不存在**的同名内置材质，不覆盖 glTF 材质；
- `apply_material_assets` 按名字打补丁，只改 JSON 里声明了的变量；
- 句柄只追加、不改序，场景里已有的材质句柄始终有效。

### 5.7 引擎默认材质与 `.material.json` 补丁（v1 新增）

**优先级（后者只补前者没有的东西）**：

```
模型 glTF 材质参数  ──▶  .material.json 声明变量覆盖  ──▶  引擎内置默认材质（仅当没有同名材质）
```

即：材质参数**继承自模型的 glTF 文件**，`.material.json` 是**补充修改**——写几个变量就只改几个，没写的继续继承。

引擎自带 5 个兜底材质，编译期嵌入（[`crates/material/assets/default_materials/`](crates/material/assets/default_materials/default.material.json)）：

| 材质名 | 用途 | 关键参数 |
| --- | --- | --- |
| `default` | 缺材质网格的回退材质 | 中性灰 0.5，metallic 0，roughness 0.5 |
| `default_white` | 纯白参考 | albedo 1.0，roughness 0.8 |
| `default_black` | 纯黑参考 | albedo 0.02，roughness 0.9 |
| `default_glass` | 半透明示例 | alpha `blend`，roughness 0.05，double_sided |
| `default_emissive` | 自发光示例 | emissive `[1.0, 0.45, 0.1]` |

流程（[`asset.rs`](crates/material/src/asset.rs)）：

1. **首次运行**：`<project>/assets/materials/` 不存在 → 写入 5 个内置材质文件（只写一次，之后尊重用户的删除/修改）。
2. **每次启动**：读取目录下所有 `*.material.json`，按文件名排序 → 顺序稳定。
3. **应用**：同名材质存在（glTF 或内置）→ 只覆盖声明的变量；不存在 → 作为新材质追加（基值 `Material::default()`）。
4. 网格没有材质 → 回退到名为 `default` 的材质（由 JSON 控制，不再是 Rust 里的 `Material::default()`）。

例：模型 `hero.glb` 里材质名是 `hero_body`（glTF 给了 baseColor、metallic=0.9、roughness=0.2），项目里只写：

```json
{ "version": 1, "name": "hero_body", "roughness": 0.75 }
```

结果：`base_color` / `metallic` 继续继承 glTF，只有 `roughness` 变成 0.75。

---

## 6. 关键不变量（改动前必读）

| # | 不变量 | 保证者 | 破坏后果 |
| --- | --- | --- | --- |
| I1 | `MaterialUniform` 64B、`#[repr(C)]`、字段顺序 = WGSL 顺序 | 单测 `material_uniform_is_64_bytes` | GPU 读到错位参数（颜色/粗糙度错乱） |
| I2 | `TEX_BIT_*` ↔ binding 1..5 一一对应 | 两处手写逻辑 | 采样到缺省图或错槽位 |
| I3 | `normal` 位仅在 `mesh.flags.has_tangents && has_uv0` 时置位 | `from_material` + `build_command` | 无切线时法线贴图产生错误光照 |
| I4 | 5 个槽位必须始终绑定（缺省图补齐） | `build_command` 的 `get_tex_view!` | wgpu 校验失败 / 采样越界 |
| I5 | 合并材质库时所有句柄必须重映射 | **当前无人保证** | 跨模型贴图串色（M-01） |
| I6 | `MaterialLibrary` 索引一经发布不可变更 | **当前无人保证** | 旧句柄悬空/错指 |
| I7 | 纹理像素格式必须与 `TextureFormat` 声明一致 | `to_rgba8` | 上传失败或花屏（M-05） |
| I8 | 一个 `entity_id` 只对应一个材质 | `build_command` 缓存键 | 同 ID 不同材质互相覆盖 |

---

## 7. 当前能力矩阵

| 能力 | 状态 | 备注 |
| --- | --- | --- |
| glTF metallic-roughness PBR | ✅ | 标量 + 5 槽位贴图 |
| Alpha Mask（discard） | ✅ | 两条管线均支持 |
| Alpha Blend | ⚠️ | Forward+ 有混合但无排序/无 depth-write 切换；Deferred+ 不支持 |
| Double sided | ❌ | 字段存在但未生效；默认全双面 |
| 法线贴图 | ✅（有条件） | 需 tangent+uv0，scale 固定 1 |
| AO | ✅ | strength 固定 1 |
| Emissive | ⚠️ | 无 emissive strength，>1 无法表达（仅靠后处理 bloom 放大） |
| 自定义着色 | ❌ | `custom_shader` 死字段 |
| sRGB 正确性 | ❌ | 所有贴图按 Unorm 上传 |
| Mipmap | ❌ | 恒 1 级 |
| 材质资产文件 | ⚠️ | v1 已实现：`<project>/assets/materials/<name>.material.json`，标量参数可改（§5.7 / §9.7），贴图引用待纹理管线 |
| 热重载 | ❌ | 无 |
| 材质去重 / 实例 | ❌ | 每 entity 一套 GPU 资源 |
| 材质变体（shader variant） | ❌ | 单 pipeline，无变体缓存 |
| 透明排序 | ❌ | 无排序 |
| 材质动画 | ❌ | 无 |
| 每材质 UV 变换 | ❌ | 无 `KHR_texture_transform` |

---

## 8. 问题清单

> 优先级：**P0** 立即修复（画面明显错误/必崩）；**P1** 项目可用性阻塞；**P2** 架构债；**P3** 优化/打磨。

### M-01 [P0] 多模型/Prefab 合并时 `TextureHandle` 未偏移

- **位置**：[`loader.rs:81`](crates/scene/src/loader.rs#L74-L129)（材质偏移）对比 [:125-126](crates/scene/src/loader.rs#L123-L128)（纹理直接 append）；[`prefab_loader.rs:274-275`](crates/scene/src/prefab_loader.rs#L268-L277) 同样问题。
- **现象**：场景引入 ≥2 个 glTF 模型且都有贴图时，第二个模型的基色/法线/AO 全部取到第一个模型的贴图。单模型场景不会暴露，容易漏测。
- **修复**：合并时计算 `texture_offset` 并遍历 `Material` 的 5 个 `Option<TextureHandle>` 做加法；或改为全局 `TextureId` 分配器（推荐，见 §9.2）。
- **进展（部分修复）**：`MaterialLibrary::append_library` 已实现"追加即重映射纹理句柄"（用于运行时补默认材质）；但 `loader.rs` / `prefab_loader.rs` 里手写的 `materials.append` / `textures.append` 仍未改，多模型场景仍会串色。
- **测试**：两个各带 1 张贴图的 glTF → 断言两组 `TextureHandle` 不相等且都能取到 `Some(texture)`。

### M-02 [P0] `game_runtime` 未接入场景材质库

- **位置**：[`game_runtime/src/lib.rs:233`](crates/game_runtime/src/lib.rs#L217-L235) + [:352](crates/game_runtime/src/lib.rs#L342-L352)。
- **现象**：纯 Rust 运行时（非 Python 路径）里所有物体都渲染成 `Material::default()`（白、metallic=1、roughness=1），glTF 材质完全失效；`RenderStats::missing_materials` 会等于可见对象数。
- **修复**：`GameState::materials` 直接取自 `scene.materials`（或删除该字段，改由 `self.scene.materials` 传入 `prepare`），并补充断言/日志：`missing_materials > 0` 时 `log::warn!`。
- **进展（已修复）**：删除空的 `GameState.materials` 字段，`prepare` 改用 `&self.scene.materials`；启动时按需补齐内置默认材质、再把 `.material.json` 补丁应用到场景库（见 §5.6/§5.7）。
- **测试**：加载带材质的场景后 `renderer.prepare` 前断言 `materials.materials.len() > 0`。

### M-03 [P1] 缺少 sRGB 颜色空间处理

- **位置**：`to_rgba8`/`create_rgba_texture` 一律 `Rgba8Unorm`（[`common.rs:414-483`](crates/render/src/common.rs#L414-L483)）；surface 选择 sRGB 格式（[`game_runtime/src/lib.rs:162-167`](crates/game_runtime/src/lib.rs#L161-L167)）。
- **现象**：baseColor / emissive 贴图是 sRGB 编码的，被当作线性值参与光照与后续硬件 sRGB 编码 → 整体偏暗、饱和度错误。
- **修复**：区分槽位语义：base color / emissive 用 `Rgba8UnormSrgb`，MR / normal / occlusion 用 `Rgba8Unorm`；缺省白/黑贴图需按用途各建一份 sRGB 版本（或统一改为线性计算 + 显式 `pow(2.2)` 解码，但硬件路径更省）。
- **测试**：0.5 灰 sRGB 贴图 × base_factor=1，读回像素应落在 128±1（而非 ~55）。

### M-04 [P1] 纹理上传只支持 8bit 非浮点格式

- **位置**：[`common.rs:460-483`](crates/render/src/common.rs#L460-L483)。
- **现象**：glTF 里 16bit PNG（R16*）或浮点 HDR 贴图会走 `_ =>` 分支：打印 error 后返回空 `Vec`，随后 `create_texture_with_data` 的字节数与尺寸不匹配 → wgpu 校验失败/panic（取决于后端）。
- **修复**：上传前做一次格式收口：`to_rgba8` 返回 `Result`，不支持时回退到缺省贴图并 `log::warn!`（不要静默产生空缓冲）；中期引入 `image` crate 做统一解码，`Texture` 只存未压缩源 + 目标 GPU 格式。
- **测试**：构造 `TextureFormat::R16` 纹理，断言上传返回 `Err`/回退，且不 panic。

### M-05 [P1] `double_sided` 未生效 / 全局关剔除

- **位置**：字段 [`model.rs:29`](crates/material/src/model.rs#L29)；pipeline `cull_mode: None`（[`forward_plus.rs:155`](crates/render/src/forward_plus.rs#L151-L158)）。
- **现象**：所有物体双面渲染，看不到背面剔除带来的性能收益，也掩盖了法线/绕序错误的资产问题。
- **修复**：按 `double_sided` 生成两套 pipeline（`CullMode::Back` / `None`），并在 `build_command` 里选择；`front_face` 需与导入时的绕序约定对齐（glTF 为 CCW）。
- **测试**：单面立方体 + `double_sided=false`，从内部看应不可见。

### M-06 [P1] 纹理位掩码与 bind group 内容双点维护

- **位置**：`MaterialUniform::from_material`（[`common.rs:135-177`](crates/render/src/common.rs#L135-L177)）与 `build_command` 的 `tex_handles` 选择（[`forward_plus.rs:856-909`](crates/render/src/forward_plus.rs#L856-L909)）。
- **现象**：任何"只改一处"的修改都会造成位与实际绑定不一致；`has_tangent_uv` 的判定目前两处各写一遍。
- **修复**：抽出唯一入口：

  ```rust
  struct ResolvedMaterialSlots<'a> {
      views: [&'a wgpu::TextureView; 5],   // 已含缺省图
      sampler: &'a wgpu::Sampler,
      flags: u32,
  }
  fn resolve_slots(material, mesh_flags, lib, defaults, cache) -> ResolvedMaterialSlots
  ```

  `flags` 由 `resolve_slots` 一并产出，`from_material` 只负责标量。

### M-07 [P1] 透明材质渲染不正确

- **位置**：uniform alpha 输出 [`forward_plus.wgsl:184`](crates/render/shaders/forward_plus.wgsl#L182-L186)；pipeline 固定混合 + 写深度（[`forward_plus.rs:136-140`](crates/render/src/forward_plus.rs#L136-L140), [:160-166](crates/render/src/forward_plus.rs#L160-L166)）；队列无排序（[`scene.rs:65-113`](crates/render/src/scene.rs#L65-L113)）。
- **现象**：
  - 半透明物体互相遮挡处出现"自身遮挡"黑块（depth write 开着）；
  - 透明物体后面的不透明物体被写深度后遮挡；
  - 绘制顺序 = 场景对象顺序，不是由远到近。
  - Deferred+ 下 Blend 完全退化为不透明（G-Buffer 无 alpha）。
- **修复**：见 §9.6：三步队列（Opaque → Mask → Blend），Blend 用独立 pipeline（`depth_write=false`，`blend=ALPHA_BLENDING`），按相机距离降序排序；Deferred+ 对 Blend 材质回退 Forward（或直接拒绝并给 warning）。
- **测试**：两个半透明面重叠，断言绘制顺序按距离排序；开关 depth-write 的像素级对比。

### M-08 [P1] Instancing 批处理键忽略材质

- **位置**：分组只看 `mesh_key`（[`forward_plus.rs:1129-1152`](crates/render/src/forward_plus.rs#L1129-L1152)）；绘制时只绑定 base 的 `material_bind_group`，且不走 `object_bind_group`（[`forward_node.rs:88-106`](crates/render/src/passes/forward_node.rs#L88-L106)）。
- **现象**：同一 mesh 的不同材质实例被合并成一次 instanced draw → 全部用第一个实例的材质；同时 `normal` 矩阵与蒙皮矩阵丢失（instanced shader 只有 model 矩阵）。
- **修复**：批处理键改为 `(mesh_key, material_key)`；蒙皮/非均匀缩放的实例禁止进入 instanced 批次（或改用 storage buffer 携带 normal/skin 数据）。
- **测试**：两个共用 mesh、不同颜色的实体，断言 `instance_count == 1` 或渲染结果颜色正确。

### M-09 [P2] `custom_shader` 是死字段

- **位置**：[`model.rs:30-31`](crates/material/src/model.rs#L30-L31)；`ShaderGraph::generate_wgsl`（[`shader_graph.rs:80`](crates/material/src/shader_graph.rs#L80)）没有任何渲染侧调用者。
- **现象**：API 承诺可替换着色，实际无效果。
- **修复**：要么接入（按 `ShaderGraph::hash()` 生成 pipeline variant，走 `use-shader-framework`），要么删除字段并在文档中标注"计划中"。

### M-10 [P2] 材质 bind group 每帧重复创建 + 缓存失效

- **位置**：`build_command` 先在 [:947-999](crates/render/src/forward_plus.rs#L947-L999) 建/存 bind group，随后在 [:1053-1086](crates/render/src/forward_plus.rs#L1053-L1086) **无条件再建一次**并用后者绘制。中间 [:1006-1045](crates/render/src/forward_plus.rs#L1006-L1045) 的重建分支因此完全无效。
- **现象**：首帧创建 2 个 bind group（缓存用 + 绘制用）；此后每帧仍无条件至少再建 1 个（纹理变化时共 2 个）。`CachedEntityResources.material_bind_group` 字段形同摆设；`wgpu` 的 bind group 分配是 CPU 热点。
- **修复**：删除第二段创建，始终使用缓存中的 `BindGroup`（`wgpu::BindGroup` 是引用计数句柄，可 clone 到命令里）；仅在 `tex_handles`/`has_tangent_uv` 变化时重建。补一个"帧内 bind group 创建次数"的计数器做回归。
- **收益**：draw 数量不变时 CPU prepare 时间可降一个可观比例（bind group 创建是 wgpu 里较贵的操作）。

### M-11 [P2] 每 entity 一套材质 GPU 资源，无材质去重

- **位置**：`GpuResourceCache::entity_resources: HashMap<String, CachedEntityResources>`（[`common.rs:326-341`](crates/render/src/common.rs#L326-L341)）。
- **现象**：1000 个共享同一材质的物体 → 1000 个 64B uniform buffer + 1000 个 bind group + 1000 次 `write_buffer`。相同材质无法共享，也无法按材质合批。
- **修复**：`GpuMaterial` 以材质内容 hash 为键（见 §9.3），entity 只持 `MaterialKey`。

### M-12 [P2] 无 mipmap，`FilterMode` 的 mipmap 语义丢失

- **位置**：`mip_level_count: 1`（[`common.rs:432`](crates/render/src/common.rs#L428-L438)）；`convert_filter_mode` 把 6 种模式塌缩为 2 种（[:498-507](crates/render/src/common.rs#L498-L507)）。
- **现象**：远处贴图闪烁（无 mip 过滤），各向异性缺失；`LinearMipmapLinear` 与 `Linear` 行为完全相同。
- **修复**：上传时生成 mip 链（`mip_level_count = log2(max(w,h))+1`，逐级 box filter），sampler 的 `mipmap_filter` 单独转换；远期为 KTX2/basis 压缩格式。

### M-13 [P2] `MaterialLibrary` 索引不是稳定标识

- **位置**：[`model.rs:146`](crates/material/src/model.rs#L146)。
- **现象**：没有删除 API，但如果有人 `materials.remove(i)` 或重排，所有 `MaterialHandle` 静默错指；GPU 缓存按 `usize` 索引也会命中错误的旧贴图。
- **修复**：`slotmap`（`physics` / `physics_manager` / `py_engine` 已在用，见 `physics::handles::SceneId`）+ 代校验；或至少提供 `push`/`remove` 封装并禁止外部直接改 `Vec`。

### M-14 [P2] 无材质资产与热重载（`.material.json`）

- **位置**：`asset::meta::AssetTypeKind::Material` 只用于类型识别（[`meta.rs:39-40`](crates/asset/src/meta.rs#L39-L40), [:67-68](crates/asset/src/meta.rs#L65-L70)），`asset` crate 里没有对应 `AssetLoader` 实现（仅有 `GltfData`/`Mesh`/`String`）。
- **现象**：美术改一个颜色必须重新导出 glTF 或写 Python；`Material` 也没有 `Serialize`，无法序列化。
- **进展（已部分修复）**：[`material::asset`](crates/material/src/asset.rs) 已落地 `.material.json` v1（标量参数、按名字增量覆盖 glTF/内置材质、缺失时播种内置默认材质、启动时接入运行时）。**仍未做**：贴图引用（依赖纹理管线，§9.5）、文件监听式热重载（目前重启生效）、走 `asset` crate 的 `AssetCache`/`.meta`。
- **修复**：见 §9.7。

### M-15 [P2] 采样器共用 + 采样器内嵌纹理

- **位置**：`sampler_ref` 取自 base color（[`forward_plus.rs:906-909`](crates/render/src/forward_plus.rs#L906-L909)）；`Texture.sampler` 内嵌（[`model.rs:92`](crates/material/src/model.rs#L86-L93)）。
- **现象**：normal 用 Repeat、base color 用 Clamp 的资产会串味；同一贴图无法两种采样。
- **修复**：`Sampler` 独立为可共享句柄，`TextureRef { texture, sampler }` 逐槽位携带；bind group 需要 5 个 sampler binding（或按槽位分组）。

### M-16 [P2] 未读取 glTF 扩展与常用属性

- `normalTexture.scale`、`occlusionTexture.strength`：**已接入**（`Material::normal_scale` / `occlusion_strength` → uniform z/w），v1 生效。
- `KHR_materials_emissive_strength`：emissive 只能 ≤1，HDR 自发光无法表达。
- `KHR_texture_transform`：无 UV 变换（tiling/offset 只能靠美术在 DCC 里烘）。
- `KHR_materials_clearcoat / transmission / sheen / iridescence`：未支持。
- `COLOR_0` 顶点色：`Vertex` 无颜色字段（[`mesh.rs:16-24`](crates/render/src/mesh.rs#L16-L24)），base color 无法乘顶点色。

### M-17 [P2] Python 侧材质能力过窄

- **位置**：[`py_engine/src/lib.rs:569-598`](crates/py_engine/src/lib.rs#L569-L598)。
- 只能设 `name/rgb/metallic/roughness`；无法设置贴图、alpha、双面、emissive strength、AO strength。
- `get_material_library()` 的 `textures` 恒为空，即使 glTF 载入了贴图也不会出现在 Python 材质库里。

### M-18 [P3] `Material` 无 `PartialEq/Hash`，无法去重/变体键

- 直接后果：无法实现 §9.3 的材质去重，也无法作为 pipeline cache 的 key。
- 修复：为标量部分实现 `PartialEq`；贴图部分用 `TextureId` 比较；另提供稳定的 `content_hash()`（注意 `f32` 用 `to_bits`）。

### M-19 [P3] CPU 像素常驻 + 无纹理预算

- `Texture.pixels` 上传后不释放，显存/内存各一份；无总预算、无 LRU 淘汰、无 `drop` 回收。
- 修复：上传成功后可 `pixels.shrink_to_fit()`/置空（需要 `pixels` 可选的 `uploaded` 标志）；缓存加引用计数与预算。

### M-20 [P3] 测试覆盖不足

- 现有单测仅覆盖 `MaterialUniform` 的字节数与 flags 映射（[`common.rs:597-672`](crates/render/src/common.rs#L597-L672)）。
- 缺失：合并偏移（M-01）、sRGB、透明排序、instancing 分组、上传格式回退、pipeline variant 选择。
- `shader_library` 侧有 WGSL 生成的一致性断言，是好的先例。

### M-21 [P2] fragment shader 三份拷贝

- **位置与实测**：5 个采样函数在三个文件中各写一遍——[`forward_plus.wgsl:82-139`](crates/render/shaders/forward_plus.wgsl#L82-L139)（58 行）、[`deferred_geometry.wgsl:81-137`](crates/render/shaders/deferred_geometry.wgsl#L81-L137)（57 行）、[`forward_plus_instanced.wgsl:81-137`](crates/render/shaders/forward_plus_instanced.wgsl#L81-L137)（57 行）。去掉注释与空行后三份的 SHA256 完全相同（归一化后各 53 行）。
- 另外 `fs_main` 也是两份：`forward_plus.wgsl:141-186` 与 `forward_plus_instanced.wgsl:139-184`，归一化后 40 行逐字相同。
- **后果**：任何采样规则改动（例如给 `normal_scale` 真正接上、把 MR 通道顺序调换、加 sRGB 解码）都要改 3 处；`forward_plus_instanced.wgsl` 共 184 行，其中 104 行（57%）是 `forward_plus.wgsl` 的完整副本，只有 vertex 入口不同。
- **修复**：抽 `material_sampling.wgsl`（5 个函数）与 `forward_fragment.wgsl`（`fs_main`），三个 shader 通过现有 `include_str!` 拼接机制共用；instanced 变体只保留 vertex 入口。deferred 的 `fs_main` 因输出 `GBufferOutput` 必须保留独立实现，但它同样能复用采样片段。
- **测试**：对三份拼接结果做"归一化后一致"断言，或直接断言 `material_sampling.wgsl` 只被 include 一次。

---

## 9. 目标设计（v2）

设计目标按优先级：

1. **正确**：颜色空间、alpha、双面、多模型合并全部可验证正确。
2. **可共享**：相同材质只占一份 GPU 资源，并按材质合批。
3. **可资产化**：材质能从 `.material.json` 加载、热重载、被 Python/编辑器读写。
4. **可扩展**：新增一个材质参数或一种着色模型不需要改四处。
5. **可渐进**：每一步都能独立合入，不阻塞现有功能。

### 9.1 分层模型

```
MaterialAsset（磁盘，可序列化）        ← .material.json / glTF 导入结果
      │  compile()
      ▼
Material（运行期只读定义，Arc 共享）    ← 现有 Material 的扩展版
      │  resolve()
      ▼
GpuMaterial（uniform buffer + bind group，按内容 hash 去重）
      │  bind()
      ▼
draw call（group(1)）
```

- **MaterialInstance（可选，P2）**：`Arc<Material>` + 稀疏参数覆盖，用于"同材质不同颜色"的变体，避免复制整份材质。
- **有效参数** = 基材质 ⊕ 实例覆盖；解析发生在 `build_queue` 阶段，物化为 `ResolvedMaterial`。

### 9.2 句柄与资源表

```rust
// 稳定句柄：索引 + 代
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct MaterialId { index: u32, generation: u32 }
pub struct TextureId  { index: u32, generation: u32 }

pub struct MaterialLibrary {
    materials: SlotMap<MaterialSlotKey, Arc<Material>>,  // slotmap 已在依赖树中
    textures:  TextureTable,   // 句柄 → Arc<TextureAsset> + 代
    samplers:  SamplerTable,   // 去重的 wgpu::Sampler
}
```

- 所有"合并库"的操作统一走 `MaterialLibrary::import_from(&mut other) -> HandleRemap`，由它同时重映射材质与纹理句柄（彻底消除 M-01 这一类问题）。
- `HandleRemap` 形如 `{ materials: Vec<MaterialId>, textures: Vec<TextureId> }`，`ModelMesh`/`Material`/`Prefab` 的引用通过它批量改写。

### 9.3 材质去重（GpuMaterial）

```rust
pub struct MaterialKey(u64);   // 由 Material::content_hash() 得到

pub struct GpuMaterial {
    pub key: MaterialKey,
    pub uniform: wgpu::Buffer,       // 64B，仅内容变化时写
    pub bind_group: wgpu::BindGroup, // 共享
    pub slots: [Option<TextureId>; 5],
    pub flags: u32,
    pub last_used_frame: u64,        // 淘汰用
}

pub struct MaterialGpuCache {
    map: HashMap<MaterialKey, GpuMaterial>,
}
```

- `content_hash()` 覆盖：5 个 factor、alpha mode/cutoff、double_sided、5 个 `TextureId`、5 个 sampler 设置。
- 批处理键：`(mesh_key, MaterialKey)`。相同材质、相同 mesh → 真正可以 instanced。
- 一个材质被多个 entity 使用时，uniform 与 bind group 只创建一次。材质参数在运行时被修改（如 Python 改 emissive）→ 重新求 hash，旧条目按 `last_used_frame` 淘汰。

### 9.4 管线变体

材质决定 pipeline，而非只有一条：

```rust
#[derive(Hash, PartialEq, Eq)]
pub struct MaterialPipelineKey {
    pub shading: ShadingModel,   // Lit / Unlit
    pub alpha: AlphaMode,        // Opaque / Mask / Blend
    pub double_sided: bool,
    pub has_tangent_uv: bool,    // 决定是否编译法线贴图分支（可选）
    pub color_format: wgpu::TextureFormat,
    pub sample_count: u32,
}

pub struct MaterialPipelineCache { map: HashMap<MaterialPipelineKey, wgpu::RenderPipeline> }
```

- `Opaque/Mask`：`depth_write = true`，`cull = Back | None`，`blend = None`。
- `Blend`：`depth_write = false`，`cull` 依 `double_sided`，`blend = ALPHA_BLENDING`。
- `Unlit`：跳过光照，直接输出 base color（对应 glTF `unlit` 扩展、UI/特效材质）。
- 用 `has_tangent_uv` 做变体可以把 `compute_normal` 的分支编译期消掉，顺带解决 M-03 的位掩码问题（此时可保留 uniform 位仅作调试）。

### 9.5 纹理与采样器

```rust
pub struct TextureAsset {
    pub name: Option<String>,
    pub width: u32, pub height: u32,
    pub color_space: ColorSpace,     // Srgb / Linear（决定 GPU 格式）
    pub source: TextureSource,       // Encoded(压缩源) | Raw{ format, bytes }
}

pub struct TextureRef {
    pub texture: TextureId,
    pub sampler: SamplerId,          // 独立、可共享
    pub uv_set: u8,
    pub transform: UvTransform,      // 对应 KHR_texture_transform
}
```

- 上传规则：`ColorSpace::Srgb` → `Rgba8UnormSrgb`；`Linear` → `Rgba8Unorm`。base color / emissive 用 Srgb，其余 Linear。
- 上传时生成 mip 链；`SamplerId` 由 `(filter, wrap)` 去重。
- CPU 像素在成功上传后可释放（`TextureAsset` 保留"是否已上传"状态，编辑模式可保留源以便热重载）。

### 9.6 渲染队列与排序

```rust
pub struct RenderQueues<'a> {
    pub opaque: Vec<RenderCommand<'a>>,   // 按 pipeline key + mesh 排序，earliest first
    pub blend:  Vec<RenderCommand<'a>>,   // 按相机距离降序
}
```

- `SceneRenderer::build_queue` 输出三段（或 `opaque`/`blend` 两段 + `Mask` 归入 opaque）。
- 排序收益：减少 pipeline 切换；Blend 正确的由远及近。
- Deferred+ 对 `Blend` 材质：默认**踢回 Forward 队列**（需要混合前向 pass），或在启动时 `log::warn!` 并降级为 Mask，二者择一，不能静默错误。

### 9.7 材质资产格式

#### 已实现 v1（标量参数，增量补丁）

| 项 | 值 |
| --- | --- |
| 路径 | `<project>/assets/materials/<name>.material.json` |
| 内置来源 | `crates/material/assets/default_materials/`，编译期 `include_str!` |
| 加载入口 | `material::asset::load_project_material_assets(project_dir)`（返回补丁） |
| 应用 | `apply_material_assets(&mut library, &assets)`：同名补丁 / 异名追加 |
| 播种 | 目录不存在时写入全部内置材质（仅一次） |
| 解析库 | `serde_json`，`deny_unknown_fields` |

**语义：补丁，不是定义。** 同名材质已存在（来自 glTF 或内置）时只覆盖文件里声明的变量；没有同名材质时以 `Material::default()` 为基追加。

```json
{ "version": 1, "name": "hero_body", "roughness": 0.75 }
```

上面这份文件只改 `hero_body` 的 roughness，其余（base_color、metallic、alpha…）继续继承 glTF。

| 变量 | 类型 | 映射到 |
| --- | --- | --- |
| `base_color` | `[f32;4]` | `base_color_factor`（alpha 截断到 [0,1]） |
| `metallic` / `roughness` | `f32` | 同名 factor（截断到 [0,1]，越界告警） |
| `emissive` | `[f32;3]` | `emissive_factor` |
| `normal_scale` | `f32` | `Material::normal_scale` → uniform z（负数截断为 0） |
| `occlusion_strength` | `f32` | `Material::occlusion_strength` → uniform w |
| `alpha.mode` | `"opaque" \| "mask" \| "blend"` | `alpha_mode` |
| `alpha.cutoff` | `f32` | `alpha_cutoff` |
| `double_sided` | `bool` | `double_sided`（渲染侧仍未生效，M-05） |

校验规则（任何一条不满足都会报错并跳过该文件，而不是静默生效）：

1. `version` 必须等于 1；
2. `name` 必须等于文件名去掉 `.material.json`（防止"改了文件没生效"）；
3. 未知变量直接拒绝（拼写错误会明确指出）；
4. 越界数值打印 `warn!` 并截断。

匹配限制：按 `Material.name` 找同名材质，因此 glTF 里**没有名字**的材质无法被补丁命中（会退化成新增同名材质）；导入的模型请给材质命名。

仍未实现（对应 §9.5/§10）：

- **贴图引用**：需要先有纹理管线（sRGB、mipmap、独立 sampler），否则 `Texture` 只有原始像素无法落盘引用；
- **热重载**：当前改完 JSON 需要重启（文件监听需要 `notify` 或周期性 stat）；
- **接入 `asset` crate**：现在按路径直接读取，未走 `AssetCache`/`.meta`/依赖扫描。

#### 目标格式（含贴图，未实现）

```json
{
  "version": 2,
  "name": "rusted_metal",
  "shading_model": "lit",
  "base_color": [0.85, 0.72, 0.62, 1.0],
  "metallic": 0.9,
  "roughness": 0.35,
  "textures": {
    "base_color":        { "path": "textures/rust_albedo.png", "color_space": "srgb" },
    "metallic_roughness":{ "path": "textures/rust_mr.png" },
    "normal":            { "path": "textures/rust_n.png", "scale": 1.0 },
    "occlusion":         { "path": "textures/rust_ao.png", "strength": 0.8 },
    "emissive":          { "path": "textures/rust_e.png", "color_space": "srgb" }
  },
  "emissive": [0.0, 0.0, 0.0],
  "emissive_strength": 1.0,
  "alpha": { "mode": "opaque", "cutoff": 0.5 },
  "double_sided": false
}
```

- 贴图引用**路径而非句柄**，加载时解析为 `TextureId` 并登记依赖，避免句柄编号进入磁盘格式（这本身就是 M-01 的根治方式）。
- 升级到 v2 时保留 v1 的执行器（版本号分派），避免已有项目文件全部失效。

### 9.8 Rust API 草案

```rust
// 创建/修改（运行期）
impl MaterialLibrary {
    pub fn create(&mut self, material: Material) -> MaterialId;
    pub fn get(&self, id: MaterialId) -> Option<&Arc<Material>>;
    pub fn get_mut(&mut self, id: MaterialId) -> Option<&mut Material>; // 内部递增代/标脏
    pub fn import_from(&mut self, other: MaterialLibrary) -> HandleRemap;
}

// 渲染侧解析
impl MaterialGpuCache {
    pub fn resolve(&mut self, device, queue, id: MaterialId, lib: &MaterialLibrary,
                   mesh_flags: MeshFlags) -> &GpuMaterial;
}

// 队列
impl SceneRenderer {
    pub fn build_queues<'a, T, I>(&self, lib: &MaterialLibrary, objects: I,
                                  camera_pos: [f32;3]) -> RenderQueues<'a>;
}
```

### 9.9 WGSL 接口约定 v2

保留 group(1) 语义，但**每槽位独立 sampler** 需要改布局。两种方案：

| 方案 | 布局 | 代价 |
| --- | --- | --- |
| A（推荐，渐进） | binding 0 uniform，1..5 texture，6..10 sampler（每槽位一个） | 布局改动小，5 个 sampler 的 bind group 创建成本上升（但 sampler 已去重） |
| B（远期） | binding 0 uniform，1 个 texture array + 索引 uniform，或 bindless | 需要 `binding_array` / 描述符索引能力，wgpu 22 上受 feature/backends 限制 |

无论哪种，**`MaterialUniform` 的标量部分保持不变**，这是已经过验证的稳定契约（§6 I1）。

### 9.10 编辑器 / Python 暴露

- Python（`py_engine`）：提供 `material_create_pbr(name)` → `MaterialId` 句柄；`material_set_color/metallic/roughness/emissive/alpha/set_texture(slot, texture_id)`；`texture_load(path, color_space)`。
- 编辑器：材质资产面板直接操作 `MaterialAsset`（序列化），改动 → 标脏 → 热重载 → 仅重建受影响 `GpuMaterial`。
- 关键约束：**Python 与编辑器都只持有稳定句柄，不持有 `Vec` 索引**。

---

## 10. 迁移计划

每个阶段都可独立合入并保持可运行。

### 阶段 0：止血（P0，1-2 天）

| 项 | 改动 | 验收 |
| --- | --- | --- |
| M-01 | 合并时重映射 `TextureHandle`（先做最小实现：`texture_offset` 加法）；抽 `MaterialLibrary::import_from` | 新增双模型合并单测通过 |
| M-02 | `GameState.materials` 接 `scene.materials`，或直接传 `&self.scene.materials` | 运行时 `missing_materials == 0` |
| M-10 | 删除重复 `create_bind_group`，统一用缓存 | 新增 bind group 创建计数回归 |

### 阶段 1：正确性（P1，约 1 周）

| 项 | 改动 | 验收 |
| --- | --- | --- |
| M-03 | sRGB 上传（按槽位）+ 缺省图 sRGB 版本 | 灰阶贴图 golden 测试 |
| M-04 | 上传格式收口为 `Result` + 回退 + 警告 | 16bit 贴图不 panic |
| M-05 | 按 `double_sided` 生成 pipeline | 单面剔除测试 |
| M-06 | `resolve_slots` 单一入口 | 位掩码单测覆盖 5 槽位组合 |
| M-07 | 队列三分 + Blend pipeline（`depth_write=false`）+ 距离排序 | 双透明面重叠顺序测试 |
| M-08 | 批处理键加 `MaterialKey`；蒙皮/非均匀缩放禁用 instancing | 同 mesh 异材质测试 |

### 阶段 2：资源共享与批处理（P2，约 1-2 周）

- M-11 + M-13 + M-18：`slotmap` 句柄 + `MaterialKey` + `GpuMaterial` 去重。
- M-12：mipmap 生成。
- M-15：sampler 独立、逐槽位绑定（方案 A）。
- 并行推进附录 B 的"参数单一声明源"：本阶段要加的 clearcoat / emissive strength 等参数如果继续按 9 处改，重复劳动会翻倍；先做等价重构再扩参数。
- 验收指标：给定 1000 个共享材质的物体，bind group 创建数从 1000 降到 1；`prepare` 时间下降（记录基线）。

### 阶段 3：资产化与工作流（P2，约 2 周）

- M-14：~~`MaterialAsset` + `.material.json` schema~~ **v1 已完成（标量参数，见 §9.7）**；剩余：贴图引用（依赖阶段 2 的纹理管线）、`asset` crate 接入、编辑器材质面板。
- M-17：Python API 扩展。
- 热重载：先做"手动 reload 按钮"，待 `asset` 异步/`notify` 就绪后接自动监听。

### 阶段 4：高级特性（P3，按需）

- M-09：`custom_shader` 接入或删除。
- M-16：emissive strength、texture transform、clearcoat/transmission、顶点色。
- 材质实例（`MaterialInstance`）与分层材质（材质层 + 贴花/水面的统一）。
- 方案 B（bindless / texture array）。

### 风险与对策

| 风险 | 对策 |
| --- | --- |
| 布局改动（sampler 增加）导致两条管线 + 两套 shader 生成路径同时改 | 先在 `shader_library` 与手写 WGSL 加一致性测试（已有先例），再改布局 |
| `MaterialLibrary` 句柄语义变更波及 `scene`/`py_engine`/`game_runtime` | 阶段 2 引入 `HandleRemap` 与兼容 shim（`impl From<MaterialHandle> for MaterialId`），一次性替换调用点 |
| 透明排序带来 draw 顺序变化，暴露既有资产问题 | 阶段 1 加开关 `render.material.transparent_sort`，默认开；对比截图留档 |
| 无 GPU 自动化测试 | 建立最小 offscreen 测试：渲染 1 帧 → 读回像素 → 与基准比较，复用 `wgpu` 已有的 headless 能力 |

---

## 11. 测试计划

| 层级 | 内容 | 位置建议 |
| --- | --- | --- |
| 单元 | `MaterialUniform` 64B、flags 位、alpha 映射（已有） | `render::common` |
| 单元 | `resolve_slots` 5 槽位组合、`has_tangent_uv` 门控 | 新增 `render::material` 测试 |
| 单元 | 多模型/Prefab 合并重映射（M-01 回归） | `scene::loader` 测试 |
| 单元 | `content_hash` 稳定性、材质去重计数 | 新增 `render::material_gpu` 测试 |
| 单元 | 上传格式不支持时回退且不 panic | `render::common` |
| 集成 | WGSL 解析（`naga` 已是 dev-dependency）覆盖新增变体 | `render/tests` |
| 集成 | 离屏渲染 1 帧读回像素：sRGB 灰阶、双面剔除、透明顺序 | 新增 `render/tests/offscreen.rs` |
| 集成 | 同 mesh 异材质 instancing 分组 | `render::forward_plus` |
| 回归 | 每阶段记录 `draw_calls` / `bind_group_creations` / `prepare` 耗时 | `RenderStats` 扩展 |

---

## 附录 A：关键文件索引

| 文件 | 职责 |
| --- | --- |
| [`crates/material/src/model.rs`](crates/material/src/model.rs) | 数据模型、句柄、材质库 |
| [`crates/material/src/asset.rs`](crates/material/src/asset.rs) | `.material.json` 解析、内置默认材质、增量补丁 |
| [`crates/material/src/shader_graph.rs`](crates/material/src/shader_graph.rs) | 自定义着色图（节点 + WGSL 生成） |
| [`crates/material/assets/default_materials/`](crates/material/assets/default_materials/default.material.json) | 引擎默认材质源文件（编译期嵌入） |
| [`crates/render/src/material.rs`](crates/render/src/material.rs) | 兼容层：再导出 material crate（保留旧路径） |
| [`crates/render/src/common.rs`](crates/render/src/common.rs) | `MaterialUniform`、缺省贴图、上传/采样器转换、GPU 缓存 |
| [`crates/render/src/forward_plus.rs`](crates/render/src/forward_plus.rs) | group(1) 布局、`build_command`、instancing 分组 |
| [`crates/render/src/deferred_plus.rs`](crates/render/src/deferred_plus.rs) | 复用 `build_command`，G-Buffer 管线 |
| [`crates/render/src/scene.rs`](crates/render/src/scene.rs) | `SceneRenderer::build_queue`、默认材质回退 |
| [`crates/render/src/shader_library.rs`](crates/render/src/shader_library.rs) | WGSL 程序化生成版 |
| [`crates/render/shaders/pbr_common.wgsl`](crates/render/shaders/pbr_common.wgsl) | `MaterialUniform`、BRDF |
| [`crates/render/shaders/forward_plus.wgsl`](crates/render/shaders/forward_plus.wgsl) | 采样与直接光照 |
| [`crates/render/shaders/forward_plus_instanced.wgsl`](crates/render/shaders/forward_plus_instanced.wgsl) | instanced 变体：仅 vertex 入口不同，fragment 与 forward_plus 逐字重复（M-21） |
| [`crates/render/shaders/deferred_geometry.wgsl`](crates/render/shaders/deferred_geometry.wgsl) | G-Buffer 打包 |
| [`crates/render/shaders/deferred_lighting.wgsl`](crates/render/shaders/deferred_lighting.wgsl) | G-Buffer 解码与光照 |
| [`crates/scene/src/gltf_material.rs`](crates/scene/src/gltf_material.rs) | glTF → `material::Material` 转换 |
| [`crates/scene/src/loader.rs`](crates/scene/src/loader.rs) | 场景/多模型合并、程序化材质 |
| [`crates/scene/src/prefab_loader.rs`](crates/scene/src/prefab_loader.rs) | Prefab 材质合并 |
| [`crates/py_engine/src/lib.rs`](crates/py_engine/src/lib.rs) | Python 材质注册表与 API |
| [`crates/game_runtime/src/lib.rs`](crates/game_runtime/src/lib.rs) | 纯 Rust 运行时的材质库接线 |
| [`crates/game_runtime/src/python_runtime.rs`](crates/game_runtime/src/python_runtime.rs) | Python 运行时的材质库接线 |
| [`crates/asset/src/meta.rs`](crates/asset/src/meta.rs) | 材质资产类型识别（加载器缺失） |

## 附录 B：材质参数同步点（9 处 → 目标 2 处）

新增一个材质参数（例如 clearcoat）当前要碰 9 处。其中只有 **2 处是语义必需**，其余是**镜像、重复和样板**，可以用三个机制收敛掉。

### B.1 9 处逐项定性

| # | 位置 | 性质 | 能否消除 | 收敛手段 |
| --- | --- | --- | --- | --- |
| 1 | `model.rs`：字段 + `Default` | **语义必需**（数据本身） | 否，但可由声明生成 | 声明宏（B.3.1） |
| 2 | `common.rs`：`MaterialUniform` + `from_material` | 镜像（布局 + 打包） | 是 | 声明宏生成打包代码 |
| 3 | `pbr_common.wgsl`：`MaterialUniform` 结构体 | 镜像（WGSL 侧） | 是 | 声明宏生成文本 |
| 4 | `shader_library.rs`：同名 `StructDef` | **纯重复**（第 3 处的第二套写法） | 是 | 删除，引用同一片段 |
| 5 | `forward_plus.wgsl` / `deferred_geometry.wgsl` / `forward_plus_instanced.wgsl`：5 个采样函数（另：`fs_main` 在 forward 与 instanced 之间也是两份） | **重复 ×3** | 采样逻辑合并为 1 处；"使用新参数"仍要 1 处 | 共享 WGSL 片段（B.3.2） |
| 6 | `deferred_lighting.wgsl` | 条件必需（仅当参数要进 G-Buffer） | 否（数据流本身） | 保留 |
| 7 | `scene/material.rs`：glTF 映射 | 条件必需 + 样板 | 半消除 | 表驱动映射 / 字段表带 `gltf_name` |
| 8 | `py_engine`：专用 setter | 样板 | 是 | 反射式 `material_set_param`（B.3.3） |
| 9 | 测试 + 文档 | 可自动化 | 半消除 | 布局一致性测试 + 文档表由声明生成 |

因此分类是：

- **语义必需（2）**：数据声明（#1）、着色代码里真正使用它（#5 中的"使用"，不在重复的采样函数里）。
- **条件性（2）**：glTF 导入（#7）、进 G-Buffer（#6）。不做就不改。
- **可消除（5）**：#2、#3、#4、#8、#9 的大半，加上 #5 的重复部分。

### B.2 重复的量化证据

- 5 个采样函数（`sample_base_color` / `sample_metallic_roughness` / `sample_occlusion` / `sample_emissive` / `compute_normal`）在**三个**文件里各写一遍：`forward_plus.wgsl:82-139`（58 行）、`deferred_geometry.wgsl:81-137`（57 行）、`forward_plus_instanced.wgsl:81-137`（57 行）。去掉注释与空行后三份**完全一致**（各 53 行，SHA256 同为 `0xB6742E6F…`）。
- `fs_main` 同样是两份：`forward_plus.wgsl:141-186` 与 `forward_plus_instanced.wgsl:139-184`，归一化后 40 行逐字相同。`forward_plus_instanced.wgsl` 全文 184 行，其中 104 行（57%）是 `forward_plus.wgsl` 的副本。
- `pbr_common.wgsl:47-52` 与 `shader_library.rs:234-258` 描述同一个 `MaterialUniform`，字段顺序/类型各写一遍，靠测试对齐。
- `from_material`（`common.rs:135-177`）与 `build_command`（`forward_plus.rs:856-909`）各算一遍纹理位与 `has_tangent_uv`。
- 布局余量：`flags` 的 `z/w` 两个 `u32` 分量目前**空置**，可放 2 个整型/枚举参数而**不扩容**；再多的 f32 参数才需要从 64B 扩到 80B（+1 个 `vec4`）。

### B.3 三个收敛机制

#### B.3.1 单一声明源（收益最大）

用一张声明同时生成：Rust 字段、`Default`、uniform 打包、WGSL 结构体、（可选）Python setter 与文档表格。

```rust
material_layout! {
    uniform MaterialUniform {
        slot 0 => "base_color_factor" => "vec4<f32>";
        slot 2 => "metallic_roughness_normal_occlusion" => "vec4<f32>";
    }
    material Material {
        base_color_factor: [f32; 4] = ([1.0, 1.0, 1.0, 1.0]) @ (0, vec);
        metallic_factor: f32 = (1.0) @ (2, x);
        // 新增参数 = 这一行
        clearcoat: f32 = (0.0) @ (2, z);
    }
}
```

要点：

- 字段仍**强类型**，保留编译期检查（相比"参数 map"方案，热路径无需 `match`）。
- 槽位表与参数表分离：槽位是稀有事件（64→80B），参数是日常事件。
- 宏本身不建议手写 `macro_rules!` 到过度复杂的程度；槽位/参数表可以交给 `build.rs` 生成 `material_uniform.rs` + `material_uniform.wgsl`，可读性更好、`cargo:rerun-if-changed` 控制重跑。

#### B.3.2 共享 WGSL 片段

把 5 个采样函数移进 `shaders/material_sampling.wgsl`，`forward_plus.wgsl` / `deferred_geometry.wgsl` 都通过 Rust 侧拼接引入（现有 `shader_library` 已经在做字符串拼接，机制现成）。

- 改采样规则（例如把 MR 的 G/B 通道互换、加 `normal_scale` 支持）：**1 处**。
- 新增参数并在着色里使用：**1 处**（`fs_main`），采样函数不必动。

#### B.3.3 反射式脚本接口

`py_engine` 用 `material_set_param(idx, name, value)` 替换 N 个专用 setter，`name` 在字段表里查。新增参数在 Python 侧**0 改动**（除非需要类型特殊处理）。

#### B.3.4 一致性护栏（把"记得改"变成"编译/测试失败"）

- **布局测试**：用 `naga`（已是 `render` 的 dev-dependency）解析真实 WGSL，取出 `MaterialUniform` 每个成员的 offset/type，与 `std::mem::offset_of!(MaterialUniform, field)` 对比。手写与生成两条路径都覆盖。
- **覆盖测试**：断言字段表条目数 == `Material` 字段数（宏生成时天然成立；表驱动版本可加）。
- **字节数测试**：`size_of::<MaterialUniform>() == 16 * SLOT_COUNT` 由槽位表推导，扩容自动跟上。

### B.4 原型验证

`expand/material_param_proto/`（非生产代码，可随时删除）实现并验证了 B.3.1：

```
cargo test   →  5 passed; 0 failed
```

一次声明产出：

- `Material` 结构体 + `Default`
- `pack_into(&mut [[f32;4];4])`（生产版替换为 bytemuck 写 `MaterialUniform`）
- `Material::WGSL_STRUCT`（编译期文本）：

```wgsl
struct MaterialUniform {
    base_color_factor: vec4<f32>,
    emissive_alpha_cutoff: vec4<f32>,
    metallic_roughness_normal_occlusion: vec4<f32>,
};
```

第二个声明 `MaterialV2` 只多写了 **1 个槽位 + 2 个参数（3 行）**，四类产物全部自动跟随，测试断言 `clearcoat` 落在 `slots[3].x`、原槽位布局不变。

### B.5 收敛后的改动数

| 场景 | 现在 | 目标 |
| --- | --- | --- |
| 加一个 PBR 标量（复用空槽位，如 `flags.z/w`） | 9 处 | **2 处**：声明 1 行 + 着色使用 1 处 |
| 加一个 PBR 标量（需要新 vec4 槽位） | 9 处 | **3 处**：槽位 1 行 + 声明 1 行 + 着色使用 |
| 同上 + 需要从 glTF 读 | 9 处 | 4 处（+ 映射表 1 行） |
| 同上 + 需要进 G-Buffer | 9 处 | 5-6 处（+ geometry 打包 / lighting 解包，**不可消除**） |
| 修改已有参数的采样规则 | **3 处**（forward / deferred_geometry / instanced） | **1 处**（共享片段） |
| 修改 forward 的着色主流程 | 2 处（forward / instanced） | **1 处**（`forward_fragment.wgsl`；deferred 的 G-Buffer 版本保留差异） |
| 新增一个纹理槽位 | 6 处（布局/绑定/WGSL/采样/缺省图/Python） | 2-3 处（槽位表 + 缺省图 + 采样使用） |

### B.6 代价与风险

| 风险 | 说明 | 缓解 |
| --- | --- | --- |
| 宏可读性 | `macro_rules!` 报错信息差、IDE 跳转弱 | 用 `build.rs` 生成普通代码文件，报错定位回生成文件 + `cargo expand` 辅助 |
| 生成物调试 | 生成的 WGSL 不可直接读 | 生成到 `OUT_DIR` 同时落一份 `debug/` 便于 diff；`print_generated_wgsl` 测试 |
| 强类型 vs 稀疏参数 | 强类型膨胀为很多字段；稀疏 map 灵活但热路径慢 | 混合：核心 PBR 强类型 + `extra: [f32;4]` 或 `Vec<(ParamId, Value)>` 扩展槽 |
| 迁移期双源 | 重构过程中新旧两套并存 | 先做**等价重构**（宏生成的内容与手写逐字节相同），用现有 113 个测试 + 布局测试兜底 |
| `shader_library` 双轨 | 删掉手写 `StructDef` 会牵动 feature 门控 | 让两条路径都 `include_str!` 同一片段，先改结构体，再逐步收敛函数 |

### B.7 落地步骤（对应 §10 阶段 2）

1. 抽 `material_params` 宏/build script，先**等价**覆盖现有 4 槽位与全部参数（行为零变化）。
2. 用生成的文本替换 `pbr_common.wgsl` 与 `shader_library.rs` 中的两处 `MaterialUniform` 声明，删除手写副本。
3. 抽 `material_sampling.wgsl`（5 个函数）+ `forward_fragment.wgsl`（`fs_main`），三个 shader 共用；instanced 变体只保留自己的 vertex 入口，deferred 只保留自己的 G-Buffer 输出。
4. 加 B.3.4 的三类一致性测试作为护栏。
5. 阶段 3 再接反射式 Python setter 与编辑器参数表。

> 结论：**能缩**。9 处里 5 处是镜像/重复/样板，可收敛到 0-1 处；剩下 2 处（数据 + 着色使用）是材质系统本身的语义，无法也不应消除。目标态"加一个参数 = 1 行声明 + 1 处着色"是可达的，且不牺牲类型安全。

## 附录 C：术语

| 术语 | 含义 |
| --- | --- |
| 槽位（slot） | 5 个固定贴图位置（base color / MR / normal / AO / emissive） |
| flags.x | 纹理位掩码，告知 shader 哪些槽位有真实贴图 |
| 缺省贴图 | 槽位无贴图时绑定的 1×1 占位图，用于保持布局完整 |
| 代（generation） | 句柄中用于检测"索引被复用"的版本计数 |
| MaterialKey | 材质内容 hash，用于去重与合批 |
| G-Buffer | Deferred+ 几何 pass 输出的中间缓冲（3 张 RT + 深度） |
