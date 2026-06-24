use super::*;

// --- Track mattes -------------------------------------------------------

#[test]
fn matte_none_is_passthrough() {
    // No matte: factor is always 1 regardless of the source pixel.
    for px in [[0.0; 4], [1.0; 4], [0.3, 0.6, 0.9, 0.5]] {
        assert_eq!(MatteMode::None.factor(px), 1.0);
    }
    assert!(!MatteMode::None.is_active());
    assert!(MatteMode::Alpha.is_active());
}

#[test]
fn alpha_matte_reads_source_alpha() {
    // Color is irrelevant to an alpha matte; only the source alpha matters.
    assert_eq!(MatteMode::Alpha.factor([0.9, 0.1, 0.4, 1.0]), 1.0);
    assert_eq!(MatteMode::Alpha.factor([0.9, 0.1, 0.4, 0.0]), 0.0);
    assert!((MatteMode::Alpha.factor([0.0, 0.0, 0.0, 0.25]) - 0.25).abs() < 1e-6);
    // Inverted alpha is 1 - alpha.
    assert_eq!(MatteMode::AlphaInverted.factor([1.0, 1.0, 1.0, 1.0]), 0.0);
    assert_eq!(MatteMode::AlphaInverted.factor([1.0, 1.0, 1.0, 0.0]), 1.0);
}

#[test]
fn luma_matte_reads_weighted_brightness() {
    // Opaque white -> luma ~1; opaque black -> 0.
    assert!((MatteMode::Luma.factor([1.0, 1.0, 1.0, 1.0]) - 1.0).abs() < 1e-5);
    assert_eq!(MatteMode::Luma.factor([0.0, 0.0, 0.0, 1.0]), 0.0);
    // Green carries the most luma weight (Rec.709), blue the least.
    let g = MatteMode::Luma.factor([0.0, 1.0, 0.0, 1.0]);
    let b = MatteMode::Luma.factor([0.0, 0.0, 1.0, 1.0]);
    assert!(g > b, "green luma {g} should exceed blue luma {b}");
    // A transparent bright pixel mattes to ~0 (luma is weighted by alpha).
    assert_eq!(MatteMode::Luma.factor([1.0, 1.0, 1.0, 0.0]), 0.0);
    // Inverted luma flips a bright source to ~0.
    assert!(MatteMode::LumaInverted.factor([1.0, 1.0, 1.0, 1.0]) < 1e-5);
    assert!((MatteMode::LumaInverted.factor([0.0, 0.0, 0.0, 1.0]) - 1.0).abs() < 1e-5);
}

#[test]
fn matte_factor_is_clamped() {
    // Out-of-gamut source values can't push the factor past [0,1].
    assert_eq!(MatteMode::Luma.factor([5.0, 5.0, 5.0, 2.0]), 1.0);
    assert_eq!(MatteMode::AlphaInverted.factor([0.0, 0.0, 0.0, -1.0]), 1.0);
}

#[test]
fn matte_source_is_layer_above_when_active() {
    let mut c = parented_comp(); // layers: 0 (parent), 1 (child)
                                 // Layer 0 with an active matte borrows layer 1 (the one above it).
    c.layers[0].matte = MatteMode::Alpha;
    assert_eq!(c.matte_source(0), Some(1));
    // The top layer has nothing above to borrow -> no source.
    c.layers[1].matte = MatteMode::Luma;
    assert_eq!(c.matte_source(1), None);
    // Without an active matte there is no source even if a layer is above.
    c.layers[0].matte = MatteMode::None;
    assert_eq!(c.matte_source(0), None);
}

#[test]
fn is_matte_source_tracks_layer_below() {
    let mut c = parented_comp(); // 0, 1
                                 // Layer 0 mattes off layer 1 -> layer 1 is a matte source, layer 0 isn't.
    c.layers[0].matte = MatteMode::Alpha;
    assert!(c.is_matte_source(1));
    assert!(!c.is_matte_source(0));
    // Turning the matte off un-consumes layer 1.
    c.layers[0].matte = MatteMode::None;
    assert!(!c.is_matte_source(1));
}

#[test]
fn matte_serde_defaults_to_none() {
    // Pre-matte layers (no `matte` field) load as un-matted.
    let json = r#"{"name":"L","color":[1.0,1.0,1.0,1.0],"visible":true,
        "x":{"keys":[]},"y":{"keys":[]},"scale":{"keys":[]},
        "rotation":{"keys":[]},"opacity":{"keys":[]}}"#;
    let layer: PulseLayer = serde_json::from_str(json).unwrap();
    assert_eq!(layer.matte, MatteMode::None);
}

#[test]
fn parent_serde_defaults_to_none() {
    // Pre-parenting layers (no `parent`/anchor fields) load as unparented.
    let json = r#"{"name":"L","color":[1.0,1.0,1.0,1.0],"visible":true,
        "x":{"keys":[]},"y":{"keys":[]},"scale":{"keys":[]},
        "rotation":{"keys":[]},"opacity":{"keys":[]}}"#;
    let layer: PulseLayer = serde_json::from_str(json).unwrap();
    assert_eq!(layer.parent, None);
    assert!(layer.anchor_x.keys.is_empty());
    assert!(layer.anchor_y.keys.is_empty());
}

