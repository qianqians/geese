# -*- coding: UTF-8 -*-
"""服务端 AOI（Area of Interest）薄封装。

底层由 Rust crate ``aoi``（feature = "pyo3"）提供，pyclass 名为
``AoiGrid``，已通过 ``pyhub`` 模块导出。

架构：
:class:`engine.group.group` 维护一片区域的可视列表，负责 client 端
远端副本的创建/删除 RPC。
:class:`AoiManager` 是 AOI 驱动层——持有 ``group`` 引用，在 ``tick()``
中消费底层 GridAoi 计算出的 Leave 事件，调用 ``group.remove_entity``
或 ``group.remove_player`` 将离开视野的对象移出可视列表。

Enter 事件不做额外处理：group 在原 entity/player 进场时就已全量广播，
AOI 仅负责「离开」的判定与驱逐。

另外提供 :class:`AoiEntityMixin`，供 entity / player 子类按需混入，
获取 ``visible_set`` / ``observer_set`` 与 ``call_observers_notify``
精细化行为广播（仅发给看见自己的 observer）。
"""
from __future__ import annotations

from collections.abc import Callable
from typing import Optional, TYPE_CHECKING

from .pyhub import AoiGrid

if TYPE_CHECKING:
    from .group import group
    from .entity import entity
    from .player import player

Vec3 = tuple[float, float, float]

EVENT_ENTER = "enter"
EVENT_LEAVE = "leave"

AoiCallback = Callable[[str, str], None]


class AoiManager:
    """单房间 AOI 驱动器。

    持有 :class:`group` 引用，在 ``tick()`` 中将底层 GridAoi 的
    Leave 事件转译为 ``group.remove_entity / remove_player``，
    将离开视野的目标移出可视列表。
    """

    __slots__ = ("_grid", "_id_to_str", "_str_to_id", "_next_id",
                 "_visible", "_observers", "_grp",
                 "_entity_registry", "_player_registry",
                 "_on_enter", "_on_leave")

    def __init__(self, grp: "group", cell_size: float = 32.0):
        self._grid = AoiGrid(cell_size)
        self._id_to_str: dict[int, str] = {}
        self._str_to_id: dict[str, int] = {}
        self._next_id: int = 1
        self._visible: dict[str, set[str]] = {}
        self._observers: dict[str, set[str]] = {}
        self._grp = grp
        self._entity_registry: dict[str, "entity"] = {}
        self._player_registry: dict[str, "player"] = {}
        self._on_enter: Optional[AoiCallback] = None
        self._on_leave: Optional[AoiCallback] = None

    # ------------------------------------------------------------------
    # 回调
    # ------------------------------------------------------------------
    def reg_on_enter(self, callback: AoiCallback) -> None:
        self._on_enter = callback

    def reg_on_leave(self, callback: AoiCallback) -> None:
        self._on_leave = callback

    # ------------------------------------------------------------------
    # 实体注册
    # ------------------------------------------------------------------
    def add_entity(self, _e: "entity", position: Vec3, radius: float = 64.0) -> None:
        self._entity_registry[_e.entity_id] = _e
        self._do_insert(_e.entity_id, position, radius)

    def add_player(self, _p: "player", position: Vec3, radius: float = 64.0) -> None:
        self._player_registry[_p.entity_id] = _p
        self._do_insert(_p.entity_id, position, radius)

    def update_position(self, eid: str, position: Vec3) -> None:
        aoi_id = self._str_to_id.get(eid)
        if aoi_id is None:
            return
        self._grid.update(aoi_id, position)

    def remove(self, eid: str) -> None:
        self._entity_registry.pop(eid, None)
        self._player_registry.pop(eid, None)
        self._do_remove(eid)

    # ------------------------------------------------------------------
    # 内部：id 映射 + 底层调用
    # ------------------------------------------------------------------
    def _do_insert(self, eid: str, position: Vec3, radius: float) -> None:
        aoi_id = self._str_to_id.get(eid)
        if aoi_id is None:
            aoi_id = self._next_id
            self._next_id += 1
            self._str_to_id[eid] = aoi_id
            self._id_to_str[aoi_id] = eid
        self._grid.insert(aoi_id, position, radius)

    def _do_remove(self, eid: str) -> None:
        aoi_id = self._str_to_id.pop(eid, None)
        if aoi_id is None:
            return
        self._id_to_str.pop(aoi_id, None)
        self._grid.remove(aoi_id)
        for obs in list(self._observers.get(eid, ())):
            self._visible.get(obs, set()).discard(eid)
        self._observers.pop(eid, None)
        for tgt in list(self._visible.get(eid, ())):
            self._observers.get(tgt, set()).discard(eid)
        self._visible.pop(eid, None)

    # ------------------------------------------------------------------
    # 查询
    # ------------------------------------------------------------------
    def observers_of(self, target_id: str) -> set[str]:
        return set(self._observers.get(target_id, ()))

    def visible_of(self, observer_id: str) -> set[str]:
        return set(self._visible.get(observer_id, ()))

    def entity_count(self) -> int:
        return self._grid.entity_count()

    def has_entity(self, eid: str) -> bool:
        return eid in self._str_to_id

    # ------------------------------------------------------------------
    # 帧驱动 —— Leave → group.remove
    # ------------------------------------------------------------------
    def tick(self) -> list[tuple[str, str, str]]:
        """每帧调用：消费底层 Enter/Leave 事件，同步双向缓存。

        - **Enter**：仅记录缓存 + 触发回调（不做 group 操作）。
        - **Leave**：调用 ``group.remove_entity / remove_player``
          将目标移出可视列表。

        返回 ``[(kind, observer_id, target_id), ...]``。
        """
        raw = self._grid.take_events()
        result: list[tuple[str, str, str]] = []
        for kind, obs, tgt in raw:
            obs_str = self._id_to_str.get(obs)
            tgt_str = self._id_to_str.get(tgt)
            if obs_str is None or tgt_str is None:
                continue

            # 双向缓存同步
            if kind == EVENT_ENTER:
                self._visible.setdefault(obs_str, set()).add(tgt_str)
                self._observers.setdefault(tgt_str, set()).add(obs_str)
            else:  # LEAVE
                if obs_str in self._visible:
                    self._visible[obs_str].discard(tgt_str)
                if tgt_str in self._observers:
                    self._observers[tgt_str].discard(obs_str)

            result.append((kind, obs_str, tgt_str))

            if kind == EVENT_ENTER:
                if self._on_enter is not None:
                    try:
                        self._on_enter(obs_str, tgt_str)
                    except Exception as e:
                        print(f"[aoi] on_enter callback error: {e}")
            else:
                # Leave：将 target 从 group 可视列表移除
                target_e = self._entity_registry.get(tgt_str)
                if target_e is not None:
                    self._grp.remove_entity(target_e)
                else:
                    target_p = self._player_registry.get(tgt_str)
                    if target_p is not None:
                        self._grp.remove_player(target_p)
                if self._on_leave is not None:
                    try:
                        self._on_leave(obs_str, tgt_str)
                    except Exception as e:
                        print(f"[aoi] on_leave callback error: {e}")

        return result

    def __repr__(self) -> str:
        return f"AoiManager(entity_count={self.entity_count()})"


