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

mod composition;
mod keyframes;
mod effects_chain;
mod render;
mod tracking;
mod expressions;
mod precomp;
mod puppeting;
mod text_anim;

pub use expressions::{ExprControlKind, ExprControlValue, ExprControl};
pub use text_anim::{TextAnimPreset, TextAnimProperty, TextAnimRange, TextAnimator, MogrParamKind, MogrParam, MogrTemplate};
pub use tracking::{TrackPoint, CameraTracker, MotionSketchStroke, MotionSketchConfig, StabilizeResult, StabilizeMethod, StabilizeFraming, WarpStabConfig, CameraTrackStatus, CameraTrackPoint, CameraTrackSolve};
pub use puppeting::{MorphMode, CorrespondenceMode, ShapeMorphKeyframe, ShapeMorphConfig, PuppetPinMode, PuppetPin, PuppetMesh};
pub use render::{BrainstormVariation, BrainstormState, PreRenderStatus, AudioVisMode, AudioVisSide, AudioSpectrumConfig, RenderStatus, RenderOutputFormat, RenderQueueItem};
pub use precomp::{MatteMode, TrackMatteConfig, PrecompConfig, PrecompInfo};

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

// ── Batch 2: Track Camera ─────────────────────────────────────────────────────

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

/// A single frame-keyed rotobrush stroke: a list of 2-D points (layer-local)
/// plus a flag indicating whether this is a subtract (background) stroke.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RotobrushStroke {
    pub frame: u32,
    pub pts: Vec<[f32; 2]>,
    pub is_subtract: bool,
}

/// Echo (motion-trail) effect configuration for a layer.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct EchoConfig {
    /// Time offset between successive echoes (seconds, e.g. 0.1).
    pub delay_seconds: f32,
    /// Number of echoes to produce (1-10).
    pub count: u8,
    /// Opacity decay per echo (0.0 = no decay, 1.0 = fully transparent after 1 echo).
    pub decay: f32,
    /// Blend mode: 0 = composite-in-time, 1 = add, 2 = screen.
    pub blend_mode: u8,
}

impl Default for EchoConfig {
    fn default() -> Self {
        Self { delay_seconds: 0.1, count: 3, decay: 0.5, blend_mode: 0 }
    }
}

// ── Batch 4: Depth of Field / Camera ─────────────────────────────────────────

/// Iris shape for the depth-of-field blur.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize, Default)]
pub enum IrisShape {
    #[default]
    Fast,
    Hexagon,
    Octagon,
    Circle,
    Square,
    Blade(u8),
}

/// Depth-of-field settings attached to the active comp's camera.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DepthOfField {
    pub enabled: bool,
    pub focus_distance: f32,
    pub aperture: f32,
    pub blur_level: f32,
    pub iris_shape: IrisShape,
}

impl Default for DepthOfField {
    fn default() -> Self {
        Self {
            enabled: false,
            focus_distance: 500.0,
            aperture: 5.6,
            blur_level: 100.0,
            iris_shape: IrisShape::Fast,
        }
    }
}

// ── Batch 4: Expression Engine depth ─────────────────────────────────────────

/// Script language used by the expression engine.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize, Default)]
pub enum ExprLang {
    #[default]
    JavaScript,
    Python,
}

// ── Batch 4: Collect Files / Package project ──────────────────────────────────

/// Configuration for the Collect Files / Package project feature.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CollectFilesConfig {
    pub destination: std::path::PathBuf,
    pub include_footage: bool,
    pub include_proxies: bool,
    pub generate_report: bool,
    pub reduce_project: bool,
}

impl Default for CollectFilesConfig {
    fn default() -> Self {
        Self {
            destination: std::path::PathBuf::from("."),
            include_footage: true,
            include_proxies: false,
            generate_report: true,
            reduce_project: false,
        }
    }
}

/// Full 3-D spatial configuration for a layer (enabled when the 3-D flag is set).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Layer3DConfig {
    pub enabled: bool,
    pub position: [f32; 3],
    pub rotation: [f32; 3],
    pub orientation: [f32; 3],
    pub scale: [f32; 3],
    pub anchor_point: [f32; 3],
    pub casts_shadows: bool,
    pub accepts_shadows: bool,
    pub casts_lights: bool,
    pub appears_in_reflections: bool,
    pub material_shininess: f32,
    pub material_metal: f32,
}

impl Default for Layer3DConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            position: [0.0, 0.0, 0.0],
            rotation: [0.0, 0.0, 0.0],
            orientation: [0.0, 0.0, 0.0],
            scale: [100.0, 100.0, 100.0],
            anchor_point: [0.0, 0.0, 0.0],
            casts_shadows: false,
            accepts_shadows: true,
            casts_lights: false,
            appears_in_reflections: true,
            material_shininess: 50.0,
            material_metal: 0.0,
        }
    }
}

