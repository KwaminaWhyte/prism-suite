//! The [`Action`] enum — every panel→state mutation a panel can request.
//!
//! Split out of `mod.rs` to satisfy the file-size rule. The enum is the single
//! vocabulary panels use to request state changes; the [`App::apply`] dispatcher
//! in `mod.rs` routes each variant to its domain handler.
//!
//! All payload types are re-exported from the parent `app_state` module, so
//! `use super::*` brings them into scope here unchanged.

use std::path::PathBuf;

use super::*;

// --- Action enum -------------------------------------------------------------

/// Every panel→state mutation a panel can request.
#[allow(dead_code)]
#[derive(Clone, Debug)]
pub enum Action {
    /// Select the active tool (toolbar).
    SetTool(Tool),

    // --- Media import ---
    ImportMedia(PathBuf),

    /// Move clip `index` to a new timeline `start`.
    MoveClip { index: usize, start: f32 },

    // --- Trimming ---
    TrimClipIn { index: usize, t: f32 },
    TrimClipOut { index: usize, t: f32 },

    // --- Ripple / roll trim ---
    RippleTrimClipIn { index: usize, t: f32 },
    RippleTrimClipOut { index: usize, t: f32 },
    RollTrimEdit { index: usize, delta: f32 },

    // --- Slip / slide edits ---
    /// Shift a clip's source in/out window by `delta`, leaving its timeline
    /// placement (start/duration) and neighbours untouched.
    SlipClip { index: usize, delta: f32 },
    /// Move a clip along the timeline by `delta`, growing the left neighbour's
    /// tail and trimming the right neighbour's head by the same amount.
    SlideClip { index: usize, delta: f32 },

    // --- Razor / split ---
    SplitClip { index: usize, t: f32 },
    SplitAtPlayhead,

    // --- Transport / playhead ---
    Seek(f32),
    StepBy(f32),
    TogglePlay,
    Pause,

    // --- Transitions ---
    AddCrossDissolve { index: usize },
    AddTransition { index: usize, kind: TransitionKind },

    // --- Per-clip audio volume ---
    SetClipGain { index: usize, gain: f32 },

    // --- Per-clip opacity (0.0–1.0; typeable inspector field) ---
    SetClipOpacity { index: usize, opacity: f32 },

    // --- Color grade ---
    SetClipGrade { index: usize, grade: ColorGrade },

    // --- Tracks ---
    ToggleTrackEnabled(usize),

    // --- Clips ---
    SelectClip(usize),

    // --- Marquee multi-select ---
    /// Rubber-band select every clip overlapping the time band `[t0, t1]` on the
    /// given `tracks` (empty = all tracks).
    MarqueeSelect { t0: f32, t1: f32, tracks: Vec<usize> },
    /// Clear the marquee multi-selection.
    ClearMarqueeSelection,

    // --- View ---
    ZoomBy(f32),
    ResetView,

    // --- Wave 8: audio crossfade ---
    SetClipFadeIn { index: usize, secs: f32 },
    SetClipFadeOut { index: usize, secs: f32 },

    // --- Wave 8: mixer ---
    SetTrackVolume { track_idx: usize, v: f32 },
    ToggleTrackMute { track_idx: usize },
    ToggleTrackSolo { track_idx: usize },
    SetMasterVolume { v: f32 },
    ToggleMixer,

    // --- Wave 8: LUT + white balance ---
    LoadLut { path: PathBuf },
    SetWhiteBalance { temp: f32, tint: f32 },

    // --- Wave 9: speed ramp ---
    SetClipSpeed(usize, f32),
    SetClipReverse(usize, bool),
    SetSpeedCurve { clip_id: usize, curve: SpeedCurve },

    // --- Wave 9: multi-camera ---
    ToggleMulticam,
    SwitchCamera(usize),
    SwitchMulticamAngle { group_id: usize, angle: usize },
    CreateMulticamGroup { name: String },

    // --- Wave 8: titles ---
    AddTitle,
    SetTitleText { index: usize, text: String },
    SetTitleFontSize { index: usize, size: f32 },
    SetTitleColor { index: usize, color: [u8; 4] },
    SetTitleBgColor { index: usize, color: Option<[u8; 4]> },

    // --- Wave 11: audio stub ---
    SetAudioVolume(f32),
    SetAudioMute(bool),

