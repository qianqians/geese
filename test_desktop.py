#!/usr/bin/env python3
"""测试 Geese 桌面应用（Launcher + Editor 一体化）"""

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
    
    print("=" * 50)
    print("Geese Desktop 测试")
    print("=" * 50)
    print("\n📦 启动 Geese...")
    print("提示: Launcher 中打开项目 → Editor 独立窗口")
    print("      Launcher 自动隐藏，Editor 关闭后自动恢复")
    print("      所有 Editor 关闭后才能关闭 Launcher")
    
    run()
    
    print("\n✅ 已退出")

except ImportError as e:
    print(f"❌ 无法导入 pydesktop 模块: {e}")
    print(f"请确保已编译 desktop crate: cd desktop && cargo build")
except Exception as e:
    print(f"❌ 运行失败: {e}")
    import traceback
    traceback.print_exc()
