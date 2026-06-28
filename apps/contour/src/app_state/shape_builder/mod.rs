//! The **Shape Builder** tool — Illustrator's *Shape Builder* — implemented as
//! real, deterministic, pure planar-region geometry on top of Contour's existing
//! boolean pipeline ([`crate::boolean`], powered by `i_overlay`).
//!
//! Given a set of overlapping closed shapes, [`build_regions`] computes the
//! **planar arrangement**: the distinct enclosed faces (the "atomic regions")
//! their outlines carve the plane into. Two overlapping rectangles, for example,
//! split into three atomic faces — `A − B`, `A ∩ B`, and `B − A`. Each face is a
//! [`Region`] wrapping a real [`Shape`] (a closed `Path`, or a `Compound` when a
//! face comes out a ring-with-hole), styled from the input it derives from.
//!
//! The arrangement is built **incrementally** and entirely through
//! [`crate::boolean::apply`] (the `Intersect` / `Difference` / `Union` overlay
//! rules): each new shape splits every existing face into its inside/outside
//! halves, and the part of the new shape not yet covered becomes its own face.
//! Because each step is a pure `i_overlay` overlay, the result is fully
//! deterministic — the same inputs always yield the same regions in the same
//! order.
//!
//! Interactions:
//! - **Merge** ([`merge_regions`]) — union a set of selected faces into one
//!   combined shape (the user clicks / drags across the faces to weld).
//! - **Delete** — subtract (drop) a face (Alt-click semantics), handled at the
//!   apply layer.
//! - **Hit test** ([`region_at`]) — map a document point to the face under it so
//!   the UI can pick a region.
//!
//! The apply layer ([`App::apply_shape_builder`](crate::app_state::App)) is the
//! **terminal** stage of the dispatch chain (reached from `apply_width_tool`'s
//! catch-all); it owns the chain's final `_ => {}` no-op.
//!
//! ## Known limitation
//! [`crate::boolean::apply`] consumes a shape by its single outer outline ring
//! ([`Shape::outline_polygon`]), so a face that develops a **hole** is handled
//! correctly when it is *produced* (a two-shape enclosing pair yields a real
//! `Compound` frame), but a hole is not re-subdivided by a *further* overlapping
//! shape. This matches the boolean pipeline's own single-ring contract.

use crate::boolean::{self, BoolFillRule, BoolOp};
use crate::document::{flatten, point_in_rings, FillRule, Shape, SubPath};

mod apply;
pub use apply::ShapeBuilderSession;

/// The fill rule the Shape Builder feeds to every `i_overlay` overlay — non-zero
/// winding, the default that keeps same-wound overlapping input solid.
const FILL: BoolFillRule = BoolFillRule::NonZero;

/// One **atomic face** of the planar arrangement: a single enclosed region, held
/// as a real document [`Shape`] (a closed `Path`, or a `Compound` when the face
/// has a hole / disjoint pieces). Styled from the input shape it derives from.
#[derive(Clone, Debug)]
pub struct Region {
    /// The face geometry as a closed, fillable shape.
    pub shape: Shape,
}

impl Region {
    /// Wrap a face shape into a region.
    pub fn new(shape: Shape) -> Self {
        Self { shape }
    }

    /// The net filled area of this face (outer rings minus any holes), computed
    /// from its flattened sub-contours via the shoelace formula. Winding-signed,
    /// so a hole (opposite winding to its outer ring) subtracts and disjoint
    /// pieces add.
    pub fn area(&self) -> f32 {
        let (rings, _) = shape_rings(&self.shape);
        rings.iter().map(|r| ring_signed_area(r)).sum::<f32>().abs()
    }

    /// Whether the document-space point `(x, y)` lies inside this face, honouring
    /// holes via the face shape's fill rule.
    pub fn contains(&self, x: f32, y: f32) -> bool {
        let (rings, rule) = shape_rings(&self.shape);
        point_in_rings(x, y, &rings, rule)
    }
}

