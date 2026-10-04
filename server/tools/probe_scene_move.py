# -*- coding: UTF-8 -*-
"""隔离跑 scene_data 的移动逻辑：验证 '移动一帧' 不再把进程带崩。

不 import 引擎（依赖 Linux 的 pyhub.so），把相对导入剥掉 + 打桩后直接 exec 源码。
用法： python server/tools/probe_scene_move.py [--head]
      --head 用 git HEAD 里的版本（修复前）跑，用来对比。
"""
import enum
import subprocess
import sys
import time


class em_map_element_property(enum.Enum):
    em_map_empty = 0
    em_map_map = 1


class direction(enum.IntFlag):
    none = 0
    up = 1
    down = 2
    left = 4
    right = 8


class position_info:
    def __init__(self):
        self.x = 0
        self.y = 0
        self.x_speed = 0
        self.y_speed = 0
        self.dir = 0


def position_info_to_protcol(p):
    return {"x": p.x, "y": p.y, "x_speed": p.x_speed, "y_speed": p.y_speed, "dir": p.dir}


class stub_caller:
    def move(self, pos):
        pass


def load_scene_data(src: str):
    lines = [l for l in src.splitlines()
             if not l.strip().startswith("from ...") and not l.strip().startswith("from ..")]
    ns = {
        "em_map_element_property": em_map_element_property,
        "direction": direction,
        "position_info": position_info,
        "position_info_to_protcol": position_info_to_protcol,
        "time": time,
    }
    exec(compile("\n".join(lines), "scene_data.py", "exec"), ns)
    return ns["scene_data"]


def make_map(size=16, box=4):
    return {
        "map_width_box": box,
        "map_height_box": box,
        "moveLayer": [em_map_element_property.em_map_map] * size,
        "blockingLayer": [em_map_element_property.em_map_empty] * size,
        "stairsLayer": [em_map_element_property.em_map_empty] * size,
        "climbingLayer": [em_map_element_property.em_map_empty] * size,
    }


def main():
    if "--head" in sys.argv:
        src = subprocess.run(["git", "show", "HEAD:server/src/scene/data/scene_data.py"],
                             capture_output=True, text=True, encoding="utf-8").stdout
        print("== 用 HEAD（修复前）的 scene_data.py ==")
    else:
        with open("server/src/scene/data/scene_data.py", encoding="utf-8") as f:
            src = f.read()
        print("== 用当前工作区的 scene_data.py ==")

    scene_data = load_scene_data(src)
    sd = scene_data("user", {"scene_name": "map_skyland", "scene_line": 1,
                             "pos": {"x": 128, "y": 64}}, stub_caller())
    sd.postion.x = 128.7          # 浮点，和 speed*dt 累加后的真实情况一致
    sd.vertical_dir = direction.right
    m = make_map()

    try:
        print("  check_blocking_move ->", sd.check_blocking_move(m))
        print("  check_fall_off      ->", sd.check_fall_off(m))
        print("  check_stairs        ->", sd.check_stairs(m))
        sd.update(m)
        print("  update() 通过，postion.x =", sd.postion.x)
        print("结果: OK")
    except Exception as e:
        print(f"结果: 崩了 -> {type(e).__name__}: {e}")
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
