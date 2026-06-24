use super::*;

// --- Effects ------------------------------------------------------------

fn approx_rgb(a: [f32; 4], b: [f32; 3]) -> bool {
    (a[0] - b[0]).abs() < 1e-4 && (a[1] - b[1]).abs() < 1e-4 && (a[2] - b[2]).abs() < 1e-4
}

#[test]
fn effect_preserves_alpha() {
    let px = [0.5, 0.5, 0.5, 0.37];
    for e in Effect::defaults() {
        assert_eq!(e.apply(px)[3], 0.37, "{} changed alpha", e.label());
    }
}

#[test]
fn brightness_contrast_identity_is_neutral() {
    let e = Effect::BrightnessContrast {
        brightness: 0.0,
        contrast: 1.0,
    };
    assert!(approx_rgb(e.apply([0.2, 0.5, 0.8, 1.0]), [0.2, 0.5, 0.8]));
}

#[test]
fn brightness_lifts_and_contrast_pivots_about_half() {
    // +0.1 brightness lifts everything.
    let b = Effect::BrightnessContrast {
        brightness: 0.1,
        contrast: 1.0,
    };
    assert!(approx_rgb(b.apply([0.4, 0.4, 0.4, 1.0]), [0.5, 0.5, 0.5]));
    // 2x contrast: 0.5 is the pivot (unchanged), 0.75 pushes toward white.
    let c = Effect::BrightnessContrast {
        brightness: 0.0,
        contrast: 2.0,
    };
    assert!((c.apply([0.5, 0.5, 0.5, 1.0])[0] - 0.5).abs() < 1e-4);
    assert!(c.apply([0.75, 0.75, 0.75, 1.0])[0] > 0.75);
}

#[test]
fn exposure_doubles_per_stop_and_clamps() {
    let e = Effect::Exposure {
        stops: 1.0,
        offset: 0.0,
        gamma: 1.0,
    };
    // +1 stop doubles linear value: 0.25 -> 0.5.
    assert!((e.apply([0.25, 0.25, 0.25, 1.0])[0] - 0.5).abs() < 1e-4);
    // Output is clamped into [0,1] (0.8 * 2 = 1.6 -> 1.0).
    assert_eq!(e.apply([0.8, 0.8, 0.8, 1.0])[0], 1.0);
}

#[test]
fn levels_identity_is_neutral_and_remaps_range() {
    let id = Effect::Levels {
        in_black: 0.0,
        in_white: 1.0,
        gamma: 1.0,
        out_black: 0.0,
        out_white: 1.0,
    };
    assert!(approx_rgb(id.apply([0.3, 0.6, 0.9, 1.0]), [0.3, 0.6, 0.9]));
    // Lift the input black point to 0.5: anything <=0.5 clamps to out_black 0.
    let lift = Effect::Levels {
        in_black: 0.5,
        in_white: 1.0,
        gamma: 1.0,
        out_black: 0.0,
        out_white: 1.0,
    };
    assert_eq!(lift.apply([0.5, 0.5, 0.5, 1.0])[0], 0.0);
    // The new white point (1.0) maps to out_white (1.0).
    assert!((lift.apply([1.0, 1.0, 1.0, 1.0])[0] - 1.0).abs() < 1e-4);
    // Midway (0.75) sits halfway in the remapped range.
    assert!((lift.apply([0.75, 0.75, 0.75, 1.0])[0] - 0.5).abs() < 1e-4);
}

#[test]
fn tint_maps_luma_between_black_and_white() {
    // Tint black->blue, white->red at full strength: a mid-gray maps to a
    // blend, pure black to blue, pure white to red.
    let e = Effect::Tint {
        black: [0.0, 0.0, 1.0],
        white: [1.0, 0.0, 0.0],
        amount: 1.0,
    };
    assert!(approx_rgb(e.apply([0.0, 0.0, 0.0, 1.0]), [0.0, 0.0, 1.0]));
    assert!(approx_rgb(e.apply([1.0, 1.0, 1.0, 1.0]), [1.0, 0.0, 0.0]));
}

