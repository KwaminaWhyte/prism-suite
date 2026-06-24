use super::*;

// ---- Noise filters (Phase 8) ---------------------------------------------

// Add Noise is seeded-deterministic (same seed → identical result), zero-mean
// (the average is preserved), and monochromatic mode applies the SAME delta to
// R/G/B (so R==G==B on a gray field).

#[test]
fn add_noise_is_deterministic_and_zero_mean() {
    let Some((device, queue)) = device() else {
        eprintln!("no GPU adapter; skipping add_noise_is_deterministic_and_zero_mean");
        return;
    };
    let n = 32u32;
    let base = 0.5f32;
    let gray = layer_from(n, |_x, _y| [base, base, base, 1.0]);

    let run = |seed: f32, mono: bool| -> Vec<[f32; 4]> {
        let mut gpu = CanvasGpu::new(&device, wgpu::TextureFormat::Bgra8Unorm);
        gpu.ensure_canvas(&device, Size::new(n, n));
        let l0 = LayerId(0);
        gpu.ensure_layer(&device, l0);
        gpu.upload_layer(&queue, l0, &gray);
        gpu.apply_noise(&device, &queue, l0, 0.2, mono, true, seed);
        let mut px = Vec::with_capacity((n * n) as usize);
        for y in 0..n {
            for x in 0..n {
                px.push(gpu.read_pixel(&device, &queue, l0, x, y).unwrap());
            }
        }
        px
    };

    let a = run(5.0, false);
    let b = run(5.0, false);
    // Determinism: same seed → identical (no temporal randomness).
    for (pa, pb) in a.iter().zip(b.iter()) {
        for c in 0..4 {
            assert!(
                (pa[c] - pb[c]).abs() < 1e-3,
                "deterministic for a fixed seed"
            );
        }
    }
    // Zero-mean: per-channel average ≈ the base.
    for ch in 0..3 {
        let mean: f32 = a.iter().map(|p| p[ch]).sum::<f32>() / a.len() as f32;
        assert!(
            (mean - base).abs() < 0.03,
            "noise preserves the mean (ch {ch}): {mean}"
        );
    }
    // The field is actually perturbed.
    assert!(
        a.iter().any(|p| (p[0] - base).abs() > 1e-2),
        "noise perturbs the field"
    );

    // Monochromatic: R == G == B for every pixel.
    let m = run(5.0, true);
    for p in &m {
        assert!(
            (p[0] - p[1]).abs() < 2e-3 && (p[1] - p[2]).abs() < 2e-3,
            "monochromatic: equal RGB delta: {p:?}"
        );
    }
}


// Median removes a single bright impulse on a flat field, replacing it with the
// surrounding value, while leaving flat areas untouched.
#[test]
fn median_removes_an_impulse() {
    let Some((device, queue)) = device() else {
        eprintln!("no GPU adapter; skipping median_removes_an_impulse");
        return;
    };
    let mut gpu = CanvasGpu::new(&device, wgpu::TextureFormat::Bgra8Unorm);
    let n = 11u32;
    gpu.ensure_canvas(&device, Size::new(n, n));
    let l0 = LayerId(0);
    gpu.ensure_layer(&device, l0);
    let c = n / 2;
    gpu.upload_layer(
        &queue,
        l0,
        &layer_from(n, |x, y| {
            if x == c && y == c {
                [1.0, 1.0, 1.0, 1.0] // bright impulse / outlier
            } else {
                [0.4, 0.4, 0.4, 1.0]
            }
        }),
    );
    gpu.apply_median(&device, &queue, l0, 1.0, None);

    let center = gpu.read_pixel(&device, &queue, l0, c, c).unwrap();
    assert!(
        (center[0] - 0.4).abs() < 0.03,
        "median removed the impulse: {center:?}"
    );
    let flat = gpu.read_pixel(&device, &queue, l0, 1, 1).unwrap();
    assert!(
        (flat[0] - 0.4).abs() < 0.03,
        "flat area preserved: {flat:?}"
    );
}


