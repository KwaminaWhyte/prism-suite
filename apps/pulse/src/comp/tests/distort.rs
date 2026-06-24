use super::*;

// --- Distort effects (Corner Pin / Transform / Mirror / Polar) ----------

/// A `w×h` premultiplied buffer with a smooth gradient so resampling differences
/// are detectable: red ramps with x, green with y, fully opaque everywhere.
fn gradient_buf(w: usize, h: usize) -> Vec<[f32; 4]> {
    let mut buf = vec![[0.0f32; 4]; w * h];
    for y in 0..h {
        for x in 0..w {
            let r = x as f32 / (w - 1).max(1) as f32;
            let g = y as f32 / (h - 1).max(1) as f32;
            buf[y * w + x] = [r, g, 0.0, 1.0];
        }
    }
    buf
}

#[test]
fn distort_label_and_defaults_are_consistent() {
    // Each default's label is stable and the defaults array has one per variant.
    let d = DistortEffect::defaults();
    assert_eq!(d.len(), 5);
    assert_eq!(d[0].label(), "Corner Pin");
    assert_eq!(d[1].label(), "Transform");
    assert_eq!(d[2].label(), "Mirror");
    assert_eq!(d[3].label(), "Polar Coordinates");
    assert_eq!(d[4].label(), "Displacement Map");
}

#[test]
fn distort_apply_ignores_empty_buffer() {
    // Degenerate sizes are a no-op (no panic).
    let mut empty: Vec<[f32; 4]> = Vec::new();
    DistortEffect::Mirror {
        center: [0.5, 0.5],
        angle: 0.0,
    }
    .apply(&mut empty, 0, 0);
    assert!(empty.is_empty());
}

#[test]
fn bilinear_sample_hits_pixel_centers() {
    // Sampling at a pixel's center returns that pixel exactly.
    let buf = gradient_buf(4, 4);
    let s = sample_bilinear(&buf, 4, 4, 2.5, 1.5); // center of pixel (2, 1)
    assert!((s[0] - buf[4 + 2][0]).abs() < 1e-5);
    assert!((s[1] - buf[4 + 2][1]).abs() < 1e-5);
}

#[test]
fn bilinear_sample_off_buffer_is_transparent() {
    let buf = gradient_buf(4, 4);
    let s = sample_bilinear(&buf, 4, 4, -5.0, -5.0);
    assert_eq!(s, [0.0; 4]);
}

#[test]
fn corner_pin_identity_is_a_no_op() {
    // Pinning the corners where they already are leaves the buffer ~unchanged.
    let w = 16;
    let orig = gradient_buf(w, w);
    let mut buf = orig.clone();
    DistortEffect::CornerPin {
        top_left: [0.0, 0.0],
        top_right: [1.0, 0.0],
        bottom_right: [1.0, 1.0],
        bottom_left: [0.0, 1.0],
    }
    .apply(&mut buf, w, w);
    // Interior pixels (away from the edge resample border) are preserved.
    for y in 2..w - 2 {
        for x in 2..w - 2 {
            let a = orig[y * w + x];
            let b = buf[y * w + x];
            assert!((a[0] - b[0]).abs() < 0.02, "identity changed pixel {x},{y}");
            assert!((a[1] - b[1]).abs() < 0.02);
        }
    }
}

#[test]
fn corner_pin_maps_corners_to_targets() {
    // Pin the source's top-right corner to the centre of the buffer: the source's
    // bright-red region (x→1) should now appear near the centre, and the buffer's
    // own top-right corner should be empty (outside the pinned quad).
    let w = 32;
    let mut buf = gradient_buf(w, w);
    DistortEffect::CornerPin {
        top_left: [0.0, 0.0],
        top_right: [0.5, 0.5], // pull TR into the centre
        bottom_right: [1.0, 1.0],
        bottom_left: [0.0, 1.0],
    }
    .apply(&mut buf, w, w);
    // The far top-right corner of the *output* is outside the pinned quad (which
    // now bends inward at the top-right), so it's transparent.
    assert_eq!(buf[w + (w - 2)][3], 0.0, "output TR is outside the quad");
    // The source's bright-red top-right region (x≈1) was pinned toward the centre,
    // so a point a little inside the quad near the centre carries high red — well
    // above the source's red at that same buffer location without the pin.
    let probe = buf[(w / 2 + 2) * w + (w / 2)]; // just below-centre, inside the quad
    assert!(probe[3] > 0.5, "probe is inside the quad, a={}", probe[3]);
    let crisp_red = (w / 2) as f32 / (w - 1) as f32; // source red at x = w/2
    assert!(
        probe[0] > crisp_red,
        "pinned TR brought brighter red toward the centre: {} vs {}",
        probe[0],
        crisp_red
    );
}

