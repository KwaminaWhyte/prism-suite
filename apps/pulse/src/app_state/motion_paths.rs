//! **Spatial motion paths + the graph-editor temporal-easing model.**
//!
//! Two orthogonal pieces of After Effects' keyframe interpolation, implemented
//! as *pure, deterministic* math so they can be unit-tested headlessly:
//!
//! 1. **Spatial motion path** — a layer's animated `(x, y)` position keyframed in
//!    2D, where each keyframe carries **spatial Bézier tangents** (`in_handle` /
//!    `out_handle`, stored as offsets from the keyframe point). Reading the keys
//!    in order yields a cubic-Bézier spline through comp space:
//!    [`MotionPath::sample_position`] walks it with **De Casteljau**;
//!    [`MotionPath::sample_constant_speed`] re-parameterizes the spline by
//!    **arc length** so a layer can travel at constant speed regardless of how
//!    the control handles bunch the natural parameter; and
//!    [`MotionPath::orientation_at`] reads the path **tangent** to drive
//!    *Orient Along Path* (auto-orient).
//!
//! 2. **Temporal easing (graph editor)** — each keyframe carries an incoming and
//!    outgoing [`KeyframeEase`] with an **influence** (`0..100`, how far the
//!    handle reaches across the segment's time) and a **speed** (the handle's
//!    slope). A segment's pair of handles defines the standard AE two-control-
//!    point cubic-Bézier *value graph*; [`eased_progress`] solves `y` given the
//!    elapsed-time fraction `x` (Newton's method with a bisection fallback), and
//!    [`MotionPath::value_at`] maps that onto a scalar property. [`EasePreset`]
//!    supplies the F9 family — linear, easy-ease, ease-in, ease-out, hold.
//!
//! Everything here is app-side state (a `Vec<MotionPath>` on [`App`]) and a set
//! of free functions, mirroring `particles.rs`: no time source, no IO, no global
//! RNG — identical inputs always yield identical output.

use super::*;

/// Coarse per-segment easing mode (kept for the simple timeline sampler in
/// `apply_batch5.rs`). The richer graph-editor model layers [`KeyframeEase`]
/// handles on top; the only mode the Bézier samplers special-case is
/// [`MotionEasing::Hold`] (a step).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MotionEasing {
    Linear,
    EaseIn,
    EaseOut,
    EaseInOut,
    Hold,
}

/// One half (incoming **or** outgoing) of a keyframe's temporal ease, in the
/// After Effects influence/speed parameterization.
///
/// * `influence` — `0..100`: how far the Bézier handle reaches *across the
///   segment's time* as a percentage. `0` pins the handle on the keyframe
///   (a linear-leaving / linear-arriving end); `~33` is AE's Easy Ease.
/// * `speed` — the handle's **slope** as a ratio of the segment's average speed.
///   `0` makes the curve flat at that end (full ease); `1` keeps it on the
///   straight diagonal (no easing).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct KeyframeEase {
    pub influence: f32,
    pub speed: f32,
}

impl Default for KeyframeEase {
    /// The straight-diagonal ease (`y = x`): a value-neutral handle so a fresh
    /// keyframe interpolates linearly until it's eased.
    fn default() -> Self {
        Self { influence: 100.0 / 3.0, speed: 1.0 }
    }
}

impl KeyframeEase {
    /// A flat handle (`speed = 0`) reaching `influence` percent across the
    /// segment — the building block of ease-in / ease-out / easy-ease.
    pub const fn flat(influence: f32) -> Self {
        Self { influence, speed: 0.0 }
    }
}

/// The After Effects keyframe-assistant presets (the F9 family). Applying a
/// preset overwrites a keyframe's [`KeyframeEase`] handles (and its coarse
/// [`MotionEasing`] mode).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EasePreset {
    /// Constant velocity — the straight diagonal value graph.
    Linear,
    /// Symmetric ease in *and* out (AE's "Easy Ease" / F9).
    EasyEase,
    /// Slow→fast: flat leaving the key, linear arriving (CSS `ease-in`).
    EaseIn,
    /// Fast→slow: linear leaving the key, flat arriving (CSS `ease-out`).
    EaseOut,
    /// Step: hold the value until the next key.
    Hold,
}

impl EasePreset {
    /// The `(ease_in, ease_out, mode)` a keyframe takes under this preset.
    ///
    /// A *segment* between keys `A` and `B` uses `A.ease_out` and `B.ease_in`, so
    /// these are chosen such that a segment whose **both** ends carry the preset
    /// produces the canonical curve:
    /// linear → `y=x`; easy-ease → `cubic-bezier(.33,0,.67,1)`;
    /// ease-in → `(.42,0,1,1)`; ease-out → `(0,0,.58,1)`.
    fn handles(self) -> (KeyframeEase, KeyframeEase, MotionEasing) {
        match self {
            EasePreset::Linear => (
                KeyframeEase::default(),
                KeyframeEase::default(),
                MotionEasing::Linear,
            ),
            EasePreset::EasyEase => (
                KeyframeEase::flat(100.0 / 3.0),
                KeyframeEase::flat(100.0 / 3.0),
                MotionEasing::EaseInOut,
            ),
            EasePreset::EaseIn => (
                KeyframeEase::flat(0.0),
                KeyframeEase::flat(42.0),
                MotionEasing::EaseIn,
            ),
            EasePreset::EaseOut => (
                KeyframeEase::flat(42.0),
                KeyframeEase::flat(0.0),
                MotionEasing::EaseOut,
            ),
            EasePreset::Hold => (
                KeyframeEase::default(),
                KeyframeEase::default(),
                MotionEasing::Hold,
            ),
        }
    }

