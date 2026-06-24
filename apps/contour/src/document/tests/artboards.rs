use super::*;

/// A fresh document has exactly one default artboard, active.
#[test]
fn fresh_document_has_one_default_artboard() {
    let doc = Document::new();
    assert_eq!(doc.artboards.len(), 1);
    assert_eq!(doc.artboards[0].rect, [0.0, 0.0, 1000.0, 700.0]);
    assert_eq!(doc.active_artboard, 0);
}

/// Artboards and the active index round-trip through serde unchanged.
#[test]
fn artboards_round_trip() {
    let mut doc = Document::new();
    doc.artboards.push(crate::artboard::Artboard::new(
        "Mobile",
        [1100.0, 0.0, 375.0, 812.0],
    ));
    doc.active_artboard = 1;
    let json = serde_json::to_string(&doc).unwrap();
    let back: Document = serde_json::from_str(&json).unwrap();
    assert_eq!(back.artboards.len(), 2);
    assert_eq!(back.artboards[1].name, "Mobile");
    assert_eq!(back.artboards[1].rect, [1100.0, 0.0, 375.0, 812.0]);
    assert_eq!(back.active_artboard, 1);
}

/// `normalize_artboards` repairs an empty stack / out-of-range active index.
#[test]
fn normalize_repairs_artboards() {
    let mut doc = Document::new();
    doc.artboards.clear(); // simulate a corrupt / hand-edited file
    doc.active_artboard = 9;
    doc.normalize_artboards();
    assert_eq!(doc.artboards.len(), 1);
    assert_eq!(doc.active_artboard, 0);

    // Out-of-range active with valid boards clamps to the last board.
    let mut doc2 = Document::new();
    doc2.artboards
        .push(crate::artboard::Artboard::new("b", [0.0, 0.0, 10.0, 10.0]));
    doc2.active_artboard = 7;
    doc2.normalize_artboards();
    assert_eq!(doc2.active_artboard, 1);
}

#[test]
fn rects_intersect_basic_cases() {
    let a = [0.0, 0.0, 10.0, 10.0];
    // Overlapping.
    assert!(rects_intersect(&a, &[5.0, 5.0, 10.0, 10.0]));
    // Fully inside.
    assert!(rects_intersect(&a, &[2.0, 2.0, 2.0, 2.0]));
    // Contains a (b bigger).
    assert!(rects_intersect(&a, &[-5.0, -5.0, 100.0, 100.0]));
    // Edge-touching counts.
    assert!(rects_intersect(&a, &[10.0, 0.0, 5.0, 5.0]));
    // Disjoint on x.
    assert!(!rects_intersect(&a, &[20.0, 0.0, 5.0, 5.0]));
    // Disjoint on y.
    assert!(!rects_intersect(&a, &[0.0, 20.0, 5.0, 5.0]));
}
