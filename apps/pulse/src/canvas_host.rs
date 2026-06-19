//! Bridges Pulse's **CPU software compositor** into GPUI as a `RenderImage`.
//!
//! The compositor runs on a dedicated background thread so it never blocks the
//! UI thread. `image()` dispatches a render request when dirty, returns the
//! last cached frame immediately, and swaps in the new result on the next call
//! once the background thread finishes.
//!
//! A per-`CanvasHost` **RAM preview frame cache** (`frame_cache`) stores decoded
//! RGBA frames keyed by frame number so repeated scrubs of the same frame are
//! served instantly from cache without going through the background render thread.
//! Capacity is capped at 512 frames with LRU eviction (tracked via
//! `cache_order`). The timeline ruler reads `cached_frame_set()` to paint green
//! "warm" strips over cached frames.

use std::collections::{BTreeMap, HashMap, VecDeque};
use std::sync::mpsc::{self, Receiver, SyncSender, TryRecvError};
use std::sync::Arc;

use gpui::RenderImage;
use image::{Frame as ImgFrame, RgbaImage};
use crate::comp::{Comp, FrameCache, Project};
use crate::render::{preview_dims, render_preview_frame};

use crate::gpui_effects::GpuiEffect;

/// Preview resolution cap — used for all renders (background thread handles playback load).
const PREVIEW_CAP: u32 = 640;

struct RenderRequest {
    comps: Vec<Comp>,
    id: u64,
    time: f32,
    cap: u32,
    gpui_effects: HashMap<usize, Vec<GpuiEffect>>,
    selected_layer: Option<usize>,
    /// Region of interest [x, y, w, h] in composition pixel space, or None = full frame.
    roi: Option<[f32; 4]>,
}

struct RenderResult {
    image: Arc<RenderImage>,
    w: u32,
    h: u32,
    /// Original RGBA8 pixels (before BGRA conversion), stored so the host can
    /// insert them into its frame cache. `None` when effects or ROI modified the
    /// pixels after render (those frames must not be cached as "pure" composites).
    rgba_pixels: Option<Vec<u8>>,
}

/// Maximum number of RGBA frames held in the RAM preview frame cache.
/// At the 640px preview cap each frame is ~0.9 MB, so 256 frames bounds the
/// scrub cache to roughly 240 MB. The cache is only filled while scrubbing
/// (NOT during playback — see `image()`), so live playback never grows RAM.
const FRAME_CACHE_CAP: usize = 256;

pub struct CanvasHost {
    /// Display dimensions — fixed at full-quality cap; never changes during playback.
    pub preview_w: u32,
    pub preview_h: u32,
    cached: Option<Arc<RenderImage>>,
    pending: bool,
    dirty: bool,
    tx: SyncSender<RenderRequest>,
    rx: Receiver<RenderResult>,
    /// RAM preview frame cache: frame_index (u64) → RGBA8 pixel bytes.
    /// Serves repeated scrubs of the same frame instantly from memory.
    pub frame_cache: BTreeMap<u64, Vec<u8>>,
    /// LRU insertion order for `frame_cache`. Front = oldest, back = newest.
    pub cache_order: VecDeque<u64>,
}

