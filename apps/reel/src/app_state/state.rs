//! The [`App`] struct definition, its `new()` constructor, and the `Default`
//! impl — split out of `mod.rs` to satisfy the file-size rule.
//!
//! `mod.rs` retains the [`App::apply`] dispatcher (`impl App` blocks are
//! additive across the crate). All field types are re-exported from the parent
//! `app_state` module, so `use super::*` brings them into scope unchanged.

use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Instant;

use rodio::{MixerDeviceSink, Player as RodioPlayer};

use crate::canvas_host::CanvasHost;
use crate::waveform::WaveformCache;

use super::*;

// --- App struct --------------------------------------------------------------

/// The single shared application state.
pub struct App {
    /// CPU playhead-frame sampler bridged into GPUI (the preview source).
    pub host: CanvasHost,
    /// The project the host samples; panels read it, `apply` mutates it.
    pub project: Project,

    /// The current playhead position, in seconds.
    pub time: f32,
    /// The active editing tool.
    pub active: Tool,
    /// The index of the selected clip in `project.clips`, if any (UI state).
    pub selected: Option<usize>,
    /// The marquee (rubber-band) multi-selection: indices of clips selected by a
    /// drag-rectangle. The primary `selected` follows the first of these.
    pub marquee_selection: Vec<usize>,
    /// The preview zoom factor (1.0 = fit). UI state, applied at draw time.
    pub zoom: f32,
    /// Last laid-out bounds of the timeline scrub region.
    pub timeline_bounds: TimelineBounds,
    /// The in-flight clip drag, or `None` when nothing is being dragged.
    pub clip_drag: ClipDragCell,

    /// Whether the transport is playing.
    pub playing: bool,
    /// Wall-clock anchor for the play loop.
    pub(crate) last_tick: Option<Instant>,

    /// Decoded audio peak envelopes for the timeline waveform.
    pub waveforms: RefCell<WaveformCache>,

    // --- Wave 8: mixer -------------------------------------------------------
    pub track_volumes: Vec<f32>,
    pub track_muted: Vec<bool>,
    pub track_soloed: Vec<bool>,
    pub master_volume: f32,
    pub show_mixer: bool,

    // --- Wave 14: audio effects + track types --------------------------------
    pub track_eq: Vec<TrackEq>,
    pub track_comp: Vec<TrackCompressor>,
    pub track_types: Vec<AudioTrackType>,

    // --- Wave 15: log-to-Rec709 ----------------------------------------------
    pub track_log_transform: Vec<bool>,

    // --- Wave 9: multi-camera ------------------------------------------------
    pub multicam_mode: bool,
    pub multicam_tracks: Vec<usize>,

    // --- Wave 8: LUT + white balance -----------------------------------------
    pub lut_path: Option<PathBuf>,
    pub lut_table: Option<(u32, Vec<[f32; 3]>)>,
    pub white_balance_temp: f32,
    pub white_balance_tint: f32,

    // --- Wave 11: audio playback (rodio) -------------------------------------
    pub audio_playing: bool,
    pub(crate) audio_device_sink: Option<MixerDeviceSink>,
    pub(crate) audio_player: Option<RodioPlayer>,

    // --- Wave 11: snap -------------------------------------------------------
    pub snap_enabled: bool,
    pub snap_point: Option<f32>,
    pub work_area_in: f32,
    pub work_area_out: f32,

    // --- Wave 11: scopes -----------------------------------------------------
    pub scopes_open: bool,
    pub scope_data: Option<ScopeData>,
    pub scope_tab: u8,

    // --- Wave 11: dual viewer ------------------------------------------------
    pub dual_viewer: bool,
    pub source_playhead: f32,
    pub source_playing: bool,
    pub source_in: f32,
    pub source_out: f32,
    pub source_clip: Option<usize>,

    // --- Wave 11: bins -------------------------------------------------------
    pub bins: Vec<Bin>,
    pub bins_open: bool,
    pub selected_bin: usize,
    pub selected_bin_clip: Option<usize>,
    /// Case-insensitive name filter for the active bin's clip list (empty = all).
    /// Driven by the bins-panel search box; see `panels::bins`.
    pub bin_query: String,