    /// Overwrite a keyframe's ease handles + mode to this preset.
    pub fn apply_to(self, p: &mut MotionPathPoint) {
        let (ein, eout, mode) = self.handles();
        if self != EasePreset::Hold {
            p.ease_in = ein;
            p.ease_out = eout;
        }
        p.easing = mode;
    }
}

/// Which scalar channel of a path [`MotionPath::value_at`] samples.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PathAxis {
    X,
    Y,
}

/// A single position keyframe on a [`MotionPath`]: a point in comp space at a
/// time, with **spatial** Bézier tangents and **temporal** ease handles.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MotionPathPoint {
    /// Keyframe time (seconds).
    pub time_s: f32,
    /// Position in comp space (comp px).
    pub x: f32,
    pub y: f32,
    /// Spatial tangent **arriving** at this point, as an offset from `(x, y)`
    /// (the cubic control `P2` of the *incoming* segment).
    pub in_handle: (f32, f32),
    /// Spatial tangent **leaving** this point, as an offset from `(x, y)`
    /// (the cubic control `P1` of the *outgoing* segment).
    pub out_handle: (f32, f32),
    /// Coarse easing mode (mostly the `Hold` step flag for the Bézier samplers).
    pub easing: MotionEasing,
    /// Temporal ease arriving at this key (used by the *incoming* segment).
    pub ease_in: KeyframeEase,
    /// Temporal ease leaving this key (used by the *outgoing* segment).
    pub ease_out: KeyframeEase,
}

impl MotionPathPoint {
    /// The chosen scalar channel's value (for the temporal value graph).
    fn axis_value(&self, axis: PathAxis) -> f32 {
        match axis {
            PathAxis::X => self.x,
            PathAxis::Y => self.y,
        }
    }
}

/// A layer's spatial motion path: an ordered list of position keyframes forming
/// a cubic-Bézier spline, plus auto-orient state.
#[derive(Clone, Debug)]
pub struct MotionPath {
    pub id: usize,
    pub layer_id: usize,
    pub points: Vec<MotionPathPoint>,
    pub closed: bool,
    /// *Orient Along Path*: fold the path tangent into the layer's rotation.
    pub auto_orient: bool,
    pub orient_smoothness: f32,
}

// ── Pure math: cubic Bézier (1D + 2D) ─────────────────────────────────────────

type P2 = (f32, f32);

/// Linear interpolation of two 2D points.
fn lerp2(a: P2, b: P2, t: f32) -> P2 {
    (a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t)
}

/// The four control points of the cubic Bézier segment from key `a` to key `b`:
/// `P0 = a`, `P1 = a + a.out_handle`, `P2 = b + b.in_handle`, `P3 = b`.
///
/// A **zero** handle is auto-filled to the After Effects *linear* tangent —
/// one-third of the way to the other key — so a keyframe with no authored spatial
/// curve interpolates as a straight line at uniform speed (a collapsed control on
/// the endpoint would instead ease the parameterization). Authoring any non-zero
/// handle opts that end into a real Bézier curve.
fn seg_controls(a: &MotionPathPoint, b: &MotionPathPoint) -> [P2; 4] {
    let dx = b.x - a.x;
    let dy = b.y - a.y;
    let out_h = if a.out_handle == (0.0, 0.0) {
        (dx / 3.0, dy / 3.0)
    } else {
        a.out_handle
    };
    let in_h = if b.in_handle == (0.0, 0.0) {
        (-dx / 3.0, -dy / 3.0)
    } else {
        b.in_handle
    };
    [
        (a.x, a.y),
        (a.x + out_h.0, a.y + out_h.1),
        (b.x + in_h.0, b.y + in_h.1),
        (b.x, b.y),
    ]
}

/// Evaluate a cubic Bézier at parameter `s ∈ [0,1]` via **De Casteljau** (the
/// repeated-lerp construction — numerically friendly and the textbook method).
fn de_casteljau(p: &[P2; 4], s: f32) -> P2 {
    let ab = lerp2(p[0], p[1], s);
    let bc = lerp2(p[1], p[2], s);
    let cd = lerp2(p[2], p[3], s);
    let abc = lerp2(ab, bc, s);
    let bcd = lerp2(bc, cd, s);
    lerp2(abc, bcd, s)
}

/// The cubic Bézier's derivative (tangent vector) w.r.t. `s`.
/// `B'(s) = 3(1-s)²(P1-P0) + 6(1-s)s(P2-P1) + 3s²(P3-P2)`.
fn bezier_tangent(p: &[P2; 4], s: f32) -> P2 {
    let mt = 1.0 - s;
    let c0 = 3.0 * mt * mt;
    let c1 = 6.0 * mt * s;
    let c2 = 3.0 * s * s;
    (
        c0 * (p[1].0 - p[0].0) + c1 * (p[2].0 - p[1].0) + c2 * (p[3].0 - p[2].0),
        c0 * (p[1].1 - p[0].1) + c1 * (p[2].1 - p[1].1) + c2 * (p[3].1 - p[2].1),
    )
}

// ── Pure math: temporal ease (the AE value graph) ─────────────────────────────

