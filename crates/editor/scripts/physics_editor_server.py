"""
编辑器物理引擎 TCP 服务器。

为编辑器提供 rapier3d 物理世界的远程访问接口。
通过 TCP + msgpack 通信，协议与 crates/net::NetPack 兼容：
- 帧格式：4 字节 LE 长度前缀 + msgpack 编码的 body
- 请求 body: msgpack({"method": "...", "argvs": [...]})
- 响应 body: msgpack(response_dict)

启动方式: python3 physics_editor_server.py --port 9000
"""
from __future__ import annotations

import asyncio
import argparse
import json
import math
import msgpack
import os
import struct
import traceback
import uuid
from typing import Any

from pyhub import (
    PhysicsBody,
    PhysicsShape,
    PhysicsWorld,
    cast_ray as _cast_ray,
    load_gltf_collision_shapes,
)

# ---------------------------------------------------------------------------
# 全局状态
# ---------------------------------------------------------------------------
_world: PhysicsWorld | None = None
_scene_id: int = 0
_loaded_bodies: dict[str, PhysicsBody] = {}

# ---------------------------------------------------------------------------
# 协议工具
# ---------------------------------------------------------------------------

def pack_msg(data: dict) -> bytes:
    """编码 msgpack → 4 字节 LE 长度前缀 + body。"""
    body = msgpack.dumps(data, use_bin_type=True)
    return struct.pack("<I", len(body)) + body


def unpack_msg(data: bytes) -> dict:
    """解码 4 字节 LE 长度前缀 + body → msgpack dict。"""
    return msgpack.loads(data, strict_map_key=False)


# ---------------------------------------------------------------------------
# 场景碰撞体加载（内联自 scene_physics.py）
# ---------------------------------------------------------------------------

def _euler_to_quat(yaw_deg: float, pitch_deg: float, roll_deg: float):
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


def _load_scene_collision(
    manifest_path: str,
) -> dict[str, PhysicsBody]:
    """解析 .scene.json，为 collision_enabled=True 的模型创建 Fixed 刚体。"""
    global _world, _scene_id

    with open(manifest_path, "r") as f:
        manifest = json.load(f)

    base_dir = os.path.dirname(manifest_path)
    bodies: dict[str, PhysicsBody] = {}

    # 1. GLTF 模型 → TriMesh / ConvexHull 碰撞体
    for model in manifest.get("models", []):
        if not model.get("collision_enabled", False):
            continue

        gltf_path = os.path.join(base_dir, model["path"])
        if not os.path.isfile(gltf_path):
            print(f"[physics_server] gltf not found: {gltf_path}, skipping")
            continue

        transform = model.get("transform", {})
        translation = tuple(transform.get("translation", [0.0, 0.0, 0.0]))
        rotation_euler = transform.get("rotation", [0.0, 0.0, 0.0])
        rot = _euler_to_quat(*rotation_euler)

        shapes = load_gltf_collision_shapes(gltf_path)
        for shape in shapes:
            body = PhysicsBody.add_fixed(_world, _scene_id, shape, position=translation, rotation=rot)
            key = f"{model['id']}_{len(bodies)}"
            bodies[key] = body

    # 2. 程序化对象 → Cuboid 碰撞体
    for obj_def in manifest.get("objects", []):
        obj_type = obj_def.get("object_type", "")
        pos = tuple(obj_def.get("position", [0.0, 0.0, 0.0]))
        scale = tuple(obj_def.get("scale", [1.0, 1.0, 1.0]))
        rot_euler = tuple(obj_def.get("rotation_euler") or [0.0, 0.0, 0.0])
        rot = _euler_to_quat(*rot_euler)

        if obj_type == "plane":
            shape = PhysicsShape.cuboid(scale[0] * 0.5, 0.01, scale[2] * 0.5)
        elif obj_type == "cube":
            shape = PhysicsShape.cuboid(scale[0] * 0.5, scale[1] * 0.5, scale[2] * 0.5)
        else:
            continue

        body = PhysicsBody.add_fixed(_world, _scene_id, shape, position=pos, rotation=rot)
        bodies[f"proc_{len(bodies)}"] = body

    return bodies


