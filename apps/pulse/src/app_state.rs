//! The single shared application state for the GPUI host.
//!
//! `App` owns everything panels read or mutate: the Pulse [`Project`] (comp
//! model), the [`CanvasHost`] (CPU compositor bridge), the current playhead
//! time, the active tool, and the view. Panels NEVER mutate `App` fields
//! directly — they emit an [`Action`], and the root view routes it through
//! [`App::apply`], the single choke point that mutates state and marks the host
//! dirty when the composited frame changes. This keeps the panel->state seam
//! narrow so panels can be ported in parallel: a new panel only needs to (a)
//! read `&App` and (b) add `Action` variants + their `apply` arms.
//!
//! Defaults mirror the egui app (`crate::app::PulseApp::new`) so parity is
//! reachable: a fresh `Project` (demo comp, two layers), playhead at t=0.

use std::cell::Cell;
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Instant;

use gpui::{Bounds, Pixels};
use crate::comp::{
    Affine2, BrowserEntry, Interp, NewEffect, Project, Prop, Transform, WorkArea,
};
use crate::gizmo::{self, GizmoGeom, Handle as GizmoHandle};

use crate::canvas_host::CanvasHost;
use crate::effect_params::{self, EffectStack};
use crate::export::{self, ExportProgress, ExportRequest, OutputFormat};
use crate::gpui_effects::{GpuiEffect, GpuiEffectKind};
use crate::render::RenderRange;

/// Status of a job in the render queue.
#[derive(Clone, Debug)]
pub enum RenderJobStatus {
    Pending,
    Rendering(f32),
    Done,
    Failed(String),
}

/// Output format for a render queue item.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum RenderFormat {
    /// H.264 MP4 — broadly compatible, smallest file (default).
    Mp4H264,
    /// H.265/HEVC MP4 — better quality/size ratio; requires hardware support.
    Mp4H265,
    /// Apple ProRes 422 Proxy — lossless-ish, large; requires FFmpeg.
    ProResProxy,
    /// Animated GIF — legacy web format, palette-quantized.
    Gif,
}

impl RenderFormat {
    pub fn label(self) -> &'static str {
        match self {
            RenderFormat::Mp4H264 => "H.264 MP4",
            RenderFormat::Mp4H265 => "H.265 MP4",
            RenderFormat::ProResProxy => "ProRes Proxy",
            RenderFormat::Gif => "Animated GIF",
        }
    }

    pub fn extension(self) -> &'static str {
        match self {
            RenderFormat::Mp4H264 | RenderFormat::Mp4H265 => "mp4",
            RenderFormat::ProResProxy => "mov",
            RenderFormat::Gif => "gif",
        }
    }

    /// All supported render formats, for UI pickers.
    pub const ALL: [RenderFormat; 4] = [
        RenderFormat::Mp4H264,
        RenderFormat::Mp4H265,
        RenderFormat::ProResProxy,
        RenderFormat::Gif,
    ];
}

/// One export job in the render queue.
#[derive(Clone, Debug)]
pub struct RenderJob {
    pub comp_name: String,
    pub output_path: PathBuf,
    pub format: RenderFormat,
    pub status: RenderJobStatus,
}

/// Pending (not-yet-applied) composition settings for the settings dialog.
#[derive(Clone, Debug)]
pub struct PendingCompSettings {
    pub width: u32,
    pub height: u32,
    pub fps: f32,
    pub duration_secs: f32,
    pub bg_color: [f32; 4],
}

/// Cap on the undo/redo snapshot stack depth. Each entry is a full [`Project`]
/// clone; 64 levels is generous for an interactive session while bounding RAM.
const UNDO_LIMIT: usize = 64;

/// Expression control layer kind (Wave 14).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ExprControlKind {
    Slider,
    Angle,
    Checkbox,
    Color,
    Point,
}

impl ExprControlKind {
    /// Short badge label for the UI (used by the expr_controls panel).
    pub fn label(self) -> &'static str {
        match self {
            ExprControlKind::Slider => "SL",
            ExprControlKind::Angle => "∠",
            ExprControlKind::Checkbox => "☑",
            ExprControlKind::Color => "◉",
            ExprControlKind::Point => "↖",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            ExprControlKind::Slider => "Slider Control",
            ExprControlKind::Angle => "Angle Control",
            ExprControlKind::Checkbox => "Checkbox Control",
            ExprControlKind::Color => "Color Control",
            ExprControlKind::Point => "Point Control",
        }
    }

    /// All control kinds, for Add-Control buttons.
    pub const ALL: [ExprControlKind; 5] = [
        ExprControlKind::Slider,
        ExprControlKind::Angle,
        ExprControlKind::Checkbox,
        ExprControlKind::Color,
        ExprControlKind::Point,
    ];
}

/// Runtime value for an expression control layer.
#[derive(Clone, Debug)]
pub enum ExprControlValue {
    Slider(f32),
    Angle(f32),
    Checkbox(bool),
    Color([f32; 4]),
    Point(f32, f32),
}

/// Expression control layer state (one per ExpressionControl layer).
#[derive(Clone, Debug)]
pub struct ExprControl {
    pub kind: ExprControlKind,
    pub value: ExprControlValue,
}

impl ExprControl {
    pub fn default_for(kind: ExprControlKind) -> Self {
        let value = match kind {
            ExprControlKind::Slider => ExprControlValue::Slider(50.0),
            ExprControlKind::Angle => ExprControlValue::Angle(0.0),
            ExprControlKind::Checkbox => ExprControlValue::Checkbox(false),
            ExprControlKind::Color => ExprControlValue::Color([1.0, 1.0, 1.0, 1.0]),
            ExprControlKind::Point => ExprControlValue::Point(0.0, 0.0),
        };
        ExprControl { kind, value }
    }
}

/// A saved output module preset (Wave 14).
#[derive(Clone, Debug)]
pub struct OutputPreset {
    pub name: String,
    pub format: export::OutputFormat,
}

/// A captured group of layers extracted by pre-compose (Wave 11).
#[derive(Clone, Debug)]
pub struct SubComp {
    pub name: String,
    pub layers: Vec<crate::comp::PulseLayer>,
}

// ── Batch 2: Brainstorm ──────────────────────────────────────────────────────

/// One randomised keyframe-variation preview in the Brainstorm panel.
#[derive(Clone, Debug)]
pub struct BrainstormVariation {
    pub label: String,
    /// (layer_idx, prop, override_value)
    pub overrides: Vec<(usize, crate::comp::Prop, f32)>,
    pub selected: bool,
}

/// State for the Brainstorm panel (generate + pick random comp variations).
#[derive(Clone, Debug, Default)]
pub struct BrainstormState {
    pub open: bool,
    pub variations: Vec<BrainstormVariation>,
    /// Grid columns (default 2).
    pub grid_cols: u32,
    /// Grid rows (default 3).
    pub grid_rows: u32,
}

impl BrainstormState {
    fn new() -> Self {
        Self { open: false, variations: Vec::new(), grid_cols: 2, grid_rows: 3 }
    }
}

// ── Batch 2: Pre-render cache ─────────────────────────────────────────────────

/// Status of the pre-render cache.
#[derive(Clone, Debug, PartialEq)]
pub enum PreRenderStatus {
    NotStarted,
    Rendering { frames_done: u32, total: u32 },
    Done { frame_count: u32, cache_dir: std::path::PathBuf },
    Failed(String),
}

// ── Batch 2: Track Camera ─────────────────────────────────────────────────────

/// A single 2-D motion track point with per-frame position keyframes.
#[derive(Clone, Debug)]
pub struct TrackPoint {
    pub name: String,
    /// Position in comp space at the reference time.
    pub position: [f32; 2],
    /// `(time_secs, comp_space_position)` keyframes for this point.
    pub keyframes: Vec<(f32, [f32; 2])>,
}

/// State for the 2-point camera tracker panel.
#[derive(Clone, Debug, Default)]
pub struct CameraTracker {
    pub track_points: Vec<TrackPoint>,
    pub solved: bool,
    /// `(time_secs, [tx, ty, scale])` — the solved camera motion.
    pub camera_keyframes: Vec<(f32, [f32; 3])>,
    pub open: bool,
    pub analyze_progress: f32,
}

/// Shared cell holding the timeline track's painted bounds (window-relative).
///
/// The timeline panel paints a tiny `canvas` over its scrub track that records
/// the track's real laid-out [`Bounds`] here every frame; the track's mouse
/// listeners then read it to map a click/drag x-position back to a comp time.
/// Using a shared cell is how a panel (which only gets `&App`) can hand the root
/// view the geometry it needs without the root view re-deriving the flex layout.
pub type TrackBounds = Rc<Cell<Option<Bounds<Pixels>>>>;

/// The editing tools. A starter subset mirroring the egui app's transport/edit
/// modes; the GPUI host wires behavior per wave. (Pulse's egui app is modal —
/// select/scrub — rather than a brush palette like Pigment.)
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tool {
    /// Select / move layers in the preview.
    Select,
    /// Scrub the playhead (transport).
    Hand,
    /// Pen / mask authoring (placeholder this pass).
    Pen,
    /// Reposition a layer's anchor point (pivot for transforms).
    AnchorPoint,
    /// Puppet warp: place/move deformation pins on the selected layer.
    Puppet,
}

impl Tool {
    /// Short label for the toolbar / tools strip.
    pub fn label(self) -> &'static str {
        match self {
            Tool::Select => "Select",
            Tool::Hand => "Hand",
            Tool::Pen => "Pen",
            Tool::AnchorPoint => "Anchor",
            Tool::Puppet => "Puppet",
        }
    }

    /// Stable ordering for the tools strip.
    pub const ALL: [Tool; 5] = [Tool::Select, Tool::Hand, Tool::Pen, Tool::AnchorPoint, Tool::Puppet];
}

/// A single editable control in a Motion Graphics Template (Essential Graphics).
#[derive(Clone, Debug)]
pub enum MoGrtControl {
    /// An editable text string.
    Text { label: String, value: String },
    /// An editable RGBA color (straight sRGB, 0–1).
    Color { label: String, value: [f32; 4] },
    /// A numeric slider with a bounded range.
    Slider { label: String, value: f32, min: f32, max: f32 },
}

impl MoGrtControl {
    /// The label shown in the Essential Graphics panel.
    pub fn label(&self) -> &str {
        match self {
            MoGrtControl::Text { label, .. } => label,
            MoGrtControl::Color { label, .. } => label,
            MoGrtControl::Slider { label, .. } => label,
        }
    }
}

/// A Motion Graphics Template: a named collection of editable [`MoGrtControl`]s
/// that surface text / color / slider properties for quick reuse (After Effects'
/// Essential Graphics / `.mogrt` format).
#[derive(Clone, Debug, Default)]
pub struct MotionGraphicTemplate {
    pub name: String,
    pub controls: Vec<MoGrtControl>,
}

/// Every panel->state mutation a panel can request. Panels emit these; the root
/// view routes each into [`App::apply`]. EXTENSIBLE: later waves add variants
/// here and a matching arm in `apply` — that is the entire contract a parallel
/// agent touches when wiring a new interaction.
// Several variants are wired in `apply` but not yet emitted by a stub panel;
// they are the seams parallel agents fill in. Keep them rather than churn.
#[allow(dead_code)]
#[derive(Clone, Debug)]
pub enum Action {
    /// Select the active tool (tools strip / toolbar).
    SetTool(Tool),

    // --- Transport ---
    /// Set the playhead to an absolute time in seconds (re-renders the preview).
    SetTime(f32),
    /// Advance/rewind the playhead by `delta` seconds (re-renders the preview).
    StepTime(f32),
    /// Jump the playhead to the start of the active comp (re-renders).
    GoToStart,
    /// Jump the playhead to the end (duration) of the active comp.
    GoToEnd,
    /// Toggle the looping flag (playback loops at work-area end when on).
    ToggleLoop,
    /// Toggle play/pause. Starting play resets the wall-clock so the first tick
    /// advances by a small delta rather than the time since the last play.
    TogglePlay,
    /// Stop playback (without moving the playhead).
    Pause,

    // --- Layers ---
    /// Toggle a layer's visibility by index in the active comp (re-renders).
    ToggleLayerVisible(usize),
    /// Make a layer (by index) the selected layer (pure UI state).
    SelectLayer(usize),

    // --- Properties ---
    /// Set the **selected** layer's transform property `prop` to `value` at the
    /// current playhead time (re-renders). Matches the egui properties panel:
    /// `track.set_key(time, value)` overwrites the key at this instant when one
    /// exists, lays down a single constant key when the track is empty, or keys
    /// the instant when the track is animated — so a direct edit at the playhead
    /// stays consistent with any existing animation. A no-op if no layer is
    /// selected.
    SetTransform(Prop, f32),
    /// Toggle animation for the selected layer's transform property `prop` (the
    /// per-property **stopwatch**, like After Effects). When the track is empty,
    /// lay down a single key at the current playhead (start animating — further
    /// value edits then add keys); when the track already has keys, clear them
    /// all (stop animating, collapsing back to a static value). Re-renders. A
    /// no-op if no layer is selected.
    ToggleKeyframe(Prop),
    /// Move the keyframe at index `key_index` of the selected layer's `prop`
    /// track to a new absolute time (seconds), re-sorting if it crosses a
    /// neighbour (the dragged key keeps its value; only its time changes — a
    /// horizontal lane drag). Re-renders. A no-op if no layer is selected or the
    /// index is out of range.
    MoveKeyframe {
        prop: Prop,
        key_index: usize,
        time: f32,
    },

    // --- Effects ---
    /// Toggle the Effects & Presets browser open/closed (pure UI state).
    ToggleEffectBrowser,
    /// Set the effect browser's search query (pure UI state; the panel re-filters
    /// the registry via `effect_browser::filter_grouped`).
    SetEffectQuery(String),
    /// Add the effect described by `entry` to the **selected** layer's matching
    /// stack, mirroring the egui app's `add_browser_effect`: a colour effect →
    /// `effects`, a spatial → `spatial_effects`, distort → `distort_effects`,
    /// stylize → `stylize_effects`, keying → `key_effects`, and a generate fill
    /// (overwriting) the single `generate` slot. Re-renders. No-op with no
    /// selection. Undoable.
    AddEffect(BrowserEntry),
    /// Remove the effect at `index` from `stack` of the selected layer (for the
    /// `Generate` stack the index is ignored — it clears the single slot).
    /// Re-renders. Undoable.
    RemoveEffect { stack: EffectStack, index: usize },
    /// Set the scalar parameter `param` of the effect at `index` in `stack` of the
    /// selected layer to `value` (clamped to the param's range by the
    /// `effect_params` accessor). Re-renders. Undoable.
    SetEffectParam {
        stack: EffectStack,
        index: usize,
        param: usize,
        value: f32,
    },

    // --- Work area (loop / render region) ---
    /// Set the active comp's work-area **start** (in-point) to an absolute time
    /// in seconds, clamped to `[0, end]`. Mirrors the egui app's
    /// `set_work_area_start` (the `B` key / draggable in marker). Bounds playback
    /// looping and the default export range. Undoable.
    SetWorkAreaStart(f32),
    /// Set the active comp's work-area **end** (out-point) to an absolute time in
    /// seconds, clamped to `[start, duration]`. Mirrors the egui app's
    /// `set_work_area_end` (the `N` key / draggable out marker). Undoable.
    SetWorkAreaEnd(f32),
    /// Reset the active comp's work area to span the whole `[0, duration]`
    /// timeline (the egui app's "reset work area"). Undoable.
    ResetWorkArea,

    // --- Graph editor ---
    /// Toggle the graph (value-curve) editor on the timeline. Pure UI state.
    ToggleGraph,
    /// Toggle a property's visibility in the graph plot (empty set = "all keyed").
    /// Pure UI state (just changes which curves are drawn).
    ToggleGraphProp(Prop),
    /// Clear the graph's explicit property selection (show all keyed props). Pure
    /// UI state.
    ClearGraphProps,
    /// Set the interpolation mode of key `key_index` on the selected layer's
    /// `prop` track to `interp` (the graph editor's ease-handle drag promotes a
    /// segment to an editable [`Interp::Ease`] and keeps reshaping it). Re-renders.
    /// Undoable. No-op if no layer is selected or the index is out of range.
    SetInterp {
        prop: Prop,
        key_index: usize,
        interp: Interp,
    },
    /// Move keyframe `key_index` of the selected layer's `prop` track to a new
    /// absolute time **and** value (the graph editor drags keys in both axes,
    /// unlike the timeline lane which only retimes). Re-sorts if it crosses a
    /// neighbour. Re-renders. Undoable. No-op if no layer is selected or the
    /// index is out of range.
    MoveKeyframeXY {
        prop: Prop,
        key_index: usize,
        time: f32,
        value: f32,
    },

    // --- 3D layer controls ---
    /// Enable or disable the 3-D layer flag on the layer at `layer_id` (index).
    /// Undoable. Re-renders (the camera projection is applied or removed).
    SetLayer3D(usize, bool),
    /// Set the Z position of the layer at `layer_id`. Undoable. Re-renders.
    SetPositionZ(usize, f32),
    /// Set the X/Y/Z orientation (Euler degrees) of the layer at `layer_id`.
    /// Undoable. Re-renders.
    Set3DRotation(usize, f32, f32, f32),

    // --- Gizmo hover / modifier state ---
    /// Record which gizmo handle the pointer is currently hovering over (or
    /// `None` when the pointer left). Pure UI state — no undo, no re-render.
    SetHoveredGizmoHandle(Option<GizmoHandle>),
    /// Duplicate the selected layer then immediately begin a gizmo drag (the
    /// Alt+drag move gesture). Undoable.
    DuplicateLayer(usize),

    // --- Expressions ---
    /// Set (or clear when `expr` is empty) the expression string for the property
    /// `prop` on the layer at `layer_id`. Undoable.
    SetExpression { layer_id: usize, prop: String, expr: String },

    // --- Preview transform gizmo ---
    /// Key the changed transform properties of the selected layer at `time`
    /// (seconds) from a gizmo drag — `set_key` for each `(prop, value)` exactly
    /// like the properties panel, so a drag writes keyframes on an animated track
    /// and lays a constant key on a static one. Re-renders. Undoable. No-op if no
    /// layer is selected.
    GizmoKeys { time: f32, keys: Vec<(Prop, f32)> },

    // --- History ---
    /// Undo the last undoable edit (restore the previous project snapshot).
    Undo,
    /// Redo the last undone edit.
    Redo,

    // --- Wave 9: MP4 export ---
    /// Export the active comp to an MP4 at `path` (via prism-media / ffmpeg).
    ExportMp4(PathBuf),
    /// Export the active comp to an animated GIF at `path`.
    ExportGif(PathBuf),

    // --- Wave 9: Audio preview stub ---
    /// Toggle the audio preview stub on/off.
    ToggleAudioPreview,
    /// Set the audio preview volume (0.0–1.5).
    SetAudioVolume(f32),

    // --- Wave 9: GPUI-side effects ---
    /// Add a GPUI-side post-process effect of the given kind to the selected layer.
    AddGpuiEffect(GpuiEffectKind),
    /// Remove the GPUI-side effect at index `i` from the selected layer.
    RemoveGpuiEffect(usize),
    /// Update the block size of a Mosaic effect at index `i` on the selected layer.
    SetMosaicBlock { effect_idx: usize, block: u32 },
    /// Update the offset of a ChromaticAberration effect at index `i`.
    SetChromaOffset { effect_idx: usize, offset: i32 },
    /// Update the intensity of a Vignette or Noise effect at index `i`.
    SetEffectIntensity { effect_idx: usize, intensity: f32 },
    /// Update the radius of a Vignette effect at index `i`.
    SetVignetteRadius { effect_idx: usize, radius: f32 },
    /// Toggle the expand/collapse state of a GPUI-side effect card at `effect_idx`.
    ToggleGpuiEffectExpand(usize),

    // --- Wave 9: Render queue ---
    /// Add the active comp (with a file-picker output path) to the render queue.
    AddToRenderQueue,
    /// Process all pending render queue jobs serially.
    RenderAll,
    /// Remove the job at index `i` from the render queue.
    RemoveFromRenderQueue(usize),

    // --- Wave 9: Composition settings dialog ---
    /// Open or close the composition settings dialog.
    ToggleCompSettings,
    /// Set a pending comp-setting field (does NOT apply until ApplyCompSettings).
    SetPendingCompWidth(u32),
    SetPendingCompHeight(u32),
    SetPendingCompFps(f32),
    SetPendingCompDuration(f32),
    SetPendingCompBgColor([f32; 4]),
    /// Apply the pending comp settings to the active comp and resize host buffers.
    ApplyCompSettings,

    // --- Wave 11: RAM preview ---
    BuildRamPreview,
    PlayRamPreview,
    PurgeRamPreview,
    /// Clear the host-side RAM preview frame cache (BTreeMap in CanvasHost).
    ClearRamPreview,

    // --- Wave 11: layer parenting ---
    /// Set layer `child` to have `parent` as its GPUI-side parent. Undoable.
    SetParent(usize, usize),
    /// Clear the GPUI-side parent of layer `child`. Undoable.
    ClearParent(usize),
    /// Arm or disarm the pick-whip: `Some(i)` means layer i is waiting to pick.
    SetPickingParent(Option<usize>),

    // --- Wave 11: pre-compose ---
    /// Move `indices` layers into a new SubComp named `name`, leaving a Null placeholder.
    PreCompose(Vec<usize>, String),
    /// Enter the sub-comp at index `idx` in the layers panel.
    OpenSubComp(usize),
    /// Return to the main comp from a sub-comp view.
    CloseSubComp,

    // --- Wave 11: null object ---
    /// Add a new Null layer to the active comp. Undoable.
    AddNullLayer,
    /// Add a Guide layer to the active comp (skipped during compositing). Undoable.
    AddGuideLayer,

    // --- Wave 11: solid + adjustment layers ---
    /// Add a Solid layer with the given RGBA8 color to the active comp. Undoable.
    AddSolidLayer([u8; 4]),
    /// Add an Adjustment layer to the active comp (renders its effect stack onto lower layers). Undoable.
    AddAdjustmentLayer,

    // --- Wave 12: video footage import ---
    /// Open a file dialog to pick a video clip; probe it via prism-media and add
    /// it as a Footage layer in the active comp. Gracefully falls back when
    /// ffprobe is not installed (logs a warning, no panic).
    ImportVideoFootage,
    /// Set the footage source for the selected layer to a video file at `path`,
    /// using a pre-probed `fps` and `frame_count`. Used internally by the
    /// ImportVideoFootage handler.
    SetFootageVideo {
        layer_idx: usize,
        path: std::path::PathBuf,
        fps: f64,
        frame_count: u64,
        width: u32,
        height: u32,
    },

    // --- Wave 11: new GPUI effect param setters ---
    SetColorBalanceShadows { effect_idx: usize, channel: usize, value: f32 },
    SetColorBalanceMidtones { effect_idx: usize, channel: usize, value: f32 },
    SetColorBalanceHighlights { effect_idx: usize, channel: usize, value: f32 },
    SetLevelsInBlack { effect_idx: usize, value: u8 },
    SetLevelsInWhite { effect_idx: usize, value: u8 },
    SetLevelsGamma { effect_idx: usize, value: f32 },
    SetLevelsOutBlack { effect_idx: usize, value: u8 },
    SetLevelsOutWhite { effect_idx: usize, value: u8 },
    SetHueShift { effect_idx: usize, value: f32 },
    SetSaturation { effect_idx: usize, value: f32 },
    SetLightness { effect_idx: usize, value: f32 },
    SetNoiseFrequency { effect_idx: usize, value: f32 },
    SetNoiseEvolution { effect_idx: usize, value: f32 },

