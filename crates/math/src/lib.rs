use cgmath::{Point3, Vector3, /* Matrix4, InnerSpace, EuclideanSpace, Rad, Deg, PerspectiveFov */};

/// 轴对齐包围盒。
///
/// **调用方需保证 `min` 各分量 ≤ `max` 对应分量**，
/// 本结构不做自动校验或修正。
#[derive(Debug, Clone, Copy)]
pub struct AABB {
    pub min: Point3<f32>,
    pub max: Point3<f32>,
}

impl AABB {
    pub fn new(min: Point3<f32>, max: Point3<f32>) -> Self {
        AABB { min, max }
    }
    
    pub fn center(&self) -> Point3<f32> {
        Point3::new(
            (self.min.x + self.max.x) * 0.5,
            (self.min.y + self.max.y) * 0.5,
            (self.min.z + self.max.z) * 0.5,
        )
    }
    
    pub fn size(&self) -> Vector3<f32> {
        Vector3::new(
            self.max.x - self.min.x,
            self.max.y - self.min.y,
            self.max.z - self.min.z,
        )
    }
    
    pub fn contains_point(&self, point: Point3<f32>) -> bool {
        point.x >= self.min.x && point.x <= self.max.x &&
        point.y >= self.min.y && point.y <= self.max.y &&
        point.z >= self.min.z && point.z <= self.max.z
    }
    
