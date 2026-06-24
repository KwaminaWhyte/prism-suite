use super::*;

// Outer-stroke layer style: a small opaque square gets a red stroke ring just
// outside its edge; far pixels stay empty, the interior stays white.
#[test]
fn layer_stroke_outlines_edge() {
    let Some((device, queue)) = device() else {
        eprintln!("no GPU adapter; skipping layer_stroke_outlines_edge");
        return;
    };
    let mut gpu = CanvasGpu::new(&device, wgpu::TextureFormat::Bgra8Unorm);
    let size = Size::new(16, 16);
    gpu.ensure_canvas(&device, size);
    let l0 = LayerId(0);
    gpu.ensure_layer(&device, l0);
    // 4x4 white opaque square at x6..10, y6..10; rest transparent.
    let mut buf = Vec::new();
    for y in 0..16 {
        for x in 0..16 {
            let px = if (6..10).contains(&x) && (6..10).contains(&y) {
                [1.0, 1.0, 1.0, 1.0]
            } else {
                [0.0, 0.0, 0.0, 0.0]
            };
            for &c in &px {
                buf.extend_from_slice(&half::f16::from_f32(c).to_le_bytes());
            }
        }
    }
    gpu.upload_layer(&queue, l0, &buf);

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
        has_stroke: true,
        stroke_color: [1.0, 0.0, 0.0, 1.0], // red
        stroke_width: 2.0 / 16.0,           // ~2px in uv
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
    let pp = gpu.composite_now(&device, &queue, &order);
    let edge = gpu.read_composite_pixel(&device, &queue, pp, 5, 8).unwrap(); // just left of square
    let far = gpu.read_composite_pixel(&device, &queue, pp, 0, 0).unwrap();
    let inside = gpu.read_composite_pixel(&device, &queue, pp, 7, 7).unwrap();
    assert!(
        edge[0] > 0.5 && edge[1] < 0.3 && edge[3] > 0.3,
        "red stroke just outside the edge: {edge:?}"
    );
    assert!(far[3] < 0.1, "far pixel stays empty: {far:?}");
    assert!(
        inside[0] > 0.8 && inside[1] > 0.8,
        "interior stays white: {inside:?}"
    );
}


// Drop shadow: an opaque square casts a dark, offset shadow down-right; the
// shadow region (outside the square) is dark and semi-opaque, far stays empty.
#[test]
fn layer_drop_shadow_offsets_behind() {
    let Some((device, queue)) = device() else {
        eprintln!("no GPU adapter; skipping layer_drop_shadow_offsets_behind");
        return;
    };
    let mut gpu = CanvasGpu::new(&device, wgpu::TextureFormat::Bgra8Unorm);
    let size = Size::new(16, 16);
    gpu.ensure_canvas(&device, size);
    let l0 = LayerId(0);
    gpu.ensure_layer(&device, l0);
    let mut buf = Vec::new();
    for y in 0..16 {
        for x in 0..16 {
            let px = if (6..10).contains(&x) && (6..10).contains(&y) {
                [1.0, 1.0, 1.0, 1.0]
            } else {
                [0.0, 0.0, 0.0, 0.0]
            };
            for &c in &px {
                buf.extend_from_slice(&half::f16::from_f32(c).to_le_bytes());
            }
        }
    }
    gpu.upload_layer(&queue, l0, &buf);

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
        has_shadow: true,
        shadow_color: [0.0, 0.0, 0.0, 0.8], // black, mostly opaque
        shadow_offset: [4.0 / 16.0, 4.0 / 16.0], // down-right ~4px
        shadow_blur: 2.0 / 16.0,
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
    let pp = gpu.composite_now(&device, &queue, &order);
    // (12,12): square shifted +4 lands here, outside the square itself.
    let shadow = gpu
        .read_composite_pixel(&device, &queue, pp, 12, 12)
        .unwrap();
    let far = gpu.read_composite_pixel(&device, &queue, pp, 1, 1).unwrap();
    assert!(
        shadow[3] > 0.2 && shadow[0] < 0.3,
        "dark offset shadow present: {shadow:?}"
    );
    assert!(far[3] < 0.1, "far corner stays empty: {far:?}");
}