# ---------------------------------------------------------------------------
# method 处理器
# ---------------------------------------------------------------------------

async def handle_init_physics(argvs: list) -> dict:
    """argvs: [(double)gravity_x, (double)gravity_y, (double)gravity_z]"""
    global _world, _scene_id

    gravity = (0.0, -9.81, 0.0)
    if argvs and len(argvs) >= 3:
        gravity = (float(argvs[0]), float(argvs[1]), float(argvs[2]))

    _world = PhysicsWorld()
    _scene_id = _world.create_scene(gravity)
    return {"scene_id": _scene_id, "error": ""}


async def handle_load_scene(argvs: list) -> dict:
    """argvs: [(str)manifest_path]"""
    global _world, _loaded_bodies

    if _world is None:
        return {"body_count": 0, "error": "physics not initialized, call init_physics first"}

    manifest_path = argvs[0] if argvs else ""
    if not manifest_path:
        return {"body_count": 0, "error": "manifest_path is required"}

    try:
        _loaded_bodies = _load_scene_collision(manifest_path)
        return {"body_count": len(_loaded_bodies), "error": ""}
    except Exception as e:
        traceback.print_exc()
        return {"body_count": 0, "error": str(e)}


async def handle_step_physics(argvs: list) -> dict:
    """argvs: [(double)dt]"""
    global _world, _scene_id

    if _world is None:
        return {"error": "physics not initialized"}
    dt = float(argvs[0]) if argvs else 0.016
    _world.step(_scene_id, dt)
    return {"error": ""}


async def handle_get_bodies(argvs: list) -> dict:
    """无参数。"""
    global _loaded_bodies
    body_list = []
    for body_id, body in _loaded_bodies.items():
        try:
            pos = body.position()
            rot = body.rotation()
            body_list.append({
                "id": body_id,
                "position": {"x": pos[0], "y": pos[1], "z": pos[2]},
                "rotation": {"x": rot[0], "y": rot[1], "z": rot[2], "w": rot[3]},
            })
        except Exception:
            continue
    return {"bodies": body_list, "error": ""}


async def handle_get_contacts(argvs: list) -> dict:
    """无参数。"""
    global _world, _scene_id

    if _world is None:
        return {"contacts": [], "error": "physics not initialized"}

    events = _world.drain_collision_events(_scene_id)
    contacts = []
    for ev in events:
        contacts.append({
            "body1_id": ev.body1_id if hasattr(ev, 'body1_id') else "",
            "body2_id": ev.body2_id if hasattr(ev, 'body2_id') else "",
            "point": {
                "x": ev.point[0] if hasattr(ev, 'point') else 0.0,
                "y": ev.point[1] if hasattr(ev, 'point') else 0.0,
                "z": ev.point[2] if hasattr(ev, 'point') else 0.0,
            },
            "normal": {
                "x": ev.normal[0] if hasattr(ev, 'normal') else 0.0,
                "y": ev.normal[1] if hasattr(ev, 'normal') else 0.0,
                "z": ev.normal[2] if hasattr(ev, 'normal') else 0.0,
            },
        })
    return {"contacts": contacts, "error": ""}


