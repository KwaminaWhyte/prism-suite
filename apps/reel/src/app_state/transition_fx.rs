//! Phase 3 — clip **transitions** (pure per-frame math) + **time-remap / speed**.
//!
//! This is the second half of Phase 3, sitting alongside the geometry suite in
//! [`super::transitions`] and the timeline-attached [`super::timeline::Transition`]
//! record. Where those describe *how* two pre-sampled frames are warped, this
//! module owns a small, fully-deterministic **transition model** whose single
//! entry point is a pure `fn evaluate(progress: f32) -> TransitionState`, plus a
//! pure **time-remap / speed** model (`TimeRemap`) mapping a clip's local timeline
//! time to a source time.
//!
//! Nothing here samples pixels — `evaluate` returns the two clip opacities, an
//! optional dip colour, and a geometry descriptor (wipe edge + softness, slide
//! offsets, or iris radius) that the compositor consumes. Every function is pure
//! and `#[cfg(test)]`-covered so the math can be asserted at `progress`
//! `0 / 0.5 / 1` without a GPU.
//!
//! App integration is the usual single-choke-point pattern: a `transition_fx:
//! Vec<TransitionFx>` list on [`App`] plus per-clip speed / frame-blend / remap
//! keys, all mutated only through [`AppTransitionFxExt::apply_transition_fx`].

use super::{App, Action, DIP_BLACK, DIP_WHITE, WipeDir};

// --- Clamp bounds ------------------------------------------------------------

/// Smallest allowed absolute playback speed factor (avoids a frozen/zero divide).
pub const MIN_SPEED: f32 = 0.01;
/// Largest allowed absolute playback speed factor.
pub const MAX_SPEED: f32 = 100.0;
/// Minimum transition duration in frames.
pub const MIN_TRANSITION_FRAMES: u32 = 1;

// --- Small numeric helpers ---------------------------------------------------

/// Clamp to `[0, 1]`, mapping non-finite input to `0`.
#[inline]
fn clamp01(v: f32) -> f32 {
    if v.is_finite() { v.clamp(0.0, 1.0) } else { 0.0 }
}

// --- Wipe shape --------------------------------------------------------------

/// How a wipe's revealed region is shaped.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WipeShape {
    /// A straight edge sweeping in `dir`.
    Linear(WipeDir),
    /// A circle growing from the centre.
    Radial,
    /// A clock hand sweeping a full turn around the centre.
    Clock,
}

impl WipeShape {
    pub fn label(self) -> &'static str {
        match self {
            WipeShape::Linear(_) => "Linear",
            WipeShape::Radial => "Radial",
            WipeShape::Clock => "Clock",
        }
    }
}

// --- Transition alignment ----------------------------------------------------

/// Where a transition sits relative to the cut (or clip head/tail) it is anchored
/// to. Drives the mapping from a frame offset to `progress`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransitionAlign {
    /// Centred on the cut — half the duration before, half after.
    Centered,
    /// Begins at the cut / edge — the whole duration lies after the anchor.
    Start,
    /// Ends at the cut / edge — the whole duration lies before the anchor.
    End,
}

impl TransitionAlign {
    pub fn label(self) -> &'static str {
        match self {
            TransitionAlign::Centered => "Centered",
            TransitionAlign::Start => "Start at Cut",
            TransitionAlign::End => "End at Cut",
        }
    }

    /// The transition's frame span relative to the anchor frame: `(start, end)`
    /// offsets (in frames) for a transition of `dur` frames.
    pub fn span(self, dur: f32) -> (f32, f32) {
        let d = dur.max(0.0);
        match self {
            TransitionAlign::Centered => (-d * 0.5, d * 0.5),
            TransitionAlign::Start => (0.0, d),
            TransitionAlign::End => (-d, 0.0),
        }
    }
}

// --- Transition kind ---------------------------------------------------------

/// The kind of a Phase-3 clip transition. Each variant maps `progress ∈ [0,1]`
/// to a [`TransitionState`] via [`TransitionFxKind::evaluate`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TransitionFxKind {
    /// Linear opacity crossfade A→B (outgoing fades out as incoming fades in).
    CrossDissolve,
    /// Fade the outgoing out to black, then fade the incoming in from black.
    DipToBlack,
    /// Fade the outgoing out to white, then fade the incoming in from white.
    DipToWhite,
    /// A wipe with the given `shape` and an antialiased `softness` band (0..1).
    Wipe { shape: WipeShape, softness: f32 },
    /// The incoming clip slides over the (stationary) outgoing clip from `dir`.
    Slide(WipeDir),
    /// The incoming clip pushes the outgoing off-screen toward `dir`.
    Push(WipeDir),
    /// A circular iris reveal growing from the centre.
    Iris,
}

