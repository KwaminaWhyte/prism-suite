use super::*;

/// Old `.contour` JSON (pre-`visible`/`handles`) must still deserialize,
/// defaulting `visible = true` and `handles = []`.
#[test]
fn loads_legacy_document() {
    let json = r#"{"shapes":[
        {"Rect":{"rect":[0,0,10,10],"fill":[1,0,0,1],"stroke":[0,0,0,1],"stroke_w":2}},
        {"Path":{"points":[[0,0],[10,0],[10,10]],"closed":true,"fill":[0,1,0,1],"stroke":[0,0,0,1],"stroke_w":1}}
    ]}"#;
    let doc: Document = serde_json::from_str(json).unwrap();
    assert_eq!(doc.shapes.len(), 2);
    assert!(doc.shapes[0].visible());
    assert!(doc.shapes[1].visible());
    // A pre-group document loads with every shape ungrouped.
    assert_eq!(doc.shapes[0].group(), None);
    assert_eq!(doc.shapes[1].group(), None);
    // A pre-guides document loads with no guides.
    assert!(doc.guides.is_empty());
    // A pre-artboards document loads with exactly one default 1000×700 board.
    assert_eq!(doc.artboards.len(), 1);
    assert_eq!(doc.artboards[0].rect, [0.0, 0.0, 1000.0, 700.0]);
    assert_eq!(doc.active_artboard, 0);
    assert_eq!(
        doc.active_artboard().map(|a| a.rect),
        Some([0.0, 0.0, 1000.0, 700.0])
    );
    if let Shape::Path { handles, .. } = &doc.shapes[1] {
        assert!(handles.is_empty());
    } else {
        panic!("expected Path");
    }
    // A pre-live-shape document loads with every path as a plain (non-live) path.
    assert_eq!(doc.shapes[1].live_shape(), None);
    // A pre-appearance document loads with no explicit stack on any shape.
    assert!(doc.shapes[0].appearance().is_none());
    assert!(doc.shapes[1].appearance().is_none());
    // A pre-blend document loads with every shape un-blended.
    assert_eq!(doc.shapes[0].blend(), None);
    assert!(!doc.shapes[0].is_blend_step());
}

/// A pre-stroke-options `.contour` (a `stroke_style` with only caps/joins/dash)
/// loads with the new align / arrowhead fields at their defaults (center align,
/// no arrowheads, 1× scale) — so older files render unchanged.
#[test]
fn loads_legacy_stroke_style_with_default_align_and_arrows() {
    // `stroke_style` carries the original fields only; align / start_arrow /
    // end_arrow / arrow_scale are absent and must default.
    let json = r#"{"shapes":[
        {"Line":{"p0":[0,0],"p1":[10,0],"stroke":[0,0,0,1],"stroke_w":2,
                 "stroke_style":{"cap":"Round","join":"Miter","miter_limit":4,"dash":[12,6],"dash_offset":0}}}
    ]}"#;
    let doc: Document = serde_json::from_str(json).unwrap();
    let st = doc.shapes[0].stroke_style();
    assert_eq!(st.cap, LineCap::Round);
    assert!(st.is_dashed(), "legacy dash preserved");
    // New fields default cleanly.
    assert_eq!(st.align, StrokeAlign::Center);
    assert_eq!(st.start_arrow, Arrowhead::None);
    assert_eq!(st.end_arrow, Arrowhead::None);
    assert_eq!(st.arrow_scale, 1.0);
    assert!(!st.has_arrows());
}

