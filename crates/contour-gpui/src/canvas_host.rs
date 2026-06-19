//! Bridges Contour's CPU vector rasterizer into GPUI as a cached `RenderImage`.
//!
//! Contour draws vectors CPU-side (tiny-skia) — there is NO wgpu and NO
//! prism-canvas here, unlike the Pigment host. This host reuses Contour's
//! existing export raster path: `export::to_rgba8_artboard` rasterizes the
//! document, cropped to the active artboard, into a straight-RGBA8 buffer. We
//! convert RGBA8 → BGRA8 (GPUI's `RenderImage` byte order) and wrap it as a
//! `RenderImage`, cached and only rebuilt when the host is marked dirty.
//!
//! Drawing vectors as native GPUI shapes is a LATER optimization; this pass
//! bridges the same pixels the PNG exporter produces.

use std::sync::Arc;

use contour_app::document::Document;
use contour_app::export;
use gpui::RenderImage;
use image::{Frame, RgbaImage};

/// The artboard rectangle the host rasterizes. Falls back to the document's
/// default artboard size when a document somehow carries none.
fn artboard_rect(doc: &Document) -> [f32; 4] {
    doc.active_artboard()
        .map(|ab| ab.rect)
        .unwrap_or([0.0, 0.0, 1000.0, 700.0])
}

/// Contour's CPU rasterizer fronted by a cached bridged `RenderImage`.
pub struct CanvasHost {
    /// Preview pixel dimensions (the active artboard size, document-units == px).
    pub doc_w: u32,
    pub doc_h: u32,
    cached: Option<Arc<RenderImage>>,
    dirty: bool,
    /// Last rasterized frame as BGRA8 bytes, for eyedropper pixel sampling.
    pub frame_bytes: Vec<u8>,
}

impl CanvasHost {
    /// Build the host for a document. Records the active artboard's pixel size
    /// and leaves the host dirty so the first `image(doc)` rasterizes.
    pub fn new(doc: &Document) -> Self {
        let ab = artboard_rect(doc);
        Self {
            doc_w: ab[2].round().max(1.0) as u32,
            doc_h: ab[3].round().max(1.0) as u32,
            cached: None,
            dirty: true,
            frame_bytes: Vec::new(),
        }
    }

    /// The bridged document image. Re-rasterizes only when dirty; otherwise
    /// returns the cached `RenderImage`. `doc` is passed in (rather than owned)
    /// so the host stays a thin bridge over `App`'s document.
    /// `mesh_points` (16 entries = 4×4 grid) are composited as a mesh gradient
    /// overlay in doc space when present.
    pub fn image(
        &mut self,
        doc: &Document,
        mesh_points: &[((f32, f32), [f32; 4])],
        _mesh_bbox: Option<[f32; 4]>,
    ) -> Arc<RenderImage> {
        if !self.dirty {
            if let Some(c) = &self.cached {
                return c.clone();
            }
        }
        let rendered = self.rasterize_and_bridge(doc, mesh_points);
        self.cached = Some(rendered.clone());
        self.dirty = false;
        rendered
    }

    /// Force the next `image()` to re-rasterize. Call after any document mutation
    /// that changes the rasterized pixels (visibility, geometry, paint, order).
    pub fn mark_dirty(&mut self) {
        self.dirty = true;
    }

    /// Sample the RGBA pixel at artboard-local coords `(x, y)` from the last
    /// rasterized frame. Returns `[R, G, B, A]` (converted from BGRA storage).
    /// Out-of-bounds or uninitialized returns `[0, 0, 0, 0]`.
    pub fn sample_pixel(&self, x: u32, y: u32) -> [u8; 4] {
        if x >= self.doc_w || y >= self.doc_h {
            return [0, 0, 0, 0];
        }
        let idx = (y * self.doc_w + x) as usize * 4;
        if idx + 3 >= self.frame_bytes.len() {
            return [0, 0, 0, 0];
        }
        // Stored as BGRA; return RGBA
        [
            self.frame_bytes[idx + 2], // R
            self.frame_bytes[idx + 1], // G
            self.frame_bytes[idx],     // B
            self.frame_bytes[idx + 3], // A
        ]
    }

