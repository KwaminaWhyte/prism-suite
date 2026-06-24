use super::*;

// --- Masks --------------------------------------------------------------

#[test]
fn point_in_polygon_square() {
    // Unit square centered at origin.
    let sq = [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)];
    assert!(point_in_polygon(&sq, 0.0, 0.0)); // center inside
    assert!(point_in_polygon(&sq, 0.9, -0.9)); // near a corner, inside
    assert!(!point_in_polygon(&sq, 2.0, 0.0)); // right of the square
    assert!(!point_in_polygon(&sq, 0.0, -5.0)); // below
                                                // Degenerate polygons are never "inside".
    assert!(!point_in_polygon(&[(0.0, 0.0), (1.0, 0.0)], 0.5, 0.0));
}

#[test]
fn point_in_polygon_concave() {
    // An arrow/chevron concave shape: a notch cut into the right side.
    let poly = [(0.0, 0.0), (4.0, 0.0), (2.0, 2.0), (4.0, 4.0), (0.0, 4.0)];
    assert!(point_in_polygon(&poly, 1.0, 2.0)); // left bulk: inside
                                                // A point inside the notch (right of the chevron tip) is outside.
    assert!(!point_in_polygon(&poly, 3.5, 2.0));
}

#[test]
fn dist_to_polygon_is_zero_on_edge_and_grows_outside() {
    let sq = [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)];
    // On the right edge -> ~0 distance to boundary.
    assert!(dist_to_polygon(&sq, 1.0, 0.0) < 1e-4);
    // 1 unit right of the edge -> distance ~1.
    assert!((dist_to_polygon(&sq, 2.0, 0.0) - 1.0).abs() < 1e-4);
    // Inside, 1 unit from the nearest (right) edge -> distance ~1.
    assert!((dist_to_polygon(&sq, 0.0, 0.0) - 1.0).abs() < 1e-4);
}

#[test]
fn mask_rect_hard_coverage_is_binary() {
    let m = Mask::rect(10.0, 10.0);
    let poly = m.flatten();
    assert_eq!(poly.len(), 4); // four straight segments -> four points
    assert!((m.coverage_at(&poly, 0.0, 0.0) - 1.0).abs() < 1e-5); // inside
    assert_eq!(m.coverage_at(&poly, 50.0, 0.0), 0.0); // outside
}

#[test]
fn mask_feather_ramps_across_the_edge() {
    let mut m = Mask::rect(10.0, 10.0);
    m.feather = 4.0; // ramp over ±2 px around the edge
    let poly = m.flatten();
    // Exactly on the right edge -> half coverage.
    let on_edge = m.coverage_at(&poly, 10.0, 0.0);
    assert!((on_edge - 0.5).abs() < 1e-4, "edge cov {on_edge}");
    // Well inside -> full; well outside -> none.
    assert!((m.coverage_at(&poly, 0.0, 0.0) - 1.0).abs() < 1e-5);
    assert_eq!(m.coverage_at(&poly, 20.0, 0.0), 0.0);
}

#[test]
fn mask_inversion_complements_coverage() {
    let mut m = Mask::rect(10.0, 10.0);
    m.inverted = true;
    let poly = m.flatten();
    assert_eq!(m.coverage_at(&poly, 0.0, 0.0), 0.0); // inside -> hidden
    assert!((m.coverage_at(&poly, 50.0, 0.0) - 1.0).abs() < 1e-5); // outside -> shown
}

#[test]
fn mask_expansion_grows_and_shrinks() {
    let m_base = Mask::rect(10.0, 10.0);
    let poly = m_base.flatten();
    // A point 5 px outside the right edge is normally uncovered...
    assert_eq!(m_base.coverage_at(&poly, 15.0, 0.0), 0.0);
    // ...but +8 px expansion pulls the boundary out past it.
    let mut grown = m_base.clone();
    grown.expansion = 8.0;
    assert!((grown.coverage_at(&poly, 15.0, 0.0) - 1.0).abs() < 1e-5);
    // Negative expansion contracts: a point just inside is knocked out.
    let mut shrunk = m_base.clone();
    shrunk.expansion = -8.0;
    assert_eq!(shrunk.coverage_at(&poly, 5.0, 0.0), 0.0);
}

#[test]
fn mask_opacity_scales_coverage() {
    let mut m = Mask::rect(10.0, 10.0);
    m.opacity = 0.5;
    let poly = m.flatten();
    assert!((m.coverage_at(&poly, 0.0, 0.0) - 0.5).abs() < 1e-5);
}

