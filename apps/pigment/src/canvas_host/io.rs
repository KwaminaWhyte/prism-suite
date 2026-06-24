//! File I/O domain of `CanvasHost`: open / import / export across PNG, JPEG,
//! TIFF/WebP, OpenEXR, and a hand-serialized multi-layer PSD writer, plus the
//! `export_with_preset` path and the `flat_to_rgba8` conversion helper.

use prism_core::color::linear_to_srgb;
use prism_core::{BlendMode, Document, LayerKind};

use super::{f16_bytes_to_f32, f32_to_f16_bytes, rgba8_to_f16_bytes, CanvasHost};
use crate::app_state::{ExportFormat, ExportPreset};

impl CanvasHost {
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
