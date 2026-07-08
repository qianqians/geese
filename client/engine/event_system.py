# -*- coding: UTF-8 -*-
"""事件系统。

提供基于 Python 脚本的事件注册和执行机制。开发者直接在 Python 脚本中
注册触发函数和响应函数，由游戏主循环每帧调用 tick() 评估。

用法::

    from client.engine.event_system import EventSystem

    event_sys = EventSystem()

    # 注册事件：trigger 返回 True 时执行 response
    event_sys.register(
        trigger=lambda: player.distance_to(door) < 2.0,
        response=lambda: door.open(),
    )

    # 在主循环中每帧调用
    def on_update(dt):
        event_sys.tick()

触发函数签名：``def trigger() -> bool``
响应函数签名：``def response() -> None``
"""

from collections.abc import Callable
from typing import Optional, Sequence

TriggerFn = Callable[[], bool]
ResponseFn = Callable[[], None]


class EventSystem:
    """事件管理器。

    管理 trigger → response 事件对列表。
    每帧调用 :meth:`tick` 遍历所有事件条目，对 trigger 返回 True 的条目
    依次执行其 response 函数。

    Attributes:
        events: 已注册的事件条目列表
    """

    def __init__(self):
        self._events: list[tuple[TriggerFn, ResponseFn]] = []

    def register(self, trigger: TriggerFn, response: ResponseFn) -> None:
        """注册一个事件条目。

        Args:
            trigger: 触发函数 fn() -> bool
            response: 响应函数 fn()
        """
        self._events.append((trigger, response))

    def unregister(self, trigger: TriggerFn, response: ResponseFn) -> None:
        """取消注册指定的事件条目。

        Args:
            trigger: 之前注册的触发函数
            response: 之前注册的响应函数
        """
        try:
            self._events.remove((trigger, response))
        except ValueError:
            pass

    def tick(self) -> None:
        """评估所有事件条目（每帧调用一次）。

        遍历所有已注册事件，对 trigger 返回 True 的条目执行其 response。
        response 执行期间的异常不会传播，而是打印错误并继续。
        """
        for trigger, response in self._events:
            try:
                if trigger():
                    response()
            except Exception as e:
                print(f"[EventSystem] Error in event handler: {e}")

    def clear(self) -> None:
        """清除所有已注册的事件条目。"""
        self._events.clear()

    @property
    def count(self) -> int:
        """当前已注册的事件条目数量。"""
        return len(self._events)


__all__ = ["EventSystem", "TriggerFn", "ResponseFn"]