// Color overlay: a white layer with a full-strength red overlay composites red.
#[test]
fn layer_color_overlay_recolors() {
    let Some((device, queue)) = device() else {
        eprintln!("no GPU adapter; skipping layer_color_overlay_recolors");
        return;
    };
    let mut gpu = CanvasGpu::new(&device, wgpu::TextureFormat::Bgra8Unorm);
    let size = Size::new(8, 8);
    gpu.ensure_canvas(&device, size);
    let l0 = LayerId(0);
    gpu.ensure_layer(&device, l0);
    gpu.upload_layer(&queue, l0, &solid(8, 1.0, 1.0, 1.0, 1.0)); // white

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
        has_overlay: true,
        overlay_color: [1.0, 0.0, 0.0, 1.0], // full red
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
    let pp = gpu.composite_now(&device, &queue, &order);
    let px = gpu.read_composite_pixel(&device, &queue, pp, 4, 4).unwrap();
    assert!(
        px[0] > 0.8 && px[1] < 0.2 && px[2] < 0.2,
        "white layer recolored to red overlay: {px:?}"
    );
}


// A layer mask (0 on the left half) hides those pixels in the composite.
#[test]
fn layer_mask_hides() {
    let Some((device, queue)) = device() else {
        eprintln!("no GPU adapter; skipping layer_mask_hides");
        return;
    };
    let mut gpu = CanvasGpu::new(&device, wgpu::TextureFormat::Bgra8Unorm);
    let size = Size::new(8, 8);
    gpu.ensure_canvas(&device, size);
    let l0 = LayerId(0);
    gpu.ensure_layer(&device, l0);
    gpu.upload_layer(&queue, l0, &solid(8, 1.0, 0.0, 0.0, 1.0)); // red
    let mut mvals = vec![0.0f32; 64];
    for y in 0..8 {
        for x in 4..8 {
            mvals[y * 8 + x] = 1.0; // reveal right half
        }
    }
    gpu.set_mask(&device, &queue, l0, Some(&mvals));
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
    assert!(left[3] < 0.1, "masked-out left is transparent: {left:?}");
    assert!(
        right[0] > 0.9 && right[3] > 0.9,
        "revealed right is red: {right:?}"
    );
}


// Inner shadow: an opaque white square gets a dark band INSIDE its edge on the
// offset side; the square's coverage never extends past its own bounds.
#[test]
fn layer_inner_shadow_darkens_inside_edge() {
    let Some((device, queue)) = device() else {
        eprintln!("no GPU adapter; skipping layer_inner_shadow_darkens_inside_edge");
        return;
    };
    let mut gpu = CanvasGpu::new(&device, wgpu::TextureFormat::Bgra8Unorm);
    let size = Size::new(16, 16);
    gpu.ensure_canvas(&device, size);
    let l0 = LayerId(0);
    gpu.ensure_layer(&device, l0);
    gpu.upload_layer(&queue, l0, &square_16());

    let order = vec![LayerDraw {
        has_inner_shadow: true,
        inner_shadow_color: [0.0, 0.0, 0.0, 0.9], // dark
        inner_shadow_offset: [3.0 / 16.0, 3.0 / 16.0], // down-right
        inner_shadow_blur: 1.0 / 16.0,
        ..base_draw(l0)
    }];
    let pp = gpu.composite_now(&device, &queue, &order);
    // (6,6): the top-left interior corner — the inverse-alpha cast from up-left
    // lands here, so this pixel is darkened relative to plain white.
    let near = gpu.read_composite_pixel(&device, &queue, pp, 6, 6).unwrap();
    // (9,9): the down-right interior corner — away from the cast, stays bright.
    let far = gpu.read_composite_pixel(&device, &queue, pp, 9, 9).unwrap();
    let outside = gpu.read_composite_pixel(&device, &queue, pp, 2, 2).unwrap();
    assert!(
        near[3] > 0.9 && near[0] < far[0],
        "inside edge darkened by inner shadow: near={near:?} far={far:?}"
    );
    assert!(
        outside[3] < 0.1,
        "inner shadow never paints outside the shape: {outside:?}"
    );
}