    // --- Wave 13: Comp + layer markers ---
    /// Add a marker to the active composition at `time` seconds.
    AddCompMarker { time: f32, label: String },
    /// Remove comp marker by index.
    RemoveCompMarker(usize),
    /// Add a marker to a specific layer at `time` seconds.
    AddLayerMarker { layer_idx: usize, time: f32, label: String },
    /// Remove a layer marker by layer + marker index.
    RemoveLayerMarker { layer_idx: usize, idx: usize },

    // --- Wave 13: Region of interest ---
    /// Set the region of interest for the preview render (doc px `[x,y,w,h]`).
    /// `None` clears the ROI and renders the full composition.
    SetROI(Option<[f32; 4]>),

    // --- Wave 14: Anchor point, displacement map, expression controls, ProRes, presets ---

    /// Set anchor point for the selected layer (normalized 0..1 per axis).
    SetAnchorPoint { layer_idx: usize, x: f32, y: f32 },

    /// Add a Displacement Map distort effect to the selected layer.
    AddDisplacementMap { layer_idx: usize, map_layer: usize, scale_x: f32, scale_y: f32 },

    /// Set displacement map scale.
    SetDisplaceScale { layer_idx: usize, effect_idx: usize, scale_x: f32, scale_y: f32 },

    /// Add an expression control layer of the given kind.
    AddExpressionControl(ExprControlKind),

    /// Set the value of an expression control layer.
    SetExprControlValue { layer_idx: usize, value: ExprControlValue },

    /// Export current comp as ProRes via FFmpeg (falls back to error message if absent).
    ExportProRes(PathBuf),

    /// Export current comp as DNxHD via FFmpeg.
    ExportDnxHD(PathBuf),

    /// Save current render settings as a named output preset.
    SaveOutputPreset(String),

    /// Load (apply) a saved output preset by index.
    LoadOutputPreset(usize),

    /// Delete a saved output preset by index.
    DeleteOutputPreset(usize),

    // --- Batch 1: 3D Camera stub ---
    /// Set the active comp's camera position (comp-space XYZ). Undoable.
    SetCameraPosition([f32; 3]),
    /// Set the active comp's camera field of view in degrees. Undoable.
    SetCameraFov(f32),

    // --- Batch 1: Layer parenting (extended) ---
    /// Set the selected layer's parent to the layer at `parent_idx`. Undoable.
    SetParentLayer { child: usize, parent: Option<usize> },

    // --- Batch 3: Time remap ---
    SetTimeRemapEnabled { layer_id: usize, enabled: bool },
    SetTimeRemapKey { layer_id: usize, comp_time: f64, source_time: f64 },

    // --- Batch 3: Track matte ---
    SetLayerMatte { layer_id: usize, mode: crate::comp::MatteMode },

    // --- Batch 3: Puppet pins ---
    AddPuppetPin { layer_id: usize, pos: [f32; 2] },
    MovePuppetPin { layer_id: usize, pin_id: u64, pos: [f32; 2] },
    RemovePuppetPin { layer_id: usize, pin_id: u64 },

    // --- Batch 3: Solo / Shy ---
    ToggleSolo(usize),
    ToggleShy(usize),
    ToggleHideShy,

    // --- Batch 4: 3D Lights ---
    /// Add a light to the active composition.
    AddLight(crate::comp::Light),
    /// Remove the light at index `i` from the active composition.
    RemoveLight(usize),
    /// Replace the light at `index` with `light` in the active composition.
    UpdateLight { index: usize, light: crate::comp::Light },

    // --- Batch 4: Multi-comp render queue ---
    /// Add every comp in the project as a render queue job (sequenced render).
    AddAllCompsToQueue,

    // --- Batch 4: Live preview output ---
    /// Toggle writing each preview frame to /tmp/prism-pulse-preview.rgba.
    ToggleLiveOutput,

    // --- Batch 5: Comp motion blur (shutter) ---
    /// Toggle the active comp's master motion-blur switch. Undoable.
    SetMotionBlurEnabled(bool),
    /// Set the comp's shutter angle in degrees (clamped to `(0, 720]`). Undoable.
    SetMotionBlurAngle(f32),
    /// Set the comp's shutter phase in degrees. Undoable.
    SetMotionBlurPhase(f32),
    /// Set the comp's motion-blur sample count (clamped to `[1, 64]`). Undoable.
    SetMotionBlurSamples(u32),
    /// Toggle the per-layer motion-blur flag on the layer at `idx`. Undoable.
    ToggleLayerMotionBlur(usize),

    // --- Batch 6: Text Animator ---
    /// Add a default TextAnimator to the text layer at `layer_id`. Undoable.
    AddTextAnimator(usize),
    /// Remove the TextAnimator from the text layer at `layer_id`. Undoable.
    RemoveTextAnimator(usize),
    /// Set the text animator's character range (both clamped to [0,1]). Undoable.
    SetTextAnimatorRange { layer_id: usize, start: f32, end: f32 },
    /// Set the per-character x offset of the text animator. Undoable.
    SetTextAnimatorOffsetX { layer_id: usize, value: f32 },
    /// Set the per-character y offset of the text animator. Undoable.
    SetTextAnimatorOffsetY { layer_id: usize, value: f32 },
    /// Set the per-character rotation (degrees) of the text animator. Undoable.
    SetTextAnimatorRotation { layer_id: usize, value: f32 },
    /// Set the per-character scale multiplier of the text animator. Undoable.
    SetTextAnimatorScale { layer_id: usize, value: f32 },
    /// Set the per-character opacity multiplier of the text animator. Undoable.
    SetTextAnimatorOpacity { layer_id: usize, value: f32 },

    // --- Batch 6: Shape Layer — Trim Paths ---
    /// Set (or replace) Trim Paths on the shape layer at `layer_id`. Undoable.
    SetShapeTrimPaths { layer_id: usize, start: f32, end: f32, offset: f32 },
    /// Clear Trim Paths from the shape layer at `layer_id`. Undoable.
    ClearShapeTrimPaths(usize),

    // --- Batch 6: Shape Layer — Repeater ---
    /// Add a default ShapeRepeater to the shape layer at `layer_id`. Undoable.
    AddShapeRepeater(usize),
    /// Remove the ShapeRepeater from the shape layer at `layer_id`. Undoable.
    RemoveShapeRepeater(usize),
    /// Set the number of copies on the shape layer's repeater. Undoable.
    SetRepeaterCopies { layer_id: usize, copies: u32 },
    /// Set the per-copy x/y offset on the shape layer's repeater. Undoable.
    SetRepeaterOffset { layer_id: usize, x: f32, y: f32 },
    /// Set the per-copy rotation (degrees) on the shape layer's repeater. Undoable.
    SetRepeaterRotation { layer_id: usize, deg: f32 },
    /// Set the per-copy scale multiplier on the shape layer's repeater. Undoable.
    SetRepeaterScale { layer_id: usize, scale: f32 },
    /// Set the first/last-copy opacity on the shape layer's repeater. Undoable.
    SetRepeaterOpacity { layer_id: usize, start: f32, end: f32 },

    // --- Batch 6: Lumetri Color ---
    /// Add (or reset) the Lumetri Color grade on the layer at `layer_id`. Undoable.
    AddLumetriColor(usize),
    /// Remove the Lumetri Color grade from the layer at `layer_id`. Undoable.
    RemoveLumetriColor(usize),
    /// Set a single Lumetri Color parameter by name. Undoable.
    /// Recognised params: "exposure" "contrast" "highlights" "shadows" "whites"
    /// "blacks" "temperature" "tint" "saturation" "vibrance".
    SetLumetriParam { layer_id: usize, param: &'static str, value: f32 },
    /// Toggle the Lumetri Color bypass flag on the layer at `layer_id`. Undoable.
    ToggleLumetriEnabled(usize),
    /// Reset all Lumetri Color parameters to their defaults. Undoable.
    ResetLumetriColor(usize),

    // --- Batch 6: Essential Graphics / Motion Graphics Templates ---
    /// Open or close the Essential Graphics panel.
    ToggleMoGrtPanel,
    /// Add a new Motion Graphics Template with the given name.
    AddMoGrtTemplate(String),
    /// Remove the template at `idx` from the MoGrt list.
    RemoveMoGrtTemplate(usize),
    /// Select the template at `idx` in the Essential Graphics panel.
    SelectMoGrtTemplate(usize),
    /// Add a control to the template at `template_idx`.
    AddMoGrtControl { template_idx: usize, control: MoGrtControl },
    /// Remove the control at `control_idx` from template `template_idx`.
    RemoveMoGrtControl { template_idx: usize, control_idx: usize },
    /// Update the text value of a Text control in a template.
    SetMoGrtTextValue { template_idx: usize, control_idx: usize, value: String },
    /// Update the color value of a Color control in a template.
    SetMoGrtColorValue { template_idx: usize, control_idx: usize, value: [f32; 4] },
    /// Update the slider value of a Slider control in a template.
    SetMoGrtSliderValue { template_idx: usize, control_idx: usize, value: f32 },
    /// Export the template at `template_idx` to a JSON file at `path`.
    ExportMoGrt { template_idx: usize, path: PathBuf },

    // --- Batch 2: Layer split ---
    /// Split the layer at `id` at the current playhead time into two layers.
    /// The original spans [in_point..playhead]; the new copy spans [playhead..out_point].
    /// Undoable.
    SplitLayer(usize),
    /// Split the layer at `layer_id` at an explicit `time`. Undoable.
    SplitLayerAt { layer_id: usize, time: f32 },

    // --- Batch 2: Brainstorm ---
    /// Toggle the Brainstorm variations panel open/closed.
    ToggleBrainstorm,
    /// Generate `count` random keyframe-variation previews from the current comp state.
    GenerateBrainstormVariations { count: u32 },
    /// Mark variation `idx` as the selected preview.
    SelectBrainstormVariation(usize),
    /// Apply variation `idx`: bake its overrides into the comp, then close Brainstorm.
    ApplyBrainstormVariation(usize),
    /// Resize the Brainstorm grid.
    SetBrainstormGrid { cols: u32, rows: u32 },

    // --- Batch 2: Color Finesse ---
    /// Add a Color Finesse grade to the layer at `layer_id`. Undoable.
    AddColorFinesse(usize),
    /// Remove the Color Finesse grade from the layer at `layer_id`. Undoable.
    RemoveColorFinesse(usize),
    /// Enable or disable the Color Finesse grade on the layer at `layer_id`. Undoable.
    SetColorFinesseEnabled { layer_id: usize, enabled: bool },
    /// Set one parameter of the Color Finesse grade.
    /// `range`: "master" | "reds" | "yellows" | "greens" | "cyans" | "blues" | "magentas"
    /// `prop`:  "hue" | "saturation" | "lightness"
    SetColorFinesseParam { layer_id: usize, range: &'static str, prop: &'static str, value: f32 },
    /// Reset all Color Finesse parameters to defaults on the layer at `layer_id`. Undoable.
    ResetColorFinesse(usize),

    // --- Batch 2: Pre-render cache ---
    /// Begin rendering the work area to a PNG frame sequence in a temp dir.
    StartPreRender,
    /// Cancel an in-progress pre-render.
    CancelPreRender,
    /// Update the pre-render progress counters.
    SetPreRenderProgress { frames_done: u32, total: u32 },
    /// Mark pre-render complete.
    PreRenderComplete { frame_count: u32, cache_dir: std::path::PathBuf },
    /// Record a pre-render failure.
    PreRenderFailed(String),
    /// Clear the pre-render cache and reset status.
    ClearPreRenderCache,
    /// Toggle whether the compositor serves frames from the pre-render cache.
    ToggleUsePreRender,

    // --- Batch 2: Track Camera ---
    /// Toggle the Camera Tracker panel open/closed.
    ToggleCameraTracker,
    /// Add a track point at the given position (comp space) to the tracker.
    AddTrackPoint { name: String, pos: [f32; 2] },
    /// Remove the track point at `idx`.
    RemoveTrackPoint(usize),
    /// Record the position of track point `idx` at `time`.
    MoveTrackPoint { idx: usize, time: f32, pos: [f32; 2] },
    /// Solve the camera track from the current set of track points + their keyframes.
    SolveCameraTrack,
    /// Apply the solved camera motion to the comp's camera layer.
    CreateCameraFromTrack,
    /// Update the tracker's progress bar.
    SetCameraTrackerProgress(f32),
    /// Clear all track points and the solved camera keyframes.
    ClearCameraTrack,
}

impl Action {
    /// Whether applying this action mutates the [`Project`] document (so the
    /// undo stack should snapshot the pre-state before it runs). Pure transport /
    /// selection / UI-toggle actions — and the history actions themselves — are
    /// NOT snapshotted (they don't change the document, or they manage the stack
    /// directly).
    fn is_undoable(&self) -> bool {
        matches!(
            self,
            Action::ToggleLayerVisible(_)
                | Action::SetTransform(_, _)
                | Action::ToggleKeyframe(_)
                | Action::MoveKeyframe { .. }
                | Action::AddEffect(_)
                | Action::RemoveEffect { .. }
                | Action::SetEffectParam { .. }
                | Action::SetWorkAreaStart(_)
                | Action::SetWorkAreaEnd(_)
                | Action::ResetWorkArea
                | Action::SetInterp { .. }
                | Action::MoveKeyframeXY { .. }
                | Action::GizmoKeys { .. }
                | Action::SetLayer3D(_, _)
                | Action::SetPositionZ(_, _)
                | Action::Set3DRotation(_, _, _, _)
                | Action::DuplicateLayer(_)
                | Action::SetExpression { .. }
                | Action::AddGpuiEffect(_)
                | Action::RemoveGpuiEffect(_)
                | Action::SetMosaicBlock { .. }
                | Action::SetChromaOffset { .. }
                | Action::SetEffectIntensity { .. }
                | Action::SetVignetteRadius { .. }
                | Action::ApplyCompSettings
                | Action::SetParent(_, _)
                | Action::ClearParent(_)
                | Action::PreCompose(_, _)
                | Action::AddNullLayer
                | Action::AddGuideLayer
                | Action::AddSolidLayer(_)
                | Action::AddAdjustmentLayer
                | Action::SetFootageVideo { .. }
                | Action::SetColorBalanceShadows { .. }
                | Action::SetColorBalanceMidtones { .. }
                | Action::SetColorBalanceHighlights { .. }
                | Action::SetLevelsInBlack { .. }
                | Action::SetLevelsInWhite { .. }
                | Action::SetLevelsGamma { .. }
                | Action::SetLevelsOutBlack { .. }
                | Action::SetLevelsOutWhite { .. }
                | Action::SetHueShift { .. }
                | Action::SetSaturation { .. }
                | Action::SetLightness { .. }
                | Action::SetNoiseFrequency { .. }
                | Action::SetNoiseEvolution { .. }
                | Action::AddCompMarker { .. }
                | Action::RemoveCompMarker(_)
                | Action::AddLayerMarker { .. }
                | Action::RemoveLayerMarker { .. }
                | Action::SetCameraPosition(_)
                | Action::SetCameraFov(_)
                | Action::SetParentLayer { .. }
                | Action::SetTimeRemapEnabled { .. }
                | Action::SetTimeRemapKey { .. }
                | Action::SetLayerMatte { .. }
                | Action::AddPuppetPin { .. }
                | Action::MovePuppetPin { .. }
                | Action::RemovePuppetPin { .. }
                | Action::ToggleSolo(_)
                | Action::AddLight(_)
                | Action::RemoveLight(_)
                | Action::UpdateLight { .. }
                | Action::SetMotionBlurEnabled(_)
                | Action::SetMotionBlurAngle(_)
                | Action::SetMotionBlurPhase(_)
                | Action::SetMotionBlurSamples(_)
                | Action::ToggleLayerMotionBlur(_)
                | Action::AddTextAnimator(_)
                | Action::RemoveTextAnimator(_)
                | Action::SetTextAnimatorRange { .. }
                | Action::SetTextAnimatorOffsetX { .. }
                | Action::SetTextAnimatorOffsetY { .. }
                | Action::SetTextAnimatorRotation { .. }
                | Action::SetTextAnimatorScale { .. }
                | Action::SetTextAnimatorOpacity { .. }
                | Action::SetShapeTrimPaths { .. }
                | Action::ClearShapeTrimPaths(_)
                | Action::AddShapeRepeater(_)
                | Action::RemoveShapeRepeater(_)
                | Action::SetRepeaterCopies { .. }
                | Action::SetRepeaterOffset { .. }
                | Action::SetRepeaterRotation { .. }
                | Action::SetRepeaterScale { .. }
                | Action::SetRepeaterOpacity { .. }
                | Action::AddLumetriColor(_)
                | Action::RemoveLumetriColor(_)
                | Action::SetLumetriParam { .. }
                | Action::ToggleLumetriEnabled(_)
                | Action::ResetLumetriColor(_)
                | Action::SplitLayer(_)
                | Action::SplitLayerAt { .. }
                | Action::AddColorFinesse(_)
                | Action::RemoveColorFinesse(_)
                | Action::SetColorFinesseEnabled { .. }
                | Action::SetColorFinesseParam { .. }
                | Action::ResetColorFinesse(_)
        )
    }
}

/// The single shared application state. Owns the project + host and all panel-
/// facing tool/transport/view state. Mutated ONLY through [`App::apply`].
pub struct App {
    /// CPU compositor bridge (Pulse's software renderer -> GPUI `RenderImage`).
    pub host: CanvasHost,
    /// The Pulse document; panels read it, `apply` mutates it.
    pub project: Project,
    /// The playhead position in seconds (drives which frame the host renders).
    pub time: f32,
    /// The active editing tool.
    pub active: Tool,
    /// The selected layer index within the active comp (panel highlight).
    pub selected_layer: Option<usize>,
    /// Whether the transport is playing (advances `time` each animation frame).
    pub playing: bool,
    /// Whether playback loops at the work-area end rather than stopping.
    pub loop_enabled: bool,
    /// True while the user is dragging the scrub track (mouse held down on it),
    /// so `on_mouse_move` only re-times the playhead during an active drag.
    pub scrubbing: bool,
    /// Wall-clock anchor for the play loop. `None` when paused; set on play so
    /// the next tick advances by the real elapsed time (frame-rate independent).
    last_tick: Option<Instant>,
    /// The timeline scrub track's painted bounds, shared with the panel so the
    /// root view can map a pointer x back to a comp time. See [`TrackBounds`].
    pub track_bounds: TrackBounds,
    /// The keyframe currently being dragged on a timeline lane, if any:
    /// `(layer_index, prop, key_index)`. Set on mouse-down over a diamond and
    /// cleared on release; while set, lane mouse-move re-times that key. The
    /// `key_index` is kept live (updated when a drag re-sorts the key past a
    /// neighbour) so the drag keeps tracking the same keyframe.
    pub kf_drag: Option<KeyframeDrag>,
    /// Which work-area handle (in/out marker) is currently being dragged on the
    /// timeline ruler, if any. Set on mouse-down over a handle and cleared on
    /// release; while set, ruler mouse-move re-times that marker (and suppresses
    /// playhead scrubbing). See [`WorkAreaHandle`].
    pub wa_drag: Option<WorkAreaHandle>,
    /// Whether the Effects & Presets browser is open in the effects panel.
    pub effect_browser_open: bool,
    /// The effect browser's search query (filters the effect registry).
    pub effect_query: String,
    /// Undo stack: project snapshots taken **before** each undoable edit. The
    /// newest pre-state is on top; [`Action::Undo`] pops it back into `project`
    /// (pushing the current state onto `redo`).
    undo: Vec<Project>,
    /// Redo stack: project snapshots popped off `undo` by an undo, restorable by
    /// [`Action::Redo`]. Cleared whenever a fresh edit lands (the standard linear
    /// history model).
    redo: Vec<Project>,
    /// The in-flight (or just-finished) export, if any. The File menu starts one
    /// via [`start_export`](Self::start_export); the toolbar polls its
    /// [`ExportProgress`] each frame to draw a progress readout, and clears it
    /// once the worker reports finished and the user dismisses it.
    pub export: Option<ExportProgress>,
    /// Whether the graph (value-curve) editor is shown in place of the timeline
    /// lanes. Toggled from the timeline header (the egui app's Timeline/Graph
    /// editor-mode switch).
    pub graph_open: bool,
    /// Which transform properties the graph plots. Empty = every property with at
    /// least one keyframe (mirrors `crate::graph::GraphState::shown`).
    pub graph_shown: Vec<Prop>,
    /// The graph element currently being dragged (a keyframe body or an ease
    /// handle), if any. Set on mouse-down over a point and cleared on release;
    /// while set, the graph's mouse-move reshapes that element. See [`GraphGrab`].
    pub graph_grab: Option<GraphGrab>,
    /// The in-flight preview transform-gizmo drag, if any (which handle is held,
    /// the layer + grab-time transform/parent, and the grab-time pointer in comp
    /// space). The drag math is recomputed each frame against the live pointer so
    /// the result is always relative to the grab. See [`GizmoDrag`].
    pub gizmo_drag: Option<GizmoDrag>,
    /// The preview's laid-out image bounds (window-relative), painted by a canvas
    /// in the root view each frame so the gizmo can map a pointer position to comp
    /// space using the exact same fit the image is drawn with. See [`PreviewRect`].
    pub preview_rect: PreviewRect,
    /// The graph editor plot's painted bounds (window-relative), recorded by the
    /// graph canvas each frame so the plot's mouse listeners can hit-test
    /// keyframes/handles in the same coordinate frame the painter used. Same
    /// shared-cell pattern as [`TrackBounds`] / [`PreviewRect`].
    pub graph_rect: TrackBounds,
    /// The gizmo handle the pointer is currently hovering over, if any. Set by
    /// [`Action::SetHoveredGizmoHandle`]; the preview panel highlights it yellow.
    /// Pure UI state — no undo entry, no host dirty.
    pub hovered_gizmo_handle: Option<GizmoHandle>,
    /// Per-layer, per-property expression strings, keyed by `(layer_index, prop_name)`.
    /// Set by [`Action::SetExpression`]; the expression editor panel shows and edits them.
    /// An empty string means "no expression" (the entry may be absent).
    pub expressions: HashMap<(usize, String), String>,
    /// Which effects cards are expanded in the effects panel. Keyed by
    /// `(EffectStack as usize, effect_index)`. Pure UI state.
    pub effects_expanded: HashMap<(usize, usize), bool>,

