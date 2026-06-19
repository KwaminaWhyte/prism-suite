//! A headless CPU sampler that reproduces the program output at the playhead
//! into a straight-sRGB RGBA8 buffer — the bridge source for the GPUI preview.
//!
//! Reel renders its real preview *directly* through egui's painter (no readable
//! framebuffer) and decodes media CPU-side via the ffmpeg CLI (`prism_media`).
//! It has no `prism-canvas` / wgpu compositor to read back from. So — exactly as
//! the egui app's `program_frame.rs` does for the scopes — this module
//! reproduces the program's *content* on the CPU: it resolves the effective
//! clips active at the playhead on each visible track (bottom track first),
//! samples each clip's pixels, alpha-composites the tracks over one another with
//! the source-over operator honoring opacity, and flattens the result over
//! black.
//!
//! This pass samples flat-color clips, on-disk still images (via the shared
//! `prism_io::load_image`), and **real movie frames** (decoded at the playhead
//! via `prism_media::decode_frame_at`, the ffmpeg CLI bridge) — each aspect-fit
//! into the comp. Decoded video frames are cached by `(path, frame-index)` so
//! re-sampling on a redraw doesn't re-decode. It composites transitions, burns
//! the active caption cue, applies per-clip grades (basic + HSL secondary), and
//! renders nested sequences recursively (depth-guarded). It does NOT reproduce
//! geometric transforms or audio — a single playhead frame only (no real-time
//! playback). The
//! [`canvas_host`](crate::canvas_host) owns the cache and only re-samples when
//! the host is marked dirty.

use std::collections::HashMap;
use std::path::PathBuf;

use crate::app_state::{Caption, CaptionPosition, Clip, ClipSource, Project, SpeedCurve};

/// Longest preview dimension for a decoded video frame. Mirrors the egui app's
/// preview scale (≤960px) so a 4K source doesn't decode at full size for a
/// half-size on-screen preview.
const VIDEO_PREVIEW_MAX_DIM: u32 = 960;

/// Map local_t (0..duration) → source time using a cubic bezier speed curve.
/// The bezier maps normalized [0,1] timeline → [0,1] source fraction.
fn bezier_source_time(local_t: f32, duration: f32, p0: f32, p1: f32, p2: f32, p3: f32) -> f32 {
    if duration <= 0.0 { return 0.0; }
    let norm_t = (local_t / duration).clamp(0.0, 1.0);
    // Cubic bezier: B(t) = (1-t)^3*p0 + 3*(1-t)^2*t*p1 + 3*(1-t)*t^2*p2 + t^3*p3
    let u = 1.0 - norm_t;
    let frac = u*u*u*p0 + 3.0*u*u*norm_t*p1 + 3.0*u*norm_t*norm_t*p2 + norm_t*norm_t*norm_t*p3;
    frac.clamp(0.0, 1.0) * duration
}

/// Cache of decoded+fit video frames keyed by `(path, source frame index)`, so
/// the same playhead frame isn't re-decoded every redraw. An empty `Vec` marks
/// a decode that failed (missing ffmpeg / bad media) so we don't retry it every
/// frame. Owned by [`crate::canvas_host::CanvasHost`].
#[derive(Default)]
pub struct FrameCache {
    map: HashMap<(PathBuf, u64), Vec<u8>>,
}

impl FrameCache {
    /// A fresh, empty cache.
    pub fn new() -> Self {
        Self::default()
    }
}

/// Global grade applied after per-clip grade: optional 3D LUT trilinear
/// interpolation + white balance per-channel gain, and 3-way color wheels.
/// Passed from the host.
#[derive(Clone)]
pub struct GlobalGrade {
    /// Optional 3D LUT: `(size, table)`. `table.len() == size^3`.
    pub lut: Option<(u32, Vec<[f32; 3]>)>,
    /// Per-channel RGB gain from white balance.
    pub wb_gain: [f32; 3],
    /// 3-way color wheels (Lift / Gamma / Gain). Identity = default.
    pub color_wheels: crate::app_state::ColorWheels,
    /// Per-channel RGB tone curves. Identity = two-point linear.
    pub rgb_curves: crate::app_state::RgbCurves,
}

impl GlobalGrade {
    /// The identity global grade (no LUT, no white balance adjustment).
    pub fn identity() -> Self {
        Self {
            lut: None,
            wb_gain: [1.0, 1.0, 1.0],
            color_wheels: crate::app_state::ColorWheels::default(),
            rgb_curves: crate::app_state::RgbCurves::default(),
        }
    }

    /// True if this is effectively a no-op on any pixel.
    pub fn is_identity(&self) -> bool {
        self.lut.is_none()
            && (self.wb_gain[0] - 1.0).abs() < 1e-4
            && (self.wb_gain[1] - 1.0).abs() < 1e-4
            && (self.wb_gain[2] - 1.0).abs() < 1e-4
            && self.color_wheels.is_identity()
            && self.rgb_curves.is_identity()
    }

    /// Apply to one straight-sRGB pixel `rgb` (0..1) in place.
    pub fn apply(&self, rgb: &mut [f32; 3]) {
        for (v, &g) in rgb.iter_mut().zip(self.wb_gain.iter()) {
            *v = (*v * g).clamp(0.0, 1.0);
        }
        if let Some((size, table)) = &self.lut {
            let s = (*size as f32) - 1.0;
            let ri = (rgb[0] * s).clamp(0.0, s);
            let gi = (rgb[1] * s).clamp(0.0, s);
            let bi = (rgb[2] * s).clamp(0.0, s);
            let r0 = ri.floor() as usize;
            let g0 = gi.floor() as usize;
            let b0 = bi.floor() as usize;
            let r1 = (r0 + 1).min(*size as usize - 1);
            let g1 = (g0 + 1).min(*size as usize - 1);
            let b1 = (b0 + 1).min(*size as usize - 1);
            let rf = ri - ri.floor();
            let gf = gi - gi.floor();
            let bf = bi - bi.floor();
            let n = *size as usize;
            let idx = |r: usize, g: usize, b: usize| r + g * n + b * n * n;
            let c000 = table[idx(r0, g0, b0)];
            let c100 = table[idx(r1, g0, b0)];
            let c010 = table[idx(r0, g1, b0)];
            let c110 = table[idx(r1, g1, b0)];
            let c001 = table[idx(r0, g0, b1)];
            let c101 = table[idx(r1, g0, b1)];
            let c011 = table[idx(r0, g1, b1)];
            let c111 = table[idx(r1, g1, b1)];
            for ch in 0..3 {
                let v = c000[ch] * (1.0 - rf) * (1.0 - gf) * (1.0 - bf)
                    + c100[ch] * rf * (1.0 - gf) * (1.0 - bf)
                    + c010[ch] * (1.0 - rf) * gf * (1.0 - bf)
                    + c110[ch] * rf * gf * (1.0 - bf)
                    + c001[ch] * (1.0 - rf) * (1.0 - gf) * bf
                    + c101[ch] * rf * (1.0 - gf) * bf
                    + c011[ch] * (1.0 - rf) * gf * bf
                    + c111[ch] * rf * gf * bf;
                rgb[ch] = v.clamp(0.0, 1.0);
            }
        }
        // Apply 3-way color wheels after LUT.
        self.color_wheels.apply(rgb);
        // Apply per-channel RGB curves after color wheels.
        self.rgb_curves.apply(rgb);
    }
}

