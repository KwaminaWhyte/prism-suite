use super::*;

// --- Keying effects (Color / Luma / Chroma Key, Spill, Matte Choke) -----

/// A `w×h` premultiplied, fully-opaque buffer flooded with one **linear-light**
/// colour. (The compositor buffer the keyers operate on is linear; the keyers'
/// own `key` colour is authored sRGB and decoded to linear at the pass boundary,
/// so to make a buffer pixel that *matches* a sRGB key colour, decode it here.)
fn flood_buf(w: usize, h: usize, c: [f32; 3]) -> Vec<[f32; 4]> {
    vec![[c[0], c[1], c[2], 1.0]; w * h]
}

/// Decode a straight sRGB colour to linear (mirrors the keyers' key-colour decode)
/// so a test buffer pixel can be made to sit exactly on a sRGB key colour.
fn lin3(c: [f32; 3]) -> [f32; 3] {
    [
        prism_core::color::srgb_to_linear(c[0]),
        prism_core::color::srgb_to_linear(c[1]),
        prism_core::color::srgb_to_linear(c[2]),
    ]
}

#[test]
fn key_label_and_defaults_are_consistent() {
    // Each default's label is stable and the defaults array has one per variant.
    let d = KeyEffect::defaults();
    assert_eq!(d.len(), 5);
    assert_eq!(d[0].label(), "Color Key");
    assert_eq!(d[1].label(), "Luma Key");
    assert_eq!(d[2].label(), "Chroma Key");
    assert_eq!(d[3].label(), "Spill Suppression");
    assert_eq!(d[4].label(), "Matte Choke");
}

#[test]
fn key_apply_ignores_empty_buffer() {
    // Degenerate sizes are a no-op (no panic).
    let mut empty: Vec<[f32; 4]> = Vec::new();
    KeyEffect::ColorKey {
        key: [0.0, 0.6, 0.1],
        tolerance: 0.1,
        softness: 0.0,
    }
    .apply(&mut empty, 0, 0);
    assert!(empty.is_empty());
}

#[test]
fn color_key_drops_target_keeps_others() {
    // The target colour is keyed to zero alpha; a far colour is untouched.
    let key = [0.0, 0.6, 0.1];
    // The on-key buffer pixel is the *linear* version of the key (the buffer is
    // linear-light, the key colour is sRGB-decoded to linear inside the keyer).
    let mut buf = flood_buf(2, 1, lin3(key));
    let red = lin3([0.9, 0.1, 0.1]);
    buf[1] = [red[0], red[1], red[2], 1.0]; // a red pixel, far from the green key
    KeyEffect::ColorKey {
        key,
        tolerance: 0.05,
        softness: 0.02,
    }
    .apply(&mut buf, 2, 1);
    assert!(buf[0][3] < 0.01, "on-key pixel is keyed out, got {}", buf[0][3]);
    assert!(buf[1][3] > 0.99, "off-key pixel is kept, got {}", buf[1][3]);
}

#[test]
fn color_key_softness_feathers_the_edge() {
    // A pixel just past the tolerance reads a partial (feathered) alpha in the
    // softness band, between fully keyed and fully kept. The key is black, so the
    // distance is just the (linear) grey value's magnitude.
    let key = [0.0, 0.0, 0.0];
    let mut buf = vec![[0.18f32, 0.18, 0.18, 1.0]]; // linear grey ~0.31 from black
    KeyEffect::ColorKey {
        key,
        tolerance: 0.1,
        softness: 0.3,
    }
    .apply(&mut buf, 1, 1);
    let a = buf[0][3];
    assert!(a > 0.0 && a < 1.0, "softness band gives a partial alpha, got {a}");
}

