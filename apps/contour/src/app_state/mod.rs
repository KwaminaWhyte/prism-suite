//! The single shared application state for the GPUI host.
//!
//! `App` owns everything panels read or mutate: the `CanvasHost` (Contour's CPU
//! rasterizer bridged into GPUI), the `Document`, and the active tool / view.
//! Panels NEVER mutate `App` fields directly — they emit an [`Action`], and the
//! root view routes it through [`App::apply`], which is the single choke point
//! that mutates state and marks the host dirty when pixels change. This keeps
//! the panel→state seam narrow so panels can be ported in parallel without
//! colliding: a new panel only needs to (a) read `&App` and (b) add `Action`
//! variants + their `apply` arms.
//!
//! Mirrors the proven Pigment host layout (`pigment-gpui/src/app_state.rs`),
//! adapted to Contour's vector document (a `Vec<Shape>` in paint order) and its
//! CPU raster preview path (no wgpu / no GPU compositor).

use crate::align::{self, Align, Distribute};
use crate::appearance::{Appearance, Effect, Fill, Paint, Stroke as AppStroke};
use crate::boolean::{self, BoolFillRule, BoolOp};
use crate::document::{self, Document, Shape};
use crate::gradient::{Gradient, GradientKind, GradientStop};
use crate::history::History;
use crate::liveshape::LiveShape;
use crate::text::{TextAlign, TextParams};
use crate::transform::{self, Affine, Handle};

use prism_core::geometry::Rect as CoreRect;

use crate::canvas_host::CanvasHost;

pub mod types_export;
pub use types_export::*;
pub mod types_document;
pub use types_document::*;
pub mod types_symbols;
pub use types_symbols::*;
pub mod types_colors;
pub use types_colors::*;
pub mod types_tracing;
pub use types_tracing::*;
pub mod types_text;
pub use types_text::*;
pub mod types_effects;
pub use types_effects::*;

mod apply_advanced;
mod apply_extended;
mod apply_extended2;
mod apply_extended3;
mod tests_core;
mod apply_core;
mod apply_waves_w11;
mod tests_wave9;
mod apply_waves;
mod apply_waves2;
pub(super) mod helpers;
use helpers::{shape_to_svg, rgba_to_hex, path_to_svg_d, import_svg, parse_svg_path_d, rand_group_id, default_image_trace_threshold, default_image_trace_colors, default_omask_id_counter, default_graph_style_fill, colors_approx_equal, offset_polygon, sample_polyline, warp_shape_perspective, bounds_intersect, sample_document};

/// Document-unit pick tolerances for the node (Direct-Select) tool — the radius
/// within which a click grabs an anchor or a tangent-handle knob. Handles are
/// tested first so a knob sitting near its anchor still wins (mirrors the egui
/// app's `hit_path_edit`, which prioritises handles).
const ANCHOR_PICK_TOL: f32 = 6.0;
const HANDLE_PICK_TOL: f32 = 6.0;
/// How close (document units) a Pen click must land to the path's first anchor
/// to close the path instead of placing a new anchor.
const PEN_CLOSE_TOL: f32 = 8.0;

/// A path's editable geometry for the node overlay: anchor `points` paired with
/// their per-anchor out-tangent `handles` (both in document space).
pub type PathNodes = (Vec<(f32, f32)>, Vec<(f32, f32)>);

/// One editable element of a path under the node tool: an anchor point or the
/// (mirrored) tangent-handle of an anchor. Mirrors the egui app's `PathEdit`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum NodeTarget {
    Anchor(usize),
    Handle(usize),
}

/// A named symbol: a reusable collection of shape indices cloned from the canvas
/// at creation time. Instances are placed via `Action::PlaceSymbol`.
#[derive(Clone, Debug)]
pub struct Symbol {
    /// Stable id assigned at creation (sequential counter).
    pub id: u64,
    /// User-visible name.
    pub name: String,
    /// Paint-order indices of the shapes that were selected when the symbol was
    /// created (stored for display; actual content is a snapshot of those shapes).
    pub shape_ids: Vec<u64>,
}

/// In-progress Pen-tool path. Anchors are accumulated in **document space** as
/// the user clicks; `handles[i]` is anchor `i`'s out-tangent offset (the
/// in-tangent mirrors). Committed into a `Shape::Path` on close / finish.
/// Mirrors the egui app's `inter.pen_points` / `pen_handles`.
#[derive(Clone, Default, Debug)]
pub struct PenState {
    pub points: Vec<(f32, f32)>,
    pub handles: Vec<(f32, f32)>,
}

impl PenState {
    pub fn is_empty(&self) -> bool {
        self.points.is_empty()
    }
    fn clear(&mut self) {
        self.points.clear();
        self.handles.clear();
    }
}

/// The canvas view transform: where the bridged preview sits in the canvas
/// viewport and how much it is magnified. Mirrors the egui app's `canvas::View`
/// (`pan` + `zoom`), adapted to the GPUI host where the preview is an already-
/// rasterized artboard image rather than per-frame painted vectors.
///
/// `offset` is the viewport-relative position (in window pixels) of the preview
/// image's top-left corner. `scale` is the magnification of that image (window
/// pixels per preview pixel; preview pixels == document units on the artboard).
/// A document point maps to a window point by
/// `win = viewport_origin + offset + (doc - artboard_origin) * scale`.
#[derive(Clone, Copy, Debug)]
pub struct View {
    pub offset: (f32, f32),
    pub scale: f32,
}

impl View {
    /// Clamp bounds for `scale` so zoom can't invert or explode the preview.
    pub const MIN_SCALE: f32 = 0.1;
    pub const MAX_SCALE: f32 = 16.0;
}

impl Default for View {
    fn default() -> Self {
        // A small inset so the artboard doesn't hug the viewport's top-left.
        Self {
            offset: (40.0, 40.0),
            scale: 1.0,
        }
    }
}

/// The editing tools, mirroring the egui app's private `Tool` enum (see
/// `contour-app/src/app/mod.rs`). The GPUI host wires real tool behavior per
/// wave; for this Phase 1+2 starter the tool is selectable UI state only.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tool {
    Select,
    DirectSelect,
    Rect,
    Ellipse,
    Line,
    Polygon,
    Star,
    Pen,
    Artboard,
    Eyedropper,
    ShapeBuilder,
    Type,
    /// Drag a line across the canvas to split intersected shapes at the cut.
    Knife,
    /// Drag along a path stroke to vary its width (width profile tool).
    Width,
    /// Click two shapes to blend between them with interpolated steps.
    Blend,
    /// Insert a chart / graph as shapes (stub).
    Graph,
    /// Fill enclosed regions formed by crossing paths.
    LivePaint,
    /// Apply a 4-corner perspective warp to a shape.
    PerspectiveDistort,
    /// Warp a shape using a configurable grid mesh.
    Envelope,
    /// Scallop distort warp tool (like Illustrator's Scallop).
    Scallop,
    /// Crystallize distort warp tool.
    Crystallize,
    /// Wrinkle distort warp tool.
    Wrinkle,
}

impl Tool {
    /// Short label for the tools strip / toolbar.
    pub fn label(self) -> &'static str {
        match self {
            Tool::Select => "Select",
            Tool::DirectSelect => "Direct",
            Tool::Rect => "Rect",
            Tool::Ellipse => "Ellipse",
            Tool::Line => "Line",
            Tool::Polygon => "Polygon",
            Tool::Star => "Star",
            Tool::Pen => "Pen",
            Tool::Artboard => "Artboard",
            Tool::Eyedropper => "Eyedrop",
            Tool::ShapeBuilder => "Builder",
            Tool::Type => "Type",
            Tool::Knife => "Knife",
            Tool::Width => "Width",
            Tool::Blend => "Blend",
            Tool::Graph => "Graph",
            Tool::LivePaint => "Live Paint",
            Tool::PerspectiveDistort => "Persp",
            Tool::Envelope => "Envelope",
            Tool::Scallop => "Scallop",
            Tool::Crystallize => "Crystal",
            Tool::Wrinkle => "Wrinkle",
        }
    }

    /// A one- or two-letter glyph for the compact left tools strip.
    pub fn glyph(self) -> &'static str {
        match self {
            Tool::Select => "V",
            Tool::DirectSelect => "A",
            Tool::Rect => "R",
            Tool::Ellipse => "O",
            Tool::Line => "/",
            Tool::Polygon => "P",
            Tool::Star => "*",
            Tool::Pen => "✎",
            Tool::Artboard => "□",
            Tool::Eyedropper => "I",
            Tool::ShapeBuilder => "M",
            Tool::Type => "T",
            Tool::Knife => "K",
            Tool::Width => "W",
            Tool::Blend => "B",
            Tool::Graph => "G",
            Tool::LivePaint => "L",
            Tool::PerspectiveDistort => "D",
            Tool::Envelope => "E",
            Tool::Scallop => "~",
            Tool::Crystallize => "#",
            Tool::Wrinkle => "≈",
        }
    }

    /// Stable ordering for the tools strip (matches the egui palette grouping).
    pub const ALL: [Tool; 22] = [
        Tool::Select,
        Tool::DirectSelect,
        Tool::Rect,
        Tool::Ellipse,
        Tool::Line,
        Tool::Polygon,
        Tool::Star,
        Tool::Pen,
        Tool::Artboard,
        Tool::Eyedropper,
        Tool::ShapeBuilder,
        Tool::Type,
        Tool::Knife,
        Tool::Width,
        Tool::Blend,
        Tool::Graph,
        Tool::LivePaint,
        Tool::PerspectiveDistort,
        Tool::Envelope,
        Tool::Scallop,
        Tool::Crystallize,
        Tool::Wrinkle,
    ];
}

