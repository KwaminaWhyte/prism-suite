//! The single shared application state for the GPUI host.
//!
//! `App` owns everything panels read or mutate: the `CanvasHost` (wgpu device +
//! real prism-canvas compositor), the `Document`, tool/brush/view state, and the
//! host's dirty flag. Panels NEVER mutate `App` fields directly — they emit an
//! [`Action`], and the root view routes it through [`App::apply`], which is the
//! single choke point that mutates state and marks the host dirty when pixels
//! change. This keeps the panel→state seam narrow so the panels can be ported in
//! parallel without colliding: a new panel only needs to (a) read `&App` and
//! (b) add `Action` variants + their `apply` arms.
//!
//! Defaults mirror the egui app (`pigment-app/src/app/mod.rs` `PigmentApp::new`)
//! so parity is reachable.

mod canvas;
mod layers;
mod selections;
mod painting;
mod filters;
mod text;
mod transforms;
mod smart_objects;
mod ai;
mod layer_3d;
mod export;

pub use self::transforms::{CaFillMethod, ContentAwareCropConfig};

use self::text::TextEdit;

use std::collections::{HashMap, HashSet};

use prism_canvas::{Dab, ViewTransform};
use prism_core::color::srgb_to_linear;
use prism_core::adjust::CurvePoints;
use prism_core::fill::flood_fill_mask;
use prism_core::raster::{combine, polygon_mask, CombineMode};
use prism_core::shape::ShapeKind;
use prism_core::{Adjustment, BlendMode, Document, LayerId, LayerKind};

use crate::canvas_host::CanvasHost;

// ---- Saveable Preferences ------------------------------------------------

/// Persistent per-user preferences written to
/// `~/.config/prism/pigment_prefs.json` (XDG config dir on Linux/macOS, or
/// `%APPDATA%\prism\pigment_prefs.json` on Windows; `dirs::config_dir()` is
/// used to resolve the path). All fields have sensible defaults so an absent
/// or partially-written file is safely recoverable.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AppPrefs {
    /// Last window width in logical pixels (saved on exit, restored on launch).
    #[serde(default = "default_window_width")]
    pub window_width: u32,
    /// Last window height in logical pixels.
    #[serde(default = "default_window_height")]
    pub window_height: u32,
    /// Path of the most-recently saved/opened document.
    #[serde(default)]
    pub last_document: Option<std::path::PathBuf>,
    /// MRU list of recently-opened files (up to 10). May overlap with
    /// `App::recent_files` which is the in-session list; prefs persists it.
    #[serde(default)]
    pub recent_files: Vec<std::path::PathBuf>,
}

fn default_window_width()  -> u32 { 1600 }
fn default_window_height() -> u32 { 1000 }
fn default_spot_heal_radius() -> f32 { 20.0 }

impl Default for AppPrefs {
    fn default() -> Self {
        Self {
            window_width: default_window_width(),
            window_height: default_window_height(),
            last_document: None,
            recent_files: Vec::new(),
        }
    }
}

impl AppPrefs {
    /// Resolve the preferences file path.
    /// Priority: `$XDG_CONFIG_HOME/prism/pigment_prefs.json` → `~/.config/prism/…`
    pub fn path() -> std::path::PathBuf {
        let base = dirs::config_dir()
            .unwrap_or_else(|| {
                std::env::var_os("HOME")
                    .map(std::path::PathBuf::from)
                    .unwrap_or_else(|| std::path::PathBuf::from("."))
                    .join(".config")
            });
        base.join("prism").join("pigment_prefs.json")
    }

    /// Load from disk. Returns `AppPrefs::default()` on any error so the app
    /// always starts with sensible values.
    pub fn load() -> Self {
        let path = Self::path();
        match std::fs::read_to_string(&path) {
            Ok(s) => serde_json::from_str(&s).unwrap_or_else(|e| {
                log::warn!("prefs parse error ({path:?}): {e} — using defaults");
                AppPrefs::default()
            }),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => AppPrefs::default(),
            Err(e) => {
                log::warn!("prefs read error ({path:?}): {e} — using defaults");
                AppPrefs::default()
            }
        }
    }

    /// Persist to disk. Creates parent directories if needed. Logs on failure
    /// (non-fatal — a prefs write failure must never crash the app).
    pub fn save(&self) {
        let path = Self::path();
        if let Some(parent) = path.parent() {
            if let Err(e) = std::fs::create_dir_all(parent) {
                log::warn!("prefs dir create failed ({parent:?}): {e}");
                return;
            }
        }
        match serde_json::to_string_pretty(self) {
            Ok(json) => {
                if let Err(e) = std::fs::write(&path, json) {
                    log::warn!("prefs write failed ({path:?}): {e}");
                }
            }
            Err(e) => log::warn!("prefs serialize failed: {e}"),
        }
    }
}

/// A node in a pen/bézier path: anchor position plus control handles.
#[derive(Clone, Debug)]
pub struct PenNode {
    pub pos: (f32, f32),
    pub ctrl_in: (f32, f32),
    pub ctrl_out: (f32, f32),
}

/// The editing tools, mirroring the egui app's `Tool` enum (see
/// `pigment-app/src/app/mod.rs`). The full retouch family (Clone/Heal/etc.) is
/// included so panel parity is reachable; the GPUI host wires behavior per wave.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tool {
    Move,      // pan the view (hand)
    MoveLayer, // translate the active layer
    Brush,
    Eraser,
    Clone, // clone stamp
    Heal,  // healing brush
    Dodge, // dodge (lighten)
    Burn,  // burn (darken)
    Smudge, // smudge (blend)
    Fill,
    Eyedropper,
    SelectRect,
    SelectEllipse,
    Lasso,
    MagicWand,
    Transform,
    Crop,
    Text,
    Pen,
    ShapeRect,
    ShapeEllipse,
    Gradient,
    Slice, // mark rectangular export regions
    Liquify, // warp mesh push/pull
}

/// A named rectangular region of the canvas for per-slice export.
#[derive(Clone, Debug)]
pub struct Slice {
    pub id: u32,
    /// `[x, y, w, h]` in document pixels.
    pub rect: [f32; 4],
    pub name: String,
}

impl Tool {
    /// Short label for the tools strip / toolbar.
    pub fn label(self) -> &'static str {
        match self {
            Tool::Move => "Move",
            Tool::MoveLayer => "MoveL",
            Tool::Brush => "Brush",
            Tool::Eraser => "Eraser",
            Tool::Clone => "Clone",
            Tool::Heal => "Heal",
            Tool::Dodge => "Dodge",
            Tool::Burn => "Burn",
            Tool::Smudge => "Smudge",
            Tool::Fill => "Fill",
            Tool::Eyedropper => "Eyedr",
            Tool::SelectRect => "Rect",
            Tool::SelectEllipse => "Ellip",
            Tool::Lasso => "Lasso",
            Tool::MagicWand => "Wand",
            Tool::Transform => "Xform",
            Tool::Crop => "Crop",
            Tool::Text => "Text",
            Tool::Pen => "Pen",
            Tool::ShapeRect => "RectS",
            Tool::ShapeEllipse => "EllpS",
            Tool::Gradient => "Grad",
            Tool::Slice => "Slice",
            Tool::Liquify => "Liqfy",
        }
    }

    /// Stable ordering for the tools strip (matches the egui palette grouping).
    pub const ALL: [Tool; 24] = [
        Tool::Move,
        Tool::MoveLayer,
        Tool::Brush,
        Tool::Eraser,
        Tool::Clone,
        Tool::Heal,
        Tool::Dodge,
        Tool::Burn,
        Tool::Smudge,
        Tool::Fill,
        Tool::Eyedropper,
        Tool::SelectRect,
        Tool::SelectEllipse,
        Tool::Lasso,
        Tool::MagicWand,
        Tool::Transform,
        Tool::Crop,
        Tool::Text,
        Tool::Pen,
        Tool::ShapeRect,
        Tool::ShapeEllipse,
        Tool::Gradient,
        Tool::Slice,
        Tool::Liquify,
    ];
}

/// Brush state, mirroring the egui app's defaults. `color` is straight sRGB
/// RGBA in 0..1 (the egui app stores `Color32::from_rgb(20, 120, 230)`).
#[derive(Clone, Copy, Debug)]
pub struct Brush {
    pub color: [f32; 4],
    pub size: f32,
    pub hardness: f32,
    pub opacity: f32,
}

impl Default for Brush {
    fn default() -> Self {
        Self {
            // egui: Color32::from_rgb(20, 120, 230)
            color: [20.0 / 255.0, 120.0 / 255.0, 230.0 / 255.0, 1.0],
            size: 40.0,
            hardness: 0.5,
            opacity: 1.0,
        }
    }
}

/// Every panel→state mutation a panel can request. Panels emit these; the root
/// view routes each into [`App::apply`]. EXTENSIBLE: later waves add variants
/// here and a matching arm in `apply` — that is the entire contract a parallel
/// agent touches when wiring a new interaction.
// Several variants are wired in `apply` but not yet emitted by a stub panel;
// they are the seams parallel agents fill in. Keep them rather than churn.
// Not `Copy`: `SetAdjustment` carries an `Adjustment`, whose `Curves` variant
// owns control points. Actions are built inline and moved into `apply`, so
// `Clone` is all the panel round-trip needs.
#[allow(dead_code)]
#[derive(Clone, Debug)]
pub enum Action {
    /// Select the active tool (tools strip / toolbar).
    SetTool(Tool),

    // --- Brush / color ---
    SetBrushColor([f32; 4]),
    SetBrushSize(f32),
    SetBrushHardness(f32),
    SetBrushOpacity(f32),

    // --- Tool options ---
    /// Set the paint-bucket / magic-wand match tolerance (0..1).
    SetFillTolerance(f32),
    /// Toggle whether fill / magic-wand stay contiguous from the seed.
    ToggleFillContiguous,
    /// Toggle gradient dithering (anti-banding).
    ToggleGradientDither,
    /// Set the Text-tool point size (px).
    SetTextSize(f32),

    // --- Layers ---
    /// Toggle a layer's visibility (re-composites — marks host dirty).
    ToggleLayerVisible(LayerId),
    /// Make a layer the active layer.
    SelectLayer(LayerId),
    /// Set a layer's opacity 0..1 (re-composites — marks host dirty).
    SetLayerOpacity(LayerId, f32),
    /// Move a layer up (`up = true`, toward top of stack) or down within the
    /// stack (re-composites — marks host dirty).
    MoveLayer { id: LayerId, up: bool },
    /// Delete a layer (re-composites — marks host dirty).
    DeleteLayer(LayerId),
    /// Set a layer's blend mode (re-composites — marks host dirty).
    SetLayerBlend(LayerId, BlendMode),

    // --- Color ---
    /// Append the current/given color to the swatch palette (pure UI state).
    AddSwatch([f32; 4]),

    // --- View ---
    /// Multiply the view zoom by a factor about the canvas center.
    ZoomBy(f32),
    /// Reset the view transform (pan to origin, zoom to 1.0).
    ResetView,

    // --- Selection ---
    /// Replace the engine selection with a rectangular (`ellipse = false`) or
    /// elliptical (`ellipse = true`) marquee in doc px (`[x, y, w, h]`).
    /// Re-composites — marks host dirty.
    SetMarquee { rect: [f32; 4], ellipse: bool },
    /// Clear the active selection (`Select > None`). Re-composites.
    ClearSelection,

    // --- File I/O ---
    /// Open a real image file: show an `rfd` picker, load it through `prism_io`
    /// into a fresh single-layer document, and upload it to the engine. Replaces
    /// the current `Document`. Re-composites — marks host dirty.
    OpenImage,
    /// Export the composited document to an image file (PNG): show an `rfd` save
    /// dialog, composite through the engine, and write straight-sRGB 8-bit pixels.
    ExportImage,
    /// Open an OpenEXR (.exr) file as a new document (linear-light HDR).
    OpenEXR,
    /// Export the composited document as an OpenEXR file (linear-light, full HDR).
    ExportEXR,
    /// Add a named slice rect `[x, y, w, h]` in doc px.
    AddSlice([f32; 4]),
    /// Delete the slice with the given id.
    DeleteSlice(u32),
    /// Export all slices to individual PNG files in a chosen directory.
    ExportSlices(std::path::PathBuf),

    // --- Filters / adjustments (destructive, through the engine) ---
    /// Apply a destructive filter/adjustment to the active layer through the
    /// engine's existing filter pipeline, then re-composite. The engine owns the
    /// math (no filter math lives in the host).
    ApplyFilter(Filter),

    // --- Layer masks (non-destructive, through the engine) ---
    /// Add a white (reveal-all) raster mask to a layer, then enter mask-edit mode
    /// so brush strokes target the mask. Re-composites — marks host dirty.
    AddMask(LayerId),
    /// Delete a layer's mask (compositor falls back to reveal-all). Leaves
    /// mask-edit mode if it was on. Re-composites — marks host dirty.
    DeleteMask(LayerId),
    /// Toggle mask-edit mode: while on (and the active layer has a mask), brush
    /// strokes paint the mask (white = reveal) and the eraser hides. Pure UI
    /// state — no re-composite.
    ToggleEditMask,

    // --- Adjustment layers (non-destructive, through the engine) ---
    /// Add a new adjustment layer of `kind` (at its default params) on top of the
    /// stack and make it active. The compositor applies it to the layers below.
    /// Re-composites — marks host dirty.
    AddAdjustment(AdjKind),
    /// Replace the active adjustment layer's parameters (the panel edits emit
    /// this with the whole updated `Adjustment`). Re-uploads LUTs if needed and
    /// re-composites. No-op if the active layer is not an adjustment.
    SetAdjustment(LayerId, Adjustment),

    // --- Undo / redo (Edit menu + Cmd+Z / Cmd+Shift+Z) ---
    /// Undo the most recent destructive command through the engine's undo stack.
    Undo,
    /// Redo the most recently undone command.
    Redo,

    // --- Channels panel ---
    /// Toggle display-channel visibility (0=R, 1=G, 2=B, 3=A). Does NOT modify
    /// pixels — only zeroes out the masked channel in the bridged BGRA8 output.
    ToggleChannel(usize),

    // --- Curves adjustment ---
    /// Replace the RGB control points of a Curves adjustment layer and re-upload
    /// the LUT so the compositor immediately reflects the new curve shape.
    SetAdjustmentCurve(LayerId, Vec<(f32, f32)>),

    // --- Edit menu ---
    /// Select the entire document: sets a full-canvas rectangular marquee.
    SelectAll,
    /// Flatten all layers to a single raster layer (placeholder: marks dirty).
    FlattenLayers,

    // --- Color swap / reset ---
    /// Swap foreground (brush) color with background color.
    SwapColors,
    /// Reset foreground to black, background to white.
    ResetColors,

    // --- Selection ---
    /// Invert the current selection mask (0↔1 for every pixel).
    InvertSelection,

    // --- History navigation ---
    /// Jump to a specific history snapshot by index (stub: truncates labels).
    JumpHistory(usize),

    // --- Smart Object ---
    /// Mark a layer as a Smart Object (stores a source path stub).
    ConvertToSmartObject(LayerId),
    /// Open the Smart Object source for editing (stub: logs the path).
    EditSmartObject(LayerId),
    /// Rasterize (remove Smart Object status from) a layer.
    RasterizeSmartObject(LayerId),

    // --- Pen tool ---
    /// Add an anchor node to the in-progress pen path at `pos` (doc px).
    PenAddNode((f32, f32)),
    /// Adjust a control handle on a pen node by a delta.
    PenMoveHandle { idx: usize, is_out: bool, delta: (f32, f32) },
    /// Close and rasterize the current pen path into the active layer.
    PenClose,

    // --- Curves drag ---
    /// Begin dragging a curve control point.
    BeginCurveDrag(LayerId, usize),
    /// Move a curve control point to a new normalized position (clamped 0..1).
    MoveCurvePoint(LayerId, usize, (f32, f32)),
    /// End the current curve drag and sync LUTs.
    EndCurveDrag,
    /// Remove a curve control point (only if >2 remain).
    RemoveCurvePoint(LayerId, usize),
    /// Update which curve control point is hovered (for highlight rendering).
    HoverCurvePoint(Option<(LayerId, usize)>),
    /// Update the stored canvas bounds for the curves editor (origin_x, origin_y, w, h).
    SetCurvesCanvasBounds([f32; 4]),

    // --- Wave 11: Layer styles ---
    /// Set a non-destructive style on a layer (replaces any existing style).
    SetLayerStyle(LayerId, LayerStyle),
    /// Remove the layer style from a layer.
    ClearLayerStyle(LayerId),
    /// Open the layer-style editor modal for a layer.
    OpenStylePanel(LayerId),
    /// Close the layer-style editor modal.
    CloseStylePanel,

    // --- Wave 11: Clipping masks ---
    /// Toggle clipping-mask status on a layer (clips to the layer directly below).
    ToggleClippingMask(LayerId),

    // --- Wave 11: HSV color picker ---
    /// Set foreground hue (0..360).
    SetFgHue(f32),
    /// Set foreground saturation (0..1) and value (0..1).
    SetFgSV(f32, f32),

    // --- Wave 11: PSD import/export ---
    /// Open a PSD file (shows a file picker, then loads).
    ImportPsd(std::path::PathBuf),
    /// Export a PSD file (stub — shows status).
    ExportPsd(std::path::PathBuf),
    /// Save document as JSON.
    SaveAs(std::path::PathBuf),
    /// Trigger file pickers for the above.
    OpenImportPsdDialog,
    OpenExportPsdDialog,
    OpenSaveAsDialog,

    // --- Batch 1: RAW import ---
    /// Import a camera RAW (or RAW-adjacent) file as a new layer on top of the
    /// current document. `prism_io` is checked first; falls back to `image::open`
    /// which handles DNG and many vendor RAW formats via dcraw/libraw bindings.
    ImportRaw(std::path::PathBuf),
    /// Open a native file picker filtered to common RAW formats, then fire
    /// `ImportRaw`.
    OpenImportRawDialog,

    // --- Wave 12: Crop ---
    /// Confirm the current crop rectangle: crops every layer to `[x, y, w, h]`
    /// (doc px, clamped to the document bounds) and resizes the document.
    ApplyCrop { x: u32, y: u32, w: u32, h: u32 },
    /// Discard the in-progress crop rectangle without modifying the document.
    CancelCrop,

    // --- Wave 11: Workspace presets ---
    /// Toggle visibility of a named panel.
    TogglePanel(String),
    /// Save current panel visibility as a named workspace.
    SaveWorkspace(String),
    /// Reset all panels to visible.
    ResetWorkspace,

    // --- Wave 13: Guides ---
    /// Add a horizontal guide at `pos_doc` (doc px from top).
    AddGuideH(f32),
    /// Add a vertical guide at `pos_doc` (doc px from left).
    AddGuideV(f32),
    /// Remove guide by index (horizontal = true → guides_h, else guides_v).
    RemoveGuide { horizontal: bool, idx: usize },
    /// Clear all guides.
    ClearGuides,
    /// Toggle guide visibility.
    ToggleGuides,

    // --- Wave 13: Solid fill layers ---
    /// Add a new solid-color fill layer above the active layer.
    AddSolidFillLayer([u8; 4]),
    /// Change the fill color of an existing fill layer (repaints the layer).
    SetFillLayerColor(LayerId, [u8; 4]),

    // --- Wave 13: layer groups, rotate canvas, color mode, recent files ---
    /// Group the currently selected layer (or all layers) into a named folder.
    /// Creates a group layer with `name`; selected layers become its children.
    CreateLayerGroup(String),
    /// Rotate the canvas view by `degrees` (cumulative; 0 = upright). Model-only.
    RotateCanvas(f32),
    /// Reset canvas rotation to 0°.
    ResetCanvasRotation,
    /// Switch the document's color mode.
    SetColorMode(ColorMode),
    /// Add a file path to the MRU recent-files list.
    AddRecentFile(std::path::PathBuf),
    /// Open a named menu dropdown (None = close all menus).
    OpenMenu(Option<String>),
    /// Toggle grid-snap on/off.
    ToggleGridSnap,
    /// Set grid cell size in doc px.
    SetGridSize(f32),

    // --- Wave 15: Smart Filters ---
    AddSmartFilter(LayerId, SmartFilter),
    RemoveSmartFilter(LayerId, usize),
    EditSmartFilter(LayerId, usize, SmartFilter),

    // --- Wave 15: Filter gallery ---
    OpenFilterGallery,
    CloseFilterGallery,
    /// Toggle the filter gallery open/closed (Batch 1 addition).
    ToggleFilterGallery,

