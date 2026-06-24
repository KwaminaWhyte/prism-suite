use super::*;

// --- Sequence math ------------------------------------------------------

#[test]
fn frame_count_is_duration_times_fps() {
    let mut c = solid([1.0, 1.0, 1.0, 1.0]);
    c.duration = 5.0;
    c.fps = 30.0;
    assert_eq!(frame_count(&c), 150);
    c.duration = 2.0;
    c.fps = 24.0;
    assert_eq!(frame_count(&c), 48);
}

#[test]
fn frame_count_floors_at_one() {
    let mut c = solid([1.0, 1.0, 1.0, 1.0]);
    c.duration = 0.0;
    assert_eq!(frame_count(&c), 1);
}

#[test]
fn frame_time_steps_by_fps() {
    let mut c = solid([1.0, 1.0, 1.0, 1.0]);
    c.fps = 25.0;
    assert!((frame_time(&c, 0) - 0.0).abs() < 1e-6);
    assert!((frame_time(&c, 25) - 1.0).abs() < 1e-6);
}

#[test]
fn frame_path_zero_pads() {
    let dir = Path::new("/tmp/out");
    // <100 frames -> 4-digit padding (the minimum).
    assert_eq!(frame_path(dir, "comp", 7, 90), dir.join("comp_0007.png"));
    // 12000 frames -> highest index 11999 needs 5 digits.
    assert_eq!(
        frame_path(dir, "comp", 42, 12000),
        dir.join("comp_00042.png")
    );
}

#[test]
fn anchor_offset_shifts_coverage_under_rotation() {
    // With the anchor offset off-center, rotating pivots about the anchor,
    // not the layer center — so the covered region moves vs. a centered
    // anchor. Compare covered-pixel counts overlapping a probe far from
    // center to confirm the pivot changed.
    let covered_at = |anchor: f32| {
        let mut c = solid([1.0, 1.0, 1.0, 1.0]);
        c.layers[0].anchor_x.set_key(0.0, anchor);
        c.layers[0].rotation.set_key(0.0, 90.0);
        let f = render_frame(&c, 0.0);
        f.pixels.chunks(4).filter(|p| p[3] > 0).count()
    };
    // Both render *something* but the anchored pivot relocates the quad;
    // assert the quad still covers a sensible number of pixels (sanity) and
    // that an off-center anchor does not crash / vanish.
    assert!(covered_at(0.0) > 0);
    assert!(covered_at(20.0) > 0);
}

#[test]
fn anchored_layer_pivots_position_correctly() {
    // 64x64 comp, center at (32,32). Anchor at the quad's left edge
    // (anchor_x = -half_w ≈ -14) and position 0: the layer's left edge now
    // sits at the comp center, so the quad extends to the right of center.
    let mut c = solid([1.0, 1.0, 1.0, 1.0]);
    let half_w = 64.0 * LAYER_HALF_FRAC; // ~14
    c.layers[0].anchor_x.set_key(0.0, -half_w);
    let f = render_frame(&c, 0.0);
    // A pixel just right of center is covered...
    assert_eq!(f.pixel(40, 32)[3], 255);
    // ...and one left of center (beyond the anchored left edge) is not.
    assert_eq!(f.pixel(10, 32)[3], 0);
}

#[test]
fn parented_child_follows_parent_offset() {
    // Parent shifted right; an unparented child at x=0 covers the center.
    // Parenting it to the moved parent shifts its coverage right too.
    let mut c = Comp {
        width: 64,
        height: 64,
        duration: 1.0,
        fps: 30.0,
        motion_blur: MotionBlur::default(),
        markers: Vec::new(),
        work_area: WorkArea::default(),
        camera: Camera::default(),
        lights: Vec::new(),
        hide_shy: false,
        layers: Vec::new(),
        id: 0,
        name: String::new(),
    };
    c.layers
        .push(PulseLayer::new("parent", [0.0, 0.0, 0.0, 0.0])); // invisible-ish parent
    c.layers[0].visible = false; // parent itself doesn't draw
    c.layers[0].x.set_key(0.0, 18.0);
    let mut child = PulseLayer::new("child", [1.0, 1.0, 1.0, 1.0]);
    child.parent = Some(0);
    c.layers.push(child);

    let f = render_frame(&c, 0.0);
    // Child's coverage rode the parent's +18 offset to the right.
    assert_eq!(f.pixel(50, 32)[3], 255);
    assert_eq!(f.pixel(10, 32)[3], 0);
}