// Outer glow: an opaque square emits a colored halo just OUTSIDE its edge that
// fades with distance; far corners stay empty and the interior is unchanged.
#[test]
fn layer_outer_glow_halos_outside_edge() {
    let Some((device, queue)) = device() else {
        eprintln!("no GPU adapter; skipping layer_outer_glow_halos_outside_edge");
        return;
    };
    let mut gpu = CanvasGpu::new(&device, wgpu::TextureFormat::Bgra8Unorm);
    let size = Size::new(16, 16);
    gpu.ensure_canvas(&device, size);
    let l0 = LayerId(0);
    gpu.ensure_layer(&device, l0);
    gpu.upload_layer(&queue, l0, &square_16());

    let order = vec![LayerDraw {
        has_outer_glow: true,
        outer_glow_color: [0.0, 1.0, 0.0, 1.0], // green
        outer_glow_size: 3.0 / 16.0,
        ..base_draw(l0)
    }];
    let pp = gpu.composite_now(&device, &queue, &order);
    let edge = gpu.read_composite_pixel(&device, &queue, pp, 5, 8).unwrap(); // just left of square
    let far = gpu.read_composite_pixel(&device, &queue, pp, 0, 0).unwrap();
    let inside = gpu.read_composite_pixel(&device, &queue, pp, 7, 7).unwrap();
    assert!(
        edge[1] > 0.15 && edge[3] > 0.1,
        "green glow just outside the edge: {edge:?}"
    );
    assert!(far[3] < 0.15, "far corner barely lit: {far:?}");
    assert!(
        inside[0] > 0.8 && inside[1] > 0.8 && inside[2] > 0.8,
        "interior stays white: {inside:?}"
    );
}


// Inner glow: an opaque square gets a colored glow INSIDE its edge that fades
// toward the center; coverage never extends past the shape's own bounds.
#[test]
fn layer_inner_glow_lights_inside_edge() {
    let Some((device, queue)) = device() else {
        eprintln!("no GPU adapter; skipping layer_inner_glow_lights_inside_edge");
        return;
    };
    let mut gpu = CanvasGpu::new(&device, wgpu::TextureFormat::Bgra8Unorm);
    let size = Size::new(16, 16);
    gpu.ensure_canvas(&device, size);
    let l0 = LayerId(0);
    gpu.ensure_layer(&device, l0);
    gpu.upload_layer(&queue, l0, &square_16());

    let order = vec![LayerDraw {
        has_inner_glow: true,
        inner_glow_color: [0.0, 0.0, 1.0, 1.0], // blue
        inner_glow_size: 2.0 / 16.0,
        ..base_draw(l0)
    }];
    let pp = gpu.composite_now(&device, &queue, &order);
    // (6,6): an interior edge pixel — picks up the blue glow.
    let edge = gpu.read_composite_pixel(&device, &queue, pp, 6, 6).unwrap();
    let outside = gpu.read_composite_pixel(&device, &queue, pp, 2, 2).unwrap();
    assert!(
        edge[2] > 0.3 && edge[3] > 0.9,
        "inner edge tinted blue by inner glow: {edge:?}"
    );
    assert!(
        outside[3] < 0.1,
        "inner glow never paints outside the shape: {outside:?}"
    );
}


