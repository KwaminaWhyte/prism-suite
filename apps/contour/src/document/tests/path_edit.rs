use super::*;

/// A path with no handles flattens to its raw points (polyline).
#[test]
fn flatten_polyline_is_identity() {
    let pts = vec![(0.0, 0.0), (10.0, 0.0), (10.0, 10.0)];
    let out = flatten(&pts, &[], false);
    assert_eq!(out, pts);
}

/// A path with a non-zero handle flattens into more segments (a curve).
#[test]
fn flatten_curve_subdivides() {
    let pts = vec![(0.0, 0.0), (100.0, 0.0)];
    let handles = vec![(0.0, 50.0), (0.0, 50.0)];
    let out = flatten(&pts, &handles, false);
    assert!(out.len() > 2, "curve should subdivide, got {}", out.len());
}

// --- Direct-select path editing ----------------------------------------

#[test]
fn segment_count_open_vs_closed() {
    assert_eq!(segment_count(0, false), 0);
    assert_eq!(segment_count(0, true), 0);
    assert_eq!(segment_count(3, false), 2);
    assert_eq!(segment_count(3, true), 3);
}

#[test]
fn nearest_segment_picks_closest() {
    // Square-ish open path; click near the middle of the first segment.
    let pts = vec![(0.0, 0.0), (100.0, 0.0), (100.0, 100.0)];
    let (seg, t) = nearest_segment(&pts, false, 50.0, 1.0, 5.0).expect("hit");
    assert_eq!(seg, 0);
    assert!((t - 0.5).abs() < 1e-3, "t={t}");
}

#[test]
fn nearest_segment_misses_when_far() {
    let pts = vec![(0.0, 0.0), (100.0, 0.0)];
    assert!(nearest_segment(&pts, false, 50.0, 50.0, 5.0).is_none());
}

#[test]
fn nearest_segment_closed_uses_wrap_segment() {
    // Triangle; click near the closing edge (last anchor back to first).
    let pts = vec![(0.0, 0.0), (100.0, 0.0), (50.0, 100.0)];
    // Midpoint of closing segment (idx 2): (25, 50).
    let (seg, _t) = nearest_segment(&pts, true, 25.0, 50.0, 5.0).expect("hit");
    assert_eq!(seg, 2);
}

#[test]
fn insert_anchor_splits_straight_segment_at_midpoint() {
    let mut pts = vec![(0.0, 0.0), (100.0, 0.0)];
    let mut handles = vec![(0.0, 0.0), (0.0, 0.0)];
    let idx = insert_anchor(&mut pts, &mut handles, false, 0, 0.5).expect("inserted");
    assert_eq!(idx, 1);
    assert_eq!(pts.len(), 3);
    assert_eq!(handles.len(), 3);
    assert_eq!(pts[1], (50.0, 0.0));
    // New anchor on a straight segment is a corner.
    assert!(is_corner(&handles, 1));
}

#[test]
fn insert_anchor_on_curve_preserves_shape() {
    // A cubic segment; inserting at t splits it via de Casteljau, so the new
    // anchor must land exactly on the original cubic evaluated at t, and the
    // endpoints must be untouched.
    let a = (0.0, 0.0);
    let b = (100.0, 0.0);
    let pts = vec![a, b];
    let handles = vec![(30.0, 60.0), (30.0, -60.0)]; // both smooth

    // Original cubic control points (mirror in-handle of b).
    let c1 = (a.0 + handles[0].0, a.1 + handles[0].1);
    let c2 = (b.0 - handles[1].0, b.1 - handles[1].1);
    let t = 0.5_f32;
    let cubic = |t: f32| {
        let mt = 1.0 - t;
        let x = mt * mt * mt * a.0
            + 3.0 * mt * mt * t * c1.0
            + 3.0 * mt * t * t * c2.0
            + t * t * t * b.0;
        let y = mt * mt * mt * a.1
            + 3.0 * mt * mt * t * c1.1
            + 3.0 * mt * t * t * c2.1
            + t * t * t * b.1;
        (x, y)
    };
    let expected = cubic(t);

    let mut pts2 = pts.clone();
    let mut handles2 = handles.clone();
    let idx = insert_anchor(&mut pts2, &mut handles2, false, 0, t).expect("inserted");
    assert_eq!(idx, 1);
    assert_eq!(pts2.len(), 3);

    // Endpoints unchanged.
    assert_eq!(pts2[0], a);
    assert_eq!(pts2[2], b);
    // New anchor lies exactly on the original cubic at t.
    let mid = pts2[1];
    assert!(
        (mid.0 - expected.0).abs() < 1e-3 && (mid.1 - expected.1).abs() < 1e-3,
        "inserted {mid:?} != on-curve {expected:?}"
    );

    // And the split halves still trace the original curve: sample several
    // points on the new (two-segment) path against the original cubic.
    let after = flatten(&pts2, &handles2, false);
    for &(x, y) in &after {
        // nearest distance from this point to the original cubic (dense sample)
        let mut min_d = f32::INFINITY;
        for s in 0..=200 {
            let cp = cubic(s as f32 / 200.0);
            min_d = min_d.min((x - cp.0).hypot(y - cp.1));
        }
        assert!(
            min_d < 0.5,
            "split point ({x},{y}) off original curve by {min_d}"
        );
    }
}

