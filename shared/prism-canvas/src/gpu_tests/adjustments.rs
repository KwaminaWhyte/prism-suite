use super::*;

// An Invert adjustment layer over a red layer yields cyan in the composite.
#[test]
fn adjustment_invert() {
    let Some((device, queue)) = device() else {
        eprintln!("no GPU adapter; skipping adjustment_invert");
        return;
    };
    let mut gpu = CanvasGpu::new(&device, wgpu::TextureFormat::Bgra8Unorm);
    let size = Size::new(8, 8);
    gpu.ensure_canvas(&device, size);
    let l0 = LayerId(0);
    let l1 = LayerId(1);
    gpu.ensure_layer(&device, l0);
    gpu.ensure_layer(&device, l1); // adjustment layer (no pixels needed)
    gpu.upload_layer(&queue, l0, &solid(8, 1.0, 0.0, 0.0, 1.0)); // red

    let (k, p) = prism_core::adjust::Adjustment::Invert.encode();
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
            adjust_kind: k,
            adjust: p,
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
    ];
    let pp = gpu.composite_now(&device, &queue, &order);
    let px = gpu.read_composite_pixel(&device, &queue, pp, 4, 4).unwrap();
    // Inverted red (premultiplied, alpha 1): low red, high green+blue.
    assert!(
        px[0] < 0.2 && px[1] > 0.8 && px[2] > 0.8,
        "invert red -> cyan: {px:?}"
    );
}

// A Curves adjustment layer with an inverting master curve turns red -> cyan,
// exercising the full LUT build + upload + shader-sample path.
#[test]
fn curves_invert_master() {
    let Some((device, queue)) = device() else {
        eprintln!("no GPU adapter; skipping curves_invert_master");
        return;
    };
    let mut gpu = CanvasGpu::new(&device, wgpu::TextureFormat::Bgra8Unorm);
    let size = Size::new(8, 8);
    gpu.ensure_canvas(&device, size);
    let l0 = LayerId(0);
    let l1 = LayerId(1);
    gpu.ensure_layer(&device, l0);
    gpu.ensure_layer(&device, l1); // adjustment layer (no pixels needed)
    gpu.upload_layer(&queue, l0, &solid(8, 1.0, 0.0, 0.0, 1.0)); // red

    // Master curve inverts (0->1, 1->0); per-channel curves stay identity.
    let inv = [(0.0, 1.0), (1.0, 0.0)];
    let idc = [(0.0, 0.0), (1.0, 1.0)];
    gpu.set_curve_lut(&device, &queue, l1, &inv, &idc, &idc, &idc);

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
            adjust_kind: 8, // Curves
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
    ];
    let pp = gpu.composite_now(&device, &queue, &order);
    let px = gpu.read_composite_pixel(&device, &queue, pp, 4, 4).unwrap();
    assert!(
        px[0] < 0.2 && px[1] > 0.8 && px[2] > 0.8,
        "curves invert red -> cyan: {px:?}"
    );
}

// A Posterize(2) adjustment over a mid-gray layer snaps it to white (the
// sRGB-space value rounds up at 2 levels) — exercises a new adjustment kind.
#[test]
fn posterize_adjustment() {
    let Some((device, queue)) = device() else {
        eprintln!("no GPU adapter; skipping posterize_adjustment");
        return;
    };
    let mut gpu = CanvasGpu::new(&device, wgpu::TextureFormat::Bgra8Unorm);
    let size = Size::new(8, 8);
    gpu.ensure_canvas(&device, size);
    let l0 = LayerId(0);
    let l1 = LayerId(1);
    gpu.ensure_layer(&device, l0);
    gpu.ensure_layer(&device, l1);
    gpu.upload_layer(&queue, l0, &solid(8, 0.6, 0.6, 0.6, 1.0)); // mid gray

    let (k, p) = prism_core::adjust::Adjustment::Posterize { levels: 2 }.encode();
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
            adjust_kind: k,
            adjust: p,
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
    ];
    let pp = gpu.composite_now(&device, &queue, &order);
    let px = gpu.read_composite_pixel(&device, &queue, pp, 4, 4).unwrap();
    assert!(
        px[0] > 0.9,
        "posterize(2) snaps mid gray up to white: {px:?}"
    );
}

