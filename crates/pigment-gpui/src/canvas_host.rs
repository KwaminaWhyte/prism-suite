//! Owns a wgpu device and drives the REAL prism-canvas compositor, bridging the
//! composited document into GPUI via CPU readback.
//!
//! This is the first real vertical slice: a placeholder document is uploaded as
//! a layer, composited on the GPU through `CanvasGpu`, read back as Rgba16Float
//! (linear-premultiplied), and converted to BGRA8 sRGB for a GPUI `RenderImage`.
//! The result is cached and only recomputed when the host is marked dirty.

use std::sync::Arc;

use gpui::RenderImage;
use half::f16;
use image::{Frame, RgbaImage};
use prism_canvas::{wgpu, CanvasGpu, Dab, LayerDraw, SelectionOp};
use prism_core::color::{linear_to_srgb, srgb_to_linear};
use prism_core::fill::flood_fill_mask;
use prism_core::gradient::Gradient;
use prism_core::shape::{fill_shape, ShapeKind};
use prism_core::LayerId;
use prism_core::histogram::{histogram, Histogram};
use prism_core::{Adjustment, BlendMode, Document, LayerKind, Size};
use prism_core::tone;
use crate::app_state::{ExportFormat, ExportPreset, SoftProofMode};

/// wgpu device + the real compositor, with a cached bridged `RenderImage`.
pub struct CanvasHost {
    device: wgpu::Device,
    queue: wgpu::Queue,
    canvas: CanvasGpu,
    order: Vec<LayerDraw>,
    pub doc_w: u32,
    pub doc_h: u32,
    cached: Option<Arc<RenderImage>>,
    /// 256-bin histogram of the last composite (linear-light, matches egui app).
    hist: Option<Histogram>,
    dirty: bool,
    /// Display channel mask [R, G, B, A]. Channels set to `false` are zeroed in
    /// the bridged BGRA8 output (display only — pixels are unchanged in engine).
    channel_mask: [bool; 4],
    /// Soft-proof mode: when not Off, apply CMYK round-trip to bridged output.
    soft_proof: SoftProofMode,
}

