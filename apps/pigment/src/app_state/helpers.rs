//! Free helper functions used across `app_state` domain modules.
//!
//! Split out of `app_state/mod.rs` as a pure mechanical refactor (no behavior
//! change). Visibility of the previously file-private helpers is widened to
//! `pub(crate)` so sibling domain modules (and the dispatcher) can still call
//! them after the move; their bodies are unchanged.

use super::*;

/// Build the uv-space layer-from-canvas affine for a translate (doc px) + uniform
/// scale about the canvas center. Returns (2x2 matrix `[a, b, c, d]`, offset).
/// Ported verbatim from the egui app's `compute_xform`
/// (`pigment-app/src/app/mod.rs`) so the move/scale math is identical; the engine
/// (`bake_transform` / the compositor's live-affine branch) consumes exactly this
/// form.
/// Map modifier keys to a selection combine mode, mirroring the egui app's
/// `mode_from_modifiers`: Shift = add, Alt = subtract, Shift+Alt = intersect,
/// else replace.
pub(crate) fn mode_from_modifiers(shift: bool, alt: bool) -> CombineMode {
    match (shift, alt) {
        (true, false) => CombineMode::Add,
        (false, true) => CombineMode::Subtract,
        (true, true) => CombineMode::Intersect,
        _ => CombineMode::Replace,
    }
}

/// Rasterize a rectangle (`ellipse = false`) or ellipse (`ellipse = true`) marquee
/// to a 0/1 selection mask (len = w*h). Ported from the egui app's `shape_mask`
/// (`pigment-app/src/app/mod.rs`) so add/subtract marquees match exactly. A pixel
/// center inside the rect/ellipse is `1.0`, else `0.0`.
pub(crate) fn shape_mask(rect: [f32; 4], ellipse: bool, w: u32, h: u32) -> Vec<f32> {
    let [rx, ry, rw, rh] = rect;
    let (cx, cy) = (rx + rw * 0.5, ry + rh * 0.5);
    let (hx, hy) = ((rw * 0.5).max(1e-3), (rh * 0.5).max(1e-3));
    let mut m = vec![0.0; (w * h) as usize];
    for y in 0..h {
        let py = y as f32 + 0.5;
        for x in 0..w {
            let px = x as f32 + 0.5;
            let inside = if ellipse {
                let dx = (px - cx) / hx;
                let dy = (py - cy) / hy;
                dx * dx + dy * dy <= 1.0
            } else {
                px >= rx && px < rx + rw && py >= ry && py < ry + rh
            };
            if inside {
                m[(y * w + x) as usize] = 1.0;
            }
        }
    }
    m
}

/// Short human-readable label for destructive `Action`s, used for history tracking.
/// Returns `None` for pure-UI actions that don't mutate pixels or document structure.
pub(crate) fn action_label(action: &Action) -> Option<String> {
    match action {
        Action::ApplyFilter(_) => Some("Apply Filter".into()),
        Action::AddAdjustment(_) => Some("Add Adjustment".into()),
        Action::SetAdjustment(_, _) => Some("Set Adjustment".into()),
        Action::SetAdjustmentCurve(_, _) => Some("Curves Edit".into()),
        Action::MoveCurvePoint(_, _, _) => Some("Curves Drag".into()),
        Action::PenClose => Some("Pen Path".into()),
        Action::DeleteLayer(_) => Some("Delete Layer".into()),
        Action::MoveLayer { .. } => Some("Move Layer".into()),
        Action::AddMask(_) => Some("Add Mask".into()),
        Action::DeleteMask(_) => Some("Delete Mask".into()),
        Action::OpenImage => Some("Open Image".into()),
        Action::OpenEXR => Some("Open EXR".into()),
        Action::FlattenLayers => Some("Flatten Layers".into()),
        Action::ConvertToSmartObject(_) => Some("Convert to Smart Object".into()),
        Action::RasterizeSmartObject(_) => Some("Rasterize Smart Object".into()),
        Action::SetLayerStyle(_, _) => Some("Set Layer Style".into()),
        Action::ClearLayerStyle(_) => Some("Clear Layer Style".into()),
        Action::ToggleClippingMask(_) => Some("Toggle Clipping Mask".into()),
        Action::HealBrush { .. } => Some("Healing Brush".into()),
        Action::CloneStampDab { .. } => Some("Clone Stamp".into()),
        Action::RemoveRedEye { .. } => Some("Red-Eye Removal".into()),
        Action::ContentAwarePatch { .. } => Some("Content-Aware Patch".into()),
        Action::SaveAs(_) => Some("Save As".into()),
        _ => None,
    }
}

