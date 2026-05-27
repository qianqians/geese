# -*- coding: UTF-8 -*-
"""薄封装：把 pyhub 暴露的物理类整理成 Python 友好的 API。

本模块仅做调用转发与默认参数处理，不持有额外状态。多个场景可以共享一个
``World`` 实例；每个游戏房间/scene 对应一个 ``Scene``。

不依赖客户端 ``crates/scene``，状态同步走业务侧已有的 entity RPC 通道。
"""
from __future__ import annotations

from typing import Iterable, Optional, Tuple

from .pyhub import (
    PhysicsBody,
    PhysicsCollisionEvent,
    PhysicsRayHit,
    PhysicsShape,
    PhysicsWorld,
    cast_ray as _cast_ray,
)

Vec3 = Tuple[float, float, float]
Quat = Tuple[float, float, float, float]


class Shape:
    """工厂方法集：返回底层 ``PhysicsShape``。"""

    @staticmethod
    def cuboid(hx: float, hy: float, hz: float) -> PhysicsShape:
        return PhysicsShape.cuboid(hx, hy, hz)

    @staticmethod
    def ball(radius: float) -> PhysicsShape:
        return PhysicsShape.ball(radius)

    @staticmethod
    def capsule(half_height: float, radius: float) -> PhysicsShape:
        return PhysicsShape.capsule(half_height, radius)

    @staticmethod
    def cylinder(half_height: float, radius: float) -> PhysicsShape:
        return PhysicsShape.cylinder(half_height, radius)

    @staticmethod
    def trimesh(
        vertices: Iterable[Vec3],
        indices: Iterable[Tuple[int, int, int]],
    ) -> PhysicsShape:
        return PhysicsShape.trimesh(list(vertices), list(indices))


class Scene:
    """对单个 scene 的薄封装。"""

    __slots__ = ("_world", "_scene_id")

    def __init__(self, world: "World", scene_id: int):
        self._world = world
        self._scene_id = scene_id

    @property
    def id(self) -> int:
        return self._scene_id

    @property
    def world(self) -> "World":
        return self._world

    def step(self, dt: float = 0.033) -> None:
        self._world.raw.step(self._scene_id, dt)

    def set_gravity(self, gravity: Vec3) -> None:
        self._world.raw.set_gravity(self._scene_id, gravity)

    def add_dynamic(
        self,
        shape: PhysicsShape,
        position: Vec3 = (0.0, 0.0, 0.0),
        rotation: Quat = (0.0, 0.0, 0.0, 1.0),
        *,
        linvel: Vec3 = (0.0, 0.0, 0.0),
        angvel: Vec3 = (0.0, 0.0, 0.0),
        density: float = 1.0,
        friction: float = 0.5,
        restitution: float = 0.0,
        gravity_scale: float = 1.0,
        can_sleep: bool = True,
        ccd_enabled: bool = False,
        sensor: bool = False,
        events: bool = False,
    ) -> "Body":
        body = PhysicsBody.add_dynamic(
            self._world.raw,
            self._scene_id,
            shape,
            position,
            rotation,
            linvel,
            angvel,
            density,
            friction,
            restitution,
            gravity_scale,
            can_sleep,
            ccd_enabled,
            sensor,
            events,
        )
        return Body(body)

    def add_fixed(
        self,
        shape: PhysicsShape,
        position: Vec3 = (0.0, 0.0, 0.0),
        rotation: Quat = (0.0, 0.0, 0.0, 1.0),
        *,
        friction: float = 0.5,
        restitution: float = 0.0,
        sensor: bool = False,
        events: bool = False,
    ) -> "Body":
        body = PhysicsBody.add_fixed(
            self._world.raw,
            self._scene_id,
            shape,
            position,
            rotation,
            friction,
            restitution,
            sensor,
            events,
        )
        return Body(body)

    def add_kinematic(
        self,
        shape: PhysicsShape,
        position: Vec3 = (0.0, 0.0, 0.0),
        rotation: Quat = (0.0, 0.0, 0.0, 1.0),
        *,
        velocity_based: bool = False,
        events: bool = False,
    ) -> "Body":
        body = PhysicsBody.add_kinematic(
            self._world.raw,
            self._scene_id,
            shape,
            position,
            rotation,
            velocity_based,
            events,
        )
        return Body(body)

    def cast_ray(
        self,
        origin: Vec3,
        direction: Vec3,
        max_toi: float,
        solid: bool = True,
    ) -> Optional[PhysicsRayHit]:
        return _cast_ray(self._world.raw, self._scene_id, origin, direction, max_toi, solid)

    def drain_collision_events(self) -> list[PhysicsCollisionEvent]:
        return self._world.raw.drain_collision_events(self._scene_id)

    def destroy(self) -> bool:
        return self._world.raw.destroy_scene(self._scene_id)