impl TransitionFxKind {
    pub fn label(&self) -> &'static str {
        match self {
            TransitionFxKind::CrossDissolve => "Cross Dissolve",
            TransitionFxKind::DipToBlack => "Dip to Black",
            TransitionFxKind::DipToWhite => "Dip to White",
            TransitionFxKind::Wipe { .. } => "Wipe",
            TransitionFxKind::Slide(_) => "Slide",
            TransitionFxKind::Push(_) => "Push",
            TransitionFxKind::Iris => "Iris",
        }
    }

    /// Evaluate this transition at `progress ∈ [0,1]` (clamped). Pure: returns the
    /// outgoing/incoming opacities, an optional dip colour + strength, and the
    /// geometry descriptor the compositor uses to mask / offset the two frames.
    pub fn evaluate(&self, progress: f32) -> TransitionState {
        let p = clamp01(progress);
        match *self {
            TransitionFxKind::CrossDissolve => TransitionState {
                out_opacity: 1.0 - p,
                in_opacity: p,
                dip: None,
                geometry: TransitionGeometry::None,
            },
            TransitionFxKind::DipToBlack => dip_state(p, DIP_BLACK),
            TransitionFxKind::DipToWhite => dip_state(p, DIP_WHITE),
            TransitionFxKind::Wipe { shape, softness } => TransitionState {
                // Both frames stay fully opaque; the wipe edge masks the seam.
                out_opacity: 1.0,
                in_opacity: 1.0,
                dip: None,
                geometry: TransitionGeometry::Wipe { shape, edge: p, softness: clamp01(softness) },
            },
            TransitionFxKind::Slide(dir) => {
                let in_offset = slide_in_offset(dir, p);
                TransitionState {
                    out_opacity: 1.0,
                    in_opacity: 1.0,
                    dip: None,
                    geometry: TransitionGeometry::Slide { in_offset, out_offset: (0.0, 0.0) },
                }
            }
            TransitionFxKind::Push(dir) => {
                let (in_offset, out_offset) = push_offsets(dir, p);
                TransitionState {
                    out_opacity: 1.0,
                    in_opacity: 1.0,
                    dip: None,
                    geometry: TransitionGeometry::Slide { in_offset, out_offset },
                }
            }
            TransitionFxKind::Iris => TransitionState {
                out_opacity: 1.0,
                in_opacity: 1.0,
                dip: None,
                geometry: TransitionGeometry::Iris { radius: p },
            },
        }
    }
}

/// Out→colour→in dip. First half (`p < 0.5`) fades the outgoing out to `color`;
/// second half fades the incoming in from `color`. The dip strength peaks at 1.0
/// at `p = 0.5`, so the frame is the full dip colour at the midpoint.
fn dip_state(p: f32, color: [f32; 4]) -> TransitionState {
    if p < 0.5 {
        TransitionState {
            out_opacity: 1.0 - p * 2.0,
            in_opacity: 0.0,
            dip: Some((color, p * 2.0)),
            geometry: TransitionGeometry::None,
        }
    } else {
        TransitionState {
            out_opacity: 0.0,
            in_opacity: (p - 0.5) * 2.0,
            dip: Some((color, (1.0 - p) * 2.0)),
            geometry: TransitionGeometry::None,
        }
    }
}

/// Normalized offset (fractions of the frame size) of the incoming clip for a
/// **slide**: fully off-screen on the `dir` edge at `p=0`, centred at `p=1`.
fn slide_in_offset(dir: WipeDir, p: f32) -> (f32, f32) {
    let r = 1.0 - p; // remaining off-screen fraction
    match dir {
        WipeDir::Left => (-r, 0.0),
        WipeDir::Right => (r, 0.0),
        WipeDir::Up => (0.0, -r),
        WipeDir::Down => (0.0, r),
    }
}

/// Normalized `(in_offset, out_offset)` for a **push**: the outgoing clip slides
/// off toward `dir` while the incoming follows in behind it.
fn push_offsets(dir: WipeDir, p: f32) -> ((f32, f32), (f32, f32)) {
    let r = 1.0 - p;
    match dir {
        WipeDir::Left => ((r, 0.0), (-p, 0.0)),
        WipeDir::Right => ((-r, 0.0), (p, 0.0)),
        WipeDir::Up => ((0.0, r), (0.0, -p)),
        WipeDir::Down => ((0.0, -r), (0.0, p)),
    }
}

// --- Transition geometry -----------------------------------------------------

/// The spatial parameters of a transition (`None` for pure opacity / dip blends).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TransitionGeometry {
    /// No geometry — a pure opacity / dip blend.
    None,
    /// A wipe revealing the incoming clip: `edge` is the normalized sweep position
    /// (0..1), `softness` the half-width (0..1) of the antialiased band.
    Wipe { shape: WipeShape, edge: f32, softness: f32 },
    /// Slide / push: normalized pixel offsets (fractions of frame size) of the
    /// incoming and outgoing clips. `(0,0)` is centred.
    Slide { in_offset: (f32, f32), out_offset: (f32, f32) },
    /// Iris reveal: normalized circle radius 0..1 (0 hidden, 1 covers the frame).
    Iris { radius: f32 },
}

