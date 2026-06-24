//! Stylize filters (Phase 8) — Sobel edge / relief, an oil-paint Kuwahara
//! quadrant filter, and a seeded diffuse scramble. CPU references mirroring
//! `filter.wgsl`'s kinds 13–16/27. All operate in the compositor working space:
//! **linear premultiplied** RGBA `[f32; 4]`.

use super::{sample_bilinear, TAU};

/// Rec.709 luma of a premultiplied RGB triple.
fn luma(p: [f32; 4]) -> f32 {
    0.2126 * p[0] + 0.7152 * p[1] + 0.0722 * p[2]
}

/// The 3×3 Sobel gradient `(gx, gy)` of the luma field at `(x, y)`, sampling a
/// `width`-px step with edge-clamped bilinear taps (matches the WGSL Sobel).
fn sobel_grad(img: &[[f32; 4]], w: usize, h: usize, x: usize, y: usize, width: f32) -> (f32, f32) {
    let (fx, fy) = (x as f32, y as f32);
    let s = width.max(1.0);
    let l = |dx: f32, dy: f32| luma(sample_bilinear(img, w, h, fx + dx * s, fy + dy * s));
    let (tl, tc, tr) = (l(-1.0, -1.0), l(0.0, -1.0), l(1.0, -1.0));
    let (ml, mr) = (l(-1.0, 0.0), l(1.0, 0.0));
    let (bl, bc, br) = (l(-1.0, 1.0), l(0.0, 1.0), l(1.0, 1.0));
    let gx = (tr + 2.0 * mr + br) - (tl + 2.0 * ml + bl);
    let gy = (bl + 2.0 * bc + br) - (tl + 2.0 * tc + tr);
    (gx, gy)
}

/// Find Edges: the Sobel gradient magnitude inverted to a white background with
/// dark edges (Photoshop-style). Output is a gray premultiplied RGBA keeping the
/// source alpha. `width` is the Sobel sampling step in pixels.
pub fn find_edges(img: &[[f32; 4]], w: usize, h: usize, width: f32) -> Vec<[f32; 4]> {
    let mut out = vec![[0.0f32; 4]; w * h];
    for y in 0..h {
        for x in 0..w {
            let (gx, gy) = sobel_grad(img, w, h, x, y, width);
            let mag = (gx * gx + gy * gy).sqrt();
            let a = img[y * w + x][3];
            let v = (1.0 - mag).clamp(0.0, 1.0);
            out[y * w + x] = [v * a, v * a, v * a, a];
        }
    }
    out
}

/// Emboss: a directional gray relief. The luma gradient projected onto the
/// `(dir_x, dir_y)` light direction, scaled by `amount`, biased to mid-gray.
/// Output is gray premultiplied RGBA keeping the source alpha. `width` is the
/// Sobel sampling step in pixels.
pub fn emboss(
    img: &[[f32; 4]],
    w: usize,
    h: usize,
    dir_x: f32,
    dir_y: f32,
    amount: f32,
    width: f32,
) -> Vec<[f32; 4]> {
    let mut out = vec![[0.0f32; 4]; w * h];
    for y in 0..h {
        for x in 0..w {
            let (gx, gy) = sobel_grad(img, w, h, x, y, width);
            let a = img[y * w + x][3];
            let v = (0.5 + (gx * dir_x + gy * dir_y) * amount).clamp(0.0, 1.0);
            out[y * w + x] = [v * a, v * a, v * a, a];
        }
    }
    out
}

/// Glowing Edges: bright coloured edges on black. The edge magnitude (boosted by
/// `brightness`) modulates the source colour, so flats go black and edges glow.
/// `width` is the Sobel sampling step in pixels. Output is premultiplied RGBA.
pub fn glowing_edges(
    img: &[[f32; 4]],
    w: usize,
    h: usize,
    brightness: f32,
    width: f32,
) -> Vec<[f32; 4]> {
    let mut out = vec![[0.0f32; 4]; w * h];
    for y in 0..h {
        for x in 0..w {
            let (gx, gy) = sobel_grad(img, w, h, x, y, width);
            let mag = (gx * gx + gy * gy).sqrt();
            let g = (mag * brightness).clamp(0.0, 1.0);
            let mc = img[y * w + x];
            let a = mc[3].max(1e-4);
            // Unpremultiply → scale by edge strength → re-premultiply.
            let col = [mc[0] / a * g, mc[1] / a * g, mc[2] / a * g];
            out[y * w + x] = [col[0] * mc[3], col[1] * mc[3], col[2] * mc[3], mc[3]];
        }
    }
    out
}