#[test]
fn delete_anchor_keeps_min_two_points() {
    let mut pts = vec![(0.0, 0.0), (10.0, 0.0), (20.0, 0.0)];
    let mut handles = vec![(0.0, 0.0); 3];
    assert!(delete_anchor(&mut pts, &mut handles, 1));
    assert_eq!(pts, vec![(0.0, 0.0), (20.0, 0.0)]);
    assert_eq!(handles.len(), 2);
    // Now at 2 points: refuse to delete further.
    assert!(!delete_anchor(&mut pts, &mut handles, 0));
    assert_eq!(pts.len(), 2);
}

#[test]
fn toggle_anchor_corner_to_smooth_and_back() {
    let pts = vec![(0.0, 0.0), (100.0, 0.0), (200.0, 0.0)];
    let mut handles = vec![(0.0, 0.0); 3];
    // Middle anchor, neighbours straddle horizontally -> horizontal tangent.
    let now_smooth = toggle_anchor_smooth(&pts, &mut handles, false, 1);
    assert!(now_smooth);
    assert!(!is_corner(&handles, 1));
    // Tangent should be ~horizontal (dir prev->next is +x).
    let (hx, hy) = handles[1];
    assert!(hx > 0.0 && hy.abs() < 1e-3, "handle=({hx},{hy})");
    // Toggle again -> corner.
    let now_smooth = toggle_anchor_smooth(&pts, &mut handles, false, 1);
    assert!(!now_smooth);
    assert!(is_corner(&handles, 1));
}

#[test]
fn toggle_anchor_endpoint_uses_single_neighbour() {
    let pts = vec![(0.0, 0.0), (100.0, 0.0)];
    let mut handles = vec![(0.0, 0.0); 2];
    // First anchor of an open path: tangent toward the only neighbour.
    let now_smooth = toggle_anchor_smooth(&pts, &mut handles, false, 0);
    assert!(now_smooth);
    let (hx, hy) = handles[0];
    assert!(hx > 0.0 && hy.abs() < 1e-3);
}

// --- Direct-Select: marquee, handle math, convert, compound editing ----

