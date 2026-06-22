use super::{App, Action};

/// Kind of audio marker.
#[derive(Clone, Debug, PartialEq)]
pub enum MarkerKind {
    Beat,
    DownBeat,
    Cue,
    Chapter,
}

/// A time-based marker on the audio track.
#[derive(Clone, Debug)]
pub struct AudioMarker {
    pub id: usize,
    pub time_secs: f32,
    pub label: String,
    pub kind: MarkerKind,
    pub color: String,
}

/// Configuration for beat-sync snapping.
#[derive(Clone, Debug)]
pub struct BeatSyncConfig {
    pub audio_track_id: Option<usize>,
    pub bpm: f32,
    /// When beat 1 starts, in seconds.
    pub offset_secs: f32,
    pub enabled: bool,
    /// Snap keyframes to beats.
    pub sync_snapping: bool,
    pub time_sig_num: u32,
    pub time_sig_den: u32,
    /// BPM detected by AI beat detection; `None` until analyzed.
    pub detected_bpm: Option<f32>,
}

impl BeatSyncConfig {
    pub fn new() -> Self {
        Self {
            audio_track_id: None,
            bpm: 120.0,
            offset_secs: 0.0,
            enabled: false,
            sync_snapping: false,
            time_sig_num: 4,
            time_sig_den: 4,
            detected_bpm: None,
        }
    }
}

/// A group of layers that animate together in sync.
#[derive(Clone, Debug)]
pub struct SyncGroup {
    pub id: usize,
    pub name: String,
    pub layer_ids: Vec<usize>,
    pub sync_to_beat: bool,
    pub sync_to_markers: bool,
}

impl App {
    pub fn apply_beat_sync(&mut self, action: Action) {
        match action {
            Action::AddAudioMarker { time_secs, label, kind } => {
                let id = self.next_marker_id;
                self.next_marker_id += 1;
                self.audio_markers.push(AudioMarker {
                    id,
                    time_secs,
                    label,
                    kind,
                    color: "#FFFFFF".to_string(),
                });
            }
            Action::RemoveAudioMarker { marker_id } => {
                self.audio_markers.retain(|m| m.id != marker_id);
            }
            Action::MoveAudioMarker { marker_id, time_secs } => {
                if let Some(m) = self.audio_markers.iter_mut().find(|m| m.id == marker_id) {
                    m.time_secs = time_secs;
                }
            }
            Action::SetMarkerLabel { marker_id, label } => {
                if let Some(m) = self.audio_markers.iter_mut().find(|m| m.id == marker_id) {
                    m.label = label;
                }
            }
            Action::SetBeatSyncEnabled(enabled) => {
                self.beat_sync.enabled = enabled;
            }
            Action::SetBeatSyncBpm(bpm) => {
                self.beat_sync.bpm = bpm.max(1.0);
            }
            Action::SetBeatSyncOffset(offset) => {
                self.beat_sync.offset_secs = offset;
            }
            Action::SetBeatSyncAudioTrack { track_id } => {
                self.beat_sync.audio_track_id = track_id;
            }
            Action::SetDetectedBpm(bpm) => {
                self.beat_sync.detected_bpm = bpm;
            }
            Action::ToggleSyncSnapping => {
                self.beat_sync.sync_snapping = !self.beat_sync.sync_snapping;
            }
            Action::AddSyncGroup { name, layer_ids } => {
                let id = self.next_sync_group_id;
                self.next_sync_group_id += 1;
                self.sync_groups.push(SyncGroup {
                    id,
                    name,
                    layer_ids,
                    sync_to_beat: true,
                    sync_to_markers: false,
                });
            }
            Action::RemoveSyncGroup { group_id } => {
                self.sync_groups.retain(|g| g.id != group_id);
            }
            Action::SetSyncGroupLayers { group_id, layer_ids } => {
                if let Some(g) = self.sync_groups.iter_mut().find(|g| g.id == group_id) {
                    g.layer_ids = layer_ids;
                }
            }
            _ => {}
        }
    }

    /// Returns markers sorted by ascending time.
    pub fn markers_sorted(&self) -> Vec<&AudioMarker> {
        let mut v: Vec<&AudioMarker> = self.audio_markers.iter().collect();
        v.sort_by(|a, b| a.time_secs.partial_cmp(&b.time_secs).unwrap_or(std::cmp::Ordering::Equal));
        v
    }

    /// Compute the time in seconds for beat `n` (1-indexed) given current BPM + offset.
    pub fn beat_time_secs(&self, beat: u32) -> f32 {
        self.beat_sync.offset_secs + (beat.saturating_sub(1) as f32 * 60.0 / self.beat_sync.bpm)
    }
}

#[cfg(test)]
mod tests {
    use super::super::{App, Action};
    use super::MarkerKind;

    fn app() -> App {
        App::new()
    }

    #[test]
    fn test_add_audio_marker() {
        let mut a = app();
        a.apply(Action::AddAudioMarker { time_secs: 1.0, label: "Drop".to_string(), kind: MarkerKind::DownBeat });
        assert_eq!(a.audio_markers.len(), 1);
        assert_eq!(a.audio_markers[0].label, "Drop");
    }

    #[test]
    fn test_add_multiple_markers_unique_ids() {
        let mut a = app();
        a.apply(Action::AddAudioMarker { time_secs: 0.5, label: "A".to_string(), kind: MarkerKind::Beat });
        a.apply(Action::AddAudioMarker { time_secs: 1.0, label: "B".to_string(), kind: MarkerKind::Cue });
        assert_ne!(a.audio_markers[0].id, a.audio_markers[1].id);
    }

