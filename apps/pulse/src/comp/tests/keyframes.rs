use super::*;

#[test]
fn empty_track_uses_default() {
    let t = Track::default();
    assert_eq!(t.sample(2.0, 1.0), 1.0);
}

#[test]
fn single_key_is_constant() {
    let mut t = Track::default();
    t.set_key(1.0, 7.0);
    assert_eq!(t.sample(0.0, 0.0), 7.0);
    assert_eq!(t.sample(5.0, 0.0), 7.0);
}

#[test]
fn linear_interp_and_hold() {
    let mut t = Track::default();
    t.set_key(0.0, 0.0);
    t.set_key(2.0, 10.0);
    assert_eq!(t.sample(-1.0, 99.0), 0.0); // hold before first
    assert!((t.sample(1.0, 0.0) - 5.0).abs() < 1e-5); // midpoint
    assert_eq!(t.sample(9.0, 0.0), 10.0); // hold after last
}

#[test]
fn set_key_overwrites_and_sorts() {
    let mut t = Track::default();
    t.set_key(2.0, 1.0);
    t.set_key(0.0, 2.0);
    t.set_key(2.0, 5.0); // overwrite the key at t=2
    assert_eq!(t.keys.len(), 2);
    assert_eq!(t.keys[0].t, 0.0);
    assert_eq!(t.keys[1].value, 5.0);
}

// --- Easing math --------------------------------------------------------

#[test]
fn ease_endpoints_are_exact() {
    for e in [Ease::EASY, Ease::IN, Ease::OUT] {
        assert_eq!(e.eval(0.0), 0.0);
        assert_eq!(e.eval(1.0), 1.0);
        // Out-of-range x is clamped, not extrapolated.
        assert_eq!(e.eval(-1.0), 0.0);
        assert_eq!(e.eval(2.0), 1.0);
    }
}

#[test]
fn linear_ease_is_identity() {
    // cubic-bezier(1/3, 1/3, 2/3, 2/3) is the straight diagonal: y == x.
    let lin = Ease {
        out_x: 1.0 / 3.0,
        out_y: 1.0 / 3.0,
        in_x: 2.0 / 3.0,
        in_y: 2.0 / 3.0,
    };
    for i in 0..=10 {
        let x = i as f32 / 10.0;
        assert!((lin.eval(x) - x).abs() < 1e-4, "x={x}");
    }
}

#[test]
fn easy_ease_is_symmetric_and_slow_at_ends() {
    let e = Ease::EASY;
    // Symmetry about the midpoint: f(x) + f(1-x) == 1.
    for i in 1..10 {
        let x = i as f32 / 10.0;
        assert!((e.eval(x) + e.eval(1.0 - x) - 1.0).abs() < 1e-3, "x={x}");
    }
    // Midpoint sits exactly at 0.5 by symmetry.
    assert!((e.eval(0.5) - 0.5).abs() < 1e-4);
    // Eased curve lags behind linear early (slow start) ...
    assert!(e.eval(0.25) < 0.25);
    // ... and leads it late (fast then slow finish is the mirror).
    assert!(e.eval(0.75) > 0.75);
}

#[test]
fn ease_eval_inverts_x_correctly() {
    // For any handle config, eval(x) must equal bezier_y(s) where
    // bezier_x(s) == x. Check the x-solve round-trips.
    let e = Ease {
        out_x: 0.8,
        out_y: 0.1,
        in_x: 0.2,
        in_y: 0.9,
    };
    for i in 0..=20 {
        let x = i as f32 / 20.0;
        let s = solve_bezier_x(x, e.out_x.clamp(0.0, 1.0), e.in_x.clamp(0.0, 1.0));
        let reconstructed_x = cubic_bezier(s, e.out_x, e.in_x);
        assert!((reconstructed_x - x).abs() < 1e-3, "x={x}");
    }
}

#[test]
fn ease_is_monotonic_in_x_for_standard_handles() {
    // With monotonic y-handles the eased value never decreases as x grows.
    let e = Ease::EASY;
    let mut prev = -1.0;
    for i in 0..=50 {
        let y = e.eval(i as f32 / 50.0);
        assert!(y >= prev - 1e-4, "non-monotonic at i={i}");
        prev = y;
    }
}

#[test]
fn hold_interp_steps() {
    let mut t = Track::default();
    t.set_key(0.0, 0.0);
    t.set_key(2.0, 10.0);
    t.set_interp(0.0, Interp::Hold);
    assert_eq!(t.sample(0.0, 0.0), 0.0);
    assert_eq!(t.sample(1.0, 0.0), 0.0); // holds outgoing value across segment
    assert_eq!(t.sample(1.999, 0.0), 0.0);
    assert_eq!(t.sample(2.0, 0.0), 10.0); // snaps at the next key
}