// --- Transition state --------------------------------------------------------

/// The fully-evaluated per-frame state of a transition at one `progress` value.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TransitionState {
    /// Opacity of the outgoing (A) clip, 0..1.
    pub out_opacity: f32,
    /// Opacity of the incoming (B) clip, 0..1.
    pub in_opacity: f32,
    /// Optional dip colour (straight RGBA) + strength (0..1) the cut passes through.
    pub dip: Option<([f32; 4], f32)>,
    /// Geometry parameters describing any spatial warp.
    pub geometry: TransitionGeometry,
}

impl TransitionState {
    /// Sum of the two clip opacities — `~1.0` for a balanced crossfade.
    pub fn opacity_sum(&self) -> f32 {
        self.out_opacity + self.in_opacity
    }
}

// --- Transition record (stored on App) ---------------------------------------

/// A transition placed on the timeline, attached to a clip's head/tail or the cut
/// after it. Owns its kind, duration (in frames), and alignment; the per-frame
/// look comes from [`TransitionFx::evaluate`] / [`TransitionFx::evaluate_at_frame`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TransitionFx {
    pub kind: TransitionFxKind,
    /// Duration in frames (`>= MIN_TRANSITION_FRAMES`).
    pub duration_frames: u32,
    pub align: TransitionAlign,
    /// The clip this transition is anchored to (index into `project.clips`).
    pub clip_idx: usize,
}

impl TransitionFx {
    /// Build a transition, clamping the duration to at least one frame.
    pub fn new(clip_idx: usize, kind: TransitionFxKind, duration_frames: u32, align: TransitionAlign) -> Self {
        Self { kind, duration_frames: duration_frames.max(MIN_TRANSITION_FRAMES), align, clip_idx }
    }

    /// Duration in frames as an `f32` (never below one frame).
    pub fn frames(&self) -> f32 {
        self.duration_frames.max(MIN_TRANSITION_FRAMES) as f32
    }

    /// Progress 0..1 for a position `frame_from_anchor` frames from the anchor,
    /// respecting the alignment span. Outside the span clamps to 0 / 1.
    pub fn progress_at_frame(&self, frame_from_anchor: f32) -> f32 {
        let (s, e) = self.align.span(self.frames());
        if e <= s {
            return if frame_from_anchor < s { 0.0 } else { 1.0 };
        }
        ((frame_from_anchor - s) / (e - s)).clamp(0.0, 1.0)
    }

    /// Evaluate at an explicit `progress` (clamped).
    pub fn evaluate(&self, progress: f32) -> TransitionState {
        self.kind.evaluate(progress)
    }

    /// Evaluate at a frame offset from the anchor (maps through the alignment).
    pub fn evaluate_at_frame(&self, frame_from_anchor: f32) -> TransitionState {
        self.evaluate(self.progress_at_frame(frame_from_anchor))
    }
}

// --- Time remap / speed ------------------------------------------------------

/// A clip's time-remapping: a constant signed `speed` factor (negative = reverse)
/// plus an optional keyframed timeline→source curve. Pure & deterministic.
///
/// When `keys` has `< 2` entries the constant `speed` model is used; otherwise the
/// piecewise-linear `(timeline_t, source_t)` curve overrides it. All times are
/// *local* to the clip (0 = clip start).
#[derive(Clone, Debug, PartialEq)]
pub struct TimeRemap {
    /// Signed playback speed factor. `1.0` = realtime, `2.0` = double speed,
    /// `0.5` = half speed, negative = reverse. Used when `keys.len() < 2`.
    pub speed: f32,
    /// The source material's length in seconds (what's being remapped).
    pub source_duration: f32,
    /// Optional `(timeline_t, source_t)` keys, sorted by `timeline_t`. With `>= 2`
    /// entries these override `speed` (linear interpolation between keys).
    pub keys: Vec<(f32, f32)>,
    /// Sampling quality flag: blend the two adjacent source frames (`true`) vs
    /// snap to the nearest source frame (`false`). Affects sampling only, not
    /// timing.
    pub frame_blend: bool,
}

impl TimeRemap {
    /// A constant-speed remap (no keyframes).
    pub fn constant(speed: f32, source_duration: f32) -> Self {
        Self { speed, source_duration: source_duration.max(0.0), keys: Vec::new(), frame_blend: false }
    }

    /// The signed speed clamped to a sane range; non-finite → `1.0`.
    pub fn effective_speed(&self) -> f32 {
        if !self.speed.is_finite() || self.speed == 0.0 {
            return 1.0;
        }
        let mag = self.speed.abs().clamp(MIN_SPEED, MAX_SPEED);
        if self.speed < 0.0 { -mag } else { mag }
    }

    /// Whether playback runs in reverse (negative speed, constant-speed mode only).
    pub fn is_reversed(&self) -> bool {
        self.keys.len() < 2 && self.effective_speed() < 0.0
    }

