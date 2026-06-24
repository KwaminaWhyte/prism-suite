use super::*;

// --- Blend modes --------------------------------------------------------------

/// A 64x64 comp: a full-frame opaque `base` solid (index 0) with a centered
/// `top` solid (index 1) carrying blend mode `mode`. The two overlap at center.
fn blend_pair(base: [f32; 4], top: [f32; 4], mode: BlendMode) -> Comp {
    let mut c = solid(base);
    c.layers[0].scale.set_key(0.0, 3.0); // base covers the frame
    let mut t = PulseLayer::new("top", top);
    t.blend = LayerBlend(mode);
    c.layers.push(t); // index 1
    c
}

#[test]
fn normal_blend_renders_identically_to_no_blend() {
    // A Normal-blend layer must be byte-identical to one with the default blend
    // (the renderer's source-over fast path), so old projects don't shift.
    let base = render_frame(
        &blend_pair(
            [0.2, 0.3, 0.4, 1.0],
            [0.8, 0.5, 0.2, 1.0],
            BlendMode::Normal,
        ),
        0.0,
    );
    let mut plain = blend_pair(
        [0.2, 0.3, 0.4, 1.0],
        [0.8, 0.5, 0.2, 1.0],
        BlendMode::Normal,
    );
    // Explicitly clear to the struct default to prove equivalence.
    plain.layers[1].blend = LayerBlend::default();
    let other = render_frame(&plain, 0.0);
    assert_eq!(base.pixels, other.pixels);
}

#[test]
fn multiply_blend_darkens_the_overlap() {
    // A mid-gray top multiplied over a brighter base reads darker at the overlap
    // than the same top composited Normal (which would just show the top color).
    let center = |mode: BlendMode| {
        let c = blend_pair([0.8, 0.8, 0.8, 1.0], [0.5, 0.5, 0.5, 1.0], mode);
        render_frame(&c, 0.0).pixel(32, 32)[0]
    };
    let mult = center(BlendMode::Multiply);
    let normal = center(BlendMode::Normal);
    assert!(
        mult < normal,
        "multiply should darken vs normal: mult={mult} normal={normal}"
    );
}

#[test]
fn screen_blend_lightens_the_overlap() {
    // Screen over a darker base lifts the overlap above the plain top color.
    let center = |mode: BlendMode| {
        let c = blend_pair([0.3, 0.3, 0.3, 1.0], [0.4, 0.4, 0.4, 1.0], mode);
        render_frame(&c, 0.0).pixel(32, 32)[0]
    };
    let screen = center(BlendMode::Screen);
    let normal = center(BlendMode::Normal);
    assert!(
        screen > normal,
        "screen should lighten vs normal: screen={screen} normal={normal}"
    );
}

#[test]
fn blend_only_changes_pixels_with_a_backdrop() {
    // Where the top layer overhangs past the (full-frame) base it still has a
    // backdrop, so test a region with no base instead: a small base + a larger
    // multiply top — outside the base the top shows its own color unchanged.
    let mut c = solid([1.0, 1.0, 1.0, 1.0]); // small base (unit scale ~14px)
    let mut top = PulseLayer::new("top", [0.5, 0.5, 0.5, 1.0]);
    top.scale.set_key(0.0, 3.0); // top covers the frame
    top.blend = LayerBlend(BlendMode::Multiply);
    c.layers.push(top);
    let f = render_frame(&c, 0.0);
    // A far corner: only the top layer is present (no base backdrop), so the
    // multiply blend is a no-op there and the top shows its straight gray.
    let corner = f.pixel(2, 2);
    let mid_gray = enc(srgb_like(0.5));
    assert!(corner[3] == 255);
    assert!(
        (corner[0] as i32 - mid_gray as i32).abs() <= 2,
        "top shows unblended over empty backdrop: got {} want ~{mid_gray}",
        corner[0]
    );
}

/// Encode a straight sRGB component the way the renderer does (sRGB->linear at
/// input, linear->sRGB at output is identity), for asserting expected bytes.
fn srgb_like(v: f32) -> f32 {
    prism_core::color::srgb_to_linear(v)
}

#[test]
fn non_normal_blend_routes_solid_through_isolated_buffer() {
    // A solid with a blend mode but no masks/matte/spatial still renders (it now
    // takes the isolated-buffer path). Sanity: it composites without panic and
    // covers its center.
    let c = blend_pair(
        [0.0, 0.0, 0.0, 1.0],
        [1.0, 1.0, 1.0, 1.0],
        BlendMode::Screen,
    );
    let f = render_frame(&c, 0.0);
    assert_eq!(f.pixel(32, 32)[3], 255, "blended top still covers center");
}

#[test]
fn pre_blend_project_loads_with_normal_blend() {
    // A serialized layer missing the `blend` field (old project) deserializes
    // with Normal (serde default), so it renders as plain source-over.
    let json = r#"{
        "name":"L","color":[1.0,0.0,0.0,1.0],"visible":true,
        "x":{"keys":[]},"y":{"keys":[]},"scale":{"keys":[]},
        "rotation":{"keys":[]},"opacity":{"keys":[]}
    }"#;
    let layer: PulseLayer = serde_json::from_str(json).unwrap();
    assert_eq!(layer.blend_mode(), BlendMode::Normal);
}