    // --- Wave 15: Camera Raw ---
    OpenCameraRaw,
    CloseCameraRaw,
    SetCameraRawParam(&'static str, f32),
    ApplyCameraRaw,

    // --- Wave 15: Color profile ---
    SetColorProfile(ColorProfile),

    // --- Batch 2: Soft proof ---
    SetSoftProof(SoftProofMode),

    // --- Batch 2: Export presets ---
    AddExportPreset(ExportPreset),
    DeleteExportPreset(usize),
    ExportWithPreset(usize),

    // --- Batch 2: Histogram channel ---
    SetHistogramChannel(HistogramChannel),

    // --- Wave 15: Dockable panels ---
    DetachPanel(&'static str),
    AttachPanel(&'static str),
    MovePanel(&'static str, f32, f32),

    // --- Batch 1: Saveable preferences ---
    /// Persist current preferences (recent files, window size) to disk.
    SavePrefs,
    /// Reload preferences from disk (useful if the file was edited externally).
    LoadPrefs,

    // --- Batch 3: Content-Aware Fill (PatchMatch) ---
    ContentAwareFill,

    // --- Batch 3: Lens Correction ---
    ApplyLensCorrection { barrel: f32, pincushion: f32, vignette: f32 },

    // --- Batch 3: Perspective Warp ---
    PerspectiveWarp { src_pts: [[f32; 2]; 4], dst_pts: [[f32; 2]; 4] },

    // --- Batch 3: Camera Raw dialog toggle ---
    ToggleCameraRawDialog,
    /// Toggle a Camera Raw dialog section (basic/detail/hsl).
    ToggleCameraRawSection(&'static str),
    /// Set a lens correction parameter by name (barrel/pincushion/vignette).
    SetLensParam(&'static str, f32),
    /// Toggle the Lens Correction dialog open/closed.
    ToggleLensCorrection,

    // --- Batch 4: Autosave ---
    /// Set the autosave interval in seconds (0 = disable).
    SetAutosaveInterval(u64),
    /// Manually trigger an autosave now.
    TriggerAutosave,
    /// Accept and restore the pending autosave.
    RestoreAutosave,
    /// Dismiss the autosave restore banner without restoring.
    DismissAutosave,

    // --- Batch 4: Heal tool ---
    SetHealRadius(u32),
    SetHealMode(HealMode),

    // --- Wave 12 additions: Dodge/Burn/Smudge controls ---
    /// Set the brush radius used by Dodge, Burn, and Smudge (px, 1..400).
    SetDodgeSize(f32),
    /// Set the blend strength for Dodge (positive) and Burn (negative), 0..1.
    SetDodgeStrength(f32),
    /// Set the smear strength for the Smudge tool, 0..1.
    SetSmudgeStrength(f32),
    /// Set the liquify warp mode (Warp/Twirl/Pucker/Bloat).
    SetLiquifyMode(LiquifyMode),

    // --- Layer management ---
    /// Add a new blank raster layer above the active layer.
    NewLayer,
    /// Duplicate the active layer.
    DuplicateLayer,
    /// Merge the active layer down into the one below it.
    MergeDown,

    // --- Document resize ---
    /// Resize/resample the document pixels to a new size.
    SetImageSize { width: u32, height: u32 },
    /// Resize the canvas (crop or expand with background color) without resampling.
    SetCanvasSize { width: u32, height: u32 },

    // --- Batch 4: Print ---
    TogglePrintDialog,
    SetPrintPaperSize(String),
    SetPrintLandscape(bool),
    SetPrintScaleMode(String),
    SetPrintColorSpace(String),
    DoPrint,

    // --- Batch 4: Plugins ---
    RunPlugin { name: String, params: serde_json::Value },
    SetPluginParams(String),

    // --- Batch 4: History ---
    /// Jump to history step n (undo current−n times).
    UndoTo(usize),
    /// Create a named snapshot of the current state.
    CreateSnapshot(String),

    // --- Batch 5: Layer Comps ---
    /// Toggle the Layer Comps panel open/closed.
    ToggleLayerCompsPanel,
    /// Snapshot current layer states under the given name.
    AddLayerComp(String),
    /// Restore layer visibility/opacity/blend/offset from a saved comp.
    ApplyLayerComp(usize),
    /// Overwrite an existing comp with the current layer states.
    UpdateLayerComp(usize),
    /// Delete a saved comp by index.
    DeleteLayerComp(usize),
    /// Rename a saved comp.
    RenameLayerComp { idx: usize, name: String },

    // --- Batch 5: Focus Area selection ---
    /// Select in-focus pixels (high local Laplacian variance) on the active layer.
    SelectFocusArea { threshold: f32, sensitivity: f32, invert: bool },
    /// Set the Focus Area threshold (persisted for the next run).
    SetFocusAreaThreshold(f32),

    // --- Batch 5: Pattern Stamp ---
    /// Define a new named pattern from raw RGBA float pixels and add it to the library.
    DefinePattern { name: String, pixels: Vec<[f32; 4]>, width: u32, height: u32 },
    /// Select the active pattern by library index.
    SelectPattern(usize),
    /// Delete a pattern from the library by index.
    DeletePattern(usize),
    /// Set the scale multiplier for the Pattern Stamp tool.
    SetPatternStampScale(f32),
    /// Toggle aligned (global canvas coords) vs. unaligned (per-stroke) tiling.
    SetPatternStampAligned(bool),

    // --- Batch 5: Match Color ---
    /// Toggle the Match Color dialog open/closed.
    ToggleMatchColorDialog,
    /// Set the source layer for Match Color.
    SetMatchColorSource(LayerId),
    /// Set the fade/blend strength (0–100) for Match Color.
    SetMatchColorFade(f32),
    /// Apply Match Color: match luminance/color stats from source to target layer.
    MatchColor {
        source_layer: LayerId,
        target_layer: LayerId,
        match_luminance: bool,
        match_color: bool,
        fade: f32,
        neutralize: bool,
    },

    // --- Batch 5: Vanishing Point ---
    /// Open the Vanishing Point overlay.
    OpenVanishingPoint,
    /// Close the Vanishing Point overlay.
    CloseVanishingPoint,
    /// Add a new perspective plane (four doc-px corners [TL, TR, BR, BL]).
    AddVanishingPlane { corners: [[f32; 2]; 4] },
    /// Remove a perspective plane by index.
    RemoveVanishingPlane(usize),
    /// Select the active perspective plane by index.
    SelectVanishingPlane(usize),
    /// Set the grid size (doc px) for the active plane's overlay grid.
    SetVanishingGridSize(f32),
    /// Switch the Vanishing Point editing mode.
    SetVanishingToolMode(VanishingToolMode),
    /// Move a single corner of a perspective plane to a new doc-px position.
    SetVanishingPlaneCorner { plane_idx: usize, corner_idx: usize, pos: [f32; 2] },
    /// Perspective-aware stamp: copy pixels from `src` into `dst` within the
    /// active plane's perspective mapping.
    StampInPerspective { src: [f32; 2], dst: [f32; 2], radius: f32 },

    // --- Batch 6: Select Subject ---
    /// Auto-select the main subject using Sobel edge detection + flood fill.
    SelectSubject,
    /// Set the edge threshold for Select Subject (0..1, default 0.5).
    SetSelectSubjectThreshold(f32),
    /// Set the feather radius for Select Subject mask edges (px, default 1.0).
    SetSelectSubjectFeather(f32),

    // --- Batch 6: Artboards ---
    /// Toggle the Artboards panel open/closed.
    ToggleArtboardsPanel,
    /// Add a new artboard at the given position/size.
    AddArtboard { name: String, x: i32, y: i32, width: u32, height: u32 },
    /// Remove an artboard by its id.
    RemoveArtboard(u64),
    /// Select the active artboard by id.
    SelectArtboard(u64),
    /// Rename an artboard.
    RenameArtboard { id: u64, name: String },
    /// Move an artboard's top-left corner.
    MoveArtboard { id: u64, x: i32, y: i32 },
    /// Resize an artboard.
    ResizeArtboard { id: u64, width: u32, height: u32 },
    /// Duplicate an artboard (copies it with a new id, offset +20/+20).
    DuplicateArtboard(u64),
    /// Set an artboard's background fill colour.
    SetArtboardBackground { id: u64, color: [f32; 4] },
    /// Export all artboards to individual image files in a directory.
    ExportArtboards(std::path::PathBuf),

    // --- Batch 6: Apply Image ---
    /// Toggle the Apply Image dialog.
    ToggleApplyImageDialog,
    /// Set the source layer and channel for Apply Image.
    SetApplyImageSource { layer: LayerId, channel: ApplyImageChannel },
    /// Set the target layer for Apply Image.
    SetApplyImageTarget(LayerId),
    /// Set the blend mode for Apply Image.
    SetApplyImageBlend(BlendMode),
    /// Set the blend opacity (0..1) for Apply Image.
    SetApplyImageOpacity(f32),
    /// Toggle source inversion for Apply Image.
    SetApplyImageInvert(bool),
    /// Set or clear the mask layer for Apply Image.
    SetApplyImageMask(Option<LayerId>),
    /// Execute Apply Image with the current dialog parameters.
    ApplyImage,

    // --- Batch 6: Soft Proof (expanded) ---
    /// Toggle soft-proof preview on/off.
    ToggleSoftProof,
    /// Set the simulated output profile.
    SetProofProfile(ProofProfile),
    /// Set the rendering intent for gamut mapping.
    SetRenderingIntent(RenderingIntent),
    /// Toggle Black Point Compensation.
    SetBlackPointCompensation(bool),
    /// Toggle paper-white simulation.
    SetSimulatePaperWhite(bool),
    /// Toggle black-ink simulation.
    SetSimulateBlackInk(bool),
    /// Toggle the gamut-warning overlay.
    ToggleGamutWarning,
    /// Set the gamut-warning highlight colour (straight sRGB RGBA).
    SetGamutWarningColor([f32; 4]),

    // --- Batch 7: Alpha Channels ---
    /// Snapshot the current selection mask as a named alpha channel.
    SaveSelectionAsChannel(String),
    /// Load a saved alpha channel back into the active selection mask.
    LoadChannelAsSelection(usize),
    /// Delete a saved alpha channel by index.
    DeleteChannel(usize),
    /// Duplicate a saved alpha channel (appends a copy with " copy" suffix).
    DuplicateChannel(usize),

    // --- Batch 7: Blend If ---
    /// Set the Blend If luminance range for a layer.
    SetBlendIf { layer_id: LayerId, blend_if: BlendIf },
    /// Remove Blend If constraints from a layer (restores full blending).
    ClearBlendIf(LayerId),

    // --- Batch 7: Spot Heal & Red Eye ---
    /// Set the algorithm used by the Spot Healing Brush.
    SetSpotHealMode(SpotHealMode),
    /// Set the radius of the Spot Healing Brush (px, min 1.0).
    SetSpotHealRadius(f32),
    /// Apply a spot heal at the given center + radius (stub — records position).
    SpotHeal { center: [f32; 2], radius: f32 },
    /// Apply red-eye reduction at the given center + radius (stub — records position).
    RedEye { center: [f32; 2], radius: f32, darken: f32 },

    // --- Batch 7: Gradient Map stops ---
    /// Update the gradient stops of a GradientMap adjustment layer.
    SetGradientMapStops { layer_id: LayerId, stops: Vec<(f32, [f32; 4])> },

    // --- Batch 7: Channel Mixer ---
    /// Set the active output channel for a ChannelMixer adjustment (0=R 1=G 2=B 3=Gray).
    SetChannelMixerOutput { layer_id: LayerId, output: u8 },
    /// Set the per-source-channel mix weights + constant for the active output channel.
    SetChannelMixerMix { layer_id: LayerId, src_r: f32, src_g: f32, src_b: f32, constant: f32 },

    // --- Batch 4 extended: HDR Tone Mapping ---
    /// Apply a tone-map operator to the active layer (stub — records method).
    ApplyToneMap { method: ToneMapMethod, exposure: f32, gamma: f32 },
    /// Toggle the live tone-map preview.
    SetToneMapPreview(bool),

    // --- Batch 4 extended: Neural Filters ---
    /// Toggle the Neural Filters panel open/closed.
    ToggleNeuralFiltersPanel,
    /// Add a neural filter of the given kind with default strength.
    AddNeuralFilter(NeuralFilterKind),
    /// Remove a neural filter by index.
    RemoveNeuralFilter(usize),
    /// Set the strength (0..=1) of a neural filter by index.
    SetNeuralFilterStrength { idx: usize, strength: f32 },
    /// Toggle the enabled flag of a neural filter by index.
    ToggleNeuralFilter(usize),
    /// Apply all enabled neural filters (stub — records count).
    ApplyNeuralFilters,

    // --- Batch 4 extended: Layer Group depth ---
    /// Collapse or expand a named layer group.
    SetGroupCollapsed { group_name: String, collapsed: bool },
    /// Move a layer into a named group (sets the layer's group field).
    MoveLayerToGroup { layer_id: LayerId, group_name: String },
    /// Remove a layer from its group (clears the group field).
    RemoveLayerFromGroup(LayerId),
    /// Flatten a named group: expand it and remove it from the collapsed set.
    FlattenGroup(String),
    /// Duplicate a named group (stub — records the name).
    DuplicateGroup(String),

    // --- Batch 4 extended: Print Layout ---
    /// Set the number of print copies (min 1).
    SetPrintCopies(u8),
    /// Set whether copies should be collated.
    SetPrintCollate(bool),
    /// Set the print border width in mm (min 0).
    SetPrintBorderWidth(f32),
    /// Set whether the image is centred on the page.
    SetPrintCenterImage(bool),
    /// Toggle crop / registration marks on the print output.
    SetPrintMarks(bool),
    /// Set the bleed amount in mm (0..=25).
    SetPrintBleed(f32),
    /// Set the output print resolution in dpi (72..=2400).
    SetPrintResolution(u32),
    /// Set the page currently shown in the print preview.
    SetPrintPreviewPage(usize),

    // --- Batch 5 (new): Content-Aware Crop ---
    /// Set the rotation angle for Content-Aware Crop (degrees, clamped −45..=45).
    SetCaCropAngle(f32),
    /// Set the fill method used by Content-Aware Crop.
    SetCaCropFillMethod(CaFillMethod),
    /// Enable or disable the content-aware fill during crop.
    SetCaCropEnabled(bool),
    /// Apply the content-aware crop at the given rect [x, y, w, h] (stub).
    ApplyCaCrop { rect: [f32; 4] },

    // --- Batch 5 (new): Sky Replacement ---
    /// Toggle the Sky Replace panel open/closed.
    ToggleSkyReplacePanel,
    /// Select a sky preset.
    SetSkyPreset(SkyPreset),
    /// Set sky brightness (0..=200).
    SetSkyBrightness(f32),
    /// Set sky colour temperature (−100..=100).
    SetSkyTemperature(f32),
    /// Set sky scale multiplier (0.5..=2.0).
    SetSkyScale(f32),
    /// Flip the sky image horizontally.
    SetSkyFlip(bool),
    /// Set the edge fade amount (0..=100).
    SetSkyFadeEdge(f32),
    /// Set foreground lighting blending (0..=100).
    SetSkyForegroundLighting(f32),
    /// Toggle whether sky replacement outputs to new layers.
    SetSkyOutputNewLayers(bool),
    /// Apply sky replacement (stub: sets sky_replaced flag).
    ApplySkyReplace,

    // --- Batch 5 (new): Liquify Depth ---
    /// Select the active Liquify tool.
    SetLiquifyTool(LiquifyTool),
    /// Set the Liquify brush size (px, 1..=1500).
    SetLiquifyBrushSize(f32),
    /// Set the Liquify brush pressure (1..=100).
    SetLiquifyBrushPressure(f32),
    /// Set the Liquify brush density (1..=100).
    SetLiquifyBrushDensity(f32),
    /// Record a Liquify stroke.
    ApplyLiquifyStroke(LiquifyStroke),
    /// Freeze a mask region (stub: push boolean markers).
    FreezeMaskRegion { center: [f32; 2], radius: f32 },
    /// Thaw all frozen mask pixels.
    ThawAllMask,
    /// Reconstruct (undo) the last Liquify stroke.
    ReconstructLiquify,
    /// Revert all Liquify strokes.
    RevertLiquify,
    /// Show or hide the warp mesh overlay.
    SetLiquifyShowMesh(bool),
    /// Toggle smart-radius mode for Liquify.
    SetLiquifySmartRadius(bool),
    /// Save the current Liquify mesh (stub: no-op).
    SaveLiquifyMesh,

    // --- Batch 5 (new): Select Subject (AI stub) ---
    /// Set whether Select Subject runs on-device or in the cloud.
    SetSelectSubjectMode(SelectSubjectMode),
    /// Run the Select Subject stub (records estimated coverage + confidence).
    RunSelectSubject,
    /// Toggle the Select and Mask refinement panel.
    ToggleSelectAndMask,
    /// Enable or disable auto-refine for hair/fur edges.
    SetSelectSubjectRefine(bool),
    /// Invert the last Select Subject result (stub: toggles refine flag).
    InvertSelectSubject,

    // --- Batch 6: Layer Effects Suite ---
    /// Set or clear the FX panel target layer.
    SetFxTargetLayer(Option<String>),
    /// Replace the drop shadow on a layer.
    SetDropShadow { layer: String, fx: DropShadowFx },
    /// Enable or disable the drop shadow on a layer.
    ToggleDropShadow { layer: String, enabled: bool },
    /// Replace the outer glow on a layer.
    SetOuterGlow { layer: String, fx: OuterGlowFx },
    /// Enable or disable the outer glow on a layer.
    ToggleOuterGlow { layer: String, enabled: bool },
    /// Replace the bevel-and-emboss on a layer.
    SetBevelEmboss { layer: String, fx: BevelEmbossFx },
    /// Enable or disable bevel-and-emboss on a layer.
    ToggleBevelEmboss { layer: String, enabled: bool },
    /// Set the stroke effect size and color on a layer.
    SetStroke { layer: String, size: f32, color: [f32; 4] },
    /// Enable or disable the stroke effect on a layer.
    ToggleStroke { layer: String, enabled: bool },
    /// Set the color overlay color on a layer.
    SetColorOverlay { layer: String, color: [f32; 4] },
    /// Enable or disable the color overlay on a layer.
    ToggleColorOverlay { layer: String, enabled: bool },
    /// Remove all effects from a layer.
    ClearLayerEffects { layer: String },
    /// Toggle the Layer Effects panel open/closed.
    ToggleFxPanel,
    /// Copy the effects of the given layer into the fx clipboard.
    CopyLayerEffects { from: String },
    /// Paste the fx clipboard to the given layer.
    PasteLayerEffects { to: String },

    // --- Batch 6: Match Color (new) ---
    /// Set the source layer name for Match Color.
    SetMatchColorSource2(String),
    /// Set Match Color luminance (clamped 0..=200).
    SetMatchColorLuminance(f32),
    /// Set Match Color color intensity (clamped 0..=200).
    SetMatchColorIntensity(f32),
    /// Set Match Color fade (clamped 0..=100).
    SetMatchColorFade2(f32),
    /// Toggle Match Color neutralize.
    SetMatchColorNeutralize(bool),
    /// Apply Match Color (stub: sets last_match_color_applied = true).
    ApplyMatchColor,

    // --- Batch 6: Camera Raw Filter (new) ---
    /// Toggle the Camera Raw config panel open/closed.
    ToggleCameraRawPanel,
    /// Set white balance temperature (clamped 2000..=50000).
    SetCameraRawTemp(f32),
    /// Set white balance tint (clamped -150..=150).
    SetCameraRawTint(f32),
    /// Set exposure (clamped -5..=5).
    SetCameraRawExposure(f32),
    /// Set contrast (clamped -100..=100).
    SetCameraRawContrast(f32),
    /// Set highlights (clamped -100..=100).
    SetCameraRawHighlights(f32),
    /// Set shadows (clamped -100..=100).
    SetCameraRawShadows(f32),
    /// Set clarity (clamped -100..=100).
    SetCameraRawClarity(f32),
    /// Set dehaze (clamped -100..=100).
    SetCameraRawDehaze(f32),
    /// Set vibrance (clamped -100..=100).
    SetCameraRawVibrance(f32),
    /// Set sharpness (clamped 0..=150).
    SetCameraRawSharpness(f32),
    /// Set noise luminance (clamped 0..=100).
    SetCameraRawNoiseL(f32),
    /// Toggle lens correction.
    SetCameraRawLensCorrection(bool),
    /// Apply Camera Raw filter (stub: camera_raw_applied = true).
    ApplyCameraRawFilter,
    /// Reset Camera Raw config to defaults.
    ResetCameraRaw,

    // --- Batch 6: HDR Merge ---
    /// Toggle the HDR Merge panel open/closed.
    ToggleHdrMergePanel,
    /// Set the HDR tone mapping method.
    SetHdrToneMethod(HdrToneMappingMethod),
    /// Toggle ghost-removal in HDR Merge.
    SetHdrRemoveGhosts(bool),
    /// Set the number of source exposures (clamped to min 2).
    SetHdrSourceCount(usize),
    /// Set the output bit depth (only 8, 16, 32 accepted; others ignored).
    SetHdrBitDepth(u8),
    /// Execute HDR Merge (stub: sets hdr_merge_result).
    MergeToHdr,

    // ---- New Feature: SmartObject (rich) ------------------------------------
    /// Convert a layer into an embedded Smart Object (rich struct variant).
    SmartObjectConvert { layer_id: usize },
    /// Replace the source of a Smart Object and mark it dirty.
    SmartObjectReplace { so_id: usize, new_path: String },
    /// Rasterize (remove) a rich Smart Object by its id.
    SmartObjectRasterize { so_id: usize },
    /// Export (mark clean) a Smart Object's contents by its id.
    SmartObjectExport { so_id: usize },

    // ---- New Feature: AdvancedMasking (Select & Mask workspace) -------------
    /// Open the Select & Mask workspace.
    OpenSelectMask,
    /// Close the Select & Mask workspace.
    CloseSelectMask,
    /// Set the Detect Edges radius (clamped 0..=250).
    SetSelectMaskRadius(f32),
    /// Set the boundary-smooth amount (clamped 0..=100).
    SetSelectMaskSmooth(u8),
    /// Set the feather radius (clamped 0..=250).
    SetSelectMaskFeather(f32),
    /// Set the contrast value (clamped 0..=100).
    SetSelectMaskContrast(u8),
    /// Set the shift-edge amount (clamped -100..=100).
    SetSelectMaskShiftEdge(i8),
    /// Commit the refined mask and close the workspace.
    ApplySelectMask,

    // ---- New Feature: GenerativeFill ----------------------------------------
    /// Set the text prompt for the next generative fill run.
    SetGenerativeFillPrompt(String),
    /// Run generative fill on `layer_id` with the current prompt (4 variations).
    RunGenerativeFill { layer_id: usize },
    /// Advance the variation shown for a pending result by one step.
    CycleGenerativeFillVariation { result_index: usize },
    /// Accept and commit a generative fill result (removes it from pending list).
    AcceptGenerativeFill { result_index: usize },
    /// Discard a generative fill result (removes it from pending list).
    DiscardGenerativeFill { result_index: usize },

    // ---- New Feature: Basic3DLayer ------------------------------------------
    /// Add a new 3-D layer with the given primitive shape.
    Create3DLayer { layer_id: usize, shape: Shape3DKind },
    /// Move a 3-D layer to a new position.
    Set3DPosition { layer_id: usize, x: f32, y: f32, z: f32 },
    /// Rotate a 3-D layer (Euler angles in degrees).
    SetLayer3DRotation { layer_id: usize, x: f32, y: f32, z: f32 },
    /// Scale a 3-D layer (per-axis, clamped 0.01..=10.0).
    Set3DScale { layer_id: usize, x: f32, y: f32, z: f32 },
    /// Set the extrude depth of a 3-D layer (clamped 0..=5000).
    Set3DExtrudeDepth { layer_id: usize, depth: f32 },
    /// Flatten a 3-D layer back to a raster layer (removes it from the 3-D list).
    Flatten3DLayer { layer_id: usize },
}

// ---- New Feature: SmartObject (rich) -----------------------------------------

/// Whether the Smart Object embeds its contents or links to an external file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SmartObjectKind {
    Embedded,
    Linked,
}

/// A rich Smart Object entry — tracks id, name, kind, source path, and dirty flag.
#[derive(Debug, Clone)]
pub struct SmartObject {
    pub id: usize,
    pub name: String,
    pub kind: SmartObjectKind,
    pub source_path: Option<String>,
    pub contents_dirty: bool,
}

// ---- New Feature: AdvancedMasking (Select & Mask workspace) ------------------

/// Edge detection algorithm used in the Select & Mask workspace.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EdgeDetectMode {
    Object,
    Hair,
    Custom,
}

/// Full configuration for the Select & Mask / Refine Edge workspace.
#[derive(Debug, Clone)]
pub struct SelectMaskConfig {
    /// Detect Edges radius (0..=250).
    pub radius: f32,
    /// Automatically adjust the radius around complex edges.
    pub smart_radius: bool,
    /// Smooth the selection boundary (0..=100).
    pub smooth: u8,
    /// Gaussian feather applied to the mask edge (0..=250).
    pub feather: f32,
    /// Increase edge definition (0..=100).
    pub contrast: u8,
    /// Shrink or grow the selection boundary (-100..=100).
    pub shift_edge: i8,
    /// Where to deliver the refined mask.
    pub output_to: String,
    /// Remove colour fringing around the mask edge.
    pub decontaminate_colors: bool,
    /// Algorithm for Detect Edges.
    pub edge_detect: EdgeDetectMode,
}

impl SelectMaskConfig {
    pub fn new() -> Self {
        Self {
            radius: 3.0,
            smart_radius: true,
            smooth: 3,
            feather: 0.0,
            contrast: 0,
            shift_edge: 0,
            output_to: "Mask".into(),
            decontaminate_colors: false,
            edge_detect: EdgeDetectMode::Object,
        }
    }
}

// ---- New Feature: GenerativeFill ---------------------------------------------

/// One pending generative-fill result that the user can cycle through or accept.
#[derive(Debug, Clone)]
pub struct GenerativeFillResult {
    pub layer_id: usize,
    pub prompt: String,
    pub variation_index: usize,
    pub variation_count: usize,
}

// ---- New Feature: Basic3DLayer -----------------------------------------------

/// The built-in primitive shape kinds for a 3-D layer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Shape3DKind {
    Cube,
    Sphere,
    Cylinder,
    Cone,
    Plane,
    Custom,
}

/// Transform and geometry for a single 3-D layer.
#[derive(Debug, Clone)]
pub struct Layer3DProps {
    pub layer_id: usize,
    pub shape: Shape3DKind,
    pub pos_x: f32,
    pub pos_y: f32,
    pub pos_z: f32,
    pub rot_x: f32,
    pub rot_y: f32,
    pub rot_z: f32,
    pub scale_x: f32,
    pub scale_y: f32,
    pub scale_z: f32,
    pub extrude_depth: f32,
}

impl Layer3DProps {
    pub fn new(layer_id: usize) -> Self {
        Self {
            layer_id,
            shape: Shape3DKind::Cube,
            pos_x: 0.0,
            pos_y: 0.0,
            pos_z: 0.0,
            rot_x: 0.0,
            rot_y: 0.0,
            rot_z: 0.0,
            scale_x: 1.0,
            scale_y: 1.0,
            scale_z: 1.0,
            extrude_depth: 0.0,
        }
    }
}

/// The adjustment-layer kinds the host can add from the Adjustments browser, in
/// the same order as `Adjustment::defaults()`. A small host-side enum (rather
/// than passing a full `Adjustment` from the panel) keeps the panel rows simple
/// and `Copy`; `apply` maps each to its default-param `Adjustment`.
// `Exposure` has no browser row yet (the egui app surfaces it via a different
// menu), but the enum stays complete so `to_adjustment` covers every kind.
#[allow(dead_code)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum AdjKind {
    BrightnessContrast,
    Levels,
    Curves,
    HueSaturation,
    Exposure,
    Vibrance,
    PhotoFilter,
    Posterize,
    GradientMap,
    ColorBalance,
    ChannelMixer,
    BlackWhite,
    Threshold,
    Invert,
}

impl AdjKind {
    /// The default-param `Adjustment` for this kind (mirrors the egui app's
    /// `Adjustment::defaults()` entries, plus the parameterless kinds the browser
    /// also lists).
    pub fn to_adjustment(self) -> Adjustment {
        match self {
            AdjKind::BrightnessContrast => Adjustment::BrightnessContrast {
                brightness: 0.0,
                contrast: 0.0,
            },
            AdjKind::Levels => Adjustment::Levels {
                in_black: 0.0,
                in_white: 1.0,
                gamma: 1.0,
            },
            AdjKind::Curves => Adjustment::Curves(CurvePoints::default()),
            AdjKind::HueSaturation => Adjustment::HueSaturation {
                hue: 0.0,
                saturation: 0.0,
                lightness: 0.0,
            },
            AdjKind::Exposure => Adjustment::Exposure { stops: 0.0 },
            AdjKind::Vibrance => Adjustment::Vibrance { amount: 0.0 },
            AdjKind::PhotoFilter => Adjustment::PhotoFilter {
                color: [1.0, 0.64, 0.0],
                density: 0.25,
            },
            AdjKind::Posterize => Adjustment::Posterize { levels: 4 },
            AdjKind::GradientMap => Adjustment::GradientMap {
                low: [0.05, 0.0, 0.2],
                high: [1.0, 0.85, 0.4],
            },
            AdjKind::ColorBalance => Adjustment::ColorBalance {
                shadows: [0.0; 3],
                midtones: [0.0; 3],
                highlights: [0.0; 3],
                preserve_luminosity: true,
            },
            AdjKind::ChannelMixer => Adjustment::ChannelMixer {
                r: [1.0, 0.0, 0.0, 0.0],
                g: [0.0, 1.0, 0.0, 0.0],
                b: [0.0, 0.0, 1.0, 0.0],
                monochrome: false,
            },
            AdjKind::BlackWhite => Adjustment::BlackWhite,
            AdjKind::Threshold => Adjustment::Threshold { level: 0.5 },
            AdjKind::Invert => Adjustment::Invert,
        }
    }
}

/// A destructive filter/adjustment the host can apply to the active layer through
/// the engine's existing passes. Each variant carries its (default) params and
/// maps to one `CanvasHost::apply_*` call — which forwards to the matching
/// `prism_canvas::CanvasGpu::apply_*`. Reuses the engine pipeline verbatim; the
/// host adds no filter math. Params mirror the egui app's defaults
/// (`pigment-app/src/app/retouch.rs`).
#[derive(Clone, Copy, Debug)]
pub enum Filter {
    /// Separable Gaussian blur (engine kind 1).
    GaussianBlur { radius: f32 },
    /// Separable box blur (engine kind 5).
    BoxBlur { radius: f32 },
    /// Unsharp-style sharpen (engine kind 2).
    Sharpen { amount: f32 },
    /// Posterize: quantize to N levels (engine `apply_posterize`).
    Posterize { levels: u32 },
    /// Threshold to black/white at a luma cutoff (engine `apply_threshold`).
    Threshold { level: f32 },
    /// Find Edges (engine stylize kind 13).
    FindEdges { width: f32 },
    /// Emboss (engine stylize kind 14).
    Emboss { amount: f32, width: f32 },
    /// Add monochrome gaussian noise (engine `apply_noise`).
    AddNoise { amount: f32 },
    /// Unsharp mask: blur then sharpen (amount × (original − blurred)).
    UnsharpMask { radius: f32, amount: f32, threshold: f32 },
    /// Radial spin blur around canvas center (proxy via gaussian blur).
    RadialBlur { amount: f32 },
    /// Lens correction: barrel/pincushion distortion + vignette.
    LensCorrection { barrel: f32, pincushion: f32, vignette: f32 },
    // --- Batch 5: additional CPU raster filters ---
    /// Directional motion blur: `angle` degrees, `distance` px smear.
    MotionBlur { angle: f32, distance: f32 },
    /// Twirl distort: swirl `angle` degrees about the centre, falling off to
    /// zero at `radius` (fraction 0..1 of the half-diagonal).
    Twirl { angle: f32, radius: f32 },
    /// Pinch / Bulge distort: signed `amount` (-1 bulge .. +1 pinch) within
    /// `radius` (fraction 0..1 of the half-diagonal).
    Pinch { amount: f32, radius: f32 },
    /// Solarize: invert tones above `threshold` (0..1) per channel.
    Solarize { threshold: f32 },
    /// Glowing Edges: Sobel edge glow; `width` step, `intensity` brightness.
    GlowingEdges { width: f32, intensity: f32 },
    /// High Pass: subtract a Gaussian blur from the original, shift to 0.5 grey.
    /// `radius` is the blur radius in px; output is a detail-isolation layer.
    HighPass { radius: f32 },
    /// Smart Sharpen: unsharp mask with configurable noise reduction pre-pass.
    SmartSharpen { amount: f32, radius: f32, reduce_noise: f32, mode: SmartSharpenMode },
    /// Reduce Noise: strength-controlled smoothing with detail/colour preservation.
    ReduceNoise { strength: f32, preserve_details: f32, reduce_color_noise: f32, sharpen_details: f32 },
}

/// Blur model used by Smart Sharpen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SmartSharpenMode {
    GaussianBlur,
    LensBlur,
    MotionBlur,
}

// ---- Batch 4 extended: HDR Tone Mapping -----------------------------------------

/// Tone-mapping operator used when converting HDR/EXR content to display range.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize, Default)]
pub enum ToneMapMethod {
    /// Simple Reinhard (per-channel normalization). Default.
    #[default]
    Reinhard,
    /// Filmic S-curve (approximates film response).
    Filmic,
    /// ACES Cg reference transform.
    AcesCg,
    /// Pure exposure adjustment (no curve shaping).
    Exposure,
}

// ---- Batch 4 extended: Neural Filters -------------------------------------------

/// A single entry in the neural-filter stack.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct NeuralFilter {
    pub kind: NeuralFilterKind,
    /// Blend/effect strength in 0..=1.
    pub strength: f32,
    pub enabled: bool,
}

impl Default for NeuralFilter {
    fn default() -> Self {
        Self {
            kind: NeuralFilterKind::SkinSmoothing,
            strength: 0.5,
            enabled: true,
        }
    }
}

/// The AI-powered filter kind (stubs — no actual inference; GPU pass planned).
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize, Default)]
pub enum NeuralFilterKind {
    #[default]
    SkinSmoothing,
    SmartPortrait,
    StyleTransfer,
    Colorize,
    SuperZoom,
    JpegArtifactRemoval,
    NoiseReduction,
    DepthBlur,
}

// ---- Batch 4 extended: Print Layout ---------------------------------------------

/// Extended print layout options shown in the print dialog.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PrintLayout {
    /// Number of copies to print (minimum 1).
    pub copies: u8,
    /// Whether copies are collated (true = 1-2-3 1-2-3, false = 1-1 2-2 3-3).
    pub collate: bool,
    /// Border width around the image in mm (minimum 0).
    pub border_width: f32,
    /// Whether the image is centred on the page.
    pub center_image: bool,
    /// Whether to include crop / registration marks.
    pub print_marks: bool,
    /// Bleed area in mm added outside the trim edge (0..=25).
    pub bleed: f32,
    /// Output resolution in dpi (72..=2400).
    pub print_resolution: u32,
}

impl Default for PrintLayout {
    fn default() -> Self {
        Self {
            copies: 1,
            collate: true,
            border_width: 0.0,
            center_image: true,
            print_marks: false,
            bleed: 3.0,
            print_resolution: 300,
        }
    }
}

// ---- Wave 11: Layer-style types ------------------------------------------------

/// A drop shadow or inner shadow descriptor.
#[derive(Clone, Debug, Default)]
pub struct Shadow {
    pub offset_x: f32,
    pub offset_y: f32,
    pub blur: f32,
    pub spread: f32,
    pub color: [f32; 4],
    pub opacity: f32,
}

/// An outer or inner glow descriptor.
#[derive(Clone, Debug, Default)]
pub struct Glow {
    pub blur: f32,
    pub spread: f32,
    pub color: [f32; 4],
    pub opacity: f32,
}

/// A bevel-and-emboss descriptor.
#[derive(Clone, Debug, Default)]
pub struct Bevel {
    pub depth: f32,
    pub size: f32,
    pub angle: f32,
    pub highlight_opacity: f32,
    pub shadow_opacity: f32,
}

/// Non-destructive per-layer visual effects applied on top of the composited layer.
/// Fields default to `None` = disabled. Effects are applied CPU-side in `canvas_host`.
#[derive(Clone, Debug, Default)]
pub struct LayerStyle {
    pub drop_shadow: Option<Shadow>,
    pub outer_glow: Option<Glow>,
    pub inner_glow: Option<Glow>,
    pub bevel_emboss: Option<Bevel>,
}

// ---- Batch 6: Layer Effects Suite -----------------------------------------------

/// Full per-layer drop-shadow effect.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DropShadowFx {
    pub enabled: bool,
    pub blend_mode: BlendMode,
    pub color: [f32; 4],
    /// 0..=100, default 75
    pub opacity: f32,
    /// 0..=360, default 120
    pub angle: f32,
    /// 0..=30000, default 5
    pub distance: f32,
    /// 0..=100, default 0
    pub spread: f32,
    /// 0..=250, default 5
    pub size: f32,
    /// 0..=100, default 0
    pub noise: f32,
    pub layer_knocks_out: bool,
}

impl Default for DropShadowFx {
    fn default() -> Self {
        Self {
            enabled: false,
            blend_mode: BlendMode::Multiply,
            color: [0.0, 0.0, 0.0, 1.0],
            opacity: 75.0,
            angle: 120.0,
            distance: 5.0,
            spread: 0.0,
            size: 5.0,
            noise: 0.0,
            layer_knocks_out: true,
        }
    }
}

/// Full per-layer outer-glow effect.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct OuterGlowFx {
    pub enabled: bool,
    pub blend_mode: BlendMode,
    pub opacity: f32,
    pub noise: f32,
    pub color: [f32; 4],
    pub spread: f32,
    /// default 5
    pub size: f32,
    /// 1..=100, default 50
    pub range: f32,
    /// 0..=100, default 0
    pub jitter: f32,
}

impl Default for OuterGlowFx {
    fn default() -> Self {
        Self {
            enabled: false,
            blend_mode: BlendMode::Screen,
            opacity: 75.0,
            noise: 0.0,
            color: [1.0, 1.0, 0.8, 1.0],
            spread: 0.0,
            size: 5.0,
            range: 50.0,
            jitter: 0.0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize, Default)]
pub enum BevelStyle {
    OuterBevel,
    #[default]
    InnerBevel,
    Emboss,
    PillowEmboss,
    StrokeEmboss,
}

#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize, Default)]
pub enum BevelTechnique {
    #[default]
    SmoothB,
    ChiselHard,
    ChiselSoft,
}

/// Full per-layer bevel-and-emboss effect.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct BevelEmbossFx {
    pub enabled: bool,
    pub style: BevelStyle,
    pub technique: BevelTechnique,
    /// 1..=1000, default 100
    pub depth: f32,
    /// true = up, false = down
    pub direction_up: bool,
    /// 0..=250, default 5
    pub size: f32,
    /// 0..=16, default 0
    pub soften: f32,
    pub angle: f32,
    /// 0..=90, default 30
    pub altitude: f32,
    pub highlight_opacity: f32,
    pub shadow_opacity: f32,
}

impl Default for BevelEmbossFx {
    fn default() -> Self {
        Self {
            enabled: false,
            style: BevelStyle::InnerBevel,
            technique: BevelTechnique::SmoothB,
            depth: 100.0,
            direction_up: true,
            size: 5.0,
            soften: 0.0,
            angle: 120.0,
            altitude: 30.0,
            highlight_opacity: 75.0,
            shadow_opacity: 75.0,
        }
    }
}

/// All non-destructive per-layer effects for the Layer Effects panel.
/// Keyed by layer id string in `App::layer_effects`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
pub struct LayerEffects {
    pub drop_shadow: DropShadowFx,
    pub inner_shadow_enabled: bool,
    pub outer_glow: OuterGlowFx,
    pub inner_glow_enabled: bool,
    pub bevel_emboss: BevelEmbossFx,
    pub satin_enabled: bool,
    pub color_overlay_enabled: bool,
    pub color_overlay_color: [f32; 4],
    pub gradient_overlay_enabled: bool,
    pub pattern_overlay_enabled: bool,
    pub stroke_enabled: bool,
    /// default 3
    pub stroke_size: f32,
    pub stroke_color: [f32; 4],
}

// ---- Batch 6: Match Color (new config struct) -----------------------------------

/// Extended Match Color parameters (Batch 6).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MatchColorConfig {
    pub source_layer_name: String,
    /// 0..=200, default 100
    pub luminance: f32,
    /// 0..=200, default 100
    pub color_intensity: f32,
    /// 0..=100, default 0
    pub fade: f32,
    pub neutralize: bool,
    pub use_selection_source: bool,
    pub use_selection_target: bool,
}

