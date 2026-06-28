//! Tests for the Phase 4 Lumetri-grade colour pipeline (`color_grade`).
//! Split out of `mod.rs` to keep that file under the 1000-line cap.

use super::*;
use crate::app_state::{App, Action};

fn approx(a: f32, b: f32) -> bool {
    (a - b).abs() < 1e-3
}

// --- exposure ------------------------------------------------------------

#[test]
fn exposure_scale_doubles_per_stop() {
    assert!(approx(exposure_scale(0.0), 1.0));
    assert!(approx(exposure_scale(1.0), 2.0));
    assert!(approx(exposure_scale(-1.0), 0.5));
    assert!(approx(exposure_scale(2.0), 4.0));
}

#[test]
fn exposure_plus_one_stop_doubles_linear_value() {
    // Pick a mid value whose ×2 still fits in [0,1] linear.
    let lin_in = 0.25;
    let srgb_in = linear_to_srgb(lin_in);
    let mut g = LumetriGrade::default();
    g.exposure = 1.0;
    let out = grade_pixel([srgb_in, srgb_in, srgb_in, 1.0], &g);
    let lin_out = srgb_to_linear(out[0]);
    assert!(approx(lin_out, 0.5), "expected 0.5 linear, got {lin_out}");
    assert!(approx(out[3], 1.0), "alpha preserved");
}

// --- identity ------------------------------------------------------------

#[test]
fn default_grade_is_identity_passthrough() {
    let g = LumetriGrade::default();
    assert!(g.is_identity());
    let px = [0.2, 0.6, 0.8, 0.5];
    let out = grade_pixel(px, &g);
    for c in 0..4 {
        assert!(approx(out[c], px[c]), "channel {c}: {} vs {}", out[c], px[c]);
    }
}

// --- saturation ----------------------------------------------------------

#[test]
fn saturation_zero_makes_gray_and_preserves_luma() {
    let mut g = LumetriGrade::default();
    g.saturation = 0.0;
    let px = [0.8, 0.3, 0.1, 1.0];
    let before_luma = luma_of([px[0], px[1], px[2]]);
    let out = grade_pixel(px, &g);
    assert!(approx(out[0], out[1]) && approx(out[1], out[2]), "grey: {out:?}");
    let after_luma = luma_of([out[0], out[1], out[2]]);
    assert!(approx(before_luma, after_luma), "luma {before_luma} -> {after_luma}");
}

#[test]
fn saturation_one_is_passthrough() {
    let mut g = LumetriGrade::default();
    g.saturation = 1.0;
    let px = [0.8, 0.3, 0.1, 1.0];
    let out = grade_pixel(px, &g);
    for c in 0..3 {
        assert!(approx(out[c], px[c]));
    }
}

#[test]
fn saturation_boost_increases_chroma() {
    let mut g = LumetriGrade::default();
    g.saturation = 2.0;
    let px = [0.6, 0.5, 0.4, 1.0];
    let out = grade_pixel(px, &g);
    let spread_in = px[0] - px[2];
    let spread_out = out[0] - out[2];
    assert!(spread_out > spread_in, "chroma boosted: {spread_in} -> {spread_out}");
}

// --- temperature / tint --------------------------------------------------

#[test]
fn temperature_warm_shifts_toward_red() {
    let mut g = LumetriGrade::default();
    g.temperature = 0.5;
    let px = [0.5, 0.5, 0.5, 1.0];
    let out = grade_pixel(px, &g);
    assert!(out[0] > px[0], "red boosted: {} > {}", out[0], px[0]);
    assert!(out[2] < px[2], "blue cut: {} < {}", out[2], px[2]);
    assert!(out[0] > out[2], "warmer overall");
}

#[test]
fn tint_positive_pushes_magenta() {
    let mut g = LumetriGrade::default();
    g.tint = 0.5;
    let px = [0.5, 0.5, 0.5, 1.0];
    let out = grade_pixel(px, &g);
    assert!(out[1] < px[1], "green cut for magenta tint: {} < {}", out[1], px[1]);
}

// --- lift / gamma / gain -------------------------------------------------

#[test]
fn lift_raises_shadows_more_than_highlights() {
    let mut g = LumetriGrade::default();
    g.lgg.lift.master = 0.3;
    let shadow_in = 0.1;
    let high_in = 0.9;
    let shadow_out = grade_pixel([shadow_in; 4], &g)[0];
    let high_out = grade_pixel([high_in; 4], &g)[0];
    let d_shadow = shadow_out - shadow_in;
    let d_high = high_out - high_in;
    assert!(d_shadow > d_high, "shadow Δ {d_shadow} > highlight Δ {d_high}");
    assert!(d_shadow > 0.0);
}

#[test]
fn gain_scales_highlights_more_than_shadows() {
    let mut g = LumetriGrade::default();
    g.lgg.gain.master = 0.2; // ×1.2
    let shadow_in = 0.2;
    let high_in = 0.8;
    let d_shadow = grade_pixel([shadow_in; 4], &g)[0] - shadow_in;
    let d_high = grade_pixel([high_in; 4], &g)[0] - high_in;
    assert!(d_high > d_shadow, "highlight Δ {d_high} > shadow Δ {d_shadow}");
    assert!(approx(grade_pixel([0.0; 4], &g)[0], 0.0), "gain holds black at 0");
}

