use super::*;

/// `render_shapes` resolves a clip set: the mask paints nothing, and the clipped
/// content is cropped to the mask outline.
#[test]
fn render_shapes_resolves_a_clip_set() {
    let mut doc = Document::new();
    // A 20×20 content rect clipped by a 10×10 mask (set id 0; the mask is on top).
    doc.shapes
        .push(clip_rect([0.0, 0.0, 20.0, 20.0], Some(0), false));
    doc.shapes
        .push(clip_rect([0.0, 0.0, 10.0, 10.0], Some(0), true));
    // Plus a loose rect that should pass through untouched.
    doc.shapes
        .push(clip_rect([50.0, 50.0, 5.0, 5.0], None, false));

    let rendered = doc.render_shapes();
    // The mask is omitted; the content (clipped) + the loose rect remain.
    assert_eq!(rendered.len(), 2);
    let indices: Vec<usize> = rendered.iter().map(|(i, _)| *i).collect();
    assert!(indices.contains(&0)); // clipped content kept (original index 0)
    assert!(indices.contains(&2)); // loose rect kept
    assert!(!indices.contains(&1)); // mask dropped

    // The clipped content's bounds shrink to the 10×10 mask region.
    let content = &rendered.iter().find(|(i, _)| *i == 0).unwrap().1;
    let b = content.bounds().unwrap();
    assert!((b.x - 0.0).abs() < 1e-2 && (b.y - 0.0).abs() < 1e-2);
    assert!((b.w - 10.0).abs() < 1e-2 && (b.h - 10.0).abs() < 1e-2);
}

/// Content lying entirely outside the mask is dropped from the render.
#[test]
fn render_shapes_drops_content_outside_the_mask() {
    let mut doc = Document::new();
    doc.shapes
        .push(clip_rect([100.0, 100.0, 10.0, 10.0], Some(0), false));
    doc.shapes
        .push(clip_rect([0.0, 0.0, 10.0, 10.0], Some(0), true));
    let rendered = doc.render_shapes();
    // Disjoint content clips to nothing; the mask paints nothing → empty render.
    assert!(rendered.is_empty());
}

/// A clip set round-trips through serde, and clearing the tags (Release) restores
/// the originals so `render_shapes` returns every shape unclipped.
#[test]
fn clip_tags_serde_round_trip_and_release() {
    let mut doc = Document::new();
    doc.shapes
        .push(clip_rect([0.0, 0.0, 20.0, 20.0], Some(3), false));
    doc.shapes
        .push(clip_rect([0.0, 0.0, 10.0, 10.0], Some(3), true));

    let json = serde_json::to_string(&doc).unwrap();
    let back: Document = serde_json::from_str(&json).unwrap();
    assert_eq!(back.shapes[0].clip(), Some(3));
    assert!(back.shapes[1].is_mask());

    // Release: clear the clip tags; both shapes now render plainly.
    let mut released = back;
    for s in released.shapes.iter_mut() {
        s.clear_clip();
    }
    assert!(released.shapes.iter().all(|s| s.clip().is_none()));
    assert_eq!(released.render_shapes().len(), 2);
}

/// A pre-clip `.contour` (no `clip`/`mask` keys) loads unclipped and renders every
/// shape as-is.
#[test]
fn loads_pre_clip_document() {
    let json = r#"{"shapes":[
        {"Rect":{"rect":[0,0,10,10],"fill":[1,0,0,1],"stroke":[0,0,0,1],"stroke_w":2}}
    ]}"#;
    let doc: Document = serde_json::from_str(json).unwrap();
    assert_eq!(doc.shapes[0].clip(), None);
    assert!(!doc.shapes[0].is_mask());
    assert_eq!(doc.render_shapes().len(), 1);
}

// --- Opacity masks -----------------------------------------------------

/// A pre-opacity-mask `.contour` (no `omask`/`omask_path`/`omask_invert` keys)
/// loads unmasked and renders every shape as-is.
#[test]
fn loads_pre_opacity_mask_document() {
    let json = r#"{"shapes":[
        {"Rect":{"rect":[0,0,10,10],"fill":[1,0,0,1],"stroke":[0,0,0,1],"stroke_w":2}}
    ]}"#;
    let doc: Document = serde_json::from_str(json).unwrap();
    assert_eq!(doc.shapes[0].omask(), None);
    assert!(!doc.shapes[0].is_omask());
    assert!(!doc.shapes[0].omask_invert());
    assert!(doc.opacity_mask_of(0).is_none());
}

/// An opacity-masked shape round-trips through serde (id + mask flag + invert),
/// and `opacity_mask_of` resolves the content's mask shape; `render_shapes` drops
/// the mask path (it paints nothing) but keeps the masked content.
#[test]
fn opacity_mask_round_trip_and_resolution() {
    let mut doc = Document::new();
    doc.shapes.clear();
    // Content (index 0) masked by the white mask path (index 1), invert on content.
    let mut content = omask_rect([0.0, 0.0, 20.0, 20.0], [1.0, 0.0, 0.0, 1.0], Some(7), false);
    content.set_omask_invert(true);
    doc.shapes.push(content);
    doc.shapes
        .push(omask_rect([0.0, 0.0, 20.0, 20.0], [1.0, 1.0, 1.0, 1.0], Some(7), true));

    let json = serde_json::to_string(&doc).unwrap();
    let back: Document = serde_json::from_str(&json).unwrap();
    assert_eq!(back.shapes[0].omask(), Some(7));
    assert!(back.shapes[0].omask_invert());
    assert!(back.shapes[1].is_omask());

    // Resolution: the content's mask is the white rect; invert carried through.
    let (mask_shape, invert) = back.opacity_mask_of(0).expect("content has a mask");
    assert!(invert, "invert flag carried");
    assert_eq!(mask_shape.fill_color(), Some([1.0, 1.0, 1.0, 1.0]));
    // The mask path itself is not "masked".
    assert!(back.opacity_mask_of(1).is_none());

    // render_shapes drops the mask path but keeps the (still full-geometry) content.
    let rendered = back.render_shapes();
    let indices: Vec<usize> = rendered.iter().map(|(i, _)| *i).collect();
    assert!(indices.contains(&0), "masked content kept");
    assert!(!indices.contains(&1), "mask path dropped");
}
