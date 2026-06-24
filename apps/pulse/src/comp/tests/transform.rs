use super::*;

// --- Affine2 transform math --------------------------------------------


#[test]
fn affine_identity_is_a_noop() {
    assert!(approx(Affine2::IDENTITY.apply(3.0, -7.0), (3.0, -7.0)));
}

#[test]
fn affine_translate_scale_rotate() {
    assert!(approx(
        Affine2::translate(5.0, 2.0).apply(1.0, 1.0),
        (6.0, 3.0)
    ));
    assert!(approx(Affine2::scale(3.0).apply(2.0, -4.0), (6.0, -12.0)));
    // 90° about origin, +y down (clockwise on screen): (1,0) -> (0,1).
    assert!(approx(
        Affine2::rotate_deg(90.0).apply(1.0, 0.0),
        (0.0, 1.0)
    ));
    // 180°: (1,2) -> (-1,-2).
    assert!(approx(
        Affine2::rotate_deg(180.0).apply(1.0, 2.0),
        (-1.0, -2.0)
    ));
}

#[test]
fn affine_then_applies_rhs_first() {
    // then(rhs) = self ∘ rhs: scale by 2, THEN translate by (10,0).
    let m = Affine2::translate(10.0, 0.0).then(Affine2::scale(2.0));
    assert!(approx(m.apply(3.0, 1.0), (16.0, 2.0)));
    // Reversed order differs (translate first, then scale).
    let n = Affine2::scale(2.0).then(Affine2::translate(10.0, 0.0));
    assert!(approx(n.apply(3.0, 1.0), (26.0, 2.0)));
}

#[test]
fn affine_inverse_round_trips() {
    let m = Affine2::translate(7.0, -3.0)
        .then(Affine2::rotate_deg(37.0))
        .then(Affine2::scale(2.5));
    let inv = m.inverse().unwrap();
    let p = (4.0, -9.0);
    let mapped = m.apply(p.0, p.1);
    let back = inv.apply(mapped.0, mapped.1);
    assert!(approx(back, p), "inverse did not round-trip: {back:?}");
}

#[test]
fn affine_inverse_none_when_singular() {
    // Zero scale collapses the plane -> not invertible.
    assert!(Affine2::scale(0.0).inverse().is_none());
}

// --- Anchor point -------------------------------------------------------

#[test]
fn default_transform_pivots_about_center() {
    // No anchor, no position: the local matrix is just rotate·scale about
    // the layer center, so the center (0,0) stays put.
    let tf = Transform {
        anchor_x: 0.0,
        anchor_y: 0.0,
        x: 0.0,
        y: 0.0,
        scale: 2.0,
        rotation_deg: 90.0,
        opacity: 1.0,
    };
    let m = tf.local_matrix();
    assert!(approx(m.apply(0.0, 0.0), (0.0, 0.0)));
    // A point right of center: scaled x2 then rotated 90° (+y down).
    assert!(approx(m.apply(1.0, 0.0), (0.0, 2.0)));
}

#[test]
fn anchor_point_is_the_pivot_and_lands_on_position() {
    // Anchor offset (10,0); position (100, 50): the anchored local point
    // (10,0) must map exactly to comp-space position (100,50), and scale
    // pivots about the anchor, not the center.
    let tf = Transform {
        anchor_x: 10.0,
        anchor_y: 0.0,
        x: 100.0,
        y: 50.0,
        scale: 3.0,
        rotation_deg: 0.0,
        opacity: 1.0,
    };
    let m = tf.local_matrix();
    // The anchor maps to the position.
    assert!(approx(m.apply(10.0, 0.0), (100.0, 50.0)));
    // The center (0,0) sits anchor-distance*scale to the left of position:
    // local (0,0) is 10 left of the anchor -> 30 left after scale x3.
    assert!(approx(m.apply(0.0, 0.0), (70.0, 50.0)));
}

