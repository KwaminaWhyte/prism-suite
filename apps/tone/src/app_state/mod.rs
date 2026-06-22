//! The single shared application state for Tone's GPUI host.
//!
//! `App` owns everything panels read or mutate: the active [`ToneProject`]
//! (BPM, key, time signature), the flat lists of [`ToneTrack`]s,
//! [`ToneClip`]s, and [`MidiNote`]s, the mixer channels, the AI generation
//! queue, the bounce config, and the transport state. Panels NEVER mutate
//! `App` fields directly — they emit an [`Action`], and the root view routes
//! it through [`App::apply`], the single choke point for all mutations.

// ─── Domain modules ──────────────────────────────────────────────────────────

pub mod ai;
pub mod clips;
pub mod export;
pub mod history;
pub mod midi;
pub mod mixer;
pub mod project;
pub mod track_groups;
pub mod tracks;
pub mod transport;

// ─── Re-exports ──────────────────────────────────────────────────────────────

pub use ai::{AiGenerationJob, AiGenerationStatus};
pub use clips::{ClipKind, ToneClip};
pub use export::{BounceConfig, BounceFormat};
pub use history::HistoryEntry;
pub use midi::{MidiCC, MidiNote, NudgeAmount, NudgeDirection};
pub use mixer::{EqBand, MixerChannel};
pub use project::ToneProject;
pub use track_groups::TrackGroup;
pub use tracks::{InsertEffect, InsertEffectKind, ToneTrack, TrackKind, TrackSend};

// ─── Phase 2 types (data model only) ─────────────────────────────────────────

/// Quantize grid subdivision.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QuantizeGrid {
    Whole,
    Half,
    Quarter,
    Eighth,
    Sixteenth,
    ThirtySecond,
    QuarterTriplet,
    EighthTriplet,
}

impl QuantizeGrid {
    /// Returns the grid size in beats (assumes 1 beat = 1 quarter note).
    pub fn beats(self) -> f32 {
        match self {
            QuantizeGrid::Whole => 4.0,
            QuantizeGrid::Half => 2.0,
            QuantizeGrid::Quarter => 1.0,
            QuantizeGrid::Eighth => 0.5,
            QuantizeGrid::Sixteenth => 0.25,
            QuantizeGrid::ThirtySecond => 0.125,
            QuantizeGrid::QuarterTriplet => 4.0 / 3.0,
            QuantizeGrid::EighthTriplet => 2.0 / 3.0,
        }
    }
}

/// Quantize panel configuration.
#[derive(Clone, Debug)]
pub struct QuantizeConfig {
    pub grid: QuantizeGrid,
    /// Swing amount 0.0..=1.0 (0.0 = straight, 0.5 = typical swing).
    pub swing: f32,
    /// Quantize strength 0.0..=1.0 (1.0 = full snap, 0.5 = half-pull).
    pub strength: f32,
}

impl QuantizeConfig {
    pub fn new() -> Self {
        Self {
            grid: QuantizeGrid::Sixteenth,
            swing: 0.0,
            strength: 1.0,
        }
    }
}

impl Default for QuantizeConfig {
    fn default() -> Self {
        Self::new()
    }
}

// ─── Actions ─────────────────────────────────────────────────────────────────

/// Every mutation in Tone is expressed as one of these variants. `App::apply`
/// is the single choke point that processes them.
#[derive(Clone, Debug)]
pub enum Action {
    // ── Project / Session ──────────────────────────────────────────────────
    /// Set the project BPM. Clamped to 20.0..=999.0.
    SetBpm(f32),
    /// Set the time signature numerator (1..=16) and denominator (1/2/4/8/16).
    SetTimeSignature { num: u8, den: u8 },
    /// Set the project sample rate (44100, 48000, 88200, 96000).
    SetSampleRate(u32),
    /// Set the project bit depth (16, 24, 32).
    SetBitDepth(u8),
    /// Set the root key of the project (e.g. "C", "F#", "Bb").
    SetProjectKey(String),
    /// Set the scale name (e.g. "Major", "Minor", "Dorian").
    SetProjectScale(String),
    /// Rename the project.
    SetProjectName(String),

