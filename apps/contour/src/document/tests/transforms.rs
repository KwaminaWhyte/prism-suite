use super::*;

// --- Affine transforms -------------------------------------------------

#[test]
fn axis_aligned_scale_keeps_rect_a_rect() {
    let mut s = Shape::Rect {
        rect: [10.0, 20.0, 40.0, 30.0],
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
    // Scale ×2 about the origin.
    s.apply_affine(&Affine::scale(2.0, 2.0));
    match s {
        Shape::Rect { rect, .. } => {
            assert_eq!(rect, [20.0, 40.0, 80.0, 60.0]);
        }
        _ => panic!("axis-aligned scale should keep a Rect a Rect"),
    }
}

#[test]
fn flip_keeps_rect_normalized() {
    let mut s = Shape::Rect {
        rect: [10.0, 0.0, 40.0, 20.0],
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
    // Horizontal flip about x = 30 (the rect's centre): bounds unchanged,
    // width/height stay positive.
    s.apply_affine(&Affine::scale_about(-1.0, 1.0, 30.0, 0.0));
    match s {
        Shape::Rect { rect, .. } => {
            assert!((rect[0] - 10.0).abs() < 1e-3);
            assert!((rect[2] - 40.0).abs() < 1e-3);
            assert!(rect[2] > 0.0 && rect[3] > 0.0);
        }
        _ => panic!("flip should keep a Rect a Rect"),
    }
}

#[test]
fn rotation_converts_rect_to_path() {
    let mut s = Shape::Rect {
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
    s.apply_affine(&Affine::rotate_about(0.5, 5.0, 5.0));
    assert!(
        matches!(s, Shape::Path { .. }),
        "rotation must rasterise to a Path"
    );
    if let Shape::Path { points, closed, .. } = &s {
        assert_eq!(points.len(), 4);
        assert!(*closed);
    }
}

#[test]
fn rotation_preserves_rect_center() {
    let mut s = Shape::Rect {
        rect: [0.0, 0.0, 100.0, 40.0],
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
    let before = s.bounds().unwrap();
    let (cx0, cy0) = (before.x + before.w * 0.5, before.y + before.h * 0.5);
    // Rotate 90° about the rect centre; the centre must be fixed.
    s.apply_affine(&Affine::rotate_about(std::f32::consts::FRAC_PI_2, cx0, cy0));
    let after = s.bounds().unwrap();
    let (cx1, cy1) = (after.x + after.w * 0.5, after.y + after.h * 0.5);
    assert!((cx0 - cx1).abs() < 0.5 && (cy0 - cy1).abs() < 0.5);
    // A 90° turn swaps the bbox extents.
    assert!((after.w - before.h).abs() < 0.5);
    assert!((after.h - before.w).abs() < 0.5);
}

#[test]
fn ellipse_to_path_round_trips_bounds() {
    let s = Shape::Ellipse {
        rect: [0.0, 0.0, 80.0, 40.0],
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
    let p = s.to_path();
    let pb = p.bounds().unwrap();
    // The cubic ellipse path should hug the original ellipse box closely.
    assert!((pb.x - 0.0).abs() < 0.5);
    assert!((pb.y - 0.0).abs() < 0.5);
    assert!((pb.w - 80.0).abs() < 0.5);
    assert!((pb.h - 40.0).abs() < 0.5);
}

#[test]
fn path_handles_transform_by_linear_part() {
    // A path with a curve handle; under a translate the handle (an offset)
    // must NOT move, but the anchors must.
    let mut s = Shape::Path {
        points: vec![(0.0, 0.0), (10.0, 0.0)],
        closed: false,
        fill: [0.0; 4],
        fill_gradient: None,
        stroke: [0.0; 4],
        stroke_w: 1.0,
        stroke_style: StrokeStyle::default(),
        appearance: None,
        handles: vec![(3.0, 4.0), (0.0, 0.0)],
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
    s.apply_affine(&Affine::translate(100.0, 50.0));
    if let Shape::Path {
        points, handles, ..
    } = &s
    {
        assert_eq!(points[0], (100.0, 50.0));
        assert_eq!(handles[0], (3.0, 4.0)); // offset unchanged by translation
    } else {
        panic!("still a path");
    }
}
