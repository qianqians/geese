# -*- coding: UTF-8 -*-
"""Physics 冒烟测试。

运行前需先构建 ``pyhub``（cargo build / maturin develop）并把生成的扩展
拷贝到 ``server/engine/pyhub/`` 下。Linux/macOS 使用 ``pyhub.cpython-XYZ.so``，
Windows 使用 ``pyhub.cpYYY-win_amd64.pyd``。

使用：``python -m engine.tests.test_physics``。
"""
from __future__ import annotations

import os
import sys
import unittest

# 让 `python -m engine.tests.test_physics` 与裸跑都能 import engine 包。
_HERE = os.path.dirname(os.path.realpath(__file__))
_SERVER = os.path.dirname(os.path.dirname(_HERE))
if _SERVER not in sys.path:
    sys.path.insert(0, _SERVER)

from engine.physics import Shape, World, get_world  # noqa: E402
from engine.physics_component import PhysicsComponent  # noqa: E402


class PhysicsSmokeTest(unittest.TestCase):
    def test_free_fall(self):
        world = World()
        scene = world.create_scene((0.0, -9.81, 0.0))
        body = scene.add_dynamic(Shape.cuboid(0.5, 0.5, 0.5), position=(0.0, 10.0, 0.0))
        for _ in range(60):
            scene.step(1.0 / 60.0)
        x, y, z = body.position()
        # 解析解 0.5*g*1^2 ≈ 4.905；半隐式积分容差 0.4。
        self.assertAlmostEqual(10.0 - y, 4.905, delta=0.4)
        scene.destroy()

    def test_ball_rests_on_ground(self):
        world = World()
        scene = world.create_scene((0.0, -9.81, 0.0))
        scene.add_fixed(Shape.cuboid(50.0, 0.1, 50.0), position=(0.0, 0.0, 0.0))
        ball = scene.add_dynamic(Shape.ball(0.5), position=(0.0, 5.0, 0.0))
        for _ in range(240):
            scene.step(1.0 / 60.0)
        _, y, _ = ball.position()
        self.assertGreaterEqual(y, 0.5 + 0.1 - 0.05)
        vx, vy, vz = ball.linvel()
        self.assertLess((vx * vx + vy * vy + vz * vz) ** 0.5, 0.5)

    def test_raycast(self):
        world = World()
        scene = world.create_scene((0.0, -9.81, 0.0))
        scene.add_fixed(Shape.cuboid(20.0, 0.1, 20.0), position=(0.0, 0.0, 0.0))
        scene.step(1.0 / 60.0)
        hit = scene.cast_ray((0.0, 5.0, 0.0), (0.0, -1.0, 0.0), 100.0)
        self.assertIsNotNone(hit)
        self.assertAlmostEqual(hit.toi, 5.0 - 0.1, delta=0.05)
        self.assertGreater(hit.normal[1], 0.5)

    def test_component_lifecycle(self):
        world = get_world()
        scene = world.create_scene((0.0, -9.81, 0.0))
        comp = PhysicsComponent(
            scene,
            Shape.ball(0.3),
            kind="dynamic",
            position=(0.0, 1.0, 0.0),
        )
        self.assertTrue(comp.is_alive())
        scene.step(1.0 / 60.0)
        self.assertTrue(comp.destroy())
        self.assertFalse(comp.is_alive())
        scene.destroy()


if __name__ == "__main__":
    unittest.main()