    // ── Tracks ─────────────────────────────────────────────────────────────
    /// Add a new track of the given kind with an auto-generated name.
    AddTrack(TrackKind),
    /// Remove the track with the given id (and all its clips/notes).
    DeleteTrack(usize),
    /// Rename a track.
    RenameTrack { id: usize, name: String },
    /// Set the muted flag on a track.
    SetTrackMute { id: usize, muted: bool },
    /// Set the solo flag on a track.
    SetTrackSolo { id: usize, solo: bool },
    /// Set the record-arm flag on a track.
    SetTrackArm { id: usize, armed: bool },
    /// Set the fader volume on a track. Clamped 0.0..=2.0.
    SetTrackVolume { id: usize, volume: f32 },
    /// Set the stereo pan on a track. Clamped -1.0..=1.0.
    SetTrackPan { id: usize, pan: f32 },
    /// Set the display colour on a track (CSS hex string).
    SetTrackColor { id: usize, color: String },
    /// Assign an instrument to a MIDI/Instrument track.
    SetTrackInstrument { id: usize, instrument: String },
    /// Duplicate a track (deep copy including clips and notes).
    DuplicateTrack(usize),
    /// Reorder tracks: move the track at `from` index to `to` index.
    ReorderTracks { from: usize, to: usize },
    /// Set the focused / active track (piano roll, inspector follow).
    SetActiveTrack(Option<usize>),

    // ── Insert effects ─────────────────────────────────────────────────────
    /// Add an insert effect to a track's chain.
    AddInsertEffect { track_id: usize, kind: InsertEffectKind },
    /// Remove an insert effect from a track's chain.
    RemoveInsertEffect { track_id: usize, effect_id: usize },
    /// Toggle an insert effect's enabled state.
    ToggleInsertEffect { track_id: usize, effect_id: usize },
    /// Reorder an insert effect to a new position.
    ReorderInsertEffect { track_id: usize, effect_id: usize, new_index: usize },
    /// Set the input and output gain for an insert effect.
    SetInsertGain { track_id: usize, effect_id: usize, gain_in: f32, gain_out: f32 },

    // ── Send routing (Phase 2) ─────────────────────────────────────────────
    /// Add a send from a track to a bus with the given level (0.0..=1.0).
    AddTrackSend { from_track_id: usize, to_bus_id: usize, level: f32 },
    /// Remove a send by index.
    RemoveTrackSend { send_id: usize },
    /// Set the level of an existing send. Clamped 0.0..=1.0.
    SetSendLevel { send_id: usize, level: f32 },
    /// Toggle a send on or off.
    ToggleSend { send_id: usize },

    // ── Clips ──────────────────────────────────────────────────────────────
    /// Add a clip to a track at the given beat position.
    AddClip {
        track_id: usize,
        name: String,
        kind: ClipKind,
        start_beat: f32,
        duration_beats: f32,
    },
    /// Delete a clip (and all its MIDI notes).
    DeleteClip(usize),
    /// Move a clip to a different track and/or beat position.
    MoveClip { id: usize, track_id: usize, start_beat: f32 },
    /// Resize a clip to the given duration in beats.
    ResizeClip { id: usize, duration_beats: f32 },
    /// Set per-clip gain. Clamped 0.0..=4.0.
    SetClipGain { id: usize, gain: f32 },
    /// Set per-clip pitch shift in semitones. Clamped -24..=24.
    SetClipPitchShift { id: usize, semitones: i8 },
    /// Set per-clip time-stretch ratio. Clamped 0.5..=2.0.
    SetClipTimeStretch { id: usize, ratio: f32 },
    /// Toggle clip looping.
    SetClipLoop { id: usize, looping: bool },
    /// Toggle clip mute.
    SetClipMute { id: usize, muted: bool },
    /// Split a clip at `at_beat`.
    SplitClip { id: usize, at_beat: f32 },
    /// Merge a list of clips into the one starting earliest.
    MergeClips { ids: Vec<usize> },
    /// Mark the peak cache for a clip as dirty (needs recompute for waveform display).
    InvalidatePeakCache { clip_id: usize },
    /// Duplicate a clip, placing it immediately after the original.
    DuplicateClip { clip_id: usize },
    /// Consolidate a list of clips into a single spanning clip.
    ConsolidateClips { clip_ids: Vec<usize> },
    /// Set the display color of a clip.
    SetClipColor { clip_id: usize, color: String },
    /// Trim the start of a clip to a new beat position.
    TrimClipStart { clip_id: usize, new_start: f32 },
    /// Trim the end of a clip to a new beat position.
    TrimClipEnd { clip_id: usize, new_end: f32 },
    /// Set a fine-grained pitch shift (fractional semitones) on a clip.
    SetClipPitchF32 { clip_id: usize, semitones: f32 },
    /// Set the per-clip gain in dB (converted to linear gain and stored).
    SetClipGainDb { clip_id: usize, gain_db: f32 },

