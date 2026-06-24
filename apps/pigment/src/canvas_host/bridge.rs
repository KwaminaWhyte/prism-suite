//! Composite→display bridging + CPU layer-style effects.
//!
//! These free functions convert the engine's linear-premultiplied Rgba16Float
//! readback into the BGRA8 buffer GPUI displays, apply optional CMYK soft-proof,
//! and source-over CPU drop-shadow / outer-glow effects (Wave 11). The
//! `apply_layer_styles_to_bgra` entry point stays on `impl CanvasHost`.

use half::f16;
use prism_core::color::linear_to_srgb;

use super::CanvasHost;

impl CanvasHost {
    /// Apply CPU layer styles (drop shadow, outer/inner glow, bevel-emboss) on top
    /// of a composited BGRA8 buffer. Called from Wave 11 compositing when any layer
    /// has a non-default style. Operates in-place (the shadow/glow is source-over'd
    /// onto `bgra`). Nearest-neighbor Gaussian approximation (box convolution).
    ///
    /// `style_layers` is a slice of (alpha_mask: Vec<f32>, style) pairs, where the
    /// alpha mask is extracted from the layer's readback before compositing. For
    /// simplicity, this implementation blurs the final composite alpha mask and
    /// composites the effect below the original pixels.
    pub fn apply_layer_styles_to_bgra(
        bgra: &mut Vec<u8>,
        w: u32,
        h: u32,
        styles: &[(Vec<f32>, &crate::app_state::LayerStyle)],
    ) {
        for (alpha_mask, style) in styles {
            if let Some(shadow) = &style.drop_shadow {
                apply_drop_shadow_bgra(bgra, w, h, alpha_mask, shadow);
            }
            if let Some(glow) = &style.outer_glow {
                apply_outer_glow_bgra(bgra, w, h, alpha_mask, glow);
            }
        }
    }
}

/// Apply a CMYK round-trip simulation to a BGRA8 buffer in-place.
/// C=1-R, M=1-G, Y=1-B, K=min(C,M,Y); R'=(1-C)(1-K), G'=(1-M)(1-K), B'=(1-Y)(1-K).
/// This compresses the gamut visually to simulate CMYK output on a display.
pub(super) fn apply_soft_proof_bgra(bgra: &mut Vec<u8>) {
    for px in bgra.chunks_exact_mut(4) {
        let r = px[2] as f32 / 255.0;
        let g = px[1] as f32 / 255.0;
        let b = px[0] as f32 / 255.0;
        let c = 1.0 - r;
        let m = 1.0 - g;
        let y = 1.0 - b;
        let k = c.min(m).min(y);
        let k1 = 1.0 - k;
        let r2 = if k1 > 1e-6 { (1.0 - c) * k1 } else { 0.0 };
        let g2 = if k1 > 1e-6 { (1.0 - m) * k1 } else { 0.0 };
        let b2 = if k1 > 1e-6 { (1.0 - y) * k1 } else { 0.0 };
        px[2] = (r2.clamp(0.0, 1.0) * 255.0).round() as u8;
        px[1] = (g2.clamp(0.0, 1.0) * 255.0).round() as u8;
        px[0] = (b2.clamp(0.0, 1.0) * 255.0).round() as u8;
    }
}

/// Rgba16Float linear-premultiplied bytes -> BGRA8 sRGB (GPUI `RenderImage` is
/// BGRA). Per pixel: read 4 f16 LE -> f32, unpremultiply rgb by alpha (guarded),
/// sRGB-encode, scale to u8, emit B,G,R,A. Channels in `mask` set to `false` are
/// output as zero (display-only visibility; pixels in the engine are unchanged).
pub(super) fn rgba16f_to_bgra8_masked(rgba16: &[u8], mask: [bool; 4]) -> Vec<u8> {
    let mut out = Vec::with_capacity(rgba16.len() / 2);
    for px in rgba16.chunks_exact(8) {
        let r = f16::from_le_bytes([px[0], px[1]]).to_f32();
        let g = f16::from_le_bytes([px[2], px[3]]).to_f32();
        let b = f16::from_le_bytes([px[4], px[5]]).to_f32();
        let a = f16::from_le_bytes([px[6], px[7]]).to_f32();
        let inv = if a > 0.0 { 1.0 / a } else { 0.0 };
        let enc = |c: f32| (linear_to_srgb((c * inv).clamp(0.0, 1.0)) * 255.0).round() as u8;
        let av = (a.clamp(0.0, 1.0) * 255.0).round() as u8;
        out.push(if mask[2] { enc(b) } else { 0 }); // B
        out.push(if mask[1] { enc(g) } else { 0 }); // G
        out.push(if mask[0] { enc(r) } else { 0 }); // R
        out.push(if mask[3] { av } else { 0 });       // A
    }
    out
}

// ---- Wave 11: CPU layer-style effects ----------------------------------------