    // --- Wave 13: chapter markers / links / export presets -------------------
    pub chapter_markers: Vec<(f32, String)>,
    pub linked_clips: std::collections::HashSet<usize>,
    pub export_presets: Vec<(String, String, u32, u32, f32)>,
    pub active_preset: Option<usize>,

    // --- Batch 2: export formats / bezier ------------------------------------
    pub export_format: ExportFormat,
    pub bezier_drag_active: bool,
    pub bezier_drag_point: usize,

    // --- Batch 2: 3-band EQ per track ----------------------------------------
    pub track_eq3: Vec<TrackEq3>,

    pub multicam_groups: Vec<MulticamGroup>,

    // --- Batch 3: 3-way color wheels -----------------------------------------
    pub color_wheels: ColorWheels,

    // --- Batch 3: sequence settings ------------------------------------------
    pub show_sequence_settings: bool,
    pub sequence_color_space: ColorSpace,
    pub sequence_sample_rate: u32,

    // --- Batch 4: RGB curves -------------------------------------------------
    pub rgb_curves: RgbCurves,
    pub curves_channel: u8,

    // --- HSL secondary curves + 3DL/.cube LUT --------------------------------
    pub hsl_curves: HslCurves,
    pub last_cube_export_path: Option<std::path::PathBuf>,

    // --- Batch 4: audio effect chains per track ------------------------------
    pub audio_effects: Vec<Vec<AudioEffect>>,
    pub track_fx_open: Vec<bool>,
    pub track_fx_expanded: Vec<Option<usize>>,

    // --- Batch 4: closed captions --------------------------------------------
    pub captions: Vec<Caption>,
    pub selected_caption: Option<usize>,
    pub show_captions_panel: bool,

    // --- Batch 4: export preset list -----------------------------------------
    pub export_preset_list: Vec<ExportPreset>,
    pub show_export_presets: bool,

    // --- Batch 4: markers panel ----------------------------------------------
    pub show_markers_panel: bool,
    pub gpui_markers: Vec<GpuiMarker>,

    // --- Batch 5: copy/paste clipboard ---------------------------------------
    pub clipboard_clips: Vec<Clip>,

    // --- Batch 5: LUFS metering ----------------------------------------------
    pub lufs_short_term: f32,
    pub lufs_integrated: f32,
    pub lufs_power_history: Vec<f32>,

    // --- Batch 7: scene edit detection ---------------------------------------
    pub scene_edit_sensitivity: f32,
    pub last_scene_edit_result: Option<SceneEditResult>,

    // --- Batch 7: project management -----------------------------------------
    pub project_name: String,
    pub project_path: Option<std::path::PathBuf>,
    pub recent_project_paths: Vec<std::path::PathBuf>,
    pub project_notes: String,
    pub auto_save_enabled: bool,
    pub auto_save_interval_sec: u32,

    // --- Batch 8: multicam depth ---------------------------------------------
    pub multicam_angles: Vec<MulticamAngle>,
    pub multicam_active_angle: usize,
    pub multicam_sync_mode: MulticamSyncMode,
    pub multicam_display_mode: MulticamDisplayMode,
    /// Live multicam angle-switch model: per-time angle selections sampled when
    /// the timeline is cut/played.
    pub multicam_clip: MulticamClip,

    // --- Batch 8: EDL / XML interchange --------------------------------------
    pub edl_config: EdlConfig,
    pub last_edl_export_path: Option<std::path::PathBuf>,
    pub last_import_clip_count: usize,

    // --- Batch 8: audio suite ------------------------------------------------
    pub audio_suite_config: AudioSuiteConfig,
    pub audio_suite_panel_open: bool,
    pub audio_suite_preview: bool,

    // --- Batch 8: auto reframe -----------------------------------------------
    pub auto_reframe_config: AutoReframeConfig,
    pub auto_reframe_panel_open: bool,
    pub reframe_results: Vec<(usize, Vec<f32>)>,

    // --- Batch 9: Lumetri Color depth ----------------------------------------
    pub lumetri: LumetriColorConfig,
    pub lumetri_panel_open: bool,
    pub lumetri_applied_clip: Option<usize>,

