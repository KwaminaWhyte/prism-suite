use super::*;

// Drives the real GPU compositor: upload -> composite -> brush(wet)+flatten
// -> undo, asserting pixels via readback. Skips if no GPU adapter.
#[test]
fn compositor_brush_wet_undo() {
    let Some((device, queue)) = device() else {
        eprintln!("no GPU adapter; skipping compositor_brush_wet_undo");
        return;
    };
    let mut gpu = CanvasGpu::new(&device, wgpu::TextureFormat::Bgra8Unorm);
    let size = Size::new(8, 8);
    gpu.ensure_canvas(&device, size);
    let l0 = LayerId(0);
    gpu.ensure_layer(&device, l0);
    gpu.upload_layer(&queue, l0, &solid(8, 1.0, 0.0, 0.0, 1.0)); // red

    let order = vec![LayerDraw {
        id: l0,
        opacity: 1.0,
        blend: 0,
        visible: true,
        adjust_kind: 0,
        adjust: [0.0; 4],
        mix_r: [1.0, 0.0, 0.0, 0.0],
        mix_g: [0.0, 1.0, 0.0, 0.0],
        mix_b: [0.0, 0.0, 1.0, 0.0],
        has_blend_if: false,
        blend_if: [0.0, 1.0, 0.0, 1.0],
        clipped: false,
        has_stroke: false,
        stroke_color: [0.0; 4],
        stroke_width: 0.0,
        has_shadow: false,
        shadow_color: [0.0; 4],
        shadow_offset: [0.0; 2],
        shadow_blur: 0.0,
        has_overlay: false,
        overlay_color: [0.0; 4],
        has_inner_shadow: false,
        inner_shadow_color: [0.0; 4],
        inner_shadow_offset: [0.0; 2],
        inner_shadow_blur: 0.0,
        has_outer_glow: false,
        outer_glow_color: [0.0; 4],
        outer_glow_size: 0.0,
        has_inner_glow: false,
        inner_glow_color: [0.0; 4],
        inner_glow_size: 0.0,
        has_grad_overlay: false,
        grad_color0: [0.0; 4],
        grad_color1: [0.0; 4],
        grad_angle: 0.0,
        grad_opacity: 0.0,
        has_bevel: false,
        bevel_highlight: [0.0; 4],
        bevel_shadow: [0.0; 4],
        bevel_size: 0.0,
        bevel_soften: 0.0,
        bevel_angle: 0.0,
        bevel_altitude: 0.0,
    }];
    let p = gpu.composite_now(&device, &queue, &order);
    let px = gpu.read_composite_pixel(&device, &queue, p, 4, 4).unwrap();
    assert!(
        px[0] > 0.9 && px[1] < 0.1 && px[3] > 0.9,
        "composite red: {px:?}"
    );

    // Blue wet brush dab over the center, flattened.
    let mut enc = device.create_command_encoder(&Default::default());
    gpu.begin_command(&device, &mut enc, l0, "test");
    gpu.wet_begin(&mut enc, l0, 1.0);
    let dab = Dab {
        center: [2.0, 2.0],
        radius: 2.5,
        hardness: 0.95,
        color: [0.0, 0.0, 1.0, 1.0],
    };
    gpu.paint_dabs(&device, &queue, &mut enc, l0, &[dab], false, true, false);
    gpu.wet_end(&device, &queue, &mut enc);
    // Region-COW: only snapshot the touched corner.
    gpu.commit_command(&device, &mut enc, [0, 0, 5, 5]);
    queue.submit([enc.finish()]);

    let p = gpu.composite_now(&device, &queue, &order);
    let near = gpu.read_composite_pixel(&device, &queue, p, 2, 2).unwrap();
    let far = gpu.read_composite_pixel(&device, &queue, p, 7, 7).unwrap();
    assert!(
        near[2] > 0.9 && near[0] < 0.1,
        "baked blue at stroke: {near:?}"
    );
    assert!(
        far[0] > 0.9 && far[2] < 0.1,
        "far pixel untouched red: {far:?}"
    );

    // Undo restores the region to red.
    let mut enc = device.create_command_encoder(&Default::default());
    gpu.undo(&device, &mut enc);
    queue.submit([enc.finish()]);
    let p = gpu.composite_now(&device, &queue, &order);
    let near = gpu.read_composite_pixel(&device, &queue, p, 2, 2).unwrap();
    assert!(near[0] > 0.9 && near[2] < 0.1, "undo back to red: {near:?}");
}

