//! Render generators — CPU references mirroring `filter.wgsl`'s kinds 25–26.
//! Unlike the other filters these don't read the source (Clouds) or read it only
//! to difference against (Difference Clouds); they paint a deterministic
//! multi-octave value-noise (fBm) field. Output is **opaque** premultiplied RGBA
//! (a generator fills the whole layer).

use super::hash21;

/// Smooth 2D value noise in [0,1) at `(px, py)`: bilinear interpolation between
/// the four `hash21` lattice corners, smoothed with the Hermite curve
/// (3t²−2t³). Bit-for-bit the WGSL `value_noise`.
fn value_noise(px: f32, py: f32, seed: f32) -> f32 {
    let ix = px.floor();
    let iy = py.floor();
    let fx = px - ix;
    let fy = py - iy;
    let a = hash21(ix, iy, seed);
    let b = hash21(ix + 1.0, iy, seed);
    let c = hash21(ix, iy + 1.0, seed);
    let d = hash21(ix + 1.0, iy + 1.0, seed);
    let ux = fx * fx * (3.0 - 2.0 * fx);
    let uy = fy * fy * (3.0 - 2.0 * fy);
    let top = a + (b - a) * ux;
    let bot = c + (d - c) * ux;
    top + (bot - top) * uy
}

/// Fractal (fBm) sum of `octaves` value-noise layers: each octave doubles the
/// frequency and scales the amplitude by `roughness`. The running amplitude sum
/// renormalises the result into [0,1]. `scale` is the base feature size in
/// pixels (larger → broader puffs). Bit-for-bit the WGSL `fbm`, so the CPU
/// reference reproduces the shader's cloud field for a given seed.
pub fn fbm(px: f32, py: f32, seed: f32, scale: f32, roughness: f32, octaves: usize) -> f32 {
    let mut freq = 1.0 / scale.max(1.0);
    let mut amp = 1.0f32;
    let mut sum = 0.0f32;
    let mut norm = 0.0f32;
    for o in 0..octaves {
        sum += amp * value_noise(px * freq, py * freq, seed + o as f32 * 37.0);
        norm += amp;
        freq *= 2.0;
        amp *= roughness;
    }
    sum / norm.max(1e-5)
}

/// Clouds (kind 25): fill the layer with a deterministic fBm value-noise field
/// (a soft, seamless cloud texture). `seed` makes it reproducible; `scale` is
/// the base feature size (px), `roughness` the per-octave amplitude falloff,
/// `octaves` the layer count. Ignores the input; output is opaque gray.
pub fn clouds(
    w: usize,
    h: usize,
    seed: f32,
    scale: f32,
    roughness: f32,
    octaves: usize,
) -> Vec<[f32; 4]> {
    let rough = roughness.clamp(0.05, 0.95);
    let oct = octaves.clamp(1, 10);
    let s = scale.max(1.0);
    let mut out = vec![[0.0f32; 4]; w * h];
    for y in 0..h {
        for x in 0..w {
            let n = fbm(x as f32, y as f32, seed, s, rough, oct);
            out[y * w + x] = [n, n, n, 1.0];
        }
    }
    out
}

