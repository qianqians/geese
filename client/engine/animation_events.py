# -*- coding: UTF-8 -*-
"""动画标记事件处理器。

提供动画时间轴标记事件的注册和分发机制。
当动画播放跨越时间轴上的标记点时，触发同名事件到已注册的回调。

用法:
    from .animation_events import animation_event_handler

    handler = animation_event_handler()
    handler.register("footstep", lambda name, clip, time: print(f"Footstep at {time}"))
    handler.register("attack", on_attack_animation_event)
"""

from collections.abc import Callable


class animation_event_handler:
    """动画标记事件处理器。

    管理按标记名称索引的回调列表，支持多对多注册。
    在游戏主循环中通过 fire_all() 批量触发事件。
    """

    def __init__(self):
        self._handlers: dict[str, list[Callable[[str, str, float], None]]] = {}

    def register(
        self,
        marker_name: str,
        callback: Callable[[str, str, float], None],
    ):
        """注册标记事件回调。

        Args:
            marker_name: 标记名称（与编辑器中设置的标记名一致）
            callback: 回调函数，参数为 (marker_name: str, clip_name: str, time: float)
                      每次动画播放跨越标记点时调用
        """
        if marker_name not in self._handlers:
            self._handlers[marker_name] = []
        self._handlers[marker_name].append(callback)

    def unregister(
        self,
        marker_name: str,
        callback: Callable[[str, str, float], None],
    ):
        """取消注册回调。

        Args:
            marker_name: 标记名称
            callback: 之前注册的回调函数
        """
        if marker_name in self._handlers:
            try:
                self._handlers[marker_name].remove(callback)
                if not self._handlers[marker_name]:
                    del self._handlers[marker_name]
            except ValueError:
                pass

    def fire(self, marker_name: str, clip_name: str, time: float):
        """触发单个标记事件（内部调用）。

        Args:
            marker_name: 标记名称
            clip_name: 触发事件的动画剪辑名称
            time: 动画播放时间（秒）
        """
        for cb in self._handlers.get(marker_name, []):
            try:
                cb(marker_name, clip_name, time)
            except Exception as e:
                print(
                    f"[AnimationEvents] Error in handler for '{marker_name}': {e}"
                )

    def fire_all(self, events: list[tuple[str, str, float]]):
        """批量触发事件。

        Args:
            events: 事件列表，每项为 (marker_name, clip_name, time)
        """
        for name, clip, time in events:
            self.fire(name, clip, time)

    def clear(self):
        """清除所有已注册的回调。"""
        self._handlers.clear()

    def registered_markers(self) -> list[str]:
        """返回所有已注册的标记名称列表。"""
        return list(self._handlers.keys())
