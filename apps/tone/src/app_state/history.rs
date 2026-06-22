//! History domain — undo/redo stack + apply methods + tests.

// ─── Constants ────────────────────────────────────────────────────────────────

pub const MAX_UNDO: usize = 50;

// ─── Types ────────────────────────────────────────────────────────────────────

/// A snapshot of key state at a point in time, for undo/redo.
pub struct HistoryEntry {
    pub description: String,
    pub tracks_snapshot: Vec<crate::app_state::tracks::ToneTrack>,
    pub clips_snapshot: Vec<crate::app_state::clips::ToneClip>,
    pub midi_notes_snapshot: Vec<crate::app_state::midi::MidiNote>,
}

// ─── Apply methods ────────────────────────────────────────────────────────────

use super::{Action, App};

impl App {
    /// Push a checkpoint onto the undo stack. Clears the redo stack.
    pub fn push_undo_checkpoint(&mut self, description: String) {
        let entry = HistoryEntry {
            description,
            tracks_snapshot: self.tracks.clone(),
            clips_snapshot: self.clips.clone(),
            midi_notes_snapshot: self.midi_notes.clone(),
        };
        self.undo_stack.push(entry);
        // Cap at MAX_UNDO by removing oldest entries
        while self.undo_stack.len() > MAX_UNDO {
            self.undo_stack.remove(0);
        }
        self.redo_stack.clear();
    }

    /// Undo the last operation. Saves current state to redo stack.
    pub fn apply_undo(&mut self) {
        let Some(entry) = self.undo_stack.pop() else { return };
        // Save current state to redo stack
        let current = HistoryEntry {
            description: entry.description.clone(),
            tracks_snapshot: self.tracks.clone(),
            clips_snapshot: self.clips.clone(),
            midi_notes_snapshot: self.midi_notes.clone(),
        };
        self.redo_stack.push(current);
        // Restore snapshot
        self.tracks = entry.tracks_snapshot;
        self.clips = entry.clips_snapshot;
        self.midi_notes = entry.midi_notes_snapshot;
    }

    /// Redo the last undone operation. Saves current state to undo stack.
    pub fn apply_redo(&mut self) {
        let Some(entry) = self.redo_stack.pop() else { return };
        // Save current state back to undo stack
        let current = HistoryEntry {
            description: entry.description.clone(),
            tracks_snapshot: self.tracks.clone(),
            clips_snapshot: self.clips.clone(),
            midi_notes_snapshot: self.midi_notes.clone(),
        };
        self.undo_stack.push(current);
        // Restore snapshot
        self.tracks = entry.tracks_snapshot;
        self.clips = entry.clips_snapshot;
        self.midi_notes = entry.midi_notes_snapshot;
    }

