use super::*;

// ---- Blur-family filters (Phase 8) ---------------------------------------


// Motion blur smears a single opaque pixel along the angle, not across it.
#[test]
fn motion_blur_smears_along_angle() {
    let Some((device, queue)) = device() else {
        eprintln!("no GPU adapter; skipping motion_blur_smears_along_angle");
        return;
    };
    let mut gpu = CanvasGpu::new(&device, wgpu::TextureFormat::Bgra8Unorm);
    let n = 17;
    let size = Size::new(n, n);
    gpu.ensure_canvas(&device, size);
    let l0 = LayerId(0);
    gpu.ensure_layer(&device, l0);
    // One opaque white pixel at the center, rest transparent.
    let c = n / 2;
    gpu.upload_layer(
        &queue,
        l0,
        &layer_from(n, |x, y| {
            if x == c && y == c {
                [1.0, 1.0, 1.0, 1.0]
            } else {
                [0.0; 4]
            }
        }),
    );

    // Horizontal motion blur (angle 0), 3 taps each side.
    gpu.apply_motion_blur(&device, &queue, l0, 0.0, 3.0);

    let on_axis = gpu.read_pixel(&device, &queue, l0, c + 2, c).unwrap();
    let off_axis = gpu.read_pixel(&device, &queue, l0, c, c + 2).unwrap();
    assert!(
        on_axis[3] > 0.05,
        "pixel 2 along the streak picks up signal: {on_axis:?}"
    );
    assert!(
        off_axis[3] < 0.02,
        "off-axis (perpendicular) stays empty: {off_axis:?}"
    );
    assert!(
        on_axis[3] > off_axis[3] + 0.05,
        "streak axis >> perpendicular"
    );
}


// Box blur preserves a flat field exactly (kernel normalized) and spreads an
// impulse symmetrically (separable H+V).
#[test]
fn box_blur_normalizes_and_spreads() {
    let Some((device, queue)) = device() else {
        eprintln!("no GPU adapter; skipping box_blur_normalizes_and_spreads");
        return;
    };
    let mut gpu = CanvasGpu::new(&device, wgpu::TextureFormat::Bgra8Unorm);
    let n = 16;
    let size = Size::new(n, n);
    gpu.ensure_canvas(&device, size);

    // Flat opaque gray must survive a box blur unchanged (normalization).
    let l0 = LayerId(0);
    gpu.ensure_layer(&device, l0);
    gpu.upload_layer(&queue, l0, &solid(n, 0.5, 0.5, 0.5, 1.0));
    gpu.apply_filter(&device, &queue, l0, 5, 3.0, 0.0); // kind 5 = box blur
    let flat = gpu.read_pixel(&device, &queue, l0, n / 2, n / 2).unwrap();
    assert!(
        (flat[0] - 0.5).abs() < 0.02 && (flat[3] - 1.0).abs() < 0.02,
        "flat field preserved by box blur: {flat:?}"
    );

    // Impulse spreads into the neighbourhood symmetrically.
    let l1 = LayerId(1);
    gpu.ensure_layer(&device, l1);
    let c = n / 2;
    gpu.upload_layer(
        &queue,
        l1,
        &layer_from(n, |x, y| {
            if x == c && y == c {
                [1.0, 1.0, 1.0, 1.0]
            } else {
                [0.0; 4]
            }
        }),
    );
    gpu.apply_filter(&device, &queue, l1, 5, 1.0, 0.0); // radius 1 -> 3x3 box
    let center = gpu.read_pixel(&device, &queue, l1, c, c).unwrap();
    let right = gpu.read_pixel(&device, &queue, l1, c + 1, c).unwrap();
    let down = gpu.read_pixel(&device, &queue, l1, c, c + 1).unwrap();
    let far = gpu.read_pixel(&device, &queue, l1, c + 3, c + 3).unwrap();
    assert!(center[3] > 0.05, "center retains some signal: {center:?}");
    assert!(right[3] > 0.05, "spread right: {right:?}");
    assert!(down[3] > 0.05, "spread down: {down:?}");
    assert!((right[3] - down[3]).abs() < 0.02, "spread symmetric H vs V");
    assert!(far[3] < 0.02, "no signal outside the 3x3 box: {far:?}");
}


// Radial spin smears tangentially; radial zoom smears radially. An off-center
// bright pixel distinguishes the two modes.
#[test]
fn radial_spin_vs_zoom() {
    let Some((device, queue)) = device() else {
        eprintln!("no GPU adapter; skipping radial_spin_vs_zoom");
        return;
    };
    let n = 31u32;
    let size = Size::new(n, n);
    let c = (n / 2) as i32;
    let bx = (c + 8) as u32; // right of center, same row
    let by = c as u32;
    let bright = |x: u32, y: u32| {
        if x == bx && y == by {
            [1.0, 1.0, 1.0, 1.0]
        } else {
            [0.0; 4]
        }
    };

    // --- Spin: smears vertically (tangent), not horizontally (radius). ---
    let mut gpu = CanvasGpu::new(&device, wgpu::TextureFormat::Bgra8Unorm);
    gpu.ensure_canvas(&device, size);
    let ls = LayerId(0);
    gpu.ensure_layer(&device, ls);
    gpu.upload_layer(&queue, ls, &layer_from(n, bright));
    // ~17° spin, plenty of samples.
    gpu.apply_radial_blur(&device, &queue, ls, c as f32, c as f32, true, 0.3, 32);
    let tang = gpu.read_pixel(&device, &queue, ls, bx, by - 2).unwrap()[3]
        + gpu.read_pixel(&device, &queue, ls, bx, by + 2).unwrap()[3];
    let radial = gpu.read_pixel(&device, &queue, ls, bx - 2, by).unwrap()[3]
        + gpu.read_pixel(&device, &queue, ls, bx + 2, by).unwrap()[3];
    assert!(
        tang > radial + 0.02,
        "spin smears tangentially (vert {tang}) > radially (horiz {radial})"
    );

    // --- Zoom: smears horizontally (radius), not vertically (tangent). ---
    let mut gpu = CanvasGpu::new(&device, wgpu::TextureFormat::Bgra8Unorm);
    gpu.ensure_canvas(&device, size);
    let lz = LayerId(0);
    gpu.ensure_layer(&device, lz);
    gpu.upload_layer(&queue, lz, &layer_from(n, bright));
    gpu.apply_radial_blur(&device, &queue, lz, c as f32, c as f32, false, 0.4, 32);
    let z_radial = gpu.read_pixel(&device, &queue, lz, bx - 2, by).unwrap()[3]
        + gpu.read_pixel(&device, &queue, lz, bx + 2, by).unwrap()[3];
    let z_tang = gpu.read_pixel(&device, &queue, lz, bx, by - 2).unwrap()[3]
        + gpu.read_pixel(&device, &queue, lz, bx, by + 2).unwrap()[3];
    assert!(
        z_radial > z_tang + 0.02,
        "zoom smears radially (horiz {z_radial}) > tangentially (vert {z_tang})"
    );
}
