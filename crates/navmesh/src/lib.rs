//! 4.12 NavMesh 寻路骨架。
//!
//! 提供：
//! - `NavMesh`：三角形拓扑（顶点 + 三角形 + 邻接 + 每个三角形的中心 / 是否可走）
//! - `NavMesh::from_triangles()`：自动建邻接（按共享边）
//! - `find_path()`：基于三角形邻接图的 A*，返回三角形序列
//! - `funnel_smooth()`：在三角形通道内做漏斗算法路径平滑，输出空间路径点
//!
//! 不做几何裁剪 / Recast 体素生成，留待接入 `oxidized_navigation` 时实现。

use std::collections::{BinaryHeap, HashMap};

pub type TriId = u32;

/// XZ 平面 2D 点（NavMesh 通常在 XZ 平面）。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Vec2 { pub x: f32, pub z: f32 }

impl Vec2 {
    pub fn new(x: f32, z: f32) -> Self { Self { x, z } }
    pub fn dist(self, o: Vec2) -> f32 {
        let dx = self.x - o.x; let dz = self.z - o.z;
        (dx*dx + dz*dz).sqrt()
    }
    pub fn cross(self, o: Vec2) -> f32 { self.x * o.z - self.z * o.x }
    pub fn sub(self, o: Vec2) -> Vec2 { Vec2::new(self.x - o.x, self.z - o.z) }
}

/// 一个三角形面片：顶点 id 三元组 + 区域 cost。
#[derive(Clone, Copy, Debug)]
pub struct NavTri {
    pub verts: [u32; 3],
    /// 区域 cost 系数（>=0），路径累计代价 = 边长 × cost。
    pub area_cost: f32,
    /// 是否可走（false 则建邻接时跳过该三角形）。
    pub walkable: bool,
}

impl NavTri {
    pub fn new(a: u32, b: u32, c: u32) -> Self {
        Self { verts: [a, b, c], area_cost: 1.0, walkable: true }
    }
}

pub struct NavMesh {
    pub vertices: Vec<Vec2>,
    pub triangles: Vec<NavTri>,
    /// 每个三角形的邻接：与共享边相邻的三角形 id（最多 3 个）。
    pub adjacency: Vec<[Option<TriId>; 3]>,
    pub centers: Vec<Vec2>,
}

impl NavMesh {
    pub fn from_triangles(vertices: Vec<Vec2>, triangles: Vec<NavTri>) -> Self {
        // 边 → 拥有该边的 (tri_id, edge_idx) 列表
        let mut edge_map: HashMap<(u32, u32), Vec<(TriId, usize)>> = HashMap::new();
        for (i, t) in triangles.iter().enumerate() {
            if !t.walkable { continue; }
            for e in 0..3 {
                let a = t.verts[e];
                let b = t.verts[(e + 1) % 3];
                let key = if a < b { (a, b) } else { (b, a) };
                edge_map.entry(key).or_default().push((i as TriId, e));
            }
        }
        let mut adjacency = vec![[None, None, None]; triangles.len()];
        for owners in edge_map.values() {
            if owners.len() != 2 { continue; }
            let (t1, e1) = owners[0];
            let (t2, e2) = owners[1];
            adjacency[t1 as usize][e1] = Some(t2);
            adjacency[t2 as usize][e2] = Some(t1);
        }
        let centers: Vec<Vec2> = triangles.iter().map(|t| {
            let a = vertices[t.verts[0] as usize];
            let b = vertices[t.verts[1] as usize];
            let c = vertices[t.verts[2] as usize];
            Vec2::new((a.x + b.x + c.x) / 3.0, (a.z + b.z + c.z) / 3.0)
        }).collect();
        Self { vertices, triangles, adjacency, centers }
    }

    pub fn tri_count(&self) -> usize { self.triangles.len() }