/// A normalized cubic Bézier with endpoints `0` and `1` and interior controls
/// `p1, p2`, evaluated at `s`.
fn cubic_01(s: f32, p1: f32, p2: f32) -> f32 {
    let mt = 1.0 - s;
    3.0 * mt * mt * s * p1 + 3.0 * mt * s * s * p2 + s * s * s
}

/// Derivative of [`cubic_01`] w.r.t. `s`.
fn cubic_01_deriv(s: f32, p1: f32, p2: f32) -> f32 {
    let mt = 1.0 - s;
    3.0 * mt * mt * p1 + 6.0 * mt * s * (p2 - p1) + 3.0 * s * s * (1.0 - p2)
}

/// Invert the x-component of the normalized cubic: find `s` with
/// `cubic_01(s, x1, x2) == x`. **Newton-Raphson** seeded at `s = x`, falling back
/// to **bisection** when the slope is too flat to make progress (`x(s)` is
/// monotonic because `x1, x2 ∈ [0,1]`, so bisection always converges).
fn solve_s_for_x(x: f32, x1: f32, x2: f32) -> f32 {
    let mut s = x;
    for _ in 0..8 {
        let err = cubic_01(s, x1, x2) - x;
        if err.abs() < 1e-7 {
            return s;
        }
        let d = cubic_01_deriv(s, x1, x2);
        if d.abs() < 1e-7 {
            break;
        }
        s -= err / d;
    }
    let (mut lo, mut hi) = (0.0_f32, 1.0_f32);
    s = x;
    for _ in 0..40 {
        let xs = cubic_01(s, x1, x2);
        if (xs - x).abs() < 1e-7 {
            break;
        }
        if xs < x {
            lo = s;
        } else {
            hi = s;
        }
        s = 0.5 * (lo + hi);
    }
    s
}

/// Convert an `(out, in)` handle pair into the value graph's interior control
/// points `(x1, y1, x2, y2)` (endpoints are `(0,0)` and `(1,1)`).
///
/// The outgoing handle reaches `out.influence%` across time at slope
/// `out.speed`; the incoming handle reaches `in.influence%` *back* from the end.
/// `x1, x2` are clamped to `[0,1]` (CSS rule) so the graph stays a function of
/// `x`; `y` may over/undershoot.
fn ease_controls(out: KeyframeEase, in_: KeyframeEase) -> (f32, f32, f32, f32) {
    let ox = (out.influence / 100.0).clamp(0.0, 1.0);
    let ix = (in_.influence / 100.0).clamp(0.0, 1.0);
    let x1 = ox;
    let y1 = ox * out.speed;
    let x2 = (1.0 - ix).clamp(0.0, 1.0);
    let y2 = 1.0 - ix * in_.speed;
    (x1, y1, x2, y2)
}

/// Solve the temporal value graph: given the elapsed-time fraction `raw_t ∈
/// [0,1]` across a segment whose ends carry `out`/`in_` ease handles, return the
/// eased **progress** `y ∈ [0,1]` (the value fraction). The heart of the graph
/// editor — `value_at` and the spatial sampler both drive off this.
pub fn eased_progress(out: KeyframeEase, in_: KeyframeEase, raw_t: f32) -> f32 {
    let x = raw_t.clamp(0.0, 1.0);
    if x <= 0.0 {
        return 0.0;
    }
    if x >= 1.0 {
        return 1.0;
    }
    let (x1, y1, x2, y2) = ease_controls(out, in_);
    let s = solve_s_for_x(x, x1, x2);
    cubic_01(s, y1, y2)
}

// ── MotionPath sampling ───────────────────────────────────────────────────────

/// Half-step (in `s`) used to robustly estimate orientation at a degenerate
/// (zero-tangent) Bézier endpoint via a finite difference on the curve.
const ORIENT_DS: f32 = 1e-3;

/// Number of straight segments each Bézier span is flattened into for arc-length
/// computations. Dense enough that the polyline's length and the
/// constant-speed re-parameterization track the true curve closely.
const FLATTEN_STEPS: usize = 256;

impl MotionPath {
    /// Locate the segment bracketing time `t`: `(segment_index, raw_t)` where
    /// `raw_t ∈ [0,1]` is the elapsed-time fraction across it. Clamps outside the
    /// keyed range. `None` when there are fewer than two keys.
    fn bracket(&self, t: f32) -> Option<(usize, f32)> {
        let n = self.points.len();
        if n < 2 {
            return None;
        }
        if t <= self.points[0].time_s {
            return Some((0, 0.0));
        }
        if t >= self.points[n - 1].time_s {
            return Some((n - 2, 1.0));
        }
        for i in 0..n - 1 {
            let a = self.points[i].time_s;
            let b = self.points[i + 1].time_s;
            if t >= a && t <= b {
                let span = (b - a).max(f32::EPSILON);
                return Some((i, ((t - a) / span).clamp(0.0, 1.0)));
            }
        }
        None
    }

    /// The eased Bézier parameter for a segment at elapsed fraction `raw_t`:
    /// `0`/`1` for a `Hold` segment's step, else the temporal-graph progress.
    fn seg_param(&self, i: usize, raw_t: f32) -> f32 {
        let a = &self.points[i];
        if a.easing == MotionEasing::Hold {
            return if raw_t >= 1.0 { 1.0 } else { 0.0 };
        }
        eased_progress(a.ease_out, self.points[i + 1].ease_in, raw_t)
    }

