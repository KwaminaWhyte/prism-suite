//! The **Width Tool** + variable-width stroke profiles (Illustrator's Width Tool
//! / `Stroke ▸ Width Profile`, and After Effects' tapered-stroke profiles),
//! implemented as real, deterministic, pure-geometry stroke-outlining on a path's
//! editable `(points, handles, closed)` triple — the exact model that backs
//! [`Shape::Path`](crate::document::Shape::Path) / [`SubPath`](crate::document::SubPath).
//!
//! A [`WidthProfile`] is an ordered list of [`WidthPoint`]s: each carries a
//! position `t` along the path (`0.0` = start, `1.0` = end, measured by **arc
//! length**) plus an independent `left` and `right` half-width, so a stroke can
//! be asymmetric. Between width points the half-width is **smoothstep**-blended,
//! matching the visually smooth taper Illustrator draws.
//!
//! The core is [`stroke_outline`]: flatten the centreline, sample the per-vertex
//! tangent/normal, offset each side by the interpolated half-width, and build a
//! single **closed outline contour** (left side forward + right side reversed)
//! with butt / round / square caps at the open ends. This turns a centreline +
//! width profile into a fillable contour in the same `Vec<(f32, f32)>` shape that
//! backs every other contour in the document — which is exactly what "Expand
//! Stroke" bakes into a real filled [`Shape::Path`](crate::document::Shape::Path).
//!
//! Every function here is pure (no UI / global state); the apply layer
//! ([`App::apply_width_tool`](crate::app_state::App::apply_width_tool)) wires the
//! editing + expand actions onto `impl App` and owns the dispatch chain's final
//! no-op.

use crate::document::{flatten, LineCap};

mod apply;

/// Number of straight segments a round cap's semicircle is approximated with.
/// Twelve keeps the cap visually round while staying cheap to flatten / fill.
const ROUND_CAP_SEGS: usize = 12;

/// One control point of a variable-width stroke profile: a position `t` along the
/// path (`0.0` = start … `1.0` = end, by arc length) and the half-width on the
/// `left` and `right` side of the centreline there. Independent L/R half-widths
/// let a stroke bulge more on one side (the Width Tool's Alt-drag that breaks the
/// symmetric handle).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WidthPoint {
    /// Arc-length position along the path, clamped to `0.0..=1.0`.
    pub t: f32,
    /// Half-width on the left of the centreline (≥ 0).
    pub left: f32,
    /// Half-width on the right of the centreline (≥ 0).
    pub right: f32,
}

impl WidthPoint {
    /// A width point with the given (clamped, non-negative) position and sides.
    pub fn new(t: f32, left: f32, right: f32) -> Self {
        Self {
            t: t.clamp(0.0, 1.0),
            left: left.max(0.0),
            right: right.max(0.0),
        }
    }

    /// A **symmetric** width point: equal left/right half-width `half`.
    pub fn sym(t: f32, half: f32) -> Self {
        Self::new(t, half, half)
    }

    /// Total stroke width at this point (`left + right`).
    #[allow(dead_code)] // seam: read by the (not-yet-wired) Width inspector
    pub fn total(&self) -> f32 {
        self.left + self.right
    }
}

/// A few AE / AI-style starting points for a [`WidthProfile`]. Each is built
/// about a base `half`-width so applying a preset preserves the stroke's overall
/// weight while changing how it tapers.
// Variants are constructed by the (not-yet-wired) Width Profile panel that emits
// `Action::WidthApplyPreset`; kept as seams like the `Action` enum's own.
#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WidthPreset {
    /// Constant half-width along the whole path (a plain stroke).
    Uniform,
    /// Width Profile 1: pointed at **both** ends, full in the middle (a leaf /
    /// calligraphic stroke).
    TaperBoth,
    /// Width Profile 2: thin at the **start**, full at the end.
    TaperStart,
    /// Width Profile 3: full at the start, thin at the **end**.
    TaperEnd,
    /// Bulge: half-weight at both ends, full in the middle (a gentle swell).
    BulgeMiddle,
}

#[allow(dead_code)] // seam: ALL / label feed the Width Profile dropdown
impl WidthPreset {
    /// Every preset, in panel order.
    pub const ALL: [WidthPreset; 5] = [
        WidthPreset::Uniform,
        WidthPreset::TaperBoth,
        WidthPreset::TaperStart,
        WidthPreset::TaperEnd,
        WidthPreset::BulgeMiddle,
    ];

