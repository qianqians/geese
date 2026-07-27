# Shader Stage Chain 组合系统实现方案

## 背景与需求

用户希望在 geese 的 shader 组合系统中引入 **Stage Chain**（阶段链）语义，**不修改 WGSL shader 语法**，仅通过 shader 的 meta 文件（TOML/JSON）配置阶段链：

```toml
# meta 文件中的配置 — 按顺序执行 vs → shader2.vs → shader3.vs
stage = ["vs", "shader2.vs", "shader3.vs"]
```

其中：
- `vs` = base shader 自身的 vertex body
- `shader2.vs` = 名为 `shader2` 的模块的 vertex body
- `shader3.vs` = 名为 `shader3` 的模块的 vertex body
- 也支持 `fs`/`cs` 后缀对应 Fragment/Compute stage
- 不含自身时仅组合其他模块：`stage = ["shader2.vs", "shader3.vs"]`

执行时按列表顺序依次执行各 body。

## 现状分析

当前 `CompositionOp` 有三种操作：
- **Mixin**: 通过 `base()` token 实现两方 body 合并（或完全替换）
- **Override**: 替换指定的 overridable 函数
- **Compose**: 以命名空间前缀方式引入子模块

**核心差距**: `merge_body()` (compose.rs:502-525) 只支持"替换"或"base() 插入"两种模式，无法实现 N 个模块的顺序链式执行。

**关键架构特征** (经验证):
- `ShaderModule` 的 `vertex_body`/`fragment_body`/`compute_body` 是 `Option<WgslFragment>` (compose.rs:23-25)
- `WgslGenerator::generate_vertex_entry()` 在函数顶部声明 `var output: VertexOutput;`，在底部添加 `return output;`，body 直接插入中间 (generator.rs:274-288)
- Block scope `{ }` 在 WGSL 中合法，内部可读写外层变量（`output`、`input` 均可访问）
- 现有 merge helpers（merge_streams, merge_bindings 等）可直接复用

## 实现方案

### Task 1: 在 `CompositionOp` 枚举中添加 `StageChain` 变体

**文件**: `crates/shader_framework/src/compose.rs` (line 146-159)

```rust
pub enum CompositionOp {
    Mixin(String),
    Override { target_fn: String, replacement: FunctionDef },
    Compose { name: String, module: String },
    /// StageChain: sequentially chain entry-point bodies from multiple modules for a specific stage.
    /// Base module's own body (if any) is prepended automatically.
    StageChain {
        stage: ShaderStage,
        modules: Vec<String>,
    },
}
```

**设计决策**: 
- 每次 StageChain 操作针对单个 pipeline stage（Vertex/Fragment/Compute），符合用户 `stage vs(...)` 的 per-stage 语法
- Base 的 body 自动作为链的第一个元素（如果存在），对应 `stage vs(...) : vs, ...` 中 `vs` 排在首位的语义
- Base 无 body 时自动跳过，对应 `stage vs(...) : shader2.vs, shader3.vs` 语法

### Task 2: 在 `ShaderComposer::compose()` 中实现 StageChain 处理逻辑

**文件**: `crates/shader_framework/src/compose.rs` (在 line 223 的 `for op in operations` 循环内添加 match arm)

核心逻辑:
```rust
CompositionOp::StageChain { stage, modules } => {
    // 1. 收集 body 链: base body (if any) + each module's body
    let mut bodies: Vec<&WgslFragment> = Vec::new();
    
    // Base body first (if present)
    match stage {
        ShaderStage::Vertex => { if let Some(b) = vertex_body.as_ref() { bodies.push(b); } },
        ShaderStage::Fragment => { if let Some(b) = fragment_body.as_ref() { bodies.push(b); } },
        ShaderStage::Compute => { if let Some(b) = compute_body.as_ref() { bodies.push(b); } },
    }
    
    // Then each module's body
    for module_name in modules {
        let module = self.modules.get(module_name.as_str()).ok_or_else(|| {
            ShaderError::Composition { message: format!("StageChain module '{}' not found", module_name) }
        })?;
        
        // Merge non-body content (streams, bindings, structs, functions, constants, global_vars)
        Self::merge_streams(&mut input_streams, &module.input_streams);
        Self::merge_streams(&mut output_streams, &module.output_streams);
        Self::merge_bindings(&mut bindings, &module.bindings, base_name, module_name)?;
        Self::merge_structs(&mut structs, &module.structs);
        Self::merge_functions(&mut functions, &module.functions, base_name, module_name)?;
        Self::merge_constants(&mut constants, &module.constants);
        Self::merge_global_vars(&mut global_vars, &module.global_vars);
        
        // Collect stage-specific body
        let module_body = match stage {
            ShaderStage::Vertex => &module.vertex_body,
            ShaderStage::Fragment => &module.fragment_body,
            ShaderStage::Compute => &module.compute_body,
        };
        if let Some(b) = module_body {
            bodies.push(b);
        }
    }
    
    // 2. Chain bodies with block scope isolation
    if !bodies.is_empty() {
        let chained = bodies.iter()
            .map(|b| format!("{{\n{}\n}}", b.source.trim()))
            .collect::<Vec<_>>()
            .join("\n");
        let chained_fragment = WgslFragment::new(chained);
        
        match stage {
            ShaderStage::Vertex => vertex_body = Some(chained_fragment),
            ShaderStage::Fragment => fragment_body = Some(chained_fragment),
            ShaderStage::Compute => compute_body = Some(chained_fragment),
        }
    }
}
```

