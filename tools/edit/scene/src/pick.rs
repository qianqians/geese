//! 拖放落点：屏幕坐标 → 相机射线 → 与 y = 0 水平面的交点。
//!
//! 纯数学，不依赖 egui/wgpu，便于单测。NDC 约定为 wgpu：x/y ∈ [-1, 1]，z ∈ [0, 1]。

use cgmath::{InnerSpace, Matrix4, SquareMatrix, Vector3, Vector4};

/// 交点距离上限：射线接近水平时交点会飞到很远，截断在 1000 单位处仍然能落点。
const MAX_DISTANCE: f32 = 1000.0;

/// 视口内的像素坐标 → NDC（左上角原点、y 向下）。
pub fn ndc_from_screen(screen: [f32; 2], viewport: [f32; 2]) -> Option<[f32; 2]> {
    if viewport[0] <= 0.0 || viewport[1] <= 0.0 {
        return None;
    }
    Some([
        screen[0] / viewport[0] * 2.0 - 1.0,
        1.0 - screen[1] / viewport[1] * 2.0,
    ])
}

/// NDC → NDC（反向，供画落点标记用）。
pub fn screen_from_ndc(ndc: [f32; 2], viewport: [f32; 2]) -> [f32; 2] {
    [
        (ndc[0] + 1.0) * 0.5 * viewport[0],
        (1.0 - ndc[1]) * 0.5 * viewport[1],
    ]
}

/// 由 NDC 与相机矩阵求世界空间射线（起点在近平面，方向单位化）。
pub fn ray_from_ndc(ndc: [f32; 2], view_projection: &Matrix4<f32>) -> ([f32; 3], [f32; 3]) {
    let inverse = view_projection
        .invert()
        .unwrap_or_else(Matrix4::identity);

    let near = inverse * Vector4::new(ndc[0], ndc[1], 0.0, 1.0);
    let far = inverse * Vector4::new(ndc[0], ndc[1], 1.0, 1.0);

    let near = if near.w.abs() > f32::EPSILON {
        near / near.w
    } else {
        near
    };
    let far = if far.w.abs() > f32::EPSILON {
        far / far.w
    } else {
        far
    };

    let origin = Vector3::new(near.x, near.y, near.z);
    let direction = (Vector3::new(far.x, far.y, far.z) - origin)
        .normalize_or_zero();
    (origin.into(), direction.into())
}

/// 射线与 y = 0 平面的交点；射线平行于平面或反向时返回 `None`。
///
/// 交点距离超过 [`MAX_DISTANCE`] 时截断在该距离上——拖到地平线附近也能落点，
/// 只是会被放到很远的地方，用户可以再改 JSON。
pub fn intersect_y0(origin: [f32; 3], direction: [f32; 3]) -> Option<[f32; 3]> {
    if direction[1].abs() < 1e-6 {
        return None;
    }
    let t = -origin[1] / direction[1];
    if t <= 0.0 {
        return None;
    }
    let t = t.min(MAX_DISTANCE);
    Some([
        origin[0] + direction[0] * t,
        0.0,
        origin[2] + direction[2] * t,
    ])
}

/// 视口像素坐标 → 世界空间 y = 0 落点。
pub fn ground_point(
    screen: [f32; 2],
    viewport: [f32; 2],
    view_projection: &Matrix4<f32>,
) -> Option<[f32; 3]> {
    let ndc = ndc_from_screen(screen, viewport)?;
    let (origin, direction) = ray_from_ndc(ndc, view_projection);
    intersect_y0(origin, direction)
}

/// 世界空间点 → 视口像素坐标；点在相机后方时返回 `None`。
pub fn project_to_screen(
    point: [f32; 3],
    viewport: [f32; 2],
    view_projection: &Matrix4<f32>,
) -> Option<[f32; 2]> {
    let clip = view_projection * Vector4::new(point[0], point[1], point[2], 1.0);
    if clip.w <= 1e-6 {
        return None;
    }
    let ndc = [clip.x / clip.w, clip.y / clip.w];
    if !ndc[0].is_finite() || !ndc[1].is_finite() {
        return None;
    }
    Some(screen_from_ndc(ndc, viewport))
}