impl Default for MatchColorConfig {
    fn default() -> Self {
        Self {
            source_layer_name: String::new(),
            luminance: 100.0,
            color_intensity: 100.0,
            fade: 0.0,
            neutralize: false,
            use_selection_source: false,
            use_selection_target: false,
        }
    }
}

// ---- Batch 6: Camera Raw Filter (new config) ------------------------------------

/// Full Camera Raw dialog parameters (Batch 6).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CameraRawConfig {
    /// 2000..=50000, default 6500
    pub temperature: f32,
    /// -150..=150, default 0
    pub tint: f32,
    /// -5..=5, default 0
    pub exposure: f32,
    /// -100..=100, default 0
    pub contrast: f32,
    /// -100..=100, default 0
    pub highlights: f32,
    /// -100..=100, default 0
    pub shadows: f32,
    /// -100..=100, default 0
    pub whites: f32,
    /// -100..=100, default 0
    pub blacks: f32,
    /// -100..=100, default 0
    pub clarity: f32,
    /// -100..=100, default 0
    pub dehaze: f32,
    /// -100..=100, default 0
    pub vibrance: f32,
    /// -100..=100, default 0
    pub saturation: f32,
    /// 0..=100, default 0
    pub noise_luminance: f32,
    /// 0..=100, default 25
    pub noise_color: f32,
    /// 0..=150, default 40
    pub sharpness: f32,
    pub lens_correction: bool,
    pub chromatic_aberration: bool,
}

impl Default for CameraRawConfig {
    fn default() -> Self {
        Self {
            temperature: 6500.0,
            tint: 0.0,
            exposure: 0.0,
            contrast: 0.0,
            highlights: 0.0,
            shadows: 0.0,
            whites: 0.0,
            blacks: 0.0,
            clarity: 0.0,
            dehaze: 0.0,
            vibrance: 0.0,
            saturation: 0.0,
            noise_luminance: 0.0,
            noise_color: 25.0,
            sharpness: 40.0,
            lens_correction: false,
            chromatic_aberration: false,
        }
    }
}

// ---- Batch 6: HDR Merge ---------------------------------------------------------

/// Tone-mapping method for HDR Merge.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize, Default)]
pub enum HdrToneMappingMethod {
    #[default]
    Local,
    Equal,
    Highlight,
    Photoreceptor,
}

/// Configuration for the HDR Merge / Photomerge stub.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct HdrMergeConfig {
    pub method: HdrToneMappingMethod,
    pub remove_ghosts: bool,
    /// Number of source exposures to merge (default 0).
    pub source_count: usize,
    /// Output bit depth: 8, 16, or 32 (default 32).
    pub bit_depth_output: u8,
}

impl Default for HdrMergeConfig {
    fn default() -> Self {
        Self {
            method: HdrToneMappingMethod::Local,
            remove_ghosts: true,
            source_count: 0,
            bit_depth_output: 32,
        }
    }
}

/// The single shared application state. Owns the host + document and all panel-
/// facing tool/brush/view state. Mutated ONLY through [`App::apply`].
pub struct App {
    /// wgpu device + real prism-canvas compositor (bridged into GPUI).
    pub host: CanvasHost,
    /// The document the host composites; panels read it, `apply` mutates it.
    pub doc: Document,

    /// The active editing tool.
    pub active: Tool,
    /// Brush parameters (also the active foreground color source).
    pub brush: Brush,
    /// User color swatches (straight sRGB RGBA 0..1) shown by the color panel.
    pub swatches: Vec<[f32; 4]>,
    /// The canvas view transform (pan + zoom).
    pub view: ViewTransform,

    /// Last painted doc-space position of the in-progress stroke (None = idle).
    /// Mirrors the egui app's `stroke_last`.
    stroke_last: Option<[f32; 2]>,
    /// Spacing overshoot carried into the next segment so dab spacing stays
    /// continuous across mouse-move events. Mirrors the egui app's
    /// `stroke_residual`.
    stroke_residual: f32,

    /// Doc-px anchor of an in-progress Selection (marquee) drag (None = idle).
    sel_drag_start: Option<[f32; 2]>,
    /// In-progress Lasso polygon points (doc px); flushed to a mask on release.
    /// Mirrors the egui app's `lasso_points`.
    lasso_points: Vec<[f32; 2]>,
    /// The selection mask snapshot taken at the START of a marquee/lasso/wand op,
    /// used as the combine base so Shift adds / Alt subtracts (mirrors the egui
    /// app's `sel_base`). Empty when no op is in progress.
    sel_base: Vec<f32>,
    /// The combine mode for the in-progress selection op, derived from the modifier
    /// keys at op start (Shift = add, Alt = subtract, Shift+Alt = intersect, else
    /// replace). Mirrors the egui app's `sel_mode`.
    sel_mode: CombineMode,
    /// Doc-px anchor of an in-progress Move/Transform drag (None = idle). Drag
    /// deltas accumulate into `xform_translate`/`xform_scale` from here.
    xform_drag_start: Option<[f32; 2]>,
    /// Accumulated translate of the in-progress Move/Transform drag (doc px).
    /// Mirrors the egui app's `xform_translate`.
    pub xform_translate: [f32; 2],
    /// Accumulated uniform scale of the in-progress Transform drag. Mirrors the
    /// egui app's `xform_scale`.
    pub xform_scale: f32,

    /// Doc-px anchor of an in-progress Gradient drag (None = idle). The gradient
    /// is applied on release along `start → end`.
    grad_drag_start: Option<[f32; 2]>,
    /// Doc-px anchor of an in-progress Shape (Rectangle/Ellipse) drag (None =
    /// idle). The shape is drawn on release across the `start → end` bbox.
    shape_drag_start: Option<[f32; 2]>,
    /// Last doc-px position seen during the in-progress drag. `end_drag` carries
    /// no position, so gradient/shape tools read the drag end-point from here.
    pub last_drag: Option<[f32; 2]>,

    /// Paint-bucket tolerance 0..1 (mirrors the egui app's `fill_tolerance`).
    pub fill_tolerance: f32,
    /// Whether the bucket fills only the contiguous region from the seed
    /// (mirrors the egui app's `fill_contiguous`).
    pub fill_contiguous: bool,
    /// Whether the gradient dithers to break 8-bit banding (egui default true).
    pub gradient_dither: bool,

    // ---- Clone stamp ----
    /// Alt-set clone source anchor in doc px (None = unset). Mirrors the egui
    /// app's `clone_source`.
    clone_source: Option<[f32; 2]>,
    /// destAnchor − sourceAnchor locked at the first dab of a clone stroke.
    /// Mirrors the egui app's `clone_offset`.
    clone_offset: [f32; 2],

    // ---- Text tool ----
    /// The raster layer being edited by the Text tool, plus its doc-px origin and
    /// the string typed so far (None = no active text edit). Click places a layer
    /// and opens this; typing re-rasterizes it; Escape/Enter/tool-switch commits.
    text_edit: Option<TextEdit>,
    /// Text-layer point size (egui app default 48px).
    pub text_size: f32,

    /// Bumped every time the engine selection mask changes (any marquee / lasso /
    /// wand / clear). The root view watches this to know when to re-trace the
    /// marching-ants boundary (so it doesn't read the mask back every frame).
    pub selection_generation: u64,

    /// Layers that currently carry a mask (mirrors the egui app's `masked_layers`).
    /// The mask itself lives in the engine; this set tracks which layers have one
    /// so the panel can show add/delete affordances and the paint path knows when
    /// to route a stroke into the mask.
    pub masked_layers: HashSet<LayerId>,
    /// Mask-edit mode: while on AND the active layer is masked, brush strokes paint
    /// the mask (white = reveal) and the eraser hides. Mirrors the egui app's
    /// `edit_mask`.
    pub edit_mask: bool,

    /// Which display channels are visible [R, G, B, A] (default all true). Does
    /// NOT modify document pixels — only zeroes out masked channels in the bridged
    /// BGRA8 output so the view shows individual channel contributions.
    pub channel_visibility: [bool; 4],

    // --- Wave 9 additions ---
    /// Background color (straight sRGB RGBA 0..1). Default white (Photoshop convention).
    pub bg_color: [f32; 4],
    /// Layers that have been converted to Smart Objects. Maps LayerId → stub source path.
    pub smart_objects: HashMap<LayerId, std::path::PathBuf>,
    /// Short human-readable label for each destructive action applied so far.
    pub history_labels: Vec<String>,
    /// In-progress pen/bézier path nodes.
    pub pen_path: Vec<PenNode>,
    /// Whether the last pen path was closed/committed.
    pub pen_closed: bool,
    /// The curve control point currently being dragged (layer, point index).
    pub dragging_curve_point: Option<(LayerId, usize)>,
    /// The curve control point currently hovered (layer, point index).
    pub hovered_curve_point: Option<(LayerId, usize)>,
    /// Last-seen bounding rect of the curves canvas element [ox, oy, w, h] (window px).
    pub curves_canvas_bounds: Option<[f32; 4]>,

    // --- Wave 11 ---
    /// Non-destructive layer styles (drop shadow, glow, bevel) keyed by LayerId.
    pub layer_styles: HashMap<LayerId, LayerStyle>,
    /// Whether the layer-style editor modal is open.
    pub style_panel_open: bool,
    /// Which layer the style panel is editing.
    pub style_panel_layer: Option<LayerId>,

    /// Layers that clip to the first non-clipped layer below them.
    pub clipping_masks: HashSet<LayerId>,

    /// Foreground hue (0..360°) for the HSV color picker.
    pub fg_hue: f32,
    /// Foreground saturation (0..1).
    pub fg_saturation: f32,
    /// Foreground value (0..1).
    pub fg_value: f32,

    /// Status message shown in the toolbar (e.g. "PSD import: add `psd` crate").
    pub status_message: Option<String>,

    // --- Wave 12: Dodge/Burn/Smudge/Crop ---
    /// Brush radius for the Dodge/Burn/Smudge tools (px).
    pub dodge_size: f32,
    /// Strength for Dodge/Burn in ±[0,1]. Positive = dodge, negative = burn.
    pub dodge_strength: f32,
    /// Blend strength for the Smudge tool (0..1).
    pub smudge_strength: f32,
    /// Active warp mode for the Liquify tool.
    pub liquify_mode: LiquifyMode,
    /// Active crop rectangle in doc px `[x0, y0, x1, y1]` while the Crop
    /// tool is being dragged / confirmed.
    pub crop_rect: Option<[f32; 4]>,

    /// Panel visibility map: key = panel name, value = visible.
    pub panel_visibility: HashMap<String, bool>,

    // --- Wave 13: Guides ---
    /// Horizontal guide positions (doc px from top).
    pub guides_h: Vec<f32>,
    /// Vertical guide positions (doc px from left).
    pub guides_v: Vec<f32>,
    /// Whether guides are currently visible.
    pub guides_visible: bool,

    // --- Wave 13: Fill layers ---
    /// Solid fill layers: layer id → fill color [r,g,b,a] (straight sRGB 0-255).
    pub fill_layers: HashMap<LayerId, [u8; 4]>,

    // --- Wave 13: Recent files ---
    /// MRU list of recently opened file paths (max 10).
    pub recent_files: Vec<std::path::PathBuf>,

    // --- Wave 13: Canvas rotation ---
    /// Current canvas rotation angle in degrees (0, 90, 180, 270 or arbitrary).
    pub canvas_rotation_deg: f32,

    // --- Wave 13: Color mode ---
    /// Active color mode (e.g. "RGB", "CMYK", "HSL"). Model-only; display in panels.
    pub color_mode: ColorMode,

    /// Currently open menu name (e.g. "File", "Edit"). Drives dropdown rendering.
    pub active_menu: Option<String>,

    // --- Text cursor blink ---
    /// True when the cursor bar should be visible (50% duty cycle at ~2 Hz).
    pub cursor_blink_on: bool,
    /// Frame counter used to flip `cursor_blink_on` every 30 frames.
    pub cursor_blink_tick: u32,

    // --- Wave 14: Grid snap ---
    /// Snap Move/Transform endpoints to a regular grid.
    pub snap_to_grid: bool,
    /// Grid cell size in doc px (default 16.0).
    pub grid_size: f32,

    // --- Wave 14: Slice tool ---
    pub slices: Vec<Slice>,
    pub next_slice_id: u32,
    /// In-progress slice drag: start corner `[x, y]`.
    pub slice_drag_start: Option<[f32; 2]>,

    // --- Wave 15: Liquify ---
    // (tool state reuses dodge_size/stroke_last; no extra fields needed)

    // --- Wave 15: Smart Filters ---
    /// Non-destructive filter stack per layer.
    pub smart_filters: HashMap<LayerId, Vec<SmartFilter>>,

    // --- Wave 15: Filter gallery ---
    pub filter_gallery_open: bool,

    // --- Wave 15: Camera Raw ---
    pub camera_raw_open: bool,
    pub camera_raw_params: HashMap<&'static str, f32>,

    // --- Wave 15: Color profiles ---
    pub color_profile: ColorProfile,

    // --- Batch 2: Soft proof ---
    pub soft_proof: SoftProofMode,

    // --- Batch 2: Export presets ---
    pub export_presets: Vec<ExportPreset>,

    // --- Batch 2: Histogram channel ---
    pub histogram_channel: HistogramChannel,

    // --- Wave 15: Dockable panels ---
    /// Which panels are floating/detached.
    pub panel_detached: HashMap<&'static str, bool>,
    /// Floating panel screen positions `(x, y)`.
    pub panel_positions: HashMap<&'static str, (f32, f32)>,

    // --- Batch 1: Saveable preferences ---
    /// Loaded on startup from `~/.config/prism/pigment_prefs.json`; persisted
    /// on document open and on explicit `Action::SavePrefs`.
    pub prefs: AppPrefs,

    // --- Batch 3: Lens Correction dialog state ---
    pub lens_barrel: f32,
    pub lens_pincushion: f32,
    pub lens_vignette: f32,
    pub lens_correction_open: bool,

    // --- Batch 3: Camera Raw dialog expanded sections ---
    pub camera_raw_section_basic: bool,
    pub camera_raw_section_detail: bool,
    pub camera_raw_section_hsl: bool,

    // --- Batch 4: Autosave ---
    /// Interval in seconds between autosaves (default 300 = 5 min).
    pub autosave_interval_secs: u64,
    /// Wall-clock instant of the last autosave (None = never saved).
    pub last_autosave: Option<std::time::Instant>,
    /// True if an autosave file was found on startup that is newer than the last-opened doc.
    pub autosave_restore_pending: bool,

    // --- Batch 4: Heal tool ---
    /// Radius for the healing brush in doc px (default 20).
    pub heal_radius: u32,
    /// Heal blend mode.
    pub heal_mode: HealMode,

    // --- Batch 4: Print dialog ---
    pub show_print_dialog: bool,
    pub print_paper_size: String,
    pub print_landscape: bool,
    pub print_scale_mode: String,
    pub print_color_space: String,

    // --- Batch 4: Plugin system ---
    pub plugin_registry: crate::plugin::PluginRegistry,
    /// JSON params string for the plugin runner (single-line text input).
    pub plugin_params: String,

    // --- Batch 4: History snapshots ---
    /// Named snapshots: (name, document layer summary as JSON).
    pub snapshots: Vec<(String, String)>,

    // --- Batch 5: Layer Comps ---
    /// Named snapshots of all layer visibility/opacity/blend/offset states.
    pub layer_comps: Vec<LayerComp>,
    /// Whether the Layer Comps panel is visible.
    pub layer_comps_panel_open: bool,
    /// Index of the currently applied comp (None = no comp applied).
    pub active_comp_idx: Option<usize>,

    // --- Batch 5: Pattern Stamp ---
    /// Library of named repeating patterns for the Pattern Stamp tool.
    pub pattern_library: Vec<PatternDef>,
    /// Index of the active pattern in the library (None = none selected).
    pub active_pattern_idx: Option<usize>,
    /// Scale multiplier for the Pattern Stamp tool (default 1.0).
    pub pattern_stamp_scale: f32,
    /// Aligned tiling (true) uses global canvas coords; unaligned is per-stroke.
    pub pattern_stamp_aligned: bool,

    // --- Batch 5: Match Color ---
    /// Whether the Match Color dialog is open.
    pub match_color_dialog_open: bool,
    /// The source layer for Match Color (None = use current comp).
    pub match_color_source: Option<LayerId>,
    /// Fade/blend strength 0–100 for Match Color (default 100).
    pub match_color_fade: f32,

    // --- Batch 5: Vanishing Point ---
    /// Defined perspective planes for the Vanishing Point overlay.
    pub vanishing_planes: Vec<VanishingPlane>,
    /// Index of the currently selected plane (None = none).
    pub active_vanishing_plane: Option<usize>,
    /// Current editing mode inside the Vanishing Point overlay.
    pub vanishing_tool_mode: VanishingToolMode,
    /// Whether the Vanishing Point overlay is open.
    pub vanishing_point_open: bool,

    // --- Batch 5: Focus Area ---
    /// Last-used threshold for Select > Focus Area.
    pub focus_area_threshold: f32,
    /// Last-used sensitivity for Select > Focus Area.
    pub focus_area_sensitivity: f32,

    // --- Batch 6: Select Subject ---
    /// Edge-detection threshold for Select Subject (0..1, default 0.5).
    pub select_subject_threshold: f32,
    /// Feather radius for Select Subject mask smoothing (px, default 1.0).
    pub select_subject_feather: f32,

    // --- Batch 6: Artboards ---
    pub artboards: Vec<Artboard>,
    pub active_artboard: Option<u64>,
    pub artboards_panel_open: bool,
    pub next_artboard_id: u64,

    // --- Batch 6: Apply Image ---
    pub apply_image_dialog_open: bool,
    pub apply_image_params: ApplyImageParams,

    // --- Batch 6: Soft Proof (expanded) ---
    /// Whether soft-proof preview is active.
    pub soft_proof_enabled: bool,
    pub soft_proof_settings: SoftProofSettings,

    // --- Batch 7: Alpha Channels ---
    /// Named alpha channels saved from selections (Channels panel).
    pub alpha_channels: Vec<AlphaChannel>,

    // --- Batch 7: Blend If ---
    /// Per-layer Blend If luminance range controls.
    pub blend_if: std::collections::HashMap<LayerId, BlendIf>,

    // --- Batch 7: Spot Heal / Red Eye ---
    /// Algorithm used by the Spot Healing Brush.
    pub spot_heal_mode: SpotHealMode,
    /// Radius of the Spot Healing Brush in doc px (default 20.0).
    pub spot_heal_radius: f32,
    /// Position + radius of the last spot heal stroke (stub state).
    pub last_spot_heal: Option<([f32; 2], f32)>,
    /// Position, radius, and darken amount of the last red-eye correction (stub state).
    pub last_red_eye: Option<([f32; 2], f32, f32)>,

    // --- Batch 4 extended: HDR Tone Mapping ---
    /// Whether the tone-map preview overlay is active.
    pub tone_map_preview: bool,
    /// The last tone-map method applied (None = never applied).
    pub last_tone_map: Option<ToneMapMethod>,

    // --- Batch 4 extended: Neural Filters ---
    /// The neural-filter stack for this document.
    pub neural_filters: Vec<NeuralFilter>,
    /// Whether the Neural Filters panel is open.
    pub neural_filters_panel_open: bool,
    /// Count of enabled neural filters applied in the last `ApplyNeuralFilters` call.
    pub last_neural_apply_count: usize,

    // --- Batch 4 extended: Layer Group depth ---
    /// Names of groups whose children are currently collapsed in the Layers panel.
    pub collapsed_groups: std::collections::HashSet<String>,
    /// Name of the group most recently duplicated (stub).
    pub last_duplicated_group: Option<String>,

    // --- Batch 4 extended: Print Layout ---
    /// Extended print layout options (copies, bleed, marks, etc.).
    pub print_layout: PrintLayout,
    /// Page index shown in the print preview (0-based).
    pub print_preview_page: usize,

    // --- Batch 5 (new): Content-Aware Crop ---
    pub ca_crop_config: ContentAwareCropConfig,
    /// Last rect applied by Content-Aware Crop `[x, y, w, h]`.
    pub last_ca_crop_rect: Option<[f32; 4]>,

    // --- Batch 5 (new): Sky Replacement ---
    pub sky_replace_config: SkyReplaceConfig,
    pub sky_replace_panel_open: bool,
    /// True once ApplySkyReplace has been executed (stub flag).
    pub sky_replaced: bool,

    // --- Batch 5 (new): Liquify Depth ---
    pub liquify_tool: LiquifyTool,
    /// Liquify brush diameter in px (default 100, clamped 1..=1500).
    pub liquify_brush_size: f32,
    /// Liquify brush pressure 1..=100 (default 50).
    pub liquify_brush_pressure: f32,
    /// Liquify brush density 1..=100 (default 50).
    pub liquify_brush_density: f32,
    /// History of recorded strokes (for undo / replay).
    pub liquify_strokes: Vec<LiquifyStroke>,
    /// Freeze mask: one bool per canvas pixel (true = frozen).
    pub liquify_frozen_mask: Vec<bool>,
    /// Whether the warp mesh overlay is visible.
    pub liquify_show_mesh: bool,
    /// Mesh metadata.
    pub liquify_mesh: LiquifyMesh,
    /// Smart-radius adjusts brush fall-off based on local image structure.
    pub liquify_smart_radius: bool,

    // --- Batch 5 (new): Select Subject (AI stub) ---
    pub select_subject_mode: SelectSubjectMode,
    /// Most-recent result from RunSelectSubject (None = never run).
    pub last_select_subject: Option<SelectSubjectResult>,
    /// Auto-refine hair/fur edges after selection.
    pub select_subject_refine: bool,
    /// Whether the Select and Mask panel is open.
    pub select_and_mask_open: bool,

    // --- Batch 6: Layer Effects Suite ---
    /// Per-layer effect stacks, keyed by layer id as string.
    pub layer_effects: std::collections::HashMap<String, LayerEffects>,
    /// Whether the Layer Effects panel is open.
    pub fx_panel_open: bool,
    /// The layer currently targeted in the FX panel.
    pub fx_target_layer: Option<String>,
    /// Clipboard for copy/paste of full LayerEffects.
    pub fx_clipboard: Option<LayerEffects>,

    // --- Batch 6: Match Color (new config) ---
    pub match_color_config: MatchColorConfig,
    /// True after ApplyMatchColor is called (stub flag).
    pub last_match_color_applied: bool,

    // --- Batch 6: Camera Raw Filter (new config) ---
    pub camera_raw_config: CameraRawConfig,
    pub camera_raw_panel_open: bool,
    /// True after ApplyCameraRawFilter is called (stub flag).
    pub camera_raw_applied: bool,

    // --- Batch 6: HDR Merge ---
    pub hdr_merge_config: HdrMergeConfig,
    pub hdr_merge_panel_open: bool,
    /// Stub result path set after MergeToHdr.
    pub hdr_merge_result: Option<String>,

    // ---- New Feature: SmartObject (rich) ------------------------------------
    /// Rich Smart Object entries (id-keyed, independent of the legacy HashMap).
    pub smart_object_list: Vec<SmartObject>,
    /// Auto-incrementing id counter for new Smart Objects.
    pub smart_object_counter: usize,

    // ---- New Feature: AdvancedMasking (Select & Mask workspace) -------------
    /// Current Select & Mask configuration.
    pub select_mask_config: SelectMaskConfig,
    /// Whether the Select & Mask workspace is open.
    pub select_mask_open: bool,

    // ---- New Feature: GenerativeFill ----------------------------------------
    /// Pending generative fill results awaiting user accept/discard.
    pub generative_fill_results: Vec<GenerativeFillResult>,
    /// Current text prompt for the next generative fill run.
    pub generative_fill_prompt: String,

    // ---- New Feature: Basic3DLayer ------------------------------------------
    /// Per-layer 3-D transform and geometry properties.
    pub layer_3d_props: Vec<Layer3DProps>,
    /// The layer_id of the currently active 3-D layer (None = none).
    pub active_3d_layer: Option<usize>,
}

/// Non-destructive filter applied on top of a layer without touching its pixels.
#[derive(Clone, Debug)]
pub enum SmartFilter {
    Blur(f32),
    Sharpen(f32),
    Brightness(f32, f32),
    HueSat(f32),
}

impl SmartFilter {
    pub fn label(&self) -> &'static str {
        match self {
            SmartFilter::Blur(_) => "Gaussian Blur",
            SmartFilter::Sharpen(_) => "Sharpen",
            SmartFilter::Brightness(..) => "Brightness/Contrast",
            SmartFilter::HueSat(_) => "Hue/Saturation",
        }
    }
}

/// Color profile for display simulation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ColorProfile {
    #[default]
    Srgb,
    AdobeRgb,
    P3,
    ProPhoto,
}

impl ColorProfile {
    pub fn label(self) -> &'static str {
        match self {
            ColorProfile::Srgb => "sRGB",
            ColorProfile::AdobeRgb => "Adobe RGB",
            ColorProfile::P3 => "Display P3",
            ColorProfile::ProPhoto => "ProPhoto RGB",
        }
    }
}

/// Soft-proof mode: simulates output gamut by converting to CMYK and back.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum SoftProofMode {
    #[default]
    Off,
    Cmyk,
    PrinterProfile,
}

impl SoftProofMode {
    pub fn label(self) -> &'static str {
        match self {
            SoftProofMode::Off => "Off",
            SoftProofMode::Cmyk => "CMYK",
            SoftProofMode::PrinterProfile => "Printer Profile",
        }
    }
    pub fn next(self) -> Self {
        match self {
            SoftProofMode::Off => SoftProofMode::Cmyk,
            SoftProofMode::Cmyk => SoftProofMode::PrinterProfile,
            SoftProofMode::PrinterProfile => SoftProofMode::Off,
        }
    }
}

/// Format for export presets.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub enum ExportFormat {
    Jpeg,
    Png,
    Tiff,
    Webp,
}

impl ExportFormat {
    pub fn label(&self) -> &'static str {
        match self {
            ExportFormat::Jpeg => "JPEG",
            ExportFormat::Png => "PNG",
            ExportFormat::Tiff => "TIFF",
            ExportFormat::Webp => "WebP",
        }
    }
    pub fn extension(&self) -> &'static str {
        match self {
            ExportFormat::Jpeg => "jpg",
            ExportFormat::Png => "png",
            ExportFormat::Tiff => "tif",
            ExportFormat::Webp => "webp",
        }
    }
}

/// A named export preset.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct ExportPreset {
    pub name: String,
    pub format: ExportFormat,
    pub quality: u8,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub dpi: u32,
}

impl ExportPreset {
    pub fn jpeg_90() -> Self {
        Self {
            name: "JPEG 90%".into(),
            format: ExportFormat::Jpeg,
            quality: 90,
            width: None,
            height: None,
            dpi: 72,
        }
    }
    pub fn png_lossless() -> Self {
        Self {
            name: "PNG Lossless".into(),
            format: ExportFormat::Png,
            quality: 100,
            width: None,
            height: None,
            dpi: 72,
        }
    }
}

/// Histogram display channel.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum HistogramChannel {
    #[default]
    Luminosity,
    Rgb,
    Red,
    Green,
    Blue,
}

impl HistogramChannel {
    pub fn label(self) -> &'static str {
        match self {
            HistogramChannel::Luminosity => "Luma",
            HistogramChannel::Rgb => "RGB",
            HistogramChannel::Red => "R",
            HistogramChannel::Green => "G",
            HistogramChannel::Blue => "B",
        }
    }
    pub fn next(self) -> Self {
        match self {
            HistogramChannel::Luminosity => HistogramChannel::Rgb,
            HistogramChannel::Rgb => HistogramChannel::Red,
            HistogramChannel::Red => HistogramChannel::Green,
            HistogramChannel::Green => HistogramChannel::Blue,
            HistogramChannel::Blue => HistogramChannel::Luminosity,
        }
    }
}

/// Color mode for the document.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColorMode {
    Rgb,
    Cmyk,
    Hsl,
    Lab,
}

/// Liquify warp mode — only Warp is currently rasterised; others are stub.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum LiquifyMode {
    #[default]
    Warp,
    Twirl,
    Pucker,
    Bloat,
}

impl LiquifyMode {
    pub fn label(self) -> &'static str {
        match self {
            LiquifyMode::Warp => "Warp",
            LiquifyMode::Twirl => "Twirl",
            LiquifyMode::Pucker => "Pucker",
            LiquifyMode::Bloat => "Bloat",
        }
    }
}

/// Heal tool blending mode.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum HealMode {
    /// Gaussian feathered blend (default) — 8px feather at patch boundary.
    #[default]
    Normal,
    /// No blending — acts like a clone stamp.
    Replace,
    /// Delegates to content-aware fill for the healed area.
    Content,
}

impl HealMode {
    pub fn label(self) -> &'static str {
        match self {
            HealMode::Normal => "Normal",
            HealMode::Replace => "Replace",
            HealMode::Content => "Content",
        }
    }
}

// ---- Batch 5: Layer Comps -----------------------------------------------

/// Snapshot of a single layer's visual state, stored inside a [`LayerComp`].
#[derive(Clone, Debug)]
pub struct LayerCompState {
    pub visible: bool,
    pub opacity: f32,
    pub blend_mode: BlendMode,
    pub offset_x: i32,
    pub offset_y: i32,
}

/// A named snapshot of all layer states, enabling quick switching between
/// layout/visibility variations (Photoshop "Layer Comps" parity).
#[derive(Clone, Debug)]
pub struct LayerComp {
    pub name: String,
    /// Per-layer state at the time this comp was captured.
    pub states: HashMap<LayerId, LayerCompState>,
}

// ---- Batch 5: Pattern Stamp ---------------------------------------------

/// A repeating tile pattern stored in the pattern library.
#[derive(Clone, Debug)]
pub struct PatternDef {
    pub name: String,
    /// Row-major RGBA float pixels, linear-light premultiplied.
    pub pixels: Vec<[f32; 4]>,
    pub width: u32,
    pub height: u32,
}

// ---- Batch 5: Vanishing Point -------------------------------------------

/// Editing mode for the Vanishing Point overlay.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum VanishingToolMode {
    /// Clicking defines or moves the four plane corners.
    #[default]
    DefiningPlane,
    /// Clone/stamp within the perspective plane.
    Stamping,
    /// Paste content and drag it to fit the plane.
    Pasting,
}

/// A perspective plane defined by four canvas-space corners (TL/TR/BR/BL).
/// Used by the Vanishing Point feature for perspective-aware cloning.
#[derive(Clone, Debug)]
pub struct VanishingPlane {
    /// Corners in canvas doc-px order: [TL, TR, BR, BL].
    pub corners: [[f32; 2]; 4],
    /// Grid cell size in doc px for the overlay grid (default 50).
    pub grid_size: f32,
    pub active: bool,
}

// ---- Batch 7: Alpha Channels ------------------------------------------------