    pub(super) fn apply_history(&mut self, action: Action) {
        match action {
            Action::Undo => self.apply_undo(),
            Action::Redo => self.apply_redo(),
            Action::PushUndoCheckpoint { description } => self.push_undo_checkpoint(description),
            _ => {}
        }
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::super::{Action, App};
    use super::super::tracks::TrackKind;
    use super::super::clips::ClipKind;
    use super::MAX_UNDO;

    fn fresh() -> App {
        App::new()
    }

    #[test]
    fn undo_stack_empty_initially() {
        let app = fresh();
        assert!(app.undo_stack.is_empty());
        assert!(app.redo_stack.is_empty());
    }

    #[test]
    fn push_checkpoint_adds_to_undo_stack() {
        let mut app = fresh();
        app.apply(Action::PushUndoCheckpoint { description: "add track".to_string() });
        assert_eq!(app.undo_stack.len(), 1);
        assert_eq!(app.undo_stack[0].description, "add track");
    }

    #[test]
    fn undo_after_add_track_restores_state() {
        let mut app = fresh();
        let initial_track_count = app.tracks.len();
        // Checkpoint before action
        app.apply(Action::PushUndoCheckpoint { description: "before add".to_string() });
        app.apply(Action::AddTrack(TrackKind::Audio));
        assert_eq!(app.tracks.len(), initial_track_count + 1);
        // Undo
        app.apply(Action::Undo);
        assert_eq!(app.tracks.len(), initial_track_count);
    }

    #[test]
    fn undo_restores_note_state() {
        let mut app = fresh();
        app.apply(Action::AddTrack(TrackKind::Midi));
        let tid = app.tracks.last().unwrap().id;
        app.apply(Action::AddClip {
            track_id: tid,
            name: "m".into(),
            kind: ClipKind::Midi,
            start_beat: 0.0,
            duration_beats: 4.0,
        });
        let cid = app.clips[0].id;
        // Checkpoint before adding notes
        app.apply(Action::PushUndoCheckpoint { description: "before notes".to_string() });
        app.apply(Action::AddMidiNote { clip_id: cid, pitch: 60, velocity: 100, start_beat: 0.0, duration_beats: 1.0 });
        assert_eq!(app.midi_notes.len(), 1);
        app.apply(Action::Undo);
        assert_eq!(app.midi_notes.len(), 0);
    }

    #[test]
    fn redo_after_undo() {
        let mut app = fresh();
        let initial_count = app.tracks.len();
        app.apply(Action::PushUndoCheckpoint { description: "before add".to_string() });
        app.apply(Action::AddTrack(TrackKind::Audio));
        assert_eq!(app.tracks.len(), initial_count + 1);
        app.apply(Action::Undo);
        assert_eq!(app.tracks.len(), initial_count);
        app.apply(Action::Redo);
        assert_eq!(app.tracks.len(), initial_count + 1);
    }

    #[test]
    fn redo_stack_cleared_after_new_push() {
        let mut app = fresh();
        app.apply(Action::PushUndoCheckpoint { description: "step 1".to_string() });
        app.apply(Action::AddTrack(TrackKind::Audio));
        app.apply(Action::Undo);
        assert_eq!(app.redo_stack.len(), 1);
        // Pushing a new checkpoint clears redo
        app.apply(Action::PushUndoCheckpoint { description: "step 2".to_string() });
        assert!(app.redo_stack.is_empty());
    }

    #[test]
    fn undo_stack_caps_at_50() {
        let mut app = fresh();
        for i in 0..60 {
            app.apply(Action::PushUndoCheckpoint { description: format!("step {}", i) });
        }
        assert_eq!(app.undo_stack.len(), MAX_UNDO);
    }

    #[test]
    fn undo_caps_removes_oldest() {
        let mut app = fresh();
        for i in 0..55 {
            app.apply(Action::PushUndoCheckpoint { description: format!("step {}", i) });
        }
        // The oldest entries should have been evicted; most recent should remain
        assert_eq!(app.undo_stack.last().unwrap().description, "step 54");
    }

    #[test]
    fn empty_undo_is_noop() {
        let mut app = fresh();
        let initial_track_count = app.tracks.len();
        // Should not panic
        app.apply(Action::Undo);
        assert_eq!(app.tracks.len(), initial_track_count);
    }

    #[test]
    fn empty_redo_is_noop() {
        let mut app = fresh();
        let initial_track_count = app.tracks.len();
        // Should not panic
        app.apply(Action::Redo);
        assert_eq!(app.tracks.len(), initial_track_count);
    }

    #[test]
    fn multiple_undo_cycles() {
        let mut app = fresh();
        let initial = app.tracks.len();
        app.apply(Action::PushUndoCheckpoint { description: "before track 1".to_string() });
        app.apply(Action::AddTrack(TrackKind::Audio));
        app.apply(Action::PushUndoCheckpoint { description: "before track 2".to_string() });
        app.apply(Action::AddTrack(TrackKind::Midi));
        assert_eq!(app.tracks.len(), initial + 2);
        app.apply(Action::Undo);
        assert_eq!(app.tracks.len(), initial + 1);
        app.apply(Action::Undo);
        assert_eq!(app.tracks.len(), initial);
    }

    #[test]
    fn multiple_redo_cycles() {
        let mut app = fresh();
        let initial = app.tracks.len();
        app.apply(Action::PushUndoCheckpoint { description: "before track 1".to_string() });
        app.apply(Action::AddTrack(TrackKind::Audio));
        app.apply(Action::PushUndoCheckpoint { description: "before track 2".to_string() });
        app.apply(Action::AddTrack(TrackKind::Midi));
        app.apply(Action::Undo);
        app.apply(Action::Undo);
        assert_eq!(app.tracks.len(), initial);
        app.apply(Action::Redo);
        assert_eq!(app.tracks.len(), initial + 1);
        app.apply(Action::Redo);
        assert_eq!(app.tracks.len(), initial + 2);
    }

    #[test]
    fn clips_restored_on_undo() {
        let mut app = fresh();
        app.apply(Action::AddTrack(TrackKind::Audio));
        let tid = app.tracks.last().unwrap().id;
        app.apply(Action::PushUndoCheckpoint { description: "before clip".to_string() });
        app.apply(Action::AddClip {
            track_id: tid,
            name: "c".into(),
            kind: ClipKind::Audio,
            start_beat: 0.0,
            duration_beats: 4.0,
        });
        assert_eq!(app.clips.len(), 1);
        app.apply(Action::Undo);
        assert_eq!(app.clips.len(), 0);
    }
}