#[test]
fn tint_amount_zero_is_passthrough() {
    let e = Effect::Tint {
        black: [0.0, 0.0, 0.0],
        white: [1.0, 1.0, 1.0],
        amount: 0.0,
    };
    assert!(approx_rgb(e.apply([0.2, 0.5, 0.8, 1.0]), [0.2, 0.5, 0.8]));
}

#[test]
fn apply_effects_chains_in_order() {
    // Brightness +0.5 then a Levels that remaps [0,0.5]->[0,1]: order matters.
    let stack = [
        Effect::BrightnessContrast {
            brightness: 0.5,
            contrast: 1.0,
        },
        Effect::Levels {
            in_black: 0.0,
            in_white: 0.5,
            gamma: 1.0,
            out_black: 0.0,
            out_white: 1.0,
        },
    ];
    // 0.0 -> +0.5 -> remapped (0.5/0.5)=1.0.
    let out = apply_effects(&stack, [0.0, 0.0, 0.0, 1.0]);
    assert!((out[0] - 1.0).abs() < 1e-4);
    // Empty stack is a passthrough.
    let same = apply_effects(&[], [0.1, 0.2, 0.3, 0.4]);
    assert_eq!(same, [0.1, 0.2, 0.3, 0.4]);
}

// --- Effect masks -------------------------------------------------------

/// A full-strength "make it white" grade, so the effected pixel is unmistakably
/// different from the original (black).
fn whiten_stack() -> [Effect; 1] {
    [Effect::BrightnessContrast {
        brightness: 1.0,
        contrast: 1.0,
    }]
}

#[test]
fn blend_masked_lerps_orig_to_effected() {
    let orig = [0.0, 0.0, 0.0, 1.0];
    let effected = [1.0, 1.0, 1.0, 1.0];
    // Coverage 0 = original, 1 = effected, 0.5 = halfway, channel-wise.
    assert_eq!(blend_masked(orig, effected, 0.0), orig);
    assert_eq!(blend_masked(orig, effected, 1.0), effected);
    assert!(approx_rgb(blend_masked(orig, effected, 0.5), [0.5, 0.5, 0.5]));
    // Out-of-range coverage clamps.
    assert_eq!(blend_masked(orig, effected, 2.0), effected);
    assert_eq!(blend_masked(orig, effected, -1.0), orig);
}

#[test]
fn effect_mask_disabled_applies_everywhere() {
    // Default (disabled) mask: the effect applies in full at any point — exactly
    // the legacy unmasked behaviour.
    let mask = EffectMask::default();
    assert!(!mask.is_active());
    let stack = whiten_stack();
    let full = apply_effects(&stack, [0.0, 0.0, 0.0, 1.0]);
    let out = apply_effects_masked(&stack, &mask, &[], 12.0, 34.0, [0.0, 0.0, 0.0, 1.0]);
    assert_eq!(out, full);
}

#[test]
fn effect_mask_gates_inside_vs_outside() {
    // A 100x100 rect region centred at the origin (layer-local px), hard edge.
    let mut mask = EffectMask {
        enabled: true,
        region: Mask::rect(50.0, 50.0),
    };
    mask.region.feather = 0.0;
    assert!(mask.is_active());
    let poly = mask.region.flatten();
    let stack = whiten_stack();
    let black = [0.0, 0.0, 0.0, 1.0];
    let effected = apply_effects(&stack, black);

    // A point inside the region gets the full effect; a point outside is untouched.
    let inside = apply_effects_masked(&stack, &mask, &poly, 0.0, 0.0, black);
    let outside = apply_effects_masked(&stack, &mask, &poly, 200.0, 200.0, black);
    assert!(approx_rgb(inside, [effected[0], effected[1], effected[2]]));
    assert_eq!(outside, black);
}

#[test]
fn effect_mask_invert_flips_the_region() {
    let mut mask = EffectMask {
        enabled: true,
        region: Mask::rect(50.0, 50.0),
    };
    mask.region.feather = 0.0;
    mask.region.inverted = true;
    let poly = mask.region.flatten();
    let stack = whiten_stack();
    let black = [0.0, 0.0, 0.0, 1.0];
    let effected = apply_effects(&stack, black);

    // Inverted: inside is now untouched, outside gets the effect.
    let inside = apply_effects_masked(&stack, &mask, &poly, 0.0, 0.0, black);
    let outside = apply_effects_masked(&stack, &mask, &poly, 200.0, 200.0, black);
    assert_eq!(inside, black);
    assert!(approx_rgb(outside, [effected[0], effected[1], effected[2]]));
}