/// Oil Paint (Kuwahara quadrant filter, kind 27): split the `(2r+1)²` window
/// around each pixel into four overlapping `(r+1)×(r+1)` quadrants sharing the
/// centre, then output the mean colour of the quadrant with the lowest luma
/// variance. Choosing the flattest quadrant smooths interiors while snapping to
/// one side of an edge, giving painterly patches with crisp boundaries. Operates
/// in premultiplied space (matching the blur/mosaic convention) with edge-clamped
/// sampling, so the CPU reference matches the WGSL pass.
pub fn oil_paint(img: &[[f32; 4]], w: usize, h: usize, radius: usize) -> Vec<[f32; 4]> {
    let r = radius.clamp(1, 8) as i32;
    // The four quadrant offset ranges (relative to the centre pixel), each
    // sharing the centre row/column so the window is fully covered.
    let quads: [(i32, i32, i32, i32); 4] = [
        (-r, 0, -r, 0),
        (0, r, -r, 0),
        (-r, 0, 0, r),
        (0, r, 0, r),
    ];
    let mut out = vec![[0.0f32; 4]; w * h];
    for y in 0..h {
        for x in 0..w {
            let mut best_var = f32::INFINITY;
            let mut best_mean = img[y * w + x];
            for &(dx0, dx1, dy0, dy1) in &quads {
                let mut sum = [0.0f32; 4];
                let mut lsum = 0.0f32;
                let mut l2sum = 0.0f32;
                let mut n = 0.0f32;
                for dy in dy0..=dy1 {
                    for dx in dx0..=dx1 {
                        let sx = (x as i32 + dx).clamp(0, w as i32 - 1) as usize;
                        let sy = (y as i32 + dy).clamp(0, h as i32 - 1) as usize;
                        let p = img[sy * w + sx];
                        let lm = luma(p);
                        for c in 0..4 {
                            sum[c] += p[c];
                        }
                        lsum += lm;
                        l2sum += lm * lm;
                        n += 1.0;
                    }
                }
                let lmean = lsum / n;
                let variance = l2sum / n - lmean * lmean;
                if variance < best_var {
                    best_var = variance;
                    best_mean = [sum[0] / n, sum[1] / n, sum[2] / n, sum[3] / n];
                }
            }
            out[y * w + x] = best_mean;
        }
    }
    out
}

