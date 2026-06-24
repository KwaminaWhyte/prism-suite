//! Pure rendering helpers for the Drift root view.
//!
//! Keyframe interpolation, transform animation, fill colour conversion, and
//! vector-path div construction. These are free functions with no GPUI view
//! state; the root view (`Drift`) in `main.rs` calls them from `render`.

use gpui::{div, px, Styled};

use crate::app_state::{App, EasingKind, Fill, Keyframe, LayerTransform, VectorPath};

// ── Keyframe interpolation helpers ───────────────────────────────────────────

/// Linearly interpolate a single float property from keyframes at `frame`.
/// Falls back to `default_val` when no keyframes exist for the property.
pub(crate) fn kf_interpolate(keyframes: &[Keyframe], layer_id: usize, prop: &str, frame: usize, default_val: f32) -> f32 {
    let mut layer_kfs: Vec<&Keyframe> = keyframes
        .iter()
        .filter(|k| k.layer_id == layer_id && k.property == prop)
        .collect();
    if layer_kfs.is_empty() {
        return default_val;
    }
    layer_kfs.sort_by_key(|k| k.frame);

    // Before first keyframe
    if frame <= layer_kfs[0].frame {
        return layer_kfs[0].value;
    }
    // After last keyframe
    let last = layer_kfs[layer_kfs.len() - 1];
    if frame >= last.frame {
        return last.value;
    }
    // Find surrounding pair
    let after_idx = layer_kfs.iter().position(|k| k.frame > frame).unwrap_or(layer_kfs.len() - 1);
    let before = layer_kfs[after_idx - 1];
    let after  = layer_kfs[after_idx];
    let span = (after.frame - before.frame) as f32;
    if span <= 0.0 { return before.value; }
    let t = (frame - before.frame) as f32 / span;
    // Apply easing
    let t = match before.easing {
        EasingKind::EaseIn    => t * t,
        EasingKind::EaseOut   => 1.0 - (1.0 - t) * (1.0 - t),
        EasingKind::EaseInOut => if t < 0.5 { 2.0 * t * t } else { 1.0 - (-2.0 * t + 2.0).powi(2) / 2.0 },
        EasingKind::Hold      => 0.0, // jump at end
        _                     => t,   // Linear / Bezier approximated as linear
    };
    before.value + (after.value - before.value) * t
}

/// Compute the animated transform for `layer_id` at `frame`, merging keyframe
/// data over the base transform stored in `app.transforms`.
pub(crate) fn animated_transform(app: &App, layer_id: usize, frame: usize) -> LayerTransform {
    let base = app.transforms.get(&layer_id).cloned().unwrap_or_else(LayerTransform::new);
    let kfs = &app.keyframes;
    LayerTransform {
        x:         kf_interpolate(kfs, layer_id, "x",         frame, base.x),
        y:         kf_interpolate(kfs, layer_id, "y",         frame, base.y),
        scale_x:   kf_interpolate(kfs, layer_id, "scale_x",   frame, base.scale_x),
        scale_y:   kf_interpolate(kfs, layer_id, "scale_y",   frame, base.scale_y),
        rotation:  kf_interpolate(kfs, layer_id, "rotation",  frame, base.rotation),
        opacity:   kf_interpolate(kfs, layer_id, "opacity",   frame, base.opacity),
        anchor_x:  base.anchor_x,
        anchor_y:  base.anchor_y,
    }
}

/// Convert a `Fill` to a GPUI rgba colour, with a fallback for `Fill::None`.
pub(crate) fn fill_to_rgba(fill: &Fill, fallback: gpui::Rgba) -> gpui::Rgba {
    match fill {
        Fill::Solid { r, g, b, a } => {
            let ri = (r.clamp(0.0, 1.0) * 255.0) as u32;
            let gi = (g.clamp(0.0, 1.0) * 255.0) as u32;
            let bi = (b.clamp(0.0, 1.0) * 255.0) as u32;
            let ai = (a.clamp(0.0, 1.0) * 255.0) as u32;
            gpui::rgba((ri << 24) | (gi << 16) | (bi << 8) | ai)
        }
        Fill::None => fallback,
        _ => fallback,
    }
}

/// Build the GPUI div elements for all vector paths belonging to a visible layer.
/// Returns `(path_id, div)` tuples so callers can attach selection click-handlers.
pub(crate) fn render_vector_paths(
    paths: &[VectorPath],
    layer_id: usize,
    tx: f32,
    ty: f32,
    opacity: f32,
    layer_color: gpui::Rgba,
    scale: f32,
) -> Vec<(usize, gpui::Div)> {
    paths
        .iter()
        .filter(|p| p.layer_id == layer_id)
        .map(|path| {
            let path_id = path.id;
            let fill_color = fill_to_rgba(&path.fill, gpui::rgba(
                ((layer_color.r * 255.0) as u32) << 24
                | ((layer_color.g * 255.0) as u32) << 16
                | ((layer_color.b * 255.0) as u32) << 8
                | 0xcc,
            ));
            let stroke_color = gpui::rgba(
                ((path.stroke.r * 255.0) as u32) << 24
                | ((path.stroke.g * 255.0) as u32) << 16
                | ((path.stroke.b * 255.0) as u32) << 8
                | 0xff,
            );
            let shape_div = if let Some((rx, ry, rw, rh)) = path.as_rect() {
                div()
                    .absolute()
                    .left(px((rx + tx) * scale))
                    .top(px((ry + ty) * scale))
                    .w(px(rw.max(1.0) * scale))
                    .h(px(rh.max(1.0) * scale))
                    .bg(fill_color)
                    .border_1()
                    .border_color(stroke_color)
                    .opacity(opacity)
            } else if path.is_ellipse() {
                let (bx, by, bw, bh) = path.bbox();
                div()
                    .absolute()
                    .left(px((bx + tx) * scale))
                    .top(px((by + ty) * scale))
                    .w(px(bw.max(1.0) * scale))
                    .h(px(bh.max(1.0) * scale))
                    .rounded_full()
                    .bg(fill_color)
                    .border_1()
                    .border_color(stroke_color)
                    .opacity(opacity)
            } else {
                // Generic path: render bounding box outline
                let (bx, by, bw, bh) = path.bbox();
                div()
                    .absolute()
                    .left(px((bx + tx) * scale))
                    .top(px((by + ty) * scale))
                    .w(px(bw.max(1.0) * scale))
                    .h(px(bh.max(1.0) * scale))
                    .bg(fill_color)
                    .border_1()
                    .border_color(stroke_color)
                    .opacity(opacity)
            };
            (path_id, shape_div)
        })
        .collect()
}