#[test]
fn effect_mask_feather_gives_intermediate_blend() {
    // A feathered edge ramps coverage across the boundary, so a point right on the
    // edge of the rect blends original↔effected ~halfway.
    let mut mask = EffectMask {
        enabled: true,
        region: Mask::rect(50.0, 50.0),
    };
    mask.region.feather = 40.0; // wide feather straddling the x=50 edge
    let poly = mask.region.flatten();
    let stack = whiten_stack();
    let black = [0.0, 0.0, 0.0, 1.0];

    // On the boundary the feather centres coverage at ~0.5 → mid-gray.
    let edge = apply_effects_masked(&stack, &mask, &poly, 50.0, 0.0, black);
    assert!(
        edge[0] > 0.1 && edge[0] < 0.9,
        "feathered edge should be a partial blend, got {edge:?}"
    );
    // Deep inside is full effect, far outside is untouched.
    let deep_in = apply_effects_masked(&stack, &mask, &poly, 0.0, 0.0, black);
    let far_out = apply_effects_masked(&stack, &mask, &poly, 300.0, 0.0, black);
    assert!(deep_in[0] > edge[0]);
    assert!(far_out[0] < edge[0]);
}

#[test]
fn effect_mask_serde_roundtrips_and_defaults() {
    // Round-trip a layer with an active effect mask.
    let mut layer = PulseLayer::new("L", [0.0, 0.0, 0.0, 1.0]);
    layer.effects.push(Effect::BrightnessContrast {
        brightness: 1.0,
        contrast: 1.0,
    });
    layer.effect_mask.enabled = true;
    layer.effect_mask.region = Mask::ellipse(40.0, 30.0);
    layer.effect_mask.region.feather = 12.0;
    layer.effect_mask.region.inverted = true;
    let json = serde_json::to_string(&layer).unwrap();
    let back: PulseLayer = serde_json::from_str(&json).unwrap();
    assert_eq!(back.effect_mask, layer.effect_mask);

    // A legacy file with no `effect_mask` field loads with the mask disabled, so
    // the effect applies everywhere (back-compat).
    let old = r#"{"name":"L","color":[1.0,1.0,1.0,1.0],"visible":true,
        "x":{"keys":[]},"y":{"keys":[]},"scale":{"keys":[]},
        "rotation":{"keys":[]},"opacity":{"keys":[]}}"#;
    let legacy: PulseLayer = serde_json::from_str(old).unwrap();
    assert!(!legacy.effect_mask.enabled);
    assert!(!legacy.effect_mask.is_active());
}

#[test]
fn preset_captures_and_applies_effect_mask() {
    let mut src = PulseLayer::new("Src", [0.0, 0.0, 0.0, 1.0]);
    src.effects.push(Effect::BrightnessContrast {
        brightness: 1.0,
        contrast: 1.0,
    });
    src.effect_mask.enabled = true;
    src.effect_mask.region = Mask::rect(20.0, 20.0);
    src.effect_mask.region.feather = 5.0;

    let preset = AnimationPreset::capture("p", &src);
    let mut dst = PulseLayer::new("Dst", [0.0, 0.0, 0.0, 1.0]);
    preset.apply(&mut dst);
    assert_eq!(dst.effect_mask, src.effect_mask);
}

// --- Hue / Saturation, Curves, Color Balance ----------------------------

#[test]
fn hsl_round_trips() {
    // RGB -> HSL -> RGB recovers the original across a spread of colors.
    for c in [
        [0.0, 0.0, 0.0],
        [1.0, 1.0, 1.0],
        [0.5, 0.5, 0.5],
        [0.8, 0.2, 0.4],
        [0.1, 0.7, 0.3],
        [0.25, 0.4, 0.95],
    ] {
        let (h, s, l) = rgb_to_hsl(c[0], c[1], c[2]);
        let back = hsl_to_rgb(h, s, l);
        assert!(
            approx_rgb([back[0], back[1], back[2], 1.0], c),
            "round-trip failed for {c:?} -> ({h},{s},{l}) -> {back:?}"
        );
    }
}