    // --- Batch 9: Captions / Subtitles (extended) ----------------------------
    pub captions_b9: Vec<CaptionB9>,
    pub caption_styles_b9: Vec<CaptionStyleB9>,
    pub caption_track_visible: bool,
    pub last_srt_export_path: Option<std::path::PathBuf>,

    // --- Batch 9: Sequence Settings (extended) --------------------------------
    pub sequence_settings: SequenceSettings,
    pub sequence_settings_open: bool,

    // --- Batch 9: Nest Sequence (extended) -----------------------------------
    pub nested_sequences: Vec<NestedSequence>,
    pub active_nested_seq: Option<usize>,

    // --- Batch 10: Essential Graphics (Motion Graphics Templates) ------------
    pub mogr_templates: Vec<MogrTemplate>,
    pub mogr_library_open: bool,
    pub active_mogr: Option<usize>,
    pub mogr_applied_clips: Vec<(usize, usize)>,

    // --- Batch 10: Color Management ------------------------------------------
    pub color_management: ColorManagementConfig,
    pub color_management_open: bool,

    // --- Batch 10: Export Presets (new model) --------------------------------
    pub export_presets_b10: Vec<ExportPresetB10>,
    pub active_export_preset: usize,
    pub export_panel_open: bool,
    pub last_export_path: Option<std::path::PathBuf>,

    // --- Batch 10: Proxy Workflow --------------------------------------------
    pub proxy_settings: ProxySettings,
    pub proxy_ingest_open: bool,
    pub clip_proxies: Vec<ClipProxy>,
    pub toggle_proxy_enabled: bool,

    // --- Batch 11: AudioTrackMixer ---
    pub audio_mixer: AudioTrackMixer,

    // --- Batch 11: TitlesGraphics ---
    pub title_clips: Vec<TitleClip>,
    pub title_clip_counter: usize,
    pub active_title_clip: Option<usize>,

    // --- Batch 11: ProjectManager ---
    pub project_manager_config: ProjectManagerConfig,
    pub project_manager_result: Option<ProjectManagerResult>,
    pub project_manager_open: bool,

    // --- Batch 11: MediaBrowser ---
    pub media_browser: MediaBrowser,

    // --- Project management: consolidate manifest (relink/offline live on
    //     media_items) ---
    pub last_consolidate_manifest: Option<ConsolidateManifest>,

    // --- Autosave / crash recovery ---
    pub autosave_config: AutosaveConfig,
    pub autosave_ring: AutosaveRing,
    /// Count of user actions applied since launch (drives count-based autosave).
    pub autosave_action_count: u64,

    // --- Batch 5 (new): Lumetri Scopes Config --------------------------------
    pub scopes_config: reel_project::LumetriScopesConfig,

    // --- Batch 5 (new): Project Bins & Media Items ---------------------------
    pub project_bins: Vec<reel_project::ProjectBin>,
    pub media_items: Vec<reel_project::MediaItem>,
    pub next_bin_id: usize,
    pub next_media_item_id: usize,
    pub project_search_query: String,
    pub media_browser_path: String,

    // --- Batch 5 (new): Export Presets B5 ------------------------------------
    pub export_presets_b5: Vec<reel_project::ExportPresetB5>,
    pub active_export_preset_b5: Option<usize>,
    pub next_preset_id: usize,

    // --- Batch 5 (new): Sequences B5 ----------------------------------------
    pub sequences_b5: Vec<reel_project::SequenceSettingsB5>,
    pub active_sequence_id: usize,
    pub next_sequence_id: usize,

    // --- Essential Graphics / Motion Graphics Templates ----------------------
    pub graphics_templates: Vec<GraphicsTemplate>,
    pub graphics_instances: Vec<GraphicsInstance>,

    // --- Proxy workflow (job model + proxy/full toggle) ----------------------
    pub proxy_workflow: ProxyWorkflow,

    // --- Background render cache ----------------------------------------------
    pub render_cache: RenderCache,

    // --- Preferences + remappable keybindings ---------------------------------
    pub preferences: Preferences,
    pub keymap: Keymap,
    /// Commands that conflicted with the most recent keybinding remap.
    pub last_keybind_conflicts: Vec<EditorCommand>,

