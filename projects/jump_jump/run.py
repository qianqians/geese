"""跳一跳 — Python 入口脚本。

用法：
    # 先编译 Rust：cd projects/jump_jump && cargo build
    # 确保 target/debug 下有 jump_jump.dll (Windows)
    # 然后：
    python run.py
"""

import sys
import os
import importlib.machinery
import importlib.util

project_root = os.path.dirname(os.path.abspath(__file__))

# ── 路径 ──
jump_dll_path = os.path.join(project_root, "target", "debug", "jump_jump.dll")
py_engine_dll_path = os.path.normpath(os.path.join(
    project_root, "..", "..", "crates", "py_engine", "target", "debug", "py_engine.dll"))

# 将 game/ 目录加入 sys.path 以找到 jump_game.py
game_dir = os.path.join(project_root, "game")
sys.path.insert(0, os.path.normpath(game_dir))

# Windows DLL 搜索路径
if hasattr(os, "add_dll_directory"):
    os.add_dll_directory(os.path.dirname(py_engine_dll_path))
    os.add_dll_directory(os.path.dirname(jump_dll_path))

# ── 加载 py_engine.dll ──
_loader = importlib.machinery.ExtensionFileLoader("py_engine", py_engine_dll_path)
_spec = importlib.util.spec_from_loader("py_engine", _loader, origin=py_engine_dll_path)
py_engine = importlib.util.module_from_spec(_spec)
sys.modules["py_engine"] = py_engine
_spec.loader.exec_module(py_engine)

# ── 加载 jump_jump.dll ──
_loader = importlib.machinery.ExtensionFileLoader("jump_jump", jump_dll_path)
_spec = importlib.util.spec_from_loader("jump_jump", _loader, origin=jump_dll_path)
jump_jump = importlib.util.module_from_spec(_spec)
sys.modules["jump_jump"] = jump_jump
_spec.loader.exec_module(jump_jump)

from jump_jump import run

if __name__ == "__main__":
    run("jump_game")