#[test]
fn hue_saturation_identity_and_desaturate() {
    // Zeroed params are a no-op.
    let id = Effect::HueSaturation {
        hue: 0.0,
        saturation: 0.0,
        lightness: 0.0,
    };
    assert!(approx_rgb(id.apply([0.8, 0.2, 0.4, 1.0]), [0.8, 0.2, 0.4]));
    // Full desaturate (-1) collapses to gray (R==G==B at the pixel's luma-ish L).
    let gray = Effect::HueSaturation {
        hue: 0.0,
        saturation: -1.0,
        lightness: 0.0,
    };
    let out = gray.apply([0.8, 0.2, 0.4, 1.0]);
    assert!((out[0] - out[1]).abs() < 1e-4 && (out[1] - out[2]).abs() < 1e-4);
    // Alpha untouched.
    assert_eq!(gray.apply([0.8, 0.2, 0.4, 0.5])[3], 0.5);
}

#[test]
fn hue_rotation_120_cycles_channels() {
    // A pure-red pixel rotated +120° in hue becomes pure green (HSL hue wheel).
    let e = Effect::HueSaturation {
        hue: 120.0,
        saturation: 0.0,
        lightness: 0.0,
    };
    let out = e.apply([1.0, 0.0, 0.0, 1.0]);
    assert!(approx_rgb(out, [0.0, 1.0, 0.0]), "red+120 -> {out:?}");
}

#[test]
fn curves_identity_is_passthrough() {
    let id = Effect::Curves {
        points: Effect::CURVE_IDENTITY,
    };
    for v in [0.0, 0.1, 0.25, 0.5, 0.75, 0.9, 1.0] {
        assert!(
            (id.apply([v, v, v, 1.0])[0] - v).abs() < 1e-4,
            "identity curve changed {v}"
        );
    }
}

#[test]
fn curve_eval_hits_control_points() {
    // The spline passes exactly through the five control points at 0,¼,½,¾,1.
    let pts = [0.1, 0.3, 0.4, 0.85, 0.95];
    for (i, &expect) in pts.iter().enumerate() {
        let x = i as f32 * 0.25;
        assert!(
            (curve_eval(&pts, x) - expect).abs() < 1e-4,
            "curve at {x} = {} want {expect}",
            curve_eval(&pts, x)
        );
    }
    // Out-of-range inputs clamp to the end points.
    assert!((curve_eval(&pts, -1.0) - 0.1).abs() < 1e-4);
    assert!((curve_eval(&pts, 2.0) - 0.95).abs() < 1e-4);
}

#[test]
fn curves_lift_brightens_midtones() {
    // Raise the midpoint output: a mid-gray input lands brighter, ends pinned.
    let lift = Effect::Curves {
        points: [0.0, 0.4, 0.7, 0.9, 1.0],
    };
    assert!(lift.apply([0.5, 0.5, 0.5, 1.0])[0] > 0.5);
    assert!((lift.apply([0.0, 0.0, 0.0, 1.0])[0]).abs() < 1e-4);
    assert!((lift.apply([1.0, 1.0, 1.0, 1.0])[0] - 1.0).abs() < 1e-4);
}

#[test]
fn smoothstep_endpoints_and_midpoint() {
    assert_eq!(smoothstep(0.0, 1.0, -0.5), 0.0);
    assert_eq!(smoothstep(0.0, 1.0, 1.5), 1.0);
    assert!((smoothstep(0.0, 1.0, 0.5) - 0.5).abs() < 1e-6);
    // Degenerate edges (e0 == e1) act as a hard step.
    assert_eq!(smoothstep(0.5, 0.5, 0.4), 0.0);
    assert_eq!(smoothstep(0.5, 0.5, 0.6), 1.0);
}

#[test]
fn color_balance_zero_is_passthrough() {
    let id = Effect::ColorBalance {
        shadows: [0.0; 3],
        midtones: [0.0; 3],
        highlights: [0.0; 3],
    };
    assert!(approx_rgb(id.apply([0.2, 0.5, 0.8, 1.0]), [0.2, 0.5, 0.8]));
    assert_eq!(id.apply([0.2, 0.5, 0.8, 0.6])[3], 0.6);
}

