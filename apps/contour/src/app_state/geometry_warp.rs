//! Pure geometry helpers for Batch 11 distortion / Live Paint features.
//!
//! Everything here is deterministic and UI-free so it can be unit-tested in
//! isolation: warp-preset mesh construction (the Illustrator *Make with Warp*
//! envelope styles), bilinear mesh deformation of a shape's editable geometry,
//! and the closed-region detection that backs Live Paint bucket fills.
//!
//! These functions take and return plain document-space geometry; the apply
//! layer (`apply_batch11.rs`) wires them onto `impl App`.

use crate::boolean::{self, BoolFillRule, BoolOp};
use crate::document::{Shape, SubPath};
use crate::envelope::EnvelopeMesh;

use super::EnvelopeWarpStyle;

/// Build a warp-preset envelope mesh over `bbox` (`[x, y, w, h]`).
///
/// The mesh is a `rows × cols` grid of control points whose *base* positions tile
/// `bbox` uniformly; each preset then displaces those base points to model the
/// classic Illustrator warp shapes (Arc, Bulge, Flag, …). `bend` is the primary
/// amount in `-1.0..=1.0` (Illustrator's −100%..+100% / 100). `horizontal`
/// chooses the bend axis. The returned [`EnvelopeMesh`] can be fed straight to
/// [`warp_shape_with_mesh`].
pub(super) fn warp_preset_mesh(
    style: EnvelopeWarpStyle,
    bbox: [f32; 4],
    bend: f32,
    horizontal: bool,
    rows: u32,
    cols: u32,
) -> EnvelopeMesh {
    let rows = rows.max(2);
    let cols = cols.max(2);
    let [x, y, w, h] = bbox;
    let bend = bend.clamp(-1.0, 1.0);
    let mut points = Vec::with_capacity((rows * cols) as usize);
    for r in 0..rows {
        for c in 0..cols {
            let u = c as f32 / (cols - 1) as f32; // 0..1 across
            let v = r as f32 / (rows - 1) as f32; // 0..1 down
            // Centred parameters in -1..1 (about the box centre).
            let cu = u * 2.0 - 1.0;
            let cv = v * 2.0 - 1.0;
            let (mut du, mut dv) = (0.0f32, 0.0f32);
            // The "primary" axis the warp acts along, and the cross axis.
            let (p, q) = if horizontal { (cu, cv) } else { (cv, cu) };
            match style {
                EnvelopeWarpStyle::None_ => {}
                // Arc: a uniform circular bend — the whole sheet bows so the
                // mid is pushed out along the cross axis, scaled by p².
                EnvelopeWarpStyle::Arc => {
                    let off = bend * (1.0 - p * p);
                    push_cross(&mut du, &mut dv, horizontal, off * h.max(w) * 0.5);
                }
                // ArcLower / ArcUpper: bend only the lower / upper half.
                EnvelopeWarpStyle::ArcLower => {
                    let half = ((q + 1.0) * 0.5).clamp(0.0, 1.0); // 0 top .. 1 bottom
                    let off = bend * (1.0 - p * p) * half;
                    push_cross(&mut du, &mut dv, horizontal, off * h.max(w) * 0.5);
                }
                EnvelopeWarpStyle::ArcUpper => {
                    let half = ((1.0 - q) * 0.5).clamp(0.0, 1.0);
                    let off = bend * (1.0 - p * p) * half;
                    push_cross(&mut du, &mut dv, horizontal, off * h.max(w) * 0.5);
                }
                // Arch: an arch whose ends stay put and middle lifts (like Arc but
                // squared falloff sharper at the centre).
                EnvelopeWarpStyle::Arch => {
                    let off = bend * (1.0 - p.abs());
                    push_cross(&mut du, &mut dv, horizontal, off * h.max(w) * 0.5);
                }
                // Bulge: push the whole cross-section out, more toward the centre,
                // symmetric on both sides of the cross axis.
                EnvelopeWarpStyle::Bulge => {
                    let mag = bend * (1.0 - p * p);
                    push_cross(&mut du, &mut dv, horizontal, mag * q * h.max(w) * 0.5);
                }
                // Shell lower / upper: a shell curve weighted to one end.
                EnvelopeWarpStyle::ShellLower => {
                    let t = (p + 1.0) * 0.5;
                    let off = bend * t * t;
                    push_cross(&mut du, &mut dv, horizontal, off * q * h.max(w) * 0.5);
                }
                EnvelopeWarpStyle::ShellUpper => {
                    let t = (1.0 - p) * 0.5;
                    let off = bend * t * t;
                    push_cross(&mut du, &mut dv, horizontal, off * q * h.max(w) * 0.5);
                }
                // Flag: a sine wave along the primary axis displacing the cross.
                EnvelopeWarpStyle::Flag => {
                    let off = bend * (p * std::f32::consts::PI).sin();
                    push_cross(&mut du, &mut dv, horizontal, off * h.max(w) * 0.5);
                }
                // Wave: a sine wave, half a period offset from Flag (cosine shape).
                EnvelopeWarpStyle::Wave => {
                    let off = bend * (p * std::f32::consts::TAU).sin();
                    push_cross(&mut du, &mut dv, horizontal, off * h.max(w) * 0.4);
                }
                // Fish: taper one end (slope the cross axis along the primary).
                EnvelopeWarpStyle::Fish => {
                    let t = (p + 1.0) * 0.5; // 0..1
                    let scale = 1.0 - bend * t;
                    dv += if horizontal { (q * (scale - 1.0)) * h * 0.5 } else { 0.0 };
                    du += if horizontal { 0.0 } else { (q * (scale - 1.0)) * w * 0.5 };
                }
                // Rise: a linear ramp displacing the cross axis along the primary.
                EnvelopeWarpStyle::Rise => {
                    let off = bend * (p + 1.0) * 0.5;
                    push_cross(&mut du, &mut dv, horizontal, off * h.max(w) * 0.5);
                }
                // Fish-eye: radial bulge from the centre (both axes pushed out).
                EnvelopeWarpStyle::FishEye => {
                    let rr = (cu * cu + cv * cv).sqrt().min(1.0);
                    let factor = bend * (1.0 - rr * rr);
                    du += cu * factor * w * 0.5;
                    dv += cv * factor * h * 0.5;
                }
                // Inflate: push every point radially out from the centre.
                EnvelopeWarpStyle::Inflate => {
                    let factor = bend * 0.5;
                    du += cu * factor * w * 0.5;
                    dv += cv * factor * h * 0.5;
                }
                // Squeeze: pinch the middle in along the cross axis.
                EnvelopeWarpStyle::Squeeze => {
                    let factor = -bend * (1.0 - p.abs());
                    push_cross(&mut du, &mut dv, horizontal, factor * q * h.max(w) * 0.5);
                }
                // Twist: rotate each ring about the centre proportionally to its
                // radius (a swirl).
                EnvelopeWarpStyle::Twist => {
                    let rr = (cu * cu + cv * cv).sqrt().min(1.0);
                    let ang = bend * rr * std::f32::consts::PI;
                    let (s, co) = ang.sin_cos();
                    let bx = cu * w * 0.5;
                    let by = cv * h * 0.5;
                    du += (bx * co - by * s) - bx;
                    dv += (bx * s + by * co) - by;
                }
            }
            let px = x + u * w + du;
            let py = y + v * h + dv;
            points.push([px, py]);
        }
    }
    EnvelopeMesh { rows, cols, points }
}