/// Diffuse: a seeded-deterministic anisotropic neighbour scramble — each pixel
/// is replaced by a neighbour up to `amount` px away, the offset chosen by a
/// hash of `(x, y, seed)` (matching the WGSL `fract(sin(..))` hash). Stable for
/// a given seed (no temporal noise). Output is edge-clamp-sampled.
// The hash multipliers mirror the canonical GLSL/WGSL `fract(sin(..))` hash bit
// for bit, so the CPU reference matches the shader — keep them verbatim.
#[allow(clippy::excessive_precision)]
pub fn diffuse(img: &[[f32; 4]], w: usize, h: usize, amount: f32, seed: f32) -> Vec<[f32; 4]> {
    let mut out = vec![[0.0f32; 4]; w * h];
    for y in 0..h {
        for x in 0..w {
            let (fx, fy) = (x as f32, y as f32);
            let hf = ((fx * 12.9898 + fy * 78.233 + seed).sin() * 43758.5453).fract();
            let hg = ((fx * 39.3468 + fy * 11.135 + seed).sin() * 24634.6345).fract();
            let ang = hf * TAU;
            let dist = hg * amount.max(0.0);
            let sx = fx + ang.cos() * dist;
            let sy = fy + ang.sin() * dist;
            out[y * w + x] = sample_bilinear(img, w, h, sx, sy);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::filter_math::test_support::{approx, vsplit};

    // ---- Stylize ----------------------------------------------------------

    #[test]
    fn find_edges_darkens_the_edge_and_whitens_flats() {
        let n = 9;
        let img = vsplit(n);
        let out = find_edges(&img, n, n, 1.0);
        let row = n / 2;
        // A flat interior column (far from the edge) → no gradient → white (≈1).
        let flat = out[row * n + 1][0];
        // The edge column → strong gradient → dark (≪ flat). Alpha preserved.
        let edge = out[row * n + n / 2][0];
        assert!(flat > 0.9, "flat should be near white, got {flat}");
        assert!(edge < flat - 0.3, "edge {edge} should be far darker than flat {flat}");
        assert!(approx(out[row * n + n / 2][3], 1.0, 1e-6), "alpha preserved");
    }

    #[test]
    fn emboss_is_directional() {
        let n = 9;
        let img = vsplit(n);
        let row = n / 2;
        // Light along the gradient (horizontal) shifts the edge away from mid-gray.
        let horiz = emboss(&img, n, n, 1.0, 0.0, 0.2, 1.0);
        // Light perpendicular to the gradient (vertical) leaves the edge ≈ mid-gray.
        let vert = emboss(&img, n, n, 0.0, 1.0, 0.2, 1.0);
        let edge_h = horiz[row * n + n / 2][0];
        let edge_v = vert[row * n + n / 2][0];
        assert!((edge_h - 0.5).abs() > 0.05, "horizontal light should relieve the edge");
        assert!(approx(edge_v, 0.5, 0.05), "vertical light leaves a vertical edge flat");
        // A flat interior column is mid-gray regardless of light direction.
        assert!(approx(horiz[row * n + 1][0], 0.5, 1e-3), "flat → mid-gray");
    }

    #[test]
    fn glowing_edges_lights_edges_blacks_flats() {
        let n = 9;
        let img = vsplit(n);
        let row = n / 2;
        let out = glowing_edges(&img, n, n, 2.0, 1.0);
        let flat = out[row * n + 1][0];
        let edge = out[row * n + n / 2][0];
        assert!(flat < 0.05, "flat region goes black, got {flat}");
        assert!(edge > flat, "edge glows brighter than the flat, {edge} vs {flat}");
    }

    #[test]
    fn diffuse_is_deterministic_and_amount_zero_is_identity() {
        let n = 9;
        let img = vsplit(n);
        let a = diffuse(&img, n, n, 3.0, 1.0);
        let b = diffuse(&img, n, n, 3.0, 1.0);
        assert_eq!(a, b, "same seed → identical output");
        let c = diffuse(&img, n, n, 3.0, 2.0);
        assert!(a != c, "a different seed changes the field");
        let id = diffuse(&img, n, n, 0.0, 1.0);
        assert_eq!(id, img, "amount 0 is identity");
    }

    #[test]
    fn oil_paint_is_flat_field_identity() {
        // A uniform field has zero variance in every quadrant, so the mean it
        // outputs equals the (constant) source colour — oil paint is identity.
        let n = 9;
        let img = vec![[0.3, 0.6, 0.2, 1.0]; n * n];
        let out = oil_paint(&img, n, n, 2);
        for p in &out {
            assert!(approx(p[0], 0.3, 1e-4) && approx(p[1], 0.6, 1e-4), "flat → unchanged: {p:?}");
        }
    }

    #[test]
    fn oil_paint_picks_a_flat_quadrant_and_sharpens_the_edge() {
        // On a vertical black/white step, an edge pixel's quadrants straddling
        // the boundary have high variance; the flat (all-black or all-white)
        // quadrant wins, so the output snaps to a pure side colour — never the
        // ~0.5 average a plain box blur would give. Output stays opaque.
        let n = 9;
        let img = vsplit(n);
        let out = oil_paint(&img, n, n, 2);
        let row = n / 2;
        for x in 0..n {
            let v = out[row * n + x][0];
            assert!(
                !(0.01..=0.99).contains(&v),
                "every pixel snaps to a pure side, never a blend: {v} at x={x}",
            );
            assert!(approx(out[row * n + x][3], 1.0, 1e-4), "alpha preserved");
        }
        // The far-left stays black, the far-right stays white.
        assert!(out[row * n][0] < 0.01, "left side stays black");
        assert!(out[row * n + (n - 1)][0] > 0.99, "right side stays white");
    }
}