    /// Short label for the Width Profile dropdown.
    pub fn label(self) -> &'static str {
        match self {
            WidthPreset::Uniform => "Uniform",
            WidthPreset::TaperBoth => "Width Profile 1",
            WidthPreset::TaperStart => "Width Profile 2",
            WidthPreset::TaperEnd => "Width Profile 3",
            WidthPreset::BulgeMiddle => "Bulge",
        }
    }
}

/// An ordered, non-empty list of [`WidthPoint`]s describing how a stroke's
/// half-width varies along a path. Always kept sorted by `t`; sampling between
/// points uses smoothstep so the resulting outline is visually smooth.
#[derive(Clone, Debug, PartialEq)]
pub struct WidthProfile {
    /// The width points, sorted ascending by `t`. Never empty.
    pub points: Vec<WidthPoint>,
}

impl WidthProfile {
    /// A uniform profile of constant half-width `half` (two symmetric endpoints).
    pub fn uniform(half: f32) -> Self {
        Self {
            points: vec![WidthPoint::sym(0.0, half), WidthPoint::sym(1.0, half)],
        }
    }

    /// Build the profile for `preset`, scaled to a base `half`-width.
    pub fn from_preset(preset: WidthPreset, half: f32) -> Self {
        let half = half.max(0.0);
        let sym = |rows: &[(f32, f32)]| WidthProfile {
            points: rows.iter().map(|&(t, h)| WidthPoint::sym(t, h)).collect(),
        };
        match preset {
            WidthPreset::Uniform => WidthProfile::uniform(half),
            WidthPreset::TaperBoth => sym(&[(0.0, 0.0), (0.5, half), (1.0, 0.0)]),
            WidthPreset::TaperStart => sym(&[(0.0, 0.0), (1.0, half)]),
            WidthPreset::TaperEnd => sym(&[(0.0, half), (1.0, 0.0)]),
            WidthPreset::BulgeMiddle => {
                sym(&[(0.0, half * 0.5), (0.5, half), (1.0, half * 0.5)])
            }
        }
    }

    /// Sample the `(left, right)` half-widths at arc-length position `t`. Before
    /// the first / after the last point the endpoint widths are held constant;
    /// between two points the sides are smoothstep-blended.
    pub fn sample(&self, t: f32) -> (f32, f32) {
        let pts = &self.points;
        if pts.is_empty() {
            return (0.0, 0.0);
        }
        let t = t.clamp(0.0, 1.0);
        let last = pts.len() - 1;
        if t <= pts[0].t {
            return (pts[0].left, pts[0].right);
        }
        if t >= pts[last].t {
            return (pts[last].left, pts[last].right);
        }
        for i in 0..last {
            let a = pts[i];
            let b = pts[i + 1];
            if t >= a.t && t <= b.t {
                let span = (b.t - a.t).max(1e-6);
                let u = ((t - a.t) / span).clamp(0.0, 1.0);
                let s = smoothstep01(u);
                return (
                    a.left + (b.left - a.left) * s,
                    a.right + (b.right - a.right) * s,
                );
            }
        }
        (pts[last].left, pts[last].right)
    }

    /// Insert a width point, keeping the list sorted by `t`. Returns the index it
    /// landed at.
    pub fn add_point(&mut self, t: f32, left: f32, right: f32) -> usize {
        let wp = WidthPoint::new(t, left, right);
        let idx = self
            .points
            .iter()
            .position(|p| p.t > wp.t)
            .unwrap_or(self.points.len());
        self.points.insert(idx, wp);
        idx
    }

    /// Slide width point `index` to position `t`, clamped so it never passes its
    /// neighbours (matching the Width Tool — width points keep their order).
    /// Returns `true` if a point was moved.
    pub fn move_point(&mut self, index: usize, t: f32) -> bool {
        let n = self.points.len();
        if index >= n {
            return false;
        }
        let lower = if index > 0 { self.points[index - 1].t } else { 0.0 };
        let upper = if index + 1 < n {
            self.points[index + 1].t
        } else {
            1.0
        };
        self.points[index].t = t.clamp(0.0, 1.0).clamp(lower, upper);
        true
    }

    /// Delete width point `index`. Refuses to remove the last remaining point (a
    /// profile must keep at least one). Returns `true` if a point was removed.
    pub fn delete_point(&mut self, index: usize) -> bool {
        if index >= self.points.len() || self.points.len() <= 1 {
            return false;
        }
        self.points.remove(index);
        true
    }