// A rectangle selection must clip painting to the selected region.
#[test]
fn selection_clips_paint() {
    let Some((device, queue)) = device() else {
        eprintln!("no GPU adapter; skipping selection_clips_paint");
        return;
    };
    let mut gpu = CanvasGpu::new(&device, wgpu::TextureFormat::Bgra8Unorm);
    let size = Size::new(8, 8);
    gpu.ensure_canvas(&device, size);
    let l0 = LayerId(0);
    gpu.ensure_layer(&device, l0);
    gpu.upload_layer(&queue, l0, &solid(8, 1.0, 0.0, 0.0, 1.0)); // red
    let order = vec![LayerDraw {
        id: l0,
        opacity: 1.0,
        blend: 0,
        visible: true,
        adjust_kind: 0,
        adjust: [0.0; 4],
        mix_r: [1.0, 0.0, 0.0, 0.0],
        mix_g: [0.0, 1.0, 0.0, 0.0],
        mix_b: [0.0, 0.0, 1.0, 0.0],
        has_blend_if: false,
        blend_if: [0.0, 1.0, 0.0, 1.0],
        clipped: false,
        has_stroke: false,
        stroke_color: [0.0; 4],
        stroke_width: 0.0,
        has_shadow: false,
        shadow_color: [0.0; 4],
        shadow_offset: [0.0; 2],
        shadow_blur: 0.0,
        has_overlay: false,
        overlay_color: [0.0; 4],
        has_inner_shadow: false,
        inner_shadow_color: [0.0; 4],
        inner_shadow_offset: [0.0; 2],
        inner_shadow_blur: 0.0,
        has_outer_glow: false,
        outer_glow_color: [0.0; 4],
        outer_glow_size: 0.0,
        has_inner_glow: false,
        inner_glow_color: [0.0; 4],
        inner_glow_size: 0.0,
        has_grad_overlay: false,
        grad_color0: [0.0; 4],
        grad_color1: [0.0; 4],
        grad_angle: 0.0,
        grad_opacity: 0.0,
        has_bevel: false,
        bevel_highlight: [0.0; 4],
        bevel_shadow: [0.0; 4],
        bevel_size: 0.0,
        bevel_soften: 0.0,
        bevel_angle: 0.0,
        bevel_altitude: 0.0,
    }];

    // Select the left half, then paint blue over the whole canvas.
    let mut enc = device.create_command_encoder(&Default::default());
    gpu.apply_selection(
        &device,
        &queue,
        &mut enc,
        &SelectionOp::Marquee {
            rect: [0.0, 0.0, 4.0, 8.0],
            ellipse: false,
        },
    );
    let dab = Dab {
        center: [4.0, 4.0],
        radius: 16.0,
        hardness: 0.99,
        color: [0.0, 0.0, 1.0, 1.0],
    };
    gpu.paint_dabs(&device, &queue, &mut enc, l0, &[dab], false, false, false);
    queue.submit([enc.finish()]);
    assert!(gpu.has_selection());

    let p = gpu.composite_now(&device, &queue, &order);
    let inside = gpu.read_composite_pixel(&device, &queue, p, 1, 4).unwrap();
    let outside = gpu.read_composite_pixel(&device, &queue, p, 6, 4).unwrap();
    assert!(inside[2] > 0.5, "selected area painted blue: {inside:?}");
    assert!(
        outside[0] > 0.5 && outside[2] < 0.2,
        "unselected area untouched red: {outside:?}"
    );
}

// Translating a layer by +half-width then baking should clear the left half
// and keep the right half.
#[test]
fn transform_bake_translates() {
    let Some((device, queue)) = device() else {
        eprintln!("no GPU adapter; skipping transform_bake_translates");
        return;
    };
    let mut gpu = CanvasGpu::new(&device, wgpu::TextureFormat::Bgra8Unorm);
    let size = Size::new(8, 8);
    gpu.ensure_canvas(&device, size);
    let l0 = LayerId(0);
    gpu.ensure_layer(&device, l0);
    gpu.upload_layer(&queue, l0, &solid(8, 1.0, 0.0, 0.0, 1.0)); // red everywhere

    // Move right by half width: layer-from-canvas off.x = -0.5.
    gpu.set_layer_transform(Some(l0), [1.0, 0.0, 0.0, 1.0], [-0.5, 0.0]);
    let mut enc = device.create_command_encoder(&Default::default());
    gpu.bake_transform(&device, &queue, &mut enc);
    queue.submit([enc.finish()]);

    let left = gpu.read_pixel(&device, &queue, l0, 1, 4).unwrap();
    let right = gpu.read_pixel(&device, &queue, l0, 6, 4).unwrap();
    assert!(left[3] < 0.1, "left half cleared after move: {left:?}");
    assert!(
        right[0] > 0.9 && right[3] > 0.9,
        "right half keeps red: {right:?}"
    );
}

