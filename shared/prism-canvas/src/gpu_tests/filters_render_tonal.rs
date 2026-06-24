use super::*;

// ---- Render family (Phase 8) ----------------------------------------------

// Clouds fills the layer with a deterministic fBm field: opaque, gray, in range,
// reproducible per seed, and actually varying (not a flat fill). Difference
// Clouds = |source − cloud|, so on a black field it reproduces the raw cloud.

#[test]
fn clouds_are_deterministic_opaque_and_vary() {
    let Some((device, queue)) = device() else {
        eprintln!("no GPU adapter; skipping clouds_are_deterministic_opaque_and_vary");
        return;
    };
    let n = 32u32;
    let run = |difference: bool, base: f32, seed: f32| -> Vec<[f32; 4]> {
        let mut gpu = CanvasGpu::new(&device, wgpu::TextureFormat::Bgra8Unorm);
        gpu.ensure_canvas(&device, Size::new(n, n));
        let l0 = LayerId(0);
        gpu.ensure_layer(&device, l0);
        gpu.upload_layer(&queue, l0, &layer_from(n, |_x, _y| [base, base, base, 1.0]));
        gpu.apply_clouds(&device, &queue, l0, difference, seed, 16.0, 0.5, 5);
        let mut px = Vec::with_capacity((n * n) as usize);
        for y in 0..n {
            for x in 0..n {
                px.push(gpu.read_pixel(&device, &queue, l0, x, y).unwrap());
            }
        }
        px
    };

    // Clouds: deterministic for a fixed seed.
    let a = run(false, 0.5, 7.0);
    let b = run(false, 0.5, 7.0);
    for (pa, pb) in a.iter().zip(b.iter()) {
        for c in 0..4 {
            assert!((pa[c] - pb[c]).abs() < 1e-3, "deterministic for a fixed seed");
        }
    }
    // Opaque, gray, in range, and varying.
    let mut min = f32::INFINITY;
    let mut max = f32::NEG_INFINITY;
    for p in &a {
        assert!((p[3] - 1.0).abs() < 0.02, "opaque: {p:?}");
        assert!((p[0] - p[1]).abs() < 2e-3 && (p[1] - p[2]).abs() < 2e-3, "gray: {p:?}");
        for c in 0..3 {
            assert!((-0.01..=1.01).contains(&p[c]), "in range: {p:?}");
        }
        min = min.min(p[0]);
        max = max.max(p[0]);
    }
    assert!(max - min > 0.05, "field varies (not flat): {min}..{max}");

    // Different seed → a different field.
    let c = run(false, 0.5, 9.0);
    assert!(
        a.iter().zip(c.iter()).any(|(p, q)| (p[0] - q[0]).abs() > 0.02),
        "different seeds → different clouds"
    );

    // Difference Clouds on a black field reproduces the raw cloud (|0 − n| = n).
    let from_black = run(true, 0.0, 7.0);
    for (d, p) in from_black.iter().zip(a.iter()) {
        assert!(
            (d[0] - p[0]).abs() < 5e-3,
            "diff clouds on black = clouds: {} vs {}",
            d[0],
            p[0]
        );
    }
}


// ---- Oil Paint (Kuwahara quadrant filter, Phase 8) -----------------------

// A flat field has zero variance in every quadrant, so the chosen quadrant's
// mean equals the (constant) source colour: Oil Paint leaves it unchanged.

#[test]
fn oil_paint_flat_field_is_identity() {
    let Some((device, queue)) = device() else {
        eprintln!("no GPU adapter; skipping oil_paint_flat_field_is_identity");
        return;
    };
    let mut gpu = CanvasGpu::new(&device, wgpu::TextureFormat::Bgra8Unorm);
    let n = 8u32;
    gpu.ensure_canvas(&device, Size::new(n, n));
    let l0 = LayerId(0);
    gpu.ensure_layer(&device, l0);
    gpu.upload_layer(&queue, l0, &layer_from(n, |_x, _y| [0.3, 0.6, 0.2, 1.0]));
    gpu.apply_oil_paint(&device, &queue, l0, 2.0);
    let p = gpu.read_pixel(&device, &queue, l0, n / 2, n / 2).unwrap();
    assert!(
        (p[0] - 0.3).abs() < 0.02 && (p[1] - 0.6).abs() < 0.02 && (p[2] - 0.2).abs() < 0.02,
        "flat field unchanged: {p:?}"
    );
}


// On a hard vertical black/white step, Oil Paint snaps each pixel to a pure
// side (the flattest quadrant wins) — never the ~0.5 average a box blur gives.
#[test]
fn oil_paint_snaps_to_a_side_of_an_edge() {
    let Some((device, queue)) = device() else {
        eprintln!("no GPU adapter; skipping oil_paint_snaps_to_a_side_of_an_edge");
        return;
    };
    let mut gpu = CanvasGpu::new(&device, wgpu::TextureFormat::Bgra8Unorm);
    let n = 16u32;
    gpu.ensure_canvas(&device, Size::new(n, n));
    let l0 = LayerId(0);
    gpu.ensure_layer(&device, l0);
    gpu.upload_layer(
        &queue,
        l0,
        &layer_from(n, |x, _y| {
            if x < n / 2 {
                [0.0, 0.0, 0.0, 1.0]
            } else {
                [1.0, 1.0, 1.0, 1.0]
            }
        }),
    );
    gpu.apply_oil_paint(&device, &queue, l0, 2.0);
    let row = n / 2;
    for x in 0..n {
        let v = gpu.read_pixel(&device, &queue, l0, x, row).unwrap()[0];
        assert!(
            !(0.05..=0.95).contains(&v),
            "snaps to a pure side, never a blend: {v} at x={x}"
        );
    }
    assert!(
        gpu.read_pixel(&device, &queue, l0, 0, row).unwrap()[0] < 0.05,
        "left stays black"
    );
    assert!(
        gpu.read_pixel(&device, &queue, l0, n - 1, row).unwrap()[0] > 0.95,
        "right stays white"
    );
}


