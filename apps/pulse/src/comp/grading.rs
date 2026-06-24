//! Per-layer **colour grading** types extracted from `comp/mod.rs` (workspace size
//! rule): the [`TextAnimator`], [`LumetriColor`] (Premiere's Lumetri panel), and
//! [`ColorFinesse`] (Master + 6 tonal ranges) grades, plus the private HSL↔RGB
//! helpers `ColorFinesse` grades through. Behaviour is unchanged — these are the
//! exact items `mod.rs` used to define inline, re-exported from `comp` so existing
//! `crate::comp::{TextAnimator, LumetriColor, ColorFinesse, ColorFinesseRange}`
//! paths keep resolving.

use serde::{Deserialize, Serialize};

/// Per-character text animator (After Effects' Text Animator). When attached to
/// a text layer, applies per-character position/rotation/scale/opacity offsets
/// to the fraction of characters in the selector range.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TextAnimator {
    /// Fraction of text where the animator effect starts (0.0–1.0).
    pub range_start: f32,
    /// Fraction of text where the animator effect ends (0.0–1.0).
    pub range_end: f32,
    /// Per-character x offset in comp px.
    pub offset_x: f32,
    /// Per-character y offset in comp px.
    pub offset_y: f32,
    /// Per-character rotation in degrees.
    pub rotation_deg: f32,
    /// Per-character scale multiplier (1.0 = no change).
    pub scale: f32,
    /// Per-character opacity multiplier (1.0 = no change).
    pub opacity: f32,
}

impl Default for TextAnimator {
    fn default() -> Self {
        TextAnimator {
            range_start: 0.0,
            range_end: 1.0,
            offset_x: 0.0,
            offset_y: 0.0,
            rotation_deg: 0.0,
            scale: 1.0,
            opacity: 1.0,
        }
    }
}

impl TextAnimator {
    /// Whether character `i` of `total` characters is within the selector range.
    pub fn char_in_range(&self, i: usize, total: usize) -> bool {
        if total == 0 {
            return false;
        }
        let t = i as f32 / total as f32;
        t >= self.range_start.min(self.range_end) && t < self.range_start.max(self.range_end)
    }
}

/// Lumetri Color: a per-layer color grade with basic (exposure/contrast/tonal)
/// and creative (temperature/saturation) sections, matching Premiere's Lumetri
/// Color panel (and After Effects' Lumetri Color effect).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LumetriColor {
    // Basic section
    /// Exposure adjustment in EV stops (-5.0 to +5.0, default 0.0).
    pub exposure: f32,
    /// Contrast adjustment (-100 to +100, default 0.0).
    pub contrast: f32,
    /// Highlights recovery/boost (-100 to +100, default 0.0).
    pub highlights: f32,
    /// Shadows lift/crush (-100 to +100, default 0.0).
    pub shadows: f32,
    /// Whites clip point (-100 to +100, default 0.0).
    pub whites: f32,
    /// Blacks clip point (-100 to +100, default 0.0).
    pub blacks: f32,
    // Creative section
    /// Color temperature shift (-100 warm → +100 cool, default 0.0).
    pub temperature: f32,
    /// Tint shift green → magenta (-100 to +100, default 0.0).
    pub tint: f32,
    /// Saturation multiplier as a percentage (default 100.0 = unchanged).
    pub saturation: f32,
    /// Vibrance (boosts muted colors more than vivid, -100 to +100, default 0.0).
    pub vibrance: f32,
    /// Whether this grade is applied. `false` = bypass.
    pub enabled: bool,
}

impl Default for LumetriColor {
    fn default() -> Self {
        LumetriColor {
            exposure: 0.0,
            contrast: 0.0,
            highlights: 0.0,
            shadows: 0.0,
            whites: 0.0,
            blacks: 0.0,
            temperature: 0.0,
            tint: 0.0,
            saturation: 100.0,
            vibrance: 0.0,
            enabled: true,
        }
    }
}