    // --- Wave 11: snap ---
    ToggleSnap,
    BeginClipDrag { clip_id: usize, grab_offset: f32 },
    MoveClipDrag { track_idx: usize, raw_t: f32 },
    EndClipDrag,

    // --- Wave 11: scopes ---
    ToggleScopes,
    SetScopeTab(u8),

    // --- Wave 11: dual viewer ---
    ToggleDualViewer,
    SeekSource(f32),
    ToggleSourcePlay,
    SetSourceIn(f32),
    SetSourceOut(f32),
    InsertFromSource { clip_id: usize, in_t: f32, out_t: f32 },

    // --- Wave 11: bins ---
    ToggleBins,
    AddBin(String),
    SelectBin(usize),
    ImportToBin { bin_idx: usize, path: PathBuf },
    RemoveFromBin { bin_idx: usize, clip_idx: usize },
    SelectBinClip { bin_idx: usize, clip_idx: usize },
    InsertClipFromBin { bin_clip_idx: usize, track_idx: usize, at_t: f32 },
    /// Filter the active bin's clip list to names containing this query
    /// (case-insensitive; empty = show all). Drives the bins panel search box.
    SetBinQuery(String),

    // --- Wave 13: film dissolve, editable transition duration ---
    SetTransitionDuration { track_idx: usize, trans_idx: usize, duration: f32 },
    AddFilmDissolve { index: usize },

    // --- Wave 13: chapter markers ---
    AddChapterMarker { time: f32, label: String },
    RemoveChapterMarker(usize),

    // --- Wave 13: linked audio/video clips ---
    ToggleLinkClip(usize),

    // --- Wave 13: export presets ---
    SaveExportPreset { name: String, format: String, width: u32, height: u32, fps: f32 },
    DeleteExportPreset(usize),
    ApplyExportPreset(usize),

    // --- Wave 13: nest sequence ---
    NestSelectedClips { name: String },
    SetHslSecondaryGrade { clip_id: usize, grade: HslSecondaryGrade },

    // --- Wave 15: custom transitions ---
    AddDiagonalWipe { index: usize },
    AddPixelDissolve { index: usize },

    // --- Transition geometry suite (see app_state/transitions.rs) ---
    AddSlideTransition { index: usize, direction: timeline::SlideDirection, duration: f32 },
    AddSpinTransition { index: usize, direction: timeline::SpinDirection },
    AddZoomTransition2 { index: usize, grow: bool },
    AddCubeFoldTransition { index: usize, direction: timeline::CubeDirection },
    AddPushTransition { index: usize, direction: timeline::WipeDir },
    AddWipeTransition2 { index: usize, direction: timeline::WipeDir },

    // --- Wave 15: proxy media ---
    SetProxyPath { index: usize, path: PathBuf },
    ClearProxy { index: usize },

    // --- Wave 15: log-to-Rec709 ---
    ToggleLogTransform { track_idx: usize },

    // --- Wave 15: ProRes / DNxHD export via ffmpeg ---
    ExportProRes(PathBuf),
    ExportDNxHD(PathBuf),
    ExportProResProxy(PathBuf),
    ExportProRes422(PathBuf),
    ExportGif(PathBuf),
    SetExportFormat(ExportFormat),

    // --- Wave 14: audio effects + track types ---
    ToggleTrackEq { track_idx: usize },
    SetEqBand { track_idx: usize, band: usize, gain_db: f32 },
    ToggleTrackCompressor { track_idx: usize },
    SetCompressor { track_idx: usize, threshold_db: f32, ratio: f32, attack_ms: f32, release_ms: f32, makeup_db: f32 },
    CycleAudioTrackType { track_idx: usize },
    SetTrackEq3 { track_idx: usize, eq: TrackEq3 },

    // --- Batch 3: 3-way color wheels ---
    SetColorWheels(ColorWheels),

    // --- Batch 3: rate stretch ---
    RateStretchClip { index: usize, new_duration: f32 },

    // --- Batch 3: sequence settings ---
    ToggleSequenceSettings,
    SetSequenceSize { w: u32, h: u32 },
    SetFrameRate(f32),
    SetSampleRate(u32),
    SetColorSpace(ColorSpace),

    // --- Batch 4: RGB curves ---
    SetRgbCurves(RgbCurves),
    SetCurvesChannel(u8),
    AddCurvePoint { channel: u8, point: [f32; 2] },
    MoveCurvePoint { channel: u8, index: usize, point: [f32; 2] },