    // --- Wave 9 fields ---
    /// MP4 export progress fraction (0.0–1.0), `None` when not exporting.
    pub export_mp4_progress: Option<f32>,
    /// GIF export progress fraction (0.0–1.0), `None` when not exporting.
    pub export_gif_progress: Option<f32>,
    /// Whether the audio preview stub is enabled.
    pub audio_preview_enabled: bool,
    /// Audio preview volume (0.0–1.5).
    pub audio_volume: f32,
    /// Per-layer GPUI-side post-process effects. Keyed by layer index.
    pub gpui_effects: HashMap<usize, Vec<GpuiEffect>>,
    /// Which GPUI-effect cards are expanded. Keyed by `(layer_index, effect_index)`.
    pub gpui_effects_expanded: HashMap<(usize, usize), bool>,
    /// Pending export jobs.
    pub render_queue: Vec<RenderJob>,
    /// Whether the composition settings dialog is open.
    pub comp_settings_open: bool,
    /// Pending (not-yet-applied) composition settings.
    pub pending_comp_settings: Option<PendingCompSettings>,

    // --- Wave 11 fields ---
    /// RAM preview cache: frame_idx → BGRA8 bytes.
    pub ram_preview: HashMap<u32, Vec<u8>>,
    pub ram_preview_playing: bool,
    pub ram_preview_frame: u32,
    pub ram_preview_complete: bool,
    /// Layer parenting: child_idx → parent_idx (GPUI-side, not in pulse-app model).
    pub layer_parents: HashMap<usize, usize>,
    /// Which layer index is currently waiting for the user to pick-whip a parent.
    pub picking_parent_for: Option<usize>,
    /// Sub-comps created by pre-compose.
    pub sub_comps: Vec<SubComp>,
    /// Maps layer_idx → sub_comp_idx for pre-comp placeholder layers.
    pub pre_comp_layers: HashMap<usize, usize>,
    /// If Some(sci), the layers panel shows sub_comps[sci] instead of main comp.
    pub active_sub_comp: Option<usize>,
    /// Region of interest for preview render (doc px `[x,y,w,h]`). `None` = full comp.
    pub roi: Option<[f32; 4]>,
    /// Last rendered frame index (comp fps * time). Used to skip dirty mark when
    /// the playhead hasn't advanced to a new frame yet (reduces UI-thread CPU load).
    pub last_rendered_frame: u32,

    // --- Wave 14 fields ---
    /// Expression control state per layer index (only set for ExpressionControl layers).
    pub expr_controls: HashMap<usize, ExprControl>,
    /// Saved output presets.
    pub output_presets: Vec<OutputPreset>,
    /// Progress label for ProRes/DNxHD export (None when idle).
    pub prores_progress: Option<String>,
    /// Pending export format for output presets.
    pub pending_export_format: Option<export::OutputFormat>,

    // --- Batch 4 fields ---
    /// When true, each rendered preview frame is written to /tmp/prism-pulse-preview.rgba.
    pub live_output_enabled: bool,

    // --- Batch 6 fields ---
    /// Motion Graphics Templates (Essential Graphics panel).
    pub mogrt_templates: Vec<MotionGraphicTemplate>,
    /// Whether the Essential Graphics panel is open.
    pub mogrt_panel_open: bool,
    /// Currently selected template index in the Essential Graphics panel.
    pub mogrt_selected: Option<usize>,

    // --- Batch 2 fields ---
    /// Brainstorm variations panel state.
    pub brainstorm: BrainstormState,
    /// Status of the pre-render cache.
    pub pre_render_status: PreRenderStatus,
    /// Directory where pre-rendered frames are cached (`None` when not yet started).
    pub pre_render_cache_dir: Option<std::path::PathBuf>,
    /// When true, the compositor serves frames from the pre-render cache.
    pub use_pre_render: bool,
    /// 2-point camera tracker state.
    pub camera_tracker: CameraTracker,
}

/// Shared cell holding the preview image's painted bounds (window-relative), so
/// the root view's preview mouse listeners can map a pointer position into comp
/// space for the transform gizmo. Mirrors [`TrackBounds`] for the timeline.
pub type PreviewRect = Rc<Cell<Option<Bounds<Pixels>>>>;

/// Which graph element a drag is reshaping (see [`App::graph_grab`]). Mirrors the
/// egui graph editor's private `Grab`, but kept here so the GPUI panel can arm it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum GraphGrab {
    /// A keyframe body: the drag retimes (x) and revalues (y) it.
    Key { prop: Prop, key_index: usize },
    /// A Bézier ease handle on the segment leaving key `key_index` of `prop`.
    Handle {
        prop: Prop,
        key_index: usize,
        which: GizmoHandle2,
    },
}

/// Which ease handle of a graph segment a drag targets. (A thin alias over the
/// engine's [`crate::comp::Handle`], re-named to avoid clashing with the
/// gizmo's `Handle`.)
pub use crate::comp::Handle as GizmoHandle2;

/// An in-progress preview transform-gizmo drag: the held handle, the layer +
/// grab-time transform/parent for the local-space delta math, and the pointer's
/// comp-space position at grab time. Recomputed each frame against the live
/// pointer (mirrors the egui app's `GizmoDrag`).
#[derive(Clone, Copy, Debug)]
pub struct GizmoDrag {
    pub layer: usize,
    pub handle: GizmoHandle,
    /// Playhead time when the grab started (where edits are keyed).
    pub time: f32,
    /// The layer's sampled transform at grab time.
    pub start_tf: Transform,
    /// The layer's parent matrix at grab time (parent-local conversion).
    pub parent: Affine2,
    /// Pointer position (comp space) when the grab started.
    pub start_comp: (f32, f32),
}

/// Which end of the work area a timeline drag is moving (see [`App::wa_drag`]).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum WorkAreaHandle {
    /// The in-point (work-area start).
    In,
    /// The out-point (work-area end).
    Out,
}

/// Identifies the keyframe being dragged on a timeline lane (see [`App::kf_drag`]).
#[derive(Clone, Copy, Debug)]
pub struct KeyframeDrag {
    pub layer: usize,
    pub prop: Prop,
    pub key_index: usize,
}

impl App {
    /// Build the shared state: a fresh demo project (matching the egui app) and a
    /// host primed to render its first frame.
    pub fn new() -> Self {
        let project = Project::new();
        let host = CanvasHost::new(&project);
        Self {
            host,
            project,
            time: 0.0,
            active: Tool::Select,
            selected_layer: None,
            playing: false,
            loop_enabled: true,
            scrubbing: false,
            last_tick: None,
            track_bounds: Rc::new(Cell::new(None)),
            wa_drag: None,
            kf_drag: None,
            effect_browser_open: false,
            effect_query: String::new(),
            undo: Vec::new(),
            redo: Vec::new(),
            export: None,
            graph_open: false,
            graph_shown: Vec::new(),
            graph_grab: None,
            gizmo_drag: None,
            preview_rect: Rc::new(Cell::new(None)),
            graph_rect: Rc::new(Cell::new(None)),
            hovered_gizmo_handle: None,
            expressions: HashMap::new(),
            effects_expanded: HashMap::new(),
            export_mp4_progress: None,
            export_gif_progress: None,
            audio_preview_enabled: false,
            audio_volume: 1.0,
            gpui_effects: HashMap::new(),
            gpui_effects_expanded: HashMap::new(),
            render_queue: Vec::new(),
            comp_settings_open: false,
            pending_comp_settings: None,
            ram_preview: HashMap::new(),
            ram_preview_playing: false,
            ram_preview_frame: 0,
            ram_preview_complete: false,
            layer_parents: HashMap::new(),
            picking_parent_for: None,
            sub_comps: Vec::new(),
            pre_comp_layers: HashMap::new(),
            active_sub_comp: None,
            roi: None,
            last_rendered_frame: u32::MAX,
            expr_controls: HashMap::new(),
            output_presets: vec![
                OutputPreset { name: "H.264 1080p".to_string(), format: export::OutputFormat::Png },
                OutputPreset { name: "ProRes 422 4K".to_string(), format: export::OutputFormat::Png },
                OutputPreset { name: "GIF 480p".to_string(), format: export::OutputFormat::Png },
            ],
            prores_progress: None,
            pending_export_format: None,
            live_output_enabled: false,
            mogrt_templates: Vec::new(),
            mogrt_panel_open: false,
            mogrt_selected: None,
            brainstorm: BrainstormState::new(),
            pre_render_status: PreRenderStatus::NotStarted,
            pre_render_cache_dir: None,
            use_pre_render: false,
            camera_tracker: CameraTracker::default(),
        }
    }

    /// Start exporting the active comp to a PNG image sequence.
    ///
    /// Pops a native folder picker (a discrete user action, so playback is paused
    /// first), then spawns a background worker that renders the chosen
    /// [`RenderRange`] frame-by-frame and writes the sequence, reporting progress
    /// through [`self.export`](Self::export). Returns `true` if an export was
    /// launched (the user picked a folder), `false` if they cancelled. The render
    /// runs off the UI thread, so the window stays responsive throughout.
    ///
    /// Mirrors the egui app's `export_dialog`, but threaded + progress-reporting
    /// and reusing the same project-aware render engine (precomps resolve).
    pub fn start_export(&mut self, range: RenderRange) -> bool {
        self.playing = false;
        self.last_tick = None;
        let Some(dir) = rfd::FileDialog::new()
            .set_title("Export PNG sequence to folder…")
            .pick_folder()
        else {
            return false;
        };
        // Snapshot the comps so the worker is detached from further edits.
        let comps = self.project.comps.clone();
        let comp_id = self.project.comps[self.active_comp_index()].id;
        let request = ExportRequest {
            comps,
            comp_id,
            dir,
            stem: "comp".to_string(),
            range,
            format: OutputFormat::Png,
        };
        self.export = Some(export::spawn(request));
        true
    }

    /// The render range an export defaults to (After Effects-style): the **work
    /// area** when it is a real trimmed sub-range of the active comp, else the
    /// **full comp**. The File menu offers both explicitly, but this is the
    /// "Export" default.
    pub fn default_export_range(&self) -> RenderRange {
        let ci = self.active_comp_index();
        RenderRange::default_for(&self.project.comps[ci])
    }

    /// Dismiss a finished export's progress readout (clears [`self.export`](Self::export)).
    /// A no-op while an export is still running.
    pub fn clear_finished_export(&mut self) {
        if self.export.as_ref().is_some_and(|p| p.is_finished()) {
            self.export = None;
        }
    }

    /// Whether an [`Action::Undo`] would do anything (the Edit menu greys out
    /// otherwise).
    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    /// Whether an [`Action::Redo`] would do anything.
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    /// The active comp index (clamped defensively; the project is always
    /// non-empty).
    pub fn active_comp_index(&self) -> usize {
        self.project
            .active
            .min(self.project.comps.len().saturating_sub(1))
    }

