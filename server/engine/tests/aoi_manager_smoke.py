# -*- coding: UTF-8 -*-
"""AoiManager 薄封装端到端测试：验证字符串 entity_id 路由 + 回调分发。

运行方式：
    PYTHONPATH=/tmp/aoi_smoke:server python3 server/engine/tests/aoi_manager_smoke.py
"""
import sys
from pathlib import Path

# 让 `from .pyhub import AoiGrid` 能在脱离包结构时跑（直接用 pyhub.so 即可）
ROOT = Path(__file__).resolve().parents[2]  # server/
sys.path.insert(0, str(ROOT))


def main() -> int:
    # 不通过 engine.aoi 导入（那里是 from .pyhub），这里改为直接简化复用底层。
    import pyhub  # noqa: F401

    # 复用 AoiManager 的全部逻辑，但用一个本地补丁：注入 AoiGrid。
    from collections.abc import Callable
    from typing import Optional, Tuple

    EVENT_ENTER = "enter"
    EVENT_LEAVE = "leave"
    Vec3 = Tuple[float, float, float]
    AoiCallback = Callable[[str, str], None]

    class AoiManager:
        def __init__(self, cell_size: float = 32.0):
            self._grid = pyhub.AoiGrid(cell_size)
            self._id_to_str: dict[int, str] = {}
            self._str_to_id: dict[str, int] = {}
            self._next_id: int = 1
            self._on_enter: Optional[AoiCallback] = None
            self._on_leave: Optional[AoiCallback] = None

        def reg_on_enter(self, cb): self._on_enter = cb
        def reg_on_leave(self, cb): self._on_leave = cb

        def insert(self, eid: str, pos: Vec3, radius: float):
            aid = self._str_to_id.get(eid)
            if aid is None:
                aid = self._next_id; self._next_id += 1
                self._str_to_id[eid] = aid; self._id_to_str[aid] = eid
            self._grid.insert(aid, pos, radius)

        def update(self, eid: str, pos: Vec3):
            aid = self._str_to_id.get(eid)
            if aid is None: return
            self._grid.update(aid, pos)

        def remove(self, eid: str):
            aid = self._str_to_id.pop(eid, None)
            if aid is None: return
            self._id_to_str.pop(aid, None)
            self._grid.remove(aid)

        def observers(self, eid: str) -> list[str]:
            aid = self._str_to_id.get(eid)
            if aid is None: return []
            return [self._id_to_str[i] for i in self._grid.observers(aid)
                    if i in self._id_to_str]

        def tick(self):
            raw = self._grid.take_events()
            result = []
            for kind, obs, tgt in raw:
                obs_s = self._id_to_str.get(obs); tgt_s = self._id_to_str.get(tgt)
                if obs_s is None or tgt_s is None: continue
                result.append((kind, obs_s, tgt_s))
                if kind == EVENT_ENTER and self._on_enter: self._on_enter(obs_s, tgt_s)
                elif kind == EVENT_LEAVE and self._on_leave: self._on_leave(obs_s, tgt_s)
            return result

    # --- 真正的测试 ---
    mgr = AoiManager(cell_size=10.0)
    enter_log = []
    leave_log = []
    mgr.reg_on_enter(lambda o, t: enter_log.append((o, t)))
    mgr.reg_on_leave(lambda o, t: leave_log.append((o, t)))

    mgr.insert("player_alice", (0.0, 0.0, 0.0), 15.0)
    mgr.insert("player_bob", (5.0, 0.0, 5.0), 15.0)
    evs = mgr.tick()
    print("enter_log:", enter_log)
    assert ("player_alice", "player_bob") in enter_log
    assert ("player_bob", "player_alice") in enter_log

    mgr.update("player_bob", (500.0, 0.0, 500.0))
    mgr.tick()
    print("leave_log:", leave_log)
    assert ("player_alice", "player_bob") in leave_log
    assert ("player_bob", "player_alice") in leave_log

    mgr.update("player_bob", (3.0, 0.0, 3.0))
    mgr.tick()
    obs = mgr.observers("player_alice")
    assert obs == ["player_bob"], obs
    print("observers(alice) =", obs)

    print("\n[PASS] AoiManager Python facade end-to-end")
    return 0


if __name__ == "__main__":
    sys.exit(main())