/// The new stroke-options fields (align + arrowheads + scale) round-trip through
/// serde on a Shape's `stroke_style`.
#[test]
fn stroke_align_and_arrows_round_trip() {
    let mut s = Shape::Line {
        p0: (0.0, 0.0),
        p1: (10.0, 0.0),
        stroke: [0.0, 0.0, 0.0, 1.0],
        stroke_w: 2.0,
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
    {
        let st = s.stroke_style_mut();
        st.align = StrokeAlign::Outside;
        st.start_arrow = Arrowhead::Circle;
        st.end_arrow = Arrowhead::Triangle;
        st.arrow_scale = 1.75;
    }
    let doc = Document {
        shapes: vec![s],
        ..Default::default()
    };
    let json = serde_json::to_string(&doc).unwrap();
    let back: Document = serde_json::from_str(&json).unwrap();
    let st = back.shapes[0].stroke_style();
    assert_eq!(st.align, StrokeAlign::Outside);
    assert_eq!(st.start_arrow, Arrowhead::Circle);
    assert_eq!(st.end_arrow, Arrowhead::Triangle);
    assert_eq!(st.arrow_scale, 1.75);
}

/// Blend-set tags round-trip through serde on a Shape (back-compat: the new
/// `blend` / `blend_step` fields are additive, defaulting to un-blended).
#[test]
fn blend_tags_round_trip() {
    let mut s = Shape::Rect {
        rect: [0.0, 0.0, 10.0, 10.0],
        fill: [1.0, 0.0, 0.0, 1.0],
        fill_gradient: None,
        stroke: [0.0, 0.0, 0.0, 1.0],
        stroke_w: 2.0,
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
    s.set_blend(Some(7));
    s.set_blend_step(true);
    let doc = Document {
        shapes: vec![s],
        ..Default::default()
    };
    let json = serde_json::to_string(&doc).unwrap();
    let back: Document = serde_json::from_str(&json).unwrap();
    assert_eq!(back.shapes[0].blend(), Some(7));
    assert!(back.shapes[0].is_blend_step());
}

/// A live polygon / star's parameters round-trip through serde and keep the
/// generated geometry; the layer label follows the live kind.
#[test]
fn live_shape_round_trips_and_labels() {
    use crate::liveshape::LiveShape;
    let doc = Document {
        shapes: vec![
            live_path(LiveShape::Polygon {
                sides: 6,
                radius: 50.0,
                corner_radius: 0.0,
            }),
            live_path(LiveShape::Star {
                points: 5,
                radius: 40.0,
                inner_ratio: 0.5,
            }),
        ],
        ..Default::default()
    };
    assert_eq!(doc.shapes[0].label(), "Polygon");
    assert_eq!(doc.shapes[1].label(), "Star");

    let json = serde_json::to_string(&doc).unwrap();
    let back: Document = serde_json::from_str(&json).unwrap();
    assert_eq!(
        back.shapes[0].live_shape(),
        Some(LiveShape::Polygon {
            sides: 6,
            radius: 50.0,
            corner_radius: 0.0,
        })
    );
    assert_eq!(
        back.shapes[1].live_shape(),
        Some(LiveShape::Star {
            points: 5,
            radius: 40.0,
            inner_ratio: 0.5
        })
    );
    // The polygon's six vertices survived serialization.
    if let Shape::Path { points, .. } = &back.shapes[0] {
        assert_eq!(points.len(), 6);
    } else {
        panic!("expected Path");
    }
}

/// Editing a live shape's parameters regenerates its outline about the current
/// centre (so a moved shape stays put), and directly editing an anchor demotes
/// it to a plain path.
#[test]
fn live_shape_regenerates_and_demotes_on_anchor_edit() {
    use crate::liveshape::LiveShape;
    let mut s = live_path(LiveShape::Polygon {
        sides: 4,
        radius: 10.0,
        corner_radius: 0.0,
    });
    // Move it, then bump the side count: the new outline keeps the moved centre.
    s.translate(100.0, 0.0);
    assert!(s.set_live_shape(LiveShape::Polygon {
        sides: 8,
        radius: 10.0,
        corner_radius: 0.0,
    }));
    if let Shape::Path { points, .. } = &s {
        assert_eq!(points.len(), 8, "regenerated to the new side count");
        let cx = points.iter().map(|p| p.0).sum::<f32>() / points.len() as f32;
        assert!((cx - 100.0).abs() < 1e-2, "centre stayed at the moved x");
    } else {
        panic!("expected Path");
    }
    // Directly editing an anchor drops the live parameters (it becomes a plain
    // editable path, Illustrator-style).
    assert!(s.set_anchor(0, 0, 5.0, 5.0));
    assert_eq!(s.live_shape(), None);
}

/// A shape with no explicit `appearance` migrates its legacy single fill/stroke
/// into a one-fill / one-stroke effective stack on demand.
#[test]
fn legacy_shape_migrates_to_one_fill_one_stroke() {
    let json = r#"{"shapes":[
        {"Rect":{"rect":[0,0,10,10],"fill":[1,0,0,1],"stroke":[0,0,0,1],"stroke_w":2}}
    ]}"#;
    let doc: Document = serde_json::from_str(json).unwrap();
    let ap = doc.shapes[0].effective_appearance();
    assert_eq!(ap.fills.len(), 1, "one fill migrated from the legacy fill");
    assert_eq!(ap.strokes.len(), 1, "one stroke migrated from the legacy stroke");
    assert_eq!(ap.fills[0].paint.swatch(), [1.0, 0.0, 0.0, 1.0]);
    assert_eq!(ap.strokes[0].width, 2.0);
}