#[test]
fn luma_key_threshold_direction_and_softness() {
    // key_high=false drops the dark side: a dark pixel keys out, a bright one
    // stays. key_high=true flips that. Softness yields a partial alpha mid-band.
    let dark = [0.05f32, 0.05, 0.05];
    let bright = [0.9f32, 0.9, 0.9];

    let mut lo = vec![[dark[0], dark[1], dark[2], 1.0], [bright[0], bright[1], bright[2], 1.0]];
    KeyEffect::LumaKey {
        threshold: 0.5,
        softness: 0.05,
        key_high: false,
    }
    .apply(&mut lo, 2, 1);
    assert!(lo[0][3] < 0.01, "dark side keyed out when key_high=false");
    assert!(lo[1][3] > 0.99, "bright side kept when key_high=false");

    let mut hi = vec![[dark[0], dark[1], dark[2], 1.0], [bright[0], bright[1], bright[2], 1.0]];
    KeyEffect::LumaKey {
        threshold: 0.5,
        softness: 0.05,
        key_high: true,
    }
    .apply(&mut hi, 2, 1);
    assert!(hi[0][3] > 0.99, "dark side kept when key_high=true");
    assert!(hi[1][3] < 0.01, "bright side keyed out when key_high=true");

    // A mid-luma pixel strictly inside the softness band reads a partial alpha.
    // key_high=false ramps over [threshold - softness, threshold] = [0.1, 0.5];
    // a luma of 0.3 sits in the middle of that ramp.
    let mut mid = vec![[0.3f32, 0.3, 0.3, 1.0]];
    KeyEffect::LumaKey {
        threshold: 0.5,
        softness: 0.4,
        key_high: false,
    }
    .apply(&mut mid, 1, 1);
    let a = mid[0][3];
    assert!(a > 0.0 && a < 1.0, "softness gives a partial luma-key alpha, got {a}");
}

#[test]
fn chroma_key_distance_keying() {
    // A pixel at the key chroma keys out; an off-chroma colour (red) stays — the
    // hallmark of chroma keying (distance in the Cb/Cr plane, not RGB). The chroma
    // axes subtract luminance, so adding an equal amount to every channel leaves
    // the chroma unchanged: a *lit* version of the backing (key + white) still
    // keys, while a chroma far from the key (red) survives even at the same
    // brightness. Buffer pixels are linear-light (the key colour is sRGB-decoded
    // to linear inside the keyer).
    let key = [0.0, 0.6, 0.1];
    let on = lin3(key); // exactly the key chroma
    // Lift every channel equally → same chroma, higher luminance (a lit backing).
    let lit = [on[0] + 0.2, on[1] + 0.2, on[2] + 0.2];
    let red = lin3([0.9, 0.1, 0.1]); // a chroma far from the key
    let mut buf = vec![
        [on[0], on[1], on[2], 1.0],
        [lit[0], lit[1], lit[2], 1.0],
        [red[0], red[1], red[2], 1.0],
    ];
    KeyEffect::ChromaKey {
        key,
        gain: 1.0,
        balance: 0.5,
        softness: 0.05,
    }
    .apply(&mut buf, 3, 1);
    assert!(buf[0][3] < 0.2, "on-chroma keyed, got {}", buf[0][3]);
    assert!(
        buf[1][3] < 0.2,
        "lit backing (same chroma) still keyed regardless of luma, got {}",
        buf[1][3]
    );
    assert!(buf[2][3] > 0.9, "off-chroma kept, got {}", buf[2][3]);
}

#[test]
fn spill_suppression_reduces_the_key_channel() {
    // A pixel with green spill (green channel above the other two) has its green
    // pulled back toward the average; alpha is untouched.
    let mut buf = vec![[0.3f32, 0.8, 0.3, 1.0]]; // green-dominant
    let before = buf[0][1];
    KeyEffect::SpillSuppression {
        key: [0.0, 1.0, 0.0], // green key → dominant channel is green
        amount: 1.0,
    }
    .apply(&mut buf, 1, 1);
    assert!(buf[0][1] < before, "green spill reduced from {before} to {}", buf[0][1]);
    // Pulled toward the average of the other two (both 0.3).
    assert!((buf[0][1] - 0.3).abs() < 1e-4, "green neutralised to the others");
    assert_eq!(buf[0][3], 1.0, "spill leaves alpha alone");
}

