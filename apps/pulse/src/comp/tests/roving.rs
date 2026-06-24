use super::*;

// ---------------------------------------------------------------------------
// Roving keyframes (Rove Across Time): constant-velocity spatial re-timing.
// ---------------------------------------------------------------------------

/// Build a `RoveKey` from `(t, x, y, roving)` for terse test fixtures.
fn rk(t: f32, x: f32, y: f32, roving: bool) -> RoveKey {
    RoveKey {
        t,
        pos: [x, y],
        roving,
    }
}

#[test]
fn roved_times_endpoints_never_move() {
    // Endpoints are always anchored even if (illegally) flagged roving.
    let keys = [
        rk(0.0, 0.0, 0.0, true),
        rk(1.0, 50.0, 0.0, true),
        rk(2.0, 100.0, 0.0, true),
    ];
    let times = roved_times(&keys);
    assert_eq!(times[0], 0.0, "first key pinned");
    assert_eq!(times[2], 2.0, "last key pinned");
}

#[test]
fn roved_times_even_spacing_unchanged() {
    // Evenly-spaced positions in time AND space: an interior roving key keeps its
    // time (the path is already constant-velocity).
    let keys = [
        rk(0.0, 0.0, 0.0, false),
        rk(1.0, 50.0, 0.0, true), // roves
        rk(2.0, 100.0, 0.0, false),
    ];
    let times = roved_times(&keys);
    assert!(
        (times[1] - 1.0).abs() < 1e-4,
        "even path => time ~unchanged, got {}",
        times[1]
    );
}

#[test]
fn roved_times_uneven_equalizes_velocity() {
    // Unevenly-spaced positions: the interior key sits at 10% of the distance but
    // 50% of the authored time. Roving re-times it to 10% of the time so per-leg
    // speed equalizes.
    let keys = [
        rk(0.0, 0.0, 0.0, false),
        rk(1.0, 10.0, 0.0, true), // close to start in space
        rk(2.0, 100.0, 0.0, false),
    ];
    let times = roved_times(&keys);
    // Total path 100; first leg 10 => 10% of the 2s span = 0.2s.
    assert!((times[1] - 0.2).abs() < 1e-3, "expected ~0.2, got {}", times[1]);

    // Velocity check: distance/time over each leg should be ~equal.
    let v0 = 10.0 / (times[1] - times[0]);
    let v1 = 90.0 / (times[2] - times[1]);
    assert!(
        (v0 - v1).abs() / v0 < 1e-2,
        "legs should be ~constant velocity: {v0} vs {v1}"
    );
}

#[test]
fn roved_times_roving_off_unchanged() {
    // No roving flag anywhere => authored times preserved exactly (back-compat).
    let keys = [
        rk(0.0, 0.0, 0.0, false),
        rk(0.3, 10.0, 0.0, false),
        rk(2.0, 100.0, 0.0, false),
    ];
    let times = roved_times(&keys);
    assert_eq!(times, vec![0.0, 0.3, 2.0]);
}

#[test]
fn roved_times_two_keys_is_noop() {
    let keys = [rk(0.0, 0.0, 0.0, true), rk(2.0, 100.0, 0.0, true)];
    assert_eq!(roved_times(&keys), vec![0.0, 2.0]);
}

#[test]
fn roved_times_multiple_roving_in_segment() {
    // Two interior roving keys between the same anchored pair, redistributed by
    // arc length. Positions at 10 and 40 of a 100-long path over a 2s span =>
    // times 0.2 and 0.8.
    let keys = [
        rk(0.0, 0.0, 0.0, false),
        rk(0.5, 10.0, 0.0, true),
        rk(1.0, 40.0, 0.0, true),
        rk(2.0, 100.0, 0.0, false),
    ];
    let times = roved_times(&keys);
    assert!((times[1] - 0.2).abs() < 1e-3, "got {}", times[1]);
    assert!((times[2] - 0.8).abs() < 1e-3, "got {}", times[2]);
    // Strictly increasing.
    assert!(times[0] < times[1] && times[1] < times[2] && times[2] < times[3]);
}

#[test]
fn roved_times_anchored_interior_splits_segments() {
    // An anchored interior key splits the run into two independent segments; the
    // roving key in each is re-timed within its own span only.
    let keys = [
        rk(0.0, 0.0, 0.0, false),
        rk(1.0, 10.0, 0.0, true),   // roving in [0, 2]
        rk(2.0, 100.0, 0.0, false), // anchored interior
        rk(3.0, 110.0, 0.0, true),  // roving in [2, 4]
        rk(4.0, 200.0, 0.0, false),
    ];
    let times = roved_times(&keys);
    assert_eq!(times[2], 2.0, "anchored interior pinned");
    assert!((times[1] - 0.2).abs() < 1e-3, "first segment, got {}", times[1]);
    assert!(
        (times[3] - 2.2).abs() < 1e-3,
        "second segment, got {}",
        times[3]
    );
}

