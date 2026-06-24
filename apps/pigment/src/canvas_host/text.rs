//! Text-layer domain of `CanvasHost`: rasterize a new text layer or re-rasterize
//! an existing one in place, plus the `shift_rgba_f32` placement helper.

use prism_core::{Document, LayerId};

use super::{f32_to_f16_bytes, CanvasHost};

impl CanvasHost {
    /// Rasterize `text` into a NEW layer added to `doc`'s stack at the canvas
    /// origin (offset by `origin` doc px), upload it through the engine, and
    /// snapshot the empty layer for undo. Returns the new `LayerId`. Mirrors the
    /// egui app's text path (`prism_io::text::render_text` → upload), but creates
    /// a flattened raster layer (the GPUI host has no re-rasterizing text-layer
    /// sync yet, so the text is baked on placement). `color` is straight sRGB.
    #[allow(clippy::too_many_arguments)]
    pub fn rasterize_text_layer(
        &mut self,
        doc: &mut Document,
        text: &str,
        font_px: f32,
        color: [f32; 4],
        origin: [f32; 2],
        align: prism_io::text::TextAlign,
        family: Option<&str>,
    ) -> LayerId {
        let (w, h) = (self.doc_w, self.doc_h);
        // The engine rasterizes from the top-left; shift the result to `origin`
        // so the text appears where the user clicked.
        let px = prism_io::text::render_text(text, font_px, color, w, h, align, family);
        let placed = shift_rgba_f32(&px, w, h, origin[0].round() as i32, origin[1].round() as i32);
        let id = doc.layers.add_raster("Text");
        doc.active_layer = Some(id);
        self.canvas.ensure_layer(&self.device, id);
        // Snapshot the (empty) layer so the first edit to this text is undoable
        // back to a blank layer; then upload the rasterized glyphs.
        self.canvas
            .begin_command_now(&self.device, &self.queue, id, "Text");
        self.canvas
            .upload_layer(&self.queue, id, &f32_to_f16_bytes(&placed));
        self.sync_order(doc);
        self.mark_dirty();
        id
    }

    /// Re-rasterize `text` into an EXISTING raster layer `id` (in-place text
    /// editing as the user types), shifted to `origin`. Snapshots the layer for
    /// undo, then uploads. `color` is straight sRGB.
    #[allow(clippy::too_many_arguments)]
    pub fn update_text_layer(
        &mut self,
        id: LayerId,
        text: &str,
        font_px: f32,
        color: [f32; 4],
        origin: [f32; 2],
        align: prism_io::text::TextAlign,
        family: Option<&str>,
    ) {
        let (w, h) = (self.doc_w, self.doc_h);
        let px = prism_io::text::render_text(text, font_px, color, w, h, align, family);
        let placed = shift_rgba_f32(&px, w, h, origin[0].round() as i32, origin[1].round() as i32);
        self.canvas
            .begin_command_now(&self.device, &self.queue, id, "Text");
        self.canvas
            .upload_layer(&self.queue, id, &f32_to_f16_bytes(&placed));
        self.mark_dirty();
    }
}

/// Translate an RGBA-f32 buffer by `(dx, dy)` doc px on a `w × h` canvas, zero-
/// filling the exposed border. Used to place a top-left-rasterized text bitmap
/// at the click position (the egui app does the same via `place_generated`).
pub(super) fn shift_rgba_f32(src: &[f32], w: u32, h: u32, dx: i32, dy: i32) -> Vec<f32> {
    if dx == 0 && dy == 0 {
        return src.to_vec();
    }
    let (wi, hi) = (w as i32, h as i32);
    let mut out = vec![0.0f32; (w * h * 4) as usize];
    for y in 0..hi {
        let sy = y - dy;
        if sy < 0 || sy >= hi {
            continue;
        }
        for x in 0..wi {
            let sx = x - dx;
            if sx < 0 || sx >= wi {
                continue;
            }
            let di = ((y * wi + x) * 4) as usize;
            let si = ((sy * wi + sx) * 4) as usize;
            out[di..di + 4].copy_from_slice(&src[si..si + 4]);
        }
    }
    out
}

#[cfg(test)]
mod shift_tests {
    use super::shift_rgba_f32;

    // A +1px right / +1px down shift moves the single lit texel and zero-fills the
    // newly-exposed top-left border.
    #[test]
    fn shifts_texel_and_zero_fills_border() {
        // 3x3, one opaque white texel at (0,0).
        let mut src = vec![0.0f32; 3 * 3 * 4];
        src[0..4].copy_from_slice(&[1.0, 1.0, 1.0, 1.0]);
        let out = shift_rgba_f32(&src, 3, 3, 1, 1);
        // (0,0) is now empty; the lit texel landed at (1,1) = index (1*3+1)*4 = 16.
        assert_eq!(&out[0..4], &[0.0, 0.0, 0.0, 0.0]);
        assert_eq!(&out[16..20], &[1.0, 1.0, 1.0, 1.0]);
    }

    // A zero shift is the identity (fast path).
    #[test]
    fn zero_shift_is_identity() {
        let src: Vec<f32> = (0..16).map(|i| i as f32).collect();
        assert_eq!(shift_rgba_f32(&src, 2, 2, 0, 0), src);
    }

    // Shifting fully off-canvas yields an all-zero buffer.
    #[test]
    fn off_canvas_shift_is_empty() {
        let src = vec![1.0f32; 2 * 2 * 4];
        let out = shift_rgba_f32(&src, 2, 2, 5, 5);
        assert!(out.iter().all(|&v| v == 0.0));
    }
}