    // --- Batch 4: audio effect chains ---
    AddAudioEffect { track_idx: usize, effect: AudioEffect },
    RemoveAudioEffect { track_idx: usize, effect_idx: usize },
    SetAudioEffect { track_idx: usize, effect_idx: usize, effect: AudioEffect },
    ToggleTrackFx { track_idx: usize },
    ExpandTrackFx { track_idx: usize, effect_idx: Option<usize> },

    // --- Batch 4: closed captions ---
    AddCaption(Caption),
    RemoveCaption(usize),
    EditCaption { index: usize, caption: Caption },
    /// Replace just the cue text of caption `index` in `App::captions` (the
    /// list the Captions panel renders). Multi-line: the typed text may contain
    /// `\n` line breaks. Distinct from `SetCaptionText`, which targets the
    /// separate `captions_b9` list.
    SetCueText { index: usize, text: String },
    SetCaptionPosition { index: usize, position: CaptionPosition },
    SetCaptionCueColor { index: usize, color: [f32; 4] },
    ImportSrt(PathBuf),
    ExportSrt(PathBuf),
    ToggleCaptionsPanel,

    // --- Batch 4: export preset list ---
    AddExportPreset(ExportPreset),
    DeleteExportPresetNew(usize),
    ExportWithPreset(usize),
    ToggleExportPresets,

    // --- Batch 4: markers panel ---
    ToggleMarkersPanel,
    AddMarkerAt { time: f32, name: String, kind: MarkerKind },
    RemoveMarker(usize),
    RenameMarker { index: usize, name: String },
    SetInPoint(f32),
    SetOutPoint(f32),

    // --- Batch 5: copy / paste / duplicate ---
    CopySelectedClips,
    CutSelectedClips,
    PasteClips { at_t: f32 },
    DuplicateSelectedClips,

    // --- Batch 5: clip transform ---
    SetClipAnchor { clip_idx: usize, x: f32, y: f32 },
    SetClipCrop { clip_idx: usize, left: f32, right: f32, top: f32, bottom: f32 },
    SetClipBlendMode { clip_idx: usize, mode: ClipBlendMode },
    ResetClipTransform { clip_idx: usize },

    // --- Batch 5: time remap ---
    SetTimeRemapEnabled { clip_idx: usize, enabled: bool },
    AddTimeRemapKey { clip_idx: usize, timeline_t: f32, source_t: f32 },
    MoveTimeRemapKey { clip_idx: usize, key_idx: usize, source_t: f32 },
    RemoveTimeRemapKey { clip_idx: usize, key_idx: usize },
    SetFreezeFrame { clip_idx: usize, at_t: f32 },

    // --- Speed-factor time remap (piecewise integration) ---
    SetTimeRemapSpeedKeys { clip_idx: usize, keys: Vec<(f32, f32)> },
    AddTimeRemapSpeedKey { clip_idx: usize, timeline_t: f32, factor: f32 },
    AddSpeedFreezeFrame { clip_idx: usize, at_t: f32, hold_secs: f32 },

    // --- Batch 5: LUFS metering ---
    UpdateLufsMeters { power: f32 },
    ResetLufsIntegrated,

    // --- Batch 5: group ripple trim ---
    GroupRippleTrimIn { clip_indices: Vec<usize>, delta: f32 },
    GroupRippleTrimOut { clip_indices: Vec<usize>, delta: f32 },

    // --- Batch 7: per-clip video effects ---
    AddClipEffect { clip_idx: usize, effect: ClipEffect },
    RemoveClipEffect { clip_idx: usize, effect_idx: usize },
    SetClipEffect { clip_idx: usize, effect_idx: usize, effect: ClipEffect },
    ToggleClipEffect { clip_idx: usize, effect_idx: usize },
    ReorderClipEffects { clip_idx: usize, from: usize, to: usize },
    ClearClipEffects { clip_idx: usize },