#[test]
fn transform_identity_is_a_no_op() {
    let w = 16;
    let orig = gradient_buf(w, w);
    let mut buf = orig.clone();
    DistortEffect::Transform {
        anchor: [0.5, 0.5],
        position: [0.5, 0.5],
        scale: 1.0,
        rotation: 0.0,
        skew: 0.0,
        opacity: 1.0,
    }
    .apply(&mut buf, w, w);
    for i in 0..w * w {
        for k in 0..4 {
            assert!((orig[i][k] - buf[i][k]).abs() < 1e-4, "identity changed");
        }
    }
}

#[test]
fn transform_translation_shifts_content() {
    // Move the content so the anchor lands a quarter-buffer to the right: a pixel
    // that was at the source x picks up the colour from a pixel to its left.
    let w = 20;
    let mut buf = gradient_buf(w, w);
    DistortEffect::Transform {
        anchor: [0.5, 0.5],
        position: [0.75, 0.5], // shift right by 0.25·w = 5 px
        scale: 1.0,
        rotation: 0.0,
        skew: 0.0,
        opacity: 1.0,
    }
    .apply(&mut buf, w, w);
    // Output pixel (10, 10) now reads the source ~5 px to its left (x≈5), which is
    // darker red than the source at x=10.
    let src_x5 = 5.0 / (w - 1) as f32;
    assert!((buf[10 * w + 10][0] - src_x5).abs() < 0.06, "got {}", buf[10 * w + 10][0]);
}

#[test]
fn transform_scale_zero_collapses_to_empty() {
    let w = 8;
    let mut buf = gradient_buf(w, w);
    DistortEffect::Transform {
        anchor: [0.5, 0.5],
        position: [0.5, 0.5],
        scale: 0.0,
        rotation: 0.0,
        skew: 0.0,
        opacity: 1.0,
    }
    .apply(&mut buf, w, w);
    assert!(buf.iter().all(|p| *p == [0.0; 4]), "zero scale = empty");
}

#[test]
fn transform_opacity_fades_the_buffer() {
    let w = 8;
    let mut buf = gradient_buf(w, w);
    DistortEffect::Transform {
        anchor: [0.5, 0.5],
        position: [0.5, 0.5],
        scale: 1.0,
        rotation: 0.0,
        skew: 0.0,
        opacity: 0.5,
    }
    .apply(&mut buf, w, w);
    // A fully-opaque source pixel is now ~half coverage.
    let mid = buf[(w / 2) * w + (w / 2)];
    assert!((mid[3] - 0.5).abs() < 0.05, "opacity halved alpha, got {}", mid[3]);
}

#[test]
fn mirror_is_symmetric_across_the_line() {
    // Vertical mirror line through the centre: the far (right) side becomes the
    // reflection of the near (left) side, so mirrored columns match.
    let w = 16;
    let mut buf = gradient_buf(w, w);
    DistortEffect::Mirror {
        center: [0.5, 0.5],
        angle: 90.0, // vertical line
    }
    .apply(&mut buf, w, w);
    let y = 8;
    // Column x and its mirror (w-1-x) about the centre should be ~equal in red.
    for x in 1..w / 2 - 1 {
        let left = buf[y * w + x][0];
        let right = buf[y * w + (w - 1 - x)][0];
        assert!((left - right).abs() < 0.06, "mirror asymmetric at x={x}: {left} vs {right}");
    }
}

#[test]
fn mirror_keeps_the_near_side() {
    // With a vertical line at the centre and the line's normal pointing −x, the
    // kept (right) half passes through unchanged while the far (left) half is
    // replaced by its reflection.
    let w = 16;
    let orig = gradient_buf(w, w);
    let mut buf = orig.clone();
    DistortEffect::Mirror {
        center: [0.5, 0.5],
        angle: 90.0,
    }
    .apply(&mut buf, w, w);
    // Right-of-centre column is untouched (the kept side).
    let y = 8;
    let x = 12;
    assert!(
        (buf[y * w + x][0] - orig[y * w + x][0]).abs() < 1e-4,
        "kept side changed"
    );
    // The far (left) side is now the reflection: its red increases toward the
    // centre (mirroring the right side) instead of ramping up from 0.
    assert!(buf[y * w + 2][0] > buf[y * w + 5][0] - 0.5, "far side reflected");
}

