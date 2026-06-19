//! Mesh gradient rasterizer: 4×4 bilinear-interpolated control-point grid.

/// Seed a 4×4 mesh-gradient control grid covering `bbox` `[x, y, w, h]` (document
/// space). Corner / edge nodes carry `base` colour; the four interior nodes are
/// brightened toward white so a freshly-made mesh reads as a soft highlight in the
/// centre (Illustrator's `Object ▸ Create Gradient Mesh` default), giving the user
/// an editable starting point. Returns the 16 `((x, y), rgba)` nodes row-major.
pub fn seed_grid(bbox: [f32; 4], base: [f32; 4]) -> Vec<((f32, f32), [f32; 4])> {
    let [x, y, w, h] = bbox;
    let mut out = Vec::with_capacity(16);
    // Interior highlight: lift each channel a third toward white, keep alpha.
    let hi = [
        base[0] + (1.0 - base[0]) * 0.5,
        base[1] + (1.0 - base[1]) * 0.5,
        base[2] + (1.0 - base[2]) * 0.5,
        base[3],
    ];
    for row in 0..4usize {
        for col in 0..4usize {
            let px = x + w * (col as f32 / 3.0);
            let py = y + h * (row as f32 / 3.0);
            let interior = (1..=2).contains(&row) && (1..=2).contains(&col);
            out.push(((px, py), if interior { hi } else { base }));
        }
    }
    out
}


/// Render a mesh gradient as a flat RGBA8 image `(canvas_w × canvas_h)`.
///
/// `mesh_points` is a 4×4 grid (16 entries, row-major: `[row*4+col]`) of
/// `((x, y), [r, g, b, a])` control points in **canvas pixel coordinates**
/// (same space as `artboard_origin + doc_pt`). Each of the 3×3 quads is
/// rasterized by bilinearly interpolating the four corner colors over the
/// quad's bounding box pixels.
///
/// Returns a `canvas_w × canvas_h` RGBA8 image (straight, not premultiplied).
/// When `mesh_points.len() < 16`, returns a zeroed (fully transparent) image.
pub fn render_mesh_overlay(
    mesh_points: &[((f32, f32), [f32; 4])],
    canvas_w: u32,
    canvas_h: u32,
) -> Vec<u8> {
    let mut out = vec![0u8; (canvas_w * canvas_h * 4) as usize];
    if mesh_points.len() < 16 || canvas_w == 0 || canvas_h == 0 {
        return out;
    }
    for row in 0..3usize {
        for col in 0..3usize {
            let p00 = mesh_points[row * 4 + col];
            let p01 = mesh_points[row * 4 + col + 1];
            let p10 = mesh_points[(row + 1) * 4 + col];
            let p11 = mesh_points[(row + 1) * 4 + col + 1];
            let xs = [p00.0.0, p01.0.0, p10.0.0, p11.0.0];
            let ys = [p00.0.1, p01.0.1, p10.0.1, p11.0.1];
            let x_min = xs.iter().cloned().fold(f32::MAX, f32::min).max(0.0) as u32;
            let x_max = (xs.iter().cloned().fold(f32::MIN, f32::max).ceil() as u32)
                .min(canvas_w.saturating_sub(1));
            let y_min = ys.iter().cloned().fold(f32::MAX, f32::min).max(0.0) as u32;
            let y_max = (ys.iter().cloned().fold(f32::MIN, f32::max).ceil() as u32)
                .min(canvas_h.saturating_sub(1));
            let span_x = (xs[1] - xs[0]).abs().max((xs[3] - xs[2]).abs()).max(1.0);
            let span_y = (ys[2] - ys[0]).abs().max((ys[3] - ys[1]).abs()).max(1.0);
            let origin_x = xs[0].min(xs[2]);
            let origin_y = ys[0].min(ys[1]);
            for py in y_min..=y_max {
                for px in x_min..=x_max {
                    let u = ((px as f32 - origin_x) / span_x).clamp(0.0, 1.0);
                    let v = ((py as f32 - origin_y) / span_y).clamp(0.0, 1.0);
                    let c = bilerp(p00.1, p01.1, p10.1, p11.1, u, v);
                    let idx = (py * canvas_w + px) as usize * 4;
                    if idx + 3 < out.len() {
                        let src_a = c[3].clamp(0.0, 1.0);
                        let dst_a = out[idx + 3] as f32 / 255.0;
                        let out_a = src_a + dst_a * (1.0 - src_a);
                        if out_a > 0.0 {
                            let blend =
                                |s: f32, d: f32| (s * src_a + d * dst_a * (1.0 - src_a)) / out_a;
                            out[idx] =
                                (blend(c[0], out[idx] as f32 / 255.0) * 255.0).round() as u8;
                            out[idx + 1] =
                                (blend(c[1], out[idx + 1] as f32 / 255.0) * 255.0).round() as u8;
                            out[idx + 2] =
                                (blend(c[2], out[idx + 2] as f32 / 255.0) * 255.0).round() as u8;
                            out[idx + 3] = (out_a * 255.0).round() as u8;
                        }
                    }
                }
            }
        }
    }
    out
}

/// Bilinear interpolation of four RGBA colors at (u, v) in [0,1]^2.
/// Corners: (0,0)=c00, (1,0)=c01, (0,1)=c10, (1,1)=c11.
fn bilerp(
    c00: [f32; 4],
    c01: [f32; 4],
    c10: [f32; 4],
    c11: [f32; 4],
    u: f32,
    v: f32,
) -> [f32; 4] {
    let lerp4 = |a: [f32; 4], b: [f32; 4], t: f32| -> [f32; 4] {
        [
            a[0] + (b[0] - a[0]) * t,
            a[1] + (b[1] - a[1]) * t,
            a[2] + (b[2] - a[2]) * t,
            a[3] + (b[3] - a[3]) * t,
        ]
    };
    let top = lerp4(c00, c01, u);
    let bot = lerp4(c10, c11, u);
    lerp4(top, bot, v)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seed_grid_has_16_nodes_in_bbox() {
        let g = seed_grid([10.0, 20.0, 100.0, 50.0], [0.2, 0.4, 0.8, 1.0]);
        assert_eq!(g.len(), 16);
        // First node at the bbox origin, last at the far corner.
        assert_eq!(g[0].0, (10.0, 20.0));
        assert_eq!(g[15].0, (110.0, 70.0));
        // Interior nodes are lighter than the base.
        let interior = g[5].1; // row 1, col 1
        assert!(interior[0] > 0.2, "interior brightened");
    }

    #[test]
    fn seed_grid_renders_non_transparent_overlay() {
        let g = seed_grid([0.0, 0.0, 8.0, 8.0], [0.2, 0.4, 0.8, 1.0]);
        let img = render_mesh_overlay(&g, 8, 8);
        assert!(img.iter().skip(3).step_by(4).any(|&a| a > 0), "some alpha set");
    }
}
