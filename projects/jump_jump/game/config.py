"""
跳一跳 (Jump Jump) — 游戏配置。

所有可调参数集中在此模块，便于修改和扩展。
游戏逻辑代码引用 `JumpConfig` 实例获取所有常量值。
"""

from dataclasses import dataclass, field
import json
import os


# ═══════════════════════════════════════════════════════════════════
# 场景清单加载
# ═══════════════════════════════════════════════════════════════════

# 场景清单路径（相对于 game/ 目录）
_SCENE_MANIFEST_PATH = os.path.join(
    os.path.dirname(os.path.abspath(__file__)),
    "..", "assets", "scenes", "default.scene.json",
)


def _load_scene_manifest():
    """读取 default.scene.json 并返回 (ambient, directional_lights) 元组。

    如果文件缺失或格式错误，返回 None 调用方应回退到默认值。
    """
    try:
        with open(_SCENE_MANIFEST_PATH, "r", encoding="utf-8") as f:
            data = json.load(f)
        env = data.get("environment", {})
        ambient = tuple(env.get("ambient", [0.12, 0.12, 0.15]))
        dir_lights = []
        for dl in env.get("directional_lights", []):
            dir_lights.append((
                tuple(dl["direction"]),
                tuple(dl["color"]),
                dl.get("intensity", 1.2),
            ))
        return ambient, dir_lights
    except Exception:
        return None


@dataclass
class JumpConfig:
    """跳一跳游戏全部可调参数。"""

    # ── 物理 ──
    max_charge: float = 8.0
    charge_speed: float = 5.0
    horizontal_force: float = 1.5
    vertical_force: float = 5.0

    # ── 玩家 ──
    player_radius: float = 0.35
    player_half_height: float = 0.5

    # ── 平台 ──
    platform_base_half_size: float = 1.0
    platform_half_height: float = 0.25

    # ── 地面 ──
    ground_half_size: float = 1.2
    ground_half_height: float = 0.1
    ground_y: float = -2.0

    # ── 生成 ──
    min_spawn_dist: float = 2.5
    max_spawn_dist: float = 6.0
    landing_tolerance: float = 0.3
    perfect_landing_dist: float = 0.35
    fall_threshold_y: float = -15.0

    # ── 颜色 ──
    color_ground: tuple = (0.35, 0.35, 0.38)
    color_player: tuple = (1.0, 0.85, 0.1)
    color_platforms: list = field(default_factory=lambda: [
        (0.2, 0.5, 0.9),     # 蓝
        (0.9, 0.25, 0.25),   # 红
        (0.25, 0.7, 0.35),   # 绿
        (0.6, 0.3, 0.8),     # 紫
        (1.0, 0.55, 0.1),    # 橙
    ])

    # ── 光照（正常）──
    ambient: tuple = (0.12, 0.12, 0.15)
    directional_dir: tuple = (-0.3, -1.0, -0.5)
    directional_color: tuple = (1.0, 0.95, 0.85)
    directional_intensity: float = 1.2

    # ── 光照（Game Over）──
    gameover_ambient: tuple = (0.18, 0.06, 0.06)
    gameover_dir_color: tuple = (0.7, 0.3, 0.3)
    gameover_dir_intensity: float = 0.9

    def __post_init__(self):
        """尝试从场景清单同步光照配置。"""
        manifest = _load_scene_manifest()
        if manifest is not None:
            ambient, dir_lights = manifest
            self.ambient = ambient
            if dir_lights:
                d = dir_lights[0]
                self.directional_dir = d[0]
                self.directional_color = d[1]
                self.directional_intensity = d[2]


# 全局默认配置实例
CFG = JumpConfig()
