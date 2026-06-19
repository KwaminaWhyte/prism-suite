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

/// In-progress Text-tool edit: which raster layer holds the glyphs, where it was
/// placed (doc px), and the string typed so far. The host re-rasterizes the layer
/// from `string` on every keystroke (see `App::text_input`).
struct TextEdit {
    layer: LayerId,
    origin: [f32; 2],
    string: String,
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

            Action::SetBrushColor(c) => {
                self.brush.color = c;
                // Re-rasterize active in-progress text run.
                if let Some(edit) = self.text_edit.as_ref() {
                    let (layer, origin, string) = (edit.layer, edit.origin, edit.string.clone());
                    self.host.update_text_layer(
                        layer, &string, self.text_size, c, origin,
                        prism_io::text::TextAlign::Left, None,
                    );
                }
            }
            Action::SetBrushSize(s) => self.brush.size = s.clamp(1.0, 400.0),
            Action::SetBrushHardness(h) => self.brush.hardness = h.clamp(0.0, 0.99),
            Action::SetBrushOpacity(o) => self.brush.opacity = o.clamp(0.0, 1.0),

            Action::SetFillTolerance(t) => self.fill_tolerance = t.clamp(0.0, 1.0),
            Action::ToggleFillContiguous => self.fill_contiguous = !self.fill_contiguous,
            Action::ToggleGradientDither => self.gradient_dither = !self.gradient_dither,
            Action::SetTextSize(s) => self.text_size = s.clamp(6.0, 400.0),

            Action::ToggleLayerVisible(id) => {
                if let Some(l) = self.doc.layers.get_mut(id) {
                    l.visible = !l.visible;
                    self.sync_host_order_dirty();
                }
            }
            Action::SelectLayer(id) => {
                if self.doc.layers.get(id).is_some() {
                    self.doc.active_layer = Some(id);
                    // Selection is pure UI state — no re-composite needed.
                }
            }
            Action::SetLayerOpacity(id, o) => {
                if let Some(l) = self.doc.layers.get_mut(id) {
                    l.opacity = o.clamp(0.0, 1.0);
                    self.sync_host_order_dirty();
                }
            }
            Action::MoveLayer { id, up } => {
                let layers = &mut self.doc.layers.layers;
                if let Some(i) = layers.iter().position(|l| l.id == id) {
                    // Vec front = bottom of stack, so "up" (toward top) is +1.
                    let j = if up { i + 1 } else { i.wrapping_sub(1) };
                    if up && j < layers.len() {
                        layers.swap(i, j);
                        self.sync_host_order_dirty();
                    } else if !up && i > 0 {
                        layers.swap(i, i - 1);
                        self.sync_host_order_dirty();
                    }
                }
            }
            Action::DeleteLayer(id) => {
                let layers = &mut self.doc.layers.layers;
                // Keep at least one layer alive.
                if layers.len() > 1 {
                    if let Some(i) = layers.iter().position(|l| l.id == id) {
                        layers.remove(i);
                        if self.doc.active_layer == Some(id) {
                            self.doc.active_layer = self.doc.layers.layers.last().map(|l| l.id);
                        }
                        self.sync_host_order_dirty();
                    }
                }
            }

            Action::SetLayerBlend(id, mode) => {
                if let Some(l) = self.doc.layers.get_mut(id) {
                    l.blend = mode;
                    self.sync_host_order_dirty();
                }
            }

            Action::AddSwatch(c) => {
                if !self.swatches.contains(&c) {
                    self.swatches.push(c);
                }
            }

            Action::ZoomBy(factor) => {
                self.view.zoom_to(factor, glam::Vec2::ZERO);
            }
            Action::ResetView => {
                self.view = ViewTransform::default();
            }

            Action::SetMarquee { rect, ellipse } => {
                // Degenerate (zero-area) marquee → treat as a clear so a stray
                // click doesn't lock painting behind an empty mask.
                if rect[2] <= 0.5 || rect[3] <= 0.5 {
                    self.host.clear_selection();
                } else {
                    self.host.set_marquee(rect, ellipse);
                }
                self.bump_selection();
            }
            Action::ClearSelection => {
                self.host.clear_selection();
                self.bump_selection();
            }

            Action::OpenImage => {
                let Some(path) = rfd::FileDialog::new()
                    .add_filter("Images", prism_io::SUPPORTED_EXTENSIONS)
                    .pick_file()
                else {
                    return;
                };
                if let Some(doc) = self.host.open_image(&path) {
                    self.doc = doc;
                    // Reset transient interaction state for the new document.
                    self.stroke_last = None;
                    self.stroke_residual = 0.0;
                    self.sel_drag_start = None;
                    self.lasso_points.clear();
                    self.sel_base.clear();
                    self.sel_mode = CombineMode::Replace;
                    self.xform_drag_start = None;
                    self.xform_translate = [0.0, 0.0];
                    self.xform_scale = 1.0;
                    // Persist prefs: update last_document + MRU list.
                    self.recent_files.retain(|p| p != &path);
                    self.recent_files.insert(0, path.clone());
                    self.recent_files.truncate(10);
                    self.prefs.last_document = Some(path.clone());
                    self.prefs.recent_files = self.recent_files.clone();
                    self.prefs.save();
                }
            }
            Action::ExportImage => {
                let Some(path) = rfd::FileDialog::new()
                    .add_filter("Image", &["png", "jpg", "jpeg", "webp", "tif", "tiff", "bmp"])
                    .set_file_name("export.png")
                    .save_file()
                else {
                    return;
                };
                if let Err(e) = self.host.export_image(&path) {
                    log::error!("export failed: {e}");
                }
            }

