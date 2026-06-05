//! 八叉树空间剪枝结构。
//!
//! 节点中只保存 `OctreeEntry { id, aabb, center }`，**不持有 `SceneObject` 副本**。
//! 调用方根据返回的 id 索引到自己的对象数组——避免每帧 clone，并使 octree 与具体对象类型解耦。
//!
//! 典型用法：
//! ```ignore
//! let mut tree = Octree::new(bounds, max_objects, max_depth);
//! for (i, obj) in objects.iter().enumerate() {
//!     tree.insert(i, obj.aabb, obj.center);
//! }
//! let visible_ids = tree.query_frustum(&frustum);
//! let visible: Vec<&SceneObject> = visible_ids.iter().map(|&i| &objects[i]).collect();
//! ```

use camera::frustum::Frustum;
use cgmath::Point3;
use math::AABB;

/// 八叉树节点中保存的对象元数据条目。
#[derive(Clone, Copy, Debug)]
pub struct OctreeEntry {
    pub id: usize,
    pub aabb: AABB,
    pub center: Point3<f32>,
}

/// 判断 inner 是否被 outer 完全包住（闭区间）。
fn aabb_contains_aabb(outer: &AABB, inner: &AABB) -> bool {
    inner.min.x >= outer.min.x
        && inner.max.x <= outer.max.x
        && inner.min.y >= outer.min.y
        && inner.max.y <= outer.max.y
        && inner.min.z >= outer.min.z
        && inner.max.z <= outer.max.z
}

struct OctreeNode {
    bounds: AABB,
    children: Option<[Box<OctreeNode>; 8]>,
    entries: Vec<OctreeEntry>,
    max_objects: usize,
    max_depth: usize,
}

impl OctreeNode {
    fn new(bounds: AABB, max_objects: usize, max_depth: usize) -> Self {
        OctreeNode {
            bounds,
            children: None,
            entries: Vec::new(),
            max_objects,
            max_depth,
        }
    }

    fn child_bounds(&self) -> [AABB; 8] {
        let center = self.bounds.center();
        let min = self.bounds.min;
        let max = self.bounds.max;
        [
            // 前左下
            AABB::new(
                Point3::new(min.x, min.y, min.z),
                Point3::new(center.x, center.y, center.z),
            ),
            // 前右下
            AABB::new(
                Point3::new(center.x, min.y, min.z),
                Point3::new(max.x, center.y, center.z),
            ),
            // 前左上
            AABB::new(
                Point3::new(min.x, center.y, min.z),
                Point3::new(center.x, max.y, center.z),
            ),
            // 前右上
            AABB::new(
                Point3::new(center.x, center.y, min.z),
                Point3::new(max.x, max.y, center.z),
            ),
            // 后左下
            AABB::new(
                Point3::new(min.x, min.y, center.z),
                Point3::new(center.x, center.y, max.z),
            ),
            // 后右下
            AABB::new(
                Point3::new(center.x, min.y, center.z),
                Point3::new(max.x, center.y, max.z),
            ),
            // 后左上
            AABB::new(
                Point3::new(min.x, center.y, center.z),
                Point3::new(center.x, max.y, max.z),
            ),
            // 后右上
            AABB::new(
                Point3::new(center.x, center.y, center.z),
                Point3::new(max.x, max.y, max.z),
            ),
        ]
    }

