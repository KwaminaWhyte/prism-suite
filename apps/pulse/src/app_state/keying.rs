//! **Keying suite** — Chroma Key, Luma Key, Color Key, plus Keylight-style
//! **spill suppression**, implemented as deterministic CPU kernels over RGBA
//! buffers.
//!
//! Each keyer turns a *key color* (or a luma band) + *tolerance* + *softness*
//! into a per-pixel alpha:
//!
//! - **Chroma Key** keys on chrominance (hue/saturation distance with luma
//!   removed), so it ignores brightness — the classic greenscreen keyer.
//! - **Color Key** keys on straight RGB Euclidean distance to the key color.
//! - **Luma Key** keys on a pixel's luminance against a target band.
//! - **Spill suppression** desaturates the residual key-color cast (e.g. a green
//!   fringe) toward neutral by clamping the key channel to the mean of the other
//!   two, scaled by an `amount`.
//!
//! The math lives in free functions so it can be unit-tested without an `App`;
//! the `App` impl holds per-layer [`KeyConfig`] and the panel actions. Storage is
//! app-side (these do not touch the engine's effect stacks) to keep the engine
//! crate unchanged — the same convention as `effects_chain.rs` / `audio_mixer.rs`.

use std::collections::HashMap;

use super::{App, Action};

/// Which keyer a layer's [`KeyConfig`] drives.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyKind {
    /// Key on chrominance distance to the key color (greenscreen).
    Chroma,
    /// Key on RGB Euclidean distance to the key color.
    Color,
    /// Key on luminance against a target band.
    Luma,
}

impl KeyKind {
    /// Human-readable name for the panel.
    pub fn label(self) -> &'static str {
        match self {
            KeyKind::Chroma => "Chroma Key",
            KeyKind::Color => "Color Key",
            KeyKind::Luma => "Luma Key",
        }
    }
}

/// Per-layer keying configuration (Keylight-style controls).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct KeyConfig {
    /// Which keyer is active.
    pub kind: KeyKind,
    /// The key color (linear RGB, `[0,1]`). For Luma keys this is unused; the
    /// luma band comes from `luma_target`.
    pub key_color: [f32; 3],
    /// Inside this distance the pixel is fully keyed (alpha 0).
    pub tolerance: f32,
    /// The ramp width beyond `tolerance` over which alpha rises 0→1 (the soft
    /// edge). `0` is a hard key.
    pub softness: f32,
    /// Target luminance for the Luma keyer (`[0,1]`).
    pub luma_target: f32,
    /// Spill-suppression amount (`[0,1]`); `0` disables spill removal.
    pub spill: f32,
}

impl Default for KeyConfig {
    fn default() -> Self {
        Self {
            kind: KeyKind::Chroma,
            key_color: [0.0, 1.0, 0.0], // greenscreen
            tolerance: 0.15,
            softness: 0.10,
            luma_target: 0.5,
            spill: 0.0,
        }
    }
}

impl App {
    /// Apply a keying [`Action`]. Dispatched from [`App::apply`].
    pub(super) fn apply_keying(&mut self, action: Action) {
        match action {
            Action::AddKeyer { layer_id, kind } => {
                self.keyers
                    .insert(layer_id, KeyConfig { kind, ..KeyConfig::default() });
                self.host.mark_dirty();
            }
            Action::RemoveKeyer { layer_id } => {
                self.keyers.remove(&layer_id);
                self.host.mark_dirty();
            }
            Action::SetKeyKind { layer_id, kind } => {
                self.keyers.entry(layer_id).or_default().kind = kind;
                self.host.mark_dirty();
            }
            Action::SetKeyColor { layer_id, color } => {
                self.keyers.entry(layer_id).or_default().key_color = [
                    color[0].clamp(0.0, 1.0),
                    color[1].clamp(0.0, 1.0),
                    color[2].clamp(0.0, 1.0),
                ];
                self.host.mark_dirty();
            }
            Action::SetKeyParam { layer_id, param, value } => {
                let cfg = self.keyers.entry(layer_id).or_default();
                match param {
                    "tolerance" => cfg.tolerance = value.max(0.0),
                    "softness" => cfg.softness = value.max(0.0),
                    "luma_target" => cfg.luma_target = value.clamp(0.0, 1.0),
                    "spill" => cfg.spill = value.clamp(0.0, 1.0),
                    _ => {}
                }
                self.host.mark_dirty();
            }
            _ => unreachable!("apply_keying called with wrong action"),
        }
    }

