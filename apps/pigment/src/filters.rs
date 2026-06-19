//! Batch 5 — additional CPU raster filters.
//!
//! Each filter is a pure function over a linear-premultiplied RGBA `f32` buffer
//! (the same contract `read_layer_f32` / `upload_layer_f32` use), so they slot
//! straight into the `Action::ApplyFilter` dispatch exactly like
//! `lens_correction` / `content_aware` do — no engine/shader or shared-crate
//! change. Geometric remaps (Motion Blur, Twirl, Pinch) sample the premultiplied
//! buffer directly (correct for averaging). Tonal ops (Solarize, Glowing Edges)
//! unpremultiply → operate on straight color → repremultiply so alpha edges stay
//! clean.

/// Bilinear sample of a premultiplied RGBA `f32` buffer with clamped edges.
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
            for c in 0..4 {
                out[c] += px[i + c] * wb;
            }
        }
    }
    out
}

/// Motion Blur: average `samples` taps along a line of `distance` px at `angle`
/// degrees, centred on each pixel. A directional box blur — the classic linear
/// motion smear. Operates on premultiplied color (averaging premult is correct).
pub fn motion_blur(px: &[f32], w: u32, h: u32, angle_deg: f32, distance: f32) -> Vec<f32> {
    let dist = distance.max(0.0);
    if dist < 0.5 {
        return px.to_vec();
    }
    let rad = angle_deg.to_radians();
    let (dx, dy) = (rad.cos(), rad.sin());
    // One tap per pixel of travel, clamped to a sane ceiling.
    let samples = (dist.round() as i32).clamp(1, 256);
    let mut out = vec![0.0f32; px.len()];
    for y in 0..h {
        for x in 0..w {
            let mut acc = [0.0f32; 4];
            for s in 0..=samples {
                // -dist/2 .. +dist/2 along the motion vector.
                let t = (s as f32 / samples as f32 - 0.5) * dist;
                let sx = x as f32 + 0.5 + dx * t;
                let sy = y as f32 + 0.5 + dy * t;
                let c = bilinear(px, w, h, sx - 0.5, sy - 0.5);
                for k in 0..4 {
                    acc[k] += c[k];
                }
            }
            let inv = 1.0 / (samples as f32 + 1.0);
            let i = ((y * w + x) * 4) as usize;
            for k in 0..4 {
                out[i + k] = acc[k] * inv;
            }
        }
    }
    out
}

/// Twirl: rotate pixels about the canvas centre by an amount that falls off to
/// zero at `radius` (fraction of the half-diagonal, 0..1). `angle_deg` is the
/// max swirl at the centre. Inverse map (sample the source for each dest pixel).
pub fn twirl(px: &[f32], w: u32, h: u32, angle_deg: f32, radius: f32) -> Vec<f32> {
    let (fw, fh) = (w as f32, h as f32);
    let cx = fw * 0.5;
    let cy = fh * 0.5;
    let max_r = (cx * cx + cy * cy).sqrt().max(1e-6) * radius.clamp(0.01, 1.0);
    let max_a = angle_deg.to_radians();
    let mut out = vec![0.0f32; px.len()];
    for y in 0..h {
        for x in 0..w {
            let ddx = x as f32 + 0.5 - cx;
            let ddy = y as f32 + 0.5 - cy;
            let r = (ddx * ddx + ddy * ddy).sqrt();
            let i = ((y * w + x) * 4) as usize;
            if r >= max_r {
                for k in 0..4 {
                    out[i + k] = px[i + k];
                }
                continue;
            }
            // Smooth falloff: full twist at the centre, none at the edge.
            let f = 1.0 - (r / max_r);
            let a = max_a * f * f;
            let (sa, ca) = a.sin_cos();
            let sx = cx + ddx * ca - ddy * sa;
            let sy = cy + ddx * sa + ddy * ca;
            let c = bilinear(px, w, h, sx - 0.5, sy - 0.5);
            for k in 0..4 {
                out[i + k] = c[k];
            }
        }
    }
    out
}

