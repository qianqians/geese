"""
物理状态 → entity 系统同步管道。

负责：
- 物理步进后自动同步刚体变换到 entity（通过现有 RPC 推送到客户端）
- 碰撞事件收集与分发
- 射线检测工具

用法：由 :func:`app.build_scene_physics` 自动挂载到主循环，
业务 entity 通过 :class:`PhysicsComponent` 关联刚体后无需额外操作。
"""
from __future__ import annotations

import msgpack
from typing import Any, Callable, Optional
from collections import defaultdict

from .physics import Vec3, Quat, PhysicsRayHit


class PhysicsSyncManager:
    """管理 physics → entity 的自动同步。

    由 ``app()`` 创建，业务代码不直接实例化。
    """

    def __init__(self, app_ctx):
        self._app_ctx = app_ctx
        # entity_id → PhysicsComponent
        self._components: dict[str, Any] = {}
        # 上次记录的位置/旋转，仅 delta 变化时推送
        self._last_transform: dict[str, tuple[Vec3, Quat]] = {}
        # 碰撞事件回调（每个事件 body1_id/body2_id 都调用）
        self._collision_handlers: dict[str, Callable] = {}

    # ---- component 注册 ----

    def attach(self, entity_id: str, component: Any) -> None:
        """注册 entity 的 PhysicsComponent，step 后自动同步。"""
        self._components[entity_id] = component
        try:
            self._last_transform[entity_id] = (
                component.position,
                component.rotation,
            )
        except Exception:
            self._last_transform[entity_id] = ((0.0, 0.0, 0.0), (0.0, 0.0, 0.0, 1.0))

    def detach(self, entity_id: str) -> None:
        """移除 entity 的物理同步。"""
        self._components.pop(entity_id, None)
        self._last_transform.pop(entity_id, None)
        self._collision_handlers.pop(entity_id, None)

    # ---- 碰撞回调 ----

    def on_collision(self, body_id: str, handler: Callable) -> None:
        """注册碰撞事件处理器。``handler(event: PhysicsCollisionEvent)``。"""
        self._collision_handlers[body_id] = handler

    # ---- 步进后同步 (由 app.poll 调用) ----

    def flush_after_step(self, physics_world: Any, scene_id: int) -> None:
        """物理步进后调用：同步刚体变换 + 分发碰撞事件。

        参数:
            physics_world: ``PhysicsWorld`` 实例
            scene_id: 当前物理场景 ID
        """
        # 1. 同步刚体变换（仅变化时推送）
        from .app import app
        _app = app()
        moved_entities: set[str] = set()

        for entity_id, comp in self._components.items():
            if not comp.is_alive():
                self.detach(entity_id)
                continue
            try:
                pos = comp.position
                rot = comp.rotation
            except Exception:
                continue

            last = self._last_transform.get(entity_id)
            if last is None or not _transforms_equal(pos, rot, last[0], last[1]):
                self._last_transform[entity_id] = (pos, rot)
                moved_entities.add(entity_id)

        # 对变化的 entity，刷新到客户端
        if moved_entities:
            from .app import app
            for entity_id in moved_entities:
                entity = app().entity_mgr.get_entity(entity_id)
                if entity is None:
                    continue
                # 通过 entity 的 client_info + refresh_entity 推送最新状态
                try:
                    info_bytes = msgpack.dumps(entity.client_info())
                except Exception:
                    continue
                for gate_name, conn_id in getattr(entity, 'conn_client_gate', []):
                    try:
                        _app._app_ctx.hub_call_client_refresh_entity(
                            gate_name,
                            False,
                            conn_id,
                            False,  # is_main
                            entity_id,
                            entity.entity_type,
                            info_bytes,
                        )
                    except Exception:
                        pass

        # 2. 碰撞事件分发
        try:
            events = physics_world.drain_collision_events(scene_id)
        except Exception:
            return

        for ev in events:
            # 分发到注册了回调的 body
            for body_id_key in (getattr(ev, 'body1_id', None), getattr(ev, 'body2_id', None)):
                if body_id_key and body_id_key in self._collision_handlers:
                    try:
                        self._collision_handlers[body_id_key](ev)
                    except Exception:
                        pass

    # ---- 射线检测（暴露给业务 entity） ----

    def cast_ray(
        self,
        physics_world: Any,
        scene_id: int,
        origin: Vec3,
        direction: Vec3,
        max_toi: float,
        solid: bool = True,
    ) -> Optional[PhysicsRayHit]:
        """委托 pyhub 执行射线检测。"""
        from .physics import _cast_ray
        return _cast_ray(physics_world, scene_id, origin, direction, max_toi, solid)


# ---- 辅助 ----

def _transforms_equal(
    a_pos: Vec3, a_rot: Quat,
    b_pos: Vec3, b_rot: Quat,
    eps: float = 1e-5,
) -> bool:
    return (
        abs(a_pos[0] - b_pos[0]) < eps
        and abs(a_pos[1] - b_pos[1]) < eps
        and abs(a_pos[2] - b_pos[2]) < eps
        and abs(a_rot[0] - b_rot[0]) < eps
        and abs(a_rot[1] - b_rot[1]) < eps
        and abs(a_rot[2] - b_rot[2]) < eps
        and abs(a_rot[3] - b_rot[3]) < eps
    )