/// Walk a cubic bézier pen path and produce a list of `Dab`s for painting.
/// Uses 100 linear t-steps per segment; no lyon dependency required.
pub(crate) fn rasterize_pen_path(path: &[PenNode], brush: &Brush) -> Vec<prism_canvas::Dab> {
    use prism_core::color::srgb_to_linear;
    let c = brush.color;
    let color = [
        srgb_to_linear(c[0]),
        srgb_to_linear(c[1]),
        srgb_to_linear(c[2]),
        brush.opacity,
    ];
    let radius = (brush.size * 0.5).max(0.5);
    let hardness = brush.hardness.clamp(0.0, 0.99);
    let spacing = (brush.size * 0.15).max(0.75);

    let mut dabs: Vec<prism_canvas::Dab> = Vec::new();
    const STEPS: usize = 100;

    for i in 0..path.len().saturating_sub(1) {
        let p0 = path[i].pos;
        let c0 = path[i].ctrl_out;
        let c1 = path[i + 1].ctrl_in;
        let p1 = path[i + 1].pos;

        let mut last: Option<[f32; 2]> = None;
        let mut residual = 0.0f32;

        for s in 0..=STEPS {
            let t = s as f32 / STEPS as f32;
            let u = 1.0 - t;
            let x = u * u * u * p0.0
                + 3.0 * u * u * t * c0.0
                + 3.0 * u * t * t * c1.0
                + t * t * t * p1.0;
            let y = u * u * u * p0.1
                + 3.0 * u * u * t * c0.1
                + 3.0 * u * t * t * c1.1
                + t * t * t * p1.1;
            let pos = [x, y];

            if let Some(prev) = last {
                let dx = pos[0] - prev[0];
                let dy = pos[1] - prev[1];
                let dist = (dx * dx + dy * dy).sqrt();
                if dist > 1e-3 {
                    let dir = [dx / dist, dy / dist];
                    let mut tt = residual;
                    while tt <= dist {
                        let px = prev[0] + dir[0] * tt;
                        let py = prev[1] + dir[1] * tt;
                        dabs.push(prism_canvas::Dab { center: [px, py], radius, hardness, color });
                        tt += spacing;
                    }
                    residual = tt - dist;
                }
            } else {
                dabs.push(prism_canvas::Dab { center: pos, radius, hardness, color });
            }
            last = Some(pos);
        }
    }
    dabs
}

/// Convert HSV (h: 0..360, s: 0..1, v: 0..1) to straight sRGB RGBA [f32;4] (alpha=1).
pub fn hsv_to_rgb(h: f32, s: f32, v: f32) -> [f32; 4] {
    if s <= 0.0 {
        return [v, v, v, 1.0];
    }
    let hh = h / 60.0;
    let i = hh.floor() as i32;
    let f = hh - i as f32;
    let p = v * (1.0 - s);
    let q = v * (1.0 - s * f);
    let t = v * (1.0 - s * (1.0 - f));
    let (r, g, b) = match i % 6 {
        0 => (v, t, p),
        1 => (q, v, p),
        2 => (p, v, t),
        3 => (p, q, v),
        4 => (t, p, v),
        _ => (v, p, q),
    };
    [r, g, b, 1.0]
}

