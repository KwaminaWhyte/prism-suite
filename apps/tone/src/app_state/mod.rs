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
pub mod audio_engine;
pub mod bus_routing;
pub mod clips;
pub mod export;
pub mod history;
pub mod midi;
pub mod midi_control;
pub mod mixer;
pub mod plugins;
pub mod project;
pub mod recording;
pub mod scenes;
pub mod score_view;
pub mod step_sequencer;
pub mod tempo_map;
pub mod track_groups;
pub mod tracks;
pub mod transport;

// ─── Batch 4 domain modules ───────────────────────────────────────────────────
pub mod beat_detection;
pub mod clip_launch;
pub mod chord_tools;
pub mod freeze;
pub mod midi_routing;

// ─── Batch 5 AI generation modules ───────────────────────────────────────────
pub mod musicgen;
pub mod demucs;
pub mod magenta;
// ─── Batch 5 domain modules ───────────────────────────────────────────────────
pub mod ai_mastering;
pub mod vocal_tools;
pub mod smart_mix;
// ─── Batch 5 domain modules ───────────────────────────────────────────────────
pub mod midi_controllers;
pub mod loop_recording;
pub mod hardware_sync;

// ─── Re-exports ──────────────────────────────────────────────────────────────

pub use ai::{AiGenerationJob, AiGenerationStatus};
pub use audio_engine::{AudioEngineConfig, AudioInputDevice, AudioOutputDevice};
pub use bus_routing::{BusTrack, MasterBus};
pub use clips::{ClipKind, ToneClip};
pub use export::{BounceConfig, BounceFormat};
pub use history::HistoryEntry;
pub use midi::{MidiCC, MidiNote, NudgeAmount, NudgeDirection};
pub use midi_control::{MappingTarget, MidiDevice, MidiDeviceKind, MidiMapping};
pub use mixer::{EqBand, MixerChannel};
pub use plugins::{BuiltinSynth, BuiltinSynthKind, DrumKit, PluginFormat, PluginInstance, SamplerLoop};
pub use project::ToneProject;
pub use recording::{PunchConfig, RecordingSession, TakeInfo, TakeManager};
pub use scenes::{ArrangementMode, FollowAction, Scene, SceneSlot};
pub use score_view::{Clef, ChordQuality, ChordSymbol, QuantizeDisplay, ScoreView, StemDirection};
pub use step_sequencer::{ChainPattern, StepCell, StepPattern};
pub use tempo_map::{TempoEvent, TimeSigEvent};
pub use track_groups::TrackGroup;
pub use tracks::{InsertEffect, InsertEffectKind, ToneTrack, TrackKind, TrackSend};

// ─── Batch 4 re-exports ───────────────────────────────────────────────────────
pub use beat_detection::{AnalysisStatus, BeatDetectionResult, StretchAlgorithm, StretchConfig, WarpMarker};
pub use clip_launch::{ClipSlot, LaunchGrid, LaunchQuantize, SlotAction, SlotFollowAction};
pub use chord_tools::{ChordDegree, ChordProgression, ChordQualityTone, ChordSuggestion, ChordVoicing, ScaleMode};
pub use freeze::{FreezeState, FrozenTrackInfo, StemConfig, StemExportStatus, StemFormat};
pub use midi_routing::{ArpConfig, ArpPattern, MidiOutputPort, VirtualInstrument, VirtualInstrumentKind};

// ─── Batch 5 AI generation re-exports ────────────────────────────────────────
pub use musicgen::{MusicGenStatus, MusicGenJob};
pub use demucs::{DemucsStatus, StemKind, StemOutput, DemucsJob};
pub use magenta::{MagentaModel, MagentaStatus, MelodyGenJob, MelodyContinueJob, ChordVoicingJob};
// ─── Batch 5 re-exports ───────────────────────────────────────────────────────
pub use ai_mastering::{LufsTarget, AiMasterStatus, MultibandComp, AiMasterJob};
pub use vocal_tools::{VocalJobKind, VocalJobStatus, AutoTuneConfig, HarmonyConfig, VocalJob};
pub use smart_mix::{FreqAnalysis, MixSuggestion, AutoMixStatus, AutoMixSession};
// ─── Batch 5 re-exports ───────────────────────────────────────────────────────
pub use midi_controllers::{ControllerPreset, PadMapping, KnobMapping, HardwareController, MidiLearnCtrlState};
pub use loop_recording::{TakeStatus, Take, CompRegion, TakeStack};
pub use hardware_sync::{MidiClockSource, MidiClockStatus, MidiClockConfig, AbletonLinkConfig};

// ─── Phase 2 types (data model only) ─────────────────────────────────────────

