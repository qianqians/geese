# -*- coding: UTF-8 -*-
"""client.engine.scene：场景 / 八叉树 / GLTF 资源导入 Python 薄封装。

底层由 pyo3 暴露的类（实现位于 ``crates/scene``）：

- :class:`Scene`：场景容器。通过 :meth:`Scene.import_gltf` 导入 ``.gltf``/``.glb`` 文件。
- :class:`SceneNode`：场景节点（值拷贝视图）。
- :class:`SceneObject`：场景对象（mesh 实例，值拷贝视图）。
- :class:`AABB`：轴对齐包围盒。
- :class:`Transform`：T/R/S 三元组。

典型用法::

    from client.engine.scene import Scene
    from client.engine.camera import Frustum

    scene = Scene.import_gltf("assets/level.glb", max_objects=8, max_depth=6)
    print(scene)  # Scene(nodes=..., objects=..., animations=..., skins=...)

    vp = build_view_projection_matrix(...)        # row-major 4x4 list
    frustum = Frustum.from_view_projection(vp)
    visible = scene.visible_objects(frustum)

    for obj in visible:
        print(obj.entity_id, obj.center, obj.aabb.min, obj.aabb.max)

注意：八叉树（Octree）作为场景内部数据结构，不直接暴露——而是通过
``visible_objects(frustum)`` / ``all_objects()`` 等查询接口间接访问。
"""
from __future__ import annotations

from .pyclient import AABB, Scene, SceneNode, SceneObject, Transform

__all__ = ["AABB", "Scene", "SceneNode", "SceneObject", "Transform"]
