//! The single shared application state for Tone's GPUI host.
//!
//! `App` owns everything panels read or mutate: the active [`ToneProject`]
//! (BPM, key, time signature), the flat lists of [`ToneTrack`]s,
//! [`ToneClip`]s, and [`MidiNote`]s, the mixer channels, the AI generation
//! queue, the bounce config, and the transport state. Panels NEVER mutate
//! `App` fields directly — they emit an [`Action`], and the root view routes
//! it through [`App::apply`], the single choke point for all mutations.

// ─── Project / Session ────────────────────────────────────────────────────────

/// The top-level project settings for a Tone session.
#[derive(Clone, Debug)]
pub struct ToneProject {
    /// Human-readable project name.
    pub name: String,
    /// Beats per minute. Clamped to 20.0..=999.0 by `SetBpm`.
    pub bpm: f32,
    /// Numerator of the time signature (1..=16).
    pub time_signature_num: u8,
    /// Denominator of the time signature. Valid values: 1, 2, 4, 8, 16.
    pub time_signature_den: u8,
    /// Audio sample rate in Hz (44100, 48000, 88200, 96000).
    pub sample_rate: u32,
    /// Bit depth for audio. Valid values: 16, 24, 32.
    pub bit_depth: u8,
    /// Root key of the project ("C", "C#", "D", "Db", …).
    pub key: String,
    /// Scale name ("Major", "Minor", "Dorian", "Mixolydian", …).
    pub scale: String,
}

impl ToneProject {
    /// Sensible defaults: 120 BPM, 4/4, 44100 Hz, 24-bit, C Major.
    pub fn new() -> Self {
        Self {
            name: "Untitled Project".to_string(),
            bpm: 120.0,
            time_signature_num: 4,
            time_signature_den: 4,
            sample_rate: 44100,
            bit_depth: 24,
            key: "C".to_string(),
            scale: "Major".to_string(),
        }
    }
}

impl Default for ToneProject {
    fn default() -> Self {
        Self::new()
    }
}

// ─── Tracks ───────────────────────────────────────────────────────────────────

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
}

impl ToneTrack {
    fn new(id: usize, name: impl Into<String>, kind: TrackKind) -> Self {
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
        }
    }
}

// ─── Clips ────────────────────────────────────────────────────────────────────

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
}

impl ToneClip {
    fn new(id: usize, track_id: usize, name: impl Into<String>, kind: ClipKind, start_beat: f32, duration_beats: f32) -> Self {
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
        }
    }
}

// ─── Piano Roll / MIDI ────────────────────────────────────────────────────────

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
pub struct MidiController {
    pub clip_id: usize,
    /// CC number 0..=127 (e.g. 1 = Mod Wheel, 7 = Volume, 11 = Expression).
    pub cc_number: u8,
    /// Beat position inside the clip.
    pub at_beat: f32,
    /// CC value 0..=127.
    pub value: u8,
}

// ─── Mixer ────────────────────────────────────────────────────────────────────

/// The mixer state for one track channel.
#[derive(Clone, Debug)]
pub struct MixerChannel {
    pub track_id: usize,
    /// Signal level measured before the fader (read-only metering).
    pub pre_fader_level: f32,
    /// Signal level measured after the fader (read-only metering).
    pub post_fader_level: f32,
    /// Peak hold level for the level meter.
    pub peak_level: f32,
    /// Ordered list of effect plugin names in the insert chain.
    pub effects: Vec<String>,
    /// Low-shelf gain in dB. Clamped -12.0..=12.0.
    pub eq_low: f32,
    /// Mid-peak gain in dB. Clamped -12.0..=12.0.
    pub eq_mid: f32,
    /// High-shelf gain in dB. Clamped -12.0..=12.0.
    pub eq_high: f32,
    /// Compressor threshold in dB. Clamped -60.0..=0.0.
    pub comp_threshold: f32,
    /// Compressor ratio (e.g. 4.0 = 4:1). Clamped 1.0..=20.0.
    pub comp_ratio: f32,
    /// Whether the compressor insert is active.
    pub comp_enabled: bool,
}

impl MixerChannel {
    fn new(track_id: usize) -> Self {
        Self {
            track_id,
            pre_fader_level: 0.0,
            post_fader_level: 0.0,
            peak_level: 0.0,
            effects: Vec::new(),
            eq_low: 0.0,
            eq_mid: 0.0,
            eq_high: 0.0,
            comp_threshold: -18.0,
            comp_ratio: 4.0,
            comp_enabled: false,
        }
    }
}

// ─── AI Generation ────────────────────────────────────────────────────────────

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

// ─── Export / Bounce ─────────────────────────────────────────────────────────

/// Audio file format for the bounce output.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BounceFormat {
    Wav,
    Mp3,
    Flac,
    Ogg,
    /// Export each track as a separate file.
    Stems,
}

impl BounceFormat {
    pub fn label(self) -> &'static str {
        match self {
            BounceFormat::Wav => "WAV",
            BounceFormat::Mp3 => "MP3",
            BounceFormat::Flac => "FLAC",
            BounceFormat::Ogg => "OGG",
            BounceFormat::Stems => "Stems",
        }
    }
}

/// Configuration for the current bounce / export run.
#[derive(Clone, Debug)]
pub struct BounceConfig {
    pub format: BounceFormat,
    pub sample_rate: u32,
    pub bit_depth: u8,
    /// Normalise the output to 0 dBFS.
    pub normalize: bool,
    /// Apply noise-shaped dither before truncating to the target bit depth.
    pub dither: bool,
    /// Absolute path where the output file(s) will be written.
    pub export_path: String,
    /// Apply the master FX chain to the bounce.
    pub include_master_fx: bool,
    /// When true (and format = Stems), one file per track is written.
    pub stems_per_track: bool,
}

impl BounceConfig {
    /// Sensible defaults: WAV, 44100 Hz, 24-bit, normalised.
    pub fn new() -> Self {
        Self {
            format: BounceFormat::Wav,
            sample_rate: 44100,
            bit_depth: 24,
            normalize: true,
            dither: false,
            export_path: String::new(),
            include_master_fx: true,
            stems_per_track: false,
        }
    }
}

