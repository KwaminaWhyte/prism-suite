//! The per-layer **finishing** passes extracted from `render/passes.rs`
//! (workspace size rule): adjustment-layer regrade, track-matte application, mask
//! carving, and the key / spatial / depth-of-field / stylize / distort / puppet
//! whole-buffer bridges. The content-compositing passes they build on live in the
//! sibling [`composite`](super::composite) module. Behaviour is unchanged — these
//! are the exact functions `passes.rs` defined inline, re-exported via
//! `passes/mod.rs` so `render/mod.rs` imports them as before.

use super::composite::{
    composite_footage, composite_layer, composite_precomp, composite_shape, composite_text,
    decode_footage,
};
use crate::comp::{
    apply_distort_effects, apply_effects_masked, apply_key_effects, apply_spatial_effects,
    apply_stylize_effects, gaussian_blur, mask_stack_coverage, Affine2, Comp, MatteMode, PulseLayer,
};
use crate::render::{over, Geom, Lin, RenderCtx};

/// Apply an **adjustment layer**'s effect stack to the composite beneath it,
/// within the layer's transformed quad.
///
/// Unlike a solid (a constant-color source), an adjustment re-grades whatever is
/// already in the accumulator: for each covered pixel we run the effect stack on
/// the existing linear-light straight RGBA and write the result back. Coverage is
/// the same inverse-mapped quad test the solid path uses; the layer's `opacity`
/// blends the regraded result against the original so a partly-opaque adjustment
/// is a partial grade. An empty effect stack is a no-op.
pub(crate) fn apply_adjustment(
    acc: &mut [Lin],
    geom: &Geom,
    world: Affine2,
    layer: &PulseLayer,
    opacity: f32,
) {
    let &Geom {
        w,
        cx,
        cy,
        half_w,
        half_h,
        ..
    } = geom;
    if layer.effects.is_empty() {
        return;
    }
    let mix = opacity.clamp(0.0, 1.0);
    if mix <= 0.0 {
        return;
    }
    let Some(inv) = world.inverse() else {
        return;
    };
    let Some((x0, x1, y0, y1)) = geom.quad_bounds(world) else {
        return;
    };
    // Effect mask: limits the regrade to its (feathered) region within the quad.
    let fx_poly = layer.effect_mask_poly();

    for py in y0..=y1 {
        let comp_y = py as f32 + 0.5 - cy;
        for px in x0..=x1 {
            let comp_x = px as f32 + 0.5 - cx;
            let (lx, ly) = inv.apply(comp_x, comp_y);
            if lx.abs() > half_w || ly.abs() > half_h {
                continue;
            }
            let idx = (py as u32 * w + px as u32) as usize;
            let src = acc[idx];
            // Nothing underneath here — grading transparent pixels would lift
            // their (invisible) color into the buffer for no reason. Skip them.
            if src.a <= 0.0 {
                continue;
            }
            let graded = apply_effects_masked(
                &layer.effects,
                &layer.effect_mask,
                &fx_poly,
                lx,
                ly,
                [src.r, src.g, src.b, src.a],
            );
            // Blend the regrade against the original by the adjustment's opacity.
            acc[idx] = Lin {
                r: src.r + (graded[0] - src.r) * mix,
                g: src.g + (graded[1] - src.g) * mix,
                b: src.b + (graded[2] - src.b) * mix,
                a: src.a, // alpha is untouched by color grading
            };
        }
    }
}

