//! Grid line vertex generation for the editor viewport.
//!
//! Generates an infinite-looking XZ-plane grid with:
//! - Adaptive LOD based on camera distance
//! - Distance-based alpha fading
//! - Edge fade-out for infinite appearance
//! - X axis (red) and Z axis (blue) highlighting

use crate::lines::LineVertex;
use cgmath::Point3;

/// Generate grid line vertices for the editor viewport XZ plane.
///
/// `eye` is the camera world position. `distance` is the camera-orbit distance.
pub fn build_grid_vertices(eye: Point3<f32>, distance: f32) -> Vec<LineVertex> {
    let cell_size = if distance < 5.0 { 0.5 }
        else if distance < 15.0 { 1.0 }
        else if distance < 50.0 { 5.0 }
        else if distance < 150.0 { 10.0 }
        else { 50.0 };

    let half_extent = (distance * 4.0).max(20.0).min(5000.0);
    let half_cells = (half_extent / cell_size).ceil() as i32;
    let extent = half_cells as f32 * cell_size;
    let major_step = 5;
    let camera_fade_dist = distance * 6.0;

    let minor_color = [0.31, 0.31, 0.31, 1.0];
    let major_color = [0.63, 0.63, 0.63, 1.0];
    let axis_x_color = [0.94, 0.27, 0.27, 1.0];
    let axis_z_color = [0.24, 0.35, 0.94, 1.0];

    let mut verts = Vec::new();

    let mut push_line = |p1: Point3<f32>, p2: Point3<f32>, color: [f32; 4]| {
        verts.push(LineVertex { position: [p1.x, p1.y, p1.z], color });
        verts.push(LineVertex { position: [p2.x, p2.y, p2.z], color });
    };

    let fade_alpha = |mx: f32, mz: f32, edge: f32, extent: f32| -> f32 {
        let dx = mx - eye.x;
        let dz = mz - eye.z;
        let cam_dist = (dx * dx + dz * dz).sqrt();
        let cam_alpha = 1.0 - (cam_dist / camera_fade_dist).clamp(0.0, 1.0);
        let edge_t = edge.abs() / extent;
        let edge_alpha = if edge_t < 0.75 { 1.0 }
            else { 1.0 - ((edge_t - 0.75) / 0.25).clamp(0.0, 1.0) };
        (cam_alpha * edge_alpha).max(0.0)
    };

    // X-direction lines (along X axis, Z varies)
    for i in -half_cells..=half_cells {
        let z = i as f32 * cell_size;
        let p1 = Point3::new(-extent, 0.0, z);
        let p2 = Point3::new(extent, 0.0, z);
        let alpha = fade_alpha(0.0, z, z, extent);
        if alpha < 0.02 { continue; }
        let is_center = i == 0;
        let is_major = is_center || i % major_step == 0;
        let base = if is_center { axis_x_color }
            else if is_major { major_color }
            else { minor_color };
        let color = [base[0], base[1], base[2], base[3] * alpha];
        push_line(p1, p2, color);
    }

    // Z-direction lines (along Z axis, X varies)
    for i in -half_cells..=half_cells {
        let x = i as f32 * cell_size;
        let p1 = Point3::new(x, 0.0, -extent);
        let p2 = Point3::new(x, 0.0, extent);
        let alpha = fade_alpha(x, 0.0, x, extent);
        if alpha < 0.02 { continue; }
        let is_center = i == 0;
        let is_major = is_center || i % major_step == 0;
        let base = if is_center { axis_z_color }
            else if is_major { major_color }
            else { minor_color };
        let color = [base[0], base[1], base[2], base[3] * alpha];
        push_line(p1, p2, color);
    }

    verts
}