// --- Motion blur --------------------------------------------------------

#[test]
fn motion_blur_defaults_match_ae() {
    let mb = MotionBlur::default();
    assert!(!mb.enabled); // off until opted in
    assert_eq!(mb.angle, 180.0); // cinematic half-frame shutter
    assert_eq!(mb.phase, 0.0);
    assert_eq!(mb.samples, 16);
}

#[test]
fn shutter_window_width_tracks_angle() {
    let fps = 25.0; // 1 frame = 0.04 s
                    // 360° opens the shutter for a whole frame; 180° for half.
    let full = MotionBlur {
        angle: 360.0,
        ..Default::default()
    };
    let (o, c) = full.shutter_window(1.0, fps);
    assert!((o - 1.0).abs() < 1e-6); // phase 0 opens at t
    assert!((c - o - 0.04).abs() < 1e-6); // width == one frame

    let half = MotionBlur {
        angle: 180.0,
        ..Default::default()
    };
    let (o, c) = half.shutter_window(1.0, fps);
    assert!((c - o - 0.02).abs() < 1e-6); // width == half a frame
}

#[test]
fn shutter_phase_shifts_window() {
    let fps = 50.0; // 1 frame = 0.02 s
                    // phase = -angle/2 centers the window on the frame time.
    let mb = MotionBlur {
        angle: 180.0,
        phase: -90.0,
        ..Default::default()
    };
    let (o, c) = mb.shutter_window(2.0, fps);
    let mid = 0.5 * (o + c);
    assert!((mid - 2.0).abs() < 1e-6, "window not centered: mid={mid}");
}

#[test]
fn sample_times_span_window_and_count() {
    let fps = 30.0;
    let mb = MotionBlur {
        angle: 360.0,
        samples: 8,
        ..Default::default()
    };
    let times = mb.sample_times(0.5, fps);
    assert_eq!(times.len(), 8);
    let (open, close) = mb.shutter_window(0.5, fps);
    // Every sample lands strictly inside the open window, ascending.
    for w in times.windows(2) {
        assert!(w[0] < w[1]);
    }
    assert!(*times.first().unwrap() > open);
    assert!(*times.last().unwrap() < close);
    // Midpoint sampling is symmetric about the window center.
    let mid = 0.5 * (open + close);
    let first_off = mid - times.first().unwrap();
    let last_off = times.last().unwrap() - mid;
    assert!((first_off - last_off).abs() < 1e-5);
}

#[test]
fn single_sample_lands_at_window_center() {
    let mb = MotionBlur {
        samples: 1,
        angle: 200.0,
        phase: 30.0,
        ..Default::default()
    };
    let times = mb.sample_times(1.0, 24.0);
    assert_eq!(times.len(), 1);
    let (open, close) = mb.shutter_window(1.0, 24.0);
    assert!((times[0] - 0.5 * (open + close)).abs() < 1e-6);
}

#[test]
fn sample_times_clamp_count_into_range() {
    // 0 samples degrades to 1; absurd counts clamp to 64.
    let zero = MotionBlur {
        samples: 0,
        ..Default::default()
    };
    assert_eq!(zero.sample_times(0.0, 30.0).len(), 1);
    let huge = MotionBlur {
        samples: 9999,
        ..Default::default()
    };
    assert_eq!(huge.sample_times(0.0, 30.0).len(), 64);
}

#[test]
fn layer_motion_blurred_needs_both_switches() {
    let mut c = parented_comp();
    c.layers[0].motion_blur = true;
    // Comp master off -> no layer is blurred even if its flag is on.
    c.motion_blur.enabled = false;
    assert!(!c.layer_motion_blurred(0));
    // Master on, layer flag on -> blurred.
    c.motion_blur.enabled = true;
    assert!(c.layer_motion_blurred(0));
    // Master on but the layer opted out -> not blurred.
    assert!(!c.layer_motion_blurred(1));
    // Out-of-range index is never blurred.
    assert!(!c.layer_motion_blurred(99));
}

#[test]
fn motion_blur_serde_defaults_off() {
    // A pre-motion-blur comp (no `motion_blur` field) loads with MB off and a
    // layer without the flag loads un-blurred.
    let json = r#"{"width":16,"height":16,"duration":1.0,"fps":30.0,
        "layers":[{"name":"L","color":[1.0,1.0,1.0,1.0],"visible":true,
        "x":{"keys":[]},"y":{"keys":[]},"scale":{"keys":[]},
        "rotation":{"keys":[]},"opacity":{"keys":[]}}]}"#;
    let comp: Comp = serde_json::from_str(json).unwrap();
    assert!(!comp.motion_blur.enabled);
    assert_eq!(comp.motion_blur.angle, 180.0);
    assert!(!comp.layers[0].motion_blur);
    assert!(!comp.layer_motion_blurred(0));
}
