//! Phase 8 — Blur / Sharpen / Distort filter gallery (real, pure pixel math).
//!
//! Every function here is a *pure* CPU algorithm over a linear-premultiplied
//! RGBA `f32` buffer (`w*h*4`, the format `CanvasHost::read_layer_f32` returns),
//! so they slot straight into the `read_layer_f32 → pure fn → snapshot →
//! upload_layer_f32` app-action idiom (see `app_state/filters_blur.rs`) exactly
//! like `healing.rs` / `liquify.rs` / `filters_advanced.rs` do — no engine,
//! shader, or shared-crate change.
//!
//! These are deliberately distinct from the GPU-engine `Filter` variants
//! (`app_state/filters.rs`) and from the existing CPU filters in `filters.rs` /
//! `filters_extra.rs`: this module ADDS a self-contained, golden-testable blur /
//! sharpen / distort gallery rather than touching them.
//!
//! **Conventions**
//! * Blurs average **premultiplied** channels (linear interpolation of premult
//!   storage is the correct averaging operation), so alpha edges stay clean.
//! * Sharpen ops work per-channel on the premultiplied buffer and leave alpha
//!   untouched (matches `filters.rs::high_pass` / `smart_sharpen`).
//! * Every spatial access is **edge-clamped** — no read is ever out of bounds.
//! * Kernels are **normalized** (sum ≈ 1) so a constant image is unchanged and an
//!   impulse's energy is preserved.

// ---------------------------------------------------------------------------
// Shared pure helpers
// ---------------------------------------------------------------------------

/// Bilinear sample of a premultiplied RGBA `f32` buffer at continuous
/// pixel-index coordinates `(fx, fy)`, edge-clamped (never out of bounds).
#[inline]
fn sample_bilinear(buf: &[f32], w: usize, h: usize, fx: f32, fy: f32) -> [f32; 4] {
    if w == 0 || h == 0 {
        return [0.0; 4];
    }
    let cx = fx.clamp(0.0, w as f32 - 1.0);
    let cy = fy.clamp(0.0, h as f32 - 1.0);
    let x0 = cx.floor() as usize;
    let y0 = cy.floor() as usize;
    let x1 = (x0 + 1).min(w - 1);
    let y1 = (y0 + 1).min(h - 1);
    let tx = cx - x0 as f32;
    let ty = cy - y0 as f32;
    let i00 = (y0 * w + x0) * 4;
    let i10 = (y0 * w + x1) * 4;
    let i01 = (y1 * w + x0) * 4;
    let i11 = (y1 * w + x1) * 4;
    let mut out = [0.0f32; 4];
    for c in 0..4 {
        let top = buf[i00 + c] * (1.0 - tx) + buf[i10 + c] * tx;
        let bot = buf[i01 + c] * (1.0 - tx) + buf[i11 + c] * tx;
        out[c] = top * (1.0 - ty) + bot * ty;
    }
    out
}

/// Unpremultiply one texel into straight RGBA (guards against zero alpha).
#[inline]
fn unpremul(c: [f32; 4]) -> [f32; 4] {
    if c[3] > 1e-6 {
        [c[0] / c[3], c[1] / c[3], c[2] / c[3], c[3]]
    } else {
        [0.0, 0.0, 0.0, 0.0]
    }
}

/// Rec. 709 luma of a straight-RGB triple.
#[inline]
fn luma(r: f32, g: f32, b: f32) -> f32 {
    0.2126 * r + 0.7152 * g + 0.0722 * b
}

/// Build a normalized 1-D Gaussian kernel for `sigma` (kernel sums to 1).
/// The radius is `ceil(3·sigma)` — captures >99.7% of the mass.
pub fn gaussian_kernel_1d(sigma: f32) -> Vec<f32> {
    if sigma <= 0.0 {
        return vec![1.0];
    }
    let radius = (sigma * 3.0).ceil().max(1.0) as i32;
    let two_sigma2 = 2.0 * sigma * sigma;
    let mut k = Vec::with_capacity((2 * radius + 1) as usize);
    let mut sum = 0.0f32;
    for i in -radius..=radius {
        let v = (-((i * i) as f32) / two_sigma2).exp();
        k.push(v);
        sum += v;
    }
    let inv = 1.0 / sum;
    for v in &mut k {
        *v *= inv;
    }
    k
}