/// An explicit stacked appearance round-trips through serde on a Shape and is
/// preferred over the legacy fields by `effective_appearance`.
#[test]
fn appearance_round_trips_on_shape_and_overrides_legacy() {
    use crate::appearance::{Appearance, Fill};
    let mut s = Shape::Rect {
        rect: [0.0, 0.0, 10.0, 10.0],
        fill: [1.0, 0.0, 0.0, 1.0],
        fill_gradient: None,
        stroke: [0.0, 0.0, 0.0, 1.0],
        stroke_w: 2.0,
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
    // Two stacked fills override the single legacy red fill.
    s.set_appearance(Some(Appearance {
        fills: vec![
            Fill::solid([0.0, 1.0, 0.0, 1.0]),
            Fill::solid([0.0, 0.0, 1.0, 0.5]),
        ],
        strokes: vec![],
        effects: vec![],
    }));
    let doc = Document {
        shapes: vec![s],
        ..Default::default()
    };
    let json = serde_json::to_string(&doc).unwrap();
    let back: Document = serde_json::from_str(&json).unwrap();
    let ap = back.shapes[0].effective_appearance();
    assert_eq!(ap.fills.len(), 2, "stacked fills survive the round-trip");
    assert_eq!(ap.fills[1].paint.swatch(), [0.0, 0.0, 1.0, 0.5]);
    // The legacy `fill` field is untouched but ignored when a stack is present.
    assert_eq!(back.shapes[0].fill_color(), Some([1.0, 0.0, 0.0, 1.0]));
}

/// A gradient fill (with the new Angle kind, perceptual interpolation, dither,
/// multi-stop + per-stop opacity) round-trips through serde on a Shape unchanged.
#[test]
fn gradient_fill_round_trips_on_shape() {
    use crate::gradient::{Gradient, GradientKind, GradientStop, Interpolation, SpreadMode};
    let grad = Gradient {
        kind: GradientKind::Angle,
        stops: vec![
            GradientStop::new(0.0, [1.0, 0.0, 0.0, 1.0]),
            GradientStop::new(0.5, [0.0, 1.0, 0.0, 0.5]),
            GradientStop::new(1.0, [0.0, 0.0, 1.0, 0.0]),
        ],
        angle: 45.0,
        spread: SpreadMode::Reflect,
        interpolation: Interpolation::Perceptual,
        dither: true,
    };
    let mut s = Shape::Rect {
        rect: [0.0, 0.0, 10.0, 10.0],
        fill: [1.0, 0.0, 0.0, 1.0],
        fill_gradient: None,
        stroke: [0.0, 0.0, 0.0, 1.0],
        stroke_w: 2.0,
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
    s.set_fill_gradient(Some(grad.clone()));
    let doc = Document {
        shapes: vec![s],
        ..Default::default()
    };
    let json = serde_json::to_string(&doc).unwrap();
    let back: Document = serde_json::from_str(&json).unwrap();
    assert_eq!(back.shapes[0].fill_gradient(), Some(&grad));
}

/// A legacy gradient JSON (predating the `interpolation` / `dither` fields) loads
/// with the back-compat defaults — sRGB interpolation + dither off — so older
/// `.contour` files render byte-identically to how they were authored.
#[test]
fn legacy_gradient_loads_with_back_compat_defaults() {
    use crate::gradient::{GradientKind, Interpolation};
    // Note: no `interpolation` / `dither` keys, mirroring a file saved before the
    // perceptual/dither feature landed.
    let json = r#"{"shapes":[
        {"Rect":{"rect":[0,0,10,10],"fill":[1,0,0,1],"stroke":[0,0,0,1],"stroke_w":1,
                 "fill_gradient":{"kind":"Linear",
                   "stops":[{"offset":0.0,"color":[0,0,0,1]},{"offset":1.0,"color":[1,1,1,1]}],
                   "angle":0.0,"spread":"Pad"}}}
    ]}"#;
    let doc: Document = serde_json::from_str(json).unwrap();
    let g = doc.shapes[0].fill_gradient().expect("gradient loaded");
    assert_eq!(g.kind, GradientKind::Linear);
    assert_eq!(g.stops.len(), 2);
    // The absent fields take their back-compat defaults.
    assert_eq!(g.interpolation, Interpolation::Srgb);
    assert!(!g.dither);
}