// Gradient Map (red shadows -> blue highlights) over a white backdrop maps the
// high luminance to blue.
#[test]
fn gradient_map_adjustment() {
    let Some((device, queue)) = device() else {
        eprintln!("no GPU adapter; skipping gradient_map_adjustment");
        return;
    };
    let mut gpu = CanvasGpu::new(&device, wgpu::TextureFormat::Bgra8Unorm);
    let size = Size::new(8, 8);
    gpu.ensure_canvas(&device, size);
    let l0 = LayerId(0);
    let l1 = LayerId(1);
    gpu.ensure_layer(&device, l0);
    gpu.ensure_layer(&device, l1);
    gpu.upload_layer(&queue, l0, &solid(8, 1.0, 1.0, 1.0, 1.0)); // white backdrop
    gpu.set_gradient_lut(&device, &queue, l1, [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]);

    let (k, p) = prism_core::adjust::Adjustment::GradientMap {
        low: [1.0, 0.0, 0.0],
        high: [0.0, 0.0, 1.0],
    }
    .encode();
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
            adjust_kind: k,
            adjust: p,
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
    ];
    let pp = gpu.composite_now(&device, &queue, &order);
    let px = gpu.read_composite_pixel(&device, &queue, pp, 4, 4).unwrap();
    assert!(
        px[2] > 0.8 && px[0] < 0.2,
        "white backdrop maps to the highlight color (blue): {px:?}"
    );
}

// Color Balance: a strong red shadow shift over a dark-gray backdrop lifts the
// red channel (a per-channel transfer LUT, shader kind 13), leaving the brighter
// channels comparatively untouched.
#[test]
fn color_balance_shadow_red_push() {
    let Some((device, queue)) = device() else {
        eprintln!("no GPU adapter; skipping color_balance_shadow_red_push");
        return;
    };
    let mut gpu = CanvasGpu::new(&device, wgpu::TextureFormat::Bgra8Unorm);
    let size = Size::new(8, 8);
    gpu.ensure_canvas(&device, size);
    let l0 = LayerId(0);
    let l1 = LayerId(1);
    gpu.ensure_layer(&device, l0);
    gpu.ensure_layer(&device, l1);
    // Dark gray backdrop so the shadow range dominates the LUT weighting.
    gpu.upload_layer(&queue, l0, &solid(8, 0.15, 0.15, 0.15, 1.0));
    gpu.set_color_balance_lut(&device, &queue, l1, [1.0, 0.0, 0.0], [0.0; 3], [0.0; 3]);

    let (k, p) = prism_core::adjust::Adjustment::ColorBalance {
        shadows: [1.0, 0.0, 0.0],
        midtones: [0.0; 3],
        highlights: [0.0; 3],
        preserve_luminosity: false,
    }
    .encode();
    let order = vec![
        base_draw(l0),
        LayerDraw {
            adjust_kind: k,
            adjust: p,
            ..base_draw(l1)
        },
    ];
    let pp = gpu.composite_now(&device, &queue, &order);
    let px = gpu.read_composite_pixel(&device, &queue, pp, 4, 4).unwrap();
    // Composite is linear-premultiplied; 0.15 sRGB ≈ 0.0185 linear. Red must end
    // up clearly above the (unshifted) green/blue.
    assert!(
        px[0] > px[1] + 0.05 && px[0] > px[2] + 0.05,
        "shadow red push lifts the red channel above green/blue: {px:?}"
    );
}