/// Convolve `src` with a 1-D `kernel` along one axis. `horizontal` selects the
/// axis. Edge-clamped; `kernel` is centred (length must be odd). Premultiplied
/// channels are averaged directly.
fn convolve_1d(src: &[f32], w: usize, h: usize, kernel: &[f32], horizontal: bool) -> Vec<f32> {
    let radius = (kernel.len() / 2) as i32;
    let mut out = vec![0.0f32; w * h * 4];
    for y in 0..h {
        for x in 0..w {
            let mut acc = [0.0f32; 4];
            for (ki, &kw) in kernel.iter().enumerate() {
                let off = ki as i32 - radius;
                let (sx, sy) = if horizontal {
                    ((x as i32 + off).clamp(0, w as i32 - 1) as usize, y)
                } else {
                    (x, (y as i32 + off).clamp(0, h as i32 - 1) as usize)
                };
                let si = (sy * w + sx) * 4;
                for c in 0..4 {
                    acc[c] += src[si + c] * kw;
                }
            }
            let di = (y * w + x) * 4;
            out[di..di + 4].copy_from_slice(&acc);
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Gaussian blur — separable (horizontal 1-D pass, then vertical 1-D pass)
// ---------------------------------------------------------------------------

/// Separable Gaussian blur. Builds a normalized 1-D kernel from `sigma`, then
/// convolves horizontally and vertically (O(n·r) per axis, edge-clamped). A
/// `sigma <= 0` is the identity. Because the 2-D Gaussian is separable, H∘V is
/// bit-equal (up to float rounding) to a full 2-D convolution with the same
/// per-axis clamping.
pub fn gaussian_blur(px: &[f32], w: u32, h: u32, sigma: f32) -> Vec<f32> {
    let (wi, hi) = (w as usize, h as usize);
    if sigma <= 0.0 || wi == 0 || hi == 0 {
        return px.to_vec();
    }
    let kernel = gaussian_kernel_1d(sigma);
    let horiz = convolve_1d(px, wi, hi, &kernel, true);
    convolve_1d(&horiz, wi, hi, &kernel, false)
}

// ---------------------------------------------------------------------------
// Box blur — O(n) running-sum, separable; multi-pass ≈ Gaussian fast path
// ---------------------------------------------------------------------------

/// One running-sum 1-D box pass of `radius` along an axis (edge-clamped). The
/// window always holds exactly `2·radius+1` terms (clamped reads repeat the edge
/// texel), so each step adds the new rightmost and drops the old leftmost —
/// genuine O(n) regardless of radius.
fn box_pass_1d(src: &[f32], w: usize, h: usize, radius: i32, horizontal: bool) -> Vec<f32> {
    let mut out = vec![0.0f32; w * h * 4];
    let norm = 1.0 / (2 * radius + 1) as f32;
    let (lanes, line) = if horizontal { (h, w) } else { (w, h) };
    for lane in 0..lanes {
        let idx = |i: usize| -> usize {
            if horizontal {
                (lane * w + i) * 4
            } else {
                (i * w + lane) * 4
            }
        };
        let clamp = |i: i32| i.clamp(0, line as i32 - 1) as usize;
        // Seed the window centred on position 0.
        let mut acc = [0.0f32; 4];
        for k in -radius..=radius {
            let si = idx(clamp(k));
            for c in 0..4 {
                acc[c] += src[si + c];
            }
        }
        for c in 0..4 {
            out[idx(0) + c] = acc[c] * norm;
        }
        for i in 1..line {
            let add = idx(clamp(i as i32 + radius));
            let sub = idx(clamp(i as i32 - radius - 1));
            for c in 0..4 {
                acc[c] += src[add + c] - src[sub + c];
                out[idx(i) + c] = acc[c] * norm;
            }
        }
    }
    out
}

/// Separable box blur of integer `radius` (running-sum, O(n) per axis). A
/// `radius == 0` is the identity. Averaging a flat region returns that region.
pub fn box_blur(px: &[f32], w: u32, h: u32, radius: u32) -> Vec<f32> {
    let (wi, hi) = (w as usize, h as usize);
    let r = radius as i32;
    if r == 0 || wi == 0 || hi == 0 {
        return px.to_vec();
    }
    let horiz = box_pass_1d(px, wi, hi, r, true);
    box_pass_1d(&horiz, wi, hi, r, false)
}

/// Box radii for `n` successive box passes that approximate a true Gaussian of
/// the given `sigma` (Kovesi / Wells "boxes for Gauss"). Returns one radius per
/// pass.
pub fn boxes_for_gaussian(sigma: f32, n: usize) -> Vec<u32> {
    if n == 0 || sigma <= 0.0 {
        return vec![0; n];
    }
    let nf = n as f32;
    let w_ideal = ((12.0 * sigma * sigma / nf) + 1.0).sqrt();
    let mut wl = w_ideal.floor() as i32;
    if wl % 2 == 0 {
        wl -= 1;
    }
    let wu = wl + 2;
    let m_ideal = (12.0 * sigma * sigma
        - nf * (wl * wl) as f32
        - 4.0 * nf * wl as f32
        - 3.0 * nf)
        / (-4.0 * wl as f32 - 4.0);
    let m = m_ideal.round() as i64;
    (0..n)
        .map(|i| {
            let width = if (i as i64) < m { wl } else { wu };
            (((width - 1) / 2).max(0)) as u32
        })
        .collect()
}

/// Multi-pass box-blur approximation of a Gaussian (`passes` ≥ 1). Three passes
/// already match a Gaussian closely while staying O(n) per axis. A `sigma <= 0`
/// is the identity.
pub fn box_blur_gaussian(px: &[f32], w: u32, h: u32, sigma: f32, passes: usize) -> Vec<f32> {
    if sigma <= 0.0 || passes == 0 {
        return px.to_vec();
    }
    let radii = boxes_for_gaussian(sigma, passes);
    let mut buf = px.to_vec();
    for r in radii {
        if r > 0 {
            buf = box_blur(&buf, w, h, r);
        }
    }
    buf
}

// ---------------------------------------------------------------------------
// Motion blur — directional linear smear (angle + length)
// ---------------------------------------------------------------------------

/// Directional motion blur: average taps along a line of `length` px at
/// `angle_deg`, centred on each pixel — the classic linear smear. `length < 0.5`
/// is the identity. Samples are taken with bilinear interpolation on the
/// premultiplied buffer (edge-clamped). The smear runs strictly along the line,
/// so structure perpendicular to the angle is preserved.
pub fn motion_blur(px: &[f32], w: u32, h: u32, angle_deg: f32, length: f32) -> Vec<f32> {
    let (wi, hi) = (w as usize, h as usize);
    let len = length.max(0.0);
    if len < 0.5 || wi == 0 || hi == 0 {
        return px.to_vec();
    }
    let rad = angle_deg.to_radians();
    let (dx, dy) = (rad.cos(), rad.sin());
    let samples = (len.round() as i32).clamp(1, 512);
    let inv = 1.0 / (samples as f32 + 1.0);
    let mut out = vec![0.0f32; wi * hi * 4];
    for y in 0..hi {
        for x in 0..wi {
            let mut acc = [0.0f32; 4];
            for s in 0..=samples {
                let t = (s as f32 / samples as f32 - 0.5) * len;
                let sx = x as f32 + dx * t;
                let sy = y as f32 + dy * t;
                let c = sample_bilinear(px, wi, hi, sx, sy);
                for k in 0..4 {
                    acc[k] += c[k];
                }
            }
            let i = (y * wi + x) * 4;
            for k in 0..4 {
                out[i + k] = acc[k] * inv;
            }
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Radial blur — Zoom (toward/away from a centre) and Spin (around a centre)
// ---------------------------------------------------------------------------

/// Zoom blur: smear each pixel along the ray to/from `center`, scaling its
/// distance by up to `amount` over `samples` taps — the radial "rush toward the
/// centre" look. `amount <= 0` is the identity. Edge-clamped bilinear taps.
pub fn zoom_blur(px: &[f32], w: u32, h: u32, center: [f32; 2], amount: f32, samples: u32) -> Vec<f32> {
    let (wi, hi) = (w as usize, h as usize);
    let amt = amount.max(0.0);
    let n = samples.max(1);
    if amt <= 1e-4 || wi == 0 || hi == 0 {
        return px.to_vec();
    }
    let (cx, cy) = (center[0], center[1]);
    let inv = 1.0 / (n as f32 + 1.0);
    let mut out = vec![0.0f32; wi * hi * 4];
    for y in 0..hi {
        for x in 0..wi {
            let fx = x as f32;
            let fy = y as f32;
            let mut acc = [0.0f32; 4];
            for s in 0..=n {
                // scale 1.0 (no zoom) → (1 - amt) (zoomed in) across the taps.
                let t = s as f32 / n as f32;
                let scale = 1.0 - amt * t;
                let sx = cx + (fx - cx) * scale;
                let sy = cy + (fy - cy) * scale;
                let c = sample_bilinear(px, wi, hi, sx, sy);
                for k in 0..4 {
                    acc[k] += c[k];
                }
            }
            let i = (y * wi + x) * 4;
            for k in 0..4 {
                out[i + k] = acc[k] * inv;
            }
        }
    }
    out
}

/// Spin blur: smear each pixel along a circular arc around `center`, sweeping up
/// to `angle_deg` over `samples` taps — the rotational motion-blur look.
/// `angle_deg ≈ 0` is the identity. Edge-clamped bilinear taps.
pub fn spin_blur(px: &[f32], w: u32, h: u32, center: [f32; 2], angle_deg: f32, samples: u32) -> Vec<f32> {
    let (wi, hi) = (w as usize, h as usize);
    let max_a = angle_deg.to_radians();
    let n = samples.max(1);
    if max_a.abs() < 1e-4 || wi == 0 || hi == 0 {
        return px.to_vec();
    }
    let (cx, cy) = (center[0], center[1]);
    let inv = 1.0 / (n as f32 + 1.0);
    let mut out = vec![0.0f32; wi * hi * 4];
    for y in 0..hi {
        for x in 0..wi {
            let rx = x as f32 - cx;
            let ry = y as f32 - cy;
            let mut acc = [0.0f32; 4];
            for s in 0..=n {
                // -max_a/2 .. +max_a/2 around the pixel's current angle.
                let a = (s as f32 / n as f32 - 0.5) * max_a;
                let (sa, ca) = a.sin_cos();
                let sx = cx + rx * ca - ry * sa;
                let sy = cy + rx * sa + ry * ca;
                let c = sample_bilinear(px, wi, hi, sx, sy);
                for k in 0..4 {
                    acc[k] += c[k];
                }
            }
            let i = (y * wi + x) * 4;
            for k in 0..4 {
                out[i + k] = acc[k] * inv;
            }
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Sharpen — Unsharp Mask and (edge-aware) Smart Sharpen
// ---------------------------------------------------------------------------

/// Unsharp Mask: `out = orig + amount · (orig − blur)` per RGB channel, where
/// `blur` is a Gaussian of `sigma`. The boost is applied only where the local
/// detail magnitude (luma of `orig − blur`) exceeds `threshold`, so flat,
/// low-contrast areas (and noise below the threshold) are left alone. Alpha is
/// preserved; results are clamped to `[0, 1]`. `amount <= 0` is the identity.
pub fn unsharp_mask(px: &[f32], w: u32, h: u32, sigma: f32, amount: f32, threshold: f32) -> Vec<f32> {
    let (wi, hi) = (w as usize, h as usize);
    if amount <= 0.0 || wi == 0 || hi == 0 {
        return px.to_vec();
    }
    let blur = gaussian_blur(px, w, h, sigma.max(1e-3));
    let thr = threshold.max(0.0);
    let mut out = px.to_vec();
    for p in 0..(wi * hi) {
        let i = p * 4;
        let dr = px[i] - blur[i];
        let dg = px[i + 1] - blur[i + 1];
        let db = px[i + 2] - blur[i + 2];
        // Gate on the detail luma so sub-threshold detail is untouched.
        if luma(dr.abs(), dg.abs(), db.abs()) < thr {
            continue;
        }
        out[i] = (px[i] + dr * amount).clamp(0.0, 1.0);
        out[i + 1] = (px[i + 1] + dg * amount).clamp(0.0, 1.0);
        out[i + 2] = (px[i + 2] + db * amount).clamp(0.0, 1.0);
        // alpha already copied from px
    }
    out
}

/// Local luma variance in a `radius`-window around each pixel (straight luma),
/// returned as one `f32` per pixel. Edge-clamped. Used to gate Smart Sharpen.
fn local_variance(px: &[f32], w: usize, h: usize, radius: i32) -> Vec<f32> {
    let mut var = vec![0.0f32; w * h];
    for y in 0..h {
        for x in 0..w {
            let mut sum = 0.0f32;
            let mut sum2 = 0.0f32;
            let mut n = 0.0f32;
            for dy in -radius..=radius {
                let sy = (y as i32 + dy).clamp(0, h as i32 - 1) as usize;
                for dx in -radius..=radius {
                    let sx = (x as i32 + dx).clamp(0, w as i32 - 1) as usize;
                    let si = (sy * w + sx) * 4;
                    let s = unpremul([px[si], px[si + 1], px[si + 2], px[si + 3]]);
                    let l = luma(s[0], s[1], s[2]);
                    sum += l;
                    sum2 += l * l;
                    n += 1.0;
                }
            }
            let mean = sum / n;
            var[y * w + x] = (sum2 / n - mean * mean).max(0.0);
        }
    }
    var
}

/// Smart Sharpen: edge-aware unsharp mask. The unsharp boost is scaled by a gate
/// derived from local luma variance, so genuine edges (high variance) sharpen
/// while flat regions (variance ≤ `threshold²`) are left ~untouched — avoiding
/// the noise amplification a plain unsharp mask causes. `amount <= 0` is the
/// identity. Alpha preserved; output clamped to `[0, 1]`.
pub fn smart_sharpen(px: &[f32], w: u32, h: u32, sigma: f32, amount: f32, threshold: f32) -> Vec<f32> {
    let (wi, hi) = (w as usize, h as usize);
    if amount <= 0.0 || wi == 0 || hi == 0 {
        return px.to_vec();
    }
    let blur = gaussian_blur(px, w, h, sigma.max(1e-3));
    let var = local_variance(px, wi, hi, 1);
    // Edge gate: 0 below threshold², ramping to 1 well above it.
    let t2 = (threshold.max(0.0)).powi(2).max(1e-8);
    let mut out = px.to_vec();
    for p in 0..(wi * hi) {
        let i = p * 4;
        let g = (var[p] / t2).clamp(0.0, 1.0); // smooth-ish linear gate
        if g <= 0.0 {
            continue;
        }
        let dr = (px[i] - blur[i]) * amount * g;
        let dg = (px[i + 1] - blur[i + 1]) * amount * g;
        let db = (px[i + 2] - blur[i + 2]) * amount * g;
        out[i] = (px[i] + dr).clamp(0.0, 1.0);
        out[i + 1] = (px[i + 1] + dg).clamp(0.0, 1.0);
        out[i + 2] = (px[i + 2] + db).clamp(0.0, 1.0);
    }
    out
}

// ---------------------------------------------------------------------------
// Displacement map
// ---------------------------------------------------------------------------

/// Displacement map: read the displacement source's straight **R** and **G** as
/// per-pixel `x`/`y` pixel offsets (scaled by `scale_x` / `scale_y`), then
/// inverse-bilinear-sample the image at `(x − off_x, y − off_y)` (edge-clamped).
///
/// The displacement source `disp` (`dw×dh`) may differ in size from the image;
/// it is sampled bilinearly at the matching normalized position. An all-zero (or
/// fully transparent) displacement source is the exact identity — offsets are
/// zero everywhere, so each pixel reads itself. Premultiplied throughout.
#[allow(clippy::too_many_arguments)]
pub fn displacement_map(
    px: &[f32],
    w: u32,
    h: u32,
    disp: &[f32],
    dw: u32,
    dh: u32,
    scale_x: f32,
    scale_y: f32,
) -> Vec<f32> {
    let (wi, hi) = (w as usize, h as usize);
    let (dwi, dhi) = (dw as usize, dh as usize);
    let mut out = vec![0.0f32; wi * hi * 4];
    if wi == 0 || hi == 0 {
        return out;
    }
    let sxr = if wi > 0 { dwi as f32 / wi as f32 } else { 1.0 };
    let syr = if hi > 0 { dhi as f32 / hi as f32 } else { 1.0 };
    for y in 0..hi {
        for x in 0..wi {
            let (off_x, off_y) = if dwi == 0 || dhi == 0 {
                (0.0, 0.0)
            } else {
                let dxc = (x as f32 + 0.5) * sxr - 0.5;
                let dyc = (y as f32 + 0.5) * syr - 0.5;
                let d = unpremul(sample_bilinear(disp, dwi, dhi, dxc, dyc));
                (d[0] * scale_x, d[1] * scale_y)
            };
            let c = sample_bilinear(px, wi, hi, x as f32 - off_x, y as f32 - off_y);
            let i = (y * wi + x) * 4;
            out[i..i + 4].copy_from_slice(&c);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flat(w: u32, h: u32, c: [f32; 4]) -> Vec<f32> {
        let mut v = Vec::with_capacity((w * h * 4) as usize);
        for _ in 0..(w * h) {
            v.extend_from_slice(&c);
        }
        v
    }

    fn impulse(w: u32, h: u32, cx: u32, cy: u32) -> Vec<f32> {
        let mut v = vec![0.0f32; (w * h * 4) as usize];
        let i = ((cy * w + cx) * 4) as usize;
        v[i] = 1.0; // R impulse
        v[i + 3] = 1.0; // opaque
        v
    }

    fn sum_channel(buf: &[f32], c: usize) -> f32 {
        buf.iter().skip(c).step_by(4).sum()
    }

    // ---- Gaussian kernel ----

    #[test]
    fn gaussian_kernel_normalized() {
        let k = gaussian_kernel_1d(2.0);
        let s: f32 = k.iter().sum();
        assert!((s - 1.0).abs() < 1e-6, "kernel sum {s}");
        // symmetric
        for i in 0..k.len() / 2 {
            assert!((k[i] - k[k.len() - 1 - i]).abs() < 1e-7);
        }
        // peak at centre
        let mid = k.len() / 2;
        assert!(k[mid] >= *k.iter().max_by(|a, b| a.partial_cmp(b).unwrap()).unwrap() - 1e-7);
    }

    #[test]
    fn gaussian_kernel_zero_sigma_is_unit() {
        assert_eq!(gaussian_kernel_1d(0.0), vec![1.0]);
    }

    // ---- Gaussian blur ----

    #[test]
    fn gaussian_zero_sigma_identity() {
        let p = flat(5, 5, [0.5, 0.25, 0.1, 1.0]);
        assert_eq!(gaussian_blur(&p, 5, 5, 0.0), p);
    }

    #[test]
    fn gaussian_constant_unchanged() {
        let p = flat(9, 9, [0.4, 0.6, 0.2, 1.0]);
        let out = gaussian_blur(&p, 9, 9, 2.5);
        for (a, b) in out.iter().zip(p.iter()) {
            assert!((a - b).abs() < 1e-5, "constant drifted {a} vs {b}");
        }
    }

    #[test]
    fn gaussian_impulse_energy_preserved_and_symmetric() {
        // Impulse far from edges so the kernel never clamps → energy ≈ 1.
        let p = impulse(21, 21, 10, 10);
        let out = gaussian_blur(&p, 21, 21, 2.0);
        let s = sum_channel(&out, 0);
        assert!((s - 1.0).abs() < 1e-4, "energy not preserved: {s}");
        // centre is the max
        let ci = ((10 * 21 + 10) * 4) as usize;
        let edge = out[((10 * 21 + 4) * 4) as usize];
        assert!(out[ci] > edge, "centre not peak");
        // symmetry across the impulse column
        for d in 1..=5usize {
            let l = out[((10 * 21 + (10 - d)) * 4) as usize];
            let r = out[((10 * 21 + (10 + d)) * 4) as usize];
            assert!((l - r).abs() < 1e-6, "asymmetric at d={d}: {l} vs {r}");
        }
    }

    #[test]
    fn gaussian_separable_equals_2d_reference() {
        // Random-ish small image; H∘V must equal a full 2-D convolution with the
        // same per-axis clamping.
        let (w, h) = (7u32, 6u32);
        let mut p = vec![0.0f32; (w * h * 4) as usize];
        for (i, v) in p.iter_mut().enumerate() {
            *v = ((i * 37 % 100) as f32) / 100.0;
        }
        let sigma = 1.5;
        let sep = gaussian_blur(&p, w, h, sigma);

        // 2-D reference with per-axis clamp.
        let k = gaussian_kernel_1d(sigma);
        let r = (k.len() / 2) as i32;
        let (wi, hi) = (w as usize, h as usize);
        let mut reference = vec![0.0f32; wi * hi * 4];
        for y in 0..hi {
            for x in 0..wi {
                let mut acc = [0.0f32; 4];
                for (kj, &kwy) in k.iter().enumerate() {
                    let sy = (y as i32 + kj as i32 - r).clamp(0, hi as i32 - 1) as usize;
                    for (ki, &kwx) in k.iter().enumerate() {
                        let sx = (x as i32 + ki as i32 - r).clamp(0, wi as i32 - 1) as usize;
                        let si = (sy * wi + sx) * 4;
                        let wgt = kwx * kwy;
                        for c in 0..4 {
                            acc[c] += p[si + c] * wgt;
                        }
                    }
                }
                let di = (y * wi + x) * 4;
                reference[di..di + 4].copy_from_slice(&acc);
            }
        }
        for (a, b) in sep.iter().zip(reference.iter()) {
            assert!((a - b).abs() < 1e-5, "separable != 2D: {a} vs {b}");
        }
    }

    #[test]
    fn gaussian_deterministic() {
        let p = impulse(15, 15, 7, 7);
        assert_eq!(gaussian_blur(&p, 15, 15, 1.7), gaussian_blur(&p, 15, 15, 1.7));
    }

    // ---- Box blur ----

    #[test]
    fn box_zero_radius_identity() {
        let p = flat(6, 6, [0.3, 0.7, 0.2, 1.0]);
        assert_eq!(box_blur(&p, 6, 6, 0), p);
    }

    #[test]
    fn box_flat_region_is_average() {
        let p = flat(8, 8, [0.4, 0.4, 0.4, 1.0]);
        let out = box_blur(&p, 8, 8, 2);
        for ch in out.chunks(4) {
            assert!((ch[0] - 0.4).abs() < 1e-5, "flat avg drifted {}", ch[0]);
        }
    }

    #[test]
    fn box_averages_two_value_region_center() {
        // 1-px-radius box over a flat interior averages to the same value; check a
        // checkerless flat block plus a known average at an interior pixel.
        let mut p = flat(5, 1, [0.0, 0.0, 0.0, 1.0]);
        // values across a row: 0,1,0,1,0
        for x in 0..5usize {
            p[x * 4] = if x % 2 == 1 { 1.0 } else { 0.0 };
        }
        let out = box_blur(&p, 5, 1, 1);
        // interior pixel x=2: mean of (1,0,1)/3 = 0.6667
        assert!((out[2 * 4] - (2.0 / 3.0)).abs() < 1e-5, "got {}", out[2 * 4]);
    }

    #[test]
    fn box_blur_energy_preserved_interior() {
        let p = impulse(21, 21, 10, 10);
        let out = box_blur(&p, 21, 21, 2);
        let s = sum_channel(&out, 0);
        assert!((s - 1.0).abs() < 1e-4, "box energy {s}");
    }

    #[test]
    fn box_gaussian_flat_preserved() {
        let p = flat(10, 10, [0.55, 0.2, 0.9, 1.0]);
        let out = box_blur_gaussian(&p, 10, 10, 2.0, 3);
        for ch in out.chunks(4) {
            assert!((ch[0] - 0.55).abs() < 1e-4, "drift {}", ch[0]);
        }
    }

    #[test]
    fn box_gaussian_approximates_true_gaussian() {
        // On an impulse, the 3-pass box stack should land close to the real
        // Gaussian (energy preserved, peak in the same place).
        let p = impulse(31, 31, 15, 15);
        let approx = box_blur_gaussian(&p, 31, 31, 3.0, 3);
        let exact = gaussian_blur(&p, 31, 31, 3.0);
        let s = sum_channel(&approx, 0);
        assert!((s - 1.0).abs() < 1e-3, "approx energy {s}");
        let ci = ((15 * 31 + 15) * 4) as usize;
        // Peaks should be reasonably close.
        assert!((approx[ci] - exact[ci]).abs() < 0.02, "peak {} vs {}", approx[ci], exact[ci]);
    }

    #[test]
    fn box_gaussian_zero_identity() {
        let p = flat(4, 4, [0.1, 0.2, 0.3, 1.0]);
        assert_eq!(box_blur_gaussian(&p, 4, 4, 0.0, 3), p);
    }

    // ---- Motion blur ----

    #[test]
    fn motion_zero_length_identity() {
        let p = flat(4, 4, [0.5, 0.5, 0.5, 1.0]);
        assert_eq!(motion_blur(&p, 4, 4, 45.0, 0.0), p);
    }

    #[test]
    fn motion_flat_preserved() {
        let p = flat(10, 10, [0.4, 0.6, 0.2, 1.0]);
        let out = motion_blur(&p, 10, 10, 30.0, 6.0);
        for ch in out.chunks(4) {
            assert!((ch[0] - 0.4).abs() < 1e-3, "drift {}", ch[0]);
        }
    }

    #[test]
    fn motion_smears_along_angle_only() {
        // Horizontal (angle 0) smear of a centred impulse spreads only along the
        // impulse's own row — every other row stays exactly zero.
        let p = impulse(21, 21, 10, 10);
        let out = motion_blur(&p, 21, 21, 0.0, 9.0);
        // Same row spreads.
        let left = out[((10 * 21 + 7) * 4) as usize];
        assert!(left > 0.01, "did not smear along row: {left}");
        // A different row is untouched (no vertical bleed).
        for x in 0..21usize {
            let v = out[((9 * 21 + x) * 4) as usize];
            assert!(v.abs() < 1e-6, "vertical bleed at x={x}: {v}");
        }
    }

    #[test]
    fn motion_deterministic() {
        let p = impulse(16, 16, 8, 8);
        assert_eq!(motion_blur(&p, 16, 16, 20.0, 7.0), motion_blur(&p, 16, 16, 20.0, 7.0));
    }

    // ---- Radial blurs ----

    #[test]
    fn zoom_zero_amount_identity() {
        let p = flat(8, 8, [0.3, 0.3, 0.3, 1.0]);
        assert_eq!(zoom_blur(&p, 8, 8, [4.0, 4.0], 0.0, 8), p);
    }

    #[test]
    fn zoom_flat_preserved() {
        let p = flat(12, 12, [0.5, 0.4, 0.3, 1.0]);
        let out = zoom_blur(&p, 12, 12, [6.0, 6.0], 0.3, 12);
        for ch in out.chunks(4) {
            assert!((ch[0] - 0.5).abs() < 1e-3, "drift {}", ch[0]);
        }
    }

    #[test]
    fn spin_zero_angle_identity() {
        let p = flat(8, 8, [0.2, 0.5, 0.8, 1.0]);
        assert_eq!(spin_blur(&p, 8, 8, [4.0, 4.0], 0.0, 8), p);
    }

    #[test]
    fn spin_flat_preserved() {
        let p = flat(12, 12, [0.7, 0.2, 0.5, 1.0]);
        let out = spin_blur(&p, 12, 12, [6.0, 6.0], 20.0, 12);
        for ch in out.chunks(4) {
            assert!((ch[0] - 0.7).abs() < 1e-3, "drift {}", ch[0]);
        }
    }

    // ---- Unsharp mask ----

    #[test]
    fn unsharp_zero_amount_identity() {
        let p = flat(6, 6, [0.5, 0.5, 0.5, 1.0]);
        assert_eq!(unsharp_mask(&p, 6, 6, 1.5, 0.0, 0.0), p);
    }

    #[test]
    fn unsharp_flat_unchanged() {
        let p = flat(10, 10, [0.45, 0.45, 0.45, 1.0]);
        let out = unsharp_mask(&p, 10, 10, 1.5, 1.0, 0.0);
        for (a, b) in out.iter().zip(p.iter()) {
            assert!((a - b).abs() < 1e-5, "flat changed {a} vs {b}");
        }
    }

    #[test]
    fn unsharp_increases_edge_contrast() {
        // Vertical step edge: 0.3 | 0.7. After unsharp the bright side near the
        // edge overshoots up and the dark side overshoots down.
        let (w, h) = (16u32, 8u32);
        let mut p = flat(w, h, [0.3, 0.3, 0.3, 1.0]);
        for y in 0..h {
            for x in 8..w {
                let i = ((y * w + x) * 4) as usize;
                p[i] = 0.7;
                p[i + 1] = 0.7;
                p[i + 2] = 0.7;
            }
        }
        let out = unsharp_mask(&p, w, h, 1.5, 1.5, 0.0);
        // Just on the bright side of the edge (x=8) should rise above 0.7.
        let bright = out[((4 * w + 8) * 4) as usize];
        // Just on the dark side (x=7) should fall below 0.3.
        let dark = out[((4 * w + 7) * 4) as usize];
        assert!(bright > 0.7 + 1e-3, "no bright overshoot: {bright}");
        assert!(dark < 0.3 - 1e-3, "no dark overshoot: {dark}");
    }

    #[test]
    fn unsharp_threshold_skips_low_detail() {
        // A tiny ramp whose detail is below a high threshold is left unchanged.
        let (w, h) = (12u32, 4u32);
        let mut p = flat(w, h, [0.5, 0.5, 0.5, 1.0]);
        for y in 0..h {
            for x in 0..w {
                let i = ((y * w + x) * 4) as usize;
                let v = 0.5 + 0.002 * x as f32; // very gentle ramp
                p[i] = v;
                p[i + 1] = v;
                p[i + 2] = v;
            }
        }
        let out = unsharp_mask(&p, w, h, 1.5, 2.0, 0.2);
        for (a, b) in out.iter().zip(p.iter()) {
            assert!((a - b).abs() < 1e-4, "low detail boosted {a} vs {b}");
        }
    }

    // ---- Smart sharpen ----

    #[test]
    fn smart_zero_amount_identity() {
        let p = flat(6, 6, [0.5, 0.4, 0.3, 1.0]);
        assert_eq!(smart_sharpen(&p, 6, 6, 1.5, 0.0, 0.05), p);
    }

    #[test]
    fn smart_leaves_flat_untouched() {
        let p = flat(12, 12, [0.42, 0.42, 0.42, 1.0]);
        let out = smart_sharpen(&p, 12, 12, 1.5, 2.0, 0.05);
        for (a, b) in out.iter().zip(p.iter()) {
            assert!((a - b).abs() < 1e-6, "flat sharpened {a} vs {b}");
        }
    }

    #[test]
    fn smart_sharpens_edges() {
        // An edge region (high variance) should still be modified.
        let (w, h) = (16u32, 8u32);
        let mut p = flat(w, h, [0.2, 0.2, 0.2, 1.0]);
        for y in 0..h {
            for x in 8..w {
                let i = ((y * w + x) * 4) as usize;
                p[i] = 0.9;
                p[i + 1] = 0.9;
                p[i + 2] = 0.9;
            }
        }
        let out = smart_sharpen(&p, w, h, 1.5, 1.5, 0.02);
        let mut changed = false;
        for (a, b) in out.iter().zip(p.iter()) {
            if (a - b).abs() > 1e-3 {
                changed = true;
                break;
            }
        }
        assert!(changed, "edge not sharpened at all");
    }

    // ---- Displacement map ----

    #[test]
    fn displacement_zero_map_identity() {
        let p = impulse(8, 8, 4, 4);
        let disp = vec![0.0f32; 8 * 8 * 4]; // all zero ⇒ zero offset everywhere
        let out = displacement_map(&p, 8, 8, &disp, 8, 8, 10.0, 10.0);
        for (a, b) in out.iter().zip(p.iter()) {
            assert!((a - b).abs() < 1e-5, "zero map not identity {a} vs {b}");
        }
    }

    #[test]
    fn displacement_shifts_by_mapped_offset() {
        // A feature at x=5; a uniform R=1 displacement with scale_x=3 makes each
        // dest read src[x-3], so the feature appears at x=8.
        let (w, h) = (16u32, 1u32);
        let mut p = vec![0.0f32; (w * h * 4) as usize];
        let fi = (5 * 4) as usize;
        p[fi] = 1.0;
        p[fi + 3] = 1.0;
        // Displacement: R=1 (premult by a=1 ⇒ store 1), G=0, A=1.
        let mut disp = vec![0.0f32; (w * h * 4) as usize];
        for x in 0..w as usize {
            disp[x * 4] = 1.0; // R
            disp[x * 4 + 3] = 1.0; // A
        }
        let out = displacement_map(&p, w, h, &disp, w, h, 3.0, 0.0);
        assert!(out[(8 * 4) as usize] > 0.9, "feature not at x=8: {}", out[(8 * 4) as usize]);
        assert!(out[(5 * 4) as usize] < 0.1, "feature still at x=5: {}", out[(5 * 4) as usize]);
    }

    #[test]
    fn displacement_edge_clamps_no_oob() {
        // Huge scale would push reads far out of bounds — must clamp, not panic.
        let p = flat(8, 8, [0.5, 0.5, 0.5, 1.0]);
        let mut disp = vec![0.0f32; 8 * 8 * 4];
        for x in 0..8 * 8 {
            disp[x * 4] = 1.0;
            disp[x * 4 + 1] = 1.0;
            disp[x * 4 + 3] = 1.0;
        }
        let out = displacement_map(&p, 8, 8, &disp, 8, 8, 1000.0, 1000.0);
        // Flat source ⇒ clamped reads still return 0.5 everywhere.
        for ch in out.chunks(4) {
            assert!((ch[0] - 0.5).abs() < 1e-5, "clamp drift {}", ch[0]);
        }
    }

    #[test]
    fn displacement_deterministic() {
        let p = impulse(10, 10, 5, 5);
        let mut disp = vec![0.0f32; 10 * 10 * 4];
        for x in 0..10 * 10 {
            disp[x * 4 + 1] = 0.5; // G
            disp[x * 4 + 3] = 1.0;
        }
        let a = displacement_map(&p, 10, 10, &disp, 10, 10, 2.0, 4.0);
        let b = displacement_map(&p, 10, 10, &disp, 10, 10, 2.0, 4.0);
        assert_eq!(a, b);
    }

    #[test]
    fn empty_image_is_safe() {
        let empty: Vec<f32> = vec![];
        assert!(gaussian_blur(&empty, 0, 0, 2.0).is_empty());
        assert!(box_blur(&empty, 0, 0, 2).is_empty());
        assert!(motion_blur(&empty, 0, 0, 30.0, 5.0).is_empty());
        assert!(zoom_blur(&empty, 0, 0, [0.0, 0.0], 0.5, 8).is_empty());
    }
}