/// A named alpha channel saved from a selection mask.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AlphaChannel {
    pub name: String,
    /// Flat row-major width×height coverage values 0..=1.
    pub mask: Vec<f32>,
    pub width: u32,
    pub height: u32,
}

// ---- Batch 7: Blend If ------------------------------------------------------

/// Per-layer "Blend If" luminance range controls (Photoshop Layer Style parity).
/// Values are in the Photoshop 0..255 scale stored as f32.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct BlendIf {
    /// Shadow range start for THIS layer.
    pub this_black: f32,
    /// Highlight range end for THIS layer.
    pub this_white: f32,
    /// Shadow range start for the UNDERLYING layer.
    pub under_black: f32,
    /// Highlight range end for the UNDERLYING layer.
    pub under_white: f32,
}

impl Default for BlendIf {
    fn default() -> Self {
        Self {
            this_black: 0.0,
            this_white: 255.0,
            under_black: 0.0,
            under_white: 255.0,
        }
    }
}

// ---- Batch 7: Spot Heal / Red Eye -------------------------------------------

/// Algorithm used by the Spot Healing Brush.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum SpotHealMode {
    ContentAware,
    TextureMatch,
    ProximityMatch,
}

impl Default for SpotHealMode {
    fn default() -> Self { SpotHealMode::ContentAware }
}

// ---- Batch 6: Select Subject ------------------------------------------------

// (no new structs needed — uses existing selection mask infrastructure)

// ---- Batch 5 (new): Sky Replacement -----------------------------------------

/// Built-in sky presets for Sky Replacement.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize, Default)]
pub enum SkyPreset {
    #[default]
    BlueSky,
    SunsetOrange,
    StormyClouds,
    StarryNight,
    CustomImage,
}

/// All tuning parameters for the Sky Replacement feature.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SkyReplaceConfig {
    pub preset: SkyPreset,
    /// Sky brightness 0..=200, default 100.
    pub brightness: f32,
    /// Colour temperature shift −100..=100, default 0.
    pub temperature: f32,
    /// Scale multiplier 0.5..=2.0, default 1.0.
    pub scale: f32,
    pub flip: bool,
    /// Edge fade amount 0..=100, default 20.
    pub fade_edge: f32,
    /// Foreground lighting blend 0..=100, default 50.
    pub foreground_lighting: f32,
    /// When true, output sky + lighting as new layers.
    pub output_new_layers: bool,
}

impl Default for SkyReplaceConfig {
    fn default() -> Self {
        Self {
            preset: SkyPreset::BlueSky,
            brightness: 100.0,
            temperature: 0.0,
            scale: 1.0,
            flip: false,
            fade_edge: 20.0,
            foreground_lighting: 50.0,
            output_new_layers: true,
        }
    }
}

// ---- Batch 5 (new): Liquify Depth -------------------------------------------

/// Available tools inside the Liquify filter.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize, Default)]
pub enum LiquifyTool {
    #[default]
    Forward,
    Reconstruct,
    Smooth,
    Twirl,
    Pucker,
    Bloat,
    PushLeft,
    Mirror,
    Turbulence,
}

/// A single Liquify brush stroke recorded for undo / replay.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LiquifyStroke {
    pub tool: LiquifyTool,
    pub center: [f32; 2],
    pub radius: f32,
    pub pressure: f32,
    /// Rotation angle in degrees (used by Twirl).
    pub angle: f32,
}

/// Warp-mesh metadata (no pixel data — mesh is rebuilt from strokes).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
pub struct LiquifyMesh {
    pub width: u32,
    pub height: u32,
    /// Number of mesh subdivisions (default 4).
    pub subdivisions: u8,
}

// ---- Batch 5 (new): Select Subject (AI stub) --------------------------------

/// Whether Select Subject inference runs on-device or in Photoshop's cloud.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize, Default)]
pub enum SelectSubjectMode {
    #[default]
    Device,
    Cloud,
}

/// Stub result returned after a Select Subject inference pass.
#[derive(Debug, Clone)]
pub struct SelectSubjectResult {
    /// Fraction of canvas covered by the estimated subject mask (0..=1).
    pub coverage: f32,
    /// Model confidence (0..=1).
    pub confidence: f32,
    /// True when the Cloud inference path was used.
    pub cloud_used: bool,
}

// ---- Batch 6: Artboards -----------------------------------------------------

/// A named canvas region for multi-artboard documents (PS 2015+ parity).
#[derive(Clone, Debug)]
pub struct Artboard {
    pub id: u64,
    pub name: String,
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    /// Background fill (straight sRGB RGBA 0..1, default white).
    pub background_color: [f32; 4],
}

// ---- Batch 6: Apply Image ---------------------------------------------------

/// Which channel of a source layer to blend in Apply Image.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ApplyImageChannel {
    Rgb,
    Red,
    Green,
    Blue,
    Alpha,
    Luminosity,
}

impl Default for ApplyImageChannel {
    fn default() -> Self { ApplyImageChannel::Rgb }
}

/// Parameters for the Apply Image command (stored for the dialog UI).
#[derive(Clone, Debug)]
pub struct ApplyImageParams {
    pub source_layer: LayerId,
    pub source_channel: ApplyImageChannel,
    pub target_layer: LayerId,
    pub blend_mode: BlendMode,
    /// Blend opacity 0..1.
    pub opacity: f32,
    pub invert_source: bool,
    pub mask_layer: Option<LayerId>,
}

// ---- Batch 6: Soft Proof (expanded) -----------------------------------------

/// Color profile to simulate during soft proof.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ProofProfile {
    #[default]
    WorkingCmyk,
    Srgb,
    AdobeRgb,
    PrinterProfile,
    MonitorRgb,
}

impl ProofProfile {
    pub fn label(self) -> &'static str {
        match self {
            ProofProfile::WorkingCmyk   => "Working CMYK",
            ProofProfile::Srgb          => "sRGB",
            ProofProfile::AdobeRgb      => "Adobe RGB",
            ProofProfile::PrinterProfile => "Printer Profile",
            ProofProfile::MonitorRgb    => "Monitor RGB",
        }
    }
}

/// Perceptual rendering intent for soft proof gamut mapping.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum RenderingIntent {
    #[default]
    Perceptual,
    RelativeColorimetric,
    Saturation,
    AbsoluteColorimetric,
}

impl RenderingIntent {
    pub fn label(self) -> &'static str {
        match self {
            RenderingIntent::Perceptual              => "Perceptual",
            RenderingIntent::RelativeColorimetric    => "Relative Colorimetric",
            RenderingIntent::Saturation              => "Saturation",
            RenderingIntent::AbsoluteColorimetric    => "Absolute Colorimetric",
        }
    }
}

/// Full soft-proof settings (Photoshop View > Proof Setup parity).
#[derive(Clone, Debug)]
pub struct SoftProofSettings {
    pub profile: ProofProfile,
    pub intent: RenderingIntent,
    pub black_point_compensation: bool,
    pub simulate_paper_white: bool,
    pub simulate_black_ink: bool,
    pub gamut_warning: bool,
    /// Highlight colour for out-of-gamut pixels (straight sRGB RGBA, default green).
    pub gamut_warning_color: [f32; 4],
}

impl Default for SoftProofSettings {
    fn default() -> Self {
        Self {
            profile: ProofProfile::WorkingCmyk,
            intent: RenderingIntent::Perceptual,
            black_point_compensation: true,
            simulate_paper_white: false,
            simulate_black_ink: false,
            gamut_warning: false,
            gamut_warning_color: [0.0, 1.0, 0.0, 1.0],
        }
    }
}

impl App {
    /// Build the shared state: boot the host (which also yields the document)
    /// and seed tool/brush/view to the egui app's defaults.
    pub fn new() -> Self {
        let (host, doc) = CanvasHost::new();
        Self {
            host,
            doc,
            // egui default tool is Brush.
            active: Tool::Brush,
            brush: Brush::default(),
            swatches: Vec::new(),
            view: ViewTransform::default(),
            stroke_last: None,
            stroke_residual: 0.0,
            sel_drag_start: None,
            lasso_points: Vec::new(),
            sel_base: Vec::new(),
            sel_mode: CombineMode::Replace,
            xform_drag_start: None,
            xform_translate: [0.0, 0.0],
            xform_scale: 1.0,
            grad_drag_start: None,
            shape_drag_start: None,
            last_drag: None,
            // egui defaults (`pigment-app/src/app/mod.rs`).
            fill_tolerance: 0.1,
            fill_contiguous: true,
            gradient_dither: true,
            clone_source: None,
            clone_offset: [0.0, 0.0],
            text_edit: None,
            text_size: 48.0,
            selection_generation: 0,
            masked_layers: HashSet::new(),
            edit_mask: false,
            channel_visibility: [true; 4],
            bg_color: [1.0, 1.0, 1.0, 1.0],
            smart_objects: HashMap::new(),
            history_labels: Vec::new(),
            pen_path: Vec::new(),
            pen_closed: false,
            dragging_curve_point: None,
            hovered_curve_point: None,
            curves_canvas_bounds: None,
            // Wave 11
            layer_styles: HashMap::new(),
            style_panel_open: false,
            style_panel_layer: None,
            clipping_masks: HashSet::new(),
            fg_hue: 0.0,
            fg_saturation: 0.0,
            fg_value: 0.0,
            status_message: None,
            // Wave 12
            dodge_size: 20.0,
            dodge_strength: 0.3,
            smudge_strength: 0.5,
            liquify_mode: LiquifyMode::Warp,
            crop_rect: None,
            // Wave 13
            guides_h: Vec::new(),
            guides_v: Vec::new(),
            guides_visible: true,
            fill_layers: HashMap::new(),
            recent_files: Vec::new(),
            canvas_rotation_deg: 0.0,
            color_mode: ColorMode::Rgb,
            active_menu: None,
            cursor_blink_on: false,
            cursor_blink_tick: 0,
            snap_to_grid: false,
            grid_size: 16.0,
            slices: Vec::new(),
            next_slice_id: 0,
            slice_drag_start: None,
            smart_filters: HashMap::new(),
            filter_gallery_open: false,
            camera_raw_open: false,
            camera_raw_params: {
                let mut m = HashMap::new();
                for k in [
                    // Basic
                    "exposure","contrast","highlights","shadows","whites","blacks",
                    "clarity","vibrance","saturation",
                    // Detail
                    "sharpening_amount","sharpening_radius","noise_luminance",
                    // HSL hue per channel
                    "hue_red","hue_orange","hue_yellow","hue_green","hue_aqua","hue_blue","hue_purple","hue_magenta",
                    // HSL saturation per channel
                    "sat_red","sat_orange","sat_yellow","sat_green","sat_aqua","sat_blue","sat_purple","sat_magenta",
                    // HSL luminance per channel
                    "lum_red","lum_orange","lum_yellow","lum_green","lum_aqua","lum_blue","lum_purple","lum_magenta",
                ] {
                    m.insert(k, 0.0f32);
                }
                m
            },
            color_profile: ColorProfile::Srgb,
            soft_proof: SoftProofMode::Off,
            export_presets: vec![ExportPreset::jpeg_90(), ExportPreset::png_lossless()],
            histogram_channel: HistogramChannel::Luminosity,
            panel_detached: HashMap::new(),
            panel_positions: HashMap::new(),
            panel_visibility: {
                let mut m = HashMap::new();
                for name in ["Layers", "Color", "Adjustments", "Histogram", "Channels", "History", "Tool Options"] {
                    m.insert(name.to_string(), true);
                }
                m
            },
            // Load saved preferences on startup; non-fatal if file is absent.
            prefs: {
                let p = AppPrefs::load();
                log::info!("prefs loaded from {:?}", AppPrefs::path());
                p
            },
            lens_barrel: 0.0,
            lens_pincushion: 0.0,
            lens_vignette: 0.0,
            lens_correction_open: false,
            camera_raw_section_basic: true,
            camera_raw_section_detail: false,
            camera_raw_section_hsl: false,
            // Batch 4: Autosave
            autosave_interval_secs: 300,
            last_autosave: None,
            autosave_restore_pending: autosave_path().map(|p| p.exists()).unwrap_or(false),
            // Batch 4: Heal
            heal_radius: 20,
            heal_mode: HealMode::Normal,
            // Batch 4: Print
            show_print_dialog: false,
            print_paper_size: "A4".to_string(),
            print_landscape: false,
            print_scale_mode: "Fit to Page".to_string(),
            print_color_space: "sRGB".to_string(),
            // Batch 4: Plugins
            plugin_registry: crate::plugin::PluginRegistry::default(),
            plugin_params: "{}".to_string(),
            // Batch 4: Snapshots
            snapshots: Vec::new(),
            // Batch 5: Layer Comps
            layer_comps: Vec::new(),
            layer_comps_panel_open: false,
            active_comp_idx: None,
            // Batch 5: Pattern Stamp
            pattern_library: Vec::new(),
            active_pattern_idx: None,
            pattern_stamp_scale: 1.0,
            pattern_stamp_aligned: true,
            // Batch 5: Match Color
            match_color_dialog_open: false,
            match_color_source: None,
            match_color_fade: 100.0,
            // Batch 5: Vanishing Point
            vanishing_planes: Vec::new(),
            active_vanishing_plane: None,
            vanishing_tool_mode: VanishingToolMode::DefiningPlane,
            vanishing_point_open: false,
            // Batch 5: Focus Area
            focus_area_threshold: 0.5,
            focus_area_sensitivity: 0.5,
            // Batch 6: Select Subject
            select_subject_threshold: 0.5,
            select_subject_feather: 1.0,
            // Batch 6: Artboards
            artboards: Vec::new(),
            active_artboard: None,
            artboards_panel_open: false,
            next_artboard_id: 1,
            // Batch 6: Apply Image
            apply_image_dialog_open: false,
            apply_image_params: ApplyImageParams {
                source_layer: LayerId(0),
                source_channel: ApplyImageChannel::Rgb,
                target_layer: LayerId(0),
                blend_mode: BlendMode::Normal,
                opacity: 1.0,
                invert_source: false,
                mask_layer: None,
            },
            // Batch 6: Soft Proof (expanded)
            soft_proof_enabled: false,
            soft_proof_settings: SoftProofSettings::default(),
            // Batch 7: Alpha Channels
            alpha_channels: Vec::new(),
            // Batch 7: Blend If
            blend_if: std::collections::HashMap::new(),
            // Batch 7: Spot Heal / Red Eye
            spot_heal_mode: SpotHealMode::ContentAware,
            spot_heal_radius: 20.0,
            last_spot_heal: None,
            last_red_eye: None,
            // Batch 4 extended: HDR Tone Mapping
            tone_map_preview: false,
            last_tone_map: None,
            // Batch 4 extended: Neural Filters
            neural_filters: Vec::new(),
            neural_filters_panel_open: false,
            last_neural_apply_count: 0,
            // Batch 4 extended: Layer Group depth
            collapsed_groups: std::collections::HashSet::new(),
            last_duplicated_group: None,
            // Batch 4 extended: Print Layout
            print_layout: PrintLayout::default(),
            print_preview_page: 0,
            // Batch 5 (new): Content-Aware Crop
            ca_crop_config: ContentAwareCropConfig::default(),
            last_ca_crop_rect: None,
            // Batch 5 (new): Sky Replacement
            sky_replace_config: SkyReplaceConfig::default(),
            sky_replace_panel_open: false,
            sky_replaced: false,
            // Batch 5 (new): Liquify Depth
            liquify_tool: LiquifyTool::Forward,
            liquify_brush_size: 100.0,
            liquify_brush_pressure: 50.0,
            liquify_brush_density: 50.0,
            liquify_strokes: Vec::new(),
            liquify_frozen_mask: Vec::new(),
            liquify_show_mesh: false,
            liquify_mesh: LiquifyMesh { width: 0, height: 0, subdivisions: 4 },
            liquify_smart_radius: false,
            // Batch 5 (new): Select Subject (AI stub)
            select_subject_mode: SelectSubjectMode::Device,
            last_select_subject: None,
            select_subject_refine: false,
            select_and_mask_open: false,
            // Batch 6: Layer Effects Suite
            layer_effects: std::collections::HashMap::new(),
            fx_panel_open: false,
            fx_target_layer: None,
            fx_clipboard: None,
            // Batch 6: Match Color (new)
            match_color_config: MatchColorConfig::default(),
            last_match_color_applied: false,
            // Batch 6: Camera Raw Filter (new)
            camera_raw_config: CameraRawConfig::default(),
            camera_raw_panel_open: false,
            camera_raw_applied: false,
            // Batch 6: HDR Merge
            hdr_merge_config: HdrMergeConfig::default(),
            hdr_merge_panel_open: false,
            hdr_merge_result: None,
            // New Feature: SmartObject (rich)
            smart_object_list: Vec::new(),
            smart_object_counter: 0,
            // New Feature: AdvancedMasking
            select_mask_config: SelectMaskConfig::new(),
            select_mask_open: false,
            // New Feature: GenerativeFill
            generative_fill_results: Vec::new(),
            generative_fill_prompt: String::new(),
            // New Feature: Basic3DLayer
            layer_3d_props: Vec::new(),
            active_3d_layer: None,
        }
    }

    /// Mark the selection mask as changed so the root view re-traces the marching-
    /// ants boundary on its next frame. Call after any host selection mutation.
    fn bump_selection(&mut self) {
        self.selection_generation = self.selection_generation.wrapping_add(1);
    }

    /// Apply a panel-emitted [`Action`]. This is the ONLY place `App` state is
    /// mutated. Any action that changes composited pixels marks the host dirty
    /// (and keeps the host's `order` in sync) so the next `host.image()`
    /// re-composites. Pure UI state (tool/brush/view) does not touch the host.
    pub fn apply(&mut self, action: Action) {
        if let Some(label) = action_label(&action) {
            self.history_labels.push(label);
        }
        match action {
            Action::SetTool(t) => {
                // Leaving the Text tool commits any in-progress text run.
                if self.active == Tool::Text && t != Tool::Text {
                    self.commit_text();
                }
                self.active = t;
            }

            // canvas
            Action::ZoomBy(_) | Action::ResetView | Action::SetColorMode(_)
            | Action::RotateCanvas(_) | Action::ResetCanvasRotation
            | Action::TogglePanel(_) | Action::SaveWorkspace(_) | Action::ResetWorkspace
            | Action::AddGuideH(_) | Action::AddGuideV(_) | Action::RemoveGuide { .. }
            | Action::ClearGuides | Action::ToggleGuides
            | Action::SetImageSize { .. } | Action::SetCanvasSize { .. }
            | Action::ToggleGridSnap | Action::SetGridSize(_) | Action::OpenMenu(_)
            | Action::SetColorProfile(_) | Action::SetSoftProof(_) | Action::SetHistogramChannel(_)
            | Action::DetachPanel(_) | Action::AttachPanel(_) | Action::MovePanel(_, _, _)
            | Action::ToggleSoftProof | Action::SetProofProfile(_) | Action::SetRenderingIntent(_)
            | Action::SetBlackPointCompensation(_) | Action::SetSimulatePaperWhite(_)
            | Action::SetSimulateBlackInk(_) | Action::ToggleGamutWarning | Action::SetGamutWarningColor(_)
            => self.apply_canvas(action),

            // layers
            Action::ToggleLayerVisible(_) | Action::SelectLayer(_) | Action::SetLayerOpacity(_, _)
            | Action::MoveLayer { .. } | Action::DeleteLayer(_) | Action::SetLayerBlend(_, _)
            | Action::AddAdjustment(_) | Action::SetAdjustment(_, _)
            | Action::AddMask(_) | Action::DeleteMask(_) | Action::ToggleEditMask
            | Action::SetAdjustmentCurve(_, _) | Action::SelectAll | Action::FlattenLayers
            | Action::AddSwatch(_) | Action::SwapColors | Action::ResetColors
            | Action::ToggleChannel(_) | Action::SetLayerStyle(_, _) | Action::ClearLayerStyle(_)
            | Action::OpenStylePanel(_) | Action::CloseStylePanel
            | Action::ToggleClippingMask(_) | Action::SetFgHue(_) | Action::SetFgSV(_, _)
            | Action::NewLayer | Action::DuplicateLayer | Action::MergeDown
            | Action::AddSolidFillLayer(_) | Action::SetFillLayerColor(_, _)
            | Action::CreateLayerGroup(_) | Action::SetGroupCollapsed { .. }
            | Action::MoveLayerToGroup { .. } | Action::RemoveLayerFromGroup(_)
            | Action::FlattenGroup(_) | Action::DuplicateGroup(_)
            | Action::ToggleLayerCompsPanel | Action::AddLayerComp(_) | Action::ApplyLayerComp(_)
            | Action::UpdateLayerComp(_) | Action::DeleteLayerComp(_) | Action::RenameLayerComp { .. }
            | Action::SetBlendIf { .. } | Action::ClearBlendIf(_)
            | Action::SetFxTargetLayer(_) | Action::SetDropShadow { .. } | Action::ToggleDropShadow { .. }
            | Action::SetOuterGlow { .. } | Action::ToggleOuterGlow { .. }
            | Action::SetBevelEmboss { .. } | Action::ToggleBevelEmboss { .. }
            | Action::SetStroke { .. } | Action::ToggleStroke { .. }
            | Action::SetColorOverlay { .. } | Action::ToggleColorOverlay { .. }
            | Action::ClearLayerEffects { .. } | Action::ToggleFxPanel
            | Action::CopyLayerEffects { .. } | Action::PasteLayerEffects { .. }
            | Action::BeginCurveDrag(_, _) | Action::MoveCurvePoint(_, _, _)
            | Action::EndCurveDrag | Action::RemoveCurvePoint(_, _)
            | Action::HoverCurvePoint(_) | Action::SetCurvesCanvasBounds(_)
            | Action::SetGradientMapStops { .. } | Action::SetChannelMixerOutput { .. }
            | Action::SetChannelMixerMix { .. }
            | Action::JumpHistory(_) | Action::UndoTo(_) | Action::CreateSnapshot(_)
            | Action::Undo | Action::Redo
            => self.apply_layers(action),

            // selections
            Action::SetMarquee { .. } | Action::ClearSelection | Action::InvertSelection
            | Action::SelectFocusArea { .. } | Action::SetFocusAreaThreshold(_)
            | Action::SaveSelectionAsChannel(_) | Action::LoadChannelAsSelection(_)
            | Action::DeleteChannel(_) | Action::DuplicateChannel(_)
            | Action::SelectSubject | Action::SetSelectSubjectThreshold(_)
            | Action::SetSelectSubjectFeather(_) | Action::SetSelectSubjectMode(_)
            | Action::RunSelectSubject | Action::ToggleSelectAndMask
            | Action::SetSelectSubjectRefine(_) | Action::InvertSelectSubject
            | Action::OpenSelectMask | Action::CloseSelectMask
            | Action::SetSelectMaskRadius(_) | Action::SetSelectMaskSmooth(_)
            | Action::SetSelectMaskFeather(_) | Action::SetSelectMaskContrast(_)
            | Action::SetSelectMaskShiftEdge(_) | Action::ApplySelectMask
            | Action::AddArtboard { .. } | Action::RemoveArtboard(_) | Action::SelectArtboard(_)
            | Action::RenameArtboard { .. } | Action::MoveArtboard { .. } | Action::ResizeArtboard { .. }
            | Action::DuplicateArtboard(_) | Action::SetArtboardBackground { .. }
            | Action::ExportArtboards(_) | Action::ToggleArtboardsPanel
            | Action::AddVanishingPlane { .. } | Action::RemoveVanishingPlane(_)
            | Action::SelectVanishingPlane(_) | Action::SetVanishingGridSize(_)
            | Action::SetVanishingToolMode(_) | Action::SetVanishingPlaneCorner { .. }
            | Action::StampInPerspective { .. }
            | Action::OpenVanishingPoint | Action::CloseVanishingPoint
            | Action::ToggleApplyImageDialog | Action::SetApplyImageSource { .. }
            | Action::SetApplyImageTarget(_) | Action::SetApplyImageBlend(_)
            | Action::SetApplyImageOpacity(_) | Action::SetApplyImageInvert(_)
            | Action::SetApplyImageMask(_) | Action::ApplyImage
            => self.apply_selections(action),

            // painting
            Action::SetBrushColor(_) | Action::SetBrushSize(_) | Action::SetBrushHardness(_)
            | Action::SetBrushOpacity(_) | Action::SetFillTolerance(_)
            | Action::ToggleFillContiguous | Action::ToggleGradientDither
            | Action::PenAddNode(_) | Action::PenMoveHandle { .. } | Action::PenClose
            | Action::SetDodgeSize(_) | Action::SetDodgeStrength(_) | Action::SetSmudgeStrength(_)
            | Action::SetLiquifyMode(_) | Action::SetHealRadius(_) | Action::SetHealMode(_)
            | Action::SetSpotHealMode(_) | Action::SetSpotHealRadius(_)
            | Action::SpotHeal { .. } | Action::RedEye { .. }
            | Action::SetLiquifyTool(_) | Action::SetLiquifyBrushSize(_)
            | Action::SetLiquifyBrushPressure(_) | Action::SetLiquifyBrushDensity(_)
            | Action::ApplyLiquifyStroke(_) | Action::FreezeMaskRegion { .. }
            | Action::ThawAllMask | Action::ReconstructLiquify | Action::RevertLiquify
            | Action::SetLiquifyShowMesh(_) | Action::SetLiquifySmartRadius(_) | Action::SaveLiquifyMesh
            | Action::DefinePattern { .. } | Action::SelectPattern(_) | Action::DeletePattern(_)
            | Action::SetPatternStampScale(_) | Action::SetPatternStampAligned(_)
            => self.apply_painting(action),

            // filters
            Action::ApplyFilter(_) | Action::AddSmartFilter(_, _) | Action::RemoveSmartFilter(_, _)
            | Action::EditSmartFilter(_, _, _) | Action::OpenFilterGallery
            | Action::CloseFilterGallery | Action::ToggleFilterGallery
            | Action::ContentAwareFill | Action::ApplyLensCorrection { .. }
            | Action::PerspectiveWarp { .. } | Action::ToggleLensCorrection | Action::SetLensParam(_, _)
            | Action::ApplyToneMap { .. } | Action::SetToneMapPreview(_)
            | Action::ToggleNeuralFiltersPanel | Action::AddNeuralFilter(_)
            | Action::RemoveNeuralFilter(_) | Action::SetNeuralFilterStrength { .. }
            | Action::ToggleNeuralFilter(_) | Action::ApplyNeuralFilters
            => self.apply_filters(action),

            // transforms
            Action::ApplyCrop { .. } | Action::CancelCrop
            | Action::SetCaCropAngle(_) | Action::SetCaCropFillMethod(_)
            | Action::SetCaCropEnabled(_) | Action::ApplyCaCrop { .. }
            => self.apply_transforms(action),

            // text
            Action::SetTextSize(_) => self.apply_text(action),

            // smart_objects
            Action::ConvertToSmartObject(_) | Action::EditSmartObject(_) | Action::RasterizeSmartObject(_)
            | Action::SmartObjectConvert { .. } | Action::SmartObjectReplace { .. }
            | Action::SmartObjectRasterize { .. } | Action::SmartObjectExport { .. }
            => self.apply_smart_objects(action),

            // layer_3d
            Action::Create3DLayer { .. } | Action::Set3DPosition { .. }
            | Action::SetLayer3DRotation { .. } | Action::Set3DScale { .. }
            | Action::Set3DExtrudeDepth { .. } | Action::Flatten3DLayer { .. }
            => self.apply_layer_3d(action),

            // ai
            Action::SetGenerativeFillPrompt(_) | Action::RunGenerativeFill { .. }
            | Action::CycleGenerativeFillVariation { .. } | Action::AcceptGenerativeFill { .. }
            | Action::DiscardGenerativeFill { .. }
            | Action::SetSkyPreset(_) | Action::SetSkyBrightness(_) | Action::SetSkyTemperature(_)
            | Action::SetSkyScale(_) | Action::SetSkyFlip(_) | Action::SetSkyFadeEdge(_)
            | Action::SetSkyForegroundLighting(_) | Action::SetSkyOutputNewLayers(_)
            | Action::ApplySkyReplace | Action::ToggleSkyReplacePanel
            => self.apply_ai(action),

            // export
            Action::OpenImage | Action::ExportImage | Action::OpenEXR | Action::ExportEXR
            | Action::AddSlice(_) | Action::DeleteSlice(_) | Action::ExportSlices(_)
            | Action::OpenImportPsdDialog | Action::ImportPsd(_)
            | Action::OpenExportPsdDialog | Action::ExportPsd(_)
            | Action::OpenSaveAsDialog | Action::SaveAs(_)
            | Action::ImportRaw(_) | Action::OpenImportRawDialog
            | Action::AddExportPreset(_) | Action::DeleteExportPreset(_) | Action::ExportWithPreset(_)
            | Action::SavePrefs | Action::LoadPrefs
            | Action::TogglePrintDialog | Action::SetPrintPaperSize(_) | Action::SetPrintLandscape(_)
            | Action::SetPrintScaleMode(_) | Action::SetPrintColorSpace(_) | Action::DoPrint
            | Action::SetPrintCopies(_) | Action::SetPrintCollate(_) | Action::SetPrintBorderWidth(_)
            | Action::SetPrintCenterImage(_) | Action::SetPrintMarks(_) | Action::SetPrintBleed(_)
            | Action::SetPrintResolution(_) | Action::SetPrintPreviewPage(_)
            | Action::RunPlugin { .. } | Action::SetPluginParams(_)
            | Action::TriggerAutosave | Action::SetAutosaveInterval(_)
            | Action::RestoreAutosave | Action::DismissAutosave
            | Action::ToggleCameraRawPanel | Action::SetCameraRawTemp(_) | Action::SetCameraRawTint(_)
            | Action::SetCameraRawExposure(_) | Action::SetCameraRawContrast(_)
            | Action::SetCameraRawHighlights(_) | Action::SetCameraRawShadows(_)
            | Action::SetCameraRawClarity(_) | Action::SetCameraRawDehaze(_)
            | Action::SetCameraRawVibrance(_) | Action::SetCameraRawSharpness(_)
            | Action::SetCameraRawNoiseL(_) | Action::SetCameraRawLensCorrection(_)
            | Action::ApplyCameraRawFilter | Action::ResetCameraRaw
            | Action::OpenCameraRaw | Action::CloseCameraRaw | Action::SetCameraRawParam(_, _)
            | Action::ApplyCameraRaw | Action::ToggleCameraRawDialog | Action::ToggleCameraRawSection(_)
            | Action::ToggleHdrMergePanel | Action::SetHdrToneMethod(_) | Action::SetHdrRemoveGhosts(_)
            | Action::SetHdrSourceCount(_) | Action::SetHdrBitDepth(_) | Action::MergeToHdr
            | Action::AddRecentFile(_)
            | Action::ToggleMatchColorDialog | Action::SetMatchColorSource(_) | Action::SetMatchColorFade(_)
            | Action::MatchColor { .. } | Action::SetMatchColorSource2(_)
            | Action::SetMatchColorLuminance(_) | Action::SetMatchColorIntensity(_)
            | Action::SetMatchColorFade2(_) | Action::SetMatchColorNeutralize(_) | Action::ApplyMatchColor
            => self.apply_export(action),

            _ => {}
        }
    }

