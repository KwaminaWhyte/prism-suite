use super::{App, Action};

/// An audio track that can be attached to the composition for lip-sync or
/// general soundtrack purposes.
#[derive(Clone, Debug)]
pub struct DriftAudioTrack {
    pub id: usize,
    pub name: String,
    /// Path to the audio file on disk, if loaded.
    pub path: Option<String>,
    /// Number of frames to shift the audio relative to frame 0.
    pub offset_frames: i32,
    /// Playback volume, 0.0 = silent, 1.0 = full.
    pub volume: f32,
    pub muted: bool,
    pub waveform_visible: bool,
}

/// A single phoneme detected at a frame.
#[derive(Clone, Debug, PartialEq)]
pub struct PhonemeFrame {
    pub frame: u32,
    pub phoneme: Phoneme,
    /// Confidence of the detection, 0.0..=1.0.
    pub confidence: f32,
}

/// Mouth shape groups used for viseme mapping (Preston Blair scheme + silence).
#[derive(Clone, Debug, PartialEq)]
pub enum Phoneme {
    Silence,
    Ai,  // a, i
    E,   // e
    O,   // o
    U,   // u
    MBP, // m, b, p
    FV,  // f, v
    LN,  // l, n, r
    WQ,  // w, q
    Etc, // other consonants
}

/// The full lip-sync mapping derived from one audio track onto one rig.
#[derive(Clone, Debug)]
pub struct LipSyncData {
    pub audio_track_id: usize,
    /// Which rig (layer) is driven by this data.
    pub rig_id: usize,
    pub phoneme_frames: Vec<PhonemeFrame>,
}

