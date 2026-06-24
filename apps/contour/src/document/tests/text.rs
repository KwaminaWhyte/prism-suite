use super::*;

// --- Type / text objects -----------------------------------------------

/// A freshly-built text object has a non-empty glyph cache and tight bounds, and
/// fills under the even-odd rule (so glyph counters are holes).
#[test]
fn text_shape_has_glyphs_bounds_and_even_odd_fill() {
    let s = text_shape("Hi", (10.0, 20.0));
    match &s {
        Shape::Text { glyphs, .. } => assert!(!glyphs.is_empty(), "glyphs laid out"),
        _ => panic!("expected Text"),
    }
    let b = s.bounds().expect("text has bounds");
    assert!(b.w > 0.0 && b.h > 0.0, "non-empty bbox");
    assert_eq!(s.fill_rule(), Some(FillRule::EvenOdd));
    assert_eq!(s.label(), "Type");
    // The display name shows the string.
    assert_eq!(s.display_name(), "Hi");
}

/// Editing the params via `set_text_params` re-lays-out the glyph cache, growing
/// the bounds as the string lengthens.
#[test]
fn set_text_params_relays_out_glyphs() {
    let mut s = text_shape("I", (0.0, 0.0));
    let before = s.bounds().unwrap().w;
    let ok = s.set_text_params(TextParams {
        text: "IIIIIIII".into(),
        font_size: 72.0,
        align: TextAlign::Left,
        font_family: None,
        font_axes: Default::default(),
    });
    assert!(ok, "set_text_params applies to a text object");
    let after = s.bounds().unwrap().w;
    assert!(after > before, "wider string widens the bbox: {before} -> {after}");
    // A non-text shape rejects the edit.
    let mut r = layer_rect();
    assert!(!r.set_text_params(TextParams::default()));
}

/// Convert-to-outlines turns a text object into a `Compound` of glyph contours,
/// preserving paint + even-odd fill rule, with non-empty geometry.
#[test]
fn text_to_outlines_yields_compound_paths() {
    let mut s = text_shape("Ag", (5.0, 5.0));
    s.set_fill_color([0.2, 0.4, 0.8, 1.0]);
    let out = s.text_to_outlines();
    match out {
        Shape::Compound {
            subpaths,
            fill_rule,
            fill,
            ..
        } => {
            assert!(!subpaths.is_empty(), "glyph contours become sub-paths");
            assert!(
                subpaths.iter().any(|sp| sp.points.len() >= 3),
                "real geometry"
            );
            assert_eq!(fill_rule, FillRule::EvenOdd, "counters stay holes");
            assert_eq!(fill, [0.2, 0.4, 0.8, 1.0], "paint carried through");
        }
        other => panic!("expected Compound, got {other:?}"),
    }
}

/// Translating a text object moves its origin *and* its cached glyph outlines.
#[test]
fn text_translate_moves_origin_and_glyphs() {
    let mut s = text_shape("X", (0.0, 0.0));
    let b0 = s.bounds().unwrap();
    s.translate(100.0, 50.0);
    let b1 = s.bounds().unwrap();
    assert!((b1.x - (b0.x + 100.0)).abs() < 1e-2);
    assert!((b1.y - (b0.y + 50.0)).abs() < 1e-2);
    if let Shape::Text { origin, .. } = &s {
        assert_eq!(*origin, (100.0, 50.0), "editable origin tracks the move");
    } else {
        panic!("still text");
    }
}

/// A text object round-trips through serde (params + origin + glyph cache + the
/// alignment field).
#[test]
fn text_round_trips_through_serde() {
    let mut s = text_shape("Round\nTrip", (12.0, 34.0));
    s.set_text_params(TextParams {
        text: "Round\nTrip".into(),
        font_size: 48.0,
        align: TextAlign::Center,
        font_family: None,
        font_axes: Default::default(),
    });
    let doc = Document {
        shapes: vec![s],
        ..Default::default()
    };
    let json = serde_json::to_string(&doc).unwrap();
    let back: Document = serde_json::from_str(&json).unwrap();
    match &back.shapes[0] {
        Shape::Text {
            params,
            origin,
            glyphs,
            ..
        } => {
            assert_eq!(params.text, "Round\nTrip");
            assert_eq!(params.font_size, 48.0);
            assert_eq!(params.align, TextAlign::Center);
            assert_eq!(*origin, (12.0, 34.0));
            assert!(!glyphs.is_empty(), "glyph cache survives the round-trip");
        }
        other => panic!("expected Text, got {other:?}"),
    }
}