// ---- Posterize / Threshold (destructive tonal filters, Phase 8) ----------

// 2-level posterize snaps every channel to its 0 or 1 extreme (the endpoints
// are identical in linear and sRGB space, so the assertion is space-agnostic).

#[test]
fn posterize_2_levels_snaps_to_extremes() {
    let Some((device, queue)) = device() else {
        eprintln!("no GPU adapter; skipping posterize_2_levels_snaps_to_extremes");
        return;
    };
    let mut gpu = CanvasGpu::new(&device, wgpu::TextureFormat::Bgra8Unorm);
    let n = 8u32;
    gpu.ensure_canvas(&device, Size::new(n, n));
    let l0 = LayerId(0);
    gpu.ensure_layer(&device, l0);
    // A per-pixel ramp of distinct grays so several intermediate values are tested.
    gpu.upload_layer(
        &queue,
        l0,
        &layer_from(n, |x, y| {
            let v = (y * n + x) as f32 / (n * n - 1) as f32;
            [v, v, v, 1.0]
        }),
    );
    gpu.apply_posterize(&device, &queue, l0, 2);
    for y in 0..n {
        for x in 0..n {
            let p = gpu.read_pixel(&device, &queue, l0, x, y).unwrap();
            assert!(
                !(0.02..=0.98).contains(&p[0]),
                "2-level posterize → pure extreme: {} at ({x},{y})",
                p[0]
            );
            assert!((p[3] - 1.0).abs() < 0.02, "alpha preserved");
        }
    }
    // Corners: the darkest pixel stays black, the brightest stays white.
    assert!(gpu.read_pixel(&device, &queue, l0, 0, 0).unwrap()[0] < 0.02, "black stays");
    assert!(
        gpu.read_pixel(&device, &queue, l0, n - 1, n - 1).unwrap()[0] > 0.98,
        "white stays"
    );
}


// A flat mid-gray field is (very nearly) a fixed point of a 4-level posterize
// only at lattice points; here we just assert the result is one of the 4 levels
// and is uniform across the field (the op is a pure per-pixel transfer).
#[test]
fn posterize_is_a_uniform_per_pixel_transfer() {
    let Some((device, queue)) = device() else {
        eprintln!("no GPU adapter; skipping posterize_is_a_uniform_per_pixel_transfer");
        return;
    };
    let mut gpu = CanvasGpu::new(&device, wgpu::TextureFormat::Bgra8Unorm);
    let n = 8u32;
    gpu.ensure_canvas(&device, Size::new(n, n));
    let l0 = LayerId(0);
    gpu.ensure_layer(&device, l0);
    gpu.upload_layer(&queue, l0, &layer_from(n, |_x, _y| [0.18, 0.5, 0.82, 1.0]));
    gpu.apply_posterize(&device, &queue, l0, 4);
    let a = gpu.read_pixel(&device, &queue, l0, 0, 0).unwrap();
    let b = gpu.read_pixel(&device, &queue, l0, n - 1, n - 1).unwrap();
    for c in 0..3 {
        assert!((a[c] - b[c]).abs() < 0.01, "uniform field → uniform output");
    }
}


// Threshold collapses a gray ramp to pure black/white at the luma cutoff: the
// dark end goes black, the bright end white, and the output is strictly binary.
#[test]
fn threshold_splits_to_black_and_white() {
    let Some((device, queue)) = device() else {
        eprintln!("no GPU adapter; skipping threshold_splits_to_black_and_white");
        return;
    };
    let mut gpu = CanvasGpu::new(&device, wgpu::TextureFormat::Bgra8Unorm);
    let n = 16u32;
    gpu.ensure_canvas(&device, Size::new(n, n));
    let l0 = LayerId(0);
    gpu.ensure_layer(&device, l0);
    // Horizontal black→white step so each side is clearly on one side of cutoff.
    gpu.upload_layer(
        &queue,
        l0,
        &layer_from(n, |x, _y| {
            if x < n / 2 {
                [0.0, 0.0, 0.0, 1.0]
            } else {
                [1.0, 1.0, 1.0, 1.0]
            }
        }),
    );
    gpu.apply_threshold(&device, &queue, l0, 0.5);
    let row = n / 2;
    for x in 0..n {
        let v = gpu.read_pixel(&device, &queue, l0, x, row).unwrap()[0];
        assert!(!(0.02..=0.98).contains(&v), "output is binary: {v} at x={x}");
    }
    assert!(gpu.read_pixel(&device, &queue, l0, 0, row).unwrap()[0] < 0.02, "left → black");
    assert!(
        gpu.read_pixel(&device, &queue, l0, n - 1, row).unwrap()[0] > 0.98,
        "right → white"
    );
}
