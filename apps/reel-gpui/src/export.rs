//! The GPUI host's **video export engine** — render the program to an MP4 with
//! audio, off the UI thread, with progress.
//!
//! This mirrors the egui `reel-app`'s `export.rs` + `render_queue.rs`, reusing
//! Reel's shared CPU program sampler ([`crate::program_frame::render_program_at`])
//! at **full comp resolution** so the exported frames match the preview (same
//! track fold, opacity, and cross-dissolve transitions). Frames are rendered
//! **lazily**, one at a time, and piped to ffmpeg via the shared, read-only
//! [`prism_media`] encoder (`encode_h264` / `encode_h264_with_audio`), so a long
//! export never holds every frame in memory.
//!
//! # Audio
//!
//! The program audio mix is rendered by summing each [`ClipSource::Audio`] clip's
//! decoded PCM into the export span (see [`render_program_audio`]). A non-empty
//! mix is muxed as an AAC track; a silent program falls back to a silent encode.
//!
//! # Threading
//!
//! [`run_job`] runs one [`JobSpec`] to completion on a worker thread, streaming
//! [`JobUpdate`] progress over an `mpsc` channel back to the UI. The toolbar's
//! Export action gates on [`prism_media::ffmpeg_available`], snapshots the
//! project + audio mix into a `JobSpec`, spawns the worker, and polls the
//! channel each animation frame to show progress.

use std::path::{Path, PathBuf};
use std::sync::mpsc::Sender;

use crate::app_state::{
    AudioEffect, AudioTrackType, ClipSource, EqBand, Project, TrackCompressor, TrackEq, TrackEq3,
};
use crate::program_frame::{render_program_at, FrameCache, GlobalGrade};

/// Render `project`'s program at time `t` into a straight-sRGB RGBA8 buffer at
/// the comp's **full** `width`x`height`, flattened over opaque black. The
/// full-resolution twin of the preview sampler — it calls the same shared
/// compositor ([`render_program_at`]) so the export honors the same track fold,
/// opacity, and cross-dissolve transitions as the program monitor.
pub fn render_program(
    project: &Project,
    t: f32,
    cache: &mut FrameCache,
    captions: &[crate::app_state::Caption],
) -> (u32, u32, Vec<u8>) {
    render_program_at(
        project,
        t,
        project.width,
        project.height,
        cache,
        &GlobalGrade::identity(),
        None,
        captions,
    )
}

// --- Frame plan -------------------------------------------------------------

/// The inclusive frame index range `[first, last]` plus the frame `count` to
/// encode for a video export. Built by [`video_frame_plan`] from a time range +
/// fps. Pure / unit-tested. Mirrors the egui app's `FramePlan`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FramePlan {
    pub first: u64,
    pub last: u64,
    pub count: u64,
}

/// Resolve the frame plan for a video export over `[start, end)` at `fps`.
/// Returns `None` for a zero-length / inverted / sub-frame range (a video must
/// have ≥1 real frame). Mirrors the egui app's `video_frame_plan`.
pub fn video_frame_plan(start: f32, end: f32, fps: f32) -> Option<FramePlan> {
    let fps = fps.max(1.0);
    let start = start.max(0.0);
    if end <= start {
        return None;
    }
    let count = ((end - start) * fps).round() as i64;
    if count <= 0 {
        return None;
    }
    let count = count as u64;
    let first = (start * fps).round().max(0.0) as u64;
    Some(FramePlan {
        first,
        last: first + count - 1,
        count,
    })
}

/// Ensure an output path ends in `.mp4` (case-insensitive): appends the
/// extension when missing / different. Pure. Mirrors the egui app's
/// `ensure_mp4_extension`.
pub fn ensure_mp4_extension(path: &Path) -> PathBuf {
    let is_mp4 = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.eq_ignore_ascii_case("mp4"))
        .unwrap_or(false);
    if is_mp4 {
        path.to_path_buf()
    } else {
        let mut s = path.as_os_str().to_os_string();
        s.push(".mp4");
        PathBuf::from(s)
    }
}

/// Ensure an output path ends in `.mov` (case-insensitive).
pub fn ensure_mov_extension(path: &Path) -> PathBuf {
    let is_mov = path.extension().and_then(|e| e.to_str())
        .map(|e| e.eq_ignore_ascii_case("mov")).unwrap_or(false);
    if is_mov { path.to_path_buf() } else {
        let mut s = path.as_os_str().to_os_string(); s.push(".mov"); PathBuf::from(s)
    }
}

/// Ensure an output path ends in `.gif` (case-insensitive).
pub fn ensure_gif_extension(path: &Path) -> PathBuf {
    let is_gif = path.extension().and_then(|e| e.to_str())
        .map(|e| e.eq_ignore_ascii_case("gif")).unwrap_or(false);
    if is_gif { path.to_path_buf() } else {
        let mut s = path.as_os_str().to_os_string(); s.push(".gif"); PathBuf::from(s)
    }
}

/// Sanitize a string into a filesystem-safe file stem for the default export
/// name: keep `[A-Za-z0-9._-]`, collapse the rest to `_`, falling back to
/// `"export"` for an empty result. Pure. Mirrors the egui app's `sanitize_stem`.
pub fn sanitize_stem(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-') {
                c
            } else {
                '_'
            }
        })
        .collect();
    let trimmed = cleaned.trim_matches('_');
    if trimmed.is_empty() {
        "export".to_string()
    } else {
        cleaned
    }
}

// --- Audio mix --------------------------------------------------------------

/// The sample rate the export audio mix is rendered at (matches the AAC mux's
/// expectation; ffmpeg resamples as needed).
pub const EXPORT_SAMPLE_RATE: u32 = 48_000;
/// The channel count of the export audio mix (stereo).
pub const EXPORT_CHANNELS: u16 = 2;

/// An owned, thread-safe audio clip for the export mix: a decoded mono PCM buffer
/// plus its timeline placement. Built from each [`ClipSource::Audio`] clip by
/// [`build_mix_clips`] so the worker thread can render the mix without touching
/// the live editor.
#[derive(Clone, Debug)]
pub struct MixClip {
    /// Timeline start (seconds).
    pub start: f32,
    /// Timeline duration (seconds).
    pub duration: f32,
    /// In-point into the source media (seconds).
    pub source_in: f32,
    /// Per-clip linear gain (volume) applied to every sample before summing.
    pub gain: f32,
    /// Decoded mono samples at [`EXPORT_SAMPLE_RATE`].
    pub samples: std::sync::Arc<Vec<f32>>,
    /// Parametric EQ to apply during mix (from the clip's parent track).
    pub eq: Option<TrackEq>,
    /// Peak compressor to apply during mix (from the clip's parent track).
    pub comp: Option<TrackCompressor>,
    /// The parent track's ordered audio-effect chain (3-band EQ / compressor /
    /// reverb / delay) applied during mix after the track EQ/compressor above.
    pub fx: Vec<AudioEffect>,
    /// Channel format of the source track.
    pub track_type: AudioTrackType,
}

