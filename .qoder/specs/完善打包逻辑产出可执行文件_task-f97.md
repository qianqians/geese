# 完善打包逻辑，产出可执行文件

## 核心策略

以**最小变更**为原则，不引入 Cargo workspace，不修改项目结构。通过以下三步实现目标：
1. **修复现有 bug** — 路径硬编码、.dll→.pyd 命名、缺失脚本
2. **创建构建打包脚本** — 统一编译三个 Cargo 项目，收集产物到 dist/
3. **添加 release 优化** — 各项目独立配置 LTO + strip，不依赖 workspace

---

## Task 1: 修复 `run_editor.py` 硬编码路径

**文件**: `d:\Personal\lib\geese\run_editor.py`

**问题**: 4 处硬编码 `D:/Personal/geese/`，实际路径是 `d:\Personal\lib\geese`，且路径少了 `lib/` 层级。

**修改**:
- L5: `'D:/Personal/geese/desktop/target/release/pydesktop.dll'` → 用 `os.path.join(os.path.dirname(os.path.abspath(__file__)), 'desktop', 'target', 'release', 'pydesktop.dll')`
- L12: `'D:/Personal/geese/projects/My'` → 用 `os.path.join(os.path.dirname(os.path.abspath(__file__)), 'projects', 'My')`
- L17/L22: `'D:/Personal/geese/editor_crash.txt'` 等 → 用 `os.path.join(os.path.dirname(os.path.abspath(__file__)), 'editor_crash.txt')`

**风险**: 极低 — 仅改路径字符串

---

## Task 2: 修复 `test_desktop.py` 和 `test_launcher.py` 导入方式

**文件**: `d:\Personal\lib\geese\test_desktop.py`、`d:\Personal\lib\geese\test_launcher.py`

**问题**: 这两个脚本用 `sys.path.insert` + `from pydesktop import run` 加载模块，但 Python 3.14 的扩展模块后缀是 `['.cp314-win_amd64.pyd', '.pyd']`，**不包含 `.dll`**。Cargo 输出的是 `pydesktop.dll`，因此 `from pydesktop import run` 会失败。

**修改**: 改为与 `run_editor.py` 相同的 `ExtensionFileLoader` 方式直接加载 `.dll` 文件，或支持从 debug/release 目录加载：
```python
import importlib.machinery, importlib.util
_dll = os.path.join(os.path.dirname(__file__), 'desktop', 'target', 'debug', 'pydesktop.dll')
_loader = importlib.machinery.ExtensionFileLoader('pydesktop', _dll)
_spec = importlib.util.spec_from_loader('pydesktop', _loader, origin=_dll)
pydesktop = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(pydesktop)
```

**风险**: 低 — 仅改加载方式，逻辑不变

---

## Task 3: 修复 `sample/server/start.bat` 路径问题

**文件**: `d:\Personal\lib\geese\sample\server\start.bat`

**问题**:
1. L1/L4/L9/L15 的 `cd` 缺少 `/d` 参数，跨盘符时无法切换
2. L16 引用 `rank_app.py` 但该文件不存在

**修改**:
- 所有 `cd` 改为 `cd /d`
- L16 的 `rank_app.py` 行暂时注释掉（见 Task 4）

---

## Task 4: 创建缺失的 `rank_app.py`

**文件（新建）**: `d:\Personal\lib\geese\sample\server\src\rank_app.py`

**依据**: `start.bat` L16 引用了它，配置文件 `rank.cfg` 已存在。参照 `app.py`（L107-118）的结构，但注册为 "Rank" 服务。