/// Guides round-trip through serde and load back as the same variant.
#[test]
fn guides_round_trip() {
    let mut doc = Document::new();
    doc.guides.push(Guide::Vertical(100.0));
    doc.guides.push(Guide::Horizontal(42.5));
    let json = serde_json::to_string(&doc).unwrap();
    let back: Document = serde_json::from_str(&json).unwrap();
    assert_eq!(
        back.guides,
        vec![Guide::Vertical(100.0), Guide::Horizontal(42.5)]
    );
}

/// Placed images carried on the document round-trip through serde, and a fresh
/// document has none.
#[test]
fn placed_images_round_trip_on_document() {
    use crate::placed_image::{ImageSource, PlacedImage};
    let doc = Document::new();
    assert!(doc.placed_images.is_empty(), "fresh document places no images");

    let mut doc = Document::new();
    let id = doc.placed_images.place(
        "logo",
        ImageSource::Embedded {
            width: 2,
            height: 2,
            rgba: vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16],
        },
        10.0,
        20.0,
    );
    doc.placed_images
        .get_mut(id)
        .unwrap()
        .set_clip(vec![(0.0, 0.0), (5.0, 0.0), (5.0, 5.0)]);
    doc.placed_images.list.push(PlacedImage::new(
        9,
        "linked",
        ImageSource::Linked {
            path: std::path::PathBuf::from("/tmp/a.png"),
            width: 4,
            height: 3,
        },
        1.0,
        2.0,
    ));
    let json = serde_json::to_string(&doc).unwrap();
    let back: Document = serde_json::from_str(&json).unwrap();
    assert_eq!(back.placed_images.len(), 2);
    assert!(back.placed_images.get(id).unwrap().clip.is_some());
    assert_eq!(
        back.placed_images.list[1].source.natural_size(),
        (4, 3),
        "linked natural size preserved"
    );
}

/// A pre-Place `.contour` (JSON missing the `placed_images` key) loads with an
/// empty placed-image collection — the additive default.
#[test]
fn legacy_document_loads_without_placed_images() {
    // A minimal legacy document: just a shapes list (the other fields are all
    // `#[serde(default)]`).
    let json = r#"{ "shapes": [] }"#;
    let doc: Document = serde_json::from_str(json).unwrap();
    assert!(doc.placed_images.is_empty());
}

/// `.contour` files written before stroke styles existed must load with a
/// default (solid, butt, miter) stroke style on every shape.
#[test]
fn loads_pre_stroke_style_document() {
    let json = r#"{"shapes":[
        {"Line":{"p0":[0,0],"p1":[10,0],"stroke":[0,0,0,1],"stroke_w":2}}
    ]}"#;
    let doc: Document = serde_json::from_str(json).unwrap();
    assert_eq!(doc.shapes.len(), 1);
    let st = doc.shapes[0].stroke_style();
    assert_eq!(st, &StrokeStyle::default());
}
