//! Audio waveform peaks for the timeline lane.
//!
//! An audio clip ([`crate::app_state::ClipSource::Audio`]) shows a min/max
//! waveform in its timeline lane. Decoding the whole file to PCM every redraw
//! would be ruinous, so this module decodes each audio file **once** (via
//! `prism_media::decode_audio`, the ffmpeg CLI bridge — read-only), mixes it to
//! mono, and bins it into a fixed-resolution min/max **peak envelope** keyed by
//! path. The timeline panel then samples that cached envelope for the clip's
//! visible source window and paints it with a GPUI [`gpui::Path`].
//!
//! This mirrors the egui app's `project::audio::waveform_peaks` (mono mixdown,
//! per-column min/max) but pre-bins at decode time so per-frame draw is a cheap
//! envelope lookup, not a re-scan of the raw samples.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Peak-envelope resolution: bins per second of source audio. At 400 bins/sec a
/// 10-minute file is ~240k peaks (~2 MB), and a timeline lane never needs more
/// detail than this for a min/max stroke. The draw step downsamples further to
/// the lane's pixel width.
const BINS_PER_SEC: f32 = 400.0;

/// The mono sample rate the file is decoded at for peak binning. Lower than the
/// source so a long file decodes fast; min/max peaks are insensitive to it.
const PEAK_SAMPLE_RATE: u32 = 16_000;

/// One min/max peak: the extremes of the mono signal in a single time bin, in
/// `[-1, 1]`.
#[derive(Clone, Copy, Debug)]
pub struct Peak {
    pub min: f32,
    pub max: f32,
}

/// A decoded file's full min/max peak envelope: `bins` peaks spanning
/// `duration` seconds at [`BINS_PER_SEC`] resolution. An empty `peaks` marks a
/// decode that failed (missing ffmpeg / bad media) so it isn't retried.
struct Envelope {
    peaks: Vec<Peak>,
    duration: f32,
}

/// Per-path peak-envelope cache. Decodes each audio file once; subsequent
/// redraws read the binned envelope. Owned by the GPUI root view alongside the
/// program-frame cache.
#[derive(Default)]
pub struct WaveformCache {
    map: HashMap<PathBuf, Envelope>,
}

impl WaveformCache {
    /// A fresh, empty cache.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sample `columns` min/max peaks for the source window `[start_t, end_t)`
    /// (seconds into the file) of the audio at `path`, decoding+binning the file
    /// on first use. Returns an empty `Vec` when the decode failed or the window
    /// is degenerate. Each output column folds the envelope bins it spans, so a
    /// zoomed-out clip still shows a representative min/max stroke.
    pub fn peaks_for(
        &mut self,
        path: &Path,
        start_t: f32,
        end_t: f32,
        columns: usize,
    ) -> Vec<Peak> {
        if columns == 0 {
            return Vec::new();
        }
        let env = self.envelope(path);
        if env.peaks.is_empty() || env.duration <= 0.0 {
            return Vec::new();
        }
        let start_t = start_t.max(0.0);
        let end_t = end_t.max(start_t);
        let n = env.peaks.len();
        let frac_to_bin = |t: f32| -> f32 { (t / env.duration) * n as f32 };

        let mut out = Vec::with_capacity(columns);
        for col in 0..columns {
            // The source-time slice this output column covers, folded half-open
            // [b0, b1) so adjacent columns don't double-count the boundary bin.
            let f0 = col as f32 / columns as f32;
            let f1 = (col + 1) as f32 / columns as f32;
            let t0 = start_t + (end_t - start_t) * f0;
            let t1 = start_t + (end_t - start_t) * f1;
            let b0 = (frac_to_bin(t0).floor().max(0.0) as usize).min(n);
            let mut b1 = (frac_to_bin(t1).ceil() as usize).min(n);
            if b1 <= b0 {
                b1 = (b0 + 1).min(n);
            }
            let mut lo = f32::INFINITY;
            let mut hi = f32::NEG_INFINITY;
            for b in b0..b1 {
                let p = env.peaks[b];
                lo = lo.min(p.min);
                hi = hi.max(p.max);
            }
            if !lo.is_finite() || !hi.is_finite() {
                lo = 0.0;
                hi = 0.0;
            }
            out.push(Peak { min: lo, max: hi });
        }
        out
    }