// Regression for "move snaps back to origin": the drag-stop frame fires the
// bake. The bug was that the app cleared the live affine (set_layer_transform
// with None) on that same frame *before* baking — turning the bake into a no-op
// (`xform_layer` was None, so `bake_transform` returned early) and snapping the
// layer back to where it started. The fix keeps the affine live for the bake
// frame, so the bake must land. This is the GPU half of the seam; the gating
// fix itself lives in `app::view` (egui interaction, not unit-testable).
#[test]
fn move_persists_when_baked_on_drag_stop() {
    let Some((device, queue)) = device() else {
        eprintln!("no GPU adapter; skipping move_persists_when_baked_on_drag_stop");
        return;
    };
    let mut gpu = CanvasGpu::new(&device, wgpu::TextureFormat::Bgra8Unorm);
    let size = Size::new(8, 8);
    gpu.ensure_canvas(&device, size);
    let l0 = LayerId(0);
    gpu.ensure_layer(&device, l0);
    gpu.upload_layer(&queue, l0, &solid(8, 1.0, 0.0, 0.0, 1.0)); // red everywhere

    // Drag frames keep the live affine (right by half width) on the layer; the
    // fixed drag-stop frame re-sends that same affine instead of clearing it,
    // then issues the bake. With the bug (a None here) the bake was a no-op.
    gpu.set_layer_transform(Some(l0), [1.0, 0.0, 0.0, 1.0], [-0.5, 0.0]);
    let mut enc = device.create_command_encoder(&Default::default());
    gpu.bake_transform(&device, &queue, &mut enc);
    queue.submit([enc.finish()]);

    let left = gpu.read_pixel(&device, &queue, l0, 1, 4).unwrap();
    let right = gpu.read_pixel(&device, &queue, l0, 6, 4).unwrap();
    assert!(left[3] < 0.1, "left half cleared after move persists: {left:?}");
    assert!(
        right[0] > 0.9 && right[3] > 0.9,
        "right half keeps red after move persists: {right:?}"
    );
}

// Clone stamp: source = left-half green / right-half red. Stamping the right
// half with offset +4px samples the green left half, so the dab paints green.
#[test]
fn clone_stamp_copies_source() {
    let Some((device, queue)) = device() else {
        eprintln!("no GPU adapter; skipping clone_stamp_copies_source");
        return;
    };
    let mut gpu = CanvasGpu::new(&device, wgpu::TextureFormat::Bgra8Unorm);
    let size = Size::new(8, 8);
    gpu.ensure_canvas(&device, size); // selection exists, has_selection = false
    let l0 = LayerId(0);
    gpu.ensure_layer(&device, l0);

    // Left half (x<4) green, right half red — premultiplied f16.
    let mut buf = Vec::new();
    for _y in 0..8 {
        for x in 0..8 {
            let px = if x < 4 {
                [0.0, 1.0, 0.0, 1.0]
            } else {
                [1.0, 0.0, 0.0, 1.0]
            };
            for &c in &px {
                buf.extend_from_slice(&half::f16::from_f32(c).to_le_bytes());
            }
        }
    }
    gpu.upload_layer(&queue, l0, &buf);

    // Stamp the right half sampling 4px to the left (green source).
    let dab = Dab {
        center: [6.0, 4.0],
        radius: 2.0,
        hardness: 0.99,
        color: [0.0, 0.0, 0.0, 1.0],
    };
    let mut enc = device.create_command_encoder(&Default::default());
    gpu.snapshot_clone_source(&device, &mut enc, l0);
    gpu.paint_clone_dabs(&device, &queue, &mut enc, l0, &[dab], [4.0, 0.0]);
    queue.submit([enc.finish()]);

    let px = gpu.read_pixel(&device, &queue, l0, 6, 4).unwrap();
    assert!(
        px[1] > 0.8 && px[0] < 0.2,
        "clone stamped green over red: {px:?}"
    );
}
