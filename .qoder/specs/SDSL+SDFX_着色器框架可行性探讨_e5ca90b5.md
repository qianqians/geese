
# Geese 引擎 SDSL+SDFX 着色器框架可行性探讨

---

## 一、现状诊断：Geese 当前着色器系统

### 1.1 架构概览

当前系统采用 **WGSL + naga + wgpu** 管线，25+ 个 WGSL 文件以单文件着色器模式运行：

```
.wgsl 源文件 → [naga IR 解析] → [自定义 #import 展开] → [字符串参数替换] → wgpu::ShaderModule
```

- **加载流程**：`asset.rs` 的 `AssetShaderLoader` 负责加载 → `load_shader_module()` 用 naga FrontEnd 解析 → 自定义 `wgsl_import()` 处理 `#import` 指令 → `replace_params()` 做 `#define` 式字符串替换 → 创建 wgpu ShaderModule
- **参数系统**：简单的字符串替换机制，类似 C 预处理器但更简陋（`crates/render/src/asset.rs`）
- **组合机制**：`#import` 文件包含 + 参数替换，仅此而已
- **管线管理**：`scene_pipeline.rs` 的 `ScenePipelineDescriptor` 硬编码选择 Forward+ 或 Deferred+ 路径

### 1.2 关键痛点

| 痛点 | 表现 | 影响 |
|------|------|------|
| **零组合性** | 无法跨着色器复用函数/结构体，`skinning.wgsl` 的骨骼矩阵计算无法被其他着色器引用 | 代码重复、维护困难 |
| **硬编码顶点格式** | 每个着色器硬编码顶点输入布局，无法共享 | 新增几何类型需修改所有相关着色器 |
| **无数据流抽象** | 无 Vertex→Fragment 的数据路由抽象，手动匹配 location | 容易出错，重构代价大 |
| **材质与着色器绑定** | 硬编码在 Rust 侧管线代码中，无声明式绑定 | 新增材质需同时改 Rust + WGSL |
| **无 Shader 变体系统** | `replace_params()` 仅做简单文本替换，无排列组合管理 | 无法系统化地管理 `SKINNING_ENABLED` 等条件分支 |
| **特效链硬编码** | 后处理链（SSAO→SSR→DoF→MotionBlur→Bloom→Tonemap）硬编码在 Rust 代码中 | 无法动态配置渲染特效组合 |

---

## 二、目标参照：Stride SDSL+SDFX 核心理念

### 2.1 SDSL（Stride Shading Language）

Stride 的核心创新是 **面向组合的着色器语言**：

```
// SDSL 伪代码示例
shader class PBRShader : StreamShader
{
    stream float3 Normal;
    stream float2 UV;
    
    stage float3 Shade()
    {
        return BaseColor * LightingCompute(Normal);
    }
}

shader class SkinningMixin
{
    compose Transform = SkinTransform;
    override stream float4 Position => Transform.Apply(SkinWeights, BoneIndices);
}

// 组合：PBR + Skinning
shader CharacterShader = PBRShader + SkinningMixin;
```

**核心能力**：
- **类继承与组合**：shader class 支持继承、mixin 组合
- **Stream 路由**：自动管理顶点数据在阶段间的流动
- **Stage 管线**：vertex → pixel 各阶段可独立覆盖
- **Compose 语义**：类似依赖注入，运行时替换子组件
- **Link 语义**：阶段间数据传递的显式声明
- **编译器**：SDSL → HLSL/GLSL 的完整转译器（基于 Irony.NET 解析器）

### 2.2 SDFX（Stride Effects）

SDFX 是声明式的 **渲染特效组合系统**：

```
// SDFX 伪代码示例
effect CharacterEffect
{
    passes:
    {
        GBufferPass = GBufferPass(SurfaceShader: PBRShader, StreamInitializer: SkinStreamInit)
        LightingPass = DeferredLightingPass(LightingModel: PBR)
        ShadowPass = ShadowMapPass(StreamInitializer: SkinStreamInit)
    }
}
```

**核心能力**：
- **声明式 Pass 组合**：一个 Effect 由多个 Pass 组成，每个 Pass 引用 SDSL shader
- **RenderFeature 模块化**：光照、阴影、后处理等作为独立 Feature 插入
- **材质绑定**：Effect 声明中直接绑定材质参数到 shader 参数

