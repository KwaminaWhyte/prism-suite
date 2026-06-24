//! **Pattern Brush** geometry: tile a small vector *pattern unit* repeatedly
//! along a path's stroke, each instance oriented to the path tangent at evenly
//! spaced arc-length intervals — Illustrator's Pattern Brush in its simplest,
//! deterministic, fully testable form.
//!
//! The pattern unit is supplied as a set of `Shape`s in **unit space**: a
//! conventional [0, 1] × [-0.5, 0.5] box where +x runs *along* the path and the
//! origin sits on the path centreline. [`place_pattern_along_path`] walks the
//! (already flattened) path polyline, drops one transformed copy of the unit at
//! every `spacing` document units, rotating it to the local tangent and scaling
//! it by `scale`, and returns the placed copies ready to add to the document.
//!
//! Pure geometry — no document mutation, no `App` — so it unit-tests on a
//! synthetic path + tile and asserts the exact instance count and placement.

use crate::document::Shape;
use crate::transform::Affine;

/// One placement frame along the path: the centre point and a unit tangent.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placement {
    pub center: (f32, f32),
    /// Unit tangent (cos θ, sin θ) of the path at this point.
    pub tangent: (f32, f32),
    /// Arc-length distance of this placement from the path start.
    pub distance: f32,
}

/// Cumulative arc-length at each vertex of a polyline.
fn arc_lengths(poly: &[(f32, f32)]) -> Vec<f32> {
    let mut arc = Vec::with_capacity(poly.len());
    let mut acc = 0.0;
    for (i, &p) in poly.iter().enumerate() {
        if i > 0 {
            let q = poly[i - 1];
            acc += ((p.0 - q.0).powi(2) + (p.1 - q.1).powi(2)).sqrt();
        }
        arc.push(acc);
    }
    arc
}

/// Sample the point and unit tangent of `poly` at arc-length `dist`.
fn sample(poly: &[(f32, f32)], arc: &[f32], dist: f32) -> Placement {
    let n = poly.len();
    if n == 0 {
        return Placement {
            center: (0.0, 0.0),
            tangent: (1.0, 0.0),
            distance: dist,
        };
    }
    if n == 1 {
        return Placement {
            center: poly[0],
            tangent: (1.0, 0.0),
            distance: dist,
        };
    }
    let total = arc[n - 1];
    let dist = dist.clamp(0.0, total);
    // Segment containing `dist`.
    let seg = arc
        .windows(2)
        .position(|w| w[0] <= dist && dist <= w[1])
        .unwrap_or(n - 2);
    let (a, b) = (poly[seg], poly[seg + 1]);
    let seg_len = (arc[seg + 1] - arc[seg]).max(1e-9);
    let t = (dist - arc[seg]) / seg_len;
    let center = (a.0 + t * (b.0 - a.0), a.1 + t * (b.1 - a.1));
    let (tx, ty) = (b.0 - a.0, b.1 - a.1);
    let tlen = (tx * tx + ty * ty).sqrt().max(1e-9);
    Placement {
        center,
        tangent: (tx / tlen, ty / tlen),
        distance: dist,
    }
}

/// Compute the placement frames along `poly` at `spacing` intervals (document
/// units). Always places at least the start frame for a non-empty path. The
/// frame count is `floor(total_len / spacing) + 1`, so a length exactly equal to
/// `k·spacing` yields `k+1` frames (start … end inclusive).
pub fn placements_along(poly: &[(f32, f32)], spacing: f32) -> Vec<Placement> {
    if poly.len() < 2 {
        return poly
            .first()
            .map(|&p| {
                vec![Placement {
                    center: p,
                    tangent: (1.0, 0.0),
                    distance: 0.0,
                }]
            })
            .unwrap_or_default();
    }
    let arc = arc_lengths(poly);
    let total = arc[poly.len() - 1];
    let step = spacing.max(0.001);
    let count = (total / step).floor() as usize + 1;
    (0..count)
        .map(|i| sample(poly, &arc, i as f32 * step))
        .collect()
}

/// Build a placement [`Affine`] for a unit-space tile: scale uniformly by
/// `scale`, optionally flip across (mirror in y) and/or along (mirror in x) the
/// path, rotate to the tangent, then translate to the placement centre.
///
/// Order (applied right-to-left to a unit-space point): translate ∘ rotate ∘
/// flip ∘ scale.
pub fn placement_affine(p: &Placement, scale: f32, flip_across: bool, flip_along: bool) -> Affine {
    let sx = if flip_along { -scale } else { scale };
    let sy = if flip_across { -scale } else { scale };
    let scale_m = Affine::scale(sx, sy);
    let angle = p.tangent.1.atan2(p.tangent.0);
    let rot = Affine::rotate(angle);
    let trans = Affine::translate(p.center.0, p.center.1);
    // Affine::then composes self-then-other, so build translate(rotate(scale(x))).
    scale_m.then(rot).then(trans)
}