class AoiEntityMixin:
    """供 ``engine.entity.entity`` / ``engine.player.player`` 子类多重继承。

    使用示例：
        class MyHero(player, AoiEntityMixin):
            def on_position_changed(self, pos):
                self.aoi_update(pos)
                # 行为只广播给看见自己的 observers
                self.call_observers_notify("on_move", msgpack.dumps({"pos": pos}))

    Mixin 不会主动调用 ``aoi_register``；业务代码应在实体完成进场后显式调用，
    退场时调 ``aoi_unregister``。
    """
    _aoi_mgr: Optional[AoiManager] = None

    def aoi_register(self, mgr: AoiManager, position: Vec3, radius: float = 64.0) -> None:
        self._aoi_mgr = mgr
        mgr._do_insert(self.entity_id, position, radius)  # type: ignore[attr-defined]

    def aoi_update(self, position: Vec3) -> None:
        if self._aoi_mgr is not None:
            self._aoi_mgr.update_position(self.entity_id, position)  # type: ignore[attr-defined]

    def aoi_unregister(self) -> None:
        if self._aoi_mgr is not None:
            self._aoi_mgr._do_remove(self.entity_id)  # type: ignore[attr-defined]
            self._aoi_mgr = None

    @property
    def visible_set(self) -> set[str]:
        if self._aoi_mgr is None:
            return set()
        return self._aoi_mgr.visible_of(self.entity_id)  # type: ignore[attr-defined]

    @property
    def observer_set(self) -> set[str]:
        if self._aoi_mgr is None:
            return set()
        return self._aoi_mgr.observers_of(self.entity_id)  # type: ignore[attr-defined]

    def call_observers_notify(self, method: str, argvs: bytes) -> int:
        """行为只广播给 observer_set 中的 player。"""
        if self._aoi_mgr is None:
            return 0
        from app import app
        observers = self._aoi_mgr.observers_of(self.entity_id)  # type: ignore[attr-defined]
        if not observers:
            return 0
        sent = 0
        player_mgr = app().player_mgr
        for obs_id in observers:
            target = player_mgr.get_player(obs_id) if hasattr(player_mgr, "get_player") else None
            if target is None:
                continue
            gate = getattr(target, "client_gate_name", None)
            conn = getattr(target, "client_conn_id", None)
            if not gate or not conn:
                continue
            app().ctx.hub_call_client_ntf(gate, conn, method, argvs)
            sent += 1
        return sent