#[test]
fn anchors_in_rect_selects_only_contained_anchors() {
    let pts = vec![(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (50.0, 50.0)];
    // A box covering the first three anchors but not the far one.
    let inside = anchors_in_rect(&pts, &[-1.0, -1.0, 12.0, 12.0]);
    assert_eq!(inside, vec![0, 1, 2]);
    // Edge-touching counts (anchor exactly on the boundary).
    let edge = anchors_in_rect(&pts, &[10.0, 0.0, 40.0, 50.0]);
    assert!(edge.contains(&1) && edge.contains(&3));
    // A box catching nothing.
    assert!(anchors_in_rect(&pts, &[100.0, 100.0, 5.0, 5.0]).is_empty());
}

#[test]
fn anchors_in_rect_normalises_negative_extent() {
    let pts = vec![(5.0, 5.0), (50.0, 50.0)];
    // A box dragged "up-left" (negative w/h) still selects by its real extent.
    let sel = anchors_in_rect(&pts, &[10.0, 10.0, -10.0, -10.0]);
    assert_eq!(sel, vec![0]);
}

#[test]
fn handle_endpoints_mirror_about_anchor() {
    let pts = vec![(10.0, 10.0), (50.0, 10.0)];
    let handles = vec![(5.0, -8.0), (0.0, 0.0)];
    // Smooth anchor: out = anchor + offset, in = anchor − offset (mirror).
    let (out, inp) = handle_endpoints(&pts, &handles, 0).expect("has handle");
    assert_eq!(out, (15.0, 2.0));
    assert_eq!(inp, (5.0, 18.0));
    // Corner anchor: no handle endpoints.
    assert!(handle_endpoints(&pts, &handles, 1).is_none());
}

#[test]
fn make_corner_drops_handle_make_smooth_adds_mirror() {
    let pts = vec![(0.0, 0.0), (100.0, 0.0), (200.0, 0.0)];
    let mut handles = vec![(0.0, 0.0); 3];
    // Corner → smooth: middle anchor gets a non-zero (mirrored) tangent.
    assert!(make_smooth(&pts, &mut handles, false, 1));
    assert!(!is_corner(&handles, 1));
    let (hx, hy) = handles[1];
    assert!(hx > 0.0 && hy.abs() < 1e-3, "smooth tangent ~horizontal");
    // make_smooth on an already-smooth anchor is a no-op.
    assert!(!make_smooth(&pts, &mut handles, false, 1));
    // Smooth → corner: handle zeroed.
    assert!(make_corner(&mut handles, pts.len(), 1));
    assert!(is_corner(&handles, 1));
    // make_corner on an already-corner anchor is a no-op.
    assert!(!make_corner(&mut handles, pts.len(), 1));
}

#[test]
fn shape_contour_count_and_access() {
    let path = open_path();
    assert_eq!(path.contour_count(), 1);
    assert!(path.contour(0).is_some());
    assert!(path.contour(1).is_none());

    let compound = donut(FillRule::NonZero);
    assert_eq!(compound.contour_count(), 2);
    let (pts, _, closed) = compound.contour(1).expect("inner ring");
    assert!(closed);
    assert_eq!(pts.len(), 4);

    // Non-editable shapes expose no contours.
    let rect = Shape::Rect {
        rect: [0.0, 0.0, 10.0, 10.0],
        fill: [0.0; 4],
        fill_gradient: None,
        stroke: [0.0; 4],
        stroke_w: 1.0,
        stroke_style: StrokeStyle::default(),
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
    };
    assert_eq!(rect.contour_count(), 0);
}

#[test]
fn set_anchor_and_handle_move_the_right_point() {
    let mut path = open_path();
    assert!(path.set_anchor(0, 1, 42.0, 7.0));
    let (pts, _, _) = path.contour(0).unwrap();
    assert_eq!(pts[1], (42.0, 7.0));

    // set_handle places the out-knob at the cursor (offset stored relative to
    // the anchor).
    assert!(path.set_handle(0, 1, 52.0, 7.0));
    let (pts, handles, _) = path.contour(0).unwrap();
    assert_eq!(handles[1], (52.0 - pts[1].0, 7.0 - pts[1].1));
}

#[test]
fn insert_and_delete_anchor_on_compound_subcontour() {
    let mut compound = donut(FillRule::NonZero);
    // Insert on the inner ring (contour 1), first segment, midpoint.
    let before = compound.contour(1).unwrap().0.len();
    let idx = compound.insert_anchor_in(1, 0, 0.5).expect("inserted");
    assert_eq!(idx, 1);
    assert_eq!(compound.contour(1).unwrap().0.len(), before + 1);
    // Delete it again.
    assert!(compound.delete_anchor_in(1, idx));
    assert_eq!(compound.contour(1).unwrap().0.len(), before);
    // The outer ring (contour 0) is untouched.
    assert_eq!(compound.contour(0).unwrap().0.len(), 4);
}

#[test]
fn convert_anchor_on_compound_toggles_smooth_corner() {
    let mut compound = donut(FillRule::NonZero);
    // Inner ring corner → smooth.
    let smooth = compound.toggle_anchor_smooth_in(1, 0);
    assert!(smooth);
    let (_, handles, _) = compound.contour(1).unwrap();
    assert!(!is_corner(handles, 0));
    // Back to corner.
    let smooth = compound.toggle_anchor_smooth_in(1, 0);
    assert!(!smooth);
    let (_, handles, _) = compound.contour(1).unwrap();
    assert!(is_corner(handles, 0));
}

#[test]
fn delete_anchor_in_refuses_below_two_points() {
    // A two-point open path: deleting any anchor would leave a single point.
    let mut path = Shape::Path {
        points: vec![(0.0, 0.0), (10.0, 0.0)],
        closed: false,
        fill: [0.0; 4],
        fill_gradient: None,
        stroke: [0.0; 4],
        stroke_w: 1.0,
        stroke_style: StrokeStyle::default(),
        appearance: None,
        handles: vec![(0.0, 0.0); 2],
        live: None,
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
    };
    assert!(!path.delete_anchor_in(0, 0));
    assert_eq!(path.contour(0).unwrap().0.len(), 2);
}
