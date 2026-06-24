import sys, traceback, os
sys.path.insert(0, 'D:/Personal/geese/desktop/target/release')
try:
    from pydesktop import open_editor
    print('Import OK', flush=True)
    open_editor('D:/Personal/geese/projects/My')
    print('Editor exited normally', flush=True)
except SystemExit as e:
    print(f'SystemExit: {e.code}', flush=True)
except BaseException as e:
    with open('D:/Personal/geese/editor_crash.txt', 'w') as f:
        f.write(f'{type(e).__name__}: {e}\n')
        traceback.print_exc(file=f)
    print(f'ERROR: {type(e).__name__}: {e}', flush=True)

# 写入退出码
with open('D:/Personal/geese/editor_exit.txt', 'w') as f:
    f.write('exited')
