use super::{App, Action};

#[derive(Clone, Debug, PartialEq)]
pub enum FreezeState {
    Unfrozen,
    Freezing { progress: f32 },
    Frozen { file_path: String },
}

#[derive(Clone, Debug)]
pub struct FrozenTrackInfo {
    pub track_id: usize,
    pub freeze_state: FreezeState,
    pub freeze_tail_secs: f32,
    pub pre_fx: bool,
    pub locked: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub enum StemFormat {
    Wav24bit,
    Wav32float,
    Aiff,
    Flac,
}

#[derive(Clone, Debug, PartialEq)]
pub enum StemExportStatus {
    Idle,
    Running { progress: f32 },
    Done,
    Failed(String),
}

#[derive(Clone, Debug)]
pub struct StemConfig {
    pub id: usize,
    pub name: String,
    pub track_ids: Vec<usize>,
    pub output_path: String,
    pub format: StemFormat,
    pub normalize: bool,
    pub export_status: StemExportStatus,
}

impl App {
    pub(crate) fn apply_freeze(&mut self, action: Action) {
        match action {
            Action::FreezeTrack { track_id, pre_fx, tail_secs } => {
                let entry = self.frozen_tracks.entry(track_id).or_insert(FrozenTrackInfo {
                    track_id,
                    freeze_state: FreezeState::Unfrozen,
                    freeze_tail_secs: 0.0,
                    pre_fx: false,
                    locked: false,
                });
                entry.freeze_state = FreezeState::Freezing { progress: 0.0 };
                entry.pre_fx = pre_fx;
                entry.freeze_tail_secs = tail_secs.max(0.0);
            }
            Action::UnfreezeTrack { track_id } => {
                if let Some(info) = self.frozen_tracks.get_mut(&track_id) {
                    info.freeze_state = FreezeState::Unfrozen;
                    info.locked = false;
                }
            }
            Action::SetFreezeState { track_id, state } => {
                if let Some(info) = self.frozen_tracks.get_mut(&track_id) {
                    info.freeze_state = state;
                }
            }
            Action::LockFrozenTrack { track_id, locked } => {
                if let Some(info) = self.frozen_tracks.get_mut(&track_id) {
                    info.locked = locked;
                }
            }
            Action::FlattenTrack { track_id } => {
                self.frozen_tracks.remove(&track_id);
            }
            Action::AddStem { name, track_ids, format } => {
                let id = self.next_stem_id;
                self.next_stem_id += 1;
                self.stems.push(StemConfig {
                    id,
                    name,
                    track_ids,
                    output_path: String::new(),
                    format,
                    normalize: false,
                    export_status: StemExportStatus::Idle,
                });
            }
            Action::RemoveStem { stem_id } => {
                self.stems.retain(|s| s.id != stem_id);
            }
            Action::SetStemTracks { stem_id, track_ids } => {
                if let Some(s) = self.stems.iter_mut().find(|s| s.id == stem_id) {
                    s.track_ids = track_ids;
                }
            }
            Action::SetStemOutputPath { stem_id, path } => {
                if let Some(s) = self.stems.iter_mut().find(|s| s.id == stem_id) {
                    s.output_path = path;
                }
            }
            Action::ExportStems => {
                for s in &mut self.stems {
                    s.export_status = StemExportStatus::Running { progress: 0.0 };
                }
            }
            Action::UpdateStemStatus { stem_id, status } => {
                if let Some(s) = self.stems.iter_mut().find(|s| s.id == stem_id) {
                    s.export_status = status;
                }
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
    fn freeze_track_creates_entry() {
        let mut app = fresh();
        app.apply(Action::FreezeTrack { track_id: 1, pre_fx: false, tail_secs: 2.0 });
        assert!(app.frozen_tracks.contains_key(&1));
        assert!(matches!(app.frozen_tracks[&1].freeze_state, FreezeState::Freezing { .. }));
    }

    #[test]
    fn freeze_track_pre_fx_flag() {
        let mut app = fresh();
        app.apply(Action::FreezeTrack { track_id: 1, pre_fx: true, tail_secs: 0.0 });
        assert!(app.frozen_tracks[&1].pre_fx);
    }

    #[test]
    fn unfreeze_track() {
        let mut app = fresh();
        app.apply(Action::FreezeTrack { track_id: 1, pre_fx: false, tail_secs: 0.0 });
        app.apply(Action::UnfreezeTrack { track_id: 1 });
        assert_eq!(app.frozen_tracks[&1].freeze_state, FreezeState::Unfrozen);
    }

    #[test]
    fn set_freeze_state_frozen() {
        let mut app = fresh();
        app.apply(Action::FreezeTrack { track_id: 1, pre_fx: false, tail_secs: 0.0 });
        app.apply(Action::SetFreezeState {
            track_id: 1,
            state: FreezeState::Frozen { file_path: "/tmp/frozen.wav".to_string() },
        });
        assert!(matches!(app.frozen_tracks[&1].freeze_state, FreezeState::Frozen { .. }));
    }

    #[test]
    fn lock_frozen_track() {
        let mut app = fresh();
        app.apply(Action::FreezeTrack { track_id: 1, pre_fx: false, tail_secs: 0.0 });
        app.apply(Action::LockFrozenTrack { track_id: 1, locked: true });
        assert!(app.frozen_tracks[&1].locked);
    }

    #[test]
    fn flatten_track_removes_entry() {
        let mut app = fresh();
        app.apply(Action::FreezeTrack { track_id: 1, pre_fx: false, tail_secs: 0.0 });
        app.apply(Action::FlattenTrack { track_id: 1 });
        assert!(!app.frozen_tracks.contains_key(&1));
    }

    #[test]
    fn add_stem() {
        let mut app = fresh();
        app.apply(Action::AddStem {
            name: "Drums".to_string(),
            track_ids: vec![0, 1],
            format: StemFormat::Wav24bit,
        });
        assert_eq!(app.stems.len(), 1);
        assert_eq!(app.stems[0].name, "Drums");
    }

    #[test]
    fn remove_stem() {
        let mut app = fresh();
        app.apply(Action::AddStem {
            name: "Bass".to_string(),
            track_ids: vec![2],
            format: StemFormat::Flac,
        });
        let sid = app.stems[0].id;
        app.apply(Action::RemoveStem { stem_id: sid });
        assert!(app.stems.is_empty());
    }

    #[test]
    fn set_stem_output_path() {
        let mut app = fresh();
        app.apply(Action::AddStem {
            name: "Lead".to_string(),
            track_ids: vec![3],
            format: StemFormat::Aiff,
        });
        let sid = app.stems[0].id;
        app.apply(Action::SetStemOutputPath { stem_id: sid, path: "/out/lead.aiff".to_string() });
        assert_eq!(app.stems[0].output_path, "/out/lead.aiff");
    }

    #[test]
    fn export_stems_sets_running() {
        let mut app = fresh();
        app.apply(Action::AddStem { name: "A".to_string(), track_ids: vec![], format: StemFormat::Wav32float });
        app.apply(Action::AddStem { name: "B".to_string(), track_ids: vec![], format: StemFormat::Wav32float });
        app.apply(Action::ExportStems);
        for s in &app.stems {
            assert!(matches!(s.export_status, StemExportStatus::Running { .. }));
        }
    }

    #[test]
    fn update_stem_status_done() {
        let mut app = fresh();
        app.apply(Action::AddStem { name: "X".to_string(), track_ids: vec![], format: StemFormat::Flac });
        let sid = app.stems[0].id;
        app.apply(Action::UpdateStemStatus { stem_id: sid, status: StemExportStatus::Done });
        assert_eq!(app.stems[0].export_status, StemExportStatus::Done);
    }

    #[test]
    fn set_stem_tracks() {
        let mut app = fresh();
        app.apply(Action::AddStem { name: "Mix".to_string(), track_ids: vec![0], format: StemFormat::Wav24bit });
        let sid = app.stems[0].id;
        app.apply(Action::SetStemTracks { stem_id: sid, track_ids: vec![1, 2, 3] });
        assert_eq!(app.stems[0].track_ids, vec![1, 2, 3]);
    }
}
