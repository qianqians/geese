//! 4.13 地形与流式加载骨架。
//!
//! 提供：
//! - `Heightmap`：基础高度图（width×height，存 f32）+ 双线性采样 + 法线计算
//! - `TerrainTile`：一个地形 tile（含 heightmap + 世界空间偏移 + LOD）
//! - `TerrainStreamer`：按相机位置 + 半径决定 tile 加/卸（不读盘，调用方注入 Loader）
//! - `TileCoord`：tile 坐标 `(x, z)`
//! - `compute_lod()`：geo-clipmap 风格 LOD 选择（按距离换算 mip level）
//!
//! 真实磁盘 IO / 网络流式留待接入时实现。

use std::collections::{HashMap, HashSet};

/// 单个高度图。坐标 (i, j)，i ∈ [0,width)，j ∈ [0,height)。
pub struct Heightmap {
    pub width: u32,
    pub height: u32,
    pub data: Vec<f32>,
}

impl Heightmap {
    pub fn new(width: u32, height: u32) -> Self {
        Self { width, height, data: vec![0.0; (width * height) as usize] }
    }
    pub fn from_data(width: u32, height: u32, data: Vec<f32>) -> Option<Self> {
        if data.len() != (width * height) as usize { return None; }
        Some(Self { width, height, data })
    }
    pub fn get(&self, i: u32, j: u32) -> f32 {
        let i = i.min(self.width - 1);
        let j = j.min(self.height - 1);
        self.data[(j * self.width + i) as usize]
    }
    pub fn set(&mut self, i: u32, j: u32, h: f32) {
        if i < self.width && j < self.height {
            self.data[(j * self.width + i) as usize] = h;
        }
    }
    /// uv 双线性采样，u/v ∈ [0,1]。
    pub fn sample_bilinear(&self, u: f32, v: f32) -> f32 {
        let u = u.clamp(0.0, 1.0) * (self.width - 1) as f32;
        let v = v.clamp(0.0, 1.0) * (self.height - 1) as f32;
        let i0 = u.floor() as u32;
        let j0 = v.floor() as u32;
        let i1 = (i0 + 1).min(self.width - 1);
        let j1 = (j0 + 1).min(self.height - 1);
        let fu = u - i0 as f32;
        let fv = v - j0 as f32;
        let h00 = self.get(i0, j0);
        let h10 = self.get(i1, j0);
        let h01 = self.get(i0, j1);
        let h11 = self.get(i1, j1);
        let h0 = h00 * (1.0 - fu) + h10 * fu;
        let h1 = h01 * (1.0 - fu) + h11 * fu;
        h0 * (1.0 - fv) + h1 * fv
    }
    /// 中心差分法线（XZ 平面格距 `cell` 米）。
    pub fn normal_at(&self, i: u32, j: u32, cell: f32) -> [f32; 3] {
        let hl = self.get(i.saturating_sub(1), j);
        let hr = self.get((i + 1).min(self.width - 1), j);
        let hd = self.get(i, j.saturating_sub(1));
        let hu = self.get(i, (j + 1).min(self.height - 1));
        let dx = hr - hl;
        let dz = hu - hd;
        // n = normalize((-dx, 2*cell, -dz))
        let v = [-dx, 2.0 * cell, -dz];
        let len = (v[0]*v[0] + v[1]*v[1] + v[2]*v[2]).sqrt().max(1e-6);
        [v[0] / len, v[1] / len, v[2] / len]
    }
}

/// Tile 坐标（grid 上的整数索引）。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TileCoord { pub x: i32, pub z: i32 }

impl TileCoord {
    pub fn new(x: i32, z: i32) -> Self { Self { x, z } }
}

/// 单个地形 tile。
pub struct TerrainTile {
    pub coord: TileCoord,
    pub heightmap: Heightmap,
    /// 世界空间的左下角原点（XZ 平面）。
    pub world_origin: [f32; 2],
    /// 当前 LOD：0 = 最高精度，数字越大越粗糙。
    pub lod: u8,
}

/// Tile 加载器 trait。后续接入磁盘/网络后端。
pub trait TileLoader {
    fn load(&mut self, coord: TileCoord) -> Option<TerrainTile>;
    fn unload(&mut self, coord: TileCoord);
}

/// 占位 loader：仅记录调用次数。
pub struct NullTileLoader {
    pub loaded: HashSet<TileCoord>,
    pub load_calls: usize,
    pub unload_calls: usize,
}

impl NullTileLoader {
    pub fn new() -> Self { Self { loaded: HashSet::new(), load_calls: 0, unload_calls: 0 } }
}

impl Default for NullTileLoader {
    fn default() -> Self { Self::new() }
}

impl TileLoader for NullTileLoader {
    fn load(&mut self, coord: TileCoord) -> Option<TerrainTile> {
        self.load_calls += 1;
        self.loaded.insert(coord);
        Some(TerrainTile {
            coord,
            heightmap: Heightmap::new(8, 8),
            world_origin: [coord.x as f32 * 64.0, coord.z as f32 * 64.0],
            lod: 0,
        })
    }
    fn unload(&mut self, coord: TileCoord) {
        self.unload_calls += 1;
        self.loaded.remove(&coord);
    }
}