**内容概要**:
```python
import sys
from engine.engine import *

class RankSubEntity(subentity):
    def __init__(self, is_migrate, source_hub_name, entity_type, entity_id):
        super().__init__(is_migrate, source_hub_name, entity_type, entity_id)
    def update_subentity(self, argvs):
        app().trace(f"RankSubEntity:{self.entity_id} update_subentity!")

def Creator(is_migrate, source_hub_name, entity_id, description):
    return RankSubEntity(is_migrate, source_hub_name, "RankImpl", entity_id)

def main(cfg_file):
    _app = app()
    _app.build(cfg_file)
    _app.register("RankImpl", Creator)
    _app.register_service("Rank")
    _app.run()

if __name__ == '__main__':
    main(sys.argv[1])
```

**风险**: 低 — 新文件，参照现有模式

---

## Task 5: 为三个 Cargo 项目添加 Release Profile 优化

**文件**（修改）:
- `d:\Personal\lib\geese\desktop\Cargo.toml`
- `d:\Personal\lib\geese\server\Cargo.toml`
- `d:\Personal\lib\geese\client\Cargo.toml`

**修改**: 在每个 Cargo.toml 末尾添加：
```toml
[profile.release]
lto = "thin"
codegen-units = 1
strip = true
```

**说明**:
- `lto = "thin"` — 跨 crate 优化，编译时间增加约 1.5x，二进制减小 30-50%
- `codegen-units = 1` — 最大优化，牺牲编译时间换运行时性能
- `strip = true` — 剥离调试符号，大幅减小二进制大小
- **不使用 `panic = "abort"`** — PyO3 依赖 `catch_unwind` 将 Rust panic 转为 Python 异常

**风险**: 低 — 仅影响 release 构建，debug 构建不受影响

---

## Task 6: 创建 `build.ps1` 统一构建打包脚本（核心交付物）

**文件（新建）**: `d:\Personal\lib\geese\build.ps1`

**职责**:
1. 检测系统 Python 版本，生成版本标签（如 `cp314-win_amd64`）
2. 依次构建三个独立 Cargo 项目（release 模式）
3. 创建 `dist/` 目录，收集产物并重命名 `.dll` → `.pyd`
4. 复制 Python 引擎框架、配置文件、依赖服务
5. 生成启动脚本
6. 生成 `build_info.txt` 元信息

**脚本逻辑**:
```powershell
param([string]$Profile = "release", [string]$Target = "all")
$Root = $PSScriptRoot

# 1. 检测 Python 版本
$PyVer = python -c "import sys; print(f'{sys.version_info.major}{sys.version_info.minor}')"
$PyTag = "cp$PyVer-win_amd64"

# 2. 构建三个项目
if ($Target -in @("all", "desktop")) {
    Push-Location "$Root\desktop"
    cargo build --$Profile
    Pop-Location
}
if ($Target -in @("all", "server")) {
    Push-Location "$Root\server"
    cargo build --$Profile
    Pop-Location
}
if ($Target -in @("all", "client")) {
    Push-Location "$Root\client"
    cargo build --$Profile
    Pop-Location
}

# 3. 创建 dist/ 目录结构并收集产物
# ... (详见 Task 7 的目录结构)
```

**依赖**: Task 1-5 完成后执行

---

## Task 7: dist/ 输出目录结构

