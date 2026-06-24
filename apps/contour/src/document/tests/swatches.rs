use super::*;

/// A fresh document opens with the default starter palette.
#[test]
fn fresh_document_has_starter_palette() {
    let doc = Document::new();
    assert_eq!(doc.swatches.len(), 8);
    // The starter palette includes Black and Blue.
    assert!(doc.swatches.id_for_color([0.0, 0.0, 0.0, 1.0]).is_some());
}

/// A pre-swatches `.contour` (no `swatches` key) loads with the default starter
/// palette via `#[serde(default)]`.
#[test]
fn loads_pre_swatches_document() {
    let json = r#"{"shapes":[
        {"Rect":{"rect":[0,0,10,10],"fill":[1,0,0,1],"stroke":[0,0,0,1],"stroke_w":2}}
    ]}"#;
    let doc: Document = serde_json::from_str(json).unwrap();
    assert_eq!(doc.swatches.len(), 8);
}

/// A document's swatch palette round-trips through serde unchanged.
#[test]
fn swatches_round_trip() {
    let mut doc = Document::new();
    let id = doc.swatches.add("Brand", [0.3, 0.6, 0.9, 1.0]);
    doc.swatches.set_global(id, true);
    let json = serde_json::to_string(&doc).unwrap();
    let back: Document = serde_json::from_str(&json).unwrap();
    let sw = back.swatches.get(id).unwrap();
    assert_eq!(sw.name, "Brand");
    assert_eq!(sw.color, [0.3, 0.6, 0.9, 1.0]);
    assert!(sw.global);
}

/// `Document::remap_color` rewrites a colour across fills, strokes, and gradient
/// stops — the artwork half of a global-swatch recolour — and reports the number
/// of shapes touched.
#[test]
fn remap_color_recolors_matching_fills_strokes_and_gradients() {
    let old = [0.2, 0.2, 0.2, 1.0];
    let new = [0.8, 0.1, 0.1, 1.0];
    let mut doc = Document::new();
    doc.shapes.clear();
    // (0) fill matches, (1) stroke matches, (2) nothing matches, (3) gradient stop.
    doc.shapes.push(swatch_rect(old, [1.0, 1.0, 1.0, 1.0]));
    doc.shapes.push(swatch_rect([1.0, 1.0, 1.0, 1.0], old));
    doc.shapes
        .push(swatch_rect([0.5, 0.5, 0.5, 1.0], [0.0, 0.0, 0.0, 1.0]));
    let mut grad_shape = swatch_rect([1.0, 1.0, 1.0, 1.0], [0.0, 0.0, 0.0, 1.0]);
    grad_shape.set_fill_gradient(Some(crate::gradient::Gradient::two_stop(
        crate::gradient::GradientKind::Linear,
        old,
        [0.0, 0.0, 1.0, 1.0],
    )));
    doc.shapes.push(grad_shape);

    let n = doc.remap_color(old, new);
    assert_eq!(n, 3, "fill, stroke, and gradient-stop shapes changed");
    assert_eq!(doc.shapes[0].fill_color(), Some(new));
    assert_eq!(doc.shapes[1].stroke_color(), Some(new));
    // The untouched shape keeps its colours.
    assert_eq!(doc.shapes[2].fill_color(), Some([0.5, 0.5, 0.5, 1.0]));
    // The gradient stop was remapped.
    assert_eq!(doc.shapes[3].fill_gradient().unwrap().stops[0].color, new);
}

/// Remapping a colour to itself is a no-op that touches nothing.
#[test]
fn remap_color_to_same_color_is_noop() {
    let c = [0.4, 0.4, 0.4, 1.0];
    let mut doc = Document::new();
    doc.shapes.clear();
    doc.shapes.push(swatch_rect(c, [0.0, 0.0, 0.0, 1.0]));
    assert_eq!(doc.remap_color(c, c), 0);
}

/// End-to-end global-swatch flow (the composition the app's `recolor_swatch`
/// performs): painting shapes with a **global** swatch's colour, then editing
/// the swatch, re-colours every bound shape; a **non-global** swatch edit is a
/// one-time copy that leaves all artwork untouched.
#[test]
fn global_swatch_edit_recolors_bound_shapes_non_global_does_not() {
    let brand = [0.20, 0.40, 0.80, 1.0];
    let other = [0.90, 0.90, 0.90, 1.0];
    let mut doc = Document::new();
    doc.shapes.clear();
    // Two shapes painted with the swatch colour, one painted with something else.
    doc.shapes.push(swatch_rect(brand, [0.0, 0.0, 0.0, 1.0]));
    doc.shapes.push(swatch_rect(other, brand)); // stroke uses the colour
    doc.shapes.push(swatch_rect(other, [0.0, 0.0, 0.0, 1.0]));

    // --- Global swatch: editing it follows every shape painted with it. ---
    let id = doc.swatches.add("Brand", brand);
    doc.swatches.set_global(id, true);
    let edited = [0.85, 0.10, 0.20, 1.0];
    // The app composes recolor (gives the (old,new) pair for a global) with
    // remap_color (walks the artwork).
    let pair = doc.swatches.recolor(id, edited);
    assert_eq!(pair, Some((brand, edited)));
    let (old, new) = pair.unwrap();
    let n = doc.remap_color(old, new);
    assert_eq!(n, 2, "both shapes bound to the swatch colour re-coloured");
    assert_eq!(doc.shapes[0].fill_color(), Some(edited));
    assert_eq!(doc.shapes[1].stroke_color(), Some(edited));
    // The unrelated shape is untouched, and the swatch itself now holds the edit.
    assert_eq!(doc.shapes[2].fill_color(), Some(other));
    assert_eq!(doc.swatches.get(id).unwrap().color, edited);

    // --- Non-global swatch: editing it is a one-time copy, no artwork moves. ---
    // A distinct (non-global) swatch whose colour happens to match shape 2's fill.
    let plain = doc.swatches.add("Accent", other);
    assert!(!doc.swatches.get(plain).unwrap().global);
    let snapshot: Vec<_> = doc.shapes.iter().map(|s| s.fill_color()).collect();
    assert_eq!(doc.swatches.recolor(plain, [0.0, 1.0, 0.0, 1.0]), None);
    // No remap is performed (recolor returned None) → artwork is unchanged.
    let after: Vec<_> = doc.shapes.iter().map(|s| s.fill_color()).collect();
    assert_eq!(snapshot, after, "a non-global edit never touches the artwork");
    // …but the swatch colour itself did change.
    assert_eq!(doc.swatches.get(plain).unwrap().color, [0.0, 1.0, 0.0, 1.0]);
}