/// Regression: a text layer's **transform stays put across a font-family change**.
/// Position / anchor / scale / rotation live on the layer's animatable tracks,
/// wholly separate from the glyph buffer, so switching the rendered font
/// (stroke `None` ↔ outline `Some(..)`) — like changing size / align — must not
/// touch the transform or move where the layer's anchor lands on screen. (Guards
/// against the class of bug where editing a type property resets the layer to the
/// top-left / makes the text jump.)
#[test]
fn font_family_change_preserves_text_layer_transform_and_anchor() {
    let mut layer = PulseLayer::of_kind(LayerKind::Text, "T", [1.0; 4]);
    layer.text = TextLayer {
        text: "HELLO".to_string(),
        size: 120.0,
        align: TextAlign::Center,
        font_family: None, // built-in stroke font
        ..TextLayer::default()
    };
    // Place / move the layer somewhere non-trivial (a user dragging it around):
    // an offset anchor, a position away from center, plus scale + rotation.
    layer.anchor_x.set_key(0.0, 7.0);
    layer.anchor_y.set_key(0.0, -3.0);
    layer.x.set_key(0.0, 140.0);
    layer.y.set_key(0.0, -90.0);
    layer.scale.set_key(0.0, 1.5);
    layer.rotation.set_key(0.0, 30.0);

    let before = layer.transform(0.0);
    // Where the anchor point lands in comp space, before the font change.
    let anchor_before = before.local_matrix().apply(before.anchor_x, before.anchor_y);

    // Switch to a real outline family (and bump other type settings while we're
    // here) — exactly what the Properties Font dropdown does.
    layer.text.font_family = Some("Ubuntu".to_string());
    layer.text.size = 200.0;
    layer.text.align = TextAlign::Right;

    let after = layer.transform(0.0);
    // Every transform component is untouched by the type edit.
    assert_eq!(before.anchor_x, after.anchor_x);
    assert_eq!(before.anchor_y, after.anchor_y);
    assert_eq!(before.x, after.x);
    assert_eq!(before.y, after.y);
    assert_eq!(before.scale, after.scale);
    assert_eq!(before.rotation_deg, after.rotation_deg);
    // …and the anchor still lands on the exact same comp-space point: the layer
    // (and so the text) does not jump on a font change.
    let anchor_after = after.local_matrix().apply(after.anchor_x, after.anchor_y);
    assert!(approx(anchor_before, anchor_after), "anchor moved on font change");

    // The laid-out block stays centered about the layer-local origin in *both*
    // font paths, so the text rides the same anchor regardless of font: the
    // stroke path lays out segments centered on (0,0), the outline path lays out
    // contours centered on (0,0). (A path that anchored text to its own bounds
    // would shift the centroid when metrics changed and the text would appear to
    // move.)
    let mut stroke = layer.clone();
    stroke.text.font_family = None;
    let span = |xs: &[f32]| {
        let lo = xs.iter().copied().fold(f32::INFINITY, f32::min);
        let hi = xs.iter().copied().fold(f32::NEG_INFINITY, f32::max);
        (lo + hi) * 0.5 // midpoint of the extent
    };
    let stroke_xs: Vec<f32> = stroke
        .text
        .segments()
        .iter()
        .flat_map(|&(a, b)| [a.0, b.0])
        .collect();
    let stroke_ys: Vec<f32> = stroke
        .text
        .segments()
        .iter()
        .flat_map(|&(a, b)| [a.1, b.1])
        .collect();
    let outline_xs: Vec<f32> = layer
        .text
        .outline_contours()
        .iter()
        .flat_map(|c| c.iter().map(|p| p.0))
        .collect();
    let outline_ys: Vec<f32> = layer
        .text
        .outline_contours()
        .iter()
        .flat_map(|c| c.iter().map(|p| p.1))
        .collect();
    // Both font paths center the block on the origin (within a glyph-metric
    // tolerance): the visual center coincides with the layer center either way,
    // so the on-screen placement is stable across the font switch.
    assert!(
        span(&stroke_xs).abs() < 1.0,
        "stroke block centered on x=0, got {}",
        span(&stroke_xs)
    );
    assert!(
        span(&outline_xs).abs() < layer.text.size,
        "outline block centered near x=0, got {}",
        span(&outline_xs)
    );
    assert!(
        span(&stroke_ys).abs() < 1.0,
        "stroke block centered on y=0, got {}",
        span(&stroke_ys)
    );
    assert!(
        span(&outline_ys).abs() < layer.text.size,
        "outline block centered near y=0, got {}",
        span(&outline_ys)
    );
}

// --- Parenting / world matrix ------------------------------------------


