include "common.thrift"

/*
 * 三维向量。
 */
struct vec3 {
    1:double x,
    2:double y,
    3:double z,
}

/*
 * 四元数。
 */
struct quat {
    1:double x,
    2:double y,
    3:double z,
    4:double w,
}

/*
 * 单个碰撞体快照（调试渲染用）。
 */
struct body_snapshot {
    1:string id,
    2:vec3 position,
    3:vec3 rotation,
}

/*
 * 射线检测命中结果。
 */
struct ray_hit {
    1:string body_id,
    2:vec3 point,
    3:vec3 normal,
}

// ---------------------------------------------------------------------------
// editor_physics_service 请求消息（编辑器 → 物理服务器）
// ---------------------------------------------------------------------------

/*
 * init_physics 请求。
 * 初始化物理世界。
 */
struct init_physics_req {
    1:vec3 gravity,
}

/*
 * load_scene 请求。
 * 从 .scene.json 加载碰撞体。
 */
struct load_scene_req {
    1:string manifest_path,
}

/*
 * step_physics 请求。
 * 步进一帧物理模拟。
 */
struct step_physics_req {
    1:double dt,
}

/*
 * get_bodies 请求（无参数）。
 */
struct get_bodies_req {
}

/*
 * get_contacts 请求（无参数）。
 */
struct get_contacts_req {
}

/*
 * cast_ray 请求。
 * 从 origin 沿 direction 发射射线，最大检测距离 max_toi。
 */
struct cast_ray_req {
    1:vec3 origin,
    2:vec3 direction,
    3:double max_toi,
}

/*
 * reset_physics 请求（无参数）。
 */
struct reset_physics_req {
}

// ---------------------------------------------------------------------------
// 请求路由 union（编辑器 → 物理服务器）
// ---------------------------------------------------------------------------

union editor_physics_service {
    1:init_physics_req init_physics,
    2:load_scene_req load_scene,
    3:step_physics_req step_physics,
    4:get_bodies_req get_bodies,
    5:get_contacts_req get_contacts,
    6:cast_ray_req cast_ray,
    7:reset_physics_req reset_physics,
}