/// The active editing tool in the piano roll / timeline.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToneTool {
    /// Pointer / selection tool.
    Select,
    /// Pencil / draw tool — click to create notes or clips.
    Draw,
    /// Eraser tool — click to delete notes or clips.
    Erase,
}

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

    // ── Tool selection ─────────────────────────────────────────────────────
    /// Set the active editing tool (Select / Draw / Erase).
    SetActiveTool(ToneTool),

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

    // ── Automation ─────────────────────────────────────────────────────────
    /// Create an automation lane for a parameter on the given track.
    CreateAutomationLane { track_id: usize, parameter: automation::AutomationParameter },
    /// Delete an automation lane by id.
    DeleteAutomationLane { lane_id: usize },
    /// Add a control point to an automation lane (value clamped 0..=1).
    AddAutomationPoint { lane_id: usize, beat: f32, value: f32 },
    /// Remove a control point from an automation lane.
    RemoveAutomationPoint { lane_id: usize, point_id: usize },
    /// Move a control point to a new beat/value (re-sorts lane by beat).
    MoveAutomationPoint { lane_id: usize, point_id: usize, beat: f32, value: f32 },
    /// Set the curve bias of a control point. Clamped -1..=1.
    SetAutomationCurve { lane_id: usize, point_id: usize, curve: f32 },
    /// Enable or disable an automation lane.
    SetAutomationLaneEnabled { lane_id: usize, enabled: bool },
    /// Show or hide an automation lane in the UI.
    SetAutomationLaneVisible { lane_id: usize, visible: bool },
    /// Set the global automation record mode.
    SetAutomationMode(automation::AutomationMode),
    /// Remove all control points from an automation lane.
    ClearAutomationLane { lane_id: usize },

    // ── Tempo Map ──────────────────────────────────────────────────────────
    /// Add a tempo change event at the given bar (bpm clamped 20..=999).
    AddTempoEvent { bar: f32, bpm: f32 },
    /// Remove a tempo event (cannot remove the bar-1 anchor).
    RemoveTempoEvent { event_id: usize },
    /// Move a tempo event to a new bar (cannot move bar-1 anchor).
    MoveTempoEvent { event_id: usize, bar: f32 },
    /// Add a time signature change event at the given bar.
    AddTimeSigEvent { bar: f32, numerator: u8, denominator: u8 },
    /// Remove a time signature event (cannot remove bar-1 anchor).
    RemoveTimeSigEvent { event_id: usize },
    /// Set the global BPM (updates project.bpm and bar-1 anchor event).
    SetGlobalBpm(f32),

    // ── Scenes ─────────────────────────────────────────────────────────────
    /// Add a new scene with the given name.
    AddScene { name: String },
    /// Delete a scene. Clears active_scene_id if it matches.
    DeleteScene { scene_id: usize },
    /// Rename a scene.
    RenameScene { scene_id: usize, name: String },
    /// Set the display color of a scene.
    SetSceneColor { scene_id: usize, color: String },
    /// Override the BPM for a scene (None = use project BPM).
    SetSceneBpmOverride { scene_id: usize, bpm: Option<f32> },
    /// Assign (or clear) the clip for a track slot within a scene.
    AssignClipToScene { scene_id: usize, track_id: usize, clip_id: Option<usize> },
    /// Set the follow action for a track slot within a scene.
    SetFollowAction { scene_id: usize, track_id: usize, action: FollowAction },
    /// Launch a scene (set it as active).
    LaunchScene { scene_id: usize },
    /// Switch between session and arrangement view mode.
    SetArrangementMode(ArrangementMode),
    /// Duplicate a scene (new id, name gains " (copy)").
    DuplicateScene { scene_id: usize },

    // ── Plugins ────────────────────────────────────────────────────────────
    /// Load a plugin onto a track.
    AddPlugin { track_id: usize, name: String, format: PluginFormat, path: String, is_instrument: bool },
    /// Unload a plugin.
    RemovePlugin { plugin_id: usize },
    /// Toggle a plugin's enabled state.
    TogglePlugin { plugin_id: usize },
    /// Set the active preset name on a plugin.
    SetPluginPreset { plugin_id: usize, preset_name: String },
    /// Set a named parameter value on a plugin.
    SetPluginParam { plugin_id: usize, param_name: String, value: f32 },
    /// Open the plugin's floating editor window.
    OpenPluginWindow { plugin_id: usize },
    /// Close the plugin's floating editor window.
    ClosePluginWindow { plugin_id: usize },
    /// Add a builtin synthesizer to a track.
    AddBuiltinSynth { track_id: usize, kind: BuiltinSynthKind },
    /// Remove the builtin synth from a track.
    RemoveBuiltinSynth { track_id: usize },
    /// Set the octave transpose of a builtin synth. Clamped -4..=4.
    SetBuiltinSynthOctave { track_id: usize, octave: i32 },

    // ── MIDI Control ───────────────────────────────────────────────────────
    /// Add a MIDI CC → parameter mapping.
    AddMidiMapping { channel: u8, cc: u8, target: MappingTarget },
    /// Remove a MIDI mapping.
    RemoveMidiMapping { mapping_id: usize },
    /// Set the output range of a MIDI mapping.
    SetMappingRange { mapping_id: usize, min: f32, max: f32 },
    /// Register a MIDI device.
    AddMidiDevice { name: String, kind: MidiDeviceKind },
    /// Unregister a MIDI device.
    RemoveMidiDevice { device_id: usize },
    /// Enable or disable a MIDI device.
    EnableMidiDevice { device_id: usize, enabled: bool },
    /// Start MIDI learn mode for the given target.
    StartMidiLearn { target: MappingTarget },
    /// Cancel MIDI learn without creating a mapping.
    StopMidiLearn,
    /// Complete MIDI learn: create a mapping from the learned channel+cc to the pending target.
    CompleteMidiLearn { channel: u8, cc: u8 },

    // ── Audio Engine (Batch 3) ─────────────────────────────────────────────
    /// Set the audio input and/or output device by name.
    SetAudioDevice { input: Option<String>, output: Option<String> },
    /// Set the audio buffer size (clamped to 64/128/256/512/1024).
    SetBufferSize(u32),
    /// Set the audio sample rate (44100/48000/88200/96000).
    SetAudioSampleRate(u32),
    /// Enable or disable low-latency mode.
    SetLowLatencyMode(bool),
    /// Enable or disable exclusive audio device mode.
    SetExclusiveMode(bool),
    /// Start the audio engine.
    StartAudioEngine,
    /// Stop the audio engine.
    StopAudioEngine,
    /// Report an audio overload (xrun).
    ReportAudioOverload,
    /// Reset the overload counter.
    ResetOverloadCount,
    /// Register an audio input device.
    AddInputDevice { name: String, channels: u32, is_default: bool },
    /// Register an audio output device.
    AddOutputDevice { name: String, channels: u32, is_default: bool },
    /// Select the active input device by id.
    SelectInputDevice { device_id: Option<usize> },
    /// Select the active output device by id.
    SelectOutputDevice { device_id: Option<usize> },

    // ── Recording (Batch 3) ────────────────────────────────────────────────
    /// Start recording on the given tracks.
    StartRecording { track_ids: Vec<usize> },
    /// Stop all active recording sessions.
    StopRecording,
    /// Configure punch-in point.
    SetPunchIn { enabled: bool, beat: f32 },
    /// Configure punch-out point.
    SetPunchOut { enabled: bool, beat: f32 },
    /// Enable or disable auto-punch.
    SetAutoPunch(bool),
    /// Configure count-in before recording.
    SetCountIn { enabled: bool, bars: u32 },
    /// Add a take for a track (associates a clip).
    AddTake { track_id: usize, clip_id: usize },
    /// Set the active take for a track.
    SetActiveTake { track_id: usize, take_id: usize },
    /// Mute or unmute a take.
    MuteTake { track_id: usize, take_id: usize, muted: bool },
    /// Delete a take.
    DeleteTake { track_id: usize, take_id: usize },
    /// Enable or disable comp mode on a take manager.
    EnableCompMode { track_id: usize, enabled: bool },
    /// Flatten to active take only.
    FlattenTakes { track_id: usize },

    // ── Step Sequencer (Batch 3) ───────────────────────────────────────────
    /// Add a new step pattern.
    AddPattern { name: String, track_id: usize, steps: u32 },
    /// Delete a step pattern.
    DeletePattern { pattern_id: usize },
    /// Rename a step pattern.
    RenamePattern { pattern_id: usize, name: String },
    /// Set the number of steps (8/16/32 only).
    SetPatternSteps { pattern_id: usize, steps: u32 },
    /// Set the step length in beats.
    SetPatternStepLength { pattern_id: usize, step_length: f32 },
    /// Set the swing amount (0..=1).
    SetPatternSwing { pattern_id: usize, swing: f32 },
    /// Activate or deactivate a step.
    SetStep { pattern_id: usize, step: usize, active: bool },
    /// Set velocity for a step.
    SetStepVelocity { pattern_id: usize, step: usize, velocity: u8 },
    /// Set probability for a step.
    SetStepProbability { pattern_id: usize, step: usize, prob: f32 },
    /// Set accent for a step.
    SetStepAccent { pattern_id: usize, step: usize, accent: bool },
    /// Set skip flag for a step.
    SetStepSkip { pattern_id: usize, step: usize, skip: bool },
    /// Set retrigger count for a step.
    SetStepRetrigger { pattern_id: usize, step: usize, retrigger: u32 },
    /// Clear all active steps in a pattern.
    ClearPattern { pattern_id: usize },
    /// Fill every N steps as active.
    FillPattern { pattern_id: usize, every_n: u32 },
    /// Randomize pattern with given density (0..=1).
    RandomizePattern { pattern_id: usize, density: f32 },
    /// Set the currently active/playing pattern.
    SetActivePattern { pattern_id: Option<usize> },
    /// Add a chain of patterns.
    AddChainPattern { pattern_ids: Vec<usize> },
    /// Delete a chain pattern.
    DeleteChainPattern { chain_id: usize },

    // ── Score View (Batch 3) ───────────────────────────────────────────────
    /// Toggle the score/notation view on or off.
    ToggleScoreView,
    /// Add a track to the score view.
    AddTrackToScore { track_id: usize },
    /// Remove a track from the score view.
    RemoveTrackFromScore { track_id: usize },
    /// Set horizontal zoom of the score view.
    SetScoreZoom(f32),
    /// Set the horizontal scroll position (in beats).
    SetScoreScroll(f32),
    /// Set the clef.
    SetScoreClef(Clef),
    /// Set the key signature (-7..=7 sharps/flats).
    SetScoreKeySignature(i8),
    /// Set the stem direction.
    SetScoreStemDirection(StemDirection),
    /// Set the quantize display resolution.
    SetScoreQuantizeDisplay(QuantizeDisplay),
    /// Toggle chord symbol visibility.
    ToggleChordSymbols,
    /// Add a chord symbol at a beat position.
    AddChordSymbol { beat: f32, root: String, quality: ChordQuality, extension: Option<String> },
    /// Remove chord symbol nearest to at_beat.
    RemoveChordSymbol { at_beat: f32 },
    /// Enable or disable print layout mode.
    SetPrintLayout(bool),

    // ── Bus / Return Track Routing (Batch 3) ──────────────────────────────
    /// Add a bus/return track.
    AddBusTrack { name: String },
    /// Delete a bus track.
    DeleteBusTrack { bus_id: usize },
    /// Rename a bus track.
    RenameBusTrack { bus_id: usize, name: String },
    /// Set the volume of a bus track.
    SetBusVolume { bus_id: usize, volume: f32 },
    /// Set the pan of a bus track.
    SetBusPan { bus_id: usize, pan: f32 },
    /// Mute or unmute a bus track.
    MuteBusTrack { bus_id: usize, muted: bool },
    /// Add an insert effect to a bus track.
    AddBusInsertEffect { bus_id: usize, kind: InsertEffectKind },
    /// Remove an insert effect from a bus track.
    RemoveBusInsertEffect { bus_id: usize, effect_id: usize },
    /// Set the send level from a track into a bus.
    SetSendToBus { from_track_id: usize, bus_id: usize, level: f32 },
    /// Set the master bus volume.
    SetMasterBusVolume(f32),
    /// Set the master limiter threshold (dB).
    SetMasterLimiterThreshold(f32),
    /// Set the master limiter release time (ms).
    SetMasterLimiterRelease(f32),
    /// Configure master dither (bits must be 16 or 24).
    SetMasterDither { enabled: bool, bits: u8 },
    /// Enable or disable master auto-normalize.
    SetMasterAutoNormalize(bool),
    /// Add an insert effect to the master bus.
    AddMasterInsertEffect(InsertEffectKind),
    /// Remove an insert effect from the master bus by index.
    RemoveMasterInsertEffect { index: usize },
    // ── Beat Detection ────────────────────────────────────────────────────────
    AnalyzeBeatDetection { track_id: usize },
    UpdateBeatDetectionStatus { result_id: usize, status: AnalysisStatus },
    ApplyDetectedBpm { result_id: usize },
    AddWarpMarker { clip_id: usize, orig_secs: f32, warp_beats: f32 },
    RemoveWarpMarker { marker_id: usize },
    MoveWarpMarker { marker_id: usize, warp_beats: f32 },
    LockWarpMarker { marker_id: usize, locked: bool },
    SetStretchAlgorithm { clip_id: usize, algorithm: StretchAlgorithm },
    SetFormantPreservation { clip_id: usize, enabled: bool },
    SetTransientSensitivity { clip_id: usize, sensitivity: f32 },

    // ── Clip Launch / Session View ────────────────────────────────────────────
    AddClipSlot { track_id: usize, slot_index: usize },
    AssignClipToSlot { slot_id: usize, clip_id: Option<usize> },
    LaunchSlot { slot_id: usize },
    StopSlot { slot_id: usize },
    QueueSlotAction { slot_id: usize, action: SlotAction },
    ClearSlotQueue { slot_id: usize },
    SetSlotFollowAction { slot_id: usize, action: SlotFollowAction },
    SetSlotFollowTime { slot_id: usize, bars: f32 },
    SetLaunchQuantize(LaunchQuantize),
    SetLinkEnabled(bool),
    StopAllSlots,
    LaunchSceneSlots { scene_id: usize },

    // ── Chord Tools ───────────────────────────────────────────────────────────
    AddChordProgression { name: String, root: String, scale: ScaleMode },
    DeleteChordProgression { progression_id: usize },
    RenameChordProgression { progression_id: usize, name: String },
    AddChordToProgression { progression_id: usize, degree: ChordDegree },
    RemoveChordFromProgression { progression_id: usize, index: usize },
    SetProgressionRoot { progression_id: usize, root: String },
    SetProgressionScale { progression_id: usize, scale: ScaleMode },
    SetActiveProgression(Option<usize>),
    SetScaleLock { root: Option<String>, scale: Option<ScaleMode> },
    ClearChordSuggestions,
    AddChordSuggestion { progression_id: usize, degree: u8, confidence: f32 },

    // ── Freeze / Flatten / Stems ──────────────────────────────────────────────
    FreezeTrack { track_id: usize, pre_fx: bool, tail_secs: f32 },
    UnfreezeTrack { track_id: usize },
    SetFreezeState { track_id: usize, state: FreezeState },
    LockFrozenTrack { track_id: usize, locked: bool },
    FlattenTrack { track_id: usize },
    AddStem { name: String, track_ids: Vec<usize>, format: StemFormat },
    RemoveStem { stem_id: usize },
    SetStemTracks { stem_id: usize, track_ids: Vec<usize> },
    SetStemOutputPath { stem_id: usize, path: String },
    ExportStems,
    UpdateStemStatus { stem_id: usize, status: StemExportStatus },

    // ── MIDI Output Routing + Virtual Instruments ─────────────────────────────
    AddMidiOutputPort { name: String, device: String, channel: u8 },
    RemoveMidiOutputPort { port_id: usize },
    SetMidiPortEnabled { port_id: usize, enabled: bool },
    SetMidiPortTranspose { port_id: usize, semitones: i8 },
    SetMidiPortVelocityScale { port_id: usize, scale: f32 },
    AddVirtualInstrument { name: String, kind: VirtualInstrumentKind },
    RemoveVirtualInstrument { vi_id: usize },
    SetVirtualInstrumentPreset { vi_id: usize, preset: String },
    SetVirtualInstrumentPolyphony { vi_id: usize, voices: u8 },
    ToggleVirtualInstrument { vi_id: usize },
    AddArpeggiator { track_id: usize },
    RemoveArpeggiator { arp_id: usize },
    SetArpPattern { arp_id: usize, pattern: ArpPattern },
    SetArpRate { arp_id: usize, rate: f32 },
    SetArpOctaveRange { arp_id: usize, octaves: u8 },
    ToggleArpeggiator { arp_id: usize },

    // ── MusicGen (Batch 5) ────────────────────────────────────────────────
    QueueMusicGen { prompt: String, style_tag: String, bars: u8, temperature: f32 },
    StartMusicGen { job_id: usize },
    UpdateMusicGenProgress { job_id: usize, bars_done: u8 },
    CompleteMusicGen { job_id: usize, track_id: usize },
    CancelMusicGen { job_id: usize },
    RetryMusicGen { job_id: usize },

    // ── Demucs stem splitting (Batch 5) ───────────────────────────────────
    QueueStemSplit { clip_id: usize },
    StartStemSplit { job_id: usize },
    UpdateStemSplitProgress { job_id: usize, pct: f32 },
    CompleteStemSplit { job_id: usize, stem_track_ids: Vec<usize> },
    CancelStemSplit { job_id: usize },

    // ── Magenta melody / continuation / chord voicing (Batch 5) ──────────
    QueueMelodyGen { track_id: usize, bars: u8, temperature: f32, model: MagentaModel },
    StartMelodyGen { job_id: usize },
    CompleteMelodyGen { job_id: usize, clip_id: usize },
    CancelMelodyGen { job_id: usize },
    QueueMelodyContinue { clip_id: usize, bars: u8, temperature: f32 },
    CompleteMelodyContinue { job_id: usize, clip_id: usize },
    QueueChordVoicing { progression_id: usize, voices: u8 },
    CompleteChordVoicing { job_id: usize, clip_id: usize },
    // ── AI Mastering ──────────────────────────────────────────────────────────
    QueueAiMaster { target: LufsTarget },
    StartAiMaster { job_id: usize },
    UpdateAiMasterProgress { job_id: usize, input_lufs: f32 },
    CompleteAiMaster { job_id: usize, output_lufs: f32 },
    ToggleAiMasterAB { job_id: usize },
    SetAiMasterTarget { job_id: usize, target: LufsTarget },
    CancelAiMaster { job_id: usize },

    // ── Vocal Tools ───────────────────────────────────────────────────────────
    ApplyAutoTune { clip_id: usize, config: AutoTuneConfig },
    GenVocalHarmony { clip_id: usize, config: HarmonyConfig },
    IsolateVocals { clip_id: usize },
    StartVocalJob { job_id: usize },
    CompleteVocalJob { job_id: usize, output_clip_ids: Vec<usize> },
    CancelVocalJob { job_id: usize },

    // ── Smart Mix ─────────────────────────────────────────────────────────────
    StartAutoMix,
    UpdateAutoMixAnalysis { session_id: usize, analyses: Vec<FreqAnalysis> },
    ApplyMixSuggestions { session_id: usize, suggestions: Vec<MixSuggestion> },
    SetAutoMixTarget { session_id: usize, lufs: f32 },
    ResetAutoMix { session_id: usize },
    DiscardAutoMix { session_id: usize },
    // ── MIDI Controllers (Batch 5) ────────────────────────────────────────────
    RegisterController { name: String, preset: ControllerPreset },
    UnregisterController { controller_id: usize },
    SetControllerPreset { controller_id: usize, preset: ControllerPreset },
    ActivateController { controller_id: usize },
    DeactivateController { controller_id: usize },
    StartPadLearn { controller_id: usize, pad_index: u8 },
    CompletePadLearn { controller_id: usize, note: u8 },
    StartKnobLearn { controller_id: usize, knob_index: u8 },
    CompleteKnobLearn { controller_id: usize, cc: u8 },
    CancelCtrlLearn,

    // ── Loop Recording (Batch 5) ──────────────────────────────────────────────
    StartLoopRecord { track_id: usize, beat_start: f32 },
    EndLoopRecord { take_id: usize, beat_end: f32, clip_id: usize },
    DiscardTake { take_id: usize },
    SetLoopActiveTake { stack_id: usize, take_id: usize },
    SetCompRegion { stack_id: usize, beat_start: f32, beat_end: f32 },
    BakeComp { stack_id: usize },
    DeleteTakeStack { stack_id: usize },

    // ── Hardware Sync (Batch 5) ───────────────────────────────────────────────
    SetMidiClockSource { source: MidiClockSource },
    SetMidiClockBpm { bpm: f32 },
    ToggleSendClock,
    ToggleReceiveClock,
    StartMidiClock,
    StopMidiClock,
    SetHardwareLinkEnabled { enabled: bool },
    SetLinkBpm { bpm: f32 },
    UpdateLinkPeers { count: u8 },
    SetLinkQuantum { quantum: f32 },
    ToggleLinkStartStop,
}