/// Every panel→state mutation a panel can request. Panels emit these; the root
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

    // --- Layers / shapes ---
    /// Toggle a shape's visibility by paint-order index (re-rasterizes).
    ToggleShapeVisible(usize),
    /// Make a shape the active / selected one (pure UI state).
    SelectShape(usize),

    // --- Canvas selection ---
    /// Hit-test the topmost selectable shape under a *document-space* point and
    /// make it the selection (or clear it when the click misses every shape).
    /// The root view maps the preview-pixel click → document space before
    /// emitting this (see `main.rs::window_to_doc`).
    HitTestSelect { x: f32, y: f32 },

    // --- Shape creation (Rect / Ellipse drag-create) ---
    /// Create a new shape from a drag, given the two *document-space* corners.
    /// Builds a `Shape::rect` / `Shape::ellipse` (the only tools that drag-create
    /// here) with the current default fill/stroke, appends it in paint order, and
    /// selects it. A degenerate drag (sub-unit in both axes) is dropped. The root
    /// view maps both window corners → document space before emitting.
    CreateShape {
        tool: Tool,
        a: [f32; 2],
        b: [f32; 2],
    },

    // --- Type / Text tool ---
    /// Place a new (empty) point-type object at a *document-space* point and enter
    /// text-edit mode on it (mirrors the egui app's `place_text`). The host then
    /// routes keystrokes through `TypeChar` / `TypeBackspace` until `FinishText`.
    PlaceText { x: f32, y: f32 },
    /// Re-enter text-edit mode on the existing point-type object at paint index.
    EditText(usize),
    /// Append a character to the text object being edited (re-lays-out its glyphs).
    TypeChar(char),
    /// Delete the last character of the text object being edited.
    TypeBackspace,
    /// Leave text-edit mode. An object left with empty text is removed (mirrors the
    /// egui app's `end_text_edit`).
    FinishText,

    // --- Boolean / pathfinder ops (on the two selected shapes) ---
    /// Run a boolean op on the primary + secondary selection (front = primary,
    /// back = secondary), replacing both inputs with the result batch. Undoable.
    Boolean(BoolOp),

    // --- Multi-select (boolean operands / marquee / align) ---
    /// Shift+click toggles a shape in/out of the selection set (or, if the click
    /// misses, leaves the set unchanged). The newly-added shape becomes primary.
    AddToSelection { x: f32, y: f32 },
    /// Replace the whole selection with every shape whose bounds intersect the
    /// document-space marquee rectangle `[x, y, w, h]` (empty-canvas drag). An
    /// empty hit set clears the selection.
    MarqueeSelect { rect: [f32; 4] },

    // --- Live-shape inspector (regenerate the primary polygon / star) ---
    /// Replace the primary live-shape's parameters and regenerate its outline
    /// (sides/points/radius/inner-ratio). No-op unless the primary is a live path.
    SetLiveShape(LiveShape),

    // --- Text inspector (re-lay-out the primary text object) ---
    /// Set the primary text object's em size (document units, clamped ≥ 1).
    SetTextSize(f32),
    /// Set the primary text object's horizontal alignment.
    SetTextAlign(TextAlign),
    /// Set the primary text object's font family (`None` → the bundled default).
    SetTextFont(Option<String>),
    /// Toggle the font-family dropdown open/closed.
    ToggleFontDropdown,

    // --- Align & distribute (on the multi-selection) ---
    /// Align every selected shape's matching edge / centre to the selection's
    /// combined bounding box. Needs ≥2 selected shapes. One undo step.
    AlignSelection(Align),
    /// Evenly distribute the selected shapes' chosen feature / gap. Needs ≥3
    /// selected shapes. One undo step.
    DistributeSelection(Distribute),

    // --- View transform (pan / zoom) ---
    /// Translate the view by a window-pixel delta (drag-to-pan / middle-drag).
    PanBy { dx: f32, dy: f32 },
    /// Multiply the zoom by `factor`, keeping the *viewport-relative* anchor point
    /// `(ax, ay)` (window px, viewport-local) fixed on screen (scroll / +/-). When
    /// the anchor is `None` the viewport centre is held (keyboard +/-).
    ZoomBy {
        factor: f32,
        anchor: Option<(f32, f32)>,
        viewport: (f32, f32),
    },
    /// Reset pan + zoom to the default view.
    ResetView,

    // --- Inspector edits (mutate the selected shape, re-rasterize) ---
    /// Set the selected shape's solid fill colour (straight sRGB RGBA).
    SetFillColor([f32; 4]),
    /// Set the selected shape's stroke colour (straight sRGB RGBA).
    SetStrokeColor([f32; 4]),
    /// Set the selected shape's stroke width (document units, clamped ≥ 0).
    SetStrokeWidth(f32),
    /// Set the selected shape's opacity. Contour's legacy shape model has no
    /// separate opacity field, so this writes the *fill alpha* channel — the
    /// closest single-field analogue that re-rasterizes visibly. Clamped [0,1].
    SetOpacity(f32),

    // --- Pen tool (vector authoring) ---
    /// Place a new anchor at a *document-space* point in the in-progress pen
    /// path. If the click lands within `PEN_CLOSE_TOL` of the path's first
    /// anchor (and there are ≥2 anchors) this closes + commits the path instead.
    PenAddAnchor { x: f32, y: f32 },
    /// Set the out-tangent handle of the most-recent pen anchor so its out-knob
    /// sits at the *document-space* point `(x, y)` (drag after a click-place).
    PenSetHandle { x: f32, y: f32 },
    /// Finish the in-progress pen path, committing it as a `Shape::Path`.
    /// `closed` closes the contour (double-click / click-first-anchor); an open
    /// finish (Enter) leaves it open. A path with <2 anchors is discarded.
    PenFinish { closed: bool },
    /// Abandon the in-progress pen path without committing (Escape / tool swap).
    PenCancel,

    // --- Direct-select (node edit) ---
    /// Move anchor `which` of the selected path to a *document-space* point.
    /// Coalesced into one undo entry via `BeginInteraction` / `EndInteraction`.
    MoveAnchor { which: usize, x: f32, y: f32 },
    /// Set the out-tangent handle of anchor `which` of the selected path so its
    /// out-knob sits at `(x, y)` (the in-knob mirrors). Coalesced like above.
    MoveHandle { which: usize, x: f32, y: f32 },

    // --- Delete ---
    /// Delete the selected shape (Backspace / Delete with the Select tool).
    DeleteSelected,

    // --- Undo / redo coalescing + history navigation ---
    /// Snapshot the document at the start of a coalesced drag (idempotent within
    /// the drag, so per-move calls collapse into one undo entry).
    BeginInteraction,
    /// Finalize a coalesced drag; drops the checkpoint if nothing changed.
    EndInteraction,
    /// Restore the previous document state (Cmd+Z).
    Undo,
    /// Re-apply an undone state (Cmd+Shift+Z).
    Redo,

    // --- Symbols (Wave 8) ---
    /// Create a new symbol from the current selection, giving it `name`.
    CreateSymbol(String),
    /// Place a linked instance of symbol `id` as a shape at the canvas centre.
    PlaceSymbol(u64),
    /// Enter symbol-edit mode for `id` (stub — logs and returns).
    EditSymbol(u64),

    // --- Gradient-stop editor (Wave 8) ---
    /// Add a new stop at `pos` (0..=1) with `color` to the selected shape's
    /// gradient fill. No-op if the shape has no gradient.
    AddGradientStop { shape_id: usize, pos: f32, color: [f32; 4] },
    /// Move gradient stop `stop_idx` to `new_pos` (clamped 0..=1).
    MoveGradientStop { shape_id: usize, stop_idx: usize, new_pos: f32 },
    /// Delete gradient stop `stop_idx` (only when the gradient has >2 stops).
    DeleteGradientStop { shape_id: usize, stop_idx: usize },
    /// Change which gradient stop the editor highlights / edits.
    SelectGradientStop(Option<usize>),
    /// Set the RGBA color of gradient stop `stop_idx` on `shape_id`.
    SetGradientStopColor { shape_id: usize, stop_idx: usize, color: [f32; 4] },

    // --- Image Trace (Wave 8) ---
    /// Show the Image Trace inline panel in the right dock.
    OpenTracePanel,
    /// Hide the Image Trace panel.
    CloseTracePanel,
    /// Update the threshold slider value (0–255).
    SetTraceThreshold(u8),
    /// Update the colors count stepper (2–32).
    SetTraceColors(u8),
    /// Run the trace algorithm (stub: logs params, marks host dirty).
    TraceImage { shape_id: usize, threshold: u8, colors: u8 },

    // --- Recolor Artwork (Wave 8) ---
    /// Show the Recolor panel in the right dock.
    OpenRecolorPanel,
    /// Hide the Recolor panel and clear the selected-color state.
    CloseRecolorPanel,
    /// Highlight `color` as the swatch being edited.
    SelectRecolorColor([f32; 4]),
    /// Replace every fill matching `old_color` in the selection with `new_color`.
    RecolorSelected { old_color: [f32; 4], new_color: [f32; 4] },

    // --- Artboards (Wave 8) ---
    /// Add a new artboard at the given document-space rect `[x, y, w, h]`.
    AddArtboard([f32; 4]),

    // --- Eyedropper (Wave 9) ---
    /// Pick a colour from a sampled canvas pixel (RGBA bytes). Sets `fg_color`
    /// and, if a shape is selected, applies the colour as its fill. Reverts the
    /// active tool to `prev_tool`.
    PickColor([u8; 4]),

    // --- Shape Builder (Wave 9) ---
    /// Stub: union the listed shapes into a compound. Logs + selects them.
    MergeRegion(Vec<usize>),
    /// Stub: subtract region. Logs the ids.
    SubtractRegion(Vec<usize>),
    /// Apply the Shape Builder geometry op to the current selection (≥2 shapes).
    /// `subtract` = Alt/Option held → subtract mode; else unite all faces.
    ApplyShapeBuilder { subtract: bool },

    // --- Text on Path (Wave 9) ---
    /// Attach `text_id` to flow along `path_id`.
    AttachTextToPath { text_id: usize, path_id: usize },
    /// Detach `text_id` from its path attachment.
    DetachTextFromPath(usize),

    // --- SVG I/O (Wave 9) ---
    /// Serialize the document to an SVG file at `path`.
    ExportSvg(std::path::PathBuf),
    /// Parse a basic SVG file and append its shapes to the document.
    ImportSvg(std::path::PathBuf),

    // --- Layer order / grouping (Wave 9) ---
    /// Assign a shared group id to every selected shape (≥2). Cmd+G.
    GroupSelected,
    /// Clear the group id from every selected shape. Cmd+Shift+G.
    UngroupSelected,
    /// Move shape at paint-order index `id` by `delta` steps (+ = forward /
    /// toward top, − = backward). Cmd+] / Cmd+[.
    MoveLayerOrder { id: usize, delta: i32 },

    // --- Wave 11: Gradient type toggle ---
    /// Change the gradient fill kind (Linear / Radial / Angle) for `shape_id`.
    /// No-op when the shape has no gradient fill.
    SetGradientType { shape_id: usize, kind: GradientKind },

    // --- Wave 11: Shape builder (real geometry) ---
    /// Union all shapes in `ids` into a single merged shape. Replaces them.
    MergeRegionReal(Vec<usize>),
    /// Subtract shapes `ids[1..]` from `ids[0]`. Replaces all with the result.
    SubtractRegionReal(Vec<usize>),

    // --- Wave 11: Character / paragraph panel ---
    /// Set the global default font family (applied to newly created text and the
    /// selected text object if one is active).
    SetFontFamily(String),
    /// Set the global default font size.
    SetFontSize(f32),
    /// Set the global font weight toggle.
    SetFontWeight(FontWeight),
    /// Set the global letter-spacing (tracking) in document units.
    SetLetterSpacing(f32),
    /// Set the global line-height (leading) multiplier.
    SetLineHeight(f32),
    /// Set the global paragraph text alignment (applied to selected text).
    SetParaAlign(TextAlign),

    // --- Wave 11: PDF export ---
    /// Export the document as a minimal PDF to `path`.
    ExportPdf(std::path::PathBuf),

    // --- Wave 11: Isolation mode ---
    /// Enter isolation mode: dim all shapes outside `group_id`.
    EnterIsolation(u64),
    /// Exit isolation mode.
    ExitIsolation,

    // --- Wave 11: Knife tool ---
    /// Slice all shapes that intersect the line from `start` to `end` (document
    /// space) into two halves at the intersection points.
    KnifeSlice { start: (f32, f32), end: (f32, f32) },

    // --- Wave 12: on-canvas transform handles ---
    /// Translate all selected shapes by `(dx, dy)` (document units). Coalesced.
    MoveSelection { dx: f32, dy: f32 },
    /// Begin a scale-handle drag. `handle_idx` is 0..7 (Handle::ALL order).
    /// `doc` is the cursor in document space; `bbox` is `[x,y,w,h]` of the
    /// current selection, captured at drag start.
    BeginTransformScale { handle_idx: u8, doc: [f32; 2], bbox: [f32; 4] },
    /// Drive an active scale drag. `doc` is the current cursor; `uniform` = Shift held.
    TransformScaleDrag { doc: [f32; 2], uniform: bool },

    // --- Wave 13: guides, rotate, grid, stroke props, text, transform, etc. ---
    /// Add a horizontal guide at `y` (document units).
    AddGuideH(f32),
    /// Add a vertical guide at `x` (document units).
    AddGuideV(f32),
    /// Remove a guide (`horizontal` = true → from guides_h, else guides_v).
    RemoveGuide { horizontal: bool, idx: usize },
    /// Toggle guide visibility.
    ToggleGuides,
    /// Toggle pixel-grid snap.
    ToggleGridSnap,
    /// Rotate the canvas view by `degrees`.
    RotateCanvas(f32),
    /// Reset canvas rotation to 0°.
    ResetCanvasRotation,
    /// Rotate selected shapes by `degrees` around their collective centroid.
    RotateSelection(f32),
    /// Reflect selected shapes horizontally (flip X).
    ReflectSelectionH,
    /// Reflect selected shapes vertically (flip Y).
    ReflectSelectionV,
    /// Shear selected shapes by `shear_x` (horizontal shear factor).
    ShearSelection(f32),
    /// Scale all selected shapes to `factor_x` × `factor_y` uniformly.
    ScaleSelection { factor_x: f32, factor_y: f32 },
    /// Outline text: rasterize the selected text shape to a path.
    OutlineText(usize),
    /// Set stroke alignment on selected shapes.
    SetStrokeAlignment(StrokeAlignment),
    /// Set dash pattern on selected shapes (empty = solid).
    SetDashPattern(Vec<f32>),
    /// Set arrowhead style on selected shapes.
    SetArrowHead { start: ArrowHead, end: ArrowHead },
    /// Set character kerning (tracking) for selected text shapes.
    SetKerning(f32),
    /// Set paragraph leading multiplier for selected text shapes.
    SetLeading(f32),
    /// Wrap text to width for selected text shapes.
    SetTextWrapWidth(f32),
    /// Add or restore a layer comp (snapshot of layer visibility by name).
    SaveLayerComp(String),
    /// Apply a saved layer comp (restore visibility state).
    ApplyLayerComp(String),
    /// Set pattern fill index for selected shapes.
    SetPatternFill(usize),
    /// Add a named pattern to the pattern library.
    AddPattern { name: String, tile_w: f32, tile_h: f32 },
    /// Export the current canvas as PNG to `path` via the raster pipeline.
    ExportPng(std::path::PathBuf),
    /// Open a file picker and export PNG.
    OpenExportPngDialog,

    // --- Appearance panel ---
    /// Migrate selected shape from legacy fill/stroke to an explicit Appearance stack.
    MigrateAppearance,
    /// Add a solid fill to the selected shape's appearance stack.
    AppearanceAddFill,
    /// Add a stroke to the selected shape's appearance stack.
    AppearanceAddStroke,
    /// Add a drop shadow effect to the selected shape's appearance stack.
    AppearanceAddDropShadow,
    /// Add a Gaussian blur effect to the selected shape's appearance stack.
    AppearanceAddBlur,
    /// Remove fill at `idx` from the selected shape's appearance stack.
    AppearanceRemoveFill(usize),
    /// Remove stroke at `idx` from the selected shape's appearance stack.
    AppearanceRemoveStroke(usize),
    /// Remove effect at `idx` from the selected shape's appearance stack.
    AppearanceRemoveEffect(usize),
    /// Toggle fill visibility at `idx`.
    AppearanceToggleFill(usize),
    /// Toggle stroke visibility at `idx`.
    AppearanceToggleStroke(usize),
    /// Set fill color at `idx`.
    AppearanceSetFillColor { idx: usize, color: [f32; 4] },
    /// Set stroke color at `idx`.
    AppearanceSetStrokeColor { idx: usize, color: [f32; 4] },
    /// Set stroke width at `idx`.
    AppearanceSetStrokeWidth { idx: usize, width: f32 },
    /// Raise fill `idx` one step in the stack (towards top).
    AppearanceRaiseFill(usize),
    /// Lower fill `idx` one step.
    AppearanceLowerFill(usize),
    /// Raise stroke `idx` one step.
    AppearanceRaiseStroke(usize),
    /// Lower stroke `idx` one step.
    AppearanceLowerStroke(usize),

    // --- Graphic styles ---
    /// Save the selected shape's effective appearance as a new named style.
    SaveGraphicStyle(String),
    /// Apply a graphic style (by id) to the selected shape.
    ApplyGraphicStyle(u64),
    /// Delete a graphic style from the library.
    DeleteGraphicStyle(u64),

    // --- Width tool ---
    /// Set start/end width multipliers on the selected shape's stroke.
    SetWidthProfile { start: f32, end: f32 },

    // --- Blend tool ---
    /// Create interpolated blend steps between the two selected shapes.
    CreateBlend { steps: usize },
    /// Set the blend step count (stored for the blend tool UI).
    SetBlendSteps(usize),

    // --- Perspective grid ---
    /// Toggle the perspective grid overlay.
    TogglePerspectiveGrid,
    /// Move a perspective vanishing point (vp=0 or 1) to doc-space (x,y).
    MovePerspectiveVP { vp: u8, x: f32, y: f32 },

    // --- Mesh gradient ---
    /// Set a 4×4 mesh gradient on the selected shape's fill.
    SetMeshGradient { points: Vec<((f32, f32), [f32; 4])> },
    /// Edit one mesh control point's position and color.
    EditMeshPoint { row: usize, col: usize, pos: (f32, f32), color: [f32; 4] },

    // --- AI/EPS import ---
    /// Import an AI or EPS file, appending parsed shapes to the document.
    ImportAiEps(std::path::PathBuf),

    // --- Batch 2: Variable font axes ---
    /// Set one variable-font axis on the selected text object and update its layout.
    SetFontAxis { axis: String, value: f32 },

    // --- Batch 2: Symbol library (real) ---
    /// Define a new symbol from the current selection using the real `Symbols` library.
    DefineSymbol(String),
    /// Place a linked instance of symbol `id` using the real `Symbols` library.
    PlaceSymbol2(u64),
    /// Enter symbol-edit mode for `id` (stub — logs and returns).
    EditSymbol2(u64),
    /// Delete symbol `id` and all its instances from the library.
    DeleteSymbol(u64),

    // --- Batch 2: Multiple artboards (named) ---
    /// Add a new named artboard; name is auto-generated.
    AddArtboard2 { rect: [f32; 4] },
    /// Remove the artboard at index `idx`.
    RemoveArtboard(usize),
    /// Rename artboard at index `idx`.
    RenameArtboard { idx: usize, name: String },
    /// Set the active artboard by id.
    SelectArtboard(u64),
    /// Duplicate artboard at index `idx` (offset by 20,20).
    DuplicateArtboard(usize),

    // --- Batch 2: Graph / chart tool stub ---
    /// Insert a graph from sample data as shapes at `rect`.
    InsertGraph { kind: GraphKind, rect: [f32; 4] },

    // --- Batch 2: Stroke width profile presets ---
    /// Apply a named width-profile preset to the selected shape's stroke.
    ApplyWidthPreset(WidthProfilePreset),

    // --- 3D extrude effect ---
    /// Add a 3D extrude effect to the selected shape's appearance.
    AppearanceAdd3DExtrude,
    /// Set extrude depth (document units).
    AppearanceSet3DDepth { idx: usize, depth: f32 },
    /// Set extrude angle (degrees).
    AppearanceSet3DAngle { idx: usize, angle_deg: f32 },

    // --- Batch 3: Live Paint ---
    /// Fill the topmost closed shape containing the click point with `fill`.
    ApplyLivePaint { fill: [f32; 4] },

    // --- Batch 3: Type on Path ---
    /// Attach the selected text to the selected path (selection must contain
    /// exactly one text and one non-text shape).
    PlaceTextOnPath,

    // --- Batch 3: Perspective Distort ---
    /// Set 4-corner perspective distort corners on `shape_id` (stub).
    SetPerspectiveDistort { shape_id: usize, corners: [[f32; 2]; 4] },
    /// Confirm and apply the active perspective distort (stub).
    ConfirmPerspectiveDistort,

    // --- Batch 3: Recolor Artwork HSL ---
    /// Open the HSL recolor panel.
    OpenRecolorHSLPanel,
    /// Close the HSL recolor panel.
    CloseRecolorHSLPanel,
    /// Shift hue/saturation/brightness of all selected shapes' fills and strokes.
    RecolorArtworkHSL { hue_shift: f32, saturation_scale: f32, brightness_scale: f32 },

    // --- Batch 3: Align to Artboard ---
    /// Align selection relative to the active artboard bounds.
    AlignToArtboard { alignment: Align },

    // --- Batch 4: Envelope Distort ---
    /// Create an envelope distort mesh on `shape_id` with `rows × cols` control points.
    MakeEnvelopeWithMesh { shape_id: usize, rows: u32, cols: u32 },
    /// Move envelope control point at `idx` to `pos`.
    MoveEnvelopePoint { shape_id: usize, idx: usize, pos: [f32; 2] },
    /// Release the envelope distort from `shape_id`.
    ReleaseEnvelope(usize),

    // --- Batch 4: Live Effects ---
    /// Add an outer glow effect to the selected shape's appearance stack.
    AppearanceAddGlow,
    /// Toggle effect at `index` on shape `shape_id`.
    ToggleEffect { shape_id: usize, index: usize },
    /// Remove effect at `index` from shape `shape_id`.
    RemoveEffect { shape_id: usize, index: usize },
    /// Reorder effect: move from `from` to `to` in shape `shape_id`'s effect stack.
    ReorderEffect { shape_id: usize, from: usize, to: usize },

    // --- Batch 4: Group blend modes ---
    /// Set the blend mode for a group (all shapes with group_id).
    SetGroupBlendMode { group_id: u64, mode: crate::appearance::BlendMode },
    /// Set the opacity for a group (0..=1).
    SetGroupOpacity { group_id: u64, opacity: f32 },

    // --- Batch 4: Pathfinder improvements ---
    /// Convert the selected shape's stroke to a filled outline path.
    OutlineStroke(usize),
    /// Bake appearance effects into the shape geometry (destructive).
    ExpandAppearance(usize),

    // --- Batch 5: Type on a Path (arc-length rendering) ---
    /// Set the type-on-path parameters (offset / start / flip) for the attached
    /// text `text_id` and re-bake its glyphs along its spine. No-op if the text
    /// is not attached to a path.
    SetTextOnPathParams {
        text_id: usize,
        params: crate::text_on_path::TextOnPathParams,
    },

    // --- Batch 5: Perspective Distort (real homography) ---
    /// Apply the active perspective distort to `shape_id`: warp its geometry
    /// through the homography defined by the current corners, replacing the shape
    /// with the distorted path. One undo step.
    ApplyPerspectiveDistort { shape_id: usize },

    // --- Batch 5: Gradient Mesh object ---
    /// Create a 4×4 gradient mesh seeded from the selected shape's bounds + fill.
    MakeMeshGradient,
    /// Clear the active gradient mesh overlay.
    ClearMeshGradient,

    // --- Batch 5: Variable-width stroke (width profile) ---
    /// Convert the selected path's stroke into a filled, tapered outline using its
    /// `width_profile` (start/end width multipliers). One undo step.
    OutlineWidthProfile(usize),

    // --- Batch 5: Roughen distort (Object ▸ Path ▸ Roughen) ---
    /// Roughen the selected path: subdivide + jitter anchors by `size` (document
    /// units) at `detail` inserts per segment. One undo step.
    RoughenPath { size: f32, detail: usize },

    // --- Batch 6: Transform Each ---
    /// Transform each selected shape independently around its own bounding-box
    /// centre: translate by `(dx, dy)`, scale by `(scale_x, scale_y)`, rotate by
    /// `angle_deg`, and optionally reflect across the shape's own vertical axis.
    /// Each shape is transformed about its own bbox centre, not the collective
    /// pivot (mirrors Object ▸ Transform ▸ Transform Each in Illustrator).
    TransformEach {
        dx: f32,
        dy: f32,
        scale_x: f32,
        scale_y: f32,
        angle_deg: f32,
        reflect_x: bool,
    },

    // --- Batch 6: Offset Path ---
    /// Expand (`distance > 0`) or contract (`distance < 0`) the selected path's
    /// outline by `distance` document units, replacing the shape with the offset
    /// version. Only operates on closed paths; open paths and non-path shapes are
    /// skipped. One undo step.
    OffsetPath { shape_id: usize, distance: f32 },

    // --- Batch 6: Find / Replace text ---
    /// Toggle the Find / Replace text panel open or closed.
    ToggleFindReplacePanel,
    /// Update the pending find/replace query strings without running the replace.
    SetFindReplaceQuery { find: String, replace: String },
    /// Replace every occurrence of `find` with `replace` across all text shapes
    /// in the document. Counts how many shapes were modified; sets
    /// `last_find_count`. One undo step when at least one match was found.
    FindReplaceText { find: String, replace: String },

    // --- Batch 6: Pathfinder shortcuts (Trim / Merge) ---
    /// Trim: keep the front shape whole and subtract its area from the back shape.
    /// Delegates to `boolean::apply(back, front, BoolOp::Trim)`.
    PathfinderTrim,
    /// Merge: unite shapes of the same fill colour into a single path.
    /// Delegates to `boolean::apply(back, front, BoolOp::Merge)`.
    PathfinderMerge,

    // --- Batch 6: Scatter Brush ---
    /// Configure the scatter brush: copies of symbol `symbol_id` placed at every
    /// `spacing` document-unit interval along a drawn path.
    SetScatterBrush {
        symbol_id: u64,
        spacing: f32,
        size_jitter: f32,
        rotation_jitter: f32,
    },
    /// Clear / deactivate the scatter brush.
    ClearScatterBrush,
    /// Place symbol copies along `path` (document-space points) at the current
    /// scatter brush spacing / jitter settings. No-op when no scatter brush is
    /// configured or the symbol doesn't exist.
    PlaceScatterAlongPath { path: Vec<[f32; 2]> },

    // --- Batch 7: Art Brush ---
    /// Configure the art brush with a symbol and stretch parameters.
    SetArtBrush {
        symbol_id: u64,
        width_scale: f32,
        colorize: ArtBrushColorize,
        flip: bool,
    },
    /// Clear / deactivate the art brush.
    ClearArtBrush,
    /// Paint the art-brush symbol stretched along `path` (document-space points).
    /// Places a new Path shape; no-op if no art brush is configured.
    PaintArtBrushPath { path: Vec<[f32; 2]> },

    // --- Batch 7: Live Corners (polygon corner radius) ---
    /// Set the uniform corner radius on the selected live Polygon.
    SetPolygonCornerRadius(f32),

    // --- Batch 7: Perspective Grid (active plane) ---
    /// Set the active perspective drawing plane (0=left, 1=right, 2=floor).
    SetPerspectivePlane(usize),
    /// Snap the selected shape onto the active perspective plane (stub: records the
    /// plane binding without geometric projection for now).
    SnapToPerspectivePlane,

    // --- Batch 7: Color Guide ---
    /// Set the color-harmony rule and recompute swatches from `key_color`.
    SetColorGuide { rule: ColorHarmonyRule, key_color: [f32; 4] },
    /// Apply swatch at `idx` from the Color Guide as the selected shape's fill.
    ApplyColorGuide(usize),

    // --- Batch 7: Warp Tools (Scallop / Crystallize / Wrinkle) ---
    /// Switch the active warp-tool kind.
    SetWarpToolKind(WarpToolKind),
    /// Configure warp-tool brush parameters.
    SetWarpBrush { size: f32, intensity: f32, detail: f32 },
    /// Apply one warp-tool stroke to the selected path. `center` and `radius` are
    /// in document space. Subdivides edges near the brush, then pushes anchors
    /// outward (Scallop), inward (Crystallize), or randomly ±(Wrinkle).
    ApplyWarpStroke { center: (f32, f32), radius: f32 },


    // --- Batch 8: Image Trace (extended) ---
    /// Configure image trace mode, threshold, and color count, storing them on App.
    SetImageTrace { mode: ImageTraceMode, threshold: f32, colors: u8 },
    /// Run the image trace (stub): generates placeholder traced paths from the
    /// current `image_trace_colors` count and adds them to the document.
    ApplyImageTrace,
    /// Mark the trace result as expanded (destructive, no longer live).
    ExpandImageTrace,

    // --- Batch 8: Opacity Masks ---
    /// Make the top selected shape a luminance mask for the shape below it.
    MakeOpacityMask,
    /// Release the opacity-mask set for every selected shape back to independent shapes.
    ReleaseOpacityMask,
    /// Toggle the invert flag on the masked (non-path) shapes in the selected omask set.
    InvertOpacityMask,

    // --- Batch 8: Symbol extras ---
    /// Detach one placed symbol instance from its symbol definition.
    BreakSymbolLink { shape_idx: usize },
    /// Replace all instances of `sym_id` with independent editable copies.
    ExpandSymbol(u64),

    // --- Batch 8: Graph Tool ---
    /// Set the graph type for the next `ApplyGraph`.
    SetGraphType(GraphType),
    /// Set the graph data (cols, rows, values) for the next `ApplyGraph`.
    SetGraphData { cols: usize, rows: usize, values: Vec<f32> },
    /// Set the column / category labels for the graph.
    SetGraphLabels(Vec<String>),
    /// Generate graph shapes at the given document-space rect.
    ApplyGraph { x: f32, y: f32, width: f32, height: f32 },
    /// Set the base fill color used when generating graph bar shapes.
    SetGraphStyleFill([f32; 4]),
    /// Toggle whether a legend is shown alongside the graph.
    ToggleGraphLegend,

    // --- Batch 9: Type on Path depth ---
    /// Set the per-character offset (in document units) along the path for `text_id`.
    SetTextOnPathOffset { text_id: usize, offset: f32 },
    /// Set whether `text_id` flows above (`true`) or below (`false`) the path.
    SetTextOnPathSide { text_id: usize, above: bool },
    /// Set the glyph-spacing mode for `text_id` on its path.
    SetTextOnPathSpacing { text_id: usize, spacing: TextOnPathSpacing },
    /// Flip `text_id` from above-path to below-path (or vice versa).
    FlipTextOnPath(usize),

    // --- Batch 9: Recolor Artwork depth ---
    /// Replace the entire recolor config.
    SetRecolorConfig(RecolorConfig),
    /// Set the target colour count for recolor (clamped 2..=30).
    SetRecolorColorCount(u8),
    /// Set whether to preserve black fills during recolor.
    SetRecolorPreserveBlack(bool),
    /// Set whether to preserve white fills during recolor.
    SetRecolorPreserveWhite(bool),
    /// Toggle the randomize flag in the recolor config.
    RandomizeRecolor,
    /// Save the current selection's fill colours as a named colour set.
    SaveRecolorSet,
    /// Apply a hue rotation to all selected shapes based on `recolor_color_count`.
    ApplyRecolorToSelected,

    // --- Batch 9: Live Paint depth ---
    /// Fill all closed shapes containing `(x, y)` with `color` (stub).
    LivePaintFill { x: f32, y: f32, color: [f32; 4] },
    /// Stroke all open paths passing near `(x, y)` with `color` and `width` (stub).
    LivePaintStroke { x: f32, y: f32, color: [f32; 4], width: f32 },
    /// Group all selected shapes into a Live Paint group.
    MakeLivePaintGroup,
    /// Release all Live Paint groups (stub).
    ReleaseLivePaintGroup,
    /// Expand all Live Paint groups into independent shapes (stub).
    ExpandLivePaintGroup,
    /// Toggle gap-detection mode for Live Paint.
    SetLivePaintGapDetection(bool),
    /// Set the highlight colour shown over hovered Live Paint regions.
    SetLivePaintHighlightColor([f32; 4]),

    // --- Batch 9: Symbol Sprayer ---
    /// Replace the entire symbol-spray configuration.
    SetSymbolSprayConfig(SymbolSprayConfig),
    /// Spray symbol instances around `center` with the given stylus `pressure`.
    SpraySymbols { center: [f32; 2], pressure: f32 },
    /// Set the spray density (clamped 0..=10).
    SetSymbolSprayDensity(f32),
    /// Set the spray diameter (min 1.0).
    SetSymbolSprayDiameter(f32),
    /// Shift nearby symbol instances toward `delta` direction.
    SymbolShift { center: [f32; 2], delta: [f32; 2] },
    /// Scale nearby symbol instances by `scale` factor.
    SymbolScale { center: [f32; 2], scale: f32 },
    /// Spin nearby symbol instances by `angle` radians.
    SymbolSpin { center: [f32; 2], angle: f32 },
    /// Blend nearby symbol instances' fills toward `color`.
    SymbolStain { center: [f32; 2], color: [f32; 4] },
    /// Reduce nearby symbol instances' opacity by `opacity` factor.
    SymbolScreen { center: [f32; 2], opacity: f32 },

    // --- Batch 10: Gradient Mesh depth ---
    /// Set the mesh row count (clamped 1..=50).
    SetMeshRows(u8),
    /// Set the mesh column count (clamped 1..=50).
    SetMeshCols(u8),
    /// Populate the gradient mesh with rows*cols default MeshPoints.
    CreateMesh,
    /// Set the color of a single mesh control point.
    SetMeshPointColor { idx: usize, color: [f32; 4] },
    /// Set the tension of a single mesh control point (clamped 0..=1).
    SetMeshPointTension { idx: usize, tension: f32 },
    /// Select a mesh control point by index.
    SelectMeshPoint(usize),
    /// Move a mesh control point to a new position.
    MoveMeshPoint { idx: usize, pos: [f32; 2] },
    /// Expand the mesh to the selected shape's bounds (stub).
    ExpandMeshToShape,
    /// Toggle the mesh editing tool on/off.
    ToggleMeshTool,
    /// Release (clear) the gradient mesh back to defaults.
    ReleaseMesh,

    // --- Batch 10: Flare Tool ---
    /// Toggle the flare tool on/off.
    ToggleFlareTool,
    /// Set flare brightness (clamped 0..=100).
    SetFlareBrightness(f32),
    /// Set flare halo size (clamped 0..=100).
    SetFlareHaloSize(f32),
    /// Set flare ray count (clamped 0..=250).
    SetFlareRayCount(u8),
    /// Set flare ray length (clamped 0..=300).
    SetFlareRayLength(f32),
    /// Set flare ring count (clamped 0..=50).
    SetFlareRingCount(u8),
    /// Set flare ring spacing (clamped 0..=300).
    SetFlareRingSpacing(f32),
    /// Set the flare color.
    SetFlareColor([f32; 4]),
    /// Place a flare at `pos`; adds a new shape index to the flare list.
    PlaceFlare([f32; 2]),

    // --- Batch 10: Pattern Brush depth ---
    /// Set the pattern brush scale (clamped 0..=1000).
    SetPatternBrushScale(f32),
    /// Set the pattern brush spacing (clamped 0..=1000).
    SetPatternBrushSpacing(f32),
    /// Set the tile-fit strategy.
    SetPatternBrushFit(PatternBrushFit),
    /// Set whether to flip tiles across the path.
    SetPatternBrushFlipAcross(bool),
    /// Set whether to flip tiles along the path.
    SetPatternBrushFlipAlong(bool),
    /// Save the current pattern brush config under `name`.
    SavePatternBrush { name: String },
    /// Remove a pattern brush from the library by index (no-op if out of bounds).
    DeletePatternBrush(usize),
    /// Apply the pattern brush scale to the selected shape's stroke width (stub).
    ApplyPatternBrushToSelected,

    // --- Batch 10: Variable Fonts ---
    /// Toggle the variable-font panel open/closed.
    ToggleVariableFontPanel,
    /// Add a new font axis to the variable-font config.
    AddFontAxis(FontAxis),
    /// Remove a font axis at `idx` (no-op if out of bounds).
    RemoveFontAxis(usize),
    /// Set the value of a font axis (clamped to the axis min/max).
    SetFontAxisValue { idx: usize, value: f32 },
    /// Set the preview text shown in the variable-font panel.
    SetFontPreviewText(String),
    /// Apply the current axis values to the selected shape (stub).
    ApplyVariableFontToSelected,
    /// Reset all font axes to their midpoints ((min+max)/2).
    ResetFontAxes,

    // --- Batch 11: Pathfinder depth ---
    /// Apply a Pathfinder op to the selection (stub: records last op).
    ApplyPathfinderOp(PathfinderOp),
    /// Set the Pathfinder precision (clamped 0.001..=10.0).
    SetPathfinderPrecision(f32),
    /// Set whether to remove redundant points after a Pathfinder op.
    SetPathfinderRemoveRedundant(bool),
    /// Set whether strokes are divided in Divide mode.
    SetPathfinderDivideStroke(bool),
    /// Re-apply the last recorded Pathfinder op (no-op if none was recorded).
    RepeatPathfinder,

    // --- Batch 11: 3D Extrude depth ---
    /// Toggle the 3D Extrude panel open/closed.
    ToggleExtrudePanel,
    /// Set extrude depth (clamped 0..=2000).
    SetExtrudeDepth(f32),
    /// Set extrude rotation on each axis (each clamped -180..=180).
    SetExtrudeRotation { x: f32, y: f32, z: f32 },
    /// Set extrude perspective (clamped 0..=160).
    SetExtrudePerspective(f32),
    /// Set the surface shading mode.
    SetExtrudeSurface(ExtrudeSurface),
    /// Set the cap style.
    SetExtrudeCapStyle(ExtrudeCapStyle),
    /// Set bevel height (clamped 0..=100).
    SetExtrudeBevelHeight(f32),
    /// Set all lighting parameters (each clamped 0..=100).
    SetExtrudeLighting { intensity: f32, ambient: f32, specular: f32, gloss: f32 },
    /// Set whether to map artwork onto the extruded faces.
    SetExtrudeMapArt(bool),
    /// Apply extrude to the selected shape (stub: records shape index).
    ApplyExtrude,
    /// Expand the extrude effect (stub: clears applied list).
    ExpandExtrude,

    // --- Batch 11: Chart depth ---
    /// Toggle the chart panel open/closed.
    ToggleChartPanel,
    /// Add a dataset to the chart.
    AddChartDataSet(ChartDataSet),
    /// Remove a dataset by index (no-op if out of bounds).
    RemoveChartDataSet(usize),
    /// Set the values for a dataset by index.
    SetChartDataSetValues { idx: usize, values: Vec<f64> },
    /// Set the label for a dataset by index.
    SetChartDataSetLabel { idx: usize, label: String },
    /// Set the category (X-axis) labels.
    SetChartCategoryLabels(Vec<String>),
    /// Set the chart title.
    SetChartTitle(String),
    /// Set whether the legend is visible.
    SetChartShowLegend(bool),
    /// Set whether the grid is visible.
    SetChartShowGrid(bool),
    /// Set column width (clamped 20..=100).
    SetChartColumnWidth(f32),
    /// Set cluster width (clamped 20..=100).
    SetChartClusterWidth(f32),
    /// Set the value-axis min/max range.
    SetChartValueRange { min: Option<f64>, max: Option<f64> },
    /// Generate shapes from the current chart data (stub).
    ApplyChartData,

    // --- Batch 11: Envelope Distort depth ---
    /// Toggle the envelope distort panel open/closed.
    ToggleEnvelopePanel,
    /// Set the warp preset style.
    SetEnvelopeWarpStyle(EnvelopeWarpStyle),
    /// Set the warp axis (true = horizontal).
    SetEnvelopeAxis(bool),
    /// Set the bend amount (clamped -100..=100).
    SetEnvelopeBend(f32),
    /// Set horizontal distortion (clamped -100..=100).
    SetEnvelopeHDistortion(f32),
    /// Set vertical distortion (clamped -100..=100).
    SetEnvelopeVDistortion(f32),
    /// Set the fidelity (clamped 0..=100).
    SetEnvelopeFidelity(f32),
    /// Set the edit mode (Envelope or Contents).
    SetEnvelopeEditMode(EnvelopeEditMode),
    /// Make an envelope with a warp preset (stub: records selected shape index).
    MakeEnvelopeWithWarpPreset,
    /// Make an envelope with a mesh using batch-11 config (stub: records selected shape index).
    MakeEnvelopeWithMeshPreset,
    /// Release the envelope distort from all tracked shapes (stub: clears applied list).
    ReleaseEnvelopeAll,
    /// Expand the envelope distort (stub: clears applied list).
    ExpandEnvelope,

    // --- Wave N: Image Trace (extended panel) ---
    /// Set the image trace mode.
    SetImageTraceMode(ImageTraceMode),
    /// Set the B&W binarisation threshold (0..=255).
    SetImageTraceThreshold(u8),
    /// Set the target colour count (clamped 2..=30).
    SetImageTraceColors(u8),
    /// Set path fidelity (clamped 1..=100).
    SetImageTracePaths(u8),
    /// Set corner fidelity (clamped 0..=100).
    SetImageTraceCorners(u8),
    /// Run the image trace on `image_id`, pushing a new ImageTraceResult.
    RunImageTrace { image_id: usize },
    /// Mark the trace result at `result_index` as expanded.
    ExpandImageTraceResult { result_index: usize },

    // --- Wave N: Perspective Grid (extended config) ---
    /// Set the grid type (One/Two/Three point).
    SetPerspectiveGridType(PerspectiveGridType),
    /// Toggle the grid visible flag.
    TogglePerspectiveGridConfig,
    /// Set pixel-snap on/off.
    SetPerspectiveGridSnap(bool),
    /// Set cell size (clamped 1.0..=500.0).
    SetPerspectiveGridCellSize(f32),
    /// Set grid opacity (clamped 0..=100).
    SetPerspectiveGridOpacity(u8),
    /// Activate one plane by name ("left"/"right"/"floor"); deactivates others.
    SetPerspectiveActivePlaneByName(String),
    /// Move a vanishing point ("left"/"right") to the given coordinates.
    MoveVanishingPoint { which: String, x: f32, y: f32 },

    // --- Wave N: Global Swatches ---
    /// Add a new global swatch.
    AddGlobalSwatch { name: String, color: String, is_spot: bool },
    /// Edit a swatch's color by id.
    EditGlobalSwatch { id: usize, color: String },
    /// Delete a swatch by id.
    DeleteGlobalSwatch(usize),
    /// Create a new swatch group.
    CreateSwatchGroup { name: String },
    /// Add a swatch to a group (no-op if already present).
    AddSwatchToGroup { group_id: usize, swatch_id: usize },
    /// Reorder global_swatches to match the order of ids in the vec.
    ReorderSwatches(Vec<usize>),

    // --- Wave N: Artboards (extended) ---
    /// Add a new Artboard at the given position/size.
    AddArtboardEx { x: f32, y: f32, width: f32, height: f32 },
    /// Delete an Artboard by id.
    DeleteArtboardEx(usize),
    /// Rename an Artboard.
    RenameArtboardEx { id: usize, name: String },
    /// Resize an Artboard (dimensions clamped 1.0..=32000.0).
    ResizeArtboard { id: usize, width: f32, height: f32 },
    /// Set the active Artboard by id.
    SetActiveArtboard(usize),
    /// Reorder artboards_ex to match the given id order.
    ReorderArtboards(Vec<usize>),
    /// Duplicate an Artboard (offset x += width + 20, new id, name "Copy of …").
    DuplicateArtboardEx(usize),
}

