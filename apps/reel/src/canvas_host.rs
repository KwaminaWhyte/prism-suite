//! Bridges Reel's CPU-composited program frame into GPUI as a `RenderImage`.
//!
//! Reel does NOT use wgpu / prism-canvas — its program frame is composited
//! CPU-side (see [`crate::program_frame`], the headless sampler that reproduces
//! the program content at the playhead into an RGBA8 buffer). This host calls
//! that sampler for the current playhead frame, converts the straight-sRGB
//! RGBA8 result to BGRA8 (GPUI `RenderImage` is BGRA byte order), wraps it as a
//! `RenderImage`, and caches it — only re-sampling when the host is marked dirty
//! (after a seek, a track-visibility toggle, etc.). No real-time playback this
//! pass: a single frame is sampled on demand.

use std::sync::Arc;

use gpui::RenderImage;
use image::{Frame, RgbaImage};

use crate::app_state::{Caption, Project};
use crate::program_frame::{program_frame, FrameCache, GlobalGrade};

/// The CPU program-frame sampler bridge, with a cached `RenderImage` and a
/// decoded-video-frame cache.
pub struct CanvasHost {
    pub comp_w: u32,
    pub comp_h: u32,
    cached: Option<Arc<RenderImage>>,
    dirty: bool,
    /// Decoded video frames keyed by `(path, frame-index)`, reused across
    /// re-samples so scrubbing back to a frame doesn't re-decode it.
    frames: FrameCache,
    /// Raw RGBA8 pixels from the last sampled program frame (for scopes).
    pub last_rgba: Vec<u8>,
    /// Dimensions `(width, height)` of the last sampled frame.
    pub last_dims: (u32, u32),
}

impl CanvasHost {
    /// Boot the host (still dirty: the first `image(...)` samples the playhead
    /// frame and bridges it).
    pub fn new() -> Self {
        Self {
            comp_w: 0,
            comp_h: 0,
            cached: None,
            dirty: true,
            frames: FrameCache::new(),
            last_rgba: Vec::new(),
            last_dims: (0, 0),
        }
    }

    /// The bridged program image at playhead time `t`. Samples + bridges only
    /// when dirty; otherwise returns the cached `RenderImage`.
    pub fn image(
        &mut self,
        project: &Project,
        t: f32,
        global: &GlobalGrade,
        log_tracks: Option<&[bool]>,
        captions: &[Caption],
    ) -> Arc<RenderImage> {
        if !self.dirty {
            if let Some(c) = &self.cached {
                return c.clone();
            }
        }
        let rendered = self.sample_and_bridge(project, t, global, log_tracks, captions);
        self.cached = Some(rendered.clone());
        self.dirty = false;
        rendered
    }

    /// Force the next `image(...)` to re-sample + re-bridge. Call after any
    /// mutation that changes the composited frame (seek, track visibility).
    pub fn mark_dirty(&mut self) {
        self.dirty = true;
    }

    /// Run the headless CPU sampler for the playhead frame, convert RGBA8 →
    /// BGRA8, and wrap it as a GPUI `RenderImage`.
    fn sample_and_bridge(&mut self, project: &Project, t: f32, global: &GlobalGrade, log_tracks: Option<&[bool]>, captions: &[Caption]) -> Arc<RenderImage> {
        let frame = program_frame(project, t, &mut self.frames, global, log_tracks, captions);
        self.comp_w = frame.width;
        self.comp_h = frame.height;
        self.last_rgba = frame.rgba.clone();
        self.last_dims = (frame.width, frame.height);
        log::info!(
            "sampled program frame {}x{} at t={:.3}s, bridged {} bytes",
            frame.width,
            frame.height,
            t,
            frame.rgba.len()
        );

        let bgra = rgba8_to_bgra8(&frame.rgba);
        let buf =
            RgbaImage::from_raw(frame.width, frame.height, bgra).expect("program frame size mismatch");
        Arc::new(RenderImage::new([Frame::new(buf)]))
    }
}

impl Default for CanvasHost {
    fn default() -> Self {
        Self::new()
    }
}

/// Straight-sRGB RGBA8 -> BGRA8 (GPUI `RenderImage` is BGRA byte order). Per
/// pixel: emit B, G, R, A.
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
