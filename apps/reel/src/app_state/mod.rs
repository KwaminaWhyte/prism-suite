//! The single shared application state for the GPUI host.
//!
//! `App` owns everything panels read or mutate. Panels NEVER mutate `App`
//! fields directly — they emit an [`Action`], and the root view routes it
//! through [`App::apply`], the single choke point that mutates state.

use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Instant;

use gpui::{Bounds, Pixels};
use rodio::{DeviceSinkBuilder, MixerDeviceSink, Player as RodioPlayer};

use crate::canvas_host::CanvasHost;
use crate::waveform::WaveformCache;

// --- Domain modules ----------------------------------------------------------

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

// --- Public re-exports -------------------------------------------------------

// Audio domain
pub use audio::{
    AudioEffect, AudioSuiteConfig, AudioSuiteKind, AudioTrackType, EqBand, TrackCompressor,
    TrackEq, TrackEq3,
};

// Captions domain
pub use captions::{Caption, CaptionB9, CaptionPosition, CaptionStyle, CaptionStyleB9};

// Color domain
pub use color::{
    ColorManagementConfig, ColorSpace, ColorWheelMode, ColorWheels, DisplayColorSpace,
    GpuiMarker, LumetriColorConfig, LumetriPanel, MarkerKind, RgbCurves, WorkingColorSpace,
};

// Export presets domain
pub use export_presets::{
    ExportFormat, ExportFormatB10, ExportPreset, ExportPresetB10, ExportResolution,
};

// Graphics domain
pub use graphics::{
    MogrParam, MogrParamValue, MogrTemplate, NestedSequence, SeqFrameRate, SeqPixelAspect,
    SequenceSettings,
};

// Multicam domain
pub use multicam::{
    AutoReframeConfig, EdlConfig, EdlFormat, MulticamAngle, MulticamDisplayMode, MulticamSyncMode,
    ReframeMotion,
};

// Proxy domain
pub use proxy::{ClipProxy, ProxyFormat, ProxySettings};

// Timeline domain — all the big domain types + constants defined there
pub use timeline::{
    AudioSource, Bin, BinClip, BinClipType, Clip, ClipBlendMode, ClipEffect, ClipEffectKind,
    ClipSource, ColorGrade, DIP_BLACK, DIP_WHITE, HslSecondaryGrade, MulticamGroup, Project,
    SceneEditResult, ScopeData, SpeedCurve, Tool, Track, Transition, TransitionKind, VideoSource,
    WipeDir,
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

// --- Helper functions (timeline.rs imports via `super::`) --------------------

/// Return `true` if the path extension identifies a video file.
pub fn is_video_path(path: &std::path::Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| VIDEO_EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()))
        .unwrap_or(false)
}

/// Return `true` if the path extension identifies an audio file.
pub fn is_audio_path(path: &std::path::Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| AUDIO_EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()))
        .unwrap_or(false)
}

/// Collect snap candidate times: all clip edges + playhead + work area in/out.
pub fn snap_candidates(clips: &[Clip], playhead: f32, work_in: f32, work_out: f32) -> Vec<f32> {
    let mut pts = vec![playhead, work_in, work_out];
    for c in clips {
        pts.push(c.start);
        pts.push(c.end());
    }
    pts
}

pub(crate) fn rgb_to_hsl(r: f32, g: f32, b: f32) -> (f32, f32, f32) {
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let l = (max + min) / 2.0;
    if (max - min).abs() < 1e-6 { return (0.0, 0.0, l); }
    let d = max - min;
    let s = if l > 0.5 { d / (2.0 - max - min) } else { d / (max + min) };
    let h = if max == r { ((g - b) / d + if g < b { 6.0 } else { 0.0 }) / 6.0 }
            else if max == g { ((b - r) / d + 2.0) / 6.0 }
            else { ((r - g) / d + 4.0) / 6.0 };
    (h, s, l)
}