/// The single shared application state. Owns the host + document and the panel-
/// facing tool/selection state. Mutated ONLY through [`App::apply`].
pub struct App {
    /// CPU rasterizer + cached bridged `RenderImage` (Contour's tiny-skia path).
    pub host: CanvasHost,
    /// The vector document the host rasterizes; panels read it, `apply` mutates it.
    pub doc: Document,

    /// The active editing tool.
    pub active: Tool,
    /// The canonical multi-selection, in click order (paint-order indices). The
    /// last entry is the *primary* (the shape the inspector edits); the second-to-
    /// last is the *secondary* boolean operand. `selected` / `secondary` below are
    /// kept in sync as derived views so the existing single-select code, overlays,
    /// and boolean front/back semantics keep working unchanged.
    pub selection: Vec<usize>,
    /// The primary selected shape = `selection.last()`. Pure UI state, kept in
    /// sync with `selection` via [`App::sync_legacy_selection`].
    pub selected: Option<usize>,
    /// The secondary selected shape = the second-to-last of `selection`, the
    /// boolean op's other operand (primary = *front*, secondary = *back*). Kept in
    /// sync with `selection`.
    pub secondary: Option<usize>,

    /// The point-type object currently being edited (Type tool), if any. While
    /// `Some`, keystrokes append to / delete from its string. Mirrors the egui
    /// app's `editing_text`.
    pub editing_text: Option<usize>,

