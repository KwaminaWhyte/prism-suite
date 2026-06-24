use super::*;

// --- Text layers --------------------------------------------------------------

use crate::comp::{TextAlign, TextLayer};

/// A 64x64 comp with a single centered text layer drawing `s` at the given
/// font size in the given fill color (no stroke).
fn text(s: &str, size: f32, fill: [f32; 3]) -> Comp {
    let mut c = solid([1.0, 1.0, 1.0, 1.0]);
    c.layers[0].kind = LayerKind::Text;
    c.layers[0].text = TextLayer {
        text: s.to_string(),
        size,
        tracking: 0.0,
        leading: 0.0,
        align: TextAlign::Center,
        font_family: None,
        fill: Some(Fill {
            color: fill,
            opacity: 1.0,
        }),
        stroke: None,
    };
    c
}

#[test]
fn text_layer_draws_its_glyph() {
    // A centered uppercase "I" (a single vertical bar through the center) covers
    // the comp center with the fill color.
    let c = text("I", 40.0, [1.0, 0.0, 0.0]);
    let [r, g, b, a] = render_frame(&c, 0.0).pixel(32, 32);
    assert!(a > 200, "center of the bar is covered, got a={a}");
    assert!(r > 200 && g == 0 && b == 0, "fill is red: {r},{g},{b}");
    // A far corner is uncovered.
    assert_eq!(render_frame(&c, 0.0).pixel(2, 2)[3], 0, "corner uncovered");
}

#[test]
fn empty_text_layer_renders_nothing() {
    // A text layer with blank text draws nothing.
    let mut c = solid([1.0, 1.0, 1.0, 1.0]);
    c.layers[0].kind = LayerKind::Text;
    c.layers[0].text.text = "   ".to_string();
    let f = render_frame(&c, 0.0);
    assert!(f.pixels.iter().all(|&b| b == 0), "blank text = transparent");
}

#[test]
fn text_layer_honors_opacity() {
    // Half-opacity text -> on-stroke alpha roughly halved.
    let mut c = text("I", 50.0, [1.0, 1.0, 1.0]);
    c.layers[0].opacity.set_key(0.0, 0.5);
    let a = render_frame(&c, 0.0).pixel(32, 32)[3];
    assert!(
        (90..=170).contains(&a),
        "alpha ~half on the stroke, got {a}"
    );
}

#[test]
fn text_layer_composes_with_mask() {
    // A wide "W" carved by a small centered mask: center survives where the W's
    // glyph passes through it; an off-center band is carved away.
    let mut c = text("WWW", 40.0, [1.0, 1.0, 1.0]);
    c.layers[0].masks.push(Mask::rect(6.0, 6.0));
    let f = render_frame(&c, 0.0);
    // A pixel well outside the small mask is carved regardless of glyph coverage.
    assert_eq!(f.pixel(60, 32)[3], 0, "text outside the mask is carved");
}

#[test]
fn text_stroke_outlines_the_glyph() {
    // A stroked "I": just outside the pen body edge reads the stroke color.
    let mut c = solid([1.0, 1.0, 1.0, 1.0]);
    c.layers[0].kind = LayerKind::Text;
    c.layers[0].text = TextLayer {
        text: "I".to_string(),
        size: 80.0,
        tracking: 0.0,
        leading: 0.0,
        align: TextAlign::Center,
        font_family: None,
        fill: Some(Fill {
            color: [1.0, 0.0, 0.0],
            opacity: 1.0,
        }),
        stroke: Some(Stroke {
            color: [0.0, 0.0, 1.0],
            width: 6.0,
            opacity: 1.0,
        }),
    };
    let f = render_frame(&c, 0.0);
    // Dead center of the vertical bar: the red fill body.
    let [r, _g, b, a] = f.pixel(32, 32);
    assert!(a > 200 && r > b, "body is the red fill: {r},{b}");
    // A few px to the side of the bar (past the thin pen, into the outline band):
    // the blue stroke. The bar pen half is ~80*0.055 ≈ 4.4px, stroke half 3px, so
    // x = 32 + 6 lands in the outline band.
    let [r2, _g2, b2, a2] = f.pixel(38, 32);
    assert!(a2 > 60, "outline band covered, got a={a2}");
    assert!(b2 > r2, "outline reads blue: {r2},{b2}");
}

#[test]
fn text_layer_motion_blur_widens_the_footprint() {
    // A fast-sliding text glyph with comp motion blur smears its coverage across
    // the travel, touching more columns on the center row than the crisp render.
    let make = |blur: bool| {
        let mut c = text("I", 40.0, [1.0, 1.0, 1.0]);
        c.layers[0].x.set_key(0.0, -200.0);
        c.layers[0].x.set_key(1.0, 200.0);
        c.motion_blur.enabled = blur;
        c.motion_blur.angle = 720.0;
        c.layers[0].motion_blur = blur;
        c
    };
    let covered_cols = |c: &Comp| {
        let f = render_frame(c, 0.5);
        (0..f.width).filter(|&x| f.pixel(x, 32)[3] > 0).count()
    };
    assert!(
        covered_cols(&make(true)) > covered_cols(&make(false)),
        "motion blur widens the swept text footprint"
    );
}

#[test]
fn text_layer_as_luma_matte() {
    // A text layer above a solid, used as its luma matte: the solid shows only
    // where the (bright) glyph strokes cover, transparent elsewhere.
    let mut c = solid([1.0, 0.0, 0.0, 1.0]); // index 0: the matted red solid
    c.layers[0].matte = MatteMode::Luma;
    // index 1: a white text layer above acts as the matte source.
    let mut src = PulseLayer::of_kind(LayerKind::Text, "T", [1.0; 4]);
    src.text = TextLayer {
        text: "I".to_string(),
        size: 50.0,
        tracking: 0.0,
        leading: 0.0,
        align: TextAlign::Center,
        font_family: None,
        fill: Some(Fill {
            color: [1.0, 1.0, 1.0],
            opacity: 1.0,
        }),
        stroke: None,
    };
    c.layers.push(src);
    let f = render_frame(&c, 0.0);
    // On the glyph stroke (center) the red solid shows; the matte source itself
    // doesn't composite.
    assert!(f.pixel(32, 32)[0] > 150, "solid visible under the glyph");
    // Off the glyph (a corner of the quad away from any stroke) it's matted out.
    assert_eq!(f.pixel(2, 2)[3], 0, "outside the glyph is matted away");
}

#[test]
fn pre_text_project_loads_with_default_text() {
    // A serialized layer missing the `text` field (old project) deserializes with
    // the default text (serde default), still a valid solid.
    let json = r#"{
        "name":"L","color":[1.0,0.0,0.0,1.0],"visible":true,
        "x":{"keys":[]},"y":{"keys":[]},"scale":{"keys":[]},
        "rotation":{"keys":[]},"opacity":{"keys":[]}
    }"#;
    let layer: PulseLayer = serde_json::from_str(json).unwrap();
    assert_eq!(layer.kind, LayerKind::Solid);
    assert_eq!(layer.text, TextLayer::default());
}
