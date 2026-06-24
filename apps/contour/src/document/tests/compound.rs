use super::*;

// --- Compound paths ----------------------------------------------------

/// Even-odd carves the inner ring as a hole (a click in the middle misses the
/// fill); non-zero with same-wound rings keeps the middle filled.
#[test]
fn compound_fill_rule_even_odd_carves_hole() {
    let eo = donut(FillRule::EvenOdd);
    // Inside the outer ring but inside the hole (centre 15,15): even-odd → not
    // filled; on the solid frame (5,5): filled.
    assert!(!eo.hit(15.0, 15.0, 0.1), "even-odd hole is empty");
    assert!(eo.hit(5.0, 5.0, 0.1), "even-odd frame is solid");
    // Outside the outer ring entirely: not filled.
    assert!(!eo.hit(40.0, 40.0, 0.1));

    // Non-zero with both rings wound the *same* way: the inner does not subtract,
    // so the centre is filled (the classic non-zero behaviour).
    let nz = donut(FillRule::NonZero);
    assert!(nz.hit(15.0, 15.0, 0.1), "non-zero same-wound fills the centre");
}

/// `point_in_rings` matches the documented winding behaviour directly: even-odd
/// parity vs non-zero winding, with an opposite-wound inner ring carving under
/// both rules.
#[test]
fn point_in_rings_winding_rules() {
    let outer = vec![(0.0, 0.0), (30.0, 0.0), (30.0, 30.0), (0.0, 30.0)]; // CCW-ish
    let inner_same = vec![(10.0, 10.0), (20.0, 10.0), (20.0, 20.0), (10.0, 20.0)]; // same dir
    let inner_rev = vec![(10.0, 10.0), (10.0, 20.0), (20.0, 20.0), (20.0, 10.0)]; // reversed

    // Even-odd: a ring inside another always carves, regardless of direction.
    let eo = vec![outer.clone(), inner_same.clone()];
    assert!(!point_in_rings(15.0, 15.0, &eo, FillRule::EvenOdd));
    assert!(point_in_rings(5.0, 5.0, &eo, FillRule::EvenOdd));

    // Non-zero, same winding: inner does NOT subtract (filled centre).
    assert!(point_in_rings(15.0, 15.0, &eo, FillRule::NonZero));
    // Non-zero, reversed inner winding: inner DOES subtract (empty centre).
    let nz_rev = vec![outer.clone(), inner_rev];
    assert!(!point_in_rings(15.0, 15.0, &nz_rev, FillRule::NonZero));
    assert!(point_in_rings(5.0, 5.0, &nz_rev, FillRule::NonZero));
}

/// A compound path's bounds union all sub-contours; its net is the frame.
#[test]
fn compound_bounds_union_subcontours() {
    let s = donut(FillRule::EvenOdd);
    let b = s.bounds().unwrap();
    assert!((b.x - 0.0).abs() < 1e-3 && (b.y - 0.0).abs() < 1e-3);
    assert!((b.w - 30.0).abs() < 1e-3 && (b.h - 30.0).abs() < 1e-3);
}

/// Translating a compound moves every sub-contour together.
#[test]
fn compound_translate_moves_all_subcontours() {
    let mut s = donut(FillRule::EvenOdd);
    s.translate(100.0, 50.0);
    let b = s.bounds().unwrap();
    assert!((b.x - 100.0).abs() < 1e-3 && (b.y - 50.0).abs() < 1e-3);
    // The hole is still carved after the move (point in the moved centre).
    assert!(!s.hit(115.0, 65.0, 0.1), "moved hole stays empty");
    assert!(s.hit(105.0, 55.0, 0.1), "moved frame stays solid");
}

/// A compound path round-trips through serde (sub-contours + fill rule + paint),
/// and the outline polygon is its outer ring.
#[test]
fn compound_round_trips_and_outlines_outer_ring() {
    let s = donut(FillRule::EvenOdd);
    let doc = Document {
        shapes: vec![s],
        ..Default::default()
    };
    let json = serde_json::to_string(&doc).unwrap();
    let back: Document = serde_json::from_str(&json).unwrap();
    match &back.shapes[0] {
        Shape::Compound {
            subpaths,
            fill_rule,
            fill,
            ..
        } => {
            assert_eq!(subpaths.len(), 2, "outer + hole sub-contours survive");
            assert_eq!(*fill_rule, FillRule::EvenOdd);
            assert_eq!(*fill, [1.0, 0.0, 0.0, 1.0]);
        }
        other => panic!("expected a Compound, got {other:?}"),
    }
    // The outline polygon is the outer ring (its bbox is the full 30×30).
    let outline = back.shapes[0].outline_polygon().unwrap();
    let xs: Vec<f32> = outline.iter().map(|p| p.0).collect();
    let max_x = xs.iter().cloned().fold(f32::MIN, f32::max);
    assert!((max_x - 30.0).abs() < 1e-3, "outline is the outer 30×30 ring");
}

/// A pre-compound `.contour` (no `Compound` variant) loads unchanged — the new
/// variant is additive (a back-compat check that adding the variant didn't break
/// older single-ring documents).
#[test]
fn loads_pre_compound_document() {
    let json = r#"{"shapes":[
        {"Path":{"points":[[0,0],[10,0],[10,10]],"closed":true,"fill":[0,1,0,1],"stroke":[0,0,0,1],"stroke_w":1}}
    ]}"#;
    let doc: Document = serde_json::from_str(json).unwrap();
    assert_eq!(doc.shapes.len(), 1);
    assert!(matches!(doc.shapes[0], Shape::Path { .. }));
}

/// A compound `SubPath` deserializes with its `closed` defaulting to true and an
/// empty `handles` (back-compat for a minimal / hand-written compound).
#[test]
fn compound_subpath_serde_defaults() {
    let json = r#"{"Compound":{
        "subpaths":[{"points":[[0,0],[10,0],[10,10],[0,10]]}],
        "fill":[1,0,0,1],"stroke":[0,0,0,1],"stroke_w":1
    }}"#;
    let s: Shape = serde_json::from_str(json).unwrap();
    match s {
        Shape::Compound {
            subpaths,
            fill_rule,
            ..
        } => {
            assert_eq!(subpaths.len(), 1);
            assert!(subpaths[0].closed, "closed defaults to true");
            assert!(subpaths[0].handles.is_empty(), "handles default to empty");
            assert_eq!(fill_rule, FillRule::NonZero, "fill_rule defaults to non-zero");
        }
        _ => panic!("expected Compound"),
    }
}