/// Add a displacement along the *cross* axis (perpendicular to the bend axis):
/// when `horizontal`, the bend runs left↔right so the cross displacement is in y;
/// otherwise it is in x.
#[inline]
fn push_cross(du: &mut f32, dv: &mut f32, horizontal: bool, amount: f32) {
    if horizontal {
        *dv += amount;
    } else {
        *du += amount;
    }
}

/// Warp every anchor (and bezier out-handle endpoint) of `shape` through the
/// bilinear `mesh`, normalising each point into the mesh's source `bbox` first.
/// Curves are preserved by warping the handle *endpoints* and recovering the new
/// offset, the same technique [`crate::perspective::warp_handles`] uses for the
/// homography. Returns `None` for shapes with no warpable geometry.
pub(super) fn warp_shape_with_mesh(
    shape: &Shape,
    bbox: [f32; 4],
    mesh: &EnvelopeMesh,
) -> Option<Shape> {
    let (bx, by, bw, bh) = (bbox[0], bbox[1], bbox[2].max(1e-6), bbox[3].max(1e-6));
    let map_pt = |x: f32, y: f32| -> (f32, f32) {
        let u = ((x - bx) / bw).clamp(0.0, 1.0);
        let v = ((y - by) / bh).clamp(0.0, 1.0);
        let [wx, wy] = mesh.warp(u, v);
        (wx, wy)
    };
    let map_contour = |points: &[(f32, f32)], handles: &[(f32, f32)]| {
        let wpts: Vec<(f32, f32)> = points.iter().map(|&(x, y)| map_pt(x, y)).collect();
        let whandles: Vec<(f32, f32)> = points
            .iter()
            .enumerate()
            .map(|(i, &(ax, ay))| {
                let (hx, hy) = handles.get(i).copied().unwrap_or((0.0, 0.0));
                if hx == 0.0 && hy == 0.0 {
                    return (0.0, 0.0);
                }
                let (ex, ey) = map_pt(ax + hx, ay + hy);
                let (wax, way) = wpts.get(i).copied().unwrap_or((ax, ay));
                (ex - wax, ey - way)
            })
            .collect();
        (wpts, whandles)
    };

    match shape {
        Shape::Path {
            points,
            handles,
            closed,
            fill,
            stroke,
            stroke_w,
            stroke_style,
            ..
        } => {
            if points.len() < 2 {
                return None;
            }
            let (wpts, whandles) = map_contour(points, handles);
            let mut out = Shape::path(wpts, whandles, *closed, *fill, *stroke, *stroke_w);
            if let Shape::Path { stroke_style: ss, .. } = &mut out {
                *ss = stroke_style.clone();
            }
            Some(out)
        }
        Shape::Compound {
            subpaths,
            fill_rule,
            fill,
            stroke,
            stroke_w,
            stroke_style,
            ..
        } => {
            let warped: Vec<SubPath> = subpaths
                .iter()
                .map(|sp| {
                    let (wp, wh) = map_contour(&sp.points, &sp.handles);
                    SubPath { points: wp, handles: wh, closed: sp.closed }
                })
                .collect();
            Some(Shape::Compound {
                subpaths: warped,
                fill_rule: *fill_rule,
                fill: *fill,
                fill_gradient: None,
                stroke: *stroke,
                stroke_w: *stroke_w,
                stroke_style: stroke_style.clone(),
                appearance: None,
                visible: true,
                group: None,
                clip: None,
                mask: false,
                omask: None,
                omask_path: false,
                omask_invert: false,
                blend: None,
                blend_step: false,
                name: None,
                locked: false,
                layer_color: None,
                envelope_mesh: None,
            })
        }
        _ => None,
    }
}

