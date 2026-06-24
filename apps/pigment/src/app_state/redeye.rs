//! Red-eye removal: within a circular region, detect strongly-red pixels (the
//! classic flash red-eye signature, where the red channel dominates green/blue)
//! and neutralize them by desaturating toward luma and darkening, scaled by a
//! `darken` strength. Pure pixel math; runs on the active layer's premultiplied
//! RGBA f32 buffer (read → modify → upload), like the destructive filters.

/// Process one circular red-eye region over a premultiplied RGBA f32 buffer.
///
/// For each pixel inside the radius whose (straight) red channel exceeds both
/// green and blue by a clear margin, blend its color toward neutral grey (its
/// own luma) and multiply by `1 - darken`. Returns the modified buffer. `darken`
/// is clamped to 0..1. Operates per-pixel on premultiplied storage by
/// unpremultiplying, correcting straight color, and re-premultiplying.
pub fn red_eye_region(
    px: &[f32],
    w: u32,
    h: u32,
    cx: f32,
    cy: f32,
    radius: f32,
    darken: f32,
) -> Vec<f32> {
    let dk = darken.clamp(0.0, 1.0);
    let r = radius.max(0.0);
    if r < 0.5 {
        return px.to_vec();
    }
    let r2 = r * r;
    let (wi, hi) = (w as i32, h as i32);
    let x0 = ((cx - r).floor() as i32).max(0);
    let x1 = ((cx + r).ceil() as i32).min(wi - 1);
    let y0 = ((cy - r).floor() as i32).max(0);
    let y1 = ((cy + r).ceil() as i32).min(hi - 1);
    let mut out = px.to_vec();
    for y in y0..=y1 {
        for x in x0..=x1 {
            let ddx = x as f32 + 0.5 - cx;
            let ddy = y as f32 + 0.5 - cy;
            if ddx * ddx + ddy * ddy > r2 {
                continue;
            }
            let i = ((y as u32 * w + x as u32) * 4) as usize;
            let a = px[i + 3];
            if a <= 1e-6 {
                continue;
            }
            // Unpremultiply to straight color.
            let rr = px[i] / a;
            let gg = px[i + 1] / a;
            let bb = px[i + 2] / a;
            // Red-eye signature: red clearly dominates the other channels.
            let other = gg.max(bb);
            let is_red = rr > other + 0.10 && rr > 0.20;
            if !is_red {
                continue;
            }
            // Strength of the redness, 0..1, so the correction fades at the edge
            // of the red region instead of hard-clipping.
            let redness = ((rr - other) / (rr + 1e-4)).clamp(0.0, 1.0);
            let strength = redness * dk;
            // Desaturate toward luma, then darken.
            let luma = 0.2126 * rr + 0.7152 * gg + 0.0722 * bb;
            let nr = (rr + (luma - rr) * strength) * (1.0 - 0.5 * strength);
            let ng = gg + (luma - gg) * strength;
            let nb = bb + (luma - bb) * strength;
            out[i] = nr * a;
            out[i + 1] = ng * a;
            out[i + 2] = nb * a;
            out[i + 3] = a;
        }
    }
    out
}

use super::App;

