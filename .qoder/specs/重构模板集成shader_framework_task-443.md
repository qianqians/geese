# 基于 shader_framework 重构 4 种工程模板

## 背景

最近提交的 shader_framework 引入了：
- `ShaderModuleBuilder` — Rust 原生声明式 shader 模块定义
- `ShaderComposer` — mixin/override/compose 组合
- `EffectLoader/EffectCompiler` — TOML/JSON 声明式渲染 Pass 配置
- `StageChain` — 顺序 shader body 组合
- `VariantManager` — 编译时 shader 变体
- `PipelineCompiler` — 声明式管线编译

render crate 的 `shader_library.rs` 已用 shader_framework 重写了 `pbr_common` 模块注册。但 launcher 的 4 种工程模板（empty/fps/third_person/topdown）尚未展示这些新能力。

## 策略

**增量附加，不改旧文件**：为每种模板新增 effect TOML 配置 + shader 注册模块，不修改现有 scene_builder.rs.txt / camera / player 文件。所有新增文件默认以注释 mod 形式存在，与现有 render/input 注释依赖模式一致。

## 实施步骤

### 1. 新增模板文件：effect TOML 配置（4 个文件）

为每种模板创建差异化的 effect 配置文件：

| 文件 | 模板 | 特点 |
|------|------|------|
| `crates/launcher/templates/empty_effect.toml.txt` | 空项目 | 最小 ForwardPBR，仅 Lighting |
| `crates/launcher/templates/fps_effect.toml.txt` | FPS | ForwardPBR + Shadows + Cluster Lighting |
| `crates/launcher/templates/tp_effect.toml.txt` | 第三人称 | ForwardPBR + Shadows + Fog |
| `crates/launcher/templates/td_effect.toml.txt` | 俯视角 | ForwardPBR + Shadows + Instancing |

TOML 格式遵循 `effect.rs` 中 `RenderEffect` 的序列化结构，包含 `passes`（Geometry/Lighting/Shadow）、`features`、`parameters` 段。

### 2. 新增模板文件：shader 注册模块

新增 `crates/launcher/templates/shader_registry.rs.txt`，包含：
- `register_project_shaders(composer: &mut ShaderComposer)` 函数
- 使用 `ShaderModuleBuilder` 定义 ForwardPBR / Unlit / ShadowDepth 模块
- 展示 `mixin("pbr_common")` 组合模式（参考 `shader_library.rs` 的 `pbr_common_module()`）
- 使用 `{{project_name}}` 占位符

### 3. 修改 `crates/launcher/src/templates.rs`

**3a. 更新 `*_template_files()` 函数**（4 处）

在每个函数（L553-635）的 `vec![]` 末尾追加 2 个 `TemplateFile`：
```rust
TemplateFile {
    relative_path: "src/shader_registry.rs".into(),
    content: include_str!("../templates/shader_registry.rs.txt").to_string(),
},
TemplateFile {
    relative_path: "assets/effects/default_effect.toml".into(),
    content: include_str!("../templates/xxx_effect.toml.txt").to_string(),
},
```

**3b. 更新 `cargo_toml_content()`**（L731-756）

在注释掉的 render/input 依赖后追加：
```
# shader_framework = { path = "../../crates/shader_framework" }
```

**3c. 更新 `main_rs_content()`**（L759-827）

在 `mod_decls` 构建逻辑中追加 `shader_registry` 的注释 mod 声明：
```rust
// mod shader_registry; // 取消注释以启用 shader_framework 声明式管线
```

### 4. 更新 `crates/launcher/src/lib.rs` 中的 `generate_project()`

在目录创建列表（L596）中追加：
```rust
"/assets/effects"
```

### 5. 更新测试

**修改 `crates/launcher/src/lib.rs` 中的测试：**
- `empty_template_has_free_camera`：`files.len() == 4` → `== 6`
- `empty_has_all_modules`：追加 `shader_registry.rs` 和 `default_effect.toml` 的 assert
- `launcher_initial_state`：确认 `templates.len()` 断言与实际 `all_templates()` 返回值一致（当前返回 5 个含 python_game）

### 6. 编译验证

```bash
cd crates/launcher && cargo test
```

## 依赖关系

```
步骤 1-2（新增模板文件）→ 步骤 3（修改 templates.rs 引用新文件）→ 步骤 4-5（目录+测试）→ 步骤 6（验证）
```

## 风险与缓解

| 风险 | 缓解 |
|------|------|
| 测试断言文件数量变化 | 步骤 5 同步更新所有相关断言 |
| effect TOML 格式与 `RenderEffect` 反序列化不匹配 | 参照 `effect.rs` 测试用例和 `examples/effect_loading.rs` 确保格式正确 |
| `include_str!` 路径错误 | 确保新文件放在 `crates/launcher/templates/` 下 |
| python_game 模板不应包含 shader 文件 | 仅修改 4 种 Rust 模板的 `*_template_files()`，不改 `python_game_template_files()` |

## 关键文件

1. `crates/launcher/src/templates.rs` — 模板生成主逻辑
2. `crates/launcher/src/lib.rs` — 工程生成 + 测试
3. `crates/launcher/templates/scene_builder.rs.txt` — 现有材质构建（不修改，作参考）
4. `crates/render/src/shader_library.rs` — shader_framework 使用范例
5. `crates/shader_framework/src/effect.rs` — EffectLoader/EffectCompiler API

## 被拒绝的方案

- **重写 scene_builder.rs.txt**（方案A）：Vertex 构建代码本质上无法通过 shader_framework 简化，且 scene_builder 是脚手架代码（默认注释不编译），重写收益低、风险高
- **为每种模板创建独立 shader_registry**（方案B）：增加维护成本，共用一个模板文件 + 变量替换更简洁
- **启用 render/input 依赖**（方案A/B）：超出本次范围，保持注释模式与现有约定一致
- **引入 VariantManager**（方案B）：过早优化，模板用户可后续自行添加