async def handle_cast_ray(argvs: list) -> dict:
    """argvs: [origin_x, origin_y, origin_z, dir_x, dir_y, dir_z, max_toi]"""
    global _world, _scene_id

    if _world is None:
        return {"hit": None, "error": "physics not initialized"}

    if len(argvs) < 7:
        return {"hit": None, "error": "cast_ray requires 7 args: origin(3) + direction(3) + max_toi"}

    origin = (float(argvs[0]), float(argvs[1]), float(argvs[2]))
    direction = (float(argvs[3]), float(argvs[4]), float(argvs[5]))
    max_toi = float(argvs[6])

    hit = _cast_ray(_world, _scene_id, origin, direction, max_toi, True)
    if hit is None:
        return {"hit": None, "error": ""}

    return {
        "hit": {
            "body_id": hit.body_id if hasattr(hit, 'body_id') else "",
            "point": {
                "x": hit.point[0] if hasattr(hit, 'point') else 0.0,
                "y": hit.point[1] if hasattr(hit, 'point') else 0.0,
                "z": hit.point[2] if hasattr(hit, 'point') else 0.0,
            },
            "normal": {
                "x": hit.normal[0] if hasattr(hit, 'normal') else 0.0,
                "y": hit.normal[1] if hasattr(hit, 'normal') else 0.0,
                "z": hit.normal[2] if hasattr(hit, 'normal') else 0.0,
            },
        },
        "error": "",
    }


async def handle_reset_physics(argvs: list) -> dict:
    """无参数。"""
    global _world, _scene_id, _loaded_bodies

    if _world is not None and _scene_id is not None:
        try:
            _world.destroy_scene(_scene_id)
        except Exception:
            pass
    _world = None
    _scene_id = 0
    _loaded_bodies = {}
    return {"error": ""}


# ---------------------------------------------------------------------------
# 方法路由表
# ---------------------------------------------------------------------------
_METHODS = {
    "init_physics": handle_init_physics,
    "load_scene": handle_load_scene,
    "step_physics": handle_step_physics,
    "get_bodies": handle_get_bodies,
    "get_contacts": handle_get_contacts,
    "cast_ray": handle_cast_ray,
    "reset_physics": handle_reset_physics,
}


# ---------------------------------------------------------------------------
# TCP 协议处理
# ---------------------------------------------------------------------------
class PhysicsProtocol(asyncio.Protocol):
    """TCP 协议处理器：解析 4 字节 LE 长度前缀 + msgpack body。"""

    def __init__(self):
        self._buffer = bytearray()

    def connection_made(self, transport):
        self._transport = transport
        print(f"[physics_server] client connected: {transport.get_extra_info('peername')}")

    def connection_lost(self, exc):
        print(f"[physics_server] client disconnected")

    def data_received(self, data):
        self._buffer.extend(data)

        while len(self._buffer) >= 4:
            body_len = struct.unpack("<I", bytes(self._buffer[:4]))[0]
            total_needed = 4 + body_len
            if len(self._buffer) < total_needed:
                return

            body = bytes(self._buffer[4:total_needed])
            del self._buffer[:total_needed]
            asyncio.create_task(self._handle_request(body))

    async def _handle_request(self, body: bytes):
        try:
            request = unpack_msg(body)
            method = request.get("method", "")
            argvs = request.get("argvs", [])

            handler = _METHODS.get(method)
            if handler is None:
                response = {"error": f"unknown method: {method}"}
            else:
                response = await handler(argvs)

            resp_bytes = pack_msg(response)
            self._transport.write(resp_bytes)

        except Exception as e:
            traceback.print_exc()
            err_resp = pack_msg({"error": str(e)})
            try:
                self._transport.write(err_resp)
            except Exception:
                pass


# ---------------------------------------------------------------------------
# 服务器启动
# ---------------------------------------------------------------------------
async def start_server(host: str = "127.0.0.1", port: int = 9000):
    loop = asyncio.get_event_loop()
    server = await loop.create_server(PhysicsProtocol, host, port)
    print(f"[physics_server] listening on {host}:{port}")
    async with server:
        await server.serve_forever()


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description="Geese Physics Editor Server")
    parser.add_argument("--host", default="127.0.0.1", help="bind host")
    parser.add_argument("--port", type=int, default=9000, help="bind port")
    args = parser.parse_args()
    asyncio.run(start_server(args.host, args.port))