/// Convert straight sRGB [f32;4] to HSV.
pub fn rgb_to_hsv(c: [f32; 4]) -> (f32, f32, f32) {
    let (r, g, b) = (c[0], c[1], c[2]);
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let delta = max - min;
    let v = max;
    let s = if max > 1e-6 { delta / max } else { 0.0 };
    let h = if delta < 1e-6 {
        0.0
    } else if max == r {
        60.0 * (((g - b) / delta).rem_euclid(6.0))
    } else if max == g {
        60.0 * ((b - r) / delta + 2.0)
    } else {
        60.0 * ((r - g) / delta + 4.0)
    };
    (h, s, v)
}

/// Resolve the autosave file path: `~/.local/share/prism/pigment_autosave.json`.
pub fn autosave_path() -> Option<std::path::PathBuf> {
    dirs::data_local_dir().map(|d| d.join("prism").join("pigment_autosave.json"))
}

/// Build a minimal single-page PDF that embeds `jpeg_bytes` as an image object.
pub(crate) fn build_minimal_pdf(jpeg_bytes: &[u8], img_w: u32, img_h: u32, landscape: bool) -> Vec<u8> {
    let (pw, ph): (f32, f32) = if landscape { (842.0, 595.0) } else { (595.0, 842.0) };
    let margin = 10.0_f32;
    let avail_w = pw - 2.0 * margin;
    let avail_h = ph - 2.0 * margin;
    let scale = (avail_w / img_w as f32).min(avail_h / img_h as f32);
    let draw_w = img_w as f32 * scale;
    let draw_h = img_h as f32 * scale;
    let x = (pw - draw_w) / 2.0;
    let y = (ph - draw_h) / 2.0;
    let img_len = jpeg_bytes.len();
    let img_obj = format!(
        "3 0 obj\n<< /Type /XObject /Subtype /Image /Width {img_w} /Height {img_h} \
         /ColorSpace /DeviceRGB /BitsPerComponent 8 /Filter /DCTDecode /Length {img_len} >>\nstream\n"
    );
    let img_end = "\nendstream\nendobj\n";
    let content_stream = format!("q {draw_w:.2} 0 0 {draw_h:.2} {x:.2} {y:.2} cm /Im1 Do Q");
    let cs_len = content_stream.len();
    let content_obj = format!(
        "4 0 obj\n<< /Length {cs_len} >>\nstream\n{content_stream}\nendstream\nendobj\n"
    );
    let page_obj = format!(
        "2 0 obj\n<< /Type /Page /Parent 1 0 R /MediaBox [0 0 {pw:.2} {ph:.2}] \
         /Contents 4 0 R /Resources << /XObject << /Im1 3 0 R >> >> >>\nendobj\n"
    );
    let pages_obj = "1 0 obj\n<< /Type /Pages /Kids [2 0 R] /Count 1 >>\nendobj\n";
    let catalog_obj = "5 0 obj\n<< /Type /Catalog /Pages 1 0 R >>\nendobj\n";
    let header = "%PDF-1.4\n";
    let mut pdf: Vec<u8> = Vec::new();
    pdf.extend_from_slice(header.as_bytes());
    let off1 = pdf.len();
    pdf.extend_from_slice(pages_obj.as_bytes());
    let off2 = pdf.len();
    pdf.extend_from_slice(page_obj.as_bytes());
    let off3 = pdf.len();
    pdf.extend_from_slice(img_obj.as_bytes());
    pdf.extend_from_slice(jpeg_bytes);
    pdf.extend_from_slice(img_end.as_bytes());
    let off4 = pdf.len();
    pdf.extend_from_slice(content_obj.as_bytes());
    let off5 = pdf.len();
    pdf.extend_from_slice(catalog_obj.as_bytes());
    let xref_offset = pdf.len();
    let xref = format!(
        "xref\n0 6\n0000000000 65535 f \n{off1:010} 00000 n \n{off2:010} 00000 n \n\
         {off3:010} 00000 n \n{off4:010} 00000 n \n{off5:010} 00000 n \n\
         trailer\n<< /Size 6 /Root 5 0 R >>\nstartxref\n{xref_offset}\n%%EOF\n"
    );
    pdf.extend_from_slice(xref.as_bytes());
    pdf
}

