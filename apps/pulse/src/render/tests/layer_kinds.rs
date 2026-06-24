use super::*;

// --- Layer kinds + effects ---------------------------------------------

#[test]
fn null_layer_renders_nothing() {
    let mut c = solid([1.0, 1.0, 1.0, 1.0]);
    c.layers[0].kind = crate::comp::LayerKind::Null;
    let f = render_frame(&c, 0.0);
    assert!(f.pixels.iter().all(|&b| b == 0));
}

#[test]
fn solid_effect_stack_recolors_the_quad() {
    // A black solid with a Tint mapping black->white should now read white at
    // the center (the effect runs on the layer's own color before compositing).
    let mut c = solid([0.0, 0.0, 0.0, 1.0]);
    c.layers[0].effects.push(crate::comp::Effect::Tint {
        black: [1.0, 1.0, 1.0],
        white: [1.0, 1.0, 1.0],
        amount: 1.0,
    });
    let f = render_frame(&c, 0.0);
    let [r, g, b, a] = f.pixel(32, 32);
    assert_eq!(a, 255);
    assert!(
        r > 250 && g > 250 && b > 250,
        "expected white, got {r},{g},{b}"
    );
}

#[test]
fn channel_mixer_swaps_channels_in_render_path() {
    // A pure-blue solid with a Channel Mixer that sources red from blue (R<-B)
    // should read with a high red channel at the center after compositing.
    let mut c = solid([0.0, 0.0, 1.0, 1.0]);
    c.layers[0].effects.push(crate::comp::Effect::ChannelMixer {
        red: [0.0, 0.0, 1.0, 0.0], // R <- B
        green: [0.0, 1.0, 0.0, 0.0],
        blue: [0.0, 0.0, 1.0, 0.0],
        monochrome: false,
    });
    let f = render_frame(&c, 0.0);
    let [r, g, b, a] = f.pixel(32, 32);
    assert_eq!(a, 255);
    assert!(r > 250, "expected red lifted from blue, got r={r}");
    assert!(g < 5, "green should stay zero, got {g}");
    assert!(b > 250, "blue should stay high, got {b}");
}

#[test]
fn gradient_map_recolors_solid_in_render_path() {
    // A black solid through a Gradient Map whose shadow stop is pure red should
    // composite as red at the center (luma 0 -> first stop).
    let mut c = solid([0.0, 0.0, 0.0, 1.0]);
    c.layers[0].effects.push(crate::comp::Effect::GradientMap {
        low: [1.0, 0.0, 0.0],
        mid: [0.0, 1.0, 0.0],
        high: [0.0, 0.0, 1.0],
        amount: 1.0,
    });
    let f = render_frame(&c, 0.0);
    let [r, g, b, a] = f.pixel(32, 32);
    assert_eq!(a, 255);
    assert!(
        r > 250 && g < 5 && b < 5,
        "expected red shadow stop, got {r},{g},{b}"
    );
}

#[test]
fn effect_mask_limits_the_grade_to_its_region() {
    // A black solid with a "make it white" effect, masked to only the RIGHT side
    // of the layer (local x in ~[2, 14]). The pixel at the layer center (local
    // 0,0) is outside the region → stays black (the unmasked grade is suppressed);
    // a pixel well to the right is inside → reads white (full grade). Without a
    // mask the whole quad would be white, so this proves the mask gates the effect.
    let half = 64.0 * LAYER_HALF_FRAC; // ~14 px
    let mut c = solid([0.0, 0.0, 0.0, 1.0]);
    c.layers[0]
        .effects
        .push(crate::comp::Effect::BrightnessContrast {
            brightness: 1.0,
            contrast: 1.0,
        });
    c.layers[0].effect_mask.enabled = true;
    // A rect region covering local x in [2, half], full height — shift a centered
    // rect's left edge rightward so the center is excluded.
    let mut region = crate::comp::Mask::rect(half, half);
    for v in &mut region.vertices {
        if v.x < 0.0 {
            v.x = 2.0; // pull the left edge to x=2
        }
    }
    c.layers[0].effect_mask.region = region;

    let f = render_frame(&c, 0.0);
    // Center pixel (local ~0,0) is outside the masked region → original black.
    let [cr, cg, cb, ca] = f.pixel(32, 32);
    assert_eq!(ca, 255);
    assert!(
        cr < 5 && cg < 5 && cb < 5,
        "center should be unmasked (black), got {cr},{cg},{cb}"
    );
    // A pixel ~8 px right of center (comp x=40, local ~+8) is inside → white.
    let [rr, rg, rb, ra] = f.pixel(40, 32);
    assert_eq!(ra, 255);
    assert!(
        rr > 250 && rg > 250 && rb > 250,
        "masked region should be graded white, got {rr},{rg},{rb}"
    );
}

#[test]
fn adjustment_layer_regrades_layers_below() {
    // A mid-gray solid beneath a full-frame adjustment that lifts brightness
    // should read brighter at the center than without the adjustment.
    let make = |with_adj: bool| {
        let mut c = solid([0.5, 0.5, 0.5, 1.0]);
        if with_adj {
            let mut adj = PulseLayer::of_kind(crate::comp::LayerKind::Adjustment, "adj", [1.0; 4]);
            adj.scale.set_key(0.0, 3.0); // cover the frame
            adj.effects.push(crate::comp::Effect::BrightnessContrast {
                brightness: 0.3,
                contrast: 1.0,
            });
            c.layers.push(adj);
        }
        render_frame(&c, 0.0).pixel(32, 32)[0]
    };
    assert!(
        make(true) > make(false),
        "adjustment did not brighten below"
    );
}

#[test]
fn adjustment_layer_draws_no_pixels_of_its_own() {
    // An adjustment over an empty comp leaves it transparent (no source).
    let mut c = Comp {
        width: 16,
        height: 16,
        duration: 1.0,
        fps: 30.0,
        motion_blur: MotionBlur::default(),
        markers: Vec::new(),
        work_area: WorkArea::default(),
        camera: Camera::default(),
        lights: Vec::new(),
        hide_shy: false,
        layers: Vec::new(),
        id: 0,
        name: String::new(),
    };
    let mut adj = PulseLayer::of_kind(crate::comp::LayerKind::Adjustment, "adj", [1.0; 4]);
    adj.scale.set_key(0.0, 3.0);
    adj.effects.push(crate::comp::Effect::BrightnessContrast {
        brightness: 0.5,
        contrast: 1.0,
    });
    c.layers.push(adj);
    let f = render_frame(&c, 0.0);
    assert!(f.pixels.iter().all(|&b| b == 0));
}

#[test]
fn adjustment_only_affects_its_quad_bounds() {
    // A small (unscaled) adjustment over a full-frame solid grades only the
    // pixels inside its quad: the center changes, a far corner does not.
    let mut c = solid([0.5, 0.5, 0.5, 1.0]);
    c.layers[0].scale.set_key(0.0, 3.0); // bottom solid covers the frame
    let mut adj = PulseLayer::of_kind(crate::comp::LayerKind::Adjustment, "adj", [1.0; 4]);
    adj.effects.push(crate::comp::Effect::BrightnessContrast {
        brightness: 0.3,
        contrast: 1.0,
    });
    c.layers.push(adj); // unit-scale: covers only ~the center quad
    let f = render_frame(&c, 0.0);
    let center = f.pixel(32, 32)[0];
    let corner = f.pixel(1, 1)[0];
    // Center (inside the small adjustment quad) is brighter than an edge
    // pixel (covered by the solid but outside the adjustment).
    assert!(
        center > corner,
        "center {center} should exceed corner {corner}"
    );
}
