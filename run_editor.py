import sys, traceback, os
import importlib.machinery, importlib.util

# Load the .dll directly (cargo build outputs .dll, not .pyd)
_dll = 'D:/Personal/geese/desktop/target/release/pydesktop.dll'
_loader = importlib.machinery.ExtensionFileLoader('pydesktop', _dll)
_spec = importlib.util.spec_from_loader('pydesktop', _loader, origin=_dll)
pydesktop = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(pydesktop)

try:
    pydesktop.open_editor('D:/Personal/geese/projects/My')
    print('Editor exited normally', flush=True)
except SystemExit as e:
    print(f'SystemExit: {e.code}', flush=True)
except BaseException as e:
    with open('D:/Personal/geese/editor_crash.txt', 'w') as f:
        f.write(f'{type(e).__name__}: {e}\n')
        traceback.print_exc(file=f)
    print(f'ERROR: {type(e).__name__}: {e}', flush=True)

with open('D:/Personal/geese/editor_exit.txt', 'w') as f:
    f.write('exited')
