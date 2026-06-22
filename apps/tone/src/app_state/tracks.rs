//! Tracks domain — `TrackKind`, `ToneTrack`, `TrackSend` types + track apply
//! methods + tests.

// ─── Types ────────────────────────────────────────────────────────────────────

/// The kind of a timeline track.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrackKind {
    /// Raw audio clips or recorded audio.
    Audio,
    /// MIDI data driving an external/virtual instrument.
    Midi,
    /// Built-in virtual instrument (combines MIDI + synthesis).
    Instrument,
    /// Submix/FX bus that other tracks route into.
    Bus,
    /// The single stereo master output track.
    Master,
}

impl TrackKind {
    pub fn label(self) -> &'static str {
        match self {
            TrackKind::Audio => "Audio",
            TrackKind::Midi => "MIDI",
            TrackKind::Instrument => "Instrument",
            TrackKind::Bus => "Bus",
            TrackKind::Master => "Master",
        }
    }
}

/// The specific kind of an insert effect.
#[derive(Clone, Debug)]
pub enum InsertEffectKind {
    Eq3Band,
    Compressor,
    Reverb { room_size: f32, wet: f32 },
    Delay { time_ms: f32, feedback: f32, wet: f32 },
    Chorus { rate: f32, depth: f32, wet: f32 },
    Distortion { drive: f32, tone: f32 },
    Gate { threshold_db: f32, attack_ms: f32, release_ms: f32 },
    Limiter { ceiling_db: f32 },
    PitchShift { semitones: f32 },
}

/// A single insert effect in a track's processing chain.
#[derive(Clone, Debug)]
pub struct InsertEffect {
    pub id: usize,
    pub effect_type: InsertEffectKind,
    pub enabled: bool,
    /// Pre-effect gain multiplier.
    pub gain_in: f32,
    /// Post-effect gain multiplier.
    pub gain_out: f32,
}

/// A single track in the Tone session.
#[derive(Clone, Debug)]
pub struct ToneTrack {
    pub id: usize,
    pub name: String,
    pub kind: TrackKind,
    /// Display color (CSS hex string, e.g. "#3B82F6").
    pub color: String,
    /// When true, this track produces no audio output.
    pub muted: bool,
    /// When true, only soloed tracks play.
    pub solo: bool,
    /// Record-arm: this track will capture incoming audio/MIDI when recording.
    pub armed: bool,
    /// Fader level. 0.0 = silence, 1.0 = 0 dB, 2.0 = +6 dB. Clamped 0.0..=2.0.
    pub volume: f32,
    /// Stereo pan. -1.0 = full left, 0.0 = centre, 1.0 = full right.
    pub pan: f32,
    /// Pre-fader send level to the global reverb bus (0.0..=1.0).
    pub send_to_reverb: f32,
    /// Pre-fader send level to the global delay bus (0.0..=1.0).
    pub send_to_delay: f32,
    /// For Instrument / MIDI tracks: the loaded instrument name (e.g. "Grand Piano").
    pub instrument: Option<String>,
    /// Insert effects chain for this track.
    pub insert_effects: Vec<InsertEffect>,
    /// Counter for assigning unique IDs to insert effects.
    pub next_insert_id: usize,
}

impl ToneTrack {
    pub fn new(id: usize, name: impl Into<String>, kind: TrackKind) -> Self {
        Self {
            id,
            name: name.into(),
            kind,
            color: "#3B82F6".to_string(),
            muted: false,
            solo: false,
            armed: false,
            volume: 1.0,
            pan: 0.0,
            send_to_reverb: 0.0,
            send_to_delay: 0.0,
            instrument: None,
            insert_effects: Vec::new(),
            next_insert_id: 0,
        }
    }
}

/// A send route from one track to a bus, for reverb/delay routing.
#[derive(Clone, Debug)]
pub struct TrackSend {
    pub from_track_id: usize,
    pub to_bus_id: usize,
    /// Send level, 0.0..=1.0.
    pub level: f32,
    pub enabled: bool,
}

// ─── Apply methods ────────────────────────────────────────────────────────────