// Channel Mixer: a red↔blue swap matrix (output R = input B, output B = input R)
// over a pure-red layer yields blue (shader kind 14).
#[test]
fn channel_mixer_swaps_red_blue() {
    let Some((device, queue)) = device() else {
        eprintln!("no GPU adapter; skipping channel_mixer_swaps_red_blue");
        return;
    };
    let mut gpu = CanvasGpu::new(&device, wgpu::TextureFormat::Bgra8Unorm);
    let size = Size::new(8, 8);
    gpu.ensure_canvas(&device, size);
    let l0 = LayerId(0);
    let l1 = LayerId(1);
    gpu.ensure_layer(&device, l0);
    gpu.ensure_layer(&device, l1);
    gpu.upload_layer(&queue, l0, &solid(8, 1.0, 0.0, 0.0, 1.0)); // pure red

    let adj = prism_core::adjust::Adjustment::ChannelMixer {
        r: [0.0, 0.0, 1.0, 0.0], // out R = in B
        g: [0.0, 1.0, 0.0, 0.0], // out G = in G
        b: [1.0, 0.0, 0.0, 0.0], // out B = in R
        monochrome: false,
    };
    let (k, p) = adj.encode();
    let m = adj.channel_mixer_matrix().unwrap();
    let order = vec![
        base_draw(l0),
        LayerDraw {
            adjust_kind: k,
            adjust: p,
            mix_r: m.r,
            mix_g: m.g,
            mix_b: m.b,
            ..base_draw(l1)
        },
    ];
    let pp = gpu.composite_now(&device, &queue, &order);
    let px = gpu.read_composite_pixel(&device, &queue, pp, 4, 4).unwrap();
    assert!(
        px[2] > 0.8 && px[0] < 0.2,
        "red/blue swap turns red into blue: {px:?}"
    );
}


// The gradient-fill path exactly as `do_gradient` runs it on the GPU: upload a
// solid layer, read it back, render a multi-stop gradient (shared
// `prism_core::gradient`) over it source-over, upload, then read pixels back.
// Asserts the gradient took effect (left vs right of a linear black→white ramp)
// and that the f16 layer round-trip preserves it. Skips if no GPU adapter.
#[test]
fn gradient_fill_writes_layer() {
    let Some((device, queue)) = device() else {
        eprintln!("no GPU adapter; skipping gradient_fill_writes_layer");
        return;
    };
    let mut gpu = CanvasGpu::new(&device, wgpu::TextureFormat::Bgra8Unorm);
    let size = Size::new(16, 4);
    gpu.ensure_canvas(&device, size);
    let l0 = LayerId(0);
    gpu.ensure_layer(&device, l0);
    // Start fully transparent so the opaque gradient is what we read back.
    gpu.upload_layer(&queue, l0, &solid(16, 0.0, 0.0, 0.0, 0.0));

    // Black→white opaque linear gradient across the 16px width, dither off so
    // the assertion is exact.
    let grad = prism_core::gradient::Gradient::two_color(
        [0.0, 0.0, 0.0],
        [1.0, 1.0, 1.0],
        prism_core::gradient::GradientType::Linear,
    );
    let grad = prism_core::gradient::Gradient {
        dither: false,
        ..grad
    };
    let g = grad.render((0.0, 0.0), (16.0, 0.0), 16, 4); // premultiplied linear

    // Read the (transparent) layer back, composite the gradient source-over.
    let bytes = gpu.read_layer(&device, &queue, l0).unwrap();
    let mut base: Vec<f32> = bytes
        .chunks_exact(2)
        .map(|b| half::f16::from_le_bytes([b[0], b[1]]).to_f32())
        .collect();
    for i in 0..(16 * 4) {
        let ga = g[i * 4 + 3];
        for c in 0..4 {
            base[i * 4 + c] = g[i * 4 + c] + base[i * 4 + c] * (1.0 - ga);
        }
    }
    let mut out = Vec::with_capacity(base.len() * 2);
    for &c in &base {
        out.extend_from_slice(&half::f16::from_f32(c).to_le_bytes());
    }
    gpu.upload_layer(&queue, l0, &out);

    // Leftmost is ~black, rightmost ~white; both opaque.
    let left = gpu.read_pixel(&device, &queue, l0, 0, 2).unwrap();
    let right = gpu.read_pixel(&device, &queue, l0, 15, 2).unwrap();
    assert!(left[3] > 0.9 && right[3] > 0.9, "opaque: {left:?} {right:?}");
    assert!(left[0] < 0.2, "left ~black, got {left:?}");
    assert!(right[0] > 0.8, "right ~white, got {right:?}");
    assert!(right[0] > left[0] + 0.5, "ramp increases L→R");
}
