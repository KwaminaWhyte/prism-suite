//! Painting + selection domain of `CanvasHost`: dabs, undo/redo, clone/heal,
//! dodge/burn, smudge, liquify, selection ops, channel-mask / soft-proof setters.

use prism_canvas::{wgpu, Dab, SelectionOp};
use prism_core::tone;
use prism_core::LayerId;

use super::{f16_bytes_to_f32, f32_to_f16_bytes, CanvasHost};
use crate::app_state::SoftProofMode;

impl CanvasHost {
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
}