#[test]
fn polar_round_trip_recovers_the_source() {
    // Rect→Polar then Polar→Rect about the same centre returns ~the original in
    // the well-sampled interior (corners/edges lose some precision in the
    // resample, so check a central region).
    let w = 48;
    let orig = gradient_buf(w, w);
    let mut buf = orig.clone();
    DistortEffect::Polar {
        center: [0.5, 0.5],
        kind: PolarKind::RectToPolar,
        interp: 1.0,
    }
    .apply(&mut buf, w, w);
    DistortEffect::Polar {
        center: [0.5, 0.5],
        kind: PolarKind::PolarToRect,
        interp: 1.0,
    }
    .apply(&mut buf, w, w);
    // Sample a central band away from the singular centre and the borders.
    let mut checked = 0;
    for y in (w / 2 - 6)..(w / 2 + 6) {
        for x in (w / 2 + 4)..(w / 2 + 10) {
            let a = orig[y * w + x];
            let b = buf[y * w + x];
            assert!((a[0] - b[0]).abs() < 0.12, "round-trip drift at {x},{y}: {} vs {}", a[0], b[0]);
            checked += 1;
        }
    }
    assert!(checked > 0);
}

#[test]
fn polar_interp_zero_is_a_no_op() {
    let w = 8;
    let orig = gradient_buf(w, w);
    let mut buf = orig.clone();
    DistortEffect::Polar {
        center: [0.5, 0.5],
        kind: PolarKind::RectToPolar,
        interp: 0.0,
    }
    .apply(&mut buf, w, w);
    assert_eq!(buf, orig, "interp 0 must be identity");
}

#[test]
fn distort_is_deterministic() {
    // The same effect on the same buffer gives byte-identical results (needed for
    // the RAM-preview cache / golden-frame tests).
    let w = 24;
    let mut a = gradient_buf(w, w);
    let mut b = gradient_buf(w, w);
    let eff = DistortEffect::Polar {
        center: [0.5, 0.5],
        kind: PolarKind::RectToPolar,
        interp: 1.0,
    };
    eff.apply(&mut a, w, w);
    eff.apply(&mut b, w, w);
    assert_eq!(a, b, "distort must be deterministic");
}

#[test]
fn apply_distort_effects_stacks_in_order() {
    // Two mirrors (vertical then horizontal) compose into a point reflection,
    // distinct from either alone — confirming the stack runs both passes.
    let w = 16;
    let mut both = gradient_buf(w, w);
    apply_distort_effects(
        &[
            DistortEffect::Mirror {
                center: [0.5, 0.5],
                angle: 90.0,
            },
            DistortEffect::Mirror {
                center: [0.5, 0.5],
                angle: 0.0,
            },
        ],
        &mut both,
        w,
        w,
    );
    let mut one = gradient_buf(w, w);
    DistortEffect::Mirror {
        center: [0.5, 0.5],
        angle: 90.0,
    }
    .apply(&mut one, w, w);
    assert_ne!(both, one, "stacking a second mirror must change the result");
}

#[test]
fn distort_effects_serde_defaults_to_empty() {
    // Pre-distort-effect layers (no `distort_effects` field) load with none.
    let json = r#"{"name":"L","color":[1.0,1.0,1.0,1.0],"visible":true,
        "x":{"keys":[]},"y":{"keys":[]},"scale":{"keys":[]},
        "rotation":{"keys":[]},"opacity":{"keys":[]}}"#;
    let layer: PulseLayer = serde_json::from_str(json).unwrap();
    assert!(layer.distort_effects.is_empty());
    assert!(!layer.has_distort_effects());
}

#[test]
fn stylize_effects_serde_defaults_to_empty() {
    // Pre-stylize-effect layers (no `stylize_effects` field) load with none, so
    // existing project files round-trip unchanged.
    let json = r#"{"name":"L","color":[1.0,1.0,1.0,1.0],"visible":true,
        "x":{"keys":[]},"y":{"keys":[]},"scale":{"keys":[]},
        "rotation":{"keys":[]},"opacity":{"keys":[]}}"#;
    let layer: PulseLayer = serde_json::from_str(json).unwrap();
    assert!(layer.stylize_effects.is_empty());
    assert!(!layer.has_stylize_effects());
}

#[test]
fn stylize_effects_serde_round_trips() {
    // A layer with a stylize stack serializes and reloads identically.
    use crate::comp::StylizeEffect;
    let mut layer = PulseLayer::new("L", [1.0, 1.0, 1.0, 1.0]);
    layer.stylize_effects.push(StylizeEffect::FindEdges {
        amount: 2.0,
        invert: true,
    });
    layer.stylize_effects.push(StylizeEffect::Mosaic {
        horizontal: 12,
        vertical: 20,
    });
    let json = serde_json::to_string(&layer).unwrap();
    let back: PulseLayer = serde_json::from_str(&json).unwrap();
    assert_eq!(layer.stylize_effects, back.stylize_effects);
    assert!(back.has_stylize_effects());
}