/// Place `tile` (a set of unit-space shapes) repeatedly along the flattened path
/// `poly` at `spacing` intervals, scaled by `scale`, oriented to the tangent.
/// Returns one transformed clone of every tile shape per placement, tagged into
/// `group_id` so the whole brush stroke selects/moves as one object.
///
/// `flip_across` mirrors each instance across the path; `flip_along` reverses
/// the tile's along-path direction.
#[allow(clippy::too_many_arguments)]
pub fn place_pattern_along_path(
    tile: &[Shape],
    poly: &[(f32, f32)],
    spacing: f32,
    scale: f32,
    flip_across: bool,
    flip_along: bool,
    group_id: u64,
) -> Vec<Shape> {
    let frames = placements_along(poly, spacing);
    let mut out = Vec::with_capacity(frames.len() * tile.len());
    for frame in &frames {
        let xf = placement_affine(frame, scale, flip_across, flip_along);
        for src in tile {
            let mut clone = src.clone();
            if !xf.is_identity() {
                clone.apply_affine(&xf);
            }
            clone.set_group(Some(group_id));
            out.push(clone);
        }
    }
    out
}

/// A default unit pattern tile: a small filled diamond centred on the origin,
/// spanning the [0,1] along-path box. Used when a brush has no explicit art so
/// `ApplyPatternBrushToSelected` always produces something visible.
pub fn default_tile(fill: [f32; 4]) -> Vec<Shape> {
    // Diamond in unit space: leading tip at (1,0), trailing at (0,0), top/bottom
    // at (0.5, ±0.4).
    let points = vec![(0.0, 0.0), (0.5, -0.4), (1.0, 0.0), (0.5, 0.4)];
    let handles = vec![(0.0, 0.0); points.len()];
    vec![Shape::path(points, handles, true, fill, [0.0, 0.0, 0.0, 0.0], 0.0)]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn placement_count_on_straight_line() {
        // A 100-unit horizontal line at spacing 25 ⇒ floor(100/25)+1 = 5 frames.
        let poly = vec![(0.0, 0.0), (100.0, 0.0)];
        let frames = placements_along(&poly, 25.0);
        assert_eq!(frames.len(), 5);
        assert_eq!(frames[0].center, (0.0, 0.0));
        assert!((frames[4].center.0 - 100.0).abs() < 1e-3);
        // Tangent points along +x.
        for f in &frames {
            assert!((f.tangent.0 - 1.0).abs() < 1e-3 && f.tangent.1.abs() < 1e-3);
        }
    }

    #[test]
    fn tangent_follows_a_corner() {
        // L-shape: right then down. The second half's tangent points +y.
        let poly = vec![(0.0, 0.0), (50.0, 0.0), (50.0, 50.0)];
        let frames = placements_along(&poly, 10.0);
        // Frame at distance 70 is on the vertical leg (tangent ~ (0,1)).
        let f = frames.iter().find(|f| (f.distance - 70.0).abs() < 1e-3).unwrap();
        assert!(f.tangent.0.abs() < 1e-3 && (f.tangent.1 - 1.0).abs() < 1e-3);
        assert!((f.center.0 - 50.0).abs() < 1e-3);
    }

    #[test]
    fn places_n_tiles() {
        let poly = vec![(0.0, 0.0), (100.0, 0.0)];
        let tile = default_tile([1.0, 0.0, 0.0, 1.0]);
        let shapes = place_pattern_along_path(&tile, &poly, 25.0, 10.0, false, false, 42);
        // 5 frames × 1 shape per tile = 5 placed shapes.
        assert_eq!(shapes.len(), 5);
        for s in &shapes {
            assert_eq!(s.group(), Some(42), "all placements share the group");
        }
    }

    #[test]
    fn scale_orients_tile_to_tangent() {
        // On a horizontal line the leading tip (unit (1,0)) lands `scale` units
        // ahead of the placement centre along +x.
        let poly = vec![(0.0, 0.0), (100.0, 0.0)];
        let frame = sample(&poly, &arc_lengths(&poly), 0.0);
        let xf = placement_affine(&frame, 10.0, false, false);
        let tip = xf.apply_point(1.0, 0.0);
        assert!((tip.0 - 10.0).abs() < 1e-3 && tip.1.abs() < 1e-3, "tip {:?}", tip);
    }

    #[test]
    fn flip_across_mirrors_y() {
        let poly = vec![(0.0, 0.0), (100.0, 0.0)];
        let frame = sample(&poly, &arc_lengths(&poly), 0.0);
        let xf = placement_affine(&frame, 10.0, true, false);
        // Unit (0.5, 0.4) → with flip_across the +y point lands at −y.
        let p = xf.apply_point(0.0, 0.4);
        assert!(p.1 < 0.0, "flip_across should mirror y, got {:?}", p);
    }

    #[test]
    fn single_point_path_places_once() {
        let poly = vec![(7.0, 3.0)];
        let frames = placements_along(&poly, 10.0);
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0].center, (7.0, 3.0));
    }
}
