---
id: game-runtime-behaviors
type: module-design
status: draft
title: game_runtime — 场景物件的 behavior 脚本契约
tags: [runtime, python, scene]
references: [tools-cli, tools-edit]
---

## 是什么

场景里的每个物件可以绑 Python 脚本，脚本里的类继承引擎提供的 `behavior`。这份 spec 只覆盖这一件事：怎么在场景里绑、脚本拿到什么、什么时候被调用、出错怎么办。`game_runtime` 的其它部分（渲染循环、物理、post-process）还没有 spec。

## 绑定（场景 JSON）

`objects[]` 与 `models[]` 条目可写：

```json
"script": [ { "path": "scripts/cube.py", "class": "cube", "props": { "speed": 90.0 } } ]
```

- 数组：一个物件可挂多个脚本，按 JSON 顺序 `start`/`update`，逆序 `end`。
- `path` 相对**工程根**（绝对路径原样用）；`class` 必填；`props` 可选，以关键字参数传给类构造函数。
- `models[]` 条目的脚本挂到该模型的**根对象**（`node.parent.is_none()` 的第一个）；程序化对象的 `entity_type` 取 `object_type`，模型根对象取 `model`。
- 装载在 `crates/scene`（`SceneObject.scripts` / `entity_type`，纯数据无 pyo3）；Prefab 实例暂不支持。

## 脚本拿到什么

| 成员 | 含义 |
| --- | --- |
| `self.entity_id` | behavior **实例自己**的 uuid（基类构造时生成），不是场景对象的 entity id |
| `self.entity_type` | 所属对象的 `object_type`（模型为 `model`） |
| `self.props` | 场景里写的 `props` dict |
| `self.transform` | 所属对象节点的**局部变换**：`translation` / `rotation`（欧拉角，度）/ `scale`，可读写 |
| `self.dt` | 当前帧间隔，`update` 前刷新 |
| `self.enabled` | 置 false 后不再调用 `update`（`start`/`end` 照常） |
| `start()` / `update(dt)` / `end()` | 生命周期回调；基类给了空实现，子类实现任意子集 |

`behavior` 有两种拿法：内置模块 `geese`（`from geese import behavior`），以及加载脚本时自动注入模块全局（直接写 `class X(behavior)` 即可）。子类自定义 `__init__` 时 `props` 也照传；**引擎在构造后会补齐** `entity_id`/`entity_type`/`props`/`transform`/`dt`/`enabled`，所以不调 `super().__init__()` 也能用。

## 生命周期

1. 场景加载完成 → 每个脚本 `start()` 一次；
2. 每帧：把对象节点的局部变换与 `dt` 写进实例 → `update(dt)` → 若实例的 `transform` 与写进去的不同，写回 `scene.nodes[obj.node].local_transform`；调用方随后 `scene.update_world_transforms()` 生效（不重算的话改动看不见）；
3. 对象销毁 / 场景重载 / 窗口关闭 → `end()`。

**重建靠签名，不靠一次性加载**：宿主每帧比对「场景里脚本集合」的哈希（对象下标 + path + class + props），变了就「全部 `end()` → 重新实例化 → 全部 `start()`」。因为 Python 游戏运行时（`python_runtime`）的场景是运行时由 Python 建的，开局是空的；只有场景预览（`geese_game`）才是一次性加载。

## 错误策略

读文件、编译、找不到类、类没继承 `behavior`、`start`/`update`/`end` 抛异常 —— 一律变成 `Err`，调用方 `log::error!` 后**让进程非零退出**（用户明确要求「写日志然后崩掉」）。不静默跳过、不每帧重试：脚本写坏时宁可什么都看不到，也不要一个看似正常实则不动的场景。

## 构建与运行前提

- feature `behaviors = ["python-runtime"]`，`game_runtime` 默认开启——所以 `geese_game` 直接带脚本能力，`geese run` 的场景预览也能跑。
- 代价：`geese_game` 链接 `python3XX.dll`，运行需要 python 的安装目录在 `PATH` 上；`geese` CLI 启动调试窗口时会先把 `python -c "import sys;print(sys.base_prefix)"` 前置到子进程 `PATH`（见 `tools-cli`）。
- `extension-module` 模式不会自动初始化解释器，场景预览入口显式调 `pyo3::prepare_freethreaded_python()`。

## 推迟

- Prefab 实例 / 嵌套 prefab 节点绑脚本。
- 脚本之间通信、事件总线、跨对象查询（第一版只给自己的变换）。
- 编辑器预览（`tools/edit/scene`）跑脚本；它只把 `script` 字段原样保留。
- 脚本热重载（`geese reload` 重启窗口即可）。