    // --- Phase 3: built-in per-clip effects (see app_state/clip_effects.rs) ---
    /// Push a new built-in effect onto a clip's ordered stack (params sanitized).
    AddBuiltinEffect { clip_idx: usize, effect: BuiltinEffect },
    /// Remove the effect at `effect_idx` from a clip's stack.
    RemoveBuiltinEffect { clip_idx: usize, effect_idx: usize },
    /// Move an effect from one stack position to another.
    ReorderBuiltinEffects { clip_idx: usize, from: usize, to: usize },
    /// Toggle the `enabled` (bypass) flag of a single built-in effect.
    ToggleBuiltinEffect { clip_idx: usize, effect_idx: usize },
    /// Remove every built-in effect from a clip.
    ClearBuiltinEffects { clip_idx: usize },
    /// Replace one effect's parameters (sanitized/clamped on apply).
    SetBuiltinEffectParams { clip_idx: usize, effect_idx: usize, params: EffectParams },

    // --- Batch 7: clip motion (transform) ---
    SetClipMotion { clip_idx: usize, x: f32, y: f32 },
    SetClipMotionScale { clip_idx: usize, sx: f32, sy: f32 },
    SetClipMotionRotation { clip_idx: usize, angle: f32 },
    ResetClipMotion { clip_idx: usize },

    // --- Batch 7: scene edit detection ---
    SetSceneEditSensitivity(f32),
    DetectSceneEdits { clip_idx: usize },
    ApplySceneEditSplits { clip_idx: usize },

    // --- Batch 7: project management ---
    SetProjectName(String),
    SetProjectPath(std::path::PathBuf),
    AddRecentProject(std::path::PathBuf),
    SetProjectNotes(String),
    SetAutoSaveEnabled(bool),
    SetAutoSaveInterval(u32),
    TriggerAutoSave,

    // --- Batch 8: multicam depth ---
    AddMulticamAngle(MulticamAngle),
    RemoveMulticamAngle(usize),
    SetMulticamAngleLabel { idx: usize, label: String },
    SetMulticamAngleSyncOffset { idx: usize, offset: f32 },
    ToggleMulticamAngle(usize),
    SetMulticamSyncMode(MulticamSyncMode),
    SetMulticamDisplayMode(MulticamDisplayMode),
    FlattenMulticam,

    // --- Live multicam angle switching + sync ---
    /// Record a live angle switch at `time` (seconds) → `angle`.
    SwitchMulticamLive { time: f32, angle: usize },
    /// Set the number of available angles in the live multicam clip.
    SetMulticamAngleCount(usize),
    /// Waveform-correlate angle `a_idx` vs `b_idx` (using their cached audio
    /// envelopes) and store the resulting sync offset on `b_idx`.
    SyncMulticamWaveform { a_idx: usize, b_idx: usize, bins_per_sec: f32 },
    /// Timecode-sync angle `b_idx` (frame count `b_tc`) onto `a_idx` (`a_tc`).
    SyncMulticamTimecode { a_idx: usize, b_idx: usize, a_tc: u64, b_tc: u64, fps: u32 },

    // --- Batch 8: EDL / XML interchange ---
    SetEdlFormat(EdlFormat),
    SetEdlFrameRate(f32),
    SetEdlReelName(String),
    SetEdlIncludeAudio(bool),
    SetEdlIncludeVideo(bool),
    ExportEdl(std::path::PathBuf),
    ImportEdl(std::path::PathBuf),
    ExportFcpXml(std::path::PathBuf),
    ImportFcpXml(std::path::PathBuf),
    ExportOtio(std::path::PathBuf),

    // --- EDL / FCP-XML writers (real content, see app_state/edl.rs) ---
    WriteEdl { path: std::path::PathBuf },
    WriteFcpXml { path: std::path::PathBuf },

    // --- HSL secondary curves + 3DL LUT (see app_state/color_curves.rs) ---
    SetHslHueVsHue(Vec<[f32; 2]>),
    SetHslHueVsSat(Vec<[f32; 2]>),
    SetHslHueVsLuma(Vec<[f32; 2]>),
    ResetHslCurves,
    Load3dlLut { path: std::path::PathBuf },
    ExportCubeLut { path: std::path::PathBuf },

    // --- Batch 8: audio suite ---
    ToggleAudioSuitePanel,
    SetAudioSuiteKind(AudioSuiteKind),
    SetAudioSuiteGain(f32),
    SetAudioSuitePreserve(bool),
    SetAudioSuiteProcessInPlace(bool),
    SetAudioSuiteTargetLevel(f32),
    SetAudioSuitePitch(f32),
    SetAudioSuiteStretch(f32),
    ToggleAudioSuitePreview,
    ApplyAudioSuite { clip_idx: usize },