#[test]
fn color_balance_pushes_target_range() {
    // A highlight red push reddens a bright pixel far more than a dark one.
    let e = Effect::ColorBalance {
        shadows: [0.0; 3],
        midtones: [0.0; 3],
        highlights: [1.0, 0.0, 0.0],
    };
    let bright = e.apply([0.9, 0.9, 0.9, 1.0]);
    let dark = e.apply([0.1, 0.1, 0.1, 1.0]);
    let bright_gain = bright[0] - 0.9;
    let dark_gain = dark[0] - 0.1;
    assert!(
        bright_gain > dark_gain,
        "highlight push should weight brights: bright +{bright_gain}, dark +{dark_gain}"
    );
    // The push only moves red here; green/blue at the bright pixel are ~unchanged.
    assert!((bright[1] - 0.9).abs() < 1e-3 && (bright[2] - 0.9).abs() < 1e-3);
}

#[test]
fn new_effects_preserve_alpha() {
    // Every default effect (including the three new ones) leaves alpha intact.
    let px = [0.4, 0.55, 0.7, 0.42];
    for e in Effect::defaults() {
        assert_eq!(e.apply(px)[3], 0.42, "{} changed alpha", e.label());
    }
}

// --- Channel Mixer, Gradient Map, Tritone -------------------------------

#[test]
fn channel_mixer_identity_is_passthrough() {
    // The default channel mixer (each output = its own input) is a no-op.
    let id = Effect::ChannelMixer {
        red: [1.0, 0.0, 0.0, 0.0],
        green: [0.0, 1.0, 0.0, 0.0],
        blue: [0.0, 0.0, 1.0, 0.0],
        monochrome: false,
    };
    assert!(approx_rgb(id.apply([0.2, 0.5, 0.8, 1.0]), [0.2, 0.5, 0.8]));
    assert_eq!(id.apply([0.2, 0.5, 0.8, 0.3])[3], 0.3);
}

#[test]
fn channel_mixer_swaps_red_from_blue() {
    // Output red sourced entirely from input blue (R←B); green/blue unchanged.
    let swap = Effect::ChannelMixer {
        red: [0.0, 0.0, 1.0, 0.0],
        green: [0.0, 1.0, 0.0, 0.0],
        blue: [0.0, 0.0, 1.0, 0.0],
        monochrome: false,
    };
    let out = swap.apply([0.1, 0.4, 0.9, 1.0]);
    assert!(approx_rgb(out, [0.9, 0.4, 0.9]), "R<-B failed: {out:?}");
}

#[test]
fn channel_mixer_constant_and_clamp() {
    // A +0.5 constant lifts the channel; output stays clamped to [0,1].
    let lift = Effect::ChannelMixer {
        red: [1.0, 0.0, 0.0, 0.5],
        green: [0.0, 1.0, 0.0, 0.0],
        blue: [0.0, 0.0, 1.0, 0.0],
        monochrome: false,
    };
    assert!((lift.apply([0.2, 0.2, 0.2, 1.0])[0] - 0.7).abs() < 1e-4);
    // 0.8 + 0.5 = 1.3 -> clamped to 1.0.
    assert_eq!(lift.apply([0.8, 0.2, 0.2, 1.0])[0], 1.0);
}

#[test]
fn channel_mixer_monochrome_writes_gray_from_red_row() {
    // Monochrome collapses every output to the red row's weighted gray.
    let mono = Effect::ChannelMixer {
        red: [0.3, 0.59, 0.11, 0.0], // luma-ish weights
        green: [0.0, 1.0, 0.0, 0.0], // ignored when monochrome
        blue: [0.0, 0.0, 1.0, 0.0],  // ignored when monochrome
        monochrome: true,
    };
    let out = mono.apply([1.0, 0.0, 0.0, 1.0]);
    assert!((out[0] - out[1]).abs() < 1e-6 && (out[1] - out[2]).abs() < 1e-6);
    assert!((out[0] - 0.3).abs() < 1e-4, "gray = {}", out[0]);
}