    /// Blinking cursor state for the text tool.
    pub cursor_blink_on: bool,
    pub cursor_blink_tick: u32,

    /// Whether the font-family dropdown is open in the type inspector.
    pub font_dropdown_open: bool,

    /// The canvas view transform (pan + zoom). Applied both when displaying the
    /// bridged preview and when mapping window ↔ document coordinates.
    pub view: View,

    /// Default fill / stroke / stroke-width a freshly drag-created shape inherits
    /// (the GPUI host's analogue of the egui app's `self.fill` / `self.stroke` /
    /// `self.stroke_w` tool defaults). Straight sRGB RGBA in 0..1.
    pub default_fill: [f32; 4],
    pub default_stroke: [f32; 4],
    pub default_stroke_w: f32,

    /// Live-shape defaults a freshly drag-created Polygon / Star inherits (the
    /// host's analogue of the egui app's `poly_sides` / `star_points` /
    /// `star_ratio`). The shape stays editable as a `LiveShape` afterward.
    pub poly_sides: u32,
    pub star_points: u32,
    pub star_ratio: f32,
    /// Default em size (document units) a freshly placed point-type object uses.
    pub default_font_size: f32,

    /// Undo / redo over whole-document snapshots. Reuses `contour-app`'s
    /// [`History`] (the same model the egui app drives) — discrete edits
    /// checkpoint via `push`; drags coalesce via `begin` / `commit`.
    pub history: History,
    /// The in-progress Pen-tool path, accumulated click-by-click in document
    /// space until it is closed / finished / cancelled.
    pub pen: PenState,