/// Difference Clouds (kind 26): the same fBm field differenced against the
/// existing (unpremultiplied) layer colour via per-channel absolute difference
/// (Photoshop-style), so repeated application builds veins. Output is opaque.
pub fn difference_clouds(
    img: &[[f32; 4]],
    w: usize,
    h: usize,
    seed: f32,
    scale: f32,
    roughness: f32,
    octaves: usize,
) -> Vec<[f32; 4]> {
    let rough = roughness.clamp(0.05, 0.95);
    let oct = octaves.clamp(1, 10);
    let s = scale.max(1.0);
    let mut out = vec![[0.0f32; 4]; w * h];
    for y in 0..h {
        for x in 0..w {
            let n = fbm(x as f32, y as f32, seed, s, rough, oct);
            let mc = img[y * w + x];
            let a = mc[3].max(1e-4);
            let mut col = [0.0f32; 3];
            for c in 0..3 {
                col[c] = (mc[c] / a - n).abs();
            }
            out[y * w + x] = [col[0], col[1], col[2], 1.0];
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::filter_math::test_support::{approx, flat_gray};

    // ---- Render: Clouds / Difference Clouds -------------------------------

    #[test]
    fn clouds_are_deterministic_for_a_fixed_seed() {
        // Same seed (and params) → bit-identical fields, every time (no temporal
        // randomness): the whole point of a seeded generator.
        let n = 24;
        let a = clouds(n, n, 7.0, 16.0, 0.5, 5);
        let b = clouds(n, n, 7.0, 16.0, 0.5, 5);
        assert_eq!(a, b, "clouds reproduce exactly for a fixed seed");
    }

    #[test]
    fn clouds_differ_for_different_seeds() {
        // A different seed reshuffles the lattice hashes → a visibly different
        // field (not every pixel must move, but the field as a whole must).
        let n = 24;
        let a = clouds(n, n, 1.0, 16.0, 0.5, 5);
        let b = clouds(n, n, 2.0, 16.0, 0.5, 5);
        assert!(a != b, "different seeds → different clouds");
        let diff: f32 = a
            .iter()
            .zip(b.iter())
            .map(|(p, q)| (p[0] - q[0]).abs())
            .sum::<f32>()
            / (n * n) as f32;
        assert!(diff > 0.01, "fields differ substantially: mean |Δ| = {diff}");
    }

    #[test]
    fn clouds_output_is_in_range_opaque_and_gray() {
        // A generator fills the whole layer: every pixel opaque, in [0,1], and
        // monochrome (R==G==B). The field also actually varies (it isn't flat).
        let n = 24;
        let out = clouds(n, n, 3.0, 12.0, 0.6, 6);
        let mut min = f32::INFINITY;
        let mut max = f32::NEG_INFINITY;
        for p in &out {
            for c in 0..3 {
                assert!((0.0..=1.0).contains(&p[c]), "in range: {}", p[c]);
            }
            assert!(approx(p[0], p[1], 1e-6) && approx(p[1], p[2], 1e-6), "gray");
            assert!(approx(p[3], 1.0, 1e-6), "opaque");
            min = min.min(p[0]);
            max = max.max(p[0]);
        }
        assert!(max - min > 0.05, "field varies (not flat): {min}..{max}");
    }

    #[test]
    fn clouds_are_smooth_not_white_noise() {
        // fBm value noise is spatially smooth: adjacent pixels differ far less,
        // on average, than independent samples would. Compare the mean horizontal
        // neighbour delta against the global spread.
        let n = 32;
        let out = clouds(n, n, 9.0, 24.0, 0.5, 5);
        let mut neigh = 0.0f32;
        let mut cnt = 0.0f32;
        for y in 0..n {
            for x in 1..n {
                neigh += (out[y * n + x][0] - out[y * n + x - 1][0]).abs();
                cnt += 1.0;
            }
        }
        let neigh = neigh / cnt;
        let mean: f32 = out.iter().map(|p| p[0]).sum::<f32>() / out.len() as f32;
        let spread: f32 =
            (out.iter().map(|p| (p[0] - mean).powi(2)).sum::<f32>() / out.len() as f32).sqrt();
        assert!(
            neigh < spread,
            "neighbours are correlated (smooth): step {neigh} < spread {spread}"
        );
    }

    #[test]
    fn difference_clouds_is_abs_of_base_minus_noise() {
        // Difference Clouds == |base − cloud| per channel, exactly. Verify against
        // an independent `clouds` field and a known base colour.
        let n = 16;
        let base = 0.3f32;
        let img = flat_gray(n, base);
        let field = clouds(n, n, 4.0, 16.0, 0.5, 5);
        let out = difference_clouds(&img, n, n, 4.0, 16.0, 0.5, 5);
        for i in 0..(n * n) {
            let expect = (base - field[i][0]).abs();
            for c in 0..3 {
                assert!(
                    approx(out[i][c], expect, 1e-6),
                    "diff clouds = |base − noise|: {} vs {}",
                    out[i][c],
                    expect
                );
            }
            assert!(approx(out[i][3], 1.0, 1e-6), "opaque output");
        }
    }

    #[test]
    fn difference_clouds_folds_on_repeat() {
        // Photoshop's signature: re-running Difference Clouds on its own output
        // (same seed) keeps folding the field — it does NOT converge to identity,
        // each pass re-differences against the freshly-warped pixels, building the
        // characteristic veins. So a second pass differs from the first.
        let n = 32;
        let once = difference_clouds(&flat_gray(n, 0.5), n, n, 5.0, 24.0, 0.5, 5);
        let twice = difference_clouds(&once, n, n, 5.0, 24.0, 0.5, 5);
        assert!(once != twice, "a second fold keeps transforming the field");
        let changed = once
            .iter()
            .zip(twice.iter())
            .filter(|(a, b)| (a[0] - b[0]).abs() > 1e-3)
            .count();
        assert!(
            changed > n * n / 4,
            "the fold reworks much of the field: {changed} px changed"
        );
        // Folding is contractive on the *difference from a vein*, so a single
        // pass against a flat field already yields the noise itself: applying
        // Difference Clouds to a black field reproduces the raw cloud field.
        let from_black = difference_clouds(&flat_gray(n, 0.0), n, n, 5.0, 24.0, 0.5, 5);
        let field = clouds(n, n, 5.0, 24.0, 0.5, 5);
        for i in 0..(n * n) {
            assert!(
                approx(from_black[i][0], field[i][0], 1e-6),
                "|0 − noise| = noise"
            );
        }
    }
}