    /// Bake the `layer_id` keyer over a straight-RGBA `[0,1]` buffer (row-major),
    /// returning a new buffer with the computed alpha and (if enabled)
    /// spill-suppressed RGB. `None` if the layer has no keyer configured.
    /// Deterministic.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn apply_keyer(&self, layer_id: usize, rgba: &[f32]) -> Option<Vec<f32>> {
        let cfg = self.keyers.get(&layer_id)?;
        Some(key_buffer(*cfg, rgba))
    }
}

// ── Color helpers ─────────────────────────────────────────────────────────────

/// Rec.709 luminance of a linear RGB triple.
pub fn luma(rgb: [f32; 3]) -> f32 {
    0.2126 * rgb[0] + 0.7152 * rgb[1] + 0.0722 * rgb[2]
}

/// Project a color onto its **chrominance** by subtracting its luminance — what's
/// left is the hue/saturation the chroma keyer compares, with brightness
/// removed. Returns the `(r-l, g-l, b-l)` offset triple.
fn chroma_vec(rgb: [f32; 3]) -> [f32; 3] {
    let l = luma(rgb);
    [rgb[0] - l, rgb[1] - l, rgb[2] - l]
}

/// Euclidean distance between two RGB triples.
fn rgb_dist(a: [f32; 3], b: [f32; 3]) -> f32 {
    let (dr, dg, db) = (a[0] - b[0], a[1] - b[1], a[2] - b[2]);
    (dr * dr + dg * dg + db * db).sqrt()
}

/// Map a raw `distance` through a key's `tolerance`/`softness` ramp into an
/// **alpha** in `[0,1]`:
/// - `distance <= tolerance` → `0` (fully keyed / transparent),
/// - `distance >= tolerance + softness` → `1` (fully opaque),
/// - in between → a smooth `smoothstep` ramp.
pub fn alpha_from_distance(distance: f32, tolerance: f32, softness: f32) -> f32 {
    if distance <= tolerance {
        return 0.0;
    }
    if softness <= 0.0 {
        return 1.0;
    }
    let t = ((distance - tolerance) / softness).clamp(0.0, 1.0);
    // smoothstep for a C¹ edge.
    t * t * (3.0 - 2.0 * t)
}

/// **Chroma-key** alpha for one pixel: chrominance distance (brightness-free) to
/// the key color, ramped through tolerance/softness. A pixel matching the key
/// hue → 0; a far hue → 1.
pub fn chroma_key_alpha(rgb: [f32; 3], cfg: KeyConfig) -> f32 {
    let d = rgb_dist(chroma_vec(rgb), chroma_vec(cfg.key_color));
    alpha_from_distance(d, cfg.tolerance, cfg.softness)
}

/// **Color-key** alpha: straight RGB distance to the key color.
pub fn color_key_alpha(rgb: [f32; 3], cfg: KeyConfig) -> f32 {
    let d = rgb_dist(rgb, cfg.key_color);
    alpha_from_distance(d, cfg.tolerance, cfg.softness)
}

/// **Luma-key** alpha: distance of the pixel's luminance from the target band.
pub fn luma_key_alpha(rgb: [f32; 3], cfg: KeyConfig) -> f32 {
    let d = (luma(rgb) - cfg.luma_target).abs();
    alpha_from_distance(d, cfg.tolerance, cfg.softness)
}

/// Dispatch to the right keyer for `cfg.kind`.
pub fn key_alpha(rgb: [f32; 3], cfg: KeyConfig) -> f32 {
    match cfg.kind {
        KeyKind::Chroma => chroma_key_alpha(rgb, cfg),
        KeyKind::Color => color_key_alpha(rgb, cfg),
        KeyKind::Luma => luma_key_alpha(rgb, cfg),
    }
}