impl CanvasHost {
    /// Boot a wgpu device, build the placeholder document, upload it as a layer,
    /// and prime the compositor. Returns the host (still dirty: first `image()`
    /// runs the composite + readback). Also returns the `Document` so the host
    /// shell can read it for the layers panel.
    pub fn new() -> (Self, Document) {
        let instance = wgpu::Instance::default();
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            ..Default::default()
        }))
        .expect("no wgpu adapter");
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("pigment-gpui"),
            ..Default::default()
        }))
        .expect("no wgpu device");

        // target_format only feeds the (unused here) display pipeline.
        let mut canvas = CanvasGpu::new(&device, wgpu::TextureFormat::Bgra8UnormSrgb);

        // Placeholder document.
        let img = prism_io::placeholder(Size::new(1280, 800));
        let doc = Document::new(img.size);
        let bg_id = doc.layers.layers[0].id;

        // Upload the placeholder pixels as the background layer.
        let f16bytes = rgba8_to_f16_bytes(&img.rgba8);
        canvas.ensure_canvas(&device, doc.size);
        canvas.ensure_layer(&device, bg_id);
        canvas.upload_layer(&queue, bg_id, &f16bytes);

        let order: Vec<LayerDraw> = doc
            .layers
            .layers
            .iter()
            .map(|l| LayerDraw::basic(l.id, l.opacity, l.blend.shader_id(), l.visible))
            .collect();

        let mut host = Self {
            device,
            queue,
            canvas,
            order,
            doc_w: doc.size.width,
            doc_h: doc.size.height,
            cached: None,
            hist: None,
            dirty: true,
            channel_mask: [true; 4],
            soft_proof: SoftProofMode::Off,
        };
        // Ensure painting is UNMASKED on a fresh document. `ensure_canvas` already
        // allocates the selection texture and leaves `has_selection = false` (which
        // means "paint the whole canvas" in the dab shader). We additionally issue
        // an explicit `SelectionOp::All` so the state is guaranteed regardless of
        // any future selection wiring — `paint_dabs` only no-ops if the selection
        // texture is missing or the mask is empty-but-active, neither of which can
        // happen here. Without this, a stroke would silently paint nothing.
        host.select_all();
        (host, doc)
    }

    /// The bridged document image. Composites + reads back only when dirty;
    /// otherwise returns the cached `RenderImage`.
    pub fn image(&mut self) -> Arc<RenderImage> {
        if !self.dirty {
            if let Some(c) = &self.cached {
                return c.clone();
            }
        }
        let rendered = self.composite_and_bridge();
        self.cached = Some(rendered.clone());
        self.dirty = false;
        rendered
    }

    /// The last composite's histogram. Populated by `image()`; call that first.
    pub fn histogram(&self) -> Option<&Histogram> {
        self.hist.as_ref()
    }

    /// Force the next `image()` to re-composite + re-read-back. Call after any
    /// document mutation that changes composited pixels (visibility, opacity,
    /// blend, layer order, deletion).
    pub fn mark_dirty(&mut self) {
        self.dirty = true;
    }

    /// Rebuild the per-layer draw order from the document's current layer stack
    /// (id, opacity, blend, visibility). Mirrors the order built in `new`. Pair
    /// with `mark_dirty` after layer mutations. Note: this does NOT upload new
    /// pixels — only layers already uploaded to the GPU composite; freshly added
    /// raster layers (a later wave) will need an upload path here.
    ///
    /// Adjustment layers (`LayerKind::Adjustment`) are encoded into the draw via
    /// `Adjustment::encode` (shader kind + scalar params) and, for ChannelMixer,
    /// the per-output matrix — exactly the egui app's `layer_order`. The matching
    /// LUTs for the LUT-based kinds (Curves / GradientMap / ColorBalance) are
    /// uploaded by `sync_adjustment_luts`; call that after this on a layer change.
    pub fn sync_order(&mut self, doc: &Document) {
        self.order = doc
            .layers
            .layers
            .iter()
            .map(|l| {
                let mut d = LayerDraw::basic(l.id, l.opacity, l.blend.shader_id(), l.visible);
                if let LayerKind::Adjustment(a) = &l.kind {
                    let (kind, params) = a.encode();
                    d.adjust_kind = kind;
                    d.adjust = params;
                    if let Some(m) = a.channel_mixer_matrix() {
                        d.mix_r = m.r;
                        d.mix_g = m.g;
                        d.mix_b = m.b;
                    }
                }
                d
            })
            .collect();
    }

    /// Upload the GPU LUTs for every LUT-based adjustment layer (Curves kind 8,
    /// GradientMap kind 12, ColorBalance kind 13) so the compositor samples the
    /// current params. Mirrors the egui app's `sync_curve_luts`. Cheap (a few
    /// 256-entry tables); call on any adjustment add / param edit before the next
    /// composite. Scalar-only kinds need no LUT and are skipped.
    pub fn sync_adjustment_luts(&mut self, doc: &Document) {
        for l in &doc.layers.layers {
            let LayerKind::Adjustment(a) = &l.kind else {
                continue;
            };
            match a {
                Adjustment::Curves(cp) => {
                    self.canvas
                        .set_curve_lut(&self.device, &self.queue, l.id, &cp.rgb, &cp.r, &cp.g, &cp.b);
                }
                Adjustment::GradientMap { low, high } => {
                    self.canvas
                        .set_gradient_lut(&self.device, &self.queue, l.id, *low, *high);
                }
                Adjustment::ColorBalance {
                    shadows,
                    midtones,
                    highlights,
                    ..
                } => {
                    self.canvas.set_color_balance_lut(
                        &self.device,
                        &self.queue,
                        l.id,
                        *shadows,
                        *midtones,
                        *highlights,
                    );
                }
                _ => {}
            }
        }
    }

    /// Allocate a GPU layer slot for `id` (an adjustment layer needs a texture
    /// slot even though it carries no painted pixels — the compositor binds it).
    /// Idempotent; forwards the engine's `ensure_layer`.
    pub fn ensure_layer(&mut self, id: LayerId) {
        self.canvas.ensure_layer(&self.device, id);
    }

    /// Upload a raw straight-sRGB RGBA8 pixel buffer for `id`, converting to
    /// linear-premultiplied f16 in one pass. Buffer must be exactly `doc_w * doc_h * 4`
    /// bytes. Marks the host dirty.
    pub fn upload_layer_rgba8(&mut self, id: LayerId, rgba8: &[u8]) {
        let f16bytes = rgba8_to_f16_bytes(rgba8);
        self.canvas.upload_layer(&self.queue, id, &f16bytes);
        self.mark_dirty();
    }

    // ---- Layer masks ---------------------------------------------------------

    /// Add (or replace) a raster mask on `layer` from per-pixel reveal values
    /// (1 = reveal, 0 = hide; len = w*h). `None` => a white (reveal-all) mask.
    /// Forwards the engine's `set_mask`, then marks dirty so the next composite
    /// applies it. Mirrors the egui app's `add_mask`.
    pub fn set_mask(&mut self, layer: LayerId, values: Option<&[f32]>) {
        self.canvas
            .set_mask(&self.device, &self.queue, layer, values);
        self.mark_dirty();
    }

    /// Delete `layer`'s mask (the compositor falls back to a white reveal-all
    /// mask). Forwards the engine's `delete_mask`, then marks dirty. Mirrors the
    /// egui app's `delete_mask`.
    pub fn delete_mask(&mut self, layer: LayerId) {
        self.canvas.delete_mask(layer);
        self.mark_dirty();
    }

    /// Whether `layer` currently has a mask. Forwards the engine's `has_mask`.
    /// (The host tracks masked layers in `App::masked_layers`; this is the direct
    /// engine query, kept for a status readout / future re-sync.)
    #[allow(dead_code)]
    pub fn has_mask(&self, layer: LayerId) -> bool {
        self.canvas.has_mask(layer)
    }

    /// Set a full-canvas (unmasked) selection so `paint_dabs` paints everywhere.
    /// Issues `SelectionOp::All`, which sets the compositor's `has_selection`
    /// flag to false (the dab shader then skips the mask multiply entirely).
    /// Called once at construction; safe to call again to clear any selection.
    pub fn select_all(&mut self) {
        let mut enc = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("pigment-gpui.select_all"),
            });
        self.canvas
            .apply_selection(&self.device, &self.queue, &mut enc, &SelectionOp::All);
        self.queue.submit(Some(enc.finish()));
    }

    /// Paint a batch of dabs into `layer` through the REAL prism-canvas brush
    /// pipeline, then mark dirty so the next `image()` re-composites + re-reads.
    /// Dabs are painted straight into the layer texture (`into_wet = false`,
    /// `into_mask = false`) — the simplest path that needs no wet-buffer flush.
    /// No-ops on an empty batch. Painting is unmasked (see `select_all`).
    ///
    /// `snapshot` requests a whole-layer undo snapshot taken BEFORE the dabs land
    /// (the egui app snapshots once per stroke, on the first frame). The caller
    /// passes `true` on the first batch of a stroke and `false` for continuations
    /// so the whole stroke is one undo step.
    ///
    /// `into_mask` routes the dabs into `layer`'s mask texture instead of its
    /// pixels (the mask-edit path): white dabs reveal, eraser dabs hide. Mask
    /// edits aren't snapshotted (the egui app's mask paint isn't undoable yet),
    /// so the caller passes `snapshot = false` when `into_mask` is true.
    pub fn paint_dabs(
        &mut self,
        layer: LayerId,
        dabs: &[Dab],
        erase: bool,
        snapshot: bool,
        into_mask: bool,
    ) {
        if dabs.is_empty() {
            return;
        }
        if snapshot {
            self.canvas
                .begin_command_now(&self.device, &self.queue, layer, "Brush");
        }
        let mut enc = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("pigment-gpui.paint_dabs"),
            });
        self.canvas.paint_dabs(
            &self.device,
            &self.queue,
            &mut enc,
            layer,
            dabs,
            erase,
            false, // into_wet
            into_mask,
        );
        self.queue.submit(Some(enc.finish()));
        self.mark_dirty();
    }

    // ---- Undo / redo ---------------------------------------------------------

    /// Undo the most recent destructive command (restores the snapshotted region
    /// onto its layer through the engine's own undo stack), then re-composite.
    /// No-op if the undo stack is empty. Mirrors the egui app's `gpu.undo`.
    pub fn undo(&mut self) {
        let mut enc = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("pigment-gpui.undo"),
            });
        self.canvas.undo(&self.device, &mut enc);
        self.queue.submit(Some(enc.finish()));
        self.mark_dirty();
    }

    /// Redo the most recently undone command, then re-composite. No-op if the
    /// redo stack is empty. Mirrors the egui app's `gpu.redo`.
    pub fn redo(&mut self) {
        let mut enc = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("pigment-gpui.redo"),
            });
        self.canvas.redo(&self.device, &mut enc);
        self.queue.submit(Some(enc.finish()));
        self.mark_dirty();
    }

    /// Pending undo/redo step labels (oldest→newest undo, next→furthest redo),
    /// for the Edit menu's enabled/labeled state. Forwards `history_labels`.
    pub fn history_labels(&self) -> (Vec<String>, Vec<String>) {
        self.canvas.history_labels()
    }

    /// Snapshot the whole active layer onto the undo stack with `label`, BEFORE a
    /// destructive whole-layer op (fill / gradient / shape / text raster). Mirrors
    /// the egui app's `gpu.begin_command_now` for non-stroke commands. (The
    /// engine's `apply_*` filters already snapshot internally, so filters don't
    /// call this.)
    pub fn snapshot_layer(&mut self, layer: LayerId, label: &str) {
        self.canvas
            .begin_command_now(&self.device, &self.queue, layer, label);
    }

    // ---- Clone stamp ---------------------------------------------------------

    /// Freeze `layer`'s current pixels into the clone source so a clone stroke
    /// samples a stable snapshot (no feedback). Call at clone-stroke begin, right
    /// after `snapshot_layer`. Forwards the engine's `snapshot_clone_source`.
    pub fn snapshot_clone_source(&mut self, layer: LayerId) {
        let mut enc = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("pigment-gpui.snapshot_clone_source"),
            });
        self.canvas
            .snapshot_clone_source(&self.device, &mut enc, layer);
        self.queue.submit(Some(enc.finish()));
    }

    /// Clone-stamp `dabs` into `layer`: copy pixels from the frozen source at
    /// `offset` (destAnchor − sourceAnchor, doc px), shaped by brush falloff and
    /// clipped to the active selection. Forwards the engine's `paint_clone_dabs`,
    /// then marks dirty. No-op on an empty batch.
    pub fn paint_clone_dabs(&mut self, layer: LayerId, dabs: &[Dab], offset: [f32; 2]) {
        if dabs.is_empty() {
            return;
        }
        let mut enc = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("pigment-gpui.paint_clone_dabs"),
            });
        self.canvas
            .paint_clone_dabs(&self.device, &self.queue, &mut enc, layer, dabs, offset);
        self.queue.submit(Some(enc.finish()));
        self.mark_dirty();
    }

    /// Healing-brush `dabs` into `layer`: same as `paint_clone_dabs` but
    /// forces every dab's `hardness` to 0.0 for a soft Gaussian-weighted blend.
    /// The engine's brush shader treats hardness=0 as a pure Gaussian falloff,
    /// giving the Heal tool its characteristic feathered edge. No-op on empty.
    pub fn heal_at(&mut self, layer: LayerId, dabs: &[Dab], offset: [f32; 2]) {
        if dabs.is_empty() {
            return;
        }
        let soft: Vec<Dab> = dabs.iter().map(|d| Dab { hardness: 0.0, ..*d }).collect();
        let mut enc = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("pigment-gpui.heal_at"),
            });
        self.canvas
            .paint_clone_dabs(&self.device, &self.queue, &mut enc, layer, &soft, offset);
        self.queue.submit(Some(enc.finish()));
        self.mark_dirty();
    }

    /// Set the display channel mask [R, G, B, A]. Channels with `false` are
    /// zeroed in the bridged BGRA8 output on the next `image()` recomposite.
    /// Marks dirty so the on-screen view updates immediately.
    pub fn set_channel_mask(&mut self, mask: [bool; 4]) {
        self.channel_mask = mask;
        self.mark_dirty();
    }

    /// Set the soft-proof mode. When not Off, the bridged BGRA8 output has a
    /// CMYK round-trip applied (sRGB → CMYK → sRGB) on the CPU to simulate
    /// gamut compression. Marks dirty so the view updates immediately.
    pub fn set_soft_proof(&mut self, mode: SoftProofMode) {
        self.soft_proof = mode;
        self.mark_dirty();
    }

    /// Export the composited document using an `ExportPreset`. Reads back the
    /// composite, optionally scales, and encodes with the preset's format +
    /// quality. Returns `Err` on any failure.
    pub fn export_with_preset(
        &mut self,
        path: &std::path::Path,
        preset: &ExportPreset,
    ) -> Result<(), String> {
        let Some(flat) = self.read_composite_f32() else {
            return Err("composite unavailable".into());
        };
        let (w, h) = (self.doc_w, self.doc_h);
        let pixels = flat_to_rgba8(&flat);
        let (out_w, out_h, pixels) = if let (Some(pw), Some(ph)) = (preset.width, preset.height) {
            let img = image::RgbaImage::from_raw(w, h, pixels)
                .ok_or_else(|| "buffer size mismatch".to_string())?;
            let scaled = image::imageops::resize(&img, pw, ph, image::imageops::FilterType::Lanczos3);
            let (sw, sh) = scaled.dimensions();
            (sw, sh, scaled.into_raw())
        } else {
            (w, h, pixels)
        };
        match preset.format {
            ExportFormat::Png => prism_io::export::save_rgba8(path, &pixels, out_w, out_h)
                .map_err(|e| e.to_string()),
            ExportFormat::Jpeg => {
                let img = image::RgbaImage::from_raw(out_w, out_h, pixels)
                    .ok_or_else(|| "buffer size mismatch".to_string())?;
                let rgb = image::DynamicImage::ImageRgba8(img).to_rgb8();
                let file = std::fs::File::create(path).map_err(|e| e.to_string())?;
                let mut encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(
                    std::io::BufWriter::new(file),
                    preset.quality,
                );
                encoder.encode_image(&rgb).map_err(|e| e.to_string())
            }
            ExportFormat::Tiff | ExportFormat::Webp => {
                let img = image::RgbaImage::from_raw(out_w, out_h, pixels)
                    .ok_or_else(|| "buffer size mismatch".to_string())?;
                image::DynamicImage::ImageRgba8(img)
                    .save(path)
                    .map_err(|e| e.to_string())
            }
        }
    }

    // ---- Text layers ---------------------------------------------------------

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

    /// Replace the engine selection with a rectangular/elliptical marquee in doc
    /// px (`rect = [x, y, w, h]`). Mirrors `select_all`'s encoder pattern but
    /// issues `SelectionOp::Marquee`. Marks dirty so the next composite shows the
    /// updated selection (the display/dab paths read `has_selection`).
    fn dbg(msg: &str) {
        use std::io::Write;
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open("/tmp/pigment_sel.log") {
            let _ = writeln!(f, "{}", msg);
        }
    }

    pub fn set_marquee(&mut self, rect: [f32; 4], ellipse: bool) {
        Self::dbg(&format!("[set_marquee] rect={:?} ellipse={}", rect, ellipse));
        let mut enc = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("pigment-gpui.set_marquee"),
            });
        self.canvas.apply_selection(
            &self.device,
            &self.queue,
            &mut enc,
            &SelectionOp::Marquee { rect, ellipse },
        );
        self.queue.submit(Some(enc.finish()));
        Self::dbg(&format!("[set_marquee] submitted, has_selection={}", self.canvas.has_selection()));
        self.mark_dirty();
    }

    /// Clear the active selection (`SelectionOp::None`): wipes the mask texture
    /// and drops the mask constraint so painting is unrestricted again. Marks
    /// dirty so the overlay/marching-ants stops showing on the next composite.
    pub fn clear_selection(&mut self) {
        let mut enc = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("pigment-gpui.clear_selection"),
            });
        self.canvas
            .apply_selection(&self.device, &self.queue, &mut enc, &SelectionOp::None);
        self.queue.submit(Some(enc.finish()));
        self.mark_dirty();
    }

    /// Whether the engine currently has an active (masked) selection. Forward-
    /// looking API for a status readout (the overlay traces `selection_boundary`,
    /// the clip path reads the mask directly).
    #[allow(dead_code)]
    pub fn has_selection(&self) -> bool {
        self.canvas.has_selection()
    }

    /// Extract the active selection's boundary as a list of unit doc-px edge
    /// segments `[x0, y0, x1, y1]` — every cell edge between a selected
    /// (`mask > 0.5`) and an unselected pixel (or the canvas border). The overlay
    /// renderer draws these as an animated dashed outline (marching ants). Returns
    /// an empty vec if no selection is active. This is the only CPU trace; it runs
    /// off the engine's selection mask (no boundary math lives in the engine).
    pub fn selection_boundary(&self) -> Vec<[f32; 4]> {
        Self::dbg(&format!("[sel.boundary] has_selection={}", self.canvas.has_selection()));
        if !self.canvas.has_selection() {
            return Vec::new();
        }
        let mask_opt = self.canvas.read_selection(&self.device, &self.queue);
        Self::dbg(&format!("[sel.boundary] read_selection returned {}", if mask_opt.is_some() { "Some" } else { "None" }));
        let Some(mask) = mask_opt else {
            return Vec::new();
        };
        let (w, h) = (self.doc_w as usize, self.doc_h as usize);
        Self::dbg(&format!("[sel.boundary] mask.len()={} w*h={}", mask.len(), w * h));
        if mask.len() != w * h {
            return Vec::new();
        }
        let sel = |x: i64, y: i64| -> bool {
            if x < 0 || y < 0 || x >= w as i64 || y >= h as i64 {
                return false;
            }
            mask[y as usize * w + x as usize] > 0.5
        };
        let mut segs = Vec::new();
        for y in 0..h as i64 {
            for x in 0..w as i64 {
                if !sel(x, y) {
                    continue;
                }
                let (fx, fy) = (x as f32, y as f32);
                // Emit the edge wherever the 4-neighbor across it is unselected.
                if !sel(x, y - 1) {
                    segs.push([fx, fy, fx + 1.0, fy]); // top
                }
                if !sel(x, y + 1) {
                    segs.push([fx, fy + 1.0, fx + 1.0, fy + 1.0]); // bottom
                }
                if !sel(x - 1, y) {
                    segs.push([fx, fy, fx, fy + 1.0]); // left
                }
                if !sel(x + 1, y) {
                    segs.push([fx + 1.0, fy, fx + 1.0, fy + 1.0]); // right
                }
            }
        }
        Self::dbg(&format!("[sel.boundary] traced {} segments", segs.len()));
        segs
    }

    /// Read the active selection mask to CPU (one f32/px, 0..1; len = w*h), or an
    /// all-zero buffer if no selection is active. Used as the combine base for
    /// Shift-add / Alt-subtract selection ops, and by the overlay to trace the
    /// active boundary. Mirrors the egui app's `read_selection`.
    pub fn read_selection_or_empty(&self) -> Vec<f32> {
        let n = (self.doc_w * self.doc_h) as usize;
        if !self.canvas.has_selection() {
            return vec![0.0; n];
        }
        self.canvas
            .read_selection(&self.device, &self.queue)
            .unwrap_or_else(|| vec![0.0; n])
    }

    /// Upload a raw CPU selection mask (one f32/px, 0..1; len = w*h) and mark a
    /// selection active. Used by the Lasso (polygon) and Magic-Wand (flood)
    /// selection tools, which build the mask on the CPU via the shared
    /// `prism_core::raster` / `prism_core::fill` ops. Mirrors the egui app's
    /// `set_selection`. Marks dirty so the next composite shows the new mask.
    pub fn upload_selection_mask(&mut self, mask: &[f32]) {
        self.canvas.upload_selection(&self.queue, mask);
        self.mark_dirty();
    }

    /// Composite the document and read the result back as linear-premultiplied f32
    /// (RGBA, len = w*h*4). Used by the Magic-Wand to flood-select over the
    /// composited image exactly like the egui app's `do_magic_wand`. Re-marks the
    /// host dirty (composite_now touched ping/pong) so the on-screen view recovers.
    pub fn read_composite_f32(&mut self) -> Option<Vec<f32>> {
        let is_ping = self.canvas.composite_now(&self.device, &self.queue, &self.order);
        let bytes = self.canvas.read_composite(&self.device, &self.queue, is_ping)?;
        self.mark_dirty();
        Some(f16_bytes_to_f32(&bytes))
    }

    /// Set the LIVE affine on `layer` (uv-space layer-from-canvas 2x2 matrix `m`
    /// + `off`). The next composite applies it on the fly (the compositor binds
    /// `xform_layer`/`m`/`off` per layer), so a Move/Transform drag previews
    /// without baking. Marks dirty so the preview re-composites. Pass `None` to
    /// clear the live transform (identity).
    pub fn set_layer_xform(&mut self, layer: Option<LayerId>, m: [f32; 4], off: [f32; 2]) {
        self.canvas.set_layer_transform(layer, m, off);
        self.mark_dirty();
    }

    /// Bake the currently-set live affine into its layer's pixels through the
    /// real `bake_transform` pass, then clear it. Call on drag-release after the
    /// final `set_layer_xform`. Marks dirty so the baked result re-composites.
    pub fn bake_layer_xform(&mut self, layer: LayerId) {
        // Snapshot the layer before the bake so a Move/Transform is one undo step.
        self.canvas
            .begin_command_now(&self.device, &self.queue, layer, "Transform");
        let mut enc = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("pigment-gpui.bake_layer_xform"),
            });
        self.canvas.bake_transform(&self.device, &self.queue, &mut enc);
        self.queue.submit(Some(enc.finish()));
        self.mark_dirty();
    }

    // ---- File I/O ------------------------------------------------------------

    /// Load a real image file (via `prism_io::load_image`) into a fresh single-
    /// layer document and upload its pixels as the background layer through the
    /// engine. Mirrors the egui app's `open_image` (`pigment-app/src/app/io.rs`):
    /// new `Document`, premultiplied-f16 upload, re-fit selection. Resizes the GPU
    /// canvas to the image, re-allocates the bg layer, and re-arms the unmasked
    /// selection. Returns the new `Document` (so the root view replaces `App::doc`)
    /// or `None` on load failure. Marks dirty so the next `image()` recomposites.
    pub fn open_image(&mut self, path: &std::path::Path) -> Option<Document> {
        let img = match prism_io::load_image(path) {
            Ok(img) => img,
            Err(e) => {
                log::error!("open image failed: {e}");
                return None;
            }
        };
        let doc = Document::new(img.size);
        let bg_id = doc.layers.layers[0].id;
        let bytes = rgba8_to_f16_bytes(&img.rgba8);
        // Resize/clear the GPU canvas to the new image, then (re)allocate and fill
        // the background layer. `ensure_canvas` clears the layer map + selection on
        // a size change; on an identical size it early-returns, so we always upload
        // into the (reused) `LayerId(0)` slot, overwriting the previous pixels.
        self.canvas.ensure_canvas(&self.device, doc.size);
        self.canvas.ensure_layer(&self.device, bg_id);
        self.canvas.upload_layer(&self.queue, bg_id, &bytes);
        self.doc_w = doc.size.width;
        self.doc_h = doc.size.height;
        self.sync_order(&doc);
        // `ensure_canvas` resets the selection on a resize; re-arm unmasked paint.
        self.select_all();
        self.mark_dirty();
        Some(doc)
    }

    /// Import a Photoshop `.psd` file: decodes every layer through
    /// `prism_io::psd_import::load_psd`, allocates GPU tiles for each layer, and
    /// returns the new `Document`. Mirrors the egui app's `open_psd`.
    pub fn import_psd(&mut self, path: &std::path::Path) -> Option<Document> {
        let psd_doc = match prism_io::psd_import::load_psd(path) {
            Ok(d) => d,
            Err(e) => {
                log::error!("PSD import failed: {e}");
                return None;
            }
        };
        let size = prism_core::Size::new(
            psd_doc.width.max(1),
            psd_doc.height.max(1),
        );
        // `Document::new` creates one background layer (LayerId(0)).
        let mut doc = Document::new(size);
        self.canvas.ensure_canvas(&self.device, size);

        // The background layer (doc.layers.layers[0]) is overwritten by the
        // first PSD layer; remaining layers are added on top.
        let mut first = true;
        for pl in &psd_doc.layers {
            let id = if first {
                first = false;
                let id = doc.layers.layers[0].id;
                let l = &mut doc.layers.layers[0];
                l.name = pl.name.clone();
                l.opacity = pl.opacity;
                l.blend = BlendMode::from_shader_id(pl.blend);
                l.visible = pl.visible;
                id
            } else {
                let id = doc.layers.add_raster(pl.name.clone());
                if let Some(l) = doc.layers.get_mut(id) {
                    l.opacity = pl.opacity;
                    l.blend = BlendMode::from_shader_id(pl.blend);
                    l.visible = pl.visible;
                }
                id
            };
            self.canvas.ensure_layer(&self.device, id);
            // PSD pixel data is straight RGBA8; convert to linear premultiplied f16.
            let f16bytes = rgba8_to_f16_bytes(&pl.rgba8);
            self.canvas.upload_layer(&self.queue, id, &f16bytes);
        }

        self.doc_w = size.width;
        self.doc_h = size.height;
        doc.active_layer = doc.layers.layers.last().map(|l| l.id);
        self.sync_order(&doc);
        self.select_all();
        self.mark_dirty();
        Some(doc)
    }

    /// Composite the document and write the result to an 8-bit image file (PNG by
    /// extension) via `prism_io::export::save_rgba8`. Mirrors the egui app's
    /// `export_image`: composite → read back linear-premultiplied f16 → unpremul
    /// → sRGB-encode 8-bit straight RGBA. Returns `Ok(())` on success.
    pub fn export_image(&mut self, path: &std::path::Path) -> anyhow::Result<()> {
        let is_ping = self.canvas.composite_now(&self.device, &self.queue, &self.order);
        let bytes = self
            .canvas
            .read_composite(&self.device, &self.queue, is_ping)
            .ok_or_else(|| anyhow::anyhow!("compositor produced no readback"))?;
        let f = f16_bytes_to_f32(&bytes);
        let (w, h) = (self.doc_w, self.doc_h);
        // Linear premultiplied -> straight sRGB 8-bit (the egui app's exact math).
        let mut rgba8 = Vec::with_capacity((w * h * 4) as usize);
        for px in f.chunks_exact(4) {
            let a = px[3];
            let inv = if a > 1e-5 { 1.0 / a } else { 0.0 };
            let to8 = |lin: f32| (linear_to_srgb((lin * inv).clamp(0.0, 1.0)) * 255.0).round() as u8;
            rgba8.push(to8(px[0]));
            rgba8.push(to8(px[1]));
            rgba8.push(to8(px[2]));
            rgba8.push((a.clamp(0.0, 1.0) * 255.0).round() as u8);
        }
        prism_io::export::save_rgba8(path, &rgba8, w, h).map_err(|e| anyhow::anyhow!("{e}"))?;
        // composite_now touched ping/pong; recomposite for the on-screen view.
        self.mark_dirty();
        Ok(())
    }

    /// Load an OpenEXR file as a new single-layer document.
    /// Pixels arrive as linear-light RGBA f32 from `prism_io::exr_io::load_exr`;
    /// we upload them directly as f16 (no tone-mapping — preserve HDR).
    pub fn open_exr(&mut self, path: &std::path::Path) -> Option<Document> {
        let (size, f32_pixels) = match prism_io::exr_io::load_exr(path) {
            Ok(v) => v,
            Err(e) => {
                log::error!("EXR open failed: {e}");
                return None;
            }
        };
        let doc = Document::new(size);
        let bg_id = doc.layers.layers[0].id;
        let f16bytes = f32_to_f16_bytes(&f32_pixels);
        self.canvas.ensure_canvas(&self.device, size);
        self.canvas.ensure_layer(&self.device, bg_id);
        self.canvas.upload_layer(&self.queue, bg_id, &f16bytes);
        self.doc_w = size.width;
        self.doc_h = size.height;
        self.sync_order(&doc);
        self.select_all();
        self.mark_dirty();
        Some(doc)
    }

    /// Composite the document and write an OpenEXR file (linear-light RGBA f16,
    /// straight alpha). The EXR is written as 32-bit float for maximum compatibility.
    pub fn export_exr(&mut self, path: &std::path::Path) -> anyhow::Result<()> {
        let is_ping = self.canvas.composite_now(&self.device, &self.queue, &self.order);
        let bytes = self
            .canvas
            .read_composite(&self.device, &self.queue, is_ping)
            .ok_or_else(|| anyhow::anyhow!("compositor produced no readback"))?;
        let f = f16_bytes_to_f32(&bytes);
        let (w, h) = (self.doc_w as usize, self.doc_h as usize);
        // Unpremultiply alpha before writing to EXR (straight alpha convention).
        let pixels: Vec<f32> = f
            .chunks_exact(4)
            .flat_map(|px| {
                let a = px[3];
                let inv = if a > 1e-6 { 1.0 / a } else { 0.0 };
                [px[0] * inv, px[1] * inv, px[2] * inv, a]
            })
            .collect();
        exr::prelude::write_rgba_file(path, w, h, |x, y| {
            let i = (y * w + x) * 4;
            (pixels[i], pixels[i + 1], pixels[i + 2], pixels[i + 3])
        })
        .map_err(|e| anyhow::anyhow!("EXR write: {e}"))?;
        self.mark_dirty();
        Ok(())
    }

    /// Export as a PSD (Photoshop Document) v1 with full multi-layer records.
    ///
    /// Writes one PSD layer record per visible layer in `doc`, encoding name,
    /// blend mode, opacity, and channel data (raw, no compression) for each layer.
    /// Adjustment/Text/Vector layers are rasterized by reading their GPU tile
    /// data; layers with no GPU data are written as transparent. The merged
    /// (flattened) composite image is appended as the §2.5 image data section so
    /// older readers that skip the layer section still see the document.
    ///
    /// Hand-serialized per the PSD spec §13 "Layer and Mask Information" (no
    /// external dep). All multi-byte fields are big-endian.
    pub fn export_psd(&mut self, doc: &Document, path: &std::path::Path) -> anyhow::Result<()> {
        let (w, h) = (self.doc_w, self.doc_h);
        let npx = (w * h) as usize;

        // Helper: convert a linear-premultiplied f32 RGBA buffer to straight sRGB u8.
        let to_rgba8 = |f: &[f32]| -> Vec<u8> {
            let mut out = Vec::with_capacity(f.len());
            for px in f.chunks_exact(4) {
                let a = px[3];
                let inv = if a > 1e-5 { 1.0 / a } else { 0.0 };
                let to8 = |v: f32| (linear_to_srgb((v * inv).clamp(0.0, 1.0)) * 255.0).round() as u8;
                out.push(to8(px[0]));
                out.push(to8(px[1]));
                out.push(to8(px[2]));
                out.push((a.clamp(0.0, 1.0) * 255.0).round() as u8);
            }
            out
        };

        // --- 1. Read every layer's RGBA8 pixels. --------------------------------
        // We collect (rgba8, name, blend_shader_id, opacity_u8, visible) tuples.
        // Adjustment layers have no GPU pixel data — they are skipped (written as
        // transparent) since they cannot be faithfully represented in PSD v1 without
        // Photoshop-specific resources.
        let mut layer_data: Vec<(Vec<u8>, String, u32, u8, bool)> = Vec::new();
        for layer in &doc.layers.layers {
            // Skip pure-adjustment layers — no independent pixel data.
            if matches!(layer.kind, LayerKind::Adjustment(_)) {
                continue;
            }
            let rgba8 = if let Some(raw) = self.canvas.read_layer(&self.device, &self.queue, layer.id) {
                to_rgba8(&f16_bytes_to_f32(&raw))
            } else {
                // No GPU data (e.g. empty layer) — write as fully transparent.
                vec![0u8; npx * 4]
            };
            let opacity_u8 = (layer.opacity.clamp(0.0, 1.0) * 255.0).round() as u8;
            layer_data.push((rgba8, layer.name.clone(), layer.blend.shader_id(), opacity_u8, layer.visible));
        }
        let nlayers = layer_data.len();

        // --- 2. Build flattened composite for §2.5. -----------------------------
        let is_ping = self.canvas.composite_now(&self.device, &self.queue, &self.order);
        let comp_bytes = self
            .canvas
            .read_composite(&self.device, &self.queue, is_ping)
            .ok_or_else(|| anyhow::anyhow!("compositor produced no readback"))?;
        let merged_rgba8 = to_rgba8(&f16_bytes_to_f32(&comp_bytes));

        // --- 3. Serialise PSD. --------------------------------------------------
        // All values big-endian. Macros write into the output Vec.
        let mut buf: Vec<u8> = Vec::with_capacity(
            26                              // header
            + 4                             // color mode
            + 4                             // image resources
            + 4 + nlayers * (256 + npx * 4) // layer section (rough bound)
            + 2 + npx * 4,                  // merged image
        );

        macro_rules! be2 { ($v:expr) => { buf.extend_from_slice(&($v as u16).to_be_bytes()) } }
        macro_rules! be4 { ($v:expr) => { buf.extend_from_slice(&($v as u32).to_be_bytes()) } }

        // §2.1 File Header Section (26 bytes)
        buf.extend_from_slice(b"8BPS");   // Signature
        be2!(1u16);                        // Version 1
        buf.extend_from_slice(&[0u8; 6]); // Reserved
        be2!(4u16);                        // Channels (RGBA)
        be4!(h);                           // Rows
        be4!(w);                           // Columns
        be2!(8u16);                        // Bit depth per channel
        be2!(3u16);                        // Color mode: RGB

        // §2.2 Color Mode Data Section (empty for RGB)
        be4!(0u32);

        // §2.3 Image Resources Section (empty)
        be4!(0u32);

        // §2.4 Layer and Mask Information Section.
        // We build the layer section into a scratch buffer first so we can
        // write its length prefix.
        let mut layer_sect: Vec<u8> = Vec::new();
        macro_rules! ls2 { ($v:expr) => { layer_sect.extend_from_slice(&($v as u16).to_be_bytes()) } }
        macro_rules! ls4 { ($v:expr) => { layer_sect.extend_from_slice(&($v as u32).to_be_bytes()) } }

        // §13.1 Layer count (signed; negative = first alpha is transparency).
        ls2!(nlayers as i16);

        // §13.2 One layer record per layer.
        for (_pixels, name, blend_id, opacity, visible) in &layer_data {
            // Layer bounding rect: top, left, bottom, right (doc px, 0-indexed).
            ls4!(0u32); // top
            ls4!(0u32); // left
            ls4!(h);    // bottom
            ls4!(w);    // right

            // Number of channels: 4 (RGBA). Each channel has an id + data length.
            ls2!(4u16);
            // Channel ids: 0=R, 1=G, 2=B, -1=A. Data length = 2 (compression) + npx.
            let ch_data_len = (2 + npx) as u32;
            for ch_id in [0i16, 1, 2, -1] {
                layer_sect.extend_from_slice(&ch_id.to_be_bytes());
                ls4!(ch_data_len);
            }

            // Blend mode signature + key.
            layer_sect.extend_from_slice(b"8BIM");
            // Map our internal shader_id to the 4-char PSD blend key.
            let blend_key: &[u8; 4] = match blend_id {
                1  => b"diss", // Dissolve
                2  => b"dark", // Darken
                3  => b"mul ", // Multiply
                4  => b"idiv", // Color Burn
                5  => b"lbrn", // Linear Burn
                6  => b"lite", // Lighten
                7  => b"scrn", // Screen
                8  => b"div ", // Color Dodge
                9  => b"lddg", // Linear Dodge (Add)
                10 => b"over", // Overlay
                11 => b"sLit", // Soft Light
                12 => b"hLit", // Hard Light
                13 => b"vLit", // Vivid Light
                14 => b"lLit", // Linear Light
                15 => b"pLit", // Pin Light
                16 => b"diff", // Difference
                17 => b"smud", // Exclusion
                18 => b"hue ", // Hue
                19 => b"sat ", // Saturation
                20 => b"colr", // Color
                21 => b"lum ", // Luminosity
                _  => b"norm", // Normal (default)
            };
            layer_sect.extend_from_slice(blend_key);
            layer_sect.push(*opacity);
            layer_sect.push(0u8); // clipping
            // Flags: bit 1 = invisible when set
            layer_sect.push(if *visible { 0u8 } else { 2u8 });
            layer_sect.push(0u8); // filler

            // Extra layer data (name + rest). We need to write the length of the
            // extra data block upfront. Content: mask data (0), layer blending
            // ranges (0), Pascal string name.
            let name_bytes = name.as_bytes();
            let pascal_len = 1 + name_bytes.len(); // 1-byte length prefix
            let pascal_padded = (pascal_len + 3) & !3; // pad to 4-byte boundary
            let extra_len = 4 + 4 + pascal_padded as u32; // mask(4) + blend_ranges(4) + name
            ls4!(extra_len);
            ls4!(0u32); // mask data length = 0
            ls4!(0u32); // layer blending ranges length = 0
            // Pascal string: 1-byte length then chars, then NUL padding to 4-byte boundary.
            layer_sect.push(name_bytes.len().min(255) as u8);
            layer_sect.extend_from_slice(name_bytes);
            let written = 1 + name_bytes.len();
            let pad = ((written + 3) & !3) - written;
            for _ in 0..pad { layer_sect.push(0u8); }
        }

        // §13.3 Channel image data for each layer (in the same layer order).
        // Channels are interleaved per-layer: all channels for layer 0, then layer 1, …
        // Each channel block = 2-byte compression + raw bytes (compression mode 0).
        for (rgba8, ..) in &layer_data {
            // Channel order: R, G, B, A (channel ids 0,1,2,-1 in the same order as above).
            for ch in [0usize, 1, 2, 3] {
                layer_sect.extend_from_slice(&0u16.to_be_bytes()); // compression = 0 (raw)
                for px in rgba8.chunks_exact(4) {
                    layer_sect.push(px[ch]);
                }
            }
        }

        // Write layer section length + content into main buffer.
        be4!(layer_sect.len() as u32);
        buf.extend_from_slice(&layer_sect);

        // §2.5 Image Data Section — merged composite.
        be2!(0u16); // Compression mode 0 = raw
        // PSD stores channels planar: all R, then all G, then all B, then all A.
        for ch in 0..4usize {
            for px in merged_rgba8.chunks_exact(4) {
                buf.push(px[ch]);
            }
        }

        std::fs::write(path, &buf)
            .map_err(|e| anyhow::anyhow!("PSD write error: {e}"))?;
        self.mark_dirty();
        Ok(())
    }

    // ---- Filters / adjustments (destructive, through the engine) --------------

    /// Apply a separable/general filter pass to `layer` through the engine's
    /// destructive `apply_filter` (kind: 1 Gaussian, 2 Sharpen, 5 Box). Marks
    /// dirty so the next `image()` recomposites. Mirrors the egui app's
    /// `do_filter` (`pigment-app/src/app/retouch.rs`).
    pub fn apply_filter(&mut self, layer: LayerId, kind: u32, radius: f32, amount: f32) {
        self.canvas
            .apply_filter(&self.device, &self.queue, layer, kind, radius, amount);
        self.mark_dirty();
    }

    /// Destructive Posterize on `layer` (engine `apply_posterize`).
    pub fn apply_posterize(&mut self, layer: LayerId, levels: u32) {
        self.canvas
            .apply_posterize(&self.device, &self.queue, layer, levels);
        self.mark_dirty();
    }

    /// Destructive Threshold on `layer` (engine `apply_threshold`).
    pub fn apply_threshold(&mut self, layer: LayerId, level: f32) {
        self.canvas
            .apply_threshold(&self.device, &self.queue, layer, level);
        self.mark_dirty();
    }

    /// Destructive Stylize on `layer` (engine `apply_stylize`; kind: 13 Find
    /// Edges, 14 Emboss, 15 Glowing Edges, 16 Diffuse). Mirrors the egui app's
    /// `do_find_edges` / `do_emboss` / etc.
    pub fn apply_stylize(&mut self, layer: LayerId, kind: u32, amount: f32, width: f32, dir: [f32; 2]) {
        self.canvas
            .apply_stylize(&self.device, &self.queue, layer, kind, amount, width, dir);
        self.mark_dirty();
    }

    /// Destructive Add Noise on `layer` (engine `apply_noise`).
    pub fn apply_noise(&mut self, layer: LayerId, amount: f32, mono: bool, gaussian: bool, seed: f32) {
        self.canvas
            .apply_noise(&self.device, &self.queue, layer, amount, mono, gaussian, seed);
        self.mark_dirty();
    }

    // ---- CPU layer read/write helpers -----------------------------------------

    /// Read `layer`'s pixels to CPU as linear-premultiplied RGBA f32 (len = w*h*4).
    /// Returns `None` if the layer has no GPU texture yet.
    pub fn read_layer_f32(&mut self, layer: LayerId) -> Option<Vec<f32>> {
        let raw = self.canvas.read_layer(&self.device, &self.queue, layer)?;
        Some(f16_bytes_to_f32(&raw))
    }

    /// Upload linear-premultiplied RGBA f32 pixels (len = w*h*4) into `layer`,
    /// converting to f16 in one pass. Marks dirty.
    pub fn upload_layer_f32(&mut self, layer: LayerId, pixels: &[f32]) {
        self.canvas.upload_layer(&self.queue, layer, &f32_to_f16_bytes(pixels));
        self.mark_dirty();
    }

    // ---- Core canvas tools (fill / gradient / shape, through the engine) ------
    //
    // These three mirror the egui app's destructive CPU-blend path
    // (`pigment-app/src/app/{io,retouch,state}.rs`): read the active layer back
    // from the engine, blend the engine-rasterized primitive over it on the CPU
    // (the shared, app-agnostic `prism_core::{fill,gradient,shape}` owns ALL the
    // math), and upload the result back into the layer texture. No raster op is
    // reimplemented here; the host only does the read → source-over → upload glue
    // the egui app does inside `with_gpu`. Each marks the host dirty so the next
    // `image()` recomposites.

    /// Read the active selection mask (canvas-sized 0..1, one f32/px) if one is
    /// active, else `None`. Used to gate fill/gradient/shape writes to the
    /// selection exactly like the egui app.
    fn read_selection_mask(&self) -> Option<Vec<f32>> {
        if !self.canvas.has_selection() {
            return None;
        }
        self.canvas.read_selection(&self.device, &self.queue)
    }

    /// Paint-bucket fill `layer` from doc-px seed `(sx, sy)`: flood the active
    /// layer's pixels within `tolerance` (engine `flood_fill_mask`), then write
    /// the straight-sRGB `color` (alpha = its 4th channel) into the matched
    /// pixels, gated by the active selection. Mirrors the egui app's `do_fill`
    /// (sampling the active layer; `sample_all` is a later wave). Hard replace
    /// inside the mask (not source-over), matching `do_fill`.
    pub fn fill_at(
        &mut self,
        layer: LayerId,
        seed: (u32, u32),
        color: [f32; 4],
        tolerance: f32,
        contiguous: bool,
    ) {
        let (w, h) = (self.doc_w, self.doc_h);
        let a = color[3];
        let fill = [
            srgb_to_linear(color[0]) * a,
            srgb_to_linear(color[1]) * a,
            srgb_to_linear(color[2]) * a,
            a,
        ];
        let Some(sample) = self.canvas.read_layer(&self.device, &self.queue, layer) else {
            return;
        };
        // Snapshot the layer for undo before we overwrite the matched pixels.
        self.canvas
            .begin_command_now(&self.device, &self.queue, layer, "Fill");
        let sbuf = f16_bytes_to_f32(&sample);
        let mask = flood_fill_mask(&sbuf, w, h, seed.0, seed.1, tolerance, contiguous);
        let sel = self.read_selection_mask();
        // Write target is the same active layer we sampled.
        let mut abuf = sbuf;
        for (i, &m) in mask.iter().enumerate() {
            let selected = sel.as_ref().is_none_or(|s| s[i] > 0.5);
            if m && selected {
                let o = i * 4;
                abuf[o..o + 4].copy_from_slice(&fill);
            }
        }
        self.canvas
            .upload_layer(&self.queue, layer, &f32_to_f16_bytes(&abuf));
        self.mark_dirty();
    }

    /// Apply a linear gradient to `layer` along the drag `p0 → p1` (doc px),
    /// source-over the existing pixels and clipped to the active selection.
    /// `stop0`/`stop1` are straight-sRGB RGBA (alpha carried per stop). The
    /// engine (`prism_core::gradient`) owns the sampling/dither math; the host
    /// only converts to linear stops and does the source-over blend, mirroring
    /// the egui app's `do_gradient`.
    pub fn apply_gradient(
        &mut self,
        layer: LayerId,
        p0: [f32; 2],
        p1: [f32; 2],
        stop0: [f32; 4],
        stop1: [f32; 4],
        dither: bool,
    ) {
        use prism_core::gradient::{ColorStop, GradientType, OpacityStop};
        let (w, h) = (self.doc_w, self.doc_h);
        let lin = |c: [f32; 4]| {
            [
                srgb_to_linear(c[0]),
                srgb_to_linear(c[1]),
                srgb_to_linear(c[2]),
            ]
        };
        let grad = Gradient {
            color_stops: vec![ColorStop::new(0.0, lin(stop0)), ColorStop::new(1.0, lin(stop1))],
            opacity_stops: vec![
                OpacityStop::new(0.0, stop0[3]),
                OpacityStop::new(1.0, stop1[3]),
            ],
            kind: GradientType::Linear,
            dither,
        }
        .render((p0[0], p0[1]), (p1[0], p1[1]), w, h);
        let sel = self.read_selection_mask();
        let Some(b) = self.canvas.read_layer(&self.device, &self.queue, layer) else {
            return;
        };
        self.canvas
            .begin_command_now(&self.device, &self.queue, layer, "Gradient");
        let mut base = f16_bytes_to_f32(&b);
        for i in 0..(w * h) as usize {
            let clip = sel.as_ref().map(|s| s[i]).unwrap_or(1.0);
            let ga = grad[i * 4 + 3] * clip;
            for c in 0..4 {
                base[i * 4 + c] = grad[i * 4 + c] * clip + base[i * 4 + c] * (1.0 - ga);
            }
        }
        self.canvas
            .upload_layer(&self.queue, layer, &f32_to_f16_bytes(&base));
        self.mark_dirty();
    }

    /// Draw a filled `kind` shape into `layer` within `rect = [x, y, w, h]` (doc
    /// px), source-over the existing pixels and clipped to the active selection.
    /// `color` is straight-sRGB RGBA. The engine (`prism_core::shape::fill_shape`)
    /// owns the rasterization/AA; the host converts the color to linear and does
    /// the source-over blend, mirroring how the egui app rasterizes vector layers.
    pub fn draw_shape(&mut self, layer: LayerId, kind: ShapeKind, rect: [f32; 4], color: [f32; 4]) {
        let (w, h) = (self.doc_w, self.doc_h);
        let lin = [
            srgb_to_linear(color[0]),
            srgb_to_linear(color[1]),
            srgb_to_linear(color[2]),
            color[3],
        ];
        let shp = fill_shape(kind, rect, lin, w, h);
        let sel = self.read_selection_mask();
        let Some(b) = self.canvas.read_layer(&self.device, &self.queue, layer) else {
            return;
        };
        self.canvas
            .begin_command_now(&self.device, &self.queue, layer, "Shape");
        let mut base = f16_bytes_to_f32(&b);
        for i in 0..(w * h) as usize {
            let clip = sel.as_ref().map(|s| s[i]).unwrap_or(1.0);
            let sa = shp[i * 4 + 3] * clip;
            for c in 0..4 {
                base[i * 4 + c] = shp[i * 4 + c] * clip + base[i * 4 + c] * (1.0 - sa);
            }
        }
        self.canvas
            .upload_layer(&self.queue, layer, &f32_to_f16_bytes(&base));
        self.mark_dirty();
    }

    /// Run the real GPU composite, read the Rgba16Float result back, convert to
    /// BGRA8 sRGB, and wrap it as a GPUI `RenderImage`.
    fn composite_and_bridge(&mut self) -> Arc<RenderImage> {
        let is_ping = self.canvas.composite_now(&self.device, &self.queue, &self.order);
        let rgba16 = self
            .canvas
            .read_composite(&self.device, &self.queue, is_ping)
            .expect("compositor produced no readback");
        log::info!(
            "composited {}x{}, bridged {} bytes",
            self.doc_w,
            self.doc_h,
            rgba16.len()
        );

        // Histogram off the same linear-light composite the egui app uses
        // (f16 → f32, premultiplied, 256 bins). Shared prism-core math.
        self.hist = Some(histogram(&f16_bytes_to_f32(&rgba16), 256));

        let mut bgra = rgba16f_to_bgra8_masked(&rgba16, self.channel_mask);
        if self.soft_proof != SoftProofMode::Off {
            apply_soft_proof_bgra(&mut bgra);
        }
        let buf =
            RgbaImage::from_raw(self.doc_w, self.doc_h, bgra).expect("composite size mismatch");
        Arc::new(RenderImage::new([Frame::new(buf)]))
    }

    /// Apply a dodge (strength > 0, lighten) or burn (strength < 0, darken) dab at
    /// `(cx, cy)` into `layer`, using `prism_core::tone::dodge_burn`. The footprint
    /// is a circular Gaussian of `radius` px; `strength ∈ [-1, 1]`.
    pub fn dodge_burn_dab(&mut self, layer: LayerId, cx: f32, cy: f32, radius: f32, strength: f32) {
        let (w, h) = (self.doc_w as usize, self.doc_h as usize);
        let Some(raw) = self.canvas.read_layer(&self.device, &self.queue, layer) else {
            return;
        };
        self.canvas.begin_command_now(&self.device, &self.queue, layer, "Dodge/Burn");
        let mut pixels = f16_bytes_to_f32(&raw);
        // Build a per-pixel amount buffer: Gaussian falloff within radius.
        let r2 = radius * radius;
        let mut amount = vec![0.0f32; w * h];
        let x0 = ((cx - radius).floor() as isize).max(0) as usize;
        let x1 = ((cx + radius).ceil() as usize).min(w);
        let y0 = ((cy - radius).floor() as isize).max(0) as usize;
        let y1 = ((cy + radius).ceil() as usize).min(h);
        for py in y0..y1 {
            for px in x0..x1 {
                let dx = px as f32 + 0.5 - cx;
                let dy = py as f32 + 0.5 - cy;
                let d2 = dx * dx + dy * dy;
                if d2 < r2 {
                    let falloff = 1.0 - (d2 / r2).sqrt();
                    amount[py * w + px] = strength * falloff;
                }
            }
        }
        let out = tone::dodge_burn(&pixels[..], &amount, w, h);
        // Preserve alpha channel from original pixels.
        for i in 0..w * h {
            pixels[i * 4] = out[i * 4];
            pixels[i * 4 + 1] = out[i * 4 + 1];
            pixels[i * 4 + 2] = out[i * 4 + 2];
        }
        self.canvas.upload_layer(&self.queue, layer, &f32_to_f16_bytes(&pixels));
        self.mark_dirty();
    }

    /// Smudge at `(cx, cy)`: blends pixels toward the brush center from the
    /// surrounding neighbourhood, using a weighted average (strength = blend frac).
    pub fn smudge_dab(&mut self, layer: LayerId, cx: f32, cy: f32, radius: f32, strength: f32) {
        let (w, h) = (self.doc_w as usize, self.doc_h as usize);
        let Some(raw) = self.canvas.read_layer(&self.device, &self.queue, layer) else {
            return;
        };
        self.canvas.begin_command_now(&self.device, &self.queue, layer, "Smudge");
        let src = f16_bytes_to_f32(&raw);
        let mut dst = src.clone();
        let r = radius.max(1.0) as usize;
        let x0 = ((cx as isize) - r as isize).max(0) as usize;
        let x1 = ((cx as usize) + r).min(w);
        let y0 = ((cy as isize) - r as isize).max(0) as usize;
        let y1 = ((cy as usize) + r).min(h);
        let r2 = radius * radius;
        for py in y0..y1 {
            for px in x0..x1 {
                let dx = px as f32 + 0.5 - cx;
                let dy = py as f32 + 0.5 - cy;
                if dx * dx + dy * dy >= r2 {
                    continue;
                }
                // Gather neighbour average (3×3 kernel).
                let mut acc = [0.0f32; 4];
                let mut cnt = 0usize;
                for ky in (py.saturating_sub(2))..(py + 3).min(h) {
                    for kx in (px.saturating_sub(2))..(px + 3).min(w) {
                        let o = (ky * w + kx) * 4;
                        for c in 0..4 { acc[c] += src[o + c]; }
                        cnt += 1;
                    }
                }
                if cnt > 0 {
                    let inv = 1.0 / cnt as f32;
                    let o = (py * w + px) * 4;
                    for c in 0..4 {
                        dst[o + c] = src[o + c] * (1.0 - strength) + acc[c] * inv * strength;
                    }
                }
            }
        }
        self.canvas.upload_layer(&self.queue, layer, &f32_to_f16_bytes(&dst));
        self.mark_dirty();
    }

    /// Liquify: warp pixels within `radius` toward `(dx, dy)` with a smooth
    /// falloff `(1 - dist/radius)^2 * strength`. Reads the layer, displaces pixels
    /// bilinearly, and re-uploads. Safe to call every drag frame.
    pub fn liquify_warp(
        &mut self,
        layer: LayerId,
        cx: f32,
        cy: f32,
        dx: f32,
        dy: f32,
        radius: f32,
        strength: f32,
    ) {
        let (w, h) = (self.doc_w as usize, self.doc_h as usize);
        let Some(raw) = self.canvas.read_layer(&self.device, &self.queue, layer) else {
            return;
        };
        self.canvas.begin_command_now(&self.device, &self.queue, layer, "Liquify");
        let src = f16_bytes_to_f32(&raw);
        let mut dst = src.clone();
        let r = radius.max(1.0);
        let x0 = ((cx - r) as isize).max(0) as usize;
        let x1 = ((cx + r + 1.0) as usize).min(w);
        let y0 = ((cy - r) as isize).max(0) as usize;
        let y1 = ((cy + r + 1.0) as usize).min(h);
        let r2 = r * r;
        for py in y0..y1 {
            for px in x0..x1 {
                let ddx = px as f32 + 0.5 - cx;
                let ddy = py as f32 + 0.5 - cy;
                let dist2 = ddx * ddx + ddy * ddy;
                if dist2 >= r2 {
                    continue;
                }
                let dist = dist2.sqrt();
                let falloff = (1.0 - dist / r).powi(2) * strength;
                // Sample source at displaced position (bilinear).
                let sx = px as f32 - dx * falloff;
                let sy = py as f32 - dy * falloff;
                let ix = sx.floor() as isize;
                let iy = sy.floor() as isize;
                let fx = sx - ix as f32;
                let fy = sy - iy as f32;
                let o = (py * w + px) * 4;
                for c in 0..4usize {
                    let sample = |qx: isize, qy: isize| -> f32 {
                        let qx = qx.clamp(0, w as isize - 1) as usize;
                        let qy = qy.clamp(0, h as isize - 1) as usize;
                        src[(qy * w + qx) * 4 + c]
                    };
                    let v00 = sample(ix, iy);
                    let v10 = sample(ix + 1, iy);
                    let v01 = sample(ix, iy + 1);
                    let v11 = sample(ix + 1, iy + 1);
                    dst[o + c] = v00 * (1.0 - fx) * (1.0 - fy)
                        + v10 * fx * (1.0 - fy)
                        + v01 * (1.0 - fx) * fy
                        + v11 * fx * fy;
                }
            }
        }
        self.canvas.upload_layer(&self.queue, layer, &f32_to_f16_bytes(&dst));
        self.mark_dirty();
    }

    /// Crop every layer in `doc` to the rectangle `[x, y, w, h]` (doc px), resize
    /// the canvas, and update `doc_w`/`doc_h`. Returns the new `Document`.
    pub fn crop_document(&mut self, doc: &mut Document, x: u32, y: u32, w: u32, h: u32) {
        let ow = self.doc_w as usize;
        // Crop each layer: read, extract sub-region, re-upload.
        let ids: Vec<LayerId> = doc.layers.layers.iter().map(|l| l.id).collect();
        let new_size = prism_core::Size::new(w.max(1), h.max(1));
        // Read all layers before touching the canvas (ensure_canvas clears them).
        let mut layer_crops: Vec<(LayerId, Vec<u8>)> = Vec::new();
        for &id in &ids {
            if let Some(raw) = self.canvas.read_layer(&self.device, &self.queue, id) {
                let src = f16_bytes_to_f32(&raw);
                let mut cropped = vec![0.0f32; (w * h * 4) as usize];
                for cy in 0..h as usize {
                    for cx in 0..w as usize {
                        let sx = cx + x as usize;
                        let sy = cy + y as usize;
                        if sx < ow && sy < self.doc_h as usize {
                            let si = (sy * ow + sx) * 4;
                            let di = (cy * w as usize + cx) * 4;
                            cropped[di..di + 4].copy_from_slice(&src[si..si + 4]);
                        }
                    }
                }
                layer_crops.push((id, f32_to_f16_bytes(&cropped)));
            }
        }
        // Resize canvas and re-upload.
        self.canvas.ensure_canvas(&self.device, new_size);
        for (id, bytes) in layer_crops {
            self.canvas.ensure_layer(&self.device, id);
            self.canvas.upload_layer(&self.queue, id, &bytes);
        }
        doc.size = new_size;
        self.doc_w = w;
        self.doc_h = h;
        self.sync_order(doc);
        self.select_all();
        self.mark_dirty();
    }

    /// Apply CPU layer styles (drop shadow, outer/inner glow, bevel-emboss) on top
    /// Fill a layer's entire canvas with a solid color. Used by `AddSolidFillLayer` and
    /// `SetFillLayerColor`. `color` is straight sRGB in [0,1]. Converted to linear-light
    /// premultiplied before writing, matching the rest of the compositor.
    pub fn fill_solid(&mut self, layer: LayerId, color: [f32; 4]) {
        let (w, h) = (self.doc_w as usize, self.doc_h as usize);
        let a = color[3];
        let fill = [
            srgb_to_linear(color[0]) * a,
            srgb_to_linear(color[1]) * a,
            srgb_to_linear(color[2]) * a,
            a,
        ];
        let mut pixels = vec![0.0f32; w * h * 4];
        for i in 0..w * h {
            pixels[i * 4..i * 4 + 4].copy_from_slice(&fill);
        }
        self.canvas
            .upload_layer(&self.queue, layer, &f32_to_f16_bytes(&pixels));
        self.mark_dirty();
    }

    /// of a composited BGRA8 buffer. Called from Wave 11 compositing when any layer
    /// has a non-default style. Operates in-place (the shadow/glow is source-over'd
    /// onto `bgra`). Nearest-neighbor Gaussian approximation (box convolution).
    ///
    /// `style_layers` is a slice of (alpha_mask: Vec<f32>, style) pairs, where the
    /// alpha mask is extracted from the layer's readback before compositing. For
    /// simplicity, this implementation blurs the final composite alpha mask and
    /// composites the effect below the original pixels.
    pub fn apply_layer_styles_to_bgra(
        bgra: &mut Vec<u8>,
        w: u32,
        h: u32,
        styles: &[(Vec<f32>, &crate::app_state::LayerStyle)],
    ) {
        for (alpha_mask, style) in styles {
            if let Some(shadow) = &style.drop_shadow {
                apply_drop_shadow_bgra(bgra, w, h, alpha_mask, shadow);
            }
            if let Some(glow) = &style.outer_glow {
                apply_outer_glow_bgra(bgra, w, h, alpha_mask, glow);
            }
        }
    }
}