// Dust & Scratches only replaces pixels that differ from the window median by
// more than the threshold: a strong speck is removed, a sub-threshold ripple is
// preserved.
#[test]
fn dust_scratches_only_changes_above_threshold() {
    let Some((device, queue)) = device() else {
        eprintln!("no GPU adapter; skipping dust_scratches_only_changes_above_threshold");
        return;
    };
    let mut gpu = CanvasGpu::new(&device, wgpu::TextureFormat::Bgra8Unorm);
    let n = 11u32;
    gpu.ensure_canvas(&device, Size::new(n, n));
    let l0 = LayerId(0);
    gpu.ensure_layer(&device, l0);
    let base = 0.5f32;
    let c = n / 2;
    let (sx, sy) = (2u32, 2u32); // a sub-threshold pixel
    gpu.upload_layer(
        &queue,
        l0,
        &layer_from(n, |x, y| {
            if x == c && y == c {
                [0.95, 0.95, 0.95, 1.0] // big deviation (0.45)
            } else if x == sx && y == sy {
                [base + 0.05, base + 0.05, base + 0.05, 1.0] // tiny (0.05)
            } else {
                [base, base, base, 1.0]
            }
        }),
    );
    gpu.apply_median(&device, &queue, l0, 1.0, Some(0.2));

    let speck = gpu.read_pixel(&device, &queue, l0, c, c).unwrap();
    assert!(
        (speck[0] - base).abs() < 0.03,
        "above-threshold speck replaced by the median: {speck:?}"
    );
    let small = gpu.read_pixel(&device, &queue, l0, sx, sy).unwrap();
    assert!(
        (small[0] - (base + 0.05)).abs() < 0.03,
        "below-threshold pixel preserved: {small:?}"
    );
}


// ---- Pixelate family (Phase 8) -------------------------------------------

// Mosaic averages each cell to one colour: every pixel inside a cell is equal
// (the cell is uniform), and that value is the block mean — here a half-black /
// half-white cell averages to mid-gray.

#[test]
fn mosaic_cell_is_uniform_block_average() {
    let Some((device, queue)) = device() else {
        eprintln!("no GPU adapter; skipping mosaic_cell_is_uniform_block_average");
        return;
    };
    let mut gpu = CanvasGpu::new(&device, wgpu::TextureFormat::Bgra8Unorm);
    let n = 8u32;
    gpu.ensure_canvas(&device, Size::new(n, n));
    let l0 = LayerId(0);
    gpu.ensure_layer(&device, l0);
    // A single 8×8 cell whose left half is black, right half white → mean 0.5.
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
    gpu.apply_mosaic(&device, &queue, l0, n as f32);
    // Every pixel is the same mid-gray average.
    let a = gpu.read_pixel(&device, &queue, l0, 0, 0).unwrap();
    let b = gpu.read_pixel(&device, &queue, l0, n - 1, n - 1).unwrap();
    assert!(
        (a[0] - 0.5).abs() < 0.02,
        "cell averages to mid-gray: {a:?}"
    );
    for c in 0..4 {
        assert!((a[c] - b[c]).abs() < 0.01, "cell is uniform: {a:?} {b:?}");
    }
}


// Crystallize is seeded-deterministic and snaps to source colours: every output
// pixel equals some input pixel (no blending), and a flat field is unchanged.
#[test]
fn crystallize_is_deterministic_and_snaps() {
    let Some((device, queue)) = device() else {
        eprintln!("no GPU adapter; skipping crystallize_is_deterministic_and_snaps");
        return;
    };
    let n = 16u32;
    let make = |x: u32, _y: u32| {
        let v = x as f32 / (n - 1) as f32;
        [v, 1.0 - v, 0.5, 1.0]
    };
    let run = |seed: f32| -> Vec<[f32; 4]> {
        let mut gpu = CanvasGpu::new(&device, wgpu::TextureFormat::Bgra8Unorm);
        gpu.ensure_canvas(&device, Size::new(n, n));
        let l0 = LayerId(0);
        gpu.ensure_layer(&device, l0);
        gpu.upload_layer(&queue, l0, &layer_from(n, make));
        gpu.apply_crystallize(&device, &queue, l0, 4.0, seed);
        let mut px = Vec::new();
        for y in 0..n {
            for x in 0..n {
                px.push(gpu.read_pixel(&device, &queue, l0, x, y).unwrap());
            }
        }
        px
    };
    let a = run(7.0);
    let b = run(7.0);
    for (pa, pb) in a.iter().zip(b.iter()) {
        for c in 0..4 {
            assert!((pa[c] - pb[c]).abs() < 1e-3, "deterministic for a seed");
        }
    }
    // Snapping: every output red value matches one of the source's red values
    // (the source reds are evenly spaced k/(n-1)).
    for p in &a {
        let near = (0..n).any(|k| (p[0] - k as f32 / (n - 1) as f32).abs() < 0.03);
        assert!(near, "snaps to a source colour, no blend: {p:?}");
    }
    let c = run(8.0);
    assert!(a != c, "different seed → different cells");
}


