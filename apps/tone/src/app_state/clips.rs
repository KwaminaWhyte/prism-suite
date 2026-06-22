//! Clips domain — `ClipKind`, `ToneClip` types + clip apply methods + tests.

// ─── Types ────────────────────────────────────────────────────────────────────

/// The kind of a clip on the timeline.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClipKind {
    /// A recorded or imported audio waveform.
    Audio,
    /// A block of MIDI note data.
    Midi,
    /// A clip whose content was synthesised by the AI engine.
    AiGenerated,
}

/// A clip placed on a track in the timeline.
#[derive(Clone, Debug)]
pub struct ToneClip {
    pub id: usize,
    pub track_id: usize,
    pub name: String,
    pub kind: ClipKind,
    /// Start position in beats from the project start.
    pub start_beat: f32,
    /// Length in beats.
    pub duration_beats: f32,
    /// Absolute path to the audio file for `Audio` clips.
    pub source_path: Option<String>,
    /// Clip gain multiplier. 0.0 = silence, 1.0 = unity, 4.0 = +12 dB. Clamped 0.0..=4.0.
    pub gain: f32,
    /// Pitch shift in semitones. Clamped -24..=24.
    pub pitch_shift: i8,
    /// Time-stretch ratio. 0.5 = half-speed, 1.0 = normal, 2.0 = double-speed. Clamped 0.5..=2.0.
    pub time_stretch: f32,
    /// When true, the clip plays back on loop.
    pub looping: bool,
    /// When true, the clip is excluded from the mix.
    pub muted: bool,
    /// Display colour (CSS hex string).
    pub color: String,
    /// The text prompt used to generate this clip (AI clips only).
    pub ai_prompt: Option<String>,
    /// Normalized peak values for waveform display, one per display column.
    pub peak_cache: Vec<f32>,
    /// When true, the peak cache needs to be recomputed.
    pub peak_cache_dirty: bool,
    /// Fine-grained pitch shift in fractional semitones. Clamped -24.0..=24.0.
    pub pitch_shift_f32: f32,
}

impl ToneClip {
    pub fn new(id: usize, track_id: usize, name: impl Into<String>, kind: ClipKind, start_beat: f32, duration_beats: f32) -> Self {
        Self {
            id,
            track_id,
            name: name.into(),
            kind,
            start_beat,
            duration_beats,
            source_path: None,
            gain: 1.0,
            pitch_shift: 0,
            time_stretch: 1.0,
            looping: false,
            muted: false,
            color: "#8B5CF6".to_string(),
            ai_prompt: None,
            peak_cache: Vec::new(),
            peak_cache_dirty: true,
            pitch_shift_f32: 0.0,
        }
    }
}

// ─── Apply methods ────────────────────────────────────────────────────────────

use super::{Action, App};

