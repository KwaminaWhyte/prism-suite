use super::*;

// Tilt-Shift (Blur Gallery, kind 30): a vertical hard edge runs the full canvas
// height; a horizontal focus band sits on the middle rows. Inside the band the
// edge stays a hard step; far above the band the edge is blurred (bleeds), so
// the column just left of the step is no longer pure black. Skips if no adapter.
#[test]
fn tilt_shift_band_sharp_outside_blurred() {
    let Some((device, queue)) = device() else {
        eprintln!("no GPU adapter; skipping tilt_shift_band_sharp_outside_blurred");
        return;
    };
    let mut gpu = CanvasGpu::new(&device, wgpu::TextureFormat::Bgra8Unorm);
    let n = 32u32;
    gpu.ensure_canvas(&device, Size::new(n, n));
    let l0 = LayerId(0);
    gpu.ensure_layer(&device, l0);
    // Black left half, white right half (hard vertical edge at x = n/2).
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
    // Focus band centred on the middle row, narrow band + feather, strong blur.
    let center = 0.5; // uv (mid height)
    gpu.apply_tilt_shift(&device, &queue, l0, center, 3.0, 4.0, 8.0, 0.0);

    let col = n / 2; // x just right of the step is white, x-1 is black
    // In-band row: the edge is untouched (still a hard black→white step).
    let mid = n / 2;
    let in_l = gpu.read_pixel(&device, &queue, l0, col - 1, mid).unwrap()[0];
    let in_r = gpu.read_pixel(&device, &queue, l0, col, mid).unwrap()[0];
    assert!(in_l < 0.05, "in-band left of edge stays black: {in_l}");
    assert!(in_r > 0.95, "in-band right of edge stays white: {in_r}");
    // Far-from-band row (top): the edge is blurred, so the black side bleeds up.
    let far_l = gpu.read_pixel(&device, &queue, l0, col - 1, 0).unwrap()[0];
    assert!(
        far_l > 0.05,
        "far-from-band edge should blur (bleed), got {far_l}"
    );
}


// Iris Blur (Blur Gallery, kind 31): a vertical hard edge runs the full canvas;
// a small focus ellipse sits at the canvas center. The center pixel stays a hard
// step; a corner pixel (well outside the ellipse) is blurred (bleeds), so the
// column just left of the step is no longer pure black. Skips if no adapter.
#[test]
fn iris_blur_center_sharp_outside_blurred() {
    let Some((device, queue)) = device() else {
        eprintln!("no GPU adapter; skipping iris_blur_center_sharp_outside_blurred");
        return;
    };
    let mut gpu = CanvasGpu::new(&device, wgpu::TextureFormat::Bgra8Unorm);
    let n = 41u32;
    gpu.ensure_canvas(&device, Size::new(n, n));
    let l0 = LayerId(0);
    gpu.ensure_layer(&device, l0);
    // Black left half, white right half (hard vertical edge at x = n/2).
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
    let c = (n / 2) as f32;
    // Small focus ellipse at the center, strong blur outside.
    gpu.apply_iris_blur(&device, &queue, l0, c, c, 5.0, 5.0, 0.5, 8.0);

    let col = n / 2; // x just right of the step is white, x-1 is black
    let mid = n / 2;
    // Center (inside the ellipse): the edge is untouched (hard step).
    let in_l = gpu.read_pixel(&device, &queue, l0, col - 1, mid).unwrap()[0];
    let in_r = gpu.read_pixel(&device, &queue, l0, col, mid).unwrap()[0];
    assert!(in_l < 0.05, "center left of edge stays black: {in_l}");
    assert!(in_r > 0.95, "center right of edge stays white: {in_r}");
    // Corner (top, far outside the ellipse): the edge is blurred, so it bleeds.
    let far_l = gpu.read_pixel(&device, &queue, l0, col - 1, 0).unwrap()[0];
    assert!(
        far_l > 0.05,
        "far-from-center edge should blur (bleed), got {far_l}"
    );
}


// Field Blur (Blur Gallery, kind 33): a vertical hard edge runs the full canvas.
// Two pins — a sharp pin (amount 0) at the canvas center and a strong-blur pin in
// the top-left corner — make the radius field grow from center to corner. The
// center stays a hard step; the top-left corner is blurred, so the column just
// left of the step bleeds there. Skips if no GPU adapter.
#[test]
fn field_blur_center_sharp_corner_blurred() {
    let Some((device, queue)) = device() else {
        eprintln!("no GPU adapter; skipping field_blur_center_sharp_corner_blurred");
        return;
    };
    let mut gpu = CanvasGpu::new(&device, wgpu::TextureFormat::Bgra8Unorm);
    let n = 41u32;
    gpu.ensure_canvas(&device, Size::new(n, n));
    let l0 = LayerId(0);
    gpu.ensure_layer(&device, l0);
    // Black left half, white right half (hard vertical edge at x = n/2).
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
    let c = (n / 2) as f32;
    // Sharp pin at the center (amount 0), strong-blur pin in the top-left corner.
    let pins = [(c, c, 0.0), (2.0, 2.0, 12.0)];
    gpu.apply_field_blur(&device, &queue, l0, &pins);

    let col = n / 2; // x just right of the step is white, x-1 is black
    let mid = n / 2;
    // Center (sharp pin dominates) → the edge is a hard step.
    let in_l = gpu.read_pixel(&device, &queue, l0, col - 1, mid).unwrap()[0];
    let in_r = gpu.read_pixel(&device, &queue, l0, col, mid).unwrap()[0];
    assert!(in_l < 0.2, "center left of edge stays mostly black: {in_l}");
    assert!(in_r > 0.8, "center right of edge stays mostly white: {in_r}");
    // Top-left corner (near the blur pin) → the edge bleeds across.
    let far_l = gpu.read_pixel(&device, &queue, l0, col - 1, 1).unwrap()[0];
    assert!(
        far_l > 0.05,
        "near the blur pin the edge should blur (bleed), got {far_l}"
    );
}