#[test]
fn channel_mixer_matches_shared_prism_core_math() {
    // Pulse must defer to the shared prism_core ChannelMixerMatrix — assert the
    // result is bit-identical to calling the shared math directly (no reimpl).
    let red = [0.4, 0.2, 0.1, 0.05];
    let green = [0.1, 0.7, 0.2, 0.0];
    let blue = [0.0, 0.3, 0.6, -0.1];
    let e = Effect::ChannelMixer {
        red,
        green,
        blue,
        monochrome: false,
    };
    let px = [0.35, 0.6, 0.8];
    let shared = prism_core::adjust::ChannelMixerMatrix {
        r: red,
        g: green,
        b: blue,
        monochrome: false,
    }
    .apply(px);
    let out = e.apply([px[0], px[1], px[2], 1.0]);
    assert_eq!([out[0], out[1], out[2]], shared);
}

#[test]
fn gradient_map_black_white_mid() {
    // Map black->first stop, white->last stop, mid-gray->mid stop.
    let e = Effect::GradientMap {
        low: [0.0, 0.0, 1.0],  // blue shadows
        mid: [0.0, 1.0, 0.0],  // green mids
        high: [1.0, 0.0, 0.0], // red highlights
        amount: 1.0,
    };
    assert!(approx_rgb(e.apply([0.0, 0.0, 0.0, 1.0]), [0.0, 0.0, 1.0]));
    assert!(approx_rgb(e.apply([1.0, 1.0, 1.0, 1.0]), [1.0, 0.0, 0.0]));
    // Pure gray at luma 0.5 lands on the mid stop.
    let mid = e.apply([0.5, 0.5, 0.5, 1.0]);
    assert!(approx_rgb(mid, [0.0, 1.0, 0.0]), "mid = {mid:?}");
}

#[test]
fn gradient_map_amount_zero_is_passthrough() {
    let e = Effect::GradientMap {
        low: [0.0, 0.0, 1.0],
        mid: [0.0, 1.0, 0.0],
        high: [1.0, 0.0, 0.0],
        amount: 0.0,
    };
    assert!(approx_rgb(e.apply([0.2, 0.5, 0.8, 1.0]), [0.2, 0.5, 0.8]));
    assert_eq!(e.apply([0.2, 0.5, 0.8, 0.7])[3], 0.7);
}

#[test]
fn gradient_map_interpolates_between_stops() {
    // A grayscale identity gradient (black/gray/white) maps luma->luma; a value
    // a quarter of the way up should read ~that luma on all channels.
    let e = Effect::GradientMap {
        low: [0.0, 0.0, 0.0],
        mid: [0.5, 0.5, 0.5],
        high: [1.0, 1.0, 1.0],
        amount: 1.0,
    };
    let out = e.apply([0.25, 0.25, 0.25, 1.0]);
    assert!((out[0] - 0.25).abs() < 1e-3, "out = {out:?}");
    assert!((out[0] - out[1]).abs() < 1e-6 && (out[1] - out[2]).abs() < 1e-6);
}

#[test]
fn tritone_maps_three_tones_by_luma() {
    // Tritone shares the gradient-map primitive: dark->shadows, mid->midtones,
    // bright->highlights.
    let e = Effect::Tritone {
        shadows: [0.1, 0.0, 0.3],
        midtones: [0.6, 0.4, 0.2],
        highlights: [1.0, 0.95, 0.8],
        amount: 1.0,
    };
    assert!(approx_rgb(e.apply([0.0, 0.0, 0.0, 1.0]), [0.1, 0.0, 0.3]));
    assert!(approx_rgb(e.apply([0.5, 0.5, 0.5, 1.0]), [0.6, 0.4, 0.2]));
    assert!(approx_rgb(e.apply([1.0, 1.0, 1.0, 1.0]), [1.0, 0.95, 0.8]));
    // Alpha untouched.
    assert_eq!(e.apply([0.5, 0.5, 0.5, 0.4])[3], 0.4);
}

#[test]
fn color_effects_are_deterministic() {
    // The new color effects are pure: identical inputs yield identical outputs.
    let px = [0.33, 0.61, 0.27, 0.9];
    for e in [
        Effect::defaults()[7], // Channel Mixer
        Effect::defaults()[8], // Gradient Map
        Effect::defaults()[9], // Tritone
    ] {
        assert_eq!(e.apply(px), e.apply(px), "{} not deterministic", e.label());
    }
}
