//! Recording domain — punch-in/out, takes, comp mode.

use super::{Action, App};

// ─── Types ────────────────────────────────────────────────────────────────────

pub struct RecordingSession {
    pub id: usize,
    pub track_id: usize,
    pub start_beat: f32,
    pub end_beat: Option<f32>,
    pub take_number: u32,
    pub is_active: bool,
}

pub struct PunchConfig {
    pub punch_in: bool,
    pub punch_out: bool,
    pub punch_in_beat: f32,
    pub punch_out_beat: f32,
    pub auto_punch: bool,
}

impl PunchConfig {
    pub fn new() -> Self {
        Self {
            punch_in: false,
            punch_out: false,
            punch_in_beat: 0.0,
            punch_out_beat: 16.0,
            auto_punch: false,
        }
    }
}

impl Default for PunchConfig {
    fn default() -> Self {
        Self::new()
    }
}

pub struct TakeManager {
    pub track_id: usize,
    pub takes: Vec<TakeInfo>,
    pub active_take: usize,
    pub comp_mode: bool,
}

pub struct TakeInfo {
    pub id: usize,
    pub name: String,
    pub clip_id: usize,
    pub muted: bool,
}

// ─── Apply methods ────────────────────────────────────────────────────────────

impl App {
    pub(super) fn apply_recording(&mut self, action: Action) {
        match action {
            Action::StartRecording { track_ids } => {
                for track_id in track_ids {
                    let take_number = self.recording_sessions.iter()
                        .filter(|s| s.track_id == track_id)
                        .count() as u32 + 1;
                    let id = self.next_session_id;
                    self.next_session_id += 1;
                    self.recording_sessions.push(RecordingSession {
                        id,
                        track_id,
                        start_beat: self.playhead_beat,
                        end_beat: None,
                        take_number,
                        is_active: true,
                    });
                }
            }
            Action::StopRecording => {
                let end = self.playhead_beat;
                for session in self.recording_sessions.iter_mut() {
                    if session.is_active {
                        session.end_beat = Some(end);
                        session.is_active = false;
                    }
                }
            }
            Action::SetPunchIn { enabled, beat } => {
                self.punch_config.punch_in = enabled;
                self.punch_config.punch_in_beat = beat;
            }
            Action::SetPunchOut { enabled, beat } => {
                self.punch_config.punch_out = enabled;
                self.punch_config.punch_out_beat = beat;
            }
            Action::SetAutoPunch(enabled) => {
                self.punch_config.auto_punch = enabled;
            }
            Action::SetCountIn { enabled, bars } => {
                self.count_in_enabled = enabled;
                self.count_in_bars = bars;
            }
            Action::AddTake { track_id, clip_id } => {
                let manager = self.take_managers.iter_mut().find(|m| m.track_id == track_id);
                if let Some(mgr) = manager {
                    let n = mgr.takes.len();
                    let id = n;
                    mgr.takes.push(TakeInfo {
                        id,
                        name: format!("Take {}", n + 1),
                        clip_id,
                        muted: false,
                    });
                } else {
                    self.take_managers.push(TakeManager {
                        track_id,
                        takes: vec![TakeInfo {
                            id: 0,
                            name: "Take 1".to_string(),
                            clip_id,
                            muted: false,
                        }],
                        active_take: 0,
                        comp_mode: false,
                    });
                }
            }
            Action::SetActiveTake { track_id, take_id } => {
                if let Some(mgr) = self.take_managers.iter_mut().find(|m| m.track_id == track_id) {
                    if mgr.takes.iter().any(|t| t.id == take_id) {
                        mgr.active_take = take_id;
                    }
                }
            }
            Action::MuteTake { track_id, take_id, muted } => {
                if let Some(mgr) = self.take_managers.iter_mut().find(|m| m.track_id == track_id) {
                    if let Some(take) = mgr.takes.iter_mut().find(|t| t.id == take_id) {
                        take.muted = muted;
                    }
                }
            }
            Action::DeleteTake { track_id, take_id } => {
                if let Some(mgr) = self.take_managers.iter_mut().find(|m| m.track_id == track_id) {
                    mgr.takes.retain(|t| t.id != take_id);
                    // If active take was deleted, reset to first take
                    if mgr.active_take == take_id {
                        mgr.active_take = mgr.takes.first().map(|t| t.id).unwrap_or(0);
                    }
                }
            }
            Action::EnableCompMode { track_id, enabled } => {
                if let Some(mgr) = self.take_managers.iter_mut().find(|m| m.track_id == track_id) {
                    mgr.comp_mode = enabled;
                }
            }
            Action::FlattenTakes { track_id } => {
                if let Some(mgr) = self.take_managers.iter_mut().find(|m| m.track_id == track_id) {
                    let active = mgr.active_take;
                    mgr.takes.retain(|t| t.id == active);
                }
            }
            _ => {}
        }
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::super::{Action, App};

    fn fresh() -> App {
        App::new()
    }

    #[test]
    fn recording_defaults() {
        let app = fresh();
        assert!(app.recording_sessions.is_empty());
        assert!(app.take_managers.is_empty());
        assert_eq!(app.next_session_id, 0);
        assert_eq!(app.count_in_bars, 1);
        assert!(!app.count_in_enabled);
        assert!(!app.punch_config.punch_in);
        assert!(!app.punch_config.punch_out);
    }

    #[test]
    fn start_recording_creates_sessions() {
        let mut app = fresh();
        app.apply(Action::StartRecording { track_ids: vec![1, 2] });
        assert_eq!(app.recording_sessions.len(), 2);
        assert!(app.recording_sessions[0].is_active);
        assert!(app.recording_sessions[1].is_active);
        assert_eq!(app.recording_sessions[0].track_id, 1);
        assert_eq!(app.recording_sessions[1].track_id, 2);
    }

    #[test]
    fn start_recording_take_number_increments() {
        let mut app = fresh();
        app.apply(Action::StartRecording { track_ids: vec![1] });
        app.apply(Action::StopRecording);
        app.apply(Action::StartRecording { track_ids: vec![1] });
        assert_eq!(app.recording_sessions[0].take_number, 1);
        assert_eq!(app.recording_sessions[1].take_number, 2);
    }

    #[test]
    fn stop_recording_closes_active_sessions() {
        let mut app = fresh();
        app.apply(Action::StartRecording { track_ids: vec![1] });
        app.apply(Action::SetPlayheadBeat(8.0));
        app.apply(Action::StopRecording);
        assert!(!app.recording_sessions[0].is_active);
        assert_eq!(app.recording_sessions[0].end_beat, Some(8.0));
    }

    #[test]
    fn set_punch_in() {
        let mut app = fresh();
        app.apply(Action::SetPunchIn { enabled: true, beat: 4.0 });
        assert!(app.punch_config.punch_in);
        assert!((app.punch_config.punch_in_beat - 4.0).abs() < 0.001);
    }

    #[test]
    fn set_punch_out() {
        let mut app = fresh();
        app.apply(Action::SetPunchOut { enabled: true, beat: 12.0 });
        assert!(app.punch_config.punch_out);
        assert!((app.punch_config.punch_out_beat - 12.0).abs() < 0.001);
    }

    #[test]
    fn set_auto_punch() {
        let mut app = fresh();
        assert!(!app.punch_config.auto_punch);
        app.apply(Action::SetAutoPunch(true));
        assert!(app.punch_config.auto_punch);
    }

    #[test]
    fn set_count_in() {
        let mut app = fresh();
        app.apply(Action::SetCountIn { enabled: true, bars: 2 });
        assert!(app.count_in_enabled);
        assert_eq!(app.count_in_bars, 2);
    }

    #[test]
    fn add_take_creates_manager() {
        let mut app = fresh();
        app.apply(Action::AddTake { track_id: 5, clip_id: 10 });
        assert_eq!(app.take_managers.len(), 1);
        assert_eq!(app.take_managers[0].track_id, 5);
        assert_eq!(app.take_managers[0].takes.len(), 1);
        assert_eq!(app.take_managers[0].takes[0].name, "Take 1");
    }

    #[test]
    fn add_take_appends_to_existing_manager() {
        let mut app = fresh();
        app.apply(Action::AddTake { track_id: 5, clip_id: 10 });
        app.apply(Action::AddTake { track_id: 5, clip_id: 11 });
        let mgr = app.take_managers.iter().find(|m| m.track_id == 5).unwrap();
        assert_eq!(mgr.takes.len(), 2);
        assert_eq!(mgr.takes[1].name, "Take 2");
    }

    #[test]
    fn set_active_take() {
        let mut app = fresh();
        app.apply(Action::AddTake { track_id: 5, clip_id: 10 });
        app.apply(Action::AddTake { track_id: 5, clip_id: 11 });
        app.apply(Action::SetActiveTake { track_id: 5, take_id: 1 });
        let mgr = app.take_managers.iter().find(|m| m.track_id == 5).unwrap();
        assert_eq!(mgr.active_take, 1);
    }

    #[test]
    fn mute_take() {
        let mut app = fresh();
        app.apply(Action::AddTake { track_id: 5, clip_id: 10 });
        app.apply(Action::MuteTake { track_id: 5, take_id: 0, muted: true });
        let mgr = app.take_managers.iter().find(|m| m.track_id == 5).unwrap();
        assert!(mgr.takes[0].muted);
    }

    #[test]
    fn delete_take() {
        let mut app = fresh();
        app.apply(Action::AddTake { track_id: 5, clip_id: 10 });
        app.apply(Action::AddTake { track_id: 5, clip_id: 11 });
        app.apply(Action::DeleteTake { track_id: 5, take_id: 0 });
        let mgr = app.take_managers.iter().find(|m| m.track_id == 5).unwrap();
        assert_eq!(mgr.takes.len(), 1);
        assert_eq!(mgr.takes[0].id, 1);
    }

    #[test]
    fn enable_comp_mode() {
        let mut app = fresh();
        app.apply(Action::AddTake { track_id: 5, clip_id: 10 });
        app.apply(Action::EnableCompMode { track_id: 5, enabled: true });
        let mgr = app.take_managers.iter().find(|m| m.track_id == 5).unwrap();
        assert!(mgr.comp_mode);
    }

    #[test]
    fn flatten_takes_keeps_active() {
        let mut app = fresh();
        app.apply(Action::AddTake { track_id: 5, clip_id: 10 });
        app.apply(Action::AddTake { track_id: 5, clip_id: 11 });
        app.apply(Action::AddTake { track_id: 5, clip_id: 12 });
        app.apply(Action::SetActiveTake { track_id: 5, take_id: 1 });
        app.apply(Action::FlattenTakes { track_id: 5 });
        let mgr = app.take_managers.iter().find(|m| m.track_id == 5).unwrap();
        assert_eq!(mgr.takes.len(), 1);
        assert_eq!(mgr.takes[0].id, 1);
    }
}
