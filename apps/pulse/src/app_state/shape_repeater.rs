//! **Shape-layer Repeater + Trim Paths animation** — deterministic CPU models
//! for two staple shape-layer operators.
//!
//! - **Repeater** clones a shape `copies` times, applying a cumulative transform
//!   (position offset, rotation, scale, opacity ramp) to each copy — exactly
//!   like After Effects' Repeater. [`repeater_transforms`] returns the per-copy
//!   transform list so a renderer can stamp the shape N times.
//! - **Trim Paths** reveals a fraction of a path between `start` and `end` (with
//!   an `offset`), and both can animate over time via simple keyframe pairs.
//!   [`trim_at_time`] interpolates the trim span; [`trimmed_polyline`] applies
//!   it to a sampled path, returning the visible sub-path.
//!
//! Geometry lives in free functions (unit-testable without an `App`); the `App`
//! impl holds per-layer configs + panel actions. App-side storage, engine
//! untouched — matching `keying.rs` / `effects_distort.rs`.

use std::collections::HashMap;

use super::{App, Action};

// ── Repeater ──────────────────────────────────────────────────────────────────

/// Per-layer **Repeater** config.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RepeaterConfig {
    /// Number of copies (≥1; copy 0 is the original).
    pub copies: u32,
    /// Per-copy position offset (px).
    pub offset: [f32; 2],
    /// Per-copy rotation increment (degrees).
    pub rotation_deg: f32,
    /// Per-copy uniform scale multiplier (compounded per copy).
    pub scale: f32,
    /// Opacity of the first copy.
    pub start_opacity: f32,
    /// Opacity of the last copy (linearly ramped across copies).
    pub end_opacity: f32,
}

impl Default for RepeaterConfig {
    fn default() -> Self {
        Self {
            copies: 3,
            offset: [40.0, 0.0],
            rotation_deg: 0.0,
            scale: 1.0,
            start_opacity: 1.0,
            end_opacity: 1.0,
        }
    }
}

/// A single repeater copy's resolved transform.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CopyTransform {
    /// Cumulative translation (px) from the original.
    pub position: [f32; 2],
    /// Cumulative rotation (degrees).
    pub rotation_deg: f32,
    /// Cumulative scale multiplier.
    pub scale: f32,
    /// Copy opacity (`[0,1]`).
    pub opacity: f32,
}

/// Compute the per-copy transform list for a repeater. Copy `i` accumulates `i`
/// applications of the offset / rotation / scale, and its opacity is the linear
/// ramp from `start_opacity` (copy 0) to `end_opacity` (last copy). Returns one
/// [`CopyTransform`] per copy, length `copies.max(1)`.
pub fn repeater_transforms(cfg: RepeaterConfig) -> Vec<CopyTransform> {
    let n = cfg.copies.max(1);
    let mut out = Vec::with_capacity(n as usize);
    for i in 0..n {
        let fi = i as f32;
        let t = if n > 1 { fi / (n - 1) as f32 } else { 0.0 };
        out.push(CopyTransform {
            position: [cfg.offset[0] * fi, cfg.offset[1] * fi],
            rotation_deg: cfg.rotation_deg * fi,
            scale: cfg.scale.powi(i as i32),
            opacity: (cfg.start_opacity + (cfg.end_opacity - cfg.start_opacity) * t)
                .clamp(0.0, 1.0),
        });
    }
    out
}

// ── Trim Paths (animated) ─────────────────────────────────────────────────────

/// A single trim keyframe: `(time_s, start, end, offset)` — each span fraction in
/// `[0,1]` (offset may exceed `1`, it wraps).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TrimKey {
    pub time_s: f32,
    pub start: f32,
    pub end: f32,
    pub offset: f32,
}

/// Per-layer **Trim Paths** config with optional animation. When `keys` is empty
/// the static `start`/`end`/`offset` apply at all times; otherwise the keys are
/// linearly interpolated by time.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct TrimPathsConfig {
    /// Static trim start fraction `[0,1]` (used when `keys` is empty).
    pub start: f32,
    /// Static trim end fraction `[0,1]`.
    pub end: f32,
    /// Static offset fraction (wraps).
    pub offset: f32,
    /// Time-ordered animation keys. Empty ⇒ static.
    pub keys: Vec<TrimKey>,
}