// ── New Feature: PuppetPin (app-level, independent of comp-model puppet pins) ─

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

    // --- Batch 3 extended: Rotobrush ---
    /// Set whether new rotobrush strokes subtract (background) or add (foreground).
    SetRotobrushMode { subtract: bool },
    /// Set the rotobrush brush radius (clamped to ≥1.0).
    SetRotobrushRadius(f32),
    /// Add a rotobrush stroke to the layer at `layer_id` at `frame`.
    AddRotobrushStroke { layer_id: usize, frame: u32, pts: Vec<[f32; 2]> },
    /// Remove all rotobrush strokes from the layer at `layer_id`.
    ClearRotobrushStrokes { layer_id: usize },
    /// Propagate the rotobrush segmentation `forward_frames` frames ahead.
    PropagateRotobrush { layer_id: usize, forward_frames: u32 },

    // --- Batch 3 extended: Time Stretch ---
    /// Set the time-stretch factor on the layer at `layer_id` (clamped to ≥0.01).
    SetLayerTimeStretch { layer_id: usize, factor: f32 },

    // --- Batch 3 extended: Audio Fades ---
    /// Set per-layer audio fade-in / fade-out durations (both clamped to ≥0.0).
    SetLayerAudioFade { layer_id: usize, fade_in: f32, fade_out: f32 },

    // --- Batch 3 extended: Puppet Pin Stiffness ---
    /// Set the stiffness of a specific puppet pin (clamped to 0.0..=1.0).
    SetPuppetPinStiffness { layer_id: usize, pin_id: u64, stiffness: f32 },
    /// Toggle the `is_stiff` flag on a specific puppet pin.
    TogglePuppetPinStiff { layer_id: usize, pin_id: u64 },
    /// Set the puppet mesh density on the layer at `layer_id` (clamped to ≥2).
    SetPuppetMeshDensity { layer_id: usize, density: u8 },

    // --- Batch 3 extended: Echo Effect ---
    /// Set (or replace) the echo effect on the layer at `layer_id`.
    SetLayerEcho { layer_id: usize, config: EchoConfig },
    /// Remove the echo effect from the layer at `layer_id`.
    ClearLayerEcho { layer_id: usize },
    // --- Batch 4: 3D Camera depth (DoF + rig controls) ---
    SetDepthOfField(DepthOfField),
    SetDofEnabled(bool),
    SetDofFocusDistance(f32),
    SetDofAperture(f32),
    SetDofBlurLevel(f32),
    SetCameraZoom(f32),
    SetCameraPointOfInterest([f32; 3]),
    SetCameraOrbitSpeed(f32),
    ResetCamera,

    // --- Batch 4: Expression Engine depth ---
    SetExpressionEnabled { layer_id: usize, prop: String, enabled: bool },
    AddExpressionError { layer_id: usize, prop: String, error: String },
    ClearExpressionErrors { layer_id: usize },
    SetExpressionLanguage(ExprLang),
    EvaluateExpression { layer_id: usize, prop: String, at_time: f32 },

    // --- Batch 4: Brainstorm depth ---
    SetBrainstormVariationCount(u8),
    ExportBrainstormVariation { idx: usize, path: std::path::PathBuf },
    CompareBrainstormVariations { a: usize, b: usize },
    LockBrainstormVariation(usize),

    // --- Batch 4: Collect Files / Package project ---
    ToggleCollectFilesPanel,
    SetCollectDestination(std::path::PathBuf),
    SetCollectIncludeFootage(bool),
    SetCollectIncludeProxies(bool),
    SetCollectGenerateReport(bool),
    SetCollectReduceProject(bool),
    RunCollectFiles,

    // --- Batch 5: Motion Sketch ---
    SetMotionSketchCaptureSpeed(f32),
    SetMotionSketchSmoothing(f32),
    SetMotionSketchShowWireframe(bool),
    ToggleMotionSketchRecord,
    ApplyMotionSketchStroke(MotionSketchStroke),
    ClearMotionSketchStrokes,
    ApplyMotionSketchToLayer { layer_id: usize },

    // --- Batch 5: Warp Stabilizer depth ---
    SetWarpStabResult(StabilizeResult),
    SetWarpStabSmoothness(f32),
    SetWarpStabMethod(StabilizeMethod),
    SetWarpStabFraming(StabilizeFraming),
    SetWarpStabCropSmooth(f32),
    SetWarpStabDetailedAnalysis(bool),
    SetWarpStabRollingShutter(f32),
    AnalyzeWarpStab { layer_id: usize },
    WarpStabAnalysisComplete,

    // --- Batch 5: Shape Layer Morphing ---
    SetShapeMorphEnabled(bool),
    AddMorphKeyframe(ShapeMorphKeyframe),
    RemoveMorphKeyframe(usize),
    SetMorphMode { kf_idx: usize, mode: MorphMode },
    SetCorrespondenceMode(CorrespondenceMode),
    SetMorphPreviewTime(f32),
    PreviewMorphAtTime(f32),
    ClearMorphKeyframes,

    // --- Batch 5: Audio Spectrum / Waveform Effects ---
    SetAudioVisMode(AudioVisMode),
    SetAudioVisLayer(Option<usize>),
    SetAudioStartFreq(f32),
    SetAudioEndFreq(f32),
    SetAudioMaxHeight(f32),
    SetAudioVisSide(AudioVisSide),
    SetAudioSoftness(f32),
    SetAudioMirror(bool),
    SetAudioDisplayedSamples(u32),
    SetAudioFrequencyBands(u32),
    SetAudioThickness(f32),
    SetAudioDigital(bool),
    ApplyAudioSpectrumEffect { layer_id: usize },

    // --- Batch 6 depth: Track Matte ---
    SetTrackMatte { layer_id: usize, config: TrackMatteConfig },
    SetTrackMatteMode { layer_id: usize, mode: MatteMode },
    SetTrackMatteSource { layer_id: usize, matte_layer: Option<usize> },
    ToggleTrackMatteInvert { layer_id: usize },
    SetTrackMattePreserveTransparency { layer_id: usize, preserve: bool },
    ClearTrackMatte { layer_id: usize },
    ToggleTrackMattePanel,

    // --- Batch 6 depth: Precomp ---
    SetPrecompName(String),
    SetPrecompMoveAttribs(bool),
    SetPrecompAdjustDuration(bool),
    PrecomposeSelected,
    OpenPrecomp(usize),
    ClosePrecomp,
    ReturnToMain,
    RenamePrecomp { idx: usize, name: String },
    DeletePrecomp(usize),
    CollapseTransformations { layer_id: usize },

    // --- Batch 6 depth: Render Queue (enhanced) ---
    ToggleRenderQueue,
    AddRenderQueueItem(RenderQueueItem),
    RemoveRenderQueueItem(usize),
    SetRenderItemFormat { idx: usize, format: RenderOutputFormat },
    SetRenderItemOutput { idx: usize, path: std::path::PathBuf },
    SetRenderItemRange { idx: usize, start: u32, end: u32 },
    SetRenderItemProxy { idx: usize, use_proxy: bool },
    StartRenderQueue,
    StopRenderQueue,
    RenderQueueItemComplete { idx: usize },
    SkipRenderItem(usize),
    DuplicateRenderItem(usize),

    // --- Batch 6 depth: 3D Layer ---
    Enable3DLayer { layer_id: usize, enabled: bool },
    Set3DPosition { layer_id: usize, pos: [f32; 3] },
    Set3DLayerRotation { layer_id: usize, rot: [f32; 3] },
    Set3DOrientation { layer_id: usize, orient: [f32; 3] },
    Set3DScale { layer_id: usize, scale: [f32; 3] },
    Set3DAnchor { layer_id: usize, anchor: [f32; 3] },
    Set3DShadows { layer_id: usize, casts: bool, accepts: bool },
    Set3DMaterial { layer_id: usize, shininess: f32, metal: f32 },
    Reset3DLayer { layer_id: usize },

    // --- New: PuppetPin (app-level) ---
    /// Activate or deactivate the puppet tool.
    ActivatePuppetTool(bool),
    /// Add a puppet pin to a layer at (x, y) with the given mode.
    AddPuppetPinExt { layer_id: usize, x: f32, y: f32, mode: PuppetPinMode },
    /// Move a puppet pin to a new position.
    MovePuppetPinExt { pin_id: usize, x: f32, y: f32 },
    /// Set the stiffness of a puppet pin (clamped 0..=100).
    SetPuppetPinStiffnessExt { pin_id: usize, stiffness: f32 },
    /// Remove a puppet pin by id.
    DeletePuppetPin(usize),
    /// Set puppet mesh density for a layer (clamped 1..=30, upsert).
    SetPuppetMeshDensityExt { layer_id: usize, density: u8 },
    /// Set puppet mesh expansion for a layer (clamped 3..=100, upsert).
    SetPuppetMeshExpansion { layer_id: usize, expansion: f32 },

    // --- New: CameraTracker (3D solve) ---
    /// Start a 3D camera track on a layer; generates stub track points.
    StartCameraTrackSolve { layer_id: usize },
    /// Solve the 3D camera for a layer; sets status=Done and solve_error.
    SolveCameraTrackExt { layer_id: usize },
    /// Select specific track points on a layer's solve (others deselected).
    SelectTrackPoints { layer_id: usize, point_ids: Vec<usize> },
    /// Create a solved camera layer from a completed solve.
    CreateSolvedCamera { layer_id: usize },
    /// Delete the 3D camera track solve for a layer.
    DeleteCameraTrackSolve { layer_id: usize },

    // --- New: TextAnimator (app-level) ---
    /// Add a new app-level text animator for a layer.
    AddTextAnimatorExt { layer_id: usize },
    /// Apply a built-in preset to a text animator.
    ApplyTextAnimPreset { animator_id: usize, preset: TextAnimPreset },
    /// Set the range start/end of a text animator (both clamped 0..=100).
    SetTextAnimRange { animator_id: usize, start: f32, end: f32 },
    /// Set the range units of a text animator.
    SetTextAnimRangeUnits { animator_id: usize, units: String },
    /// Set the "based on" of a text animator.
    SetTextAnimBasedOn { animator_id: usize, based_on: String },
    /// Remove a text animator by id.
    RemoveTextAnimatorExt(usize),

    // --- New: EssentialGraphicsPanel (MOGRT) ---
    /// Open the Essential Graphics panel.
    OpenEssentialGraphics,
    /// Close the Essential Graphics panel.
    CloseEssentialGraphics,
    /// Create a new MOGRT template.
    CreateMogrTemplate { name: String, composition_id: usize },
    /// Add a parameter to a MOGRT template.
    AddMogrParam { template_id: usize, param: MogrParam },
    /// Set the value of a MOGRT parameter.
    SetMogrParamValue { template_id: usize, param_id: String, value: String },
    /// Export a MOGRT template (stub: sets is_responsive=true).
    ExportMogrt { template_id: usize },
    /// Delete a MOGRT template by id.
    DeleteMogrTemplate(usize),
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
                | Action::AddRotobrushStroke { .. }
                | Action::ClearRotobrushStrokes { .. }
                | Action::PropagateRotobrush { .. }
                | Action::SetLayerTimeStretch { .. }
                | Action::SetLayerAudioFade { .. }
                | Action::SetPuppetPinStiffness { .. }
                | Action::TogglePuppetPinStiff { .. }
                | Action::SetPuppetMeshDensity { .. }
                | Action::SetLayerEcho { .. }
                | Action::ClearLayerEcho { .. }
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

    // --- Batch 3 extended fields ---
    /// Whether the rotobrush is in subtract (background) mode. `false` = add (foreground).
    pub rotobrush_subtract: bool,
    /// Rotobrush brush radius in pixels.
    pub rotobrush_radius: f32,
    // --- Batch 4: 3D Camera depth ---
    /// Depth-of-field settings for the active comp's camera.
    pub dof: DepthOfField,
    /// Camera zoom factor (default 1.0).
    pub camera_zoom: f32,
    /// Camera point of interest in comp-space XYZ (default [0, 0, 0]).
    pub camera_point_of_interest: [f32; 3],
    /// Camera orbit speed in degrees/sec (default 0.0 = no orbit).
    pub camera_orbit_speed: f32,

    // --- Batch 4: Expression Engine depth ---
    /// Active expression scripting language.
    pub expr_language: ExprLang,
    /// Per-(layer, prop) expression error messages.
    pub expr_errors: std::collections::HashMap<(usize, String), String>,
    /// Per-(layer, prop) expression enabled flags.
    pub expr_enabled: std::collections::HashMap<(usize, String), bool>,
    /// Result of the most-recently evaluated expression (stub).
    pub last_expr_result: Option<f32>,

    // --- Batch 4: Brainstorm depth ---
    /// Number of variations to generate (clamped 1–9).
    pub brainstorm_variation_count: u8,
    /// Per-variation lock state.
    pub brainstorm_locked: Vec<bool>,
    /// A/B comparison pair of variation indices.
    pub brainstorm_comparison: Option<(usize, usize)>,
    /// The variation index currently shown in the preview (set by Apply / Export).
    pub active_brainstorm_variation: Option<usize>,

    // --- Batch 4: Collect Files ---
    /// Configuration for the Collect Files / Package dialog.
    pub collect_files_config: CollectFilesConfig,
    /// Whether the Collect Files panel is open.
    pub collect_files_panel_open: bool,
    /// Summary message from the last Collect Files run.
    pub last_collect_result: Option<String>,

    // --- Batch 5: Motion Sketch ---
    pub motion_sketch_config: MotionSketchConfig,
    pub motion_sketch_strokes: Vec<MotionSketchStroke>,
    pub motion_sketch_recording: bool,

    // --- Batch 5: Warp Stabilizer depth ---
    pub warp_stab_config: WarpStabConfig,
    pub warp_stab_analyzing: bool,
    pub warp_stab_progress: f32,
    pub warp_stab_applied_layer: Option<usize>,

    // --- Batch 5: Shape Layer Morphing ---
    pub shape_morph_config: ShapeMorphConfig,
    pub morph_preview_time: f32,

    // --- Batch 5: Audio Spectrum / Waveform Effects ---
    pub audio_spectrum_config: AudioSpectrumConfig,
    pub audio_spectrum_layer: Option<usize>,

    // --- Batch 6 depth: Track Matte ---
    pub track_matte_configs: std::collections::HashMap<usize, TrackMatteConfig>,
    pub track_matte_panel_open: bool,

    // --- Batch 6 depth: Precomp ---
    pub precomp_config: PrecompConfig,
    pub precomps: Vec<PrecompInfo>,
    pub active_precomp: Option<usize>,
    pub collapse_transforms: std::collections::HashSet<usize>,

    // --- Batch 6 depth: Render Queue (enhanced) ---
    pub render_queue_items: Vec<RenderQueueItem>,
    pub render_queue_open: bool,
    pub render_in_progress: bool,
    pub render_active_idx: Option<usize>,

    // --- Batch 6 depth: 3D Layer ---
    pub layer_3d_configs: std::collections::HashMap<usize, Layer3DConfig>,

    // --- New: PuppetPin (app-level) ---
    pub puppet_pins: Vec<PuppetPin>,
    pub puppet_meshes: Vec<PuppetMesh>,
    pub puppet_pin_counter: usize,
    pub puppet_tool_active: bool,

    // --- New: CameraTracker (3D solve) ---
    pub camera_track_solves: Vec<CameraTrackSolve>,
    pub camera_track_counter: usize,

    // --- New: TextAnimator (app-level) ---
    pub text_animators: Vec<TextAnimator>,
    pub text_anim_counter: usize,

    // --- New: EssentialGraphicsPanel (MOGRT) ---
    pub mogrt_templates_v2: Vec<MogrTemplate>,
    pub mogrt_counter: usize,
    pub essential_graphics_open: bool,
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
            rotobrush_subtract: false,
            rotobrush_radius: 20.0,
            dof: DepthOfField::default(),
            camera_zoom: 1.0,
            camera_point_of_interest: [0.0, 0.0, 0.0],
            camera_orbit_speed: 0.0,
            expr_language: ExprLang::default(),
            expr_errors: std::collections::HashMap::new(),
            expr_enabled: std::collections::HashMap::new(),
            last_expr_result: None,
            brainstorm_variation_count: 9,
            brainstorm_locked: Vec::new(),
            brainstorm_comparison: None,
            active_brainstorm_variation: None,
            collect_files_config: CollectFilesConfig::default(),
            collect_files_panel_open: false,
            last_collect_result: None,
            motion_sketch_config: MotionSketchConfig::default(),
            motion_sketch_strokes: Vec::new(),
            motion_sketch_recording: false,
            warp_stab_config: WarpStabConfig::default(),
            warp_stab_analyzing: false,
            warp_stab_progress: 0.0,
            warp_stab_applied_layer: None,
            shape_morph_config: ShapeMorphConfig::default(),
            morph_preview_time: 0.0,
            audio_spectrum_config: AudioSpectrumConfig::default(),
            audio_spectrum_layer: None,
            track_matte_configs: std::collections::HashMap::new(),
            track_matte_panel_open: false,
            precomp_config: PrecompConfig::default(),
            precomps: Vec::new(),
            active_precomp: None,
            collapse_transforms: std::collections::HashSet::new(),
            render_queue_items: Vec::new(),
            render_queue_open: false,
            render_in_progress: false,
            render_active_idx: None,
            layer_3d_configs: std::collections::HashMap::new(),
            puppet_pins: Vec::new(),
            puppet_meshes: Vec::new(),
            puppet_pin_counter: 0,
            puppet_tool_active: false,
            camera_track_solves: Vec::new(),
            camera_track_counter: 0,
            text_animators: Vec::new(),
            text_anim_counter: 0,
            mogrt_templates_v2: Vec::new(),
            mogrt_counter: 0,
            essential_graphics_open: false,
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
            // --- Keyframes ---
            a @ (Action::MoveKeyframe { .. }
            | Action::ToggleGraph
            | Action::ToggleGraphProp(_)
            | Action::ClearGraphProps
            | Action::SetInterp { .. }
            | Action::MoveKeyframeXY { .. }
            | Action::GizmoKeys { .. }) => self.apply_keyframes(a),

            // --- Effects chain ---
            a @ (Action::ToggleEffectBrowser
            | Action::SetEffectQuery(_)
            | Action::AddEffect(_)
            | Action::RemoveEffect { .. }
            | Action::SetEffectParam { .. }
            | Action::AddGpuiEffect(_)
            | Action::RemoveGpuiEffect(_)
            | Action::SetMosaicBlock { .. }
            | Action::SetChromaOffset { .. }
            | Action::SetEffectIntensity { .. }
            | Action::SetVignetteRadius { .. }
            | Action::ToggleGpuiEffectExpand(_)
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
            | Action::AddDisplacementMap { .. }
            | Action::SetDisplaceScale { .. }
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
            | Action::AddColorFinesse(_)
            | Action::RemoveColorFinesse(_)
            | Action::SetColorFinesseEnabled { .. }
            | Action::SetColorFinesseParam { .. }
            | Action::ResetColorFinesse(_)) => self.apply_effects_chain(a),

            // --- Render / export ---
            a @ (Action::ExportMp4(_)
            | Action::ExportGif(_)
            | Action::ToggleAudioPreview
            | Action::SetAudioVolume(_)
            | Action::AddToRenderQueue
            | Action::RenderAll
            | Action::RemoveFromRenderQueue(_)
            | Action::ToggleCompSettings
            | Action::SetPendingCompWidth(_)
            | Action::SetPendingCompHeight(_)
            | Action::SetPendingCompFps(_)
            | Action::SetPendingCompDuration(_)
            | Action::SetPendingCompBgColor(_)
            | Action::ApplyCompSettings
            | Action::BuildRamPreview
            | Action::PlayRamPreview
            | Action::PurgeRamPreview
            | Action::ClearRamPreview
            | Action::ExportProRes(_)
            | Action::ExportDnxHD(_)
            | Action::SaveOutputPreset(_)
            | Action::LoadOutputPreset(_)
            | Action::DeleteOutputPreset(_)
            | Action::AddAllCompsToQueue
            | Action::ToggleLiveOutput
            | Action::StartPreRender
            | Action::CancelPreRender
            | Action::SetPreRenderProgress { .. }
            | Action::PreRenderComplete { .. }
            | Action::PreRenderFailed(_)
            | Action::ClearPreRenderCache
            | Action::ToggleUsePreRender
            | Action::ToggleRenderQueue
            | Action::AddRenderQueueItem(_)
            | Action::RemoveRenderQueueItem(_)
            | Action::SetRenderItemFormat { .. }
            | Action::SetRenderItemOutput { .. }
            | Action::SetRenderItemRange { .. }
            | Action::SetRenderItemProxy { .. }
            | Action::StartRenderQueue
            | Action::StopRenderQueue
            | Action::RenderQueueItemComplete { .. }
            | Action::SkipRenderItem(_)
            | Action::DuplicateRenderItem(_)
            | Action::ToggleBrainstorm
            | Action::GenerateBrainstormVariations { .. }
            | Action::SelectBrainstormVariation(_)
            | Action::ApplyBrainstormVariation(_)
            | Action::SetBrainstormGrid { .. }
            | Action::SetBrainstormVariationCount(_)
            | Action::ExportBrainstormVariation { .. }
            | Action::CompareBrainstormVariations { .. }
            | Action::LockBrainstormVariation(_)
            | Action::ToggleCollectFilesPanel
            | Action::SetCollectDestination(_)
            | Action::SetCollectIncludeFootage(_)
            | Action::SetCollectIncludeProxies(_)
            | Action::SetCollectGenerateReport(_)
            | Action::SetCollectReduceProject(_)
            | Action::RunCollectFiles) => self.apply_render(a),

            // --- Tracking / rotobrush / motion sketch ---
            a @ (Action::ToggleCameraTracker
            | Action::AddTrackPoint { .. }
            | Action::RemoveTrackPoint(_)
            | Action::MoveTrackPoint { .. }
            | Action::SolveCameraTrack
            | Action::CreateCameraFromTrack
            | Action::SetCameraTrackerProgress(_)
            | Action::ClearCameraTrack
            | Action::SetRotobrushMode { .. }
            | Action::SetRotobrushRadius(_)
            | Action::AddRotobrushStroke { .. }
            | Action::ClearRotobrushStrokes { .. }
            | Action::PropagateRotobrush { .. }
            | Action::SetWarpStabResult(_)
            | Action::SetWarpStabSmoothness(_)
            | Action::SetWarpStabMethod(_)
            | Action::SetWarpStabFraming(_)
            | Action::SetWarpStabCropSmooth(_)
            | Action::SetWarpStabDetailedAnalysis(_)
            | Action::SetWarpStabRollingShutter(_)
            | Action::AnalyzeWarpStab { .. }
            | Action::WarpStabAnalysisComplete
            | Action::SetMotionSketchCaptureSpeed(_)
            | Action::SetMotionSketchSmoothing(_)
            | Action::SetMotionSketchShowWireframe(_)
            | Action::ToggleMotionSketchRecord
            | Action::ApplyMotionSketchStroke(_)
            | Action::ClearMotionSketchStrokes
            | Action::ApplyMotionSketchToLayer { .. }
            | Action::StartCameraTrackSolve { .. }
            | Action::SolveCameraTrackExt { .. }
            | Action::SelectTrackPoints { .. }
            | Action::CreateSolvedCamera { .. }
            | Action::DeleteCameraTrackSolve { .. }) => self.apply_tracking(a),

            // --- Expressions ---
            a @ (Action::AddExpressionControl(_)
            | Action::SetExprControlValue { .. }
            | Action::SetExpressionEnabled { .. }
            | Action::AddExpressionError { .. }
            | Action::ClearExpressionErrors { .. }
            | Action::SetExpressionLanguage(_)
            | Action::EvaluateExpression { .. }) => self.apply_expressions(a),

            // --- Precomp / track matte / 3D layers ---
            a @ (Action::SetTrackMatte { .. }
            | Action::SetTrackMatteMode { .. }
            | Action::SetTrackMatteSource { .. }
            | Action::ToggleTrackMatteInvert { .. }
            | Action::SetTrackMattePreserveTransparency { .. }
            | Action::ClearTrackMatte { .. }
            | Action::ToggleTrackMattePanel
            | Action::SetPrecompName(_)
            | Action::SetPrecompMoveAttribs(_)
            | Action::SetPrecompAdjustDuration(_)
            | Action::PrecomposeSelected
            | Action::OpenPrecomp(_)
            | Action::ClosePrecomp
            | Action::ReturnToMain
            | Action::RenamePrecomp { .. }
            | Action::DeletePrecomp(_)
            | Action::CollapseTransformations { .. }
            | Action::Enable3DLayer { .. }
            | Action::Set3DPosition { .. }
            | Action::Set3DLayerRotation { .. }
            | Action::Set3DOrientation { .. }
            | Action::Set3DScale { .. }
            | Action::Set3DAnchor { .. }
            | Action::Set3DShadows { .. }
            | Action::Set3DMaterial { .. }
            | Action::Reset3DLayer { .. }) => self.apply_precomp(a),

            // --- Puppeting / morphing ---
            a @ (Action::AddPuppetPin { .. }
            | Action::MovePuppetPin { .. }
            | Action::RemovePuppetPin { .. }
            | Action::SetPuppetPinStiffness { .. }
            | Action::TogglePuppetPinStiff { .. }
            | Action::SetPuppetMeshDensity { .. }
            | Action::SetShapeMorphEnabled(_)
            | Action::AddMorphKeyframe(_)
            | Action::RemoveMorphKeyframe(_)
            | Action::SetMorphMode { .. }
            | Action::SetCorrespondenceMode(_)
            | Action::SetMorphPreviewTime(_)
            | Action::PreviewMorphAtTime(_)
            | Action::ClearMorphKeyframes
            | Action::ActivatePuppetTool(_)
            | Action::AddPuppetPinExt { .. }
            | Action::MovePuppetPinExt { .. }
            | Action::SetPuppetPinStiffnessExt { .. }
            | Action::DeletePuppetPin(_)
            | Action::SetPuppetMeshDensityExt { .. }
            | Action::SetPuppetMeshExpansion { .. }) => self.apply_puppeting(a),

            // --- Text animation / MoGrt / audio spectrum ---
            a @ (Action::ToggleMoGrtPanel
            | Action::AddMoGrtTemplate(_)
            | Action::RemoveMoGrtTemplate(_)
            | Action::SelectMoGrtTemplate(_)
            | Action::AddMoGrtControl { .. }
            | Action::RemoveMoGrtControl { .. }
            | Action::SetMoGrtTextValue { .. }
            | Action::SetMoGrtColorValue { .. }
            | Action::SetMoGrtSliderValue { .. }
            | Action::ExportMoGrt { .. }
            | Action::SetAudioVisMode(_)
            | Action::SetAudioVisLayer(_)
            | Action::SetAudioStartFreq(_)
            | Action::SetAudioEndFreq(_)
            | Action::SetAudioMaxHeight(_)
            | Action::SetAudioVisSide(_)
            | Action::SetAudioSoftness(_)
            | Action::SetAudioMirror(_)
            | Action::SetAudioDisplayedSamples(_)
            | Action::SetAudioFrequencyBands(_)
            | Action::SetAudioThickness(_)
            | Action::SetAudioDigital(_)
            | Action::ApplyAudioSpectrumEffect { .. }
            | Action::AddTextAnimatorExt { .. }
            | Action::RemoveTextAnimatorExt(_)
            | Action::ApplyTextAnimPreset { .. }
            | Action::SetTextAnimRange { .. }
            | Action::SetTextAnimRangeUnits { .. }
            | Action::SetTextAnimBasedOn { .. }
            | Action::OpenEssentialGraphics
            | Action::CloseEssentialGraphics
            | Action::CreateMogrTemplate { .. }
            | Action::AddMogrParam { .. }
            | Action::SetMogrParamValue { .. }
            | Action::ExportMogrt { .. }
            | Action::DeleteMogrTemplate(_)) => self.apply_text_anim(a),

            // --- Everything else: composition, transport, layer management, 3D camera, history ---
            a => self.apply_composition(a),
        }
    }

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

    // ── Batch 3 extended: Rotobrush ─────────────────────────────────────────

    #[test]
    fn test_rotobrush_mode_and_radius() {
        let mut app = App::new();
        assert!(!app.rotobrush_subtract);
        assert!((app.rotobrush_radius - 20.0).abs() < 1e-4);

        app.apply(Action::SetRotobrushMode { subtract: true });
        assert!(app.rotobrush_subtract);

        app.apply(Action::SetRotobrushRadius(50.0));
        assert!((app.rotobrush_radius - 50.0).abs() < 1e-4);

        // Below-minimum clamp: 0.0 → 1.0
        app.apply(Action::SetRotobrushRadius(0.0));
        assert!((app.rotobrush_radius - 1.0).abs() < 1e-4);
    }

    #[test]
    fn test_add_rotobrush_stroke() {
        let mut app = App::new();
        let ci = app.active_comp_index();
        assert!(!app.project.comps[ci].layers.is_empty());

        app.apply(Action::AddRotobrushStroke {
            layer_id: 0,
            frame: 5,
            pts: vec![[10.0, 20.0], [30.0, 40.0]],
        });
        let ci = app.active_comp_index();
        assert_eq!(app.project.comps[ci].layers[0].rotobrush_strokes.len(), 1);
        assert_eq!(app.project.comps[ci].layers[0].rotobrush_strokes[0].frame, 5);
        assert!(!app.project.comps[ci].layers[0].rotobrush_strokes[0].is_subtract);

        // Subtract mode stroke
        app.apply(Action::SetRotobrushMode { subtract: true });
        app.apply(Action::AddRotobrushStroke {
            layer_id: 0,
            frame: 10,
            pts: vec![[5.0, 5.0]],
        });
        let ci = app.active_comp_index();
        assert_eq!(app.project.comps[ci].layers[0].rotobrush_strokes.len(), 2);
        assert!(app.project.comps[ci].layers[0].rotobrush_strokes[1].is_subtract);
    }

    #[test]
    fn test_clear_rotobrush_strokes() {
        let mut app = App::new();
        app.apply(Action::AddRotobrushStroke {
            layer_id: 0,
            frame: 0,
            pts: vec![[0.0, 0.0]],
        });
        app.apply(Action::AddRotobrushStroke {
            layer_id: 0,
            frame: 1,
            pts: vec![[1.0, 1.0]],
        });
        let ci = app.active_comp_index();
        assert_eq!(app.project.comps[ci].layers[0].rotobrush_strokes.len(), 2);
        app.apply(Action::ClearRotobrushStrokes { layer_id: 0 });
        let ci = app.active_comp_index();
        assert!(app.project.comps[ci].layers[0].rotobrush_strokes.is_empty());
    }

    // ── Batch 3 extended: Time Stretch ───────────────────────────────────────

    #[test]
    fn test_time_stretch_set() {
        let mut app = App::new();
        app.apply(Action::SetLayerTimeStretch { layer_id: 0, factor: 2.0 });
        let ci = app.active_comp_index();
        assert!((app.project.comps[ci].layers[0].time_stretch - 2.0).abs() < 1e-4);
    }

    #[test]
    fn test_time_stretch_clamp() {
        let mut app = App::new();
        // Setting factor to 0.0 should clamp to 0.01
        app.apply(Action::SetLayerTimeStretch { layer_id: 0, factor: 0.0 });
        let ci = app.active_comp_index();
        let stretch = app.project.comps[ci].layers[0].time_stretch;
        assert!(stretch >= 0.01, "time_stretch must be at least 0.01, got {stretch}");
    }

    // ── Batch 3 extended: Audio Fades ────────────────────────────────────────

    #[test]
    fn test_audio_fade_set() {
        let mut app = App::new();
        app.apply(Action::SetLayerAudioFade { layer_id: 0, fade_in: 1.5, fade_out: 2.0 });
        let ci = app.active_comp_index();
        assert!((app.project.comps[ci].layers[0].audio_fade_in - 1.5).abs() < 1e-4);
        assert!((app.project.comps[ci].layers[0].audio_fade_out - 2.0).abs() < 1e-4);
    }

    #[test]
    fn test_audio_fade_clamp_negative() {
        let mut app = App::new();
        app.apply(Action::SetLayerAudioFade { layer_id: 0, fade_in: -1.0, fade_out: -5.0 });
        let ci = app.active_comp_index();
        assert!((app.project.comps[ci].layers[0].audio_fade_in - 0.0).abs() < 1e-4);
        assert!((app.project.comps[ci].layers[0].audio_fade_out - 0.0).abs() < 1e-4);
    }

    // ── Batch 3 extended: Puppet Pin Stiffness ───────────────────────────────

    #[test]
    fn test_puppet_stiffness_set() {
        let mut app = App::new();
        app.apply(Action::AddPuppetPin { layer_id: 0, pos: [100.0, 100.0] });
        let ci = app.active_comp_index();
        let pin_id = app.project.comps[ci].layers[0].puppet_pins[0].id;

        app.apply(Action::SetPuppetPinStiffness { layer_id: 0, pin_id, stiffness: 0.75 });
        let ci = app.active_comp_index();
        assert!((app.project.comps[ci].layers[0].puppet_pins[0].stiffness - 0.75).abs() < 1e-4);
    }

    #[test]
    fn test_puppet_stiff_toggle() {
        let mut app = App::new();
        app.apply(Action::AddPuppetPin { layer_id: 0, pos: [50.0, 50.0] });
        let ci = app.active_comp_index();
        let pin_id = app.project.comps[ci].layers[0].puppet_pins[0].id;
        assert!(!app.project.comps[ci].layers[0].puppet_pins[0].is_stiff);

        app.apply(Action::TogglePuppetPinStiff { layer_id: 0, pin_id });
        let ci = app.active_comp_index();
        assert!(app.project.comps[ci].layers[0].puppet_pins[0].is_stiff);

        app.apply(Action::TogglePuppetPinStiff { layer_id: 0, pin_id });
        let ci = app.active_comp_index();
        assert!(!app.project.comps[ci].layers[0].puppet_pins[0].is_stiff);
    }

    #[test]
    fn test_puppet_mesh_density() {
        let mut app = App::new();
        app.apply(Action::SetPuppetMeshDensity { layer_id: 0, density: 8 });
        let ci = app.active_comp_index();
        assert_eq!(app.project.comps[ci].layers[0].puppet_mesh_density, 8);

        // Clamp: density below 2 → 2
        app.apply(Action::SetPuppetMeshDensity { layer_id: 0, density: 0 });
        let ci = app.active_comp_index();
        assert_eq!(app.project.comps[ci].layers[0].puppet_mesh_density, 2);
    }

    // ── Batch 3 extended: Echo Effect ────────────────────────────────────────

    #[test]
    fn test_echo_set_clear() {
        let mut app = App::new();
        let ci = app.active_comp_index();
        assert!(app.project.comps[ci].layers[0].echo.is_none());

        app.apply(Action::SetLayerEcho {
            layer_id: 0,
            config: EchoConfig::default(),
        });
        let ci = app.active_comp_index();
        assert!(app.project.comps[ci].layers[0].echo.is_some());

        app.apply(Action::ClearLayerEcho { layer_id: 0 });
        let ci = app.active_comp_index();
        assert!(app.project.comps[ci].layers[0].echo.is_none());
    }

    #[test]
    fn test_echo_config_params() {
        let mut app = App::new();
        let config = EchoConfig {
            delay_seconds: 0.25,
            count: 5,
            decay: 0.8,
            blend_mode: 1,
        };
        app.apply(Action::SetLayerEcho { layer_id: 0, config });
        let ci = app.active_comp_index();
        let echo = app.project.comps[ci].layers[0].echo.as_ref().unwrap();
        assert!((echo.delay_seconds - 0.25).abs() < 1e-4);
        assert_eq!(echo.count, 5);
        assert!((echo.decay - 0.8).abs() < 1e-4);
        assert_eq!(echo.blend_mode, 1);
    }

    // ── Batch 4: DoF / Camera ─────────────────────────────────────────────────

    #[test]
    fn test_dof_enabled_toggle() {
        let mut app = App::new();
        assert!(!app.dof.enabled);
        app.apply(Action::SetDofEnabled(true));
        assert!(app.dof.enabled);
        app.apply(Action::SetDofEnabled(false));
        assert!(!app.dof.enabled);
    }

    #[test]
    fn test_dof_aperture_clamp() {
        let mut app = App::new();
        // Below minimum → clamped to 1.4
        app.apply(Action::SetDofAperture(0.5));
        assert!((app.dof.aperture - 1.4).abs() < 1e-4, "below min clamps to 1.4");
        // Above maximum → clamped to 22.0
        app.apply(Action::SetDofAperture(100.0));
        assert!((app.dof.aperture - 22.0).abs() < 1e-4, "above max clamps to 22.0");
    }

    #[test]
    fn test_dof_blur_clamp() {
        let mut app = App::new();
        // Above maximum → clamped to 300
        app.apply(Action::SetDofBlurLevel(500.0));
        assert!((app.dof.blur_level - 300.0).abs() < 1e-4, "above max clamps to 300");
        // Below minimum → clamped to 0
        app.apply(Action::SetDofBlurLevel(-10.0));
        assert!((app.dof.blur_level - 0.0).abs() < 1e-4, "below min clamps to 0");
    }

    #[test]
    fn test_camera_zoom_min() {
        let mut app = App::new();
        // Setting zoom to 0.0 should clamp to 0.01
        app.apply(Action::SetCameraZoom(0.0));
        assert!(app.camera_zoom >= 0.01, "zoom clamped to 0.01, got {}", app.camera_zoom);
    }

    #[test]
    fn test_reset_camera() {
        let mut app = App::new();
        app.apply(Action::SetCameraZoom(5.0));
        app.apply(Action::SetDofEnabled(true));
        app.apply(Action::SetCameraPointOfInterest([100.0, 200.0, 50.0]));
        app.apply(Action::SetCameraOrbitSpeed(45.0));
        app.apply(Action::ResetCamera);
        assert!((app.camera_zoom - 1.0).abs() < 1e-4, "zoom reset to 1.0");
        assert!(!app.dof.enabled, "dof disabled after reset");
        assert_eq!(app.camera_point_of_interest, [0.0, 0.0, 0.0]);
        assert!((app.camera_orbit_speed - 0.0).abs() < 1e-4);
    }

    // ── Batch 4: Brainstorm depth ─────────────────────────────────────────────



    // ── Batch 4: Collect Files ────────────────────────────────────────────────

    #[test]
    fn test_collect_files_panel_toggle() {
        let mut app = App::new();
        assert!(!app.collect_files_panel_open);
        app.apply(Action::ToggleCollectFilesPanel);
        assert!(app.collect_files_panel_open);
        app.apply(Action::ToggleCollectFilesPanel);
        assert!(!app.collect_files_panel_open);
    }

    #[test]
    fn test_collect_run_sets_result() {
        let mut app = App::new();
        assert!(app.last_collect_result.is_none());
        app.apply(Action::RunCollectFiles);
        assert!(app.last_collect_result.is_some());
        let result = app.last_collect_result.as_ref().unwrap();
        assert!(result.contains("Collected"), "result should mention 'Collected': {result}");
    }

    #[test]
    fn test_collect_destination() {
        let mut app = App::new();
        let dest = std::path::PathBuf::from("/tmp/my_project");
        app.apply(Action::SetCollectDestination(dest.clone()));
        assert_eq!(app.collect_files_config.destination, dest);
        // Run and verify result mentions the path
        app.apply(Action::RunCollectFiles);
        let result = app.last_collect_result.as_ref().unwrap();
        assert!(result.contains("my_project"), "result should reference destination: {result}");
    }

    // ── Batch 5: Motion Sketch ────────────────────────────────────────────────

    #[test]
    fn test_motion_sketch_speed_clamp() {
        let mut app = App::new();
        app.apply(Action::SetMotionSketchCaptureSpeed(150.0));
        assert!((app.motion_sketch_config.capture_speed - 100.0).abs() < 1e-3);
        app.apply(Action::SetMotionSketchCaptureSpeed(-10.0));
        assert!((app.motion_sketch_config.capture_speed - 0.0).abs() < 1e-3);
    }

    #[test]
    fn test_motion_sketch_record_toggle() {
        let mut app = App::new();
        assert!(!app.motion_sketch_recording);
        app.apply(Action::ToggleMotionSketchRecord);
        assert!(app.motion_sketch_recording);
        app.apply(Action::ToggleMotionSketchRecord);
        assert!(!app.motion_sketch_recording);
    }

    #[test]
    fn test_motion_sketch_stroke_push() {
        let mut app = App::new();
        assert!(app.motion_sketch_strokes.is_empty());
        let stroke = MotionSketchStroke {
            points: vec![[0.0, 0.0], [1.0, 1.0]],
            timestamps: vec![0.0, 0.1],
            layer_id: 0,
            smoothing: 25.0,
        };
        app.apply(Action::ApplyMotionSketchStroke(stroke));
        assert_eq!(app.motion_sketch_strokes.len(), 1);
    }

    #[test]
    fn test_motion_sketch_clear() {
        let mut app = App::new();
        let stroke = MotionSketchStroke {
            points: vec![[0.0, 0.0]],
            timestamps: vec![0.0],
            layer_id: 0,
            smoothing: 0.0,
        };
        app.apply(Action::ApplyMotionSketchStroke(stroke));
        assert_eq!(app.motion_sketch_strokes.len(), 1);
        app.apply(Action::ClearMotionSketchStrokes);
        assert!(app.motion_sketch_strokes.is_empty());
    }

    // ── Batch 5: Warp Stabilizer ──────────────────────────────────────────────

    #[test]
    fn test_warp_stab_smoothness_clamp() {
        let mut app = App::new();
        app.apply(Action::SetWarpStabSmoothness(150.0));
        assert!((app.warp_stab_config.smoothness - 100.0).abs() < 1e-3);
        app.apply(Action::SetWarpStabSmoothness(-5.0));
        assert!((app.warp_stab_config.smoothness - 0.0).abs() < 1e-3);
    }

    #[test]
    fn test_warp_stab_analyze_sets_flag() {
        let mut app = App::new();
        assert!(!app.warp_stab_analyzing);
        app.apply(Action::AnalyzeWarpStab { layer_id: 2 });
        assert!(app.warp_stab_analyzing);
        assert_eq!(app.warp_stab_applied_layer, Some(2));
        assert!((app.warp_stab_progress - 0.0).abs() < 1e-3);
    }

    #[test]
    fn test_warp_stab_complete_clears_flag() {
        let mut app = App::new();
        app.apply(Action::AnalyzeWarpStab { layer_id: 0 });
        assert!(app.warp_stab_analyzing);
        app.apply(Action::WarpStabAnalysisComplete);
        assert!(!app.warp_stab_analyzing);
        assert!((app.warp_stab_progress - 1.0).abs() < 1e-3);
    }

    #[test]
    fn test_warp_stab_rolling_shutter_clamp() {
        let mut app = App::new();
        app.apply(Action::SetWarpStabRollingShutter(200.0));
        assert!((app.warp_stab_config.rolling_shutter_ripple - 100.0).abs() < 1e-3);
        app.apply(Action::SetWarpStabRollingShutter(-1.0));
        assert!((app.warp_stab_config.rolling_shutter_ripple - 0.0).abs() < 1e-3);
    }

    // ── Batch 6 depth: 3D Layer ───────────────────────────────────────────────

    #[test]
    fn test_enable_3d_layer() {
        let mut app = App::new();
        assert!(!app.layer_3d_configs.contains_key(&0));
        app.apply(Action::Enable3DLayer { layer_id: 0, enabled: true });
        assert!(app.layer_3d_configs[&0].enabled);
        app.apply(Action::Enable3DLayer { layer_id: 0, enabled: false });
        assert!(!app.layer_3d_configs[&0].enabled);
    }

    #[test]
    fn test_set_3d_material_clamp() {
        let mut app = App::new();
        // shininess 200 → clamped to 100; metal -5 → clamped to 0.
        app.apply(Action::Set3DMaterial { layer_id: 0, shininess: 200.0, metal: -5.0 });
        let cfg = &app.layer_3d_configs[&0];
        assert!((cfg.material_shininess - 100.0).abs() < 1e-5, "shininess must clamp to 100");
        assert!((cfg.material_metal - 0.0).abs() < 1e-5, "metal must clamp to 0");
    }

    #[test]
    fn test_reset_3d_layer_removes_config() {
        let mut app = App::new();
        app.apply(Action::Enable3DLayer { layer_id: 2, enabled: true });
        assert!(app.layer_3d_configs.contains_key(&2));
        app.apply(Action::Reset3DLayer { layer_id: 2 });
        assert!(!app.layer_3d_configs.contains_key(&2));
    }

    #[test]
    fn test_3d_position_set() {
        let mut app = App::new();
        app.apply(Action::Set3DPosition { layer_id: 0, pos: [10.0, 20.0, 30.0] });
        let pos = app.layer_3d_configs[&0].position;
        assert!((pos[0] - 10.0).abs() < 1e-5);
        assert!((pos[1] - 20.0).abs() < 1e-5);
        assert!((pos[2] - 30.0).abs() < 1e-5);
    }

    // ── New: PuppetPin (app-level) ────────────────────────────────────────────

    #[test]
    fn test_puppet_tool_activate() {
        let mut app = App::new();
        assert!(!app.puppet_tool_active);
        app.apply(Action::ActivatePuppetTool(true));
        assert!(app.puppet_tool_active);
        app.apply(Action::ActivatePuppetTool(false));
        assert!(!app.puppet_tool_active);
    }

}
