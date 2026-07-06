import sys, traceback, os
import importlib.machinery, importlib.util

_root = os.path.dirname(os.path.abspath(__file__))

# Load the .dll directly (cargo build outputs .dll, not .pyd)
_dll = os.path.join(_root, 'desktop', 'target', 'release', 'pydesktop.dll')
_loader = importlib.machinery.ExtensionFileLoader('pydesktop', _dll)
_spec = importlib.util.spec_from_loader('pydesktop', _loader, origin=_dll)
pydesktop = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(pydesktop)

try:
    pydesktop.open_editor(os.path.join(_root, 'projects', 'My'))
    print('Editor exited normally', flush=True)
except SystemExit as e:
    print(f'SystemExit: {e.code}', flush=True)
except BaseException as e:
    with open(os.path.join(_root, 'editor_crash.txt'), 'w') as f:
        f.write(f'{type(e).__name__}: {e}\n')
        traceback.print_exc(file=f)
    print(f'ERROR: {type(e).__name__}: {e}', flush=True)

with open(os.path.join(_root, 'editor_exit.txt'), 'w') as f:
    f.write('exited')