/// Detect the closed Live-Paint region under document-space point `(hx, hy)`
/// formed by the overlapping closed outlines in `shapes`, and return it as a new
/// filled [`Shape`] painted with `color`.
///
/// The region is the set of points that lie inside *exactly* the same outlines
/// the hit point does: we intersect every outline that contains the point, then
/// subtract every other outline that overlaps that intersection. The result is
/// the bounded face the user clicked, exactly like Illustrator's Live Paint
/// bucket flooding a region delimited by the surrounding paths. Returns `None`
/// when the point is in open space (inside no outline).
pub(super) fn live_paint_region(
    shapes: &[Shape],
    hx: f32,
    hy: f32,
    color: [f32; 4],
) -> Option<Shape> {
    // Polygons of every closed outline, paired with whether they contain (hx,hy).
    let mut containing: Vec<&Shape> = Vec::new();
    let mut others: Vec<&Shape> = Vec::new();
    for s in shapes {
        let Some(poly) = s.outline_polygon() else { continue };
        if point_in_polygon(hx, hy, &poly) {
            containing.push(s);
        } else {
            others.push(s);
        }
    }
    let first = *containing.first()?;
    // Start from the first containing shape's filled outline as a Path.
    let mut region = outline_as_path(first, color)?;

    // Intersect with each further containing outline (the face is the common
    // overlap of every shape the point is inside).
    for &s in containing.iter().skip(1) {
        let results = boolean::apply(&region, s, BoolOp::Intersect, BoolFillRule::NonZero);
        // Keep the result piece that still contains the hit point.
        match pick_containing(results, hx, hy) {
            Some(r) => region = restyle(r, color),
            None => return None,
        }
    }

    // Subtract every non-containing outline that cuts into the region (a path
    // crossing the face splits it; we keep the sub-face under the point).
    for &s in &others {
        let results = boolean::apply(&region, s, BoolOp::Difference, BoolFillRule::NonZero);
        if results.is_empty() {
            // The other outline fully covered the region — shouldn't normally
            // happen for a face the point is inside, but guard anyway.
            continue;
        }
        if let Some(r) = pick_containing(results, hx, hy) {
            region = restyle(r, color);
        }
    }
    Some(region)
}

