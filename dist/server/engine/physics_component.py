# -*- coding: UTF-8 -*-
"""``PhysicsComponent``：让业务 entity 以 component 风格持有刚体。

非侵入：不改 :mod:`base_entity`，业务 entity 在自己 ``__init__`` 中
显式 ``self.physics = PhysicsComponent(...)``，并在销毁时调用 ``destroy()``。
"""
from __future__ import annotations

from typing import Optional, Tuple

from .physics import Body, Scene, Shape, World, get_world

Vec3 = Tuple[float, float, float]
Quat = Tuple[float, float, float, float]


class PhysicsComponent:
    """绑定 entity 与一个物理刚体的轻量 component。"""

    __slots__ = ("_scene", "_body")

    def __init__(
        self,
        scene: Scene,
        shape,
        kind: str = "dynamic",
        position: Vec3 = (0.0, 0.0, 0.0),
        rotation: Quat = (0.0, 0.0, 0.0, 1.0),
        **kwargs,
    ):
        self._scene = scene
        kind = kind.lower()
        if kind == "dynamic":
            self._body = scene.add_dynamic(shape, position, rotation, **kwargs)
        elif kind == "fixed" or kind == "static":
            self._body = scene.add_fixed(shape, position, rotation, **kwargs)
        elif kind == "kinematic" or kind == "kinematic_position":
            self._body = scene.add_kinematic(shape, position, rotation, **kwargs)
        elif kind == "kinematic_velocity":
            self._body = scene.add_kinematic(
                shape, position, rotation, velocity_based=True, **kwargs
            )
        else:
            raise ValueError(f"unknown body kind: {kind}")

    @property
    def scene(self) -> Scene:
        return self._scene

    @property
    def body(self) -> Body:
        return self._body

    @property
    def collider_handle(self) -> int:
        """不透明 u64 collider 句柄，供 physics_sync 碰撞事件分发使用。"""
        return self._body.collider_handle

    @property
    def position(self) -> Vec3:
        return self._body.position()

    @position.setter
    def position(self, value: Vec3) -> None:
        self._body.set_translation(value)

    @property
    def rotation(self) -> Quat:
        return self._body.rotation()

    @rotation.setter
    def rotation(self, value: Quat) -> None:
        self._body.set_rotation(value)

    @property
    def linvel(self) -> Vec3:
        return self._body.linvel()

    @linvel.setter
    def linvel(self, value: Vec3) -> None:
        self._body.set_linvel(value)

    def apply_impulse(self, impulse: Vec3) -> None:
        self._body.apply_impulse(impulse)

    def apply_torque_impulse(self, torque: Vec3) -> None:
        self._body.apply_torque_impulse(torque)

    def is_alive(self) -> bool:
        return self._body.is_alive()

    def destroy(self) -> bool:
        if self._body is None:
            return False
        ok = self._body.remove()
        self._body = None
        return ok


__all__ = ["PhysicsComponent", "Shape", "World", "get_world"]
