//! Pixelate filters — CPU references mirroring `filter.wgsl`'s kinds 20–23.
//! All operate in the compositor working space: **linear premultiplied** RGBA.

use super::hash21;

/// Mosaic (kind 20): average each `cell`×`cell` block to one colour (the true
/// block mean of the premultiplied RGBA — a partially transparent block is
/// alpha-weighted correctly). Every pixel in a block gets that block's average,
/// so a block is uniform. Mirrors the WGSL block walk (edge-clamped, integer
/// pixel centres).
pub fn mosaic(img: &[[f32; 4]], w: usize, h: usize, cell: usize) -> Vec<[f32; 4]> {
    let cell = cell.max(1);
    let mut out = vec![[0.0f32; 4]; w * h];
    for y in 0..h {
        for x in 0..w {
            let bx = (x / cell) * cell;
            let by = (y / cell) * cell;
            let mut sum = [0.0f32; 4];
            let mut n = 0.0f32;
            for j in 0..cell {
                for i in 0..cell {
                    let sx = (bx + i).min(w - 1);
                    let sy = (by + j).min(h - 1);
                    let p = img[sy * w + sx];
                    for c in 0..4 {
                        sum[c] += p[c];
                    }
                    n += 1.0;
                }
            }
            out[y * w + x] = [sum[0] / n, sum[1] / n, sum[2] / n, sum[3] / n];
        }
    }
    out
}

/// Crystallize (kind 21): snap each pixel to the colour at its nearest jittered
/// seed point. One seed per `cell`×`cell` block, offset within the block by a
/// hash of the block index + `seed`; the 3×3 block neighbourhood is searched so
/// an adjacent jittered seed can win, giving irregular Voronoi cells. Stable for
/// a given seed (matches the WGSL hash). Edge-clamped nearest sampling.
pub fn crystallize(img: &[[f32; 4]], w: usize, h: usize, cell: usize, seed: f32) -> Vec<[f32; 4]> {
    let cellf = cell.max(1) as f32;
    let mut out = vec![[0.0f32; 4]; w * h];
    for y in 0..h {
        for x in 0..w {
            let px = x as f32;
            let py = y as f32;
            let cx = (px / cellf).floor();
            let cy = (py / cellf).floor();
            let mut best = f32::INFINITY;
            let mut bestc = [px, py];
            for gy in -1..=1 {
                for gx in -1..=1 {
                    let bx = cx + gx as f32;
                    let by = cy + gy as f32;
                    let jx = hash21(bx, by, seed);
                    let jy = hash21(bx, by, seed + 41.0);
                    let sx = (bx + jx) * cellf;
                    let sy = (by + jy) * cellf;
                    let dx = sx - px;
                    let dy = sy - py;
                    let dist = dx * dx + dy * dy;
                    if dist < best {
                        best = dist;
                        bestc = [sx, sy];
                    }
                }
            }
            // Snap the winning seed to its integer pixel centre, edge-clamped,
            // then nearest-sample — a true snap to one source colour (no blend).
            let sx = (bestc[0].floor() as i32).clamp(0, w as i32 - 1) as usize;
            let sy = (bestc[1].floor() as i32).clamp(0, h as i32 - 1) as usize;
            out[y * w + x] = img[sy * w + sx];
        }
    }
    out
}

