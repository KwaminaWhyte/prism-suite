//! The single shared application state for the GPUI host.
//!
//! `App` owns everything panels read or mutate. Panels NEVER mutate `App`
//! fields directly — they emit an [`Action`], and the root view routes it
//! through [`App::apply`], the single choke point that mutates state.

use std::cell::Cell;
use std::rc::Rc;

use gpui::{Bounds, Pixels};

// --- Domain modules ----------------------------------------------------------

mod action;
mod helpers;
mod state;
mod audio;
mod captions;
mod color;
mod effects_chain;
mod export_presets;
mod graphics;
mod media;
mod multicam;
mod proxy;
pub mod timeline;
pub mod transitions;
pub mod edl;
mod color_curves;
mod timeline_clips;
mod timeline_selection;
mod timeline_transitions;
mod timeline_playback;
mod timeline_misc;

// --- New feature modules -----------------------------------------------------
mod graphics_templates;
mod proxy_workflow;
mod render_cache;
mod prefs_keys;
mod workspaces;

// --- Batch 5 modules ---------------------------------------------------------
pub mod reel_project;
mod apply_batch5;
#[cfg(test)] mod tests_batch5;

// --- New feature modules -----------------------------------------------------
mod project_mgmt;
mod autosave;

// --- Phase 3: built-in per-clip effects --------------------------------------
mod clip_effects;

// --- Phase 3: clip transitions + time-remap / speed --------------------------
mod transition_fx;

// --- Domain trait imports (used in apply dispatcher) -------------------------

use audio::AppAudioExt;
use captions::AppCaptionsExt;
use color::AppColorExt;
use effects_chain::AppEffectsExt;
use export_presets::AppExportPresetsExt;
use graphics::AppGraphicsExt;
use media::AppMediaExt;
use multicam::AppMulticamExt;
use proxy::AppProxyExt;
use timeline::AppTimelineExt;
use transitions::AppTransitionsExt;
use edl::AppEdlExt;
use color_curves::AppColorCurvesExt;
use apply_batch5::AppBatch5Ext;
use clip_effects::AppClipEffectsExt;
use transition_fx::AppTransitionFxExt;

// --- Public re-exports -------------------------------------------------------

// Action enum (split into action.rs)
pub use action::Action;

// App struct + constructor (split into state.rs)
pub use state::App;

// Free helper functions (split into helpers.rs)
pub use helpers::{
    is_audio_path, is_video_path, k_weighted_power, lufs_integrated, lufs_short_term,
    parse_cube_lut, parse_srt, serialize_srt, snap_candidates,
};
// Crate-private helpers cannot be re-exported as `pub` (they are `pub(crate)`).
pub(crate) use helpers::{hsl_to_rgb, rgb_to_hsl};

// Audio domain
pub use audio::{
    AudioEffect, AudioSend, AudioSendDestination, AudioSuiteConfig, AudioSuiteKind,
    AudioTrackKind, AudioTrackMixer, AudioTrackMixerTrack, AudioTrackType, EqBand,
    TrackCompressor, TrackEq, TrackEq3,
};

// Captions domain
pub use captions::{
    parse_inline_tags, Caption, CaptionB9, CaptionPosition, CaptionStyle, CaptionStyleB9, StyledRun,
};

// Color domain
pub use color::{
    ColorManagementConfig, ColorSpace, ColorWheelMode, ColorWheels, DisplayColorSpace,
    GpuiMarker, LumetriColorConfig, LumetriPanel, MarkerKind, RgbCurves, WorkingColorSpace,
};

// Color curves domain (HSL secondary curves)
pub use color_curves::HslCurves;

// Export presets domain
pub use export_presets::{
    ExportFormat, ExportFormatB10, ExportPreset, ExportPresetB10, ExportResolution,
};

// Graphics domain
pub use graphics::{
    MogrParam, MogrParamValue, MogrTemplate, NestedSequence, SeqFrameRate, SeqPixelAspect,
    SequenceSettings, TitleAlign, TitleClip, TitleKind, TitleTextBox,
};

// Multicam domain
pub use multicam::{
    AutoReframeConfig, EdlConfig, EdlFormat, MulticamAngle, MulticamClip, MulticamDisplayMode,
    MulticamSyncMode, ReframeMotion,
};

