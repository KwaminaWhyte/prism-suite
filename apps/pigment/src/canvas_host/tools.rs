//! Core canvas tools + destructive engine passes on `CanvasHost`:
//! composite readback, live transforms, crop, fill / gradient / shape / solid
//! fill, and the filter / posterize / threshold / stylize / noise passes.
//!
//! The fill / gradient / shape ops mirror the egui app's destructive CPU-blend
//! path (`pigment-app/src/app/{io,retouch,state}.rs`): read the active layer back
//! from the engine, blend the engine-rasterized primitive over it on the CPU (the
//! shared, app-agnostic `prism_core::{fill,gradient,shape}` owns ALL the math),
//! and upload the result back into the layer texture. No raster op is
//! reimplemented here; the host only does the read → source-over → upload glue
//! the egui app does inside `with_gpu`. Each marks the host dirty so the next
//! `image()` recomposites.

use prism_core::color::srgb_to_linear;
use prism_core::fill::flood_fill_mask;
use prism_core::gradient::Gradient;
use prism_core::shape::{fill_shape, ShapeKind};
use prism_core::LayerId;

use super::{f16_bytes_to_f32, f32_to_f16_bytes, CanvasHost};

impl CanvasHost {
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
            .create_command_encoder(&prism_canvas::wgpu::CommandEncoderDescriptor {
                label: Some("pigment-gpui.bake_layer_xform"),
            });
        self.canvas.bake_transform(&self.device, &self.queue, &mut enc);
        self.queue.submit(Some(enc.finish()));
        self.mark_dirty();
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

    /// Crop every layer in `doc` to the rectangle `[x, y, w, h]` (doc px), resize
    /// the canvas, and update `doc_w`/`doc_h`. Returns the new `Document`.
    pub fn crop_document(&mut self, doc: &mut prism_core::Document, x: u32, y: u32, w: u32, h: u32) {
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
}