    fn subdivide(&mut self) {
        debug_assert!(
            self.max_depth > 0,
            "OctreeNode::subdivide called with max_depth == 0"
        );
        let bounds_array = self.child_bounds();
        let max_objects = self.max_objects;
        let child_depth = self.max_depth.saturating_sub(1);
        let mut children: [Box<OctreeNode>; 8] = std::array::from_fn(|i| {
            Box::new(OctreeNode::new(bounds_array[i], max_objects, child_depth))
        });

        // 重新分配：能被某个 child 完全包住 AABB 的条目下沉，否则留在父节点。
        // 这样可保证 query_frustum 用 node.bounds 做剪枝时不会漏掉跨界的大对象，
        // 同时也避免了浮点边界让条目被静默丢弃的 bug。
        let entries = std::mem::take(&mut self.entries);
        let mut retained = Vec::new();
        'outer: for entry in entries {
            for child in children.iter_mut() {
                if aabb_contains_aabb(&child.bounds, &entry.aabb) {
                    child.insert(entry);
                    continue 'outer;
                }
            }
            retained.push(entry);
        }
        self.entries = retained;
        self.children = Some(children);
    }

    fn insert(&mut self, entry: OctreeEntry) {
        // 越界条目在 Octree::insert 已 debug_assert 拦截，这里仅做兜底
        if !self.bounds.contains_point(entry.center) {
            return;
        }

        // 已细分：尝试下沉到能完全包住 entry.aabb 的 child;否则留在本节点
        if let Some(children) = &mut self.children {
            for child in children.iter_mut() {
                if aabb_contains_aabb(&child.bounds, &entry.aabb) {
                    child.insert(entry);
                    return;
                }
            }
            self.entries.push(entry);
            return;
        }

        // 未细分：直接放本节点
        self.entries.push(entry);

        // 数量超限且仍可细分时进行 subdivide
        if self.entries.len() > self.max_objects && self.max_depth > 0 {
            self.subdivide();
        }
    }

    fn query_frustum(&self, frustum: &Frustum, result: &mut Vec<usize>) {
        // 父节点 bounds 与视锥体不相交 → 整子树剔除
        if !frustum.intersects_aabb(self.bounds.min, self.bounds.max) {
            return;
        }

        // 收集与视锥相交（部分或完全可见）的条目。
        // 注意：必须用 intersects_aabb 而不是 contains_aabb，否则跨视锥边界的物体会被误剔除。
        for entry in &self.entries {
            if frustum.intersects_aabb(entry.aabb.min, entry.aabb.max) {
                result.push(entry.id);
            }
        }

        if let Some(children) = &self.children {
            for child in children.iter() {
                child.query_frustum(frustum, result);
            }
        }
    }

    fn collect_ids(&self, result: &mut Vec<usize>) {
        result.extend(self.entries.iter().map(|e| e.id));

        if let Some(children) = &self.children {
            for child in children.iter() {
                child.collect_ids(result);
            }
        }
    }

    fn entry_count(&self) -> usize {
        let mut n = self.entries.len();
        if let Some(children) = &self.children {
            for child in children.iter() {
                n += child.entry_count();
            }
        }
        n
    }
}

pub struct Octree {
    root_bounds: AABB,
    max_objects: usize,
    max_depth: usize,
    root: OctreeNode,
}

impl Octree {
    pub fn new(bounds: AABB, max_objects: usize, max_depth: usize) -> Self {
        Octree {
            root_bounds: bounds,
            max_objects,
            max_depth,
            root: OctreeNode::new(bounds, max_objects, max_depth),
        }
    }

    /// 重置八叉树，保留 bounds/max_objects/max_depth 配置。
    pub fn clear(&mut self) {
        self.root = OctreeNode::new(self.root_bounds, self.max_objects, self.max_depth);
    }

    /// 插入一条 (id, aabb, center)。id 由调用方提供（一般为 SceneObject 在数组中的索引）。
    pub fn insert(&mut self, id: usize, aabb: AABB, center: Point3<f32>) {
        // 静默丢弃越界条目会导致调试困难，这里加 debug_assert 提示
        debug_assert!(
            self.root.bounds.contains_point(center),
            "Octree::insert object center is outside root bounds; will be dropped"
        );
        self.root.insert(OctreeEntry { id, aabb, center });
    }

    /// 视锥剪枝查询，返回与视锥相交的对象 id 列表。
    pub fn query_frustum(&self, frustum: &Frustum) -> Vec<usize> {
        let mut result = Vec::new();
        self.root.query_frustum(frustum, &mut result);
        result
    }

    /// 列出树中所有对象 id。
    pub fn all_ids(&self) -> Vec<usize> {
        let mut result = Vec::new();
        self.root.collect_ids(&mut result);
        result
    }