    // --- Wave 9 state ---
    /// The foreground colour last sampled by the eyedropper (straight sRGB RGBA).
    pub fg_color: [f32; 4],
    /// The tool that was active before the eyedropper was activated, so `PickColor`
    /// can revert to it automatically after a pick.
    pub prev_tool: Tool,
    /// Text-on-path attachments: maps a text shape's paint-order index to the path
    /// index it flows along. Inspector reads this to show Attach / Detach.
    pub text_on_path: std::collections::HashMap<usize, usize>,
    /// A short status message (e.g. "Exported to …") and the instant it was set, so
    /// the toolbar can expire it after 3 s without a timer.
    pub status_message: Option<(String, std::time::Instant)>,

    // --- Wave 11 state ---
    /// Character panel: global font family (applied to new and selected text).
    pub font_family: String,
    /// Character panel: global font weight.
    pub font_weight: FontWeight,
    /// Character panel: letter-spacing (tracking) in document units.
    pub letter_spacing: f32,
    /// Character panel: line-height (leading) multiplier.
    pub line_height: f32,
    /// Character panel: paragraph text alignment.
    pub text_align: TextAlign,
    /// Isolation mode: the group id being isolated, if any.
    pub isolation_group: Option<u64>,
    /// Knife tool: the in-progress drag start/end (window-space; committed on up).
    pub knife_start: Option<(f32, f32)>,