    // ── Piano Roll / MIDI ──────────────────────────────────────────────────
    /// Open the piano roll editor focused on the given clip.
    OpenPianoRoll(usize),
    /// Close the piano roll editor.
    ClosePianoRoll,
    /// Add a MIDI note to a clip.
    AddMidiNote {
        clip_id: usize,
        pitch: u8,
        velocity: u8,
        start_beat: f32,
        duration_beats: f32,
    },
    /// Delete a MIDI note by id.
    DeleteMidiNote(usize),
    /// Move a MIDI note to a new pitch and/or beat position.
    MoveMidiNote { id: usize, pitch: u8, start_beat: f32 },
    /// Resize a MIDI note's duration.
    ResizeMidiNote { id: usize, duration_beats: f32 },
    /// Set a MIDI note's velocity. Clamped 0..=127.
    SetMidiNoteVelocity { id: usize, velocity: u8 },
    /// Select all notes in a clip (sets `selected_notes`).
    SelectAllNotesInClip(usize),
    /// Snap all notes in a clip to the nearest grid division.
    QuantizeMidiNotes { clip_id: usize, grid: f32 },
    /// Transpose all notes in the given clips by `semitones`.
    TransposeMidiNotes { clip_ids: Vec<usize>, semitones: i8 },
    /// Set the zoom level of the piano roll (horizontal and vertical).
    SetPianoRollZoom { zoom_h: f32, zoom_v: f32 },
    /// Set the scroll offset of the piano roll.
    SetPianoRollScroll { beat: f32, key: u8 },

    // ── MIDI CC lanes (Phase 2) ────────────────────────────────────────────
    /// Add a MIDI CC event to a clip.
    AddMidiCC { clip_id: usize, controller: u8, position: f32, value: u8 },
    /// Remove a MIDI CC event by index.
    RemoveMidiCC { cc_id: usize },
    /// Set the value of a MIDI CC event. Clamped 0..=127.
    SetMidiCCValue { cc_id: usize, value: u8 },
    /// Move a MIDI CC event to a new position.
    MoveMidiCC { cc_id: usize, position: f32 },

    // ── Note editing operations (Phase 2) ─────────────────────────────────
    /// Select (add to selection) a single MIDI note by id.
    SelectNote { note_id: usize },
    /// Deselect a single MIDI note by id.
    DeselectNote { note_id: usize },
    /// Clear the note selection.
    DeselectAllNotes,
    /// Delete all currently selected notes.
    DeleteSelectedNotes,
    /// Copy selected notes to clipboard.
    CopySelectedNotes,
    /// Paste clipboard notes into a clip at an offset.
    PasteNotes { clip_id: usize, offset_beats: f32 },
    /// Move all selected notes by delta_beats and/or delta_semitones.
    MoveSelectedNotes { delta_beats: f32, delta_semitones: i32 },
    /// Resize all selected notes to new_duration.
    ResizeSelectedNotes { new_duration: f32 },
    /// Set velocity on all selected notes.
    SetSelectedNotesVelocity { velocity: u8 },
    /// Nudge selected notes in a direction by a given amount.
    NudgeNotes { direction: NudgeDirection, amount: NudgeAmount },
    /// Quantize selected notes to the given grid.
    QuantizeSelectedNotes { grid: QuantizeGrid },

    // ── Quantize panel (Phase 2) ───────────────────────────────────────────
    /// Set the quantize configuration (grid, swing, strength).
    SetQuantize { grid: QuantizeGrid, swing: f32, strength: f32 },

