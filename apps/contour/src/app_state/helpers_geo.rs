use crate::document::{Document, Shape};
use prism_core::geometry::Rect as CoreRect;

pub(super) fn rand_group_id(_doc: &crate::document::Document) -> u64 {
    use std::time::SystemTime;
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(12345)
}

// --- Batch 8: serde default helpers ---
pub(super) fn default_image_trace_threshold() -> f32 { 128.0 }
pub(super) fn default_image_trace_colors() -> u8 { 6 }
pub(super) fn default_omask_id_counter() -> u64 { 1 }
pub(super) fn default_graph_style_fill() -> [f32; 4] { [0.2, 0.5, 0.9, 1.0] }

/// Whether two straight-sRGB RGBA colours are close enough to be considered the
/// same fill for the Recolor panel (tolerance 1/255 ≈ 0.004 per channel).
pub(super) fn colors_approx_equal(a: [f32; 4], b: [f32; 4]) -> bool {
    a.iter().zip(b.iter()).all(|(x, y)| (x - y).abs() < 0.01)
}

/// Expand or contract a closed polygon ring by `distance` document units using
/// the averaged-normal (Minkwoski-sum approximation) method. Each vertex is
/// displaced outward (positive) or inward (negative) along the averaged
/// unit normal of its two adjacent edges. Winding order is detected via signed
/// area so normals always point outward regardless of CW / CCW orientation.
pub(super) fn offset_polygon(points: &[(f32, f32)], distance: f32) -> Vec<(f32, f32)> {
    let n = points.len();
    if n < 3 {
        return points.to_vec();
    }
    // Signed area (shoelace): positive = CCW in math space (+y up);
    // negative = CW in math space = CW in screen space (+y down) which is what
    // Contour's rect / to_path() produces.
    let signed_area: f32 = (0..n)
        .map(|i| {
            let j = (i + 1) % n;
            points[i].0 * points[j].1 - points[j].0 * points[i].1
        })
        .sum::<f32>()
        * 0.5;
    // In screen space (+y down), a CW-wound polygon has positive signed area.
    // The outward normal is the right-side normal (ey, −ex) for CW and the
    // left-side (−ey, ex) for CCW. We pick `sign` so that `sign * (−ey, ex)`
    // always points outward: −1 for CW (positive area), +1 for CCW.
    let sign = if signed_area > 0.0 { -1.0_f32 } else { 1.0_f32 };
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let prev = points[(i + n - 1) % n];
        let cur  = points[i];
        let next = points[(i + 1) % n];
        // Edge vectors (cur − prev and next − cur).
        let (ex0, ey0) = (cur.0 - prev.0, cur.1 - prev.1);
        let (ex1, ey1) = (next.0 - cur.0, next.1 - cur.1);
        // Left-side normals (−ey, ex); multiplied by sign → outward.
        let len0 = (ex0 * ex0 + ey0 * ey0).sqrt().max(1e-9);
        let len1 = (ex1 * ex1 + ey1 * ey1).sqrt().max(1e-9);
        let n0 = (sign * -ey0 / len0, sign * ex0 / len0);
        let n1 = (sign * -ey1 / len1, sign * ex1 / len1);
        // Average outward normal, renormalized.
        let nx = (n0.0 + n1.0) * 0.5;
        let ny = (n0.1 + n1.1) * 0.5;
        let nlen = (nx * nx + ny * ny).sqrt().max(1e-9);
        out.push((cur.0 + nx / nlen * distance, cur.1 + ny / nlen * distance));
    }
    out
}

/// Interpolate a position along a polyline given arc-length distances at each
/// vertex. Returns the linearly interpolated point at arc-length `dist`.
pub(super) fn sample_polyline(path: &[[f32; 2]], arc: &[f32], dist: f32) -> (f32, f32) {
    let n = path.len();
    if n == 0 {
        return (0.0, 0.0);
    }
    if n == 1 {
        return (path[0][0], path[0][1]);
    }
    // Binary search for the segment containing `dist`.
    let dist = dist.clamp(0.0, arc[n - 1]);
    let seg = arc
        .windows(2)
        .position(|w| w[0] <= dist && dist <= w[1])
        .unwrap_or(n - 2);
    let seg_len = arc[seg + 1] - arc[seg];
    if seg_len < 1e-9 {
        return (path[seg][0], path[seg][1]);
    }
    let t = (dist - arc[seg]) / seg_len;
    let (ax, ay) = (path[seg][0], path[seg][1]);
    let (bx, by) = (path[seg + 1][0], path[seg + 1][1]);
    (ax + t * (bx - ax), ay + t * (by - ay))
}

/// Warp a `Shape` (already reduced via [`Shape::to_path`]) through the perspective
/// homography defined by `corners` relative to `bbox`. Pushes every anchor and
/// bezier out-handle of each contour through the projective map (preserving curves),
/// returning the distorted `Path` / `Compound`. `None` if the shape has no warpable
/// geometry. Used by `Action::ApplyPerspectiveDistort`.
pub(super) fn warp_shape_perspective(
    path: &Shape,
    bbox: [f32; 4],
    corners: &[[f32; 2]; 4],
) -> Option<Shape> {
    use crate::document::{Shape as S, SubPath};
    use crate::perspective::{warp_handles, warp_points};
    match path {
        S::Path {
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
            let wpts = warp_points(points, bbox, corners);
            let whandles = warp_handles(points, handles, &wpts, bbox, corners);
            let mut shape =
                Shape::path(wpts, whandles, *closed, *fill, *stroke, *stroke_w);
            if let S::Path { stroke_style: ss, .. } = &mut shape {
                *ss = stroke_style.clone();
            }
            Some(shape)
        }
        S::Compound {
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
                    let wp = warp_points(&sp.points, bbox, corners);
                    let wh = warp_handles(&sp.points, &sp.handles, &wp, bbox, corners);
                    SubPath {
                        points: wp,
                        handles: wh,
                        closed: sp.closed,
                    }
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

/// Whether a shape's axis-aligned bounds intersect the marquee rectangle (used
/// by `MarqueeSelect`). A shape with no finite bounds (an empty path) never hits.
pub(super) fn bounds_intersect(shape: &Shape, marquee: &CoreRect) -> bool {
    let Some(b) = shape.bounds() else { return false };
    b.x < marquee.x + marquee.w
        && b.x + b.w > marquee.x
        && b.y < marquee.y + marquee.h
        && b.y + b.h > marquee.y
}

/// A small starter document (a few overlapping shapes on the default artboard) so
/// the GPUI preview and the Layers panel have real content to show. The egui app
/// opens with an empty document; this host seeds one purely so the migration
/// skeleton is visible end-to-end.
pub(super) fn sample_document() -> Document {
    let mut doc = Document::new();
    // Colors are straight sRGB RGBA in 0..1 (Contour's document convention).
    doc.shapes.push(Shape::rect(
        [120.0, 120.0, 420.0, 300.0],
        [0.20, 0.55, 0.90, 1.0], // blue fill
        [0.10, 0.20, 0.35, 1.0],
        6.0,
    ));
    doc.shapes.push(Shape::ellipse(
        [360.0, 240.0, 380.0, 320.0],
        [0.95, 0.45, 0.25, 0.85], // orange, semi-transparent
        [0.40, 0.15, 0.05, 1.0],
        4.0,
    ));
    doc.shapes.push(Shape::rect(
        [220.0, 340.0, 260.0, 200.0],
        [0.30, 0.80, 0.45, 0.90], // green
        [0.10, 0.30, 0.18, 1.0],
        3.0,
    ));
    doc
}