            Action::OpenEXR => {
                let Some(path) = rfd::FileDialog::new()
                    .add_filter("OpenEXR", &["exr"])
                    .pick_file()
                else {
                    return;
                };
                if let Some(doc) = self.host.open_exr(&path) {
                    self.doc = doc;
                    self.stroke_last = None;
                    self.stroke_residual = 0.0;
                    self.sel_drag_start = None;
                    self.lasso_points.clear();
                    self.sel_base.clear();
                    self.sel_mode = CombineMode::Replace;
                    self.xform_drag_start = None;
                    self.xform_translate = [0.0, 0.0];
                    self.xform_scale = 1.0;
                }
            }

            Action::ExportEXR => {
                let Some(path) = rfd::FileDialog::new()
                    .add_filter("OpenEXR", &["exr"])
                    .set_file_name("export.exr")
                    .save_file()
                else {
                    return;
                };
                if let Err(e) = self.host.export_exr(&path) {
                    log::error!("EXR export failed: {e}");
                }
            }

            Action::AddSlice(rect) => {
                let id = self.next_slice_id;
                self.next_slice_id += 1;
                self.slices.push(Slice {
                    id,
                    rect,
                    name: format!("slice_{id:02}"),
                });
            }

            Action::DeleteSlice(id) => {
                self.slices.retain(|s| s.id != id);
            }

            Action::ExportSlices(dir) => {
                let Some(flat) = self.host.read_composite_f32() else {
                    return;
                };
                let (dw, dh) = (self.host.doc_w, self.host.doc_h);
                for slice in &self.slices {
                    let [sx, sy, sw, sh] = slice.rect;
                    let x0 = (sx.round() as i32).clamp(0, dw as i32) as u32;
                    let y0 = (sy.round() as i32).clamp(0, dh as i32) as u32;
                    let x1 = ((sx + sw).round() as i32).clamp(0, dw as i32) as u32;
                    let y1 = ((sy + sh).round() as i32).clamp(0, dh as i32) as u32;
                    let (cw, ch) = (x1.saturating_sub(x0), y1.saturating_sub(y0));
                    if cw == 0 || ch == 0 {
                        continue;
                    }
                    let mut rgba8 = Vec::with_capacity((cw * ch * 4) as usize);
                    for row in y0..y1 {
                        for col in x0..x1 {
                            let i = ((row * dw + col) * 4) as usize;
                            let (r, g, b, a) = (flat[i], flat[i+1], flat[i+2], flat[i+3]);
                            let inv = if a > 1e-5 { 1.0 / a } else { 0.0 };
                            let to8 = |v: f32| ((v * inv).clamp(0.0, 1.0) * 255.0).round() as u8;
                            rgba8.push(to8(r));
                            rgba8.push(to8(g));
                            rgba8.push(to8(b));
                            rgba8.push((a.clamp(0.0, 1.0) * 255.0).round() as u8);
                        }
                    }
                    let out = dir.join(format!("{}.png", slice.name));
                    if let Err(e) = prism_io::export::save_rgba8(&out, &rgba8, cw, ch) {
                        log::error!("slice export {}: {e}", slice.name);
                    }
                }
            }

            Action::ApplyFilter(filter) => {
                let Some(layer) = self.paint_target() else {
                    return;
                };
                match filter {
                    Filter::GaussianBlur { radius } => {
                        self.host.apply_filter(layer, 1, radius, 0.0);
                    }
                    Filter::BoxBlur { radius } => {
                        self.host.apply_filter(layer, 5, radius, 0.0);
                    }
                    Filter::Sharpen { amount } => {
                        self.host.apply_filter(layer, 2, 0.0, amount);
                    }
                    Filter::Posterize { levels } => {
                        self.host.apply_posterize(layer, levels);
                    }
                    Filter::Threshold { level } => {
                        self.host.apply_threshold(layer, level);
                    }
                    Filter::FindEdges { width } => {
                        self.host.apply_stylize(layer, 13, 0.0, width, [0.0; 2]);
                    }
                    Filter::Emboss { amount, width } => {
                        // Emboss reads a light direction; a fixed 45° diagonal
                        // matches the egui app's default emboss light.
                        self.host.apply_stylize(layer, 14, amount, width, [1.0, 1.0]);
                    }
                    Filter::AddNoise { amount } => {
                        self.host.apply_noise(layer, amount, true, true, 1.0);
                    }
                    Filter::UnsharpMask { radius, amount, .. } => {
                        // Blur first, then sharpen: a two-pass proxy for USM.
                        self.host.apply_filter(layer, 1, radius, 0.0);
                        self.host.apply_filter(layer, 2, radius, amount);
                    }
                    Filter::RadialBlur { amount } => {
                        // Proxy: a gentle Gaussian blur scaled from the spin amount.
                        let r = (amount / 30.0).max(0.5);
                        self.host.apply_filter(layer, 1, r, 0.0);
                    }
                    Filter::LensCorrection { barrel, pincushion, vignette } => {
                        self.apply(Action::ApplyLensCorrection { barrel, pincushion, vignette });
                    }
                    // Batch 5: CPU raster filters (read → pure fn → upload),
                    // mirroring the lens-correction / content-aware path.
                    Filter::MotionBlur { angle, distance } => {
                        let (dw, dh) = (self.host.doc_w, self.host.doc_h);
                        if let Some(px) = self.host.read_layer_f32(layer) {
                            let r = crate::filters::motion_blur(&px, dw, dh, angle, distance);
                            self.host.upload_layer_f32(layer, &r);
                            self.status_message = Some("Motion Blur applied".to_string());
                        }
                    }
                    Filter::Twirl { angle, radius } => {
                        let (dw, dh) = (self.host.doc_w, self.host.doc_h);
                        if let Some(px) = self.host.read_layer_f32(layer) {
                            let r = crate::filters::twirl(&px, dw, dh, angle, radius);
                            self.host.upload_layer_f32(layer, &r);
                            self.status_message = Some("Twirl applied".to_string());
                        }
                    }
                    Filter::Pinch { amount, radius } => {
                        let (dw, dh) = (self.host.doc_w, self.host.doc_h);
                        if let Some(px) = self.host.read_layer_f32(layer) {
                            let r = crate::filters::pinch(&px, dw, dh, amount, radius);
                            self.host.upload_layer_f32(layer, &r);
                            self.status_message = Some("Pinch applied".to_string());
                        }
                    }
                    Filter::Solarize { threshold } => {
                        if let Some(px) = self.host.read_layer_f32(layer) {
                            let r = crate::filters::solarize(&px, threshold);
                            self.host.upload_layer_f32(layer, &r);
                            self.status_message = Some("Solarize applied".to_string());
                        }
                    }
                    Filter::GlowingEdges { width, intensity } => {
                        let (dw, dh) = (self.host.doc_w, self.host.doc_h);
                        if let Some(px) = self.host.read_layer_f32(layer) {
                            let r = crate::filters::glowing_edges(&px, dw, dh, width, intensity);
                            self.host.upload_layer_f32(layer, &r);
                            self.status_message = Some("Glowing Edges applied".to_string());
                        }
                    }
                }
            }

