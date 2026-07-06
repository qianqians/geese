"""
场景碰撞体加载器。

解析 .scene.json 清单，为 `collision_enabled: true` 的模型从 GLTF 文件
提取三角网格碰撞形状，创建 rapier3d Fixed 刚体注入物理场景。

服务端仅提取几何数据（顶点位置 + 三角形索引），不加载贴图、材质、
法线等渲染数据。
"""
from __future__ import annotations

import json
import math
import msgpack
import os
import uuid
from typing import Any

from pyhub import PhysicsBody, PhysicsShape, PhysicsWorld


def euler_to_quat(yaw_deg: float, pitch_deg: float, roll_deg: float) -> tuple[float, float, float, float]:
    """欧拉角（度）转四元数，顺序 Y * X * Z。"""
    yaw = math.radians(yaw_deg)
    pitch = math.radians(pitch_deg)
    roll = math.radians(roll_deg)

    cy = math.cos(yaw * 0.5)
    sy = math.sin(yaw * 0.5)
    cp = math.cos(pitch * 0.5)
    sp = math.sin(pitch * 0.5)
    cr = math.cos(roll * 0.5)
    sr = math.sin(roll * 0.5)

    x = sr * cp * cy - cr * sp * sy
    y = cr * sp * cy + sr * cp * sy
    z = cr * cp * sy - sr * sp * cy
    w = cr * cp * cy + sr * sp * sy
    return (x, y, z, w)


def load_scene_collision_from_manifest(
    physics_world: PhysicsWorld,
    scene_id: int,
    manifest_path: str,
) -> dict[str, Any]:
    """
    解析 .scene.json，为 `collision_enabled=True` 的模型创建 Fixed 刚体。

    Args:
        physics_world: 物理世界实例
        scene_id: 目标物理场景 ID
        manifest_path: .scene.json 文件的完整路径

    Returns:
        {entity_id: PhysicsBody} 映射表
    """
    with open(manifest_path, "r") as f:
        manifest = json.load(f)

    base_dir = os.path.dirname(manifest_path)
    entity_bodies: dict[str, Any] = {}

    # 1. GLTF 模型 → TriMesh 碰撞体
    for model in manifest.get("models", []):
        if not model.get("collision_enabled", False):
            continue

        gltf_path = os.path.join(base_dir, model["path"])
        if not os.path.isfile(gltf_path):
            print(f"[scene_physics] GLTF not found: {gltf_path}, skipping")
            continue

        transform = model.get("transform", {})
        translation = tuple(transform.get("translation", [0.0, 0.0, 0.0]))
        rotation_euler = transform.get("rotation", [0.0, 0.0, 0.0])
        rot = euler_to_quat(*rotation_euler)

        # 从 Rust 侧提取碰撞形状
        from pyhub import load_gltf_collision_shapes

        shapes = load_gltf_collision_shapes(gltf_path)
        for shape in shapes:
            body = PhysicsBody.add_fixed(
                physics_world,
                scene_id,
                shape,
                position=translation,
                rotation=rot,
            )
            # 以模型 id + 形状序号为 key
            key = f"{model['id']}_{len(entity_bodies)}"
            entity_bodies[key] = body

    # 2. 程序化对象 → Cuboid 碰撞体
    for obj_def in manifest.get("objects", []):
        obj_type = obj_def.get("object_type", "")
        pos = tuple(obj_def.get("position", [0.0, 0.0, 0.0]))
        scale = tuple(obj_def.get("scale", [1.0, 1.0, 1.0]))
        rot_euler = tuple(obj_def.get("rotation_euler") or [0.0, 0.0, 0.0])
        rot = euler_to_quat(*rot_euler)

        if obj_type == "plane":
            shape = PhysicsShape.cuboid(scale[0] * 0.5, 0.01, scale[2] * 0.5)
        elif obj_type == "cube":
            shape = PhysicsShape.cuboid(
                scale[0] * 0.5, scale[1] * 0.5, scale[2] * 0.5
            )
        else:
            continue

        body_kind = obj_def.get("body_kind", "fixed")
        if body_kind == "dynamic":
            body = PhysicsBody.add_dynamic(
                physics_world, scene_id, shape, position=pos, rotation=rot
            )
        else:
            body = PhysicsBody.add_fixed(
                physics_world, scene_id, shape, position=pos, rotation=rot
            )
        entity_bodies[f"proc_{len(entity_bodies)}"] = body

    return entity_bodies


