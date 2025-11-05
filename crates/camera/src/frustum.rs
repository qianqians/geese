use cgmath::{
    EuclideanSpace, /* , Rad, Deg, PerspectiveFov */
    InnerSpace, Matrix4, Point3, Vector3,
};

// 定义平面
#[derive(Debug, Clone, Copy)]
pub struct Plane {
    pub normal: Vector3<f32>,
    pub distance: f32,
}

impl Plane {
    pub fn from_coefficients(a: f32, b: f32, c: f32, d: f32) -> Self {
        let normal = Vector3::new(a, b, c);
        let length = normal.magnitude();
        Plane {
            normal: normal / length,
            distance: d / length,
        }
    }

    pub fn distance_to_point(&self, point: Point3<f32>) -> f32 {
        self.normal.dot(point.to_vec()) + self.distance
    }
}

// 定义视锥体
#[derive(Debug, Clone)]
pub struct Frustum {
    pub planes: [Plane; 6],
}

impl Frustum {
    pub fn from_view_projection_matrix(m: &Matrix4<f32>) -> Self {
        let mut planes = [
            Plane::from_coefficients(
                m[0][3] - m[0][0],
                m[1][3] - m[1][0],
                m[2][3] - m[2][0],
                m[3][3] - m[3][0],
            ),
            Plane::from_coefficients(
                m[0][3] + m[0][0],
                m[1][3] + m[1][0],
                m[2][3] + m[2][0],
                m[3][3] + m[3][0],
            ),
            Plane::from_coefficients(
                m[0][3] + m[0][1],
                m[1][3] + m[1][1],
                m[2][3] + m[2][1],
                m[3][3] + m[3][1],
            ),
            Plane::from_coefficients(
                m[0][3] - m[0][1],
                m[1][3] - m[1][1],
                m[2][3] - m[2][1],
                m[3][3] - m[3][1],
            ),
            Plane::from_coefficients(
                m[0][3] - m[0][2],
                m[1][3] - m[1][2],
                m[2][3] - m[2][2],
                m[3][3] - m[3][2],
            ),
            Plane::from_coefficients(
                m[0][3] + m[0][2],
                m[1][3] + m[1][2],
                m[2][3] + m[2][2],
                m[3][3] + m[3][2],
            ),
        ];

        for plane in planes.iter_mut() {
            let length = plane.normal.magnitude();
            plane.normal = plane.normal / length;
            plane.distance = plane.distance / length;
        }

        Frustum { planes }
    }

    pub fn contains_point(&self, point: Point3<f32>) -> bool {
        for plane in &self.planes {
            if plane.distance_to_point(point) < 0.0 {
                return false;
            }
        }
        true
    }

    pub fn contains_sphere(&self, center: Point3<f32>, radius: f32) -> bool {
        for plane in &self.planes {
            let distance = plane.distance_to_point(center);
            if distance < -radius {
                return false;
            }
        }
        true
    }

    pub fn contains_aabb(&self, min: Point3<f32>, max: Point3<f32>) -> bool {
        for plane in &self.planes {
            let mut p = Point3::new(min.x, min.y, min.z);

            if plane.normal.x >= 0.0 {
                p.x = max.x;
            }
            if plane.normal.y >= 0.0 {
                p.y = max.y;
            }
            if plane.normal.z >= 0.0 {
                p.z = max.z;
            }

            if plane.distance_to_point(p) < 0.0 {
                return false;
            }
        }
        true
    }

    pub fn intersects_aabb(&self, min: Point3<f32>, max: Point3<f32>) -> bool {
        for plane in &self.planes {
            // 找到AABB在平面法线方向上的最近点
            let mut closest = Point3::new(min.x, min.y, min.z);

            if plane.normal.x >= 0.0 {
                closest.x = max.x;
            }
            if plane.normal.y >= 0.0 {
                closest.y = max.y;
            }
            if plane.normal.z >= 0.0 {
                closest.z = max.z;
            }

            // 如果最近点在平面负侧，则AABB完全在平面外
            if plane.distance_to_point(closest) < 0.0 {
                return false;
            }
        }

        true
    }
}