/// Color Halftone (kind 22): a per-channel dot screen. Tile into `cell`×`cell`
/// cells rotated by `angle_rad`; each cell's channel average sets a dot radius
/// (darker channel → bigger dot of full ink). Inside the dot the channel is 0
/// (full ink), outside 1 (paper). Output is premultiplied, keeping source alpha.
/// Mirrors the WGSL screen math (per-channel 22.5° angle offset).
pub fn color_halftone(
    img: &[[f32; 4]],
    w: usize,
    h: usize,
    cell: usize,
    angle_rad: f32,
) -> Vec<[f32; 4]> {
    let cellf = (cell.max(2)) as f32;
    let ca = angle_rad.cos();
    let sa = angle_rad.sin();
    let mut out = vec![[0.0f32; 4]; w * h];
    for y in 0..h {
        for x in 0..w {
            let pxx = x as f32;
            let pxy = y as f32;
            let mc = img[y * w + x];
            let mut col = [0.0f32; 3];
            for (ch, cv) in col.iter_mut().enumerate() {
                let off = ch as f32 * std::f32::consts::FRAC_PI_8; // 22.5°
                let cc = off.cos() * ca - off.sin() * sa;
                let ss = off.sin() * ca + off.cos() * sa;
                let rx = pxx * cc - pxy * ss;
                let ry = pxx * ss + pxy * cc;
                let cellx = (rx / cellf).floor();
                let celly = (ry / cellf).floor();
                let cellcx = (cellx + 0.5) * cellf;
                let cellcy = (celly + 0.5) * cellf;
                // Average this channel over the cell (sampling the source).
                let mut sum = 0.0f32;
                let mut n = 0.0f32;
                let mut yy = 0.0;
                while yy < cellf {
                    let mut xx = 0.0;
                    while xx < cellf {
                        let lx = cellx * cellf + xx;
                        let ly = celly * cellf + yy;
                        let ix = lx * cc + ly * ss;
                        let iy = -lx * ss + ly * cc;
                        let sxx = (ix.floor() as i32).clamp(0, w as i32 - 1) as usize;
                        let syy = (iy.floor() as i32).clamp(0, h as i32 - 1) as usize;
                        let p = img[syy * w + sxx];
                        sum += p[ch] / p[3].max(1e-4);
                        n += 1.0;
                        xx += 1.0;
                    }
                    yy += 1.0;
                }
                let avg = sum / n.max(1.0);
                let maxr = cellf * std::f32::consts::FRAC_1_SQRT_2; // half diagonal
                let dotr = (1.0 - avg) * maxr;
                let d = ((rx - cellcx).powi(2) + (ry - cellcy).powi(2)).sqrt();
                *cv = if d <= dotr { 0.0 } else { 1.0 };
            }
            out[y * w + x] = [col[0] * mc[3], col[1] * mc[3], col[2] * mc[3], mc[3]];
        }
    }
    out
}

