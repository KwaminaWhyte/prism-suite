use super::{App, Action};

/// Per-audio-clip detailed ops (trim, fade, pan, waveform viz).
#[derive(Clone, Debug)]
pub struct AudioClipOps {
    pub clip_layer_id: usize,
    pub trim_start_frames: usize,
    pub trim_end_frames: usize,
    pub fade_in_frames: usize,
    pub fade_out_frames: usize,
    pub volume: f32,
    pub pan: f32,
}

/// Waveform visualization peaks per audio layer.
#[derive(Clone, Debug)]
pub struct WaveformPeakEntry {
    pub layer_id: usize,
    pub peaks: Vec<(f32, f32)>,
    pub sample_rate: u32,
    pub dirty: bool,
}

/// Multi-track mix config (aggregate mix bus for final output).
#[derive(Clone, Debug)]
pub struct MultiTrackMixConfig {
    pub enabled: bool,
    pub output_sample_rate: u32,
    pub output_channels: u8,
    pub normalize: bool,
    pub master_gain: f32,
}

impl Default for MultiTrackMixConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            output_sample_rate: 44100,
            output_channels: 2,
            normalize: true,
            master_gain: 1.0,
        }
    }
}

/// Rhai scripting runtime configuration.
#[derive(Clone, Debug)]
pub struct RhaiRuntimeConfig {
    pub enabled: bool,
    pub max_ops: u64,
    pub debug_mode: bool,
    pub sandbox_fs: bool,
}

impl Default for RhaiRuntimeConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            max_ops: 100_000,
            debug_mode: false,
            sandbox_fs: true,
        }
    }
}

/// AS3 importer job status.
#[derive(Clone, Debug, PartialEq)]
pub enum As3ImportStatus {
    Idle,
    Parsing,
    Done,
    Error,
}

/// AS3 importer job stub.
#[derive(Clone, Debug)]
pub struct As3ImportJob {
    pub id: usize,
    pub source_path: String,
    pub status: As3ImportStatus,
    pub scripts_found: usize,
    pub symbols_found: usize,
    pub error: Option<String>,
}

/// Audio sync marker (frame-based, extends beyond beat_sync time markers).
#[derive(Clone, Debug)]
pub struct AudioSyncMarker {
    pub id: usize,
    pub frame: usize,
    pub label: String,
    pub color: u32,
}

