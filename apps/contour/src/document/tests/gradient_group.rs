use super::*;

// --- Gradient fills ----------------------------------------------------

/// `.contour` files written before gradient fills existed must load with no
/// gradient (a solid fill), and a gradient must round-trip through serde.
#[test]
fn fill_gradient_is_additive_and_round_trips() {
    use crate::gradient::{Gradient, GradientKind};
    // A pre-gradient Rect (no `fill_gradient` key) loads with `None`.
    let json = r#"{"shapes":[
        {"Rect":{"rect":[0,0,10,10],"fill":[1,0,0,1],"stroke":[0,0,0,1],"stroke_w":1}}
    ]}"#;
    let doc: Document = serde_json::from_str(json).unwrap();
    assert!(doc.shapes[0].fill_gradient().is_none());
    assert_eq!(doc.shapes[0].fill_color(), Some([1.0, 0.0, 0.0, 1.0]));

    // Setting a gradient and serializing round-trips it back.
    let mut doc = doc;
    let g = Gradient::two_stop(
        GradientKind::Radial,
        [1.0, 1.0, 1.0, 1.0],
        [0.0, 0.0, 0.0, 1.0],
    );
    doc.shapes[0].set_fill_gradient(Some(g.clone()));
    let s = serde_json::to_string(&doc).unwrap();
    let back: Document = serde_json::from_str(&s).unwrap();
    assert_eq!(back.shapes[0].fill_gradient(), Some(&g));
}

/// A `Line` has no fill region, so setting a gradient on it is a no-op.
#[test]
fn line_ignores_gradient_fill() {
    use crate::gradient::{Gradient, GradientKind};
    let mut line = Shape::Line {
        p0: (0.0, 0.0),
        p1: (10.0, 0.0),
        stroke: [0.0; 4],
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
    line.set_fill_gradient(Some(Gradient::two_stop(
        GradientKind::Linear,
        [0.0; 4],
        [1.0; 4],
    )));
    assert!(line.fill_gradient().is_none());
    assert!(line.fill_color().is_none());
}

// --- Group membership --------------------------------------------------

/// The additive `group` tag round-trips through serde and is `None` on a
/// document written before grouping existed.
#[test]
fn group_tag_is_additive_and_round_trips() {
    // A pre-group Rect (no `group` key) loads ungrouped.
    let json = r#"{"shapes":[
        {"Rect":{"rect":[0,0,10,10],"fill":[1,0,0,1],"stroke":[0,0,0,1],"stroke_w":1}}
    ]}"#;
    let mut doc: Document = serde_json::from_str(json).unwrap();
    assert_eq!(doc.shapes[0].group(), None);

    // Tagging it with a group and serializing round-trips the id back.
    doc.shapes[0].set_group(Some(7));
    let s = serde_json::to_string(&doc).unwrap();
    let back: Document = serde_json::from_str(&s).unwrap();
    assert_eq!(back.shapes[0].group(), Some(7));
}

/// `set_group` / `group` work uniformly across every variant, including `Line`
/// (which has no fill but can still belong to a group).
#[test]
fn group_accessor_covers_every_variant() {
    let mut line = Shape::Line {
        p0: (0.0, 0.0),
        p1: (10.0, 0.0),
        stroke: [0.0; 4],
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
    line.set_group(Some(3));
    assert_eq!(line.group(), Some(3));
    line.set_group(None);
    assert_eq!(line.group(), None);
}

/// Converting a grouped shape to a path preserves its group membership, so a
/// rotation (which rasterises a `Rect`/`Ellipse` into a `Path`) keeps it in its
/// group.
#[test]
fn to_path_preserves_group() {
    let r = Shape::Rect {
        rect: [0.0, 0.0, 10.0, 10.0],
        fill: [0.0; 4],
        fill_gradient: None,
        stroke: [0.0; 4],
        stroke_w: 1.0,
        stroke_style: StrokeStyle::default(),
        appearance: None,
        visible: true,
        group: Some(42),
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
    assert_eq!(r.to_path().group(), Some(42));

    // And a rotation (Rect -> Path under the hood) keeps the group too.
    let mut r2 = r;
    r2.apply_affine(&Affine::rotate_about(0.5, 5.0, 5.0));
    assert!(matches!(r2, Shape::Path { .. }));
    assert_eq!(r2.group(), Some(42));
}

/// A `Rect` with a gradient fill carries that gradient through `with_outline`,
/// and the produced shape is a clipped, plain (un-clip-tagged) closed path.
#[test]
fn with_outline_inherits_paint_and_clears_clip() {
    let mut s = Shape::Rect {
        rect: [0.0, 0.0, 10.0, 10.0],
        fill: [0.2, 0.4, 0.6, 1.0],
        fill_gradient: None,
        stroke: [0.1, 0.1, 0.1, 1.0],
        stroke_w: 3.0,
        stroke_style: StrokeStyle::default(),
        appearance: None,
        visible: true,
        group: Some(5),
        clip: Some(9),
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
    s.set_mask(true);
    let ring = vec![(0.0, 0.0), (4.0, 0.0), (4.0, 4.0), (0.0, 4.0)];
    let out = s.with_outline(ring.clone());
    match out {
        Shape::Path {
            points,
            closed,
            fill,
            stroke_w,
            group,
            clip,
            mask,
            ..
        } => {
            assert_eq!(points, ring);
            assert!(closed);
            assert_eq!(fill, [0.2, 0.4, 0.6, 1.0]);
            assert_eq!(stroke_w, 3.0);
            assert_eq!(group, Some(5)); // group survives
            assert_eq!(clip, None); // clip tag cleared (already clipped)
            assert!(!mask);
        }
        _ => panic!("with_outline must produce a Path"),
    }
}