    // --- Batch 8: auto reframe ---
    ToggleAutoReframePanel,
    SetReframeAspect { w: u32, h: u32 },
    SetReframeMotion(ReframeMotion),
    SetReframeKeepScale(bool),
    SetReframeAnalyzeOnImport(bool),
    AnalyzeReframe { clip_idx: usize },
    ApplyReframe { clip_idx: usize },
    ClearReframeResults,

    // --- Batch 9: Lumetri Color depth ---
    SetLumetriPanel(LumetriPanel),
    SetLumetriExposure(f32),
    SetLumetriContrast(f32),
    SetLumetriHighlights(f32),
    SetLumetriShadows(f32),
    SetLumetriWhites(f32),
    SetLumetriBlacks(f32),
    SetLumetriTemp(f32),
    SetLumetriTint(f32),
    SetLumetriSaturation(f32),
    SetLumetriFadedFilm(f32),
    SetLumetriSharpen(f32),
    SetLumetriVibrance(f32),
    SetLumetriHslHueRange([f32; 2]),
    SetLumetriHslSatRange([f32; 2]),
    SetLumetriHslShifts { hue: f32, sat: f32, luma: f32 },
    SetLumetriVignette { amount: f32, midpoint: f32, roundness: f32, feather: f32 },
    ResetLumetriPanel,
    ApplyLumetriToClip { clip_idx: usize },
    ToggleLumetriPanel,

    // --- Batch 9: Captions / Subtitles (extended) ---
    AddCaptionB9(CaptionB9),
    RemoveCaptionB9(usize),
    SetCaptionText { idx: usize, text: String },
    SetCaptionTiming { idx: usize, start: f32, end: f32 },
    SetCaptionSpeaker { idx: usize, speaker: String },
    SetCaptionStyle { idx: usize, style_id: usize },
    AddCaptionStyleB9(CaptionStyleB9),
    ToggleCaptionTrack,
    ExportSrtB9(std::path::PathBuf),
    ImportSrtB9(std::path::PathBuf),
    AutoTranscribe,

    // --- Batch 9: Sequence Settings (extended) ---
    ToggleSequenceSettingsPanel,
    SetSeqResolution { w: u32, h: u32 },
    SetSeqFrameRate(SeqFrameRate),
    SetSeqPixelAspect(SeqPixelAspect),
    SetSeqAudioSampleRate(u32),
    SetSeqAudioChannels(u8),
    SetSeqPreviewCodec(String),
    ApplySequenceSettings,

    // --- Batch 9: Nest Sequence (extended) ---
    NestSelectedClipsB9 { name: String },
    UnnestSequence(usize),
    EnterNestedSequence(usize),
    ExitNestedSequence,
    RenameNestedSequence { idx: usize, name: String },
    DuplicateNestedSequence(usize),

    // --- Batch 10: Essential Graphics (Motion Graphics Templates) ---
    ToggleMogrLibrary,
    AddMogrTemplate(MogrTemplate),
    RemoveMogrTemplate(usize),
    SetActiveMogr(Option<usize>),
    SetMogrParamText { template_idx: usize, param_idx: usize, text: String },
    SetMogrParamNumber { template_idx: usize, param_idx: usize, value: f32 },
    SetMogrParamColor { template_idx: usize, param_idx: usize, color: [f32; 4] },
    ApplyMogrToClip { clip_idx: usize, template_idx: usize },
    DetachMogrFromClip { clip_idx: usize },

    // --- Batch 10: Color Management ---
    ToggleColorManagementPanel,
    SetColorManagementEnabled(bool),
    SetDisplayColorSpace(DisplayColorSpace),
    SetWorkingColorSpace(WorkingColorSpace),
    SetOutputLutPath(Option<std::path::PathBuf>),
    SetHdrOutput(bool),
    SetMaxLuminance(f32),
    SetApplyLutOnExport(bool),
    ResetColorManagement,

    // --- Batch 10: Export Presets (new model) ---
    ToggleExportPanel,
    AddExportPresetB10(ExportPresetB10),
    RemoveExportPresetB10(usize),
    SetActiveExportPreset(usize),
    SetExportFormatB10(ExportFormatB10),
    SetExportResolution(ExportResolution),
    SetExportFrameRate(f32),
    SetExportBitrate(f32),
    SetExportAudioBitrate(u32),
    SetExportTwoPass(bool),
    SetExportPath(std::path::PathBuf),
    StartExport,