### 2.3 Stride 给我们的关键启示

| Stride 解决的问题 | Geese 当前状态 |
|---|---|
| 着色器函数/结构体复用 | 无复用机制，每个 .wgsl 独立 |
| 顶点数据流抽象 | 每个着色器硬编码顶点布局 |
| Shader 变体管理 | 简单 #define 替换，无系统化管理 |
| 材质-着色器声明式绑定 | Rust 侧硬编码 |
| 渲染特效声明式组合 | 后处理链 Rust 硬编码 |
| 多 Pass 协调（GBuffer/Lighting/Shadow 共享数据布局） | 每个 Pass 独立管理，无协调机制 |

---

## 三、平台约束：wgpu/WGSL 能力边界

### 3.1 WGSL 硬性限制

| 限制 | 影响 |
|------|------|
| **无 #include / #import 原生支持** | 必须在编译前预处理（Geese 已自行实现） |
| **无 #ifdef 条件编译** | 无法原生做 shader 变体（Geese 用字符串替换绕过） |
| **无泛型/模板** | 无法写参数化 shader 组件 |
| **location 绑定必须显式** | 数据流无法自动路由，必须工具层辅助 |
| **单一 main 入口** | 不能在一个模块中组合多个 shader 的入口 |

### 3.2 wgpu 管线约束

- `wgpu::ShaderModule` 接受完整 WGSL/SPIR-V 源码，不支持运行时 partial compilation
- Bind Group Layout 必须在 Rust 侧显式定义，与 WGSL 的 `@group/@binding` 严格匹配
- Pipeline Layout 决定了 shader 能访问的资源范围

### 3.3 可行窗口

- **naga IR 可编程操作**：可以 parse WGSL → 操作 naga IR → emit WGSL/HLSL/GLSL
- **WGSL 字符串处理成熟**：Geese 已有 `#import` 和参数替换基础设施
- **wgpu 支持 SPIR-V 输入**：理论上可以用任意前端生成 SPIR-V 再喂给 wgpu
- **naga 支持多后端输出**：同一 IR 可输出 WGSL/HLSL/GLSL/Metal

---

## 四、实现路径分析

### 路径 A：增强型 WGSL 预处理器（推荐起步路径）

**思路**：在现有 `#import` 基础上，构建一个功能更强的 WGSL 预处理器，不引入新语言。

**增强能力**：
1. **条件编译**：`#ifdef SKINNING / #endif`，支持 boolean/enum 条件
2. **Shader 片段库**：标准化的函数/结构体片段库（`streams.wgsl`、`lighting.wgsl`、`skinning.wgsl`），通过增强版 `#import` 组合
3. **Stream 宏**：`#stream VERTEX_INPUT(position: vec3f, normal: vec3f, uv: vec2f)` 自动展开为顶点输入声明 + fragment 输入匹配
4. **变体管理**：Rust 侧 `ShaderVariant` 类型，管理 `(base_shader, defines)` 的所有合法组合
5. **数据驱动 Pass 配置**：JSON/TOML 文件描述渲染 pass 链（替代硬编码后处理链）

**工作量**：中等（2-3 个月），主要扩展已有 `asset.rs` 和 `wgsl_import.rs`
**风险**：低——不引入新语言，不改变编译管线，渐进式迁移

### 路径 B：Rust 原生 Shader 组合框架

**思路**：用 Rust 类型系统和 trait 体系构建 shader 组合 DSL，生成 WGSL 代码。

**核心设计**：
```rust
// 概念示意
trait ShaderStream { fn vertex_input() -> VertexLayout; }
trait ShaderStage { fn code() -> WgslFragment; }
trait ShaderCompose<A, B> { fn compose(a: A, b: B) -> CombinedShader; }

// 组合
let character_shader = PBRShader::compose(SkinningMixin);
let effect = RenderEffect::new()
    .add_pass(GBufferPass::new(character_shader))
    .add_pass(LightingPass::deferred_pbr())
    .add_pass(ShadowPass::new(character_shader));
```

**工作量**：大（6-12 个月），需要设计完整的类型体系、WGSL 代码生成器、stream 路由器
**风险**：高——设计空间大，容易过度工程化；但类型安全、IDE 支持好

### 路径 C：独立 SDSL 类语言 + 转译器

