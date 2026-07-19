"""Geese 通用游戏启动器。

用法:
    python run_game.py <project_dir> <game_module> [--class ClassName] [--title Title] [--width W] [--height H]

示例:
    python run_game.py ../../projects/jump_jump jump_game --class JumpGame --title "跳一跳"
"""

import sys, os, argparse, importlib.machinery, importlib.util, ctypes


def _die(msg: str):
    print(msg, file=sys.stderr)
    sys.exit(1)


def _load_native(name: str, path: str):
    loader = importlib.machinery.ExtensionFileLoader(name, path)
    spec = importlib.util.spec_from_loader(name, loader, origin=path)
    mod = importlib.util.module_from_spec(spec)
    sys.modules[name] = mod
    spec.loader.exec_module(mod)
    return mod


def main():
    p = argparse.ArgumentParser(description="Geese 通用游戏启动器")
    p.add_argument("project_dir", help="游戏项目目录路径")
    p.add_argument("game_module", help="Python 游戏模块名 (如 jump_game)")
    p.add_argument("--class", dest="cls", default=None, help="游戏类名 (默认自动检测)")
    p.add_argument("--title", default=None, help="窗口标题 (默认使用模块名)")
    p.add_argument("--width", type=int, default=1280, help="窗口宽度 (默认 1280)")
    p.add_argument("--height", type=int, default=720, help="窗口高度 (默认 720)")
    p.add_argument("--direct", action="store_true",
                   help="直接加载模式：跳过 pyo3，直接加载 geese_game.dll 运行")
    args = p.parse_args()

    project_dir = os.path.abspath(args.project_dir)
    script_dir = os.path.dirname(os.path.abspath(__file__))
    engine_root = os.path.normpath(os.path.join(script_dir, os.pardir, os.pardir))
    ext = ".dll" if sys.platform == "win32" else (
        ".dylib" if sys.platform == "darwin" else ".so"
    )

    # Resolve geese_game DLL
    # 优先查找：GEESE_GAME_DLL 环境变量
    geese_game_dll = os.environ.get("GEESE_GAME_DLL", "")
    if geese_game_dll and os.path.isfile(geese_game_dll):
        pass  # 使用环境变量指定的路径
    else:
        # 回退：debug 构建输出目录
        geese_game_dll = os.path.join(
            script_dir, "target", "debug", "geese_game" + ext
        )
    if not os.path.isfile(geese_game_dll):
        # 再回退：release 构建输出目录
        geese_game_dll = os.path.join(
            script_dir, "target", "release", "geese_game" + ext
        )
    if not os.path.isfile(geese_game_dll):
        _die(f"[ERROR] geese_game not found: {geese_game_dll}")

    # Add project game/ directory to sys.path
    game_dir = os.path.join(project_dir, "game")
    sys.path.insert(0, os.path.normpath(game_dir))

    if args.direct:
        # ── 直接加载模式：跳过 pyo3，直接通过 ctypes 调用 Rust ──
        if hasattr(os, "add_dll_directory"):
            os.add_dll_directory(os.path.dirname(geese_game_dll))

        lib = ctypes.CDLL(geese_game_dll)

        run_game_rust = lib.run_game_rust
        run_game_rust.argtypes = [
            ctypes.c_char_p,  # project_path
            ctypes.c_char_p,  # module_name
            ctypes.c_char_p,  # class_name
            ctypes.c_char_p,  # title
            ctypes.c_int,     # width
            ctypes.c_int,     # height
        ]
        run_game_rust.restype = None

        game_class = args.cls or "JumpGame"
        run_game_rust(
            project_dir.encode(),
            args.game_module.encode(),
            game_class.encode(),
            (args.title or args.game_module).encode(),
            args.width,
            args.height,
        )
    else:
        # ── pyo3 模式 ──
        # Resolve py_engine DLL
        py_engine_dll = os.environ.get("GEESE_ENGINE_PATH") or os.path.join(
            engine_root, "crates", "py_engine", "target", "debug", "py_engine" + ext
        )
        if not os.path.isfile(py_engine_dll):
            _die(f"[ERROR] py_engine not found: {py_engine_dll}")

        # Register DLL search directories on Windows
        if hasattr(os, "add_dll_directory"):
            os.add_dll_directory(os.path.dirname(py_engine_dll))
            os.add_dll_directory(os.path.dirname(geese_game_dll))

        # Load native extension modules
        _load_native("py_engine", py_engine_dll)
        _load_native("geese_game", geese_game_dll)

        # Auto-detect game class if not specified
        import importlib as _il

        game_mod = _il.import_module(args.game_module)
        game_class = args.cls
        if game_class is None:
            for _n, _o in vars(game_mod).items():
                if isinstance(_o, type) and hasattr(_o, "update"):
                    game_class = _n
                    break
            else:
                game_class = ""

        from geese_game import run_game

        run_game(args.game_module, game_class, args.title or args.game_module,
                 args.width, args.height)


if __name__ == "__main__":
    main()