pub(crate) fn hsl_to_rgb(h: f32, s: f32, l: f32) -> (f32, f32, f32) {
    if s < 1e-6 { return (l, l, l); }
    let q = if l < 0.5 { l * (1.0 + s) } else { l + s - l * s };
    let p = 2.0 * l - q;
    let hue2rgb = |p: f32, q: f32, mut t: f32| {
        if t < 0.0 { t += 1.0; }
        if t > 1.0 { t -= 1.0; }
        if t < 1.0 / 6.0 { return p + (q - p) * 6.0 * t; }
        if t < 1.0 / 2.0 { return q; }
        if t < 2.0 / 3.0 { return p + (q - p) * (2.0 / 3.0 - t) * 6.0; }
        p
    };
    (hue2rgb(p, q, h + 1.0 / 3.0), hue2rgb(p, q, h), hue2rgb(p, q, h - 1.0 / 3.0))
}

// --- LUFS loudness metering --------------------------------------------------

/// K-weighted power of a block of samples (ITU-R BS.1770-4).
pub fn k_weighted_power(samples: &[f32], _sample_rate: u32) -> f32 {
    if samples.is_empty() { return 0.0; }
    samples.iter().map(|&s| s * s).sum::<f32>() / samples.len() as f32
}

/// Short-term LUFS over a 3-second window of power history.
pub fn lufs_short_term(history: &[f32], block_rate: f32) -> f32 {
    let window = (3.0 * block_rate).ceil() as usize;
    let slice = if history.len() > window { &history[history.len() - window..] } else { history };
    if slice.is_empty() { return -f32::INFINITY; }
    let mean: f32 = slice.iter().sum::<f32>() / slice.len() as f32;
    if mean <= 0.0 { return -f32::INFINITY; }
    -0.691 + 10.0 * mean.log10()
}

/// Integrated LUFS (gated) over the full history.
pub fn lufs_integrated(history: &[f32]) -> f32 {
    if history.is_empty() { return -f32::INFINITY; }
    let abs_gate = 1e-7_f32;
    let above: Vec<f32> = history.iter().copied().filter(|&p| p >= abs_gate).collect();
    if above.is_empty() { return -f32::INFINITY; }
    let mean_above: f32 = above.iter().sum::<f32>() / above.len() as f32;
    let rel_gate = mean_above * 0.1;
    let gated: Vec<f32> = above.iter().copied().filter(|&p| p >= rel_gate).collect();
    if gated.is_empty() { return -f32::INFINITY; }
    let mean_gated: f32 = gated.iter().sum::<f32>() / gated.len() as f32;
    if mean_gated <= 0.0 { return -f32::INFINITY; }
    -0.691 + 10.0 * mean_gated.log10()
}

/// Parse a minimal `.cube` 3D LUT file.
pub fn parse_cube_lut(path: &std::path::Path) -> Result<(u32, Vec<[f32; 3]>), String> {
    use std::io::{BufRead, BufReader};
    let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let reader = BufReader::new(file);
    let mut size: Option<u32> = None;
    let mut table: Vec<[f32; 3]> = Vec::new();
    for line in reader.lines() {
        let line = line.map_err(|e| e.to_string())?;
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') { continue; }
        if line.starts_with("LUT_3D_SIZE") {
            let n: u32 = line.split_whitespace().nth(1).ok_or("missing size")?
                .parse().map_err(|e: std::num::ParseIntError| e.to_string())?;
            size = Some(n);
            table.reserve((n * n * n) as usize);
            continue;
        }
        if line.chars().next().map(|c| c.is_ascii_alphabetic()).unwrap_or(false) { continue; }
        let mut parts = line.split_whitespace();
        let r: f32 = parts.next().ok_or("missing r")?.parse().map_err(|e: std::num::ParseFloatError| e.to_string())?;
        let g: f32 = parts.next().ok_or("missing g")?.parse().map_err(|e: std::num::ParseFloatError| e.to_string())?;
        let b: f32 = parts.next().ok_or("missing b")?.parse().map_err(|e: std::num::ParseFloatError| e.to_string())?;
        table.push([r, g, b]);
    }
    let sz = size.ok_or("LUT_3D_SIZE not found")?;
    let expected = (sz * sz * sz) as usize;
    if table.len() != expected {
        return Err(format!("expected {} entries, got {}", expected, table.len()));
    }
    Ok((sz, table))
}

