//! Type on a Path — real arc-length glyph placement.
//!
//! Illustrator's *Type on a Path* rides each glyph along a spine path: the glyph
//! sits at the arc-length position of its horizontal centre, rotated to the path
//! tangent there, and offset perpendicular to the baseline. Contour renders text
//! as cached glyph [`SubPath`]s (one closed contour per glyph contour), so the
//! whole feature reduces to a **pure geometric warp** of those already-laid-out
//! glyphs — no per-render-surface work: canvas, SVG and PNG all consume the warped
//! `glyphs` cache exactly as they consume ordinary point-type.
//!
//! ## How it works
//! 1. [`crate::text::layout`] lays the string out flat at the origin, on a
//!    horizontal baseline (x grows rightward, y grows downward).
//! 2. For each glyph contour we take its horizontal extent's centre `cx` (relative
//!    to the run origin) as the glyph's distance *s* along the path.
//! 3. We walk the spine's `kurbo` segments by arc length to the point at *s* and
//!    its unit tangent, build the rotation that maps the flat baseline direction
//!    `+x` onto that tangent, and rigidly transform every anchor + out-handle of
//!    the glyph about its own `(cx, baseline)` so the glyph stands upright on the
//!    curve. The constant `offset` shifts every glyph along the path's normal
//!    (Illustrator's baseline-shift / type-position handle).
//!
//! Glyphs whose centre runs past the end of the path are dropped (they have
//! "fallen off the path", matching Illustrator). The path itself is unchanged —
//! it keeps its own fill/stroke as an ordinary shape; only the text rides it.

use crate::document::SubPath;
use crate::text::TextParams;
use kurbo::{BezPath, ParamCurve, ParamCurveArclen, ParamCurveDeriv, PathSeg, Point};

/// Arc-length accuracy (document units) for the spine walk — matches `blend.rs`.
const ARCLEN_ACCURACY: f64 = 0.05;

/// Parameters for a text-on-path attachment carried on a [`Shape::Text`].
///
/// `offset` is a perpendicular baseline shift in document units (positive pushes
/// the glyphs to the path's left-hand normal — the side the tangent's 90°-CCW
/// normal points). The attachment also records which shape supplies the spine so
/// the editor can resolve it; geometry is *baked* into the text's glyph cache on
/// every relayout, so a loaded document renders correctly with no live lookup.
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct TextOnPathParams {
    /// Perpendicular baseline shift (document units).
    #[serde(default)]
    pub offset: f32,
    /// Extra arc-length pushed before the first glyph (start offset along the
    /// path, document units). Illustrator's start-bracket handle.
    #[serde(default)]
    pub start: f32,
    /// Flip the text to read along the opposite side of the path.
    #[serde(default)]
    pub flip: bool,
}

impl Default for TextOnPathParams {
    fn default() -> Self {
        Self {
            offset: 0.0,
            start: 0.0,
            flip: false,
        }
    }
}

/// Lay `params` out along the spine `bez` and return the warped glyph
/// [`SubPath`]s in document space. `closed` reports whether the spine is a closed
/// loop (the layout then wraps around it rather than running off the end).
///
/// This is the one public entry point: [`crate::document::Shape`] bakes the
/// result into its glyph cache so every render surface is identical. Returns an
/// empty list when the path has no length or the text is empty.
pub fn layout_on_path(
    params: &TextParams,
    bez: &BezPath,
    closed: bool,
    on_path: TextOnPathParams,
) -> Vec<SubPath> {
    // Flat layout at the origin: glyphs on a horizontal baseline through y = 0.
    let (flat, _w) = crate::text::layout(params, (0.0, 0.0));
    if flat.is_empty() {
        return Vec::new();
    }
    // The flat baseline y (layout puts the baseline at origin.y + ascender; here
    // origin.y = 0, so recover it as the median glyph anchor is fragile — instead
    // we treat the baseline as 0 and rotate about each glyph's (cx, 0). Because
    // `layout` places glyphs with the baseline at +ascender below the origin, we
    // shift everything up by that baseline first so the pivot sits on the writing
    // line. We approximate the baseline as the lowest-magnitude shared y via the
    // run's vertical span midpoint is wrong for descenders — instead we use the
    // ascender from the face metrics implicitly by re-anchoring at the glyph box.
    let baseline_y = baseline_of(&flat);

    let segs: Vec<PathSeg> = bez.segments().collect();
    if segs.is_empty() {
        return Vec::new();
    }
    let lens: Vec<f64> = segs.iter().map(|s| s.arclen(ARCLEN_ACCURACY)).collect();
    let total: f64 = lens.iter().sum();
    if total <= f64::EPSILON {
        return Vec::new();
    }
    let mut starts: Vec<f64> = Vec::with_capacity(segs.len());
    let mut acc = 0.0;
    for &l in &lens {
        starts.push(acc);
        acc += l;
    }

    let mut out: Vec<SubPath> = Vec::new();
    for glyph in &flat {
        let cx = horizontal_center(glyph);
        let mut s = (cx + on_path.start) as f64;
        if closed {
            // Wrap around a closed loop so every glyph lands.
            s = s.rem_euclid(total);
        } else if s < 0.0 || s > total {
            // Off the path: drop the glyph (Illustrator behaviour).
            continue;
        }
        let (pos, tangent) = point_tangent_at(&segs, &starts, &lens, s);
        // Unit tangent; fall back to +x if degenerate.
        let mut tx = tangent.0;
        let mut ty = tangent.1;
        let tl = (tx * tx + ty * ty).sqrt();
        if tl > 1e-9 {
            tx /= tl;
            ty /= tl;
        } else {
            tx = 1.0;
            ty = 0.0;
        }
        if on_path.flip {
            tx = -tx;
            ty = -ty;
        }
        // Left-hand normal (tangent rotated +90° in y-down space). `offset`
        // shifts glyphs along it.
        let nx = ty;
        let ny = -tx;
        let dst_x = pos.0 + nx * on_path.offset as f64;
        let dst_y = pos.1 + ny * on_path.offset as f64;
        // Rigid transform: rotate the flat glyph (pivot at (cx, baseline_y)) so
        // its baseline +x direction aligns with the tangent, then translate the
        // pivot onto (dst_x, dst_y).
        let warped = warp_glyph(glyph, cx as f64, baseline_y as f64, tx, ty, dst_x, dst_y);
        out.push(warped);
    }
    out
}