**关键设计点**:
- 每个 body 包裹在 `{ }` block scope 中，防止局部变量名冲突
- WGSL 中 block 内可读写外层 `output`/`input` 变量，语义正确
- Fragment shader 的 `return` 语句需要特别处理：只有链的最后一个 body 可以包含 `return`（在 block scope 内 `return` 会退出整个函数）

### Task 3: 在 `CompositionBuilder` 中添加 `stage_chain()` 便捷方法

**文件**: `crates/shader_framework/src/compose.rs` (line 537-586 CompositionBuilder impl)

```rust
/// Add a stage chain: sequentially execute multiple modules' entry-point bodies for a stage.
pub fn stage_chain(mut self, stage: ShaderStage, modules: Vec<impl Into<String>>) -> Self {
    self.operations.push(CompositionOp::StageChain {
        stage,
        modules: modules.into_iter().map(|m| m.into()).collect(),
    });
    self
}
```

### Task 4: 在 Effect 系统中支持 meta 文件的 `stage` 字段

**文件**: `crates/shader_framework/src/effect.rs`

4a. 扩展 `PassDef` (line 54-68) 添加可选的 `stage` 字段:
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PassDef {
    pub name: String,
    pub pass_type: PassType,
    pub shader: String,
    #[serde(default)]
    pub compositions: Vec<CompositionDef>,
    #[serde(default)]
    pub features: HashMap<String, String>,
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Stage chain: ordered list of stage body references.
    /// Format: ["vs", "shader2.vs", "shader3.vs"] or ["fs", "mixin.fs"]
    /// Bare "vs"/"fs"/"cs" = base shader's own body.
    /// "module.stage" = referenced module's body.
    #[serde(default)]
    pub stage: Vec<String>,
}
```

4b. 在 `EffectCompiler::compile_pass()` (line 214-244) 中，解析 `stage` 字段并转化为 `CompositionOp::StageChain`:
```rust
// 在 "Convert CompositionDef → CompositionOp and compose" 逻辑之后，
// 如果 pass.stage 非空，额外追加 StageChain 操作
let mut stage_chain_ops = Vec::new();
if !pass.stage.is_empty() {
    // 解析 stage 列表：推断 stage 类型，分离 base/外部引用
    let (shader_stage, modules) = Self::parse_stage_list(&pass.stage)?;
    stage_chain_ops.push(CompositionOp::StageChain {
        stage: shader_stage,
        modules,  // 仅外部模块名列表（不含 base 自身）
    });
}
```

4c. 添加 `parse_stage_list()` 私有辅助方法:
```rust
/// Parse a stage list like ["vs", "shader2.vs", "shader3.vs"] into
/// (ShaderStage, Vec<external_module_names>).
/// Bare "vs"/"fs"/"cs" refers to the base module (skipped in output).
/// "module.suffix" refers to an external module.
fn parse_stage_list(items: &[String]) -> ShaderResult<(ShaderStage, Vec<String>)> {
    let mut stage: Option<ShaderStage> = None;
    let mut modules = Vec::new();
    
    for item in items {
        let (module_part, stage_suffix) = if item.contains('.') {
            let parts: Vec<&str> = item.rsplitn(2, '.').collect();
            (parts[1], parts[0])
        } else {
            ("", item.as_str())  // bare stage name = base module
        };
        
        // Determine stage type from suffix
        let item_stage = match stage_suffix {
            "vs" => ShaderStage::Vertex,
            "fs" => ShaderStage::Fragment,
            "cs" => ShaderStage::Compute,
            _ => return Err(ShaderError::Effect {
                message: format!("Invalid stage suffix '{}' in stage list. Expected 'vs', 'fs', or 'cs'", stage_suffix),
            }),
        };
        
        // Validate all items target the same stage
        if let Some(existing) = stage {
            if existing != item_stage {
                return Err(ShaderError::Effect {
                    message: "All items in a stage list must target the same stage (e.g. all '.vs')".into(),
                });
            }
        } else {
            stage = Some(item_stage);
        }
        
        // Only add external modules (skip base references)
        if !module_part.is_empty() {
            modules.push(module_part.to_string());
        }
    }
    
    let stage = stage.ok_or_else(|| ShaderError::Effect {
        message: "Empty stage list".into(),
    })?;
    
    Ok((stage, modules))
}
```

**TOML meta 文件使用示例**:
```toml
[effect]
name = "CharacterEffect"