#[test]
fn matte_choke_erodes_and_dilates_alpha() {
    // A single opaque pixel in a transparent field. Eroding (negative choke)
    // wipes it (its neighbours are transparent); dilating (positive choke) grows
    // its coverage to neighbours.
    let w = 5;
    let h = 5;
    let center = 2 * w + 2;

    let mut erode = vec![[0.0f32; 4]; w * h];
    erode[center] = [1.0, 1.0, 1.0, 1.0];
    KeyEffect::MatteChoke {
        choke: -1.0,
        clip_black: 0.0,
        clip_white: 1.0,
    }
    .apply(&mut erode, w, h);
    assert_eq!(erode[center][3], 0.0, "erode wipes the lone pixel");

    let mut dilate = vec![[0.0f32; 4]; w * h];
    dilate[center] = [1.0, 1.0, 1.0, 1.0];
    KeyEffect::MatteChoke {
        choke: 1.0,
        clip_black: 0.0,
        clip_white: 1.0,
    }
    .apply(&mut dilate, w, h);
    let grown: f32 = dilate.iter().map(|p| p[3]).sum();
    assert!(grown > 1.0, "dilate grows coverage to neighbours, total {grown}");
}

#[test]
fn matte_choke_clip_levels_crush_the_tails() {
    // Clip black raises everything at/below it to 0; clip white drops everything
    // at/above it to 1; the middle rescales.
    let mut buf = vec![
        [0.1f32, 0.1, 0.1, 0.1], // below clip_black → 0
        [0.5f32, 0.5, 0.5, 0.5], // mid → rescaled into (0,1)
        [0.95f32, 0.95, 0.95, 0.95], // above clip_white → 1
    ];
    KeyEffect::MatteChoke {
        choke: 0.0,
        clip_black: 0.2,
        clip_white: 0.8,
    }
    .apply(&mut buf, 3, 1);
    assert_eq!(buf[0][3], 0.0, "low tail clipped to 0");
    assert_eq!(buf[2][3], 1.0, "high tail clipped to 1");
    assert!(buf[1][3] > 0.0 && buf[1][3] < 1.0, "mid rescaled, got {}", buf[1][3]);
}

#[test]
fn apply_key_effects_stacks_in_order() {
    // A Color Key (pulls a soft matte) followed by a Matte Choke clip-white that
    // hardens it differs from the Color Key alone — confirming the stack runs
    // both passes in order.
    let key = [0.0, 0.6, 0.1];
    let mut buf = flood_buf(4, 1, [0.1, 0.5, 0.15]); // near, but not on, the key
    let mut one = buf.clone();
    KeyEffect::ColorKey {
        key,
        tolerance: 0.0,
        softness: 0.6,
    }
    .apply(&mut one, 4, 1);
    apply_key_effects(
        &[
            KeyEffect::ColorKey {
                key,
                tolerance: 0.0,
                softness: 0.6,
            },
            KeyEffect::MatteChoke {
                choke: 0.0,
                clip_black: 0.0,
                clip_white: 0.5,
            },
        ],
        &mut buf,
        4,
        1,
    );
    assert_ne!(buf, one, "the choke pass after the key must change the result");
}

#[test]
fn key_is_deterministic() {
    let key = [0.0, 0.6, 0.1];
    let effects = [
        KeyEffect::ChromaKey {
            key,
            gain: 1.2,
            balance: 0.5,
            softness: 0.1,
        },
        KeyEffect::MatteChoke {
            choke: 1.0,
            clip_black: 0.05,
            clip_white: 0.95,
        },
    ];
    let mut a = flood_buf(8, 8, [0.1, 0.55, 0.12]);
    let mut b = a.clone();
    apply_key_effects(&effects, &mut a, 8, 8);
    apply_key_effects(&effects, &mut b, 8, 8);
    assert_eq!(a, b, "keying must be deterministic");
}

#[test]
fn key_effects_serde_defaults_to_empty() {
    // Pre-keying layers (no `key_effects` field) load with none.
    let json = r#"{"name":"L","color":[1.0,1.0,1.0,1.0],"visible":true,
        "x":{"keys":[]},"y":{"keys":[]},"scale":{"keys":[]},
        "rotation":{"keys":[]},"opacity":{"keys":[]}}"#;
    let layer: PulseLayer = serde_json::from_str(json).unwrap();
    assert!(layer.key_effects.is_empty());
    assert!(!layer.has_key_effects());
}