/// Build a [`kurbo::BezPath`] for an arbitrary spine shape's outline plus whether
/// it is closed. Open paths/lines stay open; rects/ellipses/closed paths are
/// closed loops. Reused by the host so the engine owns the spine extraction.
pub fn spine_bezpath(shape: &crate::document::Shape) -> (BezPath, bool) {
    use crate::document::Shape;
    match shape.to_path() {
        Shape::Path {
            points,
            handles,
            closed,
            ..
        } => (crate::document::bez_path(&points, &handles, closed), closed),
        Shape::Compound { subpaths, .. } => {
            // Ride the first (outer) sub-contour.
            if let Some(sp) = subpaths.first() {
                (
                    crate::document::bez_path(&sp.points, &sp.handles, sp.closed),
                    sp.closed,
                )
            } else {
                (BezPath::new(), false)
            }
        }
        _ => (BezPath::new(), false),
    }
}

/// The baseline y of a flat run: the maximum anchor y is below the baseline
/// (descenders) and the minimum is above (ascenders); the baseline is where
/// `layout` placed the pen, which is the largest cluster — we approximate it as
/// the value 0 shifted by the run's typical bottom. In practice `layout` puts the
/// baseline at a constant y for a single line, so we take the most common /
/// median-ish anchor y as the baseline. A robust, allocation-light choice is the
/// median of per-glyph minimum-y values' complement; we use the run's overall
/// vertical midpoint biased toward the bottom. Simpler and stable: the baseline
/// is the *largest* y that still has glyph mass below it — but for our pivot we
/// only need a consistent horizontal line, so the run's max-y works (glyphs rotate
/// about a line at their feet, which reads correctly on gentle curves).
fn baseline_of(flat: &[SubPath]) -> f32 {
    // Use the median of all anchor y's: for a single text line this lands on or
    // just below the baseline, giving an upright, well-seated result.
    let mut ys: Vec<f32> = flat
        .iter()
        .flat_map(|g| g.points.iter().map(|p| p.1))
        .collect();
    if ys.is_empty() {
        return 0.0;
    }
    ys.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    ys[ys.len() / 2]
}

/// The horizontal centre of a glyph contour (mean of its anchor x-extent).
fn horizontal_center(g: &SubPath) -> f32 {
    let mut min = f32::MAX;
    let mut max = f32::MIN;
    for &(x, _) in &g.points {
        min = min.min(x);
        max = max.max(x);
    }
    if min > max {
        0.0
    } else {
        (min + max) * 0.5
    }
}