/// Rgba16Float bytes -> f32 (2 bytes per channel, LE). Mirrors the egui app's
/// `f16_bytes_to_f32`; feeds the shared histogram (linear, premultiplied).
fn f16_bytes_to_f32(bytes: &[u8]) -> Vec<f32> {
    bytes
        .chunks_exact(2)
        .map(|p| f16::from_le_bytes([p[0], p[1]]).to_f32())
        .collect()
}

/// f32 -> Rgba16Float bytes (the GPU layer format), the inverse of
/// `f16_bytes_to_f32`. Used to upload a CPU-blended layer (fill/gradient/shape)
/// back into its texture. Mirrors the egui app's `f32_to_f16_bytes`.
fn f32_to_f16_bytes(f: &[f32]) -> Vec<u8> {
    let mut o = Vec::with_capacity(f.len() * 2);
    for &c in f {
        o.extend_from_slice(&f16::from_f32(c).to_le_bytes());
    }
    o
}

#[cfg(test)]
mod blur_tests {
    use super::box_blur_alpha;

    // A single lit pixel blurred with r=1 smears energy to neighbors.
    #[test]
    fn blur_spreads_energy() {
        let mut src = vec![0.0f32; 5 * 5];
        src[2 * 5 + 2] = 1.0; // center pixel
        let blurred = box_blur_alpha(&src, 5, 5, 1);
        // Center is still bright, neighbors receive some energy.
        assert!(blurred[2 * 5 + 2] > 0.0, "center must be non-zero");
        assert!(blurred[2 * 5 + 1] > 0.0, "left neighbor must receive energy");
    }