    // --- Workspaces (named panel layouts) -------------------------------------
    pub workspaces: WorkspaceManager,

    // --- Phase 3: clip transitions (see app_state/transition_fx.rs) ----------
    pub transition_fx: Vec<TransitionFx>,
}

impl App {
    /// Build the shared state with sensible defaults.
    pub fn new() -> Self {
        let project = Project::new();
        let host = CanvasHost::new();
        let n_tracks = project.tracks.len();
        Self {
            host,
            project,
            time: 4.0,
            active: Tool::Select,
            selected: Some(0),
            marquee_selection: vec![0],
            zoom: 1.0,
            timeline_bounds: Rc::new(Cell::new(None)),
            clip_drag: Rc::new(Cell::new(None)),
            playing: false,
            last_tick: None,
            waveforms: RefCell::new(WaveformCache::new()),
            track_volumes: vec![1.0; n_tracks],
            track_muted: vec![false; n_tracks],
            track_soloed: vec![false; n_tracks],
            master_volume: 1.0,
            show_mixer: false,
            track_eq: (0..n_tracks).map(|_| TrackEq::default()).collect(),
            track_comp: (0..n_tracks).map(|_| TrackCompressor::default()).collect(),
            track_types: vec![AudioTrackType::Stereo; n_tracks],
            track_log_transform: vec![false; n_tracks],
            multicam_mode: false,
            multicam_tracks: (0..n_tracks).collect(),
            lut_path: None,
            lut_table: None,
            white_balance_temp: 6500.0,
            white_balance_tint: 0.0,
            audio_playing: false,
            audio_device_sink: None,
            audio_player: None,
            snap_enabled: true,
            snap_point: None,
            work_area_in: 0.0,
            work_area_out: 30.0,
            scopes_open: false,
            scope_data: None,
            scope_tab: 0,
            dual_viewer: false,
            source_playhead: 0.0,
            source_playing: false,
            source_in: 0.0,
            source_out: 0.0,
            source_clip: None,
            bins: vec![Bin { name: "Project".into(), clips: Vec::new() }],
            bins_open: false,
            selected_bin: 0,
            selected_bin_clip: None,
            bin_query: String::new(),
            chapter_markers: Vec::new(),
            linked_clips: std::collections::HashSet::new(),
            export_presets: Vec::new(),
            active_preset: None,
            export_format: ExportFormat::default(),
            bezier_drag_active: false,
            bezier_drag_point: 0,
            track_eq3: (0..n_tracks).map(|_| TrackEq3::default()).collect(),
            multicam_groups: Vec::new(),
            color_wheels: ColorWheels::default(),
            show_sequence_settings: false,
            sequence_color_space: ColorSpace::default(),
            sequence_sample_rate: 48000,
            rgb_curves: RgbCurves::default(),
            curves_channel: 0,
            hsl_curves: HslCurves::default(),
            last_cube_export_path: None,
            audio_effects: vec![Vec::new(); n_tracks],
            track_fx_open: vec![false; n_tracks],
            track_fx_expanded: vec![None; n_tracks],
            captions: Vec::new(),
            selected_caption: None,
            show_captions_panel: false,
            export_preset_list: vec![
                ExportPreset { name: "1080p H.264".into(), format: ExportFormat::H264Mp4, width: 1920, height: 1080, fps: 30.0, bitrate_kbps: 8000 },
                ExportPreset { name: "4K H.264".into(), format: ExportFormat::H264Mp4, width: 3840, height: 2160, fps: 30.0, bitrate_kbps: 35000 },
                ExportPreset { name: "720p GIF".into(), format: ExportFormat::Gif, width: 1280, height: 720, fps: 15.0, bitrate_kbps: 0 },
                ExportPreset { name: "ProRes Proxy".into(), format: ExportFormat::ProResProxy, width: 1920, height: 1080, fps: 30.0, bitrate_kbps: 45000 },
            ],
            show_export_presets: false,
            show_markers_panel: false,
            gpui_markers: Vec::new(),
            clipboard_clips: Vec::new(),
            lufs_short_term: -f32::INFINITY,
            lufs_integrated: -f32::INFINITY,
            lufs_power_history: Vec::new(),
            scene_edit_sensitivity: 0.5,
            last_scene_edit_result: None,
            project_name: "Untitled Project".to_string(),
            project_path: None,
            recent_project_paths: Vec::new(),
            project_notes: String::new(),
            auto_save_enabled: true,
            auto_save_interval_sec: 300,
            multicam_angles: Vec::new(),
            multicam_active_angle: 0,
            multicam_sync_mode: MulticamSyncMode::Timecode,
            multicam_display_mode: MulticamDisplayMode::Grid,
            multicam_clip: MulticamClip::default(),
            edl_config: EdlConfig::default(),
            last_edl_export_path: None,
            last_import_clip_count: 0,
            audio_suite_config: AudioSuiteConfig::default(),
            audio_suite_panel_open: false,
            audio_suite_preview: false,
            auto_reframe_config: AutoReframeConfig::default(),
            auto_reframe_panel_open: false,
            reframe_results: Vec::new(),
            lumetri: LumetriColorConfig::new(),
            lumetri_panel_open: false,
            lumetri_applied_clip: None,
            captions_b9: Vec::new(),
            caption_styles_b9: vec![CaptionStyleB9::default_style()],
            caption_track_visible: true,
            last_srt_export_path: None,
            sequence_settings: SequenceSettings::hd_1080p(),
            sequence_settings_open: false,
            nested_sequences: Vec::new(),
            active_nested_seq: None,
            mogr_templates: Vec::new(),
            mogr_library_open: false,
            active_mogr: None,
            mogr_applied_clips: Vec::new(),
            color_management: ColorManagementConfig::new(),
            color_management_open: false,
            export_presets_b10: vec![ExportPresetB10::h264_1080p()],
            active_export_preset: 0,
            export_panel_open: false,
            last_export_path: None,
            proxy_settings: ProxySettings::default(),
            proxy_ingest_open: false,
            clip_proxies: Vec::new(),
            toggle_proxy_enabled: false,
            audio_mixer: AudioTrackMixer::new(),
            title_clips: Vec::new(),
            title_clip_counter: 0,
            active_title_clip: None,
            project_manager_config: ProjectManagerConfig::new(),
            project_manager_result: None,
            project_manager_open: false,
            media_browser: MediaBrowser::new(),
            last_consolidate_manifest: None,
            autosave_config: AutosaveConfig::default(),
            autosave_ring: AutosaveRing::new(10),
            autosave_action_count: 0,
            // --- Batch 5 (new) -----------------------------------------------
            scopes_config: reel_project::LumetriScopesConfig::default(),
            project_bins: vec![reel_project::ProjectBin {
                id: 0, name: "Project".into(), parent_id: None,
                color_label: reel_project::BinColor::None,
                item_ids: Vec::new(), expanded: true,
            }],
            media_items: Vec::new(),
            next_bin_id: 1,
            next_media_item_id: 0,
            project_search_query: String::new(),
            media_browser_path: String::new(),
            export_presets_b5: vec![
                reel_project::ExportPresetB5::youtube_1080p(),
                reel_project::ExportPresetB5::youtube_4k(),
                reel_project::ExportPresetB5::twitter(),
                reel_project::ExportPresetB5::vimeo_1080p(),
                reel_project::ExportPresetB5::prores_422(),
                reel_project::ExportPresetB5::gif(),
                reel_project::ExportPresetB5::mp3_audio(),
            ],
            active_export_preset_b5: Some(1),
            next_preset_id: 8,
            sequences_b5: vec![reel_project::SequenceSettingsB5::default_1080p(0, "Sequence 01")],
            active_sequence_id: 0,
            next_sequence_id: 1,
            // --- New feature modules ------------------------------------------
            graphics_templates: Vec::new(),
            graphics_instances: Vec::new(),
            proxy_workflow: ProxyWorkflow::default(),
            render_cache: RenderCache::default(),
            preferences: Preferences::default(),
            keymap: Keymap::premiere_defaults(),
            last_keybind_conflicts: Vec::new(),
            workspaces: WorkspaceManager::default(),
            transition_fx: Vec::new(),
        }
    }
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}