    /// Effective timeline duration after applying the constant speed:
    /// `source_duration / |speed|`. So 2× speed halves the duration. (Keyframed
    /// remaps define their own span, so this reflects the constant-speed model.)
    pub fn effective_duration(&self) -> f32 {
        let s = self.effective_speed().abs().max(MIN_SPEED);
        (self.source_duration / s).max(0.0)
    }

    /// Map a *local* timeline time (seconds, 0 = clip start) to a source time
    /// (seconds). Piecewise-linear over `keys` when present, else constant speed
    /// (walking backward from the source end when reversed). Clamped to
    /// `[0, source_duration]`.
    pub fn source_time_at(&self, timeline_t: f32) -> f32 {
        let raw = if self.keys.len() >= 2 {
            remap_keys(&self.keys, timeline_t)
        } else {
            let s = self.effective_speed();
            let t = timeline_t.max(0.0);
            if s < 0.0 {
                // Reverse: start at the source end and walk backward (s < 0).
                self.source_duration + s * t
            } else {
                s * t
            }
        };
        raw.clamp(0.0, self.source_duration.max(0.0))
    }
}

/// Piecewise-linear interpolation of a sorted `(timeline_t, source_t)` key list,
/// clamped to the first/last source value beyond the ends.
fn remap_keys(keys: &[(f32, f32)], t: f32) -> f32 {
    if keys.is_empty() {
        return 0.0;
    }
    if t <= keys[0].0 {
        return keys[0].1.max(0.0);
    }
    let last = keys[keys.len() - 1];
    if t >= last.0 {
        return last.1.max(0.0);
    }
    for w in keys.windows(2) {
        let (t0, s0) = w[0];
        let (t1, s1) = w[1];
        if t <= t1 {
            let frac = (t - t0) / (t1 - t0).max(1e-9);
            return (s0 + frac * (s1 - s0)).max(0.0);
        }
    }
    last.1.max(0.0)
}

// --- App action handler ------------------------------------------------------

pub trait AppTransitionFxExt {
    /// Build the pure [`TimeRemap`] model for a clip from its current speed /
    /// reverse / frame-blend / remap-key state. `None` for a missing clip.
    fn clip_time_remap(&self, clip_idx: usize) -> Option<TimeRemap>;
    /// Apply a Phase-3 transition / speed / time-remap action.
    fn apply_transition_fx(&mut self, action: Action);
}

impl AppTransitionFxExt for App {
    fn clip_time_remap(&self, clip_idx: usize) -> Option<TimeRemap> {
        let c = self.project.clips.get(clip_idx)?;
        let source_duration = c.source.source_len().unwrap_or(c.duration);
        let speed = if c.reversed { -c.speed } else { c.speed };
        // Translate the clip's absolute remap keys to local (clip-relative) time.
        let keys = if c.time_remap_enabled {
            c.time_remap_keys
                .iter()
                .map(|&(tt, st)| ((tt - c.start).max(0.0), st))
                .collect()
        } else {
            Vec::new()
        };
        Some(TimeRemap { speed, source_duration, keys, frame_blend: c.remap_frame_blend })
    }