```
dist/
├── desktop/
│   ├── pydesktop.pyd                     ← desktop/target/release/pydesktop.dll 重命名
│   ├── run.py                             ← 统一入口（修复后的 run_editor.py 副本）
│   └── projects/                          ← 示例项目（可选复制）
│       └── My/
├── server/
│   ├── bin/
│   │   ├── dbproxy.exe                    ← server/target/release/dbproxy.exe
│   │   └── gate.exe                       ← server/target/release/gate.exe
│   ├── engine/                            ← 复制 server/engine/ 整个 Python 框架
│   │   ├── pyhub/
│   │   │   ├── __init__.py
│   │   │   └── pyhub.pyd                  ← server/target/release/pyhub.dll 重命名
│   │   ├── bson/                          ← 含 _cbson.cp312 .pyd（见风险说明）
│   │   ├── msgpack/                       ← 含 _cmsgpack.cp312 .pyd
│   │   ├── pymongo/                       ← 含 _cmessage.cp312 .pyd
│   │   ├── redis/                         ← 纯 Python，无 .pyd
│   │   └── *.py                           ← engine 框架 Python 文件
│   ├── config/                            ← 复制 sample/server/config/
│   │   ├── dbproxy.cfg
│   │   ├── gate.cfg
│   │   ├── player.cfg
│   │   └── rank.cfg
│   ├── src/                               ← 复制 sample/server/src/
│   │   ├── app.py
│   │   └── rank_app.py                    ← Task 4 创建的文件
│   ├── dependences/                       ← 复制 server/dependences/
│   │   ├── consul/consul.exe
│   │   └── redis/redis-server.exe
│   └── start.ps1                          ← 修复后的启动脚本
├── client/
│   ├── engine/                            ← 复制 client/engine/ 整个 Python 框架
│   │   ├── pyclient/
│   │   │   ├── __init__.py
│   │   │   └── pyclient.pyd               ← client/target/release/pyclient.dll 重命名
│   │   ├── msgpack/                       ← 纯 Python fallback 版
│   │   └── *.py
│   └── run.py                             ← 客户端测试入口
└── build_info.txt                         ← 构建元信息
```

**关键设计**:
- `.dll` → `.pyd` 重命名：PyO3 cdylib 输出 `.dll`，但 Python 只认 `.pyd` 后缀。重命名后标准 `import` 语句可直接工作
- **不使用版本标签 .pyd 文件名**（如 `pydesktop.cp314-win_amd64.pyd`）：因为 `__init__.py` 中 `from .pyhub import *` 使用的是简单包名导入，`.pyd` 后缀即可匹配 `EXTENSION_SUFFIXES = ['.cp314-win_amd64.pyd', '.pyd']` 中的第二项
- engine 框架整包复制：保持 `from .bson import ...` 等相对导入的完整性

---

## Task 8: 创建 `dist/desktop/run.py` 启动脚本

**文件（由 build.ps1 生成或作为模板复制）**: `dist/desktop/run.py`

**内容**: 修复后的 `run_editor.py`，路径改为基于 `__file__` 的相对路径：
```python
import sys, os, traceback
import importlib.machinery, importlib.util

_dir = os.path.dirname(os.path.abspath(__file__))
_dll = os.path.join(_dir, 'pydesktop.dll')  # 或 .pyd
_loader = importlib.machinery.ExtensionFileLoader('pydesktop', _dll)
_spec = importlib.util.spec_from_loader('pydesktop', _loader, origin=_dll)
pydesktop = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(pydesktop)

project = sys.argv[1] if len(sys.argv) > 1 else os.path.join(_dir, '..', 'server')
pydesktop.open_editor(project)
```

---

## Task 9: 创建 `dist/server/start.ps1` 启动脚本

**文件（由 build.ps1 生成或作为模板复制）**: `dist/server/start.ps1`

**替代** `sample/server/start.bat`，使用 `$PSScriptRoot` 锚定路径：

```powershell
$Dir = $PSScriptRoot

# 1. 启动 Consul
Start-Process "$Dir\dependences\consul\consul.exe" -ArgumentList "agent","-dev"

# 2. 启动 Redis
Start-Process "$Dir\dependences\redis\redis-server.exe" -WorkingDirectory "$Dir\dependences\redis"

# 3. 等待服务就绪
Start-Sleep -Seconds 3

# 4. 启动 dbproxy
Start-Process "$Dir\bin\dbproxy.exe" -ArgumentList "$Dir\config\dbproxy.cfg"

# 5. 启动 gate
Start-Process "$Dir\bin\gate.exe" -ArgumentList "$Dir\config\gate.cfg"

# 6. 等待 gate 就绪
Start-Sleep -Seconds 3

# 7. 启动 Python hub 脚本
Start-Process python -ArgumentList "$Dir\src\app.py","$Dir\config\player.cfg" -WorkingDirectory "$Dir\src"
Start-Sleep -Seconds 3
Start-Process python -ArgumentList "$Dir\src\rank_app.py","$Dir\config\rank.cfg" -WorkingDirectory "$Dir\src"
```

