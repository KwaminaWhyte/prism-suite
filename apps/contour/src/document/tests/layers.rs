use super::*;

/// A **locked** shape is excluded from selection / hit-testing via the shared
/// [`Shape::selectable`] gate (it still reports `visible`).
#[test]
fn locked_shape_is_not_selectable() {
    let mut s = layer_rect();
    assert!(s.selectable(), "an unlocked, visible shape is selectable");
    s.set_locked(true);
    assert!(s.locked());
    assert!(s.visible(), "locking doesn't change visibility");
    assert!(!s.selectable(), "a locked shape can't be selected / picked");
    // It still geometrically contains the point — only the *gate* blocks the pick.
    assert!(s.hit(5.0, 5.0, 1.0), "geometry is unchanged by the lock");
    s.toggle_locked();
    assert!(s.selectable(), "unlocking restores selectability");
}

/// A **hidden** shape is excluded from selection / hit-testing via the shared
/// gate (the canvas pick paths use `selectable()`, the renderers use `visible()`).
#[test]
fn hidden_shape_is_not_selectable() {
    let mut s = layer_rect();
    s.toggle_visible();
    assert!(!s.visible());
    assert!(!s.selectable(), "a hidden shape can't be picked");
    // The hit-test geometry itself is unaffected; the gate is what excludes it.
    assert!(s.hit(5.0, 5.0, 1.0));
}

/// `selectable()` requires **both** visible and unlocked.
#[test]
fn selectable_requires_visible_and_unlocked() {
    let mut s = layer_rect();
    s.set_locked(true);
    s.toggle_visible(); // now hidden AND locked
    assert!(!s.selectable());
    s.set_locked(false);
    assert!(!s.selectable(), "still hidden");
    s.toggle_visible(); // now visible AND unlocked
    assert!(s.selectable());
}

/// The new Layers-panel metadata (name / locked / layer-colour) round-trips
/// through serde on a Shape, alongside the existing `visible` flag.
#[test]
fn layer_metadata_round_trips() {
    let mut s = layer_rect();
    s.set_name("Hero badge");
    s.set_locked(true);
    s.set_layer_color(Some([0.2, 0.4, 0.6, 1.0]));
    s.toggle_visible(); // hidden
    let doc = Document {
        shapes: vec![s],
        ..Default::default()
    };
    let json = serde_json::to_string(&doc).unwrap();
    let back: Document = serde_json::from_str(&json).unwrap();
    let r = &back.shapes[0];
    assert_eq!(r.name(), Some("Hero badge"));
    assert_eq!(r.display_name(), "Hero badge");
    assert!(r.locked());
    assert!(!r.visible());
    assert_eq!(r.layer_color(), Some([0.2, 0.4, 0.6, 1.0]));
}

/// A pre-Layers-panel `.contour` (no `name` / `locked` / `layer_color` keys)
/// deserializes with the additive defaults: unnamed (falls back to the type
/// label), unlocked, no layer colour — so older files load unchanged.
#[test]
fn legacy_document_defaults_layer_metadata() {
    let json = r#"{"shapes":[
        {"Rect":{"rect":[0,0,10,10],"fill":[1,0,0,1],"stroke":[0,0,0,1],"stroke_w":2}}
    ]}"#;
    let doc: Document = serde_json::from_str(json).unwrap();
    let r = &doc.shapes[0];
    assert_eq!(r.name(), None, "no stored name");
    assert_eq!(r.display_name(), "Rectangle", "falls back to the type label");
    assert!(!r.locked(), "unlocked by default");
    assert_eq!(r.layer_color(), None, "no layer colour by default");
    assert!(r.selectable(), "a legacy shape is selectable");
}

/// A blank name clears back to the type label (stored as `None`, not `""`).
#[test]
fn blank_name_clears_to_label() {
    let mut s = layer_rect();
    s.set_name("Renamed");
    assert_eq!(s.name(), Some("Renamed"));
    s.set_name("   ");
    assert_eq!(s.name(), None, "a blank name is cleared");
    assert_eq!(s.display_name(), "Rectangle");
}

/// `to_path` carries the Layers-panel metadata onto the converted path so a
/// rotated rectangle (which rasterises to a `Path`) keeps its name / lock /
/// colour.
#[test]
fn to_path_preserves_layer_metadata() {
    let mut s = layer_rect();
    s.set_name("Box");
    s.set_locked(true);
    s.set_layer_color(Some([0.1, 0.2, 0.3, 1.0]));
    let p = s.to_path();
    assert_eq!(p.name(), Some("Box"));
    assert!(p.locked());
    assert_eq!(p.layer_color(), Some([0.1, 0.2, 0.3, 1.0]));
}
