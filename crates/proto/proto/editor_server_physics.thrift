include "editor_physics.thrift"

// ---------------------------------------------------------------------------
// editor_physics_service 响应消息（物理服务器 → 编辑器）
// ---------------------------------------------------------------------------

/*
 * init_physics 响应。
 */
struct init_physics_rsp {
    1:i64 scene_id,
    2:string error,
}

/*
 * load_scene 响应。
 */
struct load_scene_rsp {
    1:i64 body_count,
    2:string error,
}

/*
 * step_physics 响应。
 */
struct step_physics_rsp {
    1:string error,
}

/*
 * get_bodies 响应。
 * 返回所有碰撞体快照。
 */
struct get_bodies_rsp {
    1:list<editor_physics.body_snapshot> bodies,
    2:string error,
}

/*
 * get_contacts 响应。
 * 返回本帧碰撞事件。
 */
struct get_contacts_rsp {
    1:binary contacts,
    2:string error,
}

/*
 * cast_ray 响应。
 */
struct cast_ray_rsp {
    1:editor_physics.ray_hit hit,
    2:string error,
}

/*
 * reset_physics 响应。
 */
struct reset_physics_rsp {
    1:string error,
}

// ---------------------------------------------------------------------------
// 响应路由 union（物理服务器 → 编辑器）
// ---------------------------------------------------------------------------

union editor_server_physics_service {
    1:init_physics_rsp init_physics,
    2:load_scene_rsp load_scene,
    3:step_physics_rsp step_physics,
    4:get_bodies_rsp get_bodies,
    5:get_contacts_rsp get_contacts,
    6:cast_ray_rsp cast_ray,
    7:reset_physics_rsp reset_physics,
}