class Body:
    """对 ``PhysicsBody`` 的薄封装，仅做命名与默认值整理。"""

    __slots__ = ("_inner",)

    def __init__(self, inner: PhysicsBody):
        self._inner = inner

    @property
    def raw(self) -> PhysicsBody:
        return self._inner

    @property
    def id(self) -> int:
        return self._inner.id

    @property
    def scene_id(self) -> int:
        return self._inner.scene_id

    def position(self) -> Vec3:
        return self._inner.position()

    def rotation(self) -> Quat:
        return self._inner.rotation()

    def linvel(self) -> Vec3:
        return self._inner.linvel()

    def angvel(self) -> Vec3:
        return self._inner.angvel()

    def set_translation(self, translation: Vec3, wake_up: bool = True) -> bool:
        return self._inner.set_translation(translation, wake_up)

    def set_rotation(self, rotation: Quat, wake_up: bool = True) -> bool:
        return self._inner.set_rotation(rotation, wake_up)

    def set_linvel(self, velocity: Vec3, wake_up: bool = True) -> bool:
        return self._inner.set_linvel(velocity, wake_up)

    def set_angvel(self, velocity: Vec3, wake_up: bool = True) -> bool:
        return self._inner.set_angvel(velocity, wake_up)

    def apply_impulse(self, impulse: Vec3, wake_up: bool = True) -> bool:
        return self._inner.apply_impulse(impulse, wake_up)

    def apply_torque_impulse(self, torque: Vec3, wake_up: bool = True) -> bool:
        return self._inner.apply_torque_impulse(torque, wake_up)

    def remove(self) -> bool:
        return self._inner.remove()

    def is_alive(self) -> bool:
        return self._inner.is_alive()


class World:
    """``PhysicsWorld`` 的薄封装；可通过 :func:`get_world` 拿到进程级单例。"""

    __slots__ = ("_inner",)

    def __init__(self, inner: Optional[PhysicsWorld] = None):
        self._inner = inner if inner is not None else PhysicsWorld()

    @property
    def raw(self) -> PhysicsWorld:
        return self._inner

    def create_scene(self, gravity: Vec3 = (0.0, -9.81, 0.0)) -> Scene:
        scene_id = self._inner.create_scene(gravity)
        return Scene(self, scene_id)

    def destroy_scene(self, scene: Scene) -> bool:
        return self._inner.destroy_scene(scene.id)

    def contains_scene(self, scene_id: int) -> bool:
        return self._inner.contains_scene(scene_id)

    def scene_count(self) -> int:
        return self._inner.scene_count()


_world_singleton: Optional[World] = None


def get_world() -> World:
    """进程级 ``World`` 单例（懒创建）。"""
    global _world_singleton
    if _world_singleton is None:
        _world_singleton = World()
    return _world_singleton


__all__ = [
    "Body",
    "PhysicsCollisionEvent",
    "PhysicsRayHit",
    "Scene",
    "Shape",
    "World",
    "get_world",
]
