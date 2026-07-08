#!/usr/bin/env python3
"""Helper script to open an Editor window as a subprocess.
Called by DesktopApp with a project path argument.
"""
import sys, traceback, os
import importlib.machinery, importlib.util

project_path = sys.argv[1] if len(sys.argv) > 1 else "."

_root = os.path.dirname(os.path.abspath(__file__))

# Try debug DLL first, fall back to release
_dll_debug = os.path.join(_root, 'desktop', 'target', 'debug', 'pydesktop.dll')
_dll_release = os.path.join(_root, 'desktop', 'target', 'release', 'pydesktop.dll')
_dll = _dll_debug if os.path.exists(_dll_debug) else _dll_release

_loader = importlib.machinery.ExtensionFileLoader('pydesktop', _dll)
_spec = importlib.util.spec_from_loader('pydesktop', _loader, origin=_dll)
pydesktop = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(pydesktop)

try:
    pydesktop.open_editor(project_path)
except BaseException as e:
    print(f'Editor error: {type(e).__name__}: {e}', flush=True)
    traceback.print_exc()
