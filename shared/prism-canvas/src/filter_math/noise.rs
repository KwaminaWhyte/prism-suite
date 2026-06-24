//! Noise filters — CPU references mirroring `filter.wgsl`'s kinds 17–19.
//! All operate in the compositor working space: **linear premultiplied** RGBA.

use super::{hash21, TAU};

/// One ~unit-variance gaussian sample via Box–Muller from two hashes, matching
/// the shader's `gauss1`.
fn gauss1(ix: f32, iy: f32, seed: f32) -> f32 {
    let u1 = hash21(ix, iy, seed).max(1e-6);
    let u2 = hash21(ix, iy, seed + 101.0);
    (-2.0 * u1.ln()).sqrt() * (TAU * u2).cos()
}

/// One zero-mean uniform noise sample in (-1, 1): the difference of two i.i.d.
/// hashes (the raw `fract(sin)` hash is biased on a regular grid), matching the
/// shader's `uniform1`.
fn uniform1(ix: f32, iy: f32, seed: f32) -> f32 {
    hash21(ix, iy, seed) - hash21(ix, iy, seed + 200.0)
}

/// Add Noise (kind 17): seeded-deterministic per-pixel noise added to the
/// (unpremultiplied) colour, then re-premultiplied. `mono` applies the same
/// noise to R/G/B; `gaussian` selects gaussian (true) vs uniform (false). The
/// noise is zero-mean so the average is preserved, and stable for a given seed.
pub fn add_noise(
    img: &[[f32; 4]],
    w: usize,
    h: usize,
    amount: f32,
    mono: bool,
    gaussian: bool,
    seed: f32,
) -> Vec<[f32; 4]> {
    let mut out = vec![[0.0f32; 4]; w * h];
    for y in 0..h {
        for x in 0..w {
            let (ix, iy) = (x as f32, y as f32);
            let mc = img[y * w + x];
            let a = mc[3].max(1e-4);
            let mut nrgb = if gaussian {
                if mono {
                    [gauss1(ix, iy, seed) * 0.4; 3]
                } else {
                    [
                        gauss1(ix, iy, seed) * 0.4,
                        gauss1(ix, iy, seed + 17.0) * 0.4,
                        gauss1(ix, iy, seed + 53.0) * 0.4,
                    ]
                }
            } else if mono {
                [uniform1(ix, iy, seed); 3]
            } else {
                [
                    uniform1(ix, iy, seed),
                    uniform1(ix, iy, seed + 17.0),
                    uniform1(ix, iy, seed + 53.0),
                ]
            };
            let mut col = [0.0f32; 3];
            for c in 0..3 {
                nrgb[c] *= amount;
                col[c] = (mc[c] / a + nrgb[c]).clamp(0.0, 1.0);
            }
            out[y * w + x] = [col[0] * mc[3], col[1] * mc[3], col[2] * mc[3], mc[3]];
        }
    }
    out
}

/// Per-channel median over a `(2·radius+1)²` window of unpremultiplied colour
/// at `(x, y)`, edge-clamped. Returns the `[r, g, b]` medians.
fn window_median(
    img: &[[f32; 4]],
    w: usize,
    h: usize,
    x: usize,
    y: usize,
    radius: i32,
) -> [f32; 3] {
    let mut med = [0.0f32; 3];
    for (ch, m) in med.iter_mut().enumerate() {
        let mut vals: Vec<f32> = Vec::new();
        for dy in -radius..=radius {
            for dx in -radius..=radius {
                let sx = (x as i32 + dx).clamp(0, w as i32 - 1) as usize;
                let sy = (y as i32 + dy).clamp(0, h as i32 - 1) as usize;
                let p = img[sy * w + sx];
                vals.push(p[ch] / p[3].max(1e-4));
            }
        }
        vals.sort_by(|a, b| a.partial_cmp(b).unwrap());
        *m = vals[vals.len() / 2];
    }
    med
}