    /// Sample the spatial path position at time `t`. Walks the cubic-Bézier
    /// spline (De Casteljau) at the temporally-eased parameter, so the curve
    /// passes exactly through every keyframe (`raw_t = 0 → P0`, `1 → P3`).
    /// Empty path → `(0,0)`; single key → that key's position.
    pub fn sample_position(&self, t: f32) -> P2 {
        match self.points.len() {
            0 => (0.0, 0.0),
            1 => (self.points[0].x, self.points[0].y),
            _ => {
                let (i, raw_t) = self.bracket(t).unwrap();
                let a = &self.points[i];
                if a.easing == MotionEasing::Hold && raw_t < 1.0 {
                    return (a.x, a.y);
                }
                let ctrl = seg_controls(a, &self.points[i + 1]);
                de_casteljau(&ctrl, self.seg_param(i, raw_t))
            }
        }
    }

    /// Sample one scalar channel (`X`/`Y`) as a **temporal value graph**: a
    /// straight value lerp eased by the keyframes' handles (no spatial Bézier).
    /// This is the graph-editor view of an animated scalar property.
    pub fn value_at(&self, t: f32, axis: PathAxis) -> f32 {
        match self.points.len() {
            0 => 0.0,
            1 => self.points[0].axis_value(axis),
            _ => {
                let (i, raw_t) = self.bracket(t).unwrap();
                let a = &self.points[i];
                let b = &self.points[i + 1];
                let v0 = a.axis_value(axis);
                let v1 = b.axis_value(axis);
                if a.easing == MotionEasing::Hold && raw_t < 1.0 {
                    return v0;
                }
                v0 + (v1 - v0) * eased_progress(a.ease_out, b.ease_in, raw_t)
            }
        }
    }

    /// The path **heading** at time `t` in degrees (`0°` = +x, clockwise with
    /// `+y` down). Reads the spatial tangent; at a degenerate endpoint where the
    /// analytic tangent vanishes it falls back to a finite difference on the
    /// curve. Drives *Orient Along Path*. Stationary/short paths → `0°`.
    pub fn orientation_at(&self, t: f32) -> f32 {
        if self.points.len() < 2 {
            return 0.0;
        }
        let (i, raw_t) = self.bracket(t).unwrap();
        let a = &self.points[i];
        if a.easing == MotionEasing::Hold {
            return 0.0;
        }
        let ctrl = seg_controls(a, &self.points[i + 1]);
        let s = self.seg_param(i, raw_t);
        let (mut dx, mut dy) = bezier_tangent(&ctrl, s);
        if dx * dx + dy * dy < 1e-12 {
            // Endpoint cusp (e.g. straight line, zero handle): difference the
            // curve in `s` to recover the local direction.
            let s0 = (s - ORIENT_DS).max(0.0);
            let s1 = (s + ORIENT_DS).min(1.0);
            let p0 = de_casteljau(&ctrl, s0);
            let p1 = de_casteljau(&ctrl, s1);
            dx = p1.0 - p0.0;
            dy = p1.1 - p0.1;
        }
        if dx * dx + dy * dy < 1e-12 {
            return 0.0;
        }
        dy.atan2(dx).to_degrees()
    }

    /// Flatten the whole spline into a polyline (uniform in the Bézier
    /// parameter, ignoring temporal ease — this is the *geometric* curve). Used
    /// for arc-length work. `Hold` spans contribute a straight step to the next
    /// key.
    fn flatten(&self) -> Vec<P2> {
        let mut out = Vec::new();
        let n = self.points.len();
        if n == 0 {
            return out;
        }
        out.push((self.points[0].x, self.points[0].y));
        for i in 0..n.saturating_sub(1) {
            let a = &self.points[i];
            let b = &self.points[i + 1];
            if a.easing == MotionEasing::Hold {
                out.push((b.x, b.y));
                continue;
            }
            let ctrl = seg_controls(a, b);
            for k in 1..=FLATTEN_STEPS {
                let s = k as f32 / FLATTEN_STEPS as f32;
                out.push(de_casteljau(&ctrl, s));
            }
        }
        out
    }

    /// The flattened polyline plus the cumulative arc length at each vertex
    /// (`cum[0] == 0`, `cum[last] == total_length`).
    fn arc_table(&self) -> (Vec<P2>, Vec<f32>) {
        let poly = self.flatten();
        let mut cum = Vec::with_capacity(poly.len());
        let mut acc = 0.0_f32;
        for (k, &p) in poly.iter().enumerate() {
            if k > 0 {
                let q = poly[k - 1];
                acc += ((p.0 - q.0).powi(2) + (p.1 - q.1).powi(2)).sqrt();
            }
            cum.push(acc);
        }
        (poly, cum)
    }

    /// Total geometric length of the spline (comp px).
    pub fn total_length(&self) -> f32 {
        self.arc_table().1.last().copied().unwrap_or(0.0)
    }

