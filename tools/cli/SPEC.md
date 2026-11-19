---
id: tools-cli
type: module-design
status: draft
title: tools/cli — geese 命令行入口
tags: [tools, cli]
references: [tools-edit]
---

## 是什么

`tools/cli` 产出 `geese`：把「找工程」和「拉起别的可执行文件」两件事收在一处。它**不链接引擎**（依赖只有 `serde` / `serde_json` / `toml` / `log`），也不自动构建任何东西——编辑器与运行时的产物必须先各自 `cargo build`（编辑器见 `tools-edit`）。

## 子命令

| 命令 | 行为 | 失败 |
| --- | --- | --- |
| `geese create proj` | 当前目录建最小工程。当前目录**或任一上级**已有 `config/project.toml` → 打印工程路径、码 0、什么都不改；目录非空 → 报错码 1 | 目录非空 |
| `geese scene` / `material` / `animation` | 把工程根作为 `--workdir` 前台启动对应编辑器，退出码跟随编辑器 | 不在工程内 / 找不到二进制 |
| `geese run` | 前台拉起调试窗口并透传 stdio，退出码跟随子进程 | 见下 |
| `geese reload` | 杀掉 `.geese/dev.json` 记录的调试窗口，再按记录的 argv 前台重拉 | 记录损坏时当作没有记录 |
| `geese help` / 无参数 | 打印用法，码 0 | — |

参数错误（未知子命令、多余参数）→ 用法 + 码 2。`run` 不接受参数：不再区分 `run scene`。

## 工程判定

- 工程根 = 从当前目录向上第一个含 `config/project.toml` 的目录（当前目录自己也算）。
- `create proj` 生成：`config/project.toml`（`project.name` = 目录名、`template = "empty"`、`scene` 指向默认场景，另带 `[camera]` 段）、`assets/scenes/default.scene.json`（一块地面 + 一盏方向光，打开就能看见东西）、`assets/models/.gitkeep`、`.gitignore`（`.geese/`、`.data/`、`target/`、`__pycache__/`、`*.pyc`）。
- 要求**空目录**：非空就拒绝，绝不覆盖已有文件；写文件一律「只补缺失」。**不带 Cargo 入口、不带 Python 游戏骨架**。

## `run` 的分发

`run` 读工程配置决定跑什么：

| 条件 | 启动 |
| --- | --- |
| `[game].type = "python"` | 在工程根 `cargo run`（要求工程自带 `Cargo.toml`，否则报错码 1） |
| 其它 / 没有 `[game]` | `<geese_game> <工程根> <project.scene（缺省 assets/scenes/default.scene.json）>` |

分发放在 CLI 而不是运行时：`geese_game` 保持「给目录和场景就渲染」的单一职责，CLI 也不用拖引擎依赖。代价就是上面那行——Python 工程必须有 Cargo 入口。**未确认**：Python 工程用 debug 的 `cargo run` 而不是 `--release`。

## 调试窗口记录（`.geese/dev.json`）

`run` 在子进程启动后写 `<工程>/.geese/dev.json`：

```json
{ "pid": 1234, "cwd": "<启动时的工作目录>", "argv": ["<程序>", "…参数"],
  "kind": "scene | python", "started_at": 1763000000 }
```

- `reload` 只认这份记录，因此能和 `run` 在不同终端里配合；记录缺失/损坏 → 打印一句「没有记录」并按 `run` 重新规划，而不是失败。
- **退出时只删除 pid 与自己子进程相同的记录**，且删除方式是「先改名挪开、核对内容」——`reload` 杀掉旧进程后会立刻写新记录，直接 `读-比较-删除` 有窗口把新记录删掉（已用单测钉住）。
- 写记录时顺手把 `.geese/` 补进工程 `.gitignore`（只在缺失时追加一行）。

## 二进制查找

顺序：`$GEESE_BIN_DIR` → `geese` 自己所在目录 → 仓库根下各 `target/{debug,release}`。仓库根 = 向上第一个含 `tools/edit` 的目录；`geese_game` 在 `target/` 与 `crates/game_runtime/target/` 两处都查。全都没有 → 报错码 1，列出找过的完整路径 + 该跑的 `cargo build`。

启动调试窗口时会把 Python 的 `sys.base_prefix` 前置到子进程 `PATH`：`geese_game` 链接了 `python3XX.dll`，而 PATH 上通常只有 python 的启动 shim（见 `game-runtime-behaviors`）。

进程存活与终止走系统命令（Windows `tasklist` / `taskkill /T /F`，类 Unix `kill -0` / `-TERM` / `-KILL`），不为这点功能引入平台 crate。

## 推迟

- 编辑器子命令不接受资源文件参数（那要改四个工具的 CLI）。
- 没有 `stop` / `status` 子命令；`run`/`reload` 都是前台，`reload` 重启后的窗口是孤儿进程，原来那个终端里的 `run` 会退出。
- 不自动构建、不支持工程模板选择（只有 `empty`）、不管理多实例/多窗口。
