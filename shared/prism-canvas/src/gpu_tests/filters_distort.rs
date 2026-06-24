use super::*;

// ---- Distort filters (Phase 8) -------------------------------------------


// Twirl rotates the source about the center inside its radius; pixels outside
// the radius are untouched, and a pixel above the center samples off its column.
#[test]
fn twirl_rotates_within_radius() {
    let Some((device, queue)) = device() else {
        eprintln!("no GPU adapter; skipping twirl_rotates_within_radius");
        return;
    };
    let mut gpu = CanvasGpu::new(&device, wgpu::TextureFormat::Bgra8Unorm);
    let n = 41u32;
    gpu.ensure_canvas(&device, Size::new(n, n));
    let l0 = LayerId(0);
    gpu.ensure_layer(&device, l0);
    gpu.upload_layer(&queue, l0, &ramp_layer(n));

    let c = n / 2;
    // ~86° twirl over a generous radius.
    gpu.apply_distort(
        &device, &queue, l0, 8, c as f32, c as f32, 1.5, 18.0, [0.0; 2],
    );

    // A point above the center departs from the 0.5 center-column value.
    let above = gpu.read_pixel(&device, &queue, l0, c, c - 6).unwrap();
    assert!(
        (above[0] - 0.5).abs() > 0.04,
        "twirl rotated the sample off the center column: {above:?}"
    );
    // A far corner (outside the radius) is unchanged: ~its source ramp value.
    let corner = gpu.read_pixel(&device, &queue, l0, 1, 1).unwrap();
    let expect = 1.0 / (n - 1) as f32;
    assert!(
        (corner[0] - expect).abs() < 0.05,
        "outside-radius pixel untouched: {corner:?} (expect {expect})"
    );
}


// Pinch and bulge (signed amount) displace the source in opposite radial
// directions: on a left→right ramp, a pixel right of center reads lower under
// pinch and higher under bulge.
#[test]
fn pinch_and_bulge_are_signed() {
    let Some((device, queue)) = device() else {
        eprintln!("no GPU adapter; skipping pinch_and_bulge_are_signed");
        return;
    };
    let n = 41u32;
    let c = n / 2;
    let px = c + 6;
    let radius = 18.0;

    let mut gp = CanvasGpu::new(&device, wgpu::TextureFormat::Bgra8Unorm);
    gp.ensure_canvas(&device, Size::new(n, n));
    let lp = LayerId(0);
    gp.ensure_layer(&device, lp);
    gp.upload_layer(&queue, lp, &ramp_layer(n));
    gp.apply_distort(
        &device, &queue, lp, 9, c as f32, c as f32, 0.6, radius, [0.0; 2],
    );
    let pinch = gp.read_pixel(&device, &queue, lp, px, c).unwrap()[0];

    let mut gb = CanvasGpu::new(&device, wgpu::TextureFormat::Bgra8Unorm);
    gb.ensure_canvas(&device, Size::new(n, n));
    let lb = LayerId(0);
    gb.ensure_layer(&device, lb);
    gb.upload_layer(&queue, lb, &ramp_layer(n));
    gb.apply_distort(
        &device, &queue, lb, 9, c as f32, c as f32, -0.6, radius, [0.0; 2],
    );
    let bulge = gb.read_pixel(&device, &queue, lb, px, c).unwrap()[0];

    let identity = px as f32 / (n - 1) as f32;
    assert!(
        pinch < identity - 0.01,
        "pinch pulls source inward: {pinch}"
    );
    assert!(
        bulge > identity + 0.01,
        "bulge pushes source outward: {bulge}"
    );
    assert!(bulge > pinch, "bulge and pinch are signed-opposite");
}


// Ripple displaces sinusoidally with a wavelength period: rows one wavelength
// apart receive the same x-displacement, so they read back identical.
#[test]
fn ripple_is_periodic() {
    let Some((device, queue)) = device() else {
        eprintln!("no GPU adapter; skipping ripple_is_periodic");
        return;
    };
    let mut gpu = CanvasGpu::new(&device, wgpu::TextureFormat::Bgra8Unorm);
    let n = 60u32;
    let wl = 20.0f32;
    gpu.ensure_canvas(&device, Size::new(n, n));
    let l0 = LayerId(0);
    gpu.ensure_layer(&device, l0);
    gpu.upload_layer(&queue, l0, &ramp_layer(n));
    gpu.apply_distort(&device, &queue, l0, 10, 0.0, 0.0, 0.0, 0.0, [6.0, wl]);

    let probe_x = n / 3;
    let y0 = 12u32;
    let y1 = y0 + wl as u32;
    let a = gpu.read_pixel(&device, &queue, l0, probe_x, y0).unwrap()[0];
    let b = gpu.read_pixel(&device, &queue, l0, probe_x, y1).unwrap()[0];
    assert!(
        (a - b).abs() < 0.03,
        "rows one wavelength apart match: {a} vs {b}"
    );

    // And the warp is non-trivial: some pixel differs from its source ramp.
    let mid = gpu.read_pixel(&device, &queue, l0, n / 2, 3).unwrap()[0];
    let src = (n / 2) as f32 / (n - 1) as f32;
    assert!(
        (mid - src).abs() > 0.005,
        "ripple displaced a sample: {mid}"
    );
}


// Polar round-trip: ToRect ∘ ToPolar recovers a smooth radial image near the
// center, within resampling tolerance.
#[test]
fn polar_round_trips() {
    let Some((device, queue)) = device() else {
        eprintln!("no GPU adapter; skipping polar_round_trips");
        return;
    };
    let mut gpu = CanvasGpu::new(&device, wgpu::TextureFormat::Bgra8Unorm);
    let n = 64u32;
    let c = (n / 2) as f32;
    gpu.ensure_canvas(&device, Size::new(n, n));
    let l0 = LayerId(0);
    gpu.ensure_layer(&device, l0);
    // Smooth radial bump (bright center → dark edge).
    gpu.upload_layer(
        &queue,
        l0,
        &layer_from(n, |x, y| {
            let dx = x as f32 - c;
            let dy = y as f32 - c;
            let r = (dx * dx + dy * dy).sqrt() / c;
            let v = (1.0 - r).clamp(0.0, 1.0);
            [v, v, v, 1.0]
        }),
    );

    let probe = |g: &CanvasGpu, dev: &wgpu::Device, q: &wgpu::Queue| {
        g.read_pixel(dev, q, l0, n / 2, n / 2 - n / 4).unwrap()[0]
    };
    let before = probe(&gpu, &device, &queue);

    gpu.apply_distort(&device, &queue, l0, 11, c, c, 0.0, 0.0, [0.0; 2]); // rect->polar
    gpu.apply_distort(&device, &queue, l0, 12, c, c, 0.0, 0.0, [0.0; 2]); // polar->rect
    let after = probe(&gpu, &device, &queue);
    assert!(
        (after - before).abs() < 0.12,
        "polar round-trip recovers the value: {before} -> {after}"
    );
}
