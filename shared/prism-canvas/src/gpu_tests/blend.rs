use super::*;

// Blend-If: a gray top layer with "this layer white" pulled below its luma is
// hidden, revealing the white layer beneath.
#[test]
fn blend_if_hides_bright_source() {
    let Some((device, queue)) = device() else {
        eprintln!("no GPU adapter; skipping blend_if_hides_bright_source");
        return;
    };
    let mut gpu = CanvasGpu::new(&device, wgpu::TextureFormat::Bgra8Unorm);
    let size = Size::new(8, 8);
    gpu.ensure_canvas(&device, size);
    let l0 = LayerId(0);
    let l1 = LayerId(1);
    gpu.ensure_layer(&device, l0);
    gpu.ensure_layer(&device, l1);
    gpu.upload_layer(&queue, l0, &solid(8, 1.0, 1.0, 1.0, 1.0)); // white base
    gpu.upload_layer(&queue, l1, &solid(8, 0.5, 0.5, 0.5, 1.0)); // gray top

    // this_white = 0.45 < gray luma 0.5 -> top fully gated out.
    let order = vec![
        LayerDraw {
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
        },
        LayerDraw {
            id: l1,
            opacity: 1.0,
            blend: 0,
            visible: true,
            adjust_kind: 0,
            adjust: [0.0; 4],
            mix_r: [1.0, 0.0, 0.0, 0.0],
            mix_g: [0.0, 1.0, 0.0, 0.0],
            mix_b: [0.0, 0.0, 1.0, 0.0],
            has_blend_if: true,
            blend_if: [0.0, 0.45, 0.0, 1.0],
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
        },
    ];
    let pp = gpu.composite_now(&device, &queue, &order);
    let px = gpu.read_composite_pixel(&device, &queue, pp, 4, 4).unwrap();
    assert!(
        px[0] > 0.9,
        "blend-if hides gray top -> white shows through: {px:?}"
    );
}


// Clipping mask: a green top layer clipped to a base that's opaque on the left
// and transparent on the right shows green only on the left.
#[test]
fn clipping_mask_gates_by_base_alpha() {
    let Some((device, queue)) = device() else {
        eprintln!("no GPU adapter; skipping clipping_mask_gates_by_base_alpha");
        return;
    };
    let mut gpu = CanvasGpu::new(&device, wgpu::TextureFormat::Bgra8Unorm);
    let size = Size::new(8, 8);
    gpu.ensure_canvas(&device, size);
    let l0 = LayerId(0);
    let l1 = LayerId(1);
    gpu.ensure_layer(&device, l0);
    gpu.ensure_layer(&device, l1);
    // Base: left half opaque red, right half transparent.
    let mut base = Vec::new();
    for _y in 0..8 {
        for x in 0..8 {
            let px = if x < 4 {
                [1.0, 0.0, 0.0, 1.0]
            } else {
                [0.0, 0.0, 0.0, 0.0]
            };
            for &c in &px {
                base.extend_from_slice(&half::f16::from_f32(c).to_le_bytes());
            }
        }
    }
    gpu.upload_layer(&queue, l0, &base);
    gpu.upload_layer(&queue, l1, &solid(8, 0.0, 1.0, 0.0, 1.0)); // green top

    let order = vec![
        LayerDraw {
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
        },
        LayerDraw {
            id: l1,
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
            clipped: true,
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
        },
    ];
    let pp = gpu.composite_now(&device, &queue, &order);
    let left = gpu.read_composite_pixel(&device, &queue, pp, 2, 4).unwrap();
    let right = gpu.read_composite_pixel(&device, &queue, pp, 6, 4).unwrap();
    assert!(left[1] > 0.8, "green shows over opaque base: {left:?}");
    assert!(
        right[3] < 0.1,
        "clipped out where base is transparent: {right:?}"
    );
}


// Channels: save a left-half selection, clear it, reload from the channel, then
// paint — the restored selection clips the paint to the left half.
#[test]
fn channel_save_load_roundtrip() {
    let Some((device, queue)) = device() else {
        eprintln!("no GPU adapter; skipping channel_save_load_roundtrip");
        return;
    };
    let mut gpu = CanvasGpu::new(&device, wgpu::TextureFormat::Bgra8Unorm);
    let size = Size::new(8, 8);
    gpu.ensure_canvas(&device, size);
    let l0 = LayerId(0);
    gpu.ensure_layer(&device, l0);
    gpu.upload_layer(&queue, l0, &solid(8, 1.0, 0.0, 0.0, 1.0)); // red

    // Select the left half and save it as a channel.
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
    queue.submit([enc.finish()]);
    gpu.save_selection_as_channel(&device, &queue, "a".to_string());

    // Clear the selection, then reload it from the channel.
    let mut enc = device.create_command_encoder(&Default::default());
    gpu.apply_selection(&device, &queue, &mut enc, &SelectionOp::None);
    queue.submit([enc.finish()]);
    gpu.load_channel(&device, &queue, "a");
    assert!(gpu.has_selection(), "selection restored from channel");

    // Paint blue over the whole canvas; the restored selection clips it left.
    let dab = Dab {
        center: [4.0, 4.0],
        radius: 16.0,
        hardness: 0.99,
        color: [0.0, 0.0, 1.0, 1.0],
    };
    let mut enc = device.create_command_encoder(&Default::default());
    gpu.paint_dabs(&device, &queue, &mut enc, l0, &[dab], false, false, false);
    queue.submit([enc.finish()]);

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
    let left = gpu.read_composite_pixel(&device, &queue, p, 1, 4).unwrap();
    let right = gpu.read_composite_pixel(&device, &queue, p, 6, 4).unwrap();
    assert!(
        left[2] > 0.5,
        "restored selection paints left blue: {left:?}"
    );
    assert!(
        right[0] > 0.5 && right[2] < 0.2,
        "right half untouched red: {right:?}"
    );
}