/// Convert color temperature (Kelvin) to per-channel RGB gain multipliers,
/// normalized so 6500K is all-ones. Derived from Tanner Helland's algorithm.
pub fn kelvin_to_rgb_gain(temp: f32) -> [f32; 3] {
    let t = temp.clamp(2000.0, 10000.0) / 100.0;
    let r = if t <= 66.0 {
        1.0
    } else {
        let v = 329.698_727_44 * (t - 60.0).powf(-0.133_204_759_2);
        (v / 255.0).clamp(0.0, 1.0)
    };
    let g = if t <= 66.0 {
        let v = 99.470_802_59 * t.ln() - 161.119_568_17;
        (v / 255.0).clamp(0.0, 1.0)
    } else {
        let v = 288.122_169_93 * (t - 60.0).powf(-0.075_514_849_2);
        (v / 255.0).clamp(0.0, 1.0)
    };
    let b = if t >= 66.0 {
        1.0
    } else if t <= 19.0 {
        0.0
    } else {
        let v = 138.517_730_8 * (t - 10.0).ln() - 305.044_792_17;
        (v / 255.0).clamp(0.0, 1.0)
    };
    // Normalize against 6500K reference.
    let ref_r = {
        let t2 = 65.0_f32;
        329.698_727_44 * (t2 - 60.0).powf(-0.133_204_759_2) / 255.0
    };
    let ref_g = {
        let t2 = 65.0_f32;
        288.122_169_93 * (t2 - 60.0).powf(-0.075_514_849_2) / 255.0
    };
    let ref_b = 1.0_f32;
    [
        (r / ref_r.max(1e-4)).clamp(0.0, 4.0),
        (g / ref_g.max(1e-4)).clamp(0.0, 4.0),
        (b / ref_b).clamp(0.0, 4.0),
    ]
}