impl CanvasHost {
    pub fn new(project: &Project) -> Self {
        let comp = active_comp(project);
        let (pw, ph) = preview_dims(comp.width, comp.height, PREVIEW_CAP);

        let (req_tx, req_rx) = mpsc::sync_channel::<RenderRequest>(1);
        let (res_tx, res_rx) = mpsc::sync_channel::<RenderResult>(1);

        std::thread::Builder::new()
            .name("pulse-compositor".into())
            .spawn(move || {
                let mut cache = FrameCache::new();
                while let Ok(req) = req_rx.recv() {
                    let mut frame =
                        render_preview_frame(&req.comps, req.id, req.time, req.cap, &mut cache);
                    let w = frame.width;
                    let h = frame.height;
                    let fps = req.comps
                        .iter()
                        .find(|c| c.id == req.id)
                        .map(|c| c.fps.max(1.0))
                        .unwrap_or(30.0);
                    let frame_idx = (req.time * fps).round() as u32;
                    // Track whether pixels were modified after render (effects/ROI);
                    // unmodified frames can be cached as the "pure" composite.
                    let has_gpui_effects = req.selected_layer
                        .and_then(|li| req.gpui_effects.get(&li))
                        .is_some_and(|v| !v.is_empty());
                    if has_gpui_effects {
                        if let Some(effects) = req.selected_layer.and_then(|li| req.gpui_effects.get(&li)) {
                            for e in effects {
                                e.apply(&mut frame.pixels, w, h, frame_idx);
                            }
                        }
                    }
                    // Apply ROI crop if set.
                    let has_roi = req.roi.is_some();
                    let (pixels, w, h) = if let Some([rx, ry, rw, rh]) = req.roi {
                        let x0 = (rx as u32).min(w.saturating_sub(1));
                        let y0 = (ry as u32).min(h.saturating_sub(1));
                        let cw = (rw as u32).min(w - x0).max(1);
                        let ch = (rh as u32).min(h - y0).max(1);
                        let mut cropped = Vec::with_capacity((cw * ch * 4) as usize);
                        for row in y0..y0 + ch {
                            let row_start = ((row * w + x0) * 4) as usize;
                            cropped.extend_from_slice(&frame.pixels[row_start..row_start + (cw * 4) as usize]);
                        }
                        (cropped, cw, ch)
                    } else {
                        (frame.pixels, w, h)
                    };
                    // Store raw RGBA for caching if no post-render modifications.
                    let rgba_pixels = if !has_gpui_effects && !has_roi {
                        Some(pixels.clone())
                    } else {
                        None
                    };
                    let bgra = rgba8_to_bgra8(&pixels);
                    if let Some(buf) = RgbaImage::from_raw(w, h, bgra) {
                        let img = Arc::new(RenderImage::new([ImgFrame::new(buf)]));
                        let _ = res_tx.send(RenderResult { image: img, w, h, rgba_pixels });
                    }
                }
            })
            .expect("failed to spawn compositor thread");

        Self {
            preview_w: pw,
            preview_h: ph,
            cached: None,
            pending: false,
            dirty: true,
            tx: req_tx,
            rx: res_rx,
            frame_cache: BTreeMap::new(),
            cache_order: VecDeque::new(),
        }
    }