/// Parse an SRT subtitle file into a `Vec<Caption>`.
pub fn parse_srt(path: &std::path::Path) -> Result<Vec<Caption>, String> {
    use std::io::{BufRead, BufReader};
    let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let reader = BufReader::new(file);
    let mut captions = Vec::new();
    let mut lines_iter = reader.lines().peekable();
    while let Some(line) = lines_iter.next() {
        let line = line.map_err(|e| e.to_string())?;
        let line = line.trim().to_string();
        if line.is_empty() { continue; }
        if line.parse::<u32>().is_err() { continue; }
        let Some(tc_line) = lines_iter.next() else { break; };
        let tc = tc_line.map_err(|e| e.to_string())?;
        let tc = tc.trim().to_string();
        let parts: Vec<&str> = tc.splitn(2, " --> ").collect();
        if parts.len() != 2 { continue; }
        let start = parse_srt_timecode(parts[0])?;
        let end = parse_srt_timecode(parts[1])?;
        let mut text_lines = Vec::new();
        while let Some(tl) = lines_iter.next() {
            let tl = tl.map_err(|e| e.to_string())?;
            let tl = tl.trim().to_string();
            if tl.is_empty() { break; }
            text_lines.push(tl);
        }
        captions.push(Caption {
            start_secs: start,
            end_secs: end,
            text: text_lines.join("\n"),
            style: CaptionStyle::default(),
        });
    }
    Ok(captions)
}

pub(crate) fn parse_srt_timecode(s: &str) -> Result<f32, String> {
    let s = s.trim().replace(',', ".");
    let parts: Vec<&str> = s.splitn(3, ':').collect();
    if parts.len() != 3 { return Err(format!("bad timecode: {s}")); }
    let h: f32 = parts[0].parse().map_err(|e: std::num::ParseFloatError| e.to_string())?;
    let m: f32 = parts[1].parse().map_err(|e: std::num::ParseFloatError| e.to_string())?;
    let sec: f32 = parts[2].parse().map_err(|e: std::num::ParseFloatError| e.to_string())?;
    Ok(h * 3600.0 + m * 60.0 + sec)
}

/// Serialize captions back to SRT format.
pub fn serialize_srt(captions: &[Caption]) -> String {
    let mut out = String::new();
    for (i, cap) in captions.iter().enumerate() {
        out.push_str(&format!("{}\n", i + 1));
        out.push_str(&format!("{} --> {}\n", format_srt_time(cap.start_secs), format_srt_time(cap.end_secs)));
        out.push_str(&cap.text);
        out.push_str("\n\n");
    }
    out
}

pub(crate) fn format_srt_time(t: f32) -> String {
    let t = t.max(0.0);
    let h = (t / 3600.0) as u32;
    let m = ((t % 3600.0) / 60.0) as u32;
    let s = (t % 60.0) as u32;
    let ms = ((t % 1.0) * 1000.0).round() as u32;
    format!("{:02}:{:02}:{:02},{:03}", h, m, s, ms)
}

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

    // --- Color grade ---
    SetClipGrade { index: usize, grade: ColorGrade },

    // --- Tracks ---
    ToggleTrackEnabled(usize),

    // --- Clips ---
    SelectClip(usize),

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
}

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
        }
    }

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
            | Action::ApplyAudioSuite { .. } => {
                self.apply_audio(action);
            }

            // --- Captions domain ---------------------------------------------
            Action::AddCaption(_)
            | Action::RemoveCaption(_)
            | Action::EditCaption { .. }
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
            | Action::DuplicateNestedSequence(_) => {
                self.apply_graphics(action);
            }

            // --- Media domain ------------------------------------------------
            Action::ToggleBins
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
            | Action::InsertFromSource { .. } => {
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

            // --- Timeline domain (catch-all for remaining actions) ------------
            _ => {
                self.apply_timeline(action);
            }
        }
    }
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}