    // --- Wave 12 state: on-canvas transform handles ---
    /// Snapshots of selected shapes at the start of a scale-handle drag, so each
    /// drag step can restore + re-apply without cumulative drift.
    pub xform_snapshot: Vec<(usize, Shape)>,
    /// The pivot point (doc space) for the active scale drag — the opposite handle.
    pub xform_pivot: (f32, f32),
    /// The active scale handle (index into `Handle::ALL`).
    pub xform_handle: Option<Handle>,
    /// The bbox `[x,y,w,h]` at scale-drag start (for scale factor computation).
    pub xform_bbox: [f32; 4],
    /// The cursor position (doc space) at scale-drag start.
    pub xform_start: (f32, f32),

    // --- Wave 8 state ---
    /// Legacy stub symbols list (kept for backward compat with existing code).
    pub symbols: Vec<Symbol>,
    /// Real symbol library (Batch 2).
    pub symbol_lib: crate::symbols::Symbols,
    /// Which gradient stop is currently selected in the stop editor (`None` = none).
    pub selected_gradient_stop: Option<usize>,
    /// Whether the Image Trace inline panel is open.
    pub trace_panel_open: bool,
    /// Image Trace: binarisation threshold (0–255).
    pub trace_threshold: u8,
    /// Image Trace: target colour count (2–32).
    pub trace_colors: u8,
    /// Whether the Recolor Artwork panel is open.
    pub recolor_panel_open: bool,
    /// The fill colour currently selected for editing in the Recolor panel.
    pub recolor_selected_color: Option<[f32; 4]>,
    /// Named artboard entries in document space (Batch 2 upgrade).
    pub artboard_entries: Vec<ArtboardEntry>,
    /// Next id counter for artboards.
    pub artboard_next_id: u64,
    /// The currently active artboard id, if any.
    pub active_artboard: Option<u64>,
    /// Legacy artboard rects (kept for backward-compat canvas rendering).
    pub artboards: Vec<[f32; 4]>,

    // --- Wave 13 state ---
    pub guides_h: Vec<f32>,
    pub guides_v: Vec<f32>,
    pub guides_visible: bool,
    pub grid_snap: bool,
    pub canvas_rotation_deg: f32,
    /// Layer comps: named snapshots of layer visibility. `(name, visibility_map)`.
    pub layer_comps: Vec<(String, std::collections::HashMap<u64, bool>)>,
    /// Pattern library: `(name, tile_w, tile_h)`.
    pub patterns: Vec<(String, f32, f32)>,

    // --- Wave 14 state ---
    /// Blend tool: number of intermediate steps between source and target.
    pub blend_steps: usize,
    /// Perspective grid overlay state.
    pub perspective_grid: Option<PerspectiveGrid>,
    /// Mesh gradient control points for the selected shape: 4×4 grid of (pos, color).
    pub mesh_points: Vec<((f32, f32), [f32; 4])>,

    // --- Batch 3 state ---
    /// Active perspective distort corners (top-left, top-right, bottom-right, bottom-left).
    pub perspective_distort_corners: [[f32; 2]; 4],
    /// Whether a perspective distort is currently being edited.
    pub perspective_distort_active: bool,
    /// Whether the HSL recolor panel is open.
    pub recolor_hsl_open: bool,
    /// HSL recolor: hue shift in degrees.
    pub recolor_hue_shift: f32,
    /// HSL recolor: saturation multiplier.
    pub recolor_sat_scale: f32,
    /// HSL recolor: brightness (lightness) multiplier.
    pub recolor_bright_scale: f32,

    // --- Batch 4: Group blend modes / opacity ---
    /// Per-group blend mode overrides (group_id → mode).
    pub group_blend_modes: std::collections::HashMap<u64, crate::appearance::BlendMode>,
    /// Per-group opacity overrides (group_id → opacity 0..=1).
    pub group_opacities: std::collections::HashMap<u64, f32>,

    // --- Batch 5: Type on a Path (arc-length rendering) ---
    /// Per-text-shape type-on-path parameters (offset / start / flip), keyed by the
    /// same paint-order index used in [`text_on_path`](Self::text_on_path). The
    /// glyph cache of an attached text is baked along its spine via
    /// [`relayout_text_on_path`](Self::relayout_text_on_path); these params drive
    /// the bake. Cleared when the attachment is detached.
    pub text_on_path_params:
        std::collections::HashMap<usize, crate::text_on_path::TextOnPathParams>,

    // --- Batch 6: Find / Replace ---
    /// The pending find string (updated by `SetFindReplaceQuery`).
    pub find_query: String,
    /// The pending replace string (updated by `SetFindReplaceQuery`).
    pub replace_query: String,
    /// Whether the Find / Replace panel is open.
    pub find_replace_open: bool,
    /// How many text shapes were modified by the last `FindReplaceText` action.
    pub last_find_count: usize,

    // --- Batch 6: Scatter Brush ---
    /// Active scatter brush configuration (None = scatter brush inactive).
    pub scatter_brush: Option<ScatterBrushConfig>,

    // --- Batch 7: Art Brush ---
    /// Active art-brush configuration (None = art brush inactive).
    pub art_brush: Option<ArtBrushConfig>,

    // --- Batch 7: Color Guide ---
    /// Color Guide panel state.
    pub color_guide: ColorGuide,

    // --- Batch 7: Warp tools ---
    /// Active warp-tool kind (Scallop / Crystallize / Wrinkle).
    pub warp_tool_kind: WarpToolKind,
    /// Warp-tool brush radius in document units.
    pub warp_brush_size: f32,
    /// Warp-tool strength (0–1).
    pub warp_brush_intensity: f32,
    /// Warp-tool detail (subdivisions per unit, 0.5–10).
    pub warp_detail: f32,

    // --- Batch 7: Perspective grid (extended) ---
    /// Which plane (0=left, 1=right, 2=floor) is active for perspective drawing.
    pub perspective_active_plane: usize,


    // --- Batch 8: Image Trace (extended) ---
    /// The active image trace mode (Color / Grayscale / BlackWhite / Outlined).
    pub image_trace_mode: ImageTraceMode,
    /// Binarisation threshold for Black & White mode (0–255, clamped).
    pub image_trace_threshold: f32,
    /// Target colour count for Color mode (clamped ≥ 2).
    pub image_trace_colors: u8,
    /// Whether the trace result has been expanded to editable paths.
    pub image_trace_expanded: bool,

    // --- Batch 8: Opacity Mask ---
    /// Counter for unique opacity-mask group ids.
    pub omask_id_counter: u64,

    // --- Batch 8: Graph Tool ---
    /// Graph data model (type, cols, rows, values, labels).
    pub graph_data: GraphData,
    /// Base fill color for generated graph bar shapes (straight sRGB RGBA).
    pub graph_style_fill: [f32; 4],
    /// Whether a legend should be shown alongside the graph.
    pub graph_show_legend: bool,

    // --- Batch 9: Type on Path depth ---
    /// Per-text per-path character offset (document units), keyed by text shape index.
    pub text_on_path_offsets: std::collections::HashMap<usize, f32>,
    /// Per-text "above path" flag; `true` means above, `false` means below.
    pub text_on_path_above: std::collections::HashMap<usize, bool>,
    /// Per-text glyph-spacing mode on a path.
    pub text_on_path_spacing: std::collections::HashMap<usize, TextOnPathSpacing>,

    // --- Batch 9: Recolor Artwork depth ---
    /// Advanced recolor configuration (harmony rule, preserve flags, randomize).
    pub recolor_config: RecolorConfig,
    /// How many distinct colours the Recolor panel targets (clamped 2..=30).
    pub recolor_color_count: u8,
    /// Previously saved colour sets (up to 10 fills each).
    pub recolor_history: Vec<Vec<[f32; 4]>>,

    // --- Batch 9: Live Paint depth ---
    /// Whether Live Paint gap-detection is active.
    pub live_paint_gap_detection: bool,
    /// Highlight colour shown over hovered Live Paint regions.
    pub live_paint_highlight_color: [f32; 4],
    /// Group ids that have been designated as Live Paint groups.
    pub live_paint_group_ids: Vec<u64>,

    // --- Batch 9: Symbol Sprayer ---
    /// Configuration for the Symbol Sprayer tool.
    pub symbol_spray_config: SymbolSprayConfig,

    // --- Batch 10: Gradient Mesh depth ---
    /// Gradient mesh configuration for the selected shape.
    pub gradient_mesh: GradientMeshConfig,
    /// Whether the mesh editing tool is currently active.
    pub mesh_tool_active: bool,
    /// Index of the currently selected mesh control point.
    pub selected_mesh_point: Option<usize>,

    // --- Batch 10: Flare Tool ---
    /// Configuration for the active flare effect.
    pub flare_config: FlareConfig,
    /// Whether the flare tool is currently active.
    pub flare_tool_active: bool,
    /// Shape indices that represent placed flares.
    pub flare_shapes: Vec<usize>,

    // --- Batch 10: Pattern Brush depth ---
    /// Configuration for the pattern brush.
    pub pattern_brush_config: PatternBrushConfig,
    /// Library of saved pattern brush names.
    pub pattern_brush_library: Vec<String>,

    // --- Batch 10: Variable Fonts ---
    /// Variable-font axis configuration.
    pub variable_font_config: VariableFontConfig,
    /// Whether the variable-font panel is open.
    pub variable_font_panel_open: bool,

    // --- Batch 11: Pathfinder depth ---
    /// The last Pathfinder op that was applied (for RepeatPathfinder).
    pub last_pathfinder_op: Option<PathfinderOp>,
    /// Precision for Pathfinder geometry operations (0.001..=10.0).
    pub pathfinder_precision: f32,
    /// Whether to remove redundant points after a Pathfinder op.
    pub pathfinder_remove_redundant: bool,
    /// Whether strokes are divided in Divide mode.
    pub pathfinder_divide_stroke: bool,

    // --- Batch 11: 3D Extrude depth ---
    /// Configuration for the 3D Extrude & Bevel effect.
    pub extrude_config: ExtrudeConfig,
    /// Whether the 3D Extrude panel is open.
    pub extrude_panel_open: bool,
    /// Indices of shapes that have had Extrude applied (stub tracking).
    pub extrude_applied_shapes: Vec<usize>,

    // --- Batch 11: Chart depth ---
    /// Configuration for the chart tool.
    pub chart_config: ChartConfig,
    /// Whether the chart panel is open.
    pub chart_panel_open: bool,

    // --- Batch 11: Envelope Distort depth ---
    /// Configuration for the Envelope Distort warp.
    pub envelope_config: EnvelopeConfig,
    /// Indices of shapes that have had an envelope applied (stub tracking).
    pub envelope_applied_shapes: Vec<usize>,
    /// Whether the envelope distort panel is open.
    pub envelope_panel_open: bool,