/// From a batch of boolean results, the one whose filled outline still contains
/// `(hx, hy)` (the sub-face the user clicked).
fn pick_containing(results: Vec<Shape>, hx: f32, hy: f32) -> Option<Shape> {
    results.into_iter().find(|s| {
        s.outline_polygon()
            .map(|p| point_in_polygon(hx, hy, &p))
            .unwrap_or(false)
    })
}

/// Re-paint a region shape with a flat fill (Live Paint fills are solid colours).
fn restyle(mut s: Shape, color: [f32; 4]) -> Shape {
    s.set_fill_color(color);
    s.set_fill_gradient(None);
    s
}

/// A shape's filled outline as a fresh closed [`Shape::Path`] with `color`, used
/// as the Live-Paint seed region. `None` if the shape has no closed outline.
fn outline_as_path(shape: &Shape, color: [f32; 4]) -> Option<Shape> {
    let poly = shape.outline_polygon()?;
    if poly.len() < 3 {
        return None;
    }
    Some(Shape::path(
        poly,
        Vec::new(),
        true,
        color,
        [0.0, 0.0, 0.0, 0.0],
        0.0,
    ))
}

/// Even-odd point-in-polygon test on a flat ring (ray casting). Mirrors the
/// document model's private test; duplicated here so this module stays a leaf.
fn point_in_polygon(px: f32, py: f32, pts: &[(f32, f32)]) -> bool {
    let n = pts.len();
    if n < 3 {
        return false;
    }
    let mut inside = false;
    let mut j = n - 1;
    for i in 0..n {
        let (xi, yi) = pts[i];
        let (xj, yj) = pts[j];
        if (yi > py) != (yj > py) {
            let x_int = (xj - xi) * (py - yi) / (yj - yi) + xi;
            if px < x_int {
                inside = !inside;
            }
        }
        j = i;
    }
    inside
}

#[cfg(test)]
mod tests {
    use super::*;

    fn area(pts: &[(f32, f32)]) -> f32 {
        let n = pts.len();
        let mut a = 0.0;
        for i in 0..n {
            let (x0, y0) = pts[i];
            let (x1, y1) = pts[(i + 1) % n];
            a += x0 * y1 - x1 * y0;
        }
        (a * 0.5).abs()
    }

    fn rect(x: f32, y: f32, w: f32, h: f32, fill: [f32; 4]) -> Shape {
        Shape::rect([x, y, w, h], fill, [0.0, 0.0, 0.0, 1.0], 1.0)
    }

    #[test]
    fn warp_preset_none_is_identity_grid() {
        let mesh = warp_preset_mesh(EnvelopeWarpStyle::None_, [0.0, 0.0, 100.0, 100.0], 0.5, true, 4, 4);
        // None_ leaves base positions untouched; corners map to the bbox corners.
        assert_eq!(mesh.points.first(), Some(&[0.0, 0.0]));
        assert_eq!(mesh.points.last(), Some(&[100.0, 100.0]));
    }

    #[test]
    fn warp_preset_arc_bows_the_middle() {
        // A horizontal arc pushes the mid-top row off the corner baseline — the
        // sheet bows.
        let mesh = warp_preset_mesh(EnvelopeWarpStyle::Arc, [0.0, 0.0, 100.0, 100.0], 0.5, true, 3, 3);
        // Row 0 (top): centre col index 1.
        let top_centre = mesh.points[1];
        let top_left = mesh.points[0];
        assert!(
            (top_centre[1] - top_left[1]).abs() > 1.0,
            "arc displaces the centre off the corner baseline: {top_centre:?} vs {top_left:?}"
        );
    }