/// Decode each [`ClipSource::Audio`] clip in `project` into an owned [`MixClip`]
/// at [`EXPORT_SAMPLE_RATE`] (mono), skipping clips whose audio fails to decode.
/// Called at enqueue time (on the UI thread) so the worker's mix render is pure.
/// A missing ffmpeg / bad media yields an empty list → a silent export.
pub fn build_mix_clips(
    project: &Project,
    track_eq: &[TrackEq],
    track_comp: &[TrackCompressor],
    track_types: &[AudioTrackType],
    audio_effects: &[Vec<AudioEffect>],
) -> Vec<MixClip> {
    let mut out = Vec::new();
    for clip in &project.clips {
        let ClipSource::Audio(audio) = &clip.source else {
            continue;
        };
        let ti = clip.track;
        let eq = track_eq.get(ti).cloned().filter(|e| e.enabled);
        let comp = track_comp.get(ti).cloned().filter(|c| c.enabled);
        let fx = audio_effects.get(ti).cloned().unwrap_or_default();
        let track_type = track_types.get(ti).copied().unwrap_or(AudioTrackType::Stereo);
        match prism_media::decode_audio(&audio.path, EXPORT_SAMPLE_RATE, 1) {
            Ok(buf) => out.push(MixClip {
                start: clip.start,
                duration: clip.duration,
                source_in: clip.source_in,
                gain: audio.effective_gain(),
                samples: std::sync::Arc::new(buf.samples),
                eq,
                comp,
                fx,
                track_type,
            }),
            Err(e) => log::warn!(
                "reel-gpui: export audio decode failed for {} ({e}); skipping",
                audio.path.display()
            ),
        }
    }
    out
}

/// Apply a single biquad peaking-EQ band in-place to `samples`.
fn apply_eq_band(samples: &mut [f32], band: &EqBand, sample_rate: u32) {
    if band.gain_db.abs() < 0.01 {
        return;
    }
    let w0 = 2.0 * std::f64::consts::PI * band.freq as f64 / sample_rate as f64;
    let a = 10f64.powf(band.gain_db as f64 / 40.0);
    let alpha = w0.sin() / (2.0 * band.q as f64);
    let cos_w0 = w0.cos();
    let b0 = 1.0 + alpha * a;
    let b1 = -2.0 * cos_w0;
    let b2 = 1.0 - alpha * a;
    let a0 = 1.0 + alpha / a;
    let a1 = -2.0 * cos_w0;
    let a2 = 1.0 - alpha / a;
    let (b0, b1, b2, a1, a2) = (b0/a0, b1/a0, b2/a0, a1/a0, a2/a0);
    let (mut x1, mut x2, mut y1, mut y2) = (0f64, 0f64, 0f64, 0f64);
    for s in samples.iter_mut() {
        let x0 = *s as f64;
        let y0 = b0*x0 + b1*x1 + b2*x2 - a1*y1 - a2*y2;
        x2 = x1; x1 = x0; y2 = y1; y1 = y0;
        *s = y0 as f32;
    }
}

/// Apply a simple peak compressor in-place.
fn apply_compressor(samples: &mut [f32], comp: &TrackCompressor, sample_rate: u32) {
    let threshold = 10f32.powf(comp.threshold_db / 20.0);
    let makeup = 10f32.powf(comp.makeup_db / 20.0);
    let attack = (1.0 - (-2.2 / (comp.attack_ms as f32 * 0.001 * sample_rate as f32)).exp()).clamp(0.0, 1.0);
    let release = (1.0 - (-2.2 / (comp.release_ms as f32 * 0.001 * sample_rate as f32)).exp()).clamp(0.0, 1.0);
    let mut env = 0f32;
    for s in samples.iter_mut() {
        let level = s.abs();
        if level > env { env += attack * (level - env); } else { env += release * (level - env); }
        let gain = if env > threshold {
            let excess = env / threshold;
            let reduced = threshold * excess.powf(1.0 / comp.ratio - 1.0);
            (reduced / env).clamp(0.0, 1.0)
        } else {
            1.0
        };
        *s *= gain * makeup;
    }
}

/// Dispatch one [`AudioEffect`] from a track's chain, applying its DSP to
/// `samples` in place at `sample_rate`. Each effect is a real, deterministic
/// processor (no I/O), so the export mix and the playback mix read identically.
fn apply_audio_effect(samples: &mut [f32], fx: &AudioEffect, sample_rate: u32) {
    match fx {
        AudioEffect::Eq3(eq) => apply_eq3(samples, eq, sample_rate),
        AudioEffect::Compressor { threshold_db, ratio, attack_ms, release_ms } => {
            // Reuse the peak compressor (no make-up gain on a chain compressor).
            let comp = TrackCompressor {
                enabled: true,
                threshold_db: *threshold_db,
                ratio: *ratio,
                attack_ms: *attack_ms,
                release_ms: *release_ms,
                makeup_db: 0.0,
            };
            apply_compressor(samples, &comp, sample_rate);
        }
        AudioEffect::Reverb { room_size, damping, wet } => {
            apply_reverb(samples, *room_size, *damping, *wet, sample_rate)
        }
        AudioEffect::Delay { time_ms, feedback, wet } => {
            apply_delay(samples, *time_ms, *feedback, *wet, sample_rate)
        }
    }
}

/// Apply a 3-band EQ ([`TrackEq3`]) to `samples` in place: a low shelf at 200 Hz,
/// a parametric peak at `mid_freq`, and a high shelf at 8 kHz, each as one RBJ
/// biquad. A 0 dB band is a no-op.
fn apply_eq3(samples: &mut [f32], eq: &TrackEq3, sample_rate: u32) {
    apply_shelf(samples, 200.0, eq.low_gain_db, sample_rate, true);
    apply_eq_band(
        samples,
        &EqBand { freq: eq.mid_freq.clamp(20.0, 20_000.0), gain_db: eq.mid_gain_db, q: 0.9 },
        sample_rate,
    );
    apply_shelf(samples, 8000.0, eq.high_gain_db, sample_rate, false);
}

