//! The export **audio mix** pipeline split out of `export/mod.rs` (file-size
//! rule): the owned [`MixClip`] model, the decode-to-mix builder
//! ([`build_mix_clips`]), the per-clip DSP processors (parametric EQ, peak
//! compressor, 3-band EQ, shelves, Schroeder reverb, feedback delay), and the
//! summing mixdown ([`render_program_audio`]). Pure DSP (no UI state); the
//! public items are re-exported from the parent module so callers keep using
//! `crate::export::X`.

use crate::app_state::{
    AudioEffect, AudioTrackType, ClipSource, EqBand, Project, TrackCompressor, TrackEq, TrackEq3,
};

use super::FramePlan;

/// The sample rate of the export audio mix (48 kHz).
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app_state::AudioTrackType;
    use crate::export::video_frame_plan;

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
}