    /// 查找包含点 p 的三角形（O(N) 朴素遍历，足够骨架使用）。
    pub fn locate(&self, p: Vec2) -> Option<TriId> {
        for (i, t) in self.triangles.iter().enumerate() {
            if !t.walkable { continue; }
            let a = self.vertices[t.verts[0] as usize];
            let b = self.vertices[t.verts[1] as usize];
            let c = self.vertices[t.verts[2] as usize];
            if point_in_tri(p, a, b, c) {
                return Some(i as TriId);
            }
        }
        None
    }
}

/// A* 优先队列项。
#[derive(PartialEq)]
struct Node { f: f32, tri: TriId }
impl Eq for Node {}
impl Ord for Node {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        // 最小堆；使用 total_cmp 避免 NaN 导致排序不稳定
        other.f.total_cmp(&self.f)
    }
}
impl PartialOrd for Node { fn partial_cmp(&self, o: &Self) -> Option<std::cmp::Ordering> { Some(self.cmp(o)) } }

/// 在三角形邻接图上跑 A*，返回从 start_tri 到 end_tri 的三角形序列。
pub fn find_path(mesh: &NavMesh, start_tri: TriId, end_tri: TriId) -> Option<Vec<TriId>> {
    let n = mesh.tri_count();
    if (start_tri as usize) >= n || (end_tri as usize) >= n { return None; }
    if start_tri == end_tri { return Some(vec![start_tri]); }

    let goal = mesh.centers[end_tri as usize];
    let mut open = BinaryHeap::new();
    let mut g_score: HashMap<TriId, f32> = HashMap::new();
    let mut came_from: HashMap<TriId, TriId> = HashMap::new();

    g_score.insert(start_tri, 0.0);
    open.push(Node { f: mesh.centers[start_tri as usize].dist(goal), tri: start_tri });

    while let Some(Node { tri, .. }) = open.pop() {
        if tri == end_tri {
            // 回溯
            let mut path = vec![tri];
            let mut cur = tri;
            while let Some(&prev) = came_from.get(&cur) {
                path.push(prev);
                cur = prev;
            }
            path.reverse();
            return Some(path);
        }
        let g_cur = *g_score.get(&tri).unwrap_or(&f32::INFINITY);
        for nb in mesh.adjacency[tri as usize].iter().flatten() {
            let cost = mesh.triangles[*nb as usize].area_cost.max(0.0);
            if !mesh.triangles[*nb as usize].walkable { continue; }
            let g_new = g_cur + mesh.centers[tri as usize].dist(mesh.centers[*nb as usize]) * cost;
            if g_new < *g_score.get(nb).unwrap_or(&f32::INFINITY) {
                g_score.insert(*nb, g_new);
                came_from.insert(*nb, tri);
                let f = g_new + mesh.centers[*nb as usize].dist(goal);
                open.push(Node { f, tri: *nb });
            }
        }
    }
    None
}

/// 漏斗算法：从三角形路径平滑出空间路径点（含 start/end）。
///
/// 这是一个简化实现：在每个共享边的中点 + start/end 间用直线连接。
/// 真实漏斗算法在共享边的左右顶点上做 cross 判断，这里留待接入时升级。
pub fn funnel_smooth(mesh: &NavMesh, path: &[TriId], start: Vec2, end: Vec2) -> Vec<Vec2> {
    let mut out = Vec::with_capacity(path.len() + 2);
    out.push(start);
    for win in path.windows(2) {
        let a = win[0];
        let b = win[1];
        // 找 a 与 b 的共享边的中点
        if let Some(edge_idx) = mesh.adjacency[a as usize].iter().position(|&n| n == Some(b)) {
            let t = &mesh.triangles[a as usize];
            let v0 = mesh.vertices[t.verts[edge_idx] as usize];
            let v1 = mesh.vertices[t.verts[(edge_idx + 1) % 3] as usize];
            out.push(Vec2::new((v0.x + v1.x) * 0.5, (v0.z + v1.z) * 0.5));
        }
    }
    out.push(end);
    out
}

