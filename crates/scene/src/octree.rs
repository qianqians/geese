use cgmath::{Point3/* , Matrix4, Vector3, InnerSpace, EuclideanSpace, Rad, Deg, PerspectiveFov */};
use std::fmt::Debug;
use math::AABB;
use camera::frustum::{Frustum};

// 场景对象 trait
pub trait SceneObject: Debug {
    fn entity_id(&self) -> String;
    fn aabb(&self) -> AABB;
    fn center(&self) -> Point3<f32>;
}

// 八叉树节点
#[derive(Debug)]
struct OctreeNode<T: SceneObject> {
    bounds: AABB,
    children: Option<[Box<OctreeNode<T>>; 8]>,
    objects: Vec<T>,
    max_objects: usize,
    max_depth: usize,
}

impl<T: SceneObject> OctreeNode<T> {
    fn new(bounds: AABB, max_objects: usize, max_depth: usize) -> Self {
        OctreeNode {
            bounds,
            children: None,
            objects: Vec::new(),
            max_objects,
            max_depth,
        }
    }
    
    fn subdivide(&mut self) {
        let center = self.bounds.center();
        //let half_size = self.bounds.size() * 0.5;
        
        let children_bounds = [
            // 前左下
            AABB::new(
                Point3::new(self.bounds.min.x, self.bounds.min.y, self.bounds.min.z),
                Point3::new(center.x, center.y, center.z),
            ),
            // 前右下
            AABB::new(
                Point3::new(center.x, self.bounds.min.y, self.bounds.min.z),
                Point3::new(self.bounds.max.x, center.y, center.z),
            ),
            // 前左上
            AABB::new(
                Point3::new(self.bounds.min.x, center.y, self.bounds.min.z),
                Point3::new(center.x, self.bounds.max.y, center.z),
            ),
            // 前右上
            AABB::new(
                Point3::new(center.x, center.y, self.bounds.min.z),
                Point3::new(self.bounds.max.x, self.bounds.max.y, center.z),
            ),
            // 后左下
            AABB::new(
                Point3::new(self.bounds.min.x, self.bounds.min.y, center.z),
                Point3::new(center.x, center.y, self.bounds.max.z),
            ),
            // 后右下
            AABB::new(
                Point3::new(center.x, self.bounds.min.y, center.z),
                Point3::new(self.bounds.max.x, center.y, self.bounds.max.z),
            ),
            // 后左上
            AABB::new(
                Point3::new(self.bounds.min.x, center.y, center.z),
                Point3::new(center.x, self.bounds.max.y, self.bounds.max.z),
            ),
            // 后右上
            AABB::new(
                Point3::new(center.x, center.y, center.z),
                Point3::new(self.bounds.max.x, self.bounds.max.y, self.bounds.max.z),
            ),
        ];
        
        let mut children = [
            Box::new(OctreeNode::new(children_bounds[0], self.max_objects, self.max_depth - 1)),
            Box::new(OctreeNode::new(children_bounds[1], self.max_objects, self.max_depth - 1)),
            Box::new(OctreeNode::new(children_bounds[2], self.max_objects, self.max_depth - 1)),
            Box::new(OctreeNode::new(children_bounds[3], self.max_objects, self.max_depth - 1)),
            Box::new(OctreeNode::new(children_bounds[4], self.max_objects, self.max_depth - 1)),
            Box::new(OctreeNode::new(children_bounds[5], self.max_objects, self.max_depth - 1)),
            Box::new(OctreeNode::new(children_bounds[6], self.max_objects, self.max_depth - 1)),
            Box::new(OctreeNode::new(children_bounds[7], self.max_objects, self.max_depth - 1)),
        ];
        
        // 将当前对象重新分配到子节点中
        let objects = std::mem::take(&mut self.objects);
        for obj in objects {
            for child in children.iter_mut() {
                if child.bounds.contains_point(obj.center()) {
                    child.insert(obj);
                    break;
                }
            }
        }
        
        self.children = Some(children);
    }
    
    fn insert(&mut self, object: T) {
        // 如果对象不在本节点的边界内，则不插入
        if !self.bounds.contains_point(object.center()) {
            return;
        }
        
        // 如果有子节点，尝试插入到子节点中
        if let Some(children) = &mut self.children {
            for child in children.iter_mut() {
                if child.bounds.contains_point(object.center()) {
                    child.insert(object);
                    return;
                }
            }
        }
        
        // 否则添加到当前节点
        self.objects.push(object);
        
        // 如果对象数量超过阈值且还有深度，进行细分
        if self.objects.len() > self.max_objects && self.max_depth > 0 && self.children.is_none() {
            self.subdivide();
        }
    }
    
    fn query_frustum<'a>(&'a self, frustum: &Frustum, result: &mut Vec<&'a T>) {
        // 检查当前节点的边界是否与视锥体相交
        if !frustum.intersects_aabb(self.bounds.min, self.bounds.max) {
            return;
        }
        
        // 添加当前节点中所有在视锥体内的对象
        for obj in &self.objects {
            if frustum.contains_aabb(obj.aabb().min, obj.aabb().max) {
                result.push(obj);
            }
        }
        
        // 递归查询子节点
        if let Some(children) = &self.children {
            for child in children.iter() {
                child.query_frustum(frustum, result);
            }
        }
    }
}

// 八叉树
#[derive(Debug)]
pub struct Octree<T: SceneObject> {
    root: OctreeNode<T>,
}

impl<T: SceneObject> Octree<T> {
    pub fn new(bounds: AABB, max_objects: usize, max_depth: usize) -> Self {
        Octree {
            root: OctreeNode::new(bounds, max_objects, max_depth),
        }
    }
    
    pub fn insert(&mut self, object: T) {
        self.root.insert(object);
    }
    
    pub fn query_frustum<'a>(&'a self, frustum: &Frustum) -> Vec<&'a T> {
        let mut result = Vec::new();
        self.root.query_frustum(frustum, &mut result);
        result
    }
}
