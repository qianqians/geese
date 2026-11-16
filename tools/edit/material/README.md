# 材质编辑器（material-editor）

Geese 引擎的 `.material.json` 材质资产编辑器。Rust + egui 原生桌面应用，
左侧管理材质文件、右侧编辑 PBR 标量参数，窗口背景实时渲染一个球体预览。

## 构建

```bash
cd tools/edit/material
cargo build --release
```

## 运行

```bash
# 编辑当前目录下的项目材质（assets/materials/*.material.json）
cargo run

# 指定项目目录
cargo run -- /path/to/project
```

首次打开会在 `<project_dir>/assets/materials/` 下补齐引擎内置默认材质。

## 功能

- 列出 / 新建 / 保存 `.material.json`（完整 `MaterialAsset` v1 序列化）
- 编辑：base_color（RGBA）、metallic、roughness、emissive（RGB）、
  normal_scale、occlusion_strength、alpha（opaque/mask/blend + cutoff）、double_sided
- 实时 3D 球体预览（Forward+ PBR），左键拖拽旋转、滚轮缩放
- 颜色编辑在 UI 用 sRGB，落盘为线性空间

## 说明

- 数据模型与序列化全部直接调用 `crates/material`（`Material` / `MaterialAsset`）。
- 预览渲染复用 `crates/render`（`WgpuSceneRenderer` Forward+ 路径）。
- v1 仅编辑标量参数；贴图槽位、shader graph 可视化、后处理预览未实现。
