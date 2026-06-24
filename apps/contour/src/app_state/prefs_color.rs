//! Colour-picker state and application **Preferences** data model.
//!
//! Two cooperating pieces, both pure data + conversions (no UI):
//!
//! - [`ColorPicker`] — a colour held simultaneously as RGB, HSB, CMYK, and a hex
//!   string, with lossless-as-possible conversions between every representation
//!   (Illustrator's Color picker, which keeps all four fields live). Editing one
//!   model re-derives the others.
//! - [`Preferences`] — undo levels, snapping, grid, and document units, with
//!   JSON load/save (`serde_json`) so settings round-trip to disk.
//!
//! These plug into [`App`](super::App) via the Batch-13 actions; the helpers /
//! dispatch live in `apply_batch13.rs`.

use serde::{Deserialize, Serialize};

// --- Colour picker -----------------------------------------------------------

/// A picker colour kept in RGB (the canonical store) plus on-demand HSB / CMYK /
/// hex views. RGB and alpha are straight-sRGB `0..=1`. The view conversions are
/// derived, so [`ColorPicker::rgba`] is always the source of truth.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct ColorPicker {
    /// Canonical straight-sRGB RGBA in `0..=1`.
    pub rgba: [f32; 4],
}

impl Default for ColorPicker {
    fn default() -> Self {
        Self { rgba: [0.0, 0.0, 0.0, 1.0] }
    }
}

impl ColorPicker {
    pub fn from_rgba(rgba: [f32; 4]) -> Self {
        Self {
            rgba: [
                rgba[0].clamp(0.0, 1.0),
                rgba[1].clamp(0.0, 1.0),
                rgba[2].clamp(0.0, 1.0),
                rgba[3].clamp(0.0, 1.0),
            ],
        }
    }

    /// Set from RGB (alpha preserved unless given).
    pub fn set_rgb(&mut self, r: f32, g: f32, b: f32) {
        self.rgba[0] = r.clamp(0.0, 1.0);
        self.rgba[1] = g.clamp(0.0, 1.0);
        self.rgba[2] = b.clamp(0.0, 1.0);
    }

    /// The colour as HSB: hue in `0..360`, saturation/brightness in `0..1`.
    pub fn hsb(&self) -> (f32, f32, f32) {
        rgb_to_hsb(self.rgba[0], self.rgba[1], self.rgba[2])
    }

    /// Set the colour from HSB (hue degrees, sat/bri 0..1).
    pub fn set_hsb(&mut self, h: f32, s: f32, b: f32) {
        let (r, g, bl) = hsb_to_rgb(h, s.clamp(0.0, 1.0), b.clamp(0.0, 1.0));
        self.set_rgb(r, g, bl);
    }

    /// The colour as CMYK, all channels `0..1`.
    pub fn cmyk(&self) -> (f32, f32, f32, f32) {
        rgb_to_cmyk(self.rgba[0], self.rgba[1], self.rgba[2])
    }

    /// Set the colour from CMYK (all channels 0..1).
    pub fn set_cmyk(&mut self, c: f32, m: f32, y: f32, k: f32) {
        let (r, g, b) = cmyk_to_rgb(
            c.clamp(0.0, 1.0),
            m.clamp(0.0, 1.0),
            y.clamp(0.0, 1.0),
            k.clamp(0.0, 1.0),
        );
        self.set_rgb(r, g, b);
    }

    /// The colour as a `#RRGGBB` hex string (alpha omitted).
    pub fn hex(&self) -> String {
        let to8 = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
        format!(
            "#{:02X}{:02X}{:02X}",
            to8(self.rgba[0]),
            to8(self.rgba[1]),
            to8(self.rgba[2])
        )
    }

    /// Set the colour from a hex string (`#RGB`, `#RRGGBB`, or `#RRGGBBAA`, with
    /// or without the leading `#`). Returns `true` if it parsed.
    pub fn set_hex(&mut self, hex: &str) -> bool {
        if let Some(rgba) = parse_hex(hex) {
            self.rgba = rgba;
            true
        } else {
            false
        }
    }
}

/// Parse a hex colour string into straight-sRGB RGBA `0..=1`. Accepts `#RGB`,
/// `#RRGGBB`, and `#RRGGBBAA` (the `#` is optional). Alpha defaults to 1.
pub fn parse_hex(hex: &str) -> Option<[f32; 4]> {
    let h = hex.trim().trim_start_matches('#');
    let bytes = |s: &str| u8::from_str_radix(s, 16).ok();
    match h.len() {
        3 => {
            // #RGB → expand each nibble.
            let r = u8::from_str_radix(&h[0..1].repeat(2), 16).ok()?;
            let g = u8::from_str_radix(&h[1..2].repeat(2), 16).ok()?;
            let b = u8::from_str_radix(&h[2..3].repeat(2), 16).ok()?;
            Some([r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, 1.0])
        }
        6 => {
            let r = bytes(&h[0..2])?;
            let g = bytes(&h[2..4])?;
            let b = bytes(&h[4..6])?;
            Some([r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, 1.0])
        }
        8 => {
            let r = bytes(&h[0..2])?;
            let g = bytes(&h[2..4])?;
            let b = bytes(&h[4..6])?;
            let a = bytes(&h[6..8])?;
            Some([
                r as f32 / 255.0,
                g as f32 / 255.0,
                b as f32 / 255.0,
                a as f32 / 255.0,
            ])
        }
        _ => None,
    }
}