impl App {
    pub(super) fn apply_audio_ops(&mut self, action: Action) {
        match action {
            Action::SetAudioClipTrim { layer_id, trim_start, trim_end } => {
                if let Some(ops) = self.audio_clip_ops.iter_mut().find(|o| o.clip_layer_id == layer_id) {
                    ops.trim_start_frames = trim_start;
                    ops.trim_end_frames = trim_end;
                } else {
                    self.audio_clip_ops.push(AudioClipOps {
                        clip_layer_id: layer_id,
                        trim_start_frames: trim_start,
                        trim_end_frames: trim_end,
                        fade_in_frames: 0,
                        fade_out_frames: 0,
                        volume: 1.0,
                        pan: 0.0,
                    });
                }
            }
            Action::SetAudioFade { layer_id, fade_in, fade_out } => {
                if let Some(ops) = self.audio_clip_ops.iter_mut().find(|o| o.clip_layer_id == layer_id) {
                    ops.fade_in_frames = fade_in;
                    ops.fade_out_frames = fade_out;
                }
            }
            Action::SetAudioPan { layer_id, pan } => {
                if let Some(ops) = self.audio_clip_ops.iter_mut().find(|o| o.clip_layer_id == layer_id) {
                    ops.pan = pan.clamp(-1.0, 1.0);
                } else {
                    self.audio_clip_ops.push(AudioClipOps {
                        clip_layer_id: layer_id,
                        trim_start_frames: 0,
                        trim_end_frames: 0,
                        fade_in_frames: 0,
                        fade_out_frames: 0,
                        volume: 1.0,
                        pan: pan.clamp(-1.0, 1.0),
                    });
                }
            }
            Action::SetAudioClipVolume { layer_id, volume } => {
                if let Some(ops) = self.audio_clip_ops.iter_mut().find(|o| o.clip_layer_id == layer_id) {
                    ops.volume = volume.clamp(0.0, 4.0);
                } else {
                    self.audio_clip_ops.push(AudioClipOps {
                        clip_layer_id: layer_id,
                        trim_start_frames: 0,
                        trim_end_frames: 0,
                        fade_in_frames: 0,
                        fade_out_frames: 0,
                        volume: volume.clamp(0.0, 4.0),
                        pan: 0.0,
                    });
                }
            }
            Action::SetWaveformPeaks { layer_id, peaks, sample_rate } => {
                if let Some(e) = self.waveform_peaks.iter_mut().find(|e| e.layer_id == layer_id) {
                    e.peaks = peaks;
                    e.sample_rate = sample_rate;
                    e.dirty = false;
                } else {
                    self.waveform_peaks.push(WaveformPeakEntry {
                        layer_id,
                        peaks,
                        sample_rate,
                        dirty: false,
                    });
                }
            }
            Action::InvalidateWaveformPeaks { layer_id } => {
                if let Some(e) = self.waveform_peaks.iter_mut().find(|e| e.layer_id == layer_id) {
                    e.dirty = true;
                } else {
                    self.waveform_peaks.push(WaveformPeakEntry {
                        layer_id,
                        peaks: vec![],
                        sample_rate: 44100,
                        dirty: true,
                    });
                }
            }
            Action::SetMultiTrackMix { enabled, normalize, master_gain } => {
                self.mix_config.enabled = enabled;
                self.mix_config.normalize = normalize;
                self.mix_config.master_gain = master_gain.clamp(0.0, 4.0);
            }
            Action::SetRhaiRuntime { enabled, max_ops, debug } => {
                self.rhai_runtime.enabled = enabled;
                self.rhai_runtime.max_ops = max_ops;
                self.rhai_runtime.debug_mode = debug;
            }
            Action::AddAudioSyncMarker { frame, label, color } => {
                let id = self.next_sync_marker_id;
                self.next_sync_marker_id += 1;
                self.audio_sync_markers.push(AudioSyncMarker {
                    id,
                    frame,
                    label,
                    color,
                });
            }
            Action::RemoveAudioSyncMarker { id } => {
                self.audio_sync_markers.retain(|m| m.id != id);
            }
            Action::StartAs3Import { source_path } => {
                let id = self.next_as3_job_id;
                self.next_as3_job_id += 1;
                self.as3_jobs.push(As3ImportJob {
                    id,
                    source_path,
                    status: As3ImportStatus::Parsing,
                    scripts_found: 0,
                    symbols_found: 0,
                    error: None,
                });
            }
            Action::CompleteAs3Import { job_id, scripts_found, symbols_found } => {
                if let Some(j) = self.as3_jobs.iter_mut().find(|j| j.id == job_id) {
                    j.status = As3ImportStatus::Done;
                    j.scripts_found = scripts_found;
                    j.symbols_found = symbols_found;
                }
            }
            Action::FailAs3Import { job_id, error } => {
                if let Some(j) = self.as3_jobs.iter_mut().find(|j| j.id == job_id) {
                    j.status = As3ImportStatus::Error;
                    j.error = Some(error);
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::{App, Action};
    use super::As3ImportStatus;

    fn app() -> App {
        App::new()
    }

    #[test]
    fn test_set_audio_clip_trim_creates_entry() {
        let mut a = app();
        a.apply(Action::SetAudioClipTrim { layer_id: 1, trim_start: 5, trim_end: 10 });
        assert_eq!(a.audio_clip_ops.len(), 1);
        assert_eq!(a.audio_clip_ops[0].trim_start_frames, 5);
        assert_eq!(a.audio_clip_ops[0].trim_end_frames, 10);
    }

    #[test]
    fn test_set_audio_clip_trim_updates_existing() {
        let mut a = app();
        a.apply(Action::SetAudioClipTrim { layer_id: 1, trim_start: 5, trim_end: 10 });
        a.apply(Action::SetAudioClipTrim { layer_id: 1, trim_start: 2, trim_end: 8 });
        assert_eq!(a.audio_clip_ops.len(), 1);
        assert_eq!(a.audio_clip_ops[0].trim_start_frames, 2);
        assert_eq!(a.audio_clip_ops[0].trim_end_frames, 8);
    }

    #[test]
    fn test_set_audio_fade_updates_entry() {
        let mut a = app();
        a.apply(Action::SetAudioClipTrim { layer_id: 2, trim_start: 0, trim_end: 0 });
        a.apply(Action::SetAudioFade { layer_id: 2, fade_in: 3, fade_out: 6 });
        assert_eq!(a.audio_clip_ops[0].fade_in_frames, 3);
        assert_eq!(a.audio_clip_ops[0].fade_out_frames, 6);
    }

    #[test]
    fn test_set_audio_fade_no_entry_noop() {
        let mut a = app();
        // no clip entry exists — should not panic
        a.apply(Action::SetAudioFade { layer_id: 99, fade_in: 3, fade_out: 6 });
        assert!(a.audio_clip_ops.is_empty());
    }

    #[test]
    fn test_set_audio_pan_clamps() {
        let mut a = app();
        a.apply(Action::SetAudioPan { layer_id: 3, pan: 2.5 });
        assert_eq!(a.audio_clip_ops[0].pan, 1.0);
        a.apply(Action::SetAudioPan { layer_id: 3, pan: -3.0 });
        assert_eq!(a.audio_clip_ops[0].pan, -1.0);
    }

    #[test]
    fn test_set_audio_pan_creates_entry() {
        let mut a = app();
        a.apply(Action::SetAudioPan { layer_id: 5, pan: 0.5 });
        assert_eq!(a.audio_clip_ops.len(), 1);
        assert_eq!(a.audio_clip_ops[0].pan, 0.5);
    }

    #[test]
    fn test_set_audio_clip_volume_clamps() {
        let mut a = app();
        a.apply(Action::SetAudioClipVolume { layer_id: 4, volume: 5.0 });
        assert_eq!(a.audio_clip_ops[0].volume, 4.0);
        a.apply(Action::SetAudioClipVolume { layer_id: 4, volume: -0.5 });
        assert_eq!(a.audio_clip_ops[0].volume, 0.0);
    }

    #[test]
    fn test_set_waveform_peaks_creates_entry() {
        let mut a = app();
        let peaks = vec![(0.0f32, 1.0f32), (0.5, 0.8)];
        a.apply(Action::SetWaveformPeaks { layer_id: 1, peaks: peaks.clone(), sample_rate: 48000 });
        assert_eq!(a.waveform_peaks.len(), 1);
        assert_eq!(a.waveform_peaks[0].sample_rate, 48000);
        assert!(!a.waveform_peaks[0].dirty);
        assert_eq!(a.waveform_peaks[0].peaks.len(), 2);
    }

    #[test]
    fn test_set_waveform_peaks_updates_existing() {
        let mut a = app();
        a.apply(Action::SetWaveformPeaks { layer_id: 1, peaks: vec![], sample_rate: 44100 });
        let peaks2 = vec![(0.1f32, 0.9f32)];
        a.apply(Action::SetWaveformPeaks { layer_id: 1, peaks: peaks2, sample_rate: 48000 });
        assert_eq!(a.waveform_peaks.len(), 1);
        assert_eq!(a.waveform_peaks[0].peaks.len(), 1);
    }

    #[test]
    fn test_invalidate_waveform_peaks_marks_dirty() {
        let mut a = app();
        a.apply(Action::SetWaveformPeaks { layer_id: 7, peaks: vec![], sample_rate: 44100 });
        a.apply(Action::InvalidateWaveformPeaks { layer_id: 7 });
        assert!(a.waveform_peaks[0].dirty);
    }

    #[test]
    fn test_invalidate_waveform_peaks_creates_dirty_entry() {
        let mut a = app();
        a.apply(Action::InvalidateWaveformPeaks { layer_id: 9 });
        assert_eq!(a.waveform_peaks.len(), 1);
        assert!(a.waveform_peaks[0].dirty);
    }

    #[test]
    fn test_set_multi_track_mix() {
        let mut a = app();
        a.apply(Action::SetMultiTrackMix { enabled: true, normalize: false, master_gain: 2.0 });
        assert!(a.mix_config.enabled);
        assert!(!a.mix_config.normalize);
        assert_eq!(a.mix_config.master_gain, 2.0);
    }

    #[test]
    fn test_set_multi_track_mix_clamps_gain() {
        let mut a = app();
        a.apply(Action::SetMultiTrackMix { enabled: true, normalize: true, master_gain: 10.0 });
        assert_eq!(a.mix_config.master_gain, 4.0);
    }

    #[test]
    fn test_set_rhai_runtime() {
        let mut a = app();
        a.apply(Action::SetRhaiRuntime { enabled: false, max_ops: 50_000, debug: true });
        assert!(!a.rhai_runtime.enabled);
        assert_eq!(a.rhai_runtime.max_ops, 50_000);
        assert!(a.rhai_runtime.debug_mode);
    }

    #[test]
    fn test_add_audio_sync_marker() {
        let mut a = app();
        a.apply(Action::AddAudioSyncMarker { frame: 30, label: "Hit".to_string(), color: 0xFF0000 });
        assert_eq!(a.audio_sync_markers.len(), 1);
        assert_eq!(a.audio_sync_markers[0].frame, 30);
        assert_eq!(a.audio_sync_markers[0].label, "Hit");
    }

    #[test]
    fn test_add_audio_sync_marker_increments_id() {
        let mut a = app();
        a.apply(Action::AddAudioSyncMarker { frame: 10, label: "A".to_string(), color: 0x00FF00 });
        a.apply(Action::AddAudioSyncMarker { frame: 20, label: "B".to_string(), color: 0x0000FF });
        assert_ne!(a.audio_sync_markers[0].id, a.audio_sync_markers[1].id);
    }

    #[test]
    fn test_remove_audio_sync_marker() {
        let mut a = app();
        a.apply(Action::AddAudioSyncMarker { frame: 10, label: "A".to_string(), color: 0 });
        let id = a.audio_sync_markers[0].id;
        a.apply(Action::RemoveAudioSyncMarker { id });
        assert!(a.audio_sync_markers.is_empty());
    }

    #[test]
    fn test_start_as3_import() {
        let mut a = app();
        a.apply(Action::StartAs3Import { source_path: "/path/to/scene.fla".to_string() });
        assert_eq!(a.as3_jobs.len(), 1);
        assert_eq!(a.as3_jobs[0].status, As3ImportStatus::Parsing);
        assert_eq!(a.as3_jobs[0].source_path, "/path/to/scene.fla");
    }

    #[test]
    fn test_complete_as3_import() {
        let mut a = app();
        a.apply(Action::StartAs3Import { source_path: "/tmp/anim.fla".to_string() });
        let jid = a.as3_jobs[0].id;
        a.apply(Action::CompleteAs3Import { job_id: jid, scripts_found: 3, symbols_found: 10 });
        assert_eq!(a.as3_jobs[0].status, As3ImportStatus::Done);
        assert_eq!(a.as3_jobs[0].scripts_found, 3);
        assert_eq!(a.as3_jobs[0].symbols_found, 10);
    }

    #[test]
    fn test_fail_as3_import() {
        let mut a = app();
        a.apply(Action::StartAs3Import { source_path: "/tmp/bad.fla".to_string() });
        let jid = a.as3_jobs[0].id;
        a.apply(Action::FailAs3Import { job_id: jid, error: "parse error".to_string() });
        assert_eq!(a.as3_jobs[0].status, As3ImportStatus::Error);
        assert_eq!(a.as3_jobs[0].error.as_deref(), Some("parse error"));
    }
}