/// A hand-written / legacy text object missing the additive `glyphs` and `align`
/// keys deserializes with the back-compat defaults (empty cache, left align), and
/// `relayout_text` rebuilds the glyphs from `params` + `origin`.
#[test]
fn text_serde_defaults_and_relayout_rebuilds_cache() {
    let json = r#"{"shapes":[
        {"Text":{
            "params":{"text":"Hi","font_size":72.0},
            "origin":[0.0,0.0],
            "fill":[0,0,0,1],"stroke":[0,0,0,1],"stroke_w":0
        }}
    ]}"#;
    let mut doc: Document = serde_json::from_str(json).unwrap();
    match &doc.shapes[0] {
        Shape::Text { params, glyphs, .. } => {
            assert_eq!(params.align, TextAlign::Left, "align defaults to Left");
            assert!(glyphs.is_empty(), "glyph cache defaults to empty");
        }
        _ => panic!("expected Text"),
    }
    // Repairing the document lays the glyphs out from the params.
    doc.relayout_text();
    match &doc.shapes[0] {
        Shape::Text { glyphs, .. } => assert!(!glyphs.is_empty(), "relayout filled the cache"),
        _ => panic!("expected Text"),
    }
    // And the text now has real bounds.
    assert!(doc.shapes[0].bounds().is_some());
}

/// A text object hit-tests by its bounding box (so it is easy to select).
#[test]
fn text_hit_tests_inside_bounds() {
    let s = text_shape("Hi", (0.0, 0.0));
    let b = s.bounds().unwrap();
    let (cx, cy) = (b.x + b.w * 0.5, b.y + b.h * 0.5);
    assert!(s.hit(cx, cy, 0.1), "centre of the text box hits");
    assert!(!s.hit(b.x + b.w + 100.0, cy, 0.1), "far outside misses");
}

/// A pre-graphic-styles `.contour` (no `graphic_styles` key) loads with an empty
/// style library — the additive `#[serde(default)]` field keeps older files
/// round-tripping unchanged.
#[test]
fn loads_legacy_document_with_empty_graphic_styles() {
    let json = r#"{"shapes":[
        {"Rect":{"rect":[0,0,10,10],"fill":[1,0,0,1],"stroke":[0,0,0,1],"stroke_w":2}}
    ]}"#;
    let doc: Document = serde_json::from_str(json).unwrap();
    assert!(doc.graphic_styles.is_empty(), "missing key defaults to empty");
}

/// The document's graphic-styles library — each entry a full `Appearance`
/// snapshot — round-trips through `.contour` serialization unchanged.
#[test]
fn graphic_styles_library_round_trips_on_document() {
    use crate::appearance::{Appearance, BlendMode, Effect, Fill, Paint, Stroke as AppStroke};
    let style = Appearance {
        fills: vec![
            Fill::solid([0.1, 0.2, 0.3, 1.0]),
            Fill {
                paint: Paint::Gradient(crate::gradient::Gradient::default()),
                opacity: 0.5,
                blend: BlendMode::Multiply,
                visible: false,
            },
        ],
        strokes: vec![AppStroke {
            paint: Paint::Solid([1.0, 0.0, 0.0, 0.8]),
            width: 3.0,
            style: StrokeStyle::default(),
            opacity: 0.75,
            blend: BlendMode::Screen,
            visible: true,
        }],
        effects: vec![Effect::drop_shadow()],
    };
    let mut doc = Document::default();
    let id = doc.graphic_styles.add("Card", style.clone());

    let json = serde_json::to_string(&doc).unwrap();
    let back: Document = serde_json::from_str(&json).unwrap();
    assert_eq!(back.graphic_styles.len(), 1, "the style survives the round-trip");
    assert_eq!(back.graphic_styles.get(id).map(|s| s.name.as_str()), Some("Card"));
    // The whole captured appearance stack is preserved byte-for-byte.
    assert_eq!(back.graphic_styles.appearance_of(id), Some(&style));
}