/// The resolved `(start, end, offset)` of a trim at `time_s`. With no keys, the
/// static values; with keys, linear interpolation (clamped to the end keys).
pub fn trim_at_time(cfg: &TrimPathsConfig, time_s: f32) -> (f32, f32, f32) {
    if cfg.keys.is_empty() {
        return (cfg.start, cfg.end, cfg.offset);
    }
    if cfg.keys.len() == 1 || time_s <= cfg.keys[0].time_s {
        let k = cfg.keys[0];
        return (k.start, k.end, k.offset);
    }
    let last = cfg.keys[cfg.keys.len() - 1];
    if time_s >= last.time_s {
        return (last.start, last.end, last.offset);
    }
    // Find the bracketing keys.
    for w in cfg.keys.windows(2) {
        let (a, b) = (w[0], w[1]);
        if time_s >= a.time_s && time_s <= b.time_s {
            let span = (b.time_s - a.time_s).max(1e-6);
            let t = ((time_s - a.time_s) / span).clamp(0.0, 1.0);
            let lerp = |x: f32, y: f32| x + (y - x) * t;
            return (lerp(a.start, b.start), lerp(a.end, b.end), lerp(a.offset, b.offset));
        }
    }
    (last.start, last.end, last.offset)
}

/// Apply a trim `(start, end, offset)` to a sampled closed/open polyline (a list
/// of points, treated as a uniform parameterization over `[0,1]`). Returns the
/// sub-path of points whose normalized arc-position lies inside the trimmed span
/// `[start+offset, end+offset]` (wrapping at 1.0). A full `start=0, end=1` trim
/// returns the whole path.
pub fn trimmed_polyline(points: &[[f32; 2]], start: f32, end: f32, offset: f32) -> Vec<[f32; 2]> {
    if points.len() < 2 {
        return points.to_vec();
    }
    let s = (start + offset).rem_euclid(1.0);
    let e = (end + offset).rem_euclid(1.0);
    let n = points.len();
    let mut out = Vec::new();
    let wraps = s > e;
    for (i, &p) in points.iter().enumerate() {
        let u = i as f32 / (n - 1) as f32;
        let inside = if (end - start).abs() >= 1.0 - 1e-6 {
            true // full path
        } else if !wraps {
            u >= s - 1e-6 && u <= e + 1e-6
        } else {
            u >= s - 1e-6 || u <= e + 1e-6
        };
        if inside {
            out.push(p);
        }
    }
    out
}

impl App {
    /// Apply a repeater / trim-paths [`Action`]. Dispatched from [`App::apply`].
    pub(super) fn apply_shape_repeater(&mut self, action: Action) {
        match action {
            Action::AddRepeater { layer_id } => {
                self.repeaters.insert(layer_id, RepeaterConfig::default());
                self.host.mark_dirty();
            }
            Action::RemoveRepeater { layer_id } => {
                self.repeaters.remove(&layer_id);
                self.host.mark_dirty();
            }
            Action::SetRepeaterCount { layer_id, copies } => {
                self.repeaters.entry(layer_id).or_default().copies = copies.clamp(1, 1000);
                self.host.mark_dirty();
            }
            Action::SetRepeaterTransform { layer_id, offset_x, offset_y, rotation_deg, scale } => {
                let cfg = self.repeaters.entry(layer_id).or_default();
                cfg.offset = [offset_x, offset_y];
                cfg.rotation_deg = rotation_deg;
                cfg.scale = scale.max(0.0);
                self.host.mark_dirty();
            }
            Action::SetRepeaterOpacityRamp { layer_id, start, end } => {
                let cfg = self.repeaters.entry(layer_id).or_default();
                cfg.start_opacity = start.clamp(0.0, 1.0);
                cfg.end_opacity = end.clamp(0.0, 1.0);
                self.host.mark_dirty();
            }
            Action::SetTrimPaths { layer_id, start, end, offset } => {
                let cfg = self.trim_paths.entry(layer_id).or_default();
                cfg.start = start.clamp(0.0, 1.0);
                cfg.end = end.clamp(0.0, 1.0);
                cfg.offset = offset;
                self.host.mark_dirty();
            }
            Action::AddTrimKey { layer_id, time_s, start, end, offset } => {
                let cfg = self.trim_paths.entry(layer_id).or_default();
                cfg.keys.push(TrimKey {
                    time_s,
                    start: start.clamp(0.0, 1.0),
                    end: end.clamp(0.0, 1.0),
                    offset,
                });
                cfg.keys.sort_by(|a, b| a.time_s.partial_cmp(&b.time_s).unwrap_or(std::cmp::Ordering::Equal));
                self.host.mark_dirty();
            }
            Action::ClearTrimPaths { layer_id } => {
                self.trim_paths.remove(&layer_id);
                self.host.mark_dirty();
            }
            _ => unreachable!("apply_shape_repeater called with wrong action"),
        }
    }