    /// 当前持有的条目总数（含子节点）。
    pub fn len(&self) -> usize {
        self.root.entry_count()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use camera::frustum::Plane;
    use cgmath::Vector3;

    fn make_entry(id: usize, center: Point3<f32>, half_extent: f32) -> (usize, AABB, Point3<f32>) {
        let half = Vector3::new(half_extent, half_extent, half_extent);
        let aabb = AABB::new(center - half, center + half);
        (id, aabb, center)
    }

    fn root_bounds() -> AABB {
        AABB::new(
            Point3::new(-10.0, -10.0, -10.0),
            Point3::new(10.0, 10.0, 10.0),
        )
    }

    /// 用 6 个轴对齐平面构造一个盒形 "视锥"，便于测试 frustum 相关行为。
    /// Plane: distance_to_point(p) = dot(normal, p) + distance, >= 0 表示在内侧。
    fn make_axis_aligned_frustum(min: Point3<f32>, max: Point3<f32>) -> Frustum {
        let planes = [
            Plane { normal: Vector3::new(1.0, 0.0, 0.0), distance: -min.x },
            Plane { normal: Vector3::new(-1.0, 0.0, 0.0), distance: max.x },
            Plane { normal: Vector3::new(0.0, 1.0, 0.0), distance: -min.y },
            Plane { normal: Vector3::new(0.0, -1.0, 0.0), distance: max.y },
            Plane { normal: Vector3::new(0.0, 0.0, 1.0), distance: -min.z },
            Plane { normal: Vector3::new(0.0, 0.0, -1.0), distance: max.z },
        ];
        Frustum { planes }
    }

    #[test]
    fn insert_and_collect_preserves_all_ids() {
        let mut tree = Octree::new(root_bounds(), 2, 4);
        let centers = [
            Point3::new(-5.0, -5.0, -5.0),
            Point3::new(5.0, -5.0, -5.0),
            Point3::new(-5.0, 5.0, -5.0),
            Point3::new(-5.0, -5.0, 5.0),
            Point3::new(5.0, 5.0, 5.0),
        ];
        for (i, c) in centers.iter().enumerate() {
            let (id, aabb, center) = make_entry(i, *c, 0.5);
            tree.insert(id, aabb, center);
        }
        assert_eq!(tree.len(), centers.len());
        let mut ids = tree.all_ids();
        ids.sort();
        assert_eq!(ids, vec![0, 1, 2, 3, 4]);
    }

    #[test]
    fn subdivide_keeps_objects_on_split_plane() {
        // center 落在分割面（原点）上 → 细分时不应丢失（覆盖原 subdivide bug）
        let mut tree = Octree::new(root_bounds(), 1, 4);
        let (id, aabb, center) = make_entry(0, Point3::new(-5.0, -5.0, -5.0), 0.1);
        tree.insert(id, aabb, center);
        let (id, aabb, center) = make_entry(1, Point3::new(5.0, 5.0, 5.0), 0.1);
        tree.insert(id, aabb, center);
        let (id, aabb, center) = make_entry(2, Point3::new(0.0, 0.0, 0.0), 0.1);
        tree.insert(id, aabb, center);

        assert_eq!(tree.len(), 3);
    }

    #[test]
    fn large_object_stays_in_parent_and_visible_via_frustum() {
        // 大物体跨多个子节点 → 应留在父节点，仍能被视锥查到（覆盖 center-only 分配 bug）
        let mut tree = Octree::new(root_bounds(), 1, 4);
        let (id, aabb, center) = make_entry(0, Point3::new(-5.0, -5.0, -5.0), 0.1);
        tree.insert(id, aabb, center);
        let (id, aabb, center) = make_entry(1, Point3::new(5.0, 5.0, 5.0), 0.1);
        tree.insert(id, aabb, center);
        let big_id = 42;
        let (id, aabb, center) = make_entry(big_id, Point3::new(0.0, 0.0, 0.0), 8.0);
        tree.insert(id, aabb, center);

        assert_eq!(tree.len(), 3);

        let frustum = make_axis_aligned_frustum(
            Point3::new(0.0, -10.0, -10.0),
            Point3::new(10.0, 10.0, 10.0),
        );
        let visible = tree.query_frustum(&frustum);
        assert!(
            visible.contains(&big_id),
            "large object should be visible in +X half: visible = {:?}",
            visible
        );
    }

    #[test]
    fn frustum_partial_intersection_includes_edge_objects() {
        // 与视锥仅部分相交的对象应被纳入结果（覆盖原 contains_aabb 错误剔除 bug）
        let mut tree = Octree::new(root_bounds(), 8, 4);
        let (id, aabb, center) = make_entry(7, Point3::new(1.0, 0.0, 0.0), 2.0);
        tree.insert(id, aabb, center);

        let frustum = make_axis_aligned_frustum(
            Point3::new(2.0, -10.0, -10.0),
            Point3::new(10.0, 10.0, 10.0),
        );
        let visible = tree.query_frustum(&frustum);
        assert_eq!(
            visible,
            vec![7],
            "object partially inside frustum should not be culled"
        );
    }

    #[test]
    fn clear_resets_entries_but_keeps_config() {
        let mut tree = Octree::new(root_bounds(), 2, 4);
        for i in 0..5 {
            let (id, aabb, center) =
                make_entry(i, Point3::new(i as f32 - 2.0, 0.0, 0.0), 0.1);
            tree.insert(id, aabb, center);
        }
        assert_eq!(tree.len(), 5);
        tree.clear();
        assert!(tree.is_empty());

        // 清空后还能继续 insert
        let (id, aabb, center) = make_entry(99, Point3::new(0.0, 0.0, 0.0), 0.1);
        tree.insert(id, aabb, center);
        assert_eq!(tree.all_ids(), vec![99]);
    }
}
