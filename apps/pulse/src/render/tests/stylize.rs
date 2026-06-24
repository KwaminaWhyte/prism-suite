use super::*;

// --- Stylize effects ----------------------------------------------------

use crate::comp::StylizeEffect;

#[test]
fn full_resolution_mosaic_routes_through_isolated_buffer() {
    // A solid with a per-pixel Mosaic (one block per pixel) renders the same as
    // the crisp solid — the isolated-buffer routing is value-neutral when the
    // stylize is an identity (mirrors the distort / key identity-routing tests).
    let mut c = solid([0.3, 0.6, 0.9, 1.0]);
    c.layers[0].stylize_effects.push(StylizeEffect::Mosaic {
        horizontal: 64,
        vertical: 64,
    });
    let base = render_frame(&solid([0.3, 0.6, 0.9, 1.0]), 0.0);
    let routed = render_frame(&c, 0.0);
    assert_eq!(base.pixels, routed.pixels);
}

#[test]
fn find_edges_whitens_a_flat_solid_interior() {
    // Find Edges on a flat full-frame solid: the interior has no colour gradient,
    // so (inverted, AE default) it reads ~white. Determinism on re-render.
    let mut c = full_frame_solid();
    // A mid-grey solid so the whitening is unambiguous (the input isn't white).
    c.layers[0].color = [0.4, 0.4, 0.4, 1.0];
    c.layers[0].stylize_effects.push(StylizeEffect::FindEdges {
        amount: 1.0,
        invert: false,
    });
    let f = render_frame(&c, 0.0);
    let [r, g, b, a] = f.pixel(32, 32);
    assert_eq!(a, 255, "interior stays opaque");
    assert!(r > 245 && g > 245 && b > 245, "flat interior whitens, got {r},{g},{b}");
}

#[test]
fn mosaic_pools_the_frame_into_constant_blocks() {
    // A two-tone masked layer through a coarse Mosaic pools detail into blocks: a
    // 1×1 mosaic collapses every covered pixel to one constant colour. Render-path
    // smoke + determinism.
    let mut c = full_frame_solid();
    c.layers[0].color = [0.8, 0.2, 0.1, 1.0];
    c.layers[0].stylize_effects.push(StylizeEffect::Mosaic {
        horizontal: 1,
        vertical: 1,
    });
    let f = render_frame(&c, 0.0);
    // Every fully-covered pixel reads the same pooled colour (the whole-frame avg).
    let center = f.pixel(32, 32);
    let other = f.pixel(20, 44);
    assert_eq!(center, other, "1×1 mosaic pools to one constant block");
    // Deterministic re-render.
    assert_eq!(f.pixels, render_frame(&c, 0.0).pixels);
}

#[test]
fn stylize_composes_with_mask() {
    // A stylize (Find Edges) on a masked layer still respects the mask: Find Edges
    // preserves the per-pixel alpha, so a far corner outside the small centered
    // mask stays carved away after the stylize runs (the mask carves before the
    // stylize pass, and Find Edges only reshapes RGB). (Mosaic deliberately pools
    // coverage across its blocks, so it is not used for this carved-corner check.)
    let mut c = full_frame_solid();
    c.layers[0].masks.push(Mask::rect(8.0, 8.0));
    c.layers[0].stylize_effects.push(StylizeEffect::FindEdges {
        amount: 1.0,
        invert: false,
    });
    let f = render_frame(&c, 0.0);
    assert_eq!(f.pixel(2, 2)[3], 0, "corner stays carved with a stylize");
}