    // ── Mixer / Master ─────────────────────────────────────────────────────
    /// Set the EQ low-shelf gain (dB) for a channel. Clamped -12..=12.
    SetChannelEqLow { track_id: usize, gain_db: f32 },
    /// Set the EQ mid-peak gain (dB) for a channel. Clamped -12..=12.
    SetChannelEqMid { track_id: usize, gain_db: f32 },
    /// Set the EQ high-shelf gain (dB) for a channel. Clamped -12..=12.
    SetChannelEqHigh { track_id: usize, gain_db: f32 },
    /// Set the compressor threshold (dB) for a channel. Clamped -60..=0.
    SetChannelCompThreshold { track_id: usize, threshold_db: f32 },
    /// Set the compressor ratio for a channel. Clamped 1..=20.
    SetChannelCompRatio { track_id: usize, ratio: f32 },
    /// Toggle the compressor on/off for a channel.
    ToggleChannelComp(usize),
    /// Insert a named effect at the end of a channel's insert chain.
    AddChannelEffect { track_id: usize, effect: String },
    /// Remove the effect at `index` from a channel's insert chain.
    RemoveChannelEffect { track_id: usize, index: usize },
    /// Set the master output fader. Clamped 0.0..=2.0.
    SetMasterVolume(f32),
    /// Toggle the master limiter on/off.
    ToggleMasterLimiter,

    // ── Extended mixer params (Phase 2) ────────────────────────────────────
    /// Add a channel send (bus routing) to a mixer channel.
    AddChannelSend { channel_id: usize, bus_id: usize, level: f32 },
    /// Remove a channel send from a mixer channel.
    RemoveChannelSend { channel_id: usize, bus_id: usize },
    /// Set the send level for a bus in a channel's send list.
    SetChannelSendLevel { channel_id: usize, bus_id: usize, level: f32 },
    /// Set the pre-fader listen (PFL/solo-in-place) flag for a channel.
    SetChannelPfl { channel_id: usize, pfl: bool },
    /// Invert the phase of a channel.
    SetChannelPhaseInvert { channel_id: usize, invert: bool },
    /// Set the stereo width of a channel. Clamped 0.0..=2.0.
    SetStereoWidth { channel_id: usize, width: f32 },
    /// Set the trim gain (dB) before the fader. Clamped -6.0..=6.0.
    SetChannelTrim { channel_id: usize, trim_db: f32 },
    /// Reset a channel to default values.
    ResetChannel { channel_id: usize },

    // ── Transport ──────────────────────────────────────────────────────────
    Play,
    Stop,
    Pause,
    Record,
    SetPlayheadBeat(f32),
    ToggleLoop,
    SetLoopRange { start: f32, end: f32 },
    ToggleMetronome,
    Rewind,
    FastForward,

    // ── Loop region bar controls (Phase 2) ────────────────────────────────
    /// Set the loop region in bars.
    SetLoopRegion { start_bar: f32, end_bar: f32 },
    /// Enable or disable the loop region.
    SetLoopBarEnabled(bool),
    /// Move the loop region by a delta in bars.
    MoveLoopRegion { delta_bars: f32 },
    /// Resize the loop region's start position.
    ResizeLoopStart { bar: f32 },
    /// Resize the loop region's end position.
    ResizeLoopEnd { bar: f32 },

    // ── History (Phase 2) ──────────────────────────────────────────────────
    /// Undo the last operation.
    Undo,
    /// Redo the last undone operation.
    Redo,
    /// Push an undo checkpoint with a description.
    PushUndoCheckpoint { description: String },

    // ── Track groups (Phase 2) ────────────────────────────────────────────
    /// Create a new track group containing the given tracks.
    CreateTrackGroup { name: String, track_ids: Vec<usize> },
    /// Add a track to an existing group.
    AddTrackToGroup { group_id: usize, track_id: usize },
    /// Remove a track from a group.
    RemoveTrackFromGroup { group_id: usize, track_id: usize },
    /// Collapse or expand a track group.
    CollapseGroup { group_id: usize, collapsed: bool },
    /// Delete a track group (does not delete the tracks).
    DeleteTrackGroup { group_id: usize },
    /// Set the display color of a track group.
    SetGroupColor { group_id: usize, color: String },
    /// Rename a track group.
    RenameTrackGroup { group_id: usize, name: String },

    // ── AI Generation ──────────────────────────────────────────────────────
    SetAiPrompt(String),
    SetAiStyle(String),
    SetAiDurationBars(u8),
    GenerateTrack { track_id: usize },
    CompleteAiGeneration { job_id: usize },
    GenerateChordProgression { track_id: usize, bars: u8 },
    GenerateDrumPattern { track_id: usize },
    HarmonizeMelody { track_id: usize },
    AiMasterTrack,
    SuggestChords { key: String, mood: String },

