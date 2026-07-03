"""
编辑器物理引擎 TCP 服务器（RPC 数组协议版）。

为编辑器提供 rapier3d 物理世界的远程访问接口。
通过 TCP + msgpack 通信，协议帧格式与 crates/net::NetPack 兼容：
- 帧格式：4 字节 LE 长度前缀 + msgpack 编码的 body
- 请求 body: msgpack([method_str, args_array])  — 数组格式，与 RPC stubs 对齐
- 响应 body: msgpack([result1, result2, ...])   — 数组格式
  - 成功: [result_fields...]
  - 错误: ["__err__", err_str]

struct 序列化格式与生成的 physics_common_cli.py / physics_common_svr.py 一致：
  - vec3 → {"x": float, "y": float, "z": float}
  - quat → {"x": float, "y": float, "z": float, "w": float}
  - body_snapshot → {"id": str, "position": vec3_dict, "rotation": quat_dict}
  - ray_hit → {"body_id": str, "point": vec3_dict, "normal": vec3_dict}
  - contact_info → {"body1_id": str, "body2_id": str, "point": vec3_dict, "normal": vec3_dict}

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
import sys
import traceback
from typing import Any, Optional

# 导入共享的 euler_to_quat，避免与 server/engine/scene_physics.py 重复实现
_SERVER_ENGINE_DIR = os.path.normpath(
    os.path.join(os.path.dirname(__file__), "..", "..", "..", "server", "engine")
)
if _SERVER_ENGINE_DIR not in sys.path:
    sys.path.insert(0, _SERVER_ENGINE_DIR)
from scene_physics import euler_to_quat as _euler_to_quat  # noqa: E402

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
# struct 序列化工具（与 generated stubs 一致）
# ---------------------------------------------------------------------------

def _vec3_to_dict(x: float, y: float, z: float) -> dict:
    return {"x": float(x), "y": float(y), "z": float(z)}


def _quat_to_dict(x: float, y: float, z: float, w: float) -> dict:
    return {"x": float(x), "y": float(y), "z": float(z), "w": float(w)}


def _body_snapshot_to_dict(body_id: str, pos: tuple, rot: tuple) -> dict:
    return {
        "id": body_id,
        "position": _vec3_to_dict(*pos),
        "rotation": _quat_to_dict(*rot),
    }


def _ray_hit_to_dict(body_id: str, point: tuple, normal: tuple) -> dict:
    return {
        "body_id": body_id,
        "point": _vec3_to_dict(*point),
        "normal": _vec3_to_dict(*normal),
    }


def _contact_to_dict(b1: str, b2: str, point: tuple, normal: tuple) -> dict:
    return {
        "body1_id": b1,
        "body2_id": b2,
        "point": _vec3_to_dict(*point),
        "normal": _vec3_to_dict(*normal),
    }


# ---------------------------------------------------------------------------
# 协议工具（RPC 数组格式）
# ---------------------------------------------------------------------------

_ERR_TAG = "__err__"


def pack_request(method: str, argvs: list) -> bytes:
    """编码请求: [method, args_array] → 4 字节 LE + msgpack。"""
    body = msgpack.dumps([method, argvs], use_bin_type=True)
    return struct.pack("<I", len(body)) + body


def unpack_request(data: bytes) -> tuple[str, list]:
    """解码请求: msgpack → (method, argvs)。"""
    arr = msgpack.loads(data, strict_map_key=False)
    return arr[0], arr[1]


def pack_response(argvs: list) -> bytes:
    """编码成功响应: [result_fields...] → 4 字节 LE + msgpack。"""
    body = msgpack.dumps(argvs, use_bin_type=True)
    return struct.pack("<I", len(body)) + body


def pack_error(err: str) -> bytes:
    """编码错误响应: ["__err__", err_str] → 4 字节 LE + msgpack。"""
    body = msgpack.dumps([_ERR_TAG, err], use_bin_type=True)
    return struct.pack("<I", len(body)) + body


# ---------------------------------------------------------------------------
# 场景碰撞体加载
# ---------------------------------------------------------------------------

# _euler_to_quat 从 server/engine/scene_physics.py 导入（见文件顶部 sys.path 设置）


def _load_scene_collision(manifest_path: str) -> dict[str, PhysicsBody]:
    """解析 .scene.json，为 collision_enabled=True 的模型创建 Fixed 刚体。"""
    global _world, _scene_id

    with open(manifest_path, "r") as f:
        manifest = json.load(f)

    base_dir = os.path.dirname(manifest_path)
    bodies: dict[str, PhysicsBody] = {}

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
            body = PhysicsBody.add_fixed(
                _world, _scene_id, shape, position=translation, rotation=rot
            )
            key = f"{model['id']}_{len(bodies)}"
            bodies[key] = body

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
        body_kind = obj_def.get("body_kind", "fixed")
        if body_kind == "dynamic":
            body = PhysicsBody.add_dynamic(_world, _scene_id, shape, position=pos, rotation=rot)
        else:
            body = PhysicsBody.add_fixed(_world, _scene_id, shape, position=pos, rotation=rot)
        bodies[f"proc_{len(bodies)}"] = body

    return bodies


# ---------------------------------------------------------------------------
# method 处理器 — 返回 list（RPC 数组格式）
# ---------------------------------------------------------------------------

async def handle_init_physics(argvs: list) -> list:
    """argvs: [gravity_x, gravity_y, gravity_z] → rsp: [scene_id]"""
    global _world, _scene_id

    gravity = (0.0, -9.81, 0.0)
    if argvs and len(argvs) >= 3:
        gravity = (float(argvs[0]), float(argvs[1]), float(argvs[2]))

    _world = PhysicsWorld()
    _scene_id = _world.create_scene(gravity)
    return [_scene_id]


async def handle_load_scene(argvs: list) -> list:
    """argvs: [manifest_path] → rsp: [body_count]"""
    global _world, _loaded_bodies

    if _world is None:
        return [_ERR_TAG, "physics not initialized, call init_physics first"]

    manifest_path = argvs[0] if argvs else ""
    if not manifest_path:
        return [_ERR_TAG, "manifest_path is required"]

    try:
        _loaded_bodies = _load_scene_collision(manifest_path)
        return [len(_loaded_bodies)]
    except Exception as e:
        traceback.print_exc()
        return [_ERR_TAG, str(e)]


async def handle_step_physics(argvs: list) -> list:
    """argvs: [dt] → rsp: []"""
    global _world, _scene_id

    if _world is None:
        return [_ERR_TAG, "physics not initialized"]
    dt = float(argvs[0]) if argvs else 0.016
    _world.step(_scene_id, dt)
    return []


async def handle_get_bodies(argvs: list) -> list:
    """argvs: [] → rsp: [body_snapshot_dicts]"""
    global _loaded_bodies

    body_list = []
    for body_id, body in _loaded_bodies.items():
        try:
            pos = body.position()
            rot = body.rotation()
            body_list.append(_body_snapshot_to_dict(body_id, pos, rot))
        except Exception:
            continue
    return [body_list]


async def handle_get_contacts(argvs: list) -> list:
    """argvs: [] → rsp: [contact_info_dicts]

    PyCollisionEvent 字段: a(u64), b(u64), started(bool), sensor(bool)
    无 point/normal — 这里仅传碰撞体句柄和 started 标志。
    """
    global _world, _scene_id

    if _world is None:
        return [_ERR_TAG, "physics not initialized"]

    events = _world.drain_collision_events(_scene_id)
    contacts = []
    for ev in events:
        contacts.append(_contact_to_dict(
            str(ev.a),
            str(ev.b),
            (0.0, 0.0, 0.0),   # PyCollisionEvent 无 point
            (0.0, 0.0, 0.0),   # PyCollisionEvent 无 normal
        ))
    return [contacts]


async def handle_cast_ray(argvs: list) -> list:
    """argvs: [ox, oy, oz, dx, dy, dz, max_toi] → rsp: [ray_hit_dict|null, has_hit]"""
    global _world, _scene_id

    if _world is None:
        return [_ERR_TAG, "physics not initialized"]
    if len(argvs) < 7:
        return [_ERR_TAG, "cast_ray requires 7 args: origin(3) + direction(3) + max_toi"]

    origin = (float(argvs[0]), float(argvs[1]), float(argvs[2]))
    direction = (float(argvs[3]), float(argvs[4]), float(argvs[5]))
    max_toi = float(argvs[6])

    hit = _cast_ray(_world, _scene_id, origin, direction, max_toi, True)
    if hit is None:
        return [None, False]

    # PyRayHit 字段: body(u64), collider(u64), toi(f32), point(tuple), normal(tuple)
    hit_dict = _ray_hit_to_dict(
        str(hit.body),
        hit.point,
        hit.normal,
    )
    return [hit_dict, True]


async def handle_reset_physics(argvs: list) -> list:
    """argvs: [] → rsp: []"""
    global _world, _scene_id, _loaded_bodies

    if _world is not None and _scene_id is not None:
        try:
            _world.destroy_scene(_scene_id)
        except Exception:
            pass
    _world = None
    _scene_id = 0
    _loaded_bodies = {}
    return []


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
    """TCP 协议处理器：4 字节 LE 长度前缀 + msgpack body（RPC 数组格式）。"""

    def __init__(self):
        self._buffer = bytearray()

    def connection_made(self, transport):
        self._transport = transport
        print(f"[physics_server] client connected: {transport.get_extra_info('peername')}")

    def connection_lost(self, exc):
        print("[physics_server] client disconnected")

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
            method, argvs = unpack_request(body)
            handler = _METHODS.get(method)
            if handler is None:
                resp_bytes = pack_error(f"unknown method: {method}")
            else:
                result = await handler(argvs)
                # 检查处理器是否返回了错误标记
                if result and len(result) >= 2 and result[0] == _ERR_TAG:
                    resp_bytes = pack_error(result[1])
                else:
                    resp_bytes = pack_response(result)
            self._transport.write(resp_bytes)

        except Exception as e:
            traceback.print_exc()
            try:
                self._transport.write(pack_error(str(e)))
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
