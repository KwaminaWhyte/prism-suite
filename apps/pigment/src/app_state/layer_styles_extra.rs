//! Destructive "bake" arms for the two remaining layer styles: **Satin** and
//! **Pattern Overlay**. The non-destructive config + storage already lives in
//! `shapes.rs` (`SetSatinEffect` / `SetPatternOverlay`); these arms take that
//! stored config and actually rasterize the effect into the layer pixels, the
//! same read → blend → upload pattern the destructive filters use.
//!
//! All effect math is in free functions so it is unit-testable without a GPU.

use super::{Action, App};

/// Convert an sRGB-ish straight `[u8;4]` style color into linear-premultiplied
/// `[f32;4]`. Style colors in this app are stored 0..255 straight; we keep them
/// in the same working space the layer buffers use (linear-premultiplied), which
/// for these overlay effects we approximate by treating the 0..1 value as linear
/// (the layer buffer is already linear-premultiplied f32).
#[inline]
fn style_color_to_linear_premul(c: [u8; 4], opacity_0_100: u8) -> [f32; 4] {
    let a = (c[3] as f32 / 255.0) * (opacity_0_100 as f32 / 100.0);
    let r = c[0] as f32 / 255.0;
    let g = c[1] as f32 / 255.0;
    let b = c[2] as f32 / 255.0;
    [r * a, g * a, b * a, a]
}

/// Blend a premultiplied `src` over a premultiplied `dst` with the named blend
/// mode, masked by `mask` (0..1). Only a small set of modes is supported here;
/// unknown names fall back to Normal. Returns the new premultiplied dst.
fn blend_over(dst: [f32; 4], src: [f32; 4], mode: &str, mask: f32) -> [f32; 4] {
    let m = mask.clamp(0.0, 1.0);
    // Effective source alpha after masking.
    let sa = src[3] * m;
    if sa <= 0.0 {
        return dst;
    }
    // Unpremultiply both to operate on straight color for the blend math.
    let unp = |c: [f32; 4]| -> [f32; 4] {
        if c[3] > 1e-6 {
            [c[0] / c[3], c[1] / c[3], c[2] / c[3], c[3]]
        } else {
            [0.0, 0.0, 0.0, 0.0]
        }
    };
    let d = unp(dst);
    let s = unp([src[0], src[1], src[2], 1.0]); // straight src color
    let blended = |dc: f32, sc: f32| -> f32 {
        match mode {
            "Multiply" => dc * sc,
            "Screen" => 1.0 - (1.0 - dc) * (1.0 - sc),
            "Overlay" => {
                if dc < 0.5 {
                    2.0 * dc * sc
                } else {
                    1.0 - 2.0 * (1.0 - dc) * (1.0 - sc)
                }
            }
            "Linear Dodge" | "Add" => (dc + sc).min(1.0),
            // Normal and anything unknown.
            _ => sc,
        }
    };
    let mut out_straight = [0.0f32; 3];
    for c in 0..3 {
        let bc = blended(d[c], s[c]);
        // Source-over with the (masked) source alpha against existing dst color.
        out_straight[c] = bc * sa + d[c] * (1.0 - sa);
    }
    // New alpha: source-over composite of alphas.
    let out_a = sa + dst[3] * (1.0 - sa);
    [
        out_straight[0] * out_a,
        out_straight[1] * out_a,
        out_straight[2] * out_a,
        out_a,
    ]
}

/// Build a Satin interference mask (0..1 per pixel) from the layer alpha.
///
/// Photoshop's Satin offsets the layer shape twice in opposite directions and
/// combines them; the overlap forms a soft interference band. We approximate
/// that: sample alpha offset by `+offset` and `-offset` (at `angle`), take the
/// absolute difference, smooth it slightly, and optionally invert.
pub fn satin_mask(
    alpha: &[f32],
    w: u32,
    h: u32,
    angle_deg: f32,
    distance: f32,
    invert: bool,
) -> Vec<f32> {
    let (wi, hi) = (w as i32, h as i32);
    let rad = angle_deg.to_radians();
    let ox = rad.cos() * distance;
    let oy = rad.sin() * distance;
    let sample = |x: i32, y: i32| -> f32 {
        let xx = x.clamp(0, wi - 1) as usize;
        let yy = y.clamp(0, hi - 1) as usize;
        alpha[yy * w as usize + xx]
    };
    let mut mask = vec![0.0f32; (w * h) as usize];
    for y in 0..hi {
        for x in 0..wi {
            let a_pos = sample(x + ox.round() as i32, y + oy.round() as i32);
            let a_neg = sample(x - ox.round() as i32, y - oy.round() as i32);
            let mut m = (a_pos - a_neg).abs();
            if invert {
                m = 1.0 - m;
            }
            // Confine the satin to where the layer itself has coverage.
            let here = sample(x, y);
            mask[(y * wi + x) as usize] = (m * here).clamp(0.0, 1.0);
        }
    }
    mask
}