    // ── Export / Bounce ────────────────────────────────────────────────────
    SetBounceFormat(BounceFormat),
    SetBounceNormalize(bool),
    SetBounceDither(bool),
    SetBounceExportPath(String),
    SetBounceStemsPerTrack(bool),
    StartBounce,
    CancelBounce,
}

// ─── App ─────────────────────────────────────────────────────────────────────

/// The root application state. Panels borrow this read-only and route
/// mutations through `App::apply`.
pub struct App {
    // ── Project ────────────────────────────────────────────────────────────
    pub project: ToneProject,

    // ── Tracks ─────────────────────────────────────────────────────────────
    pub tracks: Vec<ToneTrack>,
    pub track_counter: usize,
    pub active_track: Option<usize>,

    // ── Send routing (Phase 2) ─────────────────────────────────────────────
    pub track_sends: Vec<TrackSend>,
    pub next_send_id: usize,

    // ── Clips ──────────────────────────────────────────────────────────────
    pub clips: Vec<ToneClip>,
    pub clip_counter: usize,

    // ── MIDI ───────────────────────────────────────────────────────────────
    pub midi_notes: Vec<MidiNote>,
    pub midi_note_counter: usize,
    pub piano_roll_clip: Option<usize>,
    pub selected_notes: Vec<usize>,
    /// Horizontal zoom level (beats per screen width).
    pub piano_roll_zoom_h: f32,
    /// Vertical zoom level (keys visible).
    pub piano_roll_zoom_v: f32,
    /// Horizontal scroll in beats.
    pub piano_roll_scroll_beat: f32,
    /// Top visible MIDI key (default 108 = C8).
    pub piano_roll_scroll_key: u8,

    // ── MIDI CC lanes (Phase 2) ────────────────────────────────────────────
    pub midi_cc: Vec<MidiCC>,
    pub next_cc_id: usize,

    // ── Quantize panel (Phase 2) ───────────────────────────────────────────
    pub quantize_config: QuantizeConfig,

    // ── Mixer ──────────────────────────────────────────────────────────────
    pub mixer_channels: Vec<MixerChannel>,
    pub master_volume: f32,
    pub master_limiter: bool,

    // ── Transport ──────────────────────────────────────────────────────────
    pub playhead_beat: f32,
    pub playing: bool,
    pub recording: bool,
    pub loop_enabled: bool,
    pub loop_start: f32,
    pub loop_end: f32,
    pub metronome_enabled: bool,

    // ── AI ─────────────────────────────────────────────────────────────────
    pub ai_prompt: String,
    pub ai_style: String,
    pub ai_duration_bars: u8,
    pub ai_jobs: Vec<AiGenerationJob>,
    pub ai_job_counter: usize,

    // ── Bounce ─────────────────────────────────────────────────────────────
    pub bounce_config: BounceConfig,
    pub bounce_in_progress: bool,
}

impl App {
    /// Create a fresh App with one default Master track.
    pub fn new() -> Self {
        let master = ToneTrack::new(0, "Master", TrackKind::Master);
        let master_channel = MixerChannel::new(0);
        Self {
            project: ToneProject::new(),
            tracks: vec![master],
            track_counter: 1,
            active_track: None,
            track_sends: Vec::new(),
            next_send_id: 0,
            clips: Vec::new(),
            clip_counter: 0,
            midi_notes: Vec::new(),
            midi_note_counter: 0,
            piano_roll_clip: None,
            selected_notes: Vec::new(),
            piano_roll_zoom_h: 8.0,
            piano_roll_zoom_v: 48.0,
            piano_roll_scroll_beat: 0.0,
            piano_roll_scroll_key: 108,
            midi_cc: Vec::new(),
            next_cc_id: 0,
            quantize_config: QuantizeConfig::new(),
            mixer_channels: vec![master_channel],
            master_volume: 1.0,
            master_limiter: false,
            playhead_beat: 0.0,
            playing: false,
            recording: false,
            loop_enabled: false,
            loop_start: 0.0,
            loop_end: 16.0,
            metronome_enabled: false,
            ai_prompt: String::new(),
            ai_style: "Electronic".to_string(),
            ai_duration_bars: 8,
            ai_jobs: Vec::new(),
            ai_job_counter: 0,
            bounce_config: BounceConfig::new(),
            bounce_in_progress: false,
        }
    }

