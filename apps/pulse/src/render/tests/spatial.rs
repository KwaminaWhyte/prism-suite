use super::*;

// --- Spatial effects ----------------------------------------------------

use crate::comp::{RadialKind, SpatialEffect};

#[test]
fn gaussian_blur_softens_the_layer_edge() {
    // A small centered solid: blurring it adds a band of partial-alpha edge
    // pixels along the center row vs. the crisp render.
    let partial_count = |sigma: f32| {
        let mut c = solid([1.0, 1.0, 1.0, 1.0]);
        if sigma > 0.0 {
            c.layers[0]
                .spatial_effects
                .push(SpatialEffect::GaussianBlur {
                    sigma_x: sigma,
                    sigma_y: sigma,
                    repeat_edge: false,
                });
        }
        let f = render_frame(&c, 0.0);
        (0..f.width)
            .filter(|&x| {
                let a = f.pixel(x, 32)[3];
                a > 0 && a < 255
            })
            .count()
    };
    assert!(
        partial_count(4.0) > partial_count(0.0),
        "blur should add partial-coverage edge pixels"
    );
}

#[test]
fn drop_shadow_appears_in_the_composite() {
    // A solid with a hard (0-softness) black drop shadow offset down-right:
    // a pixel just past the quad in the shadow direction picks up dark,
    // semi-opaque shadow coverage where the crisp layer had nothing.
    let mut c = solid([1.0, 1.0, 1.0, 1.0]);
    c.layers[0].spatial_effects.push(SpatialEffect::DropShadow {
        color: [0.0, 0.0, 0.0],
        opacity: 1.0,
        angle: 45.0, // down-right (+x,+y)
        distance: 10.0,
        softness: 0.0,
        shadow_only: false,
    });
    let crisp = render_frame(&solid([1.0, 1.0, 1.0, 1.0]), 0.0);
    let shad = render_frame(&c, 0.0);
    // The half-extent is ~14px; sample a pixel down-right of the quad's
    // bottom-right corner that the shadow offset reaches.
    let (sx, sy) = (32 + 16, 32 + 16);
    assert_eq!(
        crisp.pixel(sx, sy)[3],
        0,
        "no coverage here without a shadow"
    );
    let p = shad.pixel(sx, sy);
    assert!(p[3] > 0, "drop shadow added coverage past the layer");
    assert!(
        p[0] < 60 && p[1] < 60 && p[2] < 60,
        "shadow should be dark, got {},{},{}",
        p[0],
        p[1],
        p[2]
    );
}

#[test]
fn glow_brightens_a_bright_layer() {
    // A bright (but not pure-white) layer reads brighter at center once a
    // glow blooms its highlights back on top.
    let center_r = |with_glow: bool| {
        let mut c = solid([0.85, 0.85, 0.85, 1.0]);
        if with_glow {
            c.layers[0].spatial_effects.push(SpatialEffect::Glow {
                threshold: 0.4,
                radius: 6.0,
                intensity: 2.0,
            });
        }
        render_frame(&c, 0.0).pixel(32, 32)[0]
    };
    assert!(center_r(true) >= center_r(false), "glow should not darken");
}

#[test]
fn spatial_effect_routes_layer_through_isolated_buffer() {
    // A solid with only a (zero-sigma, identity) blur still renders the same
    // as the crisp solid — the isolated-buffer routing is value-neutral when
    // the pass is identity.
    let mut c = solid([0.3, 0.6, 0.9, 1.0]);
    c.layers[0]
        .spatial_effects
        .push(SpatialEffect::GaussianBlur {
            sigma_x: 0.0,
            sigma_y: 0.0,
            repeat_edge: false,
        });
    let base = render_frame(&solid([0.3, 0.6, 0.9, 1.0]), 0.0);
    let routed = render_frame(&c, 0.0);
    assert_eq!(base.pixels, routed.pixels);
}

#[test]
fn box_blur_softens_the_layer_edge() {
    // A box blur, like the Gaussian, adds partial-alpha edge pixels along the
    // center row vs. the crisp render — and composites into the buffer.
    let partial_count = |radius: f32| {
        let mut c = solid([1.0, 1.0, 1.0, 1.0]);
        if radius > 0.0 {
            c.layers[0].spatial_effects.push(SpatialEffect::BoxBlur {
                radius,
                iterations: 3,
                repeat_edge: false,
            });
        }
        let f = render_frame(&c, 0.0);
        (0..f.width)
            .filter(|&x| {
                let a = f.pixel(x, 32)[3];
                a > 0 && a < 255
            })
            .count()
    };
    assert!(
        partial_count(5.0) > partial_count(0.0),
        "box blur should add partial-coverage edge pixels"
    );
}

#[test]
fn directional_blur_smears_into_the_composite() {
    // A horizontal directional blur on a centered solid extends partial-coverage
    // along the center row past the crisp edge, but leaves a vertical column off
    // the layer crisp (no off-axis smear) — proving the angle drives the streak in
    // the real render path.
    let mut c = solid([1.0, 1.0, 1.0, 1.0]);
    c.layers[0]
        .spatial_effects
        .push(SpatialEffect::DirectionalBlur {
            angle: 0.0,
            length: 12.0,
        });
    let crisp = render_frame(&solid([1.0, 1.0, 1.0, 1.0]), 0.0);
    let smeared = render_frame(&c, 0.0);
    assert_ne!(crisp.pixels, smeared.pixels, "directional blur must change the frame");
    // Partial-coverage pixels appear along the center row (the smear axis).
    let row_partial = |f: &Frame| {
        (0..f.width)
            .filter(|&x| {
                let a = f.pixel(x, 32)[3];
                a > 0 && a < 255
            })
            .count()
    };
    assert!(
        row_partial(&smeared) > row_partial(&crisp),
        "horizontal smear adds partial coverage along the row"
    );
}

#[test]
fn radial_blur_changes_the_frame_and_is_deterministic() {
    // A spin radial blur about the centre warps a wide solid (rotational smear);
    // render-path smoke + determinism.
    let mut c = solid([0.8, 0.4, 0.2, 1.0]);
    c.layers[0].scale.set_key(0.0, 2.0); // a wide quad so the sweep has content
    let crisp = {
        let mut b = solid([0.8, 0.4, 0.2, 1.0]);
        b.layers[0].scale.set_key(0.0, 2.0);
        render_frame(&b, 0.0)
    };
    c.layers[0].spatial_effects.push(SpatialEffect::RadialBlur {
        center: [0.5, 0.5],
        kind: RadialKind::Spin,
        amount: 30.0,
    });
    let warped = render_frame(&c, 0.0);
    assert_ne!(crisp.pixels, warped.pixels, "radial blur must change the frame");
    assert_eq!(warped.pixels, render_frame(&c, 0.0).pixels, "deterministic");
}
