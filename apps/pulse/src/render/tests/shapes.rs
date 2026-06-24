use super::*;


/// A 64x64 comp with a single centered shape layer holding one filled item.
fn shape(primitive: ShapePrimitive, fill: [f32; 3]) -> Comp {
    let mut c = solid([1.0, 1.0, 1.0, 1.0]);
    c.layers[0].kind = LayerKind::Shape;
    let mut item = ShapeItem::new(primitive);
    item.fill = Some(Fill {
        color: fill,
        opacity: 1.0,
    });
    c.layers[0].shape.items.push(item);
    c
}

#[test]
fn shape_layer_fills_its_center() {
    // A centered red-filled rectangle covers the comp center with its color.
    let c = shape(
        ShapePrimitive::Rectangle {
            half_w: 16.0,
            half_h: 16.0,
            radius: 0.0,
        },
        [1.0, 0.0, 0.0],
    );
    let [r, g, b, a] = render_frame(&c, 0.0).pixel(32, 32);
    assert_eq!(a, 255, "center is opaque");
    assert!(
        r > 250 && g == 0 && b == 0,
        "center is the fill red: {r},{g},{b}"
    );
    // A far corner is outside the 16px half-extent rect.
    assert_eq!(render_frame(&c, 0.0).pixel(2, 2)[3], 0, "corner uncovered");
}

#[test]
fn empty_shape_layer_renders_nothing() {
    // A shape layer with no items draws nothing (and doesn't fall back to a quad).
    let mut c = solid([1.0, 1.0, 1.0, 1.0]);
    c.layers[0].kind = LayerKind::Shape;
    let f = render_frame(&c, 0.0);
    assert!(f.pixels.iter().all(|&b| b == 0), "no items = transparent");
}

#[test]
fn shape_layer_honors_opacity() {
    // Half-opacity shape layer -> center alpha roughly halved.
    let mut c = shape(
        ShapePrimitive::Ellipse { rx: 16.0, ry: 16.0 },
        [1.0, 1.0, 1.0],
    );
    c.layers[0].opacity.set_key(0.0, 0.5);
    let a = render_frame(&c, 0.0).pixel(32, 32)[3];
    assert!((100..=160).contains(&a), "alpha ~half, got {a}");
}

#[test]
fn shape_ellipse_clips_the_corner() {
    // An ellipse leaves its bounding-box corners transparent.
    let c = shape(
        ShapePrimitive::Ellipse { rx: 20.0, ry: 20.0 },
        [1.0, 1.0, 1.0],
    );
    let f = render_frame(&c, 0.0);
    assert_eq!(f.pixel(32, 32)[3], 255, "center covered");
    // Diagonal point near the bbox corner of the circle is outside the disc.
    assert_eq!(f.pixel(48, 48)[3], 0, "circle corner uncovered");
}

#[test]
fn shape_layer_composes_with_mask() {
    // A big shape carved by a small centered mask: center survives, edge gone.
    let mut c = shape(
        ShapePrimitive::Rectangle {
            half_w: 24.0,
            half_h: 24.0,
            radius: 0.0,
        },
        [1.0, 1.0, 1.0],
    );
    c.layers[0].masks.push(Mask::rect(6.0, 6.0));
    let f = render_frame(&c, 0.0);
    assert_eq!(f.pixel(32, 32)[3], 255, "center passes the mask");
    assert_eq!(
        f.pixel(50, 32)[3],
        0,
        "shape pixel outside the mask is carved"
    );
}

#[test]
fn shape_stroke_outlines_an_unfilled_shape() {
    // A stroked, unfilled rectangle: the boundary is colored, the interior is
    // hollow.
    let mut c = solid([1.0, 1.0, 1.0, 1.0]);
    c.layers[0].kind = LayerKind::Shape;
    let mut item = ShapeItem::new(ShapePrimitive::Rectangle {
        half_w: 16.0,
        half_h: 16.0,
        radius: 0.0,
    });
    item.fill = None;
    item.stroke = Some(Stroke {
        color: [0.0, 0.0, 1.0],
        width: 4.0,
        opacity: 1.0,
    });
    c.layers[0].shape.items.push(item);
    let f = render_frame(&c, 0.0);
    // Interior (center) is hollow.
    assert_eq!(f.pixel(32, 32)[3], 0, "unfilled interior is transparent");
    // On the boundary (x ~= 32 + 16 = 48) the stroke is present and blue.
    let [r, _g, b, a] = f.pixel(48, 32);
    assert!(a > 100, "stroke band covered, got a={a}");
    assert!(b > r, "stroke reads blue");
}

#[test]
fn shape_layer_motion_blur_widens_the_footprint() {
    // A fast-sliding shape with comp motion blur smears its coverage across the
    // travel, so the center row is touched (alpha > 0) over a wider span of
    // columns than the crisp single-instant render. (The shape rasterizer is
    // antialiased, so we compare covered-column *count*, not partial-alpha.)
    let make = |blur: bool| {
        let mut c = shape(
            ShapePrimitive::Rectangle {
                half_w: 8.0,
                half_h: 8.0,
                radius: 0.0,
            },
            [1.0, 1.0, 1.0],
        );
        c.layers[0].x.set_key(0.0, -200.0);
        c.layers[0].x.set_key(1.0, 200.0);
        c.motion_blur.enabled = blur;
        c.motion_blur.angle = 720.0; // wide shutter so the smear is visible
        c.layers[0].motion_blur = blur;
        c
    };
    let covered_cols = |c: &Comp| {
        let f = render_frame(c, 0.5);
        (0..f.width).filter(|&x| f.pixel(x, 32)[3] > 0).count()
    };
    assert!(
        covered_cols(&make(true)) > covered_cols(&make(false)),
        "motion blur widens the swept footprint"
    );
}

#[test]
fn pre_shape_project_loads_with_empty_shape() {
    // A serialized layer missing the `shape` field (old project) deserializes
    // with an empty shape (serde default), still a valid solid.
    let json = r#"{
        "name":"L","color":[1.0,0.0,0.0,1.0],"visible":true,
        "x":{"keys":[]},"y":{"keys":[]},"scale":{"keys":[]},
        "rotation":{"keys":[]},"opacity":{"keys":[]}
    }"#;
    let layer: PulseLayer = serde_json::from_str(json).unwrap();
    assert!(layer.shape.is_empty());
    assert_eq!(layer.kind, LayerKind::Solid);
}