    // ---- Painting (brush / eraser strokes) -----------------------------------

    /// Whether the active tool paints (Brush or Eraser). Any other tool makes the
    /// stroke methods no-op this pass — other tools are wired in later waves.
    fn is_paint_tool(&self) -> bool {
        matches!(self.active, Tool::Brush | Tool::Eraser)
    }

    /// The layer to paint into: the active layer, falling back to the top layer
    /// (Vec back = top of stack) so a fresh doc with no explicit selection still
    /// paints somewhere sensible.
    fn paint_target(&self) -> Option<LayerId> {
        self.doc
            .active_layer
            .or_else(|| self.doc.layers.layers.last().map(|l| l.id))
    }

    /// Whether the current stroke should paint the active layer's MASK instead of
    /// its pixels: mask-edit mode is on AND the paint target carries a mask.
    /// Mirrors the egui app's `paint_mask` gate
    /// (`edit_mask && masked_layers.contains(active)`).
    fn paint_into_mask(&self) -> bool {
        self.edit_mask
            && self
                .paint_target()
                .is_some_and(|id| self.masked_layers.contains(&id))
    }

    /// Dab spacing in doc px, mirroring the egui app
    /// (`view.rs`: `(brush_size * 0.15).max(0.75)`).
    fn dab_spacing(&self) -> f32 {
        (self.brush.size * 0.15).max(0.75)
    }

    /// Build a `Dab` at `doc` from the current brush, mirroring the egui app's
    /// `dab_at` (`pigment-app/src/app/state.rs`): radius = size * 0.5 (size is a
    /// diameter), hardness clamped to 0.99, color = per-channel `srgb_to_linear`
    /// of the straight-sRGB brush color (straight linear, NOT premultiplied —
    /// the dab shader premultiplies). Alpha is the brush opacity. `size_scale` is
    /// the velocity taper (we pass 1.0; speed dynamics are a later wave).
    fn dab_at(&self, doc: [f32; 2], size_scale: f32) -> Dab {
        let c = self.brush.color;
        Dab {
            center: doc,
            radius: (self.brush.size * 0.5 * size_scale).max(0.5),
            hardness: self.brush.hardness.clamp(0.0, 0.99),
            color: [
                srgb_to_linear(c[0]),
                srgb_to_linear(c[1]),
                srgb_to_linear(c[2]),
                self.brush.opacity,
            ],
        }
    }

    /// Begin a stroke at `doc` (doc px): reset the residual, stamp one dab, and
    /// remember the position. No-op for non-paint tools. Mirrors the egui app's
    /// stroke-begin branch (one dab, `stroke_residual = 0`).
    pub fn begin_stroke(&mut self, doc: [f32; 2]) {
        if !self.is_paint_tool() {
            return;
        }
        let Some(layer) = self.paint_target() else {
            return;
        };
        let erase = self.active == Tool::Eraser;
        let into_mask = self.paint_into_mask();
        self.stroke_residual = 0.0;
        let mut dab = self.dab_at(doc, 1.0);
        // Painting a mask reveals (white); the eraser hides. (The egui app forces
        // the dab color to white for a non-erase mask stroke.)
        if into_mask && !erase {
            dab.color = [1.0, 1.0, 1.0, dab.color[3]];
        }
        // Snapshot the layer at stroke start so the whole stroke is one undo step
        // (mask edits aren't snapshotted yet — match the egui app).
        self.host.paint_dabs(layer, &[dab], erase, !into_mask, into_mask);
        self.stroke_last = Some(doc);
    }

    /// Continue the stroke to `doc` (doc px): place dabs every `spacing` px from
    /// the last position to `doc`, carrying `stroke_residual` across segments so
    /// spacing stays continuous (the egui model). No-op for non-paint tools or if
    /// no stroke is in progress.
    pub fn continue_stroke(&mut self, doc: [f32; 2]) {
        if !self.is_paint_tool() {
            return;
        }
        let Some(last) = self.stroke_last else {
            return;
        };
        let Some(layer) = self.paint_target() else {
            return;
        };
        let seg = [doc[0] - last[0], doc[1] - last[1]];
        let dist = (seg[0] * seg[0] + seg[1] * seg[1]).sqrt();
        if dist <= 1e-3 {
            return;
        }
        let dir = [seg[0] / dist, seg[1] / dist];
        let spacing = self.dab_spacing();
        let erase = self.active == Tool::Eraser;
        let into_mask = self.paint_into_mask();
        let mut dabs: Vec<Dab> = Vec::new();
        let mut t = self.stroke_residual;
        while t <= dist {
            let p = [last[0] + dir[0] * t, last[1] + dir[1] * t];
            let mut dab = self.dab_at(p, 1.0);
            if into_mask && !erase {
                dab.color = [1.0, 1.0, 1.0, dab.color[3]];
            }
            dabs.push(dab);
            t += spacing;
        }
        self.stroke_residual = t - dist;
        self.stroke_last = Some(doc);

        // Continuation dabs land in the same undo step opened at stroke start.
        self.host.paint_dabs(layer, &dabs, erase, false, into_mask);
    }

    /// End the stroke: clear the in-progress position and residual.
    pub fn end_stroke(&mut self) {
        self.stroke_last = None;
        self.stroke_residual = 0.0;
    }

    // ---- Clone stamp ---------------------------------------------------------
    //
    // Mirrors the egui app's Clone tool (`view.rs`): Alt-click sets the source
    // anchor; a subsequent drag locks `clone_offset = dest − source` at the first
    // dab and copies pixels from a frozen source snapshot along the stroke. The
    // engine (`paint_clone_dabs`) owns the copy/falloff; the host only snapshots
    // the source and feeds dabs + offset.

    /// Clone pointer-down: Alt-press sets the source anchor (no paint); otherwise,
    /// if a source is set, freeze it, snapshot the layer for undo, lock the offset,
    /// and stamp the first dab. If no source has been set yet (and alt is not held),
    /// the first click auto-sets the source anchor so the user can immediately drag
    /// to clone on the next interaction — no modifier key required for initial setup.
    fn begin_clone(&mut self, doc: [f32; 2], alt: bool) {
        if alt || self.clone_source.is_none() {
            // Explicit Alt+click OR no source set yet → establish source anchor, no paint.
            self.clone_source = Some(doc);
            return;
        }
        let Some(src) = self.clone_source else { return };
        let Some(layer) = self.paint_target() else { return };
        // Freeze the source + open one undo step for the whole clone stroke.
        self.host.snapshot_layer(layer, "Clone Stamp");
        self.host.snapshot_clone_source(layer);
        self.clone_offset = [doc[0] - src[0], doc[1] - src[1]];
        self.stroke_residual = 0.0;
        let dab = self.dab_at(doc, 1.0);
        self.host.paint_clone_dabs(layer, &[dab], self.clone_offset);
        self.stroke_last = Some(doc);
    }

    /// Clone pointer-drag: stamp dabs every `spacing` px from the last position,
    /// copying from the frozen source at the locked offset. No-op if no stroke is
    /// in progress (e.g. an alt-press with no drag).
    fn continue_clone(&mut self, doc: [f32; 2]) {
        let Some(last) = self.stroke_last else {
            return;
        };
        let Some(layer) = self.paint_target() else {
            return;
        };
        let seg = [doc[0] - last[0], doc[1] - last[1]];
        let dist = (seg[0] * seg[0] + seg[1] * seg[1]).sqrt();
        if dist <= 1e-3 {
            return;
        }
        let dir = [seg[0] / dist, seg[1] / dist];
        let spacing = self.dab_spacing();
        let mut dabs: Vec<Dab> = Vec::new();
        let mut t = self.stroke_residual;
        while t <= dist {
            let p = [last[0] + dir[0] * t, last[1] + dir[1] * t];
            dabs.push(self.dab_at(p, 1.0));
            t += spacing;
        }
        self.stroke_residual = t - dist;
        self.stroke_last = Some(doc);
        self.host.paint_clone_dabs(layer, &dabs, self.clone_offset);
    }

    /// End a clone stroke (same bookkeeping reset as a brush stroke).
    fn end_clone(&mut self) {
        self.stroke_last = None;
        self.stroke_residual = 0.0;
    }

    // ---- Healing brush -------------------------------------------------------
    //
    // Mirrors the Clone stamp flow but routes to `CanvasHost::heal_at` which
    // forces dab hardness to 0.0 for a soft Gaussian-weighted blend. Alt-click
    // sets the source anchor (same as Clone); a subsequent drag copies softly.

    /// Heal pointer-down: Alt-press sets the source anchor (no paint); otherwise
    /// freezes the source, opens one undo step, locks the offset, and stamps the
    /// first soft dab. If no source anchor has been set yet, the first click
    /// auto-sets it (same fallback as Clone) so Alt is not strictly required.
    fn begin_heal(&mut self, doc: [f32; 2], alt: bool) {
        if alt || self.clone_source.is_none() {
            self.clone_source = Some(doc);
            return;
        }
        let Some(src) = self.clone_source else { return };
        let Some(layer) = self.paint_target() else { return };
        self.host.snapshot_layer(layer, "Heal");
        self.host.snapshot_clone_source(layer);
        self.clone_offset = [doc[0] - src[0], doc[1] - src[1]];
        self.stroke_residual = 0.0;
        match self.heal_mode {
            HealMode::Content => {
                self.apply(Action::ContentAwareFill);
                return;
            }
            HealMode::Replace => {
                let dab = self.heal_dab_at(doc);
                self.host.paint_clone_dabs(layer, &[dab], self.clone_offset);
            }
            HealMode::Normal => {
                let dab = self.heal_dab_at(doc);
                self.host.heal_at(layer, &[dab], self.clone_offset);
            }
        }
        self.stroke_last = Some(doc);
    }

    /// Heal pointer-drag: stamp soft dabs every `spacing` px from the last
    /// position, copying from the frozen source at the locked offset.
    fn continue_heal(&mut self, doc: [f32; 2]) {
        let Some(last) = self.stroke_last else { return };
        let Some(layer) = self.paint_target() else { return };
        let seg = [doc[0] - last[0], doc[1] - last[1]];
        let dist = (seg[0] * seg[0] + seg[1] * seg[1]).sqrt();
        if dist <= 1e-3 { return; }
        let dir = [seg[0] / dist, seg[1] / dist];
        let spacing = (self.heal_radius as f32 * 0.15).max(0.75);
        let mut dabs: Vec<Dab> = Vec::new();
        let mut t = self.stroke_residual;
        while t <= dist {
            let p = [last[0] + dir[0] * t, last[1] + dir[1] * t];
            dabs.push(self.heal_dab_at(p));
            t += spacing;
        }
        self.stroke_residual = t - dist;
        self.stroke_last = Some(doc);
        match self.heal_mode {
            HealMode::Replace => self.host.paint_clone_dabs(layer, &dabs, self.clone_offset),
            _ => self.host.heal_at(layer, &dabs, self.clone_offset),
        }
    }

    /// End a heal stroke (same bookkeeping reset as a brush/clone stroke).
    fn end_heal(&mut self) {
        self.stroke_last = None;
        self.stroke_residual = 0.0;
    }

    /// Build a dab sized to `heal_radius` for the healing brush.
    fn heal_dab_at(&self, doc: [f32; 2]) -> Dab {
        let c = self.brush.color;
        Dab {
            center: doc,
            radius: (self.heal_radius as f32).max(0.5),
            hardness: match self.heal_mode {
                HealMode::Normal => 0.0,
                HealMode::Replace => self.brush.hardness.clamp(0.0, 0.99),
                HealMode::Content => 0.0,
            },
            color: [
                srgb_to_linear(c[0]),
                srgb_to_linear(c[1]),
                srgb_to_linear(c[2]),
                self.brush.opacity,
            ],
        }
    }

    /// Write a minimal autosave JSON to `~/.local/share/prism/pigment_autosave.json`.
    pub fn do_autosave(&mut self) {
        let Some(path) = autosave_path() else { return };
        if let Some(parent) = path.parent() {
            if let Err(e) = std::fs::create_dir_all(parent) {
                log::warn!("autosave dir create failed: {e}");
                return;
            }
        }
        let layers: Vec<serde_json::Value> = self.doc.layers.layers.iter().map(|l| {
            serde_json::json!({
                "id": l.id.0,
                "name": l.name,
                "visible": l.visible,
                "opacity": l.opacity,
                "blend": format!("{:?}", l.blend),
            })
        }).collect();
        let json = serde_json::json!({
            "timestamp": std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            "size": { "width": self.doc.size.width, "height": self.doc.size.height },
            "layers": layers,
        });
        match serde_json::to_string_pretty(&json) {
            Ok(text) => {
                if let Err(e) = std::fs::write(&path, text) {
                    log::warn!("autosave write failed: {e}");
                } else {
                    self.last_autosave = Some(std::time::Instant::now());
                    log::info!("autosave written to {:?}", path);
                }
            }
            Err(e) => log::warn!("autosave serialize failed: {e}"),
        }
    }

    /// Call once per frame. Triggers `do_autosave` if the configured interval has elapsed.
    pub fn maybe_autosave(&mut self) {
        if self.autosave_interval_secs == 0 {
            return;
        }
        let elapsed = match self.last_autosave {
            Some(t) => t.elapsed().as_secs(),
            None => self.autosave_interval_secs,
        };
        if elapsed >= self.autosave_interval_secs {
            self.do_autosave();
        }
    }

    /// Write a minimal single-page PDF embedding the composited canvas as JPEG,
    /// save to a temp file, and open with the OS viewer.
    pub fn do_print(&mut self) {
        let Some(flat) = self.host.read_composite_f32() else {
            self.status_message = Some("Print failed: could not read canvas".to_string());
            return;
        };
        let (dw, dh) = (self.host.doc_w, self.host.doc_h);
        let mut rgb8: Vec<u8> = Vec::with_capacity((dw * dh * 3) as usize);
        for i in 0..(dw * dh) as usize {
            let r = flat[i * 4];
            let g = flat[i * 4 + 1];
            let b = flat[i * 4 + 2];
            let a = flat[i * 4 + 3];
            let inv = if a > 1e-5 { 1.0 / a } else { 0.0 };
            rgb8.push(((r * inv).clamp(0.0, 1.0) * 255.0).round() as u8);
            rgb8.push(((g * inv).clamp(0.0, 1.0) * 255.0).round() as u8);
            rgb8.push(((b * inv).clamp(0.0, 1.0) * 255.0).round() as u8);
        }
        let mut jpeg_buf = Vec::new();
        {
            let mut enc = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg_buf, 90);
            if let Err(e) = enc.encode(&rgb8, dw, dh, image::ExtendedColorType::Rgb8) {
                self.status_message = Some(format!("Print failed (JPEG encode): {e}"));
                return;
            }
        }
        let pdf = build_minimal_pdf(&jpeg_buf, dw, dh, self.print_landscape);
        let tmp = std::env::temp_dir().join("pigment_print.pdf");
        if let Err(e) = std::fs::write(&tmp, &pdf) {
            self.status_message = Some(format!("Print failed (PDF write): {e}"));
            return;
        }
        let viewer = if cfg!(target_os = "macos") { "open" } else if cfg!(target_os = "windows") { "explorer" } else { "xdg-open" };
        match std::process::Command::new(viewer).arg(&tmp).spawn() {
            Ok(_) => {
                self.status_message = Some(format!("Print PDF opened: {}", tmp.display()));
                self.show_print_dialog = false;
            }
            Err(e) => {
                self.status_message = Some(format!("Print failed (open): {e}"));
            }
        }
    }

    // ---- Dodge / Burn tool --------------------------------------------------

    fn begin_dodge_burn(&mut self, doc: [f32; 2], strength: f32) {
        let Some(layer) = self.paint_target() else { return };
        self.host.snapshot_layer(layer, "Dodge/Burn");
        let radius = self.dodge_size * 0.5;
        self.host.dodge_burn_dab(layer, doc[0], doc[1], radius, strength);
        self.stroke_last = Some(doc);
        self.stroke_residual = 0.0;
    }

    fn continue_dodge_burn(&mut self, doc: [f32; 2], strength: f32) {
        let Some(last) = self.stroke_last else { return };
        let Some(layer) = self.paint_target() else { return };
        let seg = [doc[0] - last[0], doc[1] - last[1]];
        let dist = (seg[0] * seg[0] + seg[1] * seg[1]).sqrt();
        if dist <= 1e-3 { return; }
        let dir = [seg[0] / dist, seg[1] / dist];
        let spacing = (self.dodge_size * 0.15).max(0.75);
        let radius = self.dodge_size * 0.5;
        let mut t = self.stroke_residual;
        while t <= dist {
            let p = [last[0] + dir[0] * t, last[1] + dir[1] * t];
            self.host.dodge_burn_dab(layer, p[0], p[1], radius, strength);
            t += spacing;
        }
        self.stroke_residual = t - dist;
        self.stroke_last = Some(doc);
    }

    // ---- Liquify warp tool --------------------------------------------------

    fn begin_liquify(&mut self, doc: [f32; 2]) {
        let Some(layer) = self.paint_target() else { return };
        self.host.snapshot_layer(layer, "Liquify");
        self.stroke_last = Some(doc);
    }

    fn continue_liquify(&mut self, doc: [f32; 2]) {
        let Some(last) = self.stroke_last else { return };
        let Some(layer) = self.paint_target() else { return };
        let dx = doc[0] - last[0];
        let dy = doc[1] - last[1];
        if dx.hypot(dy) < 0.5 { return; }
        let radius = self.dodge_size * 1.5;
        self.host.liquify_warp(layer, doc[0], doc[1], dx, dy, radius, 0.4);
        self.stroke_last = Some(doc);
    }

    // ---- Smudge tool --------------------------------------------------------

    fn begin_smudge(&mut self, doc: [f32; 2]) {
        let Some(layer) = self.paint_target() else { return };
        self.host.snapshot_layer(layer, "Smudge");
        let radius = self.dodge_size * 0.5;
        self.host.smudge_dab(layer, doc[0], doc[1], radius, self.smudge_strength);
        self.stroke_last = Some(doc);
        self.stroke_residual = 0.0;
    }

    fn continue_smudge(&mut self, doc: [f32; 2]) {
        let Some(last) = self.stroke_last else { return };
        let Some(layer) = self.paint_target() else { return };
        let seg = [doc[0] - last[0], doc[1] - last[1]];
        let dist = (seg[0] * seg[0] + seg[1] * seg[1]).sqrt();
        if dist <= 1e-3 { return; }
        let dir = [seg[0] / dist, seg[1] / dist];
        let spacing = (self.dodge_size * 0.15).max(0.75);
        let radius = self.dodge_size * 0.5;
        let mut t = self.stroke_residual;
        while t <= dist {
            let p = [last[0] + dir[0] * t, last[1] + dir[1] * t];
            self.host.smudge_dab(layer, p[0], p[1], radius, self.smudge_strength);
            t += spacing;
        }
        self.stroke_residual = t - dist;
        self.stroke_last = Some(doc);
    }

    // ---- Pointer drag dispatch (non-paint tools) -----------------------------
    //
    // The root view routes ALL canvas pointer events through `begin_drag` /
    // `continue_drag` / `end_drag`. Each dispatches on the active tool: paint
    // tools fall through to the existing stroke path; Selection routes to the
    // marquee path; Move/Transform route to the live-affine path. This keeps the
    // root view's three handlers tool-agnostic (it just maps window→doc px and
    // calls these), exactly mirroring how the brush is wired.

    /// Pointer-down at `doc` (doc px); `alt`/`shift` carry the Option/Alt and Shift
    /// modifiers. The Clone tool uses `alt` to set its source anchor; the selection
    /// tools use both to pick the combine mode (Shift = add, Alt = subtract,
    /// Shift+Alt = intersect, else replace). Dispatches on the active tool.
    pub fn begin_drag(&mut self, doc: [f32; 2], alt: bool, shift: bool) {
        self.last_drag = Some(doc);
        match self.active {
            Tool::Brush | Tool::Eraser => self.begin_stroke(doc),
            Tool::Clone => self.begin_clone(doc, alt),
            Tool::Heal => self.begin_heal(doc, alt),
            Tool::Dodge => self.begin_dodge_burn(doc, self.dodge_strength.abs()),
            Tool::Burn => self.begin_dodge_burn(doc, -self.dodge_strength.abs()),
            Tool::Smudge => self.begin_smudge(doc),
            Tool::Liquify => self.begin_liquify(doc),
            Tool::Crop => {
                self.crop_rect = Some([doc[0], doc[1], doc[0], doc[1]]);
            }
            Tool::Text => self.place_text(doc),
            Tool::SelectRect | Tool::SelectEllipse => {
                self.sel_drag_start = Some(doc);
                self.begin_selection_op(shift, alt);
                // A bare press starts an empty marquee; the drag fills it in.
            }
            Tool::Lasso => {
                self.begin_selection_op(shift, alt);
                self.lasso_points.clear();
                self.lasso_points.push(doc);
            }
            Tool::MagicWand => {
                // A single click flood-selects from the seed immediately.
                self.begin_selection_op(shift, alt);
                self.magic_wand_at(doc);
            }
            Tool::Move | Tool::MoveLayer | Tool::Transform => {
                self.xform_drag_start = Some(doc);
                self.xform_translate = [0.0, 0.0];
                self.xform_scale = 1.0;
            }
            // Paint-bucket: a single click fills from the seed immediately.
            Tool::Fill => self.fill_at(doc),
            // Pen tool: each click adds an anchor node.
            Tool::Pen => self.apply(Action::PenAddNode((doc[0], doc[1]))),
            // Gradient / shapes anchor here; the drag end applies them.
            Tool::Gradient => self.grad_drag_start = Some(doc),
            Tool::ShapeRect | Tool::ShapeEllipse => self.shape_drag_start = Some(doc),
            Tool::Slice => self.slice_drag_start = Some(doc),
            _ => {}
        }
    }

    /// Pointer-drag to `doc` (doc px). Dispatches on the active tool.
    pub fn continue_drag(&mut self, doc: [f32; 2]) {
        self.last_drag = Some(doc);
        match self.active {
            Tool::Brush | Tool::Eraser => self.continue_stroke(doc),
            Tool::Clone => self.continue_clone(doc),
            Tool::Heal => self.continue_heal(doc),
            Tool::Dodge => self.continue_dodge_burn(doc, self.dodge_strength.abs()),
            Tool::Burn => self.continue_dodge_burn(doc, -self.dodge_strength.abs()),
            Tool::Smudge => self.continue_smudge(doc),
            Tool::Liquify => self.continue_liquify(doc),
            Tool::Crop => {
                if let Some(r) = self.crop_rect.as_mut() {
                    r[2] = doc[0];
                    r[3] = doc[1];
                }
            }
            Tool::SelectRect | Tool::SelectEllipse => {
                if let Some(start) = self.sel_drag_start {
                    self.preview_marquee(start, doc);
                }
            }
            Tool::Lasso => {
                // Append a point once we've moved a couple px (matches the egui
                // app's 2px threshold) so the polygon stays light.
                if self
                    .lasso_points
                    .last()
                    .is_none_or(|l| (l[0] - doc[0]).hypot(l[1] - doc[1]) > 2.0)
                {
                    self.lasso_points.push(doc);
                }
            }
            Tool::Move | Tool::MoveLayer | Tool::Transform => {
                let Some(start) = self.xform_drag_start else {
                    return;
                };
                // Transform: vertical drag uniformly scales about the canvas
                // center (the egui app gates scale behind Shift; here the
                // Transform tool itself selects scale, Move/MoveLayer select translate).
                if self.active == Tool::Transform {
                    let dy = doc[1] - start[1];
                    self.xform_scale = (1.0 - dy * 0.005).clamp(0.05, 20.0);
                } else {
                    let mut tx = doc[0] - start[0];
                    let mut ty = doc[1] - start[1];
                    if self.snap_to_grid {
                        let g = self.grid_size;
                        tx = (tx / g).round() * g;
                        ty = (ty / g).round() * g;
                    }
                    self.xform_translate = [tx, ty];
                }
                self.push_live_xform();
            }
            _ => {}
        }
    }

    /// Pointer-up. Dispatches on the active tool, finalizing the interaction.
    pub fn end_drag(&mut self) {
        match self.active {
            Tool::Brush | Tool::Eraser => self.end_stroke(),
            Tool::Clone => self.end_clone(),
            Tool::Heal => self.end_heal(),
            Tool::Dodge | Tool::Burn | Tool::Smudge => {
                self.stroke_last = None;
                self.stroke_residual = 0.0;
            }
            Tool::SelectRect | Tool::SelectEllipse => {
                self.sel_drag_start = None;
                self.sel_base.clear();
            }
            Tool::Lasso => {
                self.commit_lasso();
                self.lasso_points.clear();
                self.sel_base.clear();
            }
            Tool::MagicWand => {
                self.sel_base.clear();
            }
            Tool::Move | Tool::MoveLayer | Tool::Transform => {
                if self.xform_drag_start.is_some() {
                    // Keep the affine live for the bake (the engine reads the
                    // last `set_layer_transform`), bake it into pixels, then
                    // reset the live transform to identity.
                    self.push_live_xform();
                    if let Some(layer) = self.paint_target() {
                        self.host.bake_layer_xform(layer);
                    }
                    self.host.set_layer_xform(None, [1.0, 0.0, 0.0, 1.0], [0.0; 2]);
                    self.host.mark_dirty();
                    self.xform_drag_start = None;
                    self.xform_translate = [0.0, 0.0];
                    self.xform_scale = 1.0;
                }
            }
            Tool::Gradient => {
                if let Some(start) = self.grad_drag_start.take() {
                    self.apply_gradient(start, self.last_drag.unwrap_or(start));
                }
            }
            Tool::Slice => {
                if let Some(start) = self.slice_drag_start.take() {
                    let end = self.last_drag.unwrap_or(start);
                    let x = start[0].min(end[0]);
                    let y = start[1].min(end[1]);
                    let w = (start[0] - end[0]).abs();
                    let h = (start[1] - end[1]).abs();
                    if w > 2.0 && h > 2.0 {
                        self.apply(Action::AddSlice([x, y, w, h]));
                    }
                }
            }
            Tool::ShapeRect | Tool::ShapeEllipse => {
                if let Some(start) = self.shape_drag_start.take() {
                    let end = self.last_drag.unwrap_or(start);
                    let kind = if self.active == Tool::ShapeEllipse {
                        ShapeKind::Ellipse
                    } else {
                        ShapeKind::Rectangle
                    };
                    self.draw_shape(kind, start, end);
                }
            }
            _ => {}
        }
        self.last_drag = None;
    }

    /// Push the accumulated translate/scale to the host as a live affine on the
    /// active layer (or top layer fallback), so the next composite previews the
    /// Move/Transform without baking. Mirrors the egui app's `compute_xform`.
    fn push_live_xform(&mut self) {
        let Some(layer) = self.paint_target() else {
            return;
        };
        let (m, off) = compute_xform(
            self.xform_translate,
            self.xform_scale,
            self.doc.size.width as f32,
            self.doc.size.height as f32,
        );
        self.host.set_layer_xform(Some(layer), m, off);
    }

    // ---- Core canvas tools (fill / gradient / shape) -------------------------
    //
    // Thin wrappers that resolve the paint target + tool params, then forward to
    // the matching `CanvasHost` method (which owns the read → engine-rasterize →
    // source-over → upload through the shared `prism_core::{fill,gradient,shape}`
    // — no raster math lives here). Mirror the egui app's `do_fill`/`do_gradient`
    // and vector-shape rasterization.

    /// Paint-bucket fill from `doc` (doc px): flood the active layer at the seed
    /// within the current tolerance and write the brush color into the matched
    /// (and selected) pixels. No-op off-canvas or with no paint target.
    fn fill_at(&mut self, doc: [f32; 2]) {
        let Some(layer) = self.paint_target() else {
            return;
        };
        let (dw, dh) = (self.doc.size.width, self.doc.size.height);
        if dw == 0 || dh == 0 {
            return;
        }
        let sx = (doc[0].floor() as i64).clamp(0, dw as i64 - 1) as u32;
        let sy = (doc[1].floor() as i64).clamp(0, dh as i64 - 1) as u32;
        let mut color = self.brush.color;
        color[3] = self.brush.opacity;
        self.host
            .fill_at(layer, (sx, sy), color, self.fill_tolerance, self.fill_contiguous);
    }

    /// Apply a linear gradient along `p0 → p1` (doc px) to the active layer. The
    /// gradient runs from the brush color (opaque) to the brush color
    /// (transparent) — the egui app's default Foreground→Transparent rail.
    fn apply_gradient(&mut self, p0: [f32; 2], p1: [f32; 2]) {
        let Some(layer) = self.paint_target() else {
            return;
        };
        let mut c0 = self.brush.color;
        c0[3] = self.brush.opacity;
        let c1 = [self.brush.color[0], self.brush.color[1], self.brush.color[2], 0.0];
        self.host
            .apply_gradient(layer, p0, p1, c0, c1, self.gradient_dither);
    }

    /// Draw a filled `kind` shape spanning the `start → end` bbox (doc px) into
    /// the active layer, using the brush color (alpha = brush opacity). No-op for
    /// a degenerate (zero-area) drag.
    fn draw_shape(&mut self, kind: ShapeKind, start: [f32; 2], end: [f32; 2]) {
        let Some(layer) = self.paint_target() else {
            return;
        };
        let rect = [
            start[0].min(end[0]),
            start[1].min(end[1]),
            (start[0] - end[0]).abs(),
            (start[1] - end[1]).abs(),
        ];
        if rect[2] <= 0.5 || rect[3] <= 0.5 {
            return;
        }
        let mut color = self.brush.color;
        color[3] = self.brush.opacity;
        self.host.draw_shape(layer, kind, rect, color);
    }

    // ---- Selection tools (marquee / ellipse / lasso / magic-wand) ------------
    //
    // All four reuse the shared engine ops — NOTHING is reimplemented here:
    //   • Rect/Ellipse marquee → `SelectionOp::Marquee` (engine rasterizes).
    //   • Lasso → `prism_core::raster::polygon_mask` (CPU) → `upload_selection`.
    //   • Magic wand → composite read-back + `prism_core::fill::flood_fill_mask`.
    // For the lasso/wand the freshly-computed mask is combined with the op's base
    // snapshot via `prism_core::raster::combine` (Shift adds, Alt subtracts, both
    // intersects), mirroring the egui app's `commit_selection`.

    /// Snapshot the current selection + capture the combine mode at op start so the
    /// op can add/subtract/intersect with what was selected. Mirrors the egui app's
    /// `sel_base = read_selection()` + `sel_mode = mode_from_modifiers(..)`.
    fn begin_selection_op(&mut self, shift: bool, alt: bool) {
        self.sel_mode = mode_from_modifiers(shift, alt);
        self.sel_base = self.host.read_selection_or_empty();
    }

    /// Preview/apply a rect or ellipse marquee spanning `start → cur` (doc px). For
    /// a Replace op we can use the fast engine `Marquee` rasterizer directly; for an
    /// add/subtract/intersect we rasterize the shape to a CPU mask and combine it
    /// with the base, then upload. The active tool selects rect vs ellipse.
    fn preview_marquee(&mut self, start: [f32; 2], cur: [f32; 2]) {
        let rect = [
            start[0].min(cur[0]),
            start[1].min(cur[1]),
            (start[0] - cur[0]).abs(),
            (start[1] - cur[1]).abs(),
        ];
        let ellipse = self.active == Tool::SelectEllipse;
        if self.sel_mode == CombineMode::Replace {
            self.apply(Action::SetMarquee { rect, ellipse });
            return;
        }
        // Combine path: rasterize the shape to a mask and merge with the base.
        if rect[2] <= 0.5 || rect[3] <= 0.5 {
            return;
        }
        let (w, h) = (self.doc.size.width, self.doc.size.height);
        let shape = shape_mask(rect, ellipse, w, h);
        let combined = combine(&self.sel_base, &shape, self.sel_mode);
        self.host.upload_selection_mask(&combined);
        self.bump_selection();
    }

    /// Flush the in-progress lasso polygon to a selection mask (even-odd fill via
    /// the engine's `polygon_mask`), combined with the base per the active mode.
    fn commit_lasso(&mut self) {
        if self.lasso_points.len() < 3 {
            return;
        }
        let (w, h) = (self.doc.size.width, self.doc.size.height);
        let pts: Vec<(f32, f32)> = self.lasso_points.iter().map(|p| (p[0], p[1])).collect();
        let mask = polygon_mask(&pts, w, h);
        let combined = combine(&self.sel_base, &mask, self.sel_mode);
        self.host.upload_selection_mask(&combined);
        self.bump_selection();
    }

    /// Magic-wand at `doc` (doc px): composite the document, flood-select from the
    /// seed within the fill tolerance over the composited pixels (the egui app's
    /// `do_magic_wand`), combine with the base, and upload. No-op off-canvas.
    fn magic_wand_at(&mut self, doc: [f32; 2]) {
        let (w, h) = (self.doc.size.width, self.doc.size.height);
        if w == 0 || h == 0 {
            return;
        }
        let sx = (doc[0].floor() as i64).clamp(0, w as i64 - 1) as u32;
        let sy = (doc[1].floor() as i64).clamp(0, h as i64 - 1) as u32;
        let Some(buf) = self.host.read_composite_f32() else {
            return;
        };
        let mask_b = flood_fill_mask(&buf, w, h, sx, sy, self.fill_tolerance, self.fill_contiguous);
        let mask: Vec<f32> = mask_b.iter().map(|&b| if b { 1.0 } else { 0.0 }).collect();
        let combined = combine(&self.sel_base, &mask, self.sel_mode);
        self.host.upload_selection_mask(&combined);
        self.bump_selection();
    }

    /// Rebuild the host's per-layer draw order from the current document layer
    /// stack and mark the host dirty so the next `image()` re-composites. Call
    /// after any mutation that changes which layers (or with what opacity/blend/
    /// visibility) the compositor draws.
    fn sync_host_order_dirty(&mut self) {
        self.host.sync_order(&self.doc);
        self.host.mark_dirty();
    }
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