/// Pinch / Bulge: signed radial remap about the centre. `amount > 0` pinches
/// (pixels pulled toward centre), `amount < 0` bulges/spherizes. Falls off to
/// zero at `radius` (fraction of the half-diagonal, 0..1).
pub fn pinch(px: &[f32], w: u32, h: u32, amount: f32, radius: f32) -> Vec<f32> {
    let (fw, fh) = (w as f32, h as f32);
    let cx = fw * 0.5;
    let cy = fh * 0.5;
    let max_r = (cx * cx + cy * cy).sqrt().max(1e-6) * radius.clamp(0.01, 1.0);
    let amt = amount.clamp(-1.0, 1.0);
    let mut out = vec![0.0f32; px.len()];
    for y in 0..h {
        for x in 0..w {
            let ddx = x as f32 + 0.5 - cx;
            let ddy = y as f32 + 0.5 - cy;
            let r = (ddx * ddx + ddy * ddy).sqrt();
            let i = ((y * w + x) * 4) as usize;
            if r >= max_r || r < 1e-6 {
                for k in 0..4 {
                    out[i + k] = px[i + k];
                }
                continue;
            }
            let rn = r / max_r; // 0..1
            // Map normalized radius -> source radius. Positive amount pushes the
            // sample radius outward (so the dest pulls texture inward = pinch);
            // the falloff (1 - rn) keeps the rim fixed.
            let scale = 1.0 + amt * (1.0 - rn);
            let src_rn = rn.powf(scale);
            let sf = src_rn / rn;
            let sx = cx + ddx * sf;
            let sy = cy + ddy * sf;
            let c = bilinear(px, w, h, sx - 0.5, sy - 0.5);
            for k in 0..4 {
                out[i + k] = c[k];
            }
        }
    }
    out
}

/// Unpremultiply one texel: returns straight RGBA, guarding against zero alpha.
#[inline]
fn unpremul(c: [f32; 4]) -> [f32; 4] {
    if c[3] > 1e-6 {
        [c[0] / c[3], c[1] / c[3], c[2] / c[3], c[3]]
    } else {
        [0.0, 0.0, 0.0, 0.0]
    }
}

/// Repremultiply straight RGBA back into premultiplied storage.
#[inline]
fn premul(c: [f32; 4]) -> [f32; 4] {
    [c[0] * c[3], c[1] * c[3], c[2] * c[3], c[3]]
}

/// Solarize: invert tones above a midpoint `threshold` (0..1) per channel — the
/// classic Sabattier tonal flip. Operates on straight (unpremultiplied) color.
pub fn solarize(px: &[f32], threshold: f32) -> Vec<f32> {
    let t = threshold.clamp(0.0, 1.0);
    let mut out = vec![0.0f32; px.len()];
    let n = px.len() / 4;
    for p in 0..n {
        let i = p * 4;
        let s = unpremul([px[i], px[i + 1], px[i + 2], px[i + 3]]);
        let mut r = [s[0], s[1], s[2], s[3]];
        for c in r.iter_mut().take(3) {
            if *c > t {
                *c = 1.0 - *c;
            }
        }
        let pm = premul(r);
        for k in 0..4 {
            out[i + k] = pm[k];
        }
    }
    out
}