# Entity type 常量，与 crates/scene/src/net.rs 的 ENTITY_TYPE_STATIC 一致。
ENTITY_TYPE_STATIC = "scene_object_static"
ENTITY_TYPE_DYNAMIC = "scene_object_dynamic"


def sync_scene_to_group(
    manifest_path: str,
    group,  # group 实例
    gate_name: str,
    conn_ids: list[str],
) -> list[str]:
    """
    将 .scene.json 清单中的静态场景对象同步给指定 group 的客户端。

    首次调用时生成 entity_id 并缓存到 ``group._scene_cache``;
    后续调用（如 join 重同步）复用缓存，保证同一 scene 对象的 entity_id 一致。

    Args:
        manifest_path: .scene.json 文件路径
        group: group 实例（会在此 group 上缓存场景数据）
        gate_name: 目标 gate 名称
        conn_ids: 目标客户端 conn_id 列表

    Returns:
        创建的远程 entity_id 列表
    """
    # 利用 group 缓存：已解析过的场景无需重新生成 entity_id
    if hasattr(group, "_scene_cache") and group._scene_cache is not None:
        cached = group._scene_cache  # dict[str, bytes]
        for entity_id, msg_bytes in cached.items():
            from app import app
            app().ctx.hub_call_client_create_remote_entity(
                gate_name,
                False,  # is_migrate
                conn_ids,
                "",  # main_conn_id
                entity_id,
                ENTITY_TYPE_STATIC,
                msg_bytes,
            )
        return list(cached.keys())

    # 首次同步：生成 entity_id 并构造 msgpack
    with open(manifest_path, "r") as f:
        manifest = json.load(f)

    base_dir = os.path.dirname(manifest_path)
    entity_ids: list[str] = []
    cache: dict[str, bytes] = {}

    # 1. GLTF 模型 → SceneObjectNetMsg.mesh_ref
    for model in manifest.get("models", []):
        entity_id = str(uuid.uuid4())
        transform = model.get("transform", {})
        translation = transform.get("translation", [0.0, 0.0, 0.0])
        rotation = transform.get("rotation", [0.0, 0.0, 0.0])
        scale = transform.get("scale", [1.0, 1.0, 1.0])

        msg = {
            "entity_id": entity_id,
            "type": "mesh_ref",
            "transform": {
                "translation": translation,
                "rotation": rotation,
                "scale": scale,
            },
            "mesh_ref": {
                "gltf_path": model["path"],
                "mesh_name": None,
            },
        }

        msg_bytes = msgpack.dumps(msg)
        cache[entity_id] = msg_bytes

        from app import app
        app().ctx.hub_call_client_create_remote_entity(
            gate_name,
            False,  # is_migrate
            conn_ids,
            "",  # main_conn_id（场景对象无主连接）
            entity_id,
            ENTITY_TYPE_STATIC,
            msg_bytes,
        )
        entity_ids.append(entity_id)

    # 2. 程序化对象 → SceneObjectNetMsg.procedural
    for obj_def in manifest.get("objects", []):
        entity_id = str(uuid.uuid4())
        obj_type = obj_def.get("object_type", "")
        if obj_type not in ("plane", "cube"):
            continue

        pos = obj_def.get("position", [0.0, 0.0, 0.0])
        scale = obj_def.get("scale", [1.0, 1.0, 1.0])
        rot_euler = obj_def.get("rotation_euler") or [0.0, 0.0, 0.0]
        color = obj_def.get("color", [0.5, 0.5, 0.5])

        msg = {
            "entity_id": entity_id,
            "type": obj_type,
            "transform": {
                "translation": pos,
                "rotation": rot_euler,
                "scale": scale,
            },
            "color": color,
            "dimensions": scale,
        }

        msg_bytes = msgpack.dumps(msg)
        cache[entity_id] = msg_bytes

        from app import app
        app().ctx.hub_call_client_create_remote_entity(
            gate_name,
            False,  # is_migrate
            conn_ids,
            "",  # main_conn_id（场景对象无主连接）
            entity_id,
            ENTITY_TYPE_STATIC,
            msg_bytes,
        )
        entity_ids.append(entity_id)

    # 记录到 group，后续 join/leave 时自动管理
    group._scene_cache = cache
    group.scene_object_ids = set(cache.keys())
    group.scene_manifest_path = manifest_path

    return entity_ids


# ---- 脏标记（dirty-flag）常量，与 Rust 侧 DirtyFlags 一致 ----
DIRTY_TRANSFORM = 0x01
DIRTY_MESH = 0x02