/// RGB (0..1) → HSB: hue degrees `0..360`, sat/bri `0..1`.
pub fn rgb_to_hsb(r: f32, g: f32, b: f32) -> (f32, f32, f32) {
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let delta = max - min;
    let mut h = if delta < 1e-9 {
        0.0
    } else if max == r {
        60.0 * (((g - b) / delta) % 6.0)
    } else if max == g {
        60.0 * ((b - r) / delta + 2.0)
    } else {
        60.0 * ((r - g) / delta + 4.0)
    };
    if h < 0.0 {
        h += 360.0;
    }
    let s = if max < 1e-9 { 0.0 } else { delta / max };
    (h, s, max)
}

/// HSB (hue deg, sat/bri 0..1) → RGB (0..1).
pub fn hsb_to_rgb(h: f32, s: f32, v: f32) -> (f32, f32, f32) {
    let h = h.rem_euclid(360.0);
    let c = v * s;
    let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
    let m = v - c;
    let (r1, g1, b1) = match (h / 60.0) as i32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    (r1 + m, g1 + m, b1 + m)
}

/// RGB (0..1) → CMYK (all 0..1).
pub fn rgb_to_cmyk(r: f32, g: f32, b: f32) -> (f32, f32, f32, f32) {
    let k = 1.0 - r.max(g).max(b);
    if (1.0 - k).abs() < 1e-9 {
        // Pure black: no colour ink.
        return (0.0, 0.0, 0.0, 1.0);
    }
    let c = (1.0 - r - k) / (1.0 - k);
    let m = (1.0 - g - k) / (1.0 - k);
    let y = (1.0 - b - k) / (1.0 - k);
    (c.clamp(0.0, 1.0), m.clamp(0.0, 1.0), y.clamp(0.0, 1.0), k)
}

/// CMYK (all 0..1) → RGB (0..1).
pub fn cmyk_to_rgb(c: f32, m: f32, y: f32, k: f32) -> (f32, f32, f32) {
    let r = (1.0 - c) * (1.0 - k);
    let g = (1.0 - m) * (1.0 - k);
    let b = (1.0 - y) * (1.0 - k);
    (r, g, b)
}

// --- Preferences -------------------------------------------------------------

/// The ruler / measurement unit the document and prefs work in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum PrefUnit {
    #[default]
    Pixels,
    Points,
    Picas,
    Inches,
    Millimeters,
    Centimeters,
}

impl PrefUnit {
    /// Document points (1/72 inch) per one of this unit, the conversion the ruler
    /// uses. Pixels are treated as 1:1 with points (96-dpi-agnostic baseline).
    pub fn points_per_unit(self) -> f32 {
        match self {
            PrefUnit::Pixels => 1.0,
            PrefUnit::Points => 1.0,
            PrefUnit::Picas => 12.0,
            PrefUnit::Inches => 72.0,
            PrefUnit::Millimeters => 72.0 / 25.4,
            PrefUnit::Centimeters => 72.0 / 2.54,
        }
    }
}

/// Application preferences: undo depth, snapping, grid, and units. Serializes to
/// JSON for on-disk persistence.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Preferences {
    /// Maximum number of undo steps the history keeps.
    pub undo_levels: u32,
    /// Snap dragged geometry to other objects' anchors / edges.
    pub snap_to_point: bool,
    /// Snap dragged geometry to the grid.
    pub snap_to_grid: bool,
    /// Whether the grid is shown.
    pub show_grid: bool,
    /// Grid spacing in document points.
    pub grid_spacing: f32,
    /// Grid subdivisions per major line.
    pub grid_subdivisions: u32,
    /// Snap tolerance in document points.
    pub snap_tolerance: f32,
    /// The display / ruler unit.
    pub unit: PrefUnit,
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            undo_levels: 100,
            snap_to_point: true,
            snap_to_grid: false,
            show_grid: false,
            grid_spacing: 72.0,
            grid_subdivisions: 8,
            snap_tolerance: 4.0,
            unit: PrefUnit::Pixels,
        }
    }
}

