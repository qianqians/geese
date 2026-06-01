//! 数学类型别名与互转 helper。
//!
//! rapier3d 0.32 起，核心数学类型迁移到 `glamx`（基于 `glam`），不再使用
//! `nalgebra` 的 `Vector3` / `UnitQuaternion`。本模块统一对内暴露 rapier 的
//! glamx 类型别名，避免上层与第三方数学库耦合。

pub use rapier3d::math::Vec3;
pub use rapier3d::math::{Pose3 as Iso3, Rot3 as Quat};

#[inline]
pub fn vec3_from_tuple(v: (f32, f32, f32)) -> Vec3 {
    Vec3::new(v.0, v.1, v.2)
}

#[inline]
pub fn vec3_to_tuple(v: Vec3) -> (f32, f32, f32) {
    (v.x, v.y, v.z)
}

/// 四元数顺序约定：(x, y, z, w)。
#[inline]
pub fn quat_from_tuple(q: (f32, f32, f32, f32)) -> Quat {
    Quat::from_xyzw(q.0, q.1, q.2, q.3)
}

/// 输出顺序：(x, y, z, w)。
#[inline]
pub fn quat_to_tuple(q: Quat) -> (f32, f32, f32, f32) {
    (q.x, q.y, q.z, q.w)
}

#[inline]
pub fn iso_from_parts(translation: (f32, f32, f32), rotation: (f32, f32, f32, f32)) -> Iso3 {
    Iso3::from_parts(vec3_from_tuple(translation), quat_from_tuple(rotation))
}