    /// The per-copy transforms for a layer's repeater (if any).
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn repeater_copies(&self, layer_id: usize) -> Option<Vec<CopyTransform>> {
        self.repeaters.get(&layer_id).map(|c| repeater_transforms(*c))
    }

    /// The trim `(start, end, offset)` for a layer at `time_s` (if configured).
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn trim_at(&self, layer_id: usize, time_s: f32) -> Option<(f32, f32, f32)> {
        self.trim_paths.get(&layer_id).map(|c| trim_at_time(c, time_s))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repeater_makes_n_copies() {
        let cfg = RepeaterConfig { copies: 5, ..Default::default() };
        let t = repeater_transforms(cfg);
        assert_eq!(t.len(), 5, "repeater yields N copies");
    }

    #[test]
    fn repeater_copy_zero_is_original() {
        let cfg = RepeaterConfig {
            copies: 4,
            offset: [10.0, 5.0],
            rotation_deg: 15.0,
            scale: 0.9,
            ..Default::default()
        };
        let t = repeater_transforms(cfg);
        assert_eq!(t[0].position, [0.0, 0.0], "copy 0 is unmoved");
        assert_eq!(t[0].rotation_deg, 0.0);
        assert!((t[0].scale - 1.0).abs() < 1e-6);
    }

    #[test]
    fn repeater_accumulates_transform() {
        let cfg = RepeaterConfig {
            copies: 3,
            offset: [10.0, 0.0],
            rotation_deg: 20.0,
            scale: 0.5,
            ..Default::default()
        };
        let t = repeater_transforms(cfg);
        // Copy 2: 2x offset, 2x rotation, scale^2.
        assert_eq!(t[2].position, [20.0, 0.0]);
        assert!((t[2].rotation_deg - 40.0).abs() < 1e-5);
        assert!((t[2].scale - 0.25).abs() < 1e-6);
    }

    #[test]
    fn repeater_opacity_ramp() {
        let cfg = RepeaterConfig { copies: 3, start_opacity: 1.0, end_opacity: 0.0, ..Default::default() };
        let t = repeater_transforms(cfg);
        assert!((t[0].opacity - 1.0).abs() < 1e-6, "first copy opaque");
        assert!((t[1].opacity - 0.5).abs() < 1e-6, "middle halfway");
        assert!((t[2].opacity - 0.0).abs() < 1e-6, "last copy clear");
    }

    #[test]
    fn repeater_single_copy_is_safe() {
        let cfg = RepeaterConfig { copies: 1, ..Default::default() };
        let t = repeater_transforms(cfg);
        assert_eq!(t.len(), 1);
        assert!((t[0].opacity - 1.0).abs() < 1e-6);
    }

    #[test]
    fn trim_static_returns_static() {
        let cfg = TrimPathsConfig { start: 0.2, end: 0.8, offset: 0.1, keys: vec![] };
        assert_eq!(trim_at_time(&cfg, 5.0), (0.2, 0.8, 0.1));
    }

    #[test]
    fn trim_animates_between_keys() {
        let cfg = TrimPathsConfig {
            keys: vec![
                TrimKey { time_s: 0.0, start: 0.0, end: 0.0, offset: 0.0 },
                TrimKey { time_s: 2.0, start: 0.0, end: 1.0, offset: 0.0 },
            ],
            ..Default::default()
        };
        // Halfway through, end should be ~0.5 (a wipe-on reveal).
        let (s, e, _o) = trim_at_time(&cfg, 1.0);
        assert!((s - 0.0).abs() < 1e-5);
        assert!((e - 0.5).abs() < 1e-5, "end interpolates to 0.5, got {e}");
    }

    #[test]
    fn trim_clamps_outside_key_range() {
        let cfg = TrimPathsConfig {
            keys: vec![
                TrimKey { time_s: 1.0, start: 0.1, end: 0.4, offset: 0.0 },
                TrimKey { time_s: 3.0, start: 0.6, end: 0.9, offset: 0.0 },
            ],
            ..Default::default()
        };
        assert_eq!(trim_at_time(&cfg, -1.0), (0.1, 0.4, 0.0), "before first key clamps");
        assert_eq!(trim_at_time(&cfg, 10.0), (0.6, 0.9, 0.0), "after last key clamps");
    }

    #[test]
    fn trimmed_polyline_full_path() {
        let pts: Vec<[f32; 2]> = (0..=10).map(|i| [i as f32, 0.0]).collect();
        let out = trimmed_polyline(&pts, 0.0, 1.0, 0.0);
        assert_eq!(out.len(), pts.len(), "full trim keeps all points");
    }

    #[test]
    fn trimmed_polyline_half() {
        let pts: Vec<[f32; 2]> = (0..=10).map(|i| [i as f32, 0.0]).collect();
        let out = trimmed_polyline(&pts, 0.0, 0.5, 0.0);
        // Points with u in [0, 0.5] → indices 0..=5.
        assert!(out.len() >= 5 && out.len() <= 7, "about half the points, got {}", out.len());
        assert_eq!(out[0], [0.0, 0.0]);
    }

    #[test]
    fn trimmed_polyline_wraps_with_offset() {
        let pts: Vec<[f32; 2]> = (0..=10).map(|i| [i as f32, 0.0]).collect();
        // start 0.8, end 0.2 with offset wraps around the seam.
        let out = trimmed_polyline(&pts, 0.8, 1.2, 0.0);
        assert!(!out.is_empty(), "wrapped span still yields points");
    }

    #[test]
    fn repeater_actions_roundtrip() {
        let mut app = App::new();
        app.apply(Action::AddRepeater { layer_id: 0 });
        assert!(app.repeaters.contains_key(&0));
        app.apply(Action::SetRepeaterCount { layer_id: 0, copies: 10000 });
        assert_eq!(app.repeaters[&0].copies, 1000, "copies clamp to 1000");
        app.apply(Action::SetRepeaterTransform { layer_id: 0, offset_x: 5.0, offset_y: 6.0, rotation_deg: 30.0, scale: -1.0 });
        assert_eq!(app.repeaters[&0].offset, [5.0, 6.0]);
        assert_eq!(app.repeaters[&0].scale, 0.0, "scale clamps to >= 0");
        let copies = app.repeater_copies(0).expect("copies");
        assert_eq!(copies.len(), 1000);
        app.apply(Action::RemoveRepeater { layer_id: 0 });
        assert!(!app.repeaters.contains_key(&0));
    }

    #[test]
    fn trim_actions_roundtrip() {
        let mut app = App::new();
        app.apply(Action::SetTrimPaths { layer_id: 1, start: -0.5, end: 2.0, offset: 0.3 });
        let cfg = &app.trim_paths[&1];
        assert_eq!(cfg.start, 0.0, "start clamps to >= 0");
        assert_eq!(cfg.end, 1.0, "end clamps to <= 1");
        app.apply(Action::AddTrimKey { layer_id: 1, time_s: 2.0, start: 0.0, end: 1.0, offset: 0.0 });
        app.apply(Action::AddTrimKey { layer_id: 1, time_s: 0.0, start: 0.0, end: 0.0, offset: 0.0 });
        // Keys are kept time-sorted.
        assert!(app.trim_paths[&1].keys[0].time_s <= app.trim_paths[&1].keys[1].time_s);
        let (_s, e, _o) = app.trim_at(1, 1.0).expect("trim");
        assert!((e - 0.5).abs() < 1e-5, "animated trim resolves, got {e}");
        app.apply(Action::ClearTrimPaths { layer_id: 1 });
        assert!(!app.trim_paths.contains_key(&1));
    }
}

/// Type aliases used by the `App` field declarations in `mod.rs`.
pub type RepeaterMap = HashMap<usize, RepeaterConfig>;
pub type TrimPathsMap = HashMap<usize, TrimPathsConfig>;