    // ── Internal helpers ─────────────────────────────────────────────────────

    pub(crate) fn find_track_mut(&mut self, id: usize) -> Option<&mut ToneTrack> {
        self.tracks.iter_mut().find(|t| t.id == id)
    }

    pub(crate) fn find_clip_mut(&mut self, id: usize) -> Option<&mut ToneClip> {
        self.clips.iter_mut().find(|c| c.id == id)
    }

    pub(crate) fn find_note_mut(&mut self, id: usize) -> Option<&mut MidiNote> {
        self.midi_notes.iter_mut().find(|n| n.id == id)
    }

    pub(crate) fn find_mixer_mut(&mut self, track_id: usize) -> Option<&mut MixerChannel> {
        self.mixer_channels.iter_mut().find(|m| m.track_id == track_id)
    }

    pub(crate) fn next_clip_id(&mut self) -> usize {
        let id = self.clip_counter;
        self.clip_counter += 1;
        id
    }

    pub(crate) fn next_note_id(&mut self) -> usize {
        let id = self.midi_note_counter;
        self.midi_note_counter += 1;
        id
    }

    // ── Dispatcher ───────────────────────────────────────────────────────────

    /// Apply a single action, mutating state in place. This is the only place
    /// state is mutated — panels never touch fields directly.
    pub fn apply(&mut self, action: Action) {
        match &action {
            // ── Project ──────────────────────────────────────────────────────
            Action::SetBpm(..)
            | Action::SetTimeSignature { .. }
            | Action::SetSampleRate(..)
            | Action::SetBitDepth(..)
            | Action::SetProjectKey(..)
            | Action::SetProjectScale(..)
            | Action::SetProjectName(..) => self.apply_project(action),

            // ── Tracks ───────────────────────────────────────────────────────
            Action::AddTrack(..)
            | Action::DeleteTrack(..)
            | Action::RenameTrack { .. }
            | Action::SetTrackMute { .. }
            | Action::SetTrackSolo { .. }
            | Action::SetTrackArm { .. }
            | Action::SetTrackVolume { .. }
            | Action::SetTrackPan { .. }
            | Action::SetTrackColor { .. }
            | Action::SetTrackInstrument { .. }
            | Action::DuplicateTrack(..)
            | Action::ReorderTracks { .. }
            | Action::SetActiveTrack(..)
            | Action::AddTrackSend { .. }
            | Action::RemoveTrackSend { .. }
            | Action::SetSendLevel { .. }
            | Action::ToggleSend { .. } => self.apply_tracks(action),

            // ── Clips ────────────────────────────────────────────────────────
            Action::AddClip { .. }
            | Action::DeleteClip(..)
            | Action::MoveClip { .. }
            | Action::ResizeClip { .. }
            | Action::SetClipGain { .. }
            | Action::SetClipPitchShift { .. }
            | Action::SetClipTimeStretch { .. }
            | Action::SetClipLoop { .. }
            | Action::SetClipMute { .. }
            | Action::SplitClip { .. }
            | Action::MergeClips { .. }
            | Action::InvalidatePeakCache { .. } => self.apply_clips(action),

            // ── MIDI / Piano Roll ─────────────────────────────────────────────
            Action::OpenPianoRoll(..)
            | Action::ClosePianoRoll
            | Action::AddMidiNote { .. }
            | Action::DeleteMidiNote(..)
            | Action::MoveMidiNote { .. }
            | Action::ResizeMidiNote { .. }
            | Action::SetMidiNoteVelocity { .. }
            | Action::SelectAllNotesInClip(..)
            | Action::QuantizeMidiNotes { .. }
            | Action::TransposeMidiNotes { .. }
            | Action::SetPianoRollZoom { .. }
            | Action::SetPianoRollScroll { .. }
            | Action::AddMidiCC { .. }
            | Action::RemoveMidiCC { .. }
            | Action::SetMidiCCValue { .. }
            | Action::MoveMidiCC { .. } => self.apply_midi(action),

            // ── Quantize ─────────────────────────────────────────────────────
            Action::SetQuantize { .. } => {
                if let Action::SetQuantize { grid, swing, strength } = action {
                    self.quantize_config.grid = grid;
                    self.quantize_config.swing = swing.clamp(0.0, 1.0);
                    self.quantize_config.strength = strength.clamp(0.0, 1.0);
                }
            }

            // ── Mixer ─────────────────────────────────────────────────────────
            Action::SetChannelEqLow { .. }
            | Action::SetChannelEqMid { .. }
            | Action::SetChannelEqHigh { .. }
            | Action::SetChannelCompThreshold { .. }
            | Action::SetChannelCompRatio { .. }
            | Action::ToggleChannelComp(..)
            | Action::AddChannelEffect { .. }
            | Action::RemoveChannelEffect { .. }
            | Action::SetMasterVolume(..)
            | Action::ToggleMasterLimiter => self.apply_mixer(action),

            // ── Transport ────────────────────────────────────────────────────
            Action::Play
            | Action::Stop
            | Action::Pause
            | Action::Record
            | Action::SetPlayheadBeat(..)
            | Action::ToggleLoop
            | Action::SetLoopRange { .. }
            | Action::ToggleMetronome
            | Action::Rewind
            | Action::FastForward => self.apply_transport(action),

            // ── AI ────────────────────────────────────────────────────────────
            Action::SetAiPrompt(..)
            | Action::SetAiStyle(..)
            | Action::SetAiDurationBars(..)
            | Action::GenerateTrack { .. }
            | Action::CompleteAiGeneration { .. }
            | Action::GenerateChordProgression { .. }
            | Action::GenerateDrumPattern { .. }
            | Action::HarmonizeMelody { .. }
            | Action::AiMasterTrack
            | Action::SuggestChords { .. } => self.apply_ai(action),

            // ── Export ────────────────────────────────────────────────────────
            Action::SetBounceFormat(..)
            | Action::SetBounceNormalize(..)
            | Action::SetBounceDither(..)
            | Action::SetBounceExportPath(..)
            | Action::SetBounceStemsPerTrack(..)
            | Action::StartBounce
            | Action::CancelBounce => self.apply_export(action),
        }
    }
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

// ─── Integration tests ────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn fresh() -> App {
        App::new()
    }

    #[test]
    fn full_session_workflow() {
        let mut app = fresh();
        app.apply(Action::SetProjectName("Summer Vibes".to_string()));
        app.apply(Action::SetBpm(105.0));
        app.apply(Action::SetProjectKey("A".to_string()));
        app.apply(Action::SetProjectScale("Minor".to_string()));
        app.apply(Action::AddTrack(TrackKind::Instrument));
        let piano_id = app.tracks.last().unwrap().id;
        app.apply(Action::SetTrackInstrument { id: piano_id, instrument: "Electric Piano".to_string() });
        app.apply(Action::AddTrack(TrackKind::Audio));
        let audio_id = app.tracks.last().unwrap().id;
        app.apply(Action::SetTrackVolume { id: audio_id, volume: 0.85 });
        app.apply(Action::AddClip { track_id: piano_id, name: "Verse Piano".into(), kind: ClipKind::Midi, start_beat: 0.0, duration_beats: 16.0 });
        let cid = app.clips[0].id;
        for (pitch, beat) in [(60u8, 0.0f32), (62, 1.0), (64, 2.0), (65, 3.0)] {
            app.apply(Action::AddMidiNote { clip_id: cid, pitch, velocity: 90, start_beat: beat, duration_beats: 0.75 });
        }
        app.apply(Action::QuantizeMidiNotes { clip_id: cid, grid: 0.25 });
        app.apply(Action::Play);
        assert!(app.playing);
        app.apply(Action::Stop);
        assert!(!app.playing);
        app.apply(Action::SetBounceFormat(BounceFormat::Wav));
        app.apply(Action::SetBounceExportPath("/tmp/summer_vibes.wav".to_string()));
        app.apply(Action::StartBounce);
        assert!(app.bounce_in_progress);
        assert_eq!(app.project.bpm, 105.0);
        assert_eq!(app.midi_notes.len(), 4);
    }

    #[test]
    fn quantize_config_defaults() {
        let app = fresh();
        assert_eq!(app.quantize_config.grid, QuantizeGrid::Sixteenth);
        assert_eq!(app.quantize_config.swing, 0.0);
        assert_eq!(app.quantize_config.strength, 1.0);
    }

    #[test]
    fn set_quantize_action() {
        let mut app = fresh();
        app.apply(Action::SetQuantize { grid: QuantizeGrid::Eighth, swing: 0.5, strength: 0.75 });
        assert_eq!(app.quantize_config.grid, QuantizeGrid::Eighth);
        assert!((app.quantize_config.swing - 0.5).abs() < 0.001);
        assert!((app.quantize_config.strength - 0.75).abs() < 0.001);
    }

    #[test]
    fn set_quantize_swing_clamped() {
        let mut app = fresh();
        app.apply(Action::SetQuantize { grid: QuantizeGrid::Quarter, swing: 2.0, strength: 0.5 });
        assert!(app.quantize_config.swing <= 1.0);
    }

    #[test]
    fn set_quantize_strength_clamped() {
        let mut app = fresh();
        app.apply(Action::SetQuantize { grid: QuantizeGrid::Quarter, swing: 0.0, strength: -1.0 });
        assert!(app.quantize_config.strength >= 0.0);
    }

    #[test]
    fn quantize_grid_beats() {
        assert_eq!(QuantizeGrid::Whole.beats(), 4.0);
        assert_eq!(QuantizeGrid::Half.beats(), 2.0);
        assert_eq!(QuantizeGrid::Quarter.beats(), 1.0);
        assert_eq!(QuantizeGrid::Eighth.beats(), 0.5);
        assert_eq!(QuantizeGrid::Sixteenth.beats(), 0.25);
        assert_eq!(QuantizeGrid::ThirtySecond.beats(), 0.125);
    }

    #[test]
    fn piano_roll_defaults() {
        let app = fresh();
        assert!((app.piano_roll_zoom_h - 8.0).abs() < 0.001);
        assert!((app.piano_roll_zoom_v - 48.0).abs() < 0.001);
        assert_eq!(app.piano_roll_scroll_beat, 0.0);
        assert_eq!(app.piano_roll_scroll_key, 108);
    }

    #[test]
    fn quantize_grid_triplet_beats() {
        assert!((QuantizeGrid::QuarterTriplet.beats() - 4.0 / 3.0).abs() < 0.001);
        assert!((QuantizeGrid::EighthTriplet.beats() - 2.0 / 3.0).abs() < 0.001);
    }

    #[test]
    fn set_quantize_all_grids() {
        let mut app = fresh();
        for grid in [
            QuantizeGrid::Whole,
            QuantizeGrid::Half,
            QuantizeGrid::Quarter,
            QuantizeGrid::Eighth,
            QuantizeGrid::Sixteenth,
            QuantizeGrid::ThirtySecond,
            QuantizeGrid::QuarterTriplet,
            QuantizeGrid::EighthTriplet,
        ] {
            app.apply(Action::SetQuantize { grid, swing: 0.0, strength: 1.0 });
            assert_eq!(app.quantize_config.grid, grid);
        }
    }

    #[test]
    fn app_starts_with_no_clips() {
        let app = fresh();
        assert!(app.clips.is_empty());
    }

    #[test]
    fn app_starts_with_no_midi_notes() {
        let app = fresh();
        assert!(app.midi_notes.is_empty());
    }

    #[test]
    fn app_starts_with_no_ai_jobs() {
        let app = fresh();
        assert!(app.ai_jobs.is_empty());
    }

    #[test]
    fn app_starts_with_no_track_sends() {
        let app = fresh();
        assert!(app.track_sends.is_empty());
    }

    #[test]
    fn app_starts_with_no_midi_cc() {
        let app = fresh();
        assert!(app.midi_cc.is_empty());
    }

    #[test]
    fn app_starts_with_ai_style_electronic() {
        let app = fresh();
        assert_eq!(app.ai_style, "Electronic");
    }

    #[test]
    fn app_starts_with_ai_duration_8_bars() {
        let app = fresh();
        assert_eq!(app.ai_duration_bars, 8);
    }

    #[test]
    fn master_volume_starts_at_unity() {
        let app = fresh();
        assert_eq!(app.master_volume, 1.0);
    }

    #[test]
    fn master_limiter_starts_off() {
        let app = fresh();
        assert!(!app.master_limiter);
    }

    #[test]
    fn bounce_not_in_progress_initially() {
        let app = fresh();
        assert!(!app.bounce_in_progress);
    }
}
