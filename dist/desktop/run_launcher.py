import sys, os, traceback
import importlib.machinery, importlib.util

_dir = os.path.dirname(os.path.abspath(__file__))

_dll = os.path.join(_dir, 'pydesktop.pyd')
_loader = importlib.machinery.ExtensionFileLoader('pydesktop', _dll)
_spec = importlib.util.spec_from_loader('pydesktop', _loader, origin=_dll)
pydesktop = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(pydesktop)

try:
    pydesktop.run()
    print('Launcher exited normally', flush=True)
except SystemExit as e:
    print(f'SystemExit: {e.code}', flush=True)
except BaseException as e:
    with open(os.path.join(_dir, 'launcher_crash.txt'), 'w') as f:
        f.write(f'{type(e).__name__}: {e}\n')
        traceback.print_exc(file=f)
    print(f'ERROR: {type(e).__name__}: {e}', flush=True)