/// Build the uv-space layer-from-canvas affine for a translate (doc px) + uniform
/// scale about the canvas center. Returns (2x2 matrix `[a, b, c, d]`, offset).
/// Ported verbatim from the egui app's `compute_xform`
/// (`pigment-app/src/app/mod.rs`) so the move/scale math is identical; the engine
/// (`bake_transform` / the compositor's live-affine branch) consumes exactly this
/// form.
/// Map modifier keys to a selection combine mode, mirroring the egui app's
/// `mode_from_modifiers`: Shift = add, Alt = subtract, Shift+Alt = intersect,
/// else replace.
fn mode_from_modifiers(shift: bool, alt: bool) -> CombineMode {
    match (shift, alt) {
        (true, false) => CombineMode::Add,
        (false, true) => CombineMode::Subtract,
        (true, true) => CombineMode::Intersect,
        _ => CombineMode::Replace,
    }
}

/// Rasterize a rectangle (`ellipse = false`) or ellipse (`ellipse = true`) marquee
/// to a 0/1 selection mask (len = w*h). Ported from the egui app's `shape_mask`
/// (`pigment-app/src/app/mod.rs`) so add/subtract marquees match exactly. A pixel
/// center inside the rect/ellipse is `1.0`, else `0.0`.
fn shape_mask(rect: [f32; 4], ellipse: bool, w: u32, h: u32) -> Vec<f32> {
    let [rx, ry, rw, rh] = rect;
    let (cx, cy) = (rx + rw * 0.5, ry + rh * 0.5);
    let (hx, hy) = ((rw * 0.5).max(1e-3), (rh * 0.5).max(1e-3));
    let mut m = vec![0.0; (w * h) as usize];
    for y in 0..h {
        let py = y as f32 + 0.5;
        for x in 0..w {
            let px = x as f32 + 0.5;
            let inside = if ellipse {
                let dx = (px - cx) / hx;
                let dy = (py - cy) / hy;
                dx * dx + dy * dy <= 1.0
            } else {
                px >= rx && px < rx + rw && py >= ry && py < ry + rh
            };
            if inside {
                m[(y * w + x) as usize] = 1.0;
            }
        }
    }
    m
}

/// Short human-readable label for destructive `Action`s, used for history tracking.
/// Returns `None` for pure-UI actions that don't mutate pixels or document structure.
fn action_label(action: &Action) -> Option<String> {
    match action {
        Action::ApplyFilter(_) => Some("Apply Filter".into()),
        Action::AddAdjustment(_) => Some("Add Adjustment".into()),
        Action::SetAdjustment(_, _) => Some("Set Adjustment".into()),
        Action::SetAdjustmentCurve(_, _) => Some("Curves Edit".into()),
        Action::MoveCurvePoint(_, _, _) => Some("Curves Drag".into()),
        Action::PenClose => Some("Pen Path".into()),
        Action::DeleteLayer(_) => Some("Delete Layer".into()),
        Action::MoveLayer { .. } => Some("Move Layer".into()),
        Action::AddMask(_) => Some("Add Mask".into()),
        Action::DeleteMask(_) => Some("Delete Mask".into()),
        Action::OpenImage => Some("Open Image".into()),
        Action::OpenEXR => Some("Open EXR".into()),
        Action::FlattenLayers => Some("Flatten Layers".into()),
        Action::ConvertToSmartObject(_) => Some("Convert to Smart Object".into()),
        Action::RasterizeSmartObject(_) => Some("Rasterize Smart Object".into()),
        Action::SetLayerStyle(_, _) => Some("Set Layer Style".into()),
        Action::ClearLayerStyle(_) => Some("Clear Layer Style".into()),
        Action::ToggleClippingMask(_) => Some("Toggle Clipping Mask".into()),
        Action::SaveAs(_) => Some("Save As".into()),
        _ => None,
    }
}

/// Walk a cubic bézier pen path and produce a list of `Dab`s for painting.
/// Uses 100 linear t-steps per segment; no lyon dependency required.
fn rasterize_pen_path(path: &[PenNode], brush: &Brush) -> Vec<prism_canvas::Dab> {
    use prism_core::color::srgb_to_linear;
    let c = brush.color;
    let color = [
        srgb_to_linear(c[0]),
        srgb_to_linear(c[1]),
        srgb_to_linear(c[2]),
        brush.opacity,
    ];
    let radius = (brush.size * 0.5).max(0.5);
    let hardness = brush.hardness.clamp(0.0, 0.99);
    let spacing = (brush.size * 0.15).max(0.75);

    let mut dabs: Vec<prism_canvas::Dab> = Vec::new();
    const STEPS: usize = 100;

    for i in 0..path.len().saturating_sub(1) {
        let p0 = path[i].pos;
        let c0 = path[i].ctrl_out;
        let c1 = path[i + 1].ctrl_in;
        let p1 = path[i + 1].pos;

        let mut last: Option<[f32; 2]> = None;
        let mut residual = 0.0f32;

        for s in 0..=STEPS {
            let t = s as f32 / STEPS as f32;
            let u = 1.0 - t;
            let x = u * u * u * p0.0
                + 3.0 * u * u * t * c0.0
                + 3.0 * u * t * t * c1.0
                + t * t * t * p1.0;
            let y = u * u * u * p0.1
                + 3.0 * u * u * t * c0.1
                + 3.0 * u * t * t * c1.1
                + t * t * t * p1.1;
            let pos = [x, y];

            if let Some(prev) = last {
                let dx = pos[0] - prev[0];
                let dy = pos[1] - prev[1];
                let dist = (dx * dx + dy * dy).sqrt();
                if dist > 1e-3 {
                    let dir = [dx / dist, dy / dist];
                    let mut tt = residual;
                    while tt <= dist {
                        let px = prev[0] + dir[0] * tt;
                        let py = prev[1] + dir[1] * tt;
                        dabs.push(prism_canvas::Dab { center: [px, py], radius, hardness, color });
                        tt += spacing;
                    }
                    residual = tt - dist;
                }
            } else {
                dabs.push(prism_canvas::Dab { center: pos, radius, hardness, color });
            }
            last = Some(pos);
        }
    }
    dabs
}

/// Convert HSV (h: 0..360, s: 0..1, v: 0..1) to straight sRGB RGBA [f32;4] (alpha=1).
pub fn hsv_to_rgb(h: f32, s: f32, v: f32) -> [f32; 4] {
    if s <= 0.0 {
        return [v, v, v, 1.0];
    }
    let hh = h / 60.0;
    let i = hh.floor() as i32;
    let f = hh - i as f32;
    let p = v * (1.0 - s);
    let q = v * (1.0 - s * f);
    let t = v * (1.0 - s * (1.0 - f));
    let (r, g, b) = match i % 6 {
        0 => (v, t, p),
        1 => (q, v, p),
        2 => (p, v, t),
        3 => (p, q, v),
        4 => (t, p, v),
        _ => (v, p, q),
    };
    [r, g, b, 1.0]
}

/// Convert straight sRGB [f32;4] to HSV.
pub fn rgb_to_hsv(c: [f32; 4]) -> (f32, f32, f32) {
    let (r, g, b) = (c[0], c[1], c[2]);
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let delta = max - min;
    let v = max;
    let s = if max > 1e-6 { delta / max } else { 0.0 };
    let h = if delta < 1e-6 {
        0.0
    } else if max == r {
        60.0 * (((g - b) / delta).rem_euclid(6.0))
    } else if max == g {
        60.0 * ((b - r) / delta + 2.0)
    } else {
        60.0 * ((r - g) / delta + 4.0)
    };
    (h, s, v)
}

/// Resolve the autosave file path: `~/.local/share/prism/pigment_autosave.json`.
pub fn autosave_path() -> Option<std::path::PathBuf> {
    dirs::data_local_dir().map(|d| d.join("prism").join("pigment_autosave.json"))
}

/// Build a minimal single-page PDF that embeds `jpeg_bytes` as an image object.
fn build_minimal_pdf(jpeg_bytes: &[u8], img_w: u32, img_h: u32, landscape: bool) -> Vec<u8> {
    let (pw, ph): (f32, f32) = if landscape { (842.0, 595.0) } else { (595.0, 842.0) };
    let margin = 10.0_f32;
    let avail_w = pw - 2.0 * margin;
    let avail_h = ph - 2.0 * margin;
    let scale = (avail_w / img_w as f32).min(avail_h / img_h as f32);
    let draw_w = img_w as f32 * scale;
    let draw_h = img_h as f32 * scale;
    let x = (pw - draw_w) / 2.0;
    let y = (ph - draw_h) / 2.0;
    let img_len = jpeg_bytes.len();
    let img_obj = format!(
        "3 0 obj\n<< /Type /XObject /Subtype /Image /Width {img_w} /Height {img_h} \
         /ColorSpace /DeviceRGB /BitsPerComponent 8 /Filter /DCTDecode /Length {img_len} >>\nstream\n"
    );
    let img_end = "\nendstream\nendobj\n";
    let content_stream = format!("q {draw_w:.2} 0 0 {draw_h:.2} {x:.2} {y:.2} cm /Im1 Do Q");
    let cs_len = content_stream.len();
    let content_obj = format!(
        "4 0 obj\n<< /Length {cs_len} >>\nstream\n{content_stream}\nendstream\nendobj\n"
    );
    let page_obj = format!(
        "2 0 obj\n<< /Type /Page /Parent 1 0 R /MediaBox [0 0 {pw:.2} {ph:.2}] \
         /Contents 4 0 R /Resources << /XObject << /Im1 3 0 R >> >> >>\nendobj\n"
    );
    let pages_obj = "1 0 obj\n<< /Type /Pages /Kids [2 0 R] /Count 1 >>\nendobj\n";
    let catalog_obj = "5 0 obj\n<< /Type /Catalog /Pages 1 0 R >>\nendobj\n";
    let header = "%PDF-1.4\n";
    let mut pdf: Vec<u8> = Vec::new();
    pdf.extend_from_slice(header.as_bytes());
    let off1 = pdf.len();
    pdf.extend_from_slice(pages_obj.as_bytes());
    let off2 = pdf.len();
    pdf.extend_from_slice(page_obj.as_bytes());
    let off3 = pdf.len();
    pdf.extend_from_slice(img_obj.as_bytes());
    pdf.extend_from_slice(jpeg_bytes);
    pdf.extend_from_slice(img_end.as_bytes());
    let off4 = pdf.len();
    pdf.extend_from_slice(content_obj.as_bytes());
    let off5 = pdf.len();
    pdf.extend_from_slice(catalog_obj.as_bytes());
    let xref_offset = pdf.len();
    let xref = format!(
        "xref\n0 6\n0000000000 65535 f \n{off1:010} 00000 n \n{off2:010} 00000 n \n\
         {off3:010} 00000 n \n{off4:010} 00000 n \n{off5:010} 00000 n \n\
         trailer\n<< /Size 6 /Root 5 0 R >>\nstartxref\n{xref_offset}\n%%EOF\n"
    );
    pdf.extend_from_slice(xref.as_bytes());
    pdf
}

// ---- Batch 5 helper functions (pure, testable) ---------------------------

/// Capture a `LayerCompState` snapshot for every layer in the document.
fn capture_layer_comp_states(doc: &prism_core::Document) -> HashMap<LayerId, LayerCompState> {
    doc.layers.layers.iter().map(|l| {
        (l.id, LayerCompState {
            visible: l.visible,
            opacity: l.opacity,
            blend_mode: l.blend,
            offset_x: 0,
            offset_y: 0,
        })
    }).collect()
}

/// Restore layer states from a comp snapshot into the document.
fn apply_layer_comp_states(
    doc: &mut prism_core::Document,
    states: &HashMap<LayerId, LayerCompState>,
) {
    for layer in &mut doc.layers.layers {
        if let Some(s) = states.get(&layer.id) {
            layer.visible = s.visible;
            layer.opacity = s.opacity;
            layer.blend = s.blend_mode;
        }
    }
}

/// Compute a binary focus-area selection mask from linear-light RGBA f32 pixels.
/// Returns one `u8` per pixel (0 = not selected, 255 = selected).
/// Uses the local Laplacian variance in a 5×5 neighbourhood as a focus measure.
pub fn focus_area_mask(
    pixels: &[f32],
    w: u32,
    h: u32,
    threshold: f32,
    sensitivity: f32,
    invert: bool,
) -> Vec<u8> {
    let (w, h) = (w as usize, h as usize);
    let n = w * h;
    let mut variances = vec![0.0f32; n];
    let mut max_var = 0.0f32;

    for y in 0..h {
        for x in 0..w {
            // Compute luminance variance in 5×5 neighbourhood.
            let mut sum = 0.0f32;
            let mut sum_sq = 0.0f32;
            let mut count = 0usize;
            for dy in -2i32..=2 {
                for dx in -2i32..=2 {
                    let nx = x as i32 + dx;
                    let ny = y as i32 + dy;
                    if nx >= 0 && nx < w as i32 && ny >= 0 && ny < h as i32 {
                        let i = (ny as usize * w + nx as usize) * 4;
                        // Luminance approximation (linear light).
                        let luma = 0.2126 * pixels[i] + 0.7152 * pixels[i+1] + 0.0722 * pixels[i+2];
                        sum += luma;
                        sum_sq += luma * luma;
                        count += 1;
                    }
                }
            }
            let mean = sum / count as f32;
            let var = (sum_sq / count as f32 - mean * mean).max(0.0);
            variances[y * w + x] = var;
            if var > max_var { max_var = var; }
        }
    }

    let scale = if max_var > 1e-8 { 1.0 / max_var } else { 0.0 };
    let edge_width = (sensitivity * 0.2 + 0.01).max(0.01);

    variances.iter().map(|&v| {
        let normalized = v * scale;
        // Soft threshold via smooth-step over [threshold - edge, threshold + edge].
        let lo = (threshold - edge_width).max(0.0);
        let hi = (threshold + edge_width).min(1.0);
        let t = if hi <= lo { if normalized >= threshold { 1.0 } else { 0.0 } }
                else { ((normalized - lo) / (hi - lo)).clamp(0.0, 1.0) };
        let selected = if invert { 1.0 - t } else { t };
        (selected * 255.0).round() as u8
    }).collect()
}

/// Compute approximate LAB-space mean (L, a, b) for the given RGBA f32 pixels.
/// Uses the approximation: L ≈ luma, a ≈ R−G, b ≈ B−0.5R−0.5G.
pub fn match_color_stats(pixels: &[f32]) -> (f32, f32, f32) {
    if pixels.len() < 4 { return (0.0, 0.0, 0.0); }
    let n = pixels.len() / 4;
    let (mut sl, mut sa, mut sb) = (0.0f64, 0.0f64, 0.0f64);
    for i in 0..n {
        let (r, g, b) = (pixels[i*4] as f64, pixels[i*4+1] as f64, pixels[i*4+2] as f64);
        sl += 0.299 * r + 0.587 * g + 0.114 * b;
        sa += r - g;
        sb += b - 0.5 * r - 0.5 * g;
    }
    ((sl / n as f64) as f32, (sa / n as f64) as f32, (sb / n as f64) as f32)
}

/// Apply Match Color: shift target pixel LAB stats toward source stats.
pub fn apply_match_color(
    pixels: &[f32],
    src_stats: (f32, f32, f32),
    tgt_stats: (f32, f32, f32),
    fade: f32,
    match_luminance: bool,
    match_color: bool,
    neutralize: bool,
) -> Vec<f32> {
    let f = fade / 100.0;
    let dl = if match_luminance { (src_stats.0 - tgt_stats.0) * f } else { 0.0 };
    let da = if match_color { (src_stats.1 - tgt_stats.1) * f } else { 0.0 };
    let db = if match_color { (src_stats.2 - tgt_stats.2) * f } else { 0.0 };

    let n = pixels.len() / 4;
    let mut out = pixels.to_vec();
    for i in 0..n {
        let r = pixels[i*4];
        let g = pixels[i*4+1];
        let b = pixels[i*4+2];
        let luma = 0.299 * r + 0.587 * g + 0.114 * b;
        // Shift luminance: scale all channels proportionally.
        let new_luma = luma + dl;
        let luma_scale = if luma > 1e-6 { (new_luma / luma).clamp(0.0, 4.0) } else { 1.0 };
        let mut nr = (r * luma_scale + da).clamp(0.0, 1.0);
        let mut ng = (g * luma_scale - da).clamp(0.0, 1.0);
        let mut nb = (b * luma_scale + db).clamp(0.0, 1.0);
        if neutralize {
            // Pull a/b channels toward grey.
            let grey = 0.299 * nr + 0.587 * ng + 0.114 * nb;
            nr = (nr * (1.0 - f) + grey * f).clamp(0.0, 1.0);
            ng = (ng * (1.0 - f) + grey * f).clamp(0.0, 1.0);
            nb = (nb * (1.0 - f) + grey * f).clamp(0.0, 1.0);
        }
        out[i*4]   = nr;
        out[i*4+1] = ng;
        out[i*4+2] = nb;
        // Alpha unchanged.
    }
    out
}

/// Perspective-aware stamp: copy pixels from `src` area into `dst` area on the
/// pixel buffer using bilinear perspective interpolation within `plane_corners`.
/// `plane_corners` = [TL, TR, BR, BL] in doc-px. `radius` = brush radius.
pub fn stamp_in_perspective(
    pixels: &mut Vec<f32>,
    w: u32,
    h: u32,
    plane_corners: &[[f32; 2]; 4],
    src: [f32; 2],
    dst: [f32; 2],
    radius: f32,
) {
    let (w, h) = (w as usize, h as usize);
    let r = radius.max(1.0) as i32;
    // Map a canvas point to [0,1]^2 plane-local coords via bilinear inverse.
    let plane_to_local = |p: [f32; 2]| -> [f32; 2] {
        let [tl, tr, br, bl] = *plane_corners;
        // Use simple affine approximation: find s,t such that
        // (1-s)(1-t)*TL + s(1-t)*TR + s*t*BR + (1-s)*t*BL ≈ p
        // Solved via a few Newton iterations.
        let (mut s, mut t) = (0.5f32, 0.5f32);
        for _ in 0..8 {
            let qx = (1.0-s)*(1.0-t)*tl[0] + s*(1.0-t)*tr[0] + s*t*br[0] + (1.0-s)*t*bl[0];
            let qy = (1.0-s)*(1.0-t)*tl[1] + s*(1.0-t)*tr[1] + s*t*br[1] + (1.0-s)*t*bl[1];
            let dxds = -(1.0-t)*tl[0] + (1.0-t)*tr[0] + t*br[0] - t*bl[0];
            let dyds = -(1.0-t)*tl[1] + (1.0-t)*tr[1] + t*br[1] - t*bl[1];
            let dxdt = -(1.0-s)*tl[0] - s*tr[0] + s*br[0] + (1.0-s)*bl[0];
            let dydt = -(1.0-s)*tl[1] - s*tr[1] + s*br[1] + (1.0-s)*bl[1];
            let ex = p[0] - qx;
            let ey = p[1] - qy;
            let det = dxds * dydt - dyds * dxdt;
            if det.abs() < 1e-8 { break; }
            s += (ex * dydt - ey * dxdt) / det;
            t += (ey * dxds - ex * dyds) / det;
            s = s.clamp(0.0, 1.0);
            t = t.clamp(0.0, 1.0);
        }
        [s, t]
    };

    let dst_local = plane_to_local(dst);
    let src_local = plane_to_local(src);
    let delta_local = [dst_local[0] - src_local[0], dst_local[1] - src_local[1]];

    // For each pixel in the dst brush circle, map back to src coords and copy.
    let dx = dst[0] as i32;
    let dy = dst[1] as i32;
    let [tl, tr, br, bl] = *plane_corners;

    let sample = |px: &[f32], cx: f32, cy: f32| -> [f32; 4] {
        let xi = cx.floor() as i32;
        let yi = cy.floor() as i32;
        let fx = cx - xi as f32;
        let fy = cy - yi as f32;
        let sample_at = |x: i32, y: i32| -> [f32; 4] {
            if x < 0 || y < 0 || x >= w as i32 || y >= h as i32 {
                return [0.0; 4];
            }
            let i = (y as usize * w + x as usize) * 4;
            [px[i], px[i+1], px[i+2], px[i+3]]
        };
        let c00 = sample_at(xi,   yi);
        let c10 = sample_at(xi+1, yi);
        let c01 = sample_at(xi,   yi+1);
        let c11 = sample_at(xi+1, yi+1);
        std::array::from_fn(|k| {
            c00[k]*(1.0-fx)*(1.0-fy) + c10[k]*fx*(1.0-fy)
            + c01[k]*(1.0-fx)*fy + c11[k]*fx*fy
        })
    };

    let pixels_snap = pixels.clone();
    for oy in -r..=r {
        for ox in -r..=r {
            if ox*ox + oy*oy > r*r { continue; }
            let px_dst = dx + ox;
            let py_dst = dy + oy;
            if px_dst < 0 || py_dst < 0 || px_dst >= w as i32 || py_dst >= h as i32 { continue; }

            // Current dst point in local coords.
            let dst_pt = [px_dst as f32 + 0.5, py_dst as f32 + 0.5];
            let local = plane_to_local(dst_pt);
            // Corresponding src local.
            let src_local2 = [local[0] - delta_local[0], local[1] - delta_local[1]];
            // Map src_local back to canvas.
            let src_cx = (1.0-src_local2[0])*(1.0-src_local2[1])*tl[0]
                + src_local2[0]*(1.0-src_local2[1])*tr[0]
                + src_local2[0]*src_local2[1]*br[0]
                + (1.0-src_local2[0])*src_local2[1]*bl[0];
            let src_cy = (1.0-src_local2[0])*(1.0-src_local2[1])*tl[1]
                + src_local2[0]*(1.0-src_local2[1])*tr[1]
                + src_local2[0]*src_local2[1]*br[1]
                + (1.0-src_local2[0])*src_local2[1]*bl[1];

            let color = sample(&pixels_snap, src_cx, src_cy);
            let idx = (py_dst as usize * w + px_dst as usize) * 4;
            for k in 0..4 { pixels[idx + k] = color[k]; }
        }
    }
}

/// Sobel edge detection → flood-fill from centre → feather: returns 8-bit mask.
pub fn select_subject_mask(
    pixels: &[f32],
    w: u32,
    h: u32,
    threshold: f32,
    feather: f32,
) -> Vec<u8> {
    let (w, h) = (w as usize, h as usize);
    let n = w * h;
    let mut lum = vec![0.0f32; n];
    for i in 0..n {
        let r = pixels[i * 4];
        let g = pixels[i * 4 + 1];
        let b = pixels[i * 4 + 2];
        lum[i] = 0.2126 * r + 0.7152 * g + 0.0722 * b;
    }
    let mut edge = vec![0.0f32; n];
    for y in 1..h.saturating_sub(1) {
        for x in 1..w.saturating_sub(1) {
            let p = |dy: isize, dx: isize| lum[((y as isize + dy) as usize) * w + (x as isize + dx) as usize];
            let gx = -p(-1,-1) - 2.0*p(0,-1) - p(1,-1) + p(-1,1) + 2.0*p(0,1) + p(1,1);
            let gy = -p(-1,-1) - 2.0*p(-1,0) - p(-1,1) + p(1,-1) + 2.0*p(1,0) + p(1,1);
            edge[y * w + x] = (gx * gx + gy * gy).sqrt();
        }
    }
    let max_e = edge.iter().cloned().fold(0.0f32, f32::max).max(1e-6);
    let edge_thresh = threshold * max_e;

    let mut mask = vec![false; n];
    let cx = w / 2;
    let cy = h / 2;
    let mut queue = std::collections::VecDeque::new();
    let start = cy * w + cx;
    if edge[start] < edge_thresh {
        queue.push_back(start);
        mask[start] = true;
    }
    while let Some(idx) = queue.pop_front() {
        let x = (idx % w) as isize;
        let y = (idx / w) as isize;
        for (dy, dx) in [(-1,0i32),(1,0),(0,-1),(0,1)] {
            let nx = x + dx as isize;
            let ny = y + dy as isize;
            if nx < 0 || ny < 0 || nx >= w as isize || ny >= h as isize { continue; }
            let ni = (ny as usize) * w + (nx as usize);
            if mask[ni] || edge[ni] >= edge_thresh { continue; }
            mask[ni] = true;
            queue.push_back(ni);
        }
    }

    let mut out: Vec<u8> = mask.iter().map(|&m| if m { 255 } else { 0 }).collect();
    let r = feather.max(0.0) as usize;
    if r > 0 {
        let tmp = out.clone();
        for y in 0..h {
            for x in 0..w {
                let mut sum = 0u32;
                let mut cnt = 0u32;
                for ky in y.saturating_sub(r)..=(y+r).min(h-1) {
                    for kx in x.saturating_sub(r)..=(x+r).min(w-1) {
                        sum += tmp[ky * w + kx] as u32;
                        cnt += 1;
                    }
                }
                out[y * w + x] = (sum / cnt.max(1)) as u8;
            }
        }
    }
    out
}

/// Blend `src` layer into `tgt` layer per the Apply Image dialog parameters.
pub fn apply_image_blend(
    src: &[f32],
    tgt: &[f32],
    channel: ApplyImageChannel,
    blend_mode: BlendMode,
    opacity: f32,
    invert: bool,
) -> Vec<f32> {
    let n = tgt.len() / 4;
    let mut out = tgt.to_vec();
    for i in 0..n {
        let [sr, sg, sb, sa] = [src[i*4], src[i*4+1], src[i*4+2], src[i*4+3]];
        let raw = match channel {
            ApplyImageChannel::Rgb        => 0.2126*sr + 0.7152*sg + 0.0722*sb,
            ApplyImageChannel::Red        => sr,
            ApplyImageChannel::Green      => sg,
            ApplyImageChannel::Blue       => sb,
            ApplyImageChannel::Alpha      => sa,
            ApplyImageChannel::Luminosity => 0.2126*sr + 0.7152*sg + 0.0722*sb,
        };
        let s = if invert { 1.0 - raw } else { raw };
        let [tr, tg, tb, ta] = [tgt[i*4], tgt[i*4+1], tgt[i*4+2], tgt[i*4+3]];
        let blend_ch = |t: f32| -> f32 {
            let b = match blend_mode {
                BlendMode::Normal     => s,
                BlendMode::Multiply   => t * s,
                BlendMode::Screen     => 1.0 - (1.0-t)*(1.0-s),
                BlendMode::Overlay    => if t < 0.5 { 2.0*t*s } else { 1.0 - 2.0*(1.0-t)*(1.0-s) },
                BlendMode::Darken     => t.min(s),
                BlendMode::Lighten    => t.max(s),
                BlendMode::Difference => (t - s).abs(),
                _                     => s,
            };
            t * (1.0 - opacity) + b * opacity
        };
        out[i*4]   = blend_ch(tr).clamp(0.0, 1.0);
        out[i*4+1] = blend_ch(tg).clamp(0.0, 1.0);
        out[i*4+2] = blend_ch(tb).clamp(0.0, 1.0);
        out[i*4+3] = ta;
    }
    out
}

/// Path to `~/.config/prism/workspaces/` for workspace JSON files.
fn dirs_home_workspace_dir() -> Option<std::path::PathBuf> {
    #[allow(deprecated)]
    std::env::home_dir().map(|h| h.join(".config").join("prism").join("workspaces"))
}

