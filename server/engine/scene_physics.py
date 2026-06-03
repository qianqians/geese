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

        body = PhysicsBody.add_fixed(
            physics_world, scene_id, shape, position=pos, rotation=rot
        )
        entity_bodies[f"proc_{len(entity_bodies)}"] = body

    return entity_bodies


# Entity type 常量，与 crates/scene/src/net.rs 的 ENTITY_TYPE_STATIC 一致。
ENTITY_TYPE_STATIC = "scene_object_static"


def sync_scene_to_group(
    manifest_path: str,
    group,  # group 实例
    gate_name: str,
    conn_ids: list[str],
) -> list[str]:
    """
    将 .scene.json 清单中的静态场景对象同步给指定 group 的客户端。

    对每个模型（collision_enabled 或非 collision_enabled 均可同步渲染），
    构造 ``SceneObjectNetMsg`` JSON 并通过 ``create_remote_entity`` 发送。

    Args:
        manifest_path: .scene.json 文件路径
        group: group 实例
        gate_name: 目标 gate 名称
        conn_ids: 目标客户端 conn_id 列表

    Returns:
        创建的远程 entity_id 列表
    """
    with open(manifest_path, "r") as f:
        manifest = json.load(f)

    base_dir = os.path.dirname(manifest_path)
    entity_ids: list[str] = []

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

        from app import app
        app().ctx.hub_call_client_create_remote_entity(
            gate_name,
            False,  # is_migrate
            conn_ids,
            entity_id,
            ENTITY_TYPE_STATIC,
            json.dumps(msg).encode("utf-8"),
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

        from app import app
        app().ctx.hub_call_client_create_remote_entity(
            gate_name,
            False,
            conn_ids,
            entity_id,
            ENTITY_TYPE_STATIC,
            json.dumps(msg).encode("utf-8"),
        )
        entity_ids.append(entity_id)

    return entity_ids