/// Modulate an already-rendered layer buffer's alpha by a **track matte**.
///
/// `layer_buf` holds the matted layer's isolated straight-linear RGBA. The matte
/// source (`src_idx`) is rasterized per-pixel into the same comp space; each
/// matte pixel yields a factor in `[0, 1]` (alpha/luma, optionally inverted) that
/// multiplies the corresponding `layer_buf` pixel's alpha. Color is untouched —
/// only coverage changes — so the subsequent source-over honors the matte. The
/// matte source's own transform / parent chain / effects are respected.
#[allow(clippy::too_many_arguments)]
pub(crate) fn apply_track_matte(
    layer_buf: &mut [Lin],
    geom: &Geom,
    comp: &Comp,
    src_idx: usize,
    cache: &mut crate::comp::FrameCache,
    mode: MatteMode,
    t: f32,
    ctx: RenderCtx,
) {
    // Render the matte source into its own isolated buffer (so its alpha/luma is
    // measured in isolation, not on top of anything below it).
    let mut matte = vec![Lin::CLEAR; (geom.w * geom.h) as usize];
    let src_world = comp.world_matrix(src_idx, t);
    let src_op = comp.layer_opacity(src_idx, t);
    if let Some(src_layer) = comp.layers.get(src_idx) {
        if src_layer.has_shape() {
            composite_shape(&mut matte, geom, src_world, src_layer, src_op);
        } else if src_layer.has_text() {
            composite_text(&mut matte, geom, src_world, src_layer, src_op);
        } else if src_layer.has_footage() {
            // A footage matte source honours its own time remap (and frame
            // blending) too.
            let src_t = comp.layer_source_time(src_idx, t);
            if let Some(frame) = decode_footage(cache, src_layer, src_t, comp.fps) {
                composite_footage(&mut matte, geom, src_world, src_layer, &frame, src_op);
            }
        } else if src_layer.has_precomp() {
            let src_t = comp.layer_source_time(src_idx, t);
            composite_precomp(&mut matte, geom, src_world, src_layer, cache, src_t, src_op, ctx);
        } else {
            composite_layer(&mut matte, geom, src_world, src_layer, src_op);
        }
    }
    for (px, m) in matte.iter().enumerate() {
        let f = mode.factor([m.r, m.g, m.b, m.a]);
        layer_buf[px].a *= f;
    }
}

/// Carve a rendered layer buffer's alpha by the layer's **mask stack**.
///
/// `layer_buf` holds the layer's isolated straight-linear RGBA. Each pixel is
/// inverse-mapped through the layer's `world` matrix back into layer-local space
/// (where the masks are authored), and the folded [`mask_stack_coverage`] there
/// multiplies the pixel's alpha — color is untouched, only coverage changes, so
/// the subsequent source-over honors the masks. A singular world matrix (zero
/// scale) leaves nothing to mask. Assumes the layer has at least one active mask
/// (the caller gates on [`PulseLayer::has_active_masks`]).
pub(crate) fn apply_masks(layer_buf: &mut [Lin], geom: &Geom, world: Affine2, layer: &PulseLayer) {
    let &Geom { w, h, cx, cy, .. } = geom;
    let Some(inv) = world.inverse() else {
        // Collapsed transform: no coverage survives.
        for px in layer_buf.iter_mut() {
            px.a = 0.0;
        }
        return;
    };
    // Pre-flatten each mask once so the per-pixel loop is just point tests.
    let polys: Vec<Vec<(f32, f32)>> = layer.masks.iter().map(|m| m.flatten()).collect();
    for py in 0..h {
        let comp_y = py as f32 + 0.5 - cy;
        for px in 0..w {
            let idx = (py * w + px) as usize;
            // Skip already-transparent pixels — masking them is a no-op.
            if layer_buf[idx].a <= 0.0 {
                continue;
            }
            let comp_x = px as f32 + 0.5 - cx;
            let (lx, ly) = inv.apply(comp_x, comp_y);
            let cov = mask_stack_coverage(&layer.masks, &polys, lx, ly);
            layer_buf[idx].a *= cov;
        }
    }
}

/// Run a layer's **key effect stack** (Color / Luma / Chroma Key, Spill
/// Suppression, Matte Choke) over its isolated rendered buffer.
///
/// The twin of [`apply_spatial`]: the [`Lin`] accumulator is already
/// **premultiplied** linear-light — exactly what the keyers operate on (they
/// un-premultiply per pixel to test the straight colour, then re-premultiply by
/// the new coverage) — so this is a zero-conversion bridge: view the `Lin` slice
/// as `[[f32; 4]]`, run [`apply_key_effects`], then write the keyed values back.
/// Runs *before* the spatial passes so a key carves the matte first and a later
/// blur can soften the keyed edge. Assumes the layer has at least one key effect
/// (the caller gates on [`PulseLayer::has_key_effects`]).
pub(crate) fn apply_key(layer_buf: &mut [Lin], geom: &Geom, layer: &PulseLayer) {
    let (w, h) = (geom.w as usize, geom.h as usize);
    let mut rgba: Vec<[f32; 4]> = layer_buf.iter().map(|p| [p.r, p.g, p.b, p.a]).collect();
    apply_key_effects(&layer.key_effects, &mut rgba, w, h);
    for (dst, src) in layer_buf.iter_mut().zip(rgba.iter()) {
        dst.r = src[0];
        dst.g = src[1];
        dst.b = src[2];
        dst.a = src[3];
    }
}