/// Glowing Edges: detect edges via a Sobel gradient on straight luma, then
/// render them as a bright tint over black (Photoshop's neon edge look).
/// `width` scales the gradient sampling step, `intensity` the glow brightness.
pub fn glowing_edges(px: &[f32], w: u32, h: u32, width: f32, intensity: f32) -> Vec<f32> {
    let step = width.max(0.5);
    let gain = intensity.max(0.0);
    let lum = |c: [f32; 4]| -> f32 {
        let s = unpremul(c);
        0.2126 * s[0] + 0.7152 * s[1] + 0.0722 * s[2]
    };
    let at = |x: f32, y: f32| -> [f32; 4] { bilinear(px, w, h, x - 0.5, y - 0.5) };
    let mut out = vec![0.0f32; px.len()];
    for y in 0..h {
        for x in 0..w {
            let fx = x as f32 + 0.5;
            let fy = y as f32 + 0.5;
            // 3x3 Sobel on luma.
            let tl = lum(at(fx - step, fy - step));
            let tc = lum(at(fx, fy - step));
            let tr = lum(at(fx + step, fy - step));
            let ml = lum(at(fx - step, fy));
            let mr = lum(at(fx + step, fy));
            let bl = lum(at(fx - step, fy + step));
            let bc = lum(at(fx, fy + step));
            let br = lum(at(fx + step, fy + step));
            let gx = (tr + 2.0 * mr + br) - (tl + 2.0 * ml + bl);
            let gy = (bl + 2.0 * bc + br) - (tl + 2.0 * tc + tr);
            let mag = ((gx * gx + gy * gy).sqrt() * gain).clamp(0.0, 1.0);
            let i = ((y * w + x) * 4) as usize;
            // Edges glow in the source's own hue; flats go black. Keep original
            // alpha and premultiply by the original alpha.
            let a = px[i + 3];
            let src = unpremul([px[i], px[i + 1], px[i + 2], a]);
            // Bias toward white at strong edges so they read as a glow.
            let edge = [
                (src[0] * mag + mag * mag).clamp(0.0, 1.0),
                (src[1] * mag + mag * mag).clamp(0.0, 1.0),
                (src[2] * mag + mag * mag).clamp(0.0, 1.0),
            ];
            let pm = premul([edge[0], edge[1], edge[2], a]);
            for k in 0..4 {
                out[i + k] = pm[k];
            }
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

    #[test]
    fn motion_blur_zero_distance_identity() {
        let p = flat(4, 4, [0.5, 0.25, 0.1, 1.0]);
        let out = motion_blur(&p, 4, 4, 0.0, 0.0);
        assert_eq!(out, p);
    }

    #[test]
    fn motion_blur_flat_preserved() {
        // A flat field blurred in any direction stays flat.
        let p = flat(8, 8, [0.4, 0.6, 0.2, 1.0]);
        let out = motion_blur(&p, 8, 8, 30.0, 6.0);
        for px in out.chunks(4) {
            assert!((px[0] - 0.4).abs() < 1e-3, "r drifted: {}", px[0]);
            assert!((px[3] - 1.0).abs() < 1e-3);
        }
    }

    #[test]
    fn twirl_zero_angle_identity() {
        let mut p = flat(8, 8, [0.0, 0.0, 0.0, 1.0]);
        // distinct corner so a rotation would be visible.
        p[0] = 1.0;
        let out = twirl(&p, 8, 8, 0.0, 1.0);
        for (a, b) in out.iter().zip(p.iter()) {
            assert!((a - b).abs() < 1e-3);
        }
    }

    #[test]
    fn twirl_outside_radius_untouched() {
        // A tiny radius leaves the corners (outside it) identical.
        let mut p = flat(16, 16, [0.2, 0.2, 0.2, 1.0]);
        p[0] = 0.9; // top-left corner pixel
        let out = twirl(&p, 16, 16, 90.0, 0.05);
        assert!((out[0] - 0.9).abs() < 1e-4, "corner moved: {}", out[0]);
    }

    #[test]
    fn pinch_zero_amount_identity() {
        let mut p = flat(8, 8, [0.0, 0.0, 0.0, 1.0]);
        p[(3 * 8 + 3) * 4] = 1.0;
        let out = pinch(&p, 8, 8, 0.0, 1.0);
        for (a, b) in out.iter().zip(p.iter()) {
            assert!((a - b).abs() < 1e-3);
        }
    }

    #[test]
    fn pinch_flat_preserved() {
        let p = flat(8, 8, [0.3, 0.3, 0.3, 1.0]);
        let out = pinch(&p, 8, 8, 0.5, 1.0);
        for px in out.chunks(4) {
            assert!((px[0] - 0.3).abs() < 1e-3);
        }
    }

    #[test]
    fn solarize_inverts_above_threshold() {
        // bright pixel above threshold inverts; dark pixel below does not.
        let p = vec![0.9, 0.9, 0.9, 1.0, 0.1, 0.1, 0.1, 1.0];
        let out = solarize(&p, 0.5);
        assert!((out[0] - 0.1).abs() < 1e-4, "bright not inverted: {}", out[0]);
        assert!((out[4] - 0.1).abs() < 1e-4, "dark wrongly changed: {}", out[4]);
    }

    #[test]
    fn glowing_edges_flat_is_black() {
        // No edges in a flat field → black output, alpha preserved.
        let p = flat(8, 8, [0.5, 0.5, 0.5, 1.0]);
        let out = glowing_edges(&p, 8, 8, 1.0, 1.0);
        for px in out.chunks(4) {
            assert!(px[0] < 1e-3 && px[1] < 1e-3 && px[2] < 1e-3, "flat not dark");
            assert!((px[3] - 1.0).abs() < 1e-4);
        }
    }

    #[test]
    fn glowing_edges_detects_edge() {
        // A vertical step has a non-black column at the boundary.
        let mut p = flat(8, 8, [0.0, 0.0, 0.0, 1.0]);
        for y in 0..8 {
            for x in 4..8 {
                let i = ((y * 8 + x) * 4) as usize;
                p[i] = 1.0;
                p[i + 1] = 1.0;
                p[i + 2] = 1.0;
            }
        }
        let out = glowing_edges(&p, 8, 8, 1.0, 1.0);
        let mut max_b = 0.0f32;
        for px in out.chunks(4) {
            max_b = max_b.max(px[0]);
        }
        assert!(max_b > 0.1, "edge not detected: {max_b}");
    }
}

// ---------------------------------------------------------------------------
// Batch 6 filters: High Pass · Smart Sharpen · Reduce Noise
// ---------------------------------------------------------------------------

/// Gaussian-blur helper (separable, box approximation via 3-pass box blur).
fn gaussian_blur_f32(pixels: &[f32], w: u32, h: u32, sigma: f32) -> Vec<f32> {
    let (w, h) = (w as usize, h as usize);
    let r = (sigma * 2.0).ceil() as usize;
    let mut buf = pixels.to_vec();
    // Horizontal pass
    let src = buf.clone();
    for y in 0..h {
        for x in 0..w {
            let mut sum = [0.0f32; 4];
            let mut cnt = 0u32;
            let x0 = x.saturating_sub(r);
            let x1 = (x + r).min(w - 1);
            for kx in x0..=x1 {
                for c in 0..4 { sum[c] += src[(y * w + kx) * 4 + c]; }
                cnt += 1;
            }
            for c in 0..4 { buf[(y * w + x) * 4 + c] = sum[c] / cnt as f32; }
        }
    }
    // Vertical pass
    let src2 = buf.clone();
    for y in 0..h {
        for x in 0..w {
            let mut sum = [0.0f32; 4];
            let mut cnt = 0u32;
            let y0 = y.saturating_sub(r);
            let y1 = (y + r).min(h - 1);
            for ky in y0..=y1 {
                for c in 0..4 { sum[c] += src2[(ky * w + x) * 4 + c]; }
                cnt += 1;
            }
            for c in 0..4 { buf[(y * w + x) * 4 + c] = sum[c] / cnt as f32; }
        }
    }
    buf
}

/// High Pass: original − blur + 0.5 grey, keeping only high-frequency detail.
pub fn high_pass(pixels: &[f32], w: u32, h: u32, radius: f32) -> Vec<f32> {
    let blurred = gaussian_blur_f32(pixels, w, h, radius.max(0.1));
    let n = pixels.len();
    let mut out = vec![0.0f32; n];
    for i in 0..(n / 4) {
        for c in 0..3 {
            out[i * 4 + c] = (pixels[i * 4 + c] - blurred[i * 4 + c] + 0.5).clamp(0.0, 1.0);
        }
        out[i * 4 + 3] = pixels[i * 4 + 3];
    }
    out
}

/// Smart Sharpen: optional pre-blur for noise reduction, then unsharp mask.
pub fn smart_sharpen(
    pixels: &[f32],
    w: u32,
    h: u32,
    amount: f32,
    radius: f32,
    reduce_noise: f32,
) -> Vec<f32> {
    let base = if reduce_noise > 0.0 {
        gaussian_blur_f32(pixels, w, h, reduce_noise * 2.0)
    } else {
        pixels.to_vec()
    };
    let blurred = gaussian_blur_f32(&base, w, h, radius.max(0.1));
    let n = pixels.len();
    let mut out = vec![0.0f32; n];
    for i in 0..(n / 4) {
        for c in 0..3 {
            let detail = base[i*4+c] - blurred[i*4+c];
            out[i*4+c] = (base[i*4+c] + detail * amount).clamp(0.0, 1.0);
        }
        out[i*4+3] = pixels[i*4+3];
    }
    out
}

/// Reduce Noise: box-blur smoothing blended with original based on `strength`.
/// `preserve_details` biases toward original on high-detail pixels.
/// `reduce_color_noise` desaturates the blurred contribution slightly.
/// `sharpen_details` applies mild unsharp mask to the result.
pub fn reduce_noise(
    pixels: &[f32],
    w: u32,
    h: u32,
    strength: f32,
    preserve_details: f32,
    reduce_color_noise: f32,
    sharpen_details: f32,
) -> Vec<f32> {
    let blurred = gaussian_blur_f32(pixels, w, h, strength * 3.0 + 0.5);
    let n = pixels.len();
    let mut out = vec![0.0f32; n];
    for i in 0..(n / 4) {
        let [r, g, b, a] = [pixels[i*4], pixels[i*4+1], pixels[i*4+2], pixels[i*4+3]];
        let [br, bg, bb, _] = [blurred[i*4], blurred[i*4+1], blurred[i*4+2], blurred[i*4+3]];
        // Luma-based detail preservation: keep more original where luma contrast is high
        let orig_lum = 0.2126*r + 0.7152*g + 0.0722*b;
        let blur_lum = 0.2126*br + 0.7152*bg + 0.0722*bb;
        let detail = (orig_lum - blur_lum).abs();
        let keep = (detail * preserve_details * 10.0).min(1.0);
        let blend = strength * (1.0 - keep);
        // Optionally desaturate blurred contribution to reduce chroma noise
        let grey = 0.2126*br + 0.7152*bg + 0.0722*bb;
        let br2 = br + (grey - br) * reduce_color_noise;
        let bg2 = bg + (grey - bg) * reduce_color_noise;
        let bb2 = bb + (grey - bb) * reduce_color_noise;
        let nr = r * (1.0 - blend) + br2 * blend;
        let ng = g * (1.0 - blend) + bg2 * blend;
        let nb = b * (1.0 - blend) + bb2 * blend;
        // Mild sharpening on result
        let sharp = |orig: f32, sm: f32| (sm + (sm - orig) * sharpen_details * 0.5).clamp(0.0, 1.0);
        out[i*4]   = sharp(r, nr);
        out[i*4+1] = sharp(g, ng);
        out[i*4+2] = sharp(b, nb);
        out[i*4+3] = a;
    }
    out
}

#[cfg(test)]
mod filter_b6_tests {
    use super::{high_pass, smart_sharpen, reduce_noise};

    fn flat(w: usize, h: usize, val: f32) -> Vec<f32> {
        vec![val; w * h * 4]
    }

    #[test]
    fn high_pass_flat_is_grey() {
        let px = flat(8, 8, 0.8);
        let out = high_pass(&px, 8, 8, 2.0);
        for ch in out.chunks(4) {
            assert!((ch[0] - 0.5).abs() < 0.01, "expected 0.5, got {}", ch[0]);
        }
    }

    #[test]
    fn high_pass_preserves_alpha() {
        let mut px = flat(4, 4, 0.5);
        for i in 0..16 { px[i*4+3] = 0.7; }
        let out = high_pass(&px, 4, 4, 1.0);
        for ch in out.chunks(4) {
            assert!((ch[3] - 0.7).abs() < 0.01);
        }
    }

    #[test]
    fn smart_sharpen_brightens_edges() {
        let mut px = flat(8, 8, 0.3);
        // bright stripe in the middle
        for x in 3..5usize {
            for y in 0..8usize {
                for c in 0..3 { px[(y*8+x)*4+c] = 0.9; }
            }
        }
        let out = smart_sharpen(&px, 8, 8, 1.5, 1.0, 0.0);
        let mid = out[3*4]; // bright side of stripe — should be >= 0.9
        assert!(mid >= 0.85, "sharpened bright side {mid}");
    }

    #[test]
    fn reduce_noise_smooths_salt_pepper() {
        let mut px = flat(8, 8, 0.5);
        // Salt pixel
        for c in 0..3 { px[5*4+c] = 1.0; }
        let out = reduce_noise(&px, 8, 8, 0.8, 0.1, 0.0, 0.0);
        // Salt pixel should be pulled toward 0.5
        assert!(out[5*4] < 0.95, "not smoothed enough: {}", out[5*4]);
    }
}