#[test]
fn unparented_world_matrix_equals_local() {
    let mut c = parented_comp();
    c.layers[0].x.set_key(0.0, 25.0);
    c.layers[0].rotation.set_key(0.0, 45.0);
    let world = c.world_matrix(0, 0.0);
    let local = c.layers[0].transform(0.0).local_matrix();
    assert_eq!(world, local);
}

#[test]
fn child_inherits_parent_translation() {
    let mut c = parented_comp();
    c.layers[0].x.set_key(0.0, 40.0); // parent shifted right 40
    c.layers[1].x.set_key(0.0, 10.0); // child shifted right 10 in parent space
    c.layers[1].parent = Some(0);
    // Child's local center (0,0) -> parent applies its +40 offset on top of
    // the child's own +10 = +50 in comp space.
    let world = c.world_matrix(1, 0.0);
    assert!(approx(world.apply(0.0, 0.0), (50.0, 0.0)));
}

#[test]
fn child_inherits_parent_rotation_and_scale() {
    let mut c = parented_comp();
    c.layers[0].scale.set_key(0.0, 2.0); // parent scales x2
    c.layers[0].rotation.set_key(0.0, 90.0); // and rotates 90°
    c.layers[1].x.set_key(0.0, 5.0); // child offset +5 in parent space
    c.layers[1].parent = Some(0);
    // Child center: +5 in parent space, then parent scales x2 (->10) and
    // rotates 90° (+y down): (10,0) -> (0,10).
    let world = c.world_matrix(1, 0.0);
    assert!(approx(world.apply(0.0, 0.0), (0.0, 10.0)));
}

#[test]
fn world_matrix_breaks_self_cycle() {
    let mut c = parented_comp();
    c.layers[0].parent = Some(0); // self-parent (corrupt)
    c.layers[0].x.set_key(0.0, 7.0);
    // Must terminate and apply the layer's transform exactly once.
    let world = c.world_matrix(0, 0.0);
    assert!(approx(world.apply(0.0, 0.0), (7.0, 0.0)));
}

#[test]
fn world_matrix_breaks_mutual_cycle() {
    let mut c = parented_comp();
    c.layers[0].parent = Some(1);
    c.layers[1].parent = Some(0); // 0<->1 cycle
                                  // Bounded walk; just assert it returns (no hang/overflow).
    let _ = c.world_matrix(0, 0.0);
    let _ = c.world_matrix(1, 0.0);
}

#[test]
fn can_parent_rejects_self_and_cycles() {
    let mut c = parented_comp();
    c.layers.push(PulseLayer::new("grandchild", [1.0; 4])); // 2
    c.layers[1].parent = Some(0); // child(1) -> parent(0)
    c.layers[2].parent = Some(1); // grandchild(2) -> child(1)
                                  // Self-parent is illegal.
    assert!(!c.can_parent(0, 0));
    // Out-of-range parent is illegal.
    assert!(!c.can_parent(0, 9));
    // Parenting the root (0) to its own descendants (1 or 2) would cycle.
    assert!(!c.can_parent(0, 1));
    assert!(!c.can_parent(0, 2));
    // Re-pointing the tail (2) at the root (0) is acyclic and allowed.
    assert!(c.can_parent(2, 0));
}

#[test]
fn parent_assign_then_clear_returns_to_local() {
    // Assigning a parent makes the child ride the parent's offset; clearing it
    // (back to `None`) returns the child to its own local transform exactly.
    let mut c = parented_comp();
    c.layers[0].x.set_key(0.0, 30.0); // parent shifted right 30
    c.layers[1].x.set_key(0.0, 5.0); // child shifted right 5

    // Assign: child inherits +30 on top of its own +5 = +35.
    assert!(c.can_parent(1, 0));
    c.layers[1].parent = Some(0);
    assert!(approx(c.world_matrix(1, 0.0).apply(0.0, 0.0), (35.0, 0.0)));

    // Clear: world matrix collapses back to the child's local transform (+5).
    c.layers[1].parent = None;
    assert_eq!(
        c.world_matrix(1, 0.0),
        c.layers[1].transform(0.0).local_matrix()
    );
    assert!(approx(c.world_matrix(1, 0.0).apply(0.0, 0.0), (5.0, 0.0)));
}