impl Preferences {
    /// Serialize to a pretty JSON string for saving to disk.
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_else(|_| "{}".to_string())
    }

    /// Parse preferences from a JSON string, falling back to defaults for any
    /// missing / malformed input. Unknown fields are ignored; a totally invalid
    /// document yields `Self::default()`.
    pub fn from_json(s: &str) -> Self {
        serde_json::from_str(s).unwrap_or_default()
    }

    /// Clamp every field to a sane range (called after a load / edit).
    pub fn sanitize(&mut self) {
        self.undo_levels = self.undo_levels.clamp(1, 1000);
        self.grid_spacing = self.grid_spacing.clamp(1.0, 10000.0);
        self.grid_subdivisions = self.grid_subdivisions.clamp(1, 100);
        self.snap_tolerance = self.snap_tolerance.clamp(0.0, 100.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-3
    }

    #[test]
    fn hex_round_trips() {
        let mut p = ColorPicker::default();
        assert!(p.set_hex("#FF8000"));
        assert!(close(p.rgba[0], 1.0) && close(p.rgba[1], 0.5019608) && close(p.rgba[2], 0.0));
        assert_eq!(p.hex(), "#FF8000");
    }

    #[test]
    fn hex_short_form_and_alpha() {
        assert_eq!(parse_hex("#f00"), Some([1.0, 0.0, 0.0, 1.0]));
        assert_eq!(parse_hex("00FF0080").map(|c| c[3]), Some(128.0 / 255.0));
        assert_eq!(parse_hex("xyz"), None);
    }

    #[test]
    fn rgb_hsb_round_trip() {
        let cases = [
            (1.0, 0.0, 0.0),
            (0.0, 1.0, 0.0),
            (0.0, 0.0, 1.0),
            (0.3, 0.6, 0.9),
            (0.5, 0.5, 0.5),
        ];
        for (r, g, b) in cases {
            let (h, s, v) = rgb_to_hsb(r, g, b);
            let (r2, g2, b2) = hsb_to_rgb(h, s, v);
            assert!(close(r, r2) && close(g, g2) && close(b, b2), "{r},{g},{b} != {r2},{g2},{b2}");
        }
    }

    #[test]
    fn cmyk_rgb_round_trips() {
        // The DOD requirement: CMYK↔RGB round-trips.
        let cases = [
            (0.2, 0.4, 0.6),
            (1.0, 1.0, 1.0),
            (0.0, 0.0, 0.0),
            (0.9, 0.1, 0.5),
        ];
        for (r, g, b) in cases {
            let (c, m, y, k) = rgb_to_cmyk(r, g, b);
            let (r2, g2, b2) = cmyk_to_rgb(c, m, y, k);
            assert!(close(r, r2) && close(g, g2) && close(b, b2), "{r},{g},{b} != {r2},{g2},{b2}");
        }
    }

    #[test]
    fn picker_views_consistent() {
        let mut p = ColorPicker::from_rgba([0.25, 0.5, 0.75, 1.0]);
        // HSB view re-derives the same RGB.
        let (h, s, v) = p.hsb();
        p.set_hsb(h, s, v);
        assert!(close(p.rgba[0], 0.25) && close(p.rgba[1], 0.5) && close(p.rgba[2], 0.75));
        // CMYK view re-derives the same RGB.
        let (c, m, y, k) = p.cmyk();
        p.set_cmyk(c, m, y, k);
        assert!(close(p.rgba[0], 0.25) && close(p.rgba[1], 0.5) && close(p.rgba[2], 0.75));
    }

    #[test]
    fn pure_black_cmyk_is_k_only() {
        let (c, m, y, k) = rgb_to_cmyk(0.0, 0.0, 0.0);
        assert_eq!((c, m, y, k), (0.0, 0.0, 0.0, 1.0));
    }

    #[test]
    fn prefs_json_round_trip() {
        let mut p = Preferences::default();
        p.undo_levels = 42;
        p.snap_to_grid = true;
        p.grid_spacing = 36.0;
        p.unit = PrefUnit::Millimeters;
        let json = p.to_json();
        let back = Preferences::from_json(&json);
        assert_eq!(p, back);
    }

    #[test]
    fn prefs_from_garbage_is_default() {
        assert_eq!(Preferences::from_json("not json"), Preferences::default());
    }

    #[test]
    fn prefs_sanitize_clamps() {
        let mut p = Preferences {
            undo_levels: 99999,
            grid_spacing: -5.0,
            grid_subdivisions: 0,
            snap_tolerance: 500.0,
            ..Default::default()
        };
        p.sanitize();
        assert_eq!(p.undo_levels, 1000);
        assert_eq!(p.grid_spacing, 1.0);
        assert_eq!(p.grid_subdivisions, 1);
        assert_eq!(p.snap_tolerance, 100.0);
    }

    #[test]
    fn unit_conversions() {
        assert!(close(PrefUnit::Inches.points_per_unit(), 72.0));
        assert!(close(PrefUnit::Picas.points_per_unit(), 12.0));
        assert!(close(PrefUnit::Millimeters.points_per_unit(), 2.8346457));
    }
}
