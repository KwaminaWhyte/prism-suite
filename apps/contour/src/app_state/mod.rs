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

    /// Commit the in-progress pen path as a `Shape::Path` (one undo step), or
    /// discard it when it has fewer than two anchors. Mirrors the egui app's
    /// `commit_pen`: checkpoint, drain the pen buffers into a path, select it.
    fn commit_pen(&mut self, closed: bool) {
        if self.pen.points.len() >= 2 {
            self.checkpoint();
            let points = std::mem::take(&mut self.pen.points);
            let handles = std::mem::take(&mut self.pen.handles);
            let shape = Shape::path(
                points,
                handles,
                closed,
                self.default_fill,
                self.default_stroke,
                self.default_stroke_w,
            );
            self.doc.shapes.push(shape);
            self.select_single(self.doc.shapes.len() - 1);
            self.host.mark_dirty();
        }
        self.pen.clear();
    }

    /// Build a stroked-only [`Shape::Line`] between two document-space endpoints,
    /// inheriting the host's default stroke (mirrors the egui `shape_from_drag`
    /// Line arm — a line has no fill, and its width is clamped ≥ 1).
    fn line_shape(&self, p0: (f32, f32), p1: (f32, f32)) -> Shape {
        Shape::Line {
            p0,
            p1,
            stroke: self.default_stroke,
            stroke_w: self.default_stroke_w.max(1.0),
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
        }
    }

    /// Build a closed live-shape [`Shape::Path`] (Polygon / Star) centred at
    /// `center`, generating its outline and stashing the `LiveShape` params so it
    /// stays parametrically editable (mirrors the egui `live_shape_at`).
    fn live_shape(&self, center: (f32, f32), live: LiveShape) -> Shape {
        let (points, handles) = live.outline(center);
        Shape::Path {
            points,
            closed: true,
            fill: self.default_fill,
            fill_gradient: None,
            stroke: self.default_stroke,
            stroke_w: self.default_stroke_w,
            stroke_style: Default::default(),
            handles,
            live: Some(live),
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
        }
    }

    /// Mutate the in-edit text object's string through `f` and re-lay-out its
    /// glyphs. The whole edit session is one undo step: a checkpoint is taken on
    /// the first keystroke (when `history.begin` opens a coalescing group) and
    /// committed on `FinishText`. No-op when nothing is being edited.
    fn edit_text_string(&mut self, f: impl FnOnce(&mut String)) {
        let Some(idx) = self.editing_text else { return };
        // Coalesce the whole typing session into one undo entry.
        self.history.begin(&self.doc);
        if let Some(shape) = self.doc.shapes.get(idx) {
            if let Some(params) = shape.text_params() {
                let mut new = params.clone();
                f(&mut new.text);
                self.doc.shapes[idx].set_text_params(new);
                self.host.mark_dirty();
            }
        }
    }

    /// Leave text-edit mode. A text object left with empty/whitespace-only text
    /// (placed but never typed into) is removed so the canvas isn't littered with
    /// invisible objects (mirrors the egui `end_text_edit`). Finalizes the typing
    /// session's coalesced undo entry.
    fn finish_text_edit(&mut self) {
        let Some(idx) = self.editing_text.take() else {
            return;
        };
        let empty = self
            .doc
            .shapes
            .get(idx)
            .and_then(|s| s.text_params())
            .map(|p| p.text.trim().is_empty())
            .unwrap_or(true);
        if empty {
            if idx < self.doc.shapes.len() {
                self.doc.shapes.remove(idx);
            }
            self.selection.retain(|&i| i != idx);
            self.sync_legacy_selection();
            self.host.mark_dirty();
        }
        // Close the coalesced typing group (drops it if nothing changed).
        self.history.commit(&self.doc);
    }

    /// Edit the **primary** text object's parameters through `f` and re-lay-out
    /// its glyph cache. One undo step per panel edit (mirrors the egui
    /// `type_section`). No-op unless the primary selection is a text object.
    fn edit_text_params(&mut self, f: impl FnOnce(&mut TextParams)) {
        let Some(idx) = self.selected else { return };
        let Some(mut params) = self.doc.shapes.get(idx).and_then(|s| s.text_params().cloned())
        else {
            return;
        };
        f(&mut params);
        self.checkpoint();
        if self.doc.shapes[idx].set_text_params(params) {
            self.host.mark_dirty();
        }
    }

    /// Align every selected shape's matching feature to the selection's combined
    /// bounding box, as one undo step (mirrors the egui `align_selection`, frame =
    /// selection bounds). Needs ≥2 selected shapes with geometry.
    fn align_selection(&mut self, op: Align) {
        let sel = self.selection_bounds();
        if sel.len() < 2 {
            return;
        }
        let boxes: Vec<CoreRect> = sel.iter().map(|&(_, r)| r).collect();
        let Some(frame) = align::union_bounds(&boxes) else {
            return;
        };
        let deltas = align::align_deltas(&boxes, op, frame);
        self.checkpoint();
        let mut moved = false;
        for (&(i, _), (dx, dy)) in sel.iter().zip(deltas) {
            if (dx != 0.0 || dy != 0.0) && i < self.doc.shapes.len() {
                self.doc.shapes[i].translate(dx, dy);
                moved = true;
            }
        }
        if moved {
            self.host.mark_dirty();
        }
    }

    /// Evenly distribute the selected shapes' chosen feature / gap, as one undo
    /// step (mirrors the egui `distribute_selection`). Needs ≥3 selected shapes.
    fn distribute_selection(&mut self, op: Distribute) {
        let sel = self.selection_bounds();
        if sel.len() < 3 {
            return;
        }
        let boxes: Vec<CoreRect> = sel.iter().map(|&(_, r)| r).collect();
        let deltas = align::distribute_deltas(&boxes, op);
        self.checkpoint();
        let mut moved = false;
        for (&(i, _), (dx, dy)) in sel.iter().zip(deltas) {
            if (dx != 0.0 || dy != 0.0) && i < self.doc.shapes.len() {
                self.doc.shapes[i].translate(dx, dy);
                moved = true;
            }
        }
        if moved {
            self.host.mark_dirty();
        }
    }

    /// Each selected shape's `(index, bounds)` in selection order, skipping shapes
    /// with no geometry. The input the align / distribute deltas operate over.
    fn selection_bounds(&self) -> Vec<(usize, CoreRect)> {
        self.selection
            .iter()
            .filter_map(|&i| self.doc.shapes.get(i).and_then(|s| s.bounds()).map(|r| (i, r)))
            .collect()
    }

    /// Whether the selection can be aligned (≥2 shapes with geometry).
    pub fn can_align(&self) -> bool {
        self.selection_bounds().len() >= 2
    }

    /// Whether the selection can be distributed (≥3 shapes with geometry).
    pub fn can_distribute(&self) -> bool {
        self.selection_bounds().len() >= 3
    }

    /// The combined (union) bounding box of every selected shape in document
    /// space `[x, y, w, h]`, or `None` when nothing with geometry is selected.
    /// Drives the multi-select group ring overlay.
    pub fn selection_bbox(&self) -> Option<[f32; 4]> {
        let boxes: Vec<CoreRect> = self.selection_bounds().iter().map(|&(_, r)| r).collect();
        align::union_bounds(&boxes).map(|r| [r.x, r.y, r.w, r.h])
    }

    /// The bounds of every selected shape (document space), for drawing one ring
    /// per member of a multi-selection.
    pub fn selected_member_bounds(&self) -> Vec<[f32; 4]> {
        self.selection_bounds()
            .iter()
            .map(|&(_, r)| [r.x, r.y, r.w, r.h])
            .collect()
    }

    /// Run a pathfinder boolean `op` on the two selected shapes (primary = front,
    /// secondary = back), replacing both with the result batch. One undo step.
    /// No-op unless exactly two distinct shapes are selected and the op yields
    /// geometry. Reuses `contour-app`'s `boolean::apply` (the same `i_overlay`
    /// pipeline the egui app drives).
    fn apply_boolean(&mut self, op: BoolOp) {
        let (Some(front), Some(back)) = (self.selected, self.secondary) else {
            return;
        };
        if front == back || front >= self.doc.shapes.len() || back >= self.doc.shapes.len() {
            return;
        }
        // `apply(subj=back, clip=front)` — subj is the lower shape, clip the upper.
        let results = boolean::apply(
            &self.doc.shapes[back],
            &self.doc.shapes[front],
            op,
            BoolFillRule::NonZero,
        );
        if results.is_empty() {
            return;
        }
        self.checkpoint();
        // Remove both operands (highest index first so the other stays valid),
        // then append the result batch in paint order.
        let (hi, lo) = if front > back { (front, back) } else { (back, front) };
        self.doc.shapes.remove(hi);
        self.doc.shapes.remove(lo);
        let first = self.doc.shapes.len();
        self.doc.shapes.extend(results);
        self.select_single(first);
        self.host.mark_dirty();
    }

    /// Whether a pathfinder boolean op can run right now (two distinct shapes
    /// selected). Read by the toolbar to enable / grey the boolean buttons.
    pub fn can_boolean(&self) -> bool {
        match (self.selected, self.secondary) {
            (Some(a), Some(b)) => a != b,
            _ => false,
        }
    }

    /// The primary live-shape's parameters (polygon / star), if the primary
    /// selection is a live path. Read by the inspector to gate / seed its Live
    /// Shape section.
    pub fn primary_live_shape(&self) -> Option<LiveShape> {
        self.selected_shape().and_then(|s| s.live_shape())
    }

    /// The primary text object's parameters, if the primary selection is text.
    /// Read by the inspector to gate / seed its Type section.
    pub fn primary_text_params(&self) -> Option<&TextParams> {
        self.selected_shape().and_then(|s| s.text_params())
    }

    /// Drop selection indices that fell outside the (possibly restored) document
    /// after an undo / redo (mirrors the egui app's `clamp_selection`).
    fn clamp_selection(&mut self) {
        let n = self.doc.shapes.len();
        self.selection.retain(|&i| i < n);
        self.sync_legacy_selection();
        if self.editing_text.is_some_and(|i| i >= n) {
            self.editing_text = None;
        }
    }

    /// The editable element (anchor or handle knob) of the **selected path**
    /// nearest the document-space point `(x, y)`, if any is within pick range.
    /// Handle knobs take priority over anchors (mirrors the egui
    /// `hit_path_edit`). Only sub-contour 0 of a `Shape::Path` is editable here.
    pub fn hit_node(&self, x: f32, y: f32) -> Option<NodeTarget> {
        let (points, handles, _) = self.selected_shape()?.contour(0)?;
        // Handles first: an out/in knob of any anchor that carries a tangent.
        for (k, &p) in points.iter().enumerate() {
            let h = document::handle_at(handles, k);
            if h.0 != 0.0 || h.1 != 0.0 {
                let out = (p.0 + h.0, p.1 + h.1);
                let inp = (p.0 - h.0, p.1 - h.1);
                if (x - out.0).hypot(y - out.1) <= HANDLE_PICK_TOL
                    || (x - inp.0).hypot(y - inp.1) <= HANDLE_PICK_TOL
                {
                    return Some(NodeTarget::Handle(k));
                }
            }
        }
        // Then anchors: the nearest within tolerance.
        points
            .iter()
            .enumerate()
            .filter(|(_, &p)| (x - p.0).hypot(y - p.1) <= ANCHOR_PICK_TOL)
            .min_by(|(_, &a), (_, &b)| {
                (x - a.0).hypot(y - a.1).total_cmp(&(x - b.0).hypot(y - b.1))
            })
            .map(|(k, _)| NodeTarget::Anchor(k))
    }

    /// The selected path's anchor points + per-anchor out-tangent handles
    /// (sub-contour 0), in **document space**, for drawing the node overlay.
    /// `None` unless the selection is a path.
    pub fn selected_path_nodes(&self) -> Option<PathNodes> {
        let (points, handles, _) = self.selected_shape()?.contour(0)?;
        let mut hs = handles.to_vec();
        hs.resize(points.len(), (0.0, 0.0));
        Some((points.to_vec(), hs))
    }

    /// The currently selected shape, mutably, if the selection is in range.
    fn selected_shape_mut(&mut self) -> Option<&mut Shape> {
        self.selected.and_then(|i| self.doc.shapes.get_mut(i))
    }

    /// The currently selected shape (read-only), if the selection is in range.
    /// Read by the inspector panel to seed its stepper rows from live state.
    pub fn selected_shape(&self) -> Option<&Shape> {
        self.selected.and_then(|i| self.doc.shapes.get(i))
    }

    /// The selected shape's tight bounding box in **document space**
    /// `[x, y, w, h]`, if a shape is selected and has finite bounds. The root view
    /// maps this through the view transform to draw the selection ring overlay.
    pub fn selected_bounds(&self) -> Option<[f32; 4]> {
        self.selected_shape()
            .and_then(|s| s.bounds())
            .map(|r| [r.x, r.y, r.w, r.h])
    }

    /// The secondary-selected shape's tight bounds in document space (for the
    /// amber boolean-operand ring overlay).
    pub fn secondary_bounds(&self) -> Option<[f32; 4]> {
        self.secondary
            .and_then(|i| self.doc.shapes.get(i))
            .and_then(|s| s.bounds())
            .map(|r| [r.x, r.y, r.w, r.h])
    }

    /// The selected shape's fill colour, if it is selected and has a fill region.
    pub fn selected_fill(&self) -> Option<[f32; 4]> {
        self.selected_shape().and_then(|s| s.fill_color())
    }

    /// The selected shape's stroke colour, if a shape is selected.
    pub fn selected_stroke(&self) -> Option<[f32; 4]> {
        self.selected_shape().and_then(|s| s.stroke_color())
    }

    /// Index of the topmost *selectable* shape whose region contains the
    /// document-space point `(x, y)`, or `None` when the click misses everything.
    /// "Topmost" = last in paint order, so we scan the shape vec back-to-front and
    /// return the first selectable hit. Reuses the document model's own
    /// [`Shape::hit`] so the GPUI host and the egui canvas pick identically.
    fn hit_test_topmost(&self, x: f32, y: f32) -> Option<usize> {
        // A few document units of tolerance gives thin lines / open paths a
        // clickable thickness (matches the egui canvas's pick slop).
        const TOL: f32 = 4.0;
        self.doc
            .shapes
            .iter()
            .enumerate()
            .rev()
            .find(|(_, s)| s.selectable() && s.hit(x, y, TOL))
            .map(|(i, _)| i)
    }

    /// Index of the topmost selectable shape under the document-space point
    /// `(x, y)`, or `None`. Public wrapper over the hit test so the root view can
    /// decide between a click-select and starting a marquee on empty canvas.
    pub fn hit_at(&self, x: f32, y: f32) -> Option<usize> {
        self.hit_test_topmost(x, y)
    }

    /// True if `(x, y)` (doc space) lands on any **already-selected** shape.
    /// Used to detect "drag to move selection" intent before a general hit test.
    pub fn hit_selected(&self, x: f32, y: f32) -> bool {
        const TOL: f32 = 4.0;
        self.selection.iter().any(|&i| {
            self.doc
                .shapes
                .get(i)
                .map(|s| s.hit(x, y, TOL))
                .unwrap_or(false)
        })
    }

    /// Index of the topmost point-type object under the document-space point
    /// `(x, y)`, or `None`. Lets the Type tool re-edit an existing text object on
    /// click instead of always placing a new one.
    pub fn hit_text(&self, x: f32, y: f32) -> Option<usize> {
        const TOL: f32 = 4.0;
        self.doc
            .shapes
            .iter()
            .enumerate()
            .rev()
            .find(|(_, s)| {
                s.selectable() && s.text_params().is_some() && s.hit(x, y, TOL)
            })
            .map(|(i, _)| i)
    }

    /// Whether a shape is dimmed in isolation mode (belongs to a different group).
    pub fn is_isolated_out(&self, shape: &Shape) -> bool {
        match self.isolation_group {
            Some(g) => shape.group() != Some(g),
            None => false,
        }
    }

    /// Export a minimal valid PDF (one blank page at artboard dimensions). The
    /// `printpdf` crate is not in scope, so we write a hand-crafted PDF/1.4
    /// skeleton. Raster embedding requires `printpdf`; this path logs and writes
    /// a blank page.
    fn export_pdf(&self, path: &std::path::Path) -> std::io::Result<()> {
        use std::io::Write;
        let (w, h) = (self.host.doc_w as f32, self.host.doc_h as f32);
        // PDF points: 1 pt = 1/72 inch; we treat 1 doc-unit = 1 pt.
        let wp = w;
        let hp = h;

        // Object contents (indexed 1-based).
        let obj1 = b"1 0 obj<</Type/Catalog/Pages 2 0 R>>endobj\n";
        let obj2 = b"2 0 obj<</Type/Pages/Kids[3 0 R]/Count 1>>endobj\n";
        let obj3_content = format!(
            "3 0 obj<</Type/Page/Parent 2 0 R/MediaBox[0 0 {wp:.1} {hp:.1}]>>endobj\n"
        );
        let obj3 = obj3_content.as_bytes();

        let header = b"%PDF-1.4\n";
        let off1 = header.len();
        let off2 = off1 + obj1.len();
        let off3 = off2 + obj2.len();
        let xref_offset = off3 + obj3.len();

        let xref = format!(
            "xref\n0 4\n0000000000 65535 f \n{off1:010} 00000 n \n{off2:010} 00000 n \n{off3:010} 00000 n \ntrailer<</Size 4/Root 1 0 R>>\nstartxref\n{xref_offset}\n%%EOF\n"
        );

        log::info!("PDF export: embedded image requires `printpdf` crate; wrote blank page");

        let mut f = std::fs::File::create(path)?;
        f.write_all(header)?;
        f.write_all(obj1)?;
        f.write_all(obj2)?;
        f.write_all(obj3)?;
        f.write_all(xref.as_bytes())?;
        Ok(())
    }

    /// Split every shape that the line segment (`start`→`end`, doc space)
    /// intersects into two sub-shapes at the intersection parameters. Shapes
    /// with no bounding-box intersection are left alone. Sutherland–Hodgman
    /// style: we split the shape's bounding-box polygon at the knife line.
    fn apply_knife_slice(&mut self, start: (f32, f32), end: (f32, f32)) {
        if self.doc.shapes.is_empty() {
            return;
        }
        let (x1, y1, x2, y2) = (start.0, start.1, end.0, end.1);
        // Direction vector and its normal for the split plane.
        let (dx, dy) = (x2 - x1, y2 - y1);
        if dx.abs() < 1e-6 && dy.abs() < 1e-6 {
            return;
        }
        let mut new_shapes: Vec<Shape> = Vec::new();
        let mut remove_idxs: Vec<usize> = Vec::new();

        for i in 0..self.doc.shapes.len() {
            let shape = &self.doc.shapes[i];
            let Some(b) = shape.bounds() else { continue };
            // Quick bbox-vs-line test: does the bounding box contain any
            // point on the segment? We use the simple separating-axis test on
            // the four corners vs the knife line.
            let corners = [
                (b.x, b.y),
                (b.x + b.w, b.y),
                (b.x + b.w, b.y + b.h),
                (b.x, b.y + b.h),
            ];
            let side = |p: (f32, f32)| {
                (p.0 - x1) * dy - (p.1 - y1) * dx
            };
            let signs: Vec<f32> = corners.iter().map(|&c| side(c)).collect();
            let has_pos = signs.iter().any(|&s| s > 0.0);
            let has_neg = signs.iter().any(|&s| s < 0.0);
            if !(has_pos && has_neg) {
                continue; // Entire shape is on one side; not split.
            }

            // Partition the bounding-box corners into left/right halves.
            let left: Vec<(f32, f32)> = corners.iter().copied().filter(|&c| side(c) <= 0.0).collect();
            let right: Vec<(f32, f32)> = corners.iter().copied().filter(|&c| side(c) > 0.0).collect();

            if left.len() < 2 || right.len() < 2 {
                continue;
            }

            // Build left-half bounding box and right-half bounding box.
            let bbox_of = |pts: &[(f32, f32)]| -> [f32; 4] {
                let x_min = pts.iter().map(|p| p.0).fold(f32::INFINITY, f32::min);
                let y_min = pts.iter().map(|p| p.1).fold(f32::INFINITY, f32::min);
                let x_max = pts.iter().map(|p| p.0).fold(f32::NEG_INFINITY, f32::max);
                let y_max = pts.iter().map(|p| p.1).fold(f32::NEG_INFINITY, f32::max);
                [x_min, y_min, x_max - x_min, y_max - y_min]
            };

            let left_bb = bbox_of(&left);
            let right_bb = bbox_of(&right);

            // Create two rect approximations of the split shape inheriting paint.
            let fill = shape.fill_color().unwrap_or([0.5, 0.5, 0.5, 1.0]);
            let stroke = shape.stroke_color().unwrap_or([0.0, 0.0, 0.0, 1.0]);
            let stroke_w = shape.stroke_width();

            new_shapes.push(Shape::rect(left_bb, fill, stroke, stroke_w));
            new_shapes.push(Shape::rect(right_bb, fill, stroke, stroke_w));
            remove_idxs.push(i);
        }

        if remove_idxs.is_empty() {
            return;
        }

        self.checkpoint();
        // Remove highest index first.
        remove_idxs.sort_unstable_by(|a, b| b.cmp(a));
        remove_idxs.dedup();
        for i in &remove_idxs {
            self.doc.shapes.remove(*i);
        }
        self.doc.shapes.extend(new_shapes);
        self.select_clear();
        self.host.mark_dirty();
    }

    /// Serialize visible document shapes to a plain-string SVG file.
    fn export_svg(&self, path: &std::path::Path) -> std::io::Result<()> {
        use std::io::Write;
        let mut out = String::new();
        let (w, h) = (self.host.doc_w, self.host.doc_h);
        out.push_str(&format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
             <svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{w}\" height=\"{h}\" \
             viewBox=\"0 0 {w} {h}\">\n"
        ));
        for shape in &self.doc.shapes {
            if !shape.visible() {
                continue;
            }
            out.push_str(&shape_to_svg(shape));
        }
        out.push_str("</svg>\n");
        let mut f = std::fs::File::create(path)?;
        f.write_all(out.as_bytes())?;
        Ok(())
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

    /// A drag in either corner order produces the same normalized rect, inherits
    /// the default paint, appends in paint order, and becomes the selection.
    #[test]
    fn create_shape_normalizes_and_selects() {
        let mut app = App::new();
        let before = app.doc.shapes.len();
        // Corners given bottom-right → top-left to exercise the min/abs path.
        app.apply(Action::CreateShape {
            tool: Tool::Rect,
            a: [300.0, 260.0],
            b: [100.0, 60.0],
        });
        assert_eq!(app.doc.shapes.len(), before + 1);
        let idx = before;
        assert_eq!(app.selected, Some(idx));
        let r = app.doc.shapes[idx].bounds().unwrap();
        assert!((r.x - 100.0).abs() < 1e-3 && (r.y - 60.0).abs() < 1e-3);
        assert!((r.w - 200.0).abs() < 1e-3 && (r.h - 200.0).abs() < 1e-3);
        assert_eq!(app.doc.shapes[idx].fill_color(), Some(app.default_fill));
        assert_eq!(app.doc.shapes[idx].label(), "Rectangle");
    }

    /// The Line tool drag-creates a `Shape::Line` from the raw drag endpoints.
    #[test]
    fn create_shape_line_tool() {
        let mut app = App::new();
        let before = app.doc.shapes.len();
        app.apply(Action::SetTool(Tool::Line));
        app.apply(Action::CreateShape {
            tool: Tool::Line,
            a: [10.0, 20.0],
            b: [110.0, 70.0],
        });
        assert_eq!(app.doc.shapes.len(), before + 1);
        match app.doc.shapes.last().unwrap() {
            Shape::Line { p0, p1, .. } => {
                assert_eq!(*p0, (10.0, 20.0));
                assert_eq!(*p1, (110.0, 70.0));
            }
            other => panic!("expected a Line, got {}", other.label()),
        }
    }

    /// The Polygon tool drag-creates a closed live-shape path centred at the press
    /// point, with one vertex per side (the host's `poly_sides` default).
    #[test]
    fn create_shape_polygon_tool() {
        let mut app = App::new();
        app.poly_sides = 6;
        app.apply(Action::SetTool(Tool::Polygon));
        app.apply(Action::CreateShape {
            tool: Tool::Polygon,
            a: [200.0, 200.0],
            b: [200.0, 150.0], // radius 50
        });
        match app.doc.shapes.last().unwrap() {
            Shape::Path {
                points,
                closed,
                live: Some(LiveShape::Polygon { sides, radius, .. }),
                ..
            } => {
                assert!(*closed);
                assert_eq!(*sides, 6);
                assert_eq!(points.len(), 6);
                assert!((*radius - 50.0).abs() < 1e-3);
            }
            other => panic!("expected a live Polygon path, got {}", other.label()),
        }
    }

    /// The Star tool drag-creates a closed live star with `2 * points` vertices.
    #[test]
    fn create_shape_star_tool() {
        let mut app = App::new();
        app.star_points = 5;
        app.apply(Action::SetTool(Tool::Star));
        app.apply(Action::CreateShape {
            tool: Tool::Star,
            a: [200.0, 200.0],
            b: [240.0, 230.0], // radius 50
        });
        match app.doc.shapes.last().unwrap() {
            Shape::Path {
                points,
                live: Some(LiveShape::Star { points: p, .. }),
                ..
            } => {
                assert_eq!(*p, 5);
                assert_eq!(points.len(), 10);
            }
            other => panic!("expected a live Star path, got {}", other.label()),
        }
    }

    /// A zero-radius polygon drag is dropped.
    #[test]
    fn create_shape_polygon_drops_degenerate() {
        let mut app = App::new();
        let before = app.doc.shapes.len();
        app.apply(Action::CreateShape {
            tool: Tool::Polygon,
            a: [50.0, 50.0],
            b: [50.2, 50.2],
        });
        assert_eq!(app.doc.shapes.len(), before);
    }

    /// Type: placing text then typing builds a `Shape::Text` whose string and
    /// glyph cache track the keystrokes; the whole session is one undo entry.
    #[test]
    fn type_place_and_edit_text() {
        let mut app = App::new();
        let before = app.doc.shapes.len();
        app.apply(Action::SetTool(Tool::Type));
        app.apply(Action::PlaceText { x: 100.0, y: 100.0 });
        assert_eq!(app.doc.shapes.len(), before + 1);
        let idx = app.editing_text.expect("editing the placed text");
        for c in "Hi".chars() {
            app.apply(Action::TypeChar(c));
        }
        match &app.doc.shapes[idx] {
            Shape::Text { params, glyphs, .. } => {
                assert_eq!(params.text, "Hi");
                assert!(!glyphs.is_empty(), "glyphs laid out for non-empty text");
            }
            other => panic!("expected Text, got {}", other.label()),
        }
        app.apply(Action::TypeBackspace);
        assert_eq!(app.selected_shape().unwrap().text_params().unwrap().text, "H");
        app.apply(Action::FinishText);
        assert!(app.editing_text.is_none());
        assert_eq!(app.doc.shapes.len(), before + 1);
        // The whole place→type session collapses to one undo entry.
        app.apply(Action::Undo);
        assert_eq!(app.doc.shapes.len(), before);
    }

    /// Type: an object placed but never typed into is removed on finish, and that
    /// no-op session records nothing in history.
    #[test]
    fn type_empty_text_is_discarded() {
        let mut app = App::new();
        let before = app.doc.shapes.len();
        let undo_before = app.history.can_undo();
        app.apply(Action::SetTool(Tool::Type));
        app.apply(Action::PlaceText { x: 10.0, y: 10.0 });
        app.apply(Action::FinishText);
        assert_eq!(app.doc.shapes.len(), before);
        assert_eq!(app.history.can_undo(), undo_before, "no checkpoint for an empty placed text");
    }

    /// Switching off the Type tool finishes the edit (and discards empty text).
    #[test]
    fn switching_tool_finishes_text_edit() {
        let mut app = App::new();
        app.apply(Action::SetTool(Tool::Type));
        app.apply(Action::PlaceText { x: 10.0, y: 10.0 });
        app.apply(Action::TypeChar('A'));
        app.apply(Action::SetTool(Tool::Select));
        assert!(app.editing_text.is_none());
        assert_eq!(app.doc.shapes.last().unwrap().text_params().unwrap().text, "A");
    }

    /// Boolean: uniting two overlapping rects replaces both operands with a result
    /// batch and is undoable in one step. Requires a primary + secondary selection.
    #[test]
    fn boolean_union_replaces_operands() {
        let mut app = App::new();
        // Two overlapping rects appended on top of the sample document.
        app.doc.shapes.push(Shape::rect(
            [0.0, 0.0, 100.0, 100.0],
            app.default_fill,
            app.default_stroke,
            1.0,
        ));
        app.doc.shapes.push(Shape::rect(
            [50.0, 50.0, 100.0, 100.0],
            app.default_fill,
            app.default_stroke,
            1.0,
        ));
        let a = app.doc.shapes.len() - 2;
        let b = app.doc.shapes.len() - 1;
        app.selected = Some(b);
        app.secondary = Some(a);
        assert!(app.can_boolean());
        let before = app.doc.shapes.len();
        app.apply(Action::Boolean(BoolOp::Union));
        // The two rects are gone, replaced by at least one result shape.
        assert!(app.doc.shapes.len() < before + 1);
        assert!(app.doc.shapes.len() >= before - 1);
        assert!(app.secondary.is_none());
        // One undo restores both operands.
        app.apply(Action::Undo);
        assert_eq!(app.doc.shapes.len(), before);
    }

    /// Boolean: a single selection (no secondary) can't run an op — no-op.
    #[test]
    fn boolean_needs_two_operands() {
        let mut app = App::new();
        app.apply(Action::SelectShape(0));
        let before = app.doc.shapes.len();
        assert!(!app.can_boolean());
        app.apply(Action::Boolean(BoolOp::Intersect));
        assert_eq!(app.doc.shapes.len(), before);
    }

    /// Shift+click adds a second operand; a third distinct click moves it.
    #[test]
    fn add_to_selection_tracks_secondary() {
        let mut app = App::new();
        // Sample doc has 3 shapes; pick centres that hit shapes 0 and 2.
        app.apply(Action::SelectShape(0));
        // Hit the green rect (shape 2) which is selectable and on top there.
        let g = app.doc.shapes[2].bounds().unwrap();
        app.apply(Action::AddToSelection {
            x: g.x + g.w * 0.5,
            y: g.y + g.h * 0.5,
        });
        assert!(app.secondary.is_some());
    }

    /// The Ellipse tool drag-creates an ellipse.
    #[test]
    fn create_shape_ellipse_tool() {
        let mut app = App::new();
        let before = app.doc.shapes.len();
        app.apply(Action::CreateShape {
            tool: Tool::Ellipse,
            a: [10.0, 10.0],
            b: [60.0, 90.0],
        });
        assert_eq!(app.doc.shapes.last().unwrap().label(), "Ellipse");
        assert_eq!(app.doc.shapes.len(), before + 1);
    }

    /// A sub-unit (zero-area) drag is dropped — no shape, no selection change.
    #[test]
    fn create_shape_drops_degenerate_drag() {
        let mut app = App::new();
        let before = app.doc.shapes.len();
        let sel = app.selected;
        app.apply(Action::CreateShape {
            tool: Tool::Rect,
            a: [50.0, 50.0],
            b: [50.4, 50.4],
        });
        assert_eq!(app.doc.shapes.len(), before);
        assert_eq!(app.selected, sel);
    }

    /// Panning translates the view offset by exactly the given delta.
    #[test]
    fn pan_translates_offset() {
        let mut app = App::new();
        let (ox, oy) = app.view.offset;
        app.apply(Action::PanBy { dx: 25.0, dy: -10.0 });
        assert_eq!(app.view.offset, (ox + 25.0, oy - 10.0));
    }

    /// Zooming holds the anchor point fixed: the document point under the anchor
    /// (viewport-local) maps to the same viewport-local pixel before and after.
    #[test]
    fn zoom_keeps_anchor_fixed() {
        let mut app = App::new();
        let anchor = (200.0_f32, 150.0_f32);
        // Document-local pixel under the anchor before zooming.
        let before_local = (
            (anchor.0 - app.view.offset.0) / app.view.scale,
            (anchor.1 - app.view.offset.1) / app.view.scale,
        );
        app.apply(Action::ZoomBy {
            factor: 1.5,
            anchor: Some(anchor),
            viewport: (800.0, 600.0),
        });
        // Re-project that same doc-local point and confirm it lands on the anchor.
        let projected = (
            app.view.offset.0 + before_local.0 * app.view.scale,
            app.view.offset.1 + before_local.1 * app.view.scale,
        );
        assert!((projected.0 - anchor.0).abs() < 1e-2);
        assert!((projected.1 - anchor.1).abs() < 1e-2);
        assert!((app.view.scale - 1.5).abs() < 1e-3);
    }

    /// Zoom is clamped to the configured range and never inverts.
    #[test]
    fn zoom_clamps_to_range() {
        let mut app = App::new();
        for _ in 0..50 {
            app.apply(Action::ZoomBy {
                factor: 2.0,
                anchor: None,
                viewport: (800.0, 600.0),
            });
        }
        assert!(app.view.scale <= View::MAX_SCALE + 1e-3);
        for _ in 0..50 {
            app.apply(Action::ZoomBy {
                factor: 0.5,
                anchor: None,
                viewport: (800.0, 600.0),
            });
        }
        assert!(app.view.scale >= View::MIN_SCALE - 1e-3);
    }

    /// Reset returns the view to its default pan + zoom.
    #[test]
    fn reset_view_restores_default() {
        let mut app = App::new();
        app.apply(Action::PanBy { dx: 100.0, dy: 100.0 });
        app.apply(Action::ZoomBy {
            factor: 2.0,
            anchor: None,
            viewport: (800.0, 600.0),
        });
        app.apply(Action::ResetView);
        let d = View::default();
        assert_eq!(app.view.offset, d.offset);
        assert_eq!(app.view.scale, d.scale);
    }

    /// Pen: clicking anchors then finishing closed commits a `Shape::Path` with
    /// those anchors, the host's default paint, and becomes the selection.
    #[test]
    fn pen_commits_closed_path() {
        let mut app = App::new();
        app.apply(Action::SetTool(Tool::Pen));
        let before = app.doc.shapes.len();
        app.apply(Action::PenAddAnchor { x: 100.0, y: 100.0 });
        app.apply(Action::PenAddAnchor { x: 200.0, y: 100.0 });
        app.apply(Action::PenAddAnchor { x: 150.0, y: 200.0 });
        assert_eq!(app.pen.points.len(), 3);
        app.apply(Action::PenFinish { closed: true });
        assert!(app.pen.is_empty(), "pen buffer drained on commit");
        assert_eq!(app.doc.shapes.len(), before + 1);
        let s = app.doc.shapes.last().unwrap();
        assert_eq!(s.label(), "Path");
        assert_eq!(app.selected, Some(app.doc.shapes.len() - 1));
        // The commit is one undo step that removes the path again.
        app.apply(Action::Undo);
        assert_eq!(app.doc.shapes.len(), before);
    }

    /// Pen: a click within the close tolerance of the first anchor closes +
    /// commits the path rather than placing another anchor.
    #[test]
    fn pen_first_anchor_click_closes() {
        let mut app = App::new();
        app.apply(Action::SetTool(Tool::Pen));
        let before = app.doc.shapes.len();
        app.apply(Action::PenAddAnchor { x: 100.0, y: 100.0 });
        app.apply(Action::PenAddAnchor { x: 200.0, y: 100.0 });
        app.apply(Action::PenAddAnchor { x: 150.0, y: 200.0 });
        // Click back on (almost) the first anchor.
        app.apply(Action::PenAddAnchor { x: 101.0, y: 100.5 });
        assert!(app.pen.is_empty());
        assert_eq!(app.doc.shapes.len(), before + 1);
        // The committed path closed and kept exactly the 3 placed anchors.
        if let Some(Shape::Path { points, closed, .. }) = app.doc.shapes.last() {
            assert!(*closed);
            assert_eq!(points.len(), 3);
        } else {
            panic!("expected a closed Path");
        }
    }

    /// Pen: a path with fewer than two anchors is discarded on finish.
    #[test]
    fn pen_discards_degenerate_path() {
        let mut app = App::new();
        app.apply(Action::SetTool(Tool::Pen));
        let before = app.doc.shapes.len();
        app.apply(Action::PenAddAnchor { x: 50.0, y: 50.0 });
        app.apply(Action::PenFinish { closed: false });
        assert!(app.pen.is_empty());
        assert_eq!(app.doc.shapes.len(), before);
        assert!(!app.history.can_undo(), "no checkpoint for a dropped path");
    }

    /// Switching away from the Pen tool abandons a half-drawn path.
    #[test]
    fn switching_tool_cancels_pen() {
        let mut app = App::new();
        app.apply(Action::SetTool(Tool::Pen));
        app.apply(Action::PenAddAnchor { x: 10.0, y: 10.0 });
        app.apply(Action::PenAddAnchor { x: 40.0, y: 10.0 });
        app.apply(Action::SetTool(Tool::Select));
        assert!(app.pen.is_empty());
    }

    /// Node edit: a coalesced anchor drag (begin → moves → commit) collapses to
    /// one undo entry that restores the original anchor position.
    #[test]
    fn node_anchor_move_coalesces_one_undo() {
        let mut app = App::new();
        // Build a triangle path via the pen and select it.
        app.apply(Action::SetTool(Tool::Pen));
        app.apply(Action::PenAddAnchor { x: 100.0, y: 100.0 });
        app.apply(Action::PenAddAnchor { x: 200.0, y: 100.0 });
        app.apply(Action::PenAddAnchor { x: 150.0, y: 200.0 });
        app.apply(Action::PenFinish { closed: true });
        let idx = app.selected.unwrap();
        // Original first anchor.
        let orig = match &app.doc.shapes[idx] {
            Shape::Path { points, .. } => points[0],
            _ => unreachable!(),
        };
        // Drag it (coalesced).
        app.apply(Action::BeginInteraction);
        app.apply(Action::MoveAnchor { which: 0, x: 120.0, y: 130.0 });
        app.apply(Action::MoveAnchor { which: 0, x: 140.0, y: 160.0 });
        app.apply(Action::EndInteraction);
        let moved = match &app.doc.shapes[idx] {
            Shape::Path { points, .. } => points[0],
            _ => unreachable!(),
        };
        assert_eq!(moved, (140.0, 160.0));
        // One undo restores the pre-drag position (the whole drag is one entry).
        app.apply(Action::Undo);
        let restored = match &app.doc.shapes[idx] {
            Shape::Path { points, .. } => points[0],
            _ => unreachable!(),
        };
        assert_eq!(restored, orig);
    }

    /// A no-op node drag (begin → commit with no change) records no undo entry.
    #[test]
    fn node_noop_drag_records_nothing() {
        let mut app = App::new();
        app.apply(Action::SelectShape(0));
        let depth_before = app.history.can_undo();
        app.apply(Action::BeginInteraction);
        app.apply(Action::EndInteraction);
        assert_eq!(app.history.can_undo(), depth_before);
    }

    /// `hit_node` finds an anchor of the selected path within tolerance and
    /// prioritises a handle knob when one is in range.
    #[test]
    fn hit_node_finds_anchor_and_handle() {
        let mut app = App::new();
        app.apply(Action::SetTool(Tool::Pen));
        app.apply(Action::PenAddAnchor { x: 100.0, y: 100.0 });
        app.apply(Action::PenAddAnchor { x: 200.0, y: 100.0 });
        // Give the last anchor a tangent so its knob is hit-testable.
        app.apply(Action::PenSetHandle { x: 230.0, y: 100.0 });
        app.apply(Action::PenFinish { closed: false });
        // Right on the first anchor → Anchor(0).
        assert_eq!(app.hit_node(100.0, 100.0), Some(NodeTarget::Anchor(0)));
        // On anchor 1's out-knob (anchor + handle = (230,100)) → Handle(1).
        assert_eq!(app.hit_node(230.0, 100.0), Some(NodeTarget::Handle(1)));
        // Far from everything → None.
        assert_eq!(app.hit_node(-50.0, -50.0), None);
    }

    /// Delete removes the selected shape as one undoable step.
    #[test]
    fn delete_selected_is_undoable() {
        let mut app = App::new();
        let before = app.doc.shapes.len();
        app.apply(Action::SelectShape(0));
        app.apply(Action::DeleteSelected);
        assert_eq!(app.doc.shapes.len(), before - 1);
        assert_eq!(app.selected, None);
        app.apply(Action::Undo);
        assert_eq!(app.doc.shapes.len(), before);
    }

    /// Undo / redo round-trips an appearance edit (one checkpoint per click).
    #[test]
    fn appearance_edit_undo_redo() {
        let mut app = App::new();
        app.apply(Action::SelectShape(0));
        let orig = app.selected_fill().unwrap();
        let new = [0.1, 0.2, 0.3, 1.0];
        app.apply(Action::SetFillColor(new));
        assert_eq!(app.selected_fill().unwrap(), new);
        app.apply(Action::Undo);
        assert_eq!(app.selected_fill().unwrap(), orig);
        app.apply(Action::Redo);
        assert_eq!(app.selected_fill().unwrap(), new);
    }

    /// Shift+click toggles a shape in and out of the multi-selection; the last
    /// added is primary, the previous becomes the secondary boolean operand.
    #[test]
    fn shift_click_toggles_multi_selection() {
        let mut app = App::new();
        // Select shape 0, then shift-add shape 2 (the green rect, on top there).
        app.apply(Action::SelectShape(0));
        let g = app.doc.shapes[2].bounds().unwrap();
        let (gx, gy) = (g.x + g.w * 0.5, g.y + g.h * 0.5);
        app.apply(Action::AddToSelection { x: gx, y: gy });
        assert_eq!(app.selection, vec![0, 2]);
        assert_eq!(app.selected, Some(2));
        assert_eq!(app.secondary, Some(0));
        assert!(app.can_boolean());
        // Shift-clicking shape 2 again removes it.
        app.apply(Action::AddToSelection { x: gx, y: gy });
        assert_eq!(app.selection, vec![0]);
        assert_eq!(app.selected, Some(0));
        assert!(app.secondary.is_none());
    }

    /// A marquee rectangle selects every shape whose bounds it intersects, and an
    /// empty marquee clears the selection.
    #[test]
    fn marquee_selects_intersecting_shapes() {
        let mut app = App::new();
        // The sample doc's three shapes all live within (120..740, 120..560).
        app.apply(Action::MarqueeSelect {
            rect: [0.0, 0.0, 2000.0, 2000.0],
        });
        assert_eq!(app.selection.len(), 3);
        // A marquee far off the artboard hits nothing.
        app.apply(Action::MarqueeSelect {
            rect: [-1000.0, -1000.0, 10.0, 10.0],
        });
        assert!(app.selection.is_empty());
        assert!(app.selected.is_none());
    }

    /// Editing a live polygon's sides regenerates its outline (one vertex per
    /// side) as one undo step, and mirrors the working default.
    #[test]
    fn set_live_shape_regenerates_outline() {
        let mut app = App::new();
        app.apply(Action::CreateShape {
            tool: Tool::Polygon,
            a: [200.0, 200.0],
            b: [200.0, 150.0],
        });
        let idx = app.selected.unwrap();
        let live = app.primary_live_shape().expect("polygon is live");
        let new = match live {
            LiveShape::Polygon { radius, corner_radius, .. } => LiveShape::Polygon { sides: 8, radius, corner_radius },
            other => other,
        };
        app.apply(Action::SetLiveShape(new));
        match &app.doc.shapes[idx] {
            Shape::Path {
                points,
                live: Some(LiveShape::Polygon { sides, .. }),
                ..
            } => {
                assert_eq!(*sides, 8);
                assert_eq!(points.len(), 8);
            }
            _ => panic!("expected an 8-gon"),
        }
        assert_eq!(app.poly_sides, 8);
        app.apply(Action::Undo);
        match &app.doc.shapes[idx] {
            Shape::Path { points, .. } => assert_eq!(points.len(), 6),
            _ => unreachable!(),
        }
    }

    /// Editing the primary text object's size / alignment / font re-lays-out its
    /// glyph cache through `set_text_params`, each as its own undo step.
    #[test]
    fn set_text_params_relays_out() {
        let mut app = App::new();
        app.apply(Action::SetTool(Tool::Type));
        app.apply(Action::PlaceText { x: 100.0, y: 100.0 });
        for c in "Hi".chars() {
            app.apply(Action::TypeChar(c));
        }
        app.apply(Action::FinishText);
        let idx = app.selected.unwrap();
        app.apply(Action::SetTextSize(120.0));
        assert_eq!(app.primary_text_params().unwrap().font_size, 120.0);
        app.apply(Action::SetTextAlign(TextAlign::Center));
        assert_eq!(app.primary_text_params().unwrap().align, TextAlign::Center);
        app.apply(Action::SetTextFont(Some("Helvetica".into())));
        assert_eq!(
            app.primary_text_params().unwrap().font_family.as_deref(),
            Some("Helvetica")
        );
        // The glyph cache stays consistent with the params.
        assert!(app.doc.shapes[idx].text_params().is_some());
    }

    /// Align-left moves every selected shape's left edge to the selection's
    /// combined left, as one undo step; needs ≥2 shapes.
    #[test]
    fn align_left_shares_left_edge() {
        let mut app = App::new();
        app.doc.shapes.clear();
        app.doc.shapes.push(Shape::rect([10.0, 10.0, 40.0, 40.0], app.default_fill, app.default_stroke, 1.0));
        app.doc.shapes.push(Shape::rect([100.0, 200.0, 40.0, 40.0], app.default_fill, app.default_stroke, 1.0));
        app.selection = vec![0, 1];
        app.sync_legacy_selection();
        assert!(app.can_align());
        app.apply(Action::AlignSelection(Align::Left));
        let l0 = app.doc.shapes[0].bounds().unwrap().x;
        let l1 = app.doc.shapes[1].bounds().unwrap().x;
        assert!((l0 - 10.0).abs() < 1e-3 && (l1 - 10.0).abs() < 1e-3);
        app.apply(Action::Undo);
        assert!((app.doc.shapes[1].bounds().unwrap().x - 100.0).abs() < 1e-3);
    }

    /// Distribute needs ≥3 shapes; with three it evens the centre spacing (the
    /// outer two stay put).
    #[test]
    fn distribute_centers_evens_spacing() {
        let mut app = App::new();
        app.doc.shapes.clear();
        for x in [0.0, 30.0, 200.0] {
            app.doc.shapes.push(Shape::rect([x, 0.0, 20.0, 20.0], app.default_fill, app.default_stroke, 1.0));
        }
        app.selection = vec![0, 1, 2];
        app.sync_legacy_selection();
        assert!(app.can_distribute());
        app.apply(Action::DistributeSelection(Distribute::CentersH));
        let c = |i: usize| {
            let b = app.doc.shapes[i].bounds().unwrap();
            b.x + b.w * 0.5
        };
        // The middle centre lands halfway between the (unmoved) outer two.
        let mid = (c(0) + c(2)) * 0.5;
        assert!((c(1) - mid).abs() < 1e-2, "middle centre {} vs {}", c(1), mid);
    }

    // ---- Wave 9 tests -------------------------------------------------------

    /// PickColor sets `fg_color` and reverts the active tool to `prev_tool`.
    #[test]
    fn pick_color_sets_fg_and_reverts_tool() {
        let mut app = App::new();
        app.apply(Action::SetTool(Tool::Select));
        app.apply(Action::SetTool(Tool::Eyedropper));
        assert_eq!(app.prev_tool, Tool::Select);
        app.apply(Action::PickColor([255, 128, 0, 255]));
        assert!((app.fg_color[0] - 1.0).abs() < 0.01);
        assert!((app.fg_color[1] - 128.0 / 255.0).abs() < 0.01);
        assert!((app.fg_color[2] - 0.0).abs() < 0.01);
        // Tool reverted to Select after pick.
        assert_eq!(app.active, Tool::Select);
    }

    /// ExportSvg writes a file containing `<svg` and one element per visible shape.
    #[test]
    fn export_svg_writes_file() {
        let mut app = App::new();
        let path = std::env::temp_dir().join("contour_wave9_test.svg");
        app.apply(Action::ExportSvg(path.clone()));
        assert!(path.exists(), "SVG file was not created");
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("<svg"), "no <svg> root element");
        assert!(content.contains("<rect"), "no rect element for sample doc");
        assert!(content.contains("</svg>"), "SVG not closed");
        let _ = std::fs::remove_file(&path);
    }

    /// GroupSelected assigns the same group id to all selected shapes; UngroupSelected clears it.
    #[test]
    fn group_and_ungroup_selected() {
        let mut app = App::new();
        app.selection = vec![0, 1];
        app.sync_legacy_selection();
        assert!(app.can_align());
        app.apply(Action::GroupSelected);
        let g0 = app.doc.shapes[0].group();
        let g1 = app.doc.shapes[1].group();
        assert!(g0.is_some(), "shape 0 should have a group id");
        assert_eq!(g0, g1, "both shapes should share the same group id");
        app.apply(Action::UngroupSelected);
        assert!(app.doc.shapes[0].group().is_none());
        assert!(app.doc.shapes[1].group().is_none());
    }

    /// MoveLayerOrder(+1) moves a shape one step toward the top (paint-order).
    #[test]
    fn move_layer_order_reorders_shapes() {
        let mut app = App::new();
        let n = app.doc.shapes.len();
        assert!(n >= 2);
        let label0 = app.doc.shapes[0].label();
        let label1 = app.doc.shapes[1].label();
        app.apply(Action::SelectShape(0));
        app.apply(Action::MoveLayerOrder { id: 0, delta: 1 });
        // Shape that was at index 1 is now at 0; shape that was at 0 is now at 1.
        assert_eq!(app.doc.shapes[0].label(), label1);
        assert_eq!(app.doc.shapes[1].label(), label0);
        // Selection follows the moved shape.
        assert_eq!(app.selected, Some(1));
    }

    /// AttachTextToPath inserts into text_on_path map; DetachTextFromPath removes it.
    #[test]
    fn attach_and_detach_text_to_path() {
        let mut app = App::new();
        app.apply(Action::AttachTextToPath { text_id: 0, path_id: 1 });
        assert_eq!(app.text_on_path.get(&0), Some(&1));
        app.apply(Action::DetachTextFromPath(0));
        assert!(app.text_on_path.get(&0).is_none());
    }

    /// The selection ring source: bounds reflect the selected shape, and clearing
    /// selection yields no bounds.
    #[test]
    fn selected_bounds_tracks_selection() {
        let mut app = App::new();
        app.apply(Action::SelectShape(0));
        let b = app.selected_bounds().expect("shape 0 has bounds");
        assert!(b[2] > 0.0 && b[3] > 0.0);
        // A click that misses every shape clears the selection → no bounds.
        app.apply(Action::HitTestSelect {
            x: -10_000.0,
            y: -10_000.0,
        });
        assert!(app.selected_bounds().is_none());
    }

    // ---- Wave 11 tests -------------------------------------------------------

    /// SetGradientType seeds a gradient on a shape without one and toggles kind.
    #[test]
    fn set_gradient_type_seeds_and_toggles() {
        let mut app = App::new();
        app.apply(Action::SelectShape(0));
        assert!(app.doc.shapes[0].fill_gradient().is_none(), "no gradient initially");
        // Seed a radial gradient.
        app.apply(Action::SetGradientType { shape_id: 0, kind: GradientKind::Radial });
        let g = app.doc.shapes[0].fill_gradient().expect("gradient seeded");
        assert_eq!(g.kind, GradientKind::Radial);
        // Toggle to linear.
        app.apply(Action::SetGradientType { shape_id: 0, kind: GradientKind::Linear });
        assert_eq!(app.doc.shapes[0].fill_gradient().unwrap().kind, GradientKind::Linear);
    }

    /// MergeRegionReal unions two rects into one shape (using i_overlay).
    #[test]
    fn merge_region_real_unions_two_rects() {
        let mut app = App::new();
        app.doc.shapes.clear();
        app.doc.shapes.push(Shape::rect([0.0, 0.0, 100.0, 100.0], app.default_fill, app.default_stroke, 1.0));
        app.doc.shapes.push(Shape::rect([50.0, 50.0, 100.0, 100.0], app.default_fill, app.default_stroke, 1.0));
        app.apply(Action::MergeRegionReal(vec![0, 1]));
        // Two shapes merged → one result shape.
        assert_eq!(app.doc.shapes.len(), 1, "two rects merged into one");
        assert!(app.selected.is_some());
    }

    /// SubtractRegionReal subtracts shape 1 from shape 0.
    #[test]
    fn subtract_region_real_subtracts() {
        let mut app = App::new();
        app.doc.shapes.clear();
        app.doc.shapes.push(Shape::rect([0.0, 0.0, 100.0, 100.0], app.default_fill, app.default_stroke, 1.0));
        app.doc.shapes.push(Shape::rect([50.0, 50.0, 100.0, 100.0], app.default_fill, app.default_stroke, 1.0));
        let before = app.doc.shapes.len();
        app.apply(Action::SubtractRegionReal(vec![0, 1]));
        // Should have at most the same count (result replaces both).
        assert!(app.doc.shapes.len() <= before);
    }

    /// Character panel actions update App state correctly.
    #[test]
    fn character_panel_actions_update_state() {
        let mut app = App::new();
        app.apply(Action::SetFontFamily("Georgia".to_string()));
        assert_eq!(app.font_family, "Georgia");
        app.apply(Action::SetFontWeight(FontWeight::Bold));
        assert_eq!(app.font_weight, FontWeight::Bold);
        app.apply(Action::SetLetterSpacing(50.0));
        assert!((app.letter_spacing - 50.0).abs() < 1e-3);
        app.apply(Action::SetLineHeight(1.5));
        assert!((app.line_height - 1.5).abs() < 1e-3);
        app.apply(Action::SetParaAlign(TextAlign::Center));
        assert_eq!(app.text_align, TextAlign::Center);
    }

    /// SetFontSize syncs default_font_size.
    #[test]
    fn set_font_size_syncs_default() {
        let mut app = App::new();
        app.apply(Action::SetFontSize(48.0));
        assert!((app.default_font_size - 48.0).abs() < 1e-3);
    }

    /// ExportPdf writes a file starting with "%PDF".
    #[test]
    fn export_pdf_writes_file() {
        let mut app = App::new();
        let path = std::env::temp_dir().join("contour_wave11_test.pdf");
        app.apply(Action::ExportPdf(path.clone()));
        assert!(path.exists(), "PDF file was not created");
        let bytes = std::fs::read(&path).unwrap();
        assert!(bytes.starts_with(b"%PDF"), "file does not start with %PDF");
        let _ = std::fs::remove_file(&path);
    }

    /// Isolation mode: EnterIsolation sets isolation_group; ExitIsolation clears it.
    #[test]
    fn isolation_mode_enters_and_exits() {
        let mut app = App::new();
        assert!(app.isolation_group.is_none());
        app.apply(Action::EnterIsolation(42));
        assert_eq!(app.isolation_group, Some(42));
        app.apply(Action::ExitIsolation);
        assert!(app.isolation_group.is_none());
    }

    /// KnifeSlice splits a shape that straddles the cut line into two rects.
    #[test]
    fn knife_slice_splits_straddling_shape() {
        let mut app = App::new();
        app.doc.shapes.clear();
        // A 200×200 rect centred at (100,100); a vertical knife at x=100 straddles it.
        app.doc.shapes.push(Shape::rect([0.0, 0.0, 200.0, 200.0], app.default_fill, app.default_stroke, 1.0));
        app.apply(Action::KnifeSlice { start: (100.0, -10.0), end: (100.0, 210.0) });
        // The single rect was split into two rect halves.
        assert_eq!(app.doc.shapes.len(), 2, "knife should split into two shapes");
    }

    /// KnifeSlice leaves a non-intersected shape alone.
    #[test]
    fn knife_slice_skips_non_intersected() {
        let mut app = App::new();
        app.doc.shapes.clear();
        app.doc.shapes.push(Shape::rect([0.0, 0.0, 50.0, 50.0], app.default_fill, app.default_stroke, 1.0));
        // Knife entirely to the right of the shape.
        app.apply(Action::KnifeSlice { start: (200.0, 0.0), end: (200.0, 100.0) });
        assert_eq!(app.doc.shapes.len(), 1, "shape outside knife should be unchanged");
    }

    /// Tool::Knife is in Tool::ALL.
    #[test]
    fn knife_tool_in_all() {
        assert!(Tool::ALL.contains(&Tool::Knife));
        assert_eq!(Tool::Knife.label(), "Knife");
    }

    // --- Batch 5 ---------------------------------------------------------

    /// Attaching text to a path bakes warped glyphs into the text's cache, and
    /// editing the offset re-bakes them (one undo step each).
    #[test]
    fn text_on_path_bakes_glyphs() {
        let mut app = App::new();
        app.doc.shapes.clear();
        // A text object + a spine path.
        app.apply(Action::PlaceText { x: 0.0, y: 0.0 });
        for c in "Type".chars() {
            app.apply(Action::TypeChar(c));
        }
        app.apply(Action::FinishText);
        let tid = 0usize;
        // A horizontal spine path.
        let spine = Shape::path(
            vec![(0.0, 200.0), (400.0, 200.0)],
            Vec::new(),
            false,
            [0.0, 0.0, 0.0, 0.0],
            [0.0, 0.0, 0.0, 1.0],
            1.0,
        );
        app.doc.shapes.push(spine);
        let pid = app.doc.shapes.len() - 1;
        // Capture flat glyph centroid before attaching.
        let flat_y = text_centroid_y(&app.doc.shapes[tid]);
        app.apply(Action::AttachTextToPath { text_id: tid, path_id: pid });
        assert!(app.text_on_path.get(&tid) == Some(&pid));
        // Shift glyphs up via offset; the centroid y must change.
        app.apply(Action::SetTextOnPathParams {
            text_id: tid,
            params: crate::text_on_path::TextOnPathParams {
                offset: 40.0,
                ..Default::default()
            },
        });
        let shifted_y = text_centroid_y(&app.doc.shapes[tid]);
        assert!(
            (shifted_y - flat_y).abs() > 1.0,
            "offset should move baked glyphs: {flat_y} -> {shifted_y}"
        );
        // Detach returns to a flat layout.
        app.apply(Action::DetachTextFromPath(tid));
        assert!(app.text_on_path.get(&tid).is_none());
        assert!(app.text_on_path_params.get(&tid).is_none());
    }

    fn text_centroid_y(s: &Shape) -> f32 {
        if let Shape::Text { glyphs, .. } = s {
            let mut sum = 0.0;
            let mut n = 0.0;
            for g in glyphs {
                for &(_, y) in &g.points {
                    sum += y;
                    n += 1.0;
                }
            }
            if n > 0.0 {
                sum / n
            } else {
                0.0
            }
        } else {
            0.0
        }
    }

    /// Applying perspective replaces a rect with a warped path whose top edge is
    /// narrower than its bottom edge (the default trapezoid).
    #[test]
    fn perspective_distort_warps_rect() {
        let mut app = App::new();
        app.doc.shapes.clear();
        app.doc.shapes.push(Shape::rect(
            [0.0, 0.0, 100.0, 100.0],
            app.default_fill,
            app.default_stroke,
            1.0,
        ));
        app.select_single(0);
        app.apply(Action::ApplyPerspectiveDistort { shape_id: 0 });
        // The rect is now a warped path; its top two points are inset.
        if let Shape::Path { points, .. } = &app.doc.shapes[0] {
            assert!(points.len() >= 4);
            let top_left = points[0];
            assert!(top_left.0 > 0.0, "top-left pulled inward: {top_left:?}");
        } else {
            panic!("expected a warped path");
        }
    }

    /// Outline-width-profile produces a separate filled band shape.
    #[test]
    fn outline_width_profile_emits_band() {
        let mut app = App::new();
        app.doc.shapes.clear();
        let mut line = Shape::path(
            vec![(0.0, 0.0), (100.0, 0.0)],
            Vec::new(),
            false,
            [0.0, 0.0, 0.0, 0.0],
            [0.0, 0.0, 0.0, 1.0],
            8.0,
        );
        if let Shape::Path { stroke_style, .. } = &mut line {
            stroke_style.width_profile = (1.0, 0.0); // taper to a point
        }
        app.doc.shapes.push(line);
        app.select_single(0);
        app.apply(Action::OutlineWidthProfile(0));
        assert_eq!(app.doc.shapes.len(), 2, "a band shape is inserted");
    }

    /// Roughen perturbs the selected shape's geometry.
    #[test]
    fn roughen_changes_geometry() {
        let mut app = App::new();
        app.doc.shapes.clear();
        app.doc.shapes.push(Shape::rect(
            [0.0, 0.0, 100.0, 100.0],
            app.default_fill,
            app.default_stroke,
            1.0,
        ));
        app.select_single(0);
        app.apply(Action::RoughenPath { size: 10.0, detail: 2 });
        // The rect is demoted to a roughened path with more vertices.
        if let Shape::Path { points, .. } = &app.doc.shapes[0] {
            assert!(points.len() > 4, "subdivided: {}", points.len());
        } else {
            panic!("expected a roughened path");
        }
    }

    /// MakeMeshGradient seeds 16 control points; clear empties them.
    #[test]
    fn mesh_gradient_seed_and_clear() {
        let mut app = App::new();
        app.doc.shapes.clear();
        app.doc.shapes.push(Shape::rect(
            [0.0, 0.0, 100.0, 100.0],
            [0.2, 0.4, 0.8, 1.0],
            app.default_stroke,
            1.0,
        ));
        app.select_single(0);
        app.apply(Action::MakeMeshGradient);
        assert_eq!(app.mesh_points.len(), 16);
        app.apply(Action::ClearMeshGradient);
        assert!(app.mesh_points.is_empty());
    }

}