            Action::AddMask(id) => {
                if self.doc.layers.get(id).is_some() {
                    // White (reveal-all) raster mask, then drop into mask-edit mode
                    // so the next stroke targets the mask (the egui app's flow).
                    self.host.set_mask(id, None);
                    self.masked_layers.insert(id);
                    self.doc.active_layer = Some(id);
                    self.edit_mask = true;
                }
            }
            Action::DeleteMask(id) => {
                if self.masked_layers.remove(&id) {
                    self.host.delete_mask(id);
                    // Leaving the masked layer's mask exits mask-edit mode.
                    if self.doc.active_layer == Some(id) {
                        self.edit_mask = false;
                    }
                }
            }
            Action::ToggleEditMask => {
                self.edit_mask = !self.edit_mask;
            }

            Action::AddAdjustment(kind) => {
                let adj = kind.to_adjustment();
                let id = self.doc.layers.add_adjustment(adj);
                self.doc.active_layer = Some(id);
                // An adjustment layer needs a GPU slot even with no painted pixels.
                self.host.ensure_layer(id);
                // Upload any LUT (Curves/GradientMap/ColorBalance) then re-order
                // so the compositor encodes + applies the new adjustment.
                self.host.sync_adjustment_luts(&self.doc);
                self.sync_host_order_dirty();
            }
            Action::SetAdjustment(id, adj) => {
                if let Some(l) = self.doc.layers.get_mut(id) {
                    if matches!(l.kind, LayerKind::Adjustment(_)) {
                        l.kind = LayerKind::Adjustment(adj);
                        self.host.sync_adjustment_luts(&self.doc);
                        self.sync_host_order_dirty();
                    }
                }
            }

            Action::Undo => self.host.undo(),
            Action::Redo => self.host.redo(),

            Action::ToggleChannel(ch) => {
                if ch < 4 {
                    self.channel_visibility[ch] = !self.channel_visibility[ch];
                    self.host.set_channel_mask(self.channel_visibility);
                }
            }
            Action::SetAdjustmentCurve(id, points) => {
                if let Some(l) = self.doc.layers.get_mut(id) {
                    if let LayerKind::Adjustment(Adjustment::Curves(ref mut cp)) = l.kind {
                        cp.rgb = points;
                        self.host.sync_adjustment_luts(&self.doc);
                        self.sync_host_order_dirty();
                    }
                }
            }
            Action::SelectAll => {
                let rect = [0.0, 0.0, self.doc.size.width as f32, self.doc.size.height as f32];
                self.apply(Action::SetMarquee { rect, ellipse: false });
            }
            Action::FlattenLayers => {
                // Placeholder: merging all layers into one is a future wave. Just
                // mark dirty so the host recomposites (no-op effect for now).
                self.host.mark_dirty();
            }

            Action::SwapColors => {
                std::mem::swap(&mut self.brush.color, &mut self.bg_color);
            }
            Action::ResetColors => {
                self.brush.color = [0.0, 0.0, 0.0, 1.0];
                self.bg_color = [1.0, 1.0, 1.0, 1.0];
            }

            Action::InvertSelection => {
                let mask = self.host.read_selection_or_empty();
                let inv: Vec<f32> = mask.iter().map(|&v| 1.0 - v).collect();
                self.host.upload_selection_mask(&inv);
                self.bump_selection();
            }

            Action::JumpHistory(idx) => {
                log::info!("JumpHistory({idx}) — stub: engine history is stack-based");
                if idx + 1 < self.history_labels.len() {
                    self.history_labels.truncate(idx + 1);
                }
            }

            Action::ConvertToSmartObject(id) => {
                let path = std::path::PathBuf::from(format!("/tmp/pigment-so-{}.png", id.0));
                self.smart_objects.insert(id, path);
            }
            Action::EditSmartObject(id) => {
                if let Some(path) = self.smart_objects.get(&id) {
                    log::info!("would open {:?}", path);
                }
            }
            Action::RasterizeSmartObject(id) => {
                self.smart_objects.remove(&id);
            }

            Action::PenAddNode(pos) => {
                self.pen_path.push(PenNode { pos, ctrl_in: pos, ctrl_out: pos });
            }
            Action::PenMoveHandle { idx, is_out, delta } => {
                if let Some(node) = self.pen_path.get_mut(idx) {
                    if is_out {
                        node.ctrl_out.0 += delta.0;
                        node.ctrl_out.1 += delta.1;
                    } else {
                        node.ctrl_in.0 += delta.0;
                        node.ctrl_in.1 += delta.1;
                    }
                }
            }
            Action::PenClose => {
                if self.pen_path.len() >= 2 {
                    if let Some(layer) = self.paint_target() {
                        let dabs = rasterize_pen_path(&self.pen_path, &self.brush);
                        if !dabs.is_empty() {
                            self.host.paint_dabs(layer, &dabs, false, true, false);
                        }
                    }
                }
                self.pen_path.clear();
                self.pen_closed = true;
            }