/// Apply a single RBJ low/high-shelf biquad in place. `low = true` is a low
/// shelf, `false` a high shelf. A near-0 dB gain is a no-op.
fn apply_shelf(samples: &mut [f32], freq: f32, gain_db: f32, sample_rate: u32, low: bool) {
    if gain_db.abs() < 0.01 {
        return;
    }
    let a = 10f64.powf(gain_db as f64 / 40.0);
    let w0 = 2.0 * std::f64::consts::PI * freq as f64 / sample_rate as f64;
    let cos_w0 = w0.cos();
    let sin_w0 = w0.sin();
    // Shelf slope S = 1 (Butterworth-ish).
    let alpha = sin_w0 / 2.0 * ((a + 1.0 / a) * (1.0 / 1.0 - 1.0) + 2.0).sqrt();
    let two_sqrt_a_alpha = 2.0 * a.sqrt() * alpha;
    let (b0, b1, b2, a0, a1, a2) = if low {
        (
            a * ((a + 1.0) - (a - 1.0) * cos_w0 + two_sqrt_a_alpha),
            2.0 * a * ((a - 1.0) - (a + 1.0) * cos_w0),
            a * ((a + 1.0) - (a - 1.0) * cos_w0 - two_sqrt_a_alpha),
            (a + 1.0) + (a - 1.0) * cos_w0 + two_sqrt_a_alpha,
            -2.0 * ((a - 1.0) + (a + 1.0) * cos_w0),
            (a + 1.0) + (a - 1.0) * cos_w0 - two_sqrt_a_alpha,
        )
    } else {
        (
            a * ((a + 1.0) + (a - 1.0) * cos_w0 + two_sqrt_a_alpha),
            -2.0 * a * ((a - 1.0) + (a + 1.0) * cos_w0),
            a * ((a + 1.0) + (a - 1.0) * cos_w0 - two_sqrt_a_alpha),
            (a + 1.0) - (a - 1.0) * cos_w0 + two_sqrt_a_alpha,
            2.0 * ((a - 1.0) - (a + 1.0) * cos_w0),
            (a + 1.0) - (a - 1.0) * cos_w0 - two_sqrt_a_alpha,
        )
    };
    let (b0, b1, b2, a1, a2) = (b0 / a0, b1 / a0, b2 / a0, a1 / a0, a2 / a0);
    let (mut x1, mut x2, mut y1, mut y2) = (0f64, 0f64, 0f64, 0f64);
    for s in samples.iter_mut() {
        let x0 = *s as f64;
        let y0 = b0 * x0 + b1 * x1 + b2 * x2 - a1 * y1 - a2 * y2;
        x2 = x1;
        x1 = x0;
        y2 = y1;
        y1 = y0;
        *s = y0 as f32;
    }
}

/// Apply a Schroeder reverb (4 parallel comb filters → 2 series allpass) in
/// place, blending `wet` of the reverberated signal with `1-wet` dry. `room_size`
/// (0..1) scales comb feedback (longer tail); `damping` (0..1) low-passes the
/// comb feedback (darker tail). A `wet <= 0` is a no-op.
fn apply_reverb(samples: &mut [f32], room_size: f32, damping: f32, wet: f32, sample_rate: u32) {
    let wet = wet.clamp(0.0, 1.0);
    if wet <= 0.0 || samples.is_empty() {
        return;
    }
    let room = room_size.clamp(0.0, 1.0);
    let damp = damping.clamp(0.0, 1.0);
    let feedback = 0.7 + 0.28 * room; // 0.70..0.98
    // Comb delay lengths (Freeverb tuning, scaled to the sample rate).
    let base = [1116usize, 1188, 1277, 1356];
    let sr_scale = sample_rate as f32 / 44_100.0;
    let comb_len: Vec<usize> = base.iter().map(|&l| ((l as f32 * sr_scale) as usize).max(1)).collect();
    let mut comb_bufs: Vec<Vec<f32>> = comb_len.iter().map(|&l| vec![0.0f32; l]).collect();
    let mut comb_idx = vec![0usize; comb_len.len()];
    let mut comb_filt = vec![0.0f32; comb_len.len()]; // damping low-pass state.

    // Allpass tuning.
    let ap_base = [556usize, 441];
    let ap_len: Vec<usize> = ap_base.iter().map(|&l| ((l as f32 * sr_scale) as usize).max(1)).collect();
    let mut ap_bufs: Vec<Vec<f32>> = ap_len.iter().map(|&l| vec![0.0f32; l]).collect();
    let mut ap_idx = vec![0usize; ap_len.len()];
    let ap_feedback = 0.5f32;

    for s in samples.iter_mut() {
        let dry = *s;
        // Parallel combs.
        let mut acc = 0.0f32;
        for c in 0..comb_bufs.len() {
            let buf = &mut comb_bufs[c];
            let i = comb_idx[c];
            let y = buf[i];
            acc += y;
            // Damped feedback low-pass.
            comb_filt[c] = y * (1.0 - damp) + comb_filt[c] * damp;
            buf[i] = dry + comb_filt[c] * feedback;
            comb_idx[c] = (i + 1) % buf.len();
        }
        acc /= comb_bufs.len() as f32;
        // Series allpasses.
        for a in 0..ap_bufs.len() {
            let buf = &mut ap_bufs[a];
            let i = ap_idx[a];
            let bufout = buf[i];
            let out = -acc + bufout;
            buf[i] = acc + bufout * ap_feedback;
            ap_idx[a] = (i + 1) % buf.len();
            acc = out;
        }
        *s = dry * (1.0 - wet) + acc * wet;
    }
}

/// Apply a feedback delay line in place: each sample is mixed with a copy of the
/// signal `time_ms` ago, fed back by `feedback` (0..1, clamped <1 to decay),
/// blended `wet` against the dry signal. A `wet <= 0` or non-positive time is a
/// no-op.
fn apply_delay(samples: &mut [f32], time_ms: f32, feedback: f32, wet: f32, sample_rate: u32) {
    let wet = wet.clamp(0.0, 1.0);
    let delay_samples = ((time_ms.max(0.0) / 1000.0) * sample_rate as f32).round() as usize;
    if wet <= 0.0 || delay_samples == 0 || samples.is_empty() {
        return;
    }
    let fb = feedback.clamp(0.0, 0.95);
    let mut line = vec![0.0f32; delay_samples];
    let mut idx = 0usize;
    for s in samples.iter_mut() {
        let dry = *s;
        let delayed = line[idx];
        line[idx] = dry + delayed * fb;
        idx = (idx + 1) % delay_samples;
        *s = dry * (1.0 - wet) + delayed * wet;
    }
}

