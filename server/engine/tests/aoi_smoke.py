# -*- coding: UTF-8 -*-
"""AOI 冒烟测试：直接导入 pyhub 暴露的 AoiGrid，验证基本生命周期。

运行方式（在 /tmp/aoi_smoke 内已放好 pyhub.so 后）：
    PYTHONPATH=/tmp/aoi_smoke python3 server/engine/tests/aoi_smoke.py
"""
import sys


def main() -> int:
    import pyhub  # noqa: WPS433

    grid = pyhub.AoiGrid(10.0)
    print("constructed:", grid)

    # 1) 两个相近 entity 进入彼此视野
    grid.insert(1, (0.0, 0.0, 0.0), 15.0)
    grid.insert(2, (5.0, 0.0, 5.0), 15.0)
    evs = grid.take_events()
    print("after insert pair:", evs)
    assert ("enter", 1, 2) in evs, evs
    assert ("enter", 2, 1) in evs, evs

    # 2) 远离 -> 互发 leave
    grid.update(2, (500.0, 0.0, 500.0))
    evs = grid.take_events()
    print("after move away:", evs)
    assert ("leave", 1, 2) in evs, evs
    assert ("leave", 2, 1) in evs, evs

    # 3) remove 自动通知 observer
    grid.update(2, (5.0, 0.0, 5.0))
    _ = grid.take_events()
    grid.remove(2)
    evs = grid.take_events()
    print("after remove:", evs)
    assert ("leave", 1, 2) in evs, evs

    # 4) observers 查询
    grid.insert(3, (3.0, 0.0, 3.0), 15.0)
    _ = grid.take_events()
    obs = grid.observers(1)
    assert 3 in obs, obs
    print("observers(1) =", obs)

    # 5) entity_count
    print("entity_count =", grid.entity_count())
    assert grid.entity_count() == 2

    print("\n[PASS] AoiGrid pyo3 smoke test")
    return 0


if __name__ == "__main__":
    sys.exit(main())