use super::{Action, App};
use super::clips::ClipKind;
use super::mixer::MixerChannel;

impl App {
    pub(super) fn apply_tracks(&mut self, action: Action) {
        match action {
            Action::AddTrack(kind) => {
                let id = self.track_counter;
                self.track_counter += 1;
                let name = format!("{} {}", kind.label(), self.tracks.len() + 1);
                let track = ToneTrack::new(id, name, kind);
                let channel = MixerChannel::new(id);
                self.tracks.push(track);
                self.mixer_channels.push(channel);
                self.active_track = Some(id);
            }
            Action::DeleteTrack(id) => {
                self.tracks.retain(|t| t.id != id);
                self.mixer_channels.retain(|m| m.track_id != id);
                // Remove all clips and their notes
                let clip_ids: Vec<usize> = self.clips.iter()
                    .filter(|c| c.track_id == id)
                    .map(|c| c.id)
                    .collect();
                for cid in &clip_ids {
                    self.midi_notes.retain(|n| n.clip_id != *cid);
                }
                self.clips.retain(|c| c.track_id != id);
                self.track_sends.retain(|s| s.from_track_id != id && s.to_bus_id != id);
                if self.active_track == Some(id) {
                    self.active_track = None;
                }
            }
            Action::RenameTrack { id, name } => {
                if let Some(t) = self.find_track_mut(id) {
                    t.name = name;
                }
            }
            Action::SetTrackMute { id, muted } => {
                if let Some(t) = self.find_track_mut(id) {
                    t.muted = muted;
                }
            }
            Action::SetTrackSolo { id, solo } => {
                if let Some(t) = self.find_track_mut(id) {
                    t.solo = solo;
                }
            }
            Action::SetTrackArm { id, armed } => {
                if let Some(t) = self.find_track_mut(id) {
                    t.armed = armed;
                }
            }
            Action::SetTrackVolume { id, volume } => {
                if let Some(t) = self.find_track_mut(id) {
                    t.volume = volume.clamp(0.0, 2.0);
                }
            }
            Action::SetTrackPan { id, pan } => {
                if let Some(t) = self.find_track_mut(id) {
                    t.pan = pan.clamp(-1.0, 1.0);
                }
            }
            Action::SetTrackColor { id, color } => {
                if let Some(t) = self.find_track_mut(id) {
                    t.color = color;
                }
            }
            Action::SetTrackInstrument { id, instrument } => {
                if let Some(t) = self.find_track_mut(id) {
                    t.instrument = Some(instrument);
                }
            }
            Action::DuplicateTrack(id) => {
                let Some(src) = self.tracks.iter().find(|t| t.id == id).cloned() else { return };
                let new_id = self.track_counter;
                self.track_counter += 1;
                let mut dup = src.clone();
                dup.id = new_id;
                dup.name = format!("{} (copy)", dup.name);
                let src_clips: Vec<super::clips::ToneClip> = self.clips.iter()
                    .filter(|c| c.track_id == id)
                    .cloned()
                    .collect();
                let channel = MixerChannel::new(new_id);
                self.tracks.push(dup);
                self.mixer_channels.push(channel);
                for sc in src_clips {
                    let new_clip_id = self.next_clip_id();
                    let src_notes: Vec<super::midi::MidiNote> = self.midi_notes.iter()
                        .filter(|n| n.clip_id == sc.id)
                        .cloned()
                        .collect();
                    let mut new_clip = sc.clone();
                    new_clip.id = new_clip_id;
                    new_clip.track_id = new_id;
                    self.clips.push(new_clip);
                    for sn in src_notes {
                        let new_note_id = self.next_note_id();
                        let mut nn = sn.clone();
                        nn.id = new_note_id;
                        nn.clip_id = new_clip_id;
                        self.midi_notes.push(nn);
                    }
                }
                self.active_track = Some(new_id);
            }
            Action::ReorderTracks { from, to } => {
                if from < self.tracks.len() && to < self.tracks.len() && from != to {
                    let t = self.tracks.remove(from);
                    self.tracks.insert(to, t);
                }
            }
            Action::SetActiveTrack(id) => {
                self.active_track = id;
            }
            // ── Send routing ──────────────────────────────────────────────────
            Action::AddTrackSend { from_track_id, to_bus_id, level } => {
                let id = self.next_send_id;
                self.next_send_id += 1;
                self.track_sends.push(TrackSend {
                    from_track_id,
                    to_bus_id,
                    level: level.clamp(0.0, 1.0),
                    enabled: true,
                });
                let _ = id; // id stored implicitly by position (future: store in struct)
            }
            Action::RemoveTrackSend { send_id } => {
                if send_id < self.track_sends.len() {
                    self.track_sends.remove(send_id);
                }
            }
            Action::SetSendLevel { send_id, level } => {
                if let Some(s) = self.track_sends.get_mut(send_id) {
                    s.level = level.clamp(0.0, 1.0);
                }
            }
            Action::ToggleSend { send_id } => {
                if let Some(s) = self.track_sends.get_mut(send_id) {
                    s.enabled = !s.enabled;
                }
            }
            // ── Insert effects ────────────────────────────────────────────────
            Action::AddInsertEffect { track_id, kind } => {
                if let Some(t) = self.find_track_mut(track_id) {
                    let id = t.next_insert_id;
                    t.next_insert_id += 1;
                    t.insert_effects.push(InsertEffect {
                        id,
                        effect_type: kind,
                        enabled: true,
                        gain_in: 1.0,
                        gain_out: 1.0,
                    });
                }
            }
            Action::RemoveInsertEffect { track_id, effect_id } => {
                if let Some(t) = self.find_track_mut(track_id) {
                    t.insert_effects.retain(|e| e.id != effect_id);
                }
            }
            Action::ToggleInsertEffect { track_id, effect_id } => {
                if let Some(t) = self.find_track_mut(track_id) {
                    if let Some(e) = t.insert_effects.iter_mut().find(|e| e.id == effect_id) {
                        e.enabled = !e.enabled;
                    }
                }
            }
            Action::ReorderInsertEffect { track_id, effect_id, new_index } => {
                if let Some(t) = self.find_track_mut(track_id) {
                    if let Some(pos) = t.insert_effects.iter().position(|e| e.id == effect_id) {
                        let effect = t.insert_effects.remove(pos);
                        let insert_at = new_index.min(t.insert_effects.len());
                        t.insert_effects.insert(insert_at, effect);
                    }
                }
            }
            Action::SetInsertGain { track_id, effect_id, gain_in, gain_out } => {
                if let Some(t) = self.find_track_mut(track_id) {
                    if let Some(e) = t.insert_effects.iter_mut().find(|e| e.id == effect_id) {
                        e.gain_in = gain_in;
                        e.gain_out = gain_out;
                    }
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
    use super::TrackKind;
    use super::super::clips::ClipKind;

    fn fresh() -> App {
        App::new()
    }

    #[test]
    fn add_track_increments_counter() {
        let mut app = fresh();
        let initial = app.tracks.len();
        app.apply(Action::AddTrack(TrackKind::Audio));
        assert_eq!(app.tracks.len(), initial + 1);
    }

    #[test]
    fn add_track_creates_mixer_channel() {
        let mut app = fresh();
        let initial_channels = app.mixer_channels.len();
        app.apply(Action::AddTrack(TrackKind::Audio));
        assert_eq!(app.mixer_channels.len(), initial_channels + 1);
    }

    #[test]
    fn add_track_sets_active() {
        let mut app = fresh();
        app.apply(Action::AddTrack(TrackKind::Midi));
        assert!(app.active_track.is_some());
    }

    #[test]
    fn delete_track_removes_track_and_channel() {
        let mut app = fresh();
        app.apply(Action::AddTrack(TrackKind::Audio));
        let id = app.tracks.last().unwrap().id;
        let tracks_before = app.tracks.len();
        app.apply(Action::DeleteTrack(id));
        assert_eq!(app.tracks.len(), tracks_before - 1);
        assert!(!app.mixer_channels.iter().any(|m| m.track_id == id));
    }

    #[test]
    fn delete_track_clears_active() {
        let mut app = fresh();
        app.apply(Action::AddTrack(TrackKind::Audio));
        let id = app.tracks.last().unwrap().id;
        app.apply(Action::SetActiveTrack(Some(id)));
        app.apply(Action::DeleteTrack(id));
        assert_eq!(app.active_track, None);
    }

    #[test]
    fn delete_track_removes_clips() {
        let mut app = fresh();
        app.apply(Action::AddTrack(TrackKind::Audio));
        let tid = app.tracks.last().unwrap().id;
        app.apply(Action::AddClip { track_id: tid, name: "clip".into(), kind: ClipKind::Audio, start_beat: 0.0, duration_beats: 4.0 });
        assert!(!app.clips.is_empty());
        app.apply(Action::DeleteTrack(tid));
        assert!(app.clips.is_empty());
    }

    #[test]
    fn rename_track() {
        let mut app = fresh();
        app.apply(Action::AddTrack(TrackKind::Audio));
        let id = app.tracks.last().unwrap().id;
        app.apply(Action::RenameTrack { id, name: "Lead Guitar".to_string() });
        let t = app.tracks.iter().find(|t| t.id == id).unwrap();
        assert_eq!(t.name, "Lead Guitar");
    }

    #[test]
    fn set_track_mute() {
        let mut app = fresh();
        app.apply(Action::AddTrack(TrackKind::Audio));
        let id = app.tracks.last().unwrap().id;
        app.apply(Action::SetTrackMute { id, muted: true });
        assert!(app.tracks.iter().find(|t| t.id == id).unwrap().muted);
    }

    #[test]
    fn set_track_solo() {
        let mut app = fresh();
        app.apply(Action::AddTrack(TrackKind::Audio));
        let id = app.tracks.last().unwrap().id;
        app.apply(Action::SetTrackSolo { id, solo: true });
        assert!(app.tracks.iter().find(|t| t.id == id).unwrap().solo);
    }

    #[test]
    fn set_track_arm() {
        let mut app = fresh();
        app.apply(Action::AddTrack(TrackKind::Audio));
        let id = app.tracks.last().unwrap().id;
        app.apply(Action::SetTrackArm { id, armed: true });
        assert!(app.tracks.iter().find(|t| t.id == id).unwrap().armed);
    }

    #[test]
    fn set_track_volume_clamp() {
        let mut app = fresh();
        app.apply(Action::AddTrack(TrackKind::Audio));
        let id = app.tracks.last().unwrap().id;
        app.apply(Action::SetTrackVolume { id, volume: 5.0 });
        assert_eq!(app.tracks.iter().find(|t| t.id == id).unwrap().volume, 2.0);
    }

    #[test]
    fn set_track_volume_normal() {
        let mut app = fresh();
        app.apply(Action::AddTrack(TrackKind::Audio));
        let id = app.tracks.last().unwrap().id;
        app.apply(Action::SetTrackVolume { id, volume: 0.75 });
        assert_eq!(app.tracks.iter().find(|t| t.id == id).unwrap().volume, 0.75);
    }

    #[test]
    fn set_track_pan_clamp() {
        let mut app = fresh();
        app.apply(Action::AddTrack(TrackKind::Audio));
        let id = app.tracks.last().unwrap().id;
        app.apply(Action::SetTrackPan { id, pan: -5.0 });
        assert_eq!(app.tracks.iter().find(|t| t.id == id).unwrap().pan, -1.0);
    }

    #[test]
    fn set_track_color() {
        let mut app = fresh();
        app.apply(Action::AddTrack(TrackKind::Audio));
        let id = app.tracks.last().unwrap().id;
        app.apply(Action::SetTrackColor { id, color: "#FF0000".to_string() });
        assert_eq!(app.tracks.iter().find(|t| t.id == id).unwrap().color, "#FF0000");
    }

    #[test]
    fn set_track_instrument() {
        let mut app = fresh();
        app.apply(Action::AddTrack(TrackKind::Instrument));
        let id = app.tracks.last().unwrap().id;
        app.apply(Action::SetTrackInstrument { id, instrument: "Grand Piano".to_string() });
        assert_eq!(app.tracks.iter().find(|t| t.id == id).unwrap().instrument, Some("Grand Piano".to_string()));
    }

    #[test]
    fn duplicate_track_creates_copy() {
        let mut app = fresh();
        app.apply(Action::AddTrack(TrackKind::Audio));
        let id = app.tracks.last().unwrap().id;
        let before = app.tracks.len();
        app.apply(Action::DuplicateTrack(id));
        assert_eq!(app.tracks.len(), before + 1);
    }

    #[test]
    fn duplicate_track_copies_clips() {
        let mut app = fresh();
        app.apply(Action::AddTrack(TrackKind::Audio));
        let tid = app.tracks.last().unwrap().id;
        app.apply(Action::AddClip { track_id: tid, name: "clip".into(), kind: ClipKind::Audio, start_beat: 0.0, duration_beats: 4.0 });
        let before_clips = app.clips.len();
        app.apply(Action::DuplicateTrack(tid));
        assert_eq!(app.clips.len(), before_clips + 1);
    }

    #[test]
    fn reorder_tracks() {
        let mut app = fresh();
        app.apply(Action::AddTrack(TrackKind::Audio));
        app.apply(Action::AddTrack(TrackKind::Midi));
        let id_0 = app.tracks[0].id;
        let id_1 = app.tracks[1].id;
        app.apply(Action::ReorderTracks { from: 0, to: 1 });
        assert_eq!(app.tracks[0].id, id_1);
        assert_eq!(app.tracks[1].id, id_0);
    }

    #[test]
    fn set_active_track() {
        let mut app = fresh();
        app.apply(Action::AddTrack(TrackKind::Audio));
        let id = app.tracks.last().unwrap().id;
        app.apply(Action::SetActiveTrack(Some(id)));
        assert_eq!(app.active_track, Some(id));
        app.apply(Action::SetActiveTrack(None));
        assert_eq!(app.active_track, None);
    }

    #[test]
    fn track_kind_label() {
        assert_eq!(TrackKind::Audio.label(), "Audio");
        assert_eq!(TrackKind::Midi.label(), "MIDI");
        assert_eq!(TrackKind::Instrument.label(), "Instrument");
        assert_eq!(TrackKind::Bus.label(), "Bus");
        assert_eq!(TrackKind::Master.label(), "Master");
    }

    #[test]
    fn multi_track_solo_isolation() {
        let mut app = fresh();
        app.apply(Action::AddTrack(TrackKind::Audio));
        let t1 = app.tracks.last().unwrap().id;
        app.apply(Action::AddTrack(TrackKind::Audio));
        let t2 = app.tracks.last().unwrap().id;
        app.apply(Action::SetTrackSolo { id: t1, solo: true });
        app.apply(Action::SetTrackSolo { id: t2, solo: false });
        assert!(app.tracks.iter().find(|t| t.id == t1).unwrap().solo);
        assert!(!app.tracks.iter().find(|t| t.id == t2).unwrap().solo);
    }

    #[test]
    fn mixer_channel_count_matches_track_count() {
        let mut app = fresh();
        app.apply(Action::AddTrack(TrackKind::Audio));
        app.apply(Action::AddTrack(TrackKind::Midi));
        app.apply(Action::AddTrack(TrackKind::Bus));
        assert_eq!(app.tracks.len(), app.mixer_channels.len());
    }

    #[test]
    fn app_default_has_master_track() {
        let app = fresh();
        assert!(app.tracks.iter().any(|t| t.kind == TrackKind::Master));
    }

    // ── Send routing tests ────────────────────────────────────────────────────

    #[test]
    fn add_track_send() {
        let mut app = fresh();
        app.apply(Action::AddTrack(TrackKind::Audio));
        let from = app.tracks.last().unwrap().id;
        app.apply(Action::AddTrack(TrackKind::Bus));
        let bus = app.tracks.last().unwrap().id;
        app.apply(Action::AddTrackSend { from_track_id: from, to_bus_id: bus, level: 0.5 });
        assert_eq!(app.track_sends.len(), 1);
        assert_eq!(app.track_sends[0].from_track_id, from);
        assert_eq!(app.track_sends[0].to_bus_id, bus);
        assert!((app.track_sends[0].level - 0.5).abs() < 0.001);
        assert!(app.track_sends[0].enabled);
    }

    #[test]
    fn add_track_send_level_clamped() {
        let mut app = fresh();
        app.apply(Action::AddTrack(TrackKind::Audio));
        let from = app.tracks.last().unwrap().id;
        app.apply(Action::AddTrack(TrackKind::Bus));
        let bus = app.tracks.last().unwrap().id;
        app.apply(Action::AddTrackSend { from_track_id: from, to_bus_id: bus, level: 5.0 });
        assert!((app.track_sends[0].level - 1.0).abs() < 0.001);
    }

    #[test]
    fn remove_track_send() {
        let mut app = fresh();
        app.apply(Action::AddTrack(TrackKind::Audio));
        let from = app.tracks.last().unwrap().id;
        app.apply(Action::AddTrack(TrackKind::Bus));
        let bus = app.tracks.last().unwrap().id;
        app.apply(Action::AddTrackSend { from_track_id: from, to_bus_id: bus, level: 0.5 });
        assert_eq!(app.track_sends.len(), 1);
        app.apply(Action::RemoveTrackSend { send_id: 0 });
        assert!(app.track_sends.is_empty());
    }

    #[test]
    fn remove_track_send_out_of_bounds_is_noop() {
        let mut app = fresh();
        app.apply(Action::RemoveTrackSend { send_id: 99 });
        assert!(app.track_sends.is_empty());
    }

    #[test]
    fn set_send_level() {
        let mut app = fresh();
        app.apply(Action::AddTrack(TrackKind::Audio));
        let from = app.tracks.last().unwrap().id;
        app.apply(Action::AddTrack(TrackKind::Bus));
        let bus = app.tracks.last().unwrap().id;
        app.apply(Action::AddTrackSend { from_track_id: from, to_bus_id: bus, level: 0.5 });
        app.apply(Action::SetSendLevel { send_id: 0, level: 0.8 });
        assert!((app.track_sends[0].level - 0.8).abs() < 0.001);
    }

    #[test]
    fn set_send_level_clamped() {
        let mut app = fresh();
        app.apply(Action::AddTrack(TrackKind::Audio));
        let from = app.tracks.last().unwrap().id;
        app.apply(Action::AddTrack(TrackKind::Bus));
        let bus = app.tracks.last().unwrap().id;
        app.apply(Action::AddTrackSend { from_track_id: from, to_bus_id: bus, level: 0.5 });
        app.apply(Action::SetSendLevel { send_id: 0, level: -0.5 });
        assert!((app.track_sends[0].level).abs() < 0.001);
    }

    #[test]
    fn toggle_send_enabled() {
        let mut app = fresh();
        app.apply(Action::AddTrack(TrackKind::Audio));
        let from = app.tracks.last().unwrap().id;
        app.apply(Action::AddTrack(TrackKind::Bus));
        let bus = app.tracks.last().unwrap().id;
        app.apply(Action::AddTrackSend { from_track_id: from, to_bus_id: bus, level: 0.5 });
        assert!(app.track_sends[0].enabled);
        app.apply(Action::ToggleSend { send_id: 0 });
        assert!(!app.track_sends[0].enabled);
        app.apply(Action::ToggleSend { send_id: 0 });
        assert!(app.track_sends[0].enabled);
    }

    #[test]
    fn delete_track_removes_sends() {
        let mut app = fresh();
        app.apply(Action::AddTrack(TrackKind::Audio));
        let from = app.tracks.last().unwrap().id;
        app.apply(Action::AddTrack(TrackKind::Bus));
        let bus = app.tracks.last().unwrap().id;
        app.apply(Action::AddTrackSend { from_track_id: from, to_bus_id: bus, level: 0.5 });
        assert_eq!(app.track_sends.len(), 1);
        app.apply(Action::DeleteTrack(from));
        assert!(app.track_sends.is_empty());
    }

    // ── Insert effects tests ──────────────────────────────────────────────────

    #[test]
    fn add_insert_effect() {
        let mut app = fresh();
        app.apply(Action::AddTrack(TrackKind::Audio));
        let tid = app.tracks.last().unwrap().id;
        app.apply(Action::AddInsertEffect { track_id: tid, kind: super::InsertEffectKind::Compressor });
        let t = app.tracks.iter().find(|t| t.id == tid).unwrap();
        assert_eq!(t.insert_effects.len(), 1);
        assert!(t.insert_effects[0].enabled);
    }

    #[test]
    fn add_multiple_effects_and_verify_order() {
        let mut app = fresh();
        app.apply(Action::AddTrack(TrackKind::Audio));
        let tid = app.tracks.last().unwrap().id;
        app.apply(Action::AddInsertEffect { track_id: tid, kind: super::InsertEffectKind::Eq3Band });
        app.apply(Action::AddInsertEffect { track_id: tid, kind: super::InsertEffectKind::Compressor });
        app.apply(Action::AddInsertEffect { track_id: tid, kind: super::InsertEffectKind::Reverb { room_size: 0.5, wet: 0.3 } });
        let t = app.tracks.iter().find(|t| t.id == tid).unwrap();
        assert_eq!(t.insert_effects.len(), 3);
        assert_eq!(t.insert_effects[0].id, 0);
        assert_eq!(t.insert_effects[1].id, 1);
        assert_eq!(t.insert_effects[2].id, 2);
    }

    #[test]
    fn remove_insert_effect() {
        let mut app = fresh();
        app.apply(Action::AddTrack(TrackKind::Audio));
        let tid = app.tracks.last().unwrap().id;
        app.apply(Action::AddInsertEffect { track_id: tid, kind: super::InsertEffectKind::Compressor });
        let eid = {
            let t = app.tracks.iter().find(|t| t.id == tid).unwrap();
            t.insert_effects[0].id
        };
        app.apply(Action::RemoveInsertEffect { track_id: tid, effect_id: eid });
        let t = app.tracks.iter().find(|t| t.id == tid).unwrap();
        assert!(t.insert_effects.is_empty());
    }

    #[test]
    fn toggle_insert_effect() {
        let mut app = fresh();
        app.apply(Action::AddTrack(TrackKind::Audio));
        let tid = app.tracks.last().unwrap().id;
        app.apply(Action::AddInsertEffect { track_id: tid, kind: super::InsertEffectKind::Eq3Band });
        let eid = app.tracks.iter().find(|t| t.id == tid).unwrap().insert_effects[0].id;
        assert!(app.tracks.iter().find(|t| t.id == tid).unwrap().insert_effects[0].enabled);
        app.apply(Action::ToggleInsertEffect { track_id: tid, effect_id: eid });
        assert!(!app.tracks.iter().find(|t| t.id == tid).unwrap().insert_effects[0].enabled);
        app.apply(Action::ToggleInsertEffect { track_id: tid, effect_id: eid });
        assert!(app.tracks.iter().find(|t| t.id == tid).unwrap().insert_effects[0].enabled);
    }

    #[test]
    fn reorder_insert_effect() {
        let mut app = fresh();
        app.apply(Action::AddTrack(TrackKind::Audio));
        let tid = app.tracks.last().unwrap().id;
        app.apply(Action::AddInsertEffect { track_id: tid, kind: super::InsertEffectKind::Eq3Band });
        app.apply(Action::AddInsertEffect { track_id: tid, kind: super::InsertEffectKind::Compressor });
        app.apply(Action::AddInsertEffect { track_id: tid, kind: super::InsertEffectKind::Reverb { room_size: 0.5, wet: 0.3 } });
        // Move first effect to last position
        let eid0 = app.tracks.iter().find(|t| t.id == tid).unwrap().insert_effects[0].id;
        app.apply(Action::ReorderInsertEffect { track_id: tid, effect_id: eid0, new_index: 2 });
        let t = app.tracks.iter().find(|t| t.id == tid).unwrap();
        assert_eq!(t.insert_effects[2].id, eid0);
    }

    #[test]
    fn set_insert_gain() {
        let mut app = fresh();
        app.apply(Action::AddTrack(TrackKind::Audio));
        let tid = app.tracks.last().unwrap().id;
        app.apply(Action::AddInsertEffect { track_id: tid, kind: super::InsertEffectKind::Compressor });
        let eid = app.tracks.iter().find(|t| t.id == tid).unwrap().insert_effects[0].id;
        app.apply(Action::SetInsertGain { track_id: tid, effect_id: eid, gain_in: 0.8, gain_out: 1.2 });
        let t = app.tracks.iter().find(|t| t.id == tid).unwrap();
        assert!((t.insert_effects[0].gain_in - 0.8).abs() < 0.001);
        assert!((t.insert_effects[0].gain_out - 1.2).abs() < 0.001);
    }

    #[test]
    fn reorder_effect_out_of_bounds_clamps() {
        let mut app = fresh();
        app.apply(Action::AddTrack(TrackKind::Audio));
        let tid = app.tracks.last().unwrap().id;
        app.apply(Action::AddInsertEffect { track_id: tid, kind: super::InsertEffectKind::Eq3Band });
        app.apply(Action::AddInsertEffect { track_id: tid, kind: super::InsertEffectKind::Compressor });
        let eid0 = app.tracks.iter().find(|t| t.id == tid).unwrap().insert_effects[0].id;
        // new_index of 100 should clamp to end
        app.apply(Action::ReorderInsertEffect { track_id: tid, effect_id: eid0, new_index: 100 });
        let t = app.tracks.iter().find(|t| t.id == tid).unwrap();
        assert_eq!(t.insert_effects.len(), 2);
        assert_eq!(t.insert_effects[1].id, eid0);
    }

    #[test]
    fn remove_nonexistent_effect_is_noop() {
        let mut app = fresh();
        app.apply(Action::AddTrack(TrackKind::Audio));
        let tid = app.tracks.last().unwrap().id;
        app.apply(Action::AddInsertEffect { track_id: tid, kind: super::InsertEffectKind::Eq3Band });
        // Remove non-existent effect_id
        app.apply(Action::RemoveInsertEffect { track_id: tid, effect_id: 9999 });
        let t = app.tracks.iter().find(|t| t.id == tid).unwrap();
        assert_eq!(t.insert_effects.len(), 1);
    }

    #[test]
    fn insert_effect_ids_increment() {
        let mut app = fresh();
        app.apply(Action::AddTrack(TrackKind::Audio));
        let tid = app.tracks.last().unwrap().id;
        for _ in 0..5 {
            app.apply(Action::AddInsertEffect { track_id: tid, kind: super::InsertEffectKind::Eq3Band });
        }
        let t = app.tracks.iter().find(|t| t.id == tid).unwrap();
        for (i, effect) in t.insert_effects.iter().enumerate() {
            assert_eq!(effect.id, i);
        }
    }

    #[test]
    fn insert_effects_on_unknown_track_is_noop() {
        let mut app = fresh();
        // Should not panic
        app.apply(Action::AddInsertEffect { track_id: 9999, kind: super::InsertEffectKind::Compressor });
        assert!(app.tracks.iter().all(|t| t.insert_effects.is_empty()));
    }

    #[test]
    fn insert_effect_delay_kind() {
        let mut app = fresh();
        app.apply(Action::AddTrack(TrackKind::Audio));
        let tid = app.tracks.last().unwrap().id;
        app.apply(Action::AddInsertEffect {
            track_id: tid,
            kind: super::InsertEffectKind::Delay { time_ms: 250.0, feedback: 0.4, wet: 0.3 },
        });
        let t = app.tracks.iter().find(|t| t.id == tid).unwrap();
        assert_eq!(t.insert_effects.len(), 1);
    }
}
