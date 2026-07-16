"""
跳一跳 (Jump Jump) — Python 游戏逻辑。

通过 py_engine.EngineBridge 调用引擎 API。
Rust 主循环每帧调用 update(bridge, dt) 获取游戏状态。
"""

import math
import random

# ═══════════════════════════════════════════════════════════════════
# 常量
# ═══════════════════════════════════════════════════════════════════

MAX_CHARGE = 8.0
CHARGE_SPEED = 5.0
HORIZONTAL_FORCE = 1.5
VERTICAL_FORCE = 5.0

PLAYER_RADIUS = 0.35
PLAYER_HALF_HEIGHT = 0.5

PLATFORM_BASE_HALF_SIZE = 1.0
PLATFORM_HALF_HEIGHT = 0.25
GROUND_HALF_SIZE = 1.2
GROUND_HALF_HEIGHT = 0.1
GROUND_Y = -2.0

MIN_SPAWN_DIST = 2.5
MAX_SPAWN_DIST = 6.0
LANDING_TOLERANCE = 0.3
PERFECT_LANDING_DIST = 0.35
FALL_THRESHOLD_Y = -15.0


# ═══════════════════════════════════════════════════════════════════
# 平台数据
# ═══════════════════════════════════════════════════════════════════

class Platform:
    def __init__(self, body_handle: int, entity_id: str,
                 cx: float, cy: float, cz: float,
                 hx: float, hy: float, hz: float):
        self.body_handle = body_handle
        self.entity_id = entity_id
        self.center = (cx, cy, cz)
        self.half_extents = (hx, hy, hz)


# ═══════════════════════════════════════════════════════════════════
# 游戏主类
# ═══════════════════════════════════════════════════════════════════

