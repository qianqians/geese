#!/usr/bin/env python3
"""测试启动 Geese Launcher"""

import sys
import os

# 添加 pydesktop 动态库路径
desktop_lib_path = os.path.join(os.path.dirname(__file__), 'desktop', 'target', 'debug')
sys.path.insert(0, desktop_lib_path)

try:
    from pydesktop import run
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