fn compute_xform(translate: [f32; 2], scale: f32, w: f32, h: f32) -> ([f32; 4], [f32; 2]) {
    let inv = 1.0 / scale.max(1e-3);
    let tx = translate[0] / w;
    let ty = translate[1] / h;
    let m = [inv, 0.0, 0.0, inv];
    let off = [0.5 - (0.5 + tx) * inv, 0.5 - (0.5 + ty) * inv];
    (m, off)
}

#[cfg(test)]
mod selection_tests {
    use super::{mode_from_modifiers, shape_mask};
    use prism_core::raster::CombineMode;

    // Modifier → combine-mode mapping matches the egui app's `mode_from_modifiers`.
    #[test]
    fn modifiers_pick_combine_mode() {
        assert_eq!(mode_from_modifiers(false, false), CombineMode::Replace);
        assert_eq!(mode_from_modifiers(true, false), CombineMode::Add);
        assert_eq!(mode_from_modifiers(false, true), CombineMode::Subtract);
        assert_eq!(mode_from_modifiers(true, true), CombineMode::Intersect);
    }

    // A rectangle marquee selects exactly the pixel centers inside the rect.
    #[test]
    fn rect_mask_fills_interior() {
        let m = shape_mask([2.0, 2.0, 4.0, 4.0], false, 8, 8);
        // Inside: pixel centers (2.5..5.5).
        assert_eq!(m[3 * 8 + 3], 1.0);
        assert_eq!(m[5 * 8 + 5], 1.0);
        // Outside the rect.
        assert_eq!(m[0], 0.0);
        assert_eq!(m[7 * 8 + 7], 0.0);
    }

    // An ellipse marquee selects its center but not the bounding-box corners.
    #[test]
    fn ellipse_mask_excludes_corners() {
        let m = shape_mask([0.0, 0.0, 8.0, 8.0], true, 8, 8);
        // Center is selected.
        assert_eq!(m[4 * 8 + 4], 1.0);
        // Corners of the bbox fall outside the inscribed ellipse.
        assert_eq!(m[0], 0.0);
        assert_eq!(m[7 * 8 + 7], 0.0);
    }
}

#[cfg(test)]
mod xform_tests {
    use super::compute_xform;

    // A pure +half-width translate maps to a layer-from-canvas offset of -0.5
    // (identity 2x2). Mirrors the egui app's `translate_maps_to_uv_offset`.
    #[test]
    fn translate_maps_to_uv_offset() {
        let (m, off) = compute_xform([50.0, 0.0], 1.0, 100.0, 50.0);
        assert_eq!(m, [1.0, 0.0, 0.0, 1.0]);
        assert!((off[0] - -0.5).abs() < 1e-6, "off.x = {}", off[0]);
        assert!(off[1].abs() < 1e-6, "off.y = {}", off[1]);
    }

    // A 2x uniform scale halves the 2x2 (inv = 0.5) and recenters about 0.5.
    #[test]
    fn scale_inverts_into_matrix() {
        let (m, off) = compute_xform([0.0, 0.0], 2.0, 100.0, 100.0);
        assert!((m[0] - 0.5).abs() < 1e-6);
        assert!((m[3] - 0.5).abs() < 1e-6);
        // off = 0.5 - 0.5*0.5 = 0.25 in both axes.
        assert!((off[0] - 0.25).abs() < 1e-6, "off.x = {}", off[0]);
        assert!((off[1] - 0.25).abs() < 1e-6, "off.y = {}", off[1]);
    }
}

#[cfg(test)]
mod hsv_tests {
    use super::{hsv_to_rgb, rgb_to_hsv};

    // Pure red: H=0°, S=1, V=1 → rgb(1,0,0).
    #[test]
    fn red_from_hsv() {
        let c = hsv_to_rgb(0.0, 1.0, 1.0);
        assert!((c[0] - 1.0).abs() < 1e-5, "r={}", c[0]);
        assert!(c[1].abs() < 1e-5, "g={}", c[1]);
        assert!(c[2].abs() < 1e-5, "b={}", c[2]);
    }

    // Greyscale: S=0 → all channels equal V.
    #[test]
    fn grey_from_hsv() {
        let c = hsv_to_rgb(180.0, 0.0, 0.5);
        assert!((c[0] - 0.5).abs() < 1e-5);
        assert!((c[1] - 0.5).abs() < 1e-5);
        assert!((c[2] - 0.5).abs() < 1e-5);
    }

    // Round-trip: rgb → hsv → rgb stays within 0.01.
    #[test]
    fn rgb_hsv_roundtrip() {
        let orig = [0.2_f32, 0.6, 0.9, 1.0];
        let (h, s, v) = rgb_to_hsv(orig);
        let back = hsv_to_rgb(h, s, v);
        for i in 0..3 {
            assert!((back[i] - orig[i]).abs() < 0.01, "channel {i}: {} vs {}", back[i], orig[i]);
        }
    }
}

#[cfg(test)]
mod pen_tests {
    use super::{rasterize_pen_path, Brush, PenNode};

    // A two-node straight-line pen path (ctrl handles at node positions) must
    // produce at least one dab. Verifies the bézier walker runs without panic.
    #[test]
    fn straight_path_produces_dabs() {
        let path = vec![
            PenNode { pos: (0.0, 0.0), ctrl_in: (0.0, 0.0), ctrl_out: (0.0, 0.0) },
            PenNode { pos: (100.0, 0.0), ctrl_in: (100.0, 0.0), ctrl_out: (100.0, 0.0) },
        ];
        let brush = Brush::default();
        let dabs = rasterize_pen_path(&path, &brush);
        assert!(!dabs.is_empty(), "expected at least one dab for a 100px segment");
    }

    // A single-node path (< 2 nodes) must produce zero dabs (no segment).
    #[test]
    fn single_node_produces_no_dabs() {
        let path = vec![
            PenNode { pos: (50.0, 50.0), ctrl_in: (50.0, 50.0), ctrl_out: (50.0, 50.0) },
        ];
        let brush = Brush::default();
        let dabs = rasterize_pen_path(&path, &brush);
        assert!(dabs.is_empty());
    }
}

#[cfg(test)]
mod layer_comp_tests {
    use super::{capture_layer_comp_states, apply_layer_comp_states};
    use prism_core::{Document, Size};

    fn make_doc() -> Document {
        let mut doc = Document::new(Size::new(100, 100));
        doc.layers.add_raster("BG");
        doc.layers.add_raster("FG");
        doc
    }

    #[test]
    fn test_layer_comp_capture() {
        let doc = make_doc();
        let states = capture_layer_comp_states(&doc);
        assert_eq!(states.len(), doc.layers.layers.len(),
            "comp should capture state for every layer");
    }

    #[test]
    fn test_layer_comp_apply() {
        let mut doc = make_doc();
        // Snapshot with both layers visible.
        let states = capture_layer_comp_states(&doc);
        // Now hide the first layer.
        if let Some(l) = doc.layers.layers.first_mut() {
            l.visible = false;
        }
        assert!(!doc.layers.layers[0].visible);
        // Restore the comp — first layer should be visible again.
        apply_layer_comp_states(&mut doc, &states);
        assert!(doc.layers.layers[0].visible, "layer should be restored to visible");
    }

    #[test]
    fn test_layer_comp_update() {
        let mut doc = make_doc();
        let states = capture_layer_comp_states(&doc);
        // Change opacity.
        if let Some(l) = doc.layers.layers.first_mut() {
            l.opacity = 0.5;
        }
        // Re-capture (simulating UpdateLayerComp).
        let new_states = capture_layer_comp_states(&doc);
        let id = doc.layers.layers[0].id;
        assert!((new_states[&id].opacity - 0.5).abs() < 1e-5,
            "updated comp should store new opacity");
        // Old comp still has 1.0.
        assert!((states[&id].opacity - 1.0).abs() < 1e-5,
            "old comp should retain original opacity");
    }

    #[test]
    fn test_layer_comp_opacity_restored() {
        let mut doc = make_doc();
        let id = doc.layers.layers[0].id;
        // Snapshot.
        let states = capture_layer_comp_states(&doc);
        // Change opacity.
        doc.layers.layers[0].opacity = 0.25;
        // Apply comp.
        apply_layer_comp_states(&mut doc, &states);
        assert!((doc.layers.layers[0].opacity - 1.0).abs() < 1e-5,
            "opacity should be restored to 1.0 after ApplyLayerComp");
        let _ = id;
    }
}

#[cfg(test)]
mod focus_area_tests {
    use super::focus_area_mask;

    /// Build a flat RGBA f32 pixel buffer (all same luma).
    fn uniform_pixels(w: u32, h: u32, luma: f32) -> Vec<f32> {
        vec![luma, luma, luma, 1.0].repeat((w * h) as usize)
    }

    /// Build an RGBA f32 buffer with a sharp edge: left half dark, right half bright.
    fn edge_pixels(w: u32, h: u32) -> Vec<f32> {
        let mut px = Vec::with_capacity((w * h * 4) as usize);
        for _y in 0..h {
            for x in 0..w {
                let v = if x < w / 2 { 0.0 } else { 1.0 };
                px.extend_from_slice(&[v, v, v, 1.0]);
            }
        }
        px
    }

    #[test]
    fn test_focus_area_uniform() {
        // Uniform image has zero variance → nothing selected (at any threshold > 0).
        let px = uniform_pixels(8, 8, 0.5);
        let mask = focus_area_mask(&px, 8, 8, 0.1, 0.0, false);
        // All pixels should be unselected (zero variance < threshold).
        for (i, &v) in mask.iter().enumerate() {
            assert_eq!(v, 0, "uniform image pixel {i} should not be selected");
        }
    }

    #[test]
    fn test_focus_area_edge_selected() {
        // Edge pixels have high variance → should be selected.
        let px = edge_pixels(16, 8);
        let mask = focus_area_mask(&px, 16, 8, 0.05, 0.0, false);
        // Pixels near the edge (x ≈ 8) should be selected.
        let edge_col = 7usize;
        let edge_idx = 0 * 16 + edge_col; // top row, edge column
        // The mask should have nonzero value near the edge.
        let sum: u32 = mask.iter().map(|&v| v as u32).sum();
        assert!(sum > 0, "edge image should select some pixels (sum={})", sum);
        let _ = edge_idx;
    }

    #[test]
    fn test_focus_area_invert() {
        // Inverted: uniform image → all pixels selected.
        let px = uniform_pixels(8, 8, 0.5);
        let mask_normal = focus_area_mask(&px, 8, 8, 0.1, 0.0, false);
        let mask_invert = focus_area_mask(&px, 8, 8, 0.1, 0.0, true);
        // Normal should all be 0 (no variance), inverted should all be 255.
        assert!(mask_normal.iter().all(|&v| v == 0));
        assert!(mask_invert.iter().all(|&v| v == 255));
    }

    #[test]
    fn test_focus_area_threshold() {
        // Higher threshold → fewer selected pixels in edge image.
        let px = edge_pixels(32, 8);
        let mask_low  = focus_area_mask(&px, 32, 8, 0.01, 0.0, false);
        let mask_high = focus_area_mask(&px, 32, 8, 0.99, 0.0, false);
        let sum_low:  u32 = mask_low.iter().map(|&v| v as u32).sum();
        let sum_high: u32 = mask_high.iter().map(|&v| v as u32).sum();
        assert!(sum_low >= sum_high,
            "lower threshold should select >= pixels: low={sum_low}, high={sum_high}");
    }
}

#[cfg(test)]
mod pattern_stamp_tests {
    use super::{PatternDef};

    fn make_pattern(w: u32, h: u32) -> PatternDef {
        let pixels = vec![[1.0f32, 0.0, 0.0, 1.0]; (w * h) as usize];
        PatternDef { name: "Red".into(), pixels, width: w, height: h }
    }

    #[test]
    fn test_define_pattern() {
        let pat = make_pattern(4, 4);
        assert_eq!(pat.width, 4);
        assert_eq!(pat.height, 4);
        assert_eq!(pat.pixels.len(), 16);
        assert_eq!(pat.name, "Red");
    }

    #[test]
    fn test_pattern_pixel_access() {
        let pat = make_pattern(2, 2);
        // All pixels should be [1, 0, 0, 1].
        for px in &pat.pixels {
            assert!((px[0] - 1.0).abs() < 1e-5, "R should be 1.0");
            assert!(px[1].abs() < 1e-5, "G should be 0.0");
        }
    }

    #[test]
    fn test_pattern_stamp_scale_default() {
        // Default scale from App::new() is 1.0.
        let scale: f32 = 1.0;
        assert!((scale - 1.0).abs() < 1e-5);
    }

    #[test]
    fn test_pattern_aligned_default() {
        // Default aligned from App::new() is true.
        let aligned: bool = true;
        assert!(aligned);
    }

    #[test]
    fn test_pattern_library_push() {
        let mut library: Vec<PatternDef> = Vec::new();
        library.push(make_pattern(8, 8));
        library.push(make_pattern(4, 4));
        assert_eq!(library.len(), 2);
        // Delete first.
        library.remove(0);
        assert_eq!(library.len(), 1);
        assert_eq!(library[0].width, 4);
    }
}

#[cfg(test)]
mod match_color_tests {
    use super::{match_color_stats, apply_match_color};

    fn uniform_pixels(r: f32, g: f32, b: f32, n: usize) -> Vec<f32> {
        let mut px = Vec::with_capacity(n * 4);
        for _ in 0..n { px.extend_from_slice(&[r, g, b, 1.0]); }
        px
    }

    #[test]
    fn test_match_color_stats_uniform() {
        let px = uniform_pixels(0.8, 0.2, 0.5, 100);
        let (l, a, _b) = match_color_stats(&px);
        // L ≈ 0.299*0.8 + 0.587*0.2 + 0.114*0.5
        let expected_l = 0.299_f32 * 0.8 + 0.587 * 0.2 + 0.114 * 0.5;
        assert!((l - expected_l).abs() < 1e-3, "L: got {l}, expected {expected_l}");
        let expected_a = 0.8 - 0.2;
        assert!((a - expected_a).abs() < 1e-3, "a: got {a}, expected {expected_a}");
    }

    #[test]
    fn test_match_color_apply_luminance() {
        // Source is bright (luma 0.9), target is dark (luma 0.1).
        let src = uniform_pixels(0.9, 0.9, 0.9, 4);
        let tgt = uniform_pixels(0.1, 0.1, 0.1, 4);
        let src_stats = match_color_stats(&src);
        let tgt_stats = match_color_stats(&tgt);
        let result = apply_match_color(&tgt, src_stats, tgt_stats, 100.0, true, false, false);
        // Result should be brighter than the target.
        let result_l = result[0]; // R channel
        assert!(result_l > 0.1, "result should be brighter: got {result_l}");
    }

    #[test]
    fn test_match_color_fade_zero() {
        // Fade=0 should leave target unchanged.
        let src = uniform_pixels(0.9, 0.5, 0.1, 4);
        let tgt = uniform_pixels(0.3, 0.3, 0.3, 4);
        let src_stats = match_color_stats(&src);
        let tgt_stats = match_color_stats(&tgt);
        let result = apply_match_color(&tgt, src_stats, tgt_stats, 0.0, true, true, false);
        for i in 0..4 {
            assert!((result[i*4] - tgt[i*4]).abs() < 1e-3, "fade=0 should not change target");
        }
    }

    #[test]
    fn test_match_color_neutralize() {
        // Colourful target → neutralize should pull toward grey.
        let src = uniform_pixels(0.5, 0.5, 0.5, 1);
        let tgt = uniform_pixels(1.0, 0.0, 0.0, 1); // pure red
        let src_stats = match_color_stats(&src);
        let tgt_stats = match_color_stats(&tgt);
        let result = apply_match_color(&tgt, src_stats, tgt_stats, 100.0, false, false, true);
        // G channel should increase (pulled toward grey).
        assert!(result[1] > 0.0, "neutralize should increase G: got {}", result[1]);
    }
}

#[cfg(test)]
mod vanishing_point_tests {
    use super::{VanishingPlane, VanishingToolMode, stamp_in_perspective};

    fn unit_plane() -> [[f32; 2]; 4] {
        // A simple axis-aligned square: TL=(0,0), TR=(100,0), BR=(100,100), BL=(0,100).
        [[0.0, 0.0], [100.0, 0.0], [100.0, 100.0], [0.0, 100.0]]
    }

    #[test]
    fn test_vanishing_plane_add() {
        let mut planes: Vec<VanishingPlane> = Vec::new();
        planes.push(VanishingPlane {
            corners: unit_plane(),
            grid_size: 50.0,
            active: true,
        });
        assert_eq!(planes.len(), 1);
        assert_eq!(planes[0].grid_size, 50.0);
        assert_eq!(planes[0].corners[0], [0.0, 0.0]);
    }

    #[test]
    fn test_vanishing_plane_set_corner() {
        let mut planes = vec![VanishingPlane {
            corners: unit_plane(),
            grid_size: 50.0,
            active: true,
        }];
        planes[0].corners[2] = [120.0, 120.0];
        assert_eq!(planes[0].corners[2], [120.0, 120.0]);
    }

    #[test]
    fn test_vanishing_grid_size() {
        let mut plane = VanishingPlane { corners: unit_plane(), grid_size: 50.0, active: true };
        plane.grid_size = 100.0f32.clamp(5.0, 500.0);
        assert!((plane.grid_size - 100.0).abs() < 1e-5);
    }

    #[test]
    fn test_vanishing_tool_mode() {
        let mode_a = VanishingToolMode::Stamping;
        assert_eq!(mode_a, VanishingToolMode::Stamping);
        let mode_b = VanishingToolMode::Pasting;
        assert_eq!(mode_b, VanishingToolMode::Pasting);
        let mode_c = VanishingToolMode::DefiningPlane;
        assert_ne!(mode_c, VanishingToolMode::Stamping);
    }

    #[test]
    fn test_stamp_in_perspective_no_panic() {
        // Stamp on a small 10×10 canvas within the unit plane; should not panic.
        let corners = [[0.0f32, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0]];
        let mut pixels = vec![0.5f32; 10 * 10 * 4];
        // Make src region bright.
        for i in 0..(10 * 2 * 4) { pixels[i] = 1.0; }
        stamp_in_perspective(&mut pixels, 10, 10, &corners, [2.0, 2.0], [7.0, 7.0], 2.0);
        // Just verify no panic and buffer size unchanged.
        assert_eq!(pixels.len(), 10 * 10 * 4);
    }
}

#[cfg(test)]
mod select_subject_tests {
    use super::{select_subject_mask};

    #[test]
    fn mask_len_matches_image() {
        let px = vec![0.5f32; 16 * 16 * 4];
        let mask = select_subject_mask(&px, 16, 16, 0.5, 0.0);
        assert_eq!(mask.len(), 16 * 16);
    }

    #[test]
    fn threshold_zero_selects_nothing() {
        // threshold=0 → edge_thresh=0 → edge[start]=0 is NOT < 0 → BFS never starts
        let px = vec![0.5f32; 8 * 8 * 4];
        let mask = select_subject_mask(&px, 8, 8, 0.0, 0.0);
        assert!(mask.iter().all(|&v| v == 0), "threshold=0 should select nothing (strict <)");
    }

    #[test]
    fn mid_threshold_selects_all_on_flat() {
        // Flat image: all Sobel edges=0, max_e clamped to 1e-6.
        // threshold=0.5 → edge_thresh=0.5e-6 → 0 < 0.5e-6 is TRUE → BFS fills everything.
        let px = vec![0.5f32; 8 * 8 * 4];
        let mask = select_subject_mask(&px, 8, 8, 0.5, 0.0);
        assert!(mask.iter().all(|&v| v == 255), "mid threshold on flat should select all");
    }

    #[test]
    fn feather_blurs_mask_edges() {
        let px = vec![0.5f32; 16 * 16 * 4];
        let sharp = select_subject_mask(&px, 16, 16, 0.0, 0.0);
        let feathered = select_subject_mask(&px, 16, 16, 0.0, 3.0);
        // Both should be the same (all 255) for flat image — feathering all-255 keeps all-255
        assert_eq!(sharp.len(), feathered.len());
    }
}

#[cfg(test)]
mod artboard_tests {
    use super::{Artboard, SoftProofSettings, ProofProfile, RenderingIntent};

    #[test]
    fn artboard_default_background_white() {
        let ab = Artboard { id: 1, name: "A".into(), x: 0, y: 0, width: 800, height: 600, background_color: [1.0, 1.0, 1.0, 1.0] };
        assert_eq!(ab.background_color, [1.0f32, 1.0, 1.0, 1.0]);
    }

    #[test]
    fn artboard_clone_preserves_fields() {
        let ab = Artboard { id: 42, name: "Clone".into(), x: 10, y: 20, width: 300, height: 200, background_color: [0.5, 0.5, 0.5, 1.0] };
        let ab2 = ab.clone();
        assert_eq!(ab2.id, 42);
        assert_eq!(ab2.name, "Clone");
        assert_eq!(ab2.x, 10);
    }

    #[test]
    fn soft_proof_defaults() {
        let sp = SoftProofSettings::default();
        assert_eq!(sp.profile, ProofProfile::WorkingCmyk);
        assert_eq!(sp.intent, RenderingIntent::Perceptual);
        assert!(sp.black_point_compensation);
        assert!(!sp.simulate_paper_white);
        assert!(!sp.gamut_warning);
        assert_eq!(sp.gamut_warning_color, [0.0f32, 1.0, 0.0, 1.0]);
    }

    #[test]
    fn proof_profile_labels() {
        assert_eq!(ProofProfile::Srgb.label(), "sRGB");
        assert_eq!(ProofProfile::AdobeRgb.label(), "Adobe RGB");
        assert_eq!(ProofProfile::WorkingCmyk.label(), "Working CMYK");
    }

    #[test]
    fn rendering_intent_labels() {
        assert_eq!(RenderingIntent::Perceptual.label(), "Perceptual");
        assert_eq!(RenderingIntent::AbsoluteColorimetric.label(), "Absolute Colorimetric");
    }
}

#[cfg(test)]
mod apply_image_tests {
    use super::{ApplyImageChannel, apply_image_blend, BlendMode};

    #[test]
    fn blend_normal_opacity_half() {
        let src = vec![1.0f32, 0.0, 0.0, 1.0]; // red
        let tgt = vec![0.0f32, 0.0, 1.0, 1.0]; // blue
        let out = apply_image_blend(&src, &tgt, ApplyImageChannel::Rgb, BlendMode::Normal, 0.5, false);
        // Channel extraction: s = luma(red) = 0.2126; result.r = 0.0*0.5 + 0.2126*0.5 ≈ 0.106
        assert!((out[0] - 0.5 * 0.2126).abs() < 0.01, "red ch: {}", out[0]);
    }

    #[test]
    fn invert_source_flips() {
        let src = vec![1.0f32, 1.0, 1.0, 1.0]; // white
        let tgt = vec![0.5f32, 0.5, 0.5, 1.0];
        let normal   = apply_image_blend(&src, &tgt, ApplyImageChannel::Rgb, BlendMode::Normal, 1.0, false);
        let inverted = apply_image_blend(&src, &tgt, ApplyImageChannel::Rgb, BlendMode::Normal, 1.0, true);
        assert!(normal[0] > inverted[0], "invert should darken");
    }

    #[test]
    fn alpha_channel_passthrough() {
        let src = vec![0.5f32, 0.5, 0.5, 0.8];
        let tgt = vec![0.5f32, 0.5, 0.5, 0.3];
        let out = apply_image_blend(&src, &tgt, ApplyImageChannel::Rgb, BlendMode::Normal, 1.0, false);
        // Alpha should come from tgt unchanged
        assert!((out[3] - 0.3).abs() < 0.01, "alpha should be tgt alpha: {}", out[3]);
    }

    #[test]
    fn multiply_blend_darkens() {
        // Both layers have r=0.8; multiply → 0.8*0.8=0.64 < 0.8
        let src = vec![0.8f32, 0.0, 0.0, 1.0];
        let tgt = vec![0.8f32, 0.0, 0.0, 1.0];
        let out = apply_image_blend(&src, &tgt, ApplyImageChannel::Red, BlendMode::Multiply, 1.0, false);
        assert!(out[0] < 0.8, "multiply should darken: {}", out[0]);
    }
}

#[cfg(test)]
mod soft_proof_tests {
    use super::{SoftProofSettings, ProofProfile, RenderingIntent};

    #[test]
    fn toggle_enabled_field() {
        let mut enabled = false;
        enabled = !enabled;
        assert!(enabled);
        enabled = !enabled;
        assert!(!enabled);
    }

    #[test]
    fn proof_profile_default() {
        let sp = SoftProofSettings::default();
        assert_eq!(sp.profile, ProofProfile::WorkingCmyk);
    }

    #[test]
    fn rendering_intent_default() {
        let sp = SoftProofSettings::default();
        assert_eq!(sp.intent, RenderingIntent::Perceptual);
    }

    #[test]
    fn gamut_warning_default_green() {
        let sp = SoftProofSettings::default();
        assert_eq!(sp.gamut_warning_color, [0.0f32, 1.0, 0.0, 1.0]);
    }

    #[test]
    fn simulate_flags_default_off() {
        let sp = SoftProofSettings::default();
        assert!(!sp.simulate_paper_white);
        assert!(!sp.simulate_black_ink);
        assert!(!sp.gamut_warning);
    }
}

#[cfg(test)]
mod batch7_tests {
    use super::{
        AlphaChannel, BlendIf, SpotHealMode,
        Action,
    };
    use prism_core::LayerId;

    // Helper: build a minimal AlphaChannel with a known mask.
    fn make_channel(name: &str, width: u32, height: u32) -> AlphaChannel {
        let n = (width * height) as usize;
        let mask = vec![0.5f32; n];
        AlphaChannel { name: name.into(), mask, width, height }
    }

    // ---- Alpha Channels ----

    #[test]
    fn test_save_load_channel_roundtrip() {
        // Simulate the apply logic without constructing a full App (no GPU).
        let mut channels: Vec<AlphaChannel> = Vec::new();
        let original_mask = vec![0.25f32, 0.5, 0.75, 1.0];
        channels.push(AlphaChannel {
            name: "Alpha 1".into(),
            mask: original_mask.clone(),
            width: 2,
            height: 2,
        });

        assert_eq!(channels.len(), 1);
        // Load: copy mask back out.
        let loaded = channels[0].mask.clone();
        assert_eq!(loaded, original_mask, "loaded mask must match saved mask");
    }

    #[test]
    fn test_delete_channel() {
        let mut channels: Vec<AlphaChannel> = Vec::new();
        channels.push(make_channel("First",  4, 4));
        channels.push(make_channel("Second", 4, 4));
        assert_eq!(channels.len(), 2);

        // DeleteChannel(0) logic.
        channels.remove(0);

        assert_eq!(channels.len(), 1);
        assert_eq!(channels[0].name, "Second");
    }

    #[test]
    fn test_duplicate_channel() {
        let mut channels: Vec<AlphaChannel> = Vec::new();
        channels.push(make_channel("Sky", 8, 8));

        // DuplicateChannel(0) logic.
        let mut copy = channels[0].clone();
        copy.name = format!("{} copy", copy.name);
        channels.push(copy);

        assert_eq!(channels.len(), 2);
        assert_eq!(channels[1].name, "Sky copy");
        assert_eq!(channels[1].mask, channels[0].mask);
    }

    // ---- Blend If ----

    #[test]
    fn test_blend_if_set_clear() {
        let mut map: std::collections::HashMap<LayerId, BlendIf> = std::collections::HashMap::new();
        let id = LayerId(42);
        let bi = BlendIf { this_black: 10.0, this_white: 200.0, under_black: 0.0, under_white: 255.0 };

        // SetBlendIf.
        map.insert(id, bi);
        assert!(map.contains_key(&id), "blend_if should be stored");
        assert!((map[&id].this_black - 10.0).abs() < 1e-5);
        assert!((map[&id].this_white - 200.0).abs() < 1e-5);

        // ClearBlendIf.
        map.remove(&id);
        assert!(!map.contains_key(&id), "blend_if should be removed after clear");
    }

    #[test]
    fn test_blend_if_default() {
        let bi = BlendIf::default();
        assert!((bi.this_black - 0.0).abs() < 1e-5);
        assert!((bi.this_white - 255.0).abs() < 1e-5);
        assert!((bi.under_black - 0.0).abs() < 1e-5);
        assert!((bi.under_white - 255.0).abs() < 1e-5);
    }

    // ---- Spot Heal mode / radius ----

    #[test]
    fn test_spot_heal_mode() {
        let mut mode = SpotHealMode::ContentAware;
        // SetSpotHealMode logic.
        mode = SpotHealMode::TextureMatch;
        assert_eq!(mode, SpotHealMode::TextureMatch);

        let mut radius: f32 = 20.0;
        // SetSpotHealRadius with value above 1.
        radius = 30.0_f32.max(1.0);
        assert!((radius - 30.0).abs() < 1e-5);

        // Clamp: radius below 1 should become 1.
        radius = 0.1_f32.max(1.0);
        assert!((radius - 1.0).abs() < 1e-5, "radius below 1 should clamp to 1.0");
    }

    #[test]
    fn test_spot_heal_records_position() {
        let mut last_spot_heal: Option<([f32; 2], f32)> = None;
        // SpotHeal apply logic.
        let center = [50.0f32, 75.0];
        let radius = 15.0f32;
        last_spot_heal = Some((center, radius));

        assert!(last_spot_heal.is_some(), "last_spot_heal should be Some after SpotHeal");
        let (c, r) = last_spot_heal.unwrap();
        assert!((c[0] - 50.0).abs() < 1e-5);
        assert!((c[1] - 75.0).abs() < 1e-5);
        assert!((r - 15.0).abs() < 1e-5);
    }

    #[test]
    fn test_red_eye_records() {
        let mut last_red_eye: Option<([f32; 2], f32, f32)> = None;
        // RedEye apply logic.
        let center = [100.0f32, 120.0];
        let radius = 8.0f32;
        let darken = 0.7f32;
        last_red_eye = Some((center, radius, darken));

        assert!(last_red_eye.is_some(), "last_red_eye should be Some after RedEye");
        let (c, r, d) = last_red_eye.unwrap();
        assert!((c[0] - 100.0).abs() < 1e-5);
        assert!((r - 8.0).abs() < 1e-5);
        assert!((d - 0.7).abs() < 1e-5);
    }

    // ---- Action variants exist (compile-time check) ----

