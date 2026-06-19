//! GPUI-host-side post-processing effects: Mosaic, Chromatic Aberration,
//! Vignette, Noise/Grain, Color Balance, Levels, Hue/Saturation, Fractal Noise.
//!
//! These are applied AFTER the CPU compositor renders a frame, operating
//! directly on the RGBA8 pixel buffer. They live here (not in pulse-app) so
//! they are additive-only and do not touch the engine.

/// A per-layer GPUI-side post-process effect (stored in `App`).
#[derive(Clone, Debug, PartialEq)]
pub enum GpuiEffect {
    /// Pixelate into square blocks of `block` × `block` px.
    Mosaic { block: u32 },
    /// Shift the R channel right by `offset` px and B channel left.
    ChromaticAberration { offset: i32 },
    /// Radial darkening from corners: `intensity` (0–1), `radius` (0.5–1).
    Vignette { intensity: f32, radius: f32 },
    /// Add deterministic random noise (seed = frame_idx).
    Noise { intensity: f32 },
    /// Color Balance: per-channel by luminosity zone.
    ColorBalance {
        shadows_r: f32, shadows_g: f32, shadows_b: f32,
        midtones_r: f32, midtones_g: f32, midtones_b: f32,
        highlights_r: f32, highlights_g: f32, highlights_b: f32,
    },
    /// Levels: remap pixel values.
    Levels {
        in_black: u8, in_white: u8, gamma: f32, out_black: u8, out_white: u8,
    },
    /// Hue/Saturation shift.
    HueSaturation { hue_shift: f32, saturation: f32, lightness: f32 },
    /// Fractal Noise overlay.
    FractalNoise { frequency: f32, octaves: u8, evolution: f32 },
}