impl App {
    pub(super) fn apply_clips(&mut self, action: Action) {
        match action {
            Action::AddClip { track_id, name, kind, start_beat, duration_beats } => {
                let id = self.next_clip_id();
                let clip = ToneClip::new(id, track_id, name, kind, start_beat, duration_beats);
                self.clips.push(clip);
            }
            Action::DeleteClip(id) => {
                self.midi_notes.retain(|n| n.clip_id != id);
                self.clips.retain(|c| c.id != id);
                if self.piano_roll_clip == Some(id) {
                    self.piano_roll_clip = None;
                }
            }
            Action::MoveClip { id, track_id, start_beat } => {
                if let Some(c) = self.find_clip_mut(id) {
                    c.track_id = track_id;
                    c.start_beat = start_beat.max(0.0);
                }
            }
            Action::ResizeClip { id, duration_beats } => {
                if let Some(c) = self.find_clip_mut(id) {
                    c.duration_beats = duration_beats.max(0.0625); // minimum 1/16th beat
                }
            }
            Action::SetClipGain { id, gain } => {
                if let Some(c) = self.find_clip_mut(id) {
                    c.gain = gain.clamp(0.0, 4.0);
                }
            }
            Action::SetClipPitchShift { id, semitones } => {
                if let Some(c) = self.find_clip_mut(id) {
                    c.pitch_shift = semitones.clamp(-24, 24);
                }
            }
            Action::SetClipTimeStretch { id, ratio } => {
                if let Some(c) = self.find_clip_mut(id) {
                    c.time_stretch = ratio.clamp(0.5, 2.0);
                }
            }
            Action::SetClipLoop { id, looping } => {
                if let Some(c) = self.find_clip_mut(id) {
                    c.looping = looping;
                }
            }
            Action::SetClipMute { id, muted } => {
                if let Some(c) = self.find_clip_mut(id) {
                    c.muted = muted;
                }
            }
            Action::SplitClip { id, at_beat } => {
                let Some(src) = self.clips.iter().find(|c| c.id == id).cloned() else { return };
                let at = at_beat.clamp(src.start_beat + 0.0625, src.start_beat + src.duration_beats - 0.0625);
                // Truncate original
                if let Some(c) = self.find_clip_mut(id) {
                    c.duration_beats = at - src.start_beat;
                }
                // Create tail clip
                let new_id = self.next_clip_id();
                let tail_start = at;
                let tail_dur = (src.start_beat + src.duration_beats) - at;
                let mut tail = src.clone();
                tail.id = new_id;
                tail.start_beat = tail_start;
                tail.duration_beats = tail_dur;
                tail.name = format!("{} (split)", tail.name);
                // Move notes that fall in the tail to the new clip
                for note in self.midi_notes.iter_mut() {
                    if note.clip_id == id && note.start_beat >= (at - src.start_beat) {
                        note.clip_id = new_id;
                        note.start_beat -= at - src.start_beat;
                    }
                }
                self.clips.push(tail);
            }
            Action::MergeClips { ids } => {
                if ids.is_empty() { return; }
                let to_merge: Vec<ToneClip> = self.clips.iter()
                    .filter(|c| ids.contains(&c.id))
                    .cloned()
                    .collect();
                if to_merge.is_empty() { return; }
                let earliest = to_merge.iter().map(|c| c.start_beat).fold(f32::MAX, f32::min);
                let latest_end = to_merge.iter().map(|c| c.start_beat + c.duration_beats).fold(0.0f32, f32::max);
                let survivor_id = to_merge.iter().min_by(|a, b| a.start_beat.partial_cmp(&b.start_beat).unwrap()).map(|c| c.id).unwrap();
                let delete_ids: Vec<usize> = ids.iter().copied().filter(|&i| i != survivor_id).collect();
                for did in &delete_ids {
                    let offset = to_merge.iter().find(|c| c.id == *did).map(|c| c.start_beat - earliest).unwrap_or(0.0);
                    for note in self.midi_notes.iter_mut() {
                        if note.clip_id == *did {
                            note.clip_id = survivor_id;
                            note.start_beat += offset;
                        }
                    }
                }
                self.clips.retain(|c| !delete_ids.contains(&c.id));
                if let Some(c) = self.find_clip_mut(survivor_id) {
                    c.start_beat = earliest;
                    c.duration_beats = latest_end - earliest;
                }
            }
            Action::InvalidatePeakCache { clip_id } => {
                if let Some(c) = self.find_clip_mut(clip_id) {
                    c.peak_cache_dirty = true;
                }
            }
            // ── New clip operations ───────────────────────────────────────────
            Action::DuplicateClip { clip_id } => {
                let Some(src) = self.clips.iter().find(|c| c.id == clip_id).cloned() else { return };
                let new_id = self.next_clip_id();
                let mut dup = src.clone();
                dup.id = new_id;
                dup.start_beat = src.start_beat + src.duration_beats;
                self.clips.push(dup);
            }
            Action::ConsolidateClips { clip_ids } => {
                if clip_ids.is_empty() { return; }
                let to_merge: Vec<ToneClip> = self.clips.iter()
                    .filter(|c| clip_ids.contains(&c.id))
                    .cloned()
                    .collect();
                if to_merge.is_empty() { return; }
                let earliest = to_merge.iter().map(|c| c.start_beat).fold(f32::MAX, f32::min);
                let latest_end = to_merge.iter().map(|c| c.start_beat + c.duration_beats).fold(0.0f32, f32::max);
                let survivor_id = to_merge.iter()
                    .min_by(|a, b| a.start_beat.partial_cmp(&b.start_beat).unwrap())
                    .map(|c| c.id)
                    .unwrap();
                let delete_ids: Vec<usize> = clip_ids.iter().copied().filter(|&i| i != survivor_id).collect();
                for did in &delete_ids {
                    let offset = to_merge.iter().find(|c| c.id == *did).map(|c| c.start_beat - earliest).unwrap_or(0.0);
                    for note in self.midi_notes.iter_mut() {
                        if note.clip_id == *did {
                            note.clip_id = survivor_id;
                            note.start_beat += offset;
                        }
                    }
                }
                self.clips.retain(|c| !delete_ids.contains(&c.id));
                if let Some(c) = self.find_clip_mut(survivor_id) {
                    c.start_beat = earliest;
                    c.duration_beats = latest_end - earliest;
                }
            }
            Action::SetClipColor { clip_id, color } => {
                if let Some(c) = self.find_clip_mut(clip_id) {
                    c.color = color;
                }
            }
            Action::TrimClipStart { clip_id, new_start } => {
                let Some(c) = self.clips.iter().find(|c| c.id == clip_id).cloned() else { return };
                let clip_end = c.start_beat + c.duration_beats;
                if new_start >= 0.0 && new_start < clip_end {
                    if let Some(clip) = self.find_clip_mut(clip_id) {
                        let trimmed = new_start - c.start_beat;
                        clip.duration_beats -= trimmed;
                        clip.start_beat = new_start;
                    }
                }
            }
            Action::TrimClipEnd { clip_id, new_end } => {
                let Some(c) = self.clips.iter().find(|c| c.id == clip_id).cloned() else { return };
                if new_end > c.start_beat {
                    if let Some(clip) = self.find_clip_mut(clip_id) {
                        clip.duration_beats = new_end - c.start_beat;
                    }
                }
            }
            Action::SetClipPitchF32 { clip_id, semitones } => {
                if let Some(c) = self.find_clip_mut(clip_id) {
                    c.pitch_shift_f32 = semitones.clamp(-24.0, 24.0);
                }
            }
            Action::SetClipGainDb { clip_id, gain_db } => {
                if let Some(c) = self.find_clip_mut(clip_id) {
                    let clamped_db = gain_db.clamp(-60.0, 12.0);
                    c.gain = 10f32.powf(clamped_db / 20.0);
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
    use super::ClipKind;

    fn fresh() -> App {
        App::new()
    }

    #[test]
    fn add_clip() {
        let mut app = fresh();
        app.apply(Action::AddTrack(TrackKind::Audio));
        let tid = app.tracks.last().unwrap().id;
        app.apply(Action::AddClip { track_id: tid, name: "take1".into(), kind: ClipKind::Audio, start_beat: 0.0, duration_beats: 8.0 });
        assert_eq!(app.clips.len(), 1);
        assert_eq!(app.clips[0].duration_beats, 8.0);
    }

    #[test]
    fn delete_clip_removes_notes() {
        let mut app = fresh();
        app.apply(Action::AddTrack(TrackKind::Midi));
        let tid = app.tracks.last().unwrap().id;
        app.apply(Action::AddClip { track_id: tid, name: "melody".into(), kind: ClipKind::Midi, start_beat: 0.0, duration_beats: 4.0 });
        let cid = app.clips[0].id;
        app.apply(Action::AddMidiNote { clip_id: cid, pitch: 60, velocity: 100, start_beat: 0.0, duration_beats: 1.0 });
        assert!(!app.midi_notes.is_empty());
        app.apply(Action::DeleteClip(cid));
        assert!(app.clips.is_empty());
        assert!(app.midi_notes.is_empty());
    }

    #[test]
    fn move_clip() {
        let mut app = fresh();
        app.apply(Action::AddTrack(TrackKind::Audio));
        let tid = app.tracks.last().unwrap().id;
        app.apply(Action::AddClip { track_id: tid, name: "c".into(), kind: ClipKind::Audio, start_beat: 0.0, duration_beats: 4.0 });
        let cid = app.clips[0].id;
        app.apply(Action::MoveClip { id: cid, track_id: tid, start_beat: 8.0 });
        assert_eq!(app.clips[0].start_beat, 8.0);
    }

    #[test]
    fn move_clip_clamp_negative_start() {
        let mut app = fresh();
        app.apply(Action::AddTrack(TrackKind::Audio));
        let tid = app.tracks.last().unwrap().id;
        app.apply(Action::AddClip { track_id: tid, name: "c".into(), kind: ClipKind::Audio, start_beat: 4.0, duration_beats: 4.0 });
        let cid = app.clips[0].id;
        app.apply(Action::MoveClip { id: cid, track_id: tid, start_beat: -2.0 });
        assert_eq!(app.clips[0].start_beat, 0.0);
    }

    #[test]
    fn resize_clip_minimum_enforced() {
        let mut app = fresh();
        app.apply(Action::AddTrack(TrackKind::Audio));
        let tid = app.tracks.last().unwrap().id;
        app.apply(Action::AddClip { track_id: tid, name: "c".into(), kind: ClipKind::Audio, start_beat: 0.0, duration_beats: 4.0 });
        let cid = app.clips[0].id;
        app.apply(Action::ResizeClip { id: cid, duration_beats: 0.0 });
        assert!(app.clips[0].duration_beats > 0.0);
    }

    #[test]
    fn set_clip_gain_clamp() {
        let mut app = fresh();
        app.apply(Action::AddTrack(TrackKind::Audio));
        let tid = app.tracks.last().unwrap().id;
        app.apply(Action::AddClip { track_id: tid, name: "c".into(), kind: ClipKind::Audio, start_beat: 0.0, duration_beats: 4.0 });
        let cid = app.clips[0].id;
        app.apply(Action::SetClipGain { id: cid, gain: 10.0 });
        assert_eq!(app.clips[0].gain, 4.0);
    }

    #[test]
    fn set_clip_pitch_shift_clamp() {
        let mut app = fresh();
        app.apply(Action::AddTrack(TrackKind::Audio));
        let tid = app.tracks.last().unwrap().id;
        app.apply(Action::AddClip { track_id: tid, name: "c".into(), kind: ClipKind::Audio, start_beat: 0.0, duration_beats: 4.0 });
        let cid = app.clips[0].id;
        app.apply(Action::SetClipPitchShift { id: cid, semitones: 30 });
        assert_eq!(app.clips[0].pitch_shift, 24);
        app.apply(Action::SetClipPitchShift { id: cid, semitones: -30 });
        assert_eq!(app.clips[0].pitch_shift, -24);
    }

    #[test]
    fn set_clip_time_stretch_clamp() {
        let mut app = fresh();
        app.apply(Action::AddTrack(TrackKind::Audio));
        let tid = app.tracks.last().unwrap().id;
        app.apply(Action::AddClip { track_id: tid, name: "c".into(), kind: ClipKind::Audio, start_beat: 0.0, duration_beats: 4.0 });
        let cid = app.clips[0].id;
        app.apply(Action::SetClipTimeStretch { id: cid, ratio: 5.0 });
        assert_eq!(app.clips[0].time_stretch, 2.0);
        app.apply(Action::SetClipTimeStretch { id: cid, ratio: 0.1 });
        assert_eq!(app.clips[0].time_stretch, 0.5);
    }

    #[test]
    fn set_clip_loop() {
        let mut app = fresh();
        app.apply(Action::AddTrack(TrackKind::Audio));
        let tid = app.tracks.last().unwrap().id;
        app.apply(Action::AddClip { track_id: tid, name: "c".into(), kind: ClipKind::Audio, start_beat: 0.0, duration_beats: 4.0 });
        let cid = app.clips[0].id;
        app.apply(Action::SetClipLoop { id: cid, looping: true });
        assert!(app.clips[0].looping);
    }

    #[test]
    fn set_clip_mute() {
        let mut app = fresh();
        app.apply(Action::AddTrack(TrackKind::Audio));
        let tid = app.tracks.last().unwrap().id;
        app.apply(Action::AddClip { track_id: tid, name: "c".into(), kind: ClipKind::Audio, start_beat: 0.0, duration_beats: 4.0 });
        let cid = app.clips[0].id;
        app.apply(Action::SetClipMute { id: cid, muted: true });
        assert!(app.clips[0].muted);
    }

    #[test]
    fn split_clip_creates_two() {
        let mut app = fresh();
        app.apply(Action::AddTrack(TrackKind::Audio));
        let tid = app.tracks.last().unwrap().id;
        app.apply(Action::AddClip { track_id: tid, name: "take".into(), kind: ClipKind::Audio, start_beat: 0.0, duration_beats: 8.0 });
        let cid = app.clips[0].id;
        app.apply(Action::SplitClip { id: cid, at_beat: 4.0 });
        assert_eq!(app.clips.len(), 2);
        let left = app.clips.iter().find(|c| c.id == cid).unwrap();
        let right = app.clips.iter().find(|c| c.id != cid).unwrap();
        assert!((left.duration_beats - 4.0).abs() < 0.001);
        assert!((right.start_beat - 4.0).abs() < 0.001);
        assert!((right.duration_beats - 4.0).abs() < 0.001);
    }

    #[test]
    fn merge_clips_spans_range() {
        let mut app = fresh();
        app.apply(Action::AddTrack(TrackKind::Audio));
        let tid = app.tracks.last().unwrap().id;
        app.apply(Action::AddClip { track_id: tid, name: "a".into(), kind: ClipKind::Audio, start_beat: 0.0, duration_beats: 4.0 });
        app.apply(Action::AddClip { track_id: tid, name: "b".into(), kind: ClipKind::Audio, start_beat: 4.0, duration_beats: 4.0 });
        let ids: Vec<usize> = app.clips.iter().map(|c| c.id).collect();
        app.apply(Action::MergeClips { ids });
        assert_eq!(app.clips.len(), 1);
        assert_eq!(app.clips[0].duration_beats, 8.0);
    }

    #[test]
    fn delete_clip_clears_piano_roll() {
        let mut app = fresh();
        app.apply(Action::AddTrack(TrackKind::Midi));
        let tid = app.tracks.last().unwrap().id;
        app.apply(Action::AddClip { track_id: tid, name: "m".into(), kind: ClipKind::Midi, start_beat: 0.0, duration_beats: 4.0 });
        let cid = app.clips[0].id;
        app.apply(Action::OpenPianoRoll(cid));
        app.apply(Action::DeleteClip(cid));
        assert_eq!(app.piano_roll_clip, None);
    }

    #[test]
    fn split_then_merge_round_trips() {
        let mut app = fresh();
        app.apply(Action::AddTrack(TrackKind::Audio));
        let tid = app.tracks.last().unwrap().id;
        app.apply(Action::AddClip { track_id: tid, name: "loop".into(), kind: ClipKind::Audio, start_beat: 0.0, duration_beats: 16.0 });
        let cid = app.clips[0].id;
        app.apply(Action::SplitClip { id: cid, at_beat: 8.0 });
        assert_eq!(app.clips.len(), 2);
        let ids: Vec<usize> = app.clips.iter().map(|c| c.id).collect();
        app.apply(Action::MergeClips { ids });
        assert_eq!(app.clips.len(), 1);
        assert_eq!(app.clips[0].duration_beats, 16.0);
    }

    #[test]
    fn clip_default_peak_cache_dirty() {
        let mut app = fresh();
        app.apply(Action::AddTrack(TrackKind::Audio));
        let tid = app.tracks.last().unwrap().id;
        app.apply(Action::AddClip { track_id: tid, name: "c".into(), kind: ClipKind::Audio, start_beat: 0.0, duration_beats: 4.0 });
        assert!(app.clips[0].peak_cache_dirty);
    }

    #[test]
    fn invalidate_peak_cache() {
        let mut app = fresh();
        app.apply(Action::AddTrack(TrackKind::Audio));
        let tid = app.tracks.last().unwrap().id;
        app.apply(Action::AddClip { track_id: tid, name: "c".into(), kind: ClipKind::Audio, start_beat: 0.0, duration_beats: 4.0 });
        let cid = app.clips[0].id;
        // Manually clear dirty flag as if we computed the cache
        app.clips[0].peak_cache_dirty = false;
        // Now invalidate
        app.apply(Action::InvalidatePeakCache { clip_id: cid });
        assert!(app.clips[0].peak_cache_dirty);
    }

    #[test]
    fn invalidate_peak_cache_nonexistent_clip_is_noop() {
        let mut app = fresh();
        // Should not panic
        app.apply(Action::InvalidatePeakCache { clip_id: 9999 });
    }

    // ── New clip operation tests ───────────────────────────────────────────────

    #[test]
    fn duplicate_clip_appears_at_correct_position() {
        let mut app = fresh();
        app.apply(Action::AddTrack(TrackKind::Audio));
        let tid = app.tracks.last().unwrap().id;
        app.apply(Action::AddClip { track_id: tid, name: "c".into(), kind: ClipKind::Audio, start_beat: 4.0, duration_beats: 8.0 });
        let cid = app.clips[0].id;
        app.apply(Action::DuplicateClip { clip_id: cid });
        assert_eq!(app.clips.len(), 2);
        let dup = app.clips.iter().find(|c| c.id != cid).unwrap();
        assert!((dup.start_beat - 12.0).abs() < 0.001);
    }

    #[test]
    fn duplicate_clip_has_new_id() {
        let mut app = fresh();
        app.apply(Action::AddTrack(TrackKind::Audio));
        let tid = app.tracks.last().unwrap().id;
        app.apply(Action::AddClip { track_id: tid, name: "c".into(), kind: ClipKind::Audio, start_beat: 0.0, duration_beats: 4.0 });
        let cid = app.clips[0].id;
        app.apply(Action::DuplicateClip { clip_id: cid });
        assert_eq!(app.clips.len(), 2);
        assert_ne!(app.clips[0].id, app.clips[1].id);
    }

    #[test]
    fn consolidate_clips() {
        let mut app = fresh();
        app.apply(Action::AddTrack(TrackKind::Audio));
        let tid = app.tracks.last().unwrap().id;
        app.apply(Action::AddClip { track_id: tid, name: "a".into(), kind: ClipKind::Audio, start_beat: 0.0, duration_beats: 4.0 });
        app.apply(Action::AddClip { track_id: tid, name: "b".into(), kind: ClipKind::Audio, start_beat: 6.0, duration_beats: 4.0 });
        let ids: Vec<usize> = app.clips.iter().map(|c| c.id).collect();
        app.apply(Action::ConsolidateClips { clip_ids: ids });
        assert_eq!(app.clips.len(), 1);
        assert_eq!(app.clips[0].duration_beats, 10.0);
    }

    #[test]
    fn set_clip_color() {
        let mut app = fresh();
        app.apply(Action::AddTrack(TrackKind::Audio));
        let tid = app.tracks.last().unwrap().id;
        app.apply(Action::AddClip { track_id: tid, name: "c".into(), kind: ClipKind::Audio, start_beat: 0.0, duration_beats: 4.0 });
        let cid = app.clips[0].id;
        app.apply(Action::SetClipColor { clip_id: cid, color: "#FF5500".to_string() });
        assert_eq!(app.clips[0].color, "#FF5500");
    }

    #[test]
    fn trim_clip_start() {
        let mut app = fresh();
        app.apply(Action::AddTrack(TrackKind::Audio));
        let tid = app.tracks.last().unwrap().id;
        app.apply(Action::AddClip { track_id: tid, name: "c".into(), kind: ClipKind::Audio, start_beat: 0.0, duration_beats: 8.0 });
        let cid = app.clips[0].id;
        app.apply(Action::TrimClipStart { clip_id: cid, new_start: 2.0 });
        assert!((app.clips[0].start_beat - 2.0).abs() < 0.001);
        assert!((app.clips[0].duration_beats - 6.0).abs() < 0.001);
    }

    #[test]
    fn trim_clip_start_bounds_check_past_end() {
        let mut app = fresh();
        app.apply(Action::AddTrack(TrackKind::Audio));
        let tid = app.tracks.last().unwrap().id;
        app.apply(Action::AddClip { track_id: tid, name: "c".into(), kind: ClipKind::Audio, start_beat: 0.0, duration_beats: 4.0 });
        let cid = app.clips[0].id;
        // new_start past end should be ignored
        app.apply(Action::TrimClipStart { clip_id: cid, new_start: 10.0 });
        assert!((app.clips[0].start_beat - 0.0).abs() < 0.001);
    }

    #[test]
    fn trim_clip_end() {
        let mut app = fresh();
        app.apply(Action::AddTrack(TrackKind::Audio));
        let tid = app.tracks.last().unwrap().id;
        app.apply(Action::AddClip { track_id: tid, name: "c".into(), kind: ClipKind::Audio, start_beat: 0.0, duration_beats: 8.0 });
        let cid = app.clips[0].id;
        app.apply(Action::TrimClipEnd { clip_id: cid, new_end: 6.0 });
        assert!((app.clips[0].duration_beats - 6.0).abs() < 0.001);
    }

    #[test]
    fn trim_clip_end_bounds_check_before_start() {
        let mut app = fresh();
        app.apply(Action::AddTrack(TrackKind::Audio));
        let tid = app.tracks.last().unwrap().id;
        app.apply(Action::AddClip { track_id: tid, name: "c".into(), kind: ClipKind::Audio, start_beat: 4.0, duration_beats: 4.0 });
        let cid = app.clips[0].id;
        // new_end before clip start should be ignored
        app.apply(Action::TrimClipEnd { clip_id: cid, new_end: 2.0 });
        assert!((app.clips[0].duration_beats - 4.0).abs() < 0.001);
    }

    #[test]
    fn set_clip_pitch_f32_clamp() {
        let mut app = fresh();
        app.apply(Action::AddTrack(TrackKind::Audio));
        let tid = app.tracks.last().unwrap().id;
        app.apply(Action::AddClip { track_id: tid, name: "c".into(), kind: ClipKind::Audio, start_beat: 0.0, duration_beats: 4.0 });
        let cid = app.clips[0].id;
        app.apply(Action::SetClipPitchF32 { clip_id: cid, semitones: 30.0 });
        assert_eq!(app.clips[0].pitch_shift_f32, 24.0);
        app.apply(Action::SetClipPitchF32 { clip_id: cid, semitones: -30.0 });
        assert_eq!(app.clips[0].pitch_shift_f32, -24.0);
    }

    #[test]
    fn set_clip_gain_db_conversion() {
        let mut app = fresh();
        app.apply(Action::AddTrack(TrackKind::Audio));
        let tid = app.tracks.last().unwrap().id;
        app.apply(Action::AddClip { track_id: tid, name: "c".into(), kind: ClipKind::Audio, start_beat: 0.0, duration_beats: 4.0 });
        let cid = app.clips[0].id;
        // 0 dB = unity gain
        app.apply(Action::SetClipGainDb { clip_id: cid, gain_db: 0.0 });
        assert!((app.clips[0].gain - 1.0).abs() < 0.001);
    }

    #[test]
    fn set_clip_gain_db_clamp() {
        let mut app = fresh();
        app.apply(Action::AddTrack(TrackKind::Audio));
        let tid = app.tracks.last().unwrap().id;
        app.apply(Action::AddClip { track_id: tid, name: "c".into(), kind: ClipKind::Audio, start_beat: 0.0, duration_beats: 4.0 });
        let cid = app.clips[0].id;
        // Above max (+12 dB) should clamp
        app.apply(Action::SetClipGainDb { clip_id: cid, gain_db: 100.0 });
        let expected_max = 10f32.powf(12.0 / 20.0);
        assert!((app.clips[0].gain - expected_max).abs() < 0.01);
    }

    #[test]
    fn clip_pitch_shift_f32_default() {
        let mut app = fresh();
        app.apply(Action::AddTrack(TrackKind::Audio));
        let tid = app.tracks.last().unwrap().id;
        app.apply(Action::AddClip { track_id: tid, name: "c".into(), kind: ClipKind::Audio, start_beat: 0.0, duration_beats: 4.0 });
        assert_eq!(app.clips[0].pitch_shift_f32, 0.0);
    }
}