impl App {
    pub fn apply_audio_sync(&mut self, action: Action) {
        match action {
            Action::AddAudioTrack { name } => {
                let id = self.next_audio_track_id;
                self.next_audio_track_id += 1;
                self.audio_tracks.push(DriftAudioTrack {
                    id,
                    name,
                    path: None,
                    offset_frames: 0,
                    volume: 1.0,
                    muted: false,
                    waveform_visible: true,
                });
            }
            Action::RemoveAudioTrack { id } => {
                self.audio_tracks.retain(|t| t.id != id);
                self.lip_sync_data.retain(|ls| ls.audio_track_id != id);
            }
            Action::SetAudioTrackPath { id, path } => {
                if let Some(t) = self.audio_tracks.iter_mut().find(|t| t.id == id) {
                    t.path = Some(path);
                }
            }
            Action::SetAudioTrackOffset { id, frames } => {
                if let Some(t) = self.audio_tracks.iter_mut().find(|t| t.id == id) {
                    t.offset_frames = frames;
                }
            }
            Action::SetAudioTrackVolume { id, volume } => {
                if let Some(t) = self.audio_tracks.iter_mut().find(|t| t.id == id) {
                    t.volume = volume.clamp(0.0, 1.0);
                }
            }
            Action::MuteAudioTrack { id, muted } => {
                if let Some(t) = self.audio_tracks.iter_mut().find(|t| t.id == id) {
                    t.muted = muted;
                }
            }
            Action::ToggleWaveformVisible { id } => {
                if let Some(t) = self.audio_tracks.iter_mut().find(|t| t.id == id) {
                    t.waveform_visible = !t.waveform_visible;
                }
            }
            Action::SetLipSyncData { audio_track_id, rig_id, phonemes } => {
                // Replace any existing lip-sync for this (track, rig) pair.
                self.lip_sync_data
                    .retain(|ls| !(ls.audio_track_id == audio_track_id && ls.rig_id == rig_id));
                self.lip_sync_data.push(LipSyncData {
                    audio_track_id,
                    rig_id,
                    phoneme_frames: phonemes,
                });
            }
            Action::ClearLipSyncData { audio_track_id } => {
                self.lip_sync_data
                    .retain(|ls| ls.audio_track_id != audio_track_id);
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::{App, Action};
    use super::{Phoneme, PhonemeFrame};

    fn app() -> App {
        App::new()
    }

    #[test]
    fn test_add_audio_track() {
        let mut a = app();
        a.apply(Action::AddAudioTrack { name: "VO".to_string() });
        assert_eq!(a.audio_tracks.len(), 1);
        assert_eq!(a.audio_tracks[0].name, "VO");
        assert_eq!(a.audio_tracks[0].volume, 1.0);
        assert!(!a.audio_tracks[0].muted);
    }

    #[test]
    fn test_add_multiple_audio_tracks() {
        let mut a = app();
        a.apply(Action::AddAudioTrack { name: "VO".to_string() });
        a.apply(Action::AddAudioTrack { name: "Music".to_string() });
        assert_eq!(a.audio_tracks.len(), 2);
        assert_ne!(a.audio_tracks[0].id, a.audio_tracks[1].id);
    }

    #[test]
    fn test_remove_audio_track() {
        let mut a = app();
        a.apply(Action::AddAudioTrack { name: "VO".to_string() });
        let id = a.audio_tracks[0].id;
        a.apply(Action::RemoveAudioTrack { id });
        assert!(a.audio_tracks.is_empty());
    }

    #[test]
    fn test_remove_audio_track_clears_lip_sync() {
        let mut a = app();
        a.apply(Action::AddAudioTrack { name: "VO".to_string() });
        let tid = a.audio_tracks[0].id;
        a.apply(Action::SetLipSyncData {
            audio_track_id: tid,
            rig_id: 0,
            phonemes: vec![],
        });
        assert_eq!(a.lip_sync_data.len(), 1);
        a.apply(Action::RemoveAudioTrack { id: tid });
        assert!(a.lip_sync_data.is_empty(), "lip-sync removed with track");
    }

    #[test]
    fn test_set_audio_track_path() {
        let mut a = app();
        a.apply(Action::AddAudioTrack { name: "VO".to_string() });
        let id = a.audio_tracks[0].id;
        a.apply(Action::SetAudioTrackPath { id, path: "/audio/vo.wav".to_string() });
        assert_eq!(a.audio_tracks[0].path, Some("/audio/vo.wav".to_string()));
    }

    #[test]
    fn test_set_audio_track_offset() {
        let mut a = app();
        a.apply(Action::AddAudioTrack { name: "VO".to_string() });
        let id = a.audio_tracks[0].id;
        a.apply(Action::SetAudioTrackOffset { id, frames: -5 });
        assert_eq!(a.audio_tracks[0].offset_frames, -5);
    }

    #[test]
    fn test_set_audio_track_volume_clamp() {
        let mut a = app();
        a.apply(Action::AddAudioTrack { name: "VO".to_string() });
        let id = a.audio_tracks[0].id;
        a.apply(Action::SetAudioTrackVolume { id, volume: 2.5 });
        assert_eq!(a.audio_tracks[0].volume, 1.0, "clamped to 1.0");
        a.apply(Action::SetAudioTrackVolume { id, volume: -1.0 });
        assert_eq!(a.audio_tracks[0].volume, 0.0, "clamped to 0.0");
    }

    #[test]
    fn test_mute_audio_track() {
        let mut a = app();
        a.apply(Action::AddAudioTrack { name: "VO".to_string() });
        let id = a.audio_tracks[0].id;
        a.apply(Action::MuteAudioTrack { id, muted: true });
        assert!(a.audio_tracks[0].muted);
        a.apply(Action::MuteAudioTrack { id, muted: false });
        assert!(!a.audio_tracks[0].muted);
    }

    #[test]
    fn test_toggle_waveform_visible() {
        let mut a = app();
        a.apply(Action::AddAudioTrack { name: "VO".to_string() });
        let id = a.audio_tracks[0].id;
        let initial = a.audio_tracks[0].waveform_visible;
        a.apply(Action::ToggleWaveformVisible { id });
        assert_eq!(a.audio_tracks[0].waveform_visible, !initial);
        a.apply(Action::ToggleWaveformVisible { id });
        assert_eq!(a.audio_tracks[0].waveform_visible, initial);
    }

    #[test]
    fn test_set_lip_sync_data() {
        let mut a = app();
        a.apply(Action::AddAudioTrack { name: "VO".to_string() });
        let tid = a.audio_tracks[0].id;
        let frames = vec![
            PhonemeFrame { frame: 0, phoneme: Phoneme::Silence, confidence: 1.0 },
            PhonemeFrame { frame: 10, phoneme: Phoneme::Ai, confidence: 0.9 },
        ];
        a.apply(Action::SetLipSyncData {
            audio_track_id: tid,
            rig_id: 1,
            phonemes: frames.clone(),
        });
        assert_eq!(a.lip_sync_data.len(), 1);
        assert_eq!(a.lip_sync_data[0].phoneme_frames.len(), 2);
    }

    #[test]
    fn test_set_lip_sync_data_replaces_existing() {
        let mut a = app();
        a.apply(Action::AddAudioTrack { name: "VO".to_string() });
        let tid = a.audio_tracks[0].id;
        a.apply(Action::SetLipSyncData { audio_track_id: tid, rig_id: 0, phonemes: vec![] });
        a.apply(Action::SetLipSyncData {
            audio_track_id: tid,
            rig_id: 0,
            phonemes: vec![PhonemeFrame { frame: 5, phoneme: Phoneme::E, confidence: 0.8 }],
        });
        assert_eq!(a.lip_sync_data.len(), 1, "replaced, not appended");
        assert_eq!(a.lip_sync_data[0].phoneme_frames.len(), 1);
    }

    #[test]
    fn test_clear_lip_sync_data() {
        let mut a = app();
        a.apply(Action::AddAudioTrack { name: "VO".to_string() });
        let tid = a.audio_tracks[0].id;
        a.apply(Action::SetLipSyncData { audio_track_id: tid, rig_id: 0, phonemes: vec![] });
        a.apply(Action::ClearLipSyncData { audio_track_id: tid });
        assert!(a.lip_sync_data.is_empty());
    }
}
