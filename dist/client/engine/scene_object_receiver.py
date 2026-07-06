# -*- coding: UTF-8 -*-
"""
客户端场景对象接收器。

处理服务端通过 ``create_remote_entity(entity_type="scene_object_static"/"scene_object_dynamic")``
下发的场景对象，解析 msgpack ``SceneObjectNetMsg`` 并管理本地渲染状态。
"""
from __future__ import annotations

from .base_entity import base_entity


class SceneObjectReceiver(base_entity):
    """
    场景对象——对应服务端下发的单个场景对象。

    直接继承 base_entity,不经过 receiver。场景对象不需要
    hub_notify_callback 和 receiver_mgr 注册。

    refresh_entity 由 scene_object_mgr.update 分发到此。
    """

    def __init__(self, entity_type: str, entity_id: str, argvs: dict) -> None:
        super().__init__(entity_type, entity_id)
        self._parse_and_store(argvs)

        from app import app
        app().scene_object_mgr.add(self)

    def update(self, argvs: dict) -> None:
        """服务端 refresh_entity 时调用，更新对象状态。"""
        self._parse_and_store(argvs)

    # ---- internal ----

    def _parse_and_store(self, argvs: dict) -> None:
        """解析 msgpack 字典为本地字段。

        argvs 的 key 与 Rust 侧 ``SceneObjectNetMsg`` 一致：
        - ``type``: "mesh_ref" | "plane" | "cube"
        - ``transform``: { translation, rotation, scale }
        - ``mesh_ref``: { gltf_path, mesh_name }（仅 type="mesh_ref"）
        - ``color``: [r, g, b]（仅程序化对象）
        - ``dimensions``: [x, y, z]（仅程序化对象）
        """
        self.object_type: str = argvs.get("type", "")
        self.transform: dict = argvs.get("transform", {})
        self.mesh_ref: dict | None = argvs.get("mesh_ref")
        self.color: list[float] | None = argvs.get("color")
        self.dimensions: list[float] | None = argvs.get("dimensions")


# ---------------------------------------------------------------------------
# factory
# ---------------------------------------------------------------------------

def create_scene_object(entity_type: str, entity_id: str, argvs: dict) -> SceneObjectReceiver:
    """
    注册到 ``app.__entity_create_method__`` 的 creator。

    ``entity_type`` 由 ``conn_msg_handle.on_create_remote_entity`` 传入，
    ``argvs`` 已通过 ``msgpack.loads`` 反序列化为 dict。
    """
    return SceneObjectReceiver(entity_type, entity_id, argvs)

# ---------------------------------------------------------------------------
# manager（管理所有已创建的 scene_object receiver）
# ---------------------------------------------------------------------------

class scene_object_manager:
    """追踪所有通过 ``create_remote_entity`` 创建的 SceneObjectReceiver。"""

    def __init__(self) -> None:
        self._receivers: dict[str, SceneObjectReceiver] = {}

    def add(self, r: SceneObjectReceiver) -> None:
        self._receivers[r.entity_id] = r

    def update(self, entity_id: str, argvs: dict) -> None:
        r = self._receivers.get(entity_id)
        if r is None:
            return
        r.update(argvs)

    def remove(self, entity_id: str) -> None:
        self._receivers.pop(entity_id, None)