// Gradient overlay: a full-opacity black->white horizontal gradient over a white
// square reads dark on the left and bright on the right.
#[test]
fn layer_gradient_overlay_ramps_across() {
    let Some((device, queue)) = device() else {
        eprintln!("no GPU adapter; skipping layer_gradient_overlay_ramps_across");
        return;
    };
    let mut gpu = CanvasGpu::new(&device, wgpu::TextureFormat::Bgra8Unorm);
    let size = Size::new(16, 16);
    gpu.ensure_canvas(&device, size);
    let l0 = LayerId(0);
    gpu.ensure_layer(&device, l0);
    // Full white opaque canvas so the gradient is sampled everywhere.
    gpu.upload_layer(&queue, l0, &solid(16, 1.0, 1.0, 1.0, 1.0));

    let order = vec![LayerDraw {
        has_grad_overlay: true,
        grad_color0: [0.0, 0.0, 0.0, 1.0], // black at t=0 (left)
        grad_color1: [1.0, 1.0, 1.0, 1.0], // white at t=1 (right)
        grad_angle: 0.0,                    // along +x
        grad_opacity: 1.0,
        ..base_draw(l0)
    }];
    let pp = gpu.composite_now(&device, &queue, &order);
    let left = gpu.read_composite_pixel(&device, &queue, pp, 1, 8).unwrap();
    let right = gpu.read_composite_pixel(&device, &queue, pp, 14, 8).unwrap();
    assert!(
        left[0] < right[0] && left[0] < 0.3,
        "gradient overlay dark on the left: left={left:?} right={right:?}"
    );
    assert!(right[0] > 0.7, "gradient overlay bright on the right: {right:?}");
}


// Bevel & Emboss (Inner Bevel): a gray square lit from the +x side (angle 0)
// reads a bright highlight on its light-facing (right) edge and a dark shadow on
// the opposite (left) edge, while the flat interior is unchanged.
#[test]
fn layer_bevel_lights_facing_edge() {
    let Some((device, queue)) = device() else {
        eprintln!("no GPU adapter; skipping layer_bevel_lights_facing_edge");
        return;
    };
    let mut gpu = CanvasGpu::new(&device, wgpu::TextureFormat::Bgra8Unorm);
    let size = Size::new(16, 16);
    gpu.ensure_canvas(&device, size);
    let l0 = LayerId(0);
    gpu.ensure_layer(&device, l0);
    // A mid-gray 8x8 square at x4..12, y4..12 so the bevel band has room to read.
    let mut buf = Vec::new();
    for y in 0..16 {
        for x in 0..16 {
            let px = if (4..12).contains(&x) && (4..12).contains(&y) {
                [0.5f32, 0.5, 0.5, 1.0]
            } else {
                [0.0, 0.0, 0.0, 0.0]
            };
            for &c in &px {
                buf.extend_from_slice(&half::f16::from_f32(c).to_le_bytes());
            }
        }
    }
    gpu.upload_layer(&queue, l0, &buf);

    let order = vec![LayerDraw {
        has_bevel: true,
        bevel_highlight: [1.0, 1.0, 1.0, 1.0], // white highlight
        bevel_shadow: [0.0, 0.0, 0.0, 1.0],    // black shadow
        bevel_size: 2.0 / 16.0,
        bevel_soften: 0.0,
        bevel_angle: 0.0,                      // light from +x (right)
        bevel_altitude: std::f32::consts::FRAC_PI_6, // 30°
        ..base_draw(l0)
    }];
    let pp = gpu.composite_now(&device, &queue, &order);
    // Interior edge pixels just inside the left and right borders, mid-height.
    let right = gpu.read_composite_pixel(&device, &queue, pp, 10, 8).unwrap();
    let left = gpu.read_composite_pixel(&device, &queue, pp, 5, 8).unwrap();
    let core = gpu.read_composite_pixel(&device, &queue, pp, 8, 8).unwrap();
    let outside = gpu.read_composite_pixel(&device, &queue, pp, 1, 1).unwrap();
    assert!(
        right[0] > core[0] && right[0] > left[0],
        "light-facing (right) edge brightened: right={right:?} core={core:?} left={left:?}"
    );
    assert!(
        left[0] < core[0],
        "shadowed (left) edge darkened: left={left:?} core={core:?}"
    );
    assert!(
        outside[3] < 0.1,
        "bevel never paints outside the shape: {outside:?}"
    );
}