/// Run a layer's **spatial effect stack** (Gaussian Blur / Drop Shadow / Glow)
/// over its isolated rendered buffer.
///
/// The compositor's [`Lin`] accumulator is already **premultiplied** linear-light
/// (RGB = color·coverage, A = coverage) — exactly the representation the spatial
/// passes operate on — so this is a zero-conversion bridge: view the `Lin` slice
/// as `[[f32; 4]]`, run [`apply_spatial_effects`], then write the filtered values
/// back. Assumes the layer has at least one spatial effect (the caller gates on
/// [`PulseLayer::has_spatial_effects`]).
pub(crate) fn apply_spatial(layer_buf: &mut [Lin], geom: &Geom, layer: &PulseLayer) {
    let (w, h) = (geom.w as usize, geom.h as usize);
    let mut rgba: Vec<[f32; 4]> = layer_buf.iter().map(|p| [p.r, p.g, p.b, p.a]).collect();
    apply_spatial_effects(&layer.spatial_effects, &mut rgba, w, h);
    for (dst, src) in layer_buf.iter_mut().zip(rgba.iter()) {
        dst.r = src[0];
        dst.g = src[1];
        dst.b = src[2];
        dst.a = src[3];
    }
}

/// Apply the camera's **depth-of-field** defocus to a 3-D layer's isolated
/// rendered buffer: a symmetric Gaussian blur whose radius is the layer's
/// circle-of-confusion ([`Comp::layer_dof_blur`] → [`Camera::coc_blur_radius`]).
///
/// `radius` is the comp-px circle-of-confusion radius the camera computed for
/// this layer; an in-focus layer (`radius ≈ 0`) is left untouched (no blur
/// allocation or convolution), so a sharp 3-D layer renders byte-identically to
/// the no-DoF path. The radius is mapped to a Gaussian `sigma = radius / 2`
/// (≈ a 95%-energy blur whose visible spread is the CoC diameter) and run on
/// both axes. Off-buffer samples read transparent (no edge clamp) so a defocused
/// layer's soft edges spread into the surrounding frame the way a real lens
/// blurs a bright object against the background. The bridge mirrors
/// [`apply_spatial`].
pub(crate) fn apply_dof(layer_buf: &mut [Lin], geom: &Geom, radius: f32) {
    let sigma = radius * 0.5;
    if sigma <= 0.0 {
        return; // in focus (or degenerate) — leave the buffer exactly as is.
    }
    let (w, h) = (geom.w as usize, geom.h as usize);
    let mut rgba: Vec<[f32; 4]> = layer_buf.iter().map(|p| [p.r, p.g, p.b, p.a]).collect();
    gaussian_blur(&mut rgba, w, h, sigma, sigma, false);
    for (dst, src) in layer_buf.iter_mut().zip(rgba.iter()) {
        dst.r = src[0];
        dst.g = src[1];
        dst.b = src[2];
        dst.a = src[3];
    }
}

/// Run a layer's **stylize effect stack** (Find Edges / Mosaic) over its
/// isolated rendered buffer.
///
/// The twin of [`apply_spatial`]: the [`Lin`] accumulator is already
/// **premultiplied** linear-light — exactly what the look-shaping passes operate
/// on (Find Edges un-premultiplies per pixel to detect edges in the straight
/// colour then re-premultiplies; Mosaic averages the premultiplied values
/// directly) — so this is a zero-conversion bridge: view the `Lin` slice as
/// `[[f32; 4]]`, run [`apply_stylize_effects`], then write the stylized values
/// back. Runs *after* the spatial passes and *before* the distort passes (so a
/// stylize reshapes the blurred/glowed buffer and a later distort warps the
/// result). Assumes the layer has at least one stylize effect (the caller gates on
/// [`PulseLayer::has_stylize_effects`]).
pub(crate) fn apply_stylize(layer_buf: &mut [Lin], geom: &Geom, layer: &PulseLayer) {
    let (w, h) = (geom.w as usize, geom.h as usize);
    let mut rgba: Vec<[f32; 4]> = layer_buf.iter().map(|p| [p.r, p.g, p.b, p.a]).collect();
    apply_stylize_effects(&layer.stylize_effects, &mut rgba, w, h);
    for (dst, src) in layer_buf.iter_mut().zip(rgba.iter()) {
        dst.r = src[0];
        dst.g = src[1];
        dst.b = src[2];
        dst.a = src[3];
    }
}