// Media domain (extended Batch 11 types)
pub use media::{
    MediaBrowser, MediaBrowserEntry, MediaBrowserFilter,
    ProjectCollectMode, ProjectManagerConfig, ProjectManagerResult,
};

// Proxy domain
pub use proxy::{ClipProxy, ProxyFormat, ProxySettings};

// Project-management domain (relink / consolidate / offline-online)
pub use project_mgmt::ConsolidateManifest;

// Autosave / crash-recovery domain
pub use autosave::{AutosaveConfig, AutosaveRing};

// Phase 3: built-in per-clip effects domain
pub use clip_effects::{BuiltinEffect, EffectParams};

// Phase 3: clip transitions + time-remap / speed domain
pub use transition_fx::{
    TimeRemap, TransitionAlign, TransitionFx, TransitionFxKind, TransitionGeometry,
    TransitionState, WipeShape,
};

// Graphics-templates domain (Essential Graphics / Motion Graphics Templates)
pub use graphics_templates::{
    GraphicsInstance, GraphicsTemplate, GtExposedProp, GtLayer, GtPropTarget, GtPropValue,
    GtShapeKind, GtShapeLayer, GtTextLayer,
};

// Proxy-workflow domain (proxy job model + proxy/full toggle)
pub use proxy_workflow::{
    PlaybackSource, ProxyJob, ProxyJobState, ProxyResolution, ProxyWorkflow,
};

// Render-cache domain (background render cache + render bar)
pub use render_cache::{CacheRange, CacheSegment, RenderBarStatus, RenderCache};

// Preferences + keybindings domain
pub use prefs_keys::{
    EditorCommand, KeyChord, Keymap, PlaybackResolution, Preferences, ScratchKind,
};

// Workspaces domain (named panel layouts)
pub use workspaces::{
    Panel, PanelLayout, PanelSlot, Region, Workspace, WorkspaceManager,
};

// Timeline domain — all the big domain types + constants defined there
pub use timeline::{
    AudioSource, Bin, BinClip, BinClipType, Clip, ClipBlendMode, ClipEffect, ClipEffectKind,
    ClipSource, ColorGrade, CubeDirection, DIP_BLACK, DIP_WHITE, FilmPattern, HslSecondaryGrade,
    MulticamGroup, PagePeelDirection, Project, SceneEditResult, ScopeData, SlideDirection,
    SpeedCurve, SpinDirection, SplitDirection, SwapDirection, Tool, Track, Transition,
    TransitionKind, VideoSource, WipeDir,
};

// --- Constants defined here (timeline.rs imports via `super::`) --------------

/// Default frames per second used when no video source is probed.
pub const DEFAULT_VIDEO_FPS: f64 = 30.0;

/// Minimum valid clip duration (shorter clips snap to this).
pub const MIN_DUR: f32 = 0.1;

/// Default duration for an imported video clip when probe returns nothing.
pub const DEFAULT_VIDEO_LEN: f32 = 10.0;

/// Default duration for a still image on the timeline.
pub const DEFAULT_IMAGE_LEN: f32 = 5.0;

/// Default duration for an imported audio clip when probe returns nothing.
pub const DEFAULT_AUDIO_LEN: f32 = 10.0;

/// Video file extensions the importer recognises.
pub const VIDEO_EXTENSIONS: &[&str] = &["mp4", "mov", "mkv", "webm", "avi", "m4v"];

/// Audio file extensions the importer recognises.
pub const AUDIO_EXTENSIONS: &[&str] = &["wav", "mp3", "aac", "flac", "ogg", "aiff", "m4a"];

/// Default clip audio gain (1.0 = unity).
pub const DEFAULT_AUDIO_GAIN: f32 = 1.0;

/// Maximum per-clip audio gain allowed by the gain slider.
pub const MAX_AUDIO_GAIN: f32 = 2.0;

/// Default dissolve transition duration in seconds.
pub const DEFAULT_TRANSITION_DUR: f32 = 1.0;

/// Minimum dissolve transition duration (shorter values are clamped).
pub const MIN_TRANSITION_DUR: f32 = 0.1;

// --- Type aliases defined here (timeline.rs imports via `super::`) -----------

/// Shared cell recording the last laid-out pixel bounds of the timeline lane.
pub type TimelineBounds = Rc<Cell<Option<Bounds<Pixels>>>>;

