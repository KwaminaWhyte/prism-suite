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

use crate::app_state::{Caption, Clip, ClipSource, Project, SpeedCurve};

// --- Submodules (split out of this file per the ~1000-line rule) -------------
mod grade;
mod remap;
mod compositing;
mod text;

// Re-exports so external callers (`crate::program_frame::X`) and the rest of
// this module keep resolving these unqualified / by their old paths.
pub use grade::{kelvin_to_rgb_gain, GlobalGrade};
pub use remap::remapped_source_time;
pub use text::apply_slog2_rec709;

use remap::bezier_source_time;
use compositing::{
    apply_pixel_mask, aspect_fit, black_layer, clip_to_reveal, color_fill, flatten_over_black,
    fold_tracks, translate_frame,
};
use text::{draw_active_caption, render_title_frame};

/// Longest preview dimension for a decoded video frame. Mirrors the egui app's
/// preview scale (≤960px) so a 4K source doesn't decode at full size for a
/// half-size on-screen preview.
const VIDEO_PREVIEW_MAX_DIM: u32 = 960;

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
        // Geometric transitions (Slide / Zoom / Spin / Cube fold) warp both
        // frames into a single composited layer — sample both clips, then blend.
        let geom = {
            let from_buf = project.clips.get(tr.from)
                .filter(|c| c.opacity > 0.0)
                .and_then(|c| sample_clip(c, t, w, h, cache, global, log_tracks, depth))
                .unwrap_or_else(|| vec![0u8; (w as usize) * (h as usize) * 4]);
            let to_buf = project.clips.get(tr.to)
                .filter(|c| c.opacity > 0.0)
                .and_then(|c| sample_clip(c, t, w, h, cache, global, log_tracks, depth))
                .unwrap_or_else(|| vec![0u8; (w as usize) * (h as usize) * 4]);
            use crate::app_state::transitions::TransitionGeometryExt;
            tr.geometry_blend(t, &from_buf, &to_buf, w, h)
        };
        if let Some(blended) = geom {
            layers.push((blended, 1.0));
        } else if let Some((from_off, to_off)) = tr.push_offsets(t) {
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
            // Map the playhead → clip-local source time. Time-remap (when on)
            // takes priority over the constant speed / bezier speed-curve: a
            // speed-factor key list integrates per-frame source time (honoring
            // freeze-frame factor=0); a position key list maps timeline→source
            // directly. Falling through to the speed curve preserves the old path
            // for un-remapped clips.
            let source_t = if clip.time_remap_enabled && !clip.time_remap_speed_keys.is_empty() {
                remapped_source_time(
                    &clip.time_remap_speed_keys, clip.source_in.max(0.0), clip.start, t,
                )
            } else if clip.time_remap_enabled && clip.time_remap_keys.len() >= 2 {
                clip.remapped_source_t(t)
            } else {
                let local_t = (t - clip.start).max(0.0);
                let effective_local = match &clip.speed_curve {
                    SpeedCurve::Constant => local_t * clip.speed,
                    SpeedCurve::Bezier { p0, p1, p2, p3 } => {
                        bezier_source_time(local_t, clip.duration, *p0, *p1, *p2, *p3) * clip.speed
                    }
                };
                if clip.reversed {
                    (clip.duration - effective_local).max(0.0) + clip.source_in.max(0.0)
                } else {
                    effective_local + clip.source_in.max(0.0)
                }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app_state::{
        Caption, CaptionPosition, CaptionStyle, Clip, HslSecondaryGrade, Project, Track,
    };

    fn color_clip(rgba: [f32; 4], track: usize, start: f32, dur: f32) -> Clip {
        Clip {
            name: "c".into(),
            source: ClipSource::Color(rgba),
            track,
            start,
            duration: dur,
            ..Clip::default()
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

    #[test]
    fn remap_speed_constant_unity_is_realtime() {
        // A single 1.0 key, clip starting at t=2 with source_in=0: at timeline
        // t=2 source is 0, at t=5 source has advanced 3s (real-time).
        let keys = [(2.0_f32, 1.0_f32)];
        assert!((remapped_source_time(&keys, 0.0, 2.0, 2.0) - 0.0).abs() < 1e-4);
        assert!((remapped_source_time(&keys, 0.0, 2.0, 5.0) - 3.0).abs() < 1e-4);
    }

    #[test]
    fn remap_speed_double_advances_twice_as_fast() {
        // Constant 2.0 speed: 4 timeline seconds → 8 source seconds.
        let keys = [(0.0_f32, 2.0_f32), (4.0, 2.0)];
        assert!((remapped_source_time(&keys, 0.0, 0.0, 4.0) - 8.0).abs() < 1e-4);
        assert!((remapped_source_time(&keys, 0.0, 0.0, 2.0) - 4.0).abs() < 1e-4);
    }

    #[test]
    fn remap_speed_freeze_holds_source_time() {
        // Real-time for 2s, then freeze (factor 0) for the rest: source time
        // climbs to 2.0 then stops.
        let keys = [(0.0_f32, 1.0_f32), (2.0, 1.0), (2.0, 0.0), (6.0, 0.0)];
        assert!((remapped_source_time(&keys, 0.0, 0.0, 2.0) - 2.0).abs() < 1e-4);
        // At t=4 (mid-freeze) and t=6 (end) the source is still 2.0.
        assert!((remapped_source_time(&keys, 0.0, 0.0, 4.0) - 2.0).abs() < 1e-4);
        assert!((remapped_source_time(&keys, 0.0, 0.0, 6.0) - 2.0).abs() < 1e-4);
    }

    #[test]
    fn remap_speed_ramp_integrates_trapezoid() {
        // A linear ramp 0→2 over [0,4]: area = 0.5 * (0+2) * 4 = 4.0 source secs.
        let keys = [(0.0_f32, 0.0_f32), (4.0, 2.0)];
        assert!((remapped_source_time(&keys, 0.0, 0.0, 4.0) - 4.0).abs() < 1e-4);
        // Halfway, the speed at t=2 is 1.0; area = 0.5*(0+1)*2 = 1.0.
        assert!((remapped_source_time(&keys, 0.0, 0.0, 2.0) - 1.0).abs() < 1e-4);
    }

    #[test]
    fn remap_speed_respects_source_in_offset() {
        // source_in shifts the whole curve up by the in-point.
        let keys = [(0.0_f32, 1.0_f32), (3.0, 1.0)];
        assert!((remapped_source_time(&keys, 5.0, 0.0, 0.0) - 5.0).abs() < 1e-4);
        assert!((remapped_source_time(&keys, 5.0, 0.0, 3.0) - 8.0).abs() < 1e-4);
    }
}