    /// The active artboard's top-left in document space. The preview is the
    /// artboard cropped + translated by `(-ox, -oy)` (see `export`), so a preview
    /// pixel `(px, py)` maps to the document point `(ox + px, oy + py)`. The
    /// canvas hit-test adds this origin before consulting the document model.
    pub fn artboard_origin(&self, doc: &Document) -> (f32, f32) {
        let ab = artboard_rect(doc);
        (ab[0], ab[1])
    }

    /// Rasterize the document (cropped to the active artboard) via Contour's
    /// shared raster path, composite any mesh gradient overlay, convert RGBA8 →
    /// BGRA8, and wrap it as a GPUI `RenderImage`. Falls back to a blank
    /// artboard-sized frame if the rasterizer returns nothing (degenerate artboard).
    fn rasterize_and_bridge(
        &mut self,
        doc: &Document,
        mesh_points: &[((f32, f32), [f32; 4])],
    ) -> Arc<RenderImage> {
        let ab = artboard_rect(doc);
        let (w, h, mut rgba) = export::to_rgba8_artboard(doc, ab).unwrap_or_else(|| {
            // Degenerate artboard: hand back an opaque white frame so the window
            // still lays out.
            let (w, h) = (self.doc_w.max(1), self.doc_h.max(1));
            (w, h, vec![255u8; (w * h * 4) as usize])
        });
        self.doc_w = w;
        self.doc_h = h;

        // Composite mesh gradient overlay when 16 control points are present.
        if mesh_points.len() >= 16 {
            let ox = ab[0];
            let oy = ab[1];
            let mapped: Vec<((f32, f32), [f32; 4])> = mesh_points
                .iter()
                .map(|&((px, py), c)| ((px - ox, py - oy), c))
                .collect();
            let overlay =
                contour_app::mesh_gradient::render_mesh_overlay(&mapped, w, h);
            for i in (0..rgba.len()).step_by(4) {
                if i + 3 >= overlay.len() {
                    break;
                }
                let src_a = overlay[i + 3] as f32 / 255.0;
                if src_a <= 0.0 {
                    continue;
                }
                let dst_a = rgba[i + 3] as f32 / 255.0;
                let out_a = src_a + dst_a * (1.0 - src_a);
                if out_a > 0.0 {
                    let blend = |s: u8, d: u8| {
                        let sf = s as f32 / 255.0;
                        let df = d as f32 / 255.0;
                        ((sf * src_a + df * dst_a * (1.0 - src_a)) / out_a * 255.0).round() as u8
                    };
                    rgba[i] = blend(overlay[i], rgba[i]);
                    rgba[i + 1] = blend(overlay[i + 1], rgba[i + 1]);
                    rgba[i + 2] = blend(overlay[i + 2], rgba[i + 2]);
                    rgba[i + 3] = (out_a * 255.0).round() as u8;
                }
            }
        }

        log::info!("rasterized {w}x{h}, bridged {} bytes", rgba.len());

        let bgra = rgba8_to_bgra8(&rgba);
        self.frame_bytes = bgra.clone();
        let buf = RgbaImage::from_raw(w, h, bgra).expect("raster size mismatch");
        Arc::new(RenderImage::new([Frame::new(buf)]))
    }
}

/// Straight RGBA8 → BGRA8 (GPUI's `RenderImage` stores BGRA byte order). Swaps
/// the R and B channels in place per pixel; alpha and green are untouched.
fn rgba8_to_bgra8(rgba: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(rgba.len());
    for px in rgba.chunks_exact(4) {
        out.push(px[2]); // B
        out.push(px[1]); // G
        out.push(px[0]); // R
        out.push(px[3]); // A
    }
    out
}
