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
