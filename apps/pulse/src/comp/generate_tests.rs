//! Unit tests for [`GenerateEffect`](super::GenerateEffect) and the deterministic
//! noise/cellular math in [`generate_math`](super::super::generate_math).
//!
//! Included from `generate.rs` via `#[path]` so the generator definitions and their
//! tests live in separate files (workspace size rule) while staying one logical
//! module — `super` here is the `generate` module, exactly as when the tests were
//! inline.

    use super::*;

    /// A default Fractal Noise for tweaking in tests.
    fn fractal() -> GenerateEffect {
        GenerateEffect::defaults()[0]
    }

    /// Replace the named fields of a generate effect (terse test helper).
    fn with(mut e: GenerateEffect, f: impl FnOnce(&mut GenerateEffect)) -> GenerateEffect {
        f(&mut e);
        e
    }

    fn approx(a: [f32; 4], b: [f32; 4], eps: f32) -> bool {
        a.iter().zip(b.iter()).all(|(x, y)| (x - y).abs() <= eps)
    }

    #[test]
    fn labels_and_defaults() {
        let d = GenerateEffect::defaults();
        assert_eq!(d.len(), 6);
        assert_eq!(d[0].label(), "Fractal Noise");
        assert_eq!(d[1].label(), "Gradient Ramp");
        assert_eq!(d[2].label(), "Checkerboard");
        assert_eq!(d[3].label(), "4-Color Gradient");
        assert_eq!(d[4].label(), "Grid");
        assert_eq!(d[5].label(), "Cell Pattern");
    }

    #[test]
    fn produces_color_only_for_color_generators() {
        let d = GenerateEffect::defaults();
        // Fractal Noise + Cell Pattern are grayscale-linear; the rest are colour.
        assert!(!d[0].produces_color(), "fractal noise is grayscale-linear");
        assert!(!d[5].produces_color(), "cell pattern is grayscale-linear");
        for e in &d[1..5] {
            assert!(e.produces_color(), "{} is a colour generator", e.label());
        }
    }

    // --- Fractal Noise ------------------------------------------------------

    #[test]
    fn noise_is_deterministic_across_calls() {
        // Same (params, pixel) → same value, every call. This is the whole point:
        // a frame must render identically for the cache / multi-frame render.
        let e = fractal();
        for &(x, y) in &[(0.0, 0.0), (13.0, -7.0), (200.0, 130.0), (-50.5, 88.25)] {
            let a = e.value_at(x, y);
            let b = e.value_at(x, y);
            assert_eq!(a, b, "noise must be deterministic at ({x},{y})");
        }
    }

    #[test]
    fn gradient_noise_is_deterministic_and_in_range() {
        for &(x, y, z) in &[(0.3, 0.7, 0.0), (10.1, -3.4, 2.2), (-100.0, 50.0, 9.9)] {
            let a = gradient_noise_3d(x, y, z, 0);
            let b = gradient_noise_3d(x, y, z, 0);
            assert_eq!(a, b, "gradient noise must be deterministic");
            assert!(a.abs() <= 1.5, "gradient noise roughly bounded, got {a}");
        }
    }

    #[test]
    fn value_is_in_unit_range_when_clipped() {
        let e = fractal();
        for i in 0..200 {
            let x = (i as f32) * 3.7 - 100.0;
            let y = (i as f32) * -2.1 + 40.0;
            let v = e.value_at(x, y);
            assert!((0.0..=1.0).contains(&v), "clipped value out of range: {v}");
        }
    }

    #[test]
    fn evolution_changes_the_field() {
        // Sweeping evolution must move the field — at least one sampled pixel
        // changes meaningfully (the key motion-design knob).
        let a = fractal();
        let b = with(fractal(), |e| {
            if let GenerateEffect::FractalNoise { evolution, .. } = e {
                *evolution = 5.0;
            }
        });
        let mut max_diff = 0.0f32;
        for i in 0..64 {
            let x = i as f32 * 5.0;
            let y = i as f32 * 3.0;
            max_diff = max_diff.max((a.value_at(x, y) - b.value_at(x, y)).abs());
        }
        assert!(max_diff > 0.05, "evolution should change the field, max diff {max_diff}");
    }

    #[test]
    fn seed_changes_the_field() {
        let a = fractal();
        let b = with(fractal(), |e| {
            if let GenerateEffect::FractalNoise { seed, .. } = e {
                *seed = 12345;
            }
        });
        let mut max_diff = 0.0f32;
        for i in 0..64 {
            let x = i as f32 * 5.0;
            let y = i as f32 * 3.0;
            max_diff = max_diff.max((a.value_at(x, y) - b.value_at(x, y)).abs());
        }
        assert!(max_diff > 0.05, "seed should change the field, max diff {max_diff}");
    }

    #[test]
    fn turbulent_differs_from_basic() {
        // Same seed/scale/evolution, just the fractal type flipped, must give a
        // visibly different field (abs-sum vs signed-sum).
        let basic = with(fractal(), |e| {
            if let GenerateEffect::FractalNoise { fractal_type, .. } = e {
                *fractal_type = FractalType::Basic;
            }
        });
        let turb = with(fractal(), |e| {
            if let GenerateEffect::FractalNoise { fractal_type, .. } = e {
                *fractal_type = FractalType::Turbulent;
            }
        });
        let mut max_diff = 0.0f32;
        for i in 0..64 {
            let x = i as f32 * 4.0 + 1.0;
            let y = i as f32 * 2.0 - 3.0;
            max_diff = max_diff.max((basic.value_at(x, y) - turb.value_at(x, y)).abs());
        }
        assert!(max_diff > 0.05, "turbulent should differ from basic, max diff {max_diff}");
    }

    #[test]
    fn turbulent_is_nonnegative_before_contrast() {
        // The raw turbulent fbm is an abs-sum, so it is ≥ 0. Sample fbm directly
        // (value_at adds contrast/brightness which could push it negative).
        for i in 0..50 {
            let x = i as f32 * 0.37;
            let y = i as f32 * -0.21;
            let n = fbm(x, y, 0.0, 0, 6, 0.6, 2.0, FractalType::Turbulent);
            assert!(n >= 0.0, "turbulent fbm must be non-negative, got {n}");
        }
    }

    #[test]
    fn complexity_adds_detail() {
        // More octaves should change the field (finer detail), not be a no-op.
        let low = with(fractal(), |e| {
            if let GenerateEffect::FractalNoise { complexity, .. } = e {
                *complexity = 1;
            }
        });
        let high = with(fractal(), |e| {
            if let GenerateEffect::FractalNoise { complexity, .. } = e {
                *complexity = 8;
            }
        });
        let mut max_diff = 0.0f32;
        for i in 0..64 {
            let x = i as f32 * 2.5;
            let y = i as f32 * 1.5;
            max_diff = max_diff.max((low.value_at(x, y) - high.value_at(x, y)).abs());
        }
        assert!(max_diff > 0.02, "complexity should add detail, max diff {max_diff}");
    }

    #[test]
    fn single_octave_ignores_persistence_and_scaling() {
        // With one octave there is nothing for persistence/lacunarity to act on,
        // so they must not change the result.
        let base = with(fractal(), |e| {
            if let GenerateEffect::FractalNoise { complexity, .. } = e {
                *complexity = 1;
            }
        });
        let tweaked = with(base, |e| {
            if let GenerateEffect::FractalNoise {
                sub_influence,
                sub_scaling,
                ..
            } = e
            {
                *sub_influence = 0.1;
                *sub_scaling = 4.0;
            }
        });
        for i in 0..32 {
            let x = i as f32 * 6.0;
            let y = i as f32 * 4.0;
            assert!(
                (base.value_at(x, y) - tweaked.value_at(x, y)).abs() < 1e-5,
                "one octave should ignore sub-influence/scaling"
            );
        }
    }

    #[test]
    fn contrast_pushes_away_from_mid_grey() {
        // High contrast pushes values away from 0.5; sample a pixel that isn't
        // exactly mid-grey and confirm the deviation grows.
        let flat = with(fractal(), |e| {
            if let GenerateEffect::FractalNoise {
                contrast,
                overflow,
                ..
            } = e
            {
                *contrast = 1.0;
                *overflow = Overflow::AllowHdr; // don't clip so we can see the push
            }
        });
        let punchy = with(flat, |e| {
            if let GenerateEffect::FractalNoise { contrast, .. } = e {
                *contrast = 3.0;
            }
        });
        // Find a pixel whose flat value is clearly off mid-grey.
        let (mut fx, mut fy) = (0.0f32, 0.0f32);
        let mut found = false;
        for i in 0..200 {
            let x = i as f32 * 3.3;
            let y = i as f32 * 1.7;
            if (flat.value_at(x, y) - 0.5).abs() > 0.05 {
                fx = x;
                fy = y;
                found = true;
                break;
            }
        }
        assert!(found, "expected an off-mid-grey pixel");
        let flat_dev = (flat.value_at(fx, fy) - 0.5).abs();
        let punchy_dev = (punchy.value_at(fx, fy) - 0.5).abs();
        assert!(
            punchy_dev > flat_dev,
            "higher contrast should push further from mid-grey ({punchy_dev} vs {flat_dev})"
        );
    }

    #[test]
    fn brightness_lifts_the_field() {
        let dark = with(fractal(), |e| {
            if let GenerateEffect::FractalNoise {
                brightness,
                overflow,
                ..
            } = e
            {
                *brightness = 0.0;
                *overflow = Overflow::AllowHdr;
            }
        });
        let bright = with(dark, |e| {
            if let GenerateEffect::FractalNoise { brightness, .. } = e {
                *brightness = 0.3;
            }
        });
        for i in 0..32 {
            let x = i as f32 * 5.0;
            let y = i as f32 * 3.0;
            assert!(
                (bright.value_at(x, y) - dark.value_at(x, y) - 0.3).abs() < 1e-4,
                "brightness should lift the field by its offset"
            );
        }
    }

    #[test]
    fn overflow_modes_bring_value_into_range() {
        assert_eq!(Overflow::Clip.apply(1.5), 1.0);
        assert_eq!(Overflow::Clip.apply(-0.3), 0.0);
        assert_eq!(Overflow::Clip.apply(0.4), 0.4);
        // Wrap takes the fractional part.
        assert!((Overflow::Wrap.apply(1.25) - 0.25).abs() < 1e-6);
        assert!((Overflow::Wrap.apply(-0.25) - 0.75).abs() < 1e-6);
        // AllowHdr keeps values above 1 but floors at 0.
        assert_eq!(Overflow::AllowHdr.apply(2.0), 2.0);
        assert_eq!(Overflow::AllowHdr.apply(-1.0), 0.0);
    }

    #[test]
    fn scale_changes_feature_size() {
        // A different scale samples the field at a different frequency, so the
        // value at a fixed pixel changes.
        let small = with(fractal(), |e| {
            if let GenerateEffect::FractalNoise { scale, .. } = e {
                *scale = 20.0;
            }
        });
        let large = with(fractal(), |e| {
            if let GenerateEffect::FractalNoise { scale, .. } = e {
                *scale = 200.0;
            }
        });
        let mut max_diff = 0.0f32;
        for i in 0..64 {
            let x = i as f32 * 4.0;
            let y = i as f32 * 4.0;
            max_diff = max_diff.max((small.value_at(x, y) - large.value_at(x, y)).abs());
        }
        assert!(max_diff > 0.05, "scale should change feature size, max diff {max_diff}");
    }

    #[test]
    fn zero_scale_does_not_panic() {
        // A degenerate zero scale must be guarded (no div-by-zero / NaN).
        let e = with(fractal(), |e| {
            if let GenerateEffect::FractalNoise {
                scale,
                scale_x,
                scale_y,
                ..
            } = e
            {
                *scale = 0.0;
                *scale_x = 0.0;
                *scale_y = 0.0;
            }
        });
        let v = e.value_at(10.0, 20.0);
        assert!(v.is_finite(), "zero scale must not produce NaN/inf");
    }

    #[test]
    fn opacity_is_clamped() {
        for d in GenerateEffect::defaults() {
            let e = with(d, |x| match x {
                GenerateEffect::FractalNoise { opacity, .. }
                | GenerateEffect::Ramp { opacity, .. }
                | GenerateEffect::Checkerboard { opacity, .. }
                | GenerateEffect::FourColorGradient { opacity, .. }
                | GenerateEffect::Grid { opacity, .. }
                | GenerateEffect::CellPattern { opacity, .. } => *opacity = 2.0,
            });
            assert_eq!(e.opacity(), 1.0, "{} opacity clamps", e.label());
        }
    }

    #[test]
    fn serde_round_trips_every_generator() {
        for e in GenerateEffect::defaults() {
            let json = serde_json::to_string(&e).unwrap();
            let back: GenerateEffect = serde_json::from_str(&json).unwrap();
            assert_eq!(e, back, "{} serde round-trip", e.label());
        }
    }

    // --- Gradient / Ramp ----------------------------------------------------

    /// A linear ramp from black at y=-100 to white at y=+100 (vertical).
    fn linear_ramp() -> GenerateEffect {
        GenerateEffect::Ramp {
            shape: RampShape::Linear,
            start: [0.0, -100.0],
            end: [0.0, 100.0],
            radius: 100.0,
            start_color: [0.0, 0.0, 0.0],
            end_color: [1.0, 1.0, 1.0],
            scatter: 0.0,
            opacity: 1.0,
        }
    }

    #[test]
    fn linear_ramp_endpoints_and_midpoint() {
        let r = linear_ramp();
        // At the start point: start_color (black).
        assert!(approx(r.rgba_at(0.0, -100.0, 200.0, 200.0), [0.0, 0.0, 0.0, 1.0], 1e-4));
        // At the end point: end_color (white).
        assert!(approx(r.rgba_at(0.0, 100.0, 200.0, 200.0), [1.0, 1.0, 1.0, 1.0], 1e-4));
        // Midpoint: mid-grey.
        let mid = r.rgba_at(0.0, 0.0, 200.0, 200.0);
        assert!(approx(mid, [0.5, 0.5, 0.5, 1.0], 1e-4), "midpoint grey, got {mid:?}");
    }

    #[test]
    fn linear_ramp_clamps_past_the_endpoints() {
        let r = linear_ramp();
        // Past the white end stays white (clamped, not extrapolated).
        assert!(approx(r.rgba_at(0.0, 500.0, 200.0, 200.0), [1.0, 1.0, 1.0, 1.0], 1e-4));
        // Before the black end stays black.
        assert!(approx(r.rgba_at(0.0, -500.0, 200.0, 200.0), [0.0, 0.0, 0.0, 1.0], 1e-4));
    }

    #[test]
    fn linear_ramp_is_constant_perpendicular_to_axis() {
        // A vertical ramp is constant along x.
        let r = linear_ramp();
        let a = r.rgba_at(-80.0, 0.0, 200.0, 200.0);
        let b = r.rgba_at(80.0, 0.0, 200.0, 200.0);
        assert!(approx(a, b, 1e-5), "constant across the perpendicular axis");
    }

    #[test]
    fn radial_ramp_centre_and_edge() {
        let r = GenerateEffect::Ramp {
            shape: RampShape::Radial,
            start: [0.0, 0.0],
            end: [0.0, 0.0],
            radius: 100.0,
            start_color: [0.0, 0.0, 0.0],
            end_color: [1.0, 1.0, 1.0],
            scatter: 0.0,
            opacity: 1.0,
        };
        // Centre = start_color.
        assert!(approx(r.rgba_at(0.0, 0.0, 200.0, 200.0), [0.0, 0.0, 0.0, 1.0], 1e-4));
        // At the radius (along +x) = end_color.
        assert!(approx(r.rgba_at(100.0, 0.0, 200.0, 200.0), [1.0, 1.0, 1.0, 1.0], 1e-4));
        // Halfway out = mid-grey, and isotropic (same in any direction).
        let half_x = r.rgba_at(50.0, 0.0, 200.0, 200.0);
        let half_y = r.rgba_at(0.0, 50.0, 200.0, 200.0);
        assert!(approx(half_x, [0.5, 0.5, 0.5, 1.0], 1e-4), "radial midpoint grey");
        assert!(approx(half_x, half_y, 1e-5), "radial ramp is isotropic");
    }

    #[test]
    fn degenerate_linear_ramp_does_not_nan() {
        // start == end → zero-length axis, must not divide by zero.
        let r = GenerateEffect::Ramp {
            shape: RampShape::Linear,
            start: [10.0, 10.0],
            end: [10.0, 10.0],
            radius: 100.0,
            start_color: [0.2, 0.4, 0.6],
            end_color: [0.8, 0.6, 0.4],
            scatter: 0.0,
            opacity: 1.0,
        };
        let v = r.rgba_at(50.0, 50.0, 200.0, 200.0);
        assert!(v.iter().all(|c| c.is_finite()), "degenerate ramp finite, got {v:?}");
    }

    #[test]
    fn ramp_scatter_dithers_deterministically() {
        let mut r = linear_ramp();
        if let GenerateEffect::Ramp { scatter, .. } = &mut r {
            *scatter = 0.4;
        }
        // Deterministic: the same pixel always gives the same dithered value.
        let a = r.rgba_at(13.0, 7.0, 200.0, 200.0);
        let b = r.rgba_at(13.0, 7.0, 200.0, 200.0);
        assert_eq!(a, b, "scatter must be deterministic per pixel");
        // And it actually perturbs vs the clean ramp at some pixels.
        let clean = linear_ramp();
        let mut diff = 0.0f32;
        for i in 0..64 {
            let x = i as f32 * 3.0;
            let y = i as f32 * 2.0 - 50.0;
            diff = diff.max((r.rgba_at(x, y, 200.0, 200.0)[1] - clean.rgba_at(x, y, 200.0, 200.0)[1]).abs());
        }
        assert!(diff > 0.01, "scatter should perturb the ramp, max diff {diff}");
    }

    // --- Checkerboard -------------------------------------------------------

    fn checker() -> GenerateEffect {
        GenerateEffect::Checkerboard {
            anchor: [0.0, 0.0],
            size_w: 50.0,
            size_h: 50.0,
            color1: [0.0, 0.0, 0.0],
            color2: [1.0, 1.0, 1.0],
            opacity: 1.0,
        }
    }

    #[test]
    fn checkerboard_cell_parity() {
        let c = checker();
        // Cell (0,0): even parity → color1 (black). Sample its interior.
        assert!(approx(c.rgba_at(25.0, 25.0, 200.0, 200.0), [0.0, 0.0, 0.0, 1.0], 1e-5));
        // Cell (1,0): odd parity → color2 (white).
        assert!(approx(c.rgba_at(75.0, 25.0, 200.0, 200.0), [1.0, 1.0, 1.0, 1.0], 1e-5));
        // Cell (0,1): odd parity → color2 (white).
        assert!(approx(c.rgba_at(25.0, 75.0, 200.0, 200.0), [1.0, 1.0, 1.0, 1.0], 1e-5));
        // Cell (1,1): even parity → color1 (black).
        assert!(approx(c.rgba_at(75.0, 75.0, 200.0, 200.0), [0.0, 0.0, 0.0, 1.0], 1e-5));
    }

    #[test]
    fn checkerboard_negative_cells_keep_parity() {
        // rem_euclid keeps the chequer continuous across the origin.
        let c = checker();
        // Cell (-1,0): odd → white.
        assert!(approx(c.rgba_at(-25.0, 25.0, 200.0, 200.0), [1.0, 1.0, 1.0, 1.0], 1e-5));
        // Cell (-1,-1): even → black.
        assert!(approx(c.rgba_at(-25.0, -25.0, 200.0, 200.0), [0.0, 0.0, 0.0, 1.0], 1e-5));
    }

    #[test]
    fn checkerboard_anchor_shifts_the_grid() {
        let mut c = checker();
        if let GenerateEffect::Checkerboard { anchor, .. } = &mut c {
            *anchor = [50.0, 0.0]; // shift one cell right
        }
        // The pixel that was cell (0,0) black is now cell (-1,0) odd → white.
        assert!(approx(c.rgba_at(25.0, 25.0, 200.0, 200.0), [1.0, 1.0, 1.0, 1.0], 1e-5));
    }

    #[test]
    fn checkerboard_zero_size_does_not_panic() {
        let c = GenerateEffect::Checkerboard {
            anchor: [0.0, 0.0],
            size_w: 0.0,
            size_h: 0.0,
            color1: [0.2, 0.2, 0.2],
            color2: [0.8, 0.8, 0.8],
            opacity: 1.0,
        };
        let v = c.rgba_at(10.0, 20.0, 200.0, 200.0);
        assert!(v.iter().all(|x| x.is_finite()));
    }

    // --- 4-Color Gradient ---------------------------------------------------

    fn four_color() -> GenerateEffect {
        GenerateEffect::FourColorGradient {
            tl: [1.0, 0.0, 0.0],
            tr: [0.0, 1.0, 0.0],
            bl: [0.0, 0.0, 1.0],
            br: [1.0, 1.0, 0.0],
            blend: 1.0,
            jitter: 0.0,
            opacity: 1.0,
        }
    }

    #[test]
    fn four_color_corner_values() {
        let g = four_color();
        let (hw, hh) = (100.0, 100.0);
        // Top-left corner (lx=-hw, ly=-hh) → tl (red).
        assert!(approx(g.rgba_at(-hw, -hh, hw, hh), [1.0, 0.0, 0.0, 1.0], 1e-4));
        // Top-right (lx=+hw, ly=-hh) → tr (green).
        assert!(approx(g.rgba_at(hw, -hh, hw, hh), [0.0, 1.0, 0.0, 1.0], 1e-4));
        // Bottom-left (lx=-hw, ly=+hh) → bl (blue).
        assert!(approx(g.rgba_at(-hw, hh, hw, hh), [0.0, 0.0, 1.0, 1.0], 1e-4));
        // Bottom-right (lx=+hw, ly=+hh) → br (yellow).
        assert!(approx(g.rgba_at(hw, hh, hw, hh), [1.0, 1.0, 0.0, 1.0], 1e-4));
    }

    #[test]
    fn four_color_interior_blend() {
        let g = four_color();
        let (hw, hh) = (100.0, 100.0);
        // Centre = average of the four corners.
        let c = g.rgba_at(0.0, 0.0, hw, hh);
        let avg = [
            (1.0 + 0.0 + 0.0 + 1.0) / 4.0,
            (0.0 + 1.0 + 0.0 + 1.0) / 4.0,
            (0.0 + 0.0 + 1.0 + 0.0) / 4.0,
        ];
        assert!(approx(c, [avg[0], avg[1], avg[2], 1.0], 1e-4), "centre is the average, got {c:?}");
        // Top edge midpoint = average of tl & tr.
        let top = g.rgba_at(0.0, -hh, hw, hh);
        assert!(approx(top, [0.5, 0.5, 0.0, 1.0], 1e-4), "top edge blends tl/tr, got {top:?}");
    }

    #[test]
    fn four_color_jitter_is_deterministic_and_perturbs() {
        let mut g = four_color();
        if let GenerateEffect::FourColorGradient { jitter, .. } = &mut g {
            *jitter = 0.3;
        }
        let a = g.rgba_at(11.0, 23.0, 100.0, 100.0);
        let b = g.rgba_at(11.0, 23.0, 100.0, 100.0);
        assert_eq!(a, b, "jitter deterministic per pixel");
        let clean = four_color();
        let mut diff = 0.0f32;
        for i in 0..64 {
            let x = i as f32 * 2.0 - 60.0;
            let y = i as f32 * 1.5 - 40.0;
            diff = diff.max((g.rgba_at(x, y, 100.0, 100.0)[0] - clean.rgba_at(x, y, 100.0, 100.0)[0]).abs());
        }
        assert!(diff > 0.005, "jitter should perturb the blend, max diff {diff}");
    }

    // --- Grid ---------------------------------------------------------------

    fn grid() -> GenerateEffect {
        GenerateEffect::Grid {
            anchor: [0.0, 0.0],
            size_w: 50.0,
            size_h: 50.0,
            border: 4.0,
            color: [1.0, 1.0, 1.0],
            background: [0.0, 0.0, 0.0],
            background_opacity: 0.0,
            opacity: 1.0,
        }
    }

    #[test]
    fn grid_line_vs_cell_pixels() {
        let g = grid();
        // On a vertical line (x near a multiple of 50): opaque white line.
        let on = g.rgba_at(0.0, 25.0, 200.0, 200.0);
        assert!(approx(on, [1.0, 1.0, 1.0, 1.0], 1e-5), "on a grid line, got {on:?}");
        // Cell interior (far from any line): transparent background.
        let off = g.rgba_at(25.0, 25.0, 200.0, 200.0);
        assert_eq!(off[3], 0.0, "cell interior is transparent");
    }

    #[test]
    fn grid_horizontal_and_corner_lines() {
        let g = grid();
        // On a horizontal line.
        assert!(approx(g.rgba_at(25.0, 50.0, 200.0, 200.0), [1.0, 1.0, 1.0, 1.0], 1e-5));
        // On a grid intersection (both lines).
        assert!(approx(g.rgba_at(0.0, 0.0, 200.0, 200.0), [1.0, 1.0, 1.0, 1.0], 1e-5));
    }

    #[test]
    fn grid_filled_background_is_opaque() {
        let mut g = grid();
        if let GenerateEffect::Grid {
            background_opacity, ..
        } = &mut g
        {
            *background_opacity = 1.0;
        }
        let off = g.rgba_at(25.0, 25.0, 200.0, 200.0);
        assert_eq!(off[3], 1.0, "filled background is opaque");
        assert!(approx(off, [0.0, 0.0, 0.0, 1.0], 1e-5));
    }

    #[test]
    fn grid_thicker_border_covers_more() {
        let thin = grid();
        let thick = with(grid(), |e| {
            if let GenerateEffect::Grid { border, .. } = e {
                *border = 20.0;
            }
        });
        // A pixel 8 px from a line: off for the thin border, on for the thick.
        assert_eq!(thin.rgba_at(8.0, 25.0, 200.0, 200.0)[3], 0.0);
        assert_eq!(thick.rgba_at(8.0, 25.0, 200.0, 200.0)[3], 1.0);
    }

    #[test]
    fn grid_zero_size_does_not_panic() {
        let g = GenerateEffect::Grid {
            anchor: [0.0, 0.0],
            size_w: 0.0,
            size_h: 0.0,
            border: 2.0,
            color: [1.0, 1.0, 1.0],
            background: [0.0, 0.0, 0.0],
            background_opacity: 0.0,
            opacity: 1.0,
        };
        let v = g.rgba_at(10.0, 20.0, 200.0, 200.0);
        assert!(v.iter().all(|x| x.is_finite()));
    }

    #[test]
    fn bias_is_identity_at_one_and_fixes_ends() {
        assert!((bias(0.3, 1.0) - 0.3).abs() < 1e-6);
        assert!((bias(0.0, 2.0) - 0.0).abs() < 1e-6);
        assert!((bias(1.0, 2.0) - 1.0).abs() < 1e-6);
        assert!((bias(0.5, 2.0) - 0.5).abs() < 1e-6, "0.5 is a fixed point");
        // Sharper (>1) pushes a below-mid value lower.
        assert!(bias(0.3, 2.0) < 0.3);
    }

    #[test]
    fn color_generators_are_deterministic() {
        for e in &GenerateEffect::defaults()[1..] {
            // Cell Pattern is grayscale-linear (not a colour generator), so skip it
            // here — its determinism is covered by `cell_pattern_is_deterministic`.
            if !e.produces_color() {
                continue;
            }
            for &(x, y) in &[(0.0, 0.0), (33.0, -17.0), (-90.0, 120.0)] {
                let a = e.rgba_at(x, y, 100.0, 100.0);
                let b = e.rgba_at(x, y, 100.0, 100.0);
                assert_eq!(a, b, "{} must be deterministic", e.label());
            }
        }
    }

    // --- Cell Pattern -------------------------------------------------------

    /// A default Cell Pattern (the Bubbles type) for tweaking in tests.
    fn cells() -> GenerateEffect {
        GenerateEffect::defaults()[5]
    }

    /// Replace the `cell_type` of a Cell Pattern (terse test helper).
    fn with_type(e: GenerateEffect, ct: CellType) -> GenerateEffect {
        with(e, |x| {
            if let GenerateEffect::CellPattern { cell_type, .. } = x {
                *cell_type = ct;
            }
        })
    }

    #[test]
    fn cell_pattern_is_grayscale_not_colour() {
        // Cell Pattern is the sixth generator and, like Fractal Noise, is
        // grayscale-linear (not an sRGB colour generator).
        let d = GenerateEffect::defaults();
        assert_eq!(d.len(), 6);
        assert_eq!(d[5].label(), "Cell Pattern");
        assert!(!d[5].produces_color(), "cell pattern is grayscale-linear");
        // rgba_at returns value in all of R/G/B and A (a straight grayscale fill).
        let [r, g, b, a] = d[5].rgba_at(13.0, 7.0, 100.0, 100.0);
        assert_eq!(r, g, "grayscale: R == G");
        assert_eq!(g, b, "grayscale: G == B");
        assert_eq!(a, r, "alpha carries the value too");
    }

    #[test]
    fn cell_pattern_is_deterministic() {
        // Same (params, pixel) → same value, every call (the cache / render need
        // this). Checked across every cell type.
        for ct in CellType::ALL {
            let e = with_type(cells(), ct);
            for &(x, y) in &[(0.0, 0.0), (13.0, -7.0), (200.0, 130.0), (-50.5, 88.25)] {
                let a = e.value_at(x, y);
                let b = e.value_at(x, y);
                assert_eq!(a, b, "{} must be deterministic at ({x},{y})", ct.label());
            }
        }
    }

    #[test]
    fn cell_pattern_value_in_range() {
        // Every cell type lands in [0,1] across the frame (clipped output).
        for ct in CellType::ALL {
            let e = with_type(cells(), ct);
            for i in 0..200 {
                let x = (i as f32) * 3.7 - 100.0;
                let y = (i as f32) * -2.1 + 40.0;
                let v = e.value_at(x, y);
                assert!(
                    (0.0..=1.0).contains(&v),
                    "{} value out of range: {v}",
                    ct.label()
                );
            }
        }
    }

    #[test]
    fn cell_pattern_seed_changes_the_layout() {
        // A different seed re-rolls the feature points → a different field.
        let a = cells();
        let b = with(cells(), |e| {
            if let GenerateEffect::CellPattern { seed, .. } = e {
                *seed = 12345;
            }
        });
        let mut max_diff = 0.0f32;
        for i in 0..64 {
            let x = i as f32 * 5.0;
            let y = i as f32 * 3.0;
            max_diff = max_diff.max((a.value_at(x, y) - b.value_at(x, y)).abs());
        }
        assert!(max_diff > 0.05, "seed should change the layout, max diff {max_diff}");
    }

    #[test]
    fn cell_pattern_evolution_flows_the_field() {
        // Sweeping evolution must move the field — the keyframable motion knob.
        let a = cells();
        let b = with(cells(), |e| {
            if let GenerateEffect::CellPattern { evolution, .. } = e {
                *evolution = 5.0;
            }
        });
        let mut max_diff = 0.0f32;
        for i in 0..64 {
            let x = i as f32 * 5.0;
            let y = i as f32 * 3.0;
            max_diff = max_diff.max((a.value_at(x, y) - b.value_at(x, y)).abs());
        }
        assert!(max_diff > 0.05, "evolution should flow the field, max diff {max_diff}");
    }

    #[test]
    fn cell_pattern_f1_is_zero_at_a_feature_point() {
        // Crystals output the raw F1 distance. With no disorder the feature points
        // sit at cell centres (size px apart, offset half a cell): at a centre F1 is
        // 0 and the value is 0, rising as we move away from it.
        let e = with(with_type(cells(), CellType::Crystals), |x| {
            if let GenerateEffect::CellPattern { size, disorder, .. } = x {
                *size = 100.0;
                *disorder = 0.0;
            }
        });
        // Cell (0,0)'s feature point with no disorder is at local (0.5,0.5) cells =
        // (50,50) px. F1 there is ~0, so the Crystals value is ~0.
        let at_point = e.value_at(50.0, 50.0);
        assert!(at_point < 1e-3, "F1 ~0 at a feature point, got {at_point}");
        // A little away from the point F1 grows, so the value rises.
        let near = e.value_at(50.0 + 25.0, 50.0);
        assert!(near > at_point + 0.1, "F1 grows away from the point ({near} vs {at_point})");
    }

    #[test]
    fn cell_pattern_borders_high_at_boundary_low_in_interior() {
        // Borders is the cell-web: ~0 deep inside a cell, high along the boundary
        // between two cells (where F1 ≈ F2). With no disorder on a 100 px grid the
        // feature points are at cell centres (…, 50, 150, …); the midline x=100 is
        // equidistant from the (0,0) and (1,0) points, i.e. on a boundary.
        let e = with(with_type(cells(), CellType::Borders), |x| {
            if let GenerateEffect::CellPattern { size, disorder, .. } = x {
                *size = 100.0;
                *disorder = 0.0;
            }
        });
        let on_boundary = e.value_at(100.0, 50.0); // halfway between two points
        let in_interior = e.value_at(50.0, 50.0); // right at a feature point
        assert!(
            on_boundary > 0.9,
            "borders bright on the boundary, got {on_boundary}"
        );
        assert!(
            in_interior < 0.5,
            "borders dark in the cell interior, got {in_interior}"
        );
        assert!(
            on_boundary > in_interior + 0.4,
            "boundary clearly brighter than interior ({on_boundary} vs {in_interior})"
        );
    }

    #[test]
    fn cell_pattern_invert_is_symmetric() {
        // Inverting flips the value about the clip range: at pixels where the plain
        // value isn't clamped, inverted == 1 − plain. Use Crystals so the value is
        // well inside (0,1) for many pixels.
        let plain = with(with_type(cells(), CellType::Crystals), |x| {
            if let GenerateEffect::CellPattern { invert, .. } = x {
                *invert = false;
            }
        });
        let inv = with(plain, |x| {
            if let GenerateEffect::CellPattern { invert, .. } = x {
                *invert = true;
            }
        });
        let mut checked = 0;
        for i in 0..200 {
            let x = i as f32 * 3.3;
            let y = i as f32 * 1.7;
            let p = plain.value_at(x, y);
            // Only assert where the plain value is strictly inside the range (so
            // neither side hit the clamp and broke the 1−p symmetry).
            if (0.02..=0.98).contains(&p) {
                assert!(
                    (inv.value_at(x, y) - (1.0 - p)).abs() < 1e-4,
                    "invert should mirror the value about 0.5"
                );
                checked += 1;
            }
        }
        assert!(checked > 10, "expected some mid-range pixels to check, got {checked}");
    }

    #[test]
    fn cell_pattern_types_differ() {
        // Each cell type shapes the same feature points differently, so the fields
        // are visibly distinct (pairwise).
        for (a, b) in [
            (CellType::Bubbles, CellType::Crystals),
            (CellType::Crystals, CellType::Borders),
            (CellType::Plates, CellType::Bubbles),
        ] {
            let ea = with_type(cells(), a);
            let eb = with_type(cells(), b);
            let mut max_diff = 0.0f32;
            for i in 0..64 {
                let x = i as f32 * 4.0 + 1.0;
                let y = i as f32 * 2.0 - 3.0;
                max_diff = max_diff.max((ea.value_at(x, y) - eb.value_at(x, y)).abs());
            }
            assert!(
                max_diff > 0.05,
                "{} should differ from {}, max diff {max_diff}",
                a.label(),
                b.label()
            );
        }
    }

    #[test]
    fn cell_pattern_static_plates_ignore_evolution() {
        // Static Plates' per-cell tone is independent of evolution (the steadier
        // plate field), so sweeping evolution leaves the value unchanged.
        let a = with_type(cells(), CellType::StaticPlates);
        let b = with(a, |e| {
            if let GenerateEffect::CellPattern { evolution, .. } = e {
                *evolution = 7.0;
            }
        });
        for i in 0..32 {
            let x = i as f32 * 6.0;
            let y = i as f32 * 4.0;
            assert_eq!(
                a.value_at(x, y),
                b.value_at(x, y),
                "static plates ignore evolution"
            );
        }
    }

    #[test]
    fn cell_pattern_disorder_jitters_the_points() {
        // Adding disorder displaces the feature points, so the field changes vs the
        // regular (disorder = 0) grid.
        let ordered = with(with_type(cells(), CellType::Crystals), |x| {
            if let GenerateEffect::CellPattern { disorder, .. } = x {
                *disorder = 0.0;
            }
        });
        let messy = with(ordered, |x| {
            if let GenerateEffect::CellPattern { disorder, .. } = x {
                *disorder = 1.0;
            }
        });
        let mut max_diff = 0.0f32;
        for i in 0..64 {
            let x = i as f32 * 4.0;
            let y = i as f32 * 3.0;
            max_diff = max_diff.max((ordered.value_at(x, y) - messy.value_at(x, y)).abs());
        }
        assert!(max_diff > 0.02, "disorder should jitter the points, max diff {max_diff}");
    }

    #[test]
    fn cell_pattern_zero_size_does_not_panic() {
        // A degenerate zero size must be guarded (no div-by-zero / NaN).
        let e = with(cells(), |x| {
            if let GenerateEffect::CellPattern { size, .. } = x {
                *size = 0.0;
            }
        });
        let v = e.value_at(10.0, 20.0);
        assert!(v.is_finite(), "zero size must not produce NaN/inf");
    }