    // --- Batch 10: Proxy Workflow ---
    ToggleProxyIngestPanel,
    SetProxyFormat(ProxyFormat),
    SetProxyScale(f32),
    SetProxyDestination(std::path::PathBuf),
    SetProxyCreateInBackground(bool),
    CreateProxies { clip_indices: Vec<usize> },
    AttachProxy { clip_idx: usize, path: std::path::PathBuf },
    DetachProxy { clip_idx: usize },
    ToggleProxyPlayback,
    DeleteProxies { clip_indices: Vec<usize> },

    // --- Batch 11: AudioTrackMixer ---
    OpenAudioMixer,
    CloseAudioMixer,
    AddAudioMixerTrack { track_id: usize, kind: AudioTrackKind },
    SetAudioMixerFader { track_id: usize, level: f32 },
    SetAudioMixerPan { track_id: usize, pan: f32 },
    SetAudioMixerMute { track_id: usize, muted: bool },
    SetAudioMixerSolo { track_id: usize, solo: bool },
    SetMasterFader(f32),
    AddAudioSend { track_id: usize, destination: AudioSendDestination, level: f32 },

    // --- Batch 11: TitlesGraphics ---
    CreateTitleClip { name: String },
    AddTitleTextBox { clip_id: usize, text: String, x: f32, y: f32 },
    SetTitleTextContent { clip_id: usize, box_index: usize, text: String },
    SetTitleFont { clip_id: usize, box_index: usize, font: String },
    SetTitleClipFontSize { clip_id: usize, box_index: usize, size: f32 },
    SetTitleClipColor { clip_id: usize, box_index: usize, color: String },
    SetTitleBackground { clip_id: usize, color: Option<String> },
    DeleteTitleClip(usize),

    // --- Project management: relink / consolidate / offline-online ---
    /// Mark every project media item offline/online in bulk.
    SetAllMediaOffline(bool),
    /// Build + store a consolidate manifest collecting online media into a folder.
    ConsolidateProject { destination: String },

    // --- Autosave / crash recovery ---
    SetAutosaveCadence(u64),
    SetAutosaveKeep(usize),
    SetAutosaveEnabledV2(bool),
    CaptureSnapshot,
    RestoreLatestSnapshot,

    // --- Batch 11: ProjectManager ---
    OpenProjectManager,
    CloseProjectManager,
    SetProjectManagerDestination(String),
    SetProjectManagerMode(ProjectCollectMode),
    SetProjectManagerIncludeProxies(bool),
    SetProjectManagerRenamMedia(bool),
    RunProjectManager,

    // --- Batch 11: MediaBrowser ---
    OpenMediaBrowser,
    CloseMediaBrowser,
    SetMediaBrowserPath(String),
    SetMediaBrowserFilter(MediaBrowserFilter),
    SetMediaBrowserSearch(String),
    AddMediaBrowserEntry(MediaBrowserEntry),
    ToggleMediaBrowserFavorite(String),
    ImportFromMediaBrowser { path: String },

    // --- Batch 5 (new): Lumetri Scopes State ---------------------------------
    ToggleScopesPanel,
    SetScopeKind(reel_project::ScopeKind),
    SetScopeLayout(reel_project::ScopeLayout),
    SetWaveformType(reel_project::WaveformType),
    SetParadeType(reel_project::ParadeType),
    SetVectorscopeType(reel_project::VectorscopeType),
    SetHistogramChannel(reel_project::HistogramChannel),
    SetScopeIntensity(f32),
    SetScopeColorspace(reel_project::ScopeColorspace),
    SetScopeShowClipping(bool),

    // --- Batch 5 (new): Project Bins -----------------------------------------
    CreateBin { name: String, parent_id: Option<usize> },
    RenameBin { bin_id: usize, name: String },
    DeleteBin { bin_id: usize },
    SetBinColor { bin_id: usize, color: reel_project::BinColor },
    ToggleBinExpanded { bin_id: usize },
    ImportMedia2 { path: String, bin_id: Option<usize> },
    RemoveMedia { item_id: usize },
    MoveMediaToBin { item_id: usize, bin_id: Option<usize> },
    SetMediaLabel { item_id: usize, label: reel_project::BinColor },
    SetMediaLogNote { item_id: usize, note: String },
    SetMediaOffline { item_id: usize, offline: bool },
    RelinkMedia { item_id: usize, new_path: String },
    SetProjectSearch(String),
    SetMediaBrowserPath2(String),
    AttachMediaProxy { item_id: usize, proxy_path: String },
    DetachMediaProxy { item_id: usize },