// ---- Batch 5 helper functions (pure, testable) ---------------------------

/// Capture a `LayerCompState` snapshot for every layer in the document.
pub(crate) fn capture_layer_comp_states(doc: &prism_core::Document) -> HashMap<LayerId, LayerCompState> {
    doc.layers.layers.iter().map(|l| {
        (l.id, LayerCompState {
            visible: l.visible,
            opacity: l.opacity,
            blend_mode: l.blend,
            offset_x: 0,
            offset_y: 0,
        })
    }).collect()
}

/// Restore layer states from a comp snapshot into the document.
pub(crate) fn apply_layer_comp_states(
    doc: &mut prism_core::Document,
    states: &HashMap<LayerId, LayerCompState>,
) {
    for layer in &mut doc.layers.layers {
        if let Some(s) = states.get(&layer.id) {
            layer.visible = s.visible;
            layer.opacity = s.opacity;
            layer.blend = s.blend_mode;
        }
    }
}

/// Compute a binary focus-area selection mask from linear-light RGBA f32 pixels.
/// Returns one `u8` per pixel (0 = not selected, 255 = selected).
/// Uses the local Laplacian variance in a 5×5 neighbourhood as a focus measure.
pub fn focus_area_mask(
    pixels: &[f32],
    w: u32,
    h: u32,
    threshold: f32,
    sensitivity: f32,
    invert: bool,
) -> Vec<u8> {
    let (w, h) = (w as usize, h as usize);
    let n = w * h;
    let mut variances = vec![0.0f32; n];
    let mut max_var = 0.0f32;

    for y in 0..h {
        for x in 0..w {
            // Compute luminance variance in 5×5 neighbourhood.
            let mut sum = 0.0f32;
            let mut sum_sq = 0.0f32;
            let mut count = 0usize;
            for dy in -2i32..=2 {
                for dx in -2i32..=2 {
                    let nx = x as i32 + dx;
                    let ny = y as i32 + dy;
                    if nx >= 0 && nx < w as i32 && ny >= 0 && ny < h as i32 {
                        let i = (ny as usize * w + nx as usize) * 4;
                        // Luminance approximation (linear light).
                        let luma = 0.2126 * pixels[i] + 0.7152 * pixels[i+1] + 0.0722 * pixels[i+2];
                        sum += luma;
                        sum_sq += luma * luma;
                        count += 1;
                    }
                }
            }
            let mean = sum / count as f32;
            let var = (sum_sq / count as f32 - mean * mean).max(0.0);
            variances[y * w + x] = var;
            if var > max_var { max_var = var; }
        }
    }

    let scale = if max_var > 1e-8 { 1.0 / max_var } else { 0.0 };
    let edge_width = (sensitivity * 0.2 + 0.01).max(0.01);

    variances.iter().map(|&v| {
        let normalized = v * scale;
        // Soft threshold via smooth-step over [threshold - edge, threshold + edge].
        let lo = (threshold - edge_width).max(0.0);
        let hi = (threshold + edge_width).min(1.0);
        let t = if hi <= lo { if normalized >= threshold { 1.0 } else { 0.0 } }
                else { ((normalized - lo) / (hi - lo)).clamp(0.0, 1.0) };
        let selected = if invert { 1.0 - t } else { t };
        (selected * 255.0).round() as u8
    }).collect()
}

/// Compute approximate LAB-space mean (L, a, b) for the given RGBA f32 pixels.
/// Uses the approximation: L ≈ luma, a ≈ R−G, b ≈ B−0.5R−0.5G.
pub fn match_color_stats(pixels: &[f32]) -> (f32, f32, f32) {
    if pixels.len() < 4 { return (0.0, 0.0, 0.0); }
    let n = pixels.len() / 4;
    let (mut sl, mut sa, mut sb) = (0.0f64, 0.0f64, 0.0f64);
    for i in 0..n {
        let (r, g, b) = (pixels[i*4] as f64, pixels[i*4+1] as f64, pixels[i*4+2] as f64);
        sl += 0.299 * r + 0.587 * g + 0.114 * b;
        sa += r - g;
        sb += b - 0.5 * r - 0.5 * g;
    }
    ((sl / n as f64) as f32, (sa / n as f64) as f32, (sb / n as f64) as f32)
}