/// Mezzotint (kind 23): a seeded threshold dither to pure black/white grain.
/// Each pixel's Rec.709 luma is compared against a per-pixel hashed threshold
/// (biased by `amount`); above → white, below → black. Stable for a given seed.
/// Output is premultiplied, keeping the source alpha.
pub fn mezzotint(img: &[[f32; 4]], w: usize, h: usize, amount: f32, seed: f32) -> Vec<[f32; 4]> {
    let lw = [0.2126f32, 0.7152, 0.0722];
    let mut out = vec![[0.0f32; 4]; w * h];
    for y in 0..h {
        for x in 0..w {
            let mc = img[y * w + x];
            let a = mc[3].max(1e-4);
            let luma = (mc[0] / a) * lw[0] + (mc[1] / a) * lw[1] + (mc[2] / a) * lw[2];
            let t = hash21(x as f32, y as f32, seed);
            let v = if luma > t + (amount - 0.5) { 1.0 } else { 0.0 };
            out[y * w + x] = [v * mc[3], v * mc[3], v * mc[3], mc[3]];
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::filter_math::test_support::{approx, flat_gray};

    // ---- Pixelate family --------------------------------------------------

    #[test]
    fn mosaic_cell_is_the_block_average() {
        // A 4×4 image, cell 2 → four 2×2 blocks. Each block becomes its own mean,
        // and every pixel of a block shares that value (the block is uniform).
        let n = 4;
        // Distinct per-pixel values so block means are non-trivial.
        let mut img = vec![[0.0f32; 4]; n * n];
        for y in 0..n {
            for x in 0..n {
                let v = (y * n + x) as f32 / 16.0;
                img[y * n + x] = [v, v * 0.5, v * 0.25, 1.0];
            }
        }
        let out = mosaic(&img, n, n, 2);
        // For each 2×2 block, compute the expected mean and check uniformity.
        for by in (0..n).step_by(2) {
            for bx in (0..n).step_by(2) {
                let mut sum = [0.0f32; 4];
                for j in 0..2 {
                    for i in 0..2 {
                        let p = img[(by + j) * n + bx + i];
                        for c in 0..4 {
                            sum[c] += p[c];
                        }
                    }
                }
                let mean = [sum[0] / 4.0, sum[1] / 4.0, sum[2] / 4.0, sum[3] / 4.0];
                for j in 0..2 {
                    for i in 0..2 {
                        let o = out[(by + j) * n + bx + i];
                        for c in 0..4 {
                            assert!(
                                approx(o[c], mean[c], 1e-6),
                                "block ({bx},{by}) pixel uniform = mean (ch {c}): {} vs {}",
                                o[c],
                                mean[c]
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn mosaic_of_a_flat_field_is_identity() {
        let n = 8;
        let img = flat_gray(n, 0.37);
        let out = mosaic(&img, n, n, 4);
        for p in &out {
            assert!(approx(p[0], 0.37, 1e-6), "flat field stays flat");
        }
    }

    #[test]
    fn crystallize_is_deterministic_and_snaps_cells() {
        // A smooth gradient field; crystallize with a fixed seed must be
        // reproducible, and pixels that resolve to the same nearest seed must
        // share an identical colour (cell-snapping).
        let n = 16;
        let mut img = vec![[0.0f32; 4]; n * n];
        for y in 0..n {
            for x in 0..n {
                let v = x as f32 / (n - 1) as f32;
                img[y * n + x] = [v, 1.0 - v, 0.5, 1.0];
            }
        }
        let a = crystallize(&img, n, n, 4, 7.0);
        let b = crystallize(&img, n, n, 4, 7.0);
        assert_eq!(a, b, "same seed → identical (no temporal randomness)");
        // The output colour set is drawn only from source pixels (snapping, not
        // blending): every output equals some input pixel.
        for o in &a {
            assert!(
                img.iter().any(|p| p == o),
                "crystallize snaps to a source colour, never blends: {o:?}"
            );
        }
        // A different seed shifts the jitter → a different tessellation.
        let c = crystallize(&img, n, n, 4, 8.0);
        assert!(a != c, "different seed → different cells");
    }

    #[test]
    fn crystallize_of_a_flat_field_is_identity() {
        let n = 12;
        let img = flat_gray(n, 0.6);
        let out = crystallize(&img, n, n, 3, 2.0);
        for p in &out {
            assert!(approx(p[0], 0.6, 1e-6), "flat field unchanged by snapping");
        }
    }

    #[test]
    fn color_halftone_dot_grows_as_the_cell_darkens() {
        // Halftone output is binary per channel (0 ink / 1 paper). A darker field
        // (lower channel average) must produce MORE ink pixels (bigger dots) than
        // a brighter one. Use angle 0 so the screen is axis-aligned.
        let n = 32;
        let ink = |img: &[[f32; 4]]| -> usize {
            color_halftone(img, n, n, 8, 0.0)
                .iter()
                .filter(|p| p[0] < 0.5) // red-channel ink (premultiplied, a=1)
                .count()
        };
        let dark = ink(&flat_gray(n, 0.2));
        let mid = ink(&flat_gray(n, 0.5));
        let bright = ink(&flat_gray(n, 0.8));
        assert!(
            dark > mid && mid > bright,
            "darker cell → larger dot → more ink: dark {dark} mid {mid} bright {bright}"
        );
        // A near-black field is (almost) solid ink; a near-white field is sparse.
        let black = ink(&flat_gray(n, 0.02));
        let white = ink(&flat_gray(n, 0.98));
        assert!(black > white, "black {black} > white {white}");
    }

    #[test]
    fn color_halftone_is_binary_per_channel() {
        let n = 16;
        let img = flat_gray(n, 0.5);
        let out = color_halftone(&img, n, n, 4, 0.0);
        for p in &out {
            for c in 0..3 {
                assert!(
                    approx(p[c], 0.0, 1e-6) || approx(p[c], 1.0, 1e-6),
                    "each channel is full ink or paper: {}",
                    p[c]
                );
            }
            assert!(approx(p[3], 1.0, 1e-6), "alpha preserved");
        }
    }

    #[test]
    fn mezzotint_is_binary_deterministic_and_tracks_brightness() {
        // Output is pure black/white, reproducible per seed, and a brighter field
        // yields more white pixels than a darker one.
        let n = 32;
        let white_count = |v: f32| -> usize {
            mezzotint(&flat_gray(n, v), n, n, 0.5, 4.0)
                .iter()
                .filter(|p| p[0] > 0.5)
                .count()
        };
        let dark = white_count(0.25);
        let bright = white_count(0.75);
        assert!(bright > dark, "brighter → more white: {bright} > {dark}");
        // Binary + deterministic.
        let a = mezzotint(&flat_gray(n, 0.5), n, n, 0.5, 9.0);
        let b = mezzotint(&flat_gray(n, 0.5), n, n, 0.5, 9.0);
        assert_eq!(a, b, "same seed → identical");
        for p in &a {
            assert!(
                approx(p[0], 0.0, 1e-6) || approx(p[0], 1.0, 1e-6),
                "binary output: {}",
                p[0]
            );
        }
    }
}