    #[test]
    fn warp_shape_with_mesh_moves_geometry() {
        let mesh = warp_preset_mesh(EnvelopeWarpStyle::Bulge, [0.0, 0.0, 100.0, 100.0], 0.8, true, 4, 4);
        let path = rect(0.0, 0.0, 100.0, 100.0, [1.0, 0.0, 0.0, 1.0]).to_path();
        let warped = warp_shape_with_mesh(&path, [0.0, 0.0, 100.0, 100.0], &mesh).unwrap();
        // The warped path keeps four corners.
        if let Shape::Path { points, .. } = &warped {
            assert_eq!(points.len(), 4);
        } else {
            panic!("expected a warped path");
        }
    }

    #[test]
    fn warp_preserves_corners_for_none_preset() {
        let mesh = warp_preset_mesh(EnvelopeWarpStyle::None_, [10.0, 20.0, 80.0, 60.0], 0.5, true, 3, 3);
        let path = rect(10.0, 20.0, 80.0, 60.0, [1.0, 0.0, 0.0, 1.0]).to_path();
        let warped = warp_shape_with_mesh(&path, [10.0, 20.0, 80.0, 60.0], &mesh).unwrap();
        if let Shape::Path { points, .. } = &warped {
            // None_ mesh is the identity, so corners are unchanged.
            assert!((points[0].0 - 10.0).abs() < 1e-2 && (points[0].1 - 20.0).abs() < 1e-2);
            assert!((points[2].0 - 90.0).abs() < 1e-2 && (points[2].1 - 80.0).abs() < 1e-2);
        } else {
            panic!("expected a warped path");
        }
    }

    #[test]
    fn live_paint_fills_overlap_region() {
        // Two overlapping rects: a click in the overlap fills the intersection
        // (area 25 for two 10×10 rects offset by 5).
        let a = rect(0.0, 0.0, 10.0, 10.0, [1.0, 0.0, 0.0, 1.0]);
        let b = rect(5.0, 5.0, 10.0, 10.0, [0.0, 1.0, 0.0, 1.0]);
        let shapes = vec![a, b];
        let region = live_paint_region(&shapes, 7.5, 7.5, [0.0, 0.0, 1.0, 1.0]).unwrap();
        assert_eq!(region.fill_color(), Some([0.0, 0.0, 1.0, 1.0]));
        let a = match &region {
            Shape::Path { points, .. } => area(points),
            Shape::Compound { subpaths, .. } => {
                subpaths.iter().map(|sp| area(&sp.flatten())).next().unwrap_or(0.0)
            }
            _ => panic!("expected a path/compound region"),
        };
        assert!((a - 25.0).abs() < 0.6, "overlap face area ~25, got {a}");
    }

    #[test]
    fn live_paint_fills_crescent_region() {
        // A click in the part of A not covered by B fills the crescent (area 75).
        let a = rect(0.0, 0.0, 10.0, 10.0, [1.0, 0.0, 0.0, 1.0]);
        let b = rect(5.0, 5.0, 10.0, 10.0, [0.0, 1.0, 0.0, 1.0]);
        let shapes = vec![a, b];
        let region = live_paint_region(&shapes, 2.0, 2.0, [0.3, 0.3, 0.3, 1.0]).unwrap();
        let total: f32 = match &region {
            Shape::Path { points, .. } => area(points),
            Shape::Compound { subpaths, .. } => {
                let mut areas: Vec<f32> = subpaths.iter().map(|sp| area(&sp.flatten())).collect();
                areas.sort_by(|x, y| y.partial_cmp(x).unwrap());
                let outer = areas.first().copied().unwrap_or(0.0);
                outer - areas.iter().skip(1).sum::<f32>()
            }
            _ => 0.0,
        };
        assert!((total - 75.0).abs() < 0.6, "crescent face area ~75, got {total}");
    }

    #[test]
    fn live_paint_misses_open_space() {
        let a = rect(0.0, 0.0, 10.0, 10.0, [1.0, 0.0, 0.0, 1.0]);
        let shapes = vec![a];
        // A point far outside any outline yields no region.
        assert!(live_paint_region(&shapes, 100.0, 100.0, [0.0, 0.0, 1.0, 1.0]).is_none());
    }
}