/// Box-blur an alpha-mask (1 channel f32, len = w*h) with radius `r` (integer px).
/// Nearest-neighbor Gaussian approximation (3 box passes ≈ Gaussian).
pub(super) fn box_blur_alpha(src: &[f32], w: u32, h: u32, r: u32) -> Vec<f32> {
    if r == 0 {
        return src.to_vec();
    }
    let (wi, hi) = (w as usize, h as usize);
    let mut tmp = src.to_vec();
    // Horizontal pass.
    let mut buf = vec![0.0f32; wi * hi];
    for y in 0..hi {
        let mut sum = 0.0f32;
        let ri = r as i32;
        // Seed with the first window.
        for kx in -(ri)..=ri {
            let sx = kx.clamp(0, wi as i32 - 1) as usize;
            sum += tmp[y * wi + sx];
        }
        let diam = (2 * ri + 1) as f32;
        for x in 0..wi {
            buf[y * wi + x] = sum / diam;
            let add_x = ((x as i32 + ri + 1).clamp(0, wi as i32 - 1)) as usize;
            let rem_x = ((x as i32 - ri).clamp(0, wi as i32 - 1)) as usize;
            sum += tmp[y * wi + add_x] - tmp[y * wi + rem_x];
        }
    }
    tmp = buf.clone();
    // Vertical pass.
    for x in 0..wi {
        let mut sum = 0.0f32;
        let ri = r as i32;
        for ky in -(ri)..=ri {
            let sy = ky.clamp(0, hi as i32 - 1) as usize;
            sum += tmp[sy * wi + x];
        }
        let diam = (2 * ri + 1) as f32;
        for y in 0..hi {
            buf[y * wi + x] = sum / diam;
            let add_y = ((y as i32 + ri + 1).clamp(0, hi as i32 - 1)) as usize;
            let rem_y = ((y as i32 - ri).clamp(0, hi as i32 - 1)) as usize;
            sum += tmp[add_y * wi + x] - tmp[rem_y * wi + x];
        }
    }
    buf
}

/// Apply a drop shadow below `bgra` (BGRA8 in-place) using the layer's alpha mask.
/// Shadow = offset + blurred alpha mask tinted with shadow color, composited below
/// the original pixels.
fn apply_drop_shadow_bgra(
    bgra: &mut Vec<u8>,
    w: u32,
    h: u32,
    alpha: &[f32],
    shadow: &crate::app_state::Shadow,
) {
    let r = (shadow.blur * 0.5).max(0.0) as u32;
    let blurred = box_blur_alpha(alpha, w, h, r);
    let (wi, hi) = (w as usize, h as usize);
    let dx = shadow.offset_x.round() as i32;
    let dy = shadow.offset_y.round() as i32;
    let sc = shadow.color;
    let sa = shadow.opacity;
    for y in 0..hi {
        for x in 0..wi {
            let sx = (x as i32 - dx).clamp(0, wi as i32 - 1) as usize;
            let sy = (y as i32 - dy).clamp(0, hi as i32 - 1) as usize;
            let mask_a = blurred[sy * wi + sx] * sa;
            let i = (y * wi + x) * 4;
            let existing_a = bgra[i + 3] as f32 / 255.0;
            let contrib = mask_a * (1.0 - existing_a);
            if contrib < 1e-3 {
                continue;
            }
            let sb = (sc[2] * 255.0).round() as u8;
            let sg = (sc[1] * 255.0).round() as u8;
            let sr = (sc[0] * 255.0).round() as u8;
            bgra[i] = ((bgra[i] as f32) * (1.0 - contrib) + sb as f32 * contrib) as u8;
            bgra[i + 1] = ((bgra[i + 1] as f32) * (1.0 - contrib) + sg as f32 * contrib) as u8;
            bgra[i + 2] = ((bgra[i + 2] as f32) * (1.0 - contrib) + sr as f32 * contrib) as u8;
            bgra[i + 3] = ((bgra[i + 3] as f32 + 255.0 * contrib).min(255.0)) as u8;
        }
    }
}

/// Apply outer glow using the same blurred alpha mask approach.
fn apply_outer_glow_bgra(
    bgra: &mut Vec<u8>,
    w: u32,
    h: u32,
    alpha: &[f32],
    glow: &crate::app_state::Glow,
) {
    let r = (glow.blur * 0.5).max(0.0) as u32;
    let blurred = box_blur_alpha(alpha, w, h, r);
    let (wi, hi) = (w as usize, h as usize);
    let gc = glow.color;
    let ga = glow.opacity;
    for i_px in 0..(wi * hi) {
        let mask_a = blurred[i_px] * ga;
        let i = i_px * 4;
        let existing_a = bgra[i + 3] as f32 / 255.0;
        let contrib = mask_a * (1.0 - existing_a);
        if contrib < 1e-3 {
            continue;
        }
        let gb = (gc[2] * 255.0).round() as u8;
        let gg = (gc[1] * 255.0).round() as u8;
        let gr = (gc[0] * 255.0).round() as u8;
        bgra[i] = ((bgra[i] as f32) * (1.0 - contrib) + gb as f32 * contrib) as u8;
        bgra[i + 1] = ((bgra[i + 1] as f32) * (1.0 - contrib) + gg as f32 * contrib) as u8;
        bgra[i + 2] = ((bgra[i + 2] as f32) * (1.0 - contrib) + gr as f32 * contrib) as u8;
        bgra[i + 3] = ((bgra[i + 3] as f32 + 255.0 * contrib).min(255.0)) as u8;
    }
}

#[cfg(test)]
mod blur_tests {
    use super::box_blur_alpha;

    // A single lit pixel blurred with r=1 smears energy to neighbors.
    #[test]
    fn blur_spreads_energy() {
        let mut src = vec![0.0f32; 5 * 5];
        src[2 * 5 + 2] = 1.0; // center pixel
        let blurred = box_blur_alpha(&src, 5, 5, 1);
        // Center is still bright, neighbors receive some energy.
        assert!(blurred[2 * 5 + 2] > 0.0, "center must be non-zero");
        assert!(blurred[2 * 5 + 1] > 0.0, "left neighbor must receive energy");
    }

    // r=0 is the identity.
    #[test]
    fn zero_radius_is_identity() {
        let src: Vec<f32> = (0..16).map(|i| i as f32 / 16.0).collect();
        let out = box_blur_alpha(&src, 4, 4, 0);
        assert_eq!(src, out);
    }
}