#[test]
fn key_effects_serde_round_trips() {
    // A layer with a full key stack survives a JSON round-trip value-for-value.
    let mut layer = PulseLayer::new("L", [1.0, 1.0, 1.0, 1.0]);
    layer.key_effects = vec![
        KeyEffect::ColorKey {
            key: [0.0, 0.6, 0.1],
            tolerance: 0.15,
            softness: 0.1,
        },
        KeyEffect::LumaKey {
            threshold: 0.4,
            softness: 0.05,
            key_high: true,
        },
        KeyEffect::MatteChoke {
            choke: -2.0,
            clip_black: 0.1,
            clip_white: 0.9,
        },
    ];
    let json = serde_json::to_string(&layer).unwrap();
    let back: PulseLayer = serde_json::from_str(&json).unwrap();
    assert_eq!(back.key_effects, layer.key_effects);
    assert!(back.has_key_effects());
}

#[test]
fn spatial_effects_serde_defaults_to_empty() {
    // Pre-spatial-effect layers (no `spatial_effects` field) load with none.
    let json = r#"{"name":"L","color":[1.0,1.0,1.0,1.0],"visible":true,
        "x":{"keys":[]},"y":{"keys":[]},"scale":{"keys":[]},
        "rotation":{"keys":[]},"opacity":{"keys":[]}}"#;
    let layer: PulseLayer = serde_json::from_str(json).unwrap();
    assert!(layer.spatial_effects.is_empty());
    assert!(!layer.has_spatial_effects());
}

#[test]
fn footage_serde_defaults_to_empty() {
    // A pre-footage layer (no `footage` field, no `kind`) loads as a Solid with
    // an empty footage block, so old projects are unaffected.
    let json = r#"{"name":"L","color":[1.0,1.0,1.0,1.0],"visible":true,
        "x":{"keys":[]},"y":{"keys":[]},"scale":{"keys":[]},
        "rotation":{"keys":[]},"opacity":{"keys":[]}}"#;
    let layer: PulseLayer = serde_json::from_str(json).unwrap();
    assert_eq!(layer.kind, LayerKind::Solid);
    assert!(!layer.footage.is_set());
    assert!(!layer.has_footage());
}

#[test]
fn footage_layer_serde_round_trips() {
    // A fully-configured footage layer (sequence source + alpha / fps / loop)
    // survives a JSON round-trip byte-for-value, including the FootageSource and
    // the new LayerKind::Footage variant.
    let mut layer = PulseLayer::of_kind(LayerKind::Footage, "Plate", [0.5, 0.5, 0.5, 1.0]);
    layer.footage.source = Some(FootageSource::Sequence {
        pattern: "shot/img_{}.png".to_string(),
        pad: 4,
        start: 1,
        count: 240,
    });
    layer.footage.alpha = AlphaMode::Premultiplied;
    layer.footage.fps = Some(24.0);
    layer.footage.looping = true;
    layer.footage.hold_last = false;

    let json = serde_json::to_string(&layer).unwrap();
    let back: PulseLayer = serde_json::from_str(&json).unwrap();
    assert_eq!(back.kind, LayerKind::Footage);
    assert!(back.has_footage());
    assert_eq!(back.footage.alpha, AlphaMode::Premultiplied);
    assert_eq!(back.footage.fps, Some(24.0));
    assert!(back.footage.looping);
    assert!(!back.footage.hold_last);
    assert_eq!(back.footage.source, layer.footage.source);
}

#[test]
fn footage_hold_last_serde_defaults_true() {
    // A footage block missing `hold_last` (and most fields) loads with the
    // sensible default (hold the last frame), per the field's serde default.
    let json = r#"{"name":"F","kind":"Footage","color":[0.5,0.5,0.5,1.0],"visible":true,
        "footage":{"source":null},
        "x":{"keys":[]},"y":{"keys":[]},"scale":{"keys":[]},
        "rotation":{"keys":[]},"opacity":{"keys":[]}}"#;
    let layer: PulseLayer = serde_json::from_str(json).unwrap();
    assert_eq!(layer.kind, LayerKind::Footage);
    assert!(layer.footage.hold_last, "hold_last serde-defaults to true");
    assert!(!layer.footage.looping);
    assert_eq!(layer.footage.fps, None);
}