    fn apply_transition_fx(&mut self, action: Action) {
        match action {
            // --- Transition list -------------------------------------------------
            Action::AddTransitionFx { clip_idx, kind, duration_frames, align } => {
                if clip_idx < self.project.clips.len() {
                    self.transition_fx.push(TransitionFx::new(clip_idx, kind, duration_frames, align));
                    self.host.mark_dirty();
                }
            }
            Action::RemoveTransitionFx { idx } => {
                if idx < self.transition_fx.len() {
                    self.transition_fx.remove(idx);
                    self.host.mark_dirty();
                }
            }
            Action::SetTransitionFxDuration { idx, duration_frames } => {
                if let Some(tr) = self.transition_fx.get_mut(idx) {
                    tr.duration_frames = duration_frames.max(MIN_TRANSITION_FRAMES);
                    self.host.mark_dirty();
                }
            }
            Action::SetTransitionFxAlign { idx, align } => {
                if let Some(tr) = self.transition_fx.get_mut(idx) {
                    tr.align = align;
                    self.host.mark_dirty();
                }
            }
            Action::SetTransitionFxKind { idx, kind } => {
                if let Some(tr) = self.transition_fx.get_mut(idx) {
                    tr.kind = kind;
                    self.host.mark_dirty();
                }
            }
            // --- Per-clip speed / time-remap -------------------------------------
            Action::SetClipSpeedFactor { clip_idx, factor } => {
                if let Some(c) = self.project.clips.get_mut(clip_idx) {
                    let f = if factor.is_finite() && factor != 0.0 { factor } else { 1.0 };
                    c.reversed = f < 0.0;
                    c.speed = f.abs().clamp(MIN_SPEED, MAX_SPEED);
                    // Constant speed: drop any keyframed remap so `speed` governs.
                    c.time_remap_enabled = false;
                    self.host.mark_dirty();
                }
            }
            Action::SetClipFrameBlend { clip_idx, blend } => {
                if let Some(c) = self.project.clips.get_mut(clip_idx) {
                    c.remap_frame_blend = blend;
                    self.host.mark_dirty();
                }
            }
            Action::AddRemapKeyframe { clip_idx, timeline_t, source_t } => {
                if let Some(c) = self.project.clips.get_mut(clip_idx) {
                    c.time_remap_enabled = true;
                    c.time_remap_keys.push((timeline_t, source_t.max(0.0)));
                    c.time_remap_keys
                        .sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
                    self.host.mark_dirty();
                }
            }
            Action::ClearRemapKeyframes { clip_idx } => {
                if let Some(c) = self.project.clips.get_mut(clip_idx) {
                    c.time_remap_keys.clear();
                    c.time_remap_enabled = false;
                    self.host.mark_dirty();
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app_state::{Action, App};

    // --- Cross dissolve ------------------------------------------------------

    #[test]
    fn cross_dissolve_endpoints_and_midpoint_sum_to_one() {
        let k = TransitionFxKind::CrossDissolve;
        let a = k.evaluate(0.0);
        assert!((a.out_opacity - 1.0).abs() < 1e-6 && a.in_opacity.abs() < 1e-6);
        let m = k.evaluate(0.5);
        assert!((m.out_opacity - 0.5).abs() < 1e-6 && (m.in_opacity - 0.5).abs() < 1e-6);
        // Mid-transition the two opacities sum to ~1.
        assert!((m.opacity_sum() - 1.0).abs() < 1e-6);
        let b = k.evaluate(1.0);
        assert!(b.out_opacity.abs() < 1e-6 && (b.in_opacity - 1.0).abs() < 1e-6);
        assert_eq!(m.geometry, TransitionGeometry::None);
    }

    #[test]
    fn cross_dissolve_clamps_out_of_range_progress() {
        let k = TransitionFxKind::CrossDissolve;
        let lo = k.evaluate(-3.0);
        assert!((lo.out_opacity - 1.0).abs() < 1e-6);
        let hi = k.evaluate(5.0);
        assert!((hi.in_opacity - 1.0).abs() < 1e-6);
    }

    // --- Dip to black / white ------------------------------------------------

    #[test]
    fn dip_to_black_hits_full_black_at_midpoint() {
        let s = TransitionFxKind::DipToBlack.evaluate(0.5);
        // Both clips invisible, dip at full strength → pure black frame.
        assert!(s.out_opacity.abs() < 1e-6 && s.in_opacity.abs() < 1e-6);
        let (color, amount) = s.dip.expect("dip present at midpoint");
        assert_eq!(color, DIP_BLACK);
        assert!((amount - 1.0).abs() < 1e-6);
    }

    #[test]
    fn dip_to_black_fades_out_then_in() {
        let k = TransitionFxKind::DipToBlack;
        // Quarter point: outgoing half visible, dip half strength, incoming hidden.
        let q = k.evaluate(0.25);
        assert!((q.out_opacity - 0.5).abs() < 1e-6);
        assert!(q.in_opacity.abs() < 1e-6);
        assert!((q.dip.unwrap().1 - 0.5).abs() < 1e-6);
        // Three-quarter point: outgoing hidden, incoming half visible.
        let tq = k.evaluate(0.75);
        assert!(tq.out_opacity.abs() < 1e-6);
        assert!((tq.in_opacity - 0.5).abs() < 1e-6);
        assert!((tq.dip.unwrap().1 - 0.5).abs() < 1e-6);
    }

    #[test]
    fn dip_to_white_uses_white_and_clears_at_ends() {
        let k = TransitionFxKind::DipToWhite;
        let mid = k.evaluate(0.5);
        assert_eq!(mid.dip.unwrap().0, DIP_WHITE);
        let start = k.evaluate(0.0);
        assert!((start.out_opacity - 1.0).abs() < 1e-6);
        assert!((start.dip.unwrap().1).abs() < 1e-6, "no dip at the very start");
        let end = k.evaluate(1.0);
        assert!((end.in_opacity - 1.0).abs() < 1e-6);
    }

    // --- Wipe ----------------------------------------------------------------

    #[test]
    fn wipe_edge_moves_zero_to_one() {
        let k = TransitionFxKind::Wipe { shape: WipeShape::Linear(WipeDir::Left), softness: 0.1 };
        let geom = |p: f32| match k.evaluate(p).geometry {
            TransitionGeometry::Wipe { edge, .. } => edge,
            _ => panic!("expected wipe geometry"),
        };
        assert!((geom(0.0) - 0.0).abs() < 1e-6);
        assert!((geom(0.5) - 0.5).abs() < 1e-6);
        assert!((geom(1.0) - 1.0).abs() < 1e-6);
        // Both frames stay opaque during a wipe.
        let s = k.evaluate(0.5);
        assert!((s.out_opacity - 1.0).abs() < 1e-6 && (s.in_opacity - 1.0).abs() < 1e-6);
    }

    #[test]
    fn wipe_softness_is_clamped_into_unit_range() {
        let k = TransitionFxKind::Wipe { shape: WipeShape::Radial, softness: 9.0 };
        if let TransitionGeometry::Wipe { softness, shape, .. } = k.evaluate(0.5).geometry {
            assert!((softness - 1.0).abs() < 1e-6);
            assert_eq!(shape, WipeShape::Radial);
        } else {
            panic!("expected wipe geometry");
        }
    }

    // --- Slide / push --------------------------------------------------------

    #[test]
    fn slide_incoming_starts_offscreen_and_centers() {
        let k = TransitionFxKind::Slide(WipeDir::Left);
        let off = |p: f32| match k.evaluate(p).geometry {
            TransitionGeometry::Slide { in_offset, out_offset } => (in_offset, out_offset),
            _ => panic!("expected slide geometry"),
        };
        // p=0: incoming fully off to the left (-1), outgoing stationary.
        let (i0, o0) = off(0.0);
        assert!((i0.0 + 1.0).abs() < 1e-6 && i0.1.abs() < 1e-6);
        assert_eq!(o0, (0.0, 0.0));
        // p=1: incoming centred.
        let (i1, _o1) = off(1.0);
        assert!(i1.0.abs() < 1e-6 && i1.1.abs() < 1e-6);
    }

    #[test]
    fn push_moves_both_clips_in_opposite_phase() {
        let k = TransitionFxKind::Push(WipeDir::Left);
        let off = |p: f32| match k.evaluate(p).geometry {
            TransitionGeometry::Slide { in_offset, out_offset } => (in_offset, out_offset),
            _ => panic!("expected slide geometry"),
        };
        // p=0: incoming off to the right (+1), outgoing centred.
        let (i0, o0) = off(0.0);
        assert!((i0.0 - 1.0).abs() < 1e-6);
        assert!(o0.0.abs() < 1e-6);
        // p=1: incoming centred, outgoing off to the left (-1).
        let (i1, o1) = off(1.0);
        assert!(i1.0.abs() < 1e-6);
        assert!((o1.0 + 1.0).abs() < 1e-6);
    }

    // --- Iris ----------------------------------------------------------------

    #[test]
    fn iris_radius_tracks_progress() {
        let k = TransitionFxKind::Iris;
        let r = |p: f32| match k.evaluate(p).geometry {
            TransitionGeometry::Iris { radius } => radius,
            _ => panic!("expected iris geometry"),
        };
        assert!(r(0.0).abs() < 1e-6);
        assert!((r(0.5) - 0.5).abs() < 1e-6);
        assert!((r(1.0) - 1.0).abs() < 1e-6);
    }

    // --- Alignment / progress mapping ----------------------------------------

    #[test]
    fn centered_alignment_spans_symmetrically() {
        let tr = TransitionFx::new(0, TransitionFxKind::CrossDissolve, 10, TransitionAlign::Centered);
        // Centered: span is [-5, +5]. At the anchor (0) progress is 0.5.
        assert!((tr.progress_at_frame(0.0) - 0.5).abs() < 1e-6);
        assert!(tr.progress_at_frame(-5.0).abs() < 1e-6);
        assert!((tr.progress_at_frame(5.0) - 1.0).abs() < 1e-6);
        // Beyond the span clamps.
        assert!(tr.progress_at_frame(-100.0).abs() < 1e-6);
        assert!((tr.progress_at_frame(100.0) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn start_and_end_alignment_map_progress() {
        let start = TransitionFx::new(0, TransitionFxKind::CrossDissolve, 8, TransitionAlign::Start);
        // Start: span [0, 8]. Anchor=0 → progress 0; +8 → 1.
        assert!(start.progress_at_frame(0.0).abs() < 1e-6);
        assert!((start.progress_at_frame(4.0) - 0.5).abs() < 1e-6);
        assert!((start.progress_at_frame(8.0) - 1.0).abs() < 1e-6);
        let end = TransitionFx::new(0, TransitionFxKind::CrossDissolve, 8, TransitionAlign::End);
        // End: span [-8, 0]. Anchor=0 → progress 1; -8 → 0.
        assert!((end.progress_at_frame(0.0) - 1.0).abs() < 1e-6);
        assert!((end.progress_at_frame(-4.0) - 0.5).abs() < 1e-6);
        assert!(end.progress_at_frame(-8.0).abs() < 1e-6);
    }

    #[test]
    fn evaluate_at_frame_threads_through_alignment() {
        let tr = TransitionFx::new(0, TransitionFxKind::CrossDissolve, 10, TransitionAlign::Centered);
        let s = tr.evaluate_at_frame(0.0);
        assert!((s.out_opacity - 0.5).abs() < 1e-6 && (s.in_opacity - 0.5).abs() < 1e-6);
    }

    #[test]
    fn duration_is_clamped_to_at_least_one_frame() {
        let tr = TransitionFx::new(0, TransitionFxKind::Iris, 0, TransitionAlign::Start);
        assert_eq!(tr.duration_frames, 1);
        assert!((tr.frames() - 1.0).abs() < 1e-6);
    }

    // --- Time remap / speed (pure) -------------------------------------------

    #[test]
    fn identity_at_speed_one() {
        let r = TimeRemap::constant(1.0, 10.0);
        assert!((r.source_time_at(0.0) - 0.0).abs() < 1e-6);
        assert!((r.source_time_at(3.0) - 3.0).abs() < 1e-6);
        assert!((r.source_time_at(10.0) - 10.0).abs() < 1e-6);
        assert!((r.effective_duration() - 10.0).abs() < 1e-6);
        assert!(!r.is_reversed());
    }

    #[test]
    fn speed_2x_halves_duration_and_doubles_source_advance() {
        let r = TimeRemap::constant(2.0, 10.0);
        // 2× speed → effective timeline duration halves.
        assert!((r.effective_duration() - 5.0).abs() < 1e-6);
        // At local t=2 the source has advanced to 4.
        assert!((r.source_time_at(2.0) - 4.0).abs() < 1e-6);
    }

    #[test]
    fn half_speed_doubles_duration() {
        let r = TimeRemap::constant(0.5, 10.0);
        assert!((r.effective_duration() - 20.0).abs() < 1e-6);
        assert!((r.source_time_at(4.0) - 2.0).abs() < 1e-6);
    }

    #[test]
    fn reverse_maps_end_to_start() {
        let r = TimeRemap::constant(-1.0, 10.0);
        assert!(r.is_reversed());
        // Reverse realtime: timeline 0 → source end, timeline end → source 0.
        assert!((r.source_time_at(0.0) - 10.0).abs() < 1e-6);
        assert!((r.source_time_at(10.0) - 0.0).abs() < 1e-6);
        assert!((r.source_time_at(2.5) - 7.5).abs() < 1e-6);
        // Magnitude still drives the duration.
        assert!((r.effective_duration() - 10.0).abs() < 1e-6);
    }

    #[test]
    fn remap_keys_linear_interpolate_between_keys() {
        let r = TimeRemap {
            speed: 1.0,
            source_duration: 8.0,
            keys: vec![(0.0, 0.0), (4.0, 8.0)], // 2× via the curve
            frame_blend: false,
        };
        // Halfway along the timeline segment → halfway through the source.
        assert!((r.source_time_at(2.0) - 4.0).abs() < 1e-6);
        // Before the first / after the last key clamps.
        assert!((r.source_time_at(-5.0) - 0.0).abs() < 1e-6);
        assert!((r.source_time_at(10.0) - 8.0).abs() < 1e-6);
    }

    #[test]
    fn remap_keys_multi_segment_have_different_slopes() {
        let r = TimeRemap {
            speed: 1.0,
            source_duration: 10.0,
            keys: vec![(0.0, 0.0), (2.0, 1.0), (4.0, 10.0)],
            frame_blend: true,
        };
        // First segment: slope 0.5 → at t=1 source=0.5.
        assert!((r.source_time_at(1.0) - 0.5).abs() < 1e-6);
        // Second segment: from (2,1) to (4,10) → at t=3 source = 5.5.
        assert!((r.source_time_at(3.0) - 5.5).abs() < 1e-6);
    }

    #[test]
    fn source_time_clamps_to_source_bounds() {
        let r = TimeRemap::constant(2.0, 6.0);
        // Past the end clamps to the source duration.
        assert!((r.source_time_at(100.0) - 6.0).abs() < 1e-6);
        // Negative timeline time clamps to 0.
        assert!((r.source_time_at(-1.0)).abs() < 1e-6);
    }

    #[test]
    fn effective_speed_clamps_and_handles_nonfinite() {
        assert!((TimeRemap::constant(0.0, 1.0).effective_speed() - 1.0).abs() < 1e-6);
        assert!((TimeRemap::constant(f32::NAN, 1.0).effective_speed() - 1.0).abs() < 1e-6);
        assert!((TimeRemap::constant(1e9, 1.0).effective_speed() - MAX_SPEED).abs() < 1e-3);
        assert!((TimeRemap::constant(-1e9, 1.0).effective_speed() + MAX_SPEED).abs() < 1e-3);
    }

    // --- App actions ---------------------------------------------------------

    #[test]
    fn add_and_remove_transition_fx() {
        let mut app = App::new();
        assert!(app.transition_fx.is_empty());
        app.apply(Action::AddTransitionFx {
            clip_idx: 0,
            kind: TransitionFxKind::CrossDissolve,
            duration_frames: 15,
            align: TransitionAlign::Centered,
        });
        assert_eq!(app.transition_fx.len(), 1);
        assert_eq!(app.transition_fx[0].duration_frames, 15);
        assert_eq!(app.transition_fx[0].kind.label(), "Cross Dissolve");
        app.apply(Action::RemoveTransitionFx { idx: 0 });
        assert!(app.transition_fx.is_empty());
        // Out-of-range remove is a no-op.
        app.apply(Action::RemoveTransitionFx { idx: 99 });
    }

    #[test]
    fn add_transition_fx_on_missing_clip_is_noop() {
        let mut app = App::new();
        app.apply(Action::AddTransitionFx {
            clip_idx: 999,
            kind: TransitionFxKind::Iris,
            duration_frames: 10,
            align: TransitionAlign::Start,
        });
        assert!(app.transition_fx.is_empty());
    }

    #[test]
    fn edit_transition_fx_duration_align_kind() {
        let mut app = App::new();
        app.apply(Action::AddTransitionFx {
            clip_idx: 0,
            kind: TransitionFxKind::CrossDissolve,
            duration_frames: 10,
            align: TransitionAlign::Centered,
        });
        app.apply(Action::SetTransitionFxDuration { idx: 0, duration_frames: 0 });
        assert_eq!(app.transition_fx[0].duration_frames, 1, "clamped to one frame");
        app.apply(Action::SetTransitionFxAlign { idx: 0, align: TransitionAlign::End });
        assert_eq!(app.transition_fx[0].align, TransitionAlign::End);
        app.apply(Action::SetTransitionFxKind { idx: 0, kind: TransitionFxKind::DipToBlack });
        assert_eq!(app.transition_fx[0].kind, TransitionFxKind::DipToBlack);
    }

    #[test]
    fn set_clip_speed_factor_sets_speed_and_reverse() {
        let mut app = App::new();
        app.apply(Action::SetClipSpeedFactor { clip_idx: 0, factor: -2.0 });
        let c = &app.project.clips[0];
        assert!(c.reversed);
        assert!((c.speed - 2.0).abs() < 1e-6);
        // Zero / non-finite factor falls back to realtime forward.
        app.apply(Action::SetClipSpeedFactor { clip_idx: 0, factor: 0.0 });
        let c = &app.project.clips[0];
        assert!(!c.reversed);
        assert!((c.speed - 1.0).abs() < 1e-6);
    }

    #[test]
    fn clip_time_remap_reflects_speed_and_reverse() {
        let mut app = App::new();
        // Clip 0 is a 6s colour clip → no source len, falls back to its duration.
        app.apply(Action::SetClipSpeedFactor { clip_idx: 0, factor: 2.0 });
        let r = app.clip_time_remap(0).expect("remap");
        assert!((r.effective_speed() - 2.0).abs() < 1e-6);
        assert!((r.effective_duration() - r.source_duration / 2.0).abs() < 1e-4);
        app.apply(Action::SetClipSpeedFactor { clip_idx: 0, factor: -1.0 });
        let r = app.clip_time_remap(0).expect("remap");
        assert!(r.is_reversed());
    }

    #[test]
    fn frame_blend_flag_round_trips_through_action() {
        let mut app = App::new();
        assert!(!app.project.clips[0].remap_frame_blend);
        app.apply(Action::SetClipFrameBlend { clip_idx: 0, blend: true });
        assert!(app.project.clips[0].remap_frame_blend);
        assert!(app.clip_time_remap(0).unwrap().frame_blend);
    }

    #[test]
    fn add_and_clear_remap_keyframes() {
        let mut app = App::new();
        app.apply(Action::AddRemapKeyframe { clip_idx: 0, timeline_t: 4.0, source_t: 2.0 });
        app.apply(Action::AddRemapKeyframe { clip_idx: 0, timeline_t: 1.0, source_t: 0.5 });
        let c = &app.project.clips[0];
        assert!(c.time_remap_enabled);
        assert_eq!(c.time_remap_keys.len(), 2);
        // Keys are kept sorted by timeline time.
        assert!(c.time_remap_keys[0].0 <= c.time_remap_keys[1].0);
        app.apply(Action::ClearRemapKeyframes { clip_idx: 0 });
        assert!(app.project.clips[0].time_remap_keys.is_empty());
        assert!(!app.project.clips[0].time_remap_enabled);
    }

    #[test]
    fn clip_time_remap_translates_absolute_keys_to_local() {
        let mut app = App::new();
        // Clip 1 starts at t=6 (default project). Add an absolute key at t=8.
        app.apply(Action::AddRemapKeyframe { clip_idx: 1, timeline_t: 6.0, source_t: 0.0 });
        app.apply(Action::AddRemapKeyframe { clip_idx: 1, timeline_t: 8.0, source_t: 4.0 });
        let r = app.clip_time_remap(1).expect("remap");
        // Local keys: (0,0) and (2,4). At local t=1 the source is 2.
        assert!((r.source_time_at(1.0) - 2.0).abs() < 1e-4);
    }
}