    #[test]
    fn test_remove_audio_marker() {
        let mut a = app();
        a.apply(Action::AddAudioMarker { time_secs: 2.0, label: "X".to_string(), kind: MarkerKind::Beat });
        let id = a.audio_markers[0].id;
        a.apply(Action::RemoveAudioMarker { marker_id: id });
        assert!(a.audio_markers.is_empty());
    }

    #[test]
    fn test_move_audio_marker() {
        let mut a = app();
        a.apply(Action::AddAudioMarker { time_secs: 1.0, label: "M".to_string(), kind: MarkerKind::Beat });
        let id = a.audio_markers[0].id;
        a.apply(Action::MoveAudioMarker { marker_id: id, time_secs: 3.5 });
        assert_eq!(a.audio_markers[0].time_secs, 3.5);
    }

    #[test]
    fn test_set_marker_label() {
        let mut a = app();
        a.apply(Action::AddAudioMarker { time_secs: 0.0, label: "Old".to_string(), kind: MarkerKind::Cue });
        let id = a.audio_markers[0].id;
        a.apply(Action::SetMarkerLabel { marker_id: id, label: "New".to_string() });
        assert_eq!(a.audio_markers[0].label, "New");
    }

    #[test]
    fn test_set_beat_sync_enabled() {
        let mut a = app();
        assert!(!a.beat_sync.enabled);
        a.apply(Action::SetBeatSyncEnabled(true));
        assert!(a.beat_sync.enabled);
        a.apply(Action::SetBeatSyncEnabled(false));
        assert!(!a.beat_sync.enabled);
    }

    #[test]
    fn test_set_beat_sync_bpm() {
        let mut a = app();
        a.apply(Action::SetBeatSyncBpm(140.0));
        assert_eq!(a.beat_sync.bpm, 140.0);
    }

    #[test]
    fn test_set_beat_sync_bpm_clamps_minimum() {
        let mut a = app();
        a.apply(Action::SetBeatSyncBpm(0.0));
        assert_eq!(a.beat_sync.bpm, 1.0, "minimum BPM is 1");
    }

    #[test]
    fn test_set_detected_bpm() {
        let mut a = app();
        a.apply(Action::SetDetectedBpm(Some(128.0)));
        assert_eq!(a.beat_sync.detected_bpm, Some(128.0));
        a.apply(Action::SetDetectedBpm(None));
        assert!(a.beat_sync.detected_bpm.is_none());
    }

    #[test]
    fn test_toggle_sync_snapping() {
        let mut a = app();
        assert!(!a.beat_sync.sync_snapping);
        a.apply(Action::ToggleSyncSnapping);
        assert!(a.beat_sync.sync_snapping);
        a.apply(Action::ToggleSyncSnapping);
        assert!(!a.beat_sync.sync_snapping);
    }

    #[test]
    fn test_add_sync_group() {
        let mut a = app();
        a.apply(Action::AddSyncGroup { name: "Chorus".to_string(), layer_ids: vec![0, 1, 2] });
        assert_eq!(a.sync_groups.len(), 1);
        assert_eq!(a.sync_groups[0].name, "Chorus");
        assert_eq!(a.sync_groups[0].layer_ids, vec![0, 1, 2]);
        assert!(a.sync_groups[0].sync_to_beat);
    }

    #[test]
    fn test_remove_sync_group() {
        let mut a = app();
        a.apply(Action::AddSyncGroup { name: "G".to_string(), layer_ids: vec![] });
        let id = a.sync_groups[0].id;
        a.apply(Action::RemoveSyncGroup { group_id: id });
        assert!(a.sync_groups.is_empty());
    }

    #[test]
    fn test_set_sync_group_layers() {
        let mut a = app();
        a.apply(Action::AddSyncGroup { name: "G".to_string(), layer_ids: vec![0] });
        let id = a.sync_groups[0].id;
        a.apply(Action::SetSyncGroupLayers { group_id: id, layer_ids: vec![1, 2, 3] });
        assert_eq!(a.sync_groups[0].layer_ids, vec![1, 2, 3]);
    }

    #[test]
    fn test_beat_time_secs() {
        let mut a = app();
        a.apply(Action::SetBeatSyncBpm(120.0));
        // At 120 BPM, beat 1 = 0.0s, beat 2 = 0.5s, beat 3 = 1.0s
        assert!((a.beat_time_secs(1) - 0.0).abs() < 1e-5);
        assert!((a.beat_time_secs(2) - 0.5).abs() < 1e-5);
        assert!((a.beat_time_secs(3) - 1.0).abs() < 1e-5);
    }

    #[test]
    fn test_markers_sorted() {
        let mut a = app();
        a.apply(Action::AddAudioMarker { time_secs: 3.0, label: "C".to_string(), kind: MarkerKind::Beat });
        a.apply(Action::AddAudioMarker { time_secs: 1.0, label: "A".to_string(), kind: MarkerKind::Beat });
        a.apply(Action::AddAudioMarker { time_secs: 2.0, label: "B".to_string(), kind: MarkerKind::Beat });
        let sorted = a.markers_sorted();
        assert_eq!(sorted[0].label, "A");
        assert_eq!(sorted[1].label, "B");
        assert_eq!(sorted[2].label, "C");
    }
}
