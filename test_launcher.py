#!/usr/bin/env python3
"""测试启动 Geese Launcher"""

import sys
import os
import importlib.machinery, importlib.util

# Load the .dll directly (cargo build outputs .dll, not .pyd)
_root = os.path.dirname(os.path.abspath(__file__))
_dll = os.path.join(_root, 'desktop', 'target', 'debug', 'pydesktop.dll')
_loader = importlib.machinery.ExtensionFileLoader('pydesktop', _dll)
_spec = importlib.util.spec_from_loader('pydesktop', _loader, origin=_dll)
pydesktop = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(pydesktop)

try:
    run = pydesktop.run
    print("正在启动 Geese Launcher...")
    run()
    print("\n✅ Launcher 已退出")

except ImportError as e:
    print(f"❌ 无法导入 pydesktop 模块: {e}")
    print(f"请确保已编译 desktop crate: cd desktop && cargo build")
except Exception as e:
    print(f"❌ 启动失败: {e}")
    import traceback
    traceback.print_exc()