impl App {
    /// Apply red-eye removal at `center` within `radius` px on the active layer,
    /// using `darken` strength. Also records the operation in `last_red_eye` for
    /// the UI/undo bookkeeping the previous stub relied on.
    pub(super) fn apply_red_eye(&mut self, center: [f32; 2], radius: f32, darken: f32) {
        self.last_red_eye = Some((center, radius, darken));
        let Some(layer) = self.paint_target() else {
            return;
        };
        let (dw, dh) = (self.host.doc_w, self.host.doc_h);
        if let Some(px) = self.host.read_layer_f32(layer) {
            let r = red_eye_region(&px, dw, dh, center[0], center[1], radius, darken);
            self.host.upload_layer_f32(layer, &r);
            self.status_message = Some("Red-eye removed".to_string());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn premul(straight: [f32; 4]) -> [f32; 4] {
        [
            straight[0] * straight[3],
            straight[1] * straight[3],
            straight[2] * straight[3],
            straight[3],
        ]
    }

    fn flat(w: u32, h: u32, straight: [f32; 4]) -> Vec<f32> {
        let c = premul(straight);
        let mut v = Vec::with_capacity((w * h * 4) as usize);
        for _ in 0..(w * h) {
            v.extend_from_slice(&c);
        }
        v
    }

    #[test]
    fn neutral_pixels_untouched() {
        // A grey field has no red dominance → unchanged.
        let p = flat(8, 8, [0.5, 0.5, 0.5, 1.0]);
        let out = red_eye_region(&p, 8, 8, 4.0, 4.0, 4.0, 1.0);
        for (a, b) in out.iter().zip(p.iter()) {
            assert!((a - b).abs() < 1e-5, "neutral changed");
        }
    }

    #[test]
    fn red_pixel_in_radius_is_neutralized() {
        // Strong red pixel at the centre gets desaturated + darkened.
        let mut p = flat(8, 8, [0.1, 0.1, 0.1, 1.0]);
        let i = ((4 * 8 + 4) * 4) as usize;
        // straight red 0.9 premultiplied (a=1) stays 0.9
        p[i] = 0.9;
        p[i + 1] = 0.05;
        p[i + 2] = 0.05;
        let out = red_eye_region(&p, 8, 8, 4.5, 4.5, 3.0, 1.0);
        // Red channel should drop substantially toward the others.
        assert!(out[i] < 0.6, "red not reduced: {}", out[i]);
        // Result should be roughly neutral (r close to g/b).
        assert!((out[i] - out[i + 1]).abs() < 0.35, "not desaturated enough");
    }

    #[test]
    fn red_pixel_outside_radius_untouched() {
        let mut p = flat(16, 16, [0.1, 0.1, 0.1, 1.0]);
        // Red pixel at a far corner.
        let i = 0usize;
        p[i] = 0.9;
        p[i + 1] = 0.05;
        p[i + 2] = 0.05;
        let out = red_eye_region(&p, 16, 16, 12.0, 12.0, 3.0, 1.0);
        assert!((out[i] - 0.9).abs() < 1e-5, "outside radius changed: {}", out[i]);
    }

    #[test]
    fn zero_radius_is_identity() {
        let p = flat(4, 4, [0.9, 0.1, 0.1, 1.0]);
        let out = red_eye_region(&p, 4, 4, 2.0, 2.0, 0.0, 1.0);
        assert_eq!(out, p);
    }

    #[test]
    fn darken_zero_leaves_red() {
        // darken 0 → strength 0 → no change even for red pixels.
        let mut p = flat(8, 8, [0.1, 0.1, 0.1, 1.0]);
        let i = ((4 * 8 + 4) * 4) as usize;
        p[i] = 0.9;
        p[i + 1] = 0.05;
        p[i + 2] = 0.05;
        let out = red_eye_region(&p, 8, 8, 4.5, 4.5, 3.0, 0.0);
        assert!((out[i] - 0.9).abs() < 1e-5);
    }

    #[test]
    fn respects_premultiplied_alpha() {
        // Half-transparent red pixel: stored premultiplied (0.45,0.025,0.025,0.5).
        let mut p = flat(8, 8, [0.1, 0.1, 0.1, 1.0]);
        let i = ((4 * 8 + 4) * 4) as usize;
        p[i] = 0.45;
        p[i + 1] = 0.025;
        p[i + 2] = 0.025;
        p[i + 3] = 0.5;
        let out = red_eye_region(&p, 8, 8, 4.5, 4.5, 3.0, 1.0);
        // Alpha preserved.
        assert!((out[i + 3] - 0.5).abs() < 1e-5);
        // Premultiplied red reduced.
        assert!(out[i] < 0.45, "premult red not reduced: {}", out[i]);
    }
}