/// **Spill suppression**: desaturate the dominant key channel toward the mean of
/// the other two, scaled by `amount` (`[0,1]`). For a green key, a pixel with a
/// green fringe has its green pulled down toward `(r+b)/2`, neutralising the cast
/// without touching genuinely green-free pixels. Returns the corrected RGB.
pub fn suppress_spill(rgb: [f32; 3], key_color: [f32; 3], amount: f32) -> [f32; 3] {
    if amount <= 0.0 {
        return rgb;
    }
    let amount = amount.clamp(0.0, 1.0);
    // The key channel is the largest component of the key color.
    let key_ch = if key_color[1] >= key_color[0] && key_color[1] >= key_color[2] {
        1
    } else if key_color[0] >= key_color[2] {
        0
    } else {
        2
    };
    let others = match key_ch {
        0 => (rgb[1], rgb[2]),
        1 => (rgb[0], rgb[2]),
        _ => (rgb[0], rgb[1]),
    };
    let neutral = (others.0 + others.1) * 0.5;
    let mut out = rgb;
    if out[key_ch] > neutral {
        // Only pull *down* an excess of the key channel (a spill), never boost.
        out[key_ch] += (neutral - out[key_ch]) * amount;
    }
    out
}

/// Bake a whole straight-RGBA `[0,1]` buffer through a keyer: replace each
/// pixel's alpha with the computed key alpha and apply spill suppression to its
/// RGB. Returns a new buffer; the input is unchanged. Deterministic for a fixed
/// config + input.
pub fn key_buffer(cfg: KeyConfig, rgba: &[f32]) -> Vec<f32> {
    let mut out = rgba.to_vec();
    for px in out.chunks_exact_mut(4) {
        let rgb = [px[0], px[1], px[2]];
        let a = key_alpha(rgb, cfg);
        let corrected = suppress_spill(rgb, cfg.key_color, cfg.spill);
        px[0] = corrected[0];
        px[1] = corrected[1];
        px[2] = corrected[2];
        // Combine with any existing alpha so stacked keys intersect.
        px[3] = (px[3] * a).clamp(0.0, 1.0);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const GREEN: [f32; 3] = [0.0, 1.0, 0.0];

    #[test]
    fn chroma_key_at_key_color_is_zero_alpha() {
        // A pixel matching the key color is fully keyed (alpha 0).
        let cfg = KeyConfig { kind: KeyKind::Chroma, key_color: GREEN, ..Default::default() };
        assert!(chroma_key_alpha(GREEN, cfg) < 1e-4, "key color ⇒ alpha 0");
    }

    #[test]
    fn chroma_key_far_color_is_full_alpha() {
        // A pure red pixel is far from a green key ⇒ alpha 1.
        let cfg = KeyConfig { kind: KeyKind::Chroma, key_color: GREEN, ..Default::default() };
        let a = chroma_key_alpha([1.0, 0.0, 0.0], cfg);
        assert!((a - 1.0).abs() < 1e-4, "far color ⇒ alpha 1, got {a}");
    }

    #[test]
    fn chroma_key_midtone_is_partial() {
        // A color sitting inside the soft ramp keys partially.
        let cfg = KeyConfig {
            kind: KeyKind::Chroma,
            key_color: GREEN,
            tolerance: 0.1,
            softness: 0.6,
            ..Default::default()
        };
        // A slightly-green-tinted gray: small but nonzero chroma distance.
        let a = chroma_key_alpha([0.4, 0.5, 0.4], cfg);
        assert!(a > 0.0 && a < 1.0, "midtone keys partially, got {a}");
    }

    #[test]
    fn chroma_key_ignores_brightness() {
        // A dim green still keys because chroma is brightness-independent.
        let cfg = KeyConfig { kind: KeyKind::Chroma, key_color: GREEN, ..Default::default() };
        let dark = chroma_key_alpha([0.0, 0.3, 0.0], cfg);
        assert!(dark < 0.3, "dim green still keys, got {dark}");
    }

    #[test]
    fn color_key_at_key_color_is_zero() {
        let cfg = KeyConfig { kind: KeyKind::Color, key_color: [0.2, 0.6, 0.9], ..Default::default() };
        assert!(color_key_alpha([0.2, 0.6, 0.9], cfg) < 1e-4);
    }

    #[test]
    fn color_key_far_is_one() {
        let cfg = KeyConfig { kind: KeyKind::Color, key_color: [0.0, 0.0, 0.0], ..Default::default() };
        assert!((color_key_alpha([1.0, 1.0, 1.0], cfg) - 1.0).abs() < 1e-4);
    }

    #[test]
    fn luma_key_band() {
        let cfg = KeyConfig {
            kind: KeyKind::Luma,
            luma_target: 0.0,
            tolerance: 0.05,
            softness: 0.1,
            ..Default::default()
        };
        // Black (luma 0) is keyed; white (luma 1) is opaque.
        assert!(luma_key_alpha([0.0, 0.0, 0.0], cfg) < 1e-4, "black keyed");
        assert!((luma_key_alpha([1.0, 1.0, 1.0], cfg) - 1.0).abs() < 1e-4, "white opaque");
    }

    #[test]
    fn alpha_ramp_monotonic() {
        // The ramp is non-decreasing from tolerance to tolerance+softness.
        let (tol, soft) = (0.2, 0.4);
        let mut last = -1.0;
        for i in 0..=20 {
            let d = i as f32 / 20.0;
            let a = alpha_from_distance(d, tol, soft);
            assert!(a >= last - 1e-6, "alpha ramp is monotonic");
            last = a;
        }
        assert_eq!(alpha_from_distance(0.0, tol, soft), 0.0);
        assert!((alpha_from_distance(1.0, tol, soft) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn spill_suppression_neutralises_green_fringe() {
        // A pixel with excess green has its green pulled toward (r+b)/2.
        let before = [0.3, 0.8, 0.3];
        let after = suppress_spill(before, GREEN, 1.0);
        assert!(after[1] < before[1], "green is suppressed");
        assert!((after[1] - 0.3).abs() < 1e-4, "green clamped to mean of r,b");
        // Red and blue untouched.
        assert_eq!(after[0], before[0]);
        assert_eq!(after[2], before[2]);
    }

    #[test]
    fn spill_suppression_zero_amount_is_identity() {
        let c = [0.3, 0.8, 0.3];
        assert_eq!(suppress_spill(c, GREEN, 0.0), c);
    }

    #[test]
    fn spill_does_not_boost_non_spill_pixel() {
        // A pixel whose green is already below the neutral isn't raised.
        let c = [0.8, 0.2, 0.8];
        let after = suppress_spill(c, GREEN, 1.0);
        assert_eq!(after, c, "no green excess ⇒ unchanged");
    }

    #[test]
    fn key_buffer_writes_alpha_and_suppresses() {
        // Two-pixel buffer: one green (keyed), one red (kept).
        let cfg = KeyConfig { kind: KeyKind::Chroma, key_color: GREEN, spill: 1.0, ..Default::default() };
        let rgba = vec![
            0.0, 1.0, 0.0, 1.0, // green
            1.0, 0.0, 0.0, 1.0, // red
        ];
        let out = key_buffer(cfg, &rgba);
        assert!(out[3] < 1e-3, "green pixel keyed to alpha 0");
        assert!((out[7] - 1.0).abs() < 1e-3, "red pixel stays opaque");
    }

    #[test]
    fn add_remove_keyer_action() {
        let mut app = App::new();
        app.apply(Action::AddKeyer { layer_id: 0, kind: KeyKind::Chroma });
        assert!(app.keyers.contains_key(&0));
        app.apply(Action::SetKeyKind { layer_id: 0, kind: KeyKind::Luma });
        assert_eq!(app.keyers[&0].kind, KeyKind::Luma);
        app.apply(Action::SetKeyParam { layer_id: 0, param: "tolerance", value: -1.0 });
        assert!(app.keyers[&0].tolerance >= 0.0, "tolerance clamps to >= 0");
        app.apply(Action::SetKeyParam { layer_id: 0, param: "spill", value: 5.0 });
        assert!((app.keyers[&0].spill - 1.0).abs() < 1e-4, "spill clamps to 1");
        app.apply(Action::RemoveKeyer { layer_id: 0 });
        assert!(!app.keyers.contains_key(&0));
    }

    #[test]
    fn apply_keyer_through_app() {
        let mut app = App::new();
        assert!(app.apply_keyer(0, &[0.0; 4]).is_none());
        app.apply(Action::AddKeyer { layer_id: 0, kind: KeyKind::Color });
        app.apply(Action::SetKeyColor { layer_id: 0, color: [0.0, 0.0, 0.0] });
        let out = app.apply_keyer(0, &[0.0, 0.0, 0.0, 1.0]).expect("buffer");
        assert!(out[3] < 1e-3, "black pixel keyed against black key");
    }
}

/// Type alias used by the `App` field declaration in `mod.rs`.
pub type KeyerMap = HashMap<usize, KeyConfig>;
