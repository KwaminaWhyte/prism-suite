use super::*;

// --- Masks --------------------------------------------------------------

#[test]
fn mask_clips_layer_to_its_shape() {
    // A small centered rectangular Add mask on a full-frame solid: the center
    // stays opaque, a far corner (outside the mask) is carved away.
    let mut c = full_frame_solid();
    c.layers[0].masks.push(Mask::rect(8.0, 8.0));
    let f = render_frame(&c, 0.0);
    assert_eq!(f.pixel(32, 32)[3], 255, "center inside mask stays covered");
    assert_eq!(f.pixel(2, 2)[3], 0, "corner outside mask is carved away");
}

#[test]
fn inverted_mask_keeps_the_outside() {
    // Inverting the same mask flips it: the center is punched out, the
    // surrounding frame survives.
    let mut c = full_frame_solid();
    let mut m = Mask::rect(8.0, 8.0);
    m.inverted = true;
    c.layers[0].masks.push(m);
    let f = render_frame(&c, 0.0);
    assert_eq!(f.pixel(32, 32)[3], 0, "center punched out by inverted mask");
    assert_eq!(f.pixel(2, 2)[3], 255, "outside survives inversion");
}

#[test]
fn no_active_mask_is_identical_to_unmasked() {
    // A layer whose only mask is disabled (mode None) renders byte-identical
    // to the same layer with no masks at all.
    let base = render_frame(&full_frame_solid(), 0.0);
    let mut c = full_frame_solid();
    let mut m = Mask::rect(8.0, 8.0);
    m.mode = MaskMode::None;
    c.layers[0].masks.push(m);
    let withmask = render_frame(&c, 0.0);
    assert_eq!(base.pixels, withmask.pixels);
}

#[test]
fn mask_preserves_layer_color() {
    // Masking changes coverage, never color: a blue solid masked to a small
    // rect still reads blue at the center.
    let mut c = solid([0.0, 0.0, 1.0, 1.0]);
    c.layers[0].scale.set_key(0.0, 3.0);
    c.layers[0].masks.push(Mask::rect(8.0, 8.0));
    let [r, g, b, a] = render_frame(&c, 0.0).pixel(32, 32);
    assert_eq!(a, 255);
    assert!(b > r && b > g, "center should stay blue, got {r},{g},{b}");
}

#[test]
fn feathered_mask_softens_the_edge() {
    // A hard mask has a crisp 0/255 boundary; a feathered one adds a band of
    // partial-alpha pixels along the center row.
    let partial_count = |feather: f32| {
        let mut c = full_frame_solid();
        let mut m = Mask::rect(12.0, 12.0);
        m.feather = feather;
        c.layers[0].masks.push(m);
        let f = render_frame(&c, 0.0);
        (0..f.width)
            .filter(|&x| {
                let a = f.pixel(x, 32)[3];
                a > 0 && a < 255
            })
            .count()
    };
    assert!(
        partial_count(8.0) > partial_count(0.0),
        "feather should add partial-coverage edge pixels"
    );
}

#[test]
fn add_subtract_mask_stack_punches_a_hole() {
    // A big Add mask with a smaller Subtract mask leaves a covered ring with a
    // transparent hole at the center.
    let mut c = full_frame_solid();
    c.layers[0].masks.push(Mask::rect(13.0, 13.0)); // Add (default)
    let mut sub = Mask::rect(5.0, 5.0);
    sub.mode = MaskMode::Subtract;
    c.layers[0].masks.push(sub);
    let f = render_frame(&c, 0.0);
    assert_eq!(f.pixel(32, 32)[3], 0, "center hole subtracted away");
    // A pixel inside the big rect (local ~9.5px after the layer's 3x scale)
    // but outside the small hole stays covered.
    assert_eq!(f.pixel(60, 32)[3], 255, "ring stays covered");
}

#[test]
fn mask_rides_layer_transform() {
    // The mask is in layer-local space, so moving the layer moves the masked
    // region with it. Shift the layer right and the surviving coverage shifts
    // too: the original center loses coverage, a point to the right gains it.
    let mut c = full_frame_solid();
    c.layers[0].masks.push(Mask::rect(8.0, 8.0));
    c.layers[0].x.set_key(0.0, 16.0); // slide right 16 comp px
    let f = render_frame(&c, 0.0);
    // The masked patch moved to ~x=48; the old center is now outside it.
    assert_eq!(f.pixel(48, 32)[3], 255, "masked patch followed the layer");
    assert_eq!(f.pixel(20, 32)[3], 0, "old position no longer covered");
}