/// Render the program **audio mix** over the export `plan`'s time span into an
/// interleaved `f32` [`prism_media::AudioMix`] at [`EXPORT_SAMPLE_RATE`] /
/// [`EXPORT_CHANNELS`]. Each clip's decoded mono PCM is summed into the output at
/// its timeline position (offset by `source_in`), then the sum is soft-clamped to
/// `[-1, 1]`. The span matches the video exactly: it starts at `plan.first / fps`
/// and runs `plan.count` frames. Pure (no I/O) so the range / sample-count math
/// is unit-testable without ffmpeg.
pub fn render_program_audio(clips: &[MixClip], plan: FramePlan, fps: f32) -> prism_media::AudioMix {
    let fps = fps.max(1.0);
    let rate = EXPORT_SAMPLE_RATE;
    let channels = EXPORT_CHANNELS as usize;
    let t_start = plan.first as f32 / fps;
    let seconds = plan.count as f32 / fps;
    let out_frames = (seconds * rate as f32).round().max(0.0) as usize;

    // Accumulate a mono mix, then fan out to the interleaved stereo output.
    let mut mono = vec![0.0f32; out_frames];
    for clip in clips {
        if clip.samples.is_empty() || clip.duration <= 0.0 {
            continue;
        }
        let gain = if clip.gain.is_finite() { clip.gain.max(0.0) } else { 1.0 };
        if gain <= 0.0 {
            continue;
        }
        // Extract the clip's span into a local buffer for DSP processing.
        let clip_end = clip.start + clip.duration;
        let clip_frame_start = ((clip.start - t_start) * rate as f32).round() as isize;
        let clip_frames = ((clip.duration) * rate as f32).round() as usize;
        let src = &clip.samples;
        let src_len = src.len();
        let mut clip_buf: Vec<f32> = (0..clip_frames).map(|fi| {
            let src_t = fi as f32 / rate as f32 + clip.source_in.max(0.0);
            let si = (src_t * rate as f32).round() as usize;
            if si < src_len { src[si] * gain } else { 0.0 }
        }).collect();

        // Apply per-track EQ bands.
        if let Some(eq) = &clip.eq {
            for band in &eq.bands {
                apply_eq_band(&mut clip_buf, band, rate);
            }
        }
        // Apply per-track compressor.
        if let Some(comp) = &clip.comp {
            apply_compressor(&mut clip_buf, comp, rate);
        }
        // Apply the track's ordered audio-effect chain (3-band EQ / compressor /
        // reverb / delay) — real DSP, in chain order, after the track EQ/comp.
        for fx in &clip.fx {
            apply_audio_effect(&mut clip_buf, fx, rate);
        }

        // Sum into the mono accumulator.
        for (fi, &s) in clip_buf.iter().enumerate() {
            let out_fi = clip_frame_start + fi as isize;
            if out_fi < 0 { continue; }
            let out_fi = out_fi as usize;
            if out_fi < mono.len() {
                mono[out_fi] += s;
            }
        }
        let _ = clip_end; // used above in logic
    }

    // Interleave mono → stereo with a soft clamp so summed clips don't wrap.
    let mut samples = Vec::with_capacity(out_frames * channels);
    for &m in &mono {
        let v = m.clamp(-1.0, 1.0);
        for _ in 0..channels {
            samples.push(v);
        }
    }
    prism_media::AudioMix::new(samples, rate, EXPORT_CHANNELS)
}

// --- Lazy render + encode ---------------------------------------------------

/// Render + encode `project` over `plan` to an H.264 MP4 at `path` (extension
/// normalized to `.mp4`), emitting a **per-frame progress callback** so a
/// background worker can stream progress to the UI. Frames are rendered lazily
/// (decode cached across frames, never holding every frame in memory); each
/// rendered frame fires `on_frame(frame_index)` (0-based) just before it is piped
/// to ffmpeg. When `audio` is a non-empty mix it is muxed as an AAC track; an
/// empty / silent mix encodes silently. ffmpeg errors (incl. a missing binary)
/// return the encoder's [`prism_media::MediaError`] rather than panicking.
/// Mirrors the egui app's `with_progress_encode`.
pub fn with_progress_encode<P>(
    project: &Project,
    plan: FramePlan,
    audio: Option<&prism_media::AudioMix>,
    path: &Path,
    captions: &[crate::app_state::Caption],
    mut on_frame: P,
) -> Result<usize, prism_media::MediaError>
where
    P: FnMut(u64),
{
    let out = ensure_mp4_extension(path);
    let w = project.width.max(1);
    let h = project.height.max(1);
    let fps = project.fps.max(1.0);

    // A throwaway clone the lazy iterator owns so it renders across the encode
    // without borrowing `project` for the whole call.
    let project = project.clone();
    let captions = captions.to_vec();
    let mut cache = FrameCache::new();
    let frames = (plan.first..=plan.last).enumerate().map(move |(i, index)| {
        on_frame(i as u64);
        let t = index as f32 / fps;
        let (_, _, rgba) = render_program(&project, t, &mut cache, &captions);
        rgba
    });

    let params = prism_media::EncodeParams::new(w, h, fps as f64);
    match audio {
        Some(audio) if !audio.is_empty() => {
            prism_media::encode_h264_with_audio(frames, &params, audio, &out)
        }
        _ => prism_media::encode_h264(frames, &params, &out),
    }
}

// --- Background job ----------------------------------------------------------

/// The thread-safe snapshot one export job renders from, captured at enqueue time
/// so the worker is fully decoupled from the live editor. Everything is owned +
/// `Send`. Mirrors the egui app's `JobSpec`.
#[derive(Clone, Debug)]
pub struct JobSpec {
    /// A clone of the sequence to render.
    pub project: Project,
    /// The inclusive frame plan + count to encode.
    pub plan: FramePlan,
    /// The program audio mix over the export span (muxed when non-empty).
    pub audio: prism_media::AudioMix,
    /// True when `audio` should be muxed (non-empty, pre-computed at enqueue).
    pub has_audio: bool,
    /// The output `.mp4` path (normalized by the encoder).
    pub path: PathBuf,
    /// The caption cues to burn into each exported frame (snapshot at enqueue).
    pub captions: Vec<crate::app_state::Caption>,
}

impl JobSpec {
    /// The total frame count this job renders (drives the progress bar).
    pub fn total_frames(&self) -> u64 {
        self.plan.count
    }
}

/// The lifecycle state of an export job. `Rendering → (Done | Failed)`. Mirrors
/// the egui app's `JobStatus`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum JobStatus {
    /// In flight: `done` of `total` frames encoded so far.
    Rendering { done: u64, total: u64 },
    /// Finished: `frames` were written to the output file.
    Done { frames: u64 },
    /// Failed before completing, with a human-readable reason.
    Failed { message: String },
}

impl JobStatus {
    /// True once the job has reached a terminal state (done or failed).
    pub fn is_terminal(&self) -> bool {
        matches!(self, JobStatus::Done { .. } | JobStatus::Failed { .. })
    }

    /// Fractional progress in `0.0..=1.0`.
    pub fn fraction(&self) -> f32 {
        match self {
            JobStatus::Rendering { done, total } => {
                if *total == 0 {
                    0.0
                } else {
                    (*done as f32 / *total as f32).clamp(0.0, 1.0)
                }
            }
            JobStatus::Done { .. } => 1.0,
            JobStatus::Failed { .. } => 0.0,
        }
    }

    /// A short status label for the toolbar (e.g. `"Exporting 42%"`).
    pub fn label(&self) -> String {
        match self {
            JobStatus::Rendering { done, total } => {
                let pct = (self.fraction() * 100.0).round() as u32;
                format!("Exporting {pct}% ({done}/{total})")
            }
            JobStatus::Done { frames } => format!("Exported — {frames} frame(s)"),
            JobStatus::Failed { message } => format!("Export failed — {message}"),
        }
    }
}

/// A progress / completion update streamed from the worker thread to the UI over
/// an `mpsc` channel. Mirrors the egui app's `JobUpdate`.
#[derive(Clone, Debug)]
pub struct JobUpdate {
    pub status: JobStatus,
}