**思路**：设计一种类 SDSL 的领域专用语言，构建完整的解析器 → IR → WGSL 转译管线。

**管线**：`.sdsl 文件 → Lexer/Parser(Rust) → SDSL AST → IR 优化 → WGSL 代码生成 → wgpu::ShaderModule`

**工作量**：巨大（12-18+ 个月），需要完整的语言工具链（lexer、parser、类型检查器、IR、代码生成器、错误报告）
**风险**：极高——语言设计、工具链质量、开发者学习曲线都是重大挑战

---

## 五、综合评估与建议

### 5.1 可行性结论

| 路径 | 可行性 | 推荐度 | 说明 |
|------|--------|--------|------|
| **A: 增强预处理器** | **高** | **强烈推荐起步** | 低风险、渐进式、复用现有基础设施 |
| **B: Rust 组合框架** | 中等 | 可作为第二阶段演进 | 类型安全但工程量大，适合团队熟悉后推进 |
| **C: 独立 DSL** | 低 | 不推荐 | 投入产出比极低，除非团队有编译器背景 |

### 5.2 推荐的渐进式演进路线

```
Phase 1 (2-3月)          Phase 2 (3-6月)              Phase 3 (远期)
增强预处理器 ──────────→ Rust 组合层 ──────────────→ 可选：完整 DSL
 ├ 条件编译               ├ Rust trait 组合
 ├ 标准片段库             ├ 类型安全 stream 路由
 ├ Stream 宏              ├ 声明式 Effect 定义
 ├ 变体管理               ├ 数据驱动 Pass 图
 └ 数据驱动 Pass          └ WGSL 代码生成
```

### 5.3 Phase 1 具体可落地的能力

这些能力在路径 A 下即可实现，能解决当前 80% 的痛点：

1. **`#ifdef/#else/#endif` 条件编译** → 解决 shader 变体问题
2. **标准化 stream 声明** (`#stream VERTEX_INPUT(...)`) → 解决顶点格式硬编码问题
3. **函数/结构体片段库** + 增强 `#import` → 解决代码复用问题
4. **`ShaderVariant` Rust 类型** → 系统化管理变体排列
5. **Pass 配置文件**（JSON/TOML）→ 后处理链不再硬编码

### 5.4 与 Stride 的关键差异（需要接受的设计取舍）

| Stride 能力 | Geese 可实现的等价方案 | 取舍 |
|---|---|---|
| SDSL 类型检查 | 依赖 naga 的 WGSL 验证 | 错误定位精度较低 |
| Stream 自动路由 | `#stream` 宏展开 | 需要手动声明但更可控 |
| Compose/Override 语义 | 片段库 + 条件编译 | 无继承语义但更简单直接 |
| SDFX 声明式 Effect | JSON/TOML Pass 配置 | 灵活性略低但易于理解 |
| 多语言输出 (HLSL/GLSL) | naga 天然支持 | 无需额外工作 |

### 5.5 最大风险与缓解

| 风险 | 缓解策略 |
|------|----------|
| 预处理器复杂度失控 | 严格限制宏嵌套深度（≤3层），保持宏语义简单 |
| WGSL 语法演进不兼容 | 预处理器输出标准 WGSL，不依赖 WGSL 未定特性 |
| Stream 宏与 naga 验证不匹配 | 预处理后、提交 wgpu 前做 IR 级别的 location 校验 |
| 渐进式迁移中断 | 每个 Phase 都是独立可用的完整方案，不存在半成品状态 |

---

## 六、总结

**结论：可以实现 Stride SDSL+SDFX 的核心价值，但不需要复制其实现方式。**

Stride 的 SDSL 是一个完整的语言工具链（parser + type checker + IR + codegen），这在工程上非常重。但其核心价值——**着色器组合、数据流路由、声明式特效管理**——完全可以通过更轻量的方式在 Geese 上实现。

推荐的路径是 **从增强型 WGSL 预处理器起步**（Phase 1），在保持 WGSL 生态兼容性的同时获得 80% 的组合能力。如果后续需求驱动，可以演进到 Rust 原生组合框架（Phase 2），最终视情况决定是否需要完整 DSL（Phase 3）。

这条路线的核心优势是：**每个阶段都是独立可用的完整方案**，不存在"做到一半没法用"的风险，同时保持了向更高级方案演进的空间。