    pub fn intersects_aabb(&self, other: &AABB) -> bool {
        self.min.x <= other.max.x && self.max.x >= other.min.x &&
        self.min.y <= other.max.y && self.max.y >= other.min.y &&
        self.min.z <= other.max.z && self.max.z >= other.min.z
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_aabb(min: [f32; 3], max: [f32; 3]) -> AABB {
        AABB::new(
            Point3::new(min[0], min[1], min[2]),
            Point3::new(max[0], max[1], max[2]),
        )
    }

    #[test]
    fn test_center() {
        let aabb = make_aabb([0.0, 0.0, 0.0], [4.0, 6.0, 8.0]);
        let c = aabb.center();
        assert!((c.x - 2.0).abs() < 1e-6);
        assert!((c.y - 3.0).abs() < 1e-6);
        assert!((c.z - 4.0).abs() < 1e-6);
    }

    #[test]
    fn test_size() {
        let aabb = make_aabb([1.0, 2.0, 3.0], [5.0, 8.0, 10.0]);
        let s = aabb.size();
        assert!((s.x - 4.0).abs() < 1e-6);
        assert!((s.y - 6.0).abs() < 1e-6);
        assert!((s.z - 7.0).abs() < 1e-6);
    }

    #[test]
    fn test_contains_point() {
        let aabb = make_aabb([0.0, 0.0, 0.0], [10.0, 10.0, 10.0]);
        // 内部点
        assert!(aabb.contains_point(Point3::new(5.0, 5.0, 5.0)));
        // 边界点
        assert!(aabb.contains_point(Point3::new(0.0, 0.0, 0.0)));
        assert!(aabb.contains_point(Point3::new(10.0, 10.0, 10.0)));
        // 外部点
        assert!(!aabb.contains_point(Point3::new(11.0, 5.0, 5.0)));
        assert!(!aabb.contains_point(Point3::new(5.0, -1.0, 5.0)));
    }

    #[test]
    fn test_intersects_aabb() {
        let a = make_aabb([0.0, 0.0, 0.0], [10.0, 10.0, 10.0]);
        // 重叠
        let b = make_aabb([5.0, 5.0, 5.0], [15.0, 15.0, 15.0]);
        assert!(a.intersects_aabb(&b));
        // 刚好接触（共享面）
        let c = make_aabb([10.0, 0.0, 0.0], [20.0, 10.0, 10.0]);
        assert!(a.intersects_aabb(&c));
        // 完全分离
        let d = make_aabb([11.0, 0.0, 0.0], [20.0, 10.0, 10.0]);
        assert!(!a.intersects_aabb(&d));
    }

    #[test]
    fn test_zero_volume_aabb() {
        let aabb = make_aabb([3.0, 4.0, 5.0], [3.0, 4.0, 5.0]);
        let c = aabb.center();
        assert!((c.x - 3.0).abs() < 1e-6);
        assert!((c.y - 4.0).abs() < 1e-6);
        assert!((c.z - 5.0).abs() < 1e-6);
        let s = aabb.size();
        assert!(s.x.abs() < 1e-6);
        assert!(s.y.abs() < 1e-6);
        assert!(s.z.abs() < 1e-6);
        // 零体积 AABB 包含其自身的点
        assert!(aabb.contains_point(Point3::new(3.0, 4.0, 5.0)));
    }

    #[test]
    fn test_contains_point_boundary_edges() {
        let aabb = make_aabb([-5.0, -5.0, -5.0], [5.0, 5.0, 5.0]);
        // All six face centers should be contained
        assert!(aabb.contains_point(Point3::new(-5.0, 0.0, 0.0)));
        assert!(aabb.contains_point(Point3::new(5.0, 0.0, 0.0)));
        assert!(aabb.contains_point(Point3::new(0.0, -5.0, 0.0)));
        assert!(aabb.contains_point(Point3::new(0.0, 5.0, 0.0)));
        assert!(aabb.contains_point(Point3::new(0.0, 0.0, -5.0)));
        assert!(aabb.contains_point(Point3::new(0.0, 0.0, 5.0)));
        // Just outside each face
        assert!(!aabb.contains_point(Point3::new(-5.001, 0.0, 0.0)));
        assert!(!aabb.contains_point(Point3::new(5.001, 0.0, 0.0)));
        assert!(!aabb.contains_point(Point3::new(0.0, -5.001, 0.0)));
        assert!(!aabb.contains_point(Point3::new(0.0, 5.001, 0.0)));
        assert!(!aabb.contains_point(Point3::new(0.0, 0.0, -5.001)));
        assert!(!aabb.contains_point(Point3::new(0.0, 0.0, 5.001)));
    }

    #[test]
    fn test_intersects_aabb_contained() {
        let outer = make_aabb([0.0, 0.0, 0.0], [10.0, 10.0, 10.0]);
        // Completely contained AABB should intersect
        let inner = make_aabb([2.0, 2.0, 2.0], [8.0, 8.0, 8.0]);
        assert!(outer.intersects_aabb(&inner));
        assert!(inner.intersects_aabb(&outer));
    }

    #[test]
    fn test_intersects_aabb_symmetric() {
        let a = make_aabb([0.0, 0.0, 0.0], [5.0, 5.0, 5.0]);
        let b = make_aabb([3.0, 3.0, 3.0], [8.0, 8.0, 8.0]);
        // Intersection should be symmetric
        assert_eq!(a.intersects_aabb(&b), b.intersects_aabb(&a));
        assert!(a.intersects_aabb(&b));
    }

    #[test]
    fn test_negative_coordinates() {
        let aabb = make_aabb([-10.0, -20.0, -30.0], [-1.0, -2.0, -3.0]);
        let c = aabb.center();
        assert!((c.x - (-5.5)).abs() < 1e-6);
        assert!((c.y - (-11.0)).abs() < 1e-6);
        assert!((c.z - (-16.5)).abs() < 1e-6);
        let s = aabb.size();
        assert!((s.x - 9.0).abs() < 1e-6);
        assert!((s.y - 18.0).abs() < 1e-6);
        assert!((s.z - 27.0).abs() < 1e-6);
        assert!(aabb.contains_point(Point3::new(-5.0, -10.0, -15.0)));
        assert!(!aabb.contains_point(Point3::new(0.0, 0.0, 0.0)));
    }

    #[test]
    fn test_center_size_consistency() {
        // Verify that center +/- half_size reconstructs min/max
        let aabb = make_aabb([1.0, 2.0, 3.0], [7.0, 10.0, 15.0]);
        let c = aabb.center();
        let half = aabb.size();
        let half_x = half.x * 0.5;
        let half_y = half.y * 0.5;
        let half_z = half.z * 0.5;
        assert!((c.x - half_x - 1.0).abs() < 1e-5);
        assert!((c.y - half_y - 2.0).abs() < 1e-5);
        assert!((c.z - half_z - 3.0).abs() < 1e-5);
        assert!((c.x + half_x - 7.0).abs() < 1e-5);
        assert!((c.y + half_y - 10.0).abs() < 1e-5);
        assert!((c.z + half_z - 15.0).abs() < 1e-5);
    }
}