**注意**: config 文件中的 `log_dir: "../log"` 需要改为绝对路径或确保 CWD 正确。build.ps1 中可自动改写为 `"$Dir/log"`。

---

## Task 10: 创建 `dist/client/run.py` 客户端启动脚本

**文件（由 build.ps1 生成）**: `dist/client/run.py`

```python
import sys, os
sys.path.insert(0, os.path.join(os.path.dirname(__file__), 'engine'))
from engine import app
# ... 调用 app.build() 等
```

---

## Task 11: 修复 CI 配置

**文件**: `d:\Personal\lib\geese\.github\workflows\ci.yml`

**问题**: L37/L96/L99/L134/L137 使用 `cargo build --package <name>` 从根目录执行，但根目录无 `Cargo.toml`（无 workspace），会报错。

**修改**: 为每个 build 步骤添加 `working-directory`：
```yaml
- name: Build desktop
  working-directory: desktop
  run: cargo build --release
```

同时在 Windows server job 后增加打包步骤：
```yaml
- name: Package
  if: runner.os == 'Windows'
  run: pwsh ./build.ps1 -Profile release
- name: Upload artifacts
  if: runner.os == 'Windows'
  uses: actions/upload-artifact@v4
  with:
    name: geese-dist-windows
    path: dist/
```

**Python 版本**: CI 中 `python-version: '3.11'`（L93/L131）应改为与本地一致或使用 `'>=3.11'`。由于 PyO3 cdylib 对 Python ABI 版本敏感，建议固定为 `'3.14'`。

---

## 依赖关系

```
Task 1 (修 run_editor.py) ─────────────┐
Task 2 (修 test_desktop/launcher.py) ───┤
Task 3 (修 start.bat) ── Task 4 (rank_app.py) ──┤
Task 5 (release profile) ──────────────┤
                                       ↓
                              Task 6 (build.ps1)
                                    │
                    ┌───────────────┼───────────────┐
                    ↓               ↓               ↓
              Task 7 (dist结构)  Task 8 (desktop/run)  Task 9 (server/start)
                    │               │               │
                    └───────────────┼───────────────┘
                                    ↓
                              Task 10 (client/run)
                                    │
                                    ↓
                              Task 11 (CI 修复)
```

- Task 1-5 互相独立，可并行实施
- Task 6 依赖 Task 1-5 完成（打包时复制修复后的脚本）
- Task 7-10 是 Task 6 的子产物（由 build.ps1 生成或复制）
- Task 11 独立，可单独进行

---

## 风险与缓解

| 风险 | 等级 | 缓解措施 |
|------|------|---------|
| **Python ABI 版本不匹配** | 高 | build.ps1 构建前检测 `python --version`，确保 PyO3 编译时的 Python 版本与运行时一致。PyO3 0.26.0 对 Python 3.14 的兼容性需验证，若不支持需升级 PyO3 版本 |
| **bson/msgpack/pymongo .pyd 为 cp312** | 高 | `server/engine/` 下的 `_cbson.cp312-win_amd64.pyd`、`_cmsgpack.cp312-win_amd64.pyd`、`_cmessage.cp312-win_amd64.pyd` 是 Python 3.12 编译的第三方 C 扩展，在 Python 3.14 下无法加载。缓解方案：(1) 推荐使用 Python 3.12 运行服务端，或 (2) 用 `pip install bson msgpack pymongo` 安装兼容版本并调整 import 路径，或 (3) 保留这些 .pyd 但在文档中注明需要 Python 3.12 |
| **PyO3 0.26.0 不支持 Python 3.14** | 中 | PyO3 对 Python 版本有严格上限。若编译失败，需升级到支持 3.14 的 PyO3 版本（如 0.27+），或降级系统 Python 到 3.13 |
| **config 相对路径** | 中 | `log_dir: "../log"` 依赖 CWD。start.ps1 中通过 `-WorkingDirectory` 参数设置正确的 CWD，或 build.ps1 中自动改写 config 的 log_dir 为绝对路径 |
| **MongoDB 未纳入打包** | 中 | Server 运行时依赖 MongoDB 但仓库中未包含。start.ps1 中增加 MongoDB 检测步骤，提示用户手动启动 |
| **release 构建时间增加** | 低 | LTO + codegen-units=1 会增加编译时间。仅影响 release 构建，debug 不受影响。利用 cargo 增量编译缓解 |
| **字体已嵌入 DLL** | 无 | `desktop/src/desktop_app.rs` L19 使用 `include_bytes!` 编译时嵌入字体，运行时不需要单独携带字体文件 |