            Action::BeginCurveDrag(lid, idx) => {
                self.dragging_curve_point = Some((lid, idx));
            }
            Action::MoveCurvePoint(lid, idx, (nx, ny)) => {
                let nx = nx.clamp(0.0, 1.0);
                let ny = ny.clamp(0.0, 1.0);
                if let Some(l) = self.doc.layers.get_mut(lid) {
                    if let LayerKind::Adjustment(Adjustment::Curves(ref mut cp)) = l.kind {
                        if let Some(pt) = cp.rgb.get_mut(idx) {
                            *pt = (nx, ny);
                            self.host.sync_adjustment_luts(&self.doc);
                            self.sync_host_order_dirty();
                        }
                    }
                }
            }
            Action::EndCurveDrag => {
                self.dragging_curve_point = None;
                self.host.sync_adjustment_luts(&self.doc);
            }
            Action::RemoveCurvePoint(lid, idx) => {
                if let Some(l) = self.doc.layers.get_mut(lid) {
                    if let LayerKind::Adjustment(Adjustment::Curves(ref mut cp)) = l.kind {
                        if cp.rgb.len() > 2 {
                            cp.rgb.remove(idx);
                            self.host.sync_adjustment_luts(&self.doc);
                            self.sync_host_order_dirty();
                        }
                    }
                }
            }
            Action::HoverCurvePoint(pt) => {
                self.hovered_curve_point = pt;
            }
            Action::SetCurvesCanvasBounds(b) => {
                self.curves_canvas_bounds = Some(b);
            }

            // --- Wave 11: Layer styles ---
            Action::SetLayerStyle(id, style) => {
                self.layer_styles.insert(id, style);
                self.host.mark_dirty();
            }
            Action::ClearLayerStyle(id) => {
                self.layer_styles.remove(&id);
                self.host.mark_dirty();
            }
            Action::OpenStylePanel(id) => {
                self.style_panel_open = true;
                self.style_panel_layer = Some(id);
            }
            Action::CloseStylePanel => {
                self.style_panel_open = false;
            }

            // --- Wave 11: Clipping masks ---
            Action::ToggleClippingMask(id) => {
                if self.clipping_masks.contains(&id) {
                    self.clipping_masks.remove(&id);
                } else {
                    self.clipping_masks.insert(id);
                }
                self.host.mark_dirty();
            }

            // --- Wave 11: HSV color picker ---
            Action::SetFgHue(h) => {
                self.fg_hue = h.rem_euclid(360.0);
                self.brush.color = hsv_to_rgb(self.fg_hue, self.fg_saturation, self.fg_value);
            }
            Action::SetFgSV(s, v) => {
                self.fg_saturation = s.clamp(0.0, 1.0);
                self.fg_value = v.clamp(0.0, 1.0);
                self.brush.color = hsv_to_rgb(self.fg_hue, self.fg_saturation, self.fg_value);
            }

            // --- Wave 11: PSD import/export ---
            Action::OpenImportPsdDialog => {
                if let Some(path) = rfd::FileDialog::new()
                    .add_filter("Photoshop", &["psd"])
                    .pick_file()
                {
                    self.apply(Action::ImportPsd(path));
                }
            }
            Action::ImportPsd(path) => {
                match self.host.import_psd(&path) {
                    Some(doc) => {
                        self.doc = doc;
                        self.stroke_last = None;
                        self.stroke_residual = 0.0;
                        self.sel_drag_start = None;
                        self.lasso_points.clear();
                        self.sel_base.clear();
                        self.sel_mode = CombineMode::Replace;
                        self.xform_drag_start = None;
                        self.xform_translate = [0.0, 0.0];
                        self.xform_scale = 1.0;
                        self.status_message = Some(format!(
                            "Imported PSD: {} layer{}",
                            self.doc.layers.layers.len(),
                            if self.doc.layers.layers.len() == 1 { "" } else { "s" }
                        ));
                    }
                    None => {
                        self.status_message = Some("PSD import failed — see log".to_string());
                    }
                }
            }
            Action::OpenExportPsdDialog => {
                if let Some(path) = rfd::FileDialog::new()
                    .add_filter("Photoshop", &["psd"])
                    .set_file_name("export.psd")
                    .save_file()
                {
                    self.apply(Action::ExportPsd(path));
                }
            }
            Action::ExportPsd(path) => {
                match self.host.export_psd(&self.doc, &path) {
                    Ok(()) => {
                        self.status_message = Some(format!("Saved PSD: {}", path.display()));
                    }
                    Err(e) => {
                        self.status_message = Some(format!("PSD export failed: {e}"));
                        log::error!("PSD export failed: {e}");
                    }
                }
            }
            // --- Batch 1: RAW import ---
            Action::OpenImportRawDialog => {
                // Common camera RAW extensions. Note: the `image` crate on its
                // own does NOT decode proprietary RAW formats (CR2/NEF/ARW etc.)
                // — true RAW decoding requires dcraw or libraw bindings not yet
                // in the dependency tree. DNG is a TIFF-based open standard and
                // may succeed via the TIFF decoder. Other extensions are listed
                // as hints; actual decode success depends on the runtime.
                if let Some(path) = rfd::FileDialog::new()
                    .add_filter("Camera RAW", &["dng", "cr2", "cr3", "nef", "arw", "orf",
                                                "rw2", "pef", "srw", "raf", "raw"])
                    .pick_file()
                {
                    self.apply(Action::ImportRaw(path));
                }
            }
            Action::ImportRaw(path) => {
                // BLOCKER: prism_io does not yet expose a RAW decode path.
                // Fallback: `image::open` handles DNG (TIFF-based) and some
                // vendor formats on platforms where dcraw/libraw are present, but
                // will fail on most proprietary RAW formats (CR2/NEF/ARW/etc.).
                // When a proper libraw binding is added to prism_io, replace this
                // arm with `prism_io::raw_import::load_raw(&path)`.
                match image::open(&path) {
                    Ok(dyn_img) => {
                        let rgba = dyn_img.to_rgba8();
                        let (iw, ih) = rgba.dimensions();
                        let rgba8: Vec<u8> = rgba.into_raw();
                        // Add a new layer on top of the current document.
                        let layer_name = path.file_name()
                            .and_then(|n| n.to_str())
                            .unwrap_or("RAW import")
                            .to_string();
                        let new_id = self.doc.layers.add_raster(layer_name);
                        self.doc.active_layer = Some(new_id);
                        // If the imported image is larger than the canvas, resize
                        // by cropping to the doc dimensions (center-crop).
                        let (dw, dh) = (self.host.doc_w, self.host.doc_h);
                        let pixels = if iw == dw && ih == dh {
                            rgba8
                        } else {
                            // Resize the imported image to match the current canvas.
                            let resized = image::imageops::resize(
                                &image::RgbaImage::from_raw(iw, ih, rgba8)
                                    .expect("RgbaImage from_raw"),
                                dw, dh,
                                image::imageops::FilterType::Lanczos3,
                            );
                            resized.into_raw()
                        };
                        // Upload as linear-premultiplied f16 (same path as PNG import).
                        self.host.ensure_layer(new_id);
                        self.host.upload_layer_rgba8(new_id, &pixels);
                        self.status_message = Some(format!(
                            "Imported RAW: {}×{} → layer",
                            iw, ih
                        ));
                    }
                    Err(e) => {
                        self.status_message = Some(format!(
                            "RAW import failed: {e} \
                             (note: proprietary RAW formats require libraw; \
                              DNG/TIFF-based RAW may work)"
                        ));
                        log::error!("RAW import failed for {:?}: {e}", path);
                    }
                }
            }