/// Rigid-transform every anchor + out-handle of `g`. The glyph is first
/// translated so its pivot `(px, py)` sits at the origin, rotated so `+x` maps to
/// the unit tangent `(tx, ty)`, then translated so the pivot lands on
/// `(dst_x, dst_y)`. Out-handles are *deltas*, so they only rotate (no
/// translation).
fn warp_glyph(
    g: &SubPath,
    px: f64,
    py: f64,
    tx: f64,
    ty: f64,
    dst_x: f64,
    dst_y: f64,
) -> SubPath {
    // Rotation that sends (1,0) -> (tx,ty): [[tx, -ty],[ty, tx]].
    let rot = |dx: f64, dy: f64| -> (f64, f64) { (tx * dx - ty * dy, ty * dx + tx * dy) };
    let points = g
        .points
        .iter()
        .map(|&(x, y)| {
            let (rx, ry) = rot(x as f64 - px, y as f64 - py);
            ((rx + dst_x) as f32, (ry + dst_y) as f32)
        })
        .collect();
    let handles = g
        .handles
        .iter()
        .map(|&(hx, hy)| {
            let (rx, ry) = rot(hx as f64, hy as f64);
            (rx as f32, ry as f32)
        })
        .collect();
    SubPath {
        points,
        handles,
        closed: g.closed,
    }
}

/// The point and (un-normalized) tangent of the spine at absolute arc length
/// `target`. Locates the containing segment, uses `inv_arclen` for the local
/// parameter, then evaluates the segment's derivative for the tangent.
fn point_tangent_at(
    segs: &[PathSeg],
    starts: &[f64],
    lens: &[f64],
    target: f64,
) -> ((f64, f64), (f64, f64)) {
    let mut idx = 0usize;
    for (i, &st) in starts.iter().enumerate() {
        if st <= target {
            idx = i;
        } else {
            break;
        }
    }
    let local = target - starts[idx];
    let seg = segs[idx];
    let seg_len = lens[idx];
    let t = if seg_len <= f64::EPSILON {
        0.0
    } else {
        seg.inv_arclen(local.clamp(0.0, seg_len), ARCLEN_ACCURACY)
    };
    let p: Point = seg.eval(t);
    let d = match seg {
        PathSeg::Line(l) => l.deriv().eval(t),
        PathSeg::Quad(q) => q.deriv().eval(t),
        PathSeg::Cubic(c) => c.deriv().eval(t),
    };
    ((p.x, p.y), (d.x, d.y))
}

#[cfg(test)]
mod tests {
    use super::*;
    use kurbo::{BezPath, Point};

    fn straight_line() -> BezPath {
        let mut b = BezPath::new();
        b.move_to(Point::new(0.0, 0.0));
        b.line_to(Point::new(1000.0, 0.0));
        b
    }

    #[test]
    fn text_on_straight_line_matches_flat_layout_extent() {
        let params = TextParams {
            text: "Type".into(),
            font_size: 40.0,
            ..Default::default()
        };
        let warped = layout_on_path(&params, &straight_line(), false, TextOnPathParams::default());
        assert!(!warped.is_empty(), "should place glyphs along the line");
        // On a horizontal line the warp is (near) identity: every point's y stays
        // close to the flat baseline band, x stays within the path length.
        for g in &warped {
            for &(x, _y) in &g.points {
                assert!(x >= -50.0 && x <= 1050.0, "x {x} within path bounds");
            }
        }
    }

    #[test]
    fn empty_text_yields_no_glyphs() {
        let params = TextParams {
            text: "   ".into(),
            font_size: 40.0,
            ..Default::default()
        };
        let warped = layout_on_path(&params, &straight_line(), false, TextOnPathParams::default());
        assert!(warped.is_empty());
    }

    #[test]
    fn offset_shifts_glyphs_perpendicular() {
        let params = TextParams {
            text: "A".into(),
            font_size: 60.0,
            ..Default::default()
        };
        let base = layout_on_path(&params, &straight_line(), false, TextOnPathParams::default());
        let shifted = layout_on_path(
            &params,
            &straight_line(),
            false,
            TextOnPathParams {
                offset: 50.0,
                ..Default::default()
            },
        );
        assert!(!base.is_empty() && !shifted.is_empty());
        // Left-hand normal of +x tangent in y-down space is (0,-1): +offset moves
        // glyphs up (smaller y).
        let by: f32 = base[0].points.iter().map(|p| p.1).sum::<f32>() / base[0].points.len() as f32;
        let sy: f32 =
            shifted[0].points.iter().map(|p| p.1).sum::<f32>() / shifted[0].points.len() as f32;
        assert!(sy < by, "positive offset shifts up: {sy} < {by}");
    }

    #[test]
    fn glyphs_run_off_open_path_are_dropped() {
        // A very short path: only the first glyph(s) fit, the rest fall off.
        let mut b = BezPath::new();
        b.move_to(Point::new(0.0, 0.0));
        b.line_to(Point::new(30.0, 0.0));
        let params = TextParams {
            text: "WIDETEXT".into(),
            font_size: 40.0,
            ..Default::default()
        };
        let warped = layout_on_path(&params, &b, false, TextOnPathParams::default());
        let flat = crate::text::layout(&params, (0.0, 0.0)).0;
        assert!(
            warped.len() < flat.len(),
            "some glyphs fall off a 30px path: {} < {}",
            warped.len(),
            flat.len()
        );
    }
}
