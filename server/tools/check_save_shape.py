# -*- coding: UTF-8 -*-
"""静态对比 scene/data/*.py 里 info() 写出的 key 和 __init__ 读取的 key。

存档结构一旦写/读不一致，就会在 player_data 构造时炸掉，例如：
  create_player Exception:invalid literal for int() with base 10: 'user_id'
  create_player Exception:'pos'
用 ast 解析（不 import 引擎，引擎依赖 Linux 上的 pyhub.so），提前发现这类笔误。

用法： python server/tools/check_save_shape.py
"""
import ast
import os
import sys

DATA_DIR = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "src", "scene", "data")

# 为了兼容老存档而故意多读的 key：{类名: {兼容key}}
COMPAT_READS = {
    "scene_data": {"postion"},  # 老存档里 position 的 key 被写成了 "postion"
}


def dict_keys_written(fn: ast.FunctionDef) -> set:
    """info() 里以字符串为 key 的 dict 字面量 + info["xxx"] = ... """
    keys = set()
    for node in ast.walk(fn):
        if isinstance(node, ast.Dict):
            for k in node.keys:
                if isinstance(k, ast.Constant) and isinstance(k.value, str):
                    keys.add(k.value)
        if isinstance(node, ast.Subscript) and isinstance(node.ctx, ast.Store):
            if isinstance(node.slice, ast.Constant) and isinstance(node.slice.value, str):
                keys.add(node.slice.value)
    return keys


def dict_keys_read(fn: ast.FunctionDef) -> set:
    """__init__() 里 data["xxx"] / data.get("xxx") 读的 key

    只看构造参数（user_id / data / info ...）上的取 key，
    像 pos.get("x") 这种在局部变量上的嵌套读取不算，避免误报。
    """
    roots = {a.arg for a in fn.args.args if a.arg != "self"}
    keys = set()
    for node in ast.walk(fn):
        if isinstance(node, ast.Subscript) and isinstance(node.ctx, ast.Load):
            if isinstance(node.value, ast.Name) and node.value.id in roots:
                if isinstance(node.slice, ast.Constant) and isinstance(node.slice.value, str):
                    keys.add(node.slice.value)
        if isinstance(node, ast.Call) and isinstance(node.func, ast.Attribute) and node.func.attr == "get":
            if isinstance(node.func.value, ast.Name) and node.func.value.id in roots:
                for arg in node.args[:1]:
                    if isinstance(arg, ast.Constant) and isinstance(arg.value, str):
                        keys.add(arg.value)
    return keys


def iterates_root_dict(fn: ast.FunctionDef) -> bool:
    """__init__() 有没有直接遍历整个存档 dict（data.items() / for x in data）

    info() 写的是 {"user_id": ..., "xxx": ...} 这种包装结构时，直接遍历外层 dict
    会把 "user_id" 也当成数据（equip_data 就是这么炸的）。
    """
    roots = {a.arg for a in fn.args.args if a.arg != "self"}
    for node in ast.walk(fn):
        if isinstance(node, ast.Call) and isinstance(node.func, ast.Attribute) and node.func.attr == "items":
            if isinstance(node.func.value, ast.Name) and node.func.value.id in roots:
                return True
        if isinstance(node, ast.For) and isinstance(node.iter, ast.Name) and node.iter.id in roots:
            return True
    return False


def main() -> int:
    failed = False
    for name in sorted(os.listdir(DATA_DIR)):
        if not name.endswith(".py") or name == "__init__.py":
            continue
        with open(os.path.join(DATA_DIR, name), encoding="utf-8") as f:
            tree = ast.parse(f.read(), filename=name)
        for cls in [n for n in tree.body if isinstance(n, ast.ClassDef)]:
            info = next((n for n in cls.body if isinstance(n, ast.FunctionDef) and n.name == "info"), None)
            init = next((n for n in cls.body if isinstance(n, ast.FunctionDef) and n.name == "__init__"), None)
            if info is None or init is None:
                continue
            written = dict_keys_written(info)
            read = dict_keys_read(init)
            missing = read - written - {"user_id"} - COMPAT_READS.get(cls.name, set())
            if missing:
                failed = True
                print(f"!! {name:18s} {cls.name:16s} 读档取了 info() 不会写出的 key: {sorted(missing)}")
            elif written - {"user_id"} and iterates_root_dict(init):
                failed = True
                print(f"!! {name:18s} {cls.name:16s} __init__ 直接遍历了存档外层 dict，"
                      f"应该只读 info() 写出的子 key: {sorted(written - {'user_id'})}")
            else:
                print(f"OK {name:18s} {cls.name:16s} 写出={sorted(written)}")
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