#[test]
fn gamma_brightens_midtones() {
    let mut g = LumetriGrade::default();
    g.lgg.gamma.master = 0.5; // gamma 1.5 → brighter mids
    let mid_in = 0.5;
    let out = grade_pixel([mid_in; 4], &g)[0];
    assert!(out > mid_in, "midtone brightened: {out} > {mid_in}");
    // Gamma holds the endpoints.
    assert!(approx(grade_pixel([0.0; 4], &g)[0], 0.0));
    assert!(approx(grade_pixel([1.0; 4], &g)[0], 1.0));
}

#[test]
fn lift_tints_only_target_channel() {
    let mut g = LumetriGrade::default();
    g.lgg.lift.rgb = [0.3, 0.0, 0.0]; // lift red only
    let out = grade_pixel([0.2, 0.2, 0.2, 1.0], &g);
    assert!(out[0] > out[1], "red lifted: {out:?}");
    assert!(approx(out[1], out[2]), "green/blue untouched");
}

// --- contrast ------------------------------------------------------------

#[test]
fn contrast_pivots_around_mid_gray() {
    let mut g = LumetriGrade::default();
    g.contrast = 0.5;
    let out = grade_pixel([0.5, 0.5, 0.5, 1.0], &g);
    assert!(approx(out[0], 0.5), "mid-grey invariant: {}", out[0]);
}

#[test]
fn contrast_increases_separation() {
    let mut g = LumetriGrade::default();
    g.contrast = 0.5;
    let bright = grade_pixel([0.7; 4], &g)[0];
    let dark = grade_pixel([0.3; 4], &g)[0];
    assert!(bright > 0.7, "brights pushed up: {bright}");
    assert!(dark < 0.3, "darks pushed down: {dark}");
}

// --- curves --------------------------------------------------------------

#[test]
fn curve_identity_is_passthrough() {
    let g = LumetriGrade::default();
    assert!(g.curves.is_identity());
    for &v in &[0.0, 0.13, 0.5, 0.77, 1.0] {
        let out = grade_pixel([v; 4], &g)[0];
        assert!(approx(out, v), "curve identity at {v}: {out}");
    }
}

#[test]
fn build_lut_identity_is_linear() {
    let lut = build_channel_lut(&identity_knots(), 256);
    assert_eq!(lut.len(), 256);
    for (i, &y) in lut.iter().enumerate() {
        let x = i as f32 / 255.0;
        assert!(approx(y, x), "lut[{i}] = {y}, expected {x}");
    }
}

#[test]
fn curve_lift_raises_midtone() {
    let mut g = LumetriGrade::default();
    // Bend the master curve up in the middle.
    g.curves.master = vec![[0.0, 0.0], [0.5, 0.7], [1.0, 1.0]];
    let out = grade_pixel([0.5; 4], &g)[0];
    assert!(out > 0.5, "curve lifted mid: {out}");
    // Endpoints unchanged.
    assert!(approx(grade_pixel([0.0; 4], &g)[0], 0.0));
    assert!(approx(grade_pixel([1.0; 4], &g)[0], 1.0));
}

#[test]
fn build_lut_matches_direct_eval() {
    let knots = vec![[0.0, 0.1], [0.5, 0.8], [1.0, 0.9]];
    let lut = build_channel_lut(&knots, 64);
    for (i, &y) in lut.iter().enumerate() {
        let x = i as f32 / 63.0;
        let direct = eval_curve(&knots, x).clamp(0.0, 1.0);
        assert!(approx(y, direct), "lut vs eval at {x}: {y} vs {direct}");
    }
}

// --- secondary -----------------------------------------------------------

#[test]
fn secondary_identity_passthrough() {
    let g = LumetriGrade::default();
    assert!(g.secondary.is_identity());
    let px = [0.9, 0.1, 0.1, 1.0];
    let out = grade_pixel(px, &g);
    for c in 0..3 {
        assert!(approx(out[c], px[c]));
    }
}

#[test]
fn secondary_only_affects_in_range_pixels() {
    // Key a narrow band around red (hue 0); desaturate it.
    let mut g = LumetriGrade::default();
    g.secondary = SecondaryQualifier {
        enabled: true,
        hue_center: 0.0,
        hue_width: 0.05,
        sat_scale: 0.0,
        feather: 0.02,
        ..SecondaryQualifier::default()
    };
    // Red pixel qualifies → gets desaturated (channels converge).
    let red = grade_pixel([0.9, 0.1, 0.1, 1.0], &g);
    assert!((red[0] - red[1]).abs() < 0.2, "red keyed + desaturated: {red:?}");
    // Blue pixel is out of the hue band → untouched.
    let blue_in = [0.1, 0.1, 0.9, 1.0];
    let blue = grade_pixel(blue_in, &g);
    for c in 0..3 {
        assert!(approx(blue[c], blue_in[c]), "blue untouched: {blue:?}");
    }
}