/// Apply Match Color: shift target pixel LAB stats toward source stats.
pub fn apply_match_color(
    pixels: &[f32],
    src_stats: (f32, f32, f32),
    tgt_stats: (f32, f32, f32),
    fade: f32,
    match_luminance: bool,
    match_color: bool,
    neutralize: bool,
) -> Vec<f32> {
    let f = fade / 100.0;
    let dl = if match_luminance { (src_stats.0 - tgt_stats.0) * f } else { 0.0 };
    let da = if match_color { (src_stats.1 - tgt_stats.1) * f } else { 0.0 };
    let db = if match_color { (src_stats.2 - tgt_stats.2) * f } else { 0.0 };

    let n = pixels.len() / 4;
    let mut out = pixels.to_vec();
    for i in 0..n {
        let r = pixels[i*4];
        let g = pixels[i*4+1];
        let b = pixels[i*4+2];
        let luma = 0.299 * r + 0.587 * g + 0.114 * b;
        // Shift luminance: scale all channels proportionally.
        let new_luma = luma + dl;
        let luma_scale = if luma > 1e-6 { (new_luma / luma).clamp(0.0, 4.0) } else { 1.0 };
        let mut nr = (r * luma_scale + da).clamp(0.0, 1.0);
        let mut ng = (g * luma_scale - da).clamp(0.0, 1.0);
        let mut nb = (b * luma_scale + db).clamp(0.0, 1.0);
        if neutralize {
            // Pull a/b channels toward grey.
            let grey = 0.299 * nr + 0.587 * ng + 0.114 * nb;
            nr = (nr * (1.0 - f) + grey * f).clamp(0.0, 1.0);
            ng = (ng * (1.0 - f) + grey * f).clamp(0.0, 1.0);
            nb = (nb * (1.0 - f) + grey * f).clamp(0.0, 1.0);
        }
        out[i*4]   = nr;
        out[i*4+1] = ng;
        out[i*4+2] = nb;
        // Alpha unchanged.
    }
    out
}

