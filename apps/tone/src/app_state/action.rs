//! Action enum, tool enum, and quantize types for the Tone application state.
//!
//! Every mutation in Tone is expressed as an [`Action`] variant. Pass actions
//! through [`super::App::apply`] — never mutate App fields directly.

use super::{
    TrackKind, InsertEffectKind, ClipKind, MidiNote, NudgeDirection, NudgeAmount,
    BounceFormat, FollowAction, ArrangementMode, PluginFormat, BuiltinSynthKind,
    MappingTarget, MidiDeviceKind, AnalysisStatus, StretchAlgorithm, SlotAction,
    SlotFollowAction, LaunchQuantize, ChordDegree, ScaleMode, FreezeState, StemFormat,
    StemExportStatus, VirtualInstrumentKind, ArpPattern, MusicGenStatus, StemKind,
    MagentaModel, LufsTarget, AutoTuneConfig, HarmonyConfig, FreqAnalysis, MixSuggestion,
    ControllerPreset, MidiClockSource, VstPluginInfo, SurroundFormat, SurroundPan,
    SpectralTool, NotationClef, Clef, ChordQuality, StemDirection, QuantizeDisplay,
    AutomationParameter, AutomationMode,
    OnnxModelKind, WaveformPeak,
    ModelId, InferenceBackend,
};

// ── Tool enum ─────────────────────────────────────────────────────────────────

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

// ── Quantize types ────────────────────────────────────────────────────────────

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

// ── Actions ───────────────────────────────────────────────────────────────────

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
    CreateAutomationLane { track_id: usize, parameter: AutomationParameter },
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
    SetAutomationMode(AutomationMode),
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

    // ── VST Host (Batch 5) ─────────────────────────────────────────────────
    ScanVstDirectory { path: String },
    CompletVstScan { found_plugins: Vec<VstPluginInfo> },
    LoadVst { plugin_id: String, track_id: Option<usize>, slot_index: usize },
    UnloadVst { vst_instance_id: usize },
    BypassVst { vst_instance_id: usize, bypass: bool },
    SetVstPreset { vst_instance_id: usize, preset_name: String },
    BlacklistVst { plugin_id: String },

    // ── Surround (Batch 5) ─────────────────────────────────────────────────
    SetSurroundFormat { format: SurroundFormat },
    ToggleSurroundBus,
    ToggleBinauralMonitor,
    SetTrackSurroundPan { track_id: usize, pan: SurroundPan },
    CreateAtmosObject { track_id: usize },
    SetAtmosPan { object_id: usize, pan: SurroundPan },
    RemoveAtmosObject { object_id: usize },

    // ── Spectral (Batch 5) ─────────────────────────────────────────────────
    ToggleSpectralView,
    SetSpectralFftSize { size: usize },
    SetSpectralColorMap { name: String },
    ApplySpectralBrush {
        clip_id: usize,
        freq_low: f32,
        freq_high: f32,
        time_start: f32,
        time_end: f32,
        tool: SpectralTool,
        strength: f32,
    },
    UndoLastSpectralBrush,
    QueueSpectralStretch { clip_id: usize, time_ratio: f32, preserve_pitch: bool },
    CompleteSpectralStretch { job_id: usize },

    // ── Notation (Batch 5) ─────────────────────────────────────────────────
    OpenNotationView { track_id: usize },
    CloseNotationView { staff_id: usize },
    SetStaffClef { staff_id: usize, clef: NotationClef },
    SetStaffKeySig { staff_id: usize, key_sig: i8 },
    SetStaffTranspose { staff_id: usize, semitones: i8 },
    ExportNotationPdf { path: String },
    CompleteNotationExport { export_id: usize },

    // ── Tempo Film (Batch 5) ───────────────────────────────────────────────
    AddTempoChange2 { bar: u32, bpm: f32, smooth: bool },
    RemoveTempoChange { change_id: usize },
    SetTempoChangeBpm { change_id: usize, bpm: f32 },
    AddTimeSigChange2 { bar: u32, num: u8, denom: u8 },
    RemoveTimeSigChange { change_id: usize },
    LinkVideo { path: String, fps: f32 },
    UnlinkVideo,
    SetVideoOffset { frames: i32 },
    ToggleVideoPlaybackLink,

    // ── ONNX Model Management (Batch 5) ──────────────────────────────────────
    RegisterOnnxModel { kind: OnnxModelKind, name: String, url_hint: String, size_mb: u32 },
    StartModelDownload { kind: OnnxModelKind },
    UpdateModelDownload { kind: OnnxModelKind, progress: f32 },
    CompleteModelDownload { kind: OnnxModelKind, local_path: String },
    RemoveOnnxModel { kind: OnnxModelKind },
    QueueOnnxInference { model_kind: OnnxModelKind, input_desc: String },
    CompleteOnnxInference { job_id: usize },
    FailOnnxInference { job_id: usize, error: String },

    // ── Real inference layer (backend + registry path management) ─────────────
    /// Register a local `.onnx` path against a model id for real inference.
    RegisterModelPath { model: ModelId, path: String },
    /// Clear a model's registered path (reverts to stub for that model).
    ClearModelPath { model: ModelId },
    /// Choose which inference backend runs (stub default vs. real `ort`).
    SetInferenceBackend { backend: InferenceBackend },

    // ── Waveform Peak Cache (Batch 5) ─────────────────────────────────────────
    InvalidateWaveform { clip_id: usize },
    SetWaveformPeaks { clip_id: usize, peaks: Vec<WaveformPeak>, sample_rate: u32, pixels_per_second: f32 },
    ClearWaveformCache,

    // ── Welcome screen actions ────────────────────────────────────────────────
    /// Create a new blank project (resets all state). Dispatched from the
    /// welcome screen "New Project" button and template cards.
    NewProject,
    /// Open an existing project via a file picker. Dispatched from the welcome
    /// screen "Open Project..." button. Stub — I/O wired in a later wave.
    OpenFile,
}