    /// Sample the spline at **arc-length fraction** `u ∈ [0,1]` — i.e. constant
    /// speed along the curve, independent of how the control handles bunch the
    /// natural parameter. `u = 0.5` is the geometric midpoint of the path.
    pub fn sample_constant_speed(&self, u: f32) -> P2 {
        let (poly, cum) = self.arc_table();
        match poly.len() {
            0 => return (0.0, 0.0),
            1 => return poly[0],
            _ => {}
        }
        let total = *cum.last().unwrap();
        if total <= f32::EPSILON {
            return poly[0];
        }
        let target = u.clamp(0.0, 1.0) * total;
        // Walk to the polyline edge containing `target`, then lerp within it.
        let mut i = 1;
        while i < cum.len() && cum[i] < target {
            i += 1;
        }
        if i >= cum.len() {
            return *poly.last().unwrap();
        }
        let seg_len = (cum[i] - cum[i - 1]).max(f32::EPSILON);
        let f = ((target - cum[i - 1]) / seg_len).clamp(0.0, 1.0);
        lerp2(poly[i - 1], poly[i], f)
    }
}

// ── Action layer ──────────────────────────────────────────────────────────────

/// A motion-path sub-action. Defined in this domain file and wrapped by the
/// single [`Action::MotionPaths`](super::Action::MotionPaths) variant so the
/// central `Action` enum stays small (the `particles.rs` idiom). App-side state
/// edits → **not** undoable.
#[derive(Clone, Debug, PartialEq)]
pub enum MotionPathAction {
    /// Set a keyframe's spatial Bézier tangent handles (offsets from the point).
    SetSpatialTangents {
        path_id: usize,
        index: usize,
        in_handle: (f32, f32),
        out_handle: (f32, f32),
    },
    /// Set a keyframe's temporal ease handles (influence + speed, in/out).
    SetTemporalEase {
        path_id: usize,
        index: usize,
        ease_in: KeyframeEase,
        ease_out: KeyframeEase,
    },
    /// Apply an [`EasePreset`] (linear / easy-ease / ease-in / ease-out / hold).
    ApplyEasePreset {
        path_id: usize,
        index: usize,
        preset: EasePreset,
    },
    /// Toggle *Orient Along Path* on a path.
    ToggleAutoOrient { path_id: usize },
}