/// The shared in-flight clip-drag cell (None when nothing is being dragged).
pub type ClipDragCell = Rc<Cell<Option<ClipDrag>>>;

// --- Drag types defined here (timeline.rs uses `super::ClipDrag` etc.) ------

/// What a clip drag is doing.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum ClipDragKind {
    Move,
    TrimIn,
    TrimOut,
    RippleTrimIn,
    RippleTrimOut,
    RollTrim,
    RateStretch,
}

/// An in-flight clip drag on the timeline.
#[derive(Clone, Copy, Debug)]
pub struct ClipDrag {
    pub index: usize,
    pub kind: ClipDragKind,
    pub grab_offset: f32,
}

impl App {
    /// Apply a panel-emitted [`Action`]. This is the ONLY place `App` state is
    /// mutated. Routes each action to the appropriate domain handler.
    pub fn apply(&mut self, action: Action) {
        match &action {
            // --- Audio domain ------------------------------------------------
            Action::SetTrackVolume { .. }
            | Action::ToggleTrackMute { .. }
            | Action::ToggleTrackSolo { .. }
            | Action::SetMasterVolume { .. }
            | Action::ToggleMixer
            | Action::SetAudioVolume(_)
            | Action::SetAudioMute(_)
            | Action::ToggleTrackEq { .. }
            | Action::SetEqBand { .. }
            | Action::ToggleTrackCompressor { .. }
            | Action::SetCompressor { .. }
            | Action::CycleAudioTrackType { .. }
            | Action::SetTrackEq3 { .. }
            | Action::AddAudioEffect { .. }
            | Action::RemoveAudioEffect { .. }
            | Action::SetAudioEffect { .. }
            | Action::ToggleTrackFx { .. }
            | Action::ExpandTrackFx { .. }
            | Action::UpdateLufsMeters { .. }
            | Action::ResetLufsIntegrated
            | Action::ToggleAudioSuitePanel
            | Action::SetAudioSuiteKind(_)
            | Action::SetAudioSuiteGain(_)
            | Action::SetAudioSuitePreserve(_)
            | Action::SetAudioSuiteProcessInPlace(_)
            | Action::SetAudioSuiteTargetLevel(_)
            | Action::SetAudioSuitePitch(_)
            | Action::SetAudioSuiteStretch(_)
            | Action::ToggleAudioSuitePreview
            | Action::ApplyAudioSuite { .. }
            // Batch 11: AudioTrackMixer
            | Action::OpenAudioMixer
            | Action::CloseAudioMixer
            | Action::AddAudioMixerTrack { .. }
            | Action::SetAudioMixerFader { .. }
            | Action::SetAudioMixerPan { .. }
            | Action::SetAudioMixerMute { .. }
            | Action::SetAudioMixerSolo { .. }
            | Action::SetMasterFader(_)
            | Action::AddAudioSend { .. } => {
                self.apply_audio(action);
            }

            // --- Captions domain ---------------------------------------------
            Action::AddCaption(_)
            | Action::RemoveCaption(_)
            | Action::EditCaption { .. }
            | Action::SetCueText { .. }
            | Action::SetCaptionPosition { .. }
            | Action::SetCaptionCueColor { .. }
            | Action::ImportSrt(_)
            | Action::ExportSrt(_)
            | Action::ToggleCaptionsPanel
            | Action::AddCaptionB9(_)
            | Action::RemoveCaptionB9(_)
            | Action::SetCaptionText { .. }
            | Action::SetCaptionTiming { .. }
            | Action::SetCaptionSpeaker { .. }
            | Action::SetCaptionStyle { .. }
            | Action::AddCaptionStyleB9(_)
            | Action::ToggleCaptionTrack
            | Action::ExportSrtB9(_)
            | Action::ImportSrtB9(_)
            | Action::AutoTranscribe => {
                self.apply_captions(action);
            }

            // --- Color domain ------------------------------------------------
            Action::SetColorWheels(_)
            | Action::SetRgbCurves(_)
            | Action::SetCurvesChannel(_)
            | Action::AddCurvePoint { .. }
            | Action::MoveCurvePoint { .. }
            | Action::LoadLut { .. }
            | Action::SetWhiteBalance { .. }
            | Action::ToggleSequenceSettings
            | Action::SetColorSpace(_)
            | Action::ToggleMarkersPanel
            | Action::AddMarkerAt { .. }
            | Action::RemoveMarker(_)
            | Action::RenameMarker { .. }
            | Action::SetInPoint(_)
            | Action::SetOutPoint(_)
            | Action::SetLumetriPanel(_)
            | Action::SetLumetriExposure(_)
            | Action::SetLumetriContrast(_)
            | Action::SetLumetriHighlights(_)
            | Action::SetLumetriShadows(_)
            | Action::SetLumetriWhites(_)
            | Action::SetLumetriBlacks(_)
            | Action::SetLumetriTemp(_)
            | Action::SetLumetriTint(_)
            | Action::SetLumetriSaturation(_)
            | Action::SetLumetriFadedFilm(_)
            | Action::SetLumetriSharpen(_)
            | Action::SetLumetriVibrance(_)
            | Action::SetLumetriHslHueRange(_)
            | Action::SetLumetriHslSatRange(_)
            | Action::SetLumetriHslShifts { .. }
            | Action::SetLumetriVignette { .. }
            | Action::ResetLumetriPanel
            | Action::ApplyLumetriToClip { .. }
            | Action::ToggleLumetriPanel
            | Action::ToggleColorManagementPanel
            | Action::SetColorManagementEnabled(_)
            | Action::SetDisplayColorSpace(_)
            | Action::SetWorkingColorSpace(_)
            | Action::SetOutputLutPath(_)
            | Action::SetHdrOutput(_)
            | Action::SetMaxLuminance(_)
            | Action::SetApplyLutOnExport(_)
            | Action::ResetColorManagement => {
                self.apply_color(action);
            }

            // --- Effects chain domain ----------------------------------------
            Action::AddClipEffect { .. }
            | Action::RemoveClipEffect { .. }
            | Action::SetClipEffect { .. }
            | Action::ToggleClipEffect { .. }
            | Action::ReorderClipEffects { .. }
            | Action::ClearClipEffects { .. }
            | Action::SetClipMotion { .. }
            | Action::SetClipMotionScale { .. }
            | Action::SetClipMotionRotation { .. }
            | Action::ResetClipMotion { .. }
            | Action::SetSceneEditSensitivity(_)
            | Action::DetectSceneEdits { .. }
            | Action::ApplySceneEditSplits { .. } => {
                self.apply_effects(action);
            }

            // --- Phase 3: built-in per-clip effects --------------------------
            Action::AddBuiltinEffect { .. }
            | Action::RemoveBuiltinEffect { .. }
            | Action::ReorderBuiltinEffects { .. }
            | Action::ToggleBuiltinEffect { .. }
            | Action::ClearBuiltinEffects { .. }
            | Action::SetBuiltinEffectParams { .. } => {
                self.apply_clip_effects(action);
            }

            // --- Phase 3: clip transitions + time-remap / speed --------------
            Action::AddTransitionFx { .. }
            | Action::RemoveTransitionFx { .. }
            | Action::SetTransitionFxDuration { .. }
            | Action::SetTransitionFxAlign { .. }
            | Action::SetTransitionFxKind { .. }
            | Action::SetClipSpeedFactor { .. }
            | Action::SetClipFrameBlend { .. }
            | Action::AddRemapKeyframe { .. }
            | Action::ClearRemapKeyframes { .. } => {
                self.apply_transition_fx(action);
            }

            // --- Export presets domain ----------------------------------------
            Action::AddExportPreset(_)
            | Action::DeleteExportPresetNew(_)
            | Action::ExportWithPreset(_)
            | Action::ToggleExportPresets
            | Action::ExportProRes(_)
            | Action::ExportDNxHD(_)
            | Action::ExportProResProxy(_)
            | Action::ExportProRes422(_)
            | Action::ExportGif(_)
            | Action::SetExportFormat(_)
            | Action::ToggleExportPanel
            | Action::AddExportPresetB10(_)
            | Action::RemoveExportPresetB10(_)
            | Action::SetActiveExportPreset(_)
            | Action::SetExportFormatB10(_)
            | Action::SetExportResolution(_)
            | Action::SetExportFrameRate(_)
            | Action::SetExportBitrate(_)
            | Action::SetExportAudioBitrate(_)
            | Action::SetExportTwoPass(_)
            | Action::SetExportPath(_)
            | Action::StartExport => {
                self.apply_export_presets(action);
            }

            // --- Graphics domain ---------------------------------------------
            Action::ToggleMogrLibrary
            | Action::AddMogrTemplate(_)
            | Action::RemoveMogrTemplate(_)
            | Action::SetActiveMogr(_)
            | Action::SetMogrParamText { .. }
            | Action::SetMogrParamNumber { .. }
            | Action::SetMogrParamColor { .. }
            | Action::ApplyMogrToClip { .. }
            | Action::DetachMogrFromClip { .. }
            | Action::ToggleSequenceSettingsPanel
            | Action::SetSeqResolution { .. }
            | Action::SetSeqFrameRate(_)
            | Action::SetSeqPixelAspect(_)
            | Action::SetSeqAudioSampleRate(_)
            | Action::SetSeqAudioChannels(_)
            | Action::SetSeqPreviewCodec(_)
            | Action::ApplySequenceSettings
            | Action::NestSelectedClipsB9 { .. }
            | Action::UnnestSequence(_)
            | Action::EnterNestedSequence(_)
            | Action::ExitNestedSequence
            | Action::RenameNestedSequence { .. }
            | Action::DuplicateNestedSequence(_)
            // Batch 11: TitlesGraphics
            | Action::CreateTitleClip { .. }
            | Action::AddTitleTextBox { .. }
            | Action::SetTitleTextContent { .. }
            | Action::SetTitleFont { .. }
            | Action::SetTitleClipFontSize { .. }
            | Action::SetTitleClipColor { .. }
            | Action::SetTitleBackground { .. }
            | Action::DeleteTitleClip(_) => {
                self.apply_graphics(action);
            }

            // --- Media domain ------------------------------------------------
            Action::ToggleBins
            | Action::SetBinQuery(_)
            | Action::AddBin(_)
            | Action::SelectBin(_)
            | Action::ImportToBin { .. }
            | Action::RemoveFromBin { .. }
            | Action::SelectBinClip { .. }
            | Action::InsertClipFromBin { .. }
            | Action::ToggleDualViewer
            | Action::SeekSource(_)
            | Action::ToggleSourcePlay
            | Action::SetSourceIn(_)
            | Action::SetSourceOut(_)
            | Action::InsertFromSource { .. }
            // Batch 11: ProjectManager
            | Action::OpenProjectManager
            | Action::CloseProjectManager
            | Action::SetProjectManagerDestination(_)
            | Action::SetProjectManagerMode(_)
            | Action::SetProjectManagerIncludeProxies(_)
            | Action::SetProjectManagerRenamMedia(_)
            | Action::RunProjectManager
            // Batch 11: MediaBrowser
            | Action::OpenMediaBrowser
            | Action::CloseMediaBrowser
            | Action::SetMediaBrowserPath(_)
            | Action::SetMediaBrowserFilter(_)
            | Action::SetMediaBrowserSearch(_)
            | Action::AddMediaBrowserEntry(_)
            | Action::ToggleMediaBrowserFavorite(_)
            | Action::ImportFromMediaBrowser { .. } => {
                self.apply_media(action);
            }

            // --- Multicam domain ---------------------------------------------
            Action::AddMulticamAngle(_)
            | Action::RemoveMulticamAngle(_)
            | Action::SetMulticamAngleLabel { .. }
            | Action::SetMulticamAngleSyncOffset { .. }
            | Action::ToggleMulticamAngle(_)
            | Action::SetMulticamSyncMode(_)
            | Action::SetMulticamDisplayMode(_)
            | Action::FlattenMulticam
            | Action::SwitchMulticamLive { .. }
            | Action::SetMulticamAngleCount(_)
            | Action::SyncMulticamWaveform { .. }
            | Action::SyncMulticamTimecode { .. }
            | Action::SetEdlFormat(_)
            | Action::SetEdlFrameRate(_)
            | Action::SetEdlReelName(_)
            | Action::SetEdlIncludeAudio(_)
            | Action::SetEdlIncludeVideo(_)
            | Action::ExportEdl(_)
            | Action::ImportEdl(_)
            | Action::ExportFcpXml(_)
            | Action::ImportFcpXml(_)
            | Action::ExportOtio(_)
            | Action::ToggleAutoReframePanel
            | Action::SetReframeAspect { .. }
            | Action::SetReframeMotion(_)
            | Action::SetReframeKeepScale(_)
            | Action::SetReframeAnalyzeOnImport(_)
            | Action::AnalyzeReframe { .. }
            | Action::ApplyReframe { .. }
            | Action::ClearReframeResults => {
                self.apply_multicam(action);
            }

            // --- Proxy domain ------------------------------------------------
            Action::ToggleProxyIngestPanel
            | Action::SetProxyFormat(_)
            | Action::SetProxyScale(_)
            | Action::SetProxyDestination(_)
            | Action::SetProxyCreateInBackground(_)
            | Action::CreateProxies { .. }
            | Action::AttachProxy { .. }
            | Action::DetachProxy { .. }
            | Action::ToggleProxyPlayback
            | Action::DeleteProxies { .. } => {
                self.apply_proxy(action);
            }

            // --- Batch 5 (new) -----------------------------------------------
            Action::ToggleScopesPanel
            | Action::SetScopeKind(_)
            | Action::SetScopeLayout(_)
            | Action::SetWaveformType(_)
            | Action::SetParadeType(_)
            | Action::SetVectorscopeType(_)
            | Action::SetHistogramChannel(_)
            | Action::SetScopeIntensity(_)
            | Action::SetScopeColorspace(_)
            | Action::SetScopeShowClipping(_)
            | Action::CreateBin { .. }
            | Action::RenameBin { .. }
            | Action::DeleteBin { .. }
            | Action::SetBinColor { .. }
            | Action::ToggleBinExpanded { .. }
            | Action::ImportMedia2 { .. }
            | Action::RemoveMedia { .. }
            | Action::MoveMediaToBin { .. }
            | Action::SetMediaLabel { .. }
            | Action::SetMediaLogNote { .. }
            | Action::SetMediaOffline { .. }
            | Action::RelinkMedia { .. }
            | Action::SetProjectSearch(_)
            | Action::SetMediaBrowserPath2(_)
            | Action::AttachMediaProxy { .. }
            | Action::DetachMediaProxy { .. }
            | Action::SetTransitionKind { .. }
            | Action::AddWipeTransition { .. }
            | Action::AddPagePeelTransition { .. }
            | Action::AddZoomTransition { .. }
            | Action::AddDipTransition { .. }
            | Action::AddCubeTransition { .. }
            | Action::SelectExportPresetB5 { .. }
            | Action::AddCustomExportPresetB5 { .. }
            | Action::DeleteCustomExportPresetB5 { .. }
            | Action::DuplicateExportPresetB5 { .. }
            | Action::SetExportWidthB5(_)
            | Action::SetExportHeightB5(_)
            | Action::SetExportFrameRateB5(_)
            | Action::SetExportVideoBitrateB5(_)
            | Action::SetExportAudioBitrateB5(_)
            | Action::SetExportContainerB5(_)
            | Action::SetExportVideoCodecB5(_)
            | Action::SetExportAudioCodecB5(_)
            | Action::SetExportTwoPassB5(_)
            | Action::SetExportHardwareEncodeB5(_)
            | Action::NewSequenceB5 { .. }
            | Action::DuplicateSequenceB5 { .. }
            | Action::DeleteSequenceB5 { .. }
            | Action::SetActiveSequenceB5 { .. }
            | Action::UpdateSequenceSettingsB5 { .. }
            | Action::NestSequenceB5 { .. }
            | Action::RenameSequence { .. } => {
                self.apply_batch5(action);
            }

            // --- Welcome screen actions ---------------------------------------
            Action::NewProject { width, height, frame_rate } => {
                // Reset to a blank project and apply the chosen preset dimensions.
                let (w, h, fr) = (*width, *height, *frame_rate);
                self.project = Project::new();
                self.host.comp_w = w;
                self.host.comp_h = h;
                self.time = 0.0;
                // Delegate to NewSequenceB5 so the active sequence also picks up
                // the resolution and frame-rate.
                let fr_copy = fr;
                self.apply(Action::NewSequenceB5 {
                    name: format!("{}x{} {:.2}fps", w, h, fr_copy),
                    width: w,
                    height: h,
                    frame_rate: fr_copy,
                });
            }
            Action::OpenFile => {
                // Stub: actual file-picker I/O is wired in a later wave.
                log::info!("reel: OpenFile requested from welcome screen");
            }

            // --- Transition geometry suite -----------------------------------
            Action::AddSlideTransition { .. }
            | Action::AddSpinTransition { .. }
            | Action::AddZoomTransition2 { .. }
            | Action::AddCubeFoldTransition { .. }
            | Action::AddPushTransition { .. }
            | Action::AddWipeTransition2 { .. } => {
                self.apply_transitions(action);
            }

            // --- EDL / FCP-XML export ----------------------------------------
            Action::WriteEdl { .. }
            | Action::WriteFcpXml { .. } => {
                self.apply_edl(action);
            }

            // --- HSL secondary curves + 3DL LUT ------------------------------
            Action::SetHslHueVsHue { .. }
            | Action::SetHslHueVsSat { .. }
            | Action::SetHslHueVsLuma { .. }
            | Action::ResetHslCurves
            | Action::Load3dlLut { .. }
            | Action::ExportCubeLut { .. } => {
                self.apply_color_curves(action);
            }

            // --- Project management (relink / consolidate / offline-online) ---
            Action::SetAllMediaOffline(_)
            | Action::ConsolidateProject { .. } => {
                self.apply_project_mgmt(action);
            }

            // --- Autosave / crash recovery -----------------------------------
            Action::SetAutosaveCadence(_)
            | Action::SetAutosaveKeep(_)
            | Action::SetAutosaveEnabledV2(_)
            | Action::CaptureSnapshot
            | Action::RestoreLatestSnapshot => {
                self.apply_autosave(action);
            }

            // --- Essential Graphics / Motion Graphics Templates --------------
            Action::AddGraphicsTemplate(_)
            | Action::RemoveGraphicsTemplate(_)
            | Action::ExposeTemplateProp { .. }
            | Action::SetTemplatePropValue { .. }
            | Action::InstantiateGraphicsTemplate { .. }
            | Action::SetGraphicsInstanceValue { .. }
            | Action::RemoveGraphicsInstance(_) => {
                self.apply_graphics_templates(action);
            }

            // --- Proxy workflow ----------------------------------------------
            Action::SetProxyWorkflowResolution(_)
            | Action::QueueProxyJob { .. }
            | Action::StartProxyJob(_)
            | Action::CompleteProxyJob(_)
            | Action::FailProxyJob(_)
            | Action::AttachProxyWorkflow { .. }
            | Action::DetachProxyWorkflow { .. }
            | Action::SetPlaybackSource(_)
            | Action::ToggleProxyFull => {
                self.apply_proxy_workflow(action);
            }

            // --- Background render cache -------------------------------------
            Action::RenderCacheRange { .. }
            | Action::InvalidateRenderCache { .. }
            | Action::PruneRenderCache
            | Action::ClearRenderCache
            | Action::SetRenderCacheCapacity(_) => {
                self.apply_render_cache(action);
            }

            // --- Preferences + remappable keybindings ------------------------
            Action::SetPrefAutosaveEnabled(_)
            | Action::SetPrefAutosaveInterval(_)
            | Action::SetPrefMaxVersions(_)
            | Action::SetPrefScratchDisk { .. }
            | Action::SetPrefPlaybackResolution(_)
            | Action::SetPrefPausedResolution(_)
            | Action::SetPrefDefaultTransition(_)
            | Action::RemapKeybinding { .. }
            | Action::ResetKeymap
            | Action::LoadKeymapJson(_) => {
                self.apply_prefs_keys(action);
            }

            // --- Workspaces (named panel layouts) ----------------------------
            Action::SwitchWorkspace(_)
            | Action::SwitchWorkspaceByName(_)
            | Action::SaveWorkspaceLayout(_)
            | Action::ResetWorkspace
            | Action::AddWorkspace(_)
            | Action::RemoveWorkspace(_)
            | Action::TogglePanelVisible(_) => {
                self.apply_workspaces(action);
            }

            // --- Timeline domain (catch-all for remaining actions) ------------
            _ => {
                self.apply_timeline(action);
            }
        }
    }
}