#[test]
fn mask_ellipse_is_smooth_and_inside_out() {
    let m = Mask::ellipse(10.0, 10.0);
    let poly = m.flatten();
    // Flattening a 4-segment Bézier oval yields many points.
    assert!(poly.len() > 16);
    // Center inside; a point on the bounding-box corner (outside the oval)
    // is uncovered.
    assert!((m.coverage_at(&poly, 0.0, 0.0) - 1.0).abs() < 1e-5);
    assert_eq!(m.coverage_at(&poly, 9.5, 9.5), 0.0);
    // A point near the right vertex (on-axis) is inside.
    assert!(m.coverage_at(&poly, 8.0, 0.0) > 0.5);
}

#[test]
fn mask_modes_combine_as_expected() {
    // Add unions; against an empty base it reveals exactly the shape.
    assert!((MaskMode::Add.combine(0.0, 1.0) - 1.0).abs() < 1e-6);
    assert!((MaskMode::Add.combine(0.5, 1.0) - 1.0).abs() < 1e-6);
    // Subtract knocks out.
    assert!((MaskMode::Subtract.combine(1.0, 1.0)).abs() < 1e-6);
    assert!((MaskMode::Subtract.combine(1.0, 0.0) - 1.0).abs() < 1e-6);
    // Intersect keeps the overlap.
    assert!((MaskMode::Intersect.combine(1.0, 1.0) - 1.0).abs() < 1e-6);
    assert!((MaskMode::Intersect.combine(1.0, 0.0)).abs() < 1e-6);
    // Difference is the symmetric difference.
    assert!((MaskMode::Difference.combine(1.0, 1.0)).abs() < 1e-6);
    assert!((MaskMode::Difference.combine(1.0, 0.0) - 1.0).abs() < 1e-6);
    // None passes the accumulator through untouched.
    assert!((MaskMode::None.combine(0.7, 1.0) - 0.7).abs() < 1e-6);
}

#[test]
fn mask_stack_no_active_masks_is_full_coverage() {
    // No masks -> unmasked layer (full coverage sentinel).
    assert_eq!(mask_stack_coverage(&[], &[], 0.0, 0.0), 1.0);
    // A single disabled (None) mask is still "no active masks".
    let mut m = Mask::rect(10.0, 10.0);
    m.mode = MaskMode::None;
    let polys = vec![m.flatten()];
    assert_eq!(mask_stack_coverage(&[m], &polys, 0.0, 0.0), 1.0);
}

#[test]
fn mask_stack_add_then_subtract() {
    // A big Add rectangle with a smaller Subtract rectangle punched out.
    let add = Mask::rect(20.0, 20.0);
    let mut sub = Mask::rect(5.0, 5.0);
    sub.mode = MaskMode::Subtract;
    let masks = vec![add, sub];
    let polys: Vec<_> = masks.iter().map(Mask::flatten).collect();
    // Inside the big rect but outside the hole -> covered.
    assert!((mask_stack_coverage(&masks, &polys, 12.0, 0.0) - 1.0).abs() < 1e-5);
    // Inside the punched hole -> knocked out.
    assert_eq!(mask_stack_coverage(&masks, &polys, 0.0, 0.0), 0.0);
    // Fully outside everything -> uncovered.
    assert_eq!(mask_stack_coverage(&masks, &polys, 50.0, 0.0), 0.0);
}

#[test]
fn mask_is_active_needs_three_verts_and_a_mode() {
    let mut m = Mask::rect(10.0, 10.0);
    assert!(m.is_active());
    m.mode = MaskMode::None;
    assert!(!m.is_active());
    m.mode = MaskMode::Add;
    m.vertices.truncate(2); // only 2 verts -> no area
    assert!(!m.is_active());
}

#[test]
fn masks_serde_defaults_to_empty() {
    // Pre-mask layers (no `masks` field) load unmasked.
    let json = r#"{"name":"L","color":[1.0,1.0,1.0,1.0],"visible":true,
        "x":{"keys":[]},"y":{"keys":[]},"scale":{"keys":[]},
        "rotation":{"keys":[]},"opacity":{"keys":[]}}"#;
    let layer: PulseLayer = serde_json::from_str(json).unwrap();
    assert!(layer.masks.is_empty());
    assert!(!layer.has_active_masks());
}