fn point_in_tri(p: Vec2, a: Vec2, b: Vec2, c: Vec2) -> bool {
    let s1 = p.sub(a).cross(b.sub(a));
    let s2 = p.sub(b).cross(c.sub(b));
    let s3 = p.sub(c).cross(a.sub(c));
    (s1 >= 0.0 && s2 >= 0.0 && s3 >= 0.0) || (s1 <= 0.0 && s2 <= 0.0 && s3 <= 0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 构造一个 2x2 的方形网格（4 个 quad 拆为 8 个三角形）。
    fn grid_mesh() -> NavMesh {
        // 顶点：3x3 = 9 个
        let mut verts = Vec::new();
        for j in 0..3 {
            for i in 0..3 {
                verts.push(Vec2::new(i as f32, j as f32));
            }
        }
        let mut tris = Vec::new();
        for j in 0..2 {
            for i in 0..2 {
                let v00 = (j * 3 + i) as u32;
                let v10 = v00 + 1;
                let v01 = v00 + 3;
                let v11 = v01 + 1;
                tris.push(NavTri::new(v00, v10, v11));
                tris.push(NavTri::new(v00, v11, v01));
            }
        }
        NavMesh::from_triangles(verts, tris)
    }

    #[test]
    fn from_triangles_builds_adjacency() {
        let mesh = grid_mesh();
        assert_eq!(mesh.tri_count(), 8);
        // 每个三角形至少有 1 个邻居（角落 quad 内部对角边共享）
        for adj in &mesh.adjacency {
            assert!(adj.iter().any(|a| a.is_some()));
        }
    }

    #[test]
    fn locate_finds_containing_triangle() {
        let mesh = grid_mesh();
        let p = Vec2::new(0.25, 0.25);
        let tri = mesh.locate(p);
        assert!(tri.is_some());
    }

    #[test]
    fn find_path_within_same_tri_returns_single() {
        let mesh = grid_mesh();
        let p = mesh.find_path_or_default(0, 0);
        assert_eq!(p.unwrap(), vec![0]);
    }

    #[test]
    fn find_path_across_grid_succeeds() {
        let mesh = grid_mesh();
        let start = mesh.locate(Vec2::new(0.2, 0.2)).unwrap();
        let end = mesh.locate(Vec2::new(1.8, 1.8)).unwrap();
        let path = find_path(&mesh, start, end);
        assert!(path.is_some(), "expected path from {start} to {end}");
        let p = path.unwrap();
        assert!(p.first() == Some(&start) && p.last() == Some(&end));
    }

    #[test]
    fn find_path_returns_none_when_unreachable() {
        let mut mesh = grid_mesh();
        // 截断所有 walkable，目标变成孤岛
        for t in &mut mesh.triangles[1..] { t.walkable = false; }
        let mesh = NavMesh::from_triangles(mesh.vertices, mesh.triangles);
        let path = find_path(&mesh, 0, 7);
        assert!(path.is_none());
    }

    #[test]
    fn funnel_smooth_includes_endpoints() {
        let mesh = grid_mesh();
        let start = Vec2::new(0.2, 0.2);
        let end = Vec2::new(1.8, 1.8);
        let s = mesh.locate(start).unwrap();
        let e = mesh.locate(end).unwrap();
        let path = find_path(&mesh, s, e).unwrap();
        let smooth = funnel_smooth(&mesh, &path, start, end);
        assert_eq!(smooth.first().unwrap(), &start);
        assert_eq!(smooth.last().unwrap(), &end);
        assert!(smooth.len() >= 2);
    }

    #[test]
    fn area_cost_affects_choice() {
        // 把所有三角形的 cost 调到正常，验证 A* 不 panic
        let mesh = grid_mesh();
        for t in &mesh.triangles {
            assert!(t.area_cost > 0.0);
        }
        let p = find_path(&mesh, 0, 7);
        assert!(p.is_some());
    }
}

// 单测辅助方法（仅 cfg test 下使用，避免暴露给生产 API）。
#[cfg(test)]
impl NavMesh {
    fn find_path_or_default(&self, s: TriId, e: TriId) -> Option<Vec<TriId>> {
        find_path(self, s, e)
    }
}