/// Sample a tiled pattern at doc pixel `(x, y)` with the given `scale` (percent,
/// 100 = 1:1) and pixel offset. Returns straight RGBA in 0..1.
#[inline]
pub fn sample_pattern(
    pat: &[[f32; 4]],
    pw: u32,
    ph: u32,
    x: u32,
    y: u32,
    scale_pct: u32,
    offset_x: f32,
    offset_y: f32,
) -> [f32; 4] {
    if pw == 0 || ph == 0 || pat.is_empty() {
        return [0.0, 0.0, 0.0, 0.0];
    }
    let s = (scale_pct.max(1) as f32) / 100.0;
    // Map doc px to pattern space (scaled), wrapping (tiling).
    let px = ((x as f32 - offset_x) / s).floor() as i64;
    let py = ((y as f32 - offset_y) / s).floor() as i64;
    let wx = px.rem_euclid(pw as i64) as usize;
    let wy = py.rem_euclid(ph as i64) as usize;
    pat[wy * pw as usize + wx]
}

impl App {
    pub(super) fn apply_layer_styles_extra(&mut self, action: Action) {
        match action {
            Action::BakeSatinEffect { layer_id } => {
                let cfg = match self.satin_effects.get(&layer_id) {
                    Some(c) => c.clone(),
                    None => super::shapes::SatinEffect::default(),
                };
                let lid = prism_core::LayerId(layer_id as u64);
                let (dw, dh) = (self.host.doc_w, self.host.doc_h);
                if let Some(px) = self.host.read_layer_f32(lid) {
                    let n = (dw * dh) as usize;
                    let mut alpha = vec![0.0f32; n];
                    for i in 0..n {
                        alpha[i] = px[i * 4 + 3];
                    }
                    let mask = satin_mask(
                        &alpha,
                        dw,
                        dh,
                        cfg.angle,
                        cfg.distance as f32,
                        cfg.invert,
                    );
                    let scolor = style_color_to_linear_premul(cfg.color, cfg.opacity);
                    let mut out = px.clone();
                    for i in 0..n {
                        let dst = [px[i * 4], px[i * 4 + 1], px[i * 4 + 2], px[i * 4 + 3]];
                        let res = blend_over(dst, scolor, &cfg.blend_mode, mask[i]);
                        out[i * 4] = res[0];
                        out[i * 4 + 1] = res[1];
                        out[i * 4 + 2] = res[2];
                        out[i * 4 + 3] = res[3];
                    }
                    self.host.upload_layer_f32(lid, &out);
                    self.status_message = Some("Satin baked".to_string());
                }
            }
            Action::BakePatternOverlay { layer_id } => {
                let cfg = match self.pattern_overlays.get(&layer_id) {
                    Some(c) => c.clone(),
                    None => super::shapes::PatternOverlay::default(),
                };
                // Resolve which pattern to use: the overlay's pattern_id, else the
                // active pattern, else the first library entry.
                let pat_idx = cfg
                    .pattern_id
                    .or(self.active_pattern_idx)
                    .filter(|&i| i < self.pattern_library.len());
                let Some(pi) = pat_idx else {
                    self.status_message = Some("Pattern Overlay: no pattern selected".to_string());
                    return;
                };
                let pat = self.pattern_library[pi].clone();
                let lid = prism_core::LayerId(layer_id as u64);
                let (dw, dh) = (self.host.doc_w, self.host.doc_h);
                if let Some(px) = self.host.read_layer_f32(lid) {
                    let n = (dw * dh) as usize;
                    let opa = cfg.opacity as f32 / 100.0;
                    let mut out = px.clone();
                    for y in 0..dh {
                        for x in 0..dw {
                            let i = (y * dw + x) as usize;
                            let alpha = px[i * 4 + 3];
                            if alpha <= 0.0 {
                                continue; // overlay only where the layer has pixels
                            }
                            let s = sample_pattern(
                                &pat.pixels,
                                pat.width,
                                pat.height,
                                x,
                                y,
                                cfg.scale,
                                cfg.offset_x,
                                cfg.offset_y,
                            );
                            // Premultiplied source, weighted by overlay opacity.
                            let src = [s[0] * s[3], s[1] * s[3], s[2] * s[3], s[3] * opa];
                            let dst = [px[i * 4], px[i * 4 + 1], px[i * 4 + 2], px[i * 4 + 3]];
                            // Mask the overlay by the layer alpha so it clips to shape.
                            let res = blend_over(dst, src, &cfg.blend_mode, alpha);
                            out[i * 4] = res[0];
                            out[i * 4 + 1] = res[1];
                            out[i * 4 + 2] = res[2];
                            out[i * 4 + 3] = res[3];
                        }
                    }
                    self.host.upload_layer_f32(lid, &out);
                    self.status_message = Some("Pattern Overlay baked".to_string());
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn style_color_premul_scales_by_opacity() {
        // Solid red at 50% opacity → alpha 0.5, premultiplied red 0.5.
        let c = style_color_to_linear_premul([255, 0, 0, 255], 50);
        assert!((c[3] - 0.5).abs() < 1e-4);
        assert!((c[0] - 0.5).abs() < 1e-4);
        assert!(c[1].abs() < 1e-4);
    }

    #[test]
    fn blend_normal_replaces_color() {
        // Opaque white over opaque black, normal, full mask → white.
        let dst = [0.0, 0.0, 0.0, 1.0];
        let src = [1.0, 1.0, 1.0, 1.0];
        let out = blend_over(dst, src, "Normal", 1.0);
        assert!((out[0] - 1.0).abs() < 1e-3, "{:?}", out);
        assert!((out[3] - 1.0).abs() < 1e-3);
    }

    #[test]
    fn blend_zero_mask_is_noop() {
        let dst = [0.2, 0.3, 0.4, 1.0];
        let src = [1.0, 0.0, 0.0, 1.0];
        let out = blend_over(dst, src, "Normal", 0.0);
        assert_eq!(out, dst);
    }

    #[test]
    fn blend_multiply_darkens() {
        // grey * grey < grey
        let dst = [0.5, 0.5, 0.5, 1.0];
        let src = [0.5, 0.5, 0.5, 1.0];
        let out = blend_over(dst, src, "Multiply", 1.0);
        assert!(out[0] < 0.5, "multiply did not darken: {}", out[0]);
    }

    #[test]
    fn satin_mask_flat_alpha_is_zero() {
        // Uniform alpha → no offset difference → zero satin everywhere.
        let alpha = vec![1.0f32; 64];
        let m = satin_mask(&alpha, 8, 8, 19.0, 3.0, false);
        for v in &m {
            assert!(*v < 1e-4, "flat satin nonzero: {v}");
        }
    }

    #[test]
    fn satin_mask_edge_is_nonzero() {
        // A shape with an interior edge produces a nonzero satin band.
        let mut alpha = vec![0.0f32; 16 * 16];
        for y in 0..16 {
            for x in 4..12 {
                alpha[y * 16 + x] = 1.0;
            }
        }
        let m = satin_mask(&alpha, 16, 16, 0.0, 2.0, false);
        let max = m.iter().cloned().fold(0.0f32, f32::max);
        assert!(max > 0.1, "no satin band: {max}");
    }

    #[test]
    fn satin_mask_confined_to_shape() {
        // Outside the shape (alpha 0) the satin mask must be 0.
        let mut alpha = vec![0.0f32; 16 * 16];
        for y in 6..10 {
            for x in 6..10 {
                alpha[y * 16 + x] = 1.0;
            }
        }
        let m = satin_mask(&alpha, 16, 16, 30.0, 2.0, false);
        // A far corner, alpha 0, must be 0.
        assert!(m[0] < 1e-4);
    }

    #[test]
    fn sample_pattern_tiles_and_wraps() {
        // 2x2 checker, scale 100. (0,0) and (2,0) should map to the same texel.
        let pat = vec![
            [1.0, 0.0, 0.0, 1.0],
            [0.0, 1.0, 0.0, 1.0],
            [0.0, 0.0, 1.0, 1.0],
            [1.0, 1.0, 0.0, 1.0],
        ];
        let a = sample_pattern(&pat, 2, 2, 0, 0, 100, 0.0, 0.0);
        let b = sample_pattern(&pat, 2, 2, 2, 0, 100, 0.0, 0.0);
        assert_eq!(a, b);
        let c = sample_pattern(&pat, 2, 2, 1, 0, 100, 0.0, 0.0);
        assert_ne!(a, c);
    }

    #[test]
    fn sample_pattern_empty_is_transparent() {
        let s = sample_pattern(&[], 0, 0, 5, 5, 100, 0.0, 0.0);
        assert_eq!(s, [0.0, 0.0, 0.0, 0.0]);
    }

    #[test]
    fn bake_satin_action_no_panic() {
        let mut app = App::new();
        let id = app.doc.active_layer.map(|l| l.0 as usize).unwrap_or(0);
        app.apply(Action::SetSatinEffect {
            layer_id: id,
            config: super::super::shapes::SatinEffect::default(),
        });
        app.apply(Action::BakeSatinEffect { layer_id: id });
    }

    #[test]
    fn bake_pattern_overlay_without_pattern_sets_message() {
        let mut app = App::new();
        let id = app.doc.active_layer.map(|l| l.0 as usize).unwrap_or(0);
        app.apply(Action::BakePatternOverlay { layer_id: id });
        assert!(app
            .status_message
            .as_deref()
            .unwrap_or("")
            .contains("no pattern"));
    }
}
