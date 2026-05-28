# -*- coding: UTF-8 -*-
"""client.engine.camera：相机/视锥体 Python 薄封装。

底层由 pyo3 暴露的 ``Frustum``/``Plane``（实现位于 ``crates/camera``）。
本模块仅做命名空间转发，便于在 Python 业务侧 ``from client.engine.camera import Frustum``。
"""
from __future__ import annotations

from .pyclient import Frustum, Plane

__all__ = ["Frustum", "Plane"]