    /// The cached (or freshly decoded+binned) envelope for `path`. A decode
    /// failure caches an empty envelope so it isn't retried every frame.
    fn envelope(&mut self, path: &Path) -> &Envelope {
        if !self.map.contains_key(path) {
            let env = decode_envelope(path);
            self.map.insert(path.to_path_buf(), env);
        }
        &self.map[path]
    }
}

/// Decode `path` to mono PCM and bin it into a min/max peak envelope at
/// [`BINS_PER_SEC`]. On a decode failure returns an empty envelope.
fn decode_envelope(path: &Path) -> Envelope {
    let buf = match prism_media::decode_audio(path, PEAK_SAMPLE_RATE, 1) {
        Ok(buf) => buf,
        Err(e) => {
            log::warn!(
                "reel-gpui: audio decode failed for {} ({e}); no waveform",
                path.display()
            );
            return Envelope {
                peaks: Vec::new(),
                duration: 0.0,
            };
        }
    };
    let rate = buf.sample_rate.max(1) as f32;
    let frames = buf.samples.len(); // mono → 1 sample per frame
    if frames == 0 {
        return Envelope {
            peaks: Vec::new(),
            duration: 0.0,
        };
    }
    let duration = frames as f32 / rate;
    let bins = ((duration * BINS_PER_SEC).ceil() as usize).max(1);
    let per_bin = (frames as f32 / bins as f32).max(1.0);

    let mut peaks = Vec::with_capacity(bins);
    for b in 0..bins {
        let s0 = (b as f32 * per_bin).floor() as usize;
        let s1 = (((b + 1) as f32 * per_bin).floor() as usize).min(frames);
        let mut lo = 0.0f32;
        let mut hi = 0.0f32;
        for &s in &buf.samples[s0..s1.max(s0)] {
            lo = lo.min(s);
            hi = hi.max(s);
        }
        peaks.push(Peak { min: lo, max: hi });
    }

    Envelope { peaks, duration }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a cache directly from a synthetic envelope (no ffmpeg) to exercise
    /// the column-fold sampler deterministically.
    fn seeded(path: &str, peaks: Vec<Peak>, duration: f32) -> WaveformCache {
        let mut c = WaveformCache::new();
        c.map.insert(PathBuf::from(path), Envelope { peaks, duration });
        c
    }

    #[test]
    fn peaks_fold_bins_into_columns() {
        // 4 bins over 4s: a ramp of |amplitude|.
        let env = vec![
            Peak { min: -0.1, max: 0.1 },
            Peak { min: -0.2, max: 0.3 },
            Peak { min: -0.5, max: 0.4 },
            Peak { min: -0.9, max: 0.8 },
        ];
        let mut cache = seeded("/a.wav", env, 4.0);
        // Two columns over the whole window fold bins [0,1] and [2,3].
        let out = cache.peaks_for(Path::new("/a.wav"), 0.0, 4.0, 2);
        assert_eq!(out.len(), 2);
        assert!((out[0].max - 0.3).abs() < 1e-6);
        assert!((out[0].min - (-0.2)).abs() < 1e-6);
        assert!((out[1].max - 0.8).abs() < 1e-6);
        assert!((out[1].min - (-0.9)).abs() < 1e-6);
    }

    #[test]
    fn peaks_window_is_sub_ranged() {
        let env = vec![
            Peak { min: -0.1, max: 0.1 },
            Peak { min: -0.2, max: 0.9 },
            Peak { min: -0.7, max: 0.3 },
            Peak { min: -0.4, max: 0.2 },
        ];
        let mut cache = seeded("/b.wav", env, 4.0);
        // Window [1s,3s) covers bins 1 and 2 only.
        let out = cache.peaks_for(Path::new("/b.wav"), 1.0, 3.0, 1);
        assert_eq!(out.len(), 1);
        assert!((out[0].max - 0.9).abs() < 1e-6);
        assert!((out[0].min - (-0.7)).abs() < 1e-6);
    }

    #[test]
    fn missing_or_failed_decode_yields_no_peaks() {
        let mut cache = seeded("/empty.wav", Vec::new(), 0.0);
        assert!(cache
            .peaks_for(Path::new("/empty.wav"), 0.0, 1.0, 8)
            .is_empty());
        // Zero columns is always empty.
        let mut c2 = seeded("/x.wav", vec![Peak { min: 0.0, max: 1.0 }], 1.0);
        assert!(c2.peaks_for(Path::new("/x.wav"), 0.0, 1.0, 0).is_empty());
    }
}