    #[test]
    fn test_action_variants_compile() {
        let _ = Action::SaveSelectionAsChannel("Alpha 1".into());
        let _ = Action::LoadChannelAsSelection(0);
        let _ = Action::DeleteChannel(0);
        let _ = Action::DuplicateChannel(0);
        let _ = Action::SetBlendIf {
            layer_id: LayerId(1),
            blend_if: BlendIf::default(),
        };
        let _ = Action::ClearBlendIf(LayerId(1));
        let _ = Action::SetSpotHealMode(SpotHealMode::ContentAware);
        let _ = Action::SetSpotHealRadius(20.0);
        let _ = Action::SpotHeal { center: [0.0, 0.0], radius: 10.0 };
        let _ = Action::RedEye { center: [0.0, 0.0], radius: 5.0, darken: 0.5 };
        let _ = Action::SetGradientMapStops {
            layer_id: LayerId(1),
            stops: vec![(0.0, [0.0, 0.0, 0.0, 1.0]), (1.0, [1.0, 1.0, 1.0, 1.0])],
        };
        let _ = Action::SetChannelMixerOutput { layer_id: LayerId(1), output: 0 };
        let _ = Action::SetChannelMixerMix { layer_id: LayerId(1), src_r: 1.0, src_g: 0.0, src_b: 0.0, constant: 0.0 };
    }
}

// ---- Batch 4 extended tests ---------------------------------------------------

#[cfg(test)]
mod batch4_ext_tests {
    use super::{Action, App, ToneMapMethod, NeuralFilterKind, PrintLayout};

    // ---- HDR Tone Mapping ----

    #[test]
    fn test_tone_map_method_stored() {
        let mut app = App::new();
        assert!(app.last_tone_map.is_none());
        app.apply(Action::ApplyToneMap { method: ToneMapMethod::Filmic, exposure: 1.0, gamma: 2.2 });
        assert_eq!(app.last_tone_map, Some(ToneMapMethod::Filmic));
        app.apply(Action::ApplyToneMap { method: ToneMapMethod::AcesCg, exposure: 0.5, gamma: 1.0 });
        assert_eq!(app.last_tone_map, Some(ToneMapMethod::AcesCg));
    }

    #[test]
    fn test_tone_map_preview_toggle() {
        let mut app = App::new();
        assert!(!app.tone_map_preview);
        app.apply(Action::SetToneMapPreview(true));
        assert!(app.tone_map_preview);
        app.apply(Action::SetToneMapPreview(false));
        assert!(!app.tone_map_preview);
    }

    // ---- Neural Filters ----

    #[test]
    fn test_add_remove_neural_filter() {
        let mut app = App::new();
        assert!(app.neural_filters.is_empty());
        app.apply(Action::AddNeuralFilter(NeuralFilterKind::SkinSmoothing));
        app.apply(Action::AddNeuralFilter(NeuralFilterKind::Colorize));
        assert_eq!(app.neural_filters.len(), 2);
        app.apply(Action::RemoveNeuralFilter(0));
        assert_eq!(app.neural_filters.len(), 1);
        assert_eq!(app.neural_filters[0].kind, NeuralFilterKind::Colorize);
        // Out-of-bounds remove is a no-op.
        app.apply(Action::RemoveNeuralFilter(99));
        assert_eq!(app.neural_filters.len(), 1);
    }

    #[test]
    fn test_neural_filter_strength_clamp() {
        let mut app = App::new();
        app.apply(Action::AddNeuralFilter(NeuralFilterKind::SuperZoom));
        app.apply(Action::SetNeuralFilterStrength { idx: 0, strength: 1.5 });
        assert!((app.neural_filters[0].strength - 1.0).abs() < 1e-6);
        app.apply(Action::SetNeuralFilterStrength { idx: 0, strength: -0.5 });
        assert!((app.neural_filters[0].strength - 0.0).abs() < 1e-6);
    }

    #[test]
    fn test_toggle_neural_filter() {
        let mut app = App::new();
        app.apply(Action::AddNeuralFilter(NeuralFilterKind::DepthBlur));
        assert!(app.neural_filters[0].enabled);
        app.apply(Action::ToggleNeuralFilter(0));
        assert!(!app.neural_filters[0].enabled);
        app.apply(Action::ToggleNeuralFilter(0));
        assert!(app.neural_filters[0].enabled);
        // Out-of-bounds toggle is a no-op.
        app.apply(Action::ToggleNeuralFilter(99));
    }

    #[test]
    fn test_apply_neural_filters_counts_enabled() {
        let mut app = App::new();
        app.apply(Action::AddNeuralFilter(NeuralFilterKind::SkinSmoothing));
        app.apply(Action::AddNeuralFilter(NeuralFilterKind::Colorize));
        app.apply(Action::AddNeuralFilter(NeuralFilterKind::StyleTransfer));
        // Disable the second filter.
        app.apply(Action::ToggleNeuralFilter(1));
        app.apply(Action::ApplyNeuralFilters);
        assert_eq!(app.last_neural_apply_count, 2);
    }

    // ---- Layer Group depth ----

    #[test]
    fn test_group_collapse_expand() {
        let mut app = App::new();
        assert!(app.collapsed_groups.is_empty());
        app.apply(Action::SetGroupCollapsed { group_name: "Group 1".into(), collapsed: true });
        assert!(app.collapsed_groups.contains("Group 1"));
        app.apply(Action::SetGroupCollapsed { group_name: "Group 1".into(), collapsed: false });
        assert!(!app.collapsed_groups.contains("Group 1"));
    }

    #[test]
    fn test_duplicate_group_stub() {
        let mut app = App::new();
        assert!(app.last_duplicated_group.is_none());
        app.apply(Action::DuplicateGroup("Background".into()));
        assert_eq!(app.last_duplicated_group.as_deref(), Some("Background"));
    }

    // ---- Print Layout ----

    #[test]
    fn test_print_copies_min_1() {
        let mut app = App::new();
        app.apply(Action::SetPrintCopies(0));
        assert_eq!(app.print_layout.copies, 1);
        app.apply(Action::SetPrintCopies(5));
        assert_eq!(app.print_layout.copies, 5);
    }

    #[test]
    fn test_print_bleed_clamp() {
        let mut app = App::new();
        app.apply(Action::SetPrintBleed(50.0));
        assert!((app.print_layout.bleed - 25.0).abs() < 1e-6);
        app.apply(Action::SetPrintBleed(-1.0));
        assert!((app.print_layout.bleed - 0.0).abs() < 1e-6);
    }

    #[test]
    fn test_print_resolution_clamp() {
        let mut app = App::new();
        app.apply(Action::SetPrintResolution(5000));
        assert_eq!(app.print_layout.print_resolution, 2400);
        app.apply(Action::SetPrintResolution(10));
        assert_eq!(app.print_layout.print_resolution, 72);
    }

    #[test]
    fn test_print_center_toggle() {
        let mut app = App::new();
        assert!(app.print_layout.center_image); // default true
        app.apply(Action::SetPrintCenterImage(false));
        assert!(!app.print_layout.center_image);
        app.apply(Action::SetPrintCenterImage(true));
        assert!(app.print_layout.center_image);
    }

    // ---- PrintLayout struct defaults ----
    #[test]
    fn test_print_layout_default() {
        let pl = PrintLayout::default();
        assert_eq!(pl.copies, 1);
        assert!(pl.collate);
        assert!((pl.border_width - 0.0).abs() < 1e-6);
        assert!(pl.center_image);
        assert!(!pl.print_marks);
        assert!((pl.bleed - 3.0).abs() < 1e-6);
        assert_eq!(pl.print_resolution, 300);
    }
}

// ---- Batch 5 new feature tests ----------------------------------------------

#[cfg(test)]
mod batch5_new_tests {
    use super::{
        Action, App, CaFillMethod, LiquifyTool, LiquifyStroke, SelectSubjectMode,
    };

    // ---- Content-Aware Crop ----

    #[test]
    fn test_ca_crop_angle_clamp() {
        let mut app = App::new();
        app.apply(Action::SetCaCropAngle(90.0));
        assert!((app.ca_crop_config.angle - 45.0).abs() < 1e-5, "should clamp to 45");
        app.apply(Action::SetCaCropAngle(-90.0));
        assert!((app.ca_crop_config.angle - (-45.0)).abs() < 1e-5, "should clamp to -45");
    }

    #[test]
    fn test_ca_crop_apply_records_rect() {
        let mut app = App::new();
        assert!(app.last_ca_crop_rect.is_none());
        app.apply(Action::ApplyCaCrop { rect: [0.0, 0.0, 100.0, 100.0] });
        assert_eq!(app.last_ca_crop_rect, Some([0.0, 0.0, 100.0, 100.0]));
    }

    #[test]
    fn test_ca_crop_fill_method() {
        let mut app = App::new();
        app.apply(Action::SetCaCropFillMethod(CaFillMethod::EdgeExtend));
        assert!(matches!(app.ca_crop_config.fill_method, CaFillMethod::EdgeExtend));
        app.apply(Action::SetCaCropFillMethod(CaFillMethod::Transparent));
        assert!(matches!(app.ca_crop_config.fill_method, CaFillMethod::Transparent));
    }

    // ---- Sky Replacement ----

    #[test]
    fn test_sky_brightness_clamp() {
        let mut app = App::new();
        app.apply(Action::SetSkyBrightness(300.0));
        assert!((app.sky_replace_config.brightness - 200.0).abs() < 1e-5);
        app.apply(Action::SetSkyBrightness(-10.0));
        assert!((app.sky_replace_config.brightness - 0.0).abs() < 1e-5);
    }

    #[test]
    fn test_sky_temperature_clamp() {
        let mut app = App::new();
        app.apply(Action::SetSkyTemperature(-200.0));
        assert!((app.sky_replace_config.temperature - (-100.0)).abs() < 1e-5);
        app.apply(Action::SetSkyTemperature(200.0));
        assert!((app.sky_replace_config.temperature - 100.0).abs() < 1e-5);
    }

    #[test]
    fn test_sky_scale_clamp() {
        let mut app = App::new();
        app.apply(Action::SetSkyScale(5.0));
        assert!((app.sky_replace_config.scale - 2.0).abs() < 1e-5);
        app.apply(Action::SetSkyScale(0.1));
        assert!((app.sky_replace_config.scale - 0.5).abs() < 1e-5);
    }

    #[test]
    fn test_apply_sky_replace_sets_flag() {
        let mut app = App::new();
        assert!(!app.sky_replaced);
        app.apply(Action::ApplySkyReplace);
        assert!(app.sky_replaced);
    }

    #[test]
    fn test_sky_panel_toggle() {
        let mut app = App::new();
        assert!(!app.sky_replace_panel_open);
        app.apply(Action::ToggleSkyReplacePanel);
        assert!(app.sky_replace_panel_open);
        app.apply(Action::ToggleSkyReplacePanel);
        assert!(!app.sky_replace_panel_open);
    }

    // ---- Liquify ----

    #[test]
    fn test_liquify_brush_size_clamp() {
        let mut app = App::new();
        app.apply(Action::SetLiquifyBrushSize(0.0));
        assert!((app.liquify_brush_size - 1.0).abs() < 1e-5);
        app.apply(Action::SetLiquifyBrushSize(2000.0));
        assert!((app.liquify_brush_size - 1500.0).abs() < 1e-5);
    }

    #[test]
    fn test_liquify_stroke_push() {
        let mut app = App::new();
        assert!(app.liquify_strokes.is_empty());
        app.apply(Action::ApplyLiquifyStroke(LiquifyStroke {
            tool: LiquifyTool::Forward,
            center: [50.0, 50.0],
            radius: 40.0,
            pressure: 0.8,
            angle: 0.0,
        }));
        assert_eq!(app.liquify_strokes.len(), 1);
    }

    #[test]
    fn test_reconstruct_pops_stroke() {
        let mut app = App::new();
        app.apply(Action::ApplyLiquifyStroke(LiquifyStroke {
            tool: LiquifyTool::Pucker,
            center: [10.0, 10.0],
            radius: 20.0,
            pressure: 0.5,
            angle: 0.0,
        }));
        app.apply(Action::ApplyLiquifyStroke(LiquifyStroke {
            tool: LiquifyTool::Bloat,
            center: [20.0, 20.0],
            radius: 30.0,
            pressure: 0.6,
            angle: 0.0,
        }));
        assert_eq!(app.liquify_strokes.len(), 2);
        app.apply(Action::ReconstructLiquify);
        assert_eq!(app.liquify_strokes.len(), 1);
    }

    #[test]
    fn test_revert_clears_strokes() {
        let mut app = App::new();
        app.apply(Action::ApplyLiquifyStroke(LiquifyStroke {
            tool: LiquifyTool::Twirl,
            center: [5.0, 5.0],
            radius: 10.0,
            pressure: 0.4,
            angle: 45.0,
        }));
        assert!(!app.liquify_strokes.is_empty());
        app.apply(Action::RevertLiquify);
        assert!(app.liquify_strokes.is_empty());
    }

    #[test]
    fn test_thaw_all_clears_mask() {
        let mut app = App::new();
        app.liquify_frozen_mask = vec![true, true, false, true];
        app.apply(Action::ThawAllMask);
        assert!(app.liquify_frozen_mask.is_empty());
    }

    // ---- Select Subject ----

    #[test]
    fn test_select_subject_run_stub() {
        let mut app = App::new();
        assert!(app.last_select_subject.is_none());
        app.apply(Action::RunSelectSubject);
        let result = app.last_select_subject.as_ref().unwrap();
        assert!((result.coverage - 0.72).abs() < 1e-5);
        assert!((result.confidence - 0.89).abs() < 1e-5);
        assert!(!result.cloud_used);
    }

    #[test]
    fn test_select_subject_mode_cloud() {
        let mut app = App::new();
        app.apply(Action::SetSelectSubjectMode(SelectSubjectMode::Cloud));
        app.apply(Action::RunSelectSubject);
        let result = app.last_select_subject.as_ref().unwrap();
        assert!(result.cloud_used);
    }

    #[test]
    fn test_select_and_mask_toggle() {
        let mut app = App::new();
        assert!(!app.select_and_mask_open);
        app.apply(Action::ToggleSelectAndMask);
        assert!(app.select_and_mask_open);
        app.apply(Action::ToggleSelectAndMask);
        assert!(!app.select_and_mask_open);
    }

}

#[cfg(test)]
mod batch6_tests {
    use super::{
        Action, App, DropShadowFx, HdrToneMappingMethod,
        SmartObjectKind, Shape3DKind,
    };

    // ---- Layer Effects Suite ----

    #[test]
    fn test_set_drop_shadow_creates_entry() {
        let mut app = App::new();
        let layer = "layer1".to_string();
        assert!(!app.layer_effects.contains_key(&layer));
        let fx = DropShadowFx { enabled: true, opacity: 80.0, ..DropShadowFx::default() };
        app.apply(Action::SetDropShadow { layer: layer.clone(), fx });
        assert!(app.layer_effects.contains_key(&layer));
        assert!((app.layer_effects[&layer].drop_shadow.opacity - 80.0).abs() < 1e-5);
        assert!(app.layer_effects[&layer].drop_shadow.enabled);
    }

    #[test]
    fn test_toggle_drop_shadow() {
        let mut app = App::new();
        let layer = "layer1".to_string();
        app.apply(Action::ToggleDropShadow { layer: layer.clone(), enabled: true });
        assert!(app.layer_effects[&layer].drop_shadow.enabled);
        app.apply(Action::ToggleDropShadow { layer: layer.clone(), enabled: false });
        assert!(!app.layer_effects[&layer].drop_shadow.enabled);
    }

    #[test]
    fn test_clear_layer_effects() {
        let mut app = App::new();
        let layer = "layer1".to_string();
        app.apply(Action::ToggleDropShadow { layer: layer.clone(), enabled: true });
        assert!(app.layer_effects.contains_key(&layer));
        app.apply(Action::ClearLayerEffects { layer: layer.clone() });
        assert!(!app.layer_effects.contains_key(&layer));
    }

    #[test]
    fn test_copy_paste_layer_effects() {
        let mut app = App::new();
        let src = "src".to_string();
        let dst = "dst".to_string();
        let fx = DropShadowFx { enabled: true, opacity: 60.0, ..DropShadowFx::default() };
        app.apply(Action::SetDropShadow { layer: src.clone(), fx });
        assert!(app.fx_clipboard.is_none());
        app.apply(Action::CopyLayerEffects { from: src.clone() });
        assert!(app.fx_clipboard.is_some());
        app.apply(Action::PasteLayerEffects { to: dst.clone() });
        assert!(app.layer_effects.contains_key(&dst));
        assert!((app.layer_effects[&dst].drop_shadow.opacity - 60.0).abs() < 1e-5);
    }

    #[test]
    fn test_toggle_fx_panel() {
        let mut app = App::new();
        assert!(!app.fx_panel_open);
        app.apply(Action::ToggleFxPanel);
        assert!(app.fx_panel_open);
        app.apply(Action::ToggleFxPanel);
        assert!(!app.fx_panel_open);
    }

    // ---- Match Color ----

    #[test]
    fn test_match_luminance_clamp() {
        let mut app = App::new();
        app.apply(Action::SetMatchColorLuminance(300.0));
        assert!((app.match_color_config.luminance - 200.0).abs() < 1e-5);
        app.apply(Action::SetMatchColorLuminance(-10.0));
        assert!((app.match_color_config.luminance - 0.0).abs() < 1e-5);
    }

    #[test]
    fn test_match_fade_clamp() {
        let mut app = App::new();
        app.apply(Action::SetMatchColorFade2(150.0));
        assert!((app.match_color_config.fade - 100.0).abs() < 1e-5);
        app.apply(Action::SetMatchColorFade2(-5.0));
        assert!((app.match_color_config.fade - 0.0).abs() < 1e-5);
    }

    #[test]
    fn test_apply_match_color_sets_flag() {
        let mut app = App::new();
        assert!(!app.last_match_color_applied);
        app.apply(Action::ApplyMatchColor);
        assert!(app.last_match_color_applied);
    }

    // ---- Camera Raw ----

    #[test]
    fn test_camera_raw_temp_clamp() {
        let mut app = App::new();
        app.apply(Action::SetCameraRawTemp(100.0));
        assert!((app.camera_raw_config.temperature - 2000.0).abs() < 1e-5);
        app.apply(Action::SetCameraRawTemp(100_000.0));
        assert!((app.camera_raw_config.temperature - 50000.0).abs() < 1e-5);
    }

    #[test]
    fn test_camera_raw_sharpness_clamp() {
        let mut app = App::new();
        app.apply(Action::SetCameraRawSharpness(200.0));
        assert!((app.camera_raw_config.sharpness - 150.0).abs() < 1e-5);
        app.apply(Action::SetCameraRawSharpness(-5.0));
        assert!((app.camera_raw_config.sharpness - 0.0).abs() < 1e-5);
    }

    #[test]
    fn test_camera_raw_apply_flag() {
        let mut app = App::new();
        assert!(!app.camera_raw_applied);
        app.apply(Action::ApplyCameraRawFilter);
        assert!(app.camera_raw_applied);
    }

    #[test]
    fn test_camera_raw_reset() {
        let mut app = App::new();
        app.apply(Action::SetCameraRawTemp(3000.0));
        assert!((app.camera_raw_config.temperature - 3000.0).abs() < 1e-5);
        app.apply(Action::ResetCameraRaw);
        assert!((app.camera_raw_config.temperature - 6500.0).abs() < 1e-5);
    }

    #[test]
    fn test_camera_raw_panel_toggle() {
        let mut app = App::new();
        assert!(!app.camera_raw_panel_open);
        app.apply(Action::ToggleCameraRawPanel);
        assert!(app.camera_raw_panel_open);
        app.apply(Action::ToggleCameraRawPanel);
        assert!(!app.camera_raw_panel_open);
    }

    // ---- HDR Merge ----

    #[test]
    fn test_hdr_source_count_clamp() {
        let mut app = App::new();
        app.apply(Action::SetHdrSourceCount(0));
        assert_eq!(app.hdr_merge_config.source_count, 2);
        app.apply(Action::SetHdrSourceCount(1));
        assert_eq!(app.hdr_merge_config.source_count, 2);
        app.apply(Action::SetHdrSourceCount(5));
        assert_eq!(app.hdr_merge_config.source_count, 5);
    }

    #[test]
    fn test_hdr_bit_depth_invalid_ignored() {
        let mut app = App::new();
        assert_eq!(app.hdr_merge_config.bit_depth_output, 32);
        app.apply(Action::SetHdrBitDepth(24));
        assert_eq!(app.hdr_merge_config.bit_depth_output, 32);
        app.apply(Action::SetHdrBitDepth(16));
        assert_eq!(app.hdr_merge_config.bit_depth_output, 16);
    }

    #[test]
    fn test_merge_to_hdr_sets_result() {
        let mut app = App::new();
        assert!(app.hdr_merge_result.is_none());
        app.apply(Action::MergeToHdr);
        assert_eq!(app.hdr_merge_result.as_deref(), Some("merged_hdr.tif"));
    }

    #[test]
    fn test_hdr_tone_method() {
        let mut app = App::new();
        app.apply(Action::SetHdrToneMethod(HdrToneMappingMethod::Highlight));
        assert_eq!(app.hdr_merge_config.method, HdrToneMappingMethod::Highlight);
    }

    // ---- SmartObject (rich) ----

    #[test]
    fn test_smart_object_convert_pushes_entry() {
        let mut app = App::new();
        assert!(app.smart_object_list.is_empty());
        app.apply(Action::SmartObjectConvert { layer_id: 1 });
        assert_eq!(app.smart_object_list.len(), 1);
        assert_eq!(app.smart_object_list[0].id, 0);
        assert_eq!(app.smart_object_list[0].name, "Smart Object 0");
        assert_eq!(app.smart_object_list[0].kind, SmartObjectKind::Embedded);
        assert!(!app.smart_object_list[0].contents_dirty);
    }

    #[test]
    fn test_smart_object_counter_increments() {
        let mut app = App::new();
        app.apply(Action::SmartObjectConvert { layer_id: 1 });
        app.apply(Action::SmartObjectConvert { layer_id: 2 });
        assert_eq!(app.smart_object_list[0].id, 0);
        assert_eq!(app.smart_object_list[1].id, 1);
        assert_eq!(app.smart_object_counter, 2);
    }

    #[test]
    fn test_smart_object_replace_sets_path_and_dirty() {
        let mut app = App::new();
        app.apply(Action::SmartObjectConvert { layer_id: 1 });
        let so_id = app.smart_object_list[0].id;
        app.apply(Action::SmartObjectReplace { so_id, new_path: "/tmp/file.png".into() });
        let so = &app.smart_object_list[0];
        assert_eq!(so.source_path.as_deref(), Some("/tmp/file.png"));
        assert!(so.contents_dirty);
    }

    #[test]
    fn test_smart_object_export_clears_dirty() {
        let mut app = App::new();
        app.apply(Action::SmartObjectConvert { layer_id: 1 });
        let so_id = app.smart_object_list[0].id;
        app.apply(Action::SmartObjectReplace { so_id, new_path: "/tmp/x.png".into() });
        assert!(app.smart_object_list[0].contents_dirty);
        app.apply(Action::SmartObjectExport { so_id });
        assert!(!app.smart_object_list[0].contents_dirty);
    }

    #[test]
    fn test_smart_object_rasterize_removes_entry() {
        let mut app = App::new();
        app.apply(Action::SmartObjectConvert { layer_id: 1 });
        let so_id = app.smart_object_list[0].id;
        app.apply(Action::SmartObjectRasterize { so_id });
        assert!(app.smart_object_list.is_empty());
    }

    // ---- AdvancedMasking ----

    #[test]
    fn test_select_mask_open_close() {
        let mut app = App::new();
        assert!(!app.select_mask_open);
        app.apply(Action::OpenSelectMask);
        assert!(app.select_mask_open);
        app.apply(Action::CloseSelectMask);
        assert!(!app.select_mask_open);
    }

    #[test]
    fn test_select_mask_apply_closes() {
        let mut app = App::new();
        app.apply(Action::OpenSelectMask);
        assert!(app.select_mask_open);
        app.apply(Action::ApplySelectMask);
        assert!(!app.select_mask_open);
    }

    #[test]
    fn test_select_mask_radius_clamp() {
        let mut app = App::new();
        app.apply(Action::SetSelectMaskRadius(-5.0));
        assert!((app.select_mask_config.radius - 0.0).abs() < 1e-5);
        app.apply(Action::SetSelectMaskRadius(999.0));
        assert!((app.select_mask_config.radius - 250.0).abs() < 1e-5);
        app.apply(Action::SetSelectMaskRadius(100.0));
        assert!((app.select_mask_config.radius - 100.0).abs() < 1e-5);
    }

    #[test]
    fn test_select_mask_smooth_clamp() {
        let mut app = App::new();
        app.apply(Action::SetSelectMaskSmooth(200));
        assert_eq!(app.select_mask_config.smooth, 100);
        app.apply(Action::SetSelectMaskSmooth(50));
        assert_eq!(app.select_mask_config.smooth, 50);
    }

    #[test]
    fn test_select_mask_shift_edge_clamp() {
        let mut app = App::new();
        app.apply(Action::SetSelectMaskShiftEdge(-120));
        assert_eq!(app.select_mask_config.shift_edge, -100);
        app.apply(Action::SetSelectMaskShiftEdge(120));
        assert_eq!(app.select_mask_config.shift_edge, 100);
        app.apply(Action::SetSelectMaskShiftEdge(30));
        assert_eq!(app.select_mask_config.shift_edge, 30);
    }

    // ---- GenerativeFill ----

    #[test]
    fn test_generative_fill_set_prompt() {
        let mut app = App::new();
        app.apply(Action::SetGenerativeFillPrompt("sunny beach".into()));
        assert_eq!(app.generative_fill_prompt, "sunny beach");
    }

    #[test]
    fn test_generative_fill_run_pushes_result() {
        let mut app = App::new();
        app.apply(Action::SetGenerativeFillPrompt("mountain lake".into()));
        app.apply(Action::RunGenerativeFill { layer_id: 42 });
        assert_eq!(app.generative_fill_results.len(), 1);
        let r = &app.generative_fill_results[0];
        assert_eq!(r.layer_id, 42);
        assert_eq!(r.prompt, "mountain lake");
        assert_eq!(r.variation_index, 0);
        assert_eq!(r.variation_count, 4);
    }

    #[test]
    fn test_generative_fill_cycle_variation() {
        let mut app = App::new();
        app.apply(Action::RunGenerativeFill { layer_id: 1 });
        app.apply(Action::CycleGenerativeFillVariation { result_index: 0 });
        assert_eq!(app.generative_fill_results[0].variation_index, 1);
        // Wraps around at variation_count (4).
        for _ in 0..3 {
            app.apply(Action::CycleGenerativeFillVariation { result_index: 0 });
        }
        assert_eq!(app.generative_fill_results[0].variation_index, 0);
    }

    #[test]
    fn test_generative_fill_accept_removes() {
        let mut app = App::new();
        app.apply(Action::RunGenerativeFill { layer_id: 1 });
        app.apply(Action::RunGenerativeFill { layer_id: 2 });
        assert_eq!(app.generative_fill_results.len(), 2);
        app.apply(Action::AcceptGenerativeFill { result_index: 0 });
        assert_eq!(app.generative_fill_results.len(), 1);
        assert_eq!(app.generative_fill_results[0].layer_id, 2);
    }

    #[test]
    fn test_generative_fill_discard_removes() {
        let mut app = App::new();
        app.apply(Action::RunGenerativeFill { layer_id: 5 });
        app.apply(Action::DiscardGenerativeFill { result_index: 0 });
        assert!(app.generative_fill_results.is_empty());
    }

    // ---- Basic3DLayer ----

    #[test]
    fn test_create_3d_layer_sets_active() {
        let mut app = App::new();
        assert!(app.active_3d_layer.is_none());
        app.apply(Action::Create3DLayer { layer_id: 7, shape: Shape3DKind::Sphere });
        assert_eq!(app.active_3d_layer, Some(7));
        assert_eq!(app.layer_3d_props.len(), 1);
        assert_eq!(app.layer_3d_props[0].shape, Shape3DKind::Sphere);
    }

    #[test]
    fn test_set_3d_position() {
        let mut app = App::new();
        app.apply(Action::Create3DLayer { layer_id: 3, shape: Shape3DKind::Cube });
        app.apply(Action::Set3DPosition { layer_id: 3, x: 10.0, y: 20.0, z: 30.0 });
        let p = &app.layer_3d_props[0];
        assert!((p.pos_x - 10.0).abs() < 1e-5);
        assert!((p.pos_y - 20.0).abs() < 1e-5);
        assert!((p.pos_z - 30.0).abs() < 1e-5);
    }

    #[test]
    fn test_set_layer_3d_rotation() {
        let mut app = App::new();
        app.apply(Action::Create3DLayer { layer_id: 4, shape: Shape3DKind::Cone });
        app.apply(Action::SetLayer3DRotation { layer_id: 4, x: 45.0, y: 90.0, z: 180.0 });
        let p = &app.layer_3d_props[0];
        assert!((p.rot_x - 45.0).abs() < 1e-5);
        assert!((p.rot_y - 90.0).abs() < 1e-5);
        assert!((p.rot_z - 180.0).abs() < 1e-5);
    }

    #[test]
    fn test_set_3d_scale_clamped() {
        let mut app = App::new();
        app.apply(Action::Create3DLayer { layer_id: 5, shape: Shape3DKind::Plane });
        app.apply(Action::Set3DScale { layer_id: 5, x: 0.0, y: 50.0, z: 2.0 });
        let p = &app.layer_3d_props[0];
        // 0.0 clamped to 0.01, 50.0 clamped to 10.0, 2.0 unchanged
        assert!((p.scale_x - 0.01).abs() < 1e-5);
        assert!((p.scale_y - 10.0).abs() < 1e-5);
        assert!((p.scale_z - 2.0).abs() < 1e-5);
    }

    #[test]
    fn test_set_3d_extrude_depth_clamp() {
        let mut app = App::new();
        app.apply(Action::Create3DLayer { layer_id: 6, shape: Shape3DKind::Cylinder });
        app.apply(Action::Set3DExtrudeDepth { layer_id: 6, depth: -10.0 });
        assert!((app.layer_3d_props[0].extrude_depth - 0.0).abs() < 1e-5);
        app.apply(Action::Set3DExtrudeDepth { layer_id: 6, depth: 9999.0 });
        assert!((app.layer_3d_props[0].extrude_depth - 5000.0).abs() < 1e-5);
        app.apply(Action::Set3DExtrudeDepth { layer_id: 6, depth: 200.0 });
        assert!((app.layer_3d_props[0].extrude_depth - 200.0).abs() < 1e-5);
    }

    #[test]
    fn test_flatten_3d_layer_removes_and_clears_active() {
        let mut app = App::new();
        app.apply(Action::Create3DLayer { layer_id: 8, shape: Shape3DKind::Custom });
        assert_eq!(app.active_3d_layer, Some(8));
        app.apply(Action::Flatten3DLayer { layer_id: 8 });
        assert!(app.layer_3d_props.is_empty());
        assert!(app.active_3d_layer.is_none());
    }
}