/// The flattened closed rings of a shape plus the fill rule that interprets them
/// (for hit-testing / area). A `Compound` reports each sub-contour under its own
/// rule; a closed `Path` reports its one ring; everything else falls back to its
/// single outer [`outline_polygon`](Shape::outline_polygon) ring. An open path /
/// line reports no rings.
fn shape_rings(shape: &Shape) -> (Vec<Vec<(f32, f32)>>, FillRule) {
    match shape {
        Shape::Compound {
            subpaths,
            fill_rule,
            ..
        } => {
            let rings = subpaths
                .iter()
                .filter(|s| s.closed)
                .map(|s| s.flatten())
                .collect();
            (rings, *fill_rule)
        }
        Shape::Path {
            points,
            handles,
            closed: true,
            ..
        } => (vec![flatten(points, handles, true)], FillRule::NonZero),
        other => match other.outline_polygon() {
            Some(ring) => (vec![ring], FillRule::NonZero),
            None => (Vec::new(), FillRule::NonZero),
        },
    }
}

/// Signed (shoelace) area of a closed ring; its sign encodes winding direction.
fn ring_signed_area(pts: &[(f32, f32)]) -> f32 {
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

/// Build the **planar arrangement** of `shapes`: the distinct enclosed faces
/// (atomic regions) formed by their overlapping outlines.
///
/// Shapes without a fillable outline (open paths, lines) are ignored. The
/// arrangement is built incrementally: for each input shape `s`, every existing
/// face `r` is split into `r ∩ s` (inside `s`) and `r − s` (outside `s`), and the
/// part of `s` not yet covered by any face (`s − everything-so-far`) becomes a
/// new face. Two overlapping rectangles therefore yield three faces; disjoint
/// shapes yield one face each (the inputs themselves). Fully deterministic.
pub fn build_regions(shapes: &[Shape]) -> Vec<Region> {
    let inputs: Vec<&Shape> = shapes
        .iter()
        .filter(|s| s.outline_polygon().is_some())
        .collect();

    // Faces accumulated so far always partition the union of the shapes already
    // processed, so coverage is exact and no area is double-counted.
    let mut faces: Vec<Shape> = Vec::new();
    for s in inputs {
        let mut next: Vec<Shape> = Vec::with_capacity(faces.len() * 2 + 1);
        // The part of `s` not yet assigned to any face: starts whole, shrinks as
        // each existing face is subtracted out of it.
        let mut leftover: Vec<Shape> = vec![s.clone()];
        for r in &faces {
            // Split the existing face by `s`.
            next.extend(boolean::apply(r, s, BoolOp::Intersect, FILL));
            next.extend(boolean::apply(r, s, BoolOp::Difference, FILL));
            // Remove this face from the not-yet-covered part of `s`.
            leftover = leftover
                .iter()
                .flat_map(|l| boolean::apply(l, r, BoolOp::Difference, FILL))
                .collect();
        }
        next.extend(leftover);
        faces = next;
    }
    faces.into_iter().map(Region::new).collect()
}

/// **Merge**: union a set of selected faces into one combined shape. Returns the
/// welded shape (a closed `Path` when the union is connected, a `Compound` when
/// it has holes or disjoint pieces), or `None` for an empty selection.
pub fn merge_regions(selected: &[Region]) -> Option<Shape> {
    union_all(selected.iter().map(|r| r.shape.clone()).collect())
}

/// Union a batch of shapes into a single shape via repeated pairwise
/// [`boolean::apply`] unions, then collapse whatever pieces remain into one shape
/// (a `Compound` if more than one disjoint piece survives).
fn union_all(shapes: Vec<Shape>) -> Option<Shape> {
    let mut iter = shapes.into_iter();
    let first = iter.next()?;
    let mut acc: Vec<Shape> = vec![first];
    for s in iter {
        acc = union_into(acc, s);
    }
    Some(combine_into_one(acc))
}

/// Union shape `s` into the accumulator `acc` of disjoint pieces: `s` is welded
/// with every piece it overlaps (a single-shape union result), and pieces it does
/// not touch are carried through untouched. Processing sequentially lets `s`
/// bridge two previously-disjoint pieces into one.
fn union_into(acc: Vec<Shape>, s: Shape) -> Vec<Shape> {
    let mut merged = s;
    let mut out: Vec<Shape> = Vec::new();
    for piece in acc {
        let u = boolean::apply(&merged, &piece, BoolOp::Union, FILL);
        if u.len() == 1 {
            // They overlap / abut → one connected (possibly holed) region.
            merged = u.into_iter().next().unwrap();
        } else {
            // Disjoint (or degenerate): keep the piece aside.
            out.push(piece);
        }
    }
    out.push(merged);
    out
}

/// Collapse a set of (disjoint) shapes into a single shape: the lone shape if
/// there is one, otherwise a `Compound` gathering every piece's sub-contours.
fn combine_into_one(mut shapes: Vec<Shape>) -> Shape {
    if shapes.len() == 1 {
        return shapes.pop().unwrap();
    }
    let fill = shapes
        .first()
        .and_then(|s| s.fill_color())
        .unwrap_or([0.5, 0.5, 0.5, 1.0]);
    let stroke = shapes
        .first()
        .and_then(|s| s.stroke_color())
        .unwrap_or([0.0, 0.0, 0.0, 1.0]);
    let stroke_w = shapes.first().map(|s| s.stroke_width()).unwrap_or(1.0);
    let mut subpaths: Vec<SubPath> = Vec::new();
    for s in &shapes {
        subpaths.extend(shape_subpaths(s));
    }
    compound_from_subpaths(subpaths, fill, stroke, stroke_w)
}

/// A shape's editable sub-contours (for gathering into a combined compound).
fn shape_subpaths(shape: &Shape) -> Vec<SubPath> {
    match shape {
        Shape::Compound { subpaths, .. } => subpaths.clone(),
        Shape::Path {
            points,
            handles,
            closed,
            ..
        } => vec![SubPath {
            points: points.clone(),
            handles: handles.clone(),
            closed: *closed,
        }],
        other => match other.outline_polygon() {
            Some(ring) => vec![SubPath::ring(ring)],
            None => Vec::new(),
        },
    }
}

/// Build a closed [`Shape::Compound`] from sub-contours under the even-odd rule
/// (so holes always carve and disjoint pieces fill), inheriting a flat paint.
fn compound_from_subpaths(
    subpaths: Vec<SubPath>,
    fill: [f32; 4],
    stroke: [f32; 4],
    stroke_w: f32,
) -> Shape {
    Shape::Compound {
        subpaths,
        fill_rule: FillRule::EvenOdd,
        fill,
        fill_gradient: None,
        stroke,
        stroke_w,
        stroke_style: crate::document::StrokeStyle::default(),
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
    }
}

/// Hit test: the index of the first face containing `(x, y)`, or `None`. Atomic
/// faces have disjoint interiors, so an interior point hits exactly one face.
pub fn region_at(regions: &[Region], x: f32, y: f32) -> Option<usize> {
    regions.iter().position(|r| r.contains(x, y))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A solid red rectangle.
    fn rect(x: f32, y: f32, w: f32, h: f32) -> Shape {
        Shape::rect(
            [x, y, w, h],
            [1.0, 0.0, 0.0, 1.0],
            [0.0, 0.0, 0.0, 1.0],
            1.0,
        )
    }

    /// Total filled area across a set of regions.
    fn total_area(regions: &[Region]) -> f32 {
        regions.iter().map(|r| r.area()).sum()
    }

    /// The union area of two shapes via the boolean pipeline (the invariant the
    /// arrangement must conserve).
    fn union_area(a: &Shape, b: &Shape) -> f32 {
        boolean::apply(a, b, BoolOp::Union, FILL)
            .iter()
            .map(|s| Region::new(s.clone()).area())
            .sum()
    }

    // ── build_regions ────────────────────────────────────────────────────

    #[test]
    fn two_overlapping_rects_make_three_regions() {
        let a = rect(0.0, 0.0, 10.0, 10.0);
        let b = rect(5.0, 5.0, 10.0, 10.0);
        let r = build_regions(&[a, b]);
        assert_eq!(r.len(), 3, "A-only, overlap, B-only");
    }

    #[test]
    fn three_faces_tile_the_union_exactly() {
        let a = rect(0.0, 0.0, 10.0, 10.0);
        let b = rect(5.0, 5.0, 10.0, 10.0);
        let r = build_regions(&[a, b]);
        // 25 (overlap) + 75 (A−B) + 75 (B−A) = 175, no double-counting.
        assert!((total_area(&r) - 175.0).abs() < 0.5, "faces tile the union");
    }

    #[test]
    fn faces_have_the_expected_individual_areas() {
        let a = rect(0.0, 0.0, 10.0, 10.0);
        let b = rect(5.0, 5.0, 10.0, 10.0);
        let r = build_regions(&[a, b]);
        let mut areas: Vec<f32> = r.iter().map(|f| f.area()).collect();
        areas.sort_by(|x, y| x.partial_cmp(y).unwrap());
        assert!((areas[0] - 25.0).abs() < 0.5, "overlap is 5×5");
        assert!((areas[1] - 75.0).abs() < 0.5, "one crescent");
        assert!((areas[2] - 75.0).abs() < 0.5, "the other crescent");
    }

    #[test]
    fn non_overlapping_shapes_keep_inputs() {
        let a = rect(0.0, 0.0, 10.0, 10.0);
        let b = rect(100.0, 100.0, 10.0, 10.0);
        let r = build_regions(&[a, b]);
        assert_eq!(r.len(), 2, "disjoint → one face per input");
        for f in &r {
            assert!((f.area() - 100.0).abs() < 0.5, "each face keeps its input area");
        }
    }

    #[test]
    fn single_shape_is_one_region() {
        let a = rect(0.0, 0.0, 10.0, 10.0);
        let r = build_regions(&[a]);
        assert_eq!(r.len(), 1);
        assert!((r[0].area() - 100.0).abs() < 0.5);
    }

    #[test]
    fn empty_selection_yields_no_regions() {
        assert!(build_regions(&[]).is_empty());
    }

    #[test]
    fn open_paths_are_excluded() {
        let a = rect(0.0, 0.0, 10.0, 10.0);
        // An open path has no fillable outline → ignored by the arrangement.
        let open = Shape::path(
            vec![(0.0, 0.0), (20.0, 20.0)],
            Vec::new(),
            false,
            [0.0, 0.0, 0.0, 0.0],
            [0.0, 0.0, 0.0, 1.0],
            1.0,
        );
        let r = build_regions(&[a, open]);
        assert_eq!(r.len(), 1, "only the closed rect contributes a face");
    }

    #[test]
    fn fully_enclosing_pair_makes_inner_and_frame() {
        // A big rect with a small rect entirely inside it → an inner square face
        // and a frame face (a ring-with-hole compound).
        let outer = rect(0.0, 0.0, 30.0, 30.0);
        let inner = rect(10.0, 10.0, 10.0, 10.0);
        let r = build_regions(&[outer, inner]);
        assert_eq!(r.len(), 2, "inner square + frame");
        let mut areas: Vec<f32> = r.iter().map(|f| f.area()).collect();
        areas.sort_by(|x, y| x.partial_cmp(y).unwrap());
        assert!((areas[0] - 100.0).abs() < 0.5, "inner 10×10");
        assert!((areas[1] - 800.0).abs() < 0.5, "frame = 900 − 100");
        // The face containing the centre is the inner square, not the frame.
        let hit = region_at(&r, 15.0, 15.0).expect("centre hits a face");
        assert!((r[hit].area() - 100.0).abs() < 0.5, "centre lands in the inner square");
    }

    #[test]
    fn ellipse_and_rect_overlap_conserves_area() {
        let rect = rect(0.0, 0.0, 20.0, 20.0);
        let ell = Shape::ellipse(
            [10.0, 10.0, 20.0, 20.0],
            [0.0, 0.0, 1.0, 1.0],
            [0.0, 0.0, 0.0, 1.0],
            1.0,
        );
        let r = build_regions(&[rect.clone(), ell.clone()]);
        assert_eq!(r.len(), 3, "rect-only, overlap, ellipse-only");
        // The faces must tile the union of the two inputs (area-conserving).
        let expected = union_area(&rect, &ell);
        assert!(
            (total_area(&r) - expected).abs() < 1.0,
            "faces tile the union: {} vs {expected}",
            total_area(&r)
        );
    }

    #[test]
    fn build_regions_is_deterministic() {
        let a = rect(0.0, 0.0, 10.0, 10.0);
        let b = rect(5.0, 5.0, 10.0, 10.0);
        let r1 = build_regions(&[a.clone(), b.clone()]);
        let r2 = build_regions(&[a, b]);
        assert_eq!(r1.len(), r2.len());
        for (x, y) in r1.iter().zip(r2.iter()) {
            assert!((x.area() - y.area()).abs() < 1e-4, "same inputs → same faces");
        }
    }

    // ── region_at hit testing ────────────────────────────────────────────

    #[test]
    fn region_at_hits_the_correct_face() {
        let a = rect(0.0, 0.0, 10.0, 10.0);
        let b = rect(5.0, 5.0, 10.0, 10.0);
        let r = build_regions(&[a, b]);
        // (7.5, 7.5) is inside the 5×5 overlap only.
        let over = region_at(&r, 7.5, 7.5).expect("overlap face hit");
        assert!((r[over].area() - 25.0).abs() < 0.5, "picked the overlap");
        // (2.5, 2.5) is inside A only (not in B).
        let a_only = region_at(&r, 2.5, 2.5).expect("A-only face hit");
        assert!((r[a_only].area() - 75.0).abs() < 0.5, "picked A-only");
        // (12.5, 12.5) is inside B only.
        let b_only = region_at(&r, 12.5, 12.5).expect("B-only face hit");
        assert!((r[b_only].area() - 75.0).abs() < 0.5, "picked B-only");
        // The three picks are three distinct faces.
        assert_ne!(over, a_only);
        assert_ne!(over, b_only);
        assert_ne!(a_only, b_only);
    }

    #[test]
    fn region_at_outside_everything_is_none() {
        let a = rect(0.0, 0.0, 10.0, 10.0);
        let b = rect(5.0, 5.0, 10.0, 10.0);
        let r = build_regions(&[a, b]);
        assert!(region_at(&r, -50.0, -50.0).is_none(), "no face out there");
    }

    // ── merge_regions ────────────────────────────────────────────────────

    #[test]
    fn merge_all_faces_equals_the_union() {
        let a = rect(0.0, 0.0, 10.0, 10.0);
        let b = rect(5.0, 5.0, 10.0, 10.0);
        let r = build_regions(&[a.clone(), b.clone()]);
        let merged = merge_regions(&r).expect("merge of all faces");
        let area = Region::new(merged).area();
        assert!((area - 175.0).abs() < 0.5, "merged area == union area");
        assert!((area - union_area(&a, &b)).abs() < 0.5);
    }

    #[test]
    fn merge_overlap_with_a_only_rebuilds_rect_a() {
        let a = rect(0.0, 0.0, 10.0, 10.0);
        let b = rect(5.0, 5.0, 10.0, 10.0);
        let r = build_regions(&[a, b]);
        // The overlap (25) and the A-only crescent (75) re-weld into rect A (100).
        let over = region_at(&r, 7.5, 7.5).unwrap();
        let a_only = region_at(&r, 2.5, 2.5).unwrap();
        let merged = merge_regions(&[r[over].clone(), r[a_only].clone()]).unwrap();
        assert!((Region::new(merged).area() - 100.0).abs() < 0.5, "back to rect A");
    }

    #[test]
    fn merge_two_disjoint_faces_is_symmetric_difference() {
        let a = rect(0.0, 0.0, 10.0, 10.0);
        let b = rect(5.0, 5.0, 10.0, 10.0);
        let r = build_regions(&[a, b]);
        // The two crescents (A-only + B-only), without the overlap, are the
        // symmetric difference (Exclude): two disjoint pieces, area 75 + 75.
        let a_only = region_at(&r, 2.5, 2.5).unwrap();
        let b_only = region_at(&r, 12.5, 12.5).unwrap();
        let merged = merge_regions(&[r[a_only].clone(), r[b_only].clone()]).unwrap();
        assert!((Region::new(merged.clone()).area() - 150.0).abs() < 0.5, "exclude area");
        // Disjoint pieces collapse into one compound shape.
        assert!(matches!(merged, Shape::Compound { .. }), "disjoint → compound");
    }

    #[test]
    fn merge_empty_is_none() {
        assert!(merge_regions(&[]).is_none());
    }

    #[test]
    fn merge_single_face_returns_that_face() {
        let a = rect(0.0, 0.0, 10.0, 10.0);
        let r = build_regions(&[a]);
        let merged = merge_regions(&r).expect("single face merge");
        assert!((Region::new(merged).area() - 100.0).abs() < 0.5);
    }

    #[test]
    fn merged_region_is_hit_testable() {
        let a = rect(0.0, 0.0, 10.0, 10.0);
        let b = rect(5.0, 5.0, 10.0, 10.0);
        let r = build_regions(&[a, b]);
        let merged = Region::new(merge_regions(&r).unwrap());
        // The welded union covers points in every original face.
        assert!(merged.contains(2.5, 2.5), "covers A-only");
        assert!(merged.contains(7.5, 7.5), "covers overlap");
        assert!(merged.contains(12.5, 12.5), "covers B-only");
        assert!(!merged.contains(50.0, 50.0), "nothing far away");
    }
}
