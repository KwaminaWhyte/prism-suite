use super::*;

// ---------------------------------------------------------------------------
// Spatial motion paths + auto-orient along path
// ---------------------------------------------------------------------------

/// Build a layer with the given linear position keyframes on X and Y.
fn moving_layer(keys: &[(f32, f32, f32)]) -> PulseLayer {
    let mut l = PulseLayer::new("Mover", [1.0, 1.0, 1.0, 1.0]);
    for &(t, x, y) in keys {
        l.x.set_key(t, x);
        l.y.set_key(t, y);
    }
    l
}

#[test]
fn motion_path_tangent_points_along_travel() {
    // A layer sliding straight to the right (+x): the tangent must be (1, 0) and
    // the heading 0°. Straight down the screen (+y down) heads 90°; diagonally
    // down-right heads 45°.
    let right = moving_layer(&[(0.0, -100.0, 0.0), (2.0, 100.0, 0.0)]);
    let s = sample_path(&right.x, &right.y, 1.0, 0.0, 0.0);
    let [dx, dy] = s.tangent.expect("moving → has a tangent");
    assert!((dx - 1.0).abs() < 1e-3 && dy.abs() < 1e-3, "tangent {dx},{dy}");
    assert!((s.heading_deg().unwrap() - 0.0).abs() < 1e-2);

    let down = moving_layer(&[(0.0, 0.0, -100.0), (2.0, 0.0, 100.0)]);
    assert!(
        (sample_path(&down.x, &down.y, 1.0, 0.0, 0.0)
            .heading_deg()
            .unwrap()
            - 90.0)
            .abs()
            < 1e-2
    );

    let diag = moving_layer(&[(0.0, 0.0, 0.0), (2.0, 100.0, 100.0)]);
    assert!(
        (sample_path(&diag.x, &diag.y, 1.0, 0.0, 0.0)
            .heading_deg()
            .unwrap()
            - 45.0)
            .abs()
            < 1e-2
    );
}

#[test]
fn motion_path_position_matches_transform() {
    // The path's sampled position is exactly the layer's keyframed (x, y) — the
    // tangent is auxiliary, not a separate spline that could disagree.
    let m = moving_layer(&[(0.0, -50.0, 10.0), (4.0, 50.0, -30.0)]);
    let s = sample_path(&m.x, &m.y, 2.0, 0.0, 0.0);
    let tf = m.transform(2.0);
    assert!((s.pos[0] - tf.x).abs() < 1e-4 && (s.pos[1] - tf.y).abs() < 1e-4);
}

#[test]
fn motion_path_multi_key_corner_turns_tangent() {
    // An L-shaped path: travel +x for the first segment, then +y for the second.
    // The heading in each segment's middle reflects that segment's direction.
    let m = moving_layer(&[(0.0, 0.0, 0.0), (1.0, 100.0, 0.0), (2.0, 100.0, 100.0)]);
    assert!(
        (sample_path(&m.x, &m.y, 0.5, 0.0, 0.0)
            .heading_deg()
            .unwrap())
        .abs()
            < 1e-2
    );
    assert!(
        (sample_path(&m.x, &m.y, 1.5, 0.0, 0.0)
            .heading_deg()
            .unwrap()
            - 90.0)
            .abs()
            < 1e-2
    );
}

#[test]
fn motion_path_stationary_has_no_tangent() {
    // A single key (constant position) and an unanimated layer are stationary, so
    // there is no defined heading and auto-orient contributes 0°.
    let one = moving_layer(&[(1.0, 5.0, 5.0)]);
    assert!(sample_path(&one.x, &one.y, 1.0, 0.0, 0.0).tangent.is_none());
    assert_eq!(auto_orient_deg(&one.x, &one.y, 1.0, 0.0, 0.0), 0.0);

    let empty = PulseLayer::new("Still", [1.0; 4]);
    assert!(sample_path(&empty.x, &empty.y, 0.5, 0.0, 0.0).tangent.is_none());

    // A held segment is also stationary between its keys (no interpolation).
    let mut held = moving_layer(&[(0.0, 0.0, 0.0), (2.0, 100.0, 0.0)]);
    held.x.set_interp(0.0, Interp::Hold);
    held.y.set_interp(0.0, Interp::Hold);
    assert!(sample_path(&held.x, &held.y, 1.0, 0.0, 0.0).tangent.is_none());
}

#[test]
fn auto_orient_off_leaves_rotation_unchanged() {
    // Back-compat: with auto_orient off (the serde default), the layer's effective
    // rotation is exactly its keyframed rotation regardless of its motion.
    let mut c = Comp::new();
    c.layers.clear();
    let mut l = moving_layer(&[(0.0, -100.0, 0.0), (2.0, 100.0, 0.0)]);
    l.rotation.set_key(0.0, 30.0); // a constant 30° spin
    assert!(!l.auto_orient);
    c.layers.push(l);
    assert!((c.layer_transform(0, 1.0).rotation_deg - 30.0).abs() < 1e-4);
}

#[test]
fn auto_orient_on_rotates_to_face_travel() {
    // With auto_orient on, the path heading is *added* to the keyframed rotation:
    // a layer heading +y (90°) with a 30° keyed spin reads as 120°.
    let mut c = Comp::new();
    c.layers.clear();
    let mut l = moving_layer(&[(0.0, 0.0, -100.0), (2.0, 0.0, 100.0)]); // heads +y → 90°
    l.auto_orient = true;
    l.rotation.set_key(0.0, 30.0);
    c.layers.push(l);
    assert!((c.layer_transform(0, 1.0).rotation_deg - 120.0).abs() < 1e-2);
}

#[test]
fn auto_orient_serde_roundtrips_and_legacy_off() {
    // Round-trips when set; a pre-auto-orient layer (no field) loads with it off.
    let mut l = PulseLayer::new("L", [1.0; 4]);
    l.auto_orient = true;
    let json = serde_json::to_string(&l).unwrap();
    let back: PulseLayer = serde_json::from_str(&json).unwrap();
    assert!(back.auto_orient);

    let legacy = r#"{"name":"L","color":[1.0,1.0,1.0,1.0],"visible":true,
        "x":{"keys":[]},"y":{"keys":[]},"scale":{"keys":[]},
        "rotation":{"keys":[]},"opacity":{"keys":[]}}"#;
    let layer: PulseLayer = serde_json::from_str(legacy).unwrap();
    assert!(!layer.auto_orient);
}

#[test]
fn motion_path_sampling_is_deterministic() {
    // The pure sampler returns bit-identical results across repeated calls.
    let m = moving_layer(&[(0.0, 0.0, 0.0), (1.0, 80.0, 20.0), (2.0, 30.0, 90.0)]);
    for &t in &[0.3_f32, 0.75, 1.4, 1.9] {
        let a = sample_path(&m.x, &m.y, t, 0.0, 0.0);
        let b = sample_path(&m.x, &m.y, t, 0.0, 0.0);
        assert_eq!(a, b);
    }
}
