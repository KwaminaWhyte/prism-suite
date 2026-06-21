//! AI generation domain — `AiGenerationJob`, `AiGenerationStatus` types + AI
//! apply methods + tests.

// ─── Types ────────────────────────────────────────────────────────────────────

/// Current state of an AI generation job.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AiGenerationStatus {
    /// No job running.
    Idle,
    /// The AI engine is currently synthesising audio/MIDI.
    Generating,
    /// Generation completed successfully.
    Done,
    /// Generation failed (model error, insufficient memory, etc.).
    Failed,
}

/// A single AI music generation request and its lifecycle.
#[derive(Clone, Debug)]
pub struct AiGenerationJob {
    pub id: usize,
    /// Free-text prompt describing the desired music ("lo-fi hip hop drums at 90 BPM").
    pub prompt: String,
    /// Style tag used to select the right model checkpoint.
    pub style: String,
    /// Requested output length in bars.
    pub duration_bars: u8,
    pub status: AiGenerationStatus,
    /// The clip that was created when the job completed.
    pub output_clip_id: Option<usize>,
    /// Requested stem outputs, e.g. ["drums", "bass", "melody", "harmony"].
    pub stems: Vec<String>,
}

// ─── Apply methods ────────────────────────────────────────────────────────────

use super::{Action, App};
use super::clips::{ClipKind, ToneClip};
use super::midi::MidiNote;

