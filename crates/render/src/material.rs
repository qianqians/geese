//! 兼容层：材质数据模型已迁移到 [`material`](https://docs.rs/material) crate。
//!
//! 保留 `render::material::*`（以及 `render::Material` 等顶层再导出）路径，
//! 渲染侧现有代码无需改动。GPU 相关内容（`MaterialUniform`、bind group、纹理上传）
//! 仍留在 `render::common` / `render::forward_plus`。

pub use ::material::*;
