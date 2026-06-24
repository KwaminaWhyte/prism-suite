use super::*;

// --- Stroke style ------------------------------------------------------

#[test]
fn stroke_style_default_is_solid_butt_miter() {
    let s = StrokeStyle::default();
    assert_eq!(s.cap, LineCap::Butt);
    assert_eq!(s.join, LineJoin::Miter);
    assert_eq!(s.miter_limit, 4.0);
    assert!(!s.is_dashed());
    assert!(s.normalized_dash().is_none());
}

#[test]
fn is_dashed_ignores_all_zero_pattern() {
    let solid = StrokeStyle {
        dash: vec![0.0, 0.0],
        ..Default::default()
    };
    assert!(!solid.is_dashed());
    let dashed = StrokeStyle {
        dash: vec![6.0, 3.0],
        ..Default::default()
    };
    assert!(dashed.is_dashed());
}

#[test]
fn normalized_dash_doubles_odd_pattern() {
    // Odd-length pattern must be repeated so on/off runs alternate evenly
    // (the SVG stroke-dasharray rule).
    let s = StrokeStyle {
        dash: vec![5.0],
        ..Default::default()
    };
    let n = s.normalized_dash().expect("dashed");
    assert_eq!(n, vec![5.0, 5.0]);

    let s2 = StrokeStyle {
        dash: vec![6.0, 2.0, 1.0],
        ..Default::default()
    };
    let n2 = s2.normalized_dash().expect("dashed");
    assert_eq!(n2, vec![6.0, 2.0, 1.0, 6.0, 2.0, 1.0]);
}

#[test]
fn normalized_dash_clamps_negatives() {
    let s = StrokeStyle {
        dash: vec![6.0, -2.0],
        ..Default::default()
    };
    let n = s.normalized_dash().expect("has a positive run");
    assert_eq!(n, vec![6.0, 0.0]);
}