/// Run one export `spec` to completion on the **calling (worker) thread**,
/// streaming progress over `tx` as it renders and returning the final status. A
/// missing ffmpeg / encode error returns a [`JobStatus::Failed`] rather than
/// panicking. The final status is also sent over `tx` so a UI that only watches
/// the channel sees it. Mirrors the egui app's `run_job`.
pub fn run_job(spec: &JobSpec, tx: &Sender<JobUpdate>) -> JobStatus {
    let total = spec.total_frames();
    let _ = tx.send(JobUpdate {
        status: JobStatus::Rendering { done: 0, total },
    });

    let result = with_progress_encode(
        &spec.project,
        spec.plan,
        spec.has_audio.then_some(&spec.audio),
        &spec.path,
        &spec.captions,
        |frame_index| {
            let _ = tx.send(JobUpdate {
                status: JobStatus::Rendering {
                    done: frame_index + 1,
                    total,
                },
            });
        },
    );

    let status = match result {
        Ok(frames) => JobStatus::Done {
            frames: frames as u64,
        },
        Err(e) => JobStatus::Failed {
            message: e.to_string(),
        },
    };
    let _ = tx.send(JobUpdate {
        status: status.clone(),
    });
    status
}

/// Build a [`JobSpec`] to export the whole of `project` to `path`: the full
/// `[0, duration)` time span at the project fps, with the program audio mix
/// pre-rendered and `has_audio` decided. Returns `None` when the sequence has no
/// frames (a zero / degenerate duration). The audio mix is rendered here (on the
/// caller's thread) so the worker is pure. Mirrors the egui app's export command
/// snapshotting.
pub fn build_full_export(
    project: &Project,
    path: &Path,
    captions: &[crate::app_state::Caption],
) -> Option<JobSpec> {
    let plan = video_frame_plan(0.0, project.duration, project.fps)?;
    let mix_clips = build_mix_clips(project, &[], &[], &[], &[]);
    let audio = render_program_audio(&mix_clips, plan, project.fps);
    // Muxable only if there is any audible (non-zero) content.
    let has_audio = !audio.is_empty() && audio.samples.iter().any(|&s| s != 0.0);
    Some(JobSpec {
        project: project.clone(),
        plan,
        audio,
        has_audio,
        path: ensure_mp4_extension(path),
        captions: captions.to_vec(),
    })
}

/// Used by [`crate::Reel`] to hold a running export's progress receiver + status,
/// polled each animation frame. A small wrapper so the root view owns one field.
pub struct ExportJob {
    /// The channel the worker streams [`JobUpdate`]s over.
    pub rx: std::sync::mpsc::Receiver<JobUpdate>,
    /// The latest known status (updated by polling `rx`).
    pub status: JobStatus,
}

