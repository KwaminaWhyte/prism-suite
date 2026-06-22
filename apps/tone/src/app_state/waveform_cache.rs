//! Waveform peak cache for Tone.
//!
//! Each audio clip has an entry in the cache: a vector of min/max peak pairs
//! at a given resolution (pixels-per-second). The cache entry is marked dirty
//! whenever audio content changes so the UI knows to request a recompute.

use super::{App, Action};

// ─── Types ────────────────────────────────────────────────────────────────────

/// One min/max peak sample for waveform display.
#[derive(Clone, Debug)]
pub struct WaveformPeak {
    pub min: f32,
    pub max: f32,
}

/// Cached waveform data for a single clip.
#[derive(Clone, Debug)]
pub struct WaveformChunk {
    pub clip_id: usize,
    pub peaks: Vec<WaveformPeak>,
    pub sample_rate: u32,
    pub pixels_per_second: f32,
    /// When true the peaks need to be recomputed before display.
    pub dirty: bool,
}

// ─── App impl ─────────────────────────────────────────────────────────────────

impl App {
    pub(super) fn apply_waveform_cache(&mut self, action: &Action) {
        match action {
            Action::InvalidateWaveform { clip_id } => {
                if let Some(c) = self.waveform_cache.iter_mut().find(|c| c.clip_id == *clip_id) {
                    c.dirty = true;
                } else {
                    self.waveform_cache.push(WaveformChunk {
                        clip_id: *clip_id,
                        peaks: vec![],
                        sample_rate: 44100,
                        pixels_per_second: 100.0,
                        dirty: true,
                    });
                }
            }
            Action::SetWaveformPeaks { clip_id, peaks, sample_rate, pixels_per_second } => {
                if let Some(c) = self.waveform_cache.iter_mut().find(|c| c.clip_id == *clip_id) {
                    c.peaks = peaks.clone();
                    c.sample_rate = *sample_rate;
                    c.pixels_per_second = *pixels_per_second;
                    c.dirty = false;
                } else {
                    self.waveform_cache.push(WaveformChunk {
                        clip_id: *clip_id,
                        peaks: peaks.clone(),
                        sample_rate: *sample_rate,
                        pixels_per_second: *pixels_per_second,
                        dirty: false,
                    });
                }
            }
            Action::ClearWaveformCache => {
                self.waveform_cache.clear();
            }
            _ => {}
        }
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn fresh() -> App {
        App::new()
    }

    fn peaks(n: usize) -> Vec<WaveformPeak> {
        (0..n).map(|i| WaveformPeak { min: -(i as f32 / 100.0), max: i as f32 / 100.0 }).collect()
    }

    #[test]
    fn invalidate_waveform_creates_dirty_entry() {
        let mut app = fresh();
        app.apply(Action::InvalidateWaveform { clip_id: 7 });
        assert_eq!(app.waveform_cache.len(), 1);
        assert!(app.waveform_cache[0].dirty);
        assert_eq!(app.waveform_cache[0].clip_id, 7);
    }

    #[test]
    fn invalidate_existing_entry_marks_dirty() {
        let mut app = fresh();
        // First set clean peaks
        app.apply(Action::SetWaveformPeaks {
            clip_id: 3,
            peaks: peaks(10),
            sample_rate: 44100,
            pixels_per_second: 100.0,
        });
        assert!(!app.waveform_cache[0].dirty);
        // Then invalidate
        app.apply(Action::InvalidateWaveform { clip_id: 3 });
        assert!(app.waveform_cache[0].dirty);
    }

    #[test]
    fn set_waveform_peaks_creates_clean_entry() {
        let mut app = fresh();
        app.apply(Action::SetWaveformPeaks {
            clip_id: 5,
            peaks: peaks(20),
            sample_rate: 48000,
            pixels_per_second: 200.0,
        });
        assert_eq!(app.waveform_cache.len(), 1);
        let chunk = &app.waveform_cache[0];
        assert!(!chunk.dirty);
        assert_eq!(chunk.peaks.len(), 20);
        assert_eq!(chunk.sample_rate, 48000);
        assert!((chunk.pixels_per_second - 200.0).abs() < 0.001);
    }

    #[test]
    fn set_waveform_peaks_updates_existing_entry() {
        let mut app = fresh();
        app.apply(Action::SetWaveformPeaks {
            clip_id: 1,
            peaks: peaks(5),
            sample_rate: 44100,
            pixels_per_second: 100.0,
        });
        app.apply(Action::SetWaveformPeaks {
            clip_id: 1,
            peaks: peaks(50),
            sample_rate: 96000,
            pixels_per_second: 300.0,
        });
        assert_eq!(app.waveform_cache.len(), 1);
        let chunk = &app.waveform_cache[0];
        assert_eq!(chunk.peaks.len(), 50);
        assert_eq!(chunk.sample_rate, 96000);
    }

    #[test]
    fn clear_waveform_cache() {
        let mut app = fresh();
        app.apply(Action::SetWaveformPeaks {
            clip_id: 0,
            peaks: peaks(10),
            sample_rate: 44100,
            pixels_per_second: 100.0,
        });
        app.apply(Action::SetWaveformPeaks {
            clip_id: 1,
            peaks: peaks(10),
            sample_rate: 44100,
            pixels_per_second: 100.0,
        });
        assert_eq!(app.waveform_cache.len(), 2);
        app.apply(Action::ClearWaveformCache);
        assert!(app.waveform_cache.is_empty());
    }

    #[test]
    fn multiple_clips_have_separate_entries() {
        let mut app = fresh();
        for clip_id in 0..4 {
            app.apply(Action::SetWaveformPeaks {
                clip_id,
                peaks: peaks(clip_id + 1),
                sample_rate: 44100,
                pixels_per_second: 100.0,
            });
        }
        assert_eq!(app.waveform_cache.len(), 4);
    }

    #[test]
    fn waveform_cache_starts_empty() {
        let app = fresh();
        assert!(app.waveform_cache.is_empty());
    }

    #[test]
    fn invalidate_then_set_clears_dirty_flag() {
        let mut app = fresh();
        app.apply(Action::InvalidateWaveform { clip_id: 2 });
        assert!(app.waveform_cache[0].dirty);
        app.apply(Action::SetWaveformPeaks {
            clip_id: 2,
            peaks: peaks(8),
            sample_rate: 44100,
            pixels_per_second: 100.0,
        });
        assert!(!app.waveform_cache[0].dirty);
    }
}