    /// Apply a panel-emitted [`Action`]. This is the ONLY place `App` state is
    /// mutated. Any action that changes the composited frame marks the host dirty
    /// so the next `host.image()` re-renders; pure UI state (tool/selection) does
    /// not touch the host.
    pub fn apply(&mut self, action: Action) {
        // Snapshot the pre-edit document for any document-mutating action, so the
        // edit is undoable. A fresh edit invalidates the redo stack (linear
        // history). Snapshotting *before* the mutation means `Undo` restores the
        // exact prior state. Pure UI/transport actions and Undo/Redo skip this.
        if action.is_undoable() {
            self.push_undo();
        }
        match action {
            Action::SetTool(t) => self.active = t,

            Action::SetTime(t) => {
                let dur = self.active_duration();
                self.time = t.clamp(0.0, dur);
                self.host.mark_dirty();
            }
            Action::StepTime(delta) => {
                let dur = self.active_duration();
                self.time = (self.time + delta).clamp(0.0, dur);
                self.host.mark_dirty();
            }
            Action::GoToStart => {
                self.time = 0.0;
                self.host.mark_dirty();
            }
            Action::GoToEnd => {
                let ci = self.active_comp_index();
                self.time = self.project.comps[ci].duration;
                self.host.mark_dirty();
            }
            Action::ToggleLoop => {
                self.loop_enabled = !self.loop_enabled;
            }
            Action::TogglePlay => {
                self.playing = !self.playing;
                // Re-anchor the wall clock on (re)start so the first tick steps
                // by a small real delta, not the gap since the last play.
                self.last_tick = self.playing.then(Instant::now);
                // When stopping, mark dirty so the next render is full-quality.
                if !self.playing {
                    self.host.mark_dirty();
                }
            }
            Action::Pause => {
                self.playing = false;
                self.last_tick = None;
                self.host.mark_dirty();
            }

            Action::ToggleLayerVisible(i) => {
                let ci = self.active_comp_index();
                if let Some(l) = self.project.comps[ci].layers.get_mut(i) {
                    l.visible = !l.visible;
                    self.host.mark_dirty();
                }
            }
            Action::SelectLayer(i) => {
                let ci = self.active_comp_index();
                if self.project.comps[ci].layers.get(i).is_some() {
                    self.selected_layer = Some(i);
                }
            }

            Action::SetTransform(prop, value) => {
                let Some(i) = self.selected_layer else { return };
                let ci = self.active_comp_index();
                let t = self.time;
                if let Some(layer) = self.project.comps[ci].layers.get_mut(i) {
                    // Edit the value at the current time exactly like the egui
                    // properties slider: `set_key` overwrites the key within EPS
                    // of `t` (preserving its interp), else inserts one — which for
                    // an empty track lays down a single constant key (the base
                    // value), and for an animated track re-keys this instant.
                    layer.track_mut(prop).set_key(t, value);
                    self.host.mark_dirty();
                }
            }

            Action::ToggleKeyframe(prop) => {
                let Some(i) = self.selected_layer else { return };
                let ci = self.active_comp_index();
                let t = self.time;
                // Sample the live (expression-resolved) value first so enabling
                // animation keys exactly what the preview currently shows.
                let cur = self.project.comps[ci].layer_value(i, prop, t);
                if let Some(layer) = self.project.comps[ci].layers.get_mut(i) {
                    let track = layer.track_mut(prop);
                    if track.keys.is_empty() {
                        // Stopwatch ON: lay down the first key at the playhead.
                        track.set_key(t, cur);
                    } else {
                        // Stopwatch OFF: drop all keys (collapse to static).
                        track.keys.clear();
                    }
                    self.host.mark_dirty();
                }
            }

            Action::MoveKeyframe {
                prop,
                key_index,
                time,
            } => {
                let Some(i) = self.selected_layer else { return };
                let ci = self.active_comp_index();
                let dur = self.active_duration();
                let new_t = time.clamp(0.0, dur);
                if let Some(layer) = self.project.comps[ci].layers.get_mut(i) {
                    let track = layer.track_mut(prop);
                    // Preserve the key's value; only its time changes on a drag.
                    if let Some(value) = track.keys.get(key_index).map(|k| k.value) {
                        let landed = track.move_key(key_index, new_t, value);
                        self.host.mark_dirty();
                        // Keep an in-flight lane drag tracking the same key after a
                        // re-sort past a neighbour (the index may have shifted).
                        if let Some(d) = self.kf_drag.as_mut() {
                            if d.layer == i && d.prop == prop && d.key_index == key_index {
                                d.key_index = landed;
                            }
                        }
                    }
                }
            }

            Action::ToggleEffectBrowser => {
                self.effect_browser_open = !self.effect_browser_open;
            }
            Action::SetEffectQuery(q) => {
                self.effect_query = q;
            }
            Action::AddEffect(entry) => {
                let Some(i) = self.selected_layer else { return };
                let ci = self.active_comp_index();
                if let Some(layer) = self.project.comps[ci].layers.get_mut(i) {
                    // Mirror the egui app's `add_browser_effect`: instantiate from
                    // the registry entry and push onto the matching stack (or
                    // overwrite the single generate slot).
                    match entry.instantiate() {
                        NewEffect::Color(e) => layer.effects.push(e),
                        NewEffect::Spatial(e) => layer.spatial_effects.push(e),
                        NewEffect::Distort(e) => layer.distort_effects.push(e),
                        NewEffect::Stylize(e) => layer.stylize_effects.push(e),
                        NewEffect::Keying(e) => layer.key_effects.push(e),
                        NewEffect::Generate(e) => layer.generate = Some(e),
                    }
                    self.host.mark_dirty();
                }
            }
            Action::RemoveEffect { stack, index } => {
                let Some(i) = self.selected_layer else { return };
                let ci = self.active_comp_index();
                if let Some(layer) = self.project.comps[ci].layers.get_mut(i) {
                    let removed = match stack {
                        EffectStack::Color => vec_remove(&mut layer.effects, index),
                        EffectStack::Spatial => vec_remove(&mut layer.spatial_effects, index),
                        EffectStack::Distort => vec_remove(&mut layer.distort_effects, index),
                        EffectStack::Stylize => vec_remove(&mut layer.stylize_effects, index),
                        EffectStack::Keying => vec_remove(&mut layer.key_effects, index),
                        EffectStack::Generate => layer.generate.take().is_some(),
                    };
                    if removed {
                        self.host.mark_dirty();
                    }
                }
            }
            Action::SetEffectParam {
                stack,
                index,
                param,
                value,
            } => {
                let Some(i) = self.selected_layer else { return };
                let ci = self.active_comp_index();
                if let Some(layer) = self.project.comps[ci].layers.get_mut(i) {
                    let edited = match stack {
                        EffectStack::Color => layer
                            .effects
                            .get_mut(index)
                            .map(|e| effect_params::set_color(e, param, value))
                            .is_some(),
                        EffectStack::Spatial => layer
                            .spatial_effects
                            .get_mut(index)
                            .map(|e| effect_params::set_spatial(e, param, value))
                            .is_some(),
                        EffectStack::Distort => layer
                            .distort_effects
                            .get_mut(index)
                            .map(|e| effect_params::set_distort(e, param, value))
                            .is_some(),
                        EffectStack::Stylize => layer
                            .stylize_effects
                            .get_mut(index)
                            .map(|e| effect_params::set_stylize(e, param, value))
                            .is_some(),
                        EffectStack::Keying => layer
                            .key_effects
                            .get_mut(index)
                            .map(|e| effect_params::set_key(e, param, value))
                            .is_some(),
                        EffectStack::Generate => layer
                            .generate
                            .as_mut()
                            .map(|e| effect_params::set_generate(e, param, value))
                            .is_some(),
                    };
                    if edited {
                        self.host.mark_dirty();
                    }
                }
            }

            Action::SetWorkAreaStart(t) => {
                let ci = self.active_comp_index();
                let comp = &mut self.project.comps[ci];
                let dur = comp.duration;
                let t = t.clamp(0.0, dur);
                // Match the egui app: start can't cross above the current end.
                comp.work_area.start = t.min(comp.work_area.end);
                comp.work_area = comp.work_area.clamped(dur);
                // Work-area trims don't change the composited frame, but the
                // timeline overlay must repaint; the root view's cx.notify covers
                // that, so no host dirty needed.
            }
            Action::SetWorkAreaEnd(t) => {
                let ci = self.active_comp_index();
                let comp = &mut self.project.comps[ci];
                let dur = comp.duration;
                let t = t.clamp(0.0, dur);
                comp.work_area.end = t.max(comp.work_area.start);
                comp.work_area = comp.work_area.clamped(dur);
            }
            Action::ResetWorkArea => {
                let ci = self.active_comp_index();
                let comp = &mut self.project.comps[ci];
                comp.work_area = WorkArea::full(comp.duration);
            }

            Action::ToggleGraph => {
                self.graph_open = !self.graph_open;
                // Drop any in-flight graph drag when switching views.
                self.graph_grab = None;
            }
            Action::ToggleGraphProp(prop) => {
                if let Some(i) = self.graph_shown.iter().position(|&p| p == prop) {
                    self.graph_shown.remove(i);
                } else {
                    self.graph_shown.push(prop);
                }
            }
            Action::ClearGraphProps => {
                self.graph_shown.clear();
            }
            Action::SetInterp {
                prop,
                key_index,
                interp,
            } => {
                let Some(i) = self.selected_layer else { return };
                let ci = self.active_comp_index();
                if let Some(layer) = self.project.comps[ci].layers.get_mut(i) {
                    let track = layer.track_mut(prop);
                    if let Some(k) = track.key_mut(key_index) {
                        k.interp = interp;
                        self.host.mark_dirty();
                    }
                }
            }
            Action::MoveKeyframeXY {
                prop,
                key_index,
                time,
                value,
            } => {
                let Some(i) = self.selected_layer else { return };
                let ci = self.active_comp_index();
                let dur = self.active_duration();
                let new_t = time.clamp(0.0, dur);
                if let Some(layer) = self.project.comps[ci].layers.get_mut(i) {
                    let track = layer.track_mut(prop);
                    if key_index < track.keys.len() {
                        let landed = track.move_key(key_index, new_t, value);
                        self.host.mark_dirty();
                        // Keep an in-flight graph key drag tracking the same key
                        // after a re-sort past a neighbour.
                        if let Some(GraphGrab::Key {
                            prop: gp,
                            key_index: gk,
                        }) = self.graph_grab.as_mut()
                        {
                            if *gp == prop && *gk == key_index {
                                *gk = landed;
                            }
                        }
                    }
                }
            }

            Action::GizmoKeys { time, keys } => {
                let Some(i) = self.selected_layer else { return };
                let ci = self.active_comp_index();
                let t = time;
                if let Some(layer) = self.project.comps[ci].layers.get_mut(i) {
                    for (prop, value) in keys {
                        // Same write as the properties panel: overwrite the key at
                        // this instant, else lay one down (constant on an empty
                        // track, a fresh key on an animated one).
                        layer.track_mut(prop).set_key(t, value);
                    }
                    self.host.mark_dirty();
                }
            }

            // --- 3D layer controls ---
            Action::SetLayer3D(layer_id, enable) => {
                let ci = self.active_comp_index();
                if let Some(layer) = self.project.comps[ci].layers.get_mut(layer_id) {
                    layer.threed = enable;
                    self.host.mark_dirty();
                }
            }
            Action::SetPositionZ(layer_id, z) => {
                let ci = self.active_comp_index();
                let t = self.time;
                if let Some(layer) = self.project.comps[ci].layers.get_mut(layer_id) {
                    layer.z.set_key(t, z);
                    self.host.mark_dirty();
                }
            }
            Action::Set3DRotation(layer_id, rx, ry, rz) => {
                let ci = self.active_comp_index();
                let t = self.time;
                if let Some(layer) = self.project.comps[ci].layers.get_mut(layer_id) {
                    layer.orient_x.set_key(t, rx);
                    layer.orient_y.set_key(t, ry);
                    layer.orient_z.set_key(t, rz);
                    self.host.mark_dirty();
                }
            }

            // --- Gizmo hover / modifier state ---
            Action::SetHoveredGizmoHandle(h) => {
                // Pure UI state — no undo, no dirty.
                self.hovered_gizmo_handle = h;
            }
            Action::DuplicateLayer(layer_id) => {
                let ci = self.active_comp_index();
                if layer_id < self.project.comps[ci].layers.len() {
                    let mut dup = self.project.comps[ci].layers[layer_id].clone();
                    // Append "(copy)" suffix to distinguish the duplicate.
                    dup.name = format!("{} (copy)", dup.name);
                    let new_idx = layer_id + 1;
                    self.project.comps[ci].layers.insert(new_idx, dup);
                    self.selected_layer = Some(new_idx);
                    self.host.mark_dirty();
                }
            }

            // --- Expressions ---
            Action::SetExpression { layer_id, prop, expr } => {
                // Also write the expression directly onto the layer's track so the
                // engine evaluates it each frame.
                let ci = self.active_comp_index();
                if let Some(layer) = self.project.comps[ci].layers.get_mut(layer_id) {
                    // Map prop name to a track so we can set the expression.
                    let track = match prop.as_str() {
                        "X" => Some(&mut layer.x),
                        "Y" => Some(&mut layer.y),
                        "Z" => Some(&mut layer.z),
                        "Scale" => Some(&mut layer.scale),
                        "Rotation" => Some(&mut layer.rotation),
                        "Opacity" => Some(&mut layer.opacity),
                        "AnchorX" => Some(&mut layer.anchor_x),
                        "AnchorY" => Some(&mut layer.anchor_y),
                        "OrientX" => Some(&mut layer.orient_x),
                        "OrientY" => Some(&mut layer.orient_y),
                        "OrientZ" => Some(&mut layer.orient_z),
                        _ => None,
                    };
                    if let Some(t) = track {
                        if expr.is_empty() {
                            t.expression = None;
                        } else {
                            t.expression = Some(expr.clone());
                        }
                    }
                }
                if expr.is_empty() {
                    self.expressions.remove(&(layer_id, prop));
                } else {
                    self.expressions.insert((layer_id, prop), expr);
                }
                self.host.mark_dirty();
            }

            Action::Undo => {
                if let Some(prev) = self.undo.pop() {
                    let cur = std::mem::replace(&mut self.project, prev);
                    self.redo.push(cur);
                    self.after_history_swap();
                }
            }
            Action::Redo => {
                if let Some(next) = self.redo.pop() {
                    let cur = std::mem::replace(&mut self.project, next);
                    self.undo.push(cur);
                    self.after_history_swap();
                }
            }

            // --- Wave 9: MP4 export ---
            Action::ExportMp4(path) => {
                self.playing = false;
                self.last_tick = None;
                self.export_mp4_progress = Some(0.0);
                let ci = self.active_comp_index();
                let comp = &self.project.comps[ci];
                let fps = comp.fps as f64;
                let width = comp.width;
                let height = comp.height;
                let total_frames = (comp.duration * comp.fps).ceil() as u32;
                // Collect all RGBA8 frames by rendering each one
                let comps = self.project.comps.clone();
                let comp_id = comp.id;
                let mut cache = crate::comp::FrameCache::new();
                let mut frames: Vec<Vec<u8>> = Vec::with_capacity(total_frames as usize);
                for fi in 0..total_frames {
                    let t = fi as f32 / fps as f32;
                    let frame = crate::render::render_frame_in_project(&comps, comp_id, t, &mut cache);
                    frames.push(frame.pixels);
                    self.export_mp4_progress = Some(fi as f32 / total_frames as f32 * 0.9);
                }
                // Encode via prism-media
                let params = prism_media::EncodeParams::new(width, height, fps);
                match prism_media::encode_h264(frames.into_iter(), &params, &path) {
                    Ok(n) => {
                        log::info!("MP4 export done: {} frames → {}", n, path.display());
                    }
                    Err(e) => {
                        log::warn!("MP4 export failed ({}), falling back to PNG sequence: {}", path.display(), e);
                        // Graceful fallback: write PNG sequence to the same dir
                        let dir = path.parent().unwrap_or(std::path::Path::new(".")).to_path_buf();
                        let _ = std::fs::create_dir_all(&dir);
                        let mut cache2 = crate::comp::FrameCache::new();
                        for fi in 0..total_frames {
                            let t = fi as f32 / fps as f32;
                            let frame = crate::render::render_frame_in_project(&comps, comp_id, t, &mut cache2);
                            let png_path = dir.join(format!("frame_{:04}.png", fi));
                            if let Some(img) = image::RgbaImage::from_raw(frame.width, frame.height, frame.pixels) {
                                let _ = img.save_with_format(&png_path, image::ImageFormat::Png);
                            }
                        }
                        log::info!("PNG fallback sequence written to {}", dir.display());
                    }
                }
                self.export_mp4_progress = None;
            }

            Action::ExportGif(path) => {
                self.playing = false;
                self.last_tick = None;
                let ci = self.active_comp_index();
                let comp = &self.project.comps[ci];
                let fps = comp.fps.max(1.0);
                let duration = comp.duration;
                let total_frames = (duration * fps).ceil() as u32;
                let comps = self.project.comps.clone();
                let comp_id = comp.id;
                self.export_gif_progress = Some(0.0);

                let mut cache = crate::comp::FrameCache::new();
                let mut gif_frames: Vec<(Vec<u8>, u32, u32)> = Vec::with_capacity(total_frames as usize);
                for fi in 0..total_frames {
                    let t = fi as f32 / fps;
                    let frame = crate::render::render_preview_frame(&comps, comp_id, t, 640, &mut cache);
                    gif_frames.push((frame.pixels, frame.width, frame.height));
                    self.export_gif_progress = Some(fi as f32 / total_frames as f32 * 0.9);
                }

                // Encode to GIF using `image` crate's GifEncoder.
                let frame_delay_centisecs = (100.0 / fps).round() as u16;
                let result = (|| -> Result<(), String> {
                    let file = std::fs::File::create(&path).map_err(|e| e.to_string())?;
                    let mut encoder = image::codecs::gif::GifEncoder::new(file);
                    encoder.set_repeat(image::codecs::gif::Repeat::Infinite).map_err(|e| e.to_string())?;
                    let delay = image::Delay::from_saturating_duration(
                        std::time::Duration::from_millis(frame_delay_centisecs as u64 * 10),
                    );
                    for (pixels, w, h) in &gif_frames {
                        if let Some(rgba_img) = image::RgbaImage::from_raw(*w, *h, pixels.clone()) {
                            let frame = image::Frame::from_parts(rgba_img, 0, 0, delay);
                            encoder.encode_frame(frame).map_err(|e| e.to_string())?;
                        }
                    }
                    Ok(())
                })();
                match result {
                    Ok(()) => log::info!("GIF export done: {} frames → {}", total_frames, path.display()),
                    Err(e) => log::warn!("GIF export failed: {}", e),
                }
                self.export_gif_progress = None;
            }

            // --- Wave 9: Audio preview stub ---
            Action::ToggleAudioPreview => {
                self.audio_preview_enabled = !self.audio_preview_enabled;
            }
            Action::SetAudioVolume(v) => {
                self.audio_volume = v.clamp(0.0, 1.5);
            }

            // --- Wave 9: GPUI-side effects ---
            Action::AddGpuiEffect(kind) => {
                let Some(li) = self.selected_layer else { return };
                let ci = self.active_comp_index();
                if self.project.comps[ci].layers.get(li).is_none() { return }
                let effect = GpuiEffect::default_for_kind(kind);
                self.gpui_effects.entry(li).or_default().push(effect);
                self.host.mark_dirty();
            }
            Action::RemoveGpuiEffect(ei) => {
                let Some(li) = self.selected_layer else { return };
                let stack = self.gpui_effects.entry(li).or_default();
                if ei < stack.len() {
                    stack.remove(ei);
                    self.host.mark_dirty();
                }
            }
            Action::SetMosaicBlock { effect_idx, block } => {
                let Some(li) = self.selected_layer else { return };
                if let Some(e) = self.gpui_effects.get_mut(&li).and_then(|v| v.get_mut(effect_idx)) {
                    if let GpuiEffect::Mosaic { block: b } = e { *b = block.clamp(1, 128); }
                    self.host.mark_dirty();
                }
            }
            Action::SetChromaOffset { effect_idx, offset } => {
                let Some(li) = self.selected_layer else { return };
                if let Some(e) = self.gpui_effects.get_mut(&li).and_then(|v| v.get_mut(effect_idx)) {
                    if let GpuiEffect::ChromaticAberration { offset: o } = e { *o = offset; }
                    self.host.mark_dirty();
                }
            }
            Action::SetEffectIntensity { effect_idx, intensity } => {
                let Some(li) = self.selected_layer else { return };
                if let Some(e) = self.gpui_effects.get_mut(&li).and_then(|v| v.get_mut(effect_idx)) {
                    let i = intensity.clamp(0.0, 1.0);
                    match e {
                        GpuiEffect::Vignette { intensity: iv, .. } => *iv = i,
                        GpuiEffect::Noise { intensity: ni } => *ni = i,
                        _ => {}
                    }
                    self.host.mark_dirty();
                }
            }
            Action::SetVignetteRadius { effect_idx, radius } => {
                let Some(li) = self.selected_layer else { return };
                if let Some(e) = self.gpui_effects.get_mut(&li).and_then(|v| v.get_mut(effect_idx)) {
                    if let GpuiEffect::Vignette { radius: r, .. } = e { *r = radius.clamp(0.5, 1.0); }
                    self.host.mark_dirty();
                }
            }
            Action::ToggleGpuiEffectExpand(ei) => {
                let li = self.selected_layer.unwrap_or(0);
                let key = (li, ei);
                let cur = *self.gpui_effects_expanded.get(&key).unwrap_or(&false);
                self.gpui_effects_expanded.insert(key, !cur);
            }

            // --- Wave 9: Render queue ---
            Action::AddToRenderQueue => {
                // Offer all supported render formats; default to H.264 MP4.
                // Present each format as a separate filter so the user picks by
                // extension and we infer the format from the path's extension.
                let ci = self.active_comp_index();
                let comp_name = self.project.comps[ci].name.clone();
                let comp_name = if comp_name.is_empty() {
                    format!("Comp {}", ci + 1)
                } else {
                    comp_name
                };
                if let Some(path) = rfd::FileDialog::new()
                    .set_title("Add to render queue — choose output format…")
                    .add_filter("H.264 MP4 (H.264)", &["mp4"])
                    .add_filter("H.265 MP4 (HEVC)", &["mp4"])
                    .add_filter("ProRes Proxy (requires FFmpeg)", &["mov"])
                    .add_filter("Animated GIF", &["gif"])
                    .save_file()
                {
                    // Infer format from the chosen file extension.
                    let format = match path.extension().and_then(|e| e.to_str()) {
                        Some("mov") => RenderFormat::ProResProxy,
                        Some("gif") => RenderFormat::Gif,
                        _ => RenderFormat::Mp4H264,
                    };
                    self.render_queue.push(RenderJob {
                        comp_name,
                        output_path: path,
                        format,
                        status: RenderJobStatus::Pending,
                    });
                }
            }
            Action::RenderAll => {
                // Process each Pending job serially (blocking the UI — acceptable
                // for now since it mirrors the egui app's synchronous export)
                let n = self.render_queue.len();
                for i in 0..n {
                    if !matches!(self.render_queue[i].status, RenderJobStatus::Pending) {
                        continue;
                    }
                    self.render_queue[i].status = RenderJobStatus::Rendering(0.0);
                    let path = self.render_queue[i].output_path.clone();
                    let format = self.render_queue[i].format;
                    let ci = self.active_comp_index();
                    let comp = &self.project.comps[ci];
                    let fps = comp.fps as f64;
                    let width = comp.width;
                    let height = comp.height;
                    let total_frames = (comp.duration * comp.fps).ceil() as u32;
                    let comps = self.project.comps.clone();
                    let comp_id = comp.id;
                    let result: Result<(), String> = match format {
                        RenderFormat::Mp4H264 | RenderFormat::Mp4H265 => {
                            let mut cache = crate::comp::FrameCache::new();
                            let frames_iter = (0..total_frames).map(|fi| {
                                let t = fi as f32 / fps as f32;
                                let frame = crate::render::render_frame_in_project(&comps, comp_id, t, &mut cache);
                                frame.pixels
                            });
                            let params = prism_media::EncodeParams::new(width, height, fps);
                            prism_media::encode_h264(frames_iter, &params, &path)
                                .map(|_| ())
                                .map_err(|e| e.to_string())
                        }
                        RenderFormat::ProResProxy => {
                            // ProRes requires FFmpeg available on PATH.
                            let ffmpeg_ok = std::process::Command::new("ffmpeg")
                                .arg("-version")
                                .output()
                                .is_ok();
                            if !ffmpeg_ok {
                                Err("ProRes export requires FFmpeg. Install via: brew install ffmpeg".into())
                            } else {
                                use std::io::Write;
                                use std::process::{Command, Stdio};
                                match Command::new("ffmpeg")
                                    .args([
                                        "-y",
                                        "-f", "rawvideo",
                                        "-pix_fmt", "rgba",
                                        "-s", &format!("{}x{}", width, height),
                                        "-r", &fps.to_string(),
                                        "-i", "pipe:0",
                                        "-c:v", "prores_ks",
                                        "-profile:v", "0",
                                        path.to_str().unwrap_or("output.mov"),
                                    ])
                                    .stdin(Stdio::piped())
                                    .stdout(Stdio::null())
                                    .stderr(Stdio::null())
                                    .spawn()
                                {
                                    Err(e) => Err(e.to_string()),
                                    Ok(mut child) => {
                                        if let Some(stdin) = child.stdin.as_mut() {
                                            let mut cache = crate::comp::FrameCache::new();
                                            for fi in 0..total_frames {
                                                let t = fi as f32 / fps as f32;
                                                let frame = crate::render::render_frame_in_project(&comps, comp_id, t, &mut cache);
                                                let _ = stdin.write_all(&frame.pixels);
                                            }
                                        }
                                        child.wait().map(|_| ()).map_err(|e| e.to_string())
                                    }
                                }
                            }
                        }
                        RenderFormat::Gif => {
                            // Encode as animated GIF using the image crate's GIF encoder.
                            use image::codecs::gif::{GifEncoder, Repeat};
                            use image::{Delay, Frame, RgbaImage};
                            use std::fs::File;
                            match File::create(&path) {
                                Err(e) => Err(e.to_string()),
                                Ok(file) => {
                                    let mut enc = GifEncoder::new(file);
                                    match enc.set_repeat(Repeat::Infinite) {
                                        Err(e) => Err(e.to_string()),
                                        Ok(_) => {
                                            let delay_ms = (1000.0 / fps) as u32;
                                            let mut cache = crate::comp::FrameCache::new();
                                            let mut err: Option<String> = None;
                                            for fi in 0..total_frames {
                                                if err.is_some() { break; }
                                                let t = fi as f32 / fps as f32;
                                                let frame = crate::render::render_frame_in_project(&comps, comp_id, t, &mut cache);
                                                match RgbaImage::from_raw(width, height, frame.pixels) {
                                                    None => { err = Some("GIF frame buffer mismatch".into()); }
                                                    Some(img) => {
                                                        let gif_frame = Frame::from_parts(
                                                            img, 0, 0,
                                                            Delay::from_numer_denom_ms(delay_ms, 1),
                                                        );
                                                        if let Err(e) = enc.encode_frame(gif_frame) {
                                                            err = Some(e.to_string());
                                                        }
                                                    }
                                                }
                                            }
                                            match err {
                                                Some(e) => Err(e),
                                                None => Ok(()),
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    };
                    match result {
                        Ok(_) => self.render_queue[i].status = RenderJobStatus::Done,
                        Err(e) => self.render_queue[i].status = RenderJobStatus::Failed(e),
                    }
                }
            }
            Action::RemoveFromRenderQueue(i) => {
                if i < self.render_queue.len() {
                    self.render_queue.remove(i);
                }
            }

            // --- Wave 9: Composition settings dialog ---
            Action::ToggleCompSettings => {
                self.comp_settings_open = !self.comp_settings_open;
                if self.comp_settings_open && self.pending_comp_settings.is_none() {
                    let ci = self.active_comp_index();
                    let comp = &self.project.comps[ci];
                    self.pending_comp_settings = Some(PendingCompSettings {
                        width: comp.width,
                        height: comp.height,
                        fps: comp.fps,
                        duration_secs: comp.duration,
                        bg_color: [0.0, 0.0, 0.0, 1.0],
                    });
                } else if !self.comp_settings_open {
                    self.pending_comp_settings = None;
                }
            }
            Action::SetPendingCompWidth(w) => {
                if let Some(p) = self.pending_comp_settings.as_mut() { p.width = w.max(1); }
            }
            Action::SetPendingCompHeight(h) => {
                if let Some(p) = self.pending_comp_settings.as_mut() { p.height = h.max(1); }
            }
            Action::SetPendingCompFps(f) => {
                if let Some(p) = self.pending_comp_settings.as_mut() { p.fps = f.clamp(1.0, 120.0); }
            }
            Action::SetPendingCompDuration(d) => {
                if let Some(p) = self.pending_comp_settings.as_mut() { p.duration_secs = d.max(0.1); }
            }
            Action::SetPendingCompBgColor(c) => {
                if let Some(p) = self.pending_comp_settings.as_mut() { p.bg_color = c; }
            }
            Action::ApplyCompSettings => {
                if let Some(pending) = self.pending_comp_settings.clone() {
                    let ci = self.active_comp_index();
                    let comp = &mut self.project.comps[ci];
                    comp.width = pending.width.max(1);
                    comp.height = pending.height.max(1);
                    comp.fps = pending.fps.clamp(1.0, 120.0);
                    comp.duration = pending.duration_secs.max(0.1);
                    // Clamp work area and playhead to new duration
                    comp.work_area = comp.work_area.clamped(comp.duration);
                    self.time = self.time.clamp(0.0, comp.duration);
                    // Resize host buffers by marking dirty (next render picks new dims)
                    self.host.mark_dirty();
                }
            }

            // --- Wave 11: RAM preview ---
            Action::BuildRamPreview => {
                let ci = self.active_comp_index();
                let comp = &self.project.comps[ci];
                let fps = comp.fps.max(1.0);
                let total_frames = (comp.duration * fps).ceil() as u32;
                let comp_id = comp.id;
                let comps = self.project.comps.clone();
                self.ram_preview.clear();
                let mut cache = crate::comp::FrameCache::new();
                for fi in 0..total_frames {
                    let t = fi as f32 / fps;
                    let frame = crate::render::render_preview_frame(&comps, comp_id, t, 1280, &mut cache);
                    let bgra: Vec<u8> = frame.pixels.chunks_exact(4).flat_map(|px| [px[2], px[1], px[0], px[3]]).collect();
                    self.ram_preview.insert(fi, bgra);
                }
                self.ram_preview_complete = true;
                self.ram_preview_playing = false;
            }
            Action::PlayRamPreview => {
                if self.ram_preview_complete {
                    self.ram_preview_playing = true;
                    let ci = self.active_comp_index();
                    let wa = self.active_work_area();
                    let fps = self.project.comps[ci].fps.max(1.0);
                    self.ram_preview_frame = (wa.start * fps).round() as u32;
                    self.time = wa.start;
                    self.host.mark_dirty();
                }
            }
            Action::PurgeRamPreview => {
                self.ram_preview.clear();
                self.ram_preview_complete = false;
                self.ram_preview_playing = false;
                self.ram_preview_frame = 0;
                self.host.frame_cache.clear();
                self.host.cache_order.clear();
                self.host.mark_dirty();
            }
            Action::ClearRamPreview => {
                self.host.frame_cache.clear();
                self.host.cache_order.clear();
                self.host.mark_dirty();
            }

            // --- Wave 11: layer parenting ---
            Action::SetParent(child, parent) => {
                let ci = self.active_comp_index();
                if child != parent {
                    self.layer_parents.insert(child, parent);
                    // Also wire into the comp model so the renderer follows the chain.
                    if let Some(layer) = self.project.comps[ci].layers.get_mut(child) {
                        layer.parent = Some(parent);
                    }
                    self.picking_parent_for = None;
                    self.host.mark_dirty();
                }
            }
            Action::ClearParent(child) => {
                let ci = self.active_comp_index();
                self.layer_parents.remove(&child);
                // Also clear from the comp model.
                if let Some(layer) = self.project.comps[ci].layers.get_mut(child) {
                    layer.parent = None;
                }
                self.host.mark_dirty();
            }
            Action::SetPickingParent(opt) => {
                self.picking_parent_for = opt;
            }

            // --- Wave 11: pre-compose ---
            Action::PreCompose(indices, name) => {
                if indices.is_empty() { return; }
                let ci = self.active_comp_index();
                let mut sorted = indices.clone();
                sorted.sort_unstable_by(|a, b| b.cmp(a));
                let mut taken_layers = Vec::new();
                for &idx in &sorted {
                    if idx < self.project.comps[ci].layers.len() {
                        taken_layers.push(self.project.comps[ci].layers.remove(idx));
                    }
                }
                taken_layers.reverse();
                let sub_idx = self.sub_comps.len();
                self.sub_comps.push(SubComp { name: name.clone(), layers: taken_layers });
                let insert_at = *indices.iter().min().unwrap_or(&0);
                let insert_at = insert_at.min(self.project.comps[ci].layers.len());
                let placeholder = crate::comp::PulseLayer::of_kind(
                    crate::comp::LayerKind::Null,
                    name,
                    [0.5, 0.5, 0.5, 1.0],
                );
                self.project.comps[ci].layers.insert(insert_at, placeholder);
                self.pre_comp_layers.insert(insert_at, sub_idx);
                self.host.mark_dirty();
                self.selected_layer = None;
            }
            Action::OpenSubComp(idx) => {
                self.active_sub_comp = Some(idx);
            }
            Action::CloseSubComp => {
                self.active_sub_comp = None;
            }

            // --- Wave 11: null object ---
            Action::AddNullLayer => {
                let ci = self.active_comp_index();
                let null = crate::comp::PulseLayer::of_kind(
                    crate::comp::LayerKind::Null,
                    "Null 1",
                    [0.8, 0.8, 0.8, 1.0],
                );
                self.project.comps[ci].layers.push(null);
                self.selected_layer = Some(self.project.comps[ci].layers.len() - 1);
                self.host.mark_dirty();
            }

            Action::AddGuideLayer => {
                let ci = self.active_comp_index();
                let n = self.project.comps[ci].layers.len() + 1;
                let guide = crate::comp::PulseLayer::of_kind(
                    crate::comp::LayerKind::Guide,
                    &format!("Guide {n}"),
                    [0.2, 0.2, 0.8, 1.0],
                );
                self.project.comps[ci].layers.push(guide);
                self.selected_layer = Some(self.project.comps[ci].layers.len() - 1);
                self.host.mark_dirty();
            }

            // --- Wave 11: solid + adjustment layers ---
            Action::AddSolidLayer(rgba) => {
                let ci = self.active_comp_index();
                let color = [
                    rgba[0] as f32 / 255.0,
                    rgba[1] as f32 / 255.0,
                    rgba[2] as f32 / 255.0,
                    rgba[3] as f32 / 255.0,
                ];
                let layer = crate::comp::PulseLayer::of_kind(
                    crate::comp::LayerKind::Solid,
                    "Solid 1",
                    color,
                );
                self.project.comps[ci].layers.push(layer);
                self.selected_layer = Some(self.project.comps[ci].layers.len() - 1);
                self.host.mark_dirty();
            }
            Action::AddAdjustmentLayer => {
                let ci = self.active_comp_index();
                let layer = crate::comp::PulseLayer::of_kind(
                    crate::comp::LayerKind::Adjustment,
                    "Adjustment 1",
                    [1.0, 1.0, 1.0, 1.0],
                );
                self.project.comps[ci].layers.push(layer);
                self.selected_layer = Some(self.project.comps[ci].layers.len() - 1);
                self.host.mark_dirty();
            }

            // --- Wave 12: video footage import ---
            Action::ImportVideoFootage => {
                let path = rfd::FileDialog::new()
                    .add_filter("Video", &["mp4", "mov", "mkv", "avi", "webm", "mts", "m2ts"])
                    .pick_file();
                if let Some(path) = path {
                    match prism_media::probe(&path) {
                        Ok(info) => {
                            let ci = self.active_comp_index();
                            let name = path
                                .file_name()
                                .and_then(|n| n.to_str())
                                .unwrap_or("Video")
                                .to_string();
                            let frame_count =
                                (info.duration_secs * info.fps).ceil() as u64;
                            let mut layer = crate::comp::PulseLayer::of_kind(
                                crate::comp::LayerKind::Footage,
                                &name,
                                [0.4, 0.4, 0.8, 1.0],
                            );
                            layer.footage.source = Some(crate::comp::FootageSource::Video {
                                path: path.clone(),
                                fps: info.fps,
                                frame_count: frame_count.max(1),
                                width: info.width,
                                height: info.height,
                            });
                            self.project.comps[ci].layers.push(layer);
                            self.selected_layer =
                                Some(self.project.comps[ci].layers.len() - 1);
                            self.host.mark_dirty();
                        }
                        Err(e) => {
                            log::warn!("Video probe failed (ffprobe not installed?): {e}");
                            // Still import as a still fallback so the user gets a layer.
                            let ci = self.active_comp_index();
                            let name = path
                                .file_name()
                                .and_then(|n| n.to_str())
                                .unwrap_or("Video")
                                .to_string();
                            let mut layer = crate::comp::PulseLayer::of_kind(
                                crate::comp::LayerKind::Footage,
                                &name,
                                [0.4, 0.4, 0.8, 1.0],
                            );
                            layer.footage.source =
                                Some(crate::comp::FootageSource::still(path));
                            self.project.comps[ci].layers.push(layer);
                            self.selected_layer =
                                Some(self.project.comps[ci].layers.len() - 1);
                            self.host.mark_dirty();
                        }
                    }
                }
            }
            Action::SetFootageVideo { layer_idx, path, fps, frame_count, width, height } => {
                let ci = self.active_comp_index();
                if let Some(layer) = self.project.comps[ci].layers.get_mut(layer_idx) {
                    layer.footage.source = Some(crate::comp::FootageSource::Video {
                        path,
                        fps,
                        frame_count,
                        width,
                        height,
                    });
                    self.host.mark_dirty();
                }
            }

            // --- Wave 11: new GPUI effect param setters ---
            Action::SetColorBalanceShadows { effect_idx, channel, value } => {
                let Some(li) = self.selected_layer else { return };
                if let Some(e) = self.gpui_effects.get_mut(&li).and_then(|v| v.get_mut(effect_idx)) {
                    if let crate::gpui_effects::GpuiEffect::ColorBalance { shadows_r, shadows_g, shadows_b, .. } = e {
                        match channel { 0 => *shadows_r = value, 1 => *shadows_g = value, _ => *shadows_b = value }
                    }
                    self.host.mark_dirty();
                }
            }
            Action::SetColorBalanceMidtones { effect_idx, channel, value } => {
                let Some(li) = self.selected_layer else { return };
                if let Some(e) = self.gpui_effects.get_mut(&li).and_then(|v| v.get_mut(effect_idx)) {
                    if let crate::gpui_effects::GpuiEffect::ColorBalance { midtones_r, midtones_g, midtones_b, .. } = e {
                        match channel { 0 => *midtones_r = value, 1 => *midtones_g = value, _ => *midtones_b = value }
                    }
                    self.host.mark_dirty();
                }
            }
            Action::SetColorBalanceHighlights { effect_idx, channel, value } => {
                let Some(li) = self.selected_layer else { return };
                if let Some(e) = self.gpui_effects.get_mut(&li).and_then(|v| v.get_mut(effect_idx)) {
                    if let crate::gpui_effects::GpuiEffect::ColorBalance { highlights_r, highlights_g, highlights_b, .. } = e {
                        match channel { 0 => *highlights_r = value, 1 => *highlights_g = value, _ => *highlights_b = value }
                    }
                    self.host.mark_dirty();
                }
            }
            Action::SetLevelsInBlack { effect_idx, value } => {
                let Some(li) = self.selected_layer else { return };
                if let Some(e) = self.gpui_effects.get_mut(&li).and_then(|v| v.get_mut(effect_idx)) {
                    if let crate::gpui_effects::GpuiEffect::Levels { in_black, .. } = e { *in_black = value; }
                    self.host.mark_dirty();
                }
            }
            Action::SetLevelsInWhite { effect_idx, value } => {
                let Some(li) = self.selected_layer else { return };
                if let Some(e) = self.gpui_effects.get_mut(&li).and_then(|v| v.get_mut(effect_idx)) {
                    if let crate::gpui_effects::GpuiEffect::Levels { in_white, .. } = e { *in_white = value; }
                    self.host.mark_dirty();
                }
            }
            Action::SetLevelsGamma { effect_idx, value } => {
                let Some(li) = self.selected_layer else { return };
                if let Some(e) = self.gpui_effects.get_mut(&li).and_then(|v| v.get_mut(effect_idx)) {
                    if let crate::gpui_effects::GpuiEffect::Levels { gamma, .. } = e { *gamma = value.max(0.01); }
                    self.host.mark_dirty();
                }
            }
            Action::SetLevelsOutBlack { effect_idx, value } => {
                let Some(li) = self.selected_layer else { return };
                if let Some(e) = self.gpui_effects.get_mut(&li).and_then(|v| v.get_mut(effect_idx)) {
                    if let crate::gpui_effects::GpuiEffect::Levels { out_black, .. } = e { *out_black = value; }
                    self.host.mark_dirty();
                }
            }
            Action::SetLevelsOutWhite { effect_idx, value } => {
                let Some(li) = self.selected_layer else { return };
                if let Some(e) = self.gpui_effects.get_mut(&li).and_then(|v| v.get_mut(effect_idx)) {
                    if let crate::gpui_effects::GpuiEffect::Levels { out_white, .. } = e { *out_white = value; }
                    self.host.mark_dirty();
                }
            }
            Action::SetHueShift { effect_idx, value } => {
                let Some(li) = self.selected_layer else { return };
                if let Some(e) = self.gpui_effects.get_mut(&li).and_then(|v| v.get_mut(effect_idx)) {
                    if let crate::gpui_effects::GpuiEffect::HueSaturation { hue_shift, .. } = e { *hue_shift = value; }
                    self.host.mark_dirty();
                }
            }
            Action::SetSaturation { effect_idx, value } => {
                let Some(li) = self.selected_layer else { return };
                if let Some(e) = self.gpui_effects.get_mut(&li).and_then(|v| v.get_mut(effect_idx)) {
                    if let crate::gpui_effects::GpuiEffect::HueSaturation { saturation, .. } = e { *saturation = value.clamp(0.0, 2.0); }
                    self.host.mark_dirty();
                }
            }
            Action::SetLightness { effect_idx, value } => {
                let Some(li) = self.selected_layer else { return };
                if let Some(e) = self.gpui_effects.get_mut(&li).and_then(|v| v.get_mut(effect_idx)) {
                    if let crate::gpui_effects::GpuiEffect::HueSaturation { lightness, .. } = e { *lightness = value.clamp(-1.0, 1.0); }
                    self.host.mark_dirty();
                }
            }
            Action::SetNoiseFrequency { effect_idx, value } => {
                let Some(li) = self.selected_layer else { return };
                if let Some(e) = self.gpui_effects.get_mut(&li).and_then(|v| v.get_mut(effect_idx)) {
                    if let crate::gpui_effects::GpuiEffect::FractalNoise { frequency, .. } = e { *frequency = value.max(0.001); }
                    self.host.mark_dirty();
                }
            }
            Action::SetNoiseEvolution { effect_idx, value } => {
                let Some(li) = self.selected_layer else { return };
                if let Some(e) = self.gpui_effects.get_mut(&li).and_then(|v| v.get_mut(effect_idx)) {
                    if let crate::gpui_effects::GpuiEffect::FractalNoise { evolution, .. } = e { *evolution = value; }
                    self.host.mark_dirty();
                }
            }

            // --- Wave 13: Comp + layer markers ---
            Action::AddCompMarker { time, label } => {
                let ci = self.active_comp_index();
                let mut m = crate::comp::Marker::at(time);
                m.label = label;
                self.project.comps[ci].markers.push(m);
            }
            Action::RemoveCompMarker(idx) => {
                let ci = self.active_comp_index();
                let markers = &mut self.project.comps[ci].markers;
                if idx < markers.len() {
                    markers.remove(idx);
                }
            }
            Action::AddLayerMarker { layer_idx, time, label } => {
                let ci = self.active_comp_index();
                if let Some(layer) = self.project.comps[ci].layers.get_mut(layer_idx) {
                    let mut m = crate::comp::Marker::at(time);
                    m.label = label;
                    layer.markers.push(m);
                }
            }
            Action::RemoveLayerMarker { layer_idx, idx } => {
                let ci = self.active_comp_index();
                if let Some(layer) = self.project.comps[ci].layers.get_mut(layer_idx) {
                    if idx < layer.markers.len() {
                        layer.markers.remove(idx);
                    }
                }
            }

            // --- Wave 13: Region of interest ---
            Action::SetROI(roi) => {
                self.roi = roi;
                self.host.mark_dirty();
            }

            // --- Wave 14 handlers ---

            Action::SetAnchorPoint { layer_idx, x, y } => {
                let ci = self.active_comp_index();
                if let Some(layer) = self.project.comps[ci].layers.get_mut(layer_idx) {
                    layer.anchor_x.set_key(0.0, x);
                    layer.anchor_y.set_key(0.0, y);
                    self.host.mark_dirty();
                }
            }

            Action::AddDisplacementMap { layer_idx, map_layer, scale_x, scale_y } => {
                let ci = self.active_comp_index();
                if let Some(layer) = self.project.comps[ci].layers.get_mut(layer_idx) {
                    layer.distort_effects.push(
                        crate::comp::DistortEffect::DisplacementMap {
                            map_layer,
                            scale_x,
                            scale_y,
                        }
                    );
                    self.host.mark_dirty();
                }
            }

            Action::SetDisplaceScale { layer_idx, effect_idx, scale_x, scale_y } => {
                let ci = self.active_comp_index();
                if let Some(layer) = self.project.comps[ci].layers.get_mut(layer_idx) {
                    if let Some(crate::comp::DistortEffect::DisplacementMap { scale_x: sx, scale_y: sy, .. })
                        = layer.distort_effects.get_mut(effect_idx)
                    {
                        *sx = scale_x;
                        *sy = scale_y;
                        self.host.mark_dirty();
                    }
                }
            }

            Action::AddExpressionControl(kind) => {
                let ci = self.active_comp_index();
                let name = kind.name().to_string();
                let mut layer = crate::comp::PulseLayer::of_kind(
                    crate::comp::LayerKind::ExpressionControl,
                    name,
                    [0.3, 0.5, 0.9, 1.0],
                );
                layer.visible = false; // expression controls don't render
                let idx = self.project.comps[ci].layers.len();
                self.project.comps[ci].layers.push(layer);
                self.expr_controls.insert(idx, ExprControl::default_for(kind));
            }

            Action::SetExprControlValue { layer_idx, value } => {
                if let Some(ctrl) = self.expr_controls.get_mut(&layer_idx) {
                    ctrl.value = value;
                }
            }

            Action::ExportProRes(path) => {
                self.prores_progress = Some("Checking FFmpeg…".into());
                let comp = self.project.comps[self.active_comp_index()].clone();
                let all_comps = self.project.comps.clone();
                let path_clone = path.clone();
                let (w, h) = (comp.width, comp.height);
                let fps = comp.fps;
                let dur = comp.duration;
                std::thread::spawn(move || {
                    let ffmpeg_ok = std::process::Command::new("ffmpeg")
                        .arg("-version").output().is_ok();
                    if !ffmpeg_ok {
                        // Write a marker file with the error
                        let _ = std::fs::write(
                            path_clone.with_extension("prores_error.txt"),
                            "ProRes export requires FFmpeg. Install via: brew install ffmpeg"
                        );
                        return;
                    }
                    use std::process::{Command, Stdio};
                    use std::io::Write;
                    let mut child = Command::new("ffmpeg")
                        .args([
                            "-y", "-f", "rawvideo", "-pix_fmt", "rgba",
                            "-s", &format!("{}x{}", w, h),
                            "-r", &fps.to_string(),
                            "-i", "pipe:0",
                            "-c:v", "prores_ks", "-profile:v", "3",
                        ])
                        .arg(path_clone.to_str().unwrap_or("output.mov"))
                        .stdin(Stdio::piped())
                        .spawn();
                    if let Ok(ref mut child) = child {
                        if let Some(stdin) = child.stdin.take() {
                            let mut stdin = stdin;
                            let mut cache = crate::comp::FrameCache::new();
                            let frame_count = (dur * fps).ceil() as u32;
                            for fi in 0..frame_count {
                                let t = fi as f32 / fps;
                                let frame = crate::render::render_preview_frame(
                                    &all_comps, comp.id, t, w.max(h), &mut cache
                                );
                                let _ = stdin.write_all(&frame.pixels);
                            }
                        }
                        let _ = child.wait();
                    }
                });
            }

            Action::ExportDnxHD(path) => {
                let comp = self.project.comps[self.active_comp_index()].clone();
                let all_comps = self.project.comps.clone();
                let (w, h) = (comp.width, comp.height);
                let fps = comp.fps;
                let dur = comp.duration;
                std::thread::spawn(move || {
                    let ffmpeg_ok = std::process::Command::new("ffmpeg")
                        .arg("-version").output().is_ok();
                    if !ffmpeg_ok {
                        let _ = std::fs::write(
                            path.with_extension("dnxhd_error.txt"),
                            "DNxHD export requires FFmpeg. Install via: brew install ffmpeg"
                        );
                        return;
                    }
                    use std::process::{Command, Stdio};
                    use std::io::Write;
                    let mut child = Command::new("ffmpeg")
                        .args([
                            "-y", "-f", "rawvideo", "-pix_fmt", "rgba",
                            "-s", &format!("{}x{}", w, h),
                            "-r", &fps.to_string(),
                            "-i", "pipe:0",
                            "-c:v", "dnxhd", "-b:v", "185M",
                        ])
                        .arg(path.to_str().unwrap_or("output.mxf"))
                        .stdin(Stdio::piped())
                        .spawn();
                    if let Ok(ref mut child) = child {
                        if let Some(stdin) = child.stdin.take() {
                            let mut stdin = stdin;
                            let mut cache = crate::comp::FrameCache::new();
                            let frame_count = (dur * fps).ceil() as u32;
                            for fi in 0..frame_count {
                                let t = fi as f32 / fps;
                                let frame = crate::render::render_preview_frame(
                                    &all_comps, comp.id, t, w.max(h), &mut cache
                                );
                                let _ = stdin.write_all(&frame.pixels);
                            }
                        }
                        let _ = child.wait();
                    }
                });
            }

            Action::SaveOutputPreset(name) => {
                let format = self.pending_export_format
                    .unwrap_or(export::OutputFormat::Png);
                self.output_presets.push(OutputPreset { name, format });
            }

            Action::LoadOutputPreset(idx) => {
                if let Some(preset) = self.output_presets.get(idx) {
                    self.pending_export_format = Some(preset.format);
                }
            }

            Action::DeleteOutputPreset(idx) => {
                if idx < self.output_presets.len() {
                    self.output_presets.remove(idx);
                }
            }

            // --- Batch 1: 3D Camera stub ---
            Action::SetCameraPosition(pos) => {
                let ci = self.active_comp_index();
                self.project.comps[ci].camera.position = pos;
                self.host.mark_dirty();
            }
            Action::SetCameraFov(fov) => {
                let ci = self.active_comp_index();
                self.project.comps[ci].camera.fov_deg = fov.clamp(1.0, 179.0);
                self.host.mark_dirty();
            }

            // --- Batch 1: Layer parenting (extended) ---
            Action::SetParentLayer { child, parent } => {
                let ci = self.active_comp_index();
                if child < self.project.comps[ci].layers.len() {
                    match parent {
                        Some(p) if p != child && p < self.project.comps[ci].layers.len() => {
                            self.project.comps[ci].layers[child].parent = Some(p);
                            self.layer_parents.insert(child, p);
                        }
                        _ => {
                            self.project.comps[ci].layers[child].parent = None;
                            self.layer_parents.remove(&child);
                        }
                    }
                    self.host.mark_dirty();
                }
            }

            // --- Batch 3: Time remap ---
            Action::SetTimeRemapEnabled { layer_id, enabled } => {
                let ci = self.active_comp_index();
                let dur = self.project.comps[ci].duration;
                if let Some(l) = self.project.comps[ci].layers.get_mut(layer_id) {
                    l.time_remap.enabled = enabled;
                    if enabled && l.time_remap.track.keys.is_empty() {
                        l.time_remap.seed_default(dur, None);
                    }
                    self.host.mark_dirty();
                }
            }
            Action::SetTimeRemapKey { layer_id, comp_time, source_time } => {
                let ci = self.active_comp_index();
                if let Some(l) = self.project.comps[ci].layers.get_mut(layer_id) {
                    l.time_remap.track.set_key(comp_time as f32, source_time as f32);
                    self.host.mark_dirty();
                }
            }

            // --- Batch 3: Track matte ---
            Action::SetLayerMatte { layer_id, mode } => {
                let ci = self.active_comp_index();
                if let Some(l) = self.project.comps[ci].layers.get_mut(layer_id) {
                    l.matte = mode;
                    self.host.mark_dirty();
                }
            }

            // --- Batch 3: Puppet pins ---
            Action::AddPuppetPin { layer_id, pos } => {
                let ci = self.active_comp_index();
                if let Some(l) = self.project.comps[ci].layers.get_mut(layer_id) {
                    let id = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_nanos() as u64)
                        .unwrap_or(l.puppet_pins.len() as u64);
                    l.puppet_pins.push(crate::comp::PuppetPin { id, position: pos, is_stiff: false });
                    self.host.mark_dirty();
                }
            }
            Action::MovePuppetPin { layer_id, pin_id, pos } => {
                let ci = self.active_comp_index();
                if let Some(l) = self.project.comps[ci].layers.get_mut(layer_id) {
                    if let Some(pin) = l.puppet_pins.iter_mut().find(|p| p.id == pin_id) {
                        pin.position = pos;
                        self.host.mark_dirty();
                    }
                }
            }
            Action::RemovePuppetPin { layer_id, pin_id } => {
                let ci = self.active_comp_index();
                if let Some(l) = self.project.comps[ci].layers.get_mut(layer_id) {
                    l.puppet_pins.retain(|p| p.id != pin_id);
                    self.host.mark_dirty();
                }
            }

            // --- Batch 3: Solo / Shy ---
            Action::ToggleSolo(i) => {
                let ci = self.active_comp_index();
                if let Some(l) = self.project.comps[ci].layers.get_mut(i) {
                    l.solo = !l.solo;
                    self.host.mark_dirty();
                }
            }
            Action::ToggleShy(i) => {
                let ci = self.active_comp_index();
                if let Some(l) = self.project.comps[ci].layers.get_mut(i) {
                    l.shy = !l.shy;
                }
            }
            Action::ToggleHideShy => {
                let ci = self.active_comp_index();
                self.project.comps[ci].hide_shy = !self.project.comps[ci].hide_shy;
            }

            // --- Batch 4: 3D Lights ---
            Action::AddLight(light) => {
                let ci = self.active_comp_index();
                self.project.comps[ci].lights.push(light);
                self.host.mark_dirty();
            }
            Action::RemoveLight(i) => {
                let ci = self.active_comp_index();
                if i < self.project.comps[ci].lights.len() {
                    self.project.comps[ci].lights.remove(i);
                    self.host.mark_dirty();
                }
            }
            Action::UpdateLight { index, light } => {
                let ci = self.active_comp_index();
                if let Some(l) = self.project.comps[ci].lights.get_mut(index) {
                    *l = light;
                    self.host.mark_dirty();
                }
            }

            // --- Batch 4: Multi-comp render queue ---
            Action::AddAllCompsToQueue => {
                for comp in &self.project.comps {
                    let name = if comp.name.is_empty() {
                        format!("Comp {}", comp.id)
                    } else {
                        comp.name.clone()
                    };
                    let ext = RenderFormat::Mp4H264.extension();
                    let path = std::path::PathBuf::from(format!(
                        "/tmp/pulse_render_{}.{}", name.replace(' ', "_"), ext
                    ));
                    self.render_queue.push(RenderJob {
                        comp_name: name,
                        output_path: path,
                        format: RenderFormat::Mp4H264,
                        status: RenderJobStatus::Pending,
                    });
                }
            }

            // --- Batch 4: Live preview output ---
            Action::ToggleLiveOutput => {
                self.live_output_enabled = !self.live_output_enabled;
            }

            // --- Batch 5: Comp motion blur (shutter) ---
            Action::SetMotionBlurEnabled(on) => {
                let ci = self.active_comp_index();
                self.project.comps[ci].motion_blur.enabled = on;
                self.host.mark_dirty();
            }
            Action::SetMotionBlurAngle(a) => {
                let ci = self.active_comp_index();
                self.project.comps[ci].motion_blur.angle = a.clamp(1.0, 720.0);
                self.host.mark_dirty();
            }
            Action::SetMotionBlurPhase(p) => {
                let ci = self.active_comp_index();
                self.project.comps[ci].motion_blur.phase = p.clamp(-360.0, 360.0);
                self.host.mark_dirty();
            }
            Action::SetMotionBlurSamples(n) => {
                let ci = self.active_comp_index();
                self.project.comps[ci].motion_blur.samples = n.clamp(1, 64);
                self.host.mark_dirty();
            }
            Action::ToggleLayerMotionBlur(i) => {
                let ci = self.active_comp_index();
                if let Some(l) = self.project.comps[ci].layers.get_mut(i) {
                    l.motion_blur = !l.motion_blur;
                    self.host.mark_dirty();
                }
            }

            // --- Batch 6: Text Animator ---
            Action::AddTextAnimator(id) => {
                let ci = self.active_comp_index();
                if let Some(l) = self.project.comps[ci].layers.get_mut(id) {
                    l.text_animator = Some(crate::comp::TextAnimator::default());
                    self.host.mark_dirty();
                }
            }
            Action::RemoveTextAnimator(id) => {
                let ci = self.active_comp_index();
                if let Some(l) = self.project.comps[ci].layers.get_mut(id) {
                    l.text_animator = None;
                    self.host.mark_dirty();
                }
            }
            Action::SetTextAnimatorRange { layer_id, start, end } => {
                let ci = self.active_comp_index();
                if let Some(l) = self.project.comps[ci].layers.get_mut(layer_id) {
                    if let Some(ta) = &mut l.text_animator {
                        ta.range_start = start.clamp(0.0, 1.0);
                        ta.range_end = end.clamp(0.0, 1.0);
                        self.host.mark_dirty();
                    }
                }
            }
            Action::SetTextAnimatorOffsetX { layer_id, value } => {
                let ci = self.active_comp_index();
                if let Some(l) = self.project.comps[ci].layers.get_mut(layer_id) {
                    if let Some(ta) = &mut l.text_animator {
                        ta.offset_x = value;
                        self.host.mark_dirty();
                    }
                }
            }
            Action::SetTextAnimatorOffsetY { layer_id, value } => {
                let ci = self.active_comp_index();
                if let Some(l) = self.project.comps[ci].layers.get_mut(layer_id) {
                    if let Some(ta) = &mut l.text_animator {
                        ta.offset_y = value;
                        self.host.mark_dirty();
                    }
                }
            }
            Action::SetTextAnimatorRotation { layer_id, value } => {
                let ci = self.active_comp_index();
                if let Some(l) = self.project.comps[ci].layers.get_mut(layer_id) {
                    if let Some(ta) = &mut l.text_animator {
                        ta.rotation_deg = value;
                        self.host.mark_dirty();
                    }
                }
            }
            Action::SetTextAnimatorScale { layer_id, value } => {
                let ci = self.active_comp_index();
                if let Some(l) = self.project.comps[ci].layers.get_mut(layer_id) {
                    if let Some(ta) = &mut l.text_animator {
                        ta.scale = value;
                        self.host.mark_dirty();
                    }
                }
            }
            Action::SetTextAnimatorOpacity { layer_id, value } => {
                let ci = self.active_comp_index();
                if let Some(l) = self.project.comps[ci].layers.get_mut(layer_id) {
                    if let Some(ta) = &mut l.text_animator {
                        ta.opacity = value;
                        self.host.mark_dirty();
                    }
                }
            }

            // --- Batch 6: Trim Paths ---
            Action::SetShapeTrimPaths { layer_id, start, end, offset } => {
                let ci = self.active_comp_index();
                if let Some(l) = self.project.comps[ci].layers.get_mut(layer_id) {
                    l.shape.trim_paths = Some(crate::comp::TrimPaths {
                        start: start.clamp(0.0, 1.0),
                        end: end.clamp(0.0, 1.0),
                        offset: offset.clamp(0.0, 1.0),
                    });
                    self.host.mark_dirty();
                }
            }
            Action::ClearShapeTrimPaths(id) => {
                let ci = self.active_comp_index();
                if let Some(l) = self.project.comps[ci].layers.get_mut(id) {
                    l.shape.trim_paths = None;
                    self.host.mark_dirty();
                }
            }

            // --- Batch 6: Shape Repeater ---
            Action::AddShapeRepeater(id) => {
                let ci = self.active_comp_index();
                if let Some(l) = self.project.comps[ci].layers.get_mut(id) {
                    l.shape.repeater = Some(crate::comp::ShapeRepeater::default());
                    self.host.mark_dirty();
                }
            }
            Action::RemoveShapeRepeater(id) => {
                let ci = self.active_comp_index();
                if let Some(l) = self.project.comps[ci].layers.get_mut(id) {
                    l.shape.repeater = None;
                    self.host.mark_dirty();
                }
            }
            Action::SetRepeaterCopies { layer_id, copies } => {
                let ci = self.active_comp_index();
                if let Some(l) = self.project.comps[ci].layers.get_mut(layer_id) {
                    if let Some(r) = &mut l.shape.repeater {
                        r.copies = copies.max(1);
                        self.host.mark_dirty();
                    }
                }
            }
            Action::SetRepeaterOffset { layer_id, x, y } => {
                let ci = self.active_comp_index();
                if let Some(l) = self.project.comps[ci].layers.get_mut(layer_id) {
                    if let Some(r) = &mut l.shape.repeater {
                        r.offset_x = x;
                        r.offset_y = y;
                        self.host.mark_dirty();
                    }
                }
            }
            Action::SetRepeaterRotation { layer_id, deg } => {
                let ci = self.active_comp_index();
                if let Some(l) = self.project.comps[ci].layers.get_mut(layer_id) {
                    if let Some(r) = &mut l.shape.repeater {
                        r.rotation_deg = deg;
                        self.host.mark_dirty();
                    }
                }
            }
            Action::SetRepeaterScale { layer_id, scale } => {
                let ci = self.active_comp_index();
                if let Some(l) = self.project.comps[ci].layers.get_mut(layer_id) {
                    if let Some(r) = &mut l.shape.repeater {
                        r.scale = scale;
                        self.host.mark_dirty();
                    }
                }
            }
            Action::SetRepeaterOpacity { layer_id, start, end } => {
                let ci = self.active_comp_index();
                if let Some(l) = self.project.comps[ci].layers.get_mut(layer_id) {
                    if let Some(r) = &mut l.shape.repeater {
                        r.opacity_start = start.clamp(0.0, 1.0);
                        r.opacity_end = end.clamp(0.0, 1.0);
                        self.host.mark_dirty();
                    }
                }
            }

            // --- Batch 2: Layer split ---
            Action::SplitLayer(id) => {
                let t = self.time;
                self.apply(Action::SplitLayerAt { layer_id: id, time: t });
            }
            Action::SplitLayerAt { layer_id, time } => {
                let ci = self.active_comp_index();
                let dur = self.project.comps[ci].duration;
                let t = time.clamp(0.0, dur);
                if layer_id >= self.project.comps[ci].layers.len() {
                    return;
                }
                let mut second = self.project.comps[ci].layers[layer_id].clone();
                // First layer: visible up to `t`.
                self.project.comps[ci].layers[layer_id].out_point = Some(t);
                // Second layer: starts at `t`, inherits the original's out_point.
                second.in_point = Some(t);
                // Shift keyframes in the second layer: subtract `t` from every key time
                // so the layer plays from its beginning at `t`.
                for track in [
                    &mut second.x, &mut second.y, &mut second.scale,
                    &mut second.rotation, &mut second.opacity,
                    &mut second.anchor_x, &mut second.anchor_y,
                    &mut second.z, &mut second.orient_x, &mut second.orient_y, &mut second.orient_z,
                ] {
                    for key in &mut track.keys {
                        key.t = (key.t - t).max(0.0);
                    }
                }
                self.project.comps[ci].layers.insert(layer_id + 1, second);
                self.host.mark_dirty();
            }

            // --- Batch 2: Brainstorm ---
            Action::ToggleBrainstorm => {
                self.brainstorm.open = !self.brainstorm.open;
            }
            Action::GenerateBrainstormVariations { count } => {
                self.brainstorm.variations.clear();
                let ci = self.active_comp_index();
                let n_layers = self.project.comps[ci].layers.len();
                for vi in 0..count {
                    let mut overrides = Vec::new();
                    // Pick 2–4 (layer, prop) pairs using a deterministic seed.
                    let n_overrides = 2 + (vi % 3) as usize;
                    let props = [
                        Prop::X, Prop::Y, Prop::Scale, Prop::Rotation, Prop::Opacity,
                    ];
                    for oi in 0..n_overrides {
                        let seed = vi * 1234 + oi as u32 * 37;
                        let layer_idx = (seed as usize) % n_layers.max(1);
                        let prop = props[(seed as usize / n_layers.max(1)) % props.len()];
                        let cur = self.project.comps[ci].layer_value(layer_idx, prop, self.time);
                        let scale_factor = 0.5 + (seed % 100) as f32 / 100.0;
                        let new_val = cur * scale_factor;
                        overrides.push((layer_idx, prop, new_val));
                    }
                    self.brainstorm.variations.push(BrainstormVariation {
                        label: format!("Variation {}", vi + 1),
                        overrides,
                        selected: false,
                    });
                }
            }
            Action::SelectBrainstormVariation(idx) => {
                for (i, v) in self.brainstorm.variations.iter_mut().enumerate() {
                    v.selected = i == idx;
                }
            }
            Action::ApplyBrainstormVariation(idx) => {
                if idx >= self.brainstorm.variations.len() {
                    return;
                }
                let overrides = self.brainstorm.variations[idx].overrides.clone();
                let t = self.time;
                let ci = self.active_comp_index();
                for (layer_idx, prop, value) in overrides {
                    if let Some(layer) = self.project.comps[ci].layers.get_mut(layer_idx) {
                        layer.track_mut(prop).set_key(t, value);
                    }
                }
                self.brainstorm.variations.clear();
                self.brainstorm.open = false;
                self.host.mark_dirty();
            }
            Action::SetBrainstormGrid { cols, rows } => {
                self.brainstorm.grid_cols = cols.max(1);
                self.brainstorm.grid_rows = rows.max(1);
            }

            // --- Batch 2: Color Finesse ---
            Action::AddColorFinesse(layer_id) => {
                let ci = self.active_comp_index();
                if let Some(l) = self.project.comps[ci].layers.get_mut(layer_id) {
                    if l.color_finesse.is_none() {
                        l.color_finesse = Some(crate::comp::ColorFinesse::default());
                        self.host.mark_dirty();
                    }
                }
            }
            Action::RemoveColorFinesse(layer_id) => {
                let ci = self.active_comp_index();
                if let Some(l) = self.project.comps[ci].layers.get_mut(layer_id) {
                    l.color_finesse = None;
                    self.host.mark_dirty();
                }
            }
            Action::SetColorFinesseEnabled { layer_id, enabled } => {
                let ci = self.active_comp_index();
                if let Some(cf) = self.project.comps[ci]
                    .layers.get_mut(layer_id)
                    .and_then(|l| l.color_finesse.as_mut())
                {
                    cf.enabled = enabled;
                    self.host.mark_dirty();
                }
            }
            Action::SetColorFinesseParam { layer_id, range, prop, value } => {
                let ci = self.active_comp_index();
                if let Some(cf) = self.project.comps[ci]
                    .layers.get_mut(layer_id)
                    .and_then(|l| l.color_finesse.as_mut())
                {
                    let r = match range {
                        "master"   => &mut cf.master,
                        "reds"     => &mut cf.reds,
                        "yellows"  => &mut cf.yellows,
                        "greens"   => &mut cf.greens,
                        "cyans"    => &mut cf.cyans,
                        "blues"    => &mut cf.blues,
                        "magentas" => &mut cf.magentas,
                        _          => return,
                    };
                    match prop {
                        "hue"        => r.hue_shift = value,
                        "saturation" => r.saturation = value,
                        "lightness"  => r.lightness = value,
                        _            => return,
                    }
                    self.host.mark_dirty();
                }
            }
            Action::ResetColorFinesse(layer_id) => {
                let ci = self.active_comp_index();
                if let Some(l) = self.project.comps[ci].layers.get_mut(layer_id) {
                    if l.color_finesse.is_some() {
                        l.color_finesse = Some(crate::comp::ColorFinesse::default());
                        self.host.mark_dirty();
                    }
                }
            }

            // --- Batch 6: Lumetri Color ---
            Action::AddLumetriColor(id) => {
                let ci = self.active_comp_index();
                if let Some(l) = self.project.comps[ci].layers.get_mut(id) {
                    l.lumetri = Some(crate::comp::LumetriColor::default());
                    self.host.mark_dirty();
                }
            }
            Action::RemoveLumetriColor(id) => {
                let ci = self.active_comp_index();
                if let Some(l) = self.project.comps[ci].layers.get_mut(id) {
                    l.lumetri = None;
                    self.host.mark_dirty();
                }
            }
            Action::SetLumetriParam { layer_id, param, value } => {
                let ci = self.active_comp_index();
                if let Some(l) = self.project.comps[ci].layers.get_mut(layer_id) {
                    if let Some(lc) = &mut l.lumetri {
                        match param {
                            "exposure"    => lc.exposure = value,
                            "contrast"    => lc.contrast = value,
                            "highlights"  => lc.highlights = value,
                            "shadows"     => lc.shadows = value,
                            "whites"      => lc.whites = value,
                            "blacks"      => lc.blacks = value,
                            "temperature" => lc.temperature = value,
                            "tint"        => lc.tint = value,
                            "saturation"  => lc.saturation = value,
                            "vibrance"    => lc.vibrance = value,
                            _             => {}
                        }
                        self.host.mark_dirty();
                    }
                }
            }
            Action::ToggleLumetriEnabled(id) => {
                let ci = self.active_comp_index();
                if let Some(l) = self.project.comps[ci].layers.get_mut(id) {
                    if let Some(lc) = &mut l.lumetri {
                        lc.enabled = !lc.enabled;
                        self.host.mark_dirty();
                    }
                }
            }
            Action::ResetLumetriColor(id) => {
                let ci = self.active_comp_index();
                if let Some(l) = self.project.comps[ci].layers.get_mut(id) {
                    if l.lumetri.is_some() {
                        l.lumetri = Some(crate::comp::LumetriColor::default());
                        self.host.mark_dirty();
                    }
                }
            }

            // --- Batch 6: Essential Graphics / MoGrt ---
            Action::ToggleMoGrtPanel => {
                self.mogrt_panel_open = !self.mogrt_panel_open;
            }
            Action::AddMoGrtTemplate(name) => {
                self.mogrt_templates.push(MotionGraphicTemplate {
                    name,
                    controls: Vec::new(),
                });
            }
            Action::RemoveMoGrtTemplate(idx) => {
                if idx < self.mogrt_templates.len() {
                    self.mogrt_templates.remove(idx);
                    if let Some(sel) = self.mogrt_selected {
                        if sel >= self.mogrt_templates.len() {
                            self.mogrt_selected = self.mogrt_templates.len().checked_sub(1);
                        }
                    }
                }
            }
            Action::SelectMoGrtTemplate(idx) => {
                if idx < self.mogrt_templates.len() {
                    self.mogrt_selected = Some(idx);
                }
            }
            Action::AddMoGrtControl { template_idx, control } => {
                if let Some(t) = self.mogrt_templates.get_mut(template_idx) {
                    t.controls.push(control);
                }
            }
            Action::RemoveMoGrtControl { template_idx, control_idx } => {
                if let Some(t) = self.mogrt_templates.get_mut(template_idx) {
                    if control_idx < t.controls.len() {
                        t.controls.remove(control_idx);
                    }
                }
            }
            Action::SetMoGrtTextValue { template_idx, control_idx, value } => {
                if let Some(t) = self.mogrt_templates.get_mut(template_idx) {
                    if let Some(MoGrtControl::Text { value: v, .. }) = t.controls.get_mut(control_idx) {
                        *v = value;
                    }
                }
            }
            Action::SetMoGrtColorValue { template_idx, control_idx, value } => {
                if let Some(t) = self.mogrt_templates.get_mut(template_idx) {
                    if let Some(MoGrtControl::Color { value: v, .. }) = t.controls.get_mut(control_idx) {
                        *v = value;
                    }
                }
            }
            Action::SetMoGrtSliderValue { template_idx, control_idx, value } => {
                if let Some(t) = self.mogrt_templates.get_mut(template_idx) {
                    if let Some(MoGrtControl::Slider { value: v, min, max, .. }) = t.controls.get_mut(control_idx) {
                        *v = value.clamp(*min, *max);
                    }
                }
            }
            Action::ExportMoGrt { template_idx, path } => {
                if let Some(t) = self.mogrt_templates.get(template_idx) {
                    // Serialize template to a simple JSON format.
                    let mut controls_json = Vec::new();
                    for c in &t.controls {
                        let entry = match c {
                            MoGrtControl::Text { label, value } => {
                                format!(r#"{{"type":"text","label":{:?},"value":{:?}}}"#, label, value)
                            }
                            MoGrtControl::Color { label, value } => {
                                format!(r#"{{"type":"color","label":{:?},"value":[{},{},{},{}]}}"#,
                                    label, value[0], value[1], value[2], value[3])
                            }
                            MoGrtControl::Slider { label, value, min, max } => {
                                format!(r#"{{"type":"slider","label":{:?},"value":{},"min":{},"max":{}}}"#,
                                    label, value, min, max)
                            }
                        };
                        controls_json.push(entry);
                    }
                    let json = format!(
                        r#"{{"name":{:?},"controls":[{}]}}"#,
                        t.name,
                        controls_json.join(",")
                    );
                    let _ = std::fs::write(path, json);
                }
            }

            // --- Batch 2: Pre-render cache ---
            Action::StartPreRender => {
                let ci = self.active_comp_index();
                let wa = self.project.comps[ci].clamped_work_area();
                let fps = self.project.comps[ci].fps;
                let total = ((wa.end - wa.start) * fps).ceil() as u32;
                self.pre_render_status = PreRenderStatus::Rendering {
                    frames_done: 0,
                    total,
                };
            }
            Action::CancelPreRender => {
                self.pre_render_status = PreRenderStatus::NotStarted;
            }
            Action::SetPreRenderProgress { frames_done, total } => {
                self.pre_render_status = PreRenderStatus::Rendering { frames_done, total };
            }
            Action::PreRenderComplete { frame_count, cache_dir } => {
                self.pre_render_cache_dir = Some(cache_dir.clone());
                self.pre_render_status = PreRenderStatus::Done { frame_count, cache_dir };
            }
            Action::PreRenderFailed(msg) => {
                self.pre_render_status = PreRenderStatus::Failed(msg);
            }
            Action::ClearPreRenderCache => {
                self.pre_render_status = PreRenderStatus::NotStarted;
                self.pre_render_cache_dir = None;
                self.use_pre_render = false;
            }
            Action::ToggleUsePreRender => {
                self.use_pre_render = !self.use_pre_render;
            }

            // --- Batch 2: Track Camera ---
            Action::ToggleCameraTracker => {
                self.camera_tracker.open = !self.camera_tracker.open;
            }
            Action::AddTrackPoint { name, pos } => {
                self.camera_tracker.track_points.push(TrackPoint {
                    name,
                    position: pos,
                    keyframes: vec![(0.0, pos)],
                });
            }
            Action::RemoveTrackPoint(idx) => {
                if idx < self.camera_tracker.track_points.len() {
                    self.camera_tracker.track_points.remove(idx);
                    self.camera_tracker.solved = false;
                }
            }
            Action::MoveTrackPoint { idx, time, pos } => {
                if let Some(pt) = self.camera_tracker.track_points.get_mut(idx) {
                    // Overwrite or insert keyframe at `time`.
                    if let Some(kf) = pt.keyframes.iter_mut().find(|(t, _)| (*t - time).abs() < 1e-4) {
                        kf.1 = pos;
                    } else {
                        pt.keyframes.push((time, pos));
                        pt.keyframes.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
                    }
                }
            }
            Action::SolveCameraTrack => {
                let pts = &self.camera_tracker.track_points;
                if pts.len() < 2 || pts.iter().any(|p| p.keyframes.len() < 2) {
                    return;
                }
                // Gather all unique times from all track points.
                let mut times: Vec<f32> = pts.iter()
                    .flat_map(|p| p.keyframes.iter().map(|(t, _)| *t))
                    .collect();
                times.sort_by(|a, b| a.partial_cmp(b).unwrap());
                times.dedup();

                let mut camera_keyframes = Vec::new();
                let ref_positions: Vec<[f32; 2]> = pts.iter()
                    .map(|p| p.keyframes[0].1)
                    .collect();

                for t in &times {
                    // Interpolate each track point's position at this time.
                    let cur_positions: Vec<[f32; 2]> = pts.iter().map(|p| {
                        // Linear interpolation between bracketing keyframes.
                        let kfs = &p.keyframes;
                        let pos = if *t <= kfs[0].0 {
                            kfs[0].1
                        } else if *t >= kfs[kfs.len()-1].0 {
                            kfs[kfs.len()-1].1
                        } else {
                            let j = kfs.partition_point(|(kt, _)| *kt < *t);
                            let (t0, p0) = kfs[j-1];
                            let (t1, p1) = kfs[j];
                            let frac = if (t1 - t0).abs() < 1e-6 { 0.0 } else { (*t - t0) / (t1 - t0) };
                            [p0[0] + (p1[0] - p0[0]) * frac, p0[1] + (p1[1] - p0[1]) * frac]
                        };
                        pos
                    }).collect();

                    // Translation: negative of average displacement.
                    let n = cur_positions.len() as f32;
                    let tx = -cur_positions.iter().zip(ref_positions.iter())
                        .map(|(c, r)| c[0] - r[0]).sum::<f32>() / n;
                    let ty = -cur_positions.iter().zip(ref_positions.iter())
                        .map(|(c, r)| c[1] - r[1]).sum::<f32>() / n;

                    // Scale from distance change between first two points.
                    let d_ref = {
                        let dx = ref_positions[1][0] - ref_positions[0][0];
                        let dy = ref_positions[1][1] - ref_positions[0][1];
                        (dx*dx + dy*dy).sqrt()
                    };
                    let d_cur = {
                        let dx = cur_positions[1][0] - cur_positions[0][0];
                        let dy = cur_positions[1][1] - cur_positions[0][1];
                        (dx*dx + dy*dy).sqrt()
                    };
                    let scale = if d_ref < 1e-6 { 1.0 } else { d_ref / d_cur };
                    camera_keyframes.push((*t, [tx, ty, scale]));
                }
                self.camera_tracker.camera_keyframes = camera_keyframes;
                self.camera_tracker.solved = true;
            }
            Action::CreateCameraFromTrack => {
                if !self.camera_tracker.solved {
                    return;
                }
                // Apply each solved keyframe to the comp camera position.
                for (t, [tx, ty, _scale]) in &self.camera_tracker.camera_keyframes {
                    let ci = self.active_comp_index();
                    let cur_z = self.project.comps[ci].camera.position[2];
                    // Snapshot undo once before the loop by doing it inline.
                    self.project.comps[ci].camera.position = [*tx, *ty, cur_z];
                    let _ = t; // timeline keying not done here — model uses single camera pos
                }
                self.host.mark_dirty();
            }
            Action::SetCameraTrackerProgress(p) => {
                self.camera_tracker.analyze_progress = p.clamp(0.0, 1.0);
            }
            Action::ClearCameraTrack => {
                self.camera_tracker = CameraTracker::default();
            }
        }
    }

    /// Push the current project onto the undo stack (capped at [`UNDO_LIMIT`])
    /// and clear the redo stack — the standard pre-edit snapshot for linear
    /// undo. Called by [`apply`](Self::apply) before any document-mutating
    /// action.
    fn push_undo(&mut self) {
        self.undo.push(self.project.clone());
        if self.undo.len() > UNDO_LIMIT {
            // Drop the oldest snapshot to bound memory.
            self.undo.remove(0);
        }
        self.redo.clear();
    }

    /// Re-establish invariants after an undo/redo swapped the whole project:
    /// re-render the preview, clamp the playhead into the (possibly different)
    /// active comp, and drop a stale selection / in-flight keyframe drag that
    /// the restored project may not have.
    fn after_history_swap(&mut self) {
        self.host.mark_dirty();
        let dur = self.active_duration();
        self.time = self.time.clamp(0.0, dur);
        // Clamp / drop a now-out-of-range selection.
        let ci = self.active_comp_index();
        let n = self.project.comps[ci].layers.len();
        if let Some(sel) = self.selected_layer {
            if sel >= n {
                self.selected_layer = None;
            }
        }
        self.kf_drag = None;
        self.wa_drag = None;
        self.gizmo_drag = None;
        self.graph_grab = None;
        self.hovered_gizmo_handle = None;
    }

    /// Advance the play loop by the real wall-clock time elapsed since the last
    /// tick, looping back to the comp start when the playhead reaches the end.
    /// Called once per animation frame by the root view while `playing`; a no-op
    /// when paused. Returns `true` if the playhead moved (frame went dirty).
    ///
    /// This is the GPUI analog of the egui app's per-frame `if self.playing { … }`
    /// block: the root view schedules the next frame via
    /// `window.request_animation_frame()`, and this advances `time` on each.
    pub fn tick(&mut self) -> bool {
        if !self.playing {
            return false;
        }
        let now = Instant::now();
        let dt = match self.last_tick {
            // Clamp the step so a stalled frame (e.g. window occluded) can't
            // jump the playhead by a huge gap; mirrors the egui app's `.min(0.1)`.
            Some(prev) => now.duration_since(prev).as_secs_f32().min(0.1),
            None => 0.0,
        };
        self.last_tick = Some(now);

        let dur = self.active_duration().max(1e-3);
        // Playback loops within the **work area** (After Effects' RAM-preview
        // range), mirroring the egui app: advance by `dt`, and when the playhead
        // reaches the work-area end (or sits outside it), wrap back to the
        // work-area start. A full work area degrades to looping the whole
        // `[0, duration]` timeline.
        let wa = self.active_work_area();
        let lo = wa.start;
        let hi = wa.end.max(lo + 1e-3).min(dur);
        let mut next = self.time + dt;
        if next >= hi || self.time < lo {
            next = lo;
        }
        self.time = next;
        // Only re-render when the playhead crosses to a new frame (comp fps).
        let ci = self.active_comp_index();
        let fps = self.project.comps[ci].fps.max(1.0);
        let new_frame = (next * fps).round() as u32;
        if new_frame != self.last_rendered_frame {
            self.last_rendered_frame = new_frame;
            self.host.mark_dirty();
        }
        true
    }

    /// The active comp's clamped work area (in/out range, ordered + inside the
    /// timeline). Used by the play loop (loop bounds) and the timeline overlay.
    pub fn active_work_area(&self) -> WorkArea {
        let ci = self.active_comp_index();
        self.project.comps[ci].clamped_work_area()
    }

    /// Map a pointer x (window-relative, px) over the scrub track to a comp time
    /// in seconds, clamped to `[0, duration]`. Returns `None` if the track's
    /// bounds haven't been painted yet this session.
    pub fn time_for_x(&self, x: f32) -> Option<f32> {
        let bounds = self.track_bounds.get()?;
        let left = f32::from(bounds.origin.x);
        let width = f32::from(bounds.size.width).max(1.0);
        let frac = ((x - left) / width).clamp(0.0, 1.0);
        Some(frac * self.active_duration())
    }

    /// Duration (seconds) of the active comp, for clamping the playhead.
    fn active_duration(&self) -> f32 {
        let ci = self.active_comp_index();
        self.project.comps[ci].duration
    }

    /// The preview's comp-space → screen mapping: the on-screen comp **center**
    /// (window-relative px) and the comp-pixels → screen **scale**, derived from
    /// the painted preview image bounds and the active comp's full size. Comp
    /// space has its origin at the comp center with `+y` downward — exactly what
    /// `world.apply` outputs — so the gizmo geometry maps straight onto the
    /// rendered pixels (through the host's preview-resolution cap). Returns `None`
    /// until the preview image has been painted at least once.
    pub fn preview_fit(&self) -> Option<((f32, f32), f32)> {
        let b = self.preview_rect.get()?;
        let cx = f32::from(b.origin.x) + f32::from(b.size.width) * 0.5;
        let cy = f32::from(b.origin.y) + f32::from(b.size.height) * 0.5;
        let ci = self.active_comp_index();
        let comp_w = self.project.comps[ci].width.max(1) as f32;
        // The image is drawn at preview_w/preview_h (the capped render size) and
        // letterboxed by the comp aspect; the comp→screen scale is the painted
        // width over the comp's full width.
        let scale = (f32::from(b.size.width) / comp_w).max(1e-6);
        Some(((cx, cy), scale))
    }

    /// Map a window-relative pointer position to comp space (origin at the comp
    /// center, `+y` down), using [`preview_fit`](Self::preview_fit). `None` until
    /// the preview has painted.
    pub fn pointer_to_comp(&self, sx: f32, sy: f32) -> Option<(f32, f32)> {
        let ((cx, cy), scale) = self.preview_fit()?;
        Some(gizmo::screen_to_comp(sx, sy, cx, cy, scale))
    }

    /// The topmost visible layer whose base quad contains a window-relative
    /// pointer, mapped to comp space. Used by the preview's click-to-select:
    /// layers are tested front-to-back (the layer list is back-to-front, so we
    /// iterate in reverse) and the first hit wins. `None` when the pointer is over
    /// empty canvas (or the preview hasn't painted yet).
    pub fn layer_at_pointer(&self, sx: f32, sy: f32) -> Option<usize> {
        let pc = self.pointer_to_comp(sx, sy)?;
        let ci = self.active_comp_index();
        let comp = &self.project.comps[ci];
        let half_w = comp.width as f32 * gizmo::LAYER_HALF_FRAC;
        let half_h = comp.height as f32 * gizmo::LAYER_HALF_FRAC;
        let local = [
            (-half_w, -half_h),
            (half_w, -half_h),
            (half_w, half_h),
            (-half_w, half_h),
        ];
        for i in (0..comp.layers.len()).rev() {
            if !comp.layers[i].visible {
                continue;
            }
            let world = comp.world_matrix(i, self.time);
            let corners: [(f32, f32); 4] = local.map(|(lx, ly)| world.apply(lx, ly));
            if point_in_quad(pc, &corners) {
                return Some(i);
            }
        }
        None
    }

    /// Build the transform-gizmo geometry for the selected layer at the playhead,
    /// or `None` when nothing is selected (or the layer is gone). Drives both the
    /// preview overlay paint and the pointer hit-test.
    pub fn selected_gizmo(&self) -> Option<GizmoGeom> {
        let i = self.selected_layer?;
        let ci = self.active_comp_index();
        GizmoGeom::build(&self.project.comps[ci], i, self.time)
    }

    /// The gizmo handle under a window-relative pointer (back-projected to comp
    /// space), or `None`. `tol_px` is the screen-space hit radius; it's converted
    /// to comp px through the current fit so the target stays ~constant on screen.
    pub fn gizmo_hit(&self, sx: f32, sy: f32, tol_px: f32) -> Option<GizmoHandle> {
        let geom = self.selected_gizmo()?;
        let ((_, _), scale) = self.preview_fit()?;
        let pc = self.pointer_to_comp(sx, sy)?;
        gizmo::hit_test(&geom, pc, tol_px / scale.max(1e-6))
    }

    /// Begin a gizmo drag from a window-relative pointer press, if it lands on a
    /// handle of the selected layer's gizmo. Captures the grab-time transform and
    /// parent matrix so the per-frame drag math is relative to the grab. Returns
    /// `true` if a drag was armed.
    pub fn begin_gizmo_drag(&mut self, sx: f32, sy: f32, tol_px: f32) -> bool {
        let Some(i) = self.selected_layer else {
            return false;
        };
        let Some(handle) = self.gizmo_hit(sx, sy, tol_px) else {
            return false;
        };
        let Some(start_comp) = self.pointer_to_comp(sx, sy) else {
            return false;
        };
        let ci = self.active_comp_index();
        let t = self.time;
        let comp = &self.project.comps[ci];
        self.gizmo_drag = Some(GizmoDrag {
            layer: i,
            handle,
            time: t,
            start_tf: comp.layers[i].transform(t),
            parent: gizmo::parent_matrix(comp, i, t),
            start_comp,
        });
        true
    }

    /// Continue an in-flight gizmo drag against the live (window-relative) pointer:
    /// recompute the property deltas in parent-local space and key the changed
    /// transform properties at the grab time (via an undoable [`Action::GizmoKeys`]).
    /// A no-op when no drag is armed or the pointer can't be mapped yet.
    pub fn update_gizmo_drag(&mut self, sx: f32, sy: f32) {
        let Some(drag) = self.gizmo_drag else { return };
        // The drag is bound to the layer grabbed; if the selection changed out
        // from under it (shouldn't happen mid-drag), do nothing.
        if self.selected_layer != Some(drag.layer) {
            return;
        }
        let Some(cur_comp) = self.pointer_to_comp(sx, sy) else {
            return;
        };
        let result = gizmo::drag(
            drag.handle,
            drag.start_tf,
            drag.parent,
            drag.start_comp,
            cur_comp,
        );
        let keys = result.keys();
        if !keys.is_empty() {
            self.apply(Action::GizmoKeys {
                time: drag.time,
                keys,
            });
        }
    }
}

/// Even-odd point-in-polygon test for the (possibly rotated/sheared) layer quad,
/// used by the preview's click-to-select. Mirrors the engine's private
/// `gizmo::point_in_quad`.
fn point_in_quad(p: (f32, f32), quad: &[(f32, f32); 4]) -> bool {
    let mut inside = false;
    let mut j = quad.len() - 1;
    for i in 0..quad.len() {
        let (xi, yi) = quad[i];
        let (xj, yj) = quad[j];
        let intersect = ((yi > p.1) != (yj > p.1))
            && (p.0 < (xj - xi) * (p.1 - yi) / (yj - yi + f32::EPSILON.copysign(yj - yi)) + xi);
        if intersect {
            inside = !inside;
        }
        j = i;
    }
    inside
}

/// Remove the element at `i` from `v` if in range, returning whether anything
/// was removed (so the caller only marks the host dirty on a real change).
fn vec_remove<T>(v: &mut Vec<T>, i: usize) -> bool {
    if i < v.len() {
        v.remove(i);
        true
    } else {
        false
    }
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::comp::filter_grouped;

    /// The number of colour effects on the selected layer of the active comp.
    fn color_effect_count(app: &App) -> usize {
        let ci = app.active_comp_index();
        app.project.comps[ci].layers[app.selected_layer.unwrap()]
            .effects
            .len()
    }

    /// Find a browser entry by display name (the registry is the engine's).
    fn entry(name: &str) -> crate::comp::BrowserEntry {
        filter_grouped("")
            .into_iter()
            .flat_map(|(_, hits)| hits)
            .map(|h| *h.entry)
            .find(|e| e.name == name)
            .expect("entry present")
    }

    #[test]
    fn add_remove_effect_is_undoable() {
        let mut app = App::new();
        app.apply(Action::SelectLayer(0));
        let before = color_effect_count(&app);

        // Add a Levels colour effect → stack grows, undo available.
        app.apply(Action::AddEffect(entry("Levels")));
        assert_eq!(color_effect_count(&app), before + 1);
        assert!(app.can_undo());

        // Undo removes it; redo re-adds it.
        app.apply(Action::Undo);
        assert_eq!(color_effect_count(&app), before);
        assert!(app.can_redo());
        app.apply(Action::Redo);
        assert_eq!(color_effect_count(&app), before + 1);

        // Remove it explicitly, then undo restores it.
        let idx = color_effect_count(&app) - 1;
        app.apply(Action::RemoveEffect {
            stack: EffectStack::Color,
            index: idx,
        });
        assert_eq!(color_effect_count(&app), before);
        app.apply(Action::Undo);
        assert_eq!(color_effect_count(&app), before + 1);
    }

    #[test]
    fn set_effect_param_edits_and_undoes() {
        let mut app = App::new();
        app.apply(Action::SelectLayer(0));
        app.apply(Action::AddEffect(entry("Brightness & Contrast")));
        let ei = color_effect_count(&app) - 1;

        let read = |app: &App| {
            let ci = app.active_comp_index();
            let e = &app.project.comps[ci].layers[0].effects[ei];
            effect_params::color_params(e)[0].value // Brightness
        };
        let original = read(&app);
        app.apply(Action::SetEffectParam {
            stack: EffectStack::Color,
            index: ei,
            param: 0,
            value: 0.5,
        });
        assert_eq!(read(&app), 0.5);
        app.apply(Action::Undo);
        assert_eq!(read(&app), original);
    }

    #[test]
    fn fresh_edit_clears_redo() {
        let mut app = App::new();
        app.apply(Action::SelectLayer(0));
        app.apply(Action::AddEffect(entry("Levels")));
        app.apply(Action::Undo);
        assert!(app.can_redo());
        // A new edit invalidates the redo stack (linear history).
        app.apply(Action::AddEffect(entry("Exposure")));
        assert!(!app.can_redo());
    }

    #[test]
    fn transport_actions_are_not_undoable() {
        let mut app = App::new();
        app.apply(Action::SetTime(1.0));
        app.apply(Action::TogglePlay);
        app.apply(Action::SelectLayer(0));
        // None of those mutate the document, so the undo stack stays empty.
        assert!(!app.can_undo());
    }

    #[test]
    fn work_area_actions_trim_and_loop() {
        let mut app = App::new();
        let dur = app.active_duration();
        // Trim the work area to a real sub-range.
        app.apply(Action::SetWorkAreaStart(1.0));
        app.apply(Action::SetWorkAreaEnd(3.0));
        let wa = app.active_work_area();
        assert!((wa.start - 1.0).abs() < 1e-4);
        assert!((wa.end - 3.0).abs() < 1e-4);
        // The work-area edit is undoable; undo restores the full range.
        assert!(app.can_undo());

        // The play loop wraps within the work area: a playhead at the end (or
        // before the in-point) snaps back to the in-point.
        app.playing = true;
        app.last_tick = Some(Instant::now());
        app.time = 2.99; // near the out-point
        std::thread::sleep(std::time::Duration::from_millis(20));
        app.tick();
        // Whatever the exact dt, the playhead stays within [in, out] (it wrapped
        // to `in` if it crossed `out`).
        let wa = app.active_work_area();
        assert!(app.time >= wa.start - 1e-3 && app.time <= wa.end + 0.05);

        // A start past the end clamps to the end (no inversion).
        app.apply(Action::SetWorkAreaStart(dur + 5.0));
        let wa = app.active_work_area();
        assert!(wa.start <= wa.end + 1e-4);

        // Reset spans the whole timeline again.
        app.apply(Action::ResetWorkArea);
        assert!(app.active_work_area().is_full(dur));
    }

    #[test]
    fn default_export_range_follows_work_area() {
        use crate::render::RenderRange;
        let mut app = App::new();
        // A full work area defaults to the full comp.
        assert_eq!(app.default_export_range(), RenderRange::Full);
        // A trimmed work area defaults to the work area.
        app.apply(Action::SetWorkAreaStart(1.0));
        app.apply(Action::SetWorkAreaEnd(3.0));
        assert_eq!(app.default_export_range(), RenderRange::WorkArea);
    }

    #[test]
    fn undo_drops_stale_selection() {
        // Removing the last layer then undoing keeps the selection valid; here we
        // just check selection clamps when a restored project would orphan it.
        let mut app = App::new();
        let ci = app.active_comp_index();
        let last = app.project.comps[ci].layers.len() - 1;
        app.apply(Action::SelectLayer(last));
        // Add an effect so there's an undoable step, then undo: selection must
        // remain in range (it does here — same layer count) and not panic.
        app.apply(Action::AddEffect(entry("Levels")));
        app.apply(Action::Undo);
        assert_eq!(app.selected_layer, Some(last));
    }

    /// Paint a synthetic preview rect so the comp↔screen mapping is defined: a
    /// 320-wide image whose comp is `comp.width`, centred at the origin for easy
    /// math (comp space center maps to (160, 90) here).
    fn set_preview_rect(app: &App, w: f32, h: f32) {
        use gpui::{px, size, Bounds, Point};
        app.preview_rect.set(Some(Bounds {
            origin: Point {
                x: px(0.0),
                y: px(0.0),
            },
            size: size(px(w), px(h)),
        }));
    }

    #[test]
    fn graph_toggle_and_shown_props() {
        let mut app = App::new();
        assert!(!app.graph_open);
        app.apply(Action::ToggleGraph);
        assert!(app.graph_open);
        // Toggle a prop into the shown set, then clear.
        app.apply(Action::ToggleGraphProp(Prop::X));
        assert_eq!(app.graph_shown, vec![Prop::X]);
        app.apply(Action::ToggleGraphProp(Prop::X));
        assert!(app.graph_shown.is_empty());
        app.apply(Action::ToggleGraphProp(Prop::Scale));
        app.apply(Action::ClearGraphProps);
        assert!(app.graph_shown.is_empty());
        // Toggles are pure UI: no undo entry.
        assert!(!app.can_undo());
    }

    #[test]
    fn set_interp_promotes_segment_and_is_undoable() {
        let mut app = App::new();
        app.apply(Action::SelectLayer(0));
        // Lay down two keys on X so there's a segment to ease.
        app.apply(Action::ToggleKeyframe(Prop::X)); // first key at t=0
        app.apply(Action::SetTime(1.0));
        app.apply(Action::SetTransform(Prop::X, 100.0)); // second key at t=1
        let ci = app.active_comp_index();
        // The outgoing key (index 0) starts non-eased.
        let before = app.project.comps[ci].layers[0].track(Prop::X).keys[0].interp;
        assert!(!matches!(before, Interp::Ease(_)));
        app.apply(Action::SetInterp {
            prop: Prop::X,
            key_index: 0,
            interp: Interp::Ease(crate::comp::Ease::EASY),
        });
        let after = app.project.comps[ci].layers[0].track(Prop::X).keys[0].interp;
        assert!(matches!(after, Interp::Ease(_)));
        // Undoable: undo restores the pre-ease interp.
        app.apply(Action::Undo);
        let restored = app.project.comps[ci].layers[0].track(Prop::X).keys[0].interp;
        assert!(!matches!(restored, Interp::Ease(_)));
    }

    #[test]
    fn move_keyframe_xy_retimes_and_revalues() {
        let mut app = App::new();
        app.apply(Action::SelectLayer(0));
        app.apply(Action::ToggleKeyframe(Prop::Opacity)); // key at t=0
        let ci = app.active_comp_index();
        let v0 = app.project.comps[ci].layers[0].track(Prop::Opacity).keys[0].value;
        app.apply(Action::MoveKeyframeXY {
            prop: Prop::Opacity,
            key_index: 0,
            time: 0.5,
            value: v0 - 0.3,
        });
        let k = app.project.comps[ci].layers[0].track(Prop::Opacity).keys[0];
        assert!((k.t - 0.5).abs() < 1e-4);
        assert!((k.value - (v0 - 0.3)).abs() < 1e-4);
        assert!(app.can_undo());
    }

    #[test]
    fn gizmo_drag_moves_selected_layer_and_keys() {
        let mut app = App::new();
        app.apply(Action::SelectLayer(0));
        set_preview_rect(&app, 320.0, 180.0);
        // Grab the layer body (its center in comp space) and drag it.
        let geom = app.selected_gizmo().expect("gizmo for selected layer");
        let ((cx, cy), scale) = app.preview_fit().expect("fit defined");
        // Screen position of the gizmo's anchor's *body*: pick a point inside the
        // box — the box center (average of corners).
        let bcx = geom.corners.iter().map(|c| c.0).sum::<f32>() / 4.0;
        let bcy = geom.corners.iter().map(|c| c.1).sum::<f32>() / 4.0;
        let sx = cx + bcx * scale;
        let sy = cy + bcy * scale;
        assert!(app.begin_gizmo_drag(sx, sy, 8.0));
        let ci = app.active_comp_index();
        let x_before = app.project.comps[ci].layers[0].track(Prop::X).sample(app.time, 0.0);
        // Drag right by 40 screen px → +40/scale comp px on X.
        app.update_gizmo_drag(sx + 40.0, sy);
        let x_after = app.project.comps[ci].layers[0].track(Prop::X).sample(app.time, 0.0);
        assert!(x_after > x_before, "x should increase: {x_before} -> {x_after}");
        // The drag keyed the transform (undoable).
        assert!(app.can_undo());
    }

    #[test]
    fn layer_at_pointer_picks_under_cursor() {
        let app = App::new();
        set_preview_rect(&app, 320.0, 180.0);
        // The topmost layer's quad center should resolve to a layer index.
        let ci = app.active_comp_index();
        let top = app.project.comps[ci].layers.len() - 1;
        let world = app.project.comps[ci].world_matrix(top, app.time);
        let (wx, wy) = world.apply(0.0, 0.0);
        let ((cx, cy), scale) = app.preview_fit().unwrap();
        let hit = app.layer_at_pointer(cx + wx * scale, cy + wy * scale);
        assert!(hit.is_some());
    }

    // ---- Wave 8 tests ----

    #[test]
    fn set_layer_3d_toggles_flag_and_is_undoable() {
        let mut app = App::new();
        let ci = app.active_comp_index();
        assert!(!app.project.comps[ci].layers[0].threed);
        app.apply(Action::SetLayer3D(0, true));
        assert!(app.project.comps[ci].layers[0].threed);
        assert!(app.can_undo());
        app.apply(Action::Undo);
        assert!(!app.project.comps[ci].layers[0].threed);
    }

    #[test]
    fn set_position_z_keys_z_track() {
        let mut app = App::new();
        app.apply(Action::SetLayer3D(0, true));
        app.apply(Action::SetTime(0.5));
        app.apply(Action::SetPositionZ(0, 200.0));
        let ci = app.active_comp_index();
        let z = app.project.comps[ci].layers[0].z.sample(0.5, 0.0);
        assert!((z - 200.0).abs() < 1e-3);
        assert!(app.can_undo());
    }

    #[test]
    fn set_3d_rotation_keys_orient_tracks() {
        let mut app = App::new();
        app.apply(Action::SetLayer3D(0, true));
        app.apply(Action::Set3DRotation(0, 10.0, 20.0, 30.0));
        let ci = app.active_comp_index();
        let l = &app.project.comps[ci].layers[0];
        assert!((l.orient_x.sample(app.time, 0.0) - 10.0).abs() < 1e-3);
        assert!((l.orient_y.sample(app.time, 0.0) - 20.0).abs() < 1e-3);
        assert!((l.orient_z.sample(app.time, 0.0) - 30.0).abs() < 1e-3);
        assert!(app.can_undo());
    }

    #[test]
    fn duplicate_layer_inserts_copy_and_updates_selection() {
        let mut app = App::new();
        let ci = app.active_comp_index();
        let before = app.project.comps[ci].layers.len();
        let orig_name = app.project.comps[ci].layers[0].name.clone();
        app.apply(Action::DuplicateLayer(0));
        let after = app.project.comps[ci].layers.len();
        assert_eq!(after, before + 1);
        assert_eq!(app.selected_layer, Some(1));
        assert!(app.project.comps[ci].layers[1].name.contains("copy"));
        assert_eq!(app.project.comps[ci].layers[0].name, orig_name);
        assert!(app.can_undo());
    }

    #[test]
    fn set_expression_writes_to_layer_track_and_map() {
        let mut app = App::new();
        app.apply(Action::SetExpression {
            layer_id: 0,
            prop: "X".to_string(),
            expr: "time * 50".to_string(),
        });
        let ci = app.active_comp_index();
        let expr = app.project.comps[ci].layers[0].x.expression.as_deref();
        assert_eq!(expr, Some("time * 50"));
        assert!(app.expressions.contains_key(&(0, "X".to_string())));
        // Clear by setting empty.
        app.apply(Action::SetExpression {
            layer_id: 0,
            prop: "X".to_string(),
            expr: String::new(),
        });
        let expr = app.project.comps[ci].layers[0].x.expression.as_deref();
        assert_eq!(expr, None);
        assert!(!app.expressions.contains_key(&(0, "X".to_string())));
    }

    #[test]
    fn hovered_gizmo_handle_is_not_undoable() {
        let mut app = App::new();
        app.apply(Action::SetHoveredGizmoHandle(Some(GizmoHandle::Rotate)));
        assert_eq!(app.hovered_gizmo_handle, Some(GizmoHandle::Rotate));
        // Pure UI — no undo entry.
        assert!(!app.can_undo());
    }

    // --- Batch 6: Text Animator tests ---

    #[test]
    fn test_text_animator_add_remove() {
        let mut app = App::new();
        // Layer 0 exists in the default demo project.
        assert!(app.project.comps[app.active_comp_index()].layers[0].text_animator.is_none());
        app.apply(Action::AddTextAnimator(0));
        assert!(app.project.comps[app.active_comp_index()].layers[0].text_animator.is_some());
        assert!(app.can_undo());
        app.apply(Action::RemoveTextAnimator(0));
        assert!(app.project.comps[app.active_comp_index()].layers[0].text_animator.is_none());
    }

    #[test]
    fn test_text_animator_range() {
        let mut app = App::new();
        app.apply(Action::AddTextAnimator(0));
        app.apply(Action::SetTextAnimatorRange { layer_id: 0, start: 0.2, end: 0.7 });
        let ci = app.active_comp_index();
        let ta = app.project.comps[ci].layers[0].text_animator.as_ref().unwrap();
        assert!((ta.range_start - 0.2).abs() < 1e-5);
        assert!((ta.range_end - 0.7).abs() < 1e-5);
    }

    #[test]
    fn test_text_animator_range_clamps() {
        let mut app = App::new();
        app.apply(Action::AddTextAnimator(0));
        // Values outside [0,1] should be clamped.
        app.apply(Action::SetTextAnimatorRange { layer_id: 0, start: -0.5, end: 1.5 });
        let ci = app.active_comp_index();
        let ta = app.project.comps[ci].layers[0].text_animator.as_ref().unwrap();
        assert!((ta.range_start - 0.0).abs() < 1e-5);
        assert!((ta.range_end - 1.0).abs() < 1e-5);
    }

    #[test]
    fn test_text_animator_offsets() {
        let mut app = App::new();
        app.apply(Action::AddTextAnimator(0));
        app.apply(Action::SetTextAnimatorOffsetX { layer_id: 0, value: 50.0 });
        app.apply(Action::SetTextAnimatorOffsetY { layer_id: 0, value: -20.0 });
        let ci = app.active_comp_index();
        let ta = app.project.comps[ci].layers[0].text_animator.as_ref().unwrap();
        assert!((ta.offset_x - 50.0).abs() < 1e-5);
        assert!((ta.offset_y - (-20.0)).abs() < 1e-5);
    }

    #[test]
    fn test_text_animator_default_scale_and_opacity() {
        let ta = crate::comp::TextAnimator::default();
        assert!((ta.scale - 1.0).abs() < 1e-5, "default scale = 1.0");
        assert!((ta.opacity - 1.0).abs() < 1e-5, "default opacity = 1.0");
    }

    // --- Batch 6: Trim Paths tests ---

    fn make_shape_layer_app() -> App {
        use crate::comp::{LayerKind, ShapeItem, ShapePrimitive};
        let mut app = App::new();
        // Replace layer 0 with a shape layer.
        let ci = app.active_comp_index();
        app.project.comps[ci].layers[0].kind = LayerKind::Shape;
        app.project.comps[ci].layers[0].shape.items.push(
            ShapeItem::new(ShapePrimitive::Rectangle { half_w: 50.0, half_h: 50.0, radius: 0.0 })
        );
        app
    }

    #[test]
    fn test_trim_paths_set() {
        let mut app = make_shape_layer_app();
        app.apply(Action::SetShapeTrimPaths { layer_id: 0, start: 0.1, end: 0.8, offset: 0.0 });
        let ci = app.active_comp_index();
        let tp = app.project.comps[ci].layers[0].shape.trim_paths.as_ref().unwrap();
        assert!((tp.start - 0.1).abs() < 1e-5);
        assert!((tp.end - 0.8).abs() < 1e-5);
    }

    #[test]
    fn test_trim_paths_clamp() {
        let mut app = make_shape_layer_app();
        app.apply(Action::SetShapeTrimPaths { layer_id: 0, start: -1.0, end: 2.0, offset: -0.5 });
        let ci = app.active_comp_index();
        let tp = app.project.comps[ci].layers[0].shape.trim_paths.as_ref().unwrap();
        assert!((tp.start - 0.0).abs() < 1e-5, "start clamped to 0");
        assert!((tp.end - 1.0).abs() < 1e-5, "end clamped to 1");
        assert!((tp.offset - 0.0).abs() < 1e-5, "offset clamped to 0");
    }

    #[test]
    fn test_trim_paths_clear() {
        let mut app = make_shape_layer_app();
        app.apply(Action::SetShapeTrimPaths { layer_id: 0, start: 0.0, end: 0.5, offset: 0.0 });
        app.apply(Action::ClearShapeTrimPaths(0));
        let ci = app.active_comp_index();
        assert!(app.project.comps[ci].layers[0].shape.trim_paths.is_none());
    }

    #[test]
    fn test_trim_paths_undo() {
        let mut app = make_shape_layer_app();
        app.apply(Action::SetShapeTrimPaths { layer_id: 0, start: 0.2, end: 0.9, offset: 0.1 });
        assert!(app.can_undo());
        app.apply(Action::Undo);
        let ci = app.active_comp_index();
        assert!(app.project.comps[ci].layers[0].shape.trim_paths.is_none());
    }

    // --- Batch 6: Shape Repeater tests ---

    #[test]
    fn test_repeater_add_remove() {
        let mut app = make_shape_layer_app();
        assert!(app.project.comps[app.active_comp_index()].layers[0].shape.repeater.is_none());
        app.apply(Action::AddShapeRepeater(0));
        assert!(app.project.comps[app.active_comp_index()].layers[0].shape.repeater.is_some());
        app.apply(Action::RemoveShapeRepeater(0));
        assert!(app.project.comps[app.active_comp_index()].layers[0].shape.repeater.is_none());
    }

    #[test]
    fn test_repeater_set_copies() {
        let mut app = make_shape_layer_app();
        app.apply(Action::AddShapeRepeater(0));
        app.apply(Action::SetRepeaterCopies { layer_id: 0, copies: 5 });
        let ci = app.active_comp_index();
        assert_eq!(app.project.comps[ci].layers[0].shape.repeater.as_ref().unwrap().copies, 5);
    }

    #[test]
    fn test_repeater_opacity_lerp() {
        use crate::comp::ShapeRepeater;
        let r = ShapeRepeater {
            copies: 3,
            opacity_start: 1.0,
            opacity_end: 0.0,
            ..ShapeRepeater::default()
        };
        assert!((r.copy_opacity(0) - 1.0).abs() < 1e-5, "first copy = opacity_start");
        assert!((r.copy_opacity(1) - 0.5).abs() < 1e-5, "middle copy = 0.5");
        assert!((r.copy_opacity(2) - 0.0).abs() < 1e-5, "last copy = opacity_end");
    }

    #[test]
    fn test_repeater_undo() {
        let mut app = make_shape_layer_app();
        app.apply(Action::AddShapeRepeater(0));
        assert!(app.can_undo());
        app.apply(Action::Undo);
        let ci = app.active_comp_index();
        assert!(app.project.comps[ci].layers[0].shape.repeater.is_none());
    }

    // --- Batch 6: Lumetri Color tests ---

    #[test]
    fn test_lumetri_add_remove() {
        let mut app = App::new();
        assert!(app.project.comps[app.active_comp_index()].layers[0].lumetri.is_none());
        app.apply(Action::AddLumetriColor(0));
        assert!(app.project.comps[app.active_comp_index()].layers[0].lumetri.is_some());
        app.apply(Action::RemoveLumetriColor(0));
        assert!(app.project.comps[app.active_comp_index()].layers[0].lumetri.is_none());
    }

    #[test]
    fn test_lumetri_set_param() {
        let mut app = App::new();
        app.apply(Action::AddLumetriColor(0));
        app.apply(Action::SetLumetriParam { layer_id: 0, param: "exposure", value: 2.0 });
        let ci = app.active_comp_index();
        let lc = app.project.comps[ci].layers[0].lumetri.as_ref().unwrap();
        assert!((lc.exposure - 2.0).abs() < 1e-5);
    }

    #[test]
    fn test_lumetri_reset() {
        let mut app = App::new();
        app.apply(Action::AddLumetriColor(0));
        app.apply(Action::SetLumetriParam { layer_id: 0, param: "exposure", value: 3.0 });
        app.apply(Action::SetLumetriParam { layer_id: 0, param: "contrast", value: 50.0 });
        app.apply(Action::ResetLumetriColor(0));
        let ci = app.active_comp_index();
        let lc = app.project.comps[ci].layers[0].lumetri.as_ref().unwrap();
        assert!((lc.exposure - 0.0).abs() < 1e-5, "exposure reset");
        assert!((lc.contrast - 0.0).abs() < 1e-5, "contrast reset");
        assert!((lc.saturation - 100.0).abs() < 1e-5, "saturation default 100");
    }

    #[test]
    fn test_lumetri_enabled_toggle() {
        let mut app = App::new();
        app.apply(Action::AddLumetriColor(0));
        let ci = app.active_comp_index();
        assert!(app.project.comps[ci].layers[0].lumetri.as_ref().unwrap().enabled);
        app.apply(Action::ToggleLumetriEnabled(0));
        assert!(!app.project.comps[app.active_comp_index()].layers[0].lumetri.as_ref().unwrap().enabled);
    }

    #[test]
    fn test_lumetri_pixel_exposure() {
        use crate::comp::LumetriColor;
        let mut lc = LumetriColor::default();
        lc.exposure = 1.0; // +1 EV = ×2
        let pixel = [0.25, 0.25, 0.25, 1.0];
        let result = lc.apply(pixel);
        // After 1 EV exposure, 0.25 → 0.5 (before contrast/etc., which are 0).
        assert!((result[0] - 0.5).abs() < 0.01, "exposure doubles luminance, got {}", result[0]);
        assert!((result[3] - 1.0).abs() < 1e-5, "alpha unchanged");
    }

    // --- Batch 6: Essential Graphics / MoGrt tests ---

    #[test]
    fn test_mogrt_add_remove() {
        let mut app = App::new();
        assert!(app.mogrt_templates.is_empty());
        app.apply(Action::AddMoGrtTemplate("Lower Third".to_string()));
        assert_eq!(app.mogrt_templates.len(), 1);
        assert_eq!(app.mogrt_templates[0].name, "Lower Third");
        app.apply(Action::RemoveMoGrtTemplate(0));
        assert!(app.mogrt_templates.is_empty());
    }

    #[test]
    fn test_mogrt_add_control() {
        let mut app = App::new();
        app.apply(Action::AddMoGrtTemplate("Intro".to_string()));
        app.apply(Action::AddMoGrtControl {
            template_idx: 0,
            control: MoGrtControl::Text {
                label: "Title".to_string(),
                value: "My Video".to_string(),
            },
        });
        assert_eq!(app.mogrt_templates[0].controls.len(), 1);
        assert_eq!(app.mogrt_templates[0].controls[0].label(), "Title");
    }

    #[test]
    fn test_mogrt_set_text_value() {
        let mut app = App::new();
        app.apply(Action::AddMoGrtTemplate("T".to_string()));
        app.apply(Action::AddMoGrtControl {
            template_idx: 0,
            control: MoGrtControl::Text { label: "Name".to_string(), value: "Old".to_string() },
        });
        app.apply(Action::SetMoGrtTextValue {
            template_idx: 0,
            control_idx: 0,
            value: "New".to_string(),
        });
        if let MoGrtControl::Text { value, .. } = &app.mogrt_templates[0].controls[0] {
            assert_eq!(value, "New");
        } else {
            panic!("expected Text control");
        }
    }

    #[test]
    fn test_mogrt_set_color_value() {
        let mut app = App::new();
        app.apply(Action::AddMoGrtTemplate("T".to_string()));
        app.apply(Action::AddMoGrtControl {
            template_idx: 0,
            control: MoGrtControl::Color {
                label: "BG".to_string(),
                value: [0.0, 0.0, 0.0, 1.0],
            },
        });
        app.apply(Action::SetMoGrtColorValue {
            template_idx: 0,
            control_idx: 0,
            value: [1.0, 0.5, 0.0, 1.0],
        });
        if let MoGrtControl::Color { value, .. } = &app.mogrt_templates[0].controls[0] {
            assert!((value[0] - 1.0).abs() < 1e-5);
            assert!((value[1] - 0.5).abs() < 1e-5);
        } else {
            panic!("expected Color control");
        }
    }

    // ── Batch 2: Layer Split ─────────────────────────────────────────────────

    #[test]
    fn test_split_layer_creates_two() {
        let mut app = App::new();
        let ci = app.active_comp_index();
        let before = app.project.comps[ci].layers.len();
        app.apply(Action::SetTime(2.5));
        app.apply(Action::SplitLayer(0));
        let after = app.project.comps[ci].layers.len();
        assert_eq!(after, before + 1, "split must add one layer");
    }

    #[test]
    fn test_split_layer_timing() {
        let mut app = App::new();
        app.apply(Action::SetTime(2.0));
        app.apply(Action::SplitLayer(0));
        let ci = app.active_comp_index();
        assert_eq!(app.project.comps[ci].layers[0].out_point, Some(2.0));
        assert_eq!(app.project.comps[ci].layers[1].in_point, Some(2.0));
    }

    #[test]
    fn test_split_layer_preserves_content() {
        let mut app = App::new();
        let ci = app.active_comp_index();
        let orig_name = app.project.comps[ci].layers[0].name.clone();
        app.apply(Action::SetTime(1.0));
        app.apply(Action::SplitLayer(0));
        // Both layers must have the same name (the copy is a literal clone before timing edits).
        assert_eq!(app.project.comps[ci].layers[0].name, orig_name);
        assert_eq!(app.project.comps[ci].layers[1].name, orig_name);
    }

    #[test]
    fn test_split_layer_undo() {
        let mut app = App::new();
        let ci = app.active_comp_index();
        let before = app.project.comps[ci].layers.len();
        app.apply(Action::SetTime(1.5));
        app.apply(Action::SplitLayer(0));
        assert_eq!(app.project.comps[ci].layers.len(), before + 1);
        app.apply(Action::Undo);
        assert_eq!(app.project.comps[ci].layers.len(), before, "undo must restore original count");
    }

    // ── Batch 2: Brainstorm ──────────────────────────────────────────────────

    #[test]
    fn test_brainstorm_toggle() {
        let mut app = App::new();
        assert!(!app.brainstorm.open);
        app.apply(Action::ToggleBrainstorm);
        assert!(app.brainstorm.open);
        app.apply(Action::ToggleBrainstorm);
        assert!(!app.brainstorm.open);
    }

    #[test]
    fn test_brainstorm_generate() {
        let mut app = App::new();
        app.apply(Action::GenerateBrainstormVariations { count: 6 });
        assert_eq!(app.brainstorm.variations.len(), 6);
    }

    #[test]
    fn test_brainstorm_grid() {
        let mut app = App::new();
        app.apply(Action::SetBrainstormGrid { cols: 3, rows: 2 });
        assert_eq!(app.brainstorm.grid_cols, 3);
        assert_eq!(app.brainstorm.grid_rows, 2);
    }

    #[test]
    fn test_brainstorm_select() {
        let mut app = App::new();
        app.apply(Action::GenerateBrainstormVariations { count: 4 });
        app.apply(Action::SelectBrainstormVariation(2));
        assert!(app.brainstorm.variations[2].selected);
        assert!(!app.brainstorm.variations[0].selected);
    }

    #[test]
    fn test_brainstorm_apply() {
        let mut app = App::new();
        app.apply(Action::GenerateBrainstormVariations { count: 3 });
        app.apply(Action::ApplyBrainstormVariation(0));
        // After apply, variations are cleared and panel closed.
        assert!(app.brainstorm.variations.is_empty());
        assert!(!app.brainstorm.open);
    }

    // ── Batch 2: Color Finesse ───────────────────────────────────────────────

    #[test]
    fn test_color_finesse_add_remove() {
        let mut app = App::new();
        let ci = app.active_comp_index();
        assert!(app.project.comps[ci].layers[0].color_finesse.is_none());
        app.apply(Action::AddColorFinesse(0));
        assert!(app.project.comps[ci].layers[0].color_finesse.is_some());
        app.apply(Action::RemoveColorFinesse(0));
        assert!(app.project.comps[ci].layers[0].color_finesse.is_none());
    }

    #[test]
    fn test_color_finesse_set_master_hue() {
        let mut app = App::new();
        let ci = app.active_comp_index();
        app.apply(Action::AddColorFinesse(0));
        app.apply(Action::SetColorFinesseParam {
            layer_id: 0,
            range: "master",
            prop: "hue",
            value: 90.0,
        });
        let cf = app.project.comps[ci].layers[0].color_finesse.as_ref().unwrap();
        assert!((cf.master.hue_shift - 90.0).abs() < 1e-3);
    }

    #[test]
    fn test_color_finesse_set_range() {
        let mut app = App::new();
        let ci = app.active_comp_index();
        app.apply(Action::AddColorFinesse(0));
        app.apply(Action::SetColorFinesseParam {
            layer_id: 0,
            range: "reds",
            prop: "saturation",
            value: 50.0,
        });
        let cf = app.project.comps[ci].layers[0].color_finesse.as_ref().unwrap();
        assert!((cf.reds.saturation - 50.0).abs() < 1e-3);
    }

    #[test]
    fn test_color_finesse_reset() {
        let mut app = App::new();
        let ci = app.active_comp_index();
        app.apply(Action::AddColorFinesse(0));
        app.apply(Action::SetColorFinesseParam { layer_id: 0, range: "master", prop: "hue", value: 45.0 });
        app.apply(Action::ResetColorFinesse(0));
        let cf = app.project.comps[ci].layers[0].color_finesse.as_ref().unwrap();
        assert!((cf.master.hue_shift).abs() < 1e-3);
    }

    #[test]
    fn test_color_finesse_enabled_toggle() {
        let mut app = App::new();
        let ci = app.active_comp_index();
        app.apply(Action::AddColorFinesse(0));
        assert!(app.project.comps[ci].layers[0].color_finesse.as_ref().unwrap().enabled);
        app.apply(Action::SetColorFinesseEnabled { layer_id: 0, enabled: false });
        assert!(!app.project.comps[ci].layers[0].color_finesse.as_ref().unwrap().enabled);
    }

    #[test]
    fn test_color_finesse_pixel_grade() {
        use crate::comp::ColorFinesse;
        // A green-hued pixel: R=0.0, G=1.0, B=0.0 (hue=120°, greens range).
        let cf = ColorFinesse {
            greens: crate::comp::ColorFinesseRange { hue_shift: 0.0, saturation: 100.0, lightness: 0.0 },
            enabled: true,
            ..ColorFinesse::default()
        };
        let out = cf.apply([0.0, 1.0, 0.0, 1.0]);
        // Saturation boosted; green should still be the dominant channel but fully saturated.
        assert!(out[1] > out[0], "green dominant after saturation boost");
    }

    // ── Batch 2: Pre-render Cache ────────────────────────────────────────────

    #[test]
    fn test_pre_render_start() {
        let mut app = App::new();
        app.apply(Action::StartPreRender);
        assert!(matches!(app.pre_render_status, PreRenderStatus::Rendering { .. }));
    }

    #[test]
    fn test_pre_render_progress() {
        let mut app = App::new();
        app.apply(Action::StartPreRender);
        app.apply(Action::SetPreRenderProgress { frames_done: 5, total: 30 });
        assert_eq!(
            app.pre_render_status,
            PreRenderStatus::Rendering { frames_done: 5, total: 30 }
        );
    }

    #[test]
    fn test_pre_render_complete() {
        let mut app = App::new();
        let dir = std::path::PathBuf::from("/tmp/pulse_test_cache");
        app.apply(Action::PreRenderComplete { frame_count: 30, cache_dir: dir.clone() });
        assert_eq!(
            app.pre_render_status,
            PreRenderStatus::Done { frame_count: 30, cache_dir: dir }
        );
    }

    #[test]
    fn test_pre_render_clear() {
        let mut app = App::new();
        app.apply(Action::StartPreRender);
        app.apply(Action::ClearPreRenderCache);
        assert_eq!(app.pre_render_status, PreRenderStatus::NotStarted);
        assert!(app.pre_render_cache_dir.is_none());
        assert!(!app.use_pre_render);
    }

    #[test]
    fn test_pre_render_toggle_use() {
        let mut app = App::new();
        assert!(!app.use_pre_render);
        app.apply(Action::ToggleUsePreRender);
        assert!(app.use_pre_render);
        app.apply(Action::ToggleUsePreRender);
        assert!(!app.use_pre_render);
    }

    // ── Batch 2: Track Camera ────────────────────────────────────────────────

    #[test]
    fn test_camera_tracker_toggle() {
        let mut app = App::new();
        assert!(!app.camera_tracker.open);
        app.apply(Action::ToggleCameraTracker);
        assert!(app.camera_tracker.open);
    }

    #[test]
    fn test_camera_tracker_add_remove_point() {
        let mut app = App::new();
        app.apply(Action::AddTrackPoint { name: "A".to_string(), pos: [100.0, 200.0] });
        app.apply(Action::AddTrackPoint { name: "B".to_string(), pos: [400.0, 300.0] });
        assert_eq!(app.camera_tracker.track_points.len(), 2);
        app.apply(Action::RemoveTrackPoint(0));
        assert_eq!(app.camera_tracker.track_points.len(), 1);
    }

    #[test]
    fn test_camera_tracker_solve() {
        let mut app = App::new();
        // Two track points each with 2 keyframes.
        app.apply(Action::AddTrackPoint { name: "A".to_string(), pos: [100.0, 100.0] });
        app.apply(Action::AddTrackPoint { name: "B".to_string(), pos: [300.0, 100.0] });
        app.apply(Action::MoveTrackPoint { idx: 0, time: 1.0, pos: [110.0, 110.0] });
        app.apply(Action::MoveTrackPoint { idx: 1, time: 1.0, pos: [310.0, 110.0] });
        app.apply(Action::SolveCameraTrack);
        assert!(app.camera_tracker.solved);
        assert!(!app.camera_tracker.camera_keyframes.is_empty());
    }

    #[test]
    fn test_camera_tracker_clear() {
        let mut app = App::new();
        app.apply(Action::AddTrackPoint { name: "X".to_string(), pos: [0.0, 0.0] });
        app.apply(Action::ClearCameraTrack);
        assert!(app.camera_tracker.track_points.is_empty());
        assert!(!app.camera_tracker.solved);
    }

    #[test]
    fn test_camera_tracker_move_point() {
        let mut app = App::new();
        app.apply(Action::AddTrackPoint { name: "P".to_string(), pos: [50.0, 50.0] });
        app.apply(Action::MoveTrackPoint { idx: 0, time: 1.0, pos: [60.0, 70.0] });
        let kfs = &app.camera_tracker.track_points[0].keyframes;
        let kf = kfs.iter().find(|(t, _)| (*t - 1.0).abs() < 1e-3);
        assert!(kf.is_some());
        let pos = kf.unwrap().1;
        assert!((pos[0] - 60.0).abs() < 1e-3);
        assert!((pos[1] - 70.0).abs() < 1e-3);
    }
}