#[test]
fn roved_times_coincident_positions_even_fallback() {
    // Zero-length path (all positions identical): fall back to even spacing so the
    // keys never collapse onto one instant.
    let keys = [
        rk(0.0, 5.0, 5.0, false),
        rk(0.1, 5.0, 5.0, true),
        rk(0.2, 5.0, 5.0, true),
        rk(3.0, 5.0, 5.0, false),
    ];
    let times = roved_times(&keys);
    assert!(
        (times[1] - 1.0).abs() < 1e-3,
        "even 1/3 of span, got {}",
        times[1]
    );
    assert!(
        (times[2] - 2.0).abs() < 1e-3,
        "even 2/3 of span, got {}",
        times[2]
    );
}

#[test]
fn roved_times_is_deterministic() {
    let keys = [
        rk(0.0, 0.0, 0.0, false),
        rk(1.0, 7.0, 3.0, true),
        rk(1.5, 80.0, 9.0, true),
        rk(2.0, 100.0, 0.0, false),
    ];
    assert_eq!(roved_times(&keys), roved_times(&keys));
}

#[test]
fn roved_tracks_remaps_position_for_constant_velocity() {
    // A layer-style x/y pair: the interior key is close in space to the start but
    // halfway in time. Roving re-times it so sampling reflects constant velocity.
    let mut x = Track::default();
    let mut y = Track::default();
    x.set_key(0.0, 0.0);
    x.set_key(1.0, 10.0);
    x.set_key(2.0, 100.0);
    y.set_key(0.0, 0.0);
    y.set_key(1.0, 0.0);
    y.set_key(2.0, 0.0);
    // Mark the interior x/y keys roving (the UI does this on both tracks).
    x.set_roving(1.0, true);
    y.set_roving(1.0, true);
    assert!(has_roving(&x, &y));

    let (rx, _ry) = roved_tracks(&x, &y);
    // The interior key moved from t=1.0 to ~0.2.
    let moved = rx.keys[1].t;
    assert!(
        (moved - 0.2).abs() < 1e-3,
        "interior x re-timed to ~0.2, got {moved}"
    );

    // Sampling at t=1.0 (mid authored time): with constant velocity the layer is
    // already well past x=10 (it would have stuck at 10 without roving).
    let xat1 = rx.sample(1.0, 0.0);
    assert!(
        xat1 > 40.0,
        "constant velocity puts the layer past mid-path by t=1, got {xat1}"
    );
}

#[test]
fn roved_tracks_no_roving_is_borrow_identical() {
    // Without roving flags, the roved copy equals the original (back-compat).
    let mut x = Track::default();
    let mut y = Track::default();
    x.set_key(0.0, 0.0);
    x.set_key(0.3, 10.0);
    x.set_key(2.0, 100.0);
    y.set_key(0.0, 0.0);
    y.set_key(2.0, 50.0);
    assert!(!has_roving(&x, &y));
    let (rx, ry) = roved_tracks(&x, &y);
    assert_eq!(rx, x);
    assert_eq!(ry, y);
}

#[test]
fn layer_value_honours_roving() {
    // The layer's position sampling routes through the roving re-timer.
    let mut layer = PulseLayer::new("rover", [1.0; 4]);
    layer.x.set_key(0.0, 0.0);
    layer.x.set_key(1.0, 10.0);
    layer.x.set_key(2.0, 100.0);
    layer.y.set_key(0.0, 0.0);
    layer.y.set_key(1.0, 0.0);
    layer.y.set_key(2.0, 0.0);
    let before = layer.value(Prop::X, 1.0);
    assert!((before - 10.0).abs() < 1e-4, "no roving => x=10 at t=1");

    layer.x.set_roving(1.0, true);
    layer.y.set_roving(1.0, true);
    let after = layer.value(Prop::X, 1.0);
    assert!(
        after > 40.0,
        "roving => layer is past mid-path at t=1, got {after}"
    );
}

#[test]
fn keyframe_roving_serde_roundtrips_and_defaults() {
    // A roving key round-trips; legacy JSON (no `roving` field) defaults to false.
    let mut t = Track::default();
    t.set_key(0.0, 0.0);
    t.set_key(1.0, 10.0);
    t.set_key(2.0, 100.0);
    t.set_roving(1.0, true);
    let json = serde_json::to_string(&t).unwrap();
    let back: Track = serde_json::from_str(&json).unwrap();
    assert_eq!(back, t, "roving track round-trips");
    assert!(back.keys[1].roving);
    // Exactly one key carries the flag, so it serializes once (non-roving keys
    // skip it, keeping pre-roving files byte-identical).
    assert_eq!(
        json.matches("roving").count(),
        1,
        "only the one roving key emits the flag: {json}"
    );

    // Legacy keyframe JSON without `roving` defaults to false.
    let legacy = r#"{"t":0.0,"value":5.0}"#;
    let kf: Keyframe = serde_json::from_str(legacy).unwrap();
    assert!(!kf.roving);
}