impl App {
    /// Apply a motion-path [`Action`] (dispatched via the
    /// [`Action::MotionPaths`](super::Action::MotionPaths) wrapper).
    pub(super) fn apply_motion_paths(&mut self, action: Action) {
        let Action::MotionPaths(ma) = action else {
            unreachable!("apply_motion_paths called with wrong action");
        };
        match ma {
            MotionPathAction::SetSpatialTangents { path_id, index, in_handle, out_handle } => {
                if let Some(p) = self.motion_paths.iter_mut().find(|p| p.id == path_id) {
                    if let Some(pt) = p.points.get_mut(index) {
                        pt.in_handle = in_handle;
                        pt.out_handle = out_handle;
                        self.host.mark_dirty();
                    }
                }
            }
            MotionPathAction::SetTemporalEase { path_id, index, ease_in, ease_out } => {
                if let Some(p) = self.motion_paths.iter_mut().find(|p| p.id == path_id) {
                    if let Some(pt) = p.points.get_mut(index) {
                        pt.ease_in = ease_in;
                        pt.ease_out = ease_out;
                        self.host.mark_dirty();
                    }
                }
            }
            MotionPathAction::ApplyEasePreset { path_id, index, preset } => {
                if let Some(p) = self.motion_paths.iter_mut().find(|p| p.id == path_id) {
                    if let Some(pt) = p.points.get_mut(index) {
                        preset.apply_to(pt);
                        self.host.mark_dirty();
                    }
                }
            }
            MotionPathAction::ToggleAutoOrient { path_id } => {
                if let Some(p) = self.motion_paths.iter_mut().find(|p| p.id == path_id) {
                    p.auto_orient = !p.auto_orient;
                    self.host.mark_dirty();
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dist(a: P2, b: P2) -> f32 {
        ((a.0 - b.0).powi(2) + (a.1 - b.1).powi(2)).sqrt()
    }

    /// A two-key path (t=0 → t=1) with explicit handles/easing for a test.
    fn path(a: MotionPathPoint, b: MotionPathPoint) -> MotionPath {
        MotionPath { id: 0, layer_id: 0, points: vec![a, b], closed: false, auto_orient: false, orient_smoothness: 0.5 }
    }

    fn key(time_s: f32, x: f32, y: f32) -> MotionPathPoint {
        MotionPathPoint {
            time_s,
            x,
            y,
            in_handle: (0.0, 0.0),
            out_handle: (0.0, 0.0),
            easing: MotionEasing::Linear,
            ease_in: KeyframeEase::default(),
            ease_out: KeyframeEase::default(),
        }
    }

    // ── Spatial Bézier ────────────────────────────────────────────────────────

    #[test]
    fn bezier_passes_through_keyframe_endpoints() {
        let mut a = key(0.0, 0.0, 0.0);
        let mut b = key(1.0, 100.0, 40.0);
        a.out_handle = (50.0, -80.0);
        b.in_handle = (-30.0, 60.0);
        let p = path(a, b);
        let p0 = p.sample_position(0.0);
        let p1 = p.sample_position(1.0);
        assert!(dist(p0, (0.0, 0.0)) < 1e-4, "starts at first key, got {:?}", p0);
        assert!(dist(p1, (100.0, 40.0)) < 1e-4, "ends at last key, got {:?}", p1);
    }

    #[test]
    fn before_and_after_range_hold_endpoints() {
        let p = path(key(0.0, 10.0, 20.0), key(1.0, 90.0, 80.0));
        assert_eq!(p.sample_position(-5.0), (10.0, 20.0));
        assert_eq!(p.sample_position(5.0), (90.0, 80.0));
    }

    #[test]
    fn midpoint_of_symmetric_handles_is_centered() {
        // Symmetric vertical handles bow the curve, but the x at the temporal
        // midpoint stays centered between the two keys.
        let mut a = key(0.0, 0.0, 0.0);
        let mut b = key(1.0, 100.0, 0.0);
        a.out_handle = (30.0, 50.0);
        b.in_handle = (-30.0, 50.0);
        let mid = path(a, b).sample_position(0.5);
        assert!((mid.0 - 50.0).abs() < 1e-3, "x centered, got {}", mid.0);
        assert!(mid.1 > 0.0, "curve bows toward the handles");
    }

    #[test]
    fn zero_handles_give_straight_line_interpolation() {
        // No handles ⇒ the cubic collapses to the straight segment.
        let p = path(key(0.0, 0.0, 0.0), key(1.0, 100.0, 100.0));
        let q = p.sample_position(0.25);
        assert!((q.0 - 25.0).abs() < 1e-3 && (q.1 - 25.0).abs() < 1e-3, "linear, got {:?}", q);
    }

    #[test]
    fn single_point_path_is_static() {
        let p = MotionPath { id: 0, layer_id: 0, points: vec![key(0.0, 7.0, 9.0)], closed: false, auto_orient: false, orient_smoothness: 0.5 };
        assert_eq!(p.sample_position(0.0), (7.0, 9.0));
        assert_eq!(p.sample_position(3.0), (7.0, 9.0));
        assert_eq!(p.orientation_at(1.0), 0.0);
    }

    // ── Auto-orient (tangent) ─────────────────────────────────────────────────

    #[test]
    fn auto_orient_horizontal_is_zero_degrees() {
        let p = path(key(0.0, 0.0, 0.0), key(1.0, 100.0, 0.0));
        assert!(p.orientation_at(0.5).abs() < 1e-2, "horizontal ⇒ 0°, got {}", p.orientation_at(0.5));
    }

    #[test]
    fn auto_orient_diagonal_is_45_degrees() {
        let p = path(key(0.0, 0.0, 0.0), key(1.0, 100.0, 100.0));
        assert!((p.orientation_at(0.5) - 45.0).abs() < 1e-2, "diagonal ⇒ 45°, got {}", p.orientation_at(0.5));
    }

    #[test]
    fn auto_orient_matches_tangent_on_known_curve() {
        // Straight vertical (+y down) ⇒ heading 90°, even at the endpoints where
        // the analytic tangent is degenerate (finite-difference fallback).
        let p = path(key(0.0, 0.0, 0.0), key(1.0, 0.0, 100.0));
        assert!((p.orientation_at(0.0) - 90.0).abs() < 1e-1, "start heading 90°, got {}", p.orientation_at(0.0));
        assert!((p.orientation_at(0.5) - 90.0).abs() < 1e-2, "mid heading 90°, got {}", p.orientation_at(0.5));
        assert!((p.orientation_at(1.0) - 90.0).abs() < 1e-1, "end heading 90°, got {}", p.orientation_at(1.0));
    }

    // ── Arc-length / constant speed ───────────────────────────────────────────

    #[test]
    fn total_length_of_straight_line() {
        let p = path(key(0.0, 0.0, 0.0), key(1.0, 30.0, 40.0));
        assert!((p.total_length() - 50.0).abs() < 1e-2, "3-4-5 ⇒ len 50, got {}", p.total_length());
    }

    #[test]
    fn constant_speed_endpoints_and_midpoint() {
        let p = path(key(0.0, 0.0, 0.0), key(1.0, 100.0, 0.0));
        assert!(dist(p.sample_constant_speed(0.0), (0.0, 0.0)) < 1e-3);
        assert!(dist(p.sample_constant_speed(1.0), (100.0, 0.0)) < 1e-3);
        assert!(dist(p.sample_constant_speed(0.5), (50.0, 0.0)) < 1e-1, "geometric midpoint");
    }

    #[test]
    fn arc_length_param_gives_constant_speed() {
        // A smooth bow: the natural Bézier parameter runs fast through the middle
        // and slow at the ends, but arc-length sampling spaces points evenly along
        // the curve (chord ≈ arc on a gently-curving path).
        let mut a = key(0.0, 0.0, 0.0);
        let mut b = key(1.0, 100.0, 0.0);
        a.out_handle = (30.0, 40.0);
        b.in_handle = (-30.0, 40.0);
        let p = path(a, b);

        let n = 40;
        let cs: Vec<P2> = (0..=n).map(|i| p.sample_constant_speed(i as f32 / n as f32)).collect();
        let gaps: Vec<f32> = cs.windows(2).map(|w| dist(w[0], w[1])).collect();
        let mean = gaps.iter().sum::<f32>() / gaps.len() as f32;
        let (mn, mx) = gaps.iter().fold((f32::MAX, 0.0_f32), |(lo, hi), &g| (lo.min(g), hi.max(g)));
        assert!((mx - mn) / mean < 0.05, "constant-speed spacing even: min {mn} max {mx} mean {mean}");

        // Naive parameter sampling on the same curve is far less uniform.
        let nv: Vec<P2> = (0..=n).map(|i| {
            let ctrl = seg_controls(&p.points[0], &p.points[1]);
            de_casteljau(&ctrl, i as f32 / n as f32)
        }).collect();
        let ngaps: Vec<f32> = nv.windows(2).map(|w| dist(w[0], w[1])).collect();
        let (nmn, nmx) = ngaps.iter().fold((f32::MAX, 0.0_f32), |(lo, hi), &g| (lo.min(g), hi.max(g)));
        assert!((nmx - nmn) / mean > (mx - mn) / mean, "arc-length is more uniform than naive param");
    }

    // ── Temporal ease: the value graph ────────────────────────────────────────

    #[test]
    fn eased_progress_pins_endpoints() {
        let e = KeyframeEase::flat(40.0);
        assert_eq!(eased_progress(e, e, 0.0), 0.0);
        assert_eq!(eased_progress(e, e, 1.0), 1.0);
    }

    #[test]
    fn linear_preset_equals_lerp() {
        let mut a = key(0.0, 0.0, 0.0);
        let mut b = key(1.0, 0.0, 100.0);
        EasePreset::Linear.apply_to(&mut a);
        EasePreset::Linear.apply_to(&mut b);
        let p = path(a, b);
        for &t in &[0.1_f32, 0.25, 0.5, 0.75, 0.9] {
            assert!((p.value_at(t, PathAxis::Y) - t * 100.0).abs() < 1e-2,
                "linear == lerp at t={t}, got {}", p.value_at(t, PathAxis::Y));
        }
    }

    #[test]
    fn hold_preset_is_a_step() {
        let mut a = key(0.0, 0.0, 0.0);
        let b = key(1.0, 0.0, 100.0);
        EasePreset::Hold.apply_to(&mut a);
        let p = path(a, b);
        assert_eq!(p.value_at(0.01, PathAxis::Y), 0.0);
        assert_eq!(p.value_at(0.5, PathAxis::Y), 0.0);
        assert_eq!(p.value_at(0.99, PathAxis::Y), 0.0);
        assert_eq!(p.value_at(1.0, PathAxis::Y), 100.0, "snaps at the next key");
        // The spatial sampler holds position too.
        let mut a2 = key(0.0, 0.0, 0.0);
        EasePreset::Hold.apply_to(&mut a2);
        let p2 = path(a2, key(1.0, 100.0, 0.0));
        assert_eq!(p2.sample_position(0.5), (0.0, 0.0));
    }

    #[test]
    fn ease_in_is_slow_then_fast() {
        // CSS ease-in: monotonic, increasing first derivative (convex).
        let mut a = key(0.0, 0.0, 0.0);
        let mut b = key(1.0, 0.0, 100.0);
        EasePreset::EaseIn.apply_to(&mut a);
        EasePreset::EaseIn.apply_to(&mut b);
        let p = path(a, b);
        let n = 20;
        let vs: Vec<f32> = (0..=n).map(|i| p.value_at(i as f32 / n as f32, PathAxis::Y)).collect();
        let deltas: Vec<f32> = vs.windows(2).map(|w| w[1] - w[0]).collect();
        assert!(deltas.iter().all(|&d| d >= -1e-4), "monotonic non-decreasing");
        assert!(deltas.windows(2).all(|w| w[1] >= w[0] - 1e-3), "derivative increasing (convex)");
        assert!(p.value_at(0.5, PathAxis::Y) < 50.0, "below diagonal at the midpoint (slow start)");
    }

    #[test]
    fn ease_out_is_fast_then_slow() {
        let mut a = key(0.0, 0.0, 0.0);
        let mut b = key(1.0, 0.0, 100.0);
        EasePreset::EaseOut.apply_to(&mut a);
        EasePreset::EaseOut.apply_to(&mut b);
        let p = path(a, b);
        let deltas: Vec<f32> = (0..=20).map(|i| p.value_at(i as f32 / 20.0, PathAxis::Y))
            .collect::<Vec<_>>().windows(2).map(|w| w[1] - w[0]).collect();
        assert!(deltas.iter().all(|&d| d >= -1e-4), "monotonic");
        assert!(deltas.windows(2).all(|w| w[1] <= w[0] + 1e-3), "derivative decreasing (concave)");
        assert!(p.value_at(0.5, PathAxis::Y) > 50.0, "above diagonal at the midpoint (fast start)");
    }

    #[test]
    fn easy_ease_is_symmetric() {
        let mut a = key(0.0, 0.0, 0.0);
        let mut b = key(1.0, 0.0, 100.0);
        EasePreset::EasyEase.apply_to(&mut a);
        EasePreset::EasyEase.apply_to(&mut b);
        let p = path(a, b);
        assert!((p.value_at(0.5, PathAxis::Y) - 50.0).abs() < 1e-2, "centered at the midpoint");
        for &t in &[0.1_f32, 0.25, 0.4] {
            let lo = p.value_at(t, PathAxis::Y);
            let hi = p.value_at(1.0 - t, PathAxis::Y);
            assert!((lo + hi - 100.0).abs() < 1e-1, "point-symmetric about center at t={t}: {lo}+{hi}");
        }
    }

    #[test]
    fn ease_in_and_out_differ_and_bracket_linear() {
        let lin = {
            let (mut a, mut b) = (key(0.0, 0.0, 0.0), key(1.0, 0.0, 100.0));
            EasePreset::Linear.apply_to(&mut a);
            EasePreset::Linear.apply_to(&mut b);
            path(a, b).value_at(0.5, PathAxis::Y)
        };
        let ein = {
            let (mut a, mut b) = (key(0.0, 0.0, 0.0), key(1.0, 0.0, 100.0));
            EasePreset::EaseIn.apply_to(&mut a);
            EasePreset::EaseIn.apply_to(&mut b);
            path(a, b).value_at(0.5, PathAxis::Y)
        };
        let eout = {
            let (mut a, mut b) = (key(0.0, 0.0, 0.0), key(1.0, 0.0, 100.0));
            EasePreset::EaseOut.apply_to(&mut a);
            EasePreset::EaseOut.apply_to(&mut b);
            path(a, b).value_at(0.5, PathAxis::Y)
        };
        assert!(ein < lin && lin < eout, "ease-in {ein} < linear {lin} < ease-out {eout}");
    }

    #[test]
    fn spatial_sampler_uses_temporal_ease() {
        // Same geometry, different temporal ease ⇒ different position at the same
        // time (the eased progress drives the Bézier parameter).
        let geo = |pre: EasePreset| {
            let (mut a, mut b) = (key(0.0, 0.0, 0.0), key(1.0, 100.0, 0.0));
            pre.apply_to(&mut a);
            pre.apply_to(&mut b);
            path(a, b).sample_position(0.5).0
        };
        let lin = geo(EasePreset::Linear);
        let ein = geo(EasePreset::EaseIn);
        assert!((lin - 50.0).abs() < 1e-2, "linear midpoint at x=50");
        assert!(ein < lin, "ease-in lags at the midpoint, {ein} < {lin}");
    }

    // ── App action layer ──────────────────────────────────────────────────────

    fn app_with_path() -> (App, usize) {
        let mut app = App::new();
        app.apply(Action::CreateMotionPath { layer_id: 0 });
        let id = app.motion_paths[0].id;
        app.apply(Action::AddMotionPathPoint { path_id: id, time_s: 0.0, x: 0.0, y: 0.0 });
        app.apply(Action::AddMotionPathPoint { path_id: id, time_s: 1.0, x: 100.0, y: 0.0 });
        (app, id)
    }

    #[test]
    fn action_sets_spatial_tangents() {
        let (mut app, id) = app_with_path();
        app.apply(Action::MotionPaths(MotionPathAction::SetSpatialTangents {
            path_id: id, index: 0, in_handle: (-1.0, -2.0), out_handle: (30.0, 40.0),
        }));
        let pt = &app.motion_paths[0].points[0];
        assert_eq!(pt.in_handle, (-1.0, -2.0));
        assert_eq!(pt.out_handle, (30.0, 40.0));
    }

    #[test]
    fn action_sets_temporal_ease() {
        let (mut app, id) = app_with_path();
        app.apply(Action::MotionPaths(MotionPathAction::SetTemporalEase {
            path_id: id, index: 1,
            ease_in: KeyframeEase { influence: 75.0, speed: 0.0 },
            ease_out: KeyframeEase { influence: 10.0, speed: 2.0 },
        }));
        let pt = &app.motion_paths[0].points[1];
        assert_eq!(pt.ease_in, KeyframeEase { influence: 75.0, speed: 0.0 });
        assert_eq!(pt.ease_out, KeyframeEase { influence: 10.0, speed: 2.0 });
    }

    #[test]
    fn action_applies_ease_preset() {
        let (mut app, id) = app_with_path();
        app.apply(Action::MotionPaths(MotionPathAction::ApplyEasePreset {
            path_id: id, index: 0, preset: EasePreset::Hold,
        }));
        assert_eq!(app.motion_paths[0].points[0].easing, MotionEasing::Hold);
        app.apply(Action::MotionPaths(MotionPathAction::ApplyEasePreset {
            path_id: id, index: 0, preset: EasePreset::EaseIn,
        }));
        let pt = &app.motion_paths[0].points[0];
        assert_eq!(pt.easing, MotionEasing::EaseIn);
        assert_eq!(pt.ease_out, KeyframeEase::flat(42.0));
    }

    #[test]
    fn action_toggles_auto_orient() {
        let (mut app, id) = app_with_path();
        assert!(!app.motion_paths[0].auto_orient);
        app.apply(Action::MotionPaths(MotionPathAction::ToggleAutoOrient { path_id: id }));
        assert!(app.motion_paths[0].auto_orient);
        app.apply(Action::MotionPaths(MotionPathAction::ToggleAutoOrient { path_id: id }));
        assert!(!app.motion_paths[0].auto_orient);
    }

    #[test]
    fn motion_path_actions_are_not_undoable() {
        // App-side state (like particles) ⇒ no project snapshot taken.
        let mut app = App::new();
        app.apply(Action::CreateMotionPath { layer_id: 0 });
        let id = app.motion_paths[0].id;
        app.apply(Action::AddMotionPathPoint { path_id: id, time_s: 0.0, x: 0.0, y: 0.0 });
        let undo_before = app.can_undo();
        app.apply(Action::MotionPaths(MotionPathAction::ToggleAutoOrient { path_id: id }));
        assert_eq!(app.can_undo(), undo_before, "graph-editor edits don't push undo history");
    }

    #[test]
    fn action_on_missing_path_is_noop() {
        let mut app = App::new();
        app.apply(Action::MotionPaths(MotionPathAction::ToggleAutoOrient { path_id: 999 }));
        assert!(app.motion_paths.is_empty());
    }
}