impl Default for BounceConfig {
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
    /// Split a clip at `at_beat`. The original clip is truncated; a new clip
    /// starts at `at_beat` with the remaining duration.
    SplitClip { id: usize, at_beat: f32 },
    /// Merge a list of clips into the one starting earliest. All other clips
    /// in the list are deleted; the surviving clip's duration spans all.
    MergeClips { ids: Vec<usize> },

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
    /// `grid` is in beats (e.g. 0.25 = 16th note at 4/4 quarter = 1 beat).
    QuantizeMidiNotes { clip_id: usize, grid: f32 },
    /// Transpose all notes in the given clips by `semitones`.
    TransposeMidiNotes { clip_ids: Vec<usize>, semitones: i8 },
    /// Set the horizontal zoom level of the piano roll.
    SetPianoRollZoom(f32),
    /// Set the horizontal scroll offset of the piano roll.
    SetPianoRollScroll(f32),

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

    // ── Transport ──────────────────────────────────────────────────────────
    /// Start playback from the current playhead position.
    Play,
    /// Stop playback and return playhead to loop start (or beat 0 if no loop).
    Stop,
    /// Pause playback at the current position.
    Pause,
    /// Start recording on all armed tracks.
    Record,
    /// Jump the playhead to the given beat.
    SetPlayheadBeat(f32),
    /// Toggle the loop region on/off.
    ToggleLoop,
    /// Set the loop in/out points in beats.
    SetLoopRange { start: f32, end: f32 },
    /// Toggle the click track / metronome.
    ToggleMetronome,
    /// Jump the playhead to beat 0.
    Rewind,
    /// Advance the playhead by 4 beats.
    FastForward,

    // ── AI Generation ──────────────────────────────────────────────────────
    /// Set the free-text prompt for the next AI generation.
    SetAiPrompt(String),
    /// Set the style tag for the next AI generation.
    SetAiStyle(String),
    /// Set the requested output duration in bars. Clamped 1..=128.
    SetAiDurationBars(u8),
    /// Submit an AI generation job for the given track. Pushes an
    /// `AiGenerationJob` with `status = Generating`.
    GenerateTrack { track_id: usize },
    /// Mark an AI generation job as complete and stub a clip on its track.
    CompleteAiGeneration { job_id: usize },
    /// Stub a chord progression (4 notes) on a new MIDI clip in the given track.
    GenerateChordProgression { track_id: usize, bars: u8 },
    /// Stub a 4/4 16-step kick/snare/hi-hat drum pattern on the given track.
    GenerateDrumPattern { track_id: usize },
    /// Harmonise the melody on a track by duplicating all its MIDI notes shifted
    /// up 4 semitones (major 3rd).
    HarmonizeMelody { track_id: usize },
    /// Stub AI mastering: set master_volume = 0.9 and enable the master limiter.
    AiMasterTrack,
    /// Push 4 chord-tone MIDI note stubs into the last active MIDI clip.
    SuggestChords { key: String, mood: String },

    // ── Export / Bounce ────────────────────────────────────────────────────
    /// Set the bounce output format.
    SetBounceFormat(BounceFormat),
    /// Toggle output normalisation.
    SetBounceNormalize(bool),
    /// Toggle noise-shaped dither.
    SetBounceDither(bool),
    /// Set the output file path.
    SetBounceExportPath(String),
    /// Toggle per-track stems output.
    SetBounceStemsPerTrack(bool),
    /// Begin the bounce (sets `bounce_in_progress = true`).
    StartBounce,
    /// Cancel an in-progress bounce.
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
    /// Monotonically increasing counter used to assign unique track ids.
    pub track_counter: usize,
    /// The currently focused track (inspector / piano roll follow this).
    pub active_track: Option<usize>,

    // ── Clips ──────────────────────────────────────────────────────────────
    pub clips: Vec<ToneClip>,
    pub clip_counter: usize,

    // ── MIDI ───────────────────────────────────────────────────────────────
    pub midi_notes: Vec<MidiNote>,
    pub midi_controllers: Vec<MidiController>,
    pub midi_note_counter: usize,
    /// The clip currently open in the piano roll editor.
    pub piano_roll_clip: Option<usize>,
    /// Selected note ids in the piano roll.
    pub selected_notes: Vec<usize>,
    /// Horizontal zoom level of the piano roll (beats per pixel, roughly).
    pub piano_roll_zoom: f32,
    /// Horizontal scroll offset (in beats).
    pub piano_roll_scroll: f32,

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
    /// Free-text prompt for the next generation.
    pub ai_prompt: String,
    /// Style tag for the next generation.
    pub ai_style: String,
    /// Requested output length in bars.
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
            clips: Vec::new(),
            clip_counter: 0,
            midi_notes: Vec::new(),
            midi_controllers: Vec::new(),
            midi_note_counter: 0,
            piano_roll_clip: None,
            selected_notes: Vec::new(),
            piano_roll_zoom: 1.0,
            piano_roll_scroll: 0.0,
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

    // ── Internal helpers ─────────────────────────────────────────────────

    fn find_track_mut(&mut self, id: usize) -> Option<&mut ToneTrack> {
        self.tracks.iter_mut().find(|t| t.id == id)
    }

    fn find_clip_mut(&mut self, id: usize) -> Option<&mut ToneClip> {
        self.clips.iter_mut().find(|c| c.id == id)
    }

    fn find_note_mut(&mut self, id: usize) -> Option<&mut MidiNote> {
        self.midi_notes.iter_mut().find(|n| n.id == id)
    }

    fn find_mixer_mut(&mut self, track_id: usize) -> Option<&mut MixerChannel> {
        self.mixer_channels.iter_mut().find(|m| m.track_id == track_id)
    }

    /// Allocate and return the next clip id.
    fn next_clip_id(&mut self) -> usize {
        let id = self.clip_counter;
        self.clip_counter += 1;
        id
    }

    /// Allocate and return the next MIDI note id.
    fn next_note_id(&mut self) -> usize {
        let id = self.midi_note_counter;
        self.midi_note_counter += 1;
        id
    }

    // ── Apply ────────────────────────────────────────────────────────────

