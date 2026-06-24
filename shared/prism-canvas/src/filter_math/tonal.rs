//! Tonal filters — CPU references mirroring `filter.wgsl`'s kinds 28–29. Unlike
//! the blur family these quantize in **display (sRGB)** space so the levels land
//! where the user sees them (quantizing in linear light bunches the steps in the
//! shadows). Both unpremultiply → encode to sRGB → operate → decode → re-premultiply,
//! and preserve alpha. Constants match composite.wgsl / prism-color.

/// Decode one sRGB channel (0..=1) to linear light. Bit-for-bit the WGSL `s2l1`.
fn s2l1(c: f32) -> f32 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

/// Encode one linear-light channel (0..=1) to sRGB. Bit-for-bit the WGSL `l2s1`.
fn l2s1(c: f32) -> f32 {
    if c <= 0.003_130_8 {
        c * 12.92
    } else {
        1.055 * c.powf(1.0 / 2.4) - 0.055
    }
}

/// Posterize (kind 28): quantize each colour channel to `levels` (≥2) evenly
/// spaced steps in display (sRGB) space — `floor(c·(L−1)+0.5)/(L−1)` — then
/// decode back to linear and re-premultiply. Alpha is preserved. With `levels`=2
/// every channel snaps to its 0 or 1 extreme. Mirrors the WGSL kind-28 pass.
pub fn posterize(img: &[[f32; 4]], w: usize, h: usize, levels: u32) -> Vec<[f32; 4]> {
    let l = levels.max(2) as f32;
    let mut out = vec![[0.0f32; 4]; w * h];
    for i in 0..w * h {
        let mc = img[i];
        let a = mc[3].max(1e-4);
        let mut col = [0.0f32; 3];
        for c in 0..3 {
            let s = l2s1(mc[c] / a);
            let q = (s * (l - 1.0) + 0.5).floor() / (l - 1.0);
            col[c] = s2l1(q.clamp(0.0, 1.0));
        }
        out[i] = [col[0] * mc[3], col[1] * mc[3], col[2] * mc[3], mc[3]];
    }
    out
}

/// Threshold (kind 29): convert to pure black/white at a display-space Rec.709
/// luma cutoff `level` (0..1) — at/above → white, below → black. Luma is taken on
/// the sRGB colour so the cutoff matches the histogram the user sees; alpha is
/// preserved. Mirrors the WGSL kind-29 pass.
pub fn threshold(img: &[[f32; 4]], w: usize, h: usize, level: f32) -> Vec<[f32; 4]> {
    let lw = [0.2126f32, 0.7152, 0.0722];
    let mut out = vec![[0.0f32; 4]; w * h];
    for i in 0..w * h {
        let mc = img[i];
        let a = mc[3].max(1e-4);
        let luma =
            l2s1(mc[0] / a) * lw[0] + l2s1(mc[1] / a) * lw[1] + l2s1(mc[2] / a) * lw[2];
        let v = if luma >= level { 1.0 } else { 0.0 };
        out[i] = [v * mc[3], v * mc[3], v * mc[3], mc[3]];
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::filter_math::test_support::{approx, vsplit};

    // ---- Posterize / Threshold -------------------------------------------

    #[test]
    fn posterize_2_levels_snaps_to_the_extremes() {
        // With 2 levels every channel must collapse to its 0 or 1 endpoint —
        // the nearest of {0, 1} in display space. A mid ramp splits at sRGB 0.5
        // (≈ linear 0.214): below → black, at/above → white. Alpha preserved.
        let vals = [0.0f32, 0.05, 0.21, 0.25, 0.6, 1.0];
        let img: Vec<[f32; 4]> = vals.iter().map(|&v| [v, v, v, 1.0]).collect();
        let out = posterize(&img, vals.len(), 1, 2);
        for (p, &v) in out.iter().zip(vals.iter()) {
            assert!(
                approx(p[0], 0.0, 1e-5) || approx(p[0], 1.0, 1e-5),
                "2-level posterize snaps {v} to a pure extreme, got {}",
                p[0]
            );
            assert!(approx(p[3], 1.0, 1e-6), "alpha preserved");
        }
        // The darkest stays black, the brightest stays white.
        assert!(approx(out[0][0], 0.0, 1e-5), "0 stays black");
        assert!(approx(out[5][0], 1.0, 1e-5), "1 stays white");
    }

    #[test]
    fn posterize_endpoints_and_levels_are_quantized() {
        // 0 and 1 are exact lattice points at any level → identity. A 4-level
        // posterize yields at most 4 distinct sRGB values per channel.
        let n = 16;
        let mut img = vec![[0.0f32; 4]; n];
        for (i, p) in img.iter_mut().enumerate() {
            let v = i as f32 / (n - 1) as f32;
            *p = [v, v, v, 1.0];
        }
        let out = posterize(&img, n, 1, 4);
        assert!(approx(out[0][0], 0.0, 1e-5), "black is a fixed point");
        assert!(approx(out[n - 1][0], 1.0, 1e-5), "white is a fixed point");
        let mut seen = std::collections::BTreeSet::new();
        for p in &out {
            seen.insert((l2s1(p[0]) * 1000.0).round() as i32);
        }
        assert!(seen.len() <= 4, "4 levels → ≤4 distinct steps, got {}", seen.len());
    }

    #[test]
    fn threshold_splits_a_ramp_at_the_cutoff() {
        // A horizontal sRGB-luma ramp split at cutoff 0.5: dark half → black,
        // bright half → white, output is pure binary, alpha preserved.
        let n = 21;
        let img: Vec<[f32; 4]> = (0..n)
            .map(|x| {
                let s = x as f32 / (n - 1) as f32; // sRGB value
                let lin = s2l1(s);
                [lin, lin, lin, 1.0]
            })
            .collect();
        let out = threshold(&img, n, 1, 0.5);
        for (x, p) in out.iter().enumerate() {
            let s = x as f32 / (n - 1) as f32;
            let want = if s >= 0.5 { 1.0 } else { 0.0 };
            assert!(approx(p[0], want, 1e-5), "ramp[{x}] (sRGB {s}) → {want}, got {}", p[0]);
            assert!(p[0] == 0.0 || p[0] == 1.0, "output is binary");
            assert!(approx(p[3], 1.0, 1e-6), "alpha preserved");
        }
    }

    #[test]
    fn threshold_extremes_pass_everything_or_nothing() {
        let img = vsplit(8);
        let all_white = threshold(&img, 8, 8, 0.0); // cutoff 0 → luma≥0 always
        assert!(all_white.iter().all(|p| approx(p[0], 1.0, 1e-5)), "cutoff 0 → all white");
        let all_black = threshold(&img, 8, 8, 1.001); // cutoff >1 → nothing passes
        assert!(all_black.iter().all(|p| approx(p[0], 0.0, 1e-5)), "cutoff >1 → all black");
    }
}