---

## 被拒绝的替代方案

### 方案 A: 引入 Cargo Workspace
**拒绝理由**: 25 个 crate 同时纳入 workspace 是重大架构变更。最大风险是 PyO3 `extension-module` feature 统一化可能导致 server 的 binary 目标（dbproxy.exe, gate.exe）链接失败。此外，`physics` crate 被三个项目以不同 feature 引用，feature 统一化后可能产生意外行为。当前任务只需"产出可执行文件"，不值得冒此风险。长期可考虑，但不在本次范围。

### 方案 B: 修改 Python import 机制以加载 .dll
**拒绝理由**: 虽然 `ExtensionFileLoader` 可以加载 `.dll`，但不符合 Python 标准约定。`.pyd` 是 CPython 标准扩展模块后缀，与现有 `sample/` 部署模式完全一致。重命名 `.dll` → `.pyd` 是零风险操作，且使标准 `import` 语句可直接工作。

### 方案 C: 使用 maturin 构建 Python 扩展
**拒绝理由**: `.venv/Scripts/` 中存在 `maturin.exe`，表明项目曾考虑使用 maturin。但 maturin 要求项目结构调整为 pyproject.toml 模式，变更范围过大。直接 `cargo build` + 手动重命名 `.dll` → `.pyd` 更简单直接。

---

## 验证步骤

完成所有 Task 后，执行以下验证：

1. **构建验证**: `.\build.ps1 -Profile release`，确认三个项目均编译成功
2. **产物验证**: 检查 `dist/` 目录结构完整性，确认 .exe 和 .pyd 文件存在
3. **Desktop 验证**: `python dist/desktop/run.py`，确认编辑器窗口弹出
4. **Server 验证**: `.\dist/server/start.ps1`，确认 Consul + Redis + dbproxy + gate + hub 全部启动
5. **Client 验证**: `python dist/client/run.py`，确认客户端连接到 gate

---

## 关键文件清单

1. `d:\Personal\lib\geese\build.ps1` — 新建，核心构建打包脚本
2. `d:\Personal\lib\geese\run_editor.py` — 修复 4 处硬编码路径
3. `d:\Personal\lib\geese\test_desktop.py` — 修复 .dll 导入方式
4. `d:\Personal\lib\geese\test_launcher.py` — 修复 .dll 导入方式
5. `d:\Personal\lib\geese\sample\server\start.bat` — 修复 cd /d 和 rank_app.py
6. `d:\Personal\lib\geese\sample\server\src\rank_app.py` — 新建，缺失的 rank 服务入口
7. `d:\Personal\lib\geese\desktop\Cargo.toml` — 添加 release profile
8. `d:\Personal\lib\geese\server\Cargo.toml` — 添加 release profile
9. `d:\Personal\lib\geese\client\Cargo.toml` — 添加 release profile
10. `d:\Personal\lib\geese\.github\workflows\ci.yml` — 修复 working-directory + 添加打包步骤