    /// Set the left/right half-widths of width point `index` (clamped ≥ 0).
    /// Returns `true` if the point exists.
    pub fn set_widths(&mut self, index: usize, left: f32, right: f32) -> bool {
        match self.points.get_mut(index) {
            Some(p) => {
                p.left = left.max(0.0);
                p.right = right.max(0.0);
                true
            }
            None => false,
        }
    }
}

// ─────────────────────────── small geometry helpers ────────────────────────

/// Classic Hermite smoothstep on `0..=1` (`3u² − 2u³`).
#[inline]
fn smoothstep01(u: f32) -> f32 {
    let u = u.clamp(0.0, 1.0);
    u * u * (3.0 - 2.0 * u)
}

#[inline]
fn dist(a: (f32, f32), b: (f32, f32)) -> f32 {
    (a.0 - b.0).hypot(a.1 - b.1)
}

/// Normalise a vector, falling back to `(1, 0)` for a (near) zero vector so a
/// degenerate (duplicate-point) tangent never produces a NaN.
#[inline]
fn normalize(v: (f32, f32)) -> (f32, f32) {
    let len = (v.0 * v.0 + v.1 * v.1).sqrt();
    if len <= 1e-9 {
        (1.0, 0.0)
    } else {
        (v.0 / len, v.1 / len)
    }
}

/// Drop consecutive duplicate vertices (and, for a closed ring, a closing
/// duplicate of the first point) so tangents are well defined.
fn dedup(poly: &[(f32, f32)], closed: bool) -> Vec<(f32, f32)> {
    let mut out: Vec<(f32, f32)> = Vec::with_capacity(poly.len());
    for &p in poly {
        if let Some(&last) = out.last() {
            if (p.0 - last.0).abs() < 1e-6 && (p.1 - last.1).abs() < 1e-6 {
                continue;
            }
        }
        out.push(p);
    }
    if closed && out.len() >= 2 {
        let f = out[0];
        let l = *out.last().unwrap();
        if (f.0 - l.0).abs() < 1e-6 && (f.1 - l.1).abs() < 1e-6 {
            out.pop();
        }
    }
    out
}

/// Unit tangent at vertex `i` of polyline `pts`: the centred difference of its
/// neighbours (single-sided at an open end, wrapping when closed).
fn tangent_at(pts: &[(f32, f32)], i: usize, closed: bool) -> (f32, f32) {
    let n = pts.len();
    let prev = if i > 0 {
        Some(pts[i - 1])
    } else if closed {
        Some(pts[n - 1])
    } else {
        None
    };
    let next = if i + 1 < n {
        Some(pts[i + 1])
    } else if closed {
        Some(pts[0])
    } else {
        None
    };
    let d = match (prev, next) {
        (Some(a), Some(b)) => (b.0 - a.0, b.1 - a.1),
        (None, Some(b)) => (b.0 - pts[i].0, b.1 - pts[i].1),
        (Some(a), None) => (pts[i].0 - a.0, pts[i].1 - a.1),
        (None, None) => (1.0, 0.0),
    };
    normalize(d)
}

/// Arc-length parameter (`0..=1`) of each vertex. Falls back to even spacing for
/// a zero-length (all-coincident) polyline so the parameters stay finite.
fn arc_params(pts: &[(f32, f32)], closed: bool) -> Vec<f32> {
    let n = pts.len();
    if n == 0 {
        return Vec::new();
    }
    let mut cum = vec![0.0f32; n];
    for i in 1..n {
        cum[i] = cum[i - 1] + dist(pts[i - 1], pts[i]);
    }
    let total = if closed {
        cum[n - 1] + dist(pts[n - 1], pts[0])
    } else {
        cum[n - 1]
    };
    if total <= 1e-6 {
        return (0..n)
            .map(|i| if n > 1 { i as f32 / (n - 1) as f32 } else { 0.0 })
            .collect();
    }
    cum.iter().map(|c| c / total).collect()
}

