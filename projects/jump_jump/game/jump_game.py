"""
跳一跳 (Jump Jump) — Python 游戏逻辑。

通过 py_engine.EngineBridge 调用引擎 API。
Rust 主循环每帧调用 update(bridge, dt) 获取游戏状态。

重构说明：
- 所有可调参数集中在 config.py 的 JumpConfig 中
- 静态场景定义（地面/光照）复用 default.scene.json 清单
- init/restart 共享 _build_initial_scene 消除重复
- 光照仅在 game_over 状态变化时更新
"""

import math
import random

from config import CFG


# ═══════════════════════════════════════════════════════════════════
# 平台数据
# ═══════════════════════════════════════════════════════════════════

class Platform:
    __slots__ = ("body_handle", "entity_id", "center", "half_extents")

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

        # 预构建的网格 / 材质索引
        self._ground_mesh_idx = None
        self._player_mesh_idx = None
        self._mat_ground = None
        self._mat_player = None
        self._platform_materials = []
        self._was_game_over = None

    # ────────────────────────────────────────────────────────
    # 延迟初始化
    # ────────────────────────────────────────────────────────

    def _ensure_init(self, bridge):
        """首次调用时初始化材质、网格、场景。"""
        if self._initialized:
            return
        self._initialized = True

        self._create_materials(bridge)
        self._build_meshes(bridge)
        self._build_initial_scene(bridge)

    # ────────────────────────────────────────────────────────
    # 材质与网格（仅创建一次）
    # ────────────────────────────────────────────────────────

    def _create_materials(self, bridge):
        """创建所有 PBR 材质（注册到全局 MATERIAL_REGISTRY）。"""
        self._mat_ground = bridge.material_create(
            "ground", *CFG.color_ground, 0.0, 0.8)
        self._mat_player = bridge.material_create(
            "player", *CFG.color_player, 0.0, 0.6)
        self._platform_materials = [
            bridge.material_create(f"plat_{i}", *c, 0.0, 0.7)
            for i, c in enumerate(CFG.color_platforms)
        ]

    def _build_meshes(self, bridge):
        """预构建网格（注册到全局 MESH_REGISTRY）。"""
        # 地面使用 plane（与 default.scene.json 保持一致）
        self._ground_mesh_idx = bridge.build_plane(
            CFG.ground_half_size * 2, CFG.ground_half_size * 2,
            self._mat_ground)
        # 玩家使用 cube
        self._player_mesh_idx = bridge.build_cube(
            1.0, 1.0, 1.0, self._mat_player)

    # ────────────────────────────────────────────────────────
    # 初始场景（init 与 restart 共享）
    # ────────────────────────────────────────────────────────

    def _build_initial_scene(self, bridge):
        """创建地面 + 光照 + 起始平台 + 首个目标平台 + 玩家。

        材质和网格索引已在 _ensure_init 中创建，此处仅创建
        场景对象和物理体。
        """
        # ── 光照 ──
        bridge.light_clear()
        bridge.light_set_ambient(*CFG.ambient)
        bridge.light_add_directional(
            *CFG.directional_dir, *CFG.directional_color,
            CFG.directional_intensity)

        # ── 地面（仅首次创建，重启不重建）──
        self._create_ground(bridge)

        # ── 平台 + 玩家（init 与 restart 共享）──
        self._build_gameplay_entities(bridge)

        bridge.scene_update_transforms()

    def _create_ground(self, bridge):
        """创建地面场景对象和物理体。"""
        ground_bh, _ = bridge.physics_add_body(
            "fixed", 0.0, CFG.ground_y, 0.0, 1.0,
            "cuboid", {
                "hx": CFG.ground_half_size,
                "hy": CFG.ground_half_height,
                "hz": CFG.ground_half_size,
            },
        )
        ground_eid = bridge.scene_add_static_with_material(
            self._ground_mesh_idx, self._mat_ground,
            0.0, CFG.ground_y, 0.0,
            CFG.ground_half_size * 2, CFG.ground_half_height * 2,
            CFG.ground_half_size * 2,
        )
        self.platforms = [Platform(
            ground_bh, ground_eid,
            0.0, CFG.ground_y, 0.0,
            CFG.ground_half_size, CFG.ground_half_height,
            CFG.ground_half_size,
        )]

    def _build_gameplay_entities(self, bridge, create_player=True):
        """创建起始平台 + 首个目标平台，可选创建玩家。

        Args:
            create_player: True=首次初始化（创建玩家节点），
                           False=重启（仅重置已有玩家位置）。
        """
        start_half = (
            CFG.platform_base_half_size,
            CFG.platform_half_height,
            CFG.platform_base_half_size,
        )

        # ── 起始平台 ──
        p = self._spawn_platform(
            bridge, (0, 0, 0), start_half, self._platform_materials[0])
        self.platforms.append(p)
        self.current_platform_idx = 1

        # ── 首个目标平台 ──
        dist = random.uniform(CFG.min_spawn_dist, CFG.max_spawn_dist)
        next_pos = (0.0, 0.0, dist)
        next_half = self._random_platform_size()
        p2 = self._spawn_platform(
            bridge, next_pos, next_half, self._platform_materials[1])
        self.platforms.append(p2)
        self.last_platform_center = next_pos

        # ── 玩家 ──
        player_y = (CFG.player_half_height + CFG.player_radius
                    + start_half[1])
        if create_player:
            self.player_body_handle, _ = bridge.create_player(
                0.0, player_y, 0.0,
                CFG.player_half_height, CFG.player_radius)

            ps = CFG.player_radius * 2.0
            ph = CFG.player_half_height * 2.0 + CFG.player_radius * 2.0
            _, self.player_node_index = bridge.scene_add_dynamic_with_material(
                self._player_mesh_idx, self._mat_player,
                0.0, player_y, 0.0, ps, ph, ps)
        else:
            bridge.physics_set_translation(
                self.player_body_handle, 0.0, player_y, 0.0)
            bridge.physics_set_linvel(
                self.player_body_handle, 0.0, 0.0, 0.0)

    # ────────────────────────────────────────────────────────
    # 每帧更新（由 Rust 调用）
    # ────────────────────────────────────────────────────────

    def update(self, bridge, dt: float):
        self._ensure_init(bridge)
        self._update_lighting(bridge)

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
            self.charge_power = min(
                self.charge_power + dt * CFG.charge_speed, CFG.max_charge)
            squish = 1.0 - (self.charge_power / CFG.max_charge) * 0.55
            bridge.node_set_scale_y(self.player_node_index, squish)
        else:
            cur = bridge.node_get_scale_y(self.player_node_index)
            bridge.node_set_scale_y(self.player_node_index,
                                    cur + (1.0 - cur) * 10.0 * dt)

        # ── 跳跃冲量 ──
        if self.jump_pending:
            self.jump_pending = False
            p = self.charge_power
            dx = math.sin(self.jump_direction_angle) * p * CFG.horizontal_force
            dz = math.cos(self.jump_direction_angle) * p * CFG.horizontal_force
            bridge.physics_apply_impulse(self.player_body_handle,
                                         dx, p * CFG.vertical_force, dz)
            self.charge_power = 0.0

        # ── 地面检测 ──
        self.grounded = bridge.physics_check_ground(
            self.player_body_handle,
            CFG.player_half_height, CFG.player_radius)

        # ── 物理步进 ──
        bridge.physics_step(dt)

        # ── 同步玩家渲染 ──
        px, py, pz = bridge.physics_get_position(self.player_body_handle)
        bridge.node_set_translation(self.player_node_index, px, py, pz)

        # ── 着陆检测 ──
        self._check_landing(bridge, px, py, pz)

        # ── 跌落检测 ──
        if py < CFG.fall_threshold_y:
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
        player_bottom = py - (CFG.player_half_height + CFG.player_radius)

        for i in range(1, len(self.platforms)):
            plat = self.platforms[i]
            plat_top = plat.center[1] + plat.half_extents[1]

            in_x = abs(px - plat.center[0]) < (
                plat.half_extents[0] + CFG.player_radius * 0.5)
            in_z = abs(pz - plat.center[2]) < (
                plat.half_extents[2] + CFG.player_radius * 0.5)
            near_top = abs(player_bottom - plat_top) < CFG.landing_tolerance

            if in_x and in_z and near_top and player_bottom >= plat_top - 0.05:
                if not self.grounded_was and self.grounded:
                    if i != self.current_platform_idx:
                        dist_to_center = math.sqrt(
                            (px - plat.center[0]) ** 2 +
                            (pz - plat.center[2]) ** 2)

                        if dist_to_center < CFG.perfect_landing_dist:
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
        if self.current_platform_idx >= len(self.platforms):
            self.current_platform_idx = len(self.platforms) - 1

    def _spawn_next(self, bridge):
        difficulty = 1.0 + self.score * 0.04
        dist = random.uniform(
            CFG.min_spawn_dist * difficulty,
            CFG.max_spawn_dist * difficulty)
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
        self.jump_direction_angle = random.uniform(-0.6, 0.6)

        plat = self._spawn_platform(
            bridge, next_center, scaled,
            random.choice(self._platform_materials))
        self.platforms.append(plat)

    def _spawn_platform(self, bridge, center, half_extents, material_idx):
        """统一平台创建：网格 → 场景对象 → 物理体。返回 Platform。"""
        w, h, d = half_extents[0] * 2, half_extents[1] * 2, half_extents[2] * 2
        mesh = bridge.build_cube(w, h, d, material_idx)
        bh, eid = self._add_platform(bridge, mesh, center, half_extents)
        return Platform(bh, eid, *center, *half_extents)

    # ────────────────────────────────────────────────────────
    # 重新开始
    # ────────────────────────────────────────────────────────

    def _restart(self, bridge):
        # 清除非地面平台
        for plat in self.platforms[1:]:
            bridge.scene_remove_object(plat.entity_id)
            bridge.physics_remove_body(plat.body_handle)
        self.platforms = self.platforms[:1]

        # 重置状态
        self.score = 0
        self.combo_count = 0
        self.game_over = False
        self.is_charging = False
        self.charge_power = 0.0
        self.jump_pending = False
        self.request_restart = False
        self.grounded_was = False
        self.grounded = False

        # 重建平台 + 重置玩家（地面保留）
        self._build_gameplay_entities(bridge, create_player=False)
        bridge.scene_update_transforms()

    # ────────────────────────────────────────────────────────
    # 光照管理（仅在状态变化时更新）
    # ────────────────────────────────────────────────────────

    def _update_lighting(self, bridge):
        """仅在 game_over 状态变化时更新光照。"""
        if self._was_game_over == self.game_over:
            return
        self._was_game_over = self.game_over

        bridge.light_clear()
        if self.game_over:
            bridge.light_set_ambient(*CFG.gameover_ambient)
            bridge.light_add_directional(
                *CFG.directional_dir, *CFG.gameover_dir_color,
                CFG.gameover_dir_intensity)
        else:
            bridge.light_set_ambient(*CFG.ambient)
            bridge.light_add_directional(
                *CFG.directional_dir, *CFG.directional_color,
                CFG.directional_intensity)

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
            CFG.platform_base_half_size * random.uniform(0.6, 1.6),
            CFG.platform_half_height,
            CFG.platform_base_half_size * random.uniform(0.6, 1.6),
        )

    def _state(self, bridge):
        """返回 Rust 需要的帧末状态元组。"""
        px, py, pz = bridge.physics_get_position(self.player_body_handle)
        frac = self.charge_power / CFG.max_charge if self.is_charging else 0.0
        return (frac, self.game_over, px, py, pz, self.score)


# ═══════════════════════════════════════════════════════════════════
# 启动入口
# ═══════════════════════════════════════════════════════════════════
if __name__ == "__main__":
    game = JumpGame()

    def on_update(bridge):
        game.update(bridge)
        return game._state(bridge)

    import geese_game
    geese_game.run_loop(on_update)