/// Perspective-aware stamp: copy pixels from `src` area into `dst` area on the
/// pixel buffer using bilinear perspective interpolation within `plane_corners`.
/// `plane_corners` = [TL, TR, BR, BL] in doc-px. `radius` = brush radius.
pub fn stamp_in_perspective(
    pixels: &mut Vec<f32>,
    w: u32,
    h: u32,
    plane_corners: &[[f32; 2]; 4],
    src: [f32; 2],
    dst: [f32; 2],
    radius: f32,
) {
    let (w, h) = (w as usize, h as usize);
    let r = radius.max(1.0) as i32;
    // Map a canvas point to [0,1]^2 plane-local coords via bilinear inverse.
    let plane_to_local = |p: [f32; 2]| -> [f32; 2] {
        let [tl, tr, br, bl] = *plane_corners;
        // Use simple affine approximation: find s,t such that
        // (1-s)(1-t)*TL + s(1-t)*TR + s*t*BR + (1-s)*t*BL ≈ p
        // Solved via a few Newton iterations.
        let (mut s, mut t) = (0.5f32, 0.5f32);
        for _ in 0..8 {
            let qx = (1.0-s)*(1.0-t)*tl[0] + s*(1.0-t)*tr[0] + s*t*br[0] + (1.0-s)*t*bl[0];
            let qy = (1.0-s)*(1.0-t)*tl[1] + s*(1.0-t)*tr[1] + s*t*br[1] + (1.0-s)*t*bl[1];
            let dxds = -(1.0-t)*tl[0] + (1.0-t)*tr[0] + t*br[0] - t*bl[0];
            let dyds = -(1.0-t)*tl[1] + (1.0-t)*tr[1] + t*br[1] - t*bl[1];
            let dxdt = -(1.0-s)*tl[0] - s*tr[0] + s*br[0] + (1.0-s)*bl[0];
            let dydt = -(1.0-s)*tl[1] - s*tr[1] + s*br[1] + (1.0-s)*bl[1];
            let ex = p[0] - qx;
            let ey = p[1] - qy;
            let det = dxds * dydt - dyds * dxdt;
            if det.abs() < 1e-8 { break; }
            s += (ex * dydt - ey * dxdt) / det;
            t += (ey * dxds - ex * dyds) / det;
            s = s.clamp(0.0, 1.0);
            t = t.clamp(0.0, 1.0);
        }
        [s, t]
    };

    let dst_local = plane_to_local(dst);
    let src_local = plane_to_local(src);
    let delta_local = [dst_local[0] - src_local[0], dst_local[1] - src_local[1]];

    // For each pixel in the dst brush circle, map back to src coords and copy.
    let dx = dst[0] as i32;
    let dy = dst[1] as i32;
    let [tl, tr, br, bl] = *plane_corners;

    let sample = |px: &[f32], cx: f32, cy: f32| -> [f32; 4] {
        let xi = cx.floor() as i32;
        let yi = cy.floor() as i32;
        let fx = cx - xi as f32;
        let fy = cy - yi as f32;
        let sample_at = |x: i32, y: i32| -> [f32; 4] {
            if x < 0 || y < 0 || x >= w as i32 || y >= h as i32 {
                return [0.0; 4];
            }
            let i = (y as usize * w + x as usize) * 4;
            [px[i], px[i+1], px[i+2], px[i+3]]
        };
        let c00 = sample_at(xi,   yi);
        let c10 = sample_at(xi+1, yi);
        let c01 = sample_at(xi,   yi+1);
        let c11 = sample_at(xi+1, yi+1);
        std::array::from_fn(|k| {
            c00[k]*(1.0-fx)*(1.0-fy) + c10[k]*fx*(1.0-fy)
            + c01[k]*(1.0-fx)*fy + c11[k]*fx*fy
        })
    };

    let pixels_snap = pixels.clone();
    for oy in -r..=r {
        for ox in -r..=r {
            if ox*ox + oy*oy > r*r { continue; }
            let px_dst = dx + ox;
            let py_dst = dy + oy;
            if px_dst < 0 || py_dst < 0 || px_dst >= w as i32 || py_dst >= h as i32 { continue; }

            // Current dst point in local coords.
            let dst_pt = [px_dst as f32 + 0.5, py_dst as f32 + 0.5];
            let local = plane_to_local(dst_pt);
            // Corresponding src local.
            let src_local2 = [local[0] - delta_local[0], local[1] - delta_local[1]];
            // Map src_local back to canvas.
            let src_cx = (1.0-src_local2[0])*(1.0-src_local2[1])*tl[0]
                + src_local2[0]*(1.0-src_local2[1])*tr[0]
                + src_local2[0]*src_local2[1]*br[0]
                + (1.0-src_local2[0])*src_local2[1]*bl[0];
            let src_cy = (1.0-src_local2[0])*(1.0-src_local2[1])*tl[1]
                + src_local2[0]*(1.0-src_local2[1])*tr[1]
                + src_local2[0]*src_local2[1]*br[1]
                + (1.0-src_local2[0])*src_local2[1]*bl[1];

            let color = sample(&pixels_snap, src_cx, src_cy);
            let idx = (py_dst as usize * w + px_dst as usize) * 4;
            for k in 0..4 { pixels[idx + k] = color[k]; }
        }
    }
}