impl ExportJob {
    /// Drain any pending updates from the worker, advancing `status` to the most
    /// recent. Returns `true` while the job is still in flight (the caller keeps
    /// re-arming the next animation-frame poll), `false` once terminal.
    pub fn poll(&mut self) -> bool {
        while let Ok(update) = self.rx.try_recv() {
            self.status = update.status;
        }
        !self.status.is_terminal()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app_state::{Clip, ClipSource, Project};

    #[test]
    fn video_frame_plan_indexing() {
        let p = video_frame_plan(0.0, 1.0, 24.0).expect("non-empty");
        assert_eq!((p.first, p.last, p.count), (0, 23, 24));
        let p = video_frame_plan(1.0, 2.0, 24.0).expect("non-empty");
        assert_eq!((p.first, p.last, p.count), (24, 47, 24));
        let p = video_frame_plan(0.0, 0.5, 30.0).expect("non-empty");
        assert_eq!(p.count, 15);
    }

    #[test]
    fn video_frame_plan_rejects_zero_length() {
        assert!(video_frame_plan(0.5, 0.5, 24.0).is_none());
        assert!(video_frame_plan(2.0, 1.0, 24.0).is_none());
        assert!(video_frame_plan(0.0, 0.04, 24.0).is_some());
        assert!(video_frame_plan(0.0, 0.001, 24.0).is_none());
    }

    #[test]
    fn ensure_mp4_extension_appends_when_needed() {
        assert_eq!(ensure_mp4_extension(Path::new("/tmp/out.mp4")), PathBuf::from("/tmp/out.mp4"));
        assert_eq!(ensure_mp4_extension(Path::new("/tmp/out.MP4")), PathBuf::from("/tmp/out.MP4"));
        assert_eq!(ensure_mp4_extension(Path::new("/tmp/out")), PathBuf::from("/tmp/out.mp4"));
        assert_eq!(ensure_mp4_extension(Path::new("/tmp/out.mov")), PathBuf::from("/tmp/out.mov.mp4"));
    }

    #[test]
    fn sanitize_stem_is_filesystem_safe() {
        assert_eq!(sanitize_stem("My Clip!"), "My_Clip_");
        assert_eq!(sanitize_stem("a/b:c"), "a_b_c");
        assert_eq!(sanitize_stem("   "), "export");
        assert_eq!(sanitize_stem("ok-name_1.2"), "ok-name_1.2");
    }

    /// The default project renders at full comp resolution into a buffer of the
    /// right length whose center is the seeded teal/over-clip content (opaque).
    #[test]
    fn render_program_full_res_buffer_length_and_opaque() {
        let project = Project::new();
        let mut cache = FrameCache::new();
        let (w, h, rgba) = render_program(&project, 0.0, &mut cache, &[]);
        assert_eq!(w, project.width);
        assert_eq!(h, project.height);
        assert_eq!(rgba.len(), (w as usize) * (h as usize) * 4);
        // Center pixel is opaque after flatten over black.
        let idx = (((h / 2) as usize) * (w as usize) + (w / 2) as usize) * 4;
        assert_eq!(rgba[idx + 3], 255);
    }

    /// Past the end of the program the frame is the black comp of the right size.
    #[test]
    fn render_past_end_is_black() {
        let project = Project::new();
        let mut cache = FrameCache::new();
        let (w, h, rgba) = render_program(&project, project.duration + 5.0, &mut cache, &[]);
        assert_eq!(rgba.len(), (w as usize) * (h as usize) * 4);
        assert!(rgba
            .chunks_exact(4)
            .all(|p| p[0] == 0 && p[1] == 0 && p[2] == 0 && p[3] == 255));
    }

    /// A cross-dissolve at the midpoint blends both color clips: at the exact cut
    /// (progress 0.5) the rendered center is roughly the average of the two.
    #[test]
    fn render_cross_dissolve_blends_two_color_clips() {
        // Two abutting full-frame color clips on one track + a dissolve at the cut.
        let mut project = Project {
            name: "x".into(),
            width: 16,
            height: 16,
            fps: 30.0,
            duration: 10.0,
            tracks: vec![crate::app_state::Track { name: "V1".into(), enabled: true }],
            clips: vec![
                Clip {
                    name: "Red".into(),
                    source: ClipSource::Color([1.0, 0.0, 0.0, 1.0]),
                    track: 0,
                    start: 0.0,
                    duration: 4.0,
                    source_in: 0.0,
                    opacity: 1.0,
                    grade: crate::app_state::ColorGrade::default(),
                    fade_in: 0.0,
                    fade_out: 0.0,
                    speed: 1.0,
                    reversed: false,
                    hsl_secondary: crate::app_state::HslSecondaryGrade::default(),
                    speed_curve: crate::app_state::SpeedCurve::default(),
                    proxy_path: None,
                    link_group: None,
                },
                Clip {
                    name: "Blue".into(),
                    source: ClipSource::Color([0.0, 0.0, 1.0, 1.0]),
                    track: 0,
                    start: 4.0,
                    duration: 4.0,
                    source_in: 0.0,
                    opacity: 1.0,
                    grade: crate::app_state::ColorGrade::default(),
                    fade_in: 0.0,
                    fade_out: 0.0,
                    speed: 1.0,
                    reversed: false,
                    hsl_secondary: crate::app_state::HslSecondaryGrade::default(),
                    speed_curve: crate::app_state::SpeedCurve::default(),
                    proxy_path: None,
                    link_group: None,
                },
            ],
            transitions: Vec::new(),
        };
        // Add a 2s cross-dissolve centered on the cut at t=4.
        let added =
            project.add_transition(0, 4.0, crate::app_state::TransitionKind::CrossDissolve, 2.0);
        assert!(added.is_some(), "an adjacent clip exists → a transition is added");

        let mut cache = FrameCache::new();
        // At the cut center (t=4.0): progress 0.5 → both clips at weight 0.5.
        let (w, h, rgba) = render_program(&project, 4.0, &mut cache, &[]);
        let idx = (((h / 2) as usize) * (w as usize) + (w / 2) as usize) * 4;
        let px = &rgba[idx..idx + 4];
        // Straight-over fold then flatten over black: red folds to R=128 (A=0.5),
        // blue over that gives R=64, B=128, A=0.75; the flatten scales RGB by A →
        // R≈48, B≈96. Both clips are visibly present (a real dissolve), blue
        // (the incoming clip, layered last) reading stronger than red.
        assert!(px[0] > 20 && px[0] < 80, "outgoing red present but faded: {}", px[0]);
        assert!(px[2] > 70 && px[2] > px[0], "incoming blue present + stronger: {}", px[2]);
        assert_eq!(px[3], 255, "opaque after flatten");
    }

    /// Before and after the dissolve span, the program is a clean single clip
    /// (no blend): fully red before the cut-region, fully blue after.
    #[test]
    fn render_outside_dissolve_is_a_single_clip() {
        let mut project = Project {
            name: "x".into(),
            width: 8,
            height: 8,
            fps: 30.0,
            duration: 10.0,
            tracks: vec![crate::app_state::Track { name: "V1".into(), enabled: true }],
            clips: vec![
                Clip {
                    name: "Red".into(),
                    source: ClipSource::Color([1.0, 0.0, 0.0, 1.0]),
                    track: 0,
                    start: 0.0,
                    duration: 4.0,
                    source_in: 0.0,
                    opacity: 1.0,
                    grade: crate::app_state::ColorGrade::default(),
                    fade_in: 0.0,
                    fade_out: 0.0,
                    speed: 1.0,
                    reversed: false,
                    hsl_secondary: crate::app_state::HslSecondaryGrade::default(),
                    speed_curve: crate::app_state::SpeedCurve::default(),
                    proxy_path: None,
                    link_group: None,
                },
                Clip {
                    name: "Blue".into(),
                    source: ClipSource::Color([0.0, 0.0, 1.0, 1.0]),
                    track: 0,
                    start: 4.0,
                    duration: 4.0,
                    source_in: 0.0,
                    opacity: 1.0,
                    grade: crate::app_state::ColorGrade::default(),
                    fade_in: 0.0,
                    fade_out: 0.0,
                    speed: 1.0,
                    reversed: false,
                    hsl_secondary: crate::app_state::HslSecondaryGrade::default(),
                    speed_curve: crate::app_state::SpeedCurve::default(),
                    proxy_path: None,
                    link_group: None,
                },
            ],
            transitions: Vec::new(),
        };
        project
            .add_transition(0, 4.0, crate::app_state::TransitionKind::CrossDissolve, 2.0)
            .expect("transition");
        let mut cache = FrameCache::new();
        let center = |t: f32, cache: &mut FrameCache| {
            let (w, h, rgba) = render_program(&project, t, cache, &[]);
            let i = (((h / 2) as usize) * (w as usize) + (w / 2) as usize) * 4;
            [rgba[i], rgba[i + 1], rgba[i + 2]]
        };
        // Well before the 2s span [3,5): pure red.
        assert_eq!(center(1.0, &mut cache), [255, 0, 0]);
        // Well after: pure blue.
        assert_eq!(center(6.0, &mut cache), [0, 0, 255]);
    }

    /// A two-color helper project with one transition on the cut at t=4.
    fn two_clip_project(kind: crate::app_state::TransitionKind) -> Project {
        use crate::app_state::ColorGrade;
        let mut project = Project {
            name: "x".into(),
            width: 16,
            height: 16,
            fps: 30.0,
            duration: 10.0,
            tracks: vec![crate::app_state::Track { name: "V1".into(), enabled: true }],
            clips: vec![
                Clip {
                    name: "Red".into(),
                    source: ClipSource::Color([1.0, 0.0, 0.0, 1.0]),
                    track: 0,
                    start: 0.0,
                    duration: 4.0,
                    source_in: 0.0,
                    opacity: 1.0,
                    grade: ColorGrade::default(),
                    fade_in: 0.0,
                    fade_out: 0.0,
                    speed: 1.0,
                    reversed: false,
                    hsl_secondary: crate::app_state::HslSecondaryGrade::default(),
                    speed_curve: crate::app_state::SpeedCurve::default(),
                    proxy_path: None,
                    link_group: None,
                },
                Clip {
                    name: "Blue".into(),
                    source: ClipSource::Color([0.0, 0.0, 1.0, 1.0]),
                    track: 0,
                    start: 4.0,
                    duration: 4.0,
                    source_in: 0.0,
                    opacity: 1.0,
                    grade: ColorGrade::default(),
                    fade_in: 0.0,
                    fade_out: 0.0,
                    speed: 1.0,
                    reversed: false,
                    hsl_secondary: crate::app_state::HslSecondaryGrade::default(),
                    speed_curve: crate::app_state::SpeedCurve::default(),
                    proxy_path: None,
                    link_group: None,
                },
            ],
            transitions: Vec::new(),
        };
        project.add_transition(0, 4.0, kind, 2.0).expect("transition");
        project
    }

    /// A dip-to-black at its midpoint renders (near-)black at the center — the
    /// program dips through the color before the incoming clip resolves.
    #[test]
    fn render_dip_to_black_is_dark_at_midpoint() {
        use crate::app_state::{TransitionKind, DIP_BLACK};
        let project = two_clip_project(TransitionKind::DipToColor(DIP_BLACK));
        let mut cache = FrameCache::new();
        // At the cut center (t=4.0) the dip fully covers → black.
        let (w, h, rgba) = render_program(&project, 4.0, &mut cache, &[]);
        let i = (((h / 2) as usize) * (w as usize) + (w / 2) as usize) * 4;
        assert!(rgba[i] < 20 && rgba[i + 1] < 20 && rgba[i + 2] < 20, "dipped dark: {:?}", &rgba[i..i + 3]);
        // Outside the span: clean single clip (pure red before, blue after).
        let (_, _, before) = render_program(&project, 1.0, &mut cache, &[]);
        assert_eq!(&before[i..i + 3], &[255, 0, 0]);
    }

    /// A left wipe at its midpoint shows the incoming (blue) clip on the left half
    /// of the frame and the outgoing (red) clip on the right half.
    #[test]
    fn render_wipe_reveals_incoming_on_swept_side() {
        use crate::app_state::{TransitionKind, WipeDir};
        let project = two_clip_project(TransitionKind::Wipe(WipeDir::Left));
        let mut cache = FrameCache::new();
        let (w, h, rgba) = render_program(&project, 4.0, &mut cache, &[]);
        let row = (h / 2) as usize;
        let left = (row * w as usize + (w / 8) as usize) * 4; // far left → incoming blue
        let right = (row * w as usize + (7 * w / 8) as usize) * 4; // far right → outgoing red
        assert!(rgba[left + 2] > rgba[left], "left half is incoming blue");
        assert!(rgba[right] > rgba[right + 2], "right half is outgoing red");
    }

    /// A clip with a non-identity color grade renders graded pixels: a +saturation
    /// boost on a saturated red leaves red dominant; exposure +1 brightens.
    #[test]
    fn render_applies_clip_color_grade() {
        use crate::app_state::{Clip, ColorGrade, Project, Track};
        let mut project = Project {
            name: "g".into(),
            width: 8,
            height: 8,
            fps: 30.0,
            duration: 5.0,
            tracks: vec![Track { name: "V1".into(), enabled: true }],
            clips: vec![Clip {
                name: "Grey".into(),
                source: ClipSource::Color([0.25, 0.25, 0.25, 1.0]),
                track: 0,
                start: 0.0,
                duration: 5.0,
                source_in: 0.0,
                opacity: 1.0,
                grade: ColorGrade { exposure: 1.0, ..Default::default() },
                fade_in: 0.0,
                fade_out: 0.0,
                speed: 1.0,
                reversed: false,
                hsl_secondary: crate::app_state::HslSecondaryGrade::default(),
                speed_curve: crate::app_state::SpeedCurve::default(),
                proxy_path: None,
                link_group: None,
            }],
            transitions: Vec::new(),
        };
        let mut cache = FrameCache::new();
        let (w, h, rgba) = render_program(&project, 1.0, &mut cache, &[]);
        let i = (((h / 2) as usize) * (w as usize) + (w / 2) as usize) * 4;
        // +1 stop ≈ ×2: a 0.25 grey (≈64) brightens toward 0.5 (≈128).
        assert!(rgba[i] > 110 && rgba[i] < 140, "exposure brightened: {}", rgba[i]);
        // The ungraded baseline (exposure 0) stays ≈64.
        project.clips[0].grade = ColorGrade::default();
        let (_, _, base) = render_program(&project, 1.0, &mut cache, &[]);
        assert!(base[i] > 55 && base[i] < 75, "ungraded baseline ≈64: {}", base[i]);
    }

    /// A per-clip gain scales the mix: a 0.5× gain halves the summed amplitude.
    #[test]
    fn render_program_audio_applies_clip_gain() {
        let plan = video_frame_plan(0.0, 1.0, 24.0).expect("plan");
        let frames = EXPORT_SAMPLE_RATE as usize;
        let clip = MixClip {
            start: 0.0,
            duration: 1.0,
            source_in: 0.0,
            gain: 0.5,
            samples: std::sync::Arc::new(vec![1.0; frames]),
            eq: None,
            comp: None,
            fx: Vec::new(),
            track_type: AudioTrackType::Stereo,
        };
        let mix = render_program_audio(&[clip], plan, 24.0);
        // Every sample is 1.0 × 0.5 = 0.5 (well under the soft clamp).
        assert!(mix.samples.iter().all(|&s| (s - 0.5).abs() < 1e-4), "gain 0.5 halves amplitude");
        // A zero gain is silent.
        let muted = MixClip {
            start: 0.0,
            duration: 1.0,
            source_in: 0.0,
            gain: 0.0,
            samples: std::sync::Arc::new(vec![1.0; frames]),
            eq: None,
            comp: None,
            fx: Vec::new(),
            track_type: AudioTrackType::Stereo,
        };
        let mix2 = render_program_audio(&[muted], plan, 24.0);
        assert!(mix2.samples.iter().all(|&s| s == 0.0), "muted clip is silent");
    }

    /// The audio mix has the expected interleaved sample count for the plan span,
    /// and a clip present in the span produces non-silent output.
    #[test]
    fn render_program_audio_sample_count_and_audible() {
        // 1 s at 24 fps → count 24 → 1.0 s of audio.
        let plan = video_frame_plan(0.0, 1.0, 24.0).expect("plan");
        // A constant mono clip covering the whole span.
        let frames = EXPORT_SAMPLE_RATE as usize;
        let clip = MixClip {
            start: 0.0,
            duration: 1.0,
            source_in: 0.0,
            gain: 1.0,
            samples: std::sync::Arc::new(vec![0.5; frames]),
            eq: None,
            comp: None,
            fx: Vec::new(),
            track_type: AudioTrackType::Stereo,
        };
        let mix = render_program_audio(&[clip], plan, 24.0);
        let expected = EXPORT_SAMPLE_RATE as usize * EXPORT_CHANNELS as usize;
        assert_eq!(mix.samples.len(), expected, "samples ≈ dur×rate×channels");
        assert_eq!(mix.sample_rate, EXPORT_SAMPLE_RATE);
        assert_eq!(mix.channels, EXPORT_CHANNELS);
        assert!(mix.samples.iter().any(|&s| s.abs() > 0.0), "audible content");
    }

    /// No mix clips → an all-silent mix of the right length (the silent-fallback).
    #[test]
    fn render_program_audio_no_clips_is_silent() {
        let plan = video_frame_plan(0.0, 1.0, 24.0).expect("plan");
        let mix = render_program_audio(&[], plan, 24.0);
        assert_eq!(
            mix.samples.len(),
            EXPORT_SAMPLE_RATE as usize * EXPORT_CHANNELS as usize
        );
        assert!(mix.samples.iter().all(|&s| s == 0.0), "no clips → silence");
    }

    /// A delay effect produces a delayed echo: an impulse at sample 0 reappears
    /// (scaled by `wet`) `time_ms` later, and is silent before any echo.
    #[test]
    fn delay_effect_echoes_after_the_delay_time() {
        let rate = 48_000u32;
        let mut buf = vec![0.0f32; rate as usize]; // 1 s
        buf[0] = 1.0; // unit impulse
        apply_delay(&mut buf, 100.0, 0.5, 0.5, rate);
        let d = (0.1 * rate as f32) as usize; // 100 ms
        // The echo lands at the delay offset (dry+wet mix: wet * delayed).
        assert!(buf[d] > 0.1, "echo present at the delay offset");
        // A point well before the first echo (but after the dry impulse decays)
        // is silent.
        assert!(buf[d / 2].abs() < 1e-6, "silent between the impulse and the echo");
    }

    /// A reverb with wet>0 spreads an impulse into a decaying tail (energy after
    /// the impulse), and wet=0 is a true no-op.
    #[test]
    fn reverb_spreads_an_impulse_into_a_tail() {
        let rate = 48_000u32;
        let mut buf = vec![0.0f32; rate as usize];
        buf[0] = 1.0;
        apply_reverb(&mut buf, 0.8, 0.4, 0.6, rate);
        let tail: f32 = buf[2000..4000].iter().map(|s| s.abs()).sum();
        assert!(tail > 0.0, "reverb leaves a tail after the impulse");

        let mut dry = vec![0.0f32; 100];
        dry[0] = 1.0;
        let copy = dry.clone();
        apply_reverb(&mut dry, 0.8, 0.4, 0.0, rate);
        assert_eq!(dry, copy, "wet=0 reverb is a no-op");
    }

    /// A 3-band EQ with a non-zero band changes the signal; an all-flat EQ is a
    /// no-op.
    #[test]
    fn eq3_changes_signal_only_when_boosted() {
        use crate::app_state::TrackEq3;
        let rate = 48_000u32;
        let signal: Vec<f32> = (0..2000).map(|i| (i as f32 * 0.05).sin() * 0.5).collect();

        let mut flat = signal.clone();
        apply_eq3(&mut flat, &TrackEq3::default(), rate);
        assert_eq!(flat, signal, "0 dB on every band is a no-op");

        let mut boosted = signal.clone();
        apply_eq3(
            &mut boosted,
            &TrackEq3 { low_gain_db: 6.0, mid_gain_db: -4.0, high_gain_db: 3.0, mid_freq: 1000.0 },
            rate,
        );
        assert!(
            boosted.iter().zip(&signal).any(|(a, b)| (a - b).abs() > 1e-4),
            "a boosted EQ changes the signal"
        );
    }

    /// The audio-effect chain runs in the mix path: a delay on a clip's track
    /// adds energy past the clip's dry tail.
    #[test]
    fn mix_applies_track_audio_effect_chain() {
        use crate::app_state::AudioEffect;
        let plan = video_frame_plan(0.0, 1.0, 24.0).expect("plan");
        // A 0.5 s clip that is a short tone burst then silence; the delay echo of
        // the burst reappears 100 ms later, still inside the clip's own buffer.
        let frames = EXPORT_SAMPLE_RATE as usize / 2; // 0.5 s
        let burst = EXPORT_SAMPLE_RATE as usize / 100; // first 10 ms is the tone
        let samples: Vec<f32> = (0..frames).map(|i| if i < burst { 0.5 } else { 0.0 }).collect();
        let dry = render_program_audio(
            &[MixClip {
                start: 0.0, duration: 0.5, source_in: 0.0, gain: 1.0,
                samples: std::sync::Arc::new(samples.clone()),
                eq: None, comp: None, fx: Vec::new(), track_type: AudioTrackType::Stereo,
            }],
            plan, 24.0,
        );
        let wet = render_program_audio(
            &[MixClip {
                start: 0.0, duration: 0.5, source_in: 0.0, gain: 1.0,
                samples: std::sync::Arc::new(samples),
                eq: None, comp: None,
                fx: vec![AudioEffect::Delay { time_ms: 100.0, feedback: 0.5, wet: 0.6 }],
                track_type: AudioTrackType::Stereo,
            }],
            plan, 24.0,
        );
        // Just after 100 ms (the echo of the burst start) the dry signal is silent
        // but the wet (delayed) echo is not.
        let echo = (0.102 * EXPORT_SAMPLE_RATE as f32) as usize * EXPORT_CHANNELS as usize;
        assert!(dry.samples[echo].abs() < 1e-4, "dry signal silent past the burst");
        assert!(
            wet.samples[echo].abs() > 0.01,
            "the track delay echoes the burst (wet={})",
            wet.samples[echo]
        );
    }

    /// `build_full_export` snapshots the whole sequence into a plan covering
    /// `[0, duration)` and a silent (no audio clips) default project mix.
    #[test]
    fn build_full_export_covers_whole_sequence() {
        let project = Project::new();
        let spec = build_full_export(&project, Path::new("/tmp/x"), &[]).expect("plan");
        assert_eq!(spec.path, PathBuf::from("/tmp/x.mp4"));
        assert_eq!(spec.plan.first, 0);
        // duration 30s at 30fps → 900 frames.
        assert_eq!(spec.plan.count, (project.duration * project.fps) as u64);
        assert!(!spec.has_audio, "the default project has no audio clips");
    }

    /// `JobStatus::fraction` is clamped and labels read sensibly.
    #[test]
    fn job_status_fraction_and_label() {
        assert_eq!(JobStatus::Rendering { done: 0, total: 0 }.fraction(), 0.0);
        assert_eq!(JobStatus::Rendering { done: 9, total: 4 }.fraction(), 1.0);
        assert_eq!(JobStatus::Done { frames: 3 }.fraction(), 1.0);
        assert!(JobStatus::Done { frames: 3 }.is_terminal());
        assert!(JobStatus::Failed { message: "x".into() }.is_terminal());
        assert!(!JobStatus::Rendering { done: 1, total: 2 }.is_terminal());
        assert!(JobStatus::Failed { message: "ffmpeg".into() }.label().contains("ffmpeg"));
    }

    /// End-to-end render→encode→probe of a tiny export, gated on ffmpeg presence
    /// (skips with a printed note otherwise, like the suite's decode/GPU skips).
    #[test]
    fn run_job_encodes_when_ffmpeg_present() {
        if !prism_media::ffmpeg_available() {
            eprintln!("ffmpeg not available — skipping reel-gpui run_job test");
            return;
        }
        let mut project = Project::new();
        project.width = 48;
        project.height = 32;
        project.fps = 10.0;
        project.duration = 0.5;
        let plan = video_frame_plan(0.0, project.duration, project.fps).expect("plan");
        let mut out = std::env::temp_dir();
        out.push(format!("reel_gpui_export_{}.mp4", std::process::id()));
        let spec = JobSpec {
            project,
            plan,
            audio: prism_media::AudioMix::new(Vec::new(), EXPORT_SAMPLE_RATE, EXPORT_CHANNELS),
            has_audio: false,
            path: out.clone(),
            captions: Vec::new(),
        };
        let (tx, rx) = std::sync::mpsc::channel();
        let status = run_job(&spec, &tx);
        assert!(status.is_terminal());
        assert!(matches!(status, JobStatus::Done { .. }), "encode succeeded: {status:?}");
        let info = prism_media::probe(&out).expect("probe encoded mp4");
        assert_eq!(info.width, 48);
        assert_eq!(info.height, 32);
        // The worker streamed at least the final update.
        let updates: Vec<_> = rx.try_iter().collect();
        assert!(!updates.is_empty(), "progress streamed");
        assert!(updates.last().unwrap().status.is_terminal());
        let _ = std::fs::remove_file(&out);
    }
}