/// `cgmath` 的 `normalize` 在零向量上会给出 NaN，这里退化为零向量。
trait NormalizeOrZero {
    fn normalize_or_zero(self) -> Self;
}

impl NormalizeOrZero for Vector3<f32> {
    fn normalize_or_zero(self) -> Self {
        if self.magnitude2() > f32::EPSILON {
            self.normalize()
        } else {
            Vector3::new(0.0, 0.0, 0.0)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use camera::{Camera, CameraMode};
    use cgmath::Point3;

    fn orbit_camera() -> Camera {
        let mut camera = Camera::new(
            CameraMode::Orbit {
                yaw: -0.6,
                pitch: 0.45,
                focal_point: Point3::new(0.5, 0.0, -0.5),
                distance: 5.0,
                min_distance: 0.2,
                max_distance: 200.0,
            },
            16.0 / 9.0,
        );
        camera.update_aspect(1280, 720);
        camera
    }

    #[test]
    fn intersect_plane_hits_below_and_misses_up_or_parallel() {
        let hit = intersect_y0([0.0, 5.0, 0.0], [0.0, -1.0, 0.0]).unwrap();
        assert!((hit[0]).abs() < 1e-6 && (hit[2]).abs() < 1e-6);

        assert!(intersect_y0([0.0, 5.0, 0.0], [0.0, 1.0, 0.0]).is_none(), "朝上不该落点");
        assert!(intersect_y0([0.0, 5.0, 0.0], [1.0, 0.0, 0.0]).is_none(), "平行不该落点");

        let slanted = intersect_y0([0.0, 5.0, 0.0], [0.0, -1.0, 1.0]).unwrap();
        assert!((slanted[2] - 5.0).abs() < 1e-5, "斜射应在 z=5 处落地");
    }

    #[test]
    fn ndc_and_screen_round_trip() {
        let viewport = [1280.0, 720.0];
        for screen in [[0.0, 0.0], [640.0, 360.0], [1279.0, 719.0]] {
            let ndc = ndc_from_screen(screen, viewport).unwrap();
            let back = screen_from_ndc(ndc, viewport);
            assert!((back[0] - screen[0]).abs() < 1e-3);
            assert!((back[1] - screen[1]).abs() < 1e-3);
        }
        assert!(ndc_from_screen([0.0, 0.0], [0.0, 0.0]).is_none());
    }

    #[test]
    fn ground_point_inverts_projection() {
        let camera = orbit_camera();
        let vp = camera.view_projection_matrix();
        let viewport = [1280.0, 720.0];

        for expected in [[1.0, 0.0, -0.5], [-2.0, 0.0, 1.5], [0.0, 0.0, 0.0]] {
            let screen = project_to_screen(expected, viewport, &vp).expect("点应在相机前方");
            let hit = ground_point(screen, viewport, &vp).expect("视线应能落到地面");
            assert!(
                (hit[0] - expected[0]).abs() < 0.05 && (hit[2] - expected[2]).abs() < 0.05,
                "投影/反投影应互相还原：{expected:?} → {hit:?}"
            );
            assert_eq!(hit[1], 0.0);
        }
    }

    #[test]
    fn point_behind_camera_does_not_project() {
        let camera = orbit_camera();
        let vp = camera.view_projection_matrix();
        let eye = camera.eye_position();
        // 相机背后（再往后退一倍距离）
        let back = [
            eye.x * 2.0 - 0.5,
            eye.y * 2.0,
            eye.z * 2.0 + 0.5,
        ];
        assert!(project_to_screen(back, [1280.0, 720.0], &vp).is_none());
    }
}
