//! Perspective warp via Direct Linear Transform homography.

pub fn apply_perspective_warp(
    pixels: &[f32], w: u32, h: u32,
    src_pts: [[f32; 2]; 4], dst_pts: [[f32; 2]; 4],
) -> Vec<f32> {
    // Map output (dst) coords back to source: compute H from dst->src.
    let h_mat = compute_homography(dst_pts, src_pts);
    let mut out = vec![0.0f32; pixels.len()];
    for py in 0..h {
        for px_x in 0..w {
            let wx = px_x as f32 + 0.5;
            let wy = py as f32 + 0.5;
            let [sx, sy] = homo_apply(&h_mat, wx, wy);
            let s = bilinear(pixels, w, h, sx - 0.5, sy - 0.5);
            let i = ((py * w + px_x) * 4) as usize;
            out[i..i + 4].copy_from_slice(&s);
        }
    }
    out
}

fn compute_homography(src: [[f32; 2]; 4], dst: [[f32; 2]; 4]) -> [f32; 9] {
    let mut a = [[0.0f64; 8]; 8];
    let mut b = [0.0f64; 8];
    for (i, (s, d)) in src.iter().zip(dst.iter()).enumerate() {
        let (sx, sy) = (s[0] as f64, s[1] as f64);
        let (dx, dy) = (d[0] as f64, d[1] as f64);
        let r1 = i * 2;
        let r2 = i * 2 + 1;
        a[r1] = [sx, sy, 1.0, 0.0, 0.0, 0.0, -dx * sx, -dx * sy];
        b[r1] = dx;
        a[r2] = [0.0, 0.0, 0.0, sx, sy, 1.0, -dy * sx, -dy * sy];
        b[r2] = dy;
    }
    let h = gaussian_solve(&mut a, &mut b);
    [
        h[0] as f32, h[1] as f32, h[2] as f32,
        h[3] as f32, h[4] as f32, h[5] as f32,
        h[6] as f32, h[7] as f32, 1.0,
    ]
}

fn gaussian_solve(a: &mut [[f64; 8]; 8], b: &mut [f64; 8]) -> [f64; 8] {
    let n = 8;
    for col in 0..n {
        let mut max_row = col;
        for row in col + 1..n {
            if a[row][col].abs() > a[max_row][col].abs() { max_row = row; }
        }
        a.swap(col, max_row);
        b.swap(col, max_row);
        let pivot = a[col][col];
        if pivot.abs() < 1e-12 { continue; }
        for row in col + 1..n {
            let factor = a[row][col] / pivot;
            b[row] -= factor * b[col];
            for c in col..n { a[row][c] -= factor * a[col][c]; }
        }
    }
    let mut x = [0.0f64; 8];
    for i in (0..n).rev() {
        x[i] = b[i];
        for j in i + 1..n { x[i] -= a[i][j] * x[j]; }
        if a[i][i].abs() > 1e-12 { x[i] /= a[i][i]; }
    }
    x
}

fn homo_apply(h: &[f32; 9], x: f32, y: f32) -> [f32; 2] {
    let ww = h[6] * x + h[7] * y + h[8];
    let wx = h[0] * x + h[1] * y + h[2];
    let wy = h[3] * x + h[4] * y + h[5];
    if ww.abs() < 1e-8 { return [x, y]; }
    [wx / ww, wy / ww]
}

fn bilinear(px: &[f32], w: u32, h: u32, x: f32, y: f32) -> [f32; 4] {
    let (wi, hi) = (w as i32, h as i32);
    let x0 = x.floor() as i32;
    let y0 = y.floor() as i32;
    let (tx, ty) = (x - x0 as f32, y - y0 as f32);
    let mut out = [0.0f32; 4];
    for dy in 0..2i32 {
        for dx in 0..2i32 {
            let sx = (x0 + dx).clamp(0, wi - 1);
            let sy = (y0 + dy).clamp(0, hi - 1);
            let wb = (if dx == 0 { 1.0 - tx } else { tx }) * (if dy == 0 { 1.0 - ty } else { ty });
            let i = ((sy as u32 * w + sx as u32) * 4) as usize;
            for c in 0..4 { out[c] += px[i + c] * wb; }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flat(w: u32, h: u32, c: [f32; 4]) -> Vec<f32> {
        (0..w * h).flat_map(|_| c).collect()
    }

    /// The four corners of a w×h image in pixel-centre coordinates.
    fn corners(w: u32, h: u32) -> [[f32; 2]; 4] {
        let (fw, fh) = (w as f32, h as f32);
        [[0.5, 0.5], [fw - 0.5, 0.5], [0.5, fh - 0.5], [fw - 0.5, fh - 0.5]]
    }

    #[test]
    fn identity_warp_preserves_flat_field() {
        let p = flat(8, 8, [0.4, 0.6, 0.2, 1.0]);
        let c = corners(8, 8);
        let out = apply_perspective_warp(&p, 8, 8, c, c);
        for (a, b) in out.iter().zip(p.iter()) {
            assert!((a - b).abs() < 1e-2, "pixel drifted: {a} vs {b}");
        }
    }

    #[test]
    fn flat_field_preserved_under_projective_warp() {
        // A uniform field warped to any quad still returns the same colour (bilinear
        // samples from the same value everywhere).
        let p = flat(10, 10, [0.7, 0.3, 0.5, 1.0]);
        let src = corners(10, 10);
        // Slightly skewed quad — not a pure translate/scale, so it exercises the
        // full homography path.
        let dst = [
            [1.0, 1.0],
            [9.5, 0.5],
            [0.5, 9.5],
            [9.0, 9.0f32],
        ];
        let out = apply_perspective_warp(&p, 10, 10, src, dst);
        for px in out.chunks(4) {
            assert!((px[0] - 0.7).abs() < 1e-2, "r drifted: {}", px[0]);
            assert!((px[3] - 1.0).abs() < 1e-3, "alpha changed");
        }
    }

    #[test]
    fn homography_identity_maps_corners_to_themselves() {
        // compute_homography with matching src/dst should give a near-identity H.
        let c = corners(8, 8);
        let h = super::compute_homography(c, c);
        // Identity H: [[1,0,0],[0,1,0],[0,0,1]] up to scale.
        // homo_apply on a known point should round-trip within 1px.
        for pt in &c {
            let [rx, ry] = super::homo_apply(&h, pt[0], pt[1]);
            assert!((rx - pt[0]).abs() < 0.5, "x drifted: {rx} vs {}", pt[0]);
            assert!((ry - pt[1]).abs() < 0.5, "y drifted: {ry} vs {}", pt[1]);
        }
    }

    #[test]
    fn gaussian_solve_solves_simple_system() {
        // 8×8 identity system: A = I, b = [1,2,3,4,5,6,7,8] → x = b.
        let mut a = [[0.0f64; 8]; 8];
        for i in 0..8 { a[i][i] = 1.0; }
        let mut b = [1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0f64];
        let x = super::gaussian_solve(&mut a, &mut b);
        for (i, &xi) in x.iter().enumerate() {
            assert!((xi - (i as f64 + 1.0)).abs() < 1e-9, "x[{i}] = {xi}");
        }
    }

    #[test]
    fn output_same_size_as_input() {
        let p = flat(6, 4, [0.5, 0.5, 0.5, 1.0]);
        let c = corners(6, 4);
        let out = apply_perspective_warp(&p, 6, 4, c, c);
        assert_eq!(out.len(), p.len());
    }
}