def _quat_to_euler(x: float, y: float, z: float, w: float) -> tuple[float, float, float]:
    """四元数转欧拉角（度）用于网络传输。（简化版，仅 Y-X-Z 顺序）"""
    sinr_cosp = 2.0 * (w * x + y * z)
    cosr_cosp = 1.0 - 2.0 * (x * x + y * y)
    roll = math.degrees(math.atan2(sinr_cosp, cosr_cosp))

    sinp = 2.0 * (w * y - z * x)
    if abs(sinp) >= 1.0:
        pitch = math.degrees(math.copysign(math.pi / 2.0, sinp))
    else:
        pitch = math.degrees(math.asin(sinp))

    siny_cosp = 2.0 * (w * z + x * y)
    cosy_cosp = 1.0 - 2.0 * (y * y + z * z)
    yaw = math.degrees(math.atan2(siny_cosp, cosy_cosp))

    return (yaw, pitch, roll)


def flush_scene_dirty(
    scene,  # PyScene 实例
    group,  # group 实例
    app_ctx,  # app().ctx
) -> None:
    """
    轮询 Rust 侧脏对象并下发 Thrift create_remote_entity / refresh_entity / delete_remote_entity。

    用于运行时的场景对象（add_dynamic_object / update_object_transform）
    增量同步。由服务端主循环按 tick 或操作后立即调用。

    - 已删除对象 → ``delete_remote_entity``（全组广播）
    - 首次出现的脏对象 → ``create_remote_entity``（batch conn_ids per gate）
    - 已存在的脏对象 → ``refresh_entity``（逐客户端）

    Args:
        scene: pyo3 ``PyScene`` 实例（需持有 ``collect_dirty_objects`` /
               ``drain_deleted_ids`` / ``get_object_transform`` 方法）。
        group: 目标 group 实例
        app_ctx: ``app().ctx`` (HubContext wrapper)
    """
    # 1. 已删除对象 → delete_remote_entity（全组广播）
    for entity_id in scene.drain_deleted_ids():
        group.scene_object_ids.discard(entity_id)
        cache = getattr(group, "_scene_cache", {})
        if cache:
            cache.pop(entity_id, None)
        group.dynamic_cache.pop(entity_id, None)
        for gate_name, _conn_id in group.clients:
            app_ctx.hub_call_client_delete_remote_entity(gate_name, entity_id)

    # 2. 脏对象：首次出现 → create_remote_entity；已存在 → refresh_entity
    #    按 gate_name 聚合 conn_ids，batch 发送 create。
    gate_conns: dict[str, list[str]] = {}
    for gate_name, conn_id in group.clients:
        gate_conns.setdefault(gate_name, []).append(conn_id)

    for entity_id, flags in scene.collect_dirty_objects():
        transform = scene.get_object_transform(entity_id)
        if transform is None:
            continue
        translation, rotation_quat, scale = transform
        euler = _quat_to_euler(*rotation_quat)
        msg = {
            "entity_id": entity_id,
            "type": "mesh_ref",  # 运行时对象固定为 mesh_ref
            # TODO: mesh_ref 字段（gltf_path/mesh_name）当前缺失，
            # 因为 Rust SceneObject 不存储来源路径元数据。客户端目前只存
            # 状态不渲染，短期无影响；后续需在 add_dynamic_object 时记录
            # 路径到 group，创建时从 group 查询补全字段。
            "transform": {
                "translation": list(translation),
                "rotation": list(euler),
                "scale": list(scale),
            },
        }
        msg_bytes = msgpack.dumps(msg)

        is_new = entity_id not in group.scene_object_ids
        if is_new:
            group.scene_object_ids.add(entity_id)
            group.dynamic_cache[entity_id] = msg_bytes
            # 首次同步 → create_remote_entity（batch conn_ids）
            for gate_name, conn_ids in gate_conns.items():
                app_ctx.hub_call_client_create_remote_entity(
                    gate_name,
                    False,  # is_migrate
                    conn_ids,
                    "",  # main_conn_id
                    entity_id,
                    ENTITY_TYPE_DYNAMIC,
                    msg_bytes,
                )
        else:
            # 已有对象 → refresh_entity（逐客户端）
            for gate_name, conn_id in group.clients:
                app_ctx.hub_call_client_refresh_entity(
                    gate_name,
                    False,  # is_migrate
                    conn_id,
                    False,  # is_main
                    entity_id,
                    ENTITY_TYPE_DYNAMIC,
                    msg_bytes,
                )