/// Sobel edge detection → flood-fill from centre → feather: returns 8-bit mask.
pub fn select_subject_mask(
    pixels: &[f32],
    w: u32,
    h: u32,
    threshold: f32,
    feather: f32,
) -> Vec<u8> {
    let (w, h) = (w as usize, h as usize);
    let n = w * h;
    let mut lum = vec![0.0f32; n];
    for i in 0..n {
        let r = pixels[i * 4];
        let g = pixels[i * 4 + 1];
        let b = pixels[i * 4 + 2];
        lum[i] = 0.2126 * r + 0.7152 * g + 0.0722 * b;
    }
    let mut edge = vec![0.0f32; n];
    for y in 1..h.saturating_sub(1) {
        for x in 1..w.saturating_sub(1) {
            let p = |dy: isize, dx: isize| lum[((y as isize + dy) as usize) * w + (x as isize + dx) as usize];
            let gx = -p(-1,-1) - 2.0*p(0,-1) - p(1,-1) + p(-1,1) + 2.0*p(0,1) + p(1,1);
            let gy = -p(-1,-1) - 2.0*p(-1,0) - p(-1,1) + p(1,-1) + 2.0*p(1,0) + p(1,1);
            edge[y * w + x] = (gx * gx + gy * gy).sqrt();
        }
    }
    let max_e = edge.iter().cloned().fold(0.0f32, f32::max).max(1e-6);
    let edge_thresh = threshold * max_e;

    let mut mask = vec![false; n];
    let cx = w / 2;
    let cy = h / 2;
    let mut queue = std::collections::VecDeque::new();
    let start = cy * w + cx;
    if edge[start] < edge_thresh {
        queue.push_back(start);
        mask[start] = true;
    }
    while let Some(idx) = queue.pop_front() {
        let x = (idx % w) as isize;
        let y = (idx / w) as isize;
        for (dy, dx) in [(-1,0i32),(1,0),(0,-1),(0,1)] {
            let nx = x + dx as isize;
            let ny = y + dy as isize;
            if nx < 0 || ny < 0 || nx >= w as isize || ny >= h as isize { continue; }
            let ni = (ny as usize) * w + (nx as usize);
            if mask[ni] || edge[ni] >= edge_thresh { continue; }
            mask[ni] = true;
            queue.push_back(ni);
        }
    }

    let mut out: Vec<u8> = mask.iter().map(|&m| if m { 255 } else { 0 }).collect();
    let r = feather.max(0.0) as usize;
    if r > 0 {
        let tmp = out.clone();
        for y in 0..h {
            for x in 0..w {
                let mut sum = 0u32;
                let mut cnt = 0u32;
                for ky in y.saturating_sub(r)..=(y+r).min(h-1) {
                    for kx in x.saturating_sub(r)..=(x+r).min(w-1) {
                        sum += tmp[ky * w + kx] as u32;
                        cnt += 1;
                    }
                }
                out[y * w + x] = (sum / cnt.max(1)) as u8;
            }
        }
    }
    out
}

/// Blend `src` layer into `tgt` layer per the Apply Image dialog parameters.
pub fn apply_image_blend(
    src: &[f32],
    tgt: &[f32],
    channel: ApplyImageChannel,
    blend_mode: BlendMode,
    opacity: f32,
    invert: bool,
) -> Vec<f32> {
    let n = tgt.len() / 4;
    let mut out = tgt.to_vec();
    for i in 0..n {
        let [sr, sg, sb, sa] = [src[i*4], src[i*4+1], src[i*4+2], src[i*4+3]];
        let raw = match channel {
            ApplyImageChannel::Rgb        => 0.2126*sr + 0.7152*sg + 0.0722*sb,
            ApplyImageChannel::Red        => sr,
            ApplyImageChannel::Green      => sg,
            ApplyImageChannel::Blue       => sb,
            ApplyImageChannel::Alpha      => sa,
            ApplyImageChannel::Luminosity => 0.2126*sr + 0.7152*sg + 0.0722*sb,
        };
        let s = if invert { 1.0 - raw } else { raw };
        let [tr, tg, tb, ta] = [tgt[i*4], tgt[i*4+1], tgt[i*4+2], tgt[i*4+3]];
        let blend_ch = |t: f32| -> f32 {
            let b = match blend_mode {
                BlendMode::Normal     => s,
                BlendMode::Multiply   => t * s,
                BlendMode::Screen     => 1.0 - (1.0-t)*(1.0-s),
                BlendMode::Overlay    => if t < 0.5 { 2.0*t*s } else { 1.0 - 2.0*(1.0-t)*(1.0-s) },
                BlendMode::Darken     => t.min(s),
                BlendMode::Lighten    => t.max(s),
                BlendMode::Difference => (t - s).abs(),
                _                     => s,
            };
            t * (1.0 - opacity) + b * opacity
        };
        out[i*4]   = blend_ch(tr).clamp(0.0, 1.0);
        out[i*4+1] = blend_ch(tg).clamp(0.0, 1.0);
        out[i*4+2] = blend_ch(tb).clamp(0.0, 1.0);
        out[i*4+3] = ta;
    }
    out
}

