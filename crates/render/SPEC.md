---
id: render-gpu-cache
type: module-design
status: draft
title: render — GPU 资源缓存与场景重建的边界
tags: [render, cache]
---

## 是什么

`crates/render` 里跨管线共用的一份 GPU 资源缓存（`GpuResourceCache`）**只增不淘汰**，键是 CPU 侧资源的地址或句柄。这份契约只覆盖「谁能安全地重建场景」这一个边界——渲染管线的其余部分还没有 spec，等有了再迁出去。

## 契约

三张表，各自以 CPU 侧身份为键：

| 表 | 键 | 失效条件 |
| --- | --- | --- |
| `mesh_buffers` | `compute_mesh_key` = 顶点指针 + 长度 + 索引指针 + 长度 | 结论：**永不自动失效**，只清空 |
| `textures` | 材质纹理句柄（`usize`） | 同上 |
| `entity_resources` | 实体 id（`String`），内部按 `tex_handles` 检测材质变化 | 只有纹理句柄变化时才重建 |

后果：**只要 CPU 侧对象被释放又重新分配**，新对象就可能落在同一地址/同一句柄上而命中旧条目——画面上表现为「上一套几何 / 上一张纹理」。这不是理论风险：`tools/edit/animation` 的子网格替换因此必须把换下来的网格永不释放（见 `tools-edit` 的替换规则）。

## 重建场景的唯一安全做法

整体重建（丢掉旧 `Scene`、重新加载）之前调用一次：

- `WgpuSceneRenderer::clear_resource_cache()` → `ScenePipeline::clear_resource_cache()` → `GpuResourceCache::clear()`，三张表一起清。

为什么必须整表清而不是只清 mesh：重建后材质纹理句柄从 0 重新分配，实体 id 也会复用，单独清 `mesh_buffers` 仍会错渲。

## 取舍与未做的事

- 不做引用计数 / LRU 淘汰：命中判定只花一次指针比较，淘汰需要额外的生命周期记账，目前没有任何调用方需要。
- 不做「按句柄批量失效」：清空是 O(1) 的 `HashMap::clear`，重建场景本来就是重活，不值得为局部失效加接口。
- 已知未覆盖：`DeferredPlusPipeline` 与 `ForwardPlusPipeline` 各自持有一份 cache，新管线实现 `ScenePipeline` 时必须同时实现 `clear_resource_cache`，否则那个管线会悄悄保留陈旧缓冲。