    // --- Batch 5 (new): Transitions ------------------------------------------
    SetTransitionKind { transition_id: usize, kind: TransitionKind },
    AddWipeTransition { clip_id: usize, direction: timeline::SlideDirection, duration_s: f64 },
    AddPagePeelTransition { clip_id: usize, direction: timeline::PagePeelDirection },
    AddZoomTransition { clip_id: usize, grow: bool },
    AddDipTransition { clip_id: usize, color: String },
    AddCubeTransition { clip_id: usize, direction: timeline::CubeDirection },

    // --- Batch 5 (new): Export Presets B5 ------------------------------------
    SelectExportPresetB5 { preset_id: usize },
    AddCustomExportPresetB5 { preset: reel_project::ExportPresetB5 },
    DeleteCustomExportPresetB5 { preset_id: usize },
    DuplicateExportPresetB5 { preset_id: usize },
    SetExportWidthB5(u32),
    SetExportHeightB5(u32),
    SetExportFrameRateB5(f64),
    SetExportVideoBitrateB5(u32),
    SetExportAudioBitrateB5(u32),
    SetExportContainerB5(reel_project::ExportContainer),
    SetExportVideoCodecB5(reel_project::VideoCodecB5),
    SetExportAudioCodecB5(reel_project::AudioCodecB5),
    SetExportTwoPassB5(bool),
    SetExportHardwareEncodeB5(bool),

    // --- Welcome screen actions -----------------------------------------------
    /// Create a new project with a given resolution and frame rate. Dispatched
    /// from the welcome screen preset cards. Resets the project to defaults and
    /// sets the composition dimensions / frame rate.
    NewProject { width: u32, height: u32, frame_rate: f64 },
    /// Open an existing project via a file picker dialog. Dispatched from the
    /// welcome screen "Open Project..." button. Stub — I/O in a later wave.
    OpenFile,

    // --- Batch 5 (new): Sequences B5 -----------------------------------------
    NewSequenceB5 { name: String, width: u32, height: u32, frame_rate: f64 },
    DuplicateSequenceB5 { sequence_id: usize },
    DeleteSequenceB5 { sequence_id: usize },
    SetActiveSequenceB5 { sequence_id: usize },
    UpdateSequenceSettingsB5 { sequence_id: usize, width: Option<u32>, height: Option<u32>, frame_rate: Option<f64> },
    NestSequenceB5 { sequence_id: usize, into_sequence_id: usize, at_time_s: f64 },

    // --- Essential Graphics / Motion Graphics Templates ----------------------
    AddGraphicsTemplate(GraphicsTemplate),
    RemoveGraphicsTemplate(usize),
    ExposeTemplateProp { template_idx: usize, label: String, target: GtPropTarget },
    SetTemplatePropValue { template_idx: usize, prop_idx: usize, value: GtPropValue },
    InstantiateGraphicsTemplate { template_idx: usize, start: f32, duration: f32 },
    SetGraphicsInstanceValue { instance_idx: usize, label: String, value: GtPropValue },
    RemoveGraphicsInstance(usize),

    // --- Proxy workflow (job model + proxy/full toggle) ----------------------
    SetProxyWorkflowResolution(ProxyResolution),
    QueueProxyJob { clip_idx: usize, resolution: ProxyResolution, path: std::path::PathBuf },
    StartProxyJob(usize),
    CompleteProxyJob(usize),
    FailProxyJob(usize),
    AttachProxyWorkflow { clip_idx: usize, path: std::path::PathBuf },
    DetachProxyWorkflow { clip_idx: usize },
    SetPlaybackSource(PlaybackSource),
    ToggleProxyFull,

    // --- Background render cache ----------------------------------------------
    RenderCacheRange { start: f32, end: f32 },
    InvalidateRenderCache { start: f32, end: f32 },
    PruneRenderCache,
    ClearRenderCache,
    SetRenderCacheCapacity(usize),

    // --- Preferences + remappable keybindings ---------------------------------
    SetPrefAutosaveEnabled(bool),
    SetPrefAutosaveInterval(u32),
    SetPrefMaxVersions(u32),
    SetPrefScratchDisk { kind: ScratchKind, path: std::path::PathBuf },
    SetPrefPlaybackResolution(PlaybackResolution),
    SetPrefPausedResolution(PlaybackResolution),
    SetPrefDefaultTransition(f32),
    RemapKeybinding { command: EditorCommand, chord: KeyChord },
    ResetKeymap,
    LoadKeymapJson(String),

