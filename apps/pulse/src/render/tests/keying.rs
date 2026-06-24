use super::*;

// --- Keying effects -----------------------------------------------------

use crate::comp::KeyEffect;

#[test]
fn identity_key_choke_routes_through_isolated_buffer() {
    // A solid with only an identity Matte Choke (no choke, full clip range)
    // renders the same as the crisp solid — the isolated-buffer routing is
    // value-neutral when the key is a no-op (mirrors the distort identity test).
    let mut c = solid([0.3, 0.6, 0.9, 1.0]);
    c.layers[0].key_effects.push(KeyEffect::MatteChoke {
        choke: 0.0,
        clip_black: 0.0,
        clip_white: 1.0,
    });
    let base = render_frame(&solid([0.3, 0.6, 0.9, 1.0]), 0.0);
    let routed = render_frame(&c, 0.0);
    assert_eq!(base.pixels, routed.pixels);
}

#[test]
fn color_key_removes_keyed_solid_from_the_composite() {
    // A green solid with a Color Key on its own colour keys itself out: the
    // center, opaque crisp, drops to (near-)transparent. Determinism too.
    let green = [0.0, 0.6, 0.1, 1.0];
    let crisp = render_frame(&solid(green), 0.0);
    assert_eq!(crisp.pixel(32, 32)[3], 255, "crisp solid is opaque");
    let mut c = solid(green);
    c.layers[0].key_effects.push(KeyEffect::ColorKey {
        key: [0.0, 0.6, 0.1],
        tolerance: 0.2,
        softness: 0.05,
    });
    let keyed = render_frame(&c, 0.0);
    assert_eq!(
        keyed.pixel(32, 32)[3],
        0,
        "the target colour is keyed away from the composite"
    );
    // A non-matching key colour leaves the solid intact.
    let mut c2 = solid(green);
    c2.layers[0].key_effects.push(KeyEffect::ColorKey {
        key: [1.0, 0.0, 0.0], // red — far from green
        tolerance: 0.2,
        softness: 0.05,
    });
    assert_eq!(
        render_frame(&c2, 0.0).pixel(32, 32)[3],
        255,
        "a non-matching key keeps the layer"
    );
    // Deterministic: re-render is byte-identical.
    assert_eq!(keyed.pixels, render_frame(&c, 0.0).pixels);
}

#[test]
fn key_composes_with_mask() {
    // A Color Key that does NOT match the layer (red key on a white solid) still
    // respects the mask: a far corner outside a small centered mask stays carved.
    let mut c = full_frame_solid();
    c.layers[0].masks.push(Mask::rect(8.0, 8.0));
    c.layers[0].key_effects.push(KeyEffect::ColorKey {
        key: [1.0, 0.0, 0.0],
        tolerance: 0.1,
        softness: 0.05,
    });
    let f = render_frame(&c, 0.0);
    assert_eq!(f.pixel(32, 32)[3], 255, "center inside mask survives the key");
    assert_eq!(f.pixel(2, 2)[3], 0, "corner stays carved with a key");
}

#[test]
fn matte_choke_dilate_recovers_masked_alpha() {
    // A small centered mask leaves a hole-y matte; a dilating Matte Choke grows
    // the alpha back outward, so a pixel just outside the mask edge gains
    // coverage it didn't have without the choke.
    let mut base = full_frame_solid();
    base.layers[0].masks.push(Mask::rect(6.0, 6.0));
    let no_choke = render_frame(&base, 0.0);
    let mut c = full_frame_solid();
    c.layers[0].masks.push(Mask::rect(6.0, 6.0));
    c.layers[0].key_effects.push(KeyEffect::MatteChoke {
        choke: 5.0, // dilate
        clip_black: 0.0,
        clip_white: 1.0,
    });
    let choked = render_frame(&c, 0.0);
    // Sum alpha across the frame: dilation only adds coverage.
    let total = |f: &Frame| -> u64 {
        (0..f.height)
            .flat_map(|y| (0..f.width).map(move |x| (x, y)))
            .map(|(x, y)| f.pixel(x, y)[3] as u64)
            .sum()
    };
    assert!(
        total(&choked) > total(&no_choke),
        "dilating choke grows the matte's total coverage"
    );
}

#[test]
fn mask_and_track_matte_compose() {
    // A masked base under a static alpha matte: both must clip. The mask is
    // small (rect 8) and centered; the matte source is unit-scale (~14 px).
    // The center survives both; a far corner is matted out.
    let mut c = matte_pair([1.0, 1.0, 1.0, 1.0], [1.0, 1.0, 1.0, 1.0], 1.0);
    c.layers[0].matte = crate::comp::MatteMode::Alpha;
    c.layers[0].masks.push(Mask::rect(8.0, 8.0));
    let f = render_frame(&c, 0.0);
    assert_eq!(f.pixel(32, 32)[3], 255, "center passes mask and matte");
    // Outside the small mask -> carved by the mask even within the matte.
    assert_eq!(f.pixel(2, 2)[3], 0, "corner clipped by mask+matte");
}