#[test]
fn eased_segment_matches_ease_curve() {
    let mut t = Track::default();
    t.set_key(0.0, 0.0);
    t.set_key(2.0, 100.0);
    t.set_interp(0.0, Interp::Ease(Ease::EASY));
    // At the temporal midpoint the eased value lands at the curve midpoint.
    assert!((t.sample(1.0, 0.0) - 50.0).abs() < 0.5);
    // Quarter point lags linear (which would give 25).
    assert!(t.sample(0.5, 0.0) < 25.0);
    // Endpoints unchanged.
    assert_eq!(t.sample(0.0, 0.0), 0.0);
    assert_eq!(t.sample(2.0, 0.0), 100.0);
}

#[test]
fn set_key_inherits_neighbour_interp() {
    let mut t = Track::default();
    t.set_key(0.0, 0.0);
    t.set_key(4.0, 100.0);
    t.set_interp(0.0, Interp::Hold);
    // Re-keying inside the held segment inherits Hold, not Linear.
    t.set_key(2.0, 50.0);
    assert_eq!(t.interp_at(2.0), Some(Interp::Hold));
    // Overwriting an existing key keeps its own mode.
    t.set_interp(2.0, Interp::Ease(Ease::EASY));
    t.set_key(2.0, 60.0);
    assert_eq!(t.interp_at(2.0), Some(Interp::Ease(Ease::EASY)));
}

// --- Graph-editor support ----------------------------------------------

#[test]
fn ease_linear_const_is_identity() {
    // Ease::LINEAR is the straight diagonal: converting a linear segment to
    // this eased curve must be value-neutral.
    for i in 0..=10 {
        let x = i as f32 / 10.0;
        assert!((Ease::LINEAR.eval(x) - x).abs() < 1e-4, "x={x}");
    }
}

#[test]
fn with_handles_clamp_x_keep_y_free() {
    let e = Ease::EASY.with_out(1.7, -0.4).with_in(-0.3, 1.9);
    assert_eq!(e.out_x, 1.0); // x clamped into [0,1]
    assert_eq!(e.in_x, 0.0);
    assert_eq!(e.out_y, -0.4); // y free (anticipation/overshoot)
    assert_eq!(e.in_y, 1.9);
}

#[test]
fn value_bounds_none_when_empty() {
    assert_eq!(Track::default().value_bounds(), None);
}

#[test]
fn value_bounds_spans_keyframe_values() {
    let mut t = Track::default();
    t.set_key(0.0, -5.0);
    t.set_key(1.0, 10.0);
    t.set_key(2.0, 3.0);
    let (lo, hi) = t.value_bounds().unwrap();
    assert!(lo <= -5.0 + 1e-4);
    assert!(hi >= 10.0 - 1e-4);
}

#[test]
fn value_bounds_captures_ease_overshoot() {
    // An overshooting ease (out_y/in_y beyond [0,1]) pushes the sampled value
    // past the keyframe endpoints; bounds must include the overshoot.
    let mut t = Track::default();
    t.set_key(0.0, 0.0);
    t.set_key(1.0, 100.0);
    // Big overshoot on the incoming handle.
    t.set_interp(0.0, Interp::Ease(Ease::EASY.with_in(0.67, 1.6)));
    let (_lo, hi) = t.value_bounds().unwrap();
    assert!(hi > 100.0, "expected overshoot above 100, got {hi}");
}

#[test]
fn move_key_reorders_when_crossing_neighbour() {
    let mut t = Track::default();
    t.set_key(0.0, 0.0); // idx 0
    t.set_key(1.0, 10.0); // idx 1
    t.set_key(2.0, 20.0); // idx 2
                          // Drag the middle key past the last one in time.
    let landed = t.move_key(1, 3.0, 99.0);
    assert_eq!(landed, 2);
    // Times stay sorted ascending.
    assert!(t.keys.windows(2).all(|w| w[0].t <= w[1].t));
    // The moved key kept its (new) value at its new slot.
    assert_eq!(t.keys[2].value, 99.0);
    assert_eq!(t.keys[2].t, 3.0);
}

#[test]
fn move_key_without_crossing_keeps_index() {
    let mut t = Track::default();
    t.set_key(0.0, 0.0);
    t.set_key(2.0, 10.0);
    let landed = t.move_key(0, 0.5, 5.0);
    assert_eq!(landed, 0);
    assert_eq!(t.keys[0].t, 0.5);
    assert_eq!(t.keys[0].value, 5.0);
}

#[test]
fn move_key_out_of_range_is_noop() {
    let mut t = Track::default();
    t.set_key(0.0, 0.0);
    assert_eq!(t.move_key(9, 5.0, 5.0), 9);
    assert_eq!(t.keys.len(), 1);
    assert_eq!(t.keys[0].t, 0.0);
}

#[test]
fn interp_serde_defaults_to_linear() {
    // Pre-easing keyframes (no `interp` field) must deserialize as Linear.
    let json = r#"{"keys":[{"t":0.0,"value":1.0},{"t":1.0,"value":2.0}]}"#;
    let track: Track = serde_json::from_str(json).unwrap();
    assert_eq!(track.keys.len(), 2);
    assert_eq!(track.keys[0].interp, Interp::Linear);
    // And it samples linearly.
    assert!((track.sample(0.5, 0.0) - 1.5).abs() < 1e-5);
}