// ── Color Finesse ────────────────────────────────────────────────────────────

/// Per-tonal-range Hue/Saturation/Lightness values for [`ColorFinesse`].
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ColorFinesseRange {
    /// Hue rotation in degrees, -180 to +180. Default 0.0.
    pub hue_shift: f32,
    /// Saturation delta, -100 to +100. Default 0.0.
    pub saturation: f32,
    /// Lightness delta, -100 to +100. Default 0.0.
    pub lightness: f32,
}

/// Per-layer Color Finesse grade: Master + 6 tonal ranges (Reds, Yellows,
/// Greens, Cyans, Blues, Magentas). Applied after the per-pixel effect stack.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ColorFinesse {
    pub master: ColorFinesseRange,
    pub reds: ColorFinesseRange,
    pub yellows: ColorFinesseRange,
    pub greens: ColorFinesseRange,
    pub cyans: ColorFinesseRange,
    pub blues: ColorFinesseRange,
    pub magentas: ColorFinesseRange,
    /// Master on/off switch. Default `true`.
    pub enabled: bool,
}

impl Default for ColorFinesse {
    fn default() -> Self {
        Self {
            master: ColorFinesseRange::default(),
            reds: ColorFinesseRange::default(),
            yellows: ColorFinesseRange::default(),
            greens: ColorFinesseRange::default(),
            cyans: ColorFinesseRange::default(),
            blues: ColorFinesseRange::default(),
            magentas: ColorFinesseRange::default(),
            enabled: true,
        }
    }
}

impl LumetriColor {
    /// Apply this grade to a straight sRGB pixel `[r, g, b, a]` and return the
    /// graded pixel. A disabled grade returns the pixel unchanged.
    pub fn apply(&self, pixel: [f32; 4]) -> [f32; 4] {
        if !self.enabled {
            return pixel;
        }
        let [r, g, b, a] = pixel;

        // --- Exposure ---
        let exp = 2.0_f32.powf(self.exposure);
        let (r, g, b) = (r * exp, g * exp, b * exp);

        // --- Contrast: S-curve around 0.5 midpoint ---
        let contrast_scale = 1.0 + self.contrast / 100.0;
        let s_curve = |v: f32| ((v - 0.5) * contrast_scale + 0.5).clamp(0.0, 1.0);
        let (r, g, b) = (s_curve(r), s_curve(g), s_curve(b));

        // --- Highlights / Shadows / Whites / Blacks (range masking) ---
        let hi_scale = self.highlights / 100.0;
        let sh_scale = self.shadows / 100.0;
        let wh_scale = self.whites / 100.0;
        let bl_scale = self.blacks / 100.0;
        let range_adj = |v: f32| -> f32 {
            let hi_mask = ((v - 0.5) * 2.0).clamp(0.0, 1.0);
            let sh_mask = ((0.5 - v) * 2.0).clamp(0.0, 1.0);
            (v + hi_scale * hi_mask * 0.5 + sh_scale * sh_mask * 0.5
                + wh_scale * hi_mask * 0.25
                + bl_scale * sh_mask * 0.25)
                .clamp(0.0, 1.0)
        };
        let (r, g, b) = (range_adj(r), range_adj(g), range_adj(b));

        // --- Temperature: shift R↑ B↓ (warm) or R↓ B↑ (cool) ---
        let temp = self.temperature / 100.0 * 0.1;
        let (r, g, b) = ((r + temp).clamp(0.0, 1.0), g, (b - temp).clamp(0.0, 1.0));

        // --- Tint: shift G↑ (green) or G↓/R+B↑ (magenta) ---
        let tint = self.tint / 100.0 * 0.1;
        let (r, g, b) = (r, (g + tint).clamp(0.0, 1.0), b);

        // --- Saturation: convert to luminance + chroma, scale chroma ---
        let lum = 0.2126 * r + 0.7152 * g + 0.0722 * b;
        let sat_scale = self.saturation / 100.0;
        let (r, g, b) = (
            (lum + (r - lum) * sat_scale).clamp(0.0, 1.0),
            (lum + (g - lum) * sat_scale).clamp(0.0, 1.0),
            (lum + (b - lum) * sat_scale).clamp(0.0, 1.0),
        );

        // --- Vibrance: boost low-saturation pixels more ---
        let sat = (r - lum).abs().max((g - lum).abs()).max((b - lum).abs());
        let vib = self.vibrance / 100.0 * (1.0 - sat).clamp(0.0, 1.0) * 0.5;
        let (r, g, b) = (
            (lum + (r - lum) * (1.0 + vib)).clamp(0.0, 1.0),
            (lum + (g - lum) * (1.0 + vib)).clamp(0.0, 1.0),
            (lum + (b - lum) * (1.0 + vib)).clamp(0.0, 1.0),
        );

        [r, g, b, a]
    }
}