    // --- Wave N: Image Trace (extended panel) ---
    /// Extended image trace configuration (panel-facing).
    pub image_trace_config: ImageTraceConfig,
    /// Accumulated trace results (live until expanded).
    pub image_trace_results: Vec<ImageTraceResult>,
    /// Whether the extended image trace panel is open.
    pub image_trace_panel_open: bool,

    // --- Wave N: Perspective Grid (extended config) ---
    /// Extended perspective grid configuration.
    pub perspective_grid_config: PerspectiveGridConfig,

    // --- Wave N: Global Swatches ---
    /// Global and spot swatches.
    pub global_swatches: Vec<GlobalSwatch>,
    /// Named swatch groups.
    pub swatch_groups: Vec<SwatchGroup>,
    /// Counter for assigning stable swatch ids.
    pub swatch_counter: usize,
    /// Counter for assigning stable swatch group ids.
    pub swatch_group_counter: usize,

    // --- Wave N: Artboards (extended) ---
    /// Extended artboard list.
    pub artboards_ex: Vec<Artboard>,
    /// The id of the currently active artboard (None = no active artboard).
    pub active_artboard_ex: Option<usize>,
    /// Counter for assigning stable artboard ids.
    pub artboard_counter: usize,
}

impl App {
    /// Build the shared state: seed a small sample document so the preview and
    /// the Layers panel have content, boot the host, and default the tool to
    /// Select (the egui app's resting tool).
    pub fn new() -> Self {
        let mut doc = sample_document();
        // Seed default graphic styles
        {
            use crate::appearance::{Appearance, Effect, Fill, Stroke};
            doc.graphic_styles.add("Bold Stroke", Appearance {
                fills: vec![],
                strokes: vec![Stroke::solid([0.0, 0.0, 0.0, 1.0], 4.0)],
                effects: vec![],
            });
            doc.graphic_styles.add("Drop Shadow", Appearance {
                fills: vec![Fill::solid([0.2, 0.4, 0.8, 1.0])],
                strokes: vec![],
                effects: vec![Effect::drop_shadow()],
            });
            doc.graphic_styles.add("Neon Glow", Appearance {
                fills: vec![Fill::solid([0.05, 0.05, 0.15, 1.0])],
                strokes: vec![Stroke::solid([0.0, 1.0, 0.9, 0.9], 2.0)],
                effects: vec![],
            });
            doc.graphic_styles.add("Woodcut", Appearance {
                fills: vec![Fill::solid([0.3, 0.2, 0.1, 0.5])],
                strokes: vec![Stroke::solid([0.0, 0.0, 0.0, 1.0], 4.0)],
                effects: vec![],
            });
            doc.graphic_styles.add("Wireframe", Appearance {
                fills: vec![],
                strokes: vec![Stroke::solid([0.3, 0.3, 0.3, 1.0], 1.0)],
                effects: vec![],
            });
        }
        let host = CanvasHost::new(&doc);
        Self {
            host,
            doc,
            active: Tool::Select,
            selection: Vec::new(),
            selected: None,
            secondary: None,
            editing_text: None,
            cursor_blink_on: false,
            cursor_blink_tick: 0,
            font_dropdown_open: false,
            view: View::default(),
            default_fill: [0.20, 0.55, 0.90, 1.0],
            default_stroke: [0.10, 0.20, 0.35, 1.0],
            default_stroke_w: 2.0,
            poly_sides: 6,
            star_points: 5,
            star_ratio: 0.5,
            default_font_size: 72.0,
            history: History::new(),
            pen: PenState::default(),
            fg_color: [0.0, 0.0, 0.0, 1.0],
            prev_tool: Tool::Select,
            text_on_path: std::collections::HashMap::new(),
            status_message: None,
            font_family: "Helvetica".to_string(),
            font_weight: FontWeight::Regular,
            letter_spacing: 0.0,
            line_height: 1.2,
            text_align: TextAlign::Left,
            isolation_group: None,
            knife_start: None,
            xform_snapshot: Vec::new(),
            xform_pivot: (0.0, 0.0),
            xform_handle: None,
            xform_bbox: [0.0, 0.0, 0.0, 0.0],
            xform_start: (0.0, 0.0),
            symbols: Vec::new(),
            symbol_lib: crate::symbols::Symbols::default(),
            selected_gradient_stop: None,
            trace_panel_open: false,
            trace_threshold: 128,
            trace_colors: 8,
            recolor_panel_open: false,
            recolor_selected_color: None,
            artboard_entries: Vec::new(),
            artboard_next_id: 0,
            active_artboard: None,
            artboards: Vec::new(),
            guides_h: Vec::new(),
            guides_v: Vec::new(),
            guides_visible: true,
            grid_snap: false,
            canvas_rotation_deg: 0.0,
            layer_comps: Vec::new(),
            patterns: Vec::new(),
            blend_steps: 5,
            perspective_grid: None,
            mesh_points: Vec::new(),
            perspective_distort_corners: [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]],
            perspective_distort_active: false,
            recolor_hsl_open: false,
            recolor_hue_shift: 0.0,
            recolor_sat_scale: 1.0,
            recolor_bright_scale: 1.0,
            group_blend_modes: std::collections::HashMap::new(),
            group_opacities: std::collections::HashMap::new(),
            text_on_path_params: std::collections::HashMap::new(),
            find_query: String::new(),
            replace_query: String::new(),
            find_replace_open: false,
            last_find_count: 0,
            scatter_brush: None,
            art_brush: None,
            color_guide: ColorGuide::default(),
            warp_tool_kind: WarpToolKind::default(),
            warp_brush_size: 30.0,
            warp_brush_intensity: 0.5,
            warp_detail: 1.0,
            perspective_active_plane: 0,
            // Batch 8
            image_trace_mode: ImageTraceMode::default(),
            image_trace_threshold: 128.0,
            image_trace_colors: 6,
            image_trace_expanded: false,
            omask_id_counter: 1,
            graph_data: GraphData::default(),
            graph_style_fill: [0.2, 0.5, 0.9, 1.0],
            graph_show_legend: false,
            // Batch 9
            text_on_path_offsets: std::collections::HashMap::new(),
            text_on_path_above: std::collections::HashMap::new(),
            text_on_path_spacing: std::collections::HashMap::new(),
            recolor_config: RecolorConfig::default(),
            recolor_color_count: 5,
            recolor_history: Vec::new(),
            live_paint_gap_detection: false,
            live_paint_highlight_color: [1.0, 0.5, 0.0, 1.0],
            live_paint_group_ids: Vec::new(),
            symbol_spray_config: SymbolSprayConfig::default(),
            // Batch 10
            gradient_mesh: GradientMeshConfig::default(),
            mesh_tool_active: false,
            selected_mesh_point: None,
            flare_config: FlareConfig::default(),
            flare_tool_active: false,
            flare_shapes: Vec::new(),
            pattern_brush_config: PatternBrushConfig::default(),
            pattern_brush_library: Vec::new(),
            variable_font_config: VariableFontConfig::default(),
            variable_font_panel_open: false,
            // Batch 11
            last_pathfinder_op: None,
            pathfinder_precision: 0.5,
            pathfinder_remove_redundant: true,
            pathfinder_divide_stroke: false,
            extrude_config: ExtrudeConfig::default(),
            extrude_panel_open: false,
            extrude_applied_shapes: Vec::new(),
            chart_config: ChartConfig::default(),
            chart_panel_open: false,
            envelope_config: EnvelopeConfig::default(),
            envelope_applied_shapes: Vec::new(),
            envelope_panel_open: false,
            // Wave N
            image_trace_config: ImageTraceConfig::new(),
            image_trace_results: Vec::new(),
            image_trace_panel_open: false,
            perspective_grid_config: PerspectiveGridConfig::new(),
            global_swatches: Vec::new(),
            swatch_groups: Vec::new(),
            swatch_counter: 0,
            swatch_group_counter: 0,
            artboards_ex: Vec::new(),
            active_artboard_ex: None,
            artboard_counter: 0,
        }
    }

    /// Record the current document as an undo checkpoint *before* a discrete
    /// (non-drag) mutation. The single helper every `apply` arm that edits the
    /// document calls first (mirrors the egui app's `checkpoint`).
    fn checkpoint(&mut self) {
        self.history.push(self.doc.clone());
    }

    /// Recompute the derived single-select views (`selected` = primary =
    /// `selection.last()`, `secondary` = the second-to-last operand) from the
    /// canonical `selection` set. Called after every selection mutation so the
    /// inspector, overlays, and boolean front/back semantics stay consistent.
    fn sync_legacy_selection(&mut self) {
        let n = self.selection.len();
        self.selected = self.selection.last().copied();
        self.secondary = if n >= 2 {
            Some(self.selection[n - 2])
        } else {
            None
        };
    }

    /// Replace the whole selection with a single shape (the common case: a plain
    /// click or a freshly-created shape becoming the selection).
    fn select_single(&mut self, i: usize) {
        self.selection.clear();
        self.selection.push(i);
        self.sync_legacy_selection();
    }

    /// Clear the entire selection.
    fn select_clear(&mut self) {
        self.selection.clear();
        self.sync_legacy_selection();
    }

    /// Re-bake the glyph cache of an attached text object so its glyphs ride its
    /// spine path (arc-length placement). No-op if `text_id` is not attached, the
    /// spine is missing, or the shape is not a text object. Called whenever the
    /// attachment, its params, or the spine geometry changes.
    pub fn relayout_text_on_path(&mut self, text_id: usize) {
        let Some(&path_id) = self.text_on_path.get(&text_id) else {
            return;
        };
        if text_id >= self.doc.shapes.len() || path_id >= self.doc.shapes.len() {
            return;
        }
        // Resolve the spine geometry from the path shape.
        let (bez, closed) = crate::text_on_path::spine_bezpath(&self.doc.shapes[path_id]);
        let params_on_path = self
            .text_on_path_params
            .get(&text_id)
            .copied()
            .unwrap_or_default();
        // Read the text params + bake.
        if let crate::document::Shape::Text { params, glyphs, .. } =
            &mut self.doc.shapes[text_id]
        {
            let warped = crate::text_on_path::layout_on_path(
                params,
                &bez,
                closed,
                params_on_path,
            );
            if !warped.is_empty() {
                *glyphs = warped;
            }
        }
    }

    /// Tick the text cursor blink state. Call once per render frame when editing text.
    /// Returns true if the blink phase changed (caller should request another frame).
    pub fn tick_cursor(&mut self) -> bool {
        self.cursor_blink_tick += 1;
        if self.cursor_blink_tick >= 30 {
            self.cursor_blink_tick = 0;
            self.cursor_blink_on = !self.cursor_blink_on;
            true
        } else {
            false
        }
    }

    /// Apply a panel-emitted [`Action`]. This is the ONLY place `App` state is
    /// mutated. Any action that changes the rasterized pixels marks the host
    /// dirty so the next `host.image()` re-rasterizes; pure UI state
    /// (tool/selection) does not touch the host.
    pub fn apply(&mut self, action: Action) {
        match action {
            Action::SetTool(t) => {
                // Leaving the Pen tool abandons any half-drawn path so stale
                // anchors never linger on another tool (mirrors the egui app).
                if t != Tool::Pen && !self.pen.is_empty() {
                    self.pen.clear();
                    self.host.mark_dirty();
                }
                // Leaving the Type tool commits / discards any in-progress edit.
                if t != Tool::Type && self.editing_text.is_some() {
                    self.finish_text_edit();
                }
                // Remember the pre-eyedropper tool so PickColor can revert to it.
                if t == Tool::Eyedropper && self.active != Tool::Eyedropper {
                    self.prev_tool = self.active;
                }
                self.active = t;
            }

            Action::ToggleShapeVisible(i) => {
                if i < self.doc.shapes.len() {
                    self.checkpoint();
                    self.doc.shapes[i].toggle_visible();
                    self.host.mark_dirty();
                }
            }
            Action::SelectShape(i) => {
                if i < self.doc.shapes.len() {
                    self.select_single(i);
                }
            }

            Action::HitTestSelect { x, y } => {
                match self.hit_test_topmost(x, y) {
                    Some(i) => self.select_single(i),
                    None => self.select_clear(),
                }
                // Selection itself doesn't change the rasterized artboard pixels —
                // the selection ring is drawn as a GPUI overlay, not baked in — so
                // the host stays clean.
            }

            Action::CreateShape { tool, a, b } => {
                // Each tool maps its two drag corners to geometry differently
                // (mirrors the egui app's `shape_from_drag`): Rect / Ellipse use
                // the normalized bounding box, Line uses the raw endpoints, and
                // Polygon / Star treat `a` as the centre and |a→b| as the radius.
                let shape = match tool {
                    Tool::Line => {
                        if (b[0] - a[0]).abs() < 1.0 && (b[1] - a[1]).abs() < 1.0 {
                            return;
                        }
                        self.line_shape((a[0], a[1]), (b[0], b[1]))
                    }
                    Tool::Polygon | Tool::Star => {
                        let radius = ((b[0] - a[0]).powi(2) + (b[1] - a[1]).powi(2)).sqrt();
                        if radius < 1.0 {
                            return;
                        }
                        let live = if tool == Tool::Polygon {
                            LiveShape::Polygon {
                                sides: self.poly_sides,
                                radius,
                                corner_radius: 0.0,
                            }
                        } else {
                            LiveShape::Star {
                                points: self.star_points,
                                radius,
                                inner_ratio: self.star_ratio,
                            }
                        };
                        self.live_shape((a[0], a[1]), live)
                    }
                    _ => {
                        let x = a[0].min(b[0]);
                        let y = a[1].min(b[1]);
                        let w = (b[0] - a[0]).abs();
                        let h = (b[1] - a[1]).abs();
                        // Drop a degenerate (essentially zero-area) drag.
                        if w < 1.0 && h < 1.0 {
                            return;
                        }
                        let rect = [x, y, w, h];
                        match tool {
                            Tool::Ellipse => Shape::ellipse(
                                rect,
                                self.default_fill,
                                self.default_stroke,
                                self.default_stroke_w,
                            ),
                            // Default (and Rect) → a rectangle.
                            _ => Shape::rect(
                                rect,
                                self.default_fill,
                                self.default_stroke,
                                self.default_stroke_w,
                            ),
                        }
                    }
                };
                self.checkpoint();
                self.doc.shapes.push(shape);
                self.select_single(self.doc.shapes.len() - 1);
                self.host.mark_dirty();
            }

            Action::PlaceText { x, y } => {
                // Open one coalescing group for the whole place→type→finish session
                // so it collapses to a single undo entry (idempotent with the
                // per-keystroke `begin` in `edit_text_string`).
                self.history.begin(&self.doc);
                let params = TextParams {
                    text: String::new(),
                    font_size: self.default_font_size,
                    ..Default::default()
                };
                let glyphs = crate::text::layout(&params, (x, y)).0;
                let shape = Shape::Text {
                    params,
                    origin: (x, y),
                    glyphs,
                    fill: self.default_fill,
                    fill_gradient: None,
                    stroke: self.default_stroke,
                    // New type defaults to fill only (Illustrator's default), so
                    // glyph counters read cleanly.
                    stroke_w: 0.0,
                    stroke_style: Default::default(),
                    appearance: None,
                    visible: true,
                    group: None,
                    clip: None,
                    mask: false,
                    omask: None,
                    omask_path: false,
                    omask_invert: false,
                    blend: None,
                    blend_step: false,
                    name: None,
                    locked: false,
                    layer_color: None,
                    envelope_mesh: None,
                };
                self.doc.shapes.push(shape);
                let idx = self.doc.shapes.len() - 1;
                self.select_single(idx);
                self.editing_text = Some(idx);
                self.host.mark_dirty();
            }
            Action::EditText(i) => {
                if self.doc.shapes.get(i).is_some_and(|s| s.text_params().is_some()) {
                    self.history.begin(&self.doc);
                    self.select_single(i);
                    self.editing_text = Some(i);
                }
            }
            Action::TypeChar(c) => self.edit_text_string(|s| s.push(c)),
            Action::TypeBackspace => {
                self.edit_text_string(|s| {
                    s.pop();
                });
            }
            Action::FinishText => self.finish_text_edit(),

            Action::Boolean(op) => self.apply_boolean(op),

            Action::AddToSelection { x, y } => {
                // Shift+click toggles a shape in/out of the selection set. A newly-
                // added shape moves to the end (becoming the primary); re-clicking a
                // selected shape removes it. A miss leaves the set unchanged.
                if let Some(i) = self.hit_test_topmost(x, y) {
                    if let Some(pos) = self.selection.iter().position(|&s| s == i) {
                        self.selection.remove(pos);
                    } else {
                        self.selection.push(i);
                    }
                    self.sync_legacy_selection();
                }
            }

            Action::MarqueeSelect { rect } => {
                let marquee = CoreRect::new(rect[0], rect[1], rect[2], rect[3]);
                self.selection = self
                    .doc
                    .shapes
                    .iter()
                    .enumerate()
                    .filter(|(_, s)| s.selectable() && bounds_intersect(s, &marquee))
                    .map(|(i, _)| i)
                    .collect();
                self.sync_legacy_selection();
            }

            Action::SetLiveShape(new) => {
                if let Some(i) = self.selected {
                    // Mirror the working defaults so the next new shape inherits them.
                    match new {
                        LiveShape::Polygon { sides, .. } => self.poly_sides = sides,
                        LiveShape::Star {
                            points,
                            inner_ratio,
                            ..
                        } => {
                            self.star_points = points;
                            self.star_ratio = inner_ratio;
                        }
                    }
                    if self.doc.shapes.get(i).and_then(|s| s.live_shape()).is_some() {
                        self.checkpoint();
                        if self.doc.shapes[i].set_live_shape(new) {
                            self.host.mark_dirty();
                        }
                    }
                }
            }

            Action::SetTextSize(size) => {
                self.edit_text_params(|p| p.font_size = size.clamp(1.0, 2000.0));
            }
            Action::SetTextAlign(a) => {
                self.edit_text_params(|p| p.align = a);
            }
            Action::SetTextFont(fam) => {
                self.edit_text_params(|p| p.font_family = fam);
                self.font_dropdown_open = false;
            }
            Action::ToggleFontDropdown => {
                self.font_dropdown_open = !self.font_dropdown_open;
            }

            Action::AlignSelection(op) => self.align_selection(op),
            Action::DistributeSelection(op) => self.distribute_selection(op),

            Action::PanBy { dx, dy } => {
                self.view.offset.0 += dx;
                self.view.offset.1 += dy;
            }

            Action::ZoomBy {
                factor,
                anchor,
                viewport,
            } => {
                let old = self.view.scale;
                let new = (old * factor).clamp(View::MIN_SCALE, View::MAX_SCALE);
                if new == old {
                    return;
                }
                // Hold the anchor point fixed on screen: the viewport-local point
                // under the cursor (or the viewport centre) must map to the same
                // document point before and after the zoom. With
                // `local = offset + doc_local * scale`, keeping `local` fixed gives
                // `offset' = anchor - (anchor - offset) * new/old`.
                let (ax, ay) = anchor.unwrap_or((viewport.0 * 0.5, viewport.1 * 0.5));
                let ratio = new / old;
                self.view.offset.0 = ax - (ax - self.view.offset.0) * ratio;
                self.view.offset.1 = ay - (ay - self.view.offset.1) * ratio;
                self.view.scale = new;
            }

            Action::ResetView => {
                self.view = View::default();
            }

            Action::SetFillColor(c) => {
                if self.selected_shape().is_some() {
                    self.checkpoint();
                    self.selected_shape_mut().unwrap().set_fill_color(c);
                    self.host.mark_dirty();
                }
            }
            Action::SetStrokeColor(c) => {
                if self.selected_shape().is_some() {
                    self.checkpoint();
                    self.selected_shape_mut().unwrap().set_stroke_color(c);
                    self.host.mark_dirty();
                }
            }
            Action::SetStrokeWidth(w) => {
                if self.selected_shape().is_some() {
                    self.checkpoint();
                    self.selected_shape_mut().unwrap().set_stroke_width(w.max(0.0));
                    self.host.mark_dirty();
                }
            }
            Action::SetOpacity(o) => {
                let o = o.clamp(0.0, 1.0);
                if let Some(mut c) = self.selected_shape().and_then(|s| s.fill_color()) {
                    self.checkpoint();
                    // Write the fill-alpha channel (see `Action::SetOpacity` doc).
                    c[3] = o;
                    self.selected_shape_mut().unwrap().set_fill_color(c);
                    self.host.mark_dirty();
                }
            }

            Action::PenAddAnchor { x, y } => {
                // Click near the first anchor closes + commits the path.
                if self.pen.points.len() >= 2 {
                    let (fx, fy) = self.pen.points[0];
                    if (x - fx).hypot(y - fy) <= PEN_CLOSE_TOL {
                        self.commit_pen(true);
                        return;
                    }
                }
                self.pen.points.push((x, y));
                self.pen.handles.push((0.0, 0.0));
            }
            Action::PenSetHandle { x, y } => {
                if let Some(&(ax, ay)) = self.pen.points.last() {
                    if let Some(h) = self.pen.handles.last_mut() {
                        *h = (x - ax, y - ay);
                    }
                }
            }
            Action::PenFinish { closed } => self.commit_pen(closed),
            Action::PenCancel => self.pen.clear(),

            Action::MoveAnchor { which, x, y } => {
                if let Some(i) = self.selected {
                    if let Some(s) = self.doc.shapes.get_mut(i) {
                        if s.set_anchor(0, which, x, y) {
                            self.host.mark_dirty();
                        }
                    }
                }
            }
            Action::MoveHandle { which, x, y } => {
                if let Some(i) = self.selected {
                    if let Some(s) = self.doc.shapes.get_mut(i) {
                        if s.set_handle(0, which, x, y) {
                            self.host.mark_dirty();
                        }
                    }
                }
            }

            Action::DeleteSelected => {
                if !self.selection.is_empty() {
                    self.checkpoint();
                    // Remove highest index first so the lower ones stay valid.
                    let mut idxs: Vec<usize> = self
                        .selection
                        .iter()
                        .copied()
                        .filter(|&i| i < self.doc.shapes.len())
                        .collect();
                    idxs.sort_unstable_by(|a, b| b.cmp(a));
                    idxs.dedup();
                    for i in idxs {
                        self.doc.shapes.remove(i);
                    }
                    self.select_clear();
                    self.host.mark_dirty();
                }
            }

            Action::BeginInteraction => self.history.begin(&self.doc),
            Action::EndInteraction => {
                self.history.commit(&self.doc);
            }
            Action::Undo => {
                if let Some(prev) = self.history.undo(&self.doc) {
                    self.doc = prev;
                    self.clamp_selection();
                    self.host.mark_dirty();
                }
            }
            Action::Redo => {
                if let Some(next) = self.history.redo(&self.doc) {
                    self.doc = next;
                    self.clamp_selection();
                    self.host.mark_dirty();
                }
            }

            a => self.apply_waves(a),
        }
    }
}


impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

