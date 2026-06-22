use super::{App, Action};

#[derive(Clone, Debug, PartialEq)]
pub enum AnalysisStatus {
    Pending,
    Running { progress: f32 },
    Done,
    Failed(String),
}

#[derive(Clone, Debug)]
pub struct BeatDetectionResult {
    pub id: usize,
    pub audio_track_id: usize,
    pub detected_bpm: f32,
    pub bpm_confidence: f32,
    pub beat_positions: Vec<f32>,
    pub downbeat_positions: Vec<f32>,
    pub time_signature_guess: (u32, u32),
    pub transient_positions: Vec<f32>,
    pub status: AnalysisStatus,
}

#[derive(Clone, Debug)]
pub struct WarpMarker {
    pub id: usize,
    pub clip_id: usize,
    pub original_time_secs: f32,
    pub warp_time_beats: f32,
    pub locked: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub enum StretchAlgorithm {
    Beats,
    Tones,
    Texture,
    Repitch,
    Complex,
}

#[derive(Clone, Debug)]
pub struct StretchConfig {
    pub clip_id: usize,
    pub algorithm: StretchAlgorithm,
    pub formant_preservation: bool,
    pub transient_sensitivity: f32,
}

impl App {
    pub(crate) fn apply_beat_detection(&mut self, action: Action) {
        match action {
            Action::AnalyzeBeatDetection { track_id } => {
                let id = self.next_beat_result_id;
                self.next_beat_result_id += 1;
                self.beat_results.push(BeatDetectionResult {
                    id,
                    audio_track_id: track_id,
                    detected_bpm: 120.0,
                    bpm_confidence: 0.0,
                    beat_positions: Vec::new(),
                    downbeat_positions: Vec::new(),
                    time_signature_guess: (4, 4),
                    transient_positions: Vec::new(),
                    status: AnalysisStatus::Pending,
                });
            }
            Action::UpdateBeatDetectionStatus { result_id, status } => {
                if let Some(r) = self.beat_results.iter_mut().find(|r| r.id == result_id) {
                    r.status = status;
                }
            }
            Action::ApplyDetectedBpm { result_id } => {
                if let Some(r) = self.beat_results.iter().find(|r| r.id == result_id) {
                    if r.status == AnalysisStatus::Done {
                        self.project.bpm = r.detected_bpm.clamp(20.0, 999.0);
                    }
                }
            }
            Action::AddWarpMarker { clip_id, orig_secs, warp_beats } => {
                let id = self.next_warp_marker_id;
                self.next_warp_marker_id += 1;
                self.warp_markers.push(WarpMarker {
                    id,
                    clip_id,
                    original_time_secs: orig_secs,
                    warp_time_beats: warp_beats,
                    locked: false,
                });
            }
            Action::RemoveWarpMarker { marker_id } => {
                self.warp_markers.retain(|m| m.id != marker_id);
            }
            Action::MoveWarpMarker { marker_id, warp_beats } => {
                if let Some(m) = self.warp_markers.iter_mut().find(|m| m.id == marker_id) {
                    if !m.locked {
                        m.warp_time_beats = warp_beats;
                    }
                }
            }
            Action::LockWarpMarker { marker_id, locked } => {
                if let Some(m) = self.warp_markers.iter_mut().find(|m| m.id == marker_id) {
                    m.locked = locked;
                }
            }
            Action::SetStretchAlgorithm { clip_id, algorithm } => {
                let entry = self.stretch_configs.entry(clip_id).or_insert(StretchConfig {
                    clip_id,
                    algorithm: StretchAlgorithm::Beats,
                    formant_preservation: false,
                    transient_sensitivity: 0.5,
                });
                entry.algorithm = algorithm;
            }
            Action::SetFormantPreservation { clip_id, enabled } => {
                let entry = self.stretch_configs.entry(clip_id).or_insert(StretchConfig {
                    clip_id,
                    algorithm: StretchAlgorithm::Beats,
                    formant_preservation: false,
                    transient_sensitivity: 0.5,
                });
                entry.formant_preservation = enabled;
            }
            Action::SetTransientSensitivity { clip_id, sensitivity } => {
                let entry = self.stretch_configs.entry(clip_id).or_insert(StretchConfig {
                    clip_id,
                    algorithm: StretchAlgorithm::Beats,
                    formant_preservation: false,
                    transient_sensitivity: 0.5,
                });
                entry.transient_sensitivity = sensitivity.clamp(0.0, 1.0);
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fresh() -> App { App::new() }

    #[test]
    fn analyze_beat_detection_creates_result() {
        let mut app = fresh();
        app.apply(Action::AnalyzeBeatDetection { track_id: 0 });
        assert_eq!(app.beat_results.len(), 1);
        assert_eq!(app.beat_results[0].audio_track_id, 0);
        assert_eq!(app.beat_results[0].status, AnalysisStatus::Pending);
    }

    #[test]
    fn multiple_beat_detection_results_unique_ids() {
        let mut app = fresh();
        app.apply(Action::AnalyzeBeatDetection { track_id: 0 });
        app.apply(Action::AnalyzeBeatDetection { track_id: 1 });
        assert_eq!(app.beat_results.len(), 2);
        assert_ne!(app.beat_results[0].id, app.beat_results[1].id);
    }

    #[test]
    fn update_beat_detection_status_running() {
        let mut app = fresh();
        app.apply(Action::AnalyzeBeatDetection { track_id: 0 });
        let id = app.beat_results[0].id;
        app.apply(Action::UpdateBeatDetectionStatus {
            result_id: id,
            status: AnalysisStatus::Running { progress: 0.5 },
        });
        assert!(matches!(app.beat_results[0].status, AnalysisStatus::Running { .. }));
    }

    #[test]
    fn update_beat_detection_status_done() {
        let mut app = fresh();
        app.apply(Action::AnalyzeBeatDetection { track_id: 0 });
        let id = app.beat_results[0].id;
        app.apply(Action::UpdateBeatDetectionStatus { result_id: id, status: AnalysisStatus::Done });
        assert_eq!(app.beat_results[0].status, AnalysisStatus::Done);
    }

    #[test]
    fn apply_detected_bpm_updates_project() {
        let mut app = fresh();
        app.apply(Action::AnalyzeBeatDetection { track_id: 0 });
        let id = app.beat_results[0].id;
        app.beat_results[0].detected_bpm = 140.0;
        app.apply(Action::UpdateBeatDetectionStatus { result_id: id, status: AnalysisStatus::Done });
        app.apply(Action::ApplyDetectedBpm { result_id: id });
        assert!((app.project.bpm - 140.0).abs() < 0.01);
    }

    #[test]
    fn apply_detected_bpm_only_when_done() {
        let mut app = fresh();
        app.apply(Action::AnalyzeBeatDetection { track_id: 0 });
        let id = app.beat_results[0].id;
        app.beat_results[0].detected_bpm = 140.0;
        // status is still Pending — bpm should NOT change
        app.apply(Action::ApplyDetectedBpm { result_id: id });
        assert!((app.project.bpm - 120.0).abs() < 0.01);
    }

    #[test]
    fn add_warp_marker() {
        let mut app = fresh();
        app.apply(Action::AddWarpMarker { clip_id: 1, orig_secs: 2.5, warp_beats: 3.0 });
        assert_eq!(app.warp_markers.len(), 1);
        assert_eq!(app.warp_markers[0].clip_id, 1);
        assert!(!app.warp_markers[0].locked);
    }

    #[test]
    fn remove_warp_marker() {
        let mut app = fresh();
        app.apply(Action::AddWarpMarker { clip_id: 1, orig_secs: 2.5, warp_beats: 3.0 });
        let mid = app.warp_markers[0].id;
        app.apply(Action::RemoveWarpMarker { marker_id: mid });
        assert!(app.warp_markers.is_empty());
    }

    #[test]
    fn move_warp_marker() {
        let mut app = fresh();
        app.apply(Action::AddWarpMarker { clip_id: 1, orig_secs: 2.5, warp_beats: 3.0 });
        let mid = app.warp_markers[0].id;
        app.apply(Action::MoveWarpMarker { marker_id: mid, warp_beats: 5.0 });
        assert!((app.warp_markers[0].warp_time_beats - 5.0).abs() < 0.01);
    }

    #[test]
    fn lock_warp_marker_prevents_move() {
        let mut app = fresh();
        app.apply(Action::AddWarpMarker { clip_id: 1, orig_secs: 2.5, warp_beats: 3.0 });
        let mid = app.warp_markers[0].id;
        app.apply(Action::LockWarpMarker { marker_id: mid, locked: true });
        app.apply(Action::MoveWarpMarker { marker_id: mid, warp_beats: 99.0 });
        // position unchanged
        assert!((app.warp_markers[0].warp_time_beats - 3.0).abs() < 0.01);
    }

    #[test]
    fn set_stretch_algorithm() {
        let mut app = fresh();
        app.apply(Action::SetStretchAlgorithm { clip_id: 5, algorithm: StretchAlgorithm::Tones });
        assert_eq!(app.stretch_configs[&5].algorithm, StretchAlgorithm::Tones);
    }

    #[test]
    fn set_formant_preservation() {
        let mut app = fresh();
        app.apply(Action::SetFormantPreservation { clip_id: 5, enabled: true });
        assert!(app.stretch_configs[&5].formant_preservation);
    }

    #[test]
    fn set_transient_sensitivity_clamped() {
        let mut app = fresh();
        app.apply(Action::SetTransientSensitivity { clip_id: 5, sensitivity: 1.5 });
        assert!((app.stretch_configs[&5].transient_sensitivity - 1.0).abs() < 0.01);
    }

    #[test]
    fn beat_detection_failed_status() {
        let mut app = fresh();
        app.apply(Action::AnalyzeBeatDetection { track_id: 0 });
        let id = app.beat_results[0].id;
        app.apply(Action::UpdateBeatDetectionStatus {
            result_id: id,
            status: AnalysisStatus::Failed("No audio data".to_string()),
        });
        assert!(matches!(app.beat_results[0].status, AnalysisStatus::Failed(_)));
    }
}