impl App {
    pub(super) fn apply_ai(&mut self, action: Action) {
        match action {
            Action::SetAiPrompt(prompt) => {
                self.ai_prompt = prompt;
            }
            Action::SetAiStyle(style) => {
                self.ai_style = style;
            }
            Action::SetAiDurationBars(bars) => {
                self.ai_duration_bars = bars.clamp(1, 128);
            }
            Action::GenerateTrack { track_id } => {
                let job_id = self.ai_job_counter;
                self.ai_job_counter += 1;
                self.ai_jobs.push(AiGenerationJob {
                    id: job_id,
                    prompt: self.ai_prompt.clone(),
                    style: self.ai_style.clone(),
                    duration_bars: self.ai_duration_bars,
                    status: AiGenerationStatus::Generating,
                    output_clip_id: None,
                    stems: vec![
                        "drums".to_string(),
                        "bass".to_string(),
                        "melody".to_string(),
                        "harmony".to_string(),
                    ],
                });
                // Stub a placeholder clip while generating
                let clip_id = self.next_clip_id();
                let beats = self.ai_duration_bars as f32 * self.project.time_signature_num as f32;
                let start = self.playhead_beat;
                let mut clip = ToneClip::new(clip_id, track_id, format!("AI: {}", self.ai_prompt), ClipKind::AiGenerated, start, beats);
                clip.ai_prompt = Some(self.ai_prompt.clone());
                if let Some(j) = self.ai_jobs.last_mut() {
                    j.output_clip_id = Some(clip_id);
                }
                self.clips.push(clip);
            }
            Action::CompleteAiGeneration { job_id } => {
                if let Some(j) = self.ai_jobs.iter_mut().find(|j| j.id == job_id) {
                    j.status = AiGenerationStatus::Done;
                }
            }
            Action::GenerateChordProgression { track_id, bars } => {
                let clip_id = self.next_clip_id();
                let beats = bars as f32 * self.project.time_signature_num as f32;
                let start = self.playhead_beat;
                let clip = ToneClip::new(clip_id, track_id, "Chord Progression".to_string(), ClipKind::Midi, start, beats);
                self.clips.push(clip);
                // Stub Cmaj7 chord: C4 E4 G4 B4 (pitches 60, 64, 67, 71)
                for &pitch in &[60u8, 64, 67, 71] {
                    let nid = self.next_note_id();
                    self.midi_notes.push(MidiNote { id: nid, clip_id, pitch, velocity: 90, start_beat: 0.0, duration_beats: beats });
                }
            }
            Action::GenerateDrumPattern { track_id } => {
                let clip_id = self.next_clip_id();
                let clip = ToneClip::new(clip_id, track_id, "Drum Pattern".to_string(), ClipKind::Midi, self.playhead_beat, 4.0);
                self.clips.push(clip);
                // 4/4 16-step pattern: kick (36) on 0, 2 beats; snare (38) on 1, 3; hi-hat (42) every 0.5
                for (beat, pitch) in [
                    (0.0f32, 36u8), (2.0, 36), // kick
                    (1.0, 38), (3.0, 38),       // snare
                ] {
                    let nid = self.next_note_id();
                    self.midi_notes.push(MidiNote { id: nid, clip_id, pitch, velocity: 100, start_beat: beat, duration_beats: 0.25 });
                }
                // Hi-hats every 8th note
                let mut hh = 0.0f32;
                while hh < 4.0 {
                    let nid = self.next_note_id();
                    self.midi_notes.push(MidiNote { id: nid, clip_id, pitch: 42, velocity: 70, start_beat: hh, duration_beats: 0.125 });
                    hh += 0.5;
                }
            }
            Action::HarmonizeMelody { track_id } => {
                let clip_ids: Vec<usize> = self.clips.iter()
                    .filter(|c| c.track_id == track_id && matches!(c.kind, ClipKind::Midi | ClipKind::AiGenerated))
                    .map(|c| c.id)
                    .collect();
                let src_notes: Vec<MidiNote> = self.midi_notes.iter()
                    .filter(|n| clip_ids.contains(&n.clip_id))
                    .cloned()
                    .collect();
                for sn in src_notes {
                    let new_pitch = (sn.pitch as i16 + 4).clamp(0, 127) as u8;
                    let nid = self.next_note_id();
                    self.midi_notes.push(MidiNote {
                        id: nid,
                        clip_id: sn.clip_id,
                        pitch: new_pitch,
                        velocity: (sn.velocity as f32 * 0.8) as u8,
                        start_beat: sn.start_beat,
                        duration_beats: sn.duration_beats,
                    });
                }
            }
            Action::AiMasterTrack => {
                self.master_volume = 0.9;
                self.master_limiter = true;
                for m in self.mixer_channels.iter_mut() {
                    m.comp_enabled = true;
                    if m.comp_threshold > -6.0 {
                        m.comp_threshold = -6.0;
                    }
                }
            }
            Action::SuggestChords { key, mood } => {
                let clip_id = if let Some(cid) = self.piano_roll_clip {
                    cid
                } else if let Some(tid) = self.active_track {
                    self.clips.iter()
                        .filter(|c| c.track_id == tid && matches!(c.kind, ClipKind::Midi | ClipKind::AiGenerated))
                        .last()
                        .map(|c| c.id)
                        .unwrap_or_else(|| {
                            let cid = self.clip_counter;
                            self.clip_counter += 1;
                            let cl = ToneClip::new(cid, tid, format!("{} {} chords", key, mood), ClipKind::Midi, self.playhead_beat, 4.0);
                            self.clips.push(cl);
                            cid
                        })
                } else {
                    return;
                };
                let base: u8 = 60;
                for (i, &interval) in [0u8, 4, 7, 11].iter().enumerate() {
                    let nid = self.next_note_id();
                    self.midi_notes.push(MidiNote {
                        id: nid,
                        clip_id,
                        pitch: (base + interval).min(127),
                        velocity: 85,
                        start_beat: i as f32,
                        duration_beats: 1.0,
                    });
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
    use super::AiGenerationStatus;

    fn fresh() -> App {
        App::new()
    }

    #[test]
    fn set_ai_prompt() {
        let mut app = fresh();
        app.apply(Action::SetAiPrompt("chill lo-fi piano".to_string()));
        assert_eq!(app.ai_prompt, "chill lo-fi piano");
    }

    #[test]
    fn set_ai_style() {
        let mut app = fresh();
        app.apply(Action::SetAiStyle("Jazz".to_string()));
        assert_eq!(app.ai_style, "Jazz");
    }

    #[test]
    fn set_ai_duration_bars_clamp() {
        let mut app = fresh();
        app.apply(Action::SetAiDurationBars(0));
        assert_eq!(app.ai_duration_bars, 1);
        app.apply(Action::SetAiDurationBars(200));
        assert_eq!(app.ai_duration_bars, 128);
    }

    #[test]
    fn generate_track_creates_job_and_clip() {
        let mut app = fresh();
        app.apply(Action::AddTrack(TrackKind::Instrument));
        let tid = app.tracks.last().unwrap().id;
        app.apply(Action::SetAiPrompt("upbeat pop hook".to_string()));
        app.apply(Action::GenerateTrack { track_id: tid });
        assert_eq!(app.ai_jobs.len(), 1);
        assert_eq!(app.ai_jobs[0].status, AiGenerationStatus::Generating);
        assert_eq!(app.clips.len(), 1);
        assert_eq!(app.clips[0].kind, ClipKind::AiGenerated);
    }

    #[test]
    fn complete_ai_generation() {
        let mut app = fresh();
        app.apply(Action::AddTrack(TrackKind::Instrument));
        let tid = app.tracks.last().unwrap().id;
        app.apply(Action::GenerateTrack { track_id: tid });
        let job_id = app.ai_jobs[0].id;
        app.apply(Action::CompleteAiGeneration { job_id });
        assert_eq!(app.ai_jobs[0].status, AiGenerationStatus::Done);
    }

    #[test]
    fn generate_chord_progression() {
        let mut app = fresh();
        app.apply(Action::AddTrack(TrackKind::Midi));
        let tid = app.tracks.last().unwrap().id;
        app.apply(Action::GenerateChordProgression { track_id: tid, bars: 4 });
        assert_eq!(app.clips.len(), 1);
        assert_eq!(app.midi_notes.len(), 4); // Cmaj7 chord tones
        assert_eq!(app.midi_notes[0].pitch, 60); // C4
    }

    #[test]
    fn generate_drum_pattern_creates_notes() {
        let mut app = fresh();
        app.apply(Action::AddTrack(TrackKind::Midi));
        let tid = app.tracks.last().unwrap().id;
        app.apply(Action::GenerateDrumPattern { track_id: tid });
        assert_eq!(app.clips.len(), 1);
        assert!(app.midi_notes.len() >= 8);
    }

    #[test]
    fn generate_drum_pattern_has_kick_snare_hihat() {
        let mut app = fresh();
        app.apply(Action::AddTrack(TrackKind::Midi));
        let tid = app.tracks.last().unwrap().id;
        app.apply(Action::GenerateDrumPattern { track_id: tid });
        let pitches: Vec<u8> = app.midi_notes.iter().map(|n| n.pitch).collect();
        assert!(pitches.contains(&36), "no kick");
        assert!(pitches.contains(&38), "no snare");
        assert!(pitches.contains(&42), "no hihat");
    }

    #[test]
    fn harmonize_melody_doubles_notes() {
        let mut app = fresh();
        app.apply(Action::AddTrack(TrackKind::Midi));
        let tid = app.tracks.last().unwrap().id;
        app.apply(Action::AddClip { track_id: tid, name: "melody".into(), kind: ClipKind::Midi, start_beat: 0.0, duration_beats: 4.0 });
        let cid = app.clips[0].id;
        for pitch in [60u8, 62, 64, 65] {
            app.apply(Action::AddMidiNote { clip_id: cid, pitch, velocity: 80, start_beat: 0.0, duration_beats: 1.0 });
        }
        let before = app.midi_notes.len();
        app.apply(Action::HarmonizeMelody { track_id: tid });
        assert_eq!(app.midi_notes.len(), before * 2);
        let harmony_pitches: Vec<u8> = app.midi_notes[before..].iter().map(|n| n.pitch).collect();
        assert!(harmony_pitches.contains(&64)); // 60 + 4
    }

    #[test]
    fn ai_master_track() {
        let mut app = fresh();
        app.apply(Action::AiMasterTrack);
        assert_eq!(app.master_volume, 0.9);
        assert!(app.master_limiter);
    }

    #[test]
    fn suggest_chords_with_active_track() {
        let mut app = fresh();
        app.apply(Action::AddTrack(TrackKind::Midi));
        let tid = app.tracks.last().unwrap().id;
        app.apply(Action::SetActiveTrack(Some(tid)));
        app.apply(Action::AddClip { track_id: tid, name: "chord clip".into(), kind: ClipKind::Midi, start_beat: 0.0, duration_beats: 4.0 });
        let before = app.midi_notes.len();
        app.apply(Action::SuggestChords { key: "C".to_string(), mood: "happy".to_string() });
        assert!(app.midi_notes.len() > before);
    }

    #[test]
    fn ai_job_counter_increments() {
        let mut app = fresh();
        app.apply(Action::AddTrack(TrackKind::Instrument));
        let tid = app.tracks.last().unwrap().id;
        app.apply(Action::GenerateTrack { track_id: tid });
        app.apply(Action::GenerateTrack { track_id: tid });
        assert_eq!(app.ai_jobs.len(), 2);
        assert_ne!(app.ai_jobs[0].id, app.ai_jobs[1].id);
    }

    #[test]
    fn ai_workflow_end_to_end() {
        let mut app = fresh();
        app.apply(Action::AddTrack(TrackKind::Instrument));
        let tid = app.tracks.last().unwrap().id;
        app.apply(Action::SetAiPrompt("dark ambient drone".to_string()));
        app.apply(Action::SetAiStyle("Ambient".to_string()));
        app.apply(Action::SetAiDurationBars(16));
        app.apply(Action::GenerateTrack { track_id: tid });
        assert_eq!(app.ai_jobs[0].status, AiGenerationStatus::Generating);
        assert_eq!(app.ai_jobs[0].duration_bars, 16);
        let job_id = app.ai_jobs[0].id;
        app.apply(Action::CompleteAiGeneration { job_id });
        assert_eq!(app.ai_jobs[0].status, AiGenerationStatus::Done);
        app.apply(Action::AiMasterTrack);
        assert!(app.master_limiter);
        assert_eq!(app.master_volume, 0.9);
    }

    #[test]
    fn generate_track_stores_prompt_on_clip() {
        let mut app = fresh();
        app.apply(Action::AddTrack(TrackKind::Instrument));
        let tid = app.tracks.last().unwrap().id;
        app.apply(Action::SetAiPrompt("funky bass line".to_string()));
        app.apply(Action::GenerateTrack { track_id: tid });
        assert_eq!(app.clips[0].ai_prompt, Some("funky bass line".to_string()));
    }

    #[test]
    fn ai_job_has_four_stems() {
        let mut app = fresh();
        app.apply(Action::AddTrack(TrackKind::Instrument));
        let tid = app.tracks.last().unwrap().id;
        app.apply(Action::GenerateTrack { track_id: tid });
        assert_eq!(app.ai_jobs[0].stems.len(), 4);
    }
}
