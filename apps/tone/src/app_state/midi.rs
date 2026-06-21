//! MIDI / Piano Roll domain — `MidiNote`, `MidiCC`, `PianoRollState` types
//! + midi apply methods + tests.

// ─── Types ────────────────────────────────────────────────────────────────────

/// A single MIDI note inside a MIDI or Instrument clip.
#[derive(Clone, Debug)]
pub struct MidiNote {
    pub id: usize,
    pub clip_id: usize,
    /// MIDI pitch number 0..=127 (60 = C4 / middle C).
    pub pitch: u8,
    /// MIDI velocity 0..=127.
    pub velocity: u8,
    /// Start position in beats, relative to the clip start.
    pub start_beat: f32,
    /// Duration in beats.
    pub duration_beats: f32,
}

/// A single MIDI CC (continuous controller) event inside a clip.
#[derive(Clone, Debug)]
pub struct MidiCC {
    pub clip_id: usize,
    /// CC number 0..=127 (e.g. 1 = Mod Wheel, 7 = Volume, 11 = Expression).
    pub controller: u8,
    /// Beat position inside the clip.
    pub position: f32,
    /// CC value 0..=127.
    pub value: u8,
}

// ─── Apply methods ────────────────────────────────────────────────────────────

use super::{Action, App};

impl App {
    pub(super) fn apply_midi(&mut self, action: Action) {
        match action {
            Action::OpenPianoRoll(clip_id) => {
                self.piano_roll_clip = Some(clip_id);
            }
            Action::ClosePianoRoll => {
                self.piano_roll_clip = None;
            }
            Action::AddMidiNote { clip_id, pitch, velocity, start_beat, duration_beats } => {
                let id = self.next_note_id();
                self.midi_notes.push(MidiNote {
                    id,
                    clip_id,
                    pitch: pitch.min(127),
                    velocity: velocity.min(127),
                    start_beat,
                    duration_beats: duration_beats.max(0.0625),
                });
            }
            Action::DeleteMidiNote(id) => {
                self.selected_notes.retain(|&n| n != id);
                self.midi_notes.retain(|n| n.id != id);
            }
            Action::MoveMidiNote { id, pitch, start_beat } => {
                if let Some(n) = self.find_note_mut(id) {
                    n.pitch = pitch.min(127);
                    n.start_beat = start_beat.max(0.0);
                }
            }
            Action::ResizeMidiNote { id, duration_beats } => {
                if let Some(n) = self.find_note_mut(id) {
                    n.duration_beats = duration_beats.max(0.0625);
                }
            }
            Action::SetMidiNoteVelocity { id, velocity } => {
                if let Some(n) = self.find_note_mut(id) {
                    n.velocity = velocity.clamp(0, 127);
                }
            }
            Action::SelectAllNotesInClip(clip_id) => {
                self.selected_notes = self.midi_notes.iter()
                    .filter(|n| n.clip_id == clip_id)
                    .map(|n| n.id)
                    .collect();
            }
            Action::QuantizeMidiNotes { clip_id, grid } => {
                let grid = grid.max(0.015625); // minimum 1/64th note
                for note in self.midi_notes.iter_mut().filter(|n| n.clip_id == clip_id) {
                    note.start_beat = (note.start_beat / grid).round() * grid;
                }
            }
            Action::TransposeMidiNotes { clip_ids, semitones } => {
                for note in self.midi_notes.iter_mut().filter(|n| clip_ids.contains(&n.clip_id)) {
                    let new_pitch = (note.pitch as i16 + semitones as i16).clamp(0, 127);
                    note.pitch = new_pitch as u8;
                }
            }
            Action::SetPianoRollZoom { zoom_h, zoom_v } => {
                self.piano_roll_zoom_h = zoom_h.max(0.1);
                self.piano_roll_zoom_v = zoom_v.max(1.0);
            }
            Action::SetPianoRollScroll { beat, key } => {
                self.piano_roll_scroll_beat = beat.max(0.0);
                self.piano_roll_scroll_key = key.min(127);
            }
            // ── MIDI CC lanes ─────────────────────────────────────────────────
            Action::AddMidiCC { clip_id, controller, position, value } => {
                let _id = self.next_cc_id;
                self.next_cc_id += 1;
                self.midi_cc.push(MidiCC {
                    clip_id,
                    controller: controller.min(127),
                    position: position.max(0.0),
                    value: value.min(127),
                });
            }
            Action::RemoveMidiCC { cc_id } => {
                if cc_id < self.midi_cc.len() {
                    self.midi_cc.remove(cc_id);
                }
            }
            Action::SetMidiCCValue { cc_id, value } => {
                if let Some(cc) = self.midi_cc.get_mut(cc_id) {
                    cc.value = value.min(127);
                }
            }
            Action::MoveMidiCC { cc_id, position } => {
                if let Some(cc) = self.midi_cc.get_mut(cc_id) {
                    cc.position = position.max(0.0);
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
    use super::super::tracks::TrackKind;
    use super::super::clips::ClipKind;

    fn fresh() -> App {
        App::new()
    }

    fn add_midi_clip(app: &mut App) -> (usize, usize) {
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
        (tid, cid)
    }

    #[test]
    fn open_close_piano_roll() {
        let mut app = fresh();
        let (_tid, cid) = add_midi_clip(&mut app);
        app.apply(Action::OpenPianoRoll(cid));
        assert_eq!(app.piano_roll_clip, Some(cid));
        app.apply(Action::ClosePianoRoll);
        assert_eq!(app.piano_roll_clip, None);
    }

    #[test]
    fn add_midi_note() {
        let mut app = fresh();
        let (_tid, cid) = add_midi_clip(&mut app);
        app.apply(Action::AddMidiNote { clip_id: cid, pitch: 60, velocity: 100, start_beat: 0.0, duration_beats: 1.0 });
        assert_eq!(app.midi_notes.len(), 1);
        assert_eq!(app.midi_notes[0].pitch, 60);
    }

    #[test]
    fn delete_midi_note() {
        let mut app = fresh();
        let (_tid, cid) = add_midi_clip(&mut app);
        app.apply(Action::AddMidiNote { clip_id: cid, pitch: 60, velocity: 100, start_beat: 0.0, duration_beats: 1.0 });
        let nid = app.midi_notes[0].id;
        app.apply(Action::DeleteMidiNote(nid));
        assert!(app.midi_notes.is_empty());
    }

    #[test]
    fn move_midi_note() {
        let mut app = fresh();
        let (_tid, cid) = add_midi_clip(&mut app);
        app.apply(Action::AddMidiNote { clip_id: cid, pitch: 60, velocity: 100, start_beat: 0.0, duration_beats: 1.0 });
        let nid = app.midi_notes[0].id;
        app.apply(Action::MoveMidiNote { id: nid, pitch: 64, start_beat: 2.0 });
        assert_eq!(app.midi_notes[0].pitch, 64);
        assert_eq!(app.midi_notes[0].start_beat, 2.0);
    }

    #[test]
    fn resize_midi_note() {
        let mut app = fresh();
        let (_tid, cid) = add_midi_clip(&mut app);
        app.apply(Action::AddMidiNote { clip_id: cid, pitch: 60, velocity: 100, start_beat: 0.0, duration_beats: 1.0 });
        let nid = app.midi_notes[0].id;
        app.apply(Action::ResizeMidiNote { id: nid, duration_beats: 2.0 });
        assert_eq!(app.midi_notes[0].duration_beats, 2.0);
    }

    #[test]
    fn set_midi_note_velocity_clamp() {
        let mut app = fresh();
        let (_tid, cid) = add_midi_clip(&mut app);
        app.apply(Action::AddMidiNote { clip_id: cid, pitch: 60, velocity: 100, start_beat: 0.0, duration_beats: 1.0 });
        let nid = app.midi_notes[0].id;
        app.apply(Action::SetMidiNoteVelocity { id: nid, velocity: 200 });
        assert_eq!(app.midi_notes[0].velocity, 127);
    }

    #[test]
    fn select_all_notes_in_clip() {
        let mut app = fresh();
        let (_tid, cid) = add_midi_clip(&mut app);
        for pitch in [60u8, 62, 64] {
            app.apply(Action::AddMidiNote { clip_id: cid, pitch, velocity: 80, start_beat: 0.0, duration_beats: 1.0 });
        }
        app.apply(Action::SelectAllNotesInClip(cid));
        assert_eq!(app.selected_notes.len(), 3);
    }

    #[test]
    fn quantize_midi_notes() {
        let mut app = fresh();
        let (_tid, cid) = add_midi_clip(&mut app);
        app.apply(Action::AddMidiNote { clip_id: cid, pitch: 60, velocity: 100, start_beat: 0.13, duration_beats: 1.0 });
        app.apply(Action::QuantizeMidiNotes { clip_id: cid, grid: 0.25 });
        let snapped = app.midi_notes[0].start_beat;
        assert!((snapped - 0.25).abs() < 0.001 || snapped.abs() < 0.001);
    }

    #[test]
    fn transpose_midi_notes() {
        let mut app = fresh();
        let (_tid, cid) = add_midi_clip(&mut app);
        app.apply(Action::AddMidiNote { clip_id: cid, pitch: 60, velocity: 100, start_beat: 0.0, duration_beats: 1.0 });
        app.apply(Action::TransposeMidiNotes { clip_ids: vec![cid], semitones: 7 });
        assert_eq!(app.midi_notes[0].pitch, 67);
    }

    #[test]
    fn transpose_notes_clamped_at_127() {
        let mut app = fresh();
        let (_tid, cid) = add_midi_clip(&mut app);
        app.apply(Action::AddMidiNote { clip_id: cid, pitch: 125, velocity: 100, start_beat: 0.0, duration_beats: 1.0 });
        app.apply(Action::TransposeMidiNotes { clip_ids: vec![cid], semitones: 10 });
        assert_eq!(app.midi_notes[0].pitch, 127);
    }

    #[test]
    fn transpose_notes_clamped_at_0() {
        let mut app = fresh();
        let (_tid, cid) = add_midi_clip(&mut app);
        app.apply(Action::AddMidiNote { clip_id: cid, pitch: 2, velocity: 100, start_beat: 0.0, duration_beats: 1.0 });
        app.apply(Action::TransposeMidiNotes { clip_ids: vec![cid], semitones: -10 });
        assert_eq!(app.midi_notes[0].pitch, 0);
    }

    #[test]
    fn set_piano_roll_zoom() {
        let mut app = fresh();
        app.apply(Action::SetPianoRollZoom { zoom_h: 4.0, zoom_v: 24.0 });
        assert!((app.piano_roll_zoom_h - 4.0).abs() < 0.001);
        assert!((app.piano_roll_zoom_v - 24.0).abs() < 0.001);
    }

    #[test]
    fn set_piano_roll_zoom_clamp_low() {
        let mut app = fresh();
        app.apply(Action::SetPianoRollZoom { zoom_h: 0.0, zoom_v: 0.0 });
        assert!(app.piano_roll_zoom_h >= 0.1);
        assert!(app.piano_roll_zoom_v >= 1.0);
    }

    #[test]
    fn set_piano_roll_scroll() {
        let mut app = fresh();
        app.apply(Action::SetPianoRollScroll { beat: 8.0, key: 72 });
        assert!((app.piano_roll_scroll_beat - 8.0).abs() < 0.001);
        assert_eq!(app.piano_roll_scroll_key, 72);
    }

    #[test]
    fn set_piano_roll_scroll_clamp_negative_beat() {
        let mut app = fresh();
        app.apply(Action::SetPianoRollScroll { beat: -5.0, key: 60 });
        assert_eq!(app.piano_roll_scroll_beat, 0.0);
    }

    #[test]
    fn set_piano_roll_scroll_key_clamped() {
        let mut app = fresh();
        app.apply(Action::SetPianoRollScroll { beat: 0.0, key: 200 });
        assert!(app.piano_roll_scroll_key <= 127);
    }

    // ── MIDI CC tests ─────────────────────────────────────────────────────────

    #[test]
    fn add_midi_cc() {
        let mut app = fresh();
        let (_tid, cid) = add_midi_clip(&mut app);
        app.apply(Action::AddMidiCC { clip_id: cid, controller: 1, position: 0.5, value: 64 });
        assert_eq!(app.midi_cc.len(), 1);
        assert_eq!(app.midi_cc[0].clip_id, cid);
        assert_eq!(app.midi_cc[0].controller, 1);
        assert_eq!(app.midi_cc[0].value, 64);
    }

    #[test]
    fn add_midi_cc_clamped_value() {
        let mut app = fresh();
        let (_tid, cid) = add_midi_clip(&mut app);
        app.apply(Action::AddMidiCC { clip_id: cid, controller: 7, position: 0.0, value: 200 });
        assert_eq!(app.midi_cc[0].value, 127);
    }

    #[test]
    fn add_midi_cc_clamped_controller() {
        let mut app = fresh();
        let (_tid, cid) = add_midi_clip(&mut app);
        app.apply(Action::AddMidiCC { clip_id: cid, controller: 200, position: 0.0, value: 64 });
        assert_eq!(app.midi_cc[0].controller, 127);
    }

    #[test]
    fn add_midi_cc_clamps_negative_position() {
        let mut app = fresh();
        let (_tid, cid) = add_midi_clip(&mut app);
        app.apply(Action::AddMidiCC { clip_id: cid, controller: 1, position: -1.0, value: 64 });
        assert_eq!(app.midi_cc[0].position, 0.0);
    }

    #[test]
    fn remove_midi_cc() {
        let mut app = fresh();
        let (_tid, cid) = add_midi_clip(&mut app);
        app.apply(Action::AddMidiCC { clip_id: cid, controller: 1, position: 0.0, value: 64 });
        assert_eq!(app.midi_cc.len(), 1);
        app.apply(Action::RemoveMidiCC { cc_id: 0 });
        assert!(app.midi_cc.is_empty());
    }

    #[test]
    fn remove_midi_cc_out_of_bounds_is_noop() {
        let mut app = fresh();
        app.apply(Action::RemoveMidiCC { cc_id: 99 });
        assert!(app.midi_cc.is_empty());
    }

    #[test]
    fn set_midi_cc_value() {
        let mut app = fresh();
        let (_tid, cid) = add_midi_clip(&mut app);
        app.apply(Action::AddMidiCC { clip_id: cid, controller: 1, position: 0.0, value: 64 });
        app.apply(Action::SetMidiCCValue { cc_id: 0, value: 100 });
        assert_eq!(app.midi_cc[0].value, 100);
    }

    #[test]
    fn set_midi_cc_value_clamped() {
        let mut app = fresh();
        let (_tid, cid) = add_midi_clip(&mut app);
        app.apply(Action::AddMidiCC { clip_id: cid, controller: 1, position: 0.0, value: 64 });
        app.apply(Action::SetMidiCCValue { cc_id: 0, value: 200 });
        assert_eq!(app.midi_cc[0].value, 127);
    }

    #[test]
    fn move_midi_cc() {
        let mut app = fresh();
        let (_tid, cid) = add_midi_clip(&mut app);
        app.apply(Action::AddMidiCC { clip_id: cid, controller: 1, position: 0.0, value: 64 });
        app.apply(Action::MoveMidiCC { cc_id: 0, position: 2.5 });
        assert!((app.midi_cc[0].position - 2.5).abs() < 0.001);
    }

    #[test]
    fn move_midi_cc_clamps_negative() {
        let mut app = fresh();
        let (_tid, cid) = add_midi_clip(&mut app);
        app.apply(Action::AddMidiCC { clip_id: cid, controller: 1, position: 1.0, value: 64 });
        app.apply(Action::MoveMidiCC { cc_id: 0, position: -1.0 });
        assert_eq!(app.midi_cc[0].position, 0.0);
    }

    #[test]
    fn multiple_cc_events_on_nonexistent_clip_still_stored() {
        // CC doesn't validate clip_id — it's a reference, not enforced
        let mut app = fresh();
        app.apply(Action::AddMidiCC { clip_id: 9999, controller: 11, position: 0.0, value: 80 });
        assert_eq!(app.midi_cc.len(), 1);
        assert_eq!(app.midi_cc[0].clip_id, 9999);
    }
}