/// Path to `~/.config/prism/workspaces/` for workspace JSON files.
pub(crate) fn dirs_home_workspace_dir() -> Option<std::path::PathBuf> {
    #[allow(deprecated)]
    std::env::home_dir().map(|h| h.join(".config").join("prism").join("workspaces"))
}

pub(crate) fn compute_xform(translate: [f32; 2], scale: f32, w: f32, h: f32) -> ([f32; 4], [f32; 2]) {
    let inv = 1.0 / scale.max(1e-3);
    let tx = translate[0] / w;
    let ty = translate[1] / h;
    let m = [inv, 0.0, 0.0, inv];
    let off = [0.5 - (0.5 + tx) * inv, 0.5 - (0.5 + ty) * inv];
    (m, off)
}

/// Compose a full free-transform — uniform `scale`, `rotation_deg`, and
/// `skew_x_deg`/`skew_y_deg` shear, plus `translate` (doc px) — into the
/// engine's inverse-sampling `(matrix, offset)` pair (matching `compute_xform`).
///
/// The forward (dest-from-source) transform about the canvas centre, in
/// normalized UV space, is `F = Scale · Rotate · Skew`. The engine samples the
/// source at `m · dest_uv + off`, so we return the *inverse* 2×2 of `F` and the
/// matching offset. `translate` shifts the dest in UV before inversion. Returns
/// `([m00, m01, m10, m11], [offx, offy])`.
pub(crate) fn compute_xform_full(
    translate: [f32; 2],
    scale: f32,
    rotation_deg: f32,
    skew_x_deg: f32,
    skew_y_deg: f32,
    w: f32,
    h: f32,
) -> ([f32; 4], [f32; 2]) {
    let s = scale.max(1e-3);
    let (sin, cos) = rotation_deg.to_radians().sin_cos();
    let kx = skew_x_deg.to_radians().tan();
    let ky = skew_y_deg.to_radians().tan();
    // Skew (shear) matrix.
    let (sk00, sk01, sk10, sk11) = (1.0, kx, ky, 1.0);
    // Rotation · Skew.
    let r00 = cos * sk00 - sin * sk10;
    let r01 = cos * sk01 - sin * sk11;
    let r10 = sin * sk00 + cos * sk10;
    let r11 = sin * sk01 + cos * sk11;
    // Uniform scale on top: F = s · (R·Sk).
    let f00 = s * r00;
    let f01 = s * r01;
    let f10 = s * r10;
    let f11 = s * r11;
    // Invert the 2×2 forward matrix to get the sampling (inverse) matrix.
    let det = f00 * f11 - f01 * f10;
    let inv_det = if det.abs() > 1e-9 { 1.0 / det } else { 0.0 };
    let m00 = f11 * inv_det;
    let m01 = -f01 * inv_det;
    let m10 = -f10 * inv_det;
    let m11 = f00 * inv_det;
    // Translate in normalized UV (about centre 0.5,0.5).
    let tx = translate[0] / w.max(1.0);
    let ty = translate[1] / h.max(1.0);
    // dest_uv' = dest_uv - translate; sample = m·(dest_uv' - 0.5) + 0.5.
    let cx = 0.5 + tx;
    let cy = 0.5 + ty;
    let offx = 0.5 - (m00 * cx + m01 * cy);
    let offy = 0.5 - (m10 * cx + m11 * cy);
    ([m00, m01, m10, m11], [offx, offy])
}