impl GpuiEffect {
    pub fn label(&self) -> &'static str {
        match self {
            GpuiEffect::Mosaic { .. } => "Mosaic",
            GpuiEffect::ChromaticAberration { .. } => "Chromatic Aberration",
            GpuiEffect::Vignette { .. } => "Vignette",
            GpuiEffect::Noise { .. } => "Noise / Grain",
            GpuiEffect::ColorBalance { .. } => "Color Balance",
            GpuiEffect::Levels { .. } => "Levels",
            GpuiEffect::HueSaturation { .. } => "Hue/Saturation",
            GpuiEffect::FractalNoise { .. } => "Fractal Noise",
        }
    }

    /// Apply `self` in-place to an RGBA8 `pixels` buffer of `width × height`.
    /// `frame_idx` is used as the noise seed so grain is temporally stable.
    pub fn apply(&self, pixels: &mut [u8], width: u32, height: u32, frame_idx: u32) {
        match self {
            GpuiEffect::Mosaic { block } => apply_mosaic(pixels, width, height, (*block).max(1)),
            GpuiEffect::ChromaticAberration { offset } => {
                apply_chroma(pixels, width, height, *offset)
            }
            GpuiEffect::Vignette { intensity, radius } => {
                apply_vignette(pixels, width, height, *intensity, *radius)
            }
            GpuiEffect::Noise { intensity } => apply_noise(pixels, width, height, frame_idx, *intensity),
            GpuiEffect::ColorBalance {
                shadows_r, shadows_g, shadows_b,
                midtones_r, midtones_g, midtones_b,
                highlights_r, highlights_g, highlights_b,
            } => {
                apply_color_balance(pixels, *shadows_r, *shadows_g, *shadows_b, *midtones_r, *midtones_g, *midtones_b, *highlights_r, *highlights_g, *highlights_b);
            }
            GpuiEffect::Levels { in_black, in_white, gamma, out_black, out_white } => {
                apply_levels(pixels, *in_black, *in_white, *gamma, *out_black, *out_white);
            }
            GpuiEffect::HueSaturation { hue_shift, saturation, lightness } => {
                apply_hue_saturation(pixels, *hue_shift, *saturation, *lightness);
            }
            GpuiEffect::FractalNoise { frequency, octaves, evolution } => {
                apply_fractal_noise(pixels, width, height, frame_idx, *frequency, *octaves, *evolution);
            }
        }
    }

    pub fn default_for_kind(kind: GpuiEffectKind) -> Self {
        match kind {
            GpuiEffectKind::Mosaic => GpuiEffect::Mosaic { block: 8 },
            GpuiEffectKind::ChromaticAberration => GpuiEffect::ChromaticAberration { offset: 4 },
            GpuiEffectKind::Vignette => GpuiEffect::Vignette { intensity: 0.5, radius: 0.75 },
            GpuiEffectKind::Noise => GpuiEffect::Noise { intensity: 0.1 },
            GpuiEffectKind::ColorBalance => GpuiEffect::ColorBalance {
                shadows_r: 0.0, shadows_g: 0.0, shadows_b: 0.0,
                midtones_r: 0.0, midtones_g: 0.0, midtones_b: 0.0,
                highlights_r: 0.0, highlights_g: 0.0, highlights_b: 0.0,
            },
            GpuiEffectKind::Levels => GpuiEffect::Levels {
                in_black: 0, in_white: 255, gamma: 1.0, out_black: 0, out_white: 255,
            },
            GpuiEffectKind::HueSaturation => GpuiEffect::HueSaturation {
                hue_shift: 0.0, saturation: 1.0, lightness: 0.0,
            },
            GpuiEffectKind::FractalNoise => GpuiEffect::FractalNoise {
                frequency: 0.1, octaves: 4, evolution: 0.0,
            },
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GpuiEffectKind {
    Mosaic,
    ChromaticAberration,
    Vignette,
    Noise,
    ColorBalance,
    Levels,
    HueSaturation,
    FractalNoise,
}

/// Average each block of pixels and fill with that average.
fn apply_mosaic(pixels: &mut [u8], width: u32, height: u32, block: u32) {
    let w = width as usize;
    let h = height as usize;
    let b = block as usize;
    let mut y = 0usize;
    while y < h {
        let y_end = (y + b).min(h);
        let mut x = 0usize;
        while x < w {
            let x_end = (x + b).min(w);
            let (mut sr, mut sg, mut sb, mut sa, mut count) = (0u32, 0u32, 0u32, 0u32, 0u32);
            for row in y..y_end {
                for col in x..x_end {
                    let idx = (row * w + col) * 4;
                    sr += pixels[idx] as u32;
                    sg += pixels[idx + 1] as u32;
                    sb += pixels[idx + 2] as u32;
                    sa += pixels[idx + 3] as u32;
                    count += 1;
                }
            }
            if count > 0 {
                let (r, g, b_avg, a) = (
                    (sr / count) as u8,
                    (sg / count) as u8,
                    (sb / count) as u8,
                    (sa / count) as u8,
                );
                for row in y..y_end {
                    for col in x..x_end {
                        let idx = (row * w + col) * 4;
                        pixels[idx] = r;
                        pixels[idx + 1] = g;
                        pixels[idx + 2] = b_avg;
                        pixels[idx + 3] = a;
                    }
                }
            }
            x += b;
        }
        y += b;
    }
}

fn apply_chroma(pixels: &mut [u8], width: u32, height: u32, offset: i32) {
    if offset == 0 { return; }
    let w = width as usize;
    let h = height as usize;
    let off = offset as isize;
    let orig = pixels.to_vec();
    for row in 0..h {
        for col in 0..w {
            let out_idx = (row * w + col) * 4;
            let r_col = (col as isize - off).clamp(0, w as isize - 1) as usize;
            let b_col = (col as isize + off).clamp(0, w as isize - 1) as usize;
            let r_src = (row * w + r_col) * 4;
            let b_src = (row * w + b_col) * 4;
            pixels[out_idx] = orig[r_src];
            pixels[out_idx + 2] = orig[b_src + 2];
        }
    }
}

fn apply_vignette(pixels: &mut [u8], width: u32, height: u32, intensity: f32, radius: f32) {
    let w = width as f32;
    let h = height as f32;
    let cx = w * 0.5;
    let cy = h * 0.5;
    let max_dist = (cx * cx + cy * cy).sqrt();
    let inner = max_dist * radius;
    for row in 0..height as usize {
        for col in 0..width as usize {
            let dx = col as f32 - cx;
            let dy = row as f32 - cy;
            let dist = (dx * dx + dy * dy).sqrt();
            let t = ((dist - inner) / (max_dist - inner + 1e-6)).clamp(0.0, 1.0);
            let dark = 1.0 - t * intensity;
            let idx = (row * width as usize + col) * 4;
            pixels[idx] = (pixels[idx] as f32 * dark) as u8;
            pixels[idx + 1] = (pixels[idx + 1] as f32 * dark) as u8;
            pixels[idx + 2] = (pixels[idx + 2] as f32 * dark) as u8;
        }
    }
}

fn apply_noise(pixels: &mut [u8], width: u32, height: u32, frame_idx: u32, intensity: f32) {
    let n = width as usize * height as usize;
    let mut rng = frame_idx.wrapping_mul(1664525).wrapping_add(1013904223);
    for i in 0..n {
        rng = rng.wrapping_mul(1664525).wrapping_add(1013904223);
        let noise = (rng as f32 / u32::MAX as f32) * 2.0 - 1.0;
        let delta = (noise * intensity * 255.0) as i16;
        let idx = i * 4;
        pixels[idx] = (pixels[idx] as i16 + delta).clamp(0, 255) as u8;
        pixels[idx + 1] = (pixels[idx + 1] as i16 + delta).clamp(0, 255) as u8;
        pixels[idx + 2] = (pixels[idx + 2] as i16 + delta).clamp(0, 255) as u8;
    }
}

#[allow(clippy::too_many_arguments)]
fn apply_color_balance(
    pixels: &mut [u8],
    shadows_r: f32, shadows_g: f32, shadows_b: f32,
    midtones_r: f32, midtones_g: f32, midtones_b: f32,
    highlights_r: f32, highlights_g: f32, highlights_b: f32,
) {
    for px in pixels.chunks_exact_mut(4) {
        let lum = (px[0] as f32 * 0.299 + px[1] as f32 * 0.587 + px[2] as f32 * 0.114) / 255.0;
        let s = (1.0 - lum * 3.0).clamp(0.0, 1.0);
        let m = (1.0 - (lum * 3.0 - 1.0).abs()).clamp(0.0, 1.0);
        let h = (lum * 3.0 - 2.0).clamp(0.0, 1.0);
        let dr = s * shadows_r + m * midtones_r + h * highlights_r;
        let dg = s * shadows_g + m * midtones_g + h * highlights_g;
        let db = s * shadows_b + m * midtones_b + h * highlights_b;
        px[0] = (px[0] as f32 + dr * 128.0).clamp(0.0, 255.0) as u8;
        px[1] = (px[1] as f32 + dg * 128.0).clamp(0.0, 255.0) as u8;
        px[2] = (px[2] as f32 + db * 128.0).clamp(0.0, 255.0) as u8;
    }
}

fn apply_levels(pixels: &mut [u8], in_black: u8, in_white: u8, gamma: f32, out_black: u8, out_white: u8) {
    let ib = in_black as f32;
    let iw = (in_white as f32).max(ib + 1.0);
    let ob = out_black as f32;
    let ow = out_white as f32;
    let g = gamma.max(0.01);
    for px in pixels.chunks_exact_mut(4) {
        for c in 0..3 {
            let v = px[c] as f32;
            let normalized = ((v - ib) / (iw - ib)).clamp(0.0, 1.0);
            let gamma_corrected = normalized.powf(1.0 / g);
            let out = ob + gamma_corrected * (ow - ob);
            px[c] = out.clamp(0.0, 255.0) as u8;
        }
    }
}

fn apply_hue_saturation(pixels: &mut [u8], hue_shift: f32, saturation: f32, lightness: f32) {
    for px in pixels.chunks_exact_mut(4) {
        let r = px[0] as f32 / 255.0;
        let g = px[1] as f32 / 255.0;
        let b = px[2] as f32 / 255.0;
        let (h, s, l) = rgb_to_hsl(r, g, b);
        let new_h = (h + hue_shift / 360.0).rem_euclid(1.0);
        let new_s = (s * saturation).clamp(0.0, 1.0);
        let new_l = (l + lightness).clamp(0.0, 1.0);
        let (nr, ng, nb) = hsl_to_rgb(new_h, new_s, new_l);
        px[0] = (nr * 255.0).clamp(0.0, 255.0) as u8;
        px[1] = (ng * 255.0).clamp(0.0, 255.0) as u8;
        px[2] = (nb * 255.0).clamp(0.0, 255.0) as u8;
    }
}

fn apply_fractal_noise(pixels: &mut [u8], width: u32, height: u32, frame_idx: u32, frequency: f32, octaves: u8, evolution: f32) {
    let w = width as f32;
    let h = height as f32;
    let seed = frame_idx as f32 * evolution;
    for row in 0..height as usize {
        for col in 0..width as usize {
            let ux = col as f32 / w;
            let uy = row as f32 / h;
            let mut noise = 0.0f32;
            let mut amp = 1.0f32;
            let mut freq = frequency;
            let mut total_amp = 0.0f32;
            for _ in 0..octaves.min(8) {
                noise += value_noise(ux * freq + seed, uy * freq) * amp;
                total_amp += amp;
                amp *= 0.5;
                freq *= 2.0;
            }
            if total_amp > 0.0 { noise /= total_amp; }
            let v = (noise * 255.0) as i16;
            let idx = (row * width as usize + col) * 4;
            pixels[idx] = (pixels[idx] as i16 / 2 + v / 2).clamp(0, 255) as u8;
            pixels[idx + 1] = (pixels[idx + 1] as i16 / 2 + v / 2).clamp(0, 255) as u8;
            pixels[idx + 2] = (pixels[idx + 2] as i16 / 2 + v / 2).clamp(0, 255) as u8;
        }
    }
}

fn rgb_to_hsl(r: f32, g: f32, b: f32) -> (f32, f32, f32) {
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let l = (max + min) * 0.5;
    if (max - min).abs() < 1e-6 { return (0.0, 0.0, l); }
    let d = max - min;
    let s = if l > 0.5 { d / (2.0 - max - min) } else { d / (max + min) };
    let h = if max == r {
        (g - b) / d + if g < b { 6.0 } else { 0.0 }
    } else if max == g {
        (b - r) / d + 2.0
    } else {
        (r - g) / d + 4.0
    };
    (h / 6.0, s, l)
}

fn hsl_to_rgb(h: f32, s: f32, l: f32) -> (f32, f32, f32) {
    if s < 1e-6 { return (l, l, l); }
    let q = if l < 0.5 { l * (1.0 + s) } else { l + s - l * s };
    let p = 2.0 * l - q;
    (hue_to_rgb(p, q, h + 1.0 / 3.0), hue_to_rgb(p, q, h), hue_to_rgb(p, q, h - 1.0 / 3.0))
}

fn hue_to_rgb(p: f32, q: f32, mut t: f32) -> f32 {
    if t < 0.0 { t += 1.0; }
    if t > 1.0 { t -= 1.0; }
    if t < 1.0 / 6.0 { return p + (q - p) * 6.0 * t; }
    if t < 1.0 / 2.0 { return q; }
    if t < 2.0 / 3.0 { return p + (q - p) * (2.0 / 3.0 - t) * 6.0; }
    p
}

fn value_noise(x: f32, y: f32) -> f32 {
    let xi = x.floor() as i32;
    let yi = y.floor() as i32;
    let hash = |a: i32, b: i32| -> f32 {
        let n = a.wrapping_mul(1619).wrapping_add(b.wrapping_mul(31337)).wrapping_mul(1013904223);
        (n as u32 as f32) / (u32::MAX as f32)
    };
    let fx = x - x.floor();
    let fy = y - y.floor();
    let ux = fx * fx * (3.0 - 2.0 * fx);
    let uy = fy * fy * (3.0 - 2.0 * fy);
    let a = hash(xi, yi);
    let b = hash(xi + 1, yi);
    let c = hash(xi, yi + 1);
    let d = hash(xi + 1, yi + 1);
    a + (b - a) * ux + (c - a) * uy + (a - b - c + d) * ux * uy
}