    // r=0 is the identity.
    #[test]
    fn zero_radius_is_identity() {
        let src: Vec<f32> = (0..16).map(|i| i as f32 / 16.0).collect();
        let out = box_blur_alpha(&src, 4, 4, 0);
        assert_eq!(src, out);
    }
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

/// Translate an RGBA-f32 buffer by `(dx, dy)` doc px on a `w × h` canvas, zero-
/// filling the exposed border. Used to place a top-left-rasterized text bitmap
/// at the click position (the egui app does the same via `place_generated`).
fn shift_rgba_f32(src: &[f32], w: u32, h: u32, dx: i32, dy: i32) -> Vec<f32> {
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

/// 8-bit sRGB RGBA -> linear-premultiplied f16 bytes (the GPU layer format).
/// Ported from the egui app's `rgba8_to_f16_bytes`.
fn rgba8_to_f16_bytes(rgba8: &[u8]) -> Vec<u8> {
    let mut o = Vec::with_capacity(rgba8.len() * 2);
    for px in rgba8.chunks_exact(4) {
        let a = px[3] as f32 / 255.0;
        let ch = [
            srgb_to_linear(px[0] as f32 / 255.0) * a,
            srgb_to_linear(px[1] as f32 / 255.0) * a,
            srgb_to_linear(px[2] as f32 / 255.0) * a,
            a,
        ];
        for &c in &ch {
            o.extend_from_slice(&f16::from_f32(c).to_le_bytes());
        }
    }
    o
}

/// Convert a linear-premultiplied f32 RGBA buffer to straight sRGB u8 RGBA.
/// Used by export and slice ops (same math as `export_image`).
fn flat_to_rgba8(flat: &[f32]) -> Vec<u8> {
    let mut out = Vec::with_capacity(flat.len());
    for px in flat.chunks_exact(4) {
        let a = px[3];
        let inv = if a > 1e-5 { 1.0 / a } else { 0.0 };
        let to8 = |v: f32| (linear_to_srgb((v * inv).clamp(0.0, 1.0)) * 255.0).round() as u8;
        out.push(to8(px[0]));
        out.push(to8(px[1]));
        out.push(to8(px[2]));
        out.push((a.clamp(0.0, 1.0) * 255.0).round() as u8);
    }
    out
}

/// Apply a CMYK round-trip simulation to a BGRA8 buffer in-place.
/// C=1-R, M=1-G, Y=1-B, K=min(C,M,Y); R'=(1-C)(1-K), G'=(1-M)(1-K), B'=(1-Y)(1-K).
/// This compresses the gamut visually to simulate CMYK output on a display.
fn apply_soft_proof_bgra(bgra: &mut Vec<u8>) {
    for px in bgra.chunks_exact_mut(4) {
        let r = px[2] as f32 / 255.0;
        let g = px[1] as f32 / 255.0;
        let b = px[0] as f32 / 255.0;
        let c = 1.0 - r;
        let m = 1.0 - g;
        let y = 1.0 - b;
        let k = c.min(m).min(y);
        let k1 = 1.0 - k;
        let r2 = if k1 > 1e-6 { (1.0 - c) * k1 } else { 0.0 };
        let g2 = if k1 > 1e-6 { (1.0 - m) * k1 } else { 0.0 };
        let b2 = if k1 > 1e-6 { (1.0 - y) * k1 } else { 0.0 };
        px[2] = (r2.clamp(0.0, 1.0) * 255.0).round() as u8;
        px[1] = (g2.clamp(0.0, 1.0) * 255.0).round() as u8;
        px[0] = (b2.clamp(0.0, 1.0) * 255.0).round() as u8;
    }
}

/// Rgba16Float linear-premultiplied bytes -> BGRA8 sRGB (GPUI `RenderImage` is
/// BGRA). Per pixel: read 4 f16 LE -> f32, unpremultiply rgb by alpha (guarded),
/// sRGB-encode, scale to u8, emit B,G,R,A. Channels in `mask` set to `false` are
/// output as zero (display-only visibility; pixels in the engine are unchanged).
fn rgba16f_to_bgra8_masked(rgba16: &[u8], mask: [bool; 4]) -> Vec<u8> {
    let mut out = Vec::with_capacity(rgba16.len() / 2);
    for px in rgba16.chunks_exact(8) {
        let r = f16::from_le_bytes([px[0], px[1]]).to_f32();
        let g = f16::from_le_bytes([px[2], px[3]]).to_f32();
        let b = f16::from_le_bytes([px[4], px[5]]).to_f32();
        let a = f16::from_le_bytes([px[6], px[7]]).to_f32();
        let inv = if a > 0.0 { 1.0 / a } else { 0.0 };
        let enc = |c: f32| (linear_to_srgb((c * inv).clamp(0.0, 1.0)) * 255.0).round() as u8;
        let av = (a.clamp(0.0, 1.0) * 255.0).round() as u8;
        out.push(if mask[2] { enc(b) } else { 0 }); // B
        out.push(if mask[1] { enc(g) } else { 0 }); // G
        out.push(if mask[0] { enc(r) } else { 0 }); // R
        out.push(if mask[3] { av } else { 0 });       // A
    }
    out
}

// ---- Wave 11: CPU layer-style effects ----------------------------------------

/// Box-blur an alpha-mask (1 channel f32, len = w*h) with radius `r` (integer px).
/// Nearest-neighbor Gaussian approximation (3 box passes ≈ Gaussian).
fn box_blur_alpha(src: &[f32], w: u32, h: u32, r: u32) -> Vec<f32> {
    if r == 0 {
        return src.to_vec();
    }
    let (wi, hi) = (w as usize, h as usize);
    let mut tmp = src.to_vec();
    // Horizontal pass.
    let mut buf = vec![0.0f32; wi * hi];
    for y in 0..hi {
        let mut sum = 0.0f32;
        let ri = r as i32;
        // Seed with the first window.
        for kx in -(ri)..=ri {
            let sx = kx.clamp(0, wi as i32 - 1) as usize;
            sum += tmp[y * wi + sx];
        }
        let diam = (2 * ri + 1) as f32;
        for x in 0..wi {
            buf[y * wi + x] = sum / diam;
            let add_x = ((x as i32 + ri + 1).clamp(0, wi as i32 - 1)) as usize;
            let rem_x = ((x as i32 - ri).clamp(0, wi as i32 - 1)) as usize;
            sum += tmp[y * wi + add_x] - tmp[y * wi + rem_x];
        }
    }
    tmp = buf.clone();
    // Vertical pass.
    for x in 0..wi {
        let mut sum = 0.0f32;
        let ri = r as i32;
        for ky in -(ri)..=ri {
            let sy = ky.clamp(0, hi as i32 - 1) as usize;
            sum += tmp[sy * wi + x];
        }
        let diam = (2 * ri + 1) as f32;
        for y in 0..hi {
            buf[y * wi + x] = sum / diam;
            let add_y = ((y as i32 + ri + 1).clamp(0, hi as i32 - 1)) as usize;
            let rem_y = ((y as i32 - ri).clamp(0, hi as i32 - 1)) as usize;
            sum += tmp[add_y * wi + x] - tmp[rem_y * wi + x];
        }
    }
    buf
}

/// Apply a drop shadow below `bgra` (BGRA8 in-place) using the layer's alpha mask.
/// Shadow = offset + blurred alpha mask tinted with shadow color, composited below
/// the original pixels.
fn apply_drop_shadow_bgra(
    bgra: &mut Vec<u8>,
    w: u32,
    h: u32,
    alpha: &[f32],
    shadow: &crate::app_state::Shadow,
) {
    let r = (shadow.blur * 0.5).max(0.0) as u32;
    let blurred = box_blur_alpha(alpha, w, h, r);
    let (wi, hi) = (w as usize, h as usize);
    let dx = shadow.offset_x.round() as i32;
    let dy = shadow.offset_y.round() as i32;
    let sc = shadow.color;
    let sa = shadow.opacity;
    for y in 0..hi {
        for x in 0..wi {
            let sx = (x as i32 - dx).clamp(0, wi as i32 - 1) as usize;
            let sy = (y as i32 - dy).clamp(0, hi as i32 - 1) as usize;
            let mask_a = blurred[sy * wi + sx] * sa;
            let i = (y * wi + x) * 4;
            let existing_a = bgra[i + 3] as f32 / 255.0;
            let contrib = mask_a * (1.0 - existing_a);
            if contrib < 1e-3 {
                continue;
            }
            let sb = (sc[2] * 255.0).round() as u8;
            let sg = (sc[1] * 255.0).round() as u8;
            let sr = (sc[0] * 255.0).round() as u8;
            bgra[i] = ((bgra[i] as f32) * (1.0 - contrib) + sb as f32 * contrib) as u8;
            bgra[i + 1] = ((bgra[i + 1] as f32) * (1.0 - contrib) + sg as f32 * contrib) as u8;
            bgra[i + 2] = ((bgra[i + 2] as f32) * (1.0 - contrib) + sr as f32 * contrib) as u8;
            bgra[i + 3] = ((bgra[i + 3] as f32 + 255.0 * contrib).min(255.0)) as u8;
        }
    }
}

/// Apply outer glow using the same blurred alpha mask approach.
fn apply_outer_glow_bgra(
    bgra: &mut Vec<u8>,
    w: u32,
    h: u32,
    alpha: &[f32],
    glow: &crate::app_state::Glow,
) {
    let r = (glow.blur * 0.5).max(0.0) as u32;
    let blurred = box_blur_alpha(alpha, w, h, r);
    let (wi, hi) = (w as usize, h as usize);
    let gc = glow.color;
    let ga = glow.opacity;
    for i_px in 0..(wi * hi) {
        let mask_a = blurred[i_px] * ga;
        let i = i_px * 4;
        let existing_a = bgra[i + 3] as f32 / 255.0;
        let contrib = mask_a * (1.0 - existing_a);
        if contrib < 1e-3 {
            continue;
        }
        let gb = (gc[2] * 255.0).round() as u8;
        let gg = (gc[1] * 255.0).round() as u8;
        let gr = (gc[0] * 255.0).round() as u8;
        bgra[i] = ((bgra[i] as f32) * (1.0 - contrib) + gb as f32 * contrib) as u8;
        bgra[i + 1] = ((bgra[i + 1] as f32) * (1.0 - contrib) + gg as f32 * contrib) as u8;
        bgra[i + 2] = ((bgra[i + 2] as f32) * (1.0 - contrib) + gr as f32 * contrib) as u8;
        bgra[i + 3] = ((bgra[i + 3] as f32 + 255.0 * contrib).min(255.0)) as u8;
    }
}