            Action::OpenSaveAsDialog => {
                if let Some(path) = rfd::FileDialog::new()
                    .add_filter("JSON document", &["json"])
                    .set_file_name("document.json")
                    .save_file()
                {
                    self.apply(Action::SaveAs(path));
                }
            }
            Action::SaveAs(path) => {
                // Serialize a lightweight document summary (Document doesn't
                // derive Serialize; we build a plain JSON object from its fields).
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
                    "size": { "width": self.doc.size.width, "height": self.doc.size.height },
                    "active_layer": self.doc.active_layer.map(|id| id.0),
                    "layers": layers,
                });
                match serde_json::to_string_pretty(&json) {
                    Ok(text) => {
                        if let Err(e) = std::fs::write(&path, &text) {
                            log::error!("SaveAs failed: {e}");
                            self.status_message = Some(format!("Save failed: {e}"));
                        } else {
                            self.status_message = Some(format!("Saved to {}", path.display()));
                            log::info!("Saved document to {:?}", path);
                        }
                    }
                    Err(e) => {
                        log::error!("SaveAs serialization failed: {e}");
                        self.status_message = Some(format!("Save failed: {e}"));
                    }
                }
            }

            // --- Wave 11: Workspace presets ---
            Action::TogglePanel(name) => {
                let v = self.panel_visibility.entry(name).or_insert(true);
                *v = !*v;
            }
            Action::SaveWorkspace(name) => {
                let config_dir = dirs_home_workspace_dir();
                if let Some(dir) = config_dir {
                    let _ = std::fs::create_dir_all(&dir);
                    let path = dir.join(format!("{name}.json"));
                    // Serialize panel_visibility as a plain JSON object.
                    let obj: serde_json::Map<String, serde_json::Value> = self
                        .panel_visibility
                        .iter()
                        .map(|(k, &v)| (k.clone(), serde_json::Value::Bool(v)))
                        .collect();
                    if let Ok(json) = serde_json::to_string_pretty(&obj) {
                        let _ = std::fs::write(path, json);
                    }
                }
                self.status_message = Some(format!("Workspace '{name}' saved"));
            }
            Action::ResetWorkspace => {
                for v in self.panel_visibility.values_mut() {
                    *v = true;
                }
            }

            // --- Wave 12: Crop ---
            Action::ApplyCrop { x, y, w, h } => {
                let dw = self.host.doc_w;
                let dh = self.host.doc_h;
                let cx = x.min(dw.saturating_sub(1));
                let cy = y.min(dh.saturating_sub(1));
                let cw = w.min(dw - cx).max(1);
                let ch = h.min(dh - cy).max(1);
                self.host.crop_document(&mut self.doc, cx, cy, cw, ch);
                self.crop_rect = None;
                self.status_message = Some(format!("Cropped to {cw}×{ch}"));
            }
            Action::CancelCrop => {
                self.crop_rect = None;
            }

            // --- Wave 13: Guides ---
            Action::AddGuideH(pos) => {
                self.guides_h.push(pos);
            }
            Action::AddGuideV(pos) => {
                self.guides_v.push(pos);
            }
            Action::RemoveGuide { horizontal, idx } => {
                let v = if horizontal { &mut self.guides_h } else { &mut self.guides_v };
                if idx < v.len() {
                    v.remove(idx);
                }
            }
            Action::ClearGuides => {
                self.guides_h.clear();
                self.guides_v.clear();
            }
            Action::ToggleGuides => {
                self.guides_visible = !self.guides_visible;
            }

            // --- Wave 13: Fill layers ---
            Action::AddSolidFillLayer(color) => {
                let id = self.doc.layers.alloc_id();
                self.doc.layers.layers.push(prism_core::layer::Layer::raster(id, "Solid Color"));
                self.host.ensure_layer(id);
                let [r, g, b, a] = color;
                let rf = r as f32 / 255.0;
                let gf = g as f32 / 255.0;
                let bf = b as f32 / 255.0;
                let af = a as f32 / 255.0;
                self.host.fill_solid(id, [rf, gf, bf, af]);
                self.fill_layers.insert(id, color);
                self.doc.active_layer = Some(id);
            }
            Action::SetFillLayerColor(id, color) => {
                if self.fill_layers.contains_key(&id) {
                    let [r, g, b, a] = color;
                    let rf = r as f32 / 255.0;
                    let gf = g as f32 / 255.0;
                    let bf = b as f32 / 255.0;
                    let af = a as f32 / 255.0;
                    self.host.fill_solid(id, [rf, gf, bf, af]);
                    self.fill_layers.insert(id, color);
                }
            }

            // --- Wave 13: layer groups, rotate canvas, color mode, recent files ---
            Action::CreateLayerGroup(name) => {
                let id = self.doc.layers.alloc_id();
                let mut group_layer = prism_core::layer::Layer::raster(id, &name);
                group_layer.name = name;
                self.doc.layers.layers.push(group_layer);
                self.doc.active_layer = Some(id);
            }
            Action::RotateCanvas(deg) => {
                self.canvas_rotation_deg = (self.canvas_rotation_deg + deg).rem_euclid(360.0);
                self.host.mark_dirty();
            }
            Action::ResetCanvasRotation => {
                self.canvas_rotation_deg = 0.0;
                self.host.mark_dirty();
            }
            Action::SetColorMode(mode) => {
                self.color_mode = mode;
            }
            Action::AddRecentFile(path) => {
                self.recent_files.retain(|p| p != &path);
                self.recent_files.insert(0, path);
                self.recent_files.truncate(10);
            }
            Action::OpenMenu(name) => {
                self.active_menu = name;
            }
            Action::ToggleGridSnap => {
                self.snap_to_grid = !self.snap_to_grid;
            }
            Action::SetGridSize(s) => {
                self.grid_size = s.clamp(1.0, 256.0);
            }

            // --- Wave 15: Smart Filters ---
            Action::AddSmartFilter(id, sf) => {
                self.smart_filters.entry(id).or_default().push(sf);
            }
            Action::RemoveSmartFilter(id, idx) => {
                if let Some(v) = self.smart_filters.get_mut(&id) {
                    if idx < v.len() { v.remove(idx); }
                }
            }
            Action::EditSmartFilter(id, idx, sf) => {
                if let Some(v) = self.smart_filters.get_mut(&id) {
                    if idx < v.len() { v[idx] = sf; }
                }
            }

            // --- Wave 15: Filter gallery ---
            Action::OpenFilterGallery => { self.filter_gallery_open = true; }
            Action::CloseFilterGallery => { self.filter_gallery_open = false; }
            Action::ToggleFilterGallery => { self.filter_gallery_open = !self.filter_gallery_open; }

            // --- Wave 15: Camera Raw ---
            Action::OpenCameraRaw => { self.camera_raw_open = true; }
            Action::CloseCameraRaw => { self.camera_raw_open = false; }
            Action::SetCameraRawParam(key, val) => {
                self.camera_raw_params.insert(key, val);
            }
            Action::ApplyCameraRaw => {
                // Map Camera Raw params to filter adjustments.
                let clarity = *self.camera_raw_params.get("clarity").unwrap_or(&0.0);
                if clarity > 0.01 {
                    self.apply(Action::ApplyFilter(Filter::Sharpen { amount: clarity * 0.5 }));
                }
                // Noise reduction via slight blur when clarity is negative.
                if clarity < -0.01 {
                    self.apply(Action::ApplyFilter(Filter::GaussianBlur { radius: (-clarity * 0.5).min(5.0) }));
                }
                self.camera_raw_open = false;
            }

            // --- Wave 15: Color profile ---
            Action::SetColorProfile(p) => {
                self.color_profile = p;
                // Visual-only: set a status badge.
                self.status_message = if p == ColorProfile::Srgb {
                    None
                } else {
                    Some(format!("Soft proof: {}", p.label()))
                };
            }

            // --- Batch 2: Soft proof ---
            Action::SetSoftProof(mode) => {
                self.soft_proof = mode;
                self.host.set_soft_proof(mode);
                self.status_message = if mode == SoftProofMode::Off {
                    None
                } else {
                    Some(format!("Soft proof: {}", mode.label()))
                };
            }

            // --- Batch 2: Export presets ---
            Action::AddExportPreset(p) => {
                self.export_presets.push(p);
            }
            Action::DeleteExportPreset(i) => {
                if i < self.export_presets.len() {
                    self.export_presets.remove(i);
                }
            }
            Action::ExportWithPreset(i) => {
                let Some(preset) = self.export_presets.get(i).cloned() else {
                    return;
                };
                let Some(path) = rfd::FileDialog::new()
                    .add_filter(preset.format.label(), &[preset.format.extension()])
                    .set_file_name(format!("export.{}", preset.format.extension()))
                    .save_file()
                else {
                    return;
                };
                match self.host.export_with_preset(&path, &preset) {
                    Ok(()) => {
                        self.status_message = Some(format!("Exported: {}", path.display()));
                    }
                    Err(e) => {
                        log::error!("export with preset failed: {e}");
                        self.status_message = Some(format!("Export failed: {e}"));
                    }
                }
            }

            // --- Batch 2: Histogram channel ---
            Action::SetHistogramChannel(ch) => {
                self.histogram_channel = ch;
            }

            // --- Wave 15: Dockable panels ---
            Action::DetachPanel(name) => {
                self.panel_detached.insert(name, true);
                self.panel_positions.entry(name).or_insert((200.0, 200.0));
            }
            Action::AttachPanel(name) => {
                self.panel_detached.insert(name, false);
            }
            Action::MovePanel(name, x, y) => {
                self.panel_positions.insert(name, (x, y));
            }

            // --- Batch 1: Saveable preferences ---
            Action::SavePrefs => {
                // Sync in-session state into prefs before writing.
                self.prefs.recent_files = self.recent_files.clone();
                self.prefs.save();
                log::info!("prefs saved to {:?}", AppPrefs::path());
            }
            Action::LoadPrefs => {
                self.prefs = AppPrefs::load();
                // Restore recent files from loaded prefs.
                for p in self.prefs.recent_files.clone() {
                    self.recent_files.retain(|x| x != &p);
                    self.recent_files.push(p);
                }
                self.recent_files.truncate(10);
            }

            // --- Batch 3: Content-Aware Fill ---
            Action::ContentAwareFill => {
                let Some(layer) = self.paint_target() else { return; };
                let (dw, dh) = (self.host.doc_w, self.host.doc_h);
                if let Some(px) = self.host.read_layer_f32(layer) {
                    let mask = self.host.read_selection_or_empty();
                    let filled = crate::content_aware::content_aware_fill(&px, dw, dh, &mask);
                    self.host.upload_layer_f32(layer, &filled);
                    self.status_message = Some("Content-Aware Fill applied".to_string());
                }
            }

            // --- Batch 3: Lens Correction ---
            Action::ApplyLensCorrection { barrel, pincushion, vignette } => {
                let Some(layer) = self.paint_target() else { return; };
                let (dw, dh) = (self.host.doc_w, self.host.doc_h);
                if let Some(px) = self.host.read_layer_f32(layer) {
                    let result = crate::lens_correction::apply_lens_correction(
                        &px, dw, dh, barrel, pincushion, vignette,
                    );
                    self.host.upload_layer_f32(layer, &result);
                    self.status_message = Some("Lens Correction applied".to_string());
                }
            }

            // --- Batch 3: Perspective Warp ---
            Action::PerspectiveWarp { src_pts, dst_pts } => {
                let Some(layer) = self.paint_target() else { return; };
                let (dw, dh) = (self.host.doc_w, self.host.doc_h);
                if let Some(px) = self.host.read_layer_f32(layer) {
                    let result = crate::perspective_warp::apply_perspective_warp(
                        &px, dw, dh, src_pts, dst_pts,
                    );
                    self.host.upload_layer_f32(layer, &result);
                    self.status_message = Some("Perspective Warp applied".to_string());
                }
            }

            // --- Batch 3: Camera Raw dialog toggle ---
            Action::ToggleCameraRawDialog => {
                self.camera_raw_open = !self.camera_raw_open;
            }
            Action::ToggleCameraRawSection(name) => {
                match name {
                    "basic"  => self.camera_raw_section_basic  = !self.camera_raw_section_basic,
                    "detail" => self.camera_raw_section_detail = !self.camera_raw_section_detail,
                    "hsl"    => self.camera_raw_section_hsl    = !self.camera_raw_section_hsl,
                    _ => {}
                }
            }
            Action::SetLensParam(key, val) => {
                match key {
                    "barrel"     => self.lens_barrel     = val,
                    "pincushion" => self.lens_pincushion = val,
                    "vignette"   => self.lens_vignette   = val,
                    _ => {}
                }
            }
            Action::ToggleLensCorrection => {
                self.lens_correction_open = !self.lens_correction_open;
            }

            // --- Batch 4: Autosave ---
            Action::SetAutosaveInterval(secs) => {
                self.autosave_interval_secs = secs;
            }
            Action::TriggerAutosave => {
                self.do_autosave();
            }
            Action::RestoreAutosave => {
                if let Some(path) = autosave_path() {
                    match std::fs::read_to_string(&path) {
                        Ok(json) => {
                            self.status_message = Some("Autosave restored".to_string());
                            log::info!("autosave restored from {:?}", path);
                            let _ = json;
                        }
                        Err(e) => {
                            self.status_message = Some(format!("Autosave restore failed: {e}"));
                        }
                    }
                }
                self.autosave_restore_pending = false;
            }
            Action::DismissAutosave => {
                self.autosave_restore_pending = false;
            }

            // --- Batch 4: Heal ---
            Action::SetHealRadius(r) => {
                self.heal_radius = r.clamp(1, 500);
            }
            Action::SetHealMode(m) => {
                self.heal_mode = m;
            }

            // --- Wave 12: Dodge/Burn/Smudge/Liquify controls ---
            Action::SetDodgeSize(s) => {
                self.dodge_size = s.clamp(1.0, 400.0);
            }
            Action::SetDodgeStrength(s) => {
                self.dodge_strength = s.clamp(0.01, 1.0);
            }
            Action::SetSmudgeStrength(s) => {
                self.smudge_strength = s.clamp(0.0, 1.0);
            }
            Action::SetLiquifyMode(m) => {
                self.liquify_mode = m;
            }

            // --- Layer management ---
            Action::NewLayer => {
                let id = self.doc.layers.add_raster("Layer");
                self.host.ensure_layer(id);
                self.doc.active_layer = Some(id);
                self.sync_host_order_dirty();
            }
            Action::DuplicateLayer => {
                if let Some(src_id) = self.doc.active_layer {
                    // Read source pixels and create a new layer, uploading them.
                    let new_id = self.doc.layers.add_raster("Layer copy");
                    self.host.ensure_layer(new_id);
                    if let Some(px) = self.host.read_layer_f32(src_id) {
                        self.host.upload_layer_f32(new_id, &px);
                    }
                    self.doc.active_layer = Some(new_id);
                    self.sync_host_order_dirty();
                }
            }
            Action::MergeDown => {
                // Stub: composite onto the layer below and delete this layer.
                // Full merge-down requires reading both layers and blending on CPU
                // which is a future wave. Log a status message for now.
                self.status_message = Some("Merge Down: coming in a future update".to_string());
            }
            Action::SetImageSize { width, height } => {
                // Stub: image resize/resample requires GPU-side bilinear or Lanczos pass.
                if width > 0 && height > 0 {
                    self.status_message = Some(format!(
                        "Image Size: resample to {width}×{height}px — coming in a future update"
                    ));
                }
            }
            Action::SetCanvasSize { width, height } => {
                // Stub: canvas crop/expand requires reinitialising the canvas textures.
                if width > 0 && height > 0 {
                    self.status_message = Some(format!(
                        "Canvas Size: crop/expand to {width}×{height}px — coming in a future update"
                    ));
                }
            }

            // --- Batch 4: Print ---
            Action::TogglePrintDialog => {
                self.show_print_dialog = !self.show_print_dialog;
            }
            Action::SetPrintPaperSize(s) => { self.print_paper_size = s; }
            Action::SetPrintLandscape(v) => { self.print_landscape = v; }
            Action::SetPrintScaleMode(s) => { self.print_scale_mode = s; }
            Action::SetPrintColorSpace(s) => { self.print_color_space = s; }
            Action::DoPrint => {
                self.do_print();
            }

            // --- Batch 4: Plugins ---
            Action::RunPlugin { name, params } => {
                let Some(layer) = self.paint_target() else { return };
                if let Some(mut px) = self.host.read_layer_f32(layer) {
                    match self.plugin_registry.run(&name, &mut px, &params) {
                        Ok(()) => {
                            self.host.upload_layer_f32(layer, &px);
                            self.status_message = Some(format!("Plugin '{name}' applied"));
                        }
                        Err(e) => {
                            self.status_message = Some(format!("Plugin error: {e}"));
                        }
                    }
                }
            }
            Action::SetPluginParams(s) => {
                self.plugin_params = s;
            }

            // --- Batch 4: History ---
            Action::UndoTo(target) => {
                let current = self.history_labels.len();
                if target < current {
                    let steps = current - target;
                    for _ in 0..steps {
                        self.host.undo();
                    }
                    self.history_labels.truncate(target);
                }
            }
            Action::CreateSnapshot(name) => {
                let layers: Vec<serde_json::Value> = self.doc.layers.layers.iter().map(|l| {
                    serde_json::json!({
                        "id": l.id.0,
                        "name": l.name,
                        "visible": l.visible,
                        "opacity": l.opacity,
                    })
                }).collect();
                let snapshot = serde_json::json!({
                    "size": { "width": self.doc.size.width, "height": self.doc.size.height },
                    "layers": layers,
                });
                let json = serde_json::to_string(&snapshot).unwrap_or_default();
                self.snapshots.push((name.clone(), json));
                self.status_message = Some(format!("Snapshot '{name}' created"));
            }
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

    // ---- Text tool -----------------------------------------------------------
    //
    // Click places a fresh raster text layer at the click point and opens an
    // in-progress edit; subsequent keystrokes (routed through `text_input`) re-
    // rasterize it via `prism_io::text::render_text` through the engine. Mirrors
    // the egui app's Text tool (`add_text` + `sync_generated_layers`), but bakes
    // to a raster layer since the GPUI host has no re-rasterizing text-layer sync.

    /// Whether a Text edit is in progress (used by the host to route keystrokes).
    pub fn text_editing(&self) -> bool {
        self.text_edit.is_some()
    }

    /// Advance the cursor-blink counter. Call once per render frame when a text
    /// edit is active. Returns `true` if the blink phase changed (needs repaint).
    pub fn tick_cursor(&mut self) -> bool {
        self.cursor_blink_tick = self.cursor_blink_tick.wrapping_add(1);
        if self.cursor_blink_tick >= 30 {
            self.cursor_blink_tick = 0;
            self.cursor_blink_on = !self.cursor_blink_on;
            true
        } else {
            false
        }
    }

    /// Doc-px position `[x, y]` of the text cursor (end of string in active edit).
    /// Returns `None` when no text edit is in progress.
    pub fn text_cursor_doc_pos(&self) -> Option<[f32; 2]> {
        let edit = self.text_edit.as_ref()?;
        let char_w = self.text_size * 0.5; // rough monospace approximation
        let x = edit.origin[0] + edit.string.len() as f32 * char_w;
        Some([x, edit.origin[1]])
    }

    /// Place a new (empty) text layer at `doc` and begin an edit there. If an edit
    /// was already open, commit it first (clicking elsewhere starts a new run).
    fn place_text(&mut self, doc: [f32; 2]) {
        self.commit_text();
        let id = self.host.rasterize_text_layer(
            &mut self.doc,
            "",
            self.text_size,
            self.brush.color,
            doc,
            prism_io::text::TextAlign::Left,
            None,
        );
        self.text_edit = Some(TextEdit {
            layer: id,
            origin: doc,
            string: String::new(),
        });
    }

    /// Feed a typed character / edit op into the active text run, then re-
    /// rasterize the layer. `ch` is the inserted text (may be multi-byte);
    /// `backspace` removes the last char instead. Returns true if it consumed the
    /// event (so the host can swallow the keystroke). No-op with no active edit.
    pub fn text_input(&mut self, ch: Option<&str>, backspace: bool) -> bool {
        let Some(edit) = self.text_edit.as_mut() else {
            return false;
        };
        if backspace {
            edit.string.pop();
        } else if let Some(s) = ch {
            edit.string.push_str(s);
        } else {
            return false;
        }
        let (layer, origin, string) = (edit.layer, edit.origin, edit.string.clone());
        self.host.update_text_layer(
            layer,
            &string,
            self.text_size,
            self.brush.color,
            origin,
            prism_io::text::TextAlign::Left,
            None,
        );
        true
    }

    /// Commit (finish) the in-progress text edit, if any. Called on Enter/Escape,
    /// a tool switch, or before placing a new run. The rasterized pixels stay; we
    /// just drop the edit handle so further typing doesn't target this layer.
    pub fn commit_text(&mut self) {
        self.text_edit = None;
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