/// Geo-clipmap LOD 选择：距离越远 LOD 越粗，每翻倍距离 +1 级，封顶 max_lod。
pub fn compute_lod(distance: f32, tile_size: f32, max_lod: u8) -> u8 {
    if distance <= tile_size || tile_size <= 0.0 { return 0; }
    let ratio = distance / tile_size;
    let lod = ratio.log2().floor().max(0.0) as u32;
    lod.min(max_lod as u32) as u8
}

/// Streamer：按相机位置 + 视野半径决定要 load 哪些 tile。
pub struct TerrainStreamer {
    pub tile_size: f32,
    pub view_radius_tiles: i32,
    pub max_lod: u8,
    active: HashMap<TileCoord, u8>,
}

impl TerrainStreamer {
    pub fn new(tile_size: f32, view_radius_tiles: i32, max_lod: u8) -> Self {
        Self {
            tile_size: tile_size.max(1.0),
            view_radius_tiles: view_radius_tiles.max(1),
            max_lod,
            active: HashMap::new(),
        }
    }

    pub fn active_tiles(&self) -> impl Iterator<Item = (&TileCoord, &u8)> { self.active.iter() }
    pub fn active_count(&self) -> usize { self.active.len() }

    /// 按相机位置（世界 XZ）更新激活 tile 集，调用 loader 加/卸。
    pub fn update<L: TileLoader>(&mut self, camera_xz: [f32; 2], loader: &mut L) {
        let cx = (camera_xz[0] / self.tile_size).floor() as i32;
        let cz = (camera_xz[1] / self.tile_size).floor() as i32;
        let r = self.view_radius_tiles;

        let mut desired: HashMap<TileCoord, u8> = HashMap::new();
        for dz in -r..=r {
            for dx in -r..=r {
                let coord = TileCoord::new(cx + dx, cz + dz);
                let center = [
                    (coord.x as f32 + 0.5) * self.tile_size,
                    (coord.z as f32 + 0.5) * self.tile_size,
                ];
                let d = ((center[0] - camera_xz[0]).powi(2)
                       + (center[1] - camera_xz[1]).powi(2)).sqrt();
                desired.insert(coord, compute_lod(d, self.tile_size, self.max_lod));
            }
        }

        // 卸载不再需要的
        let to_unload: Vec<TileCoord> = self.active.keys()
            .filter(|k| !desired.contains_key(k))
            .copied().collect();
        for c in to_unload {
            loader.unload(c);
            self.active.remove(&c);
        }
        // 加载新出现的
        for (c, lod) in desired {
            self.active.entry(c).or_insert_with(|| {
                let _ = loader.load(c);
                lod
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn heightmap_get_set_roundtrip() {
        let mut h = Heightmap::new(4, 4);
        h.set(2, 3, 1.5);
        assert!((h.get(2, 3) - 1.5).abs() < 1e-5);
    }

    #[test]
    fn bilinear_at_corners_returns_corner_values() {
        let mut h = Heightmap::new(2, 2);
        h.set(0, 0, 0.0);
        h.set(1, 0, 10.0);
        h.set(0, 1, 20.0);
        h.set(1, 1, 30.0);
        assert!((h.sample_bilinear(0.0, 0.0) - 0.0).abs() < 1e-4);
        assert!((h.sample_bilinear(1.0, 1.0) - 30.0).abs() < 1e-4);
        // 中心 = 15
        assert!((h.sample_bilinear(0.5, 0.5) - 15.0).abs() < 1e-4);
    }

    #[test]
    fn normal_of_flat_terrain_points_up() {
        let h = Heightmap::new(4, 4);
        let n = h.normal_at(1, 1, 1.0);
        assert!(n[1] > 0.99);
    }

    #[test]
    fn compute_lod_close_is_zero() {
        assert_eq!(compute_lod(10.0, 64.0, 4), 0);
        assert_eq!(compute_lod(64.0, 64.0, 4), 0);
    }

    #[test]
    fn compute_lod_doubles_per_octave() {
        assert_eq!(compute_lod(128.0, 64.0, 4), 1);
        assert_eq!(compute_lod(256.0, 64.0, 4), 2);
        assert_eq!(compute_lod(2048.0, 64.0, 4), 4); // 封顶
    }

    #[test]
    fn streamer_loads_view_radius_tiles() {
        let mut s = TerrainStreamer::new(10.0, 1, 4);
        let mut loader = NullTileLoader::new();
        s.update([5.0, 5.0], &mut loader);
        // r=1 → 3x3 = 9 tile
        assert_eq!(s.active_count(), 9);
        assert_eq!(loader.load_calls, 9);
    }

    #[test]
    fn streamer_unloads_when_moving_far_away() {
        let mut s = TerrainStreamer::new(10.0, 1, 4);
        let mut loader = NullTileLoader::new();
        s.update([5.0, 5.0], &mut loader);
        s.update([1000.0, 1000.0], &mut loader);
        // 原 9 个应被卸载，新 9 个加载
        assert!(loader.unload_calls >= 9);
        assert_eq!(s.active_count(), 9);
    }

    #[test]
    fn from_data_rejects_size_mismatch() {
        assert!(Heightmap::from_data(2, 2, vec![1.0; 3]).is_none());
        assert!(Heightmap::from_data(2, 2, vec![1.0; 4]).is_some());
    }
}