/// A pre-symbols `.contour` (no `symbols` key) loads with an empty symbol
/// library — the additive `#[serde(default)]` field keeps older files
/// round-tripping unchanged.
#[test]
fn loads_legacy_document_with_empty_symbols() {
    let json = r#"{"shapes":[
        {"Rect":{"rect":[0,0,10,10],"fill":[1,0,0,1],"stroke":[0,0,0,1],"stroke_w":2}}
    ]}"#;
    let doc: Document = serde_json::from_str(json).unwrap();
    assert!(doc.symbols.is_empty(), "missing key defaults to empty");
    assert!(doc.symbols.instances.is_empty());
}

/// The document's symbol library + placed instances round-trip through `.contour`
/// serialization, and a master edit (re-serialized) propagates to instances.
#[test]
fn symbols_round_trip_and_propagate_on_document() {
    use crate::transform::Affine;
    let mut doc = Document::default();
    let sq = Shape::Rect {
        rect: [0.0, 0.0, 10.0, 10.0],
        fill: [1.0, 0.0, 0.0, 1.0],
        fill_gradient: None,
        stroke: [0.0, 0.0, 0.0, 1.0],
        stroke_w: 1.0,
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
    let id = doc.symbols.add("Box", vec![sq]);
    doc.symbols.place(id, Affine::translate(100.0, 0.0));
    doc.symbols.place(id, Affine::translate(0.0, 100.0));

    // Round-trips: re-serializing the loaded doc matches the original JSON.
    let json = serde_json::to_string(&doc).unwrap();
    let back: Document = serde_json::from_str(&json).unwrap();
    assert_eq!(json, serde_json::to_string(&back).unwrap());
    assert_eq!(back.symbols.len(), 1);
    assert_eq!(back.symbols.instances.len(), 2);

    // Edit the master to 50 wide → both instances resolve 50 wide.
    let mut wider = back.symbols.get(id).unwrap().shapes[0].clone();
    if let Shape::Rect { rect, .. } = &mut wider {
        rect[2] = 50.0;
    }
    let mut edited = back;
    edited.symbols.set_master_shapes(id, vec![wider]);
    for inst in &edited.symbols.instances {
        let r = edited.symbols.resolve(inst);
        match &r[0] {
            Shape::Rect { rect, .. } => assert_eq!(rect[2], 50.0),
            _ => unreachable!(),
        }
    }
}

/// Changing a text object's **font size** (the inspector's Size path, which routes
/// through [`Shape::set_text_params`]) re-lays-out its glyphs but must keep the
/// object where the user placed it: the editable `origin` is untouched and the
/// re-extracted glyphs stay anchored at that origin — they do **not** jump to the
/// canvas corner (the Pigment-class "edit resets position to top-left" bug).
#[test]
fn text_size_change_preserves_origin_and_placement() {
    let origin = (137.0, 84.0);
    let mut shape = placed_text("Ag", 40.0, origin);

    // Top-left of the laid-out glyphs before the edit (should sit near `origin`).
    let before = shape.bounds().expect("placed text has bounds");

    let mut new = shape.text_params().unwrap().clone();
    new.font_size = 96.0; // larger size, fresh glyph extraction
    assert!(shape.set_text_params(new), "set_text_params applies to text");

    // The placement anchor is untouched.
    match &shape {
        Shape::Text { origin: o, .. } => assert_eq!(*o, origin, "origin must not move"),
        _ => panic!("still a text object"),
    }

    // The re-laid-out glyphs are still anchored at the origin: their top-left
    // tracks `origin.x` / `origin.y` exactly as before the edit (a small em-box
    // offset, never the (0,0) corner). The x-origin is exact; the y top lands a
    // hair below `origin.y` (the em-box top), and that offset is stable across
    // sizes only up to scale — so we assert the box did not jump to the corner.
    let after = shape.bounds().expect("resized text still has bounds");
    assert!(
        (after.x - before.x).abs() < 1.0,
        "glyph left edge stays put ({} vs {})",
        after.x,
        before.x
    );
    assert!(
        after.x > origin.0 - 1.0 && after.y > origin.1 - 1.0,
        "glyphs stay anchored at the placed origin, not the (0,0) corner: {:?}",
        (after.x, after.y)
    );
    assert!(
        after.w > before.w,
        "the bigger size produced wider glyphs (proves a real relayout happened)"
    );
}

/// Changing a text object's **font family** (the inspector's Font dropdown path,
/// also through [`Shape::set_text_params`]) re-extracts glyph outlines from a
/// different face but must not move the object: the `origin` is preserved and the
/// glyphs stay anchored there. Locks in that Contour does not have the
/// font-change-resets-position bug found in the sibling app.
#[test]
fn text_font_change_preserves_position() {
    let origin = (250.5, 60.0);
    let mut shape = placed_text("Hi", 50.0, origin);
    let before = shape.bounds().expect("placed text has bounds");

    let mut new = shape.text_params().unwrap().clone();
    // An unknown family resolves to the bundled face (so this runs identically on
    // any host), but it still exercises the full re-extract-on-family-change path.
    new.font_family = Some("No Such Font 99999".to_string());
    assert!(shape.set_text_params(new), "set_text_params applies to text");

    match &shape {
        Shape::Text { origin: o, params, .. } => {
            assert_eq!(*o, origin, "origin must survive a font-family change");
            assert_eq!(
                params.font_family.as_deref(),
                Some("No Such Font 99999"),
                "the chosen family is recorded"
            );
        }
        _ => panic!("still a text object"),
    }

    let after = shape.bounds().expect("re-faced text still has bounds");
    assert!(
        (after.x - before.x).abs() < 1.0 && (after.y - before.y).abs() < 1.0,
        "the text did not jump on a font change: {:?} -> {:?}",
        (before.x, before.y),
        (after.x, after.y)
    );
}

/// A text object that is **moved** and then has a property changed stays at the
/// moved location: translate keeps `origin` and the glyph cache in sync, and a
/// later [`Shape::set_text_params`] re-lays-out about the moved origin. This is
/// the end-to-end "place it, move it, change font/size — does it stay put?" path.
#[test]
fn moved_text_keeps_position_after_property_change() {
    let mut shape = placed_text("Ag", 48.0, (10.0, 10.0));
    shape.translate(200.0, 150.0); // drag the object across the canvas
    let moved_origin = match &shape {
        Shape::Text { origin, .. } => *origin,
        _ => unreachable!(),
    };
    assert_eq!(moved_origin, (210.0, 160.0), "translate moved the origin");
    let before = shape.bounds().expect("moved text has bounds");

    // Now change size + family in one edit, as the inspector does.
    let mut new = shape.text_params().unwrap().clone();
    new.font_size = 24.0;
    new.font_family = Some("Some Other Font".to_string());
    shape.set_text_params(new);

    match &shape {
        Shape::Text { origin, .. } => {
            assert_eq!(*origin, moved_origin, "origin stays at the moved location");
        }
        _ => panic!("still text"),
    }
    let after = shape.bounds().expect("edited text has bounds");
    // Left/top stay anchored near the moved origin (allowing the em-box offset),
    // and certainly nowhere near the original (10,10) placement or the corner.
    assert!(
        after.x > moved_origin.0 - 1.0 && after.y > moved_origin.1 - 1.0,
        "glyphs stay anchored at the moved origin after the edit: {:?}",
        (after.x, after.y)
    );
    assert!(
        (after.x - before.x).abs() < 1.0,
        "left edge unchanged by the edit ({} vs {})",
        after.x,
        before.x
    );
}