#[test]
fn secondary_full_range_key_affects_everything() {
    let mut g = LumetriGrade::default();
    g.secondary = SecondaryQualifier {
        enabled: true,
        hue_width: 0.5, // all hues
        lum_scale: 0.5, // darken
        ..SecondaryQualifier::default()
    };
    let q = g.secondary.qualify(&[0.2, 0.6, 0.9]);
    assert!(approx(q, 1.0), "full-range key qualifies fully: {q}");
}

// --- tone controls -------------------------------------------------------

#[test]
fn shadows_lift_darks_more_than_brights() {
    let mut g = LumetriGrade::default();
    g.shadows = 0.5;
    let d_dark = grade_pixel([0.1; 4], &g)[0] - 0.1;
    let d_bright = grade_pixel([0.9; 4], &g)[0] - 0.9;
    assert!(d_dark > d_bright, "shadows favour darks: {d_dark} vs {d_bright}");
}

#[test]
fn whites_push_only_top_end() {
    let mut g = LumetriGrade::default();
    g.whites = 0.5;
    // Mid value is below the whites shoulder (0.75) → unchanged.
    let mid = grade_pixel([0.5; 4], &g)[0];
    assert!(approx(mid, 0.5), "whites don't touch mids: {mid}");
    let top = grade_pixel([0.95; 4], &g)[0];
    assert!(top > 0.95, "whites lift the top: {top}");
}

// --- App actions ---------------------------------------------------------

#[test]
fn action_sets_exposure_on_clip_grade() {
    let mut app = App::new();
    assert!(app.clip_grades.is_empty());
    app.apply(Action::SetGradeExposure { clip: 2, value: 1.5 });
    assert!(approx(app.clip_grades[&2].exposure, 1.5));
    // Clamp is honoured.
    app.apply(Action::SetGradeExposure { clip: 2, value: 99.0 });
    assert!(approx(app.clip_grades[&2].exposure, 6.0));
}

#[test]
fn action_adds_sorted_curve_point() {
    let mut app = App::new();
    app.apply(Action::AddGradeCurvePoint {
        clip: 0,
        channel: GradeCurveChannel::Master,
        point: [0.5, 0.7],
    });
    let m = &app.clip_grades[&0].curves.master;
    assert_eq!(m.len(), 3);
    // Sorted by x: [0,0],[0.5,0.7],[1,1].
    assert!(approx(m[1][0], 0.5) && approx(m[1][1], 0.7));
    assert!(m[0][0] <= m[1][0] && m[1][0] <= m[2][0]);
}

#[test]
fn action_sets_wheel_and_clamps() {
    let mut app = App::new();
    app.apply(Action::SetGradeWheel {
        clip: 1,
        which: WheelKind::Gain,
        rgb: [2.0, -2.0, 0.1],
        master: 0.3,
    });
    let w = app.clip_grades[&1].lgg.gain;
    assert!(approx(w.rgb[0], 1.0) && approx(w.rgb[1], -1.0));
    assert!(approx(w.master, 0.3));
}

#[test]
fn action_sets_secondary_qualifier() {
    let mut app = App::new();
    let q = SecondaryQualifier {
        enabled: true,
        hue_center: 1.5, // wraps to 0.5
        sat_scale: 0.0,
        ..SecondaryQualifier::default()
    };
    app.apply(Action::SetGradeSecondary { clip: 0, qualifier: q });
    let s = app.clip_grades[&0].secondary;
    assert!(s.enabled);
    assert!(approx(s.hue_center, 0.5), "hue wrapped: {}", s.hue_center);
}

#[test]
fn action_reset_clears_clip_grade() {
    let mut app = App::new();
    app.apply(Action::SetGradeContrast { clip: 3, value: 0.4 });
    assert!(app.clip_grades.contains_key(&3));
    app.apply(Action::ResetClipGrade { clip: 3 });
    assert!(!app.clip_grades.contains_key(&3));
}

#[test]
fn full_pipeline_runs_without_clip() {
    // A non-trivial stacked grade stays in-gamut and finite.
    let g = LumetriGrade {
        exposure: 0.5,
        contrast: 0.3,
        saturation: 1.4,
        temperature: 0.2,
        tint: -0.1,
        whites: 0.2,
        blacks: -0.1,
        highlights: 0.1,
        shadows: 0.2,
        lgg: LiftGammaGain {
            lift: Wheel { rgb: [0.05, 0.0, -0.02], master: 0.03 },
            gamma: Wheel { rgb: [0.0, 0.0, 0.0], master: 0.1 },
            gain: Wheel { rgb: [0.0, 0.0, 0.05], master: 0.0 },
        },
        ..LumetriGrade::default()
    };
    let out = grade_pixel([0.4, 0.5, 0.6, 0.8], &g);
    for c in 0..4 {
        assert!(out[c].is_finite() && (0.0..=1.0).contains(&out[c]), "ch {c} = {}", out[c]);
    }
}
