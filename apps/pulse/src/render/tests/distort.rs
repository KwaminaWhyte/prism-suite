use super::*;

// --- Distort effects ----------------------------------------------------

use crate::comp::{DistortEffect, PolarKind};

#[test]
fn identity_distort_routes_through_isolated_buffer() {
    // A solid with only an identity corner-pin renders the same as the crisp
    // solid — the isolated-buffer routing is value-neutral when the remap is
    // identity (mirrors the spatial-effect identity-routing test).
    let mut c = solid([0.3, 0.6, 0.9, 1.0]);
    c.layers[0].distort_effects.push(DistortEffect::CornerPin {
        top_left: [0.0, 0.0],
        top_right: [1.0, 0.0],
        bottom_right: [1.0, 1.0],
        bottom_left: [0.0, 1.0],
    });
    let base = render_frame(&solid([0.3, 0.6, 0.9, 1.0]), 0.0);
    let routed = render_frame(&c, 0.0);
    assert_eq!(base.pixels, routed.pixels);
}

#[test]
fn distort_transform_moves_coverage_in_the_composite() {
    // A centered solid with an effect-level Transform that shifts the content far
    // right: the original center loses coverage, a band to the right gains it —
    // proving the distort pass composites into the buffer.
    let mut c = solid([1.0, 1.0, 1.0, 1.0]);
    c.layers[0].scale.set_key(0.0, 2.0); // a wider quad so the shift stays in-frame
    c.layers[0].distort_effects.push(DistortEffect::Transform {
        anchor: [0.5, 0.5],
        position: [0.78, 0.5], // shift the content right within the buffer
        scale: 1.0,
        rotation: 0.0,
        skew: 0.0,
        opacity: 1.0,
    });
    let crisp = {
        let mut b = solid([1.0, 1.0, 1.0, 1.0]);
        b.layers[0].scale.set_key(0.0, 2.0);
        render_frame(&b, 0.0)
    };
    let warped = render_frame(&c, 0.0);
    assert_ne!(crisp.pixels, warped.pixels, "transform must change the frame");
    // The far-right region picks up coverage it didn't have crisp.
    let right_cov = |f: &Frame| (0..f.height).filter(|&y| f.pixel(58, y)[3] > 0).count();
    assert!(
        right_cov(&warped) >= right_cov(&crisp),
        "transform shifted coverage right"
    );
}

#[test]
fn distort_transform_opacity_fades_the_layer() {
    // An effect-level Transform at half opacity dims the layer's center alpha.
    let mut c = solid([1.0, 1.0, 1.0, 1.0]);
    c.layers[0].distort_effects.push(DistortEffect::Transform {
        anchor: [0.5, 0.5],
        position: [0.5, 0.5],
        scale: 1.0,
        rotation: 0.0,
        skew: 0.0,
        opacity: 0.5,
    });
    let a = render_frame(&c, 0.0).pixel(32, 32)[3];
    assert!((100..=160).contains(&a), "effect opacity halved alpha, got {a}");
}

#[test]
fn distort_polar_changes_the_frame() {
    // A full-frame gradient-ish solid through a Rect→Polar remap is no longer the
    // crisp solid (the remap warps coverage); render-path smoke + determinism.
    let mut c = solid([0.8, 0.4, 0.2, 1.0]);
    c.layers[0].scale.set_key(0.0, 3.0); // cover the frame so the remap has content
    let crisp = {
        let mut b = solid([0.8, 0.4, 0.2, 1.0]);
        b.layers[0].scale.set_key(0.0, 3.0);
        render_frame(&b, 0.0)
    };
    c.layers[0].distort_effects.push(DistortEffect::Polar {
        center: [0.5, 0.5],
        kind: PolarKind::RectToPolar,
        interp: 1.0,
    });
    let warped = render_frame(&c, 0.0);
    assert_ne!(crisp.pixels, warped.pixels, "polar remap must change the frame");
    // Deterministic: re-render is byte-identical.
    assert_eq!(warped.pixels, render_frame(&c, 0.0).pixels);
}

#[test]
fn distort_composes_with_mask() {
    // A distort (mirror) on a masked layer still respects the mask: a far corner
    // outside the small centered mask stays carved away after the distort runs.
    let mut c = full_frame_solid();
    c.layers[0].masks.push(Mask::rect(8.0, 8.0));
    c.layers[0].distort_effects.push(DistortEffect::Mirror {
        center: [0.5, 0.5],
        angle: 90.0,
    });
    let f = render_frame(&c, 0.0);
    // The mask + mirror both clip; a far corner is empty.
    assert_eq!(f.pixel(2, 2)[3], 0, "corner stays carved with a distort");
}