// Field Blur with a single pin is a uniform blur everywhere; a single zero-amount
// pin is the identity. Skips if no GPU adapter.
#[test]
fn field_blur_single_pin_uniform_and_zero_is_identity() {
    let Some((device, queue)) = device() else {
        eprintln!("no GPU adapter; skipping field_blur_single_pin_uniform_and_zero_is_identity");
        return;
    };
    let n = 21u32;
    let size = Size::new(n, n);
    let edge = |x: u32, _y: u32| {
        if x < n / 2 {
            [0.0, 0.0, 0.0, 1.0]
        } else {
            [1.0, 1.0, 1.0, 1.0]
        }
    };
    // --- Zero-amount single pin: identity (the hard edge survives). ---
    let mut gpu = CanvasGpu::new(&device, wgpu::TextureFormat::Bgra8Unorm);
    gpu.ensure_canvas(&device, size);
    let li = LayerId(0);
    gpu.ensure_layer(&device, li);
    gpu.upload_layer(&queue, li, &layer_from(n, edge));
    gpu.apply_field_blur(&device, &queue, li, &[(5.0, 5.0, 0.0)]);
    let col = n / 2;
    let row = n / 2;
    let id_l = gpu.read_pixel(&device, &queue, li, col - 1, row).unwrap()[0];
    assert!(id_l < 0.05, "zero-amount pin is identity (edge stays sharp): {id_l}");

    // --- Nonzero single pin off-corner: uniform blur — even the far corner,
    // distant from the pin, is softened (the field is constant). ---
    let mut gpu = CanvasGpu::new(&device, wgpu::TextureFormat::Bgra8Unorm);
    gpu.ensure_canvas(&device, size);
    let lu = LayerId(0);
    gpu.ensure_layer(&device, lu);
    gpu.upload_layer(&queue, lu, &layer_from(n, edge));
    gpu.apply_field_blur(&device, &queue, lu, &[(1.0, 1.0, 8.0)]);
    // Bottom row, far from the lone top-left pin, must still be blurred.
    let far_l = gpu.read_pixel(&device, &queue, lu, col - 1, n - 1).unwrap()[0];
    assert!(
        far_l > 0.05,
        "single pin blurs uniformly everywhere, incl. far corner, got {far_l}"
    );
}


// Spin Blur (Blur Gallery): rotational motion blur of a flat (constant) image is
// the identity — spinning a constant color about any center reproduces it; an
// off-center bright pixel smears tangentially (vertically), not radially.
// Skips if no adapter.
#[test]
fn spin_blur_flat_identity_and_smears_tangentially() {
    let Some((device, queue)) = device() else {
        eprintln!("no GPU adapter; skipping spin_blur_flat_identity_and_smears_tangentially");
        return;
    };
    let n = 31u32;
    let size = Size::new(n, n);
    let c = (n / 2) as f32;

    // --- Flat image: spin blur is identity. ---
    let mut gpu = CanvasGpu::new(&device, wgpu::TextureFormat::Bgra8Unorm);
    gpu.ensure_canvas(&device, size);
    let lf = LayerId(0);
    gpu.ensure_layer(&device, lf);
    gpu.upload_layer(&queue, lf, &layer_from(n, |_x, _y| [0.4, 0.6, 0.8, 1.0]));
    gpu.apply_radial_blur(&device, &queue, lf, c, c, true, 30.0_f32.to_radians(), 32);
    let p = gpu.read_pixel(&device, &queue, lf, n / 2, n / 4).unwrap();
    assert!(
        (p[0] - 0.4).abs() < 0.02
            && (p[1] - 0.6).abs() < 0.02
            && (p[2] - 0.8).abs() < 0.02,
        "spin blur of a flat image is identity, got {p:?}"
    );

    // --- Off-center bright pixel: smears tangentially, not radially. ---
    let bx = n / 2 + 8; // right of center, same row
    let by = n / 2;
    let mut gpu = CanvasGpu::new(&device, wgpu::TextureFormat::Bgra8Unorm);
    gpu.ensure_canvas(&device, size);
    let ls = LayerId(0);
    gpu.ensure_layer(&device, ls);
    gpu.upload_layer(
        &queue,
        ls,
        &layer_from(n, |x, y| {
            if x == bx && y == by {
                [1.0, 1.0, 1.0, 1.0]
            } else {
                [0.0; 4]
            }
        }),
    );
    gpu.apply_radial_blur(&device, &queue, ls, c, c, true, 0.3, 32);
    let tang = gpu.read_pixel(&device, &queue, ls, bx, by - 2).unwrap()[3]
        + gpu.read_pixel(&device, &queue, ls, bx, by + 2).unwrap()[3];
    let radial = gpu.read_pixel(&device, &queue, ls, bx - 2, by).unwrap()[3]
        + gpu.read_pixel(&device, &queue, ls, bx + 2, by).unwrap()[3];
    assert!(
        tang > radial + 0.02,
        "spin smears tangentially (vert {tang}) > radially (horiz {radial})"
    );
}
