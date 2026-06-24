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
// ─── Batch 5 domain modules ───────────────────────────────────────────────────
pub mod vst_host;
pub mod surround;
pub mod spectral;
pub mod notation;
pub mod tempo_film;
// ─── Batch 5 domain modules ───────────────────────────────────────────────────
pub mod onnx_runtime;
pub mod waveform_cache;

// ─── Real ONNX inference layer (backend abstraction + stub fallback) ──────────
pub mod inference;

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
pub use project::{parse_bpm, ToneProject};
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
// ─── Batch 5 re-exports ───────────────────────────────────────────────────────
pub use vst_host::{VstFormat, VstHostStatus, VstPluginInfo, LoadedVst};
pub use surround::{SurroundFormat, SurroundPan, SurroundBusConfig, AtmosObject};
pub use spectral::{SpectralTool, SpectralBrushStroke, SpectralStretchJob, SpectralViewConfig};
pub use notation::{NotationClef, StemDir, NoteHead, NotationStaff, PdfExportStatus, NotationExport};
pub use tempo_film::{TempoChange, TimeSigChange2, VideoLockStatus, VideoFilmConfig};
// ─── Batch 5 re-exports ───────────────────────────────────────────────────────
pub use onnx_runtime::{OnnxModelKind, ModelDownloadStatus, OnnxModelEntry, OnnxInferenceJob};
pub use waveform_cache::{WaveformPeak, WaveformChunk};

// ─── Inference layer re-exports ───────────────────────────────────────────────
pub use inference::{
    InferenceBackend, InferenceError, InferenceOutput, InferenceRequest, LoadState, ModelId,
    ModelRegistry, ModelSlot,
};


// ─── Re-export automation types used in Action ────────────────────────────────

// ─── Automation (sibling module, re-exported for use in Action variants) ─────
pub mod automation;
pub use automation::{AutomationLane, AutomationMode, AutomationParameter, AutomationPoint, SendParam};

// ─── Action enum, tool enum, quantize types live in action.rs ────────────────
pub mod action;
pub use action::{ToneTool, QuantizeGrid, QuantizeConfig, Action};

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
    // ── VST Host (Batch 5) ────────────────────────────────────────────────────
    pub vst_scan_path: String,
    pub vst_scan_running: bool,
    pub vst_scan_results: Vec<VstPluginInfo>,
    pub vst_blacklist: Vec<String>,
    pub loaded_vsts: Vec<LoadedVst>,
    pub next_vst_instance_id: usize,

    // ── Surround (Batch 5) ────────────────────────────────────────────────────
    pub surround_bus: SurroundBusConfig,
    pub surround_pans: Vec<(usize, SurroundPan)>,
    pub atmos_objects: Vec<AtmosObject>,
    pub next_atmos_id: usize,

    // ── Spectral (Batch 5) ────────────────────────────────────────────────────
    pub spectral_view: SpectralViewConfig,
    pub spectral_strokes: Vec<SpectralBrushStroke>,
    pub spectral_stretch_jobs: Vec<SpectralStretchJob>,
    pub next_spectral_brush_id: usize,
    pub next_spectral_stretch_id: usize,

    // ── Notation (Batch 5) ────────────────────────────────────────────────────
    pub notation_staves: Vec<NotationStaff>,
    pub notation_exports: Vec<NotationExport>,
    pub next_notation_staff_id: usize,
    pub next_notation_export_id: usize,

    // ── Tempo Film (Batch 5) ──────────────────────────────────────────────────
    pub tempo_changes: Vec<TempoChange>,
    pub tsig_changes: Vec<TimeSigChange2>,
    pub next_tempo_change_id: usize,
    pub next_tsig_change_id: usize,
    pub video_film: VideoFilmConfig,
    // ── ONNX Model Management (Batch 5) ──────────────────────────────────────
    pub onnx_models: Vec<OnnxModelEntry>,
    pub onnx_inference_jobs: Vec<OnnxInferenceJob>,
    pub next_onnx_job_id: usize,

    // ── Real inference layer (backend abstraction + registry) ────────────────
    /// Maps each model id to its on-disk path + load state for real inference.
    pub model_registry: ModelRegistry,
    /// Which backend runs inference. Defaults to the deterministic stub; the
    /// `onnx` feature enables a real `ort` session backend.
    pub inference_backend: InferenceBackend,

    // ── Waveform Peak Cache (Batch 5) ─────────────────────────────────────────
    pub waveform_cache: Vec<WaveformChunk>,
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
            vst_scan_path: String::new(),
            vst_scan_running: false,
            vst_scan_results: Vec::new(),
            vst_blacklist: Vec::new(),
            loaded_vsts: Vec::new(),
            next_vst_instance_id: 1,
            surround_bus: SurroundBusConfig {
                format: SurroundFormat::Stereo,
                enabled: false,
                binaural_monitor: false,
            },
            surround_pans: Vec::new(),
            atmos_objects: Vec::new(),
            next_atmos_id: 1,
            spectral_view: SpectralViewConfig::default(),
            spectral_strokes: Vec::new(),
            spectral_stretch_jobs: Vec::new(),
            next_spectral_brush_id: 1,
            next_spectral_stretch_id: 1,
            notation_staves: Vec::new(),
            notation_exports: Vec::new(),
            next_notation_staff_id: 1,
            next_notation_export_id: 1,
            tempo_changes: Vec::new(),
            tsig_changes: Vec::new(),
            next_tempo_change_id: 1,
            next_tsig_change_id: 1,
            video_film: VideoFilmConfig::default(),
            onnx_models: Vec::new(),
            onnx_inference_jobs: Vec::new(),
            next_onnx_job_id: 1,
            model_registry: ModelRegistry::new(),
            inference_backend: InferenceBackend::default(),
            waveform_cache: Vec::new(),
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

}

// Dispatcher (apply method) and Default impl live in dispatcher.rs.
pub mod dispatcher;

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