/// Median filter (kind 18): replace each pixel with the per-channel median of a
/// `(2·radius+1)²` window — despeckle / salt-pepper removal. Output is
/// premultiplied RGBA keeping the source alpha.
pub fn median(img: &[[f32; 4]], w: usize, h: usize, radius: usize) -> Vec<[f32; 4]> {
    let r = radius as i32;
    let mut out = vec![[0.0f32; 4]; w * h];
    for y in 0..h {
        for x in 0..w {
            let m = window_median(img, w, h, x, y, r);
            let a = img[y * w + x][3];
            out[y * w + x] = [m[0] * a, m[1] * a, m[2] * a, a];
        }
    }
    out
}

/// Dust & Scratches (kind 19): a thresholded median — a channel is replaced by
/// the window median only when the original differs from it by more than
/// `threshold`, preserving detail while removing specks. Output is premultiplied
/// RGBA keeping the source alpha.
pub fn dust_and_scratches(
    img: &[[f32; 4]],
    w: usize,
    h: usize,
    radius: usize,
    threshold: f32,
) -> Vec<[f32; 4]> {
    let r = radius as i32;
    let mut out = vec![[0.0f32; 4]; w * h];
    for y in 0..h {
        for x in 0..w {
            let mc = img[y * w + x];
            let a = mc[3].max(1e-4);
            let m = window_median(img, w, h, x, y, r);
            let mut col = [0.0f32; 3];
            for c in 0..3 {
                let orig = mc[c] / a;
                col[c] = if (orig - m[c]).abs() > threshold {
                    m[c]
                } else {
                    orig
                };
            }
            out[y * w + x] = [col[0] * mc[3], col[1] * mc[3], col[2] * mc[3], mc[3]];
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::filter_math::test_support::{approx, channel_mean, flat_gray};

    // ---- Noise ------------------------------------------------------------

    #[test]
    fn add_noise_amount_zero_is_identity() {
        let n = 16;
        let img = flat_gray(n, 0.5);
        let out = add_noise(&img, n, n, 0.0, false, true, 1.0);
        for (a, b) in out.iter().zip(img.iter()) {
            for c in 0..4 {
                assert!(approx(a[c], b[c], 1e-6), "amount 0 is identity");
            }
        }
    }

    #[test]
    fn add_noise_is_deterministic() {
        let n = 24;
        let img = flat_gray(n, 0.5);
        let a = add_noise(&img, n, n, 0.3, false, true, 7.0);
        let b = add_noise(&img, n, n, 0.3, false, true, 7.0);
        assert_eq!(a, b, "same seed → identical noise (no temporal randomness)");
        // A different seed produces a different result.
        let c = add_noise(&img, n, n, 0.3, false, true, 8.0);
        assert!(a != c, "different seed → different noise");
    }

    #[test]
    fn add_noise_monochromatic_has_equal_rgb_delta() {
        let n = 24;
        let img = flat_gray(n, 0.5);
        let out = add_noise(&img, n, n, 0.25, true, true, 3.0);
        // Monochromatic: the per-pixel R/G/B deltas from the base are identical
        // (un-clamped here because base 0.5 + small noise stays interior).
        for p in &out {
            assert!(approx(p[0], p[1], 1e-6), "mono R==G");
            assert!(approx(p[1], p[2], 1e-6), "mono G==B");
        }
        // Colour noise breaks that equality somewhere.
        let colour = add_noise(&img, n, n, 0.25, false, true, 3.0);
        let any_diff = colour
            .iter()
            .any(|p| (p[0] - p[1]).abs() > 1e-4 || (p[1] - p[2]).abs() > 1e-4);
        assert!(any_diff, "colour noise differs per channel");
    }

    #[test]
    fn add_noise_preserves_mean_and_perturbs() {
        let n = 64;
        let base = 0.5;
        let img = flat_gray(n, base);
        // Gaussian, colour.
        let g = add_noise(&img, n, n, 0.15, false, true, 11.0);
        for ch in 0..3 {
            assert!(
                (channel_mean(&g, ch) - base).abs() < 0.02,
                "zero-mean gaussian noise preserves the channel mean (ch {ch})"
            );
        }
        // Uniform, monochromatic.
        let u = add_noise(&img, n, n, 0.15, true, false, 11.0);
        for ch in 0..3 {
            assert!(
                (channel_mean(&u, ch) - base).abs() < 0.02,
                "zero-mean uniform noise preserves the channel mean (ch {ch})"
            );
        }
        // And the field is actually perturbed (not all equal to base).
        assert!(
            g.iter().any(|p| (p[0] - base).abs() > 1e-3),
            "noise actually perturbs pixels"
        );
    }

    #[test]
    fn median_removes_an_impulse_outlier() {
        // A flat gray field with a single bright (salt) outlier; a 3×3 median
        // (radius 1) replaces it with the surrounding gray.
        let n = 9;
        let mut img = flat_gray(n, 0.4);
        let cidx = (n / 2) * n + n / 2;
        img[cidx] = [1.0, 1.0, 1.0, 1.0]; // impulse
        let out = median(&img, n, n, 1);
        for c in 0..3 {
            assert!(
                approx(out[cidx][c], 0.4, 1e-5),
                "median removes the impulse (ch {c}): {}",
                out[cidx][c]
            );
        }
        // A flat neighbour is unchanged.
        assert!(approx(out[0][0], 0.4, 1e-5), "flat area preserved");
    }

    #[test]
    fn median_radius_grows_the_window() {
        // Two adjacent outliers survive a 3×3 median at one of them (a tie can
        // remain) but a 5×5 (radius 2) median over a flat field removes them.
        let n = 11;
        let mut img = flat_gray(n, 0.3);
        let c = n / 2;
        img[c * n + c] = [0.9, 0.9, 0.9, 1.0];
        img[c * n + c + 1] = [0.9, 0.9, 0.9, 1.0];
        let out = median(&img, n, n, 2);
        assert!(
            approx(out[c * n + c][0], 0.3, 1e-5),
            "radius-2 median clears clustered outliers: {}",
            out[c * n + c][0]
        );
    }

    #[test]
    fn dust_and_scratches_only_changes_above_threshold_pixels() {
        // A flat field with one strong speck and a tiny ripple. With a threshold
        // between the two deviations, only the strong speck is replaced; the
        // sub-threshold detail is preserved (unlike a plain median).
        let n = 9;
        let base = 0.5;
        let mut img = flat_gray(n, base);
        let speck = (n / 2) * n + n / 2;
        let small = 2 * n + 2;
        img[speck] = [0.95, 0.95, 0.95, 1.0]; // big deviation (0.45)
        img[small] = [base + 0.05, base + 0.05, base + 0.05, 1.0]; // tiny (0.05)
        let out = dust_and_scratches(&img, n, n, 1, 0.2);
        // The strong speck is pulled to the median (≈ base).
        assert!(
            (out[speck][0] - base).abs() < 0.02,
            "above-threshold speck replaced: {}",
            out[speck][0]
        );
        // The sub-threshold pixel is left as-is (its own value, not the median).
        assert!(
            approx(out[small][0], base + 0.05, 1e-5),
            "below-threshold pixel preserved: {}",
            out[small][0]
        );
    }

    #[test]
    fn dust_high_threshold_is_identity() {
        // With a threshold above any deviation present, nothing changes.
        let n = 9;
        let mut img = flat_gray(n, 0.5);
        img[(n / 2) * n + n / 2] = [0.9, 0.9, 0.9, 1.0];
        let out = dust_and_scratches(&img, n, n, 1, 1.0);
        for (a, b) in out.iter().zip(img.iter()) {
            for c in 0..4 {
                assert!(approx(a[c], b[c], 1e-5), "high threshold → identity");
            }
        }
    }
}