/// The per-vertex offset geometry for `pts`: the left-side points, right-side
/// points, the sampled left/right half-widths, and the unit tangents — the shared
/// core of [`stroke_sides`] and [`stroke_outline_poly`].
#[allow(clippy::type_complexity)]
fn offset_geometry(
    pts: &[(f32, f32)],
    closed: bool,
    profile: &WidthProfile,
) -> (
    Vec<(f32, f32)>,
    Vec<(f32, f32)>,
    Vec<f32>,
    Vec<f32>,
    Vec<(f32, f32)>,
) {
    let n = pts.len();
    let ts = arc_params(pts, closed);
    let mut left = Vec::with_capacity(n);
    let mut right = Vec::with_capacity(n);
    let mut wl = Vec::with_capacity(n);
    let mut wr = Vec::with_capacity(n);
    let mut tangents = Vec::with_capacity(n);
    for i in 0..n {
        let tan = tangent_at(pts, i, closed);
        // Left normal = tangent rotated +90°.
        let nl = (-tan.1, tan.0);
        let (l, r) = profile.sample(ts[i]);
        let p = pts[i];
        left.push((p.0 + nl.0 * l, p.1 + nl.1 * l));
        right.push((p.0 - nl.0 * r, p.1 - nl.1 * r));
        wl.push(l);
        wr.push(r);
        tangents.push(tan);
    }
    (left, right, wl, wr, tangents)
}

/// The two offset polylines of a stroke: the `(left, right)` side points sampled
/// from `profile` along the (already-flattened, de-duplicated) `poly`. Exposed for
/// callers / tests that want the raw sides rather than the closed outline.
#[allow(dead_code)] // seam: used by the overlay/tests, not yet by a panel
pub fn stroke_sides(
    poly: &[(f32, f32)],
    closed: bool,
    profile: &WidthProfile,
) -> (Vec<(f32, f32)>, Vec<(f32, f32)>) {
    let pts = dedup(poly, closed);
    if pts.len() < 2 {
        return (Vec::new(), Vec::new());
    }
    let (left, right, _, _, _) = offset_geometry(&pts, closed, profile);
    (left, right)
}

/// Append a cap's *intermediate* points (endpoints excluded) between `from_pt`
/// and `to_pt`, around `center`, bulging in `out_dir`. Butt adds nothing; square
/// projects both corners out by their half-widths; round sweeps a semicircle
/// whose radius blends `w_from → w_to`.
fn append_cap(
    out: &mut Vec<(f32, f32)>,
    center: (f32, f32),
    from_pt: (f32, f32),
    to_pt: (f32, f32),
    w_from: f32,
    w_to: f32,
    out_dir: (f32, f32),
    cap: LineCap,
) {
    match cap {
        LineCap::Butt => {}
        LineCap::Square => {
            out.push((
                from_pt.0 + out_dir.0 * w_from,
                from_pt.1 + out_dir.1 * w_from,
            ));
            out.push((to_pt.0 + out_dir.0 * w_to, to_pt.1 + out_dir.1 * w_to));
        }
        LineCap::Round => {
            let start_ang = (from_pt.1 - center.1).atan2(from_pt.0 - center.0);
            for j in 1..ROUND_CAP_SEGS {
                let f = j as f32 / ROUND_CAP_SEGS as f32;
                // Sweep −π (clockwise): from `from_pt`'s side, through `out_dir`
                // at the midpoint, to `to_pt`'s side — always bulging outward.
                let ang = start_ang - std::f32::consts::PI * f;
                let r = w_from + (w_to - w_from) * f;
                out.push((center.0 + ang.cos() * r, center.1 + ang.sin() * r));
            }
        }
    }
}

/// Build a single closed outline contour from an (already-flattened) centreline
/// polyline and a width `profile`. Open paths get `cap` ends; a closed path
/// yields an annulus (outer ring forward + inner ring reversed) as one contour.
/// Returns an empty vec for a degenerate (< 2 distinct point) input.
pub fn stroke_outline_poly(
    poly: &[(f32, f32)],
    closed: bool,
    profile: &WidthProfile,
    cap: LineCap,
) -> Vec<(f32, f32)> {
    let pts = dedup(poly, closed);
    let n = pts.len();
    if n < 2 {
        return Vec::new();
    }
    let (left, right, wl, wr, tangents) = offset_geometry(&pts, closed, profile);

    if closed {
        // Outer ring forward + inner ring reversed → a single annulus contour.
        let mut out = left;
        for i in (0..n).rev() {
            out.push(right[i]);
        }
        return out;
    }

    let mut out: Vec<(f32, f32)> = Vec::with_capacity(n * 2 + 2 * ROUND_CAP_SEGS);
    // Left side forward.
    out.extend_from_slice(&left);
    // End cap: left[n-1] → right[n-1], bulging along +tangent at the last vertex.
    let t_end = tangents[n - 1];
    append_cap(
        &mut out,
        pts[n - 1],
        left[n - 1],
        right[n - 1],
        wl[n - 1],
        wr[n - 1],
        t_end,
        cap,
    );
    // Right side reversed.
    for i in (0..n).rev() {
        out.push(right[i]);
    }
    // Start cap: right[0] → left[0], bulging along −tangent at the first vertex.
    let t_start = tangents[0];
    append_cap(
        &mut out,
        pts[0],
        right[0],
        left[0],
        wr[0],
        wl[0],
        (-t_start.0, -t_start.1),
        cap,
    );
    out
}