/// One straight-sRGB RGBA8 program frame at the playhead. `rgba` is
/// `width * height * 4` bytes (R, G, B, A per pixel), opaque after flatten.
pub struct ProgramFrame {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

impl ProgramFrame {
    /// A solid black comp of `width`x`height` — the empty-program fallback and
    /// the backdrop the track stack composites over.
    fn black(width: u32, height: u32) -> Self {
        let n = (width as usize) * (height as usize);
        let mut rgba = Vec::with_capacity(n * 4);
        for _ in 0..n {
            rgba.extend_from_slice(&[0, 0, 0, 255]);
        }
        Self {
            width,
            height,
            rgba,
        }
    }
}

/// Render `project`'s program at timeline time `t` into a [`ProgramFrame`] at
/// the comp resolution (the **preview** path the host bridges into GPUI).
/// Delegates to [`render_program_at`] at the comp's native `width`x`height`.
pub fn program_frame(
    project: &Project,
    t: f32,
    cache: &mut FrameCache,
    global: &GlobalGrade,
    log_tracks: Option<&[bool]>,
    captions: &[Caption],
) -> ProgramFrame {
    let (w, h, rgba) =
        render_program_at(project, t, project.width, project.height, cache, global, log_tracks, captions);
    ProgramFrame { width: w, height: h, rgba }
}

/// Render `project`'s program at timeline time `t` at the target `target_w` x
/// `target_h` (each clamped to ≥1) into a straight-sRGB RGBA8 buffer, flattened
/// over opaque black. Returns `(w, h, rgba)` with `rgba.len() == w*h*4`.
///
/// Resolves the effective clip active on each *visible* track (bottom track
/// first), samples each into an RGBA buffer carrying its coverage alpha, then
/// folds the tracks with the over-operator honoring each clip's opacity. When a
/// transition is active at `t`, the transition track's plain clip is replaced by
/// a composited blend per the transition kind. An empty program yields a black frame.
///
/// This is the shared compositor the preview bridge ([`program_frame`]) and the
/// full-resolution video export ([`crate::export::render_program`]) both call,
/// so the exported MP4 matches the program monitor.
pub fn render_program_at(
    project: &Project,
    t: f32,
    target_w: u32,
    target_h: u32,
    cache: &mut FrameCache,
    global: &GlobalGrade,
    log_tracks: Option<&[bool]>,
    captions: &[Caption],
) -> (u32, u32, Vec<u8>) {
    render_program_inner(project, t, target_w, target_h, cache, global, log_tracks, captions, 0)
}

/// Maximum nested-sequence recursion depth. A `NestedClip` whose sub-tracks
/// reference (transitively) themselves would recurse forever; this bound makes
/// the compositor degrade to black rather than overflow the stack. Eight levels
/// is far past any sane edit.
const MAX_NEST_DEPTH: u32 = 8;

#[allow(clippy::too_many_arguments)]
fn render_program_inner(
    project: &Project,
    t: f32,
    target_w: u32,
    target_h: u32,
    cache: &mut FrameCache,
    global: &GlobalGrade,
    log_tracks: Option<&[bool]>,
    captions: &[Caption],
    depth: u32,
) -> (u32, u32, Vec<u8>) {
    let w = target_w.max(1);
    let h = target_h.max(1);

    // A transition active at `t` replaces its track's single effective clip
    // with a weighted blend of the outgoing + incoming clips. Resolve the track
    // it lives on so the per-track fold below can defer that track.
    let transition = project.active_transition(t).copied();
    let transition_track = transition
        .and_then(|tr| project.clips.get(tr.from).map(|c| c.track));

    // Sample each visible track's effective clip into a foreground RGBA buffer,
    // ordered bottom track first, so the fold composites the way the picture
    // reads (an upper clip's opacity lets the lower track show through).
    let mut layers: Vec<(Vec<u8>, f32)> = Vec::new();
    for clip in project.effective_clips_at(t) {
        // Defer the transition track: its clips are blended in the pass below.
        if Some(clip.track) == transition_track {
            continue;
        }
        let opacity = clip.opacity.clamp(0.0, 1.0);
        if opacity <= 0.0 {
            continue;
        }
        if let Some(mut fg) = sample_clip(clip, t, w, h, cache, global, log_tracks, depth) {
            // Apply per-track S-Log2→Rec709 if the track has that flag set.
            if log_tracks.and_then(|lt| lt.get(clip.track)).copied().unwrap_or(false) {
                apply_slog2_rec709(&mut fg);
            }
            layers.push((fg, opacity));
        }
    }

    // The transition pass: composite the two clips per the transition kind.
    if let Some(tr) = transition {
        if let Some((from_off, to_off)) = tr.push_offsets(t) {
            // Push: slide outgoing clip out, incoming clip in.
            for (clip_idx, offset) in [(tr.from, from_off), (tr.to, to_off)] {
                let Some(clip) = project.clips.get(clip_idx) else { continue; };
                let opacity = clip.opacity.clamp(0.0, 1.0);
                if opacity <= 0.0 { continue; }
                if let Some(fg) = sample_clip(clip, t, w, h, cache, global, log_tracks, depth) {
                    let shifted = translate_frame(&fg, w, h, offset.0, offset.1);
                    layers.push((shifted, opacity));
                }
            }
        } else if let Some(reveal) = tr.wipe_reveal(t) {
            // Wipe: outgoing clip full, incoming clip revealed in the sub-rect.
            for (clip_idx, rect) in [(tr.from, None), (tr.to, Some(reveal))] {
                let Some(clip) = project.clips.get(clip_idx) else {
                    continue;
                };
                let opacity = clip.opacity.clamp(0.0, 1.0);
                if opacity <= 0.0 {
                    continue;
                }
                if let Some(mut fg) = sample_clip(clip, t, w, h, cache, global, log_tracks, depth) {
                    if let Some(rect) = rect {
                        clip_to_reveal(&mut fg, w, h, rect);
                    }
                    layers.push((fg, opacity));
                }
            }
        } else if let Some(mask) = tr.pixel_mask(t, w, h) {
            // Iris / clock wipe: outgoing clip in full, incoming clip pixel-masked.
            for (clip_idx, use_mask) in [(tr.from, false), (tr.to, true)] {
                let Some(clip) = project.clips.get(clip_idx) else { continue; };
                let opacity = clip.opacity.clamp(0.0, 1.0);
                if opacity <= 0.0 { continue; }
                if let Some(mut fg) = sample_clip(clip, t, w, h, cache, global, log_tracks, depth) {
                    if use_mask {
                        apply_pixel_mask(&mut fg, &mask);
                    }
                    layers.push((fg, opacity));
                }
            }
        } else {
            let (from_w, to_w, dip) = tr.weights(t);
            for (clip_idx, weight) in [(tr.from, from_w), (tr.to, to_w)] {
                if weight <= 0.0 {
                    continue;
                }
                let Some(clip) = project.clips.get(clip_idx) else {
                    continue;
                };
                let opacity = clip.opacity.clamp(0.0, 1.0) * weight;
                if opacity <= 0.0 {
                    continue;
                }
                if let Some(fg) = sample_clip(clip, t, w, h, cache, global, log_tracks, depth) {
                    layers.push((fg, opacity));
                }
            }
            // A dip overlays a flat color over the (single visible) clip.
            if let Some((color, amount)) = dip {
                if amount > 0.0 {
                    let fill = color_fill(color, w, h);
                    layers.push((fill, amount.clamp(0.0, 1.0)));
                }
            }
        }
    }

    if layers.is_empty() {
        return (w, h, ProgramFrame::black(w, h).rgba);
    }

    let mut composed = fold_tracks(w, h, &layers);

    // Apply global grade (LUT + white balance) before flatten.
    if !global.is_identity() {
        for px in composed.chunks_exact_mut(4) {
            if px[3] == 0 { continue; }
            let mut rgb = [px[0] as f32 / 255.0, px[1] as f32 / 255.0, px[2] as f32 / 255.0];
            global.apply(&mut rgb);
            px[0] = (rgb[0] * 255.0).round().clamp(0.0, 255.0) as u8;
            px[1] = (rgb[1] * 255.0).round().clamp(0.0, 255.0) as u8;
            px[2] = (rgb[2] * 255.0).round().clamp(0.0, 255.0) as u8;
        }
    }

    // Burn the active caption (only at the top level — a nested sequence's own
    // captions, if any, aren't surfaced here). Drawn after the grade so the
    // caption text/box keep their authored color, before the flatten.
    if depth == 0 {
        draw_active_caption(&mut composed, w, h, t, captions);
    }

    flatten_over_black(&mut composed);
    (w, h, composed)
}

/// Burn the caption active at timeline time `t` (the last one whose
/// `[start, end)` covers `t` wins on overlap) into the composited frame, using
/// the embedded 5×7 font, the cue's [`CaptionStyle`] color, and an optional
/// translucent box behind the text per the style. Position is bottom / top /
/// custom (normalized x,y of the text block's top-left). A no-op when no cue is
/// active.
fn draw_active_caption(buf: &mut [u8], w: u32, h: u32, t: f32, captions: &[Caption]) {
    let Some(cap) = captions
        .iter()
        .rev()
        .find(|c| t >= c.start_secs && t < c.end_secs && !c.text.trim().is_empty())
    else {
        return;
    };
    // Glyph scale from the style font size, relative to the frame height so a cue
    // reads the same fraction of the picture at any comp resolution.
    let px_size = (cap.style.font_size / 720.0 * h as f32).max(7.0);
    let scale = ((px_size / 7.0).round() as usize).max(1);
    let char_w = 5 * scale + scale;
    let char_h = 7 * scale;

    // Wrap the text into lines on existing newlines (no auto-wrap this pass).
    let lines: Vec<&str> = cap.text.split('\n').collect();
    let block_h = lines.len() * char_h + lines.len().saturating_sub(1) * scale;
    let widest = lines
        .iter()
        .map(|l| l.chars().count())
        .max()
        .unwrap_or(0);
    let block_w = if widest > 0 { widest * char_w - scale } else { 0 };

    // Block top-left in pixels per the position.
    let margin = (h as f32 * 0.06) as usize;
    let (bx, by) = match cap.style.position {
        CaptionPosition::Bottom => (
            (w as usize).saturating_sub(block_w) / 2,
            (h as usize).saturating_sub(block_h + margin)),
        CaptionPosition::Top => ((w as usize).saturating_sub(block_w) / 2, margin),
        CaptionPosition::Custom(fx, fy) => (
            (fx.clamp(0.0, 1.0) * w as f32) as usize,
            (fy.clamp(0.0, 1.0) * h as f32) as usize),
    };

    let color = [
        (cap.style.color[0].clamp(0.0, 1.0) * 255.0) as u8,
        (cap.style.color[1].clamp(0.0, 1.0) * 255.0) as u8,
        (cap.style.color[2].clamp(0.0, 1.0) * 255.0) as u8,
        (cap.style.color[3].clamp(0.0, 1.0) * 255.0) as u8,
    ];

    // Translucent box behind the text for legibility (caption convention).
    let pad = scale * 2;
    let box_x0 = bx.saturating_sub(pad);
    let box_y0 = by.saturating_sub(pad);
    let box_x1 = (bx + block_w + pad).min(w as usize);
    let box_y1 = (by + block_h + pad).min(h as usize);
    for y in box_y0..box_y1 {
        for x in box_x0..box_x1 {
            let i = (y * w as usize + x) * 4;
            if i + 4 <= buf.len() {
                // 60% black over the existing pixel.
                for c in 0..3 {
                    buf[i + c] = (buf[i + c] as f32 * 0.4).round() as u8;
                }
                buf[i + 3] = 255;
            }
        }
    }

    // Draw each line centered within the block.
    for (li, line) in lines.iter().enumerate() {
        let n_chars = line.chars().count();
        let line_w = if n_chars > 0 { n_chars * char_w - scale } else { 0 };
        let lx = bx + (block_w.saturating_sub(line_w)) / 2;
        let ly = by + li * (char_h + scale);
        for (ci, ch) in line.chars().enumerate() {
            let code = ch as u32;
            if code < 32 || code > 126 {
                continue;
            }
            let glyph = FONT_5X7[(code - 32) as usize];
            let cx_base = lx + ci * char_w;
            for col in 0..5usize {
                let col_bits = glyph[col];
                for row in 0..7usize {
                    if (col_bits >> row) & 1 == 1 {
                        for sy in 0..scale {
                            let py = ly + row * scale + sy;
                            if py >= h as usize {
                                break;
                            }
                            for sx in 0..scale {
                                let px_x = cx_base + col * scale + sx;
                                if px_x >= w as usize {
                                    break;
                                }
                                let i = (py * w as usize + px_x) * 4;
                                if i + 4 <= buf.len() {
                                    buf[i] = color[0];
                                    buf[i + 1] = color[1];
                                    buf[i + 2] = color[2];
                                    buf[i + 3] = 255;
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Sample one clip's content at timeline time `t` into a `w*h*4` RGBA buffer,
/// with the clip's color grade applied. Alpha is the clip's coverage (255 where
/// it draws, 0 in letterbox). Returns `None` for a source that yields no visible
/// pixels (e.g. media that failed to decode). `cache` memoizes decoded video
/// frames by `(path, frame-index)`.
#[allow(clippy::too_many_arguments)]
fn sample_clip(
    clip: &Clip,
    t: f32,
    w: u32,
    h: u32,
    cache: &mut FrameCache,
    global: &GlobalGrade,
    log_tracks: Option<&[bool]>,
    depth: u32,
) -> Option<Vec<u8>> {
    let mut buf = sample_clip_raw(clip, t, w, h, cache, global, log_tracks, depth)?;
    // Apply the per-clip color grade in place (a true no-op when identity, so an
    // ungraded clip pays nothing). Only covered (non-letterbox) pixels are graded.
    // The basic `grade` runs first, then the `hsl_secondary` qualifier-based
    // grade, matching the inspector's stacking order so the preview, scopes, and
    // export all read the same.
    let grade = !clip.grade.is_identity();
    let hsl = !clip.hsl_secondary.is_identity();
    if grade || hsl {
        for px in buf.chunks_exact_mut(4) {
            if px[3] == 0 {
                continue; // transparent letterbox — leave it alone
            }
            let mut rgb = [
                px[0] as f32 / 255.0,
                px[1] as f32 / 255.0,
                px[2] as f32 / 255.0,
            ];
            if grade {
                clip.grade.apply(&mut rgb);
            }
            if hsl {
                clip.hsl_secondary.apply(&mut rgb);
            }
            px[0] = (rgb[0] * 255.0).round().clamp(0.0, 255.0) as u8;
            px[1] = (rgb[1] * 255.0).round().clamp(0.0, 255.0) as u8;
            px[2] = (rgb[2] * 255.0).round().clamp(0.0, 255.0) as u8;
        }
    }
    Some(buf)
}

/// Sample one clip's *ungraded* content at timeline time `t` into a `w*h*4` RGBA
/// buffer. The raw source pixels before the color grade pass (see [`sample_clip`]).
#[allow(clippy::too_many_arguments)]
fn sample_clip_raw(
    clip: &Clip,
    t: f32,
    w: u32,
    h: u32,
    cache: &mut FrameCache,
    global: &GlobalGrade,
    log_tracks: Option<&[bool]>,
    depth: u32,
) -> Option<Vec<u8>> {
    match &clip.source {
        ClipSource::Color(c) => {
            // A flat fill covering the whole frame.
            let px = [
                (c[0].clamp(0.0, 1.0) * 255.0).round() as u8,
                (c[1].clamp(0.0, 1.0) * 255.0).round() as u8,
                (c[2].clamp(0.0, 1.0) * 255.0).round() as u8,
                255u8,
            ];
            let n = (w as usize) * (h as usize);
            let mut buf = Vec::with_capacity(n * 4);
            for _ in 0..n {
                buf.extend_from_slice(&px);
            }
            Some(buf)
        }
        ClipSource::Image(path) => {
            let img = prism_io::load_image(path).ok()?;
            Some(aspect_fit(&img.rgba8, img.size.width, img.size.height, w, h))
        }
        ClipSource::Video(video) => {
            // Map the playhead → clip-local source time, applying speed and reverse.
            let local_t = (t - clip.start).max(0.0);
            let effective_local = match &clip.speed_curve {
                SpeedCurve::Constant => local_t * clip.speed,
                SpeedCurve::Bezier { p0, p1, p2, p3 } => {
                    bezier_source_time(local_t, clip.duration, *p0, *p1, *p2, *p3) * clip.speed
                }
            };
            let source_t = if clip.reversed {
                (clip.duration - effective_local).max(0.0) + clip.source_in.max(0.0)
            } else {
                effective_local + clip.source_in.max(0.0)
            };
            let frame_index = video.frame_index_at(source_t);
            let seek = video.frame_time(frame_index);
            // Proxy / optimized media: when a low-res proxy is attached, decode it
            // instead of the original (same fps/duration → same frame-index map),
            // so scrubbing/playback on heavy 4K footage uses the lighter file. The
            // original is still used on export-from-the-original paths.
            let decode_path = clip.proxy_path.as_deref().unwrap_or(video.path.as_path());
            sample_video_frame(decode_path, frame_index, seek, w, h, cache)
        }
        ClipSource::Audio(_) => {
            // An audio clip has no picture — it contributes nothing to the
            // program frame (only a timeline waveform). The track stack composites
            // over black as if the clip weren't there.
            None
        }
        ClipSource::Title { text, font_size, color, bg_color } => {
            Some(render_title_frame(text, *font_size, *color, *bg_color, w, h))
        }
        ClipSource::NestedClip { tracks, clips, duration_secs } => {
            // Recursive sub-sequence compositing: build a transient sub-`Project`
            // from the inline tracks/clips and render it through the *same*
            // compositor at the nest-local time, then composite the opaque result
            // (black backdrop, like Premiere's nested sequence) as this clip's
            // pixels. A depth guard degrades a self/cyclic nest to black instead
            // of overflowing the stack.
            if depth >= MAX_NEST_DEPTH {
                return Some(black_layer(w, h));
            }
            // Map the playhead → nest-local time: speed/reverse-aware so a sped or
            // trimmed nested clip plays the right portion of the sub-sequence.
            let local_t = (t - clip.start).max(0.0);
            let eff = local_t * clip.speed.max(0.0);
            let nest_t = if clip.reversed {
                (clip.duration * clip.speed.max(0.0) - eff).max(0.0) + clip.source_in.max(0.0)
            } else {
                eff + clip.source_in.max(0.0)
            };
            let nest_t = nest_t.min((*duration_secs - 1e-3).max(0.0));
            let sub = Project {
                name: String::new(),
                width: w,
                height: h,
                fps: 30.0,
                duration: *duration_secs,
                tracks: tracks.clone(),
                clips: clips.clone(),
                transitions: Vec::new(),
            };
            // The nest's own clips are graded by their per-clip grades; the outer
            // global grade is applied once at the top level, so render the inner
            // sequence with an identity global grade and no log-track overrides.
            let _ = (global, log_tracks);
            let (_, _, rgba) = render_program_inner(
                &sub,
                nest_t,
                w,
                h,
                cache,
                &GlobalGrade::identity(),
                None,
                &[],
                depth + 1,
            );
            Some(rgba)
        }
    }
}

/// An opaque black `w*h*4` RGBA layer (alpha 255). The nested-sequence backdrop
/// and the depth-guard fallback.
fn black_layer(w: u32, h: u32) -> Vec<u8> {
    let n = (w as usize) * (h as usize);
    let mut buf = Vec::with_capacity(n * 4);
    for _ in 0..n {
        buf.extend_from_slice(&[0, 0, 0, 255]);
    }
    buf
}

/// Decode (cached) one movie frame at `seek` seconds, aspect-fit it into `w*h`,
/// and return the RGBA buffer. The decode is scaled to a preview size
/// ([`VIDEO_PREVIEW_MAX_DIM`]) and keyed by `(path, frame_index)` so a redraw at
/// the same playhead reuses the bytes. A decode failure (missing ffmpeg / bad
/// media) caches an empty buffer and returns `None` so the clip shows as the
/// black backdrop rather than crashing the host.
fn sample_video_frame(
    path: &std::path::Path,
    frame_index: u64,
    seek: f64,
    w: u32,
    h: u32,
    cache: &mut FrameCache,
) -> Option<Vec<u8>> {
    let key = (path.to_path_buf(), frame_index);
    if let Some(buf) = cache.map.get(&key) {
        return (!buf.is_empty()).then(|| buf.clone());
    }

    // Decode at an aspect-preserving preview scale: the native resolution capped
    // so the longest side is ≤ VIDEO_PREVIEW_MAX_DIM. Probing for native dims is
    // cheap relative to a full-size decode; if the probe fails we fall back to
    // None (decode at native size — ffmpeg still succeeds, just larger).
    let scale = preview_scale(path);
    let result = match prism_media::decode_frame_at(path, seek, scale) {
        Ok(frame) => aspect_fit(&frame.rgba, frame.width, frame.height, w, h),
        Err(e) => {
            log::warn!(
                "reel-gpui: video decode failed for {} @ {:.3}s: {e}",
                path.display(),
                seek
            );
            Vec::new()
        }
    };
    cache.map.insert(key, result.clone());
    (!result.is_empty()).then_some(result)
}

/// The aspect-preserving decode scale for a movie preview: the source's native
/// resolution (probed) with the longest side capped at [`VIDEO_PREVIEW_MAX_DIM`].
/// Returns `None` when the probe fails (decode at native size as a fallback).
fn preview_scale(path: &std::path::Path) -> Option<(u32, u32)> {
    let info = prism_media::probe(path).ok()?;
    let (nw, nh) = (info.width, info.height);
    if nw == 0 || nh == 0 {
        return None;
    }
    let longest = nw.max(nh);
    if longest <= VIDEO_PREVIEW_MAX_DIM {
        return Some((nw, nh));
    }
    let s = VIDEO_PREVIEW_MAX_DIM as f32 / longest as f32;
    Some((
        ((nw as f32 * s).round() as u32).max(1),
        ((nh as f32 * s).round() as u32).max(1),
    ))
}

/// Aspect-fit a source RGBA8 image into a `dst_w`x`dst_h` frame (nearest
/// sample), centered, with transparent (alpha 0) letterbox bars so the comp
/// black shows through after flatten.
fn aspect_fit(src: &[u8], src_w: u32, src_h: u32, dst_w: u32, dst_h: u32) -> Vec<u8> {
    let mut out = vec![0u8; (dst_w as usize) * (dst_h as usize) * 4];
    if src_w == 0 || src_h == 0 {
        return out;
    }
    let scale = (dst_w as f32 / src_w as f32).min(dst_h as f32 / src_h as f32);
    let fit_w = (src_w as f32 * scale).round().max(1.0) as u32;
    let fit_h = (src_h as f32 * scale).round().max(1.0) as u32;
    let off_x = (dst_w - fit_w.min(dst_w)) / 2;
    let off_y = (dst_h - fit_h.min(dst_h)) / 2;

    for y in 0..fit_h.min(dst_h) {
        let sy = ((y as f32 + 0.5) / scale).floor() as u32;
        let sy = sy.min(src_h - 1);
        for x in 0..fit_w.min(dst_w) {
            let sx = ((x as f32 + 0.5) / scale).floor() as u32;
            let sx = sx.min(src_w - 1);
            let si = ((sy * src_w + sx) * 4) as usize;
            let di = (((y + off_y) * dst_w + (x + off_x)) * 4) as usize;
            if si + 4 <= src.len() && di + 4 <= out.len() {
                out[di] = src[si];
                out[di + 1] = src[si + 1];
                out[di + 2] = src[si + 2];
                out[di + 3] = src[si + 3];
            }
        }
    }
    out
}

/// A flat straight-sRGB color fill covering the whole `w*h` frame (alpha 255).
/// Used as the dip-to-color overlay in the transition pass.
fn color_fill(color: [f32; 4], w: u32, h: u32) -> Vec<u8> {
    let px = [
        (color[0].clamp(0.0, 1.0) * 255.0).round() as u8,
        (color[1].clamp(0.0, 1.0) * 255.0).round() as u8,
        (color[2].clamp(0.0, 1.0) * 255.0).round() as u8,
        255u8,
    ];
    let n = (w as usize) * (h as usize);
    let mut buf = Vec::with_capacity(n * 4);
    for _ in 0..n {
        buf.extend_from_slice(&px);
    }
    buf
}

/// Knock out (set alpha 0) every pixel of `buf` *outside* the `reveal`
/// fraction-rect `(x0, y0, x1, y1)` (each `0..=1` of the frame). The pixels
/// inside the rect keep their coverage so a wipe shows the incoming clip only in
/// the swept-in region. Mirrors the egui app's wipe clip.
fn clip_to_reveal(buf: &mut [u8], w: u32, h: u32, reveal: (f32, f32, f32, f32)) {
    let (x0, y0, x1, y1) = reveal;
    let px0 = (x0.clamp(0.0, 1.0) * w as f32).round() as u32;
    let px1 = (x1.clamp(0.0, 1.0) * w as f32).round() as u32;
    let py0 = (y0.clamp(0.0, 1.0) * h as f32).round() as u32;
    let py1 = (y1.clamp(0.0, 1.0) * h as f32).round() as u32;
    for y in 0..h {
        for x in 0..w {
            let inside = x >= px0 && x < px1 && y >= py0 && y < py1;
            if !inside {
                let i = ((y * w + x) * 4 + 3) as usize;
                if i < buf.len() {
                    buf[i] = 0;
                }
            }
        }
    }
}

/// Multiply each pixel's alpha by the corresponding byte in `mask` (0=transparent,
/// 255=opaque). Used for iris-circle and clock-wipe transitions.
fn apply_pixel_mask(buf: &mut [u8], mask: &[u8]) {
    for (i, &m) in mask.iter().enumerate() {
        let ai = i * 4 + 3;
        if ai < buf.len() {
            buf[ai] = ((buf[ai] as u16 * m as u16) / 255) as u8;
        }
    }
}

/// Apply a pixel-shift translate to a `w*h*4` RGBA buffer by `(dx_frac, dy_frac)`
/// fractions of the frame size (positive = shift right/down). Pixels that shift
/// out of bounds become transparent (alpha 0). Returns a new buffer.
fn translate_frame(buf: &[u8], w: u32, h: u32, dx_frac: f32, dy_frac: f32) -> Vec<u8> {
    let dx = (dx_frac * w as f32).round() as i32;
    let dy = (dy_frac * h as f32).round() as i32;
    let mut out = vec![0u8; (w as usize) * (h as usize) * 4];
    for y in 0..h as i32 {
        for x in 0..w as i32 {
            let sx = x - dx;
            let sy = y - dy;
            if sx < 0 || sx >= w as i32 || sy < 0 || sy >= h as i32 {
                continue;
            }
            let si = ((sy as u32 * w + sx as u32) * 4) as usize;
            let di = ((y as u32 * w + x as u32) * 4) as usize;
            if si + 4 <= buf.len() && di + 4 <= out.len() {
                out[di] = buf[si];
                out[di + 1] = buf[si + 1];
                out[di + 2] = buf[si + 2];
                out[di + 3] = buf[si + 3];
            }
        }
    }
    out
}

/// Fold the track layers (bottom first) into one premultiplied-ish straight
/// RGBA buffer with the source-over operator, scaling each layer's alpha by its
/// clip opacity. The accumulator starts fully transparent.
fn fold_tracks(w: u32, h: u32, layers: &[(Vec<u8>, f32)]) -> Vec<u8> {
    let n = (w as usize) * (h as usize);
    let mut acc = vec![0u8; n * 4];
    for (fg, opacity) in layers {
        over_in_place(&mut acc, fg, *opacity);
    }
    acc
}

/// Composite `fg` (scaled by `opacity`) over `acc` in place, straight-alpha
/// source-over, per pixel: `out = fg*a + acc*(1-a)` where `a = fg_alpha*opacity`.
fn over_in_place(acc: &mut [u8], fg: &[u8], opacity: f32) {
    for (a, f) in acc.chunks_exact_mut(4).zip(fg.chunks_exact(4)) {
        let fa = (f[3] as f32 / 255.0) * opacity;
        if fa <= 0.0 {
            continue;
        }
        let inv = 1.0 - fa;
        for c in 0..3 {
            a[c] = (f[c] as f32 * fa + a[c] as f32 * inv).round().clamp(0.0, 255.0) as u8;
        }
        // Straight-over alpha accumulation.
        let out_a = fa + (a[3] as f32 / 255.0) * inv;
        a[3] = (out_a * 255.0).round().clamp(0.0, 255.0) as u8;
    }
}

/// Flatten a straight-alpha RGBA buffer over an opaque black backdrop: scale RGB
/// by alpha and force alpha to 255 (the program output is opaque).
fn flatten_over_black(buf: &mut [u8]) {
    for px in buf.chunks_exact_mut(4) {
        let a = px[3] as f32 / 255.0;
        for c in 0..3 {
            px[c] = (px[c] as f32 * a).round().clamp(0.0, 255.0) as u8;
        }
        px[3] = 255;
    }
}

/// Minimal 5×7 pixel bitmap font for ASCII 32–126 (95 printable chars).
/// Each character is 5 bytes, one per column (left→right).
/// Each byte encodes rows top (bit 0) to bottom (bit 6).
#[rustfmt::skip]
const FONT_5X7: [[u8; 5]; 95] = [
    [0x00,0x00,0x00,0x00,0x00], // 32  (space)
    [0x00,0x00,0x5F,0x00,0x00], // 33  !
    [0x00,0x07,0x00,0x07,0x00], // 34  "
    [0x14,0x7F,0x14,0x7F,0x14], // 35  #
    [0x24,0x2A,0x7F,0x2A,0x12], // 36  $
    [0x23,0x13,0x08,0x64,0x62], // 37  %
    [0x36,0x49,0x56,0x20,0x50], // 38  &
    [0x00,0x06,0x07,0x00,0x00], // 39  '
    [0x00,0x1C,0x22,0x41,0x00], // 40  (
    [0x00,0x41,0x22,0x1C,0x00], // 41  )
    [0x2A,0x1C,0x7F,0x1C,0x2A], // 42  *
    [0x08,0x08,0x3E,0x08,0x08], // 43  +
    [0x00,0x50,0x30,0x00,0x00], // 44  ,
    [0x08,0x08,0x08,0x08,0x08], // 45  -
    [0x00,0x60,0x60,0x00,0x00], // 46  .
    [0x20,0x10,0x08,0x04,0x02], // 47  /
    [0x3E,0x51,0x49,0x45,0x3E], // 48  0
    [0x00,0x42,0x7F,0x40,0x00], // 49  1
    [0x42,0x61,0x51,0x49,0x46], // 50  2
    [0x21,0x41,0x45,0x4B,0x31], // 51  3
    [0x18,0x14,0x12,0x7F,0x10], // 52  4
    [0x27,0x45,0x45,0x45,0x39], // 53  5
    [0x3C,0x4A,0x49,0x49,0x30], // 54  6
    [0x01,0x71,0x09,0x05,0x03], // 55  7
    [0x36,0x49,0x49,0x49,0x36], // 56  8
    [0x06,0x49,0x49,0x29,0x1E], // 57  9
    [0x00,0x36,0x36,0x00,0x00], // 58  :
    [0x00,0x56,0x36,0x00,0x00], // 59  ;
    [0x08,0x14,0x22,0x41,0x00], // 60  <
    [0x14,0x14,0x14,0x14,0x14], // 61  =
    [0x00,0x41,0x22,0x14,0x08], // 62  >
    [0x02,0x01,0x51,0x09,0x06], // 63  ?
    [0x32,0x49,0x79,0x41,0x3E], // 64  @
    [0x7E,0x11,0x11,0x11,0x7E], // 65  A
    [0x7F,0x49,0x49,0x49,0x36], // 66  B
    [0x3E,0x41,0x41,0x41,0x22], // 67  C
    [0x7F,0x41,0x41,0x22,0x1C], // 68  D
    [0x7F,0x49,0x49,0x49,0x41], // 69  E
    [0x7F,0x09,0x09,0x09,0x01], // 70  F
    [0x3E,0x41,0x49,0x49,0x7A], // 71  G
    [0x7F,0x08,0x08,0x08,0x7F], // 72  H
    [0x00,0x41,0x7F,0x41,0x00], // 73  I
    [0x20,0x40,0x41,0x3F,0x01], // 74  J
    [0x7F,0x08,0x14,0x22,0x41], // 75  K
    [0x7F,0x40,0x40,0x40,0x40], // 76  L
    [0x7F,0x02,0x04,0x02,0x7F], // 77  M
    [0x7F,0x04,0x08,0x10,0x7F], // 78  N
    [0x3E,0x41,0x41,0x41,0x3E], // 79  O
    [0x7F,0x09,0x09,0x09,0x06], // 80  P
    [0x3E,0x41,0x51,0x21,0x5E], // 81  Q
    [0x7F,0x09,0x19,0x29,0x46], // 82  R
    [0x46,0x49,0x49,0x49,0x31], // 83  S
    [0x01,0x01,0x7F,0x01,0x01], // 84  T
    [0x3F,0x40,0x40,0x40,0x3F], // 85  U
    [0x1F,0x20,0x40,0x20,0x1F], // 86  V
    [0x3F,0x40,0x38,0x40,0x3F], // 87  W
    [0x63,0x14,0x08,0x14,0x63], // 88  X
    [0x07,0x08,0x70,0x08,0x07], // 89  Y
    [0x61,0x51,0x49,0x45,0x43], // 90  Z
    [0x00,0x7F,0x41,0x41,0x00], // 91  [
    [0x02,0x04,0x08,0x10,0x20], // 92  backslash
    [0x00,0x41,0x41,0x7F,0x00], // 93  ]
    [0x04,0x02,0x01,0x02,0x04], // 94  ^
    [0x40,0x40,0x40,0x40,0x40], // 95  _
    [0x00,0x01,0x02,0x04,0x00], // 96  `
    [0x20,0x54,0x54,0x54,0x78], // 97  a
    [0x7F,0x48,0x44,0x44,0x38], // 98  b
    [0x38,0x44,0x44,0x44,0x20], // 99  c
    [0x38,0x44,0x44,0x48,0x7F], // 100 d
    [0x38,0x54,0x54,0x54,0x18], // 101 e
    [0x08,0x7E,0x09,0x01,0x02], // 102 f
    [0x0C,0x52,0x52,0x52,0x3E], // 103 g
    [0x7F,0x08,0x04,0x04,0x78], // 104 h
    [0x00,0x44,0x7D,0x40,0x00], // 105 i
    [0x20,0x40,0x44,0x3D,0x00], // 106 j
    [0x7F,0x10,0x28,0x44,0x00], // 107 k
    [0x00,0x41,0x7F,0x40,0x00], // 108 l
    [0x7C,0x04,0x18,0x04,0x78], // 109 m
    [0x7C,0x08,0x04,0x04,0x78], // 110 n
    [0x38,0x44,0x44,0x44,0x38], // 111 o
    [0x7C,0x14,0x14,0x14,0x08], // 112 p
    [0x08,0x14,0x14,0x18,0x7C], // 113 q
    [0x7C,0x08,0x04,0x04,0x08], // 114 r
    [0x48,0x54,0x54,0x54,0x20], // 115 s
    [0x04,0x3F,0x44,0x40,0x20], // 116 t
    [0x3C,0x40,0x40,0x20,0x7C], // 117 u
    [0x1C,0x20,0x40,0x20,0x1C], // 118 v
    [0x3C,0x40,0x30,0x40,0x3C], // 119 w
    [0x44,0x28,0x10,0x28,0x44], // 120 x
    [0x0C,0x50,0x50,0x50,0x3C], // 121 y
    [0x44,0x64,0x54,0x4C,0x44], // 122 z
    [0x00,0x08,0x36,0x41,0x00], // 123 {
    [0x00,0x00,0x7F,0x00,0x00], // 124 |
    [0x00,0x41,0x36,0x08,0x00], // 125 }
    [0x10,0x08,0x08,0x10,0x08], // 126 ~
];

/// Rasterize a title clip's text using the embedded 5×7 bitmap font into a
/// `w×h` RGBA8 buffer. Text is centered on the frame. `bg_color = None` means
/// transparent background (text blends over the track beneath it).
fn render_title_frame(
    text: &str,
    font_size: f32,
    color: [u8; 4],
    bg_color: Option<[u8; 4]>,
    w: u32,
    h: u32,
) -> Vec<u8> {
    let scale = ((font_size / 7.0).round() as usize).max(1);
    let char_w = 5 * scale + scale; // 5 pixel columns + 1 pixel gap between chars
    let char_h = 7 * scale;
    let n_chars = text.chars().count();
    let text_w = if n_chars > 0 { n_chars * char_w - scale } else { 0 }; // no trailing gap
    let text_h = char_h;

    // Fill with background.
    let n_px = (w as usize) * (h as usize);
    let mut buf = if let Some(bg) = bg_color {
        let mut b = Vec::with_capacity(n_px * 4);
        for _ in 0..n_px {
            b.extend_from_slice(&[bg[0], bg[1], bg[2], bg[3]]);
        }
        b
    } else {
        vec![0u8; n_px * 4] // transparent
    };

    // Center the text block.
    let off_x = (w as usize).saturating_sub(text_w) / 2;
    let off_y = (h as usize).saturating_sub(text_h) / 2;

    for (ci, ch) in text.chars().enumerate() {
        let code = ch as u32;
        if code < 32 || code > 126 {
            continue;
        }
        let glyph = FONT_5X7[(code - 32) as usize];
        let cx_base = off_x + ci * char_w;
        for col in 0..5usize {
            let col_bits = glyph[col];
            for row in 0..7usize {
                if (col_bits >> row) & 1 == 1 {
                    for sy in 0..scale {
                        let py = off_y + row * scale + sy;
                        if py >= h as usize {
                            break;
                        }
                        for sx in 0..scale {
                            let px_x = cx_base + col * scale + sx;
                            if px_x >= w as usize {
                                break;
                            }
                            let i = (py * w as usize + px_x) * 4;
                            buf[i] = color[0];
                            buf[i + 1] = color[1];
                            buf[i + 2] = color[2];
                            buf[i + 3] = color[3];
                        }
                    }
                }
            }
        }
    }
    buf
}

/// Apply S-Log2 → Rec.709 gamma transform to an RGBA8 buffer in place.
/// S-Log2 → linear: `linear = 10^((code - 0.616596) / 0.432699) - 0.037584`
/// Linear → Rec.709: `y = 1.099 * linear^0.45 - 0.099` for linear >= 0.018.
pub fn apply_slog2_rec709(buf: &mut [u8]) {
    for px in buf.chunks_exact_mut(4) {
        for c in 0..3 {
            let code = px[c] as f32 / 255.0;
            let linear = (10.0_f32.powf((code - 0.616596) / 0.432699) - 0.037584).max(0.0);
            let rec709 = if linear >= 0.018 {
                (1.099 * linear.powf(0.45) - 0.099).clamp(0.0, 1.0)
            } else {
                (4.5 * linear).clamp(0.0, 1.0)
            };
            px[c] = (rec709 * 255.0).round() as u8;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app_state::{
        Caption, CaptionPosition, CaptionStyle, Clip, ColorGrade, HslSecondaryGrade, Project,
        SpeedCurve, Track,
    };

    fn color_clip(rgba: [f32; 4], track: usize, start: f32, dur: f32) -> Clip {
        Clip {
            name: "c".into(),
            source: ClipSource::Color(rgba),
            track,
            start,
            duration: dur,
            source_in: 0.0,
            opacity: 1.0,
            grade: ColorGrade::default(),
            hsl_secondary: HslSecondaryGrade::default(),
            fade_in: 0.0,
            fade_out: 0.0,
            speed: 1.0,
            reversed: false,
            speed_curve: SpeedCurve::default(),
            proxy_path: None,
            link_group: None,
        }
    }

    fn project_with(clips: Vec<Clip>, n_tracks: usize) -> Project {
        let tracks = (0..n_tracks)
            .map(|i| Track { name: format!("V{}", i + 1), enabled: true })
            .collect();
        Project {
            name: "t".into(),
            width: 16,
            height: 16,
            fps: 30.0,
            duration: 10.0,
            tracks,
            clips,
            transitions: Vec::new(),
        }
    }

    fn center_px(rgba: &[u8], w: u32, h: u32) -> [u8; 4] {
        let i = ((h / 2 * w + w / 2) * 4) as usize;
        [rgba[i], rgba[i + 1], rgba[i + 2], rgba[i + 3]]
    }

    #[test]
    fn nested_sequence_renders_its_content_not_a_placeholder() {
        // A nested clip whose inner sequence is a solid green frame should render
        // green at the playhead, not the old purple placeholder.
        let inner = color_clip([0.0, 1.0, 0.0, 1.0], 0, 0.0, 5.0);
        let nest = Clip {
            source: ClipSource::NestedClip {
                tracks: vec![Track { name: "V1".into(), enabled: true }],
                clips: vec![inner],
                duration_secs: 5.0,
            },
            ..color_clip([0.0, 0.0, 0.0, 1.0], 0, 0.0, 5.0)
        };
        let project = project_with(vec![nest], 1);
        let mut cache = FrameCache::new();
        let (w, h, rgba) =
            render_program_at(&project, 1.0, 16, 16, &mut cache, &GlobalGrade::identity(), None, &[]);
        let px = center_px(&rgba, w, h);
        assert!(px[1] > 200 && px[0] < 60 && px[2] < 60, "nested green shows through: {px:?}");
    }

    #[test]
    fn nested_cycle_degrades_to_black_not_stack_overflow() {
        // A nest that contains *itself* (cyclic) must terminate at the depth guard
        // and render black rather than recursing forever.
        let self_nest_inner = ClipSource::NestedClip {
            tracks: vec![Track { name: "V1".into(), enabled: true }],
            clips: vec![],
            duration_secs: 5.0,
        };
        // Build a one-level nest referencing a green clip + a deep stub; the guard
        // is what we assert (no panic / overflow), so just deeply nest empties.
        let mut src = self_nest_inner;
        for _ in 0..12 {
            src = ClipSource::NestedClip {
                tracks: vec![Track { name: "V1".into(), enabled: true }],
                clips: vec![Clip { source: src, ..color_clip([0.0, 0.0, 0.0, 1.0], 0, 0.0, 5.0) }],
                duration_secs: 5.0,
            };
        }
        let nest = Clip { source: src, ..color_clip([0.0, 0.0, 0.0, 1.0], 0, 0.0, 5.0) };
        let project = project_with(vec![nest], 1);
        let mut cache = FrameCache::new();
        // Just assert it returns (the depth guard prevents overflow).
        let (_, _, rgba) =
            render_program_at(&project, 1.0, 16, 16, &mut cache, &GlobalGrade::identity(), None, &[]);
        assert_eq!(rgba.len(), 16 * 16 * 4);
    }

    #[test]
    fn active_caption_burns_a_box_into_the_frame() {
        // A bottom caption over a white clip darkens the box region (the 60% box).
        let clip = color_clip([1.0, 1.0, 1.0, 1.0], 0, 0.0, 10.0);
        let project = project_with(vec![clip], 1);
        let cap = Caption {
            start_secs: 0.0,
            end_secs: 5.0,
            text: "HELLO".into(),
            style: CaptionStyle {
                font_size: 48.0,
                color: [1.0, 1.0, 0.0, 1.0],
                position: CaptionPosition::Bottom,
            },
        };
        let mut cache = FrameCache::new();
        // 200x120 so the caption fits.
        let mut p = project;
        p.width = 200;
        p.height = 120;
        let (w, h, with_cap) =
            render_program_at(&p, 1.0, 200, 120, &mut cache, &GlobalGrade::identity(), None, &[cap.clone()]);
        let (_, _, no_cap) =
            render_program_at(&p, 1.0, 200, 120, &mut cache, &GlobalGrade::identity(), None, &[]);
        assert_ne!(with_cap, no_cap, "an active caption changes the frame");
        // A pixel in the bottom band differs (darkened box or yellow glyph).
        let i = (((h - h / 12) * w + w / 2) * 4) as usize;
        assert!(with_cap[i] < 255 || with_cap[i + 2] < 255, "caption region altered");

        // An inactive caption (playhead outside the cue) leaves the frame clean.
        let (_, _, after_end) =
            render_program_at(&p, 8.0, 200, 120, &mut cache, &GlobalGrade::identity(), None, &[cap]);
        assert_eq!(after_end, no_cap, "an inactive caption is a no-op");
    }

    #[test]
    fn hsl_secondary_is_applied_when_non_identity() {
        // A clip with an identity HSL grade and one with a saturation push should
        // differ at the center pixel.
        let base = color_clip([0.8, 0.2, 0.2, 1.0], 0, 0.0, 10.0);
        let mut graded = base.clone();
        graded.hsl_secondary = HslSecondaryGrade {
            hue_center: 0.0,
            hue_width: 0.2,
            sat_offset: -0.8,
            lum_offset: 0.0,
        };
        let mut cache = FrameCache::new();
        let (w, h, a) = render_program_at(
            &project_with(vec![base], 1), 1.0, 16, 16, &mut cache, &GlobalGrade::identity(), None, &[]);
        let (_, _, b) = render_program_at(
            &project_with(vec![graded], 1), 1.0, 16, 16, &mut cache, &GlobalGrade::identity(), None, &[]);
        assert_ne!(center_px(&a, w, h), center_px(&b, w, h), "HSL secondary changes a matched hue");
    }
}