    /// Apply a single action, mutating state in place. This is the only place
    /// state is mutated — panels never touch fields directly.
    pub fn apply(&mut self, action: Action) {
        match action {
            // ── Project ────────────────────────────────────────────────────
            Action::SetBpm(bpm) => {
                self.project.bpm = bpm.clamp(20.0, 999.0);
            }
            Action::SetTimeSignature { num, den } => {
                if num >= 1 && num <= 16 && [1u8, 2, 4, 8, 16].contains(&den) {
                    self.project.time_signature_num = num;
                    self.project.time_signature_den = den;
                }
            }
            Action::SetSampleRate(sr) => {
                if [44100u32, 48000, 88200, 96000].contains(&sr) {
                    self.project.sample_rate = sr;
                }
            }
            Action::SetBitDepth(bd) => {
                if [16u8, 24, 32].contains(&bd) {
                    self.project.bit_depth = bd;
                }
            }
            Action::SetProjectKey(key) => {
                self.project.key = key;
            }
            Action::SetProjectScale(scale) => {
                self.project.scale = scale;
            }
            Action::SetProjectName(name) => {
                self.project.name = name;
            }

            // ── Tracks ─────────────────────────────────────────────────────
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
                // Duplicate clips for this track
                let src_clips: Vec<ToneClip> = self.clips.iter()
                    .filter(|c| c.track_id == id)
                    .cloned()
                    .collect();
                let channel = MixerChannel::new(new_id);
                self.tracks.push(dup);
                self.mixer_channels.push(channel);
                for sc in src_clips {
                    let new_clip_id = self.next_clip_id();
                    let src_notes: Vec<MidiNote> = self.midi_notes.iter()
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

            // ── Clips ──────────────────────────────────────────────────────
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
                // Find the surviving clip (earliest start)
                let survivor_id = to_merge.iter().min_by(|a, b| a.start_beat.partial_cmp(&b.start_beat).unwrap()).map(|c| c.id).unwrap();
                // Delete the others
                let delete_ids: Vec<usize> = ids.iter().copied().filter(|&i| i != survivor_id).collect();
                for did in &delete_ids {
                    // Reparent notes to survivor
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

            // ── Piano Roll / MIDI ──────────────────────────────────────────
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
            Action::SetPianoRollZoom(zoom) => {
                self.piano_roll_zoom = zoom.clamp(0.1, 10.0);
            }
            Action::SetPianoRollScroll(scroll) => {
                self.piano_roll_scroll = scroll.max(0.0);
            }

            // ── Mixer / Master ─────────────────────────────────────────────
            Action::SetChannelEqLow { track_id, gain_db } => {
                if let Some(m) = self.find_mixer_mut(track_id) {
                    m.eq_low = gain_db.clamp(-12.0, 12.0);
                }
            }
            Action::SetChannelEqMid { track_id, gain_db } => {
                if let Some(m) = self.find_mixer_mut(track_id) {
                    m.eq_mid = gain_db.clamp(-12.0, 12.0);
                }
            }
            Action::SetChannelEqHigh { track_id, gain_db } => {
                if let Some(m) = self.find_mixer_mut(track_id) {
                    m.eq_high = gain_db.clamp(-12.0, 12.0);
                }
            }
            Action::SetChannelCompThreshold { track_id, threshold_db } => {
                if let Some(m) = self.find_mixer_mut(track_id) {
                    m.comp_threshold = threshold_db.clamp(-60.0, 0.0);
                }
            }
            Action::SetChannelCompRatio { track_id, ratio } => {
                if let Some(m) = self.find_mixer_mut(track_id) {
                    m.comp_ratio = ratio.clamp(1.0, 20.0);
                }
            }
            Action::ToggleChannelComp(track_id) => {
                if let Some(m) = self.find_mixer_mut(track_id) {
                    m.comp_enabled = !m.comp_enabled;
                }
            }
            Action::AddChannelEffect { track_id, effect } => {
                if let Some(m) = self.find_mixer_mut(track_id) {
                    m.effects.push(effect);
                }
            }
            Action::RemoveChannelEffect { track_id, index } => {
                if let Some(m) = self.find_mixer_mut(track_id) {
                    if index < m.effects.len() {
                        m.effects.remove(index);
                    }
                }
            }
            Action::SetMasterVolume(vol) => {
                self.master_volume = vol.clamp(0.0, 2.0);
            }
            Action::ToggleMasterLimiter => {
                self.master_limiter = !self.master_limiter;
            }

            // ── Transport ──────────────────────────────────────────────────
            Action::Play => {
                self.playing = true;
                self.recording = false;
            }
            Action::Stop => {
                self.playing = false;
                self.recording = false;
                self.playhead_beat = if self.loop_enabled { self.loop_start } else { 0.0 };
            }
            Action::Pause => {
                self.playing = false;
            }
            Action::Record => {
                self.playing = true;
                self.recording = true;
            }
            Action::SetPlayheadBeat(beat) => {
                self.playhead_beat = beat.max(0.0);
            }
            Action::ToggleLoop => {
                self.loop_enabled = !self.loop_enabled;
            }
            Action::SetLoopRange { start, end } => {
                if end > start {
                    self.loop_start = start.max(0.0);
                    self.loop_end = end;
                }
            }
            Action::ToggleMetronome => {
                self.metronome_enabled = !self.metronome_enabled;
            }
            Action::Rewind => {
                self.playhead_beat = 0.0;
                self.playing = false;
            }
            Action::FastForward => {
                self.playhead_beat += 4.0;
            }

            // ── AI Generation ──────────────────────────────────────────────
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
                // Store the clip id on the job for CompleteAiGeneration
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
                // Collect all clip ids on this track
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
                // Stub AI mastering
                self.master_volume = 0.9;
                self.master_limiter = true;
                // Also enable compressor on all channels
                for m in self.mixer_channels.iter_mut() {
                    m.comp_enabled = true;
                    if m.comp_threshold > -6.0 {
                        m.comp_threshold = -6.0;
                    }
                }
            }
            Action::SuggestChords { key, mood } => {
                // Find the last active MIDI clip
                let clip_id = if let Some(cid) = self.piano_roll_clip {
                    cid
                } else if let Some(tid) = self.active_track {
                    self.clips.iter()
                        .filter(|c| c.track_id == tid && matches!(c.kind, ClipKind::Midi | ClipKind::AiGenerated))
                        .last()
                        .map(|c| c.id)
                        .unwrap_or_else(|| {
                            // Create a new clip
                            let cid = self.clip_counter;
                            self.clip_counter += 1;
                            let cl = ToneClip::new(cid, tid, format!("{} {} chords", key, mood), ClipKind::Midi, self.playhead_beat, 4.0);
                            self.clips.push(cl);
                            cid
                        })
                } else {
                    return;
                };
                // Stub 4 notes (Cmaj chord starting on C4)
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

            // ── Bounce ─────────────────────────────────────────────────────
            Action::SetBounceFormat(fmt) => {
                self.bounce_config.format = fmt;
            }
            Action::SetBounceNormalize(v) => {
                self.bounce_config.normalize = v;
            }
            Action::SetBounceDither(v) => {
                self.bounce_config.dither = v;
            }
            Action::SetBounceExportPath(path) => {
                self.bounce_config.export_path = path;
            }
            Action::SetBounceStemsPerTrack(v) => {
                self.bounce_config.stems_per_track = v;
            }
            Action::StartBounce => {
                self.bounce_in_progress = true;
            }
            Action::CancelBounce => {
                self.bounce_in_progress = false;
            }
        }
    }
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn fresh() -> App {
        App::new()
    }

    // ── Project ─────────────────────────────────────────────────────────────

    #[test]
    fn project_defaults() {
        let app = fresh();
        assert_eq!(app.project.bpm, 120.0);
        assert_eq!(app.project.time_signature_num, 4);
        assert_eq!(app.project.time_signature_den, 4);
        assert_eq!(app.project.sample_rate, 44100);
        assert_eq!(app.project.bit_depth, 24);
        assert_eq!(app.project.key, "C");
        assert_eq!(app.project.scale, "Major");
    }

    #[test]
    fn set_bpm_normal() {
        let mut app = fresh();
        app.apply(Action::SetBpm(140.0));
        assert_eq!(app.project.bpm, 140.0);
    }

    #[test]
    fn set_bpm_clamp_low() {
        let mut app = fresh();
        app.apply(Action::SetBpm(0.0));
        assert_eq!(app.project.bpm, 20.0);
    }

    #[test]
    fn set_bpm_clamp_high() {
        let mut app = fresh();
        app.apply(Action::SetBpm(9999.0));
        assert_eq!(app.project.bpm, 999.0);
    }

    #[test]
    fn set_time_signature_valid() {
        let mut app = fresh();
        app.apply(Action::SetTimeSignature { num: 3, den: 4 });
        assert_eq!(app.project.time_signature_num, 3);
        assert_eq!(app.project.time_signature_den, 4);
    }

    #[test]
    fn set_time_signature_invalid_den_ignored() {
        let mut app = fresh();
        app.apply(Action::SetTimeSignature { num: 4, den: 3 });
        // 3 is not a valid denominator — should remain 4/4
        assert_eq!(app.project.time_signature_den, 4);
    }

    #[test]
    fn set_sample_rate_valid() {
        let mut app = fresh();
        app.apply(Action::SetSampleRate(48000));
        assert_eq!(app.project.sample_rate, 48000);
    }

    #[test]
    fn set_sample_rate_invalid_ignored() {
        let mut app = fresh();
        app.apply(Action::SetSampleRate(22050));
        assert_eq!(app.project.sample_rate, 44100);
    }

    #[test]
    fn set_bit_depth_valid() {
        let mut app = fresh();
        app.apply(Action::SetBitDepth(16));
        assert_eq!(app.project.bit_depth, 16);
    }

    #[test]
    fn set_bit_depth_invalid_ignored() {
        let mut app = fresh();
        app.apply(Action::SetBitDepth(20));
        assert_eq!(app.project.bit_depth, 24);
    }

    #[test]
    fn set_project_key() {
        let mut app = fresh();
        app.apply(Action::SetProjectKey("F#".to_string()));
        assert_eq!(app.project.key, "F#");
    }

    #[test]
    fn set_project_scale() {
        let mut app = fresh();
        app.apply(Action::SetProjectScale("Dorian".to_string()));
        assert_eq!(app.project.scale, "Dorian");
    }

    #[test]
    fn set_project_name() {
        let mut app = fresh();
        app.apply(Action::SetProjectName("My Track".to_string()));
        assert_eq!(app.project.name, "My Track");
    }

    // ── Tracks ──────────────────────────────────────────────────────────────

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

    // ── Clips ───────────────────────────────────────────────────────────────

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

    // ── Piano Roll / MIDI ────────────────────────────────────────────────────

    #[test]
    fn open_close_piano_roll() {
        let mut app = fresh();
        app.apply(Action::AddTrack(TrackKind::Midi));
        let tid = app.tracks.last().unwrap().id;
        app.apply(Action::AddClip { track_id: tid, name: "m".into(), kind: ClipKind::Midi, start_beat: 0.0, duration_beats: 4.0 });
        let cid = app.clips[0].id;
        app.apply(Action::OpenPianoRoll(cid));
        assert_eq!(app.piano_roll_clip, Some(cid));
        app.apply(Action::ClosePianoRoll);
        assert_eq!(app.piano_roll_clip, None);
    }

    #[test]
    fn add_midi_note() {
        let mut app = fresh();
        app.apply(Action::AddTrack(TrackKind::Midi));
        let tid = app.tracks.last().unwrap().id;
        app.apply(Action::AddClip { track_id: tid, name: "m".into(), kind: ClipKind::Midi, start_beat: 0.0, duration_beats: 4.0 });
        let cid = app.clips[0].id;
        app.apply(Action::AddMidiNote { clip_id: cid, pitch: 60, velocity: 100, start_beat: 0.0, duration_beats: 1.0 });
        assert_eq!(app.midi_notes.len(), 1);
        assert_eq!(app.midi_notes[0].pitch, 60);
    }

    #[test]
    fn delete_midi_note() {
        let mut app = fresh();
        app.apply(Action::AddTrack(TrackKind::Midi));
        let tid = app.tracks.last().unwrap().id;
        app.apply(Action::AddClip { track_id: tid, name: "m".into(), kind: ClipKind::Midi, start_beat: 0.0, duration_beats: 4.0 });
        let cid = app.clips[0].id;
        app.apply(Action::AddMidiNote { clip_id: cid, pitch: 60, velocity: 100, start_beat: 0.0, duration_beats: 1.0 });
        let nid = app.midi_notes[0].id;
        app.apply(Action::DeleteMidiNote(nid));
        assert!(app.midi_notes.is_empty());
    }

    #[test]
    fn move_midi_note() {
        let mut app = fresh();
        app.apply(Action::AddTrack(TrackKind::Midi));
        let tid = app.tracks.last().unwrap().id;
        app.apply(Action::AddClip { track_id: tid, name: "m".into(), kind: ClipKind::Midi, start_beat: 0.0, duration_beats: 4.0 });
        let cid = app.clips[0].id;
        app.apply(Action::AddMidiNote { clip_id: cid, pitch: 60, velocity: 100, start_beat: 0.0, duration_beats: 1.0 });
        let nid = app.midi_notes[0].id;
        app.apply(Action::MoveMidiNote { id: nid, pitch: 64, start_beat: 2.0 });
        assert_eq!(app.midi_notes[0].pitch, 64);
        assert_eq!(app.midi_notes[0].start_beat, 2.0);
    }

    #[test]
    fn resize_midi_note() {
        let mut app = fresh();
        app.apply(Action::AddTrack(TrackKind::Midi));
        let tid = app.tracks.last().unwrap().id;
        app.apply(Action::AddClip { track_id: tid, name: "m".into(), kind: ClipKind::Midi, start_beat: 0.0, duration_beats: 4.0 });
        let cid = app.clips[0].id;
        app.apply(Action::AddMidiNote { clip_id: cid, pitch: 60, velocity: 100, start_beat: 0.0, duration_beats: 1.0 });
        let nid = app.midi_notes[0].id;
        app.apply(Action::ResizeMidiNote { id: nid, duration_beats: 2.0 });
        assert_eq!(app.midi_notes[0].duration_beats, 2.0);
    }

    #[test]
    fn set_midi_note_velocity_clamp() {
        let mut app = fresh();
        app.apply(Action::AddTrack(TrackKind::Midi));
        let tid = app.tracks.last().unwrap().id;
        app.apply(Action::AddClip { track_id: tid, name: "m".into(), kind: ClipKind::Midi, start_beat: 0.0, duration_beats: 4.0 });
        let cid = app.clips[0].id;
        app.apply(Action::AddMidiNote { clip_id: cid, pitch: 60, velocity: 100, start_beat: 0.0, duration_beats: 1.0 });
        let nid = app.midi_notes[0].id;
        app.apply(Action::SetMidiNoteVelocity { id: nid, velocity: 200 });
        assert_eq!(app.midi_notes[0].velocity, 127);
    }

    #[test]
    fn select_all_notes_in_clip() {
        let mut app = fresh();
        app.apply(Action::AddTrack(TrackKind::Midi));
        let tid = app.tracks.last().unwrap().id;
        app.apply(Action::AddClip { track_id: tid, name: "m".into(), kind: ClipKind::Midi, start_beat: 0.0, duration_beats: 4.0 });
        let cid = app.clips[0].id;
        for pitch in [60u8, 62, 64] {
            app.apply(Action::AddMidiNote { clip_id: cid, pitch, velocity: 80, start_beat: 0.0, duration_beats: 1.0 });
        }
        app.apply(Action::SelectAllNotesInClip(cid));
        assert_eq!(app.selected_notes.len(), 3);
    }

    #[test]
    fn quantize_midi_notes() {
        let mut app = fresh();
        app.apply(Action::AddTrack(TrackKind::Midi));
        let tid = app.tracks.last().unwrap().id;
        app.apply(Action::AddClip { track_id: tid, name: "m".into(), kind: ClipKind::Midi, start_beat: 0.0, duration_beats: 4.0 });
        let cid = app.clips[0].id;
        app.apply(Action::AddMidiNote { clip_id: cid, pitch: 60, velocity: 100, start_beat: 0.13, duration_beats: 1.0 });
        app.apply(Action::QuantizeMidiNotes { clip_id: cid, grid: 0.25 });
        let snapped = app.midi_notes[0].start_beat;
        assert!((snapped - 0.25).abs() < 0.001 || snapped.abs() < 0.001);
    }

    #[test]
    fn transpose_midi_notes() {
        let mut app = fresh();
        app.apply(Action::AddTrack(TrackKind::Midi));
        let tid = app.tracks.last().unwrap().id;
        app.apply(Action::AddClip { track_id: tid, name: "m".into(), kind: ClipKind::Midi, start_beat: 0.0, duration_beats: 4.0 });
        let cid = app.clips[0].id;
        app.apply(Action::AddMidiNote { clip_id: cid, pitch: 60, velocity: 100, start_beat: 0.0, duration_beats: 1.0 });
        app.apply(Action::TransposeMidiNotes { clip_ids: vec![cid], semitones: 7 });
        assert_eq!(app.midi_notes[0].pitch, 67);
    }

    #[test]
    fn set_piano_roll_zoom() {
        let mut app = fresh();
        app.apply(Action::SetPianoRollZoom(2.5));
        assert_eq!(app.piano_roll_zoom, 2.5);
    }

    #[test]
    fn set_piano_roll_zoom_clamp() {
        let mut app = fresh();
        app.apply(Action::SetPianoRollZoom(100.0));
        assert_eq!(app.piano_roll_zoom, 10.0);
    }

    #[test]
    fn set_piano_roll_scroll() {
        let mut app = fresh();
        app.apply(Action::SetPianoRollScroll(16.0));
        assert_eq!(app.piano_roll_scroll, 16.0);
    }

    // ── Mixer ────────────────────────────────────────────────────────────────

    #[test]
    fn set_channel_eq_low() {
        let mut app = fresh();
        app.apply(Action::AddTrack(TrackKind::Audio));
        let tid = app.tracks.last().unwrap().id;
        app.apply(Action::SetChannelEqLow { track_id: tid, gain_db: 6.0 });
        let m = app.mixer_channels.iter().find(|m| m.track_id == tid).unwrap();
        assert_eq!(m.eq_low, 6.0);
    }

    #[test]
    fn set_channel_eq_low_clamp() {
        let mut app = fresh();
        app.apply(Action::AddTrack(TrackKind::Audio));
        let tid = app.tracks.last().unwrap().id;
        app.apply(Action::SetChannelEqLow { track_id: tid, gain_db: -20.0 });
        let m = app.mixer_channels.iter().find(|m| m.track_id == tid).unwrap();
        assert_eq!(m.eq_low, -12.0);
    }

    #[test]
    fn set_channel_eq_mid() {
        let mut app = fresh();
        app.apply(Action::AddTrack(TrackKind::Audio));
        let tid = app.tracks.last().unwrap().id;
        app.apply(Action::SetChannelEqMid { track_id: tid, gain_db: -3.0 });
        let m = app.mixer_channels.iter().find(|m| m.track_id == tid).unwrap();
        assert_eq!(m.eq_mid, -3.0);
    }

    #[test]
    fn set_channel_eq_high() {
        let mut app = fresh();
        app.apply(Action::AddTrack(TrackKind::Audio));
        let tid = app.tracks.last().unwrap().id;
        app.apply(Action::SetChannelEqHigh { track_id: tid, gain_db: 4.0 });
        let m = app.mixer_channels.iter().find(|m| m.track_id == tid).unwrap();
        assert_eq!(m.eq_high, 4.0);
    }

    #[test]
    fn set_channel_comp_threshold() {
        let mut app = fresh();
        app.apply(Action::AddTrack(TrackKind::Audio));
        let tid = app.tracks.last().unwrap().id;
        app.apply(Action::SetChannelCompThreshold { track_id: tid, threshold_db: -24.0 });
        let m = app.mixer_channels.iter().find(|m| m.track_id == tid).unwrap();
        assert_eq!(m.comp_threshold, -24.0);
    }

    #[test]
    fn set_channel_comp_threshold_clamp() {
        let mut app = fresh();
        app.apply(Action::AddTrack(TrackKind::Audio));
        let tid = app.tracks.last().unwrap().id;
        app.apply(Action::SetChannelCompThreshold { track_id: tid, threshold_db: 10.0 });
        let m = app.mixer_channels.iter().find(|m| m.track_id == tid).unwrap();
        assert_eq!(m.comp_threshold, 0.0);
    }

    #[test]
    fn set_channel_comp_ratio() {
        let mut app = fresh();
        app.apply(Action::AddTrack(TrackKind::Audio));
        let tid = app.tracks.last().unwrap().id;
        app.apply(Action::SetChannelCompRatio { track_id: tid, ratio: 8.0 });
        let m = app.mixer_channels.iter().find(|m| m.track_id == tid).unwrap();
        assert_eq!(m.comp_ratio, 8.0);
    }

    #[test]
    fn set_channel_comp_ratio_clamp() {
        let mut app = fresh();
        app.apply(Action::AddTrack(TrackKind::Audio));
        let tid = app.tracks.last().unwrap().id;
        app.apply(Action::SetChannelCompRatio { track_id: tid, ratio: 50.0 });
        let m = app.mixer_channels.iter().find(|m| m.track_id == tid).unwrap();
        assert_eq!(m.comp_ratio, 20.0);
    }

    #[test]
    fn toggle_channel_comp() {
        let mut app = fresh();
        app.apply(Action::AddTrack(TrackKind::Audio));
        let tid = app.tracks.last().unwrap().id;
        assert!(!app.mixer_channels.iter().find(|m| m.track_id == tid).unwrap().comp_enabled);
        app.apply(Action::ToggleChannelComp(tid));
        assert!(app.mixer_channels.iter().find(|m| m.track_id == tid).unwrap().comp_enabled);
        app.apply(Action::ToggleChannelComp(tid));
        assert!(!app.mixer_channels.iter().find(|m| m.track_id == tid).unwrap().comp_enabled);
    }

    #[test]
    fn add_remove_channel_effect() {
        let mut app = fresh();
        app.apply(Action::AddTrack(TrackKind::Audio));
        let tid = app.tracks.last().unwrap().id;
        app.apply(Action::AddChannelEffect { track_id: tid, effect: "Reverb".to_string() });
        app.apply(Action::AddChannelEffect { track_id: tid, effect: "Delay".to_string() });
        let m = app.mixer_channels.iter().find(|m| m.track_id == tid).unwrap();
        assert_eq!(m.effects.len(), 2);
        app.apply(Action::RemoveChannelEffect { track_id: tid, index: 0 });
        let m = app.mixer_channels.iter().find(|m| m.track_id == tid).unwrap();
        assert_eq!(m.effects.len(), 1);
        assert_eq!(m.effects[0], "Delay");
    }

    #[test]
    fn set_master_volume() {
        let mut app = fresh();
        app.apply(Action::SetMasterVolume(0.8));
        assert_eq!(app.master_volume, 0.8);
    }

    #[test]
    fn set_master_volume_clamp() {
        let mut app = fresh();
        app.apply(Action::SetMasterVolume(3.0));
        assert_eq!(app.master_volume, 2.0);
    }

    #[test]
    fn toggle_master_limiter() {
        let mut app = fresh();
        assert!(!app.master_limiter);
        app.apply(Action::ToggleMasterLimiter);
        assert!(app.master_limiter);
        app.apply(Action::ToggleMasterLimiter);
        assert!(!app.master_limiter);
    }

    // ── Transport ────────────────────────────────────────────────────────────

    #[test]
    fn play_sets_playing() {
        let mut app = fresh();
        app.apply(Action::Play);
        assert!(app.playing);
        assert!(!app.recording);
    }

    #[test]
    fn stop_resets_state() {
        let mut app = fresh();
        app.apply(Action::Play);
        app.apply(Action::SetPlayheadBeat(10.0));
        app.apply(Action::Stop);
        assert!(!app.playing);
        assert!(!app.recording);
        assert_eq!(app.playhead_beat, 0.0);
    }

    #[test]
    fn stop_with_loop_returns_to_loop_start() {
        let mut app = fresh();
        app.apply(Action::SetLoopRange { start: 4.0, end: 8.0 });
        app.apply(Action::ToggleLoop);
        app.apply(Action::Play);
        app.apply(Action::Stop);
        assert_eq!(app.playhead_beat, 4.0);
    }

    #[test]
    fn pause_stops_without_resetting() {
        let mut app = fresh();
        app.apply(Action::SetPlayheadBeat(5.0));
        app.apply(Action::Play);
        app.apply(Action::Pause);
        assert!(!app.playing);
        assert_eq!(app.playhead_beat, 5.0);
    }

    #[test]
    fn record_sets_playing_and_recording() {
        let mut app = fresh();
        app.apply(Action::Record);
        assert!(app.playing);
        assert!(app.recording);
    }

    #[test]
    fn set_playhead_beat() {
        let mut app = fresh();
        app.apply(Action::SetPlayheadBeat(16.0));
        assert_eq!(app.playhead_beat, 16.0);
    }

    #[test]
    fn set_playhead_beat_clamp_negative() {
        let mut app = fresh();
        app.apply(Action::SetPlayheadBeat(-5.0));
        assert_eq!(app.playhead_beat, 0.0);
    }

    #[test]
    fn toggle_loop() {
        let mut app = fresh();
        assert!(!app.loop_enabled);
        app.apply(Action::ToggleLoop);
        assert!(app.loop_enabled);
    }

    #[test]
    fn set_loop_range() {
        let mut app = fresh();
        app.apply(Action::SetLoopRange { start: 4.0, end: 12.0 });
        assert_eq!(app.loop_start, 4.0);
        assert_eq!(app.loop_end, 12.0);
    }

    #[test]
    fn set_loop_range_invalid_ignored() {
        let mut app = fresh();
        let prev_start = app.loop_start;
        app.apply(Action::SetLoopRange { start: 10.0, end: 5.0 });
        assert_eq!(app.loop_start, prev_start);
    }

    #[test]
    fn toggle_metronome() {
        let mut app = fresh();
        assert!(!app.metronome_enabled);
        app.apply(Action::ToggleMetronome);
        assert!(app.metronome_enabled);
    }

    #[test]
    fn rewind() {
        let mut app = fresh();
        app.apply(Action::SetPlayheadBeat(32.0));
        app.apply(Action::Play);
        app.apply(Action::Rewind);
        assert_eq!(app.playhead_beat, 0.0);
        assert!(!app.playing);
    }

    #[test]
    fn fast_forward() {
        let mut app = fresh();
        app.apply(Action::SetPlayheadBeat(8.0));
        app.apply(Action::FastForward);
        assert_eq!(app.playhead_beat, 12.0);
    }

    // ── AI ───────────────────────────────────────────────────────────────────

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
        // Kick x2 + snare x2 + 8 hi-hats = 12 notes
        assert!(app.midi_notes.len() >= 8);
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
        // Harmony notes are 4 semitones up
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

    // ── Bounce ───────────────────────────────────────────────────────────────

    #[test]
    fn bounce_config_defaults() {
        let cfg = BounceConfig::new();
        assert_eq!(cfg.format, BounceFormat::Wav);
        assert_eq!(cfg.sample_rate, 44100);
        assert_eq!(cfg.bit_depth, 24);
        assert!(cfg.normalize);
    }

    #[test]
    fn set_bounce_format() {
        let mut app = fresh();
        app.apply(Action::SetBounceFormat(BounceFormat::Flac));
        assert_eq!(app.bounce_config.format, BounceFormat::Flac);
    }

    #[test]
    fn set_bounce_normalize() {
        let mut app = fresh();
        app.apply(Action::SetBounceNormalize(false));
        assert!(!app.bounce_config.normalize);
    }

    #[test]
    fn set_bounce_dither() {
        let mut app = fresh();
        app.apply(Action::SetBounceDither(true));
        assert!(app.bounce_config.dither);
    }

    #[test]
    fn set_bounce_export_path() {
        let mut app = fresh();
        app.apply(Action::SetBounceExportPath("/tmp/out.wav".to_string()));
        assert_eq!(app.bounce_config.export_path, "/tmp/out.wav");
    }

    #[test]
    fn set_bounce_stems_per_track() {
        let mut app = fresh();
        app.apply(Action::SetBounceStemsPerTrack(true));
        assert!(app.bounce_config.stems_per_track);
    }

    #[test]
    fn start_bounce() {
        let mut app = fresh();
        assert!(!app.bounce_in_progress);
        app.apply(Action::StartBounce);
        assert!(app.bounce_in_progress);
    }

    #[test]
    fn cancel_bounce() {
        let mut app = fresh();
        app.apply(Action::StartBounce);
        app.apply(Action::CancelBounce);
        assert!(!app.bounce_in_progress);
    }

    // ── Compound / integration tests ─────────────────────────────────────────

    #[test]
    fn full_session_workflow() {
        let mut app = fresh();
        // Set up project
        app.apply(Action::SetProjectName("Summer Vibes".to_string()));
        app.apply(Action::SetBpm(105.0));
        app.apply(Action::SetProjectKey("A".to_string()));
        app.apply(Action::SetProjectScale("Minor".to_string()));

        // Add tracks
        app.apply(Action::AddTrack(TrackKind::Instrument));
        let piano_id = app.tracks.last().unwrap().id;
        app.apply(Action::SetTrackInstrument { id: piano_id, instrument: "Electric Piano".to_string() });

        app.apply(Action::AddTrack(TrackKind::Audio));
        let audio_id = app.tracks.last().unwrap().id;
        app.apply(Action::SetTrackVolume { id: audio_id, volume: 0.85 });

        // Add clips
        app.apply(Action::AddClip { track_id: piano_id, name: "Verse Piano".into(), kind: ClipKind::Midi, start_beat: 0.0, duration_beats: 16.0 });
        let cid = app.clips[0].id;

        // Add notes
        for (pitch, beat) in [(60u8, 0.0f32), (62, 1.0), (64, 2.0), (65, 3.0)] {
            app.apply(Action::AddMidiNote { clip_id: cid, pitch, velocity: 90, start_beat: beat, duration_beats: 0.75 });
        }

        // Quantize
        app.apply(Action::QuantizeMidiNotes { clip_id: cid, grid: 0.25 });

        // Play
        app.apply(Action::Play);
        assert!(app.playing);

        // Stop
        app.apply(Action::Stop);
        assert!(!app.playing);

        // Bounce
        app.apply(Action::SetBounceFormat(BounceFormat::Wav));
        app.apply(Action::SetBounceExportPath("/tmp/summer_vibes.wav".to_string()));
        app.apply(Action::StartBounce);
        assert!(app.bounce_in_progress);

        assert_eq!(app.project.bpm, 105.0);
        assert_eq!(app.midi_notes.len(), 4);
    }

    #[test]
    fn ai_workflow_end_to_end() {
        let mut app = fresh();
        app.apply(Action::AddTrack(TrackKind::Instrument));
        let tid = app.tracks.last().unwrap().id;

        // Set AI parameters
        app.apply(Action::SetAiPrompt("dark ambient drone".to_string()));
        app.apply(Action::SetAiStyle("Ambient".to_string()));
        app.apply(Action::SetAiDurationBars(16));

        // Generate
        app.apply(Action::GenerateTrack { track_id: tid });
        assert_eq!(app.ai_jobs[0].status, AiGenerationStatus::Generating);
        assert_eq!(app.ai_jobs[0].duration_bars, 16);

        // Complete
        let job_id = app.ai_jobs[0].id;
        app.apply(Action::CompleteAiGeneration { job_id });
        assert_eq!(app.ai_jobs[0].status, AiGenerationStatus::Done);

        // AI master
        app.apply(Action::AiMasterTrack);
        assert!(app.master_limiter);
        assert_eq!(app.master_volume, 0.9);
    }

    #[test]
    fn mixer_channel_effect_chain() {
        let mut app = fresh();
        app.apply(Action::AddTrack(TrackKind::Audio));
        let tid = app.tracks.last().unwrap().id;

        app.apply(Action::AddChannelEffect { track_id: tid, effect: "3-Band EQ".to_string() });
        app.apply(Action::AddChannelEffect { track_id: tid, effect: "Compressor".to_string() });
        app.apply(Action::AddChannelEffect { track_id: tid, effect: "Reverb".to_string() });

        let m = app.mixer_channels.iter().find(|m| m.track_id == tid).unwrap();
        assert_eq!(m.effects.len(), 3);

        // Remove middle effect
        app.apply(Action::RemoveChannelEffect { track_id: tid, index: 1 });
        let m = app.mixer_channels.iter().find(|m| m.track_id == tid).unwrap();
        assert_eq!(m.effects[0], "3-Band EQ");
        assert_eq!(m.effects[1], "Reverb");
    }

    #[test]
    fn split_then_merge_round_trips() {
        let mut app = fresh();
        app.apply(Action::AddTrack(TrackKind::Audio));
        let tid = app.tracks.last().unwrap().id;
        app.apply(Action::AddClip { track_id: tid, name: "loop".into(), kind: ClipKind::Audio, start_beat: 0.0, duration_beats: 16.0 });
        let cid = app.clips[0].id;

        // Split in half
        app.apply(Action::SplitClip { id: cid, at_beat: 8.0 });
        assert_eq!(app.clips.len(), 2);

        // Merge back
        let ids: Vec<usize> = app.clips.iter().map(|c| c.id).collect();
        app.apply(Action::MergeClips { ids });
        assert_eq!(app.clips.len(), 1);
        assert_eq!(app.clips[0].duration_beats, 16.0);
    }

    #[test]
    fn bounce_format_label() {
        assert_eq!(BounceFormat::Wav.label(), "WAV");
        assert_eq!(BounceFormat::Mp3.label(), "MP3");
        assert_eq!(BounceFormat::Flac.label(), "FLAC");
        assert_eq!(BounceFormat::Stems.label(), "Stems");
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
    fn transpose_notes_clamped_at_127() {
        let mut app = fresh();
        app.apply(Action::AddTrack(TrackKind::Midi));
        let tid = app.tracks.last().unwrap().id;
        app.apply(Action::AddClip { track_id: tid, name: "m".into(), kind: ClipKind::Midi, start_beat: 0.0, duration_beats: 4.0 });
        let cid = app.clips[0].id;
        app.apply(Action::AddMidiNote { clip_id: cid, pitch: 125, velocity: 100, start_beat: 0.0, duration_beats: 1.0 });
        app.apply(Action::TransposeMidiNotes { clip_ids: vec![cid], semitones: 10 });
        assert_eq!(app.midi_notes[0].pitch, 127);
    }

    #[test]
    fn transpose_notes_clamped_at_0() {
        let mut app = fresh();
        app.apply(Action::AddTrack(TrackKind::Midi));
        let tid = app.tracks.last().unwrap().id;
        app.apply(Action::AddClip { track_id: tid, name: "m".into(), kind: ClipKind::Midi, start_beat: 0.0, duration_beats: 4.0 });
        let cid = app.clips[0].id;
        app.apply(Action::AddMidiNote { clip_id: cid, pitch: 2, velocity: 100, start_beat: 0.0, duration_beats: 1.0 });
        app.apply(Action::TransposeMidiNotes { clip_ids: vec![cid], semitones: -10 });
        assert_eq!(app.midi_notes[0].pitch, 0);
    }

    #[test]
    fn app_default_has_master_track() {
        let app = fresh();
        assert!(app.tracks.iter().any(|t| t.kind == TrackKind::Master));
    }

    #[test]
    fn app_default_has_master_mixer_channel() {
        let app = fresh();
        assert!(app.mixer_channels.iter().any(|m| m.track_id == 0));
    }

    #[test]
    fn multi_track_solo_isolation() {
        let mut app = fresh();
        app.apply(Action::AddTrack(TrackKind::Audio));
        let t1 = app.tracks.last().unwrap().id;
        app.apply(Action::AddTrack(TrackKind::Audio));
        let t2 = app.tracks.last().unwrap().id;
        app.apply(Action::SetTrackSolo { id: t1, solo: true });
        // t2 should still be independently set/unset
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
    fn bounce_stems_per_track_with_format() {
        let mut app = fresh();
        app.apply(Action::SetBounceFormat(BounceFormat::Stems));
        app.apply(Action::SetBounceStemsPerTrack(true));
        assert_eq!(app.bounce_config.format, BounceFormat::Stems);
        assert!(app.bounce_config.stems_per_track);
    }

    #[test]
    fn generate_drum_pattern_has_kick_snare_hihat() {
        let mut app = fresh();
        app.apply(Action::AddTrack(TrackKind::Midi));
        let tid = app.tracks.last().unwrap().id;
        app.apply(Action::GenerateDrumPattern { track_id: tid });
        // kick = pitch 36, snare = 38, hihat = 42
        let pitches: Vec<u8> = app.midi_notes.iter().map(|n| n.pitch).collect();
        assert!(pitches.contains(&36), "no kick");
        assert!(pitches.contains(&38), "no snare");
        assert!(pitches.contains(&42), "no hihat");
    }
}