[[effect.passes]]
name = "MainPass"
pass_type = "Geometry"
shader = "base_shader"
# Stage chain: 依次执行 base.vs → shader2.vs → shader3.vs
stage = ["vs", "shader2.vs", "shader3.vs"]
```

```toml
# 不含自身，仅组合其他模块
[[effect.passes]]
name = "CompositePass"
pass_type = "Geometry"
shader = "base_shader"
stage = ["shader2.vs", "shader3.vs"]
```

### Task 5: 更新 `validate_effect()` 以验证 stage chain 引用

**文件**: `crates/shader_framework/src/effect.rs` (line 268-287)

在现有验证循环中，对 `stage` 列表额外验证所有外部模块引用:
```rust
for pass in &effect.passes {
    // ... existing shader ref validation ...
    
    // Validate stage chain module references
    for item in &pass.stage {
        if item.contains('.') {
            let module_name = item.rsplitn(2, '.').nth(1).unwrap();
            if composer.get_module(module_name).is_none() {
                return Err(ShaderError::Effect {
                    message: format!(
                        "Pass '{}' stage list references unknown module '{}'",
                        pass.name, module_name
                    ),
                });
            }
        }
    }
}
```

同时更新 `EffectBuilder` 和所有构造 `PassDef` 的代码，为 `stage` 字段提供默认值 `Vec::new()`。

### Task 6: 添加完整测试

**文件**: `crates/shader_framework/src/compose.rs` (tests 模块, line 682+) 和 `crates/shader_framework/tests/integration_tests.rs`

测试用例:
1. **基本 StageChain**: base (有 vertex body) + 2 个模块 → 验证 WGSL 中三段 body 顺序出现
2. **无 base body 的 StageChain**: base (无 vertex body) + 2 个模块 → 验证两段 body 顺序出现
3. **变量隔离**: 各 body 使用同名局部变量 `let x` → 验证 naga 编译通过
4. **StageChain + naga 验证**: 生成的 WGSL 通过 `generate_and_validate()`
5. **StageChain 与 Mixin/Override 混合使用**: 多个 CompositionOp 组合
6. **Stream/Binding 合并**: 被 chain 模块的 streams/bindings 正确合并
7. **Effect TOML 集成**: 从 TOML 加载 stage 配置并编译
8. **Fragment stage chain**: 仅最后一个模块包含 `return` 语句
9. **模块不存在错误**: StageChain 引用不存在的模块时返回正确错误

### Task 7 (可选): 更新 lib.rs 导出

**文件**: `crates/shader_framework/src/lib.rs`

确认 `StageChain` 相关类型通过 `pub use compose::*;` 正确导出（通常无需改动）。

## 依赖关系

```
Task 1 (CompositionOp::StageChain 枚举)
  └── Task 2 (compose 处理逻辑) ── 依赖 Task 1
        ├── Task 3 (CompositionBuilder API) ── 依赖 Task 1
        ├── Task 4 (Effect 系统 TOML/JSON) ── 依赖 Task 1
        │     └── Task 5 (validate 扩展) ── 依赖 Task 4
        └── Task 6 (测试) ── 依赖 Task 2, 3, 4, 5
  └── Task 7 (导出检查) ── 依赖 Task 1
```

推荐实施顺序: Task 1 → Task 2 → Task 3 + Task 4 (并行) → Task 5 → Task 6 → Task 7

## 风险与缓解

| 风险 | 影响 | 缓解 |
|------|------|------|
| **变量名冲突** | WGSL 编译错误 | 每个 body 用 `{ }` block scope 包裹隔离 |
| **`return` 提前退出** | 链中非末位的 return 会终止整个函数 | 文档约定：仅最后一个模块可含 return；后续可增加 return 自动剥离 |
| **Stream 类型不匹配** | 运行时数据错误 | 复用现有 StreamRouter 的语义校验 |
| **Binding slot 冲突** | 编译时错误 | 复用现有 merge_bindings 的冲突检测 |
| **向后兼容性** | 现有代码断裂 | StageChain 是纯增量新增，不修改任何现有逻辑路径 |
| **PassDef serde 兼容性** | 现有 TOML/JSON 解析失败 | 新 `stage` 字段使用 `#[serde(default)]`，旧配置无需修改 |

## 被否决的方案

1. **引入新 WGSL/DSL 语法**: 在 shader 源文件中添加 `stage vs(...) : vs, shader2.vs;` 语法。被否决原因：用户明确要求不修改 WGSL shader 语法，仅通过 meta 文件配置。

2. **通过 CompositionDef 的 op="stage" 配置**: 将 stage chain 作为 composition 操作的一种，放在 `compositions` 列表中。被否决原因：用户希望 `stage` 作为 pass 级别的独立字段，格式更简洁直观。

3. **Plan A (Alex) 的无 stage 参数方案**: 使用不含 `ShaderStage` 的 `StageDef`，一次操作链所有 stage 的 body。被否决原因：无法只 chain vertex stage 而保持 fragment stage 不变。

4. **Plan B (Sam) 的 `own_body` 冗余字段**: 在 `StageChainDef` 中包含 `own_body: Option<WgslFragment>`。被否决原因：base body 已存在于 compose() 的上下文中，额外字段增加维护成本。

5. **Plan C (Tina) 的多次 stage op 自动合并**: 使用多个独立的 stage op 条目后处理合并。被否决原因：语义不直观，且用户已明确使用列表格式。