class JumpGame:
    def __init__(self):
        self._initialized = False
        self.platforms: list[Platform] = []
        self.current_platform_idx = 1
        self.last_platform_center = (0.0, 0.0, 0.0)

        self.is_charging = False
        self.charge_power = 0.0
        self.jump_pending = False

        self.score = 0
        self.game_over = False
        self.combo_count = 0

        self.request_restart = False
        self.grounded_was = False
        self.grounded = False
        self.jump_direction_angle = 0.0

        # 玩家物理 / 节点
        self.player_body_handle = 0
        self.player_node_index = 0

        # 预构建的网格索引（由 build_cube 返回）
        self._ground_mesh_idx = None
        self._player_mesh_idx = None

    # ────────────────────────────────────────────────────────
    # 延迟初始化（需要 bridge）
    # ────────────────────────────────────────────────────────

    def _ensure_init(self, bridge):
        if self._initialized:
            return
        self._initialized = True

        # 预构建常用网格
        self._ground_mesh_idx = bridge.build_cube(1.0, 1.0, 1.0, 0)
        self._player_mesh_idx = bridge.build_cube(1.0, 1.0, 1.0, 1)

        # ── 1. 地面 ──
        ground_bh, _ = bridge.physics_add_body(
            "fixed", 0.0, GROUND_Y, 0.0, 1.0,
            "cuboid", {"hx": GROUND_HALF_SIZE, "hy": GROUND_HALF_HEIGHT, "hz": GROUND_HALF_SIZE},
        )
        ground_eid = bridge.scene_add_static_with_material(
            self._ground_mesh_idx, 0,
            0.0, GROUND_Y, 0.0,
            GROUND_HALF_SIZE * 2.0, GROUND_HALF_HEIGHT * 2.0, GROUND_HALF_SIZE * 2.0,
        )
        self.platforms.append(Platform(
            ground_bh, ground_eid,
            0.0, GROUND_Y, 0.0,
            GROUND_HALF_SIZE, GROUND_HALF_HEIGHT, GROUND_HALF_SIZE,
        ))

        # ── 2. 起始平台 ──
        start_half = (PLATFORM_BASE_HALF_SIZE, PLATFORM_HALF_HEIGHT, PLATFORM_BASE_HALF_SIZE)
        start_mesh = bridge.build_cube(
            start_half[0] * 2, start_half[1] * 2, start_half[2] * 2, 2)
        start_bh, start_eid = self._add_platform(bridge, start_mesh, (0, 0, 0), start_half)
        self.platforms.append(Platform(start_bh, start_eid, 0, 0, 0, *start_half))

        # ── 3. 第一个目标平台 ──
        dist = random.uniform(MIN_SPAWN_DIST, MAX_SPAWN_DIST)
        next_pos = (0.0, 0.0, dist)
        next_half = self._random_platform_size()
        next_mesh = bridge.build_cube(
            next_half[0] * 2, next_half[1] * 2, next_half[2] * 2, 3)
        next_bh, next_eid = self._add_platform(bridge, next_mesh, next_pos, next_half)
        self.platforms.append(Platform(next_bh, next_eid, *next_pos, *next_half))
        self.last_platform_center = next_pos

        # ── 4. 玩家 ──
        player_y = PLAYER_HALF_HEIGHT + PLAYER_RADIUS + start_half[1]
        self.player_body_handle, _ = bridge.create_player(
            0.0, player_y, 0.0, PLAYER_HALF_HEIGHT, PLAYER_RADIUS)

        # 玩家可视网格（用渲染节点索引同步位置）
        ps = PLAYER_RADIUS * 2.0
        ph = PLAYER_HALF_HEIGHT * 2.0 + PLAYER_RADIUS * 2.0
        _, self.player_node_index = bridge.scene_add_dynamic_with_material(
            self._player_mesh_idx, 1,
            0.0, player_y, 0.0, ps, ph, ps)

        bridge.scene_update_transforms()

    # ────────────────────────────────────────────────────────
    # 每帧更新（由 Rust 调用）
    # 返回 (charge_frac, game_over, px, py, pz, score)
    # ────────────────────────────────────────────────────────

    def update(self, bridge, dt: float):
        self._ensure_init(bridge)

        if self.game_over:
            if bridge.input_key_pressed("R"):
                self.request_restart = True
            if self.request_restart:
                self._restart(bridge)
            return self._state(bridge)

        # ── 输入 ──
        self._handle_input(bridge)

        # ── 蓄力 ──
        if self.is_charging:
            self.charge_power = min(self.charge_power + dt * CHARGE_SPEED, MAX_CHARGE)
            squish = 1.0 - (self.charge_power / MAX_CHARGE) * 0.55
            bridge.node_set_scale_y(self.player_node_index, squish)
        else:
            cur = bridge.node_get_scale_y(self.player_node_index)
            bridge.node_set_scale_y(self.player_node_index,
                                    cur + (1.0 - cur) * 10.0 * dt)

        # ── 跳跃冲量 ──
        if self.jump_pending:
            self.jump_pending = False
            p = self.charge_power
            dx = math.sin(self.jump_direction_angle) * p * HORIZONTAL_FORCE
            dz = math.cos(self.jump_direction_angle) * p * HORIZONTAL_FORCE
            bridge.physics_apply_impulse(self.player_body_handle,
                                         dx, p * VERTICAL_FORCE, dz)
            self.charge_power = 0.0

        # ── 地面检测 & 移动 ──
        self.grounded = bridge.physics_check_ground(
            self.player_body_handle, PLAYER_HALF_HEIGHT, PLAYER_RADIUS)

        # ── 物理步进 ──
        bridge.physics_step(dt)

        # ── 同步玩家渲染 ──
        px, py, pz = bridge.physics_get_position(self.player_body_handle)
        bridge.node_set_translation(self.player_node_index, px, py, pz)

        # ── 着陆检测 ──
        self._check_landing(bridge, px, py, pz)

        # ── 跌落检测 ──
        if py < FALL_THRESHOLD_Y:
            self.game_over = True

        self.grounded_was = self.grounded
        return self._state(bridge)

    # ────────────────────────────────────────────────────────
    # 输入处理
    # ────────────────────────────────────────────────────────

    def _handle_input(self, bridge):
        if bridge.input_key_pressed("Space"):
            self.is_charging = True
            self.charge_power = 0.0

        if bridge.input_key_released("Space") and self.is_charging:
            self.is_charging = False
            if self.charge_power > 0.1:
                self.jump_pending = True

    # ────────────────────────────────────────────────────────
    # 着陆检测
    # ────────────────────────────────────────────────────────

    def _check_landing(self, bridge, px, py, pz):
        player_bottom = py - (PLAYER_HALF_HEIGHT + PLAYER_RADIUS)

        for i in range(1, len(self.platforms)):
            plat = self.platforms[i]
            plat_top = plat.center[1] + plat.half_extents[1]

            in_x = abs(px - plat.center[0]) < plat.half_extents[0] + PLAYER_RADIUS * 0.5
            in_z = abs(pz - plat.center[2]) < plat.half_extents[2] + PLAYER_RADIUS * 0.5
            near_top = abs(player_bottom - plat_top) < LANDING_TOLERANCE

            if in_x and in_z and near_top and player_bottom >= plat_top - 0.05:
                if not self.grounded_was and self.grounded:
                    if i != self.current_platform_idx:
                        dist_to_center = math.sqrt(
                            (px - plat.center[0]) ** 2 +
                            (pz - plat.center[2]) ** 2)

                        if dist_to_center < PERFECT_LANDING_DIST:
                            self.score += 2
                            self.combo_count += 1
                        else:
                            self.score += 1
                            self.combo_count = 0

                        self.current_platform_idx = i
                        self.last_platform_center = plat.center
                        self._cleanup_platforms(bridge)
                        self._spawn_next(bridge)
                break

    # ────────────────────────────────────────────────────────
    # 平台管理
    # ────────────────────────────────────────────────────────

    def _cleanup_platforms(self, bridge):
        keep = {0, self.current_platform_idx}
        to_remove = []
        for i in range(1, len(self.platforms)):
            if i not in keep:
                bridge.scene_remove_object(self.platforms[i].entity_id)
                bridge.physics_remove_body(self.platforms[i].body_handle)
                to_remove.append(i)

        for idx in reversed(to_remove):
            self.platforms.pop(idx)
        # 修正 current_platform_idx
        if self.current_platform_idx >= len(self.platforms):
            self.current_platform_idx = len(self.platforms) - 1

    def _spawn_next(self, bridge):
        difficulty = 1.0 + self.score * 0.04
        dist = random.uniform(MIN_SPAWN_DIST * difficulty,
                              MAX_SPAWN_DIST * difficulty)
        dx = math.sin(self.jump_direction_angle) * dist
        dz = math.cos(self.jump_direction_angle) * dist
        last = self.last_platform_center
        next_center = (last[0] + dx, 0.0, last[2] + dz)

        size = self._random_platform_size()
        scaled = (
            max(size[0] / math.sqrt(difficulty), 0.35),
            size[1],
            max(size[2] / math.sqrt(difficulty), 0.35),
        )

        # 下次跳跃方向 ±0.6 rad
        self.jump_direction_angle = random.uniform(-0.6, 0.6)

        mesh = bridge.build_cube(
            scaled[0] * 2, scaled[1] * 2, scaled[2] * 2,
            random.randint(2, 6))

        eid = bridge.scene_add_static(
            mesh,
            next_center[0], next_center[1], next_center[2],
            1.0, 1.0, 1.0)

        bh, _ = bridge.physics_add_body(
            "fixed",
            next_center[0], next_center[1], next_center[2],
            1.0,
            "cuboid", {"hx": scaled[0], "hy": scaled[1], "hz": scaled[2]})

        self.platforms.append(Platform(bh, eid, *next_center, *scaled))

    # ────────────────────────────────────────────────────────
    # 重新开始
    # ────────────────────────────────────────────────────────

    def _restart(self, bridge):
        # 移除所有非地面平台
        for plat in self.platforms[1:]:
            bridge.scene_remove_object(plat.entity_id)
            bridge.physics_remove_body(plat.body_handle)
        self.platforms = self.platforms[:1]

        # 重建起始平台
        start_half = (PLATFORM_BASE_HALF_SIZE, PLATFORM_HALF_HEIGHT, PLATFORM_BASE_HALF_SIZE)
        start_mesh = bridge.build_cube(
            start_half[0] * 2, start_half[1] * 2, start_half[2] * 2, 2)
        start_bh, start_eid = self._add_platform(bridge, start_mesh, (0, 0, 0), start_half)
        self.platforms.append(Platform(start_bh, start_eid, 0, 0, 0, *start_half))

        # 重建第一个目标
        self.jump_direction_angle = 0.0
        dist = random.uniform(MIN_SPAWN_DIST, MAX_SPAWN_DIST)
        next_pos = (0.0, 0.0, dist)
        next_half = self._random_platform_size()
        next_mesh = bridge.build_cube(
            next_half[0] * 2, next_half[1] * 2, next_half[2] * 2, 3)
        next_bh, next_eid = self._add_platform(bridge, next_mesh, next_pos, next_half)
        self.platforms.append(Platform(next_bh, next_eid, *next_pos, *next_half))

        # 重置玩家
        player_y = PLAYER_HALF_HEIGHT + PLAYER_RADIUS + start_half[1]
        bridge.physics_set_translation(self.player_body_handle, 0.0, player_y, 0.0)
        bridge.physics_set_linvel(self.player_body_handle, 0.0, 0.0, 0.0)

        # 重置状态
        self.current_platform_idx = 1
        self.last_platform_center = next_pos
        self.score = 0
        self.combo_count = 0
        self.game_over = False
        self.is_charging = False
        self.charge_power = 0.0
        self.jump_pending = False
        self.request_restart = False
        self.grounded_was = False
        self.grounded = False

    # ────────────────────────────────────────────────────────
    # 辅助
    # ────────────────────────────────────────────────────────

    def _add_platform(self, bridge, mesh_idx, center, half):
        """创建平台（场景对象 + 物理刚体），返回 (body_handle, entity_id)。"""
        eid = bridge.scene_add_static(
            mesh_idx,
            center[0], center[1], center[2],
            half[0] * 2, half[1] * 2, half[2] * 2)
        bh, _ = bridge.physics_add_body(
            "fixed", center[0], center[1], center[2], 1.0,
            "cuboid", {"hx": half[0], "hy": half[1], "hz": half[2]})
        return bh, eid

    @staticmethod
    def _random_platform_size():
        return (
            PLATFORM_BASE_HALF_SIZE * random.uniform(0.6, 1.6),
            PLATFORM_HALF_HEIGHT,
            PLATFORM_BASE_HALF_SIZE * random.uniform(0.6, 1.6),
        )

    def _state(self, bridge):
        """返回 Rust 需要的帧末状态元组。"""
        px, py, pz = bridge.physics_get_position(self.player_body_handle)
        frac = self.charge_power / MAX_CHARGE if self.is_charging else 0.0
        return (frac, self.game_over, px, py, pz, self.score)