/// Flatten `(points, handles, closed)` to a polyline, then build its variable-
/// width stroke outline (see [`stroke_outline_poly`]). The headline entry point:
/// a centreline + width profile → a fillable closed contour in the same
/// `Vec<(f32, f32)>` model every other document contour uses.
pub fn stroke_outline(
    points: &[(f32, f32)],
    handles: &[(f32, f32)],
    closed: bool,
    profile: &WidthProfile,
    cap: LineCap,
) -> Vec<(f32, f32)> {
    let poly = flatten(points, handles, closed);
    stroke_outline_poly(&poly, closed, profile, cap)
}

/// Signed (shoelace) area of a closed contour; magnitude is the enclosed area.
#[allow(dead_code)] // seam: a fill/area helper used by tests and future callers
pub fn contour_area(pts: &[(f32, f32)]) -> f32 {
    let n = pts.len();
    if n < 3 {
        return 0.0;
    }
    let mut a = 0.0;
    for i in 0..n {
        let (x0, y0) = pts[i];
        let (x1, y1) = pts[(i + 1) % n];
        a += x0 * y1 - x1 * y0;
    }
    a * 0.5
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Bounding box `(min_x, min_y, max_x, max_y)` of a point set.
    fn bbox(pts: &[(f32, f32)]) -> (f32, f32, f32, f32) {
        let mut b = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
        for &(x, y) in pts {
            b.0 = b.0.min(x);
            b.1 = b.1.min(y);
            b.2 = b.2.max(x);
            b.3 = b.3.max(y);
        }
        b
    }

    /// A straight horizontal centreline of `n` evenly spaced vertices from x=0..len.
    fn horizontal(n: usize, len: f32) -> Vec<(f32, f32)> {
        (0..n)
            .map(|i| (len * i as f32 / (n - 1) as f32, 0.0))
            .collect()
    }

    // ── profile sampling ────────────────────────────────────────────────────

    #[test]
    fn uniform_profile_samples_constant() {
        let p = WidthProfile::uniform(4.0);
        for &t in &[0.0, 0.25, 0.5, 0.75, 1.0] {
            assert_eq!(p.sample(t), (4.0, 4.0), "uniform is flat at t={t}");
        }
    }

    #[test]
    fn sample_clamps_outside_range() {
        let mut p = WidthProfile::uniform(2.0);
        p.points[0].t = 0.2;
        p.points[1].t = 0.8;
        assert_eq!(p.sample(0.0), (2.0, 2.0), "before first holds endpoint");
        assert_eq!(p.sample(1.0), (2.0, 2.0), "after last holds endpoint");
    }

    #[test]
    fn smoothstep_blends_midpoint() {
        // Two symmetric points 0 → 10; the midpoint blends to 5 (smoothstep(0.5)=0.5).
        let p = WidthProfile {
            points: vec![WidthPoint::sym(0.0, 0.0), WidthPoint::sym(1.0, 10.0)],
        };
        let (l, r) = p.sample(0.5);
        assert!((l - 5.0).abs() < 1e-4 && (r - 5.0).abs() < 1e-4, "mid={l}");
    }

    #[test]
    fn taper_profile_narrows_toward_end() {
        let p = WidthProfile::from_preset(WidthPreset::TaperEnd, 6.0);
        let start = p.sample(0.05).0 + p.sample(0.05).1;
        let end = p.sample(0.95).0 + p.sample(0.95).1;
        assert!(start > end, "taper-end is wider at start ({start}) than end ({end})");
        assert!(end < 1.0, "taper-end nearly vanishes ({end})");
    }

    // ── profile editing ─────────────────────────────────────────────────────

    #[test]
    fn add_point_keeps_sorted() {
        let mut p = WidthProfile::uniform(3.0);
        let idx = p.add_point(0.5, 8.0, 2.0);
        assert_eq!(idx, 1, "0.5 lands between the two endpoints");
        assert_eq!(p.points.len(), 3);
        assert!(p.points.windows(2).all(|w| w[0].t <= w[1].t), "still sorted");
        assert_eq!((p.points[1].left, p.points[1].right), (8.0, 2.0));
    }

    #[test]
    fn move_point_clamps_between_neighbours() {
        let mut p = WidthProfile::uniform(3.0);
        p.add_point(0.5, 5.0, 5.0); // index 1, between 0.0 and 1.0
        assert!(p.move_point(1, 2.0)); // try to overshoot past the last endpoint
        assert!(p.points[1].t <= p.points[2].t, "cannot pass its right neighbour");
        assert!(p.move_point(1, -1.0));
        assert!(p.points[1].t >= p.points[0].t, "cannot pass its left neighbour");
    }

    #[test]
    fn delete_point_refuses_last() {
        let mut p = WidthProfile {
            points: vec![WidthPoint::sym(0.5, 3.0)],
        };
        assert!(!p.delete_point(0), "won't delete the only point");
        let mut q = WidthProfile::uniform(3.0);
        assert!(q.delete_point(0));
        assert_eq!(q.points.len(), 1);
    }

    #[test]
    fn set_widths_clamps_non_negative() {
        let mut p = WidthProfile::uniform(3.0);
        assert!(p.set_widths(0, 9.0, -4.0));
        assert_eq!((p.points[0].left, p.points[0].right), (9.0, 0.0));
        assert!(!p.set_widths(99, 1.0, 1.0), "out-of-range index");
    }

    // ── stroke outline geometry ─────────────────────────────────────────────

    #[test]
    fn uniform_outline_is_two_half_widths_wide_everywhere() {
        let poly = horizontal(5, 100.0);
        let prof = WidthProfile::uniform(3.0);
        let (left, right) = stroke_sides(&poly, false, &prof);
        assert_eq!(left.len(), 5);
        for i in 0..5 {
            // Horizontal line: left at y=+3, right at y=−3 → total width 6 = 2×half.
            assert!((left[i].1 - 3.0).abs() < 1e-4, "left y[{i}]={}", left[i].1);
            assert!((right[i].1 + 3.0).abs() < 1e-4, "right y[{i}]={}", right[i].1);
            assert!((left[i].1 - right[i].1 - 6.0).abs() < 1e-4, "2× half-width");
        }
    }

    #[test]
    fn butt_outline_bbox_height_is_full_width() {
        let poly = horizontal(4, 80.0);
        let prof = WidthProfile::uniform(5.0);
        let out = stroke_outline_poly(&poly, false, &prof, LineCap::Butt);
        let b = bbox(&out);
        assert!((b.3 - b.1 - 10.0).abs() < 1e-3, "height ≈ 2×5 = 10, got {}", b.3 - b.1);
        // Butt cap adds no x beyond the centreline span.
        assert!((b.0).abs() < 1e-3 && (b.2 - 80.0).abs() < 1e-3, "x span unchanged");
    }

    #[test]
    fn tapered_outline_narrows_toward_end() {
        let poly = horizontal(11, 100.0);
        let prof = WidthProfile::from_preset(WidthPreset::TaperEnd, 8.0);
        let (left, right) = stroke_sides(&poly, false, &prof);
        let w_start = left[0].1 - right[0].1;
        let w_end = left[10].1 - right[10].1;
        assert!(w_start > w_end + 5.0, "start {w_start} much wider than end {w_end}");
        assert!(w_end.abs() < 1e-3, "taper reaches ~zero width at the end");
    }

    #[test]
    fn asymmetric_widths_offset_each_side_independently() {
        let poly = horizontal(2, 50.0);
        let prof = WidthProfile {
            points: vec![WidthPoint::new(0.0, 6.0, 2.0), WidthPoint::new(1.0, 6.0, 2.0)],
        };
        let (left, right) = stroke_sides(&poly, false, &prof);
        // +x travel → left normal = (0,1): left at +6, right at −2.
        assert!((left[0].1 - 6.0).abs() < 1e-4, "left y={}", left[0].1);
        assert!((right[0].1 + 2.0).abs() < 1e-4, "right y={}", right[0].1);
    }

    #[test]
    fn outline_is_closed_and_has_positive_area() {
        let poly = horizontal(6, 120.0);
        let prof = WidthProfile::uniform(4.0);
        let out = stroke_outline_poly(&poly, false, &prof, LineCap::Butt);
        assert!(out.len() >= 4, "a real ring of vertices");
        // First and last vertices differ (the closing edge is implicit).
        assert!(out[0] != out[out.len() - 1], "no degenerate closing duplicate");
        assert!(contour_area(&out).abs() > 100.0, "encloses real area");
    }

    #[test]
    fn round_cap_extends_past_the_endpoints() {
        let poly = horizontal(2, 40.0);
        let prof = WidthProfile::uniform(5.0);
        let butt = stroke_outline_poly(&poly, false, &prof, LineCap::Butt);
        let round = stroke_outline_poly(&poly, false, &prof, LineCap::Round);
        assert!(round.len() > butt.len(), "round cap adds arc vertices");
        let bb = bbox(&butt);
        let br = bbox(&round);
        // Round caps bulge ~half-width beyond each end in x.
        assert!(br.0 < bb.0 - 4.0, "round bulges before the start ({} < {})", br.0, bb.0);
        assert!(br.2 > bb.2 + 4.0, "round bulges past the end ({} > {})", br.2, bb.2);
    }

    #[test]
    fn square_cap_projects_by_half_width() {
        let poly = horizontal(2, 30.0);
        let prof = WidthProfile::uniform(5.0);
        let sq = stroke_outline_poly(&poly, false, &prof, LineCap::Square);
        let b = bbox(&sq);
        // Square cap projects half-width (5) past each end: x from −5 to 35.
        assert!((b.0 + 5.0).abs() < 1e-3, "start projected to −5, got {}", b.0);
        assert!((b.2 - 35.0).abs() < 1e-3, "end projected to 35, got {}", b.2);
    }

    #[test]
    fn zero_width_profile_is_degenerate_but_safe() {
        let poly = horizontal(4, 50.0);
        let prof = WidthProfile::uniform(0.0);
        let out = stroke_outline_poly(&poly, false, &prof, LineCap::Round);
        assert!(out.iter().all(|p| p.0.is_finite() && p.1.is_finite()), "no NaNs");
        // All offsets collapse onto the centreline → ~zero enclosed area.
        assert!(contour_area(&out).abs() < 1e-2, "zero width → ~zero area");
    }

    #[test]
    fn closed_path_outline_is_an_annulus() {
        // A unit-ish square centreline (closed).
        let poly = vec![(0.0, 0.0), (100.0, 0.0), (100.0, 100.0), (0.0, 100.0)];
        let prof = WidthProfile::uniform(5.0);
        let out = stroke_outline_poly(&poly, true, &prof, LineCap::Butt);
        // Outer ring (4) + inner ring (4) → 8 vertices, no caps.
        assert_eq!(out.len(), 8, "closed → outer + inner ring");
        let b = bbox(&out);
        // Outer edge sits ~5 beyond the centreline bbox on every side.
        assert!(b.0 < -2.0 && b.1 < -2.0, "outer ring beyond the centreline");
        assert!(b.2 > 102.0 && b.3 > 102.0, "outer ring beyond the centreline");
    }

    #[test]
    fn stroke_outline_flattens_curved_handles() {
        // A curved open segment: a non-zero handle forces flattening to >2 points.
        let points = vec![(0.0, 0.0), (100.0, 0.0)];
        let handles = vec![(0.0, 40.0), (0.0, 40.0)];
        let prof = WidthProfile::uniform(3.0);
        let out = stroke_outline(&points, &handles, false, &prof, LineCap::Butt);
        assert!(out.len() > 6, "curve flattened into many outline vertices");
        assert!(contour_area(&out).abs() > 50.0, "curved stroke encloses area");
    }

    #[test]
    fn from_preset_uniform_matches_uniform() {
        assert_eq!(
            WidthProfile::from_preset(WidthPreset::Uniform, 4.0),
            WidthProfile::uniform(4.0)
        );
    }

    #[test]
    fn taper_both_is_pointed_at_both_ends() {
        let p = WidthProfile::from_preset(WidthPreset::TaperBoth, 5.0);
        assert!(p.sample(0.0).0 < 1e-4, "pointed at start");
        assert!(p.sample(1.0).0 < 1e-4, "pointed at end");
        assert!(p.sample(0.5).0 > 4.0, "full in the middle");
    }
}