/// Run a layer's **distort effect stack** (Corner Pin / Transform / Mirror /
/// Polar Coordinates) over its isolated rendered buffer.
///
/// The twin of [`apply_spatial`]: the [`Lin`] accumulator is already
/// **premultiplied** linear-light — exactly what the coordinate-remap resampler
/// operates on (interpolating premultiplied values keeps soft edges clean) — so
/// this is a zero-conversion bridge: view the `Lin` slice as `[[f32; 4]]`, run
/// [`apply_distort_effects`], then write the remapped values back. Assumes the
/// layer has at least one distort effect (the caller gates on
/// [`PulseLayer::has_distort_effects`]).
pub(crate) fn apply_distort(layer_buf: &mut [Lin], geom: &Geom, layer: &PulseLayer) {
    let (w, h) = (geom.w as usize, geom.h as usize);
    let mut rgba: Vec<[f32; 4]> = layer_buf.iter().map(|p| [p.r, p.g, p.b, p.a]).collect();
    apply_distort_effects(&layer.distort_effects, &mut rgba, w, h);
    for (dst, src) in layer_buf.iter_mut().zip(rgba.iter()) {
        dst.r = src[0];
        dst.g = src[1];
        dst.b = src[2];
        dst.a = src[3];
    }
}

/// Apply **puppet warp** inverse-distance-weighted displacement to a layer buffer.
///
/// With fewer than 3 pins: no-op (not enough constraints for a warp).
/// With ≥3 pins: for each pixel, compute IDW displacement from each pin's
/// current position vs the center-of-mass "rest" and warp by shifting the
/// sampling UV by the weighted displacement.
pub(crate) fn apply_puppet_warp(
    buf: &mut [Lin],
    geom: &Geom,
    world: Affine2,
    layer: &PulseLayer,
) {
    let pins = &layer.puppet_pins;
    if pins.len() < 3 {
        return;
    }
    let &Geom { w, h, cx, cy, half_w, half_h, .. } = geom;
    let Some(inv) = world.inverse() else { return; };

    // Center of mass as the "rest" reference position.
    let n = pins.len() as f32;
    let rest_x = pins.iter().map(|p| p.position[0]).sum::<f32>() / n;
    let rest_y = pins.iter().map(|p| p.position[1]).sum::<f32>() / n;

    let w_usize = w as usize;
    let h_usize = h as usize;
    let mut out = buf.to_vec();

    for py in 0..h_usize {
        let comp_y = py as f32 + 0.5 - cy;
        for px in 0..w_usize {
            let comp_x = px as f32 + 0.5 - cx;
            let (lx, ly) = inv.apply(comp_x, comp_y);
            if lx.abs() > half_w || ly.abs() > half_h {
                continue;
            }
            let mut total_w = 0.0f32;
            let mut disp_x = 0.0f32;
            let mut disp_y = 0.0f32;
            for pin in pins {
                let dx = lx - pin.position[0];
                let dy = ly - pin.position[1];
                let dist2 = dx * dx + dy * dy + 1e-6;
                let w_i = 1.0 / dist2;
                disp_x += w_i * (pin.position[0] - rest_x);
                disp_y += w_i * (pin.position[1] - rest_y);
                total_w += w_i;
            }
            let (dx, dy) = if total_w > 0.0 {
                (disp_x / total_w, disp_y / total_w)
            } else {
                (0.0, 0.0)
            };
            let src_lx = lx - dx;
            let src_ly = ly - dy;
            let (src_cx, src_cy) = world.apply(src_lx, src_ly);
            let src_px = (src_cx + cx - 0.5).round() as i32;
            let src_py = (src_cy + cy - 0.5).round() as i32;
            if src_px >= 0 && src_py >= 0 && src_px < w as i32 && src_py < h as i32 {
                let src_idx = (src_py as u32 * w + src_px as u32) as usize;
                let dst_idx = (py as u32 * w + px as u32) as usize;
                out[dst_idx] = buf[src_idx];
            }
        }
    }
    buf.copy_from_slice(&out);
}