impl ColorFinesse {
    /// Apply the grade to a straight-sRGB pixel `[r, g, b, a]` and return the
    /// modified pixel. Alpha is passed through unchanged.
    pub fn apply(&self, pixel: [f32; 4]) -> [f32; 4] {
        if !self.enabled {
            return pixel;
        }
        let [r, g, b, a] = pixel;
        let (h, s, l) = rgb_to_hsl(r, g, b);
        let range = self.tonal_range(h);
        let h2 = (h + self.master.hue_shift + range.hue_shift).rem_euclid(360.0);
        let s2 = (s + (self.master.saturation + range.saturation) / 100.0).clamp(0.0, 1.0);
        let l2 = (l + (self.master.lightness + range.lightness) / 100.0).clamp(0.0, 1.0);
        let (r2, g2, b2) = hsl_to_rgb(h2, s2, l2);
        [r2, g2, b2, a]
    }

    fn tonal_range(&self, hue_deg: f32) -> &ColorFinesseRange {
        let h = hue_deg.rem_euclid(360.0);
        if h < 30.0 || h >= 330.0 {
            &self.reds
        } else if h < 90.0 {
            &self.yellows
        } else if h < 150.0 {
            &self.greens
        } else if h < 210.0 {
            &self.cyans
        } else if h < 270.0 {
            &self.blues
        } else {
            &self.magentas
        }
    }
}

fn rgb_to_hsl(r: f32, g: f32, b: f32) -> (f32, f32, f32) {
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let l = (max + min) / 2.0;
    if (max - min).abs() < 1e-6 {
        return (0.0, 0.0, l);
    }
    let d = max - min;
    let s = if l > 0.5 { d / (2.0 - max - min) } else { d / (max + min) };
    let h = if max == r {
        ((g - b) / d + if g < b { 6.0 } else { 0.0 }) * 60.0
    } else if max == g {
        ((b - r) / d + 2.0) * 60.0
    } else {
        ((r - g) / d + 4.0) * 60.0
    };
    (h, s, l)
}

fn hsl_to_rgb(h: f32, s: f32, l: f32) -> (f32, f32, f32) {
    if s < 1e-6 {
        return (l, l, l);
    }
    let q = if l < 0.5 { l * (1.0 + s) } else { l + s - l * s };
    let p = 2.0 * l - q;
    let r = hue_to_rgb(p, q, h / 360.0 + 1.0 / 3.0);
    let g = hue_to_rgb(p, q, h / 360.0);
    let b = hue_to_rgb(p, q, h / 360.0 - 1.0 / 3.0);
    (r, g, b)
}

fn hue_to_rgb(p: f32, q: f32, mut t: f32) -> f32 {
    if t < 0.0 { t += 1.0; }
    if t > 1.0 { t -= 1.0; }
    if t < 1.0 / 6.0 { return p + (q - p) * 6.0 * t; }
    if t < 1.0 / 2.0 { return q; }
    if t < 2.0 / 3.0 { return p + (q - p) * (2.0 / 3.0 - t) * 6.0; }
    p
}