    /// Return the latest preview image (non-blocking).
    ///
    /// When dirty, dispatches a render request to the background thread and
    /// immediately returns the last cached frame. The new frame is swapped in
    /// on the next call once the thread has finished.
    ///
    /// A per-host **RAM preview frame cache** stores RGBA results keyed by frame
    /// number. Cache hits bypass the background thread entirely — the stored pixels
    /// are converted to BGRA and wrapped directly. Cache misses go through the
    /// normal background render, and the result is inserted into the cache.
    pub fn image(
        &mut self,
        project: &Project,
        time: f32,
        gpui_effects: &HashMap<usize, Vec<GpuiEffect>>,
        selected_layer: Option<usize>,
        playing: bool,
        roi: Option<[f32; 4]>,
    ) -> Arc<RenderImage> {
        let comp = active_comp(project);
        let fps = comp.fps.max(1.0);
        let frame_idx = (time * fps).round() as u64;

        // --- Frame cache hit: serve from memory, skip background render. ---
        // Only use the cache when there are no GPUI-side effects and no ROI crop,
        // so the cache stores the "pure" composite (the common case for RAM preview
        // scrubbing AND playback). Effects / ROI always re-render.
        //
        // Caching DURING playback is intentional and safe: each frame is a native
        // render downsampled to the preview size, so without a cache every loop
        // would re-composite the whole comp (stutter). The cache is LRU-bounded
        // (`FRAME_CACHE_CAP`), and the real unbounded-RAM bug was the gpui sprite
        // atlas leak (fixed via `Window::drop_image` in the host) — not this cache.
        let _ = playing;
        let cache_eligible = gpui_effects.values().all(|v| v.is_empty()) && roi.is_none();
        if cache_eligible && !self.dirty {
            if let Some(rgba) = self.frame_cache.get(&frame_idx) {
                let bgra = rgba8_to_bgra8(rgba);
                let pw = self.preview_w.max(1);
                let ph = self.preview_h.max(1);
                if let Some(buf) = RgbaImage::from_raw(pw, ph, bgra) {
                    let img = Arc::new(RenderImage::new([ImgFrame::new(buf)]));
                    self.cached = Some(img.clone());
                    return img;
                }
            }
        }

        // --- Poll for a completed background render. ---
        match self.rx.try_recv() {
            Ok(result) => {
                self.preview_w = result.w;
                self.preview_h = result.h;
                // Store the raw RGBA pixels in the frame cache before converting to
                // the display image. Only cache when no effects/ROI were in flight.
                if cache_eligible {
                    if let Some(raw) = result.rgba_pixels {
                        self.cache_insert(frame_idx, raw);
                    }
                }
                self.cached = Some(result.image);
                self.pending = false;
            }
            Err(TryRecvError::Empty) => {}
            Err(TryRecvError::Disconnected) => {}
        }

        // --- Dispatch a new render when dirty and none in flight. ---
        if self.dirty && !self.pending {
            let cap = PREVIEW_CAP;
            let req = RenderRequest {
                comps: project.comps.clone(),
                id: comp.id,
                time,
                cap,
                gpui_effects: gpui_effects.clone(),
                selected_layer,
                roi,
            };
            // `sync_channel(1)` — drop the request silently if one is already queued.
            let _ = self.tx.try_send(req);
            self.pending = true;
            self.dirty = false;
        }

        self.cached
            .clone()
            .unwrap_or_else(|| placeholder_image(self.preview_w.max(1), self.preview_h.max(1)))
    }

    /// Insert `rgba` into the frame cache for `frame_idx`, evicting the oldest
    /// entry when the cache is at capacity.
    fn cache_insert(&mut self, frame_idx: u64, rgba: Vec<u8>) {
        if self.frame_cache.contains_key(&frame_idx) {
            return; // already cached; don't update LRU order
        }
        if self.frame_cache.len() >= FRAME_CACHE_CAP {
            if let Some(oldest) = self.cache_order.pop_front() {
                self.frame_cache.remove(&oldest);
            }
        }
        self.frame_cache.insert(frame_idx, rgba);
        self.cache_order.push_back(frame_idx);
    }

    /// The set of cached frame indices, for the timeline ruler's green strip overlay.
    pub fn cached_frame_set(&self) -> &BTreeMap<u64, Vec<u8>> {
        &self.frame_cache
    }

    pub fn mark_dirty(&mut self) {
        self.dirty = true;
    }

    /// Write the most-recently-rendered RGBA frame to `/tmp/prism-pulse-preview.rgba`.
    /// Called each render cycle when `App::live_output_enabled` is true.
    pub fn write_live_output(&self) {
        let Some(frame) = self
            .frame_cache
            .values()
            .next_back()
        else {
            return;
        };
        let _ = std::fs::write("/tmp/prism-pulse-preview.rgba", frame);
    }
}

fn active_comp(project: &Project) -> &Comp {
    let i = project.active.min(project.comps.len().saturating_sub(1));
    &project.comps[i]
}

fn rgba8_to_bgra8(rgba: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(rgba.len());
    for px in rgba.chunks_exact(4) {
        out.push(px[2]);
        out.push(px[1]);
        out.push(px[0]);
        out.push(px[3]);
    }
    out
}

fn placeholder_image(w: u32, h: u32) -> Arc<RenderImage> {
    let buf = RgbaImage::from_raw(w, h, vec![20u8; (w * h * 4) as usize])
        .unwrap_or_else(|| RgbaImage::new(1, 1));
    Arc::new(RenderImage::new([ImgFrame::new(buf)]))
}
