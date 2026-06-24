//! Owns a wgpu device and drives the REAL prism-canvas compositor, bridging the
//! composited document into GPUI via CPU readback.
//!
//! This is the first real vertical slice: a placeholder document is uploaded as
//! a layer, composited on the GPU through `CanvasGpu`, read back as Rgba16Float
//! (linear-premultiplied), and converted to BGRA8 sRGB for a GPUI `RenderImage`.
//! The result is cached and only recomputed when the host is marked dirty.
//!
//! The `CanvasHost` API is split across sibling domain modules:
//! - `paint`   — selection ops, dabs, undo/redo, clone/heal, dodge/burn, smudge, liquify
//! - `tools`   — fill / gradient / shape / fill_solid, transforms, crop, composite readback
//! - `text`    — text-layer rasterize / update (+ `shift_rgba_f32`)
//! - `io`       — file open / import / export (PNG / JPEG / EXR / PSD / preset)
//! - `bridge`  — composite→BGRA bridging + CPU layer-style effects

use std::sync::Arc;

use gpui::RenderImage;
use half::f16;
use image::{Frame, RgbaImage};
use prism_canvas::{wgpu, CanvasGpu, LayerDraw};
use prism_core::color::srgb_to_linear;
use prism_core::LayerId;
use prism_core::histogram::{histogram, Histogram};
use prism_core::{Adjustment, Document, LayerKind, Size};
use crate::app_state::SoftProofMode;

mod bridge;
mod io;
mod paint;
mod text;
mod tools;

use bridge::{apply_soft_proof_bgra, rgba16f_to_bgra8_masked};

/// wgpu device + the real compositor, with a cached bridged `RenderImage`.
pub struct CanvasHost {
    pub(super) device: wgpu::Device,
    pub(super) queue: wgpu::Queue,
    pub(super) canvas: CanvasGpu,
    pub(super) order: Vec<LayerDraw>,
    pub doc_w: u32,
    pub doc_h: u32,
    pub(super) cached: Option<Arc<RenderImage>>,
    /// 256-bin histogram of the last composite (linear-light, matches egui app).
    pub(super) hist: Option<Histogram>,
    pub(super) dirty: bool,
    /// Display channel mask [R, G, B, A]. Channels set to `false` are zeroed in
    /// the bridged BGRA8 output (display only — pixels are unchanged in engine).
    pub(super) channel_mask: [bool; 4],
    /// Soft-proof mode: when not Off, apply CMYK round-trip to bridged output.
    pub(super) soft_proof: SoftProofMode,
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
}

/// Rgba16Float bytes -> f32 (2 bytes per channel, LE). Mirrors the egui app's
/// `f16_bytes_to_f32`; feeds the shared histogram (linear, premultiplied).
pub(super) fn f16_bytes_to_f32(bytes: &[u8]) -> Vec<f32> {
    bytes
        .chunks_exact(2)
        .map(|p| f16::from_le_bytes([p[0], p[1]]).to_f32())
        .collect()
}

/// f32 -> Rgba16Float bytes (the GPU layer format), the inverse of
/// `f16_bytes_to_f32`. Used to upload a CPU-blended layer (fill/gradient/shape)
/// back into its texture. Mirrors the egui app's `f32_to_f16_bytes`.
pub(super) fn f32_to_f16_bytes(f: &[f32]) -> Vec<u8> {
    let mut o = Vec::with_capacity(f.len() * 2);
    for &c in f {
        o.extend_from_slice(&f16::from_f32(c).to_le_bytes());
    }
    o
}

/// 8-bit sRGB RGBA -> linear-premultiplied f16 bytes (the GPU layer format).
/// Ported from the egui app's `rgba8_to_f16_bytes`.
pub(super) fn rgba8_to_f16_bytes(rgba8: &[u8]) -> Vec<u8> {
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