// Color Halftone makes bigger dots (more ink) for darker cells: a dark field
// produces more ink pixels than a bright one.
#[test]
fn color_halftone_dot_tracks_brightness() {
    let Some((device, queue)) = device() else {
        eprintln!("no GPU adapter; skipping color_halftone_dot_tracks_brightness");
        return;
    };
    let n = 32u32;
    let ink = |v: f32| -> usize {
        let mut gpu = CanvasGpu::new(&device, wgpu::TextureFormat::Bgra8Unorm);
        gpu.ensure_canvas(&device, Size::new(n, n));
        let l0 = LayerId(0);
        gpu.ensure_layer(&device, l0);
        gpu.upload_layer(&queue, l0, &layer_from(n, |_x, _y| [v, v, v, 1.0]));
        gpu.apply_color_halftone(&device, &queue, l0, 8.0, 0.0);
        let mut count = 0;
        for y in 0..n {
            for x in 0..n {
                if gpu.read_pixel(&device, &queue, l0, x, y).unwrap()[0] < 0.5 {
                    count += 1; // red-channel ink
                }
            }
        }
        count
    };
    let dark = ink(0.2);
    let bright = ink(0.8);
    assert!(
        dark > bright,
        "darker cell → larger dot → more ink: dark {dark} bright {bright}"
    );
}


// Mezzotint produces a pure black/white field that is brighter (more white) for
// a brighter input and reproducible per seed.
#[test]
fn mezzotint_is_binary_and_tracks_brightness() {
    let Some((device, queue)) = device() else {
        eprintln!("no GPU adapter; skipping mezzotint_is_binary_and_tracks_brightness");
        return;
    };
    let n = 32u32;
    let white = |v: f32| -> usize {
        let mut gpu = CanvasGpu::new(&device, wgpu::TextureFormat::Bgra8Unorm);
        gpu.ensure_canvas(&device, Size::new(n, n));
        let l0 = LayerId(0);
        gpu.ensure_layer(&device, l0);
        gpu.upload_layer(&queue, l0, &layer_from(n, |_x, _y| [v, v, v, 1.0]));
        gpu.apply_mezzotint(&device, &queue, l0, 0.5, 4.0);
        let mut count = 0;
        for y in 0..n {
            for x in 0..n {
                let p = gpu.read_pixel(&device, &queue, l0, x, y).unwrap();
                assert!(
                    p[0] < 0.05 || p[0] > 0.95,
                    "binary black/white output: {p:?}"
                );
                if p[0] > 0.5 {
                    count += 1;
                }
            }
        }
        count
    };
    let dark = white(0.25);
    let bright = white(0.75);
    assert!(bright > dark, "brighter → more white: {bright} > {dark}");
}


// High Pass flattens locally-uniform areas to mid-gray (0.5) and keeps the edge
// detail as a signed deviation about it (an edge has two opposite-signed sides).
#[test]
fn high_pass_flattens_flats_and_keeps_edges() {
    let Some((device, queue)) = device() else {
        eprintln!("no GPU adapter; skipping high_pass_flattens_flats_and_keeps_edges");
        return;
    };
    let mut gpu = CanvasGpu::new(&device, wgpu::TextureFormat::Bgra8Unorm);
    let n = 32u32;
    gpu.ensure_canvas(&device, Size::new(n, n));
    let l0 = LayerId(0);
    gpu.ensure_layer(&device, l0);
    // Vertical black/white split at the mid column.
    let c = n / 2;
    gpu.upload_layer(
        &queue,
        l0,
        &layer_from(n, |x, _y| {
            if x >= c {
                [1.0, 1.0, 1.0, 1.0]
            } else {
                [0.0, 0.0, 0.0, 1.0]
            }
        }),
    );
    gpu.apply_high_pass(&device, &queue, l0, 3.0, 1.0);

    let row = n / 2;
    // A column far from the edge is locally flat → high pass ≈ mid-gray.
    let flat = gpu.read_pixel(&device, &queue, l0, 2, row).unwrap();
    assert!(
        (flat[0] - 0.5).abs() < 0.05 && (flat[3] - 1.0).abs() < 0.02,
        "flat area → mid-gray, alpha kept: {flat:?}"
    );
    // The two sides of the edge deviate in opposite directions about mid-gray.
    let dark_side = gpu.read_pixel(&device, &queue, l0, c - 1, row).unwrap();
    let light_side = gpu.read_pixel(&device, &queue, l0, c, row).unwrap();
    assert!(
        dark_side[0] < 0.5,
        "dark side of edge dips below mid-gray: {dark_side:?}"
    );
    assert!(
        light_side[0] > 0.5,
        "light side of edge rises above mid-gray: {light_side:?}"
    );
}