    // --- Workspaces (named panel layouts) -------------------------------------
    SwitchWorkspace(usize),
    SwitchWorkspaceByName(String),
    SaveWorkspaceLayout(PanelLayout),
    ResetWorkspace,
    AddWorkspace(String),
    RemoveWorkspace(usize),
    TogglePanelVisible(Panel),

    // --- Real-typing rename (TextField-driven) --------------------------------
    /// Rename a clip in `project.clips` by index. Empty names are ignored so a
    /// fully-cleared field never blanks the clip label.
    RenameClip { index: usize, name: String },
    /// Rename a track in `project.tracks` by index. Empty names are ignored.
    RenameTrack { index: usize, name: String },
    /// Rename a Batch-5 sequence by `id`. Empty names are ignored.
    RenameSequence { sequence_id: usize, name: String },

    // --- Phase 3: clip transitions + time-remap / speed (transition_fx.rs) ----
    /// Place a Phase-3 transition anchored to clip `clip_idx`'s cut / edge.
    AddTransitionFx { clip_idx: usize, kind: TransitionFxKind, duration_frames: u32, align: TransitionAlign },
    /// Remove the transition at `idx` from the `transition_fx` list.
    RemoveTransitionFx { idx: usize },
    /// Set a transition's duration in frames (clamped to >= 1).
    SetTransitionFxDuration { idx: usize, duration_frames: u32 },
    /// Set a transition's alignment (centered / start / end).
    SetTransitionFxAlign { idx: usize, align: TransitionAlign },
    /// Change a transition's kind.
    SetTransitionFxKind { idx: usize, kind: TransitionFxKind },
    /// Set a clip's signed speed factor (negative = reverse). Drops keyframed remap.
    SetClipSpeedFactor { clip_idx: usize, factor: f32 },
    /// Toggle frame-blend (vs nearest) sampling for a clip's time-remap.
    SetClipFrameBlend { clip_idx: usize, blend: bool },
    /// Add a `(timeline_t, source_t)` time-remap keyframe (enables remap, sorted).
    AddRemapKeyframe { clip_idx: usize, timeline_t: f32, source_t: f32 },
    /// Clear all of a clip's time-remap keyframes (disables remap).
    ClearRemapKeyframes { clip_idx: usize },

    // --- Phase 4: Lumetri-grade per-clip colour (color_grade.rs) --------------
    /// Set a clip's grade exposure in stops (clamped ±6).
    SetGradeExposure { clip: usize, value: f32 },
    /// Set a clip's grade contrast about mid-grey (clamped −1..2).
    SetGradeContrast { clip: usize, value: f32 },
    /// Set a clip's grade saturation multiplier (clamped 0..4; 0 = greyscale).
    SetGradeSaturation { clip: usize, value: f32 },
    /// Set a clip's white-balance temperature (clamped ±1; >0 warmer).
    SetGradeTemperature { clip: usize, value: f32 },
    /// Set a clip's white-balance tint (clamped ±1; >0 magenta).
    SetGradeTint { clip: usize, value: f32 },
    /// Set a clip's whites tone control (clamped ±1).
    SetGradeWhites { clip: usize, value: f32 },
    /// Set a clip's blacks tone control (clamped ±1).
    SetGradeBlacks { clip: usize, value: f32 },
    /// Set a clip's highlights tone control (clamped ±1).
    SetGradeHighlights { clip: usize, value: f32 },
    /// Set a clip's shadows tone control (clamped ±1).
    SetGradeShadows { clip: usize, value: f32 },
    /// Set one of a clip's lift/gamma/gain wheels (RGB trackball + master).
    SetGradeWheel { clip: usize, which: WheelKind, rgb: [f32; 3], master: f32 },
    /// Add a control point to one of a clip's RGB/per-channel curves (sorted).
    AddGradeCurvePoint { clip: usize, channel: GradeCurveChannel, point: [f32; 2] },
    /// Replace a clip's HSL secondary qualifier + correction.
    SetGradeSecondary { clip: usize, qualifier: SecondaryQualifier },
    /// Remove a clip's entire grade (back to identity).
    ResetClipGrade { clip: usize },
}