// ─── Re-export automation types used in Action ────────────────────────────────

pub mod automation;
pub use automation::{AutomationLane, AutomationMode, AutomationParameter, AutomationPoint, SendParam};

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

    // ── Tool selection ─────────────────────────────────────────────────────
    pub active_tool: ToneTool,

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

    // ── History (Phase 2) ──────────────────────────────────────────────────
    pub undo_stack: Vec<history::HistoryEntry>,
    pub redo_stack: Vec<history::HistoryEntry>,

    // ── MIDI clipboard (Phase 2) ───────────────────────────────────────────
    pub clipboard_notes: Vec<MidiNote>,

    // ── Track groups (Phase 2) ────────────────────────────────────────────
    pub track_groups: Vec<TrackGroup>,
    pub next_group_id: usize,

    // ── Loop region bars (Phase 2) ────────────────────────────────────────
    pub loop_start_bar: f32,
    pub loop_end_bar: f32,
    pub loop_bar_enabled: bool,

    // ── Automation ─────────────────────────────────────────────────────────
    pub automation_lanes: Vec<AutomationLane>,
    pub next_lane_id: usize,
    pub next_auto_point_id: usize,
    pub automation_record_mode: AutomationMode,

    // ── Tempo Map ──────────────────────────────────────────────────────────
    pub tempo_map: Vec<TempoEvent>,
    pub time_sig_map: Vec<TimeSigEvent>,
    pub next_tempo_event_id: usize,
    pub next_time_sig_event_id: usize,

    // ── Scenes ─────────────────────────────────────────────────────────────
    pub scenes: Vec<Scene>,
    pub active_scene_id: Option<usize>,
    pub next_scene_id: usize,
    pub arrangement_mode: ArrangementMode,

    // ── Plugins ────────────────────────────────────────────────────────────
    pub plugin_instances: Vec<PluginInstance>,
    pub builtin_synths: Vec<BuiltinSynth>,
    pub next_plugin_id: usize,

    // ── MIDI Control ───────────────────────────────────────────────────────
    pub midi_mappings: Vec<MidiMapping>,
    pub midi_devices: Vec<MidiDevice>,
    pub next_mapping_id: usize,
    pub next_device_id: usize,
    pub midi_learn_active: bool,
    pub midi_learn_target: Option<MappingTarget>,

    // ── Audio Engine (Batch 3) ─────────────────────────────────────────────
    pub audio_config: AudioEngineConfig,
    pub input_devices: Vec<AudioInputDevice>,
    pub output_devices: Vec<AudioOutputDevice>,
    pub audio_engine_running: bool,
    pub audio_overload_count: u32,
    pub active_input_device_id: Option<usize>,
    pub active_output_device_id: Option<usize>,

    // ── Recording (Batch 3) ────────────────────────────────────────────────
    pub recording_sessions: Vec<RecordingSession>,
    pub punch_config: PunchConfig,
    pub take_managers: Vec<TakeManager>,
    pub next_session_id: usize,
    pub count_in_bars: u32,
    pub count_in_enabled: bool,

    // ── Step Sequencer (Batch 3) ───────────────────────────────────────────
    pub step_patterns: Vec<StepPattern>,
    pub next_pattern_id: usize,
    pub active_pattern_id: Option<usize>,
    pub chain_patterns: Vec<ChainPattern>,
    pub next_chain_id: usize,

    // ── Score View (Batch 3) ───────────────────────────────────────────────
    pub score_view: ScoreView,
    pub chord_symbols: Vec<ChordSymbol>,
    pub next_chord_id: usize,

    // ── Bus / Return Track Routing (Batch 3) ──────────────────────────────
    pub bus_tracks: Vec<BusTrack>,
    pub master_bus: MasterBus,
    pub next_bus_track_id: usize,

    // ── Beat Detection (Batch 4) ──────────────────────────────────────────────
    pub beat_results: Vec<BeatDetectionResult>,
    pub next_beat_result_id: usize,
    pub warp_markers: Vec<WarpMarker>,
    pub next_warp_marker_id: usize,
    pub stretch_configs: std::collections::HashMap<usize, StretchConfig>,

    // ── Clip Launch (Batch 4) ─────────────────────────────────────────────────
    pub clip_slots: Vec<ClipSlot>,
    pub next_slot_id: usize,
    pub launch_grid: LaunchGrid,

    // ── Chord Tools (Batch 4) ─────────────────────────────────────────────────
    pub chord_progressions: Vec<ChordProgression>,
    pub next_progression_id: usize,
    pub active_progression_id: Option<usize>,
    pub chord_suggestions: Vec<ChordSuggestion>,
    pub scale_lock: Option<(String, ScaleMode)>,

    // ── Freeze / Stems (Batch 4) ──────────────────────────────────────────────
    pub frozen_tracks: std::collections::HashMap<usize, FrozenTrackInfo>,
    pub stems: Vec<StemConfig>,
    pub next_stem_id: usize,

    // ── MIDI Routing / VI / Arp (Batch 4) ────────────────────────────────────
    pub midi_output_ports: Vec<MidiOutputPort>,
    pub next_midi_port_id: usize,
    pub virtual_instruments: Vec<VirtualInstrument>,
    pub next_vi_id: usize,
    pub arp_configs: Vec<ArpConfig>,
    pub next_arp_id: usize,

    // ── MusicGen AI (Batch 5) ──────────────────────────────────────────────
    pub musicgen_jobs: Vec<MusicGenJob>,
    pub next_musicgen_id: usize,

    // ── Demucs stem splitting (Batch 5) ───────────────────────────────────
    pub demucs_jobs: Vec<DemucsJob>,
    pub next_demucs_id: usize,

    // ── Magenta melody / continuation / chord voicing (Batch 5) ──────────
    pub melody_gen_jobs: Vec<MelodyGenJob>,
    pub next_melody_gen_id: usize,
    pub melody_cont_jobs: Vec<MelodyContinueJob>,
    pub next_melody_cont_id: usize,
    pub chord_voicing_jobs: Vec<ChordVoicingJob>,
    pub next_chord_voicing_id: usize,
    // ── AI Mastering (Batch 5) ────────────────────────────────────────────────
    pub aimaster_jobs: Vec<AiMasterJob>,
    pub next_aimaster_id: usize,

    // ── Vocal Tools (Batch 5) ─────────────────────────────────────────────────
    pub vocal_jobs: Vec<VocalJob>,
    pub auto_tune_configs: Vec<(usize, AutoTuneConfig)>,
    pub next_vocal_job_id: usize,

    // ── Smart Mix (Batch 5) ───────────────────────────────────────────────────
    pub automix_sessions: Vec<AutoMixSession>,
    pub next_automix_id: usize,
    // ── MIDI Controllers (Batch 5) ────────────────────────────────────────────
    pub hardware_controllers: Vec<HardwareController>,
    pub next_controller_id: usize,
    pub ctrl_learn_state: MidiLearnCtrlState,

    // ── Loop Recording (Batch 5) ──────────────────────────────────────────────
    pub take_stacks: Vec<TakeStack>,
    pub loop_takes: Vec<Take>,
    pub next_take_stack_id: usize,
    pub next_take_id: usize,

    // ── Hardware Sync (Batch 5) ───────────────────────────────────────────────
    pub midi_clock: MidiClockConfig,
    pub ableton_link: AbletonLinkConfig,
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
            active_tool: ToneTool::Select,
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
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            clipboard_notes: Vec::new(),
            track_groups: Vec::new(),
            next_group_id: 0,
            loop_start_bar: 1.0,
            loop_end_bar: 5.0,
            loop_bar_enabled: false,
            automation_lanes: Vec::new(),
            next_lane_id: 0,
            next_auto_point_id: 0,
            automation_record_mode: AutomationMode::Read,
            tempo_map: vec![TempoEvent { id: 0, bar: 1.0, bpm: 120.0 }],
            time_sig_map: vec![TimeSigEvent { id: 0, bar: 1.0, numerator: 4, denominator: 4 }],
            next_tempo_event_id: 1,
            next_time_sig_event_id: 1,
            scenes: Vec::new(),
            active_scene_id: None,
            next_scene_id: 0,
            arrangement_mode: ArrangementMode::Arrangement,
            plugin_instances: Vec::new(),
            builtin_synths: Vec::new(),
            next_plugin_id: 0,
            midi_mappings: Vec::new(),
            midi_devices: Vec::new(),
            next_mapping_id: 0,
            next_device_id: 0,
            midi_learn_active: false,
            midi_learn_target: None,
            // Batch 3 fields
            audio_config: AudioEngineConfig::new(),
            input_devices: Vec::new(),
            output_devices: Vec::new(),
            audio_engine_running: false,
            audio_overload_count: 0,
            active_input_device_id: None,
            active_output_device_id: None,
            recording_sessions: Vec::new(),
            punch_config: PunchConfig::new(),
            take_managers: Vec::new(),
            next_session_id: 0,
            count_in_bars: 1,
            count_in_enabled: false,
            step_patterns: Vec::new(),
            next_pattern_id: 0,
            active_pattern_id: None,
            chain_patterns: Vec::new(),
            next_chain_id: 0,
            score_view: ScoreView::new(),
            chord_symbols: Vec::new(),
            next_chord_id: 0,
            bus_tracks: Vec::new(),
            master_bus: MasterBus::new(),
            next_bus_track_id: 0,
            // Batch 4 fields
            beat_results: Vec::new(),
            next_beat_result_id: 0,
            warp_markers: Vec::new(),
            next_warp_marker_id: 0,
            stretch_configs: std::collections::HashMap::new(),
            clip_slots: Vec::new(),
            next_slot_id: 0,
            launch_grid: LaunchGrid::new(),
            chord_progressions: Vec::new(),
            next_progression_id: 0,
            active_progression_id: None,
            chord_suggestions: Vec::new(),
            scale_lock: None,
            frozen_tracks: std::collections::HashMap::new(),
            stems: Vec::new(),
            next_stem_id: 0,
            midi_output_ports: Vec::new(),
            next_midi_port_id: 0,
            virtual_instruments: Vec::new(),
            next_vi_id: 0,
            arp_configs: Vec::new(),
            next_arp_id: 0,
            // Batch 5 fields
            musicgen_jobs: Vec::new(),
            next_musicgen_id: 1,
            demucs_jobs: Vec::new(),
            next_demucs_id: 1,
            melody_gen_jobs: Vec::new(),
            next_melody_gen_id: 1,
            melody_cont_jobs: Vec::new(),
            next_melody_cont_id: 1,
            chord_voicing_jobs: Vec::new(),
            next_chord_voicing_id: 1,
            aimaster_jobs: Vec::new(),
            next_aimaster_id: 1,
            vocal_jobs: Vec::new(),
            auto_tune_configs: Vec::new(),
            next_vocal_job_id: 1,
            automix_sessions: Vec::new(),
            next_automix_id: 1,
            hardware_controllers: vec![],
            next_controller_id: 1,
            ctrl_learn_state: MidiLearnCtrlState::Idle,
            take_stacks: vec![],
            loop_takes: vec![],
            next_take_stack_id: 1,
            next_take_id: 1,
            midi_clock: MidiClockConfig::default(),
            ableton_link: AbletonLinkConfig::default(),
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
            | Action::ToggleSend { .. }
            | Action::AddInsertEffect { .. }
            | Action::RemoveInsertEffect { .. }
            | Action::ToggleInsertEffect { .. }
            | Action::ReorderInsertEffect { .. }
            | Action::SetInsertGain { .. } => self.apply_tracks(action),

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
            | Action::InvalidatePeakCache { .. }
            | Action::DuplicateClip { .. }
            | Action::ConsolidateClips { .. }
            | Action::SetClipColor { .. }
            | Action::TrimClipStart { .. }
            | Action::TrimClipEnd { .. }
            | Action::SetClipPitchF32 { .. }
            | Action::SetClipGainDb { .. } => self.apply_clips(action),

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
            | Action::MoveMidiCC { .. }
            | Action::SelectNote { .. }
            | Action::DeselectNote { .. }
            | Action::DeselectAllNotes
            | Action::CopySelectedNotes
            | Action::PasteNotes { .. }
            | Action::NudgeNotes { .. }
            | Action::DeleteSelectedNotes
            | Action::MoveSelectedNotes { .. }
            | Action::ResizeSelectedNotes { .. }
            | Action::SetSelectedNotesVelocity { .. }
            | Action::QuantizeSelectedNotes { .. } => self.apply_midi(action),

            // ── Tool selection ────────────────────────────────────────────────
            Action::SetActiveTool(tool) => {
                self.active_tool = *tool;
            }

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
            | Action::ToggleMasterLimiter
            | Action::AddChannelSend { .. }
            | Action::RemoveChannelSend { .. }
            | Action::SetChannelSendLevel { .. }
            | Action::SetChannelPfl { .. }
            | Action::SetChannelPhaseInvert { .. }
            | Action::SetStereoWidth { .. }
            | Action::SetChannelTrim { .. }
            | Action::ResetChannel { .. } => self.apply_mixer(action),

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
            | Action::FastForward
            | Action::SetLoopRegion { .. }
            | Action::SetLoopBarEnabled(..)
            | Action::MoveLoopRegion { .. }
            | Action::ResizeLoopStart { .. }
            | Action::ResizeLoopEnd { .. } => self.apply_transport(action),

            // ── History ──────────────────────────────────────────────────────
            Action::Undo | Action::Redo | Action::PushUndoCheckpoint { .. } => {
                self.apply_history(action)
            }

            // ── Track groups ─────────────────────────────────────────────────
            Action::CreateTrackGroup { .. }
            | Action::AddTrackToGroup { .. }
            | Action::RemoveTrackFromGroup { .. }
            | Action::CollapseGroup { .. }
            | Action::DeleteTrackGroup { .. }
            | Action::SetGroupColor { .. }
            | Action::RenameTrackGroup { .. } => self.apply_track_groups(action),

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

            // ── Automation ────────────────────────────────────────────────────
            Action::CreateAutomationLane { .. }
            | Action::DeleteAutomationLane { .. }
            | Action::AddAutomationPoint { .. }
            | Action::RemoveAutomationPoint { .. }
            | Action::MoveAutomationPoint { .. }
            | Action::SetAutomationCurve { .. }
            | Action::SetAutomationLaneEnabled { .. }
            | Action::SetAutomationLaneVisible { .. }
            | Action::SetAutomationMode(..)
            | Action::ClearAutomationLane { .. } => self.apply_automation(action),

            // ── Tempo Map ─────────────────────────────────────────────────────
            Action::AddTempoEvent { .. }
            | Action::RemoveTempoEvent { .. }
            | Action::MoveTempoEvent { .. }
            | Action::AddTimeSigEvent { .. }
            | Action::RemoveTimeSigEvent { .. }
            | Action::SetGlobalBpm(..) => self.apply_tempo_map(action),

            // ── Scenes ────────────────────────────────────────────────────────
            Action::AddScene { .. }
            | Action::DeleteScene { .. }
            | Action::RenameScene { .. }
            | Action::SetSceneColor { .. }
            | Action::SetSceneBpmOverride { .. }
            | Action::AssignClipToScene { .. }
            | Action::SetFollowAction { .. }
            | Action::LaunchScene { .. }
            | Action::SetArrangementMode(..)
            | Action::DuplicateScene { .. } => self.apply_scenes(action),

            // ── Plugins ───────────────────────────────────────────────────────
            Action::AddPlugin { .. }
            | Action::RemovePlugin { .. }
            | Action::TogglePlugin { .. }
            | Action::SetPluginPreset { .. }
            | Action::SetPluginParam { .. }
            | Action::OpenPluginWindow { .. }
            | Action::ClosePluginWindow { .. }
            | Action::AddBuiltinSynth { .. }
            | Action::RemoveBuiltinSynth { .. }
            | Action::SetBuiltinSynthOctave { .. } => self.apply_plugins(action),

            // ── MIDI Control ──────────────────────────────────────────────────
            Action::AddMidiMapping { .. }
            | Action::RemoveMidiMapping { .. }
            | Action::SetMappingRange { .. }
            | Action::AddMidiDevice { .. }
            | Action::RemoveMidiDevice { .. }
            | Action::EnableMidiDevice { .. }
            | Action::StartMidiLearn { .. }
            | Action::StopMidiLearn
            | Action::CompleteMidiLearn { .. } => self.apply_midi_control(action),

            // ── Audio Engine ──────────────────────────────────────────────────
            Action::SetAudioDevice { .. }
            | Action::SetBufferSize(..)
            | Action::SetAudioSampleRate(..)
            | Action::SetLowLatencyMode(..)
            | Action::SetExclusiveMode(..)
            | Action::StartAudioEngine
            | Action::StopAudioEngine
            | Action::ReportAudioOverload
            | Action::ResetOverloadCount
            | Action::AddInputDevice { .. }
            | Action::AddOutputDevice { .. }
            | Action::SelectInputDevice { .. }
            | Action::SelectOutputDevice { .. } => self.apply_audio_engine(action),

            // ── Recording ─────────────────────────────────────────────────────
            Action::StartRecording { .. }
            | Action::StopRecording
            | Action::SetPunchIn { .. }
            | Action::SetPunchOut { .. }
            | Action::SetAutoPunch(..)
            | Action::SetCountIn { .. }
            | Action::AddTake { .. }
            | Action::SetActiveTake { .. }
            | Action::MuteTake { .. }
            | Action::DeleteTake { .. }
            | Action::EnableCompMode { .. }
            | Action::FlattenTakes { .. } => self.apply_recording(action),

            // ── Step Sequencer ────────────────────────────────────────────────
            Action::AddPattern { .. }
            | Action::DeletePattern { .. }
            | Action::RenamePattern { .. }
            | Action::SetPatternSteps { .. }
            | Action::SetPatternStepLength { .. }
            | Action::SetPatternSwing { .. }
            | Action::SetStep { .. }
            | Action::SetStepVelocity { .. }
            | Action::SetStepProbability { .. }
            | Action::SetStepAccent { .. }
            | Action::SetStepSkip { .. }
            | Action::SetStepRetrigger { .. }
            | Action::ClearPattern { .. }
            | Action::FillPattern { .. }
            | Action::RandomizePattern { .. }
            | Action::SetActivePattern { .. }
            | Action::AddChainPattern { .. }
            | Action::DeleteChainPattern { .. } => self.apply_step_sequencer(action),

            // ── Score View ────────────────────────────────────────────────────
            Action::ToggleScoreView
            | Action::AddTrackToScore { .. }
            | Action::RemoveTrackFromScore { .. }
            | Action::SetScoreZoom(..)
            | Action::SetScoreScroll(..)
            | Action::SetScoreClef(..)
            | Action::SetScoreKeySignature(..)
            | Action::SetScoreStemDirection(..)
            | Action::SetScoreQuantizeDisplay(..)
            | Action::ToggleChordSymbols
            | Action::AddChordSymbol { .. }
            | Action::RemoveChordSymbol { .. }
            | Action::SetPrintLayout(..) => self.apply_score_view(action),

            // ── Bus Routing ───────────────────────────────────────────────────
            Action::AddBusTrack { .. }
            | Action::DeleteBusTrack { .. }
            | Action::RenameBusTrack { .. }
            | Action::SetBusVolume { .. }
            | Action::SetBusPan { .. }
            | Action::MuteBusTrack { .. }
            | Action::AddBusInsertEffect { .. }
            | Action::RemoveBusInsertEffect { .. }
            | Action::SetSendToBus { .. }
            | Action::SetMasterBusVolume(..)
            | Action::SetMasterLimiterThreshold(..)
            | Action::SetMasterLimiterRelease(..)
            | Action::SetMasterDither { .. }
            | Action::SetMasterAutoNormalize(..)
            | Action::AddMasterInsertEffect(..)
            | Action::RemoveMasterInsertEffect { .. } => self.apply_bus_routing(action),
            // ── Beat Detection ────────────────────────────────────────────────
            Action::AnalyzeBeatDetection { .. }
            | Action::UpdateBeatDetectionStatus { .. }
            | Action::ApplyDetectedBpm { .. }
            | Action::AddWarpMarker { .. }
            | Action::RemoveWarpMarker { .. }
            | Action::MoveWarpMarker { .. }
            | Action::LockWarpMarker { .. }
            | Action::SetStretchAlgorithm { .. }
            | Action::SetFormantPreservation { .. }
            | Action::SetTransientSensitivity { .. } => self.apply_beat_detection(action),

            // ── Clip Launch ───────────────────────────────────────────────────
            Action::AddClipSlot { .. }
            | Action::AssignClipToSlot { .. }
            | Action::LaunchSlot { .. }
            | Action::StopSlot { .. }
            | Action::QueueSlotAction { .. }
            | Action::ClearSlotQueue { .. }
            | Action::SetSlotFollowAction { .. }
            | Action::SetSlotFollowTime { .. }
            | Action::SetLaunchQuantize(..)
            | Action::SetLinkEnabled(..)
            | Action::StopAllSlots
            | Action::LaunchSceneSlots { .. } => self.apply_clip_launch(action),

            // ── Chord Tools ───────────────────────────────────────────────────
            Action::AddChordProgression { .. }
            | Action::DeleteChordProgression { .. }
            | Action::RenameChordProgression { .. }
            | Action::AddChordToProgression { .. }
            | Action::RemoveChordFromProgression { .. }
            | Action::SetProgressionRoot { .. }
            | Action::SetProgressionScale { .. }
            | Action::SetActiveProgression(..)
            | Action::SetScaleLock { .. }
            | Action::ClearChordSuggestions
            | Action::AddChordSuggestion { .. } => self.apply_chord_tools(action),

            // ── Freeze / Stems ─────────────────────────────────────────────────
            Action::FreezeTrack { .. }
            | Action::UnfreezeTrack { .. }
            | Action::SetFreezeState { .. }
            | Action::LockFrozenTrack { .. }
            | Action::FlattenTrack { .. }
            | Action::AddStem { .. }
            | Action::RemoveStem { .. }
            | Action::SetStemTracks { .. }
            | Action::SetStemOutputPath { .. }
            | Action::ExportStems
            | Action::UpdateStemStatus { .. } => self.apply_freeze(action),

            // ── MIDI Routing / VI / Arp ───────────────────────────────────────
            Action::AddMidiOutputPort { .. }
            | Action::RemoveMidiOutputPort { .. }
            | Action::SetMidiPortEnabled { .. }
            | Action::SetMidiPortTranspose { .. }
            | Action::SetMidiPortVelocityScale { .. }
            | Action::AddVirtualInstrument { .. }
            | Action::RemoveVirtualInstrument { .. }
            | Action::SetVirtualInstrumentPreset { .. }
            | Action::SetVirtualInstrumentPolyphony { .. }
            | Action::ToggleVirtualInstrument { .. }
            | Action::AddArpeggiator { .. }
            | Action::RemoveArpeggiator { .. }
            | Action::SetArpPattern { .. }
            | Action::SetArpRate { .. }
            | Action::SetArpOctaveRange { .. }
            | Action::ToggleArpeggiator { .. } => self.apply_midi_routing(action),

            // ── MusicGen ──────────────────────────────────────────────────────
            Action::QueueMusicGen { .. }
            | Action::StartMusicGen { .. }
            | Action::UpdateMusicGenProgress { .. }
            | Action::CompleteMusicGen { .. }
            | Action::CancelMusicGen { .. }
            | Action::RetryMusicGen { .. } => self.apply_musicgen(action),

            // ── Demucs stem splitting ─────────────────────────────────────────
            Action::QueueStemSplit { .. }
            | Action::StartStemSplit { .. }
            | Action::UpdateStemSplitProgress { .. }
            | Action::CompleteStemSplit { .. }
            | Action::CancelStemSplit { .. } => self.apply_demucs(action),

            // ── Magenta melody / continuation / chord voicing ─────────────────
            Action::QueueMelodyGen { .. }
            | Action::StartMelodyGen { .. }
            | Action::CompleteMelodyGen { .. }
            | Action::CancelMelodyGen { .. }
            | Action::QueueMelodyContinue { .. }
            | Action::CompleteMelodyContinue { .. }
            | Action::QueueChordVoicing { .. }
            | Action::CompleteChordVoicing { .. } => self.apply_magenta(action),
            // ── AI Mastering ──────────────────────────────────────────────────
            Action::QueueAiMaster { .. }
            | Action::StartAiMaster { .. }
            | Action::UpdateAiMasterProgress { .. }
            | Action::CompleteAiMaster { .. }
            | Action::ToggleAiMasterAB { .. }
            | Action::SetAiMasterTarget { .. }
            | Action::CancelAiMaster { .. } => self.apply_ai_mastering(action),

            // ── Vocal Tools ───────────────────────────────────────────────────
            Action::ApplyAutoTune { .. }
            | Action::GenVocalHarmony { .. }
            | Action::IsolateVocals { .. }
            | Action::StartVocalJob { .. }
            | Action::CompleteVocalJob { .. }
            | Action::CancelVocalJob { .. } => self.apply_vocal_tools(action),

            // ── Smart Mix ─────────────────────────────────────────────────────
            Action::StartAutoMix
            | Action::UpdateAutoMixAnalysis { .. }
            | Action::ApplyMixSuggestions { .. }
            | Action::SetAutoMixTarget { .. }
            | Action::ResetAutoMix { .. }
            | Action::DiscardAutoMix { .. } => self.apply_smart_mix(action),
            // ── MIDI Controllers (Batch 5) ────────────────────────────────────
            Action::RegisterController { .. }
            | Action::UnregisterController { .. }
            | Action::SetControllerPreset { .. }
            | Action::ActivateController { .. }
            | Action::DeactivateController { .. }
            | Action::StartPadLearn { .. }
            | Action::CompletePadLearn { .. }
            | Action::StartKnobLearn { .. }
            | Action::CompleteKnobLearn { .. }
            | Action::CancelCtrlLearn => self.apply_midi_controllers(action),

            // ── Loop Recording (Batch 5) ──────────────────────────────────────
            Action::StartLoopRecord { .. }
            | Action::EndLoopRecord { .. }
            | Action::DiscardTake { .. }
            | Action::SetLoopActiveTake { .. }
            | Action::SetCompRegion { .. }
            | Action::BakeComp { .. }
            | Action::DeleteTakeStack { .. } => self.apply_loop_recording(action),

            // ── Hardware Sync (Batch 5) ───────────────────────────────────────
            Action::SetMidiClockSource { .. }
            | Action::SetMidiClockBpm { .. }
            | Action::ToggleSendClock
            | Action::ToggleReceiveClock
            | Action::StartMidiClock
            | Action::StopMidiClock
            | Action::SetHardwareLinkEnabled { .. }
            | Action::SetLinkBpm { .. }
            | Action::UpdateLinkPeers { .. }
            | Action::SetLinkQuantum { .. }
            | Action::ToggleLinkStartStop => self.apply_hardware_sync(action),
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
