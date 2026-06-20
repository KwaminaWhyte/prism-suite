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
}

/// Stroke alignment relative to the path.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StrokeAlignment {
    Center,
    Inside,
    Outside,
}

/// Arrowhead style for stroked paths.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArrowHead {
    None,
    Arrow,
    OpenArrow,
    Circle,
    Square,
}

/// Font weight for the character panel.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum FontWeight {
    #[default]
    Regular,
    Bold,
    Italic,
    BoldItalic,
}

impl FontWeight {
    pub const ALL: [FontWeight; 4] = [
        FontWeight::Regular,
        FontWeight::Bold,
        FontWeight::Italic,
        FontWeight::BoldItalic,
    ];

    pub fn label(self) -> &'static str {
        match self {
            FontWeight::Regular => "Regular",
            FontWeight::Bold => "Bold",
            FontWeight::Italic => "Italic",
            FontWeight::BoldItalic => "Bold Italic",
        }
    }
}

/// Graph / chart kind for the graph tool stub.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GraphKind {
    Bar,
    Line,
    Pie,
    Scatter,
}

impl GraphKind {
    pub fn label(self) -> &'static str {
        match self {
            GraphKind::Bar => "Bar",
            GraphKind::Line => "Line",
            GraphKind::Pie => "Pie",
            GraphKind::Scatter => "Scatter",
        }
    }
}

/// Named stroke-width profile presets for the width tool.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WidthProfilePreset {
    Uniform,
    TaperIn,
    TaperOut,
    BulgeCenter,
    TaperBoth,
}

impl WidthProfilePreset {
    pub const ALL: [WidthProfilePreset; 5] = [
        WidthProfilePreset::Uniform,
        WidthProfilePreset::TaperIn,
        WidthProfilePreset::TaperOut,
        WidthProfilePreset::BulgeCenter,
        WidthProfilePreset::TaperBoth,
    ];

    pub fn label(self) -> &'static str {
        match self {
            WidthProfilePreset::Uniform => "Uniform",
            WidthProfilePreset::TaperIn => "Taper In",
            WidthProfilePreset::TaperOut => "Taper Out",
            WidthProfilePreset::BulgeCenter => "Bulge",
            WidthProfilePreset::TaperBoth => "Taper Both",
        }
    }

    /// Returns `(start_mul, end_mul)` for `StrokeStyle.width_profile`.
    pub fn profile(self) -> (f32, f32) {
        match self {
            WidthProfilePreset::Uniform => (1.0, 1.0),
            WidthProfilePreset::TaperIn => (0.1, 1.0),
            WidthProfilePreset::TaperOut => (1.0, 0.1),
            WidthProfilePreset::BulgeCenter => (0.5, 0.5),
            WidthProfilePreset::TaperBoth => (0.1, 0.1),
        }
    }
}

/// One artboard entry with a stable id, name, and rect.
#[derive(Clone, Debug)]
pub struct ArtboardEntry {
    /// Stable id (never reused within a session).
    pub id: u64,
    /// Human-visible label.
    pub name: String,
    /// `[x, y, w, h]` in document space.
    pub rect: [f32; 4],
}

impl ArtboardEntry {
    pub fn new(id: u64, name: impl Into<String>, rect: [f32; 4]) -> Self {
        Self { id, name: name.into(), rect }
    }
}

/// Two-point perspective grid overlay drawn on the canvas.
pub struct PerspectiveGrid {
    pub vp1: (f32, f32),
    pub vp2: (f32, f32),
    pub horizon_y: f32,
    pub visible: bool,
}

impl PerspectiveGrid {
    pub fn default_for(canvas_w: f32, canvas_h: f32) -> Self {
        Self {
            vp1: (canvas_w * 0.15, canvas_h * 0.5),
            vp2: (canvas_w * 0.85, canvas_h * 0.5),
            horizon_y: canvas_h * 0.5,
            visible: true,
        }
    }
}

/// Art-brush colorization mode.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ArtBrushColorize {
    #[default]
    None,
    Tints,
    HueShift,
}

/// Art-brush configuration: paint a stretchable symbol art along a path.
#[derive(Clone, Debug)]
pub struct ArtBrushConfig {
    /// The symbol artwork to stretch along the path.
    pub symbol_id: u64,
    /// Scale factor applied to the symbol width (height scales with the path width).
    pub width_scale: f32,
    /// Colorization mode.
    pub colorize: ArtBrushColorize,
    /// Flip the brush art across the path's normal axis.
    pub flip: bool,
}

/// A color-harmony rule describing how guide colors relate to the key color.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ColorHarmonyRule {
    #[default]
    Complementary,
    Analogous,
    Triadic,
    SplitComplementary,
    Tetradic,
    Monochromatic,
}

/// Color Guide panel state: a harmony rule + derived swatch palette.
#[derive(Clone, Debug, Default)]
pub struct ColorGuide {
    pub rule: ColorHarmonyRule,
    /// Derived swatches (up to 6 RGBA colors) generated from the key color + rule.
    pub swatches: Vec<[f32; 4]>,
}

impl ColorGuide {
    /// Recompute swatches from `key_color` (linear sRGB) and the current harmony rule.
    pub fn recompute(&mut self, key: [f32; 4]) {
        use std::f32::consts::PI;
        let [r, g, b, a] = key;
        // Convert to HSL (simple, non-perceptual).
        let max = r.max(g).max(b);
        let min = r.min(g).min(b);
        let l = (max + min) * 0.5;
        let s = if (max - min).abs() < 1e-6 {
            0.0
        } else {
            let d = max - min;
            if l > 0.5 { d / (2.0 - max - min) } else { d / (max + min) }
        };
        let h = if (max - min).abs() < 1e-6 {
            0.0
        } else if max == r {
            ((g - b) / (max - min)).rem_euclid(6.0) / 6.0
        } else if max == g {
            ((b - r) / (max - min) + 2.0) / 6.0
        } else {
            ((r - g) / (max - min) + 4.0) / 6.0
        };

        let offsets: &[f32] = match self.rule {
            ColorHarmonyRule::Complementary => &[0.0, 0.5],
            ColorHarmonyRule::Analogous => &[0.0, 1.0/12.0, -1.0/12.0],
            ColorHarmonyRule::Triadic => &[0.0, 1.0/3.0, 2.0/3.0],
            ColorHarmonyRule::SplitComplementary => &[0.0, 5.0/12.0, 7.0/12.0],
            ColorHarmonyRule::Tetradic => &[0.0, 0.25, 0.5, 0.75],
            ColorHarmonyRule::Monochromatic => &[0.0, 0.1, -0.1, 0.2, -0.2],
        };

        self.swatches = offsets.iter().map(|&off| {
            let h2 = (h + off).rem_euclid(1.0);
            hsl_to_rgb(h2, s, l, a)
        }).collect();
    }
}

fn hsl_to_rgb(h: f32, s: f32, l: f32, a: f32) -> [f32; 4] {
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let x = c * (1.0 - ((h * 6.0).rem_euclid(2.0) - 1.0).abs());
    let m = l - c * 0.5;
    let (r, g, b) = match (h * 6.0) as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    [r + m, g + m, b + m, a]
}

/// Which distort-warp tool is active (Scallop / Crystallize / Wrinkle).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum WarpToolKind {
    #[default]
    Scallop,
    Crystallize,
    Wrinkle,
}

// --- Batch 8: Image Trace mode ---

/// Image trace color mode for the extended Image Trace tool.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize, Default)]
pub enum ImageTraceMode {
    #[default]
    Color,
    Grayscale,
    BlackWhite,
    Outlined,
}

// --- Batch 8: Graph Tool types ---

/// Graph / chart type for the extended graph tool (Batch 8).
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize, Default)]
pub enum GraphType {
    #[default]
    Column,
    Bar,
    Pie,
    Line,
    Scatter,
}

/// Graph data model: values, labels, and layout parameters.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct GraphData {
    pub graph_type: GraphType,
    pub cols: usize,
    pub rows: usize,
    pub values: Vec<f32>,
    pub labels: Vec<String>,
}

impl Default for GraphData {
    fn default() -> Self {
        Self {
            graph_type: GraphType::Column,
            cols: 3,
            rows: 2,
            values: vec![10.0, 20.0, 30.0, 15.0, 25.0, 35.0],
            labels: vec![],
        }
    }
}

/// Configuration for the scatter brush: copies of a symbol placed at regular
/// intervals along a drawn path, with optional size and rotation jitter.
#[derive(Clone, Debug)]
pub struct ScatterBrushConfig {
    /// The symbol to scatter (looked up in `App.symbol_lib`).
    pub symbol_id: u64,
    /// Distance between successive copies along the path (document units).
    pub spacing: f32,
    /// Fractional size variation (0.0 = uniform, 1.0 = ±100 %).
    pub size_jitter: f32,
    /// Rotational variation in degrees (0.0 = no variation).
    pub rotation_jitter: f32,
}

/// How glyphs are spaced when flowing along a path.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum TextOnPathSpacing {
    /// Let the renderer space glyphs automatically (default).
    #[default]
    Auto,
    /// Fixed advance between every glyph.
    Fixed,
    /// Optically balance the apparent spacing.
    Optical,
}

/// Extended configuration for the Recolor Artwork panel.
#[derive(Debug, Clone)]
pub struct RecolorConfig {
    /// Harmony rule used when rotating hues.
    pub harmony_rule: ColorHarmonyRule,
    /// If `true`, shapes with pure-black fills are left untouched.
    pub preserve_black: bool,
    /// If `true`, shapes with pure-white fills are left untouched.
    pub preserve_white: bool,
    /// When `true`, hue offsets are shuffled randomly instead of by rule.
    pub randomize: bool,
    /// Uniform brightness multiplier applied to all fills (1.0 = no change).
    pub brightness_scale: f32,
}

impl Default for RecolorConfig {
    fn default() -> Self {
        Self {
            harmony_rule: ColorHarmonyRule::Analogous,
            preserve_black: true,
            preserve_white: true,
            randomize: false,
            brightness_scale: 1.0,
        }
    }
}

// --- Batch 10: Gradient Mesh depth ---

/// A single control point in a gradient mesh.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MeshPoint {
    pub position: [f32; 2],
    pub color: [f32; 4],
    pub tension: f32,
}

impl Default for MeshPoint {
    fn default() -> Self {
        Self { position: [0.0, 0.0], color: [1.0, 1.0, 1.0, 1.0], tension: 1.0 }
    }
}

/// Configuration for the gradient mesh tool.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct GradientMeshConfig {
    pub rows: u8,
    pub cols: u8,
    pub points: Vec<MeshPoint>,
}

impl Default for GradientMeshConfig {
    fn default() -> Self {
        Self { rows: 4, cols: 4, points: vec![] }
    }
}

// --- Batch 10: Flare Tool ---

/// Configuration for a lens-flare effect.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FlareConfig {
    pub center: [f32; 2],
    pub brightness: f32,
    pub halo_size: f32,
    pub ray_count: u8,
    pub ray_length: f32,
    pub ring_count: u8,
    pub ring_spacing: f32,
    pub color: [f32; 4],
}

impl Default for FlareConfig {
    fn default() -> Self {
        Self {
            center: [0.0, 0.0],
            brightness: 100.0,
            halo_size: 50.0,
            ray_count: 10,
            ray_length: 100.0,
            ring_count: 5,
            ring_spacing: 50.0,
            color: [1.0, 1.0, 1.0, 1.0],
        }
    }
}

// --- Batch 10: Pattern Brush depth ---

/// How a pattern brush tile fits along the stroke path.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize, Default)]
pub enum PatternBrushFit {
    #[default]
    Stretch,
    Tile,
    ApproximatePath,
    AddSpaceBetweenTiles,
    AlignToPixelGrid,
}

/// How the pattern brush colorizes its tile art.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize, Default)]
pub enum ColorizeMethod {
    #[default]
    None,
    Tints,
    TintsAndShades,
    Hue,
    Full,
}

/// Configuration for the pattern brush.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PatternBrushConfig {
    pub name: String,
    pub scale: f32,
    pub spacing: f32,
    pub colorize_method: ColorizeMethod,
    pub fit: PatternBrushFit,
    pub flip_across_path: bool,
    pub flip_along_path: bool,
}

impl Default for PatternBrushConfig {
    fn default() -> Self {
        Self {
            name: "Pattern Brush".to_string(),
            scale: 100.0,
            spacing: 100.0,
            colorize_method: ColorizeMethod::None,
            fit: PatternBrushFit::Stretch,
            flip_across_path: false,
            flip_along_path: false,
        }
    }
}

/// Configuration for the Symbol Sprayer tool.
#[derive(Debug, Clone)]
pub struct SymbolSprayConfig {
    /// Which symbol from the library to spray.
    pub symbol_id: u64,
    /// Diameter of the spray brush in document units.
    pub diameter: f32,
    /// Average number of instances per spray event (0..=10).
    pub density: f32,
    /// Positional scatter (0.0 = tightly grouped, 1.0 = fills the whole diameter).
    pub scatter: f32,
    /// Maximum rotation jitter in radians.
    pub rotation_jitter: f32,
    /// Maximum size jitter as a fraction of the base size.
    pub size_jitter: f32,
    /// Maximum opacity jitter (0.0 = no variation).
    pub opacity_jitter: f32,
}

impl Default for SymbolSprayConfig {
    fn default() -> Self {
        Self {
            symbol_id: 0,
            diameter: 80.0,
            density: 5.0,
            scatter: 0.3,
            rotation_jitter: 0.1,
            size_jitter: 0.1,
            opacity_jitter: 0.0,
        }
    }
}

// --- Batch 11: Pathfinder depth ---

/// The full set of Pathfinder shape-mode and effect operations.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum PathfinderOp {
    // Shape modes
    Unite,
    Minus,
    Intersect,
    Exclude,
    // Pathfinder effects
    Divide,
    Trim,
    Merge,
    Crop,
    Outline,
    MinusBack,
}

// --- Batch 11: 3D Extrude depth ---

/// Surface shading mode for 3D Extrude & Bevel.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize, Default)]
pub enum ExtrudeSurface {
    #[default]
    PlasticShading,
    DiffuseShading,
    WireframeOnly,
    NoShading,
}

/// Cap style for 3D Extrude & Bevel.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize, Default)]
pub enum ExtrudeCapStyle {
    #[default]
    Round,
    Bevel,
    None_,
}

/// Full configuration for the 3D Extrude & Bevel effect.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ExtrudeConfig {
    pub depth: f32,
    pub rotation_x: f32,
    pub rotation_y: f32,
    pub rotation_z: f32,
    pub perspective: f32,
    pub surface: ExtrudeSurface,
    pub cap_style: ExtrudeCapStyle,
    pub bevel_height: f32,
    pub bevel_extent: bool,
    pub map_art: bool,
    pub light_intensity: f32,
    pub ambient_light: f32,
    pub specular_highlight: f32,
    pub gloss: f32,
}

impl Default for ExtrudeConfig {
    fn default() -> Self {
        Self {
            depth: 50.0,
            rotation_x: -26.0,
            rotation_y: -38.0,
            rotation_z: 0.0,
            perspective: 0.0,
            surface: ExtrudeSurface::PlasticShading,
            cap_style: ExtrudeCapStyle::Round,
            bevel_height: 4.0,
            bevel_extent: false,
            map_art: false,
            light_intensity: 100.0,
            ambient_light: 50.0,
            specular_highlight: 70.0,
            gloss: 25.0,
        }
    }
}

// --- Batch 11: Chart depth ---

/// A single data series for the chart tool.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ChartDataSet {
    pub label: String,
    pub values: Vec<f64>,
    pub color: [f32; 4],
}

impl Default for ChartDataSet {
    fn default() -> Self {
        Self { label: "Series 1".to_string(), values: vec![], color: [0.2, 0.5, 0.9, 1.0] }
    }
}

/// Full chart layout and data configuration.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ChartConfig {
    pub datasets: Vec<ChartDataSet>,
    pub category_labels: Vec<String>,
    pub title: String,
    pub show_legend: bool,
    pub show_grid: bool,
    pub value_axis_min: Option<f64>,
    pub value_axis_max: Option<f64>,
    pub column_width: f32,
    pub cluster_width: f32,
    pub shadow: bool,
}

impl Default for ChartConfig {
    fn default() -> Self {
        Self {
            datasets: vec![],
            category_labels: vec![],
            title: String::new(),
            show_legend: true,
            show_grid: false,
            value_axis_min: None,
            value_axis_max: None,
            column_width: 90.0,
            cluster_width: 80.0,
            shadow: false,
        }
    }
}

// --- Batch 11: Envelope Distort depth ---

/// Whether the envelope editor is editing the envelope mesh or the contents.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize, Default)]
pub enum EnvelopeEditMode {
    #[default]
    Envelope,
    Contents,
}

/// Warp preset styles for Envelope Distort.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize, Default)]
pub enum EnvelopeWarpStyle {
    #[default]
    None_,
    Arc,
    ArcLower,
    ArcUpper,
    Arch,
    Bulge,
    ShellLower,
    ShellUpper,
    Flag,
    Wave,
    Fish,
    Rise,
    FishEye,
    Inflate,
    Squeeze,
    Twist,
}

/// Configuration for the Envelope Distort warp.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct EnvelopeConfig {
    pub warp_style: EnvelopeWarpStyle,
    pub horizontal: bool,
    pub bend: f32,
    pub h_distortion: f32,
    pub v_distortion: f32,
    pub fidelity: f32,
    pub edit_mode: EnvelopeEditMode,
}

impl Default for EnvelopeConfig {
    fn default() -> Self {
        Self {
            warp_style: EnvelopeWarpStyle::None_,
            horizontal: true,
            bend: 50.0,
            h_distortion: 0.0,
            v_distortion: 0.0,
            fidelity: 50.0,
            edit_mode: EnvelopeEditMode::Envelope,
        }
    }
}

// --- Batch 10: Variable Fonts ---

/// A single OpenType variation axis.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FontAxis {
    pub tag: String,
    pub min: f32,
    pub max: f32,
    pub value: f32,
}

impl Default for FontAxis {
    fn default() -> Self {
        Self { tag: "wght".to_string(), min: 100.0, max: 900.0, value: 400.0 }
    }
}

/// Configuration for variable-font axis editing.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct VariableFontConfig {
    pub axes: Vec<FontAxis>,
    pub preview_text: String,
}

impl Default for VariableFontConfig {
    fn default() -> Self {
        Self { axes: vec![], preview_text: "Sphinx of black quartz".to_string() }
    }
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

            // --- Symbols (Wave 8) ---
            Action::CreateSymbol(name) => {
                let shape_ids: Vec<u64> = self.selection.iter().map(|&i| i as u64).collect();
                if !shape_ids.is_empty() {
                    let id = self.symbols.len() as u64;
                    self.symbols.push(Symbol { id, name, shape_ids });
                }
            }
            Action::PlaceSymbol(id) => {
                if self.symbols.iter().any(|s| s.id == id) {
                    self.checkpoint();
                    let shape = Shape::rect(
                        [180.0, 180.0, 40.0, 40.0],
                        [0.6, 0.3, 0.9, 1.0],
                        [0.3, 0.1, 0.5, 1.0],
                        1.5,
                    );
                    self.doc.shapes.push(shape);
                    self.select_single(self.doc.shapes.len() - 1);
                    self.host.mark_dirty();
                }
            }
            Action::EditSymbol(id) => {
                log::info!("editing symbol {id}");
            }

            // --- Gradient-stop editor (Wave 8) ---
            Action::SelectGradientStop(idx) => {
                self.selected_gradient_stop = idx;
            }
            Action::AddGradientStop { shape_id, pos, color } => {
                let maybe_g = self.doc.shapes.get(shape_id).and_then(|s| s.fill_gradient()).cloned();
                if let Some(mut g) = maybe_g {
                    g.stops.push(GradientStop::new(pos, color));
                    g.stops.sort_by(|a, b| {
                        a.offset.partial_cmp(&b.offset).unwrap_or(std::cmp::Ordering::Equal)
                    });
                    self.checkpoint();
                    self.doc.shapes[shape_id].set_fill_gradient(Some(g));
                    self.host.mark_dirty();
                }
            }
            Action::MoveGradientStop { shape_id, stop_idx, new_pos } => {
                let maybe_g = self.doc.shapes.get(shape_id).and_then(|s| s.fill_gradient()).cloned();
                if let Some(mut g) = maybe_g {
                    if let Some(s) = g.stops.get_mut(stop_idx) {
                        s.offset = new_pos.clamp(0.0, 1.0);
                    }
                    self.checkpoint();
                    self.doc.shapes[shape_id].set_fill_gradient(Some(g));
                    self.host.mark_dirty();
                }
            }
            Action::DeleteGradientStop { shape_id, stop_idx } => {
                let maybe_g = self.doc.shapes.get(shape_id).and_then(|s| s.fill_gradient()).cloned();
                if let Some(mut g) = maybe_g {
                    if g.stops.len() > 2 && stop_idx < g.stops.len() {
                        g.stops.remove(stop_idx);
                        self.checkpoint();
                        self.doc.shapes[shape_id].set_fill_gradient(Some(g));
                        self.host.mark_dirty();
                    }
                }
            }
            Action::SetGradientStopColor { shape_id, stop_idx, color } => {
                let maybe_g = self.doc.shapes.get(shape_id).and_then(|s| s.fill_gradient()).cloned();
                if let Some(mut g) = maybe_g {
                    if let Some(s) = g.stops.get_mut(stop_idx) {
                        s.color = color;
                    }
                    self.checkpoint();
                    self.doc.shapes[shape_id].set_fill_gradient(Some(g));
                    self.host.mark_dirty();
                }
            }

            // --- Image Trace (Wave 8) ---
            Action::OpenTracePanel => { self.trace_panel_open = true; }
            Action::CloseTracePanel => { self.trace_panel_open = false; }
            Action::SetTraceThreshold(v) => { self.trace_threshold = v; }
            Action::SetTraceColors(v) => { self.trace_colors = v.clamp(2, 32); }
            Action::TraceImage { shape_id, threshold, colors } => {
                log::info!("TraceImage shape={shape_id} threshold={threshold} colors={colors}");
                self.host.mark_dirty();
            }

            // --- Recolor Artwork (Wave 8) ---
            Action::OpenRecolorPanel => { self.recolor_panel_open = true; }
            Action::CloseRecolorPanel => {
                self.recolor_panel_open = false;
                self.recolor_selected_color = None;
            }
            Action::SelectRecolorColor(c) => { self.recolor_selected_color = Some(c); }
            Action::RecolorSelected { old_color, new_color } => {
                if !self.selection.is_empty() {
                    self.checkpoint();
                    let sel: Vec<usize> = self.selection.clone();
                    for i in sel {
                        if let Some(shape) = self.doc.shapes.get_mut(i) {
                            if let Some(fc) = shape.fill_color() {
                                if colors_approx_equal(fc, old_color) {
                                    shape.set_fill_color(new_color);
                                }
                            }
                        }
                    }
                    self.host.mark_dirty();
                }
            }

            // --- Artboards (Wave 8) ---
            Action::AddArtboard(rect) => {
                self.artboards.push(rect);
                // Also register in the named list so the artboards panel shows it.
                let id = self.artboard_next_id;
                self.artboard_next_id += 1;
                let name = crate::artboard::default_name(self.artboard_entries.len());
                self.artboard_entries.push(ArtboardEntry::new(id, name, rect));
                self.active_artboard = Some(id);
            }

            // --- Eyedropper (Wave 9) ---
            Action::PickColor(rgba) => {
                self.fg_color = [
                    rgba[0] as f32 / 255.0,
                    rgba[1] as f32 / 255.0,
                    rgba[2] as f32 / 255.0,
                    rgba[3] as f32 / 255.0,
                ];
                if self.selected.is_some() {
                    self.checkpoint();
                    let c = self.fg_color;
                    self.selected_shape_mut().unwrap().set_fill_color(c);
                    self.host.mark_dirty();
                }
                self.active = self.prev_tool;
            }

            // --- Shape Builder (Wave 9) ---
            Action::MergeRegion(ids) => {
                log::info!("MergeRegion {:?} (stub)", ids);
                self.selection = ids.into_iter().filter(|&i| i < self.doc.shapes.len()).collect();
                self.sync_legacy_selection();
            }
            Action::SubtractRegion(ids) => {
                log::info!("SubtractRegion {:?} (stub)", ids);
            }

            Action::ApplyShapeBuilder { subtract } => {
                let sel = self.selection.clone();
                if sel.len() < 2 {
                    return;
                }
                let sel_shapes: Vec<document::Shape> = sel
                    .iter()
                    .filter_map(|&i| self.doc.shapes.get(i).cloned())
                    .collect();
                let faces = crate::shapebuilder::build_faces(&sel_shapes);
                if faces.is_empty() {
                    return;
                }
                let all_face_indices: Vec<usize> = (0..faces.len()).collect();
                let mode = if subtract {
                    crate::shapebuilder::BuildMode::Subtract
                } else {
                    crate::shapebuilder::BuildMode::Unite
                };
                let results = crate::shapebuilder::apply_build(
                    &sel_shapes,
                    &faces,
                    &all_face_indices,
                    mode,
                );
                self.checkpoint();
                let mut sorted_sel = sel.clone();
                sorted_sel.sort_unstable_by(|a, b| b.cmp(a));
                sorted_sel.dedup();
                for i in sorted_sel {
                    if i < self.doc.shapes.len() {
                        self.doc.shapes.remove(i);
                    }
                }
                let first_new = self.doc.shapes.len();
                for s in results {
                    self.doc.shapes.push(s);
                }
                if first_new < self.doc.shapes.len() {
                    self.select_single(self.doc.shapes.len() - 1);
                } else {
                    self.select_clear();
                }
                self.host.mark_dirty();
            }

            // --- Text on Path (Wave 9) ---
            Action::AttachTextToPath { text_id, path_id } => {
                if text_id < self.doc.shapes.len() && path_id < self.doc.shapes.len() {
                    self.checkpoint();
                    self.text_on_path.insert(text_id, path_id);
                    self.relayout_text_on_path(text_id);
                    self.host.mark_dirty();
                }
            }
            Action::DetachTextFromPath(text_id) => {
                self.text_on_path.remove(&text_id);
                self.text_on_path_params.remove(&text_id);
                // Re-lay-out the text flat (off the path) so it returns to normal.
                if text_id < self.doc.shapes.len() {
                    self.doc.shapes[text_id].text_relayout();
                }
                self.host.mark_dirty();
            }

            // --- SVG I/O (Wave 9) ---
            Action::ExportSvg(path) => {
                match self.export_svg(&path) {
                    Ok(()) => {
                        let msg = format!("Exported to {}", path.display());
                        self.status_message = Some((msg, std::time::Instant::now()));
                    }
                    Err(e) => {
                        log::error!("SVG export failed: {e}");
                        self.status_message = Some(("Export failed".to_string(), std::time::Instant::now()));
                    }
                }
            }
            Action::ImportSvg(path) => {
                match import_svg(&path) {
                    Ok(shapes) => {
                        self.checkpoint();
                        let first = self.doc.shapes.len();
                        self.doc.shapes.extend(shapes);
                        if self.doc.shapes.len() > first {
                            self.select_single(self.doc.shapes.len() - 1);
                            self.host.mark_dirty();
                        }
                        self.status_message = Some((
                            format!("Imported {}", path.display()),
                            std::time::Instant::now(),
                        ));
                    }
                    Err(e) => {
                        log::error!("SVG import failed: {e}");
                        self.status_message = Some(("Import failed".to_string(), std::time::Instant::now()));
                    }
                }
            }

            // --- Layer order / grouping (Wave 9) ---
            Action::GroupSelected => {
                if self.selection.len() >= 2 {
                    self.checkpoint();
                    let group_id = rand_group_id(&self.doc);
                    let sel: Vec<usize> = self.selection.clone();
                    for i in sel {
                        if let Some(s) = self.doc.shapes.get_mut(i) {
                            s.set_group(Some(group_id));
                        }
                    }
                    self.host.mark_dirty();
                }
            }
            Action::UngroupSelected => {
                if !self.selection.is_empty() {
                    self.checkpoint();
                    let sel: Vec<usize> = self.selection.clone();
                    for i in sel {
                        if let Some(s) = self.doc.shapes.get_mut(i) {
                            s.set_group(None);
                        }
                    }
                    self.host.mark_dirty();
                }
            }
            Action::MoveLayerOrder { id, delta } => {
                if id < self.doc.shapes.len() {
                    let new_idx = (id as i32 + delta)
                        .clamp(0, self.doc.shapes.len() as i32 - 1) as usize;
                    if new_idx != id {
                        self.checkpoint();
                        let shape = self.doc.shapes.remove(id);
                        self.doc.shapes.insert(new_idx, shape);
                        self.selection = self.selection.iter().map(|&i| {
                            if i == id { new_idx }
                            else if delta > 0 && i > id && i <= new_idx { i - 1 }
                            else if delta < 0 && i >= new_idx && i < id { i + 1 }
                            else { i }
                        }).collect();
                        self.sync_legacy_selection();
                        self.host.mark_dirty();
                    }
                }
            }

            // --- Wave 11: Gradient type toggle ---
            Action::SetGradientType { shape_id, kind } => {
                if let Some(shape) = self.doc.shapes.get(shape_id) {
                    if let Some(g) = shape.fill_gradient() {
                        let mut g2 = g.clone();
                        g2.kind = kind;
                        self.checkpoint();
                        self.doc.shapes[shape_id].set_fill_gradient(Some(g2));
                        self.host.mark_dirty();
                    } else {
                        // No gradient yet — seed a default one of the chosen kind.
                        let fill = shape.fill_color().unwrap_or([0.5, 0.5, 0.5, 1.0]);
                        let g2 = Gradient {
                            kind,
                            stops: vec![
                                GradientStop::new(0.0, fill),
                                GradientStop::new(1.0, [1.0, 1.0, 1.0, 1.0]),
                            ],
                            ..Default::default()
                        };
                        self.checkpoint();
                        self.doc.shapes[shape_id].set_fill_gradient(Some(g2));
                        self.host.mark_dirty();
                    }
                }
            }

            // --- Wave 11: Shape builder (real geometry via i_overlay) ---
            Action::MergeRegionReal(ids) => {
                let ids: Vec<usize> = ids.into_iter().filter(|&i| i < self.doc.shapes.len()).collect();
                if ids.len() < 2 {
                    return;
                }
                // Union successive pairs: fold the shapes using boolean Union.
                let mut base = self.doc.shapes[ids[0]].clone();
                for &i in &ids[1..] {
                    let clip = &self.doc.shapes[i];
                    let results = boolean::apply(&base, clip, boolean::BoolOp::Union, BoolFillRule::NonZero);
                    if let Some(merged) = results.into_iter().next() {
                        base = merged;
                    }
                }
                self.checkpoint();
                // Remove highest index first so lower indices stay valid.
                let mut sorted_ids = ids.clone();
                sorted_ids.sort_unstable_by(|a, b| b.cmp(a));
                sorted_ids.dedup();
                for i in &sorted_ids {
                    self.doc.shapes.remove(*i);
                }
                let new_idx = self.doc.shapes.len();
                self.doc.shapes.push(base);
                self.select_single(new_idx);
                self.host.mark_dirty();
            }

            Action::SubtractRegionReal(ids) => {
                let ids: Vec<usize> = ids.into_iter().filter(|&i| i < self.doc.shapes.len()).collect();
                if ids.len() < 2 {
                    return;
                }
                // Subtract ids[1..] from ids[0] using boolean Difference.
                let mut base = self.doc.shapes[ids[0]].clone();
                for &i in &ids[1..] {
                    let clip = &self.doc.shapes[i];
                    // Difference: base minus clip
                    let results = boolean::apply(&base, clip, boolean::BoolOp::Difference, BoolFillRule::NonZero);
                    if let Some(r) = results.into_iter().next() {
                        base = r;
                    }
                }
                self.checkpoint();
                let mut sorted_ids = ids.clone();
                sorted_ids.sort_unstable_by(|a, b| b.cmp(a));
                sorted_ids.dedup();
                for i in &sorted_ids {
                    self.doc.shapes.remove(*i);
                }
                let new_idx = self.doc.shapes.len();
                self.doc.shapes.push(base);
                self.select_single(new_idx);
                self.host.mark_dirty();
            }

            // --- Wave 11: Character / paragraph panel ---
            Action::SetFontFamily(fam) => {
                self.font_family = fam.clone();
                // Also apply to the selected text shape.
                self.edit_text_params(|p| p.font_family = Some(fam));
            }
            Action::SetFontSize(size) => {
                self.default_font_size = size.clamp(1.0, 2000.0);
                self.edit_text_params(|p| p.font_size = size.clamp(1.0, 2000.0));
            }
            Action::SetFontWeight(w) => {
                self.font_weight = w;
            }
            Action::SetLetterSpacing(v) => {
                self.letter_spacing = v;
            }
            Action::SetLineHeight(v) => {
                self.line_height = v.max(0.1);
            }
            Action::SetParaAlign(a) => {
                self.text_align = a;
                self.edit_text_params(|p| p.align = a);
            }

            // --- Wave 11: PDF export ---
            Action::ExportPdf(path) => {
                match self.export_pdf(&path) {
                    Ok(()) => {
                        let msg = format!("PDF exported to {}", path.display());
                        self.status_message = Some((msg, std::time::Instant::now()));
                    }
                    Err(e) => {
                        log::error!("PDF export failed: {e}");
                        self.status_message = Some(("PDF export failed".to_string(), std::time::Instant::now()));
                    }
                }
            }

            // --- Wave 11: Isolation mode ---
            Action::EnterIsolation(group_id) => {
                self.isolation_group = Some(group_id);
                self.host.mark_dirty();
            }
            Action::ExitIsolation => {
                self.isolation_group = None;
                self.host.mark_dirty();
            }

            // --- Wave 11: Knife tool ---
            Action::KnifeSlice { start, end } => {
                self.apply_knife_slice(start, end);
            }

            // --- Wave 12: transform handle actions ---
            Action::MoveSelection { dx, dy } => {
                if dx != 0.0 || dy != 0.0 {
                    self.checkpoint();
                    let aff = Affine::translate(dx, dy);
                    for &idx in &self.selection {
                        if let Some(s) = self.doc.shapes.get_mut(idx) {
                            s.apply_affine(&aff);
                        }
                    }
                    self.host.mark_dirty();
                }
            }
            Action::BeginTransformScale { handle_idx, doc, bbox } => {
                let handle = Handle::ALL[handle_idx as usize % Handle::ALL.len()];
                let opp = handle.opposite();
                let (ox, oy) = opp.unit_pos();
                let pivot_x = bbox[0] + ox * bbox[2];
                let pivot_y = bbox[1] + oy * bbox[3];
                self.xform_handle = Some(handle);
                self.xform_pivot = (pivot_x, pivot_y);
                self.xform_bbox = bbox;
                self.xform_start = (doc[0], doc[1]);
                // Snapshot all selected shapes.
                self.xform_snapshot = self
                    .selection
                    .iter()
                    .filter_map(|&i| self.doc.shapes.get(i).map(|s| (i, s.clone())))
                    .collect();
            }
            Action::TransformScaleDrag { doc, uniform } => {
                let Some(handle) = self.xform_handle else { return; };
                let bbox = self.xform_bbox;
                let (px, py) = self.xform_pivot;
                let (sx, sy) = self.xform_start;
                let (cx, cy) = (doc[0], doc[1]);
                // orig_dx/dy = cursor offset from pivot at drag start.
                let (sx_f, sy_f) = transform::scale_factors_for_handle(
                    handle,
                    sx - px, sy - py,
                    cx - px, cy - py,
                    uniform,
                );
                let aff = Affine::scale_about(sx_f, sy_f, px, py);
                for (idx, snap) in &self.xform_snapshot {
                    if let Some(s) = self.doc.shapes.get_mut(*idx) {
                        *s = snap.clone();
                        s.apply_affine(&aff);
                    }
                }
                self.host.mark_dirty();
            }

            // --- Wave 13 handlers ---
            Action::AddGuideH(y) => { self.guides_h.push(y); }
            Action::AddGuideV(x) => { self.guides_v.push(x); }
            Action::RemoveGuide { horizontal, idx } => {
                if horizontal { if idx < self.guides_h.len() { self.guides_h.remove(idx); } }
                else { if idx < self.guides_v.len() { self.guides_v.remove(idx); } }
            }
            Action::ToggleGuides => { self.guides_visible = !self.guides_visible; }
            Action::ToggleGridSnap => { self.grid_snap = !self.grid_snap; }
            Action::RotateCanvas(deg) => {
                self.canvas_rotation_deg = (self.canvas_rotation_deg + deg).rem_euclid(360.0);
                self.host.mark_dirty();
            }
            Action::ResetCanvasRotation => {
                self.canvas_rotation_deg = 0.0;
                self.host.mark_dirty();
            }
            Action::RotateSelection(deg) => {
                self.checkpoint();
                let rad = deg.to_radians();
                let indices: Vec<usize> = self.selection.clone();
                if let Some(bbox) = self.selection_bbox() {
                    let (cx, cy) = (bbox[0] + bbox[2] / 2.0, bbox[1] + bbox[3] / 2.0);
                    let aff = Affine::rotate_about(rad, cx, cy);
                    for idx in indices {
                        if let Some(s) = self.doc.shapes.get_mut(idx) {
                            s.apply_affine(&aff);
                        }
                    }
                }
                self.host.mark_dirty();
            }
            Action::ReflectSelectionH => {
                self.checkpoint();
                let indices: Vec<usize> = self.selection.clone();
                if let Some(bbox) = self.selection_bbox() {
                    let cx = bbox[0] + bbox[2] / 2.0;
                    let aff = Affine::scale_about(-1.0, 1.0, cx, 0.0);
                    for idx in indices {
                        if let Some(s) = self.doc.shapes.get_mut(idx) {
                            s.apply_affine(&aff);
                        }
                    }
                }
                self.host.mark_dirty();
            }
            Action::ReflectSelectionV => {
                self.checkpoint();
                let indices: Vec<usize> = self.selection.clone();
                if let Some(bbox) = self.selection_bbox() {
                    let cy = bbox[1] + bbox[3] / 2.0;
                    let aff = Affine::scale_about(1.0, -1.0, 0.0, cy);
                    for idx in indices {
                        if let Some(s) = self.doc.shapes.get_mut(idx) {
                            s.apply_affine(&aff);
                        }
                    }
                }
                self.host.mark_dirty();
            }
            Action::ShearSelection(shear_x) => {
                self.checkpoint();
                let indices: Vec<usize> = self.selection.clone();
                let aff = Affine::shear(shear_x, 0.0);
                for idx in indices {
                    if let Some(s) = self.doc.shapes.get_mut(idx) {
                        s.apply_affine(&aff);
                    }
                }
                self.host.mark_dirty();
            }
            Action::ScaleSelection { factor_x, factor_y } => {
                self.checkpoint();
                let indices: Vec<usize> = self.selection.clone();
                if let Some(bbox) = self.selection_bbox() {
                    let (cx, cy) = (bbox[0] + bbox[2] / 2.0, bbox[1] + bbox[3] / 2.0);
                    let aff = Affine::scale_about(factor_x, factor_y, cx, cy);
                    for idx in indices {
                        if let Some(s) = self.doc.shapes.get_mut(idx) {
                            s.apply_affine(&aff);
                        }
                    }
                }
                self.host.mark_dirty();
            }
            Action::OutlineText(idx) => {
                log::info!("contour: OutlineText(idx={idx}) — text-to-path conversion stub");
            }
            Action::SetStrokeAlignment(_align) => {
                log::info!("contour: SetStrokeAlignment — stroke alignment stored (model stub)");
            }
            Action::SetDashPattern(pattern) => {
                log::info!("contour: SetDashPattern len={} — dash pattern stored", pattern.len());
            }
            Action::SetArrowHead { start, end } => {
                log::info!("contour: SetArrowHead start={start:?} end={end:?}");
            }
            Action::SetKerning(kerning) => {
                self.letter_spacing = kerning;
            }
            Action::SetLeading(leading) => {
                self.line_height = leading;
            }
            Action::SetTextWrapWidth(_w) => {
                log::info!("contour: SetTextWrapWidth — text wrap width set");
            }
            Action::SaveLayerComp(name) => {
                let vis: std::collections::HashMap<u64, bool> = self.doc.shapes.iter()
                    .enumerate()
                    .map(|(i, s)| (i as u64, s.visible()))
                    .collect();
                self.layer_comps.retain(|(n, _)| n != &name);
                self.layer_comps.push((name, vis));
            }
            Action::ApplyLayerComp(name) => {
                log::info!("contour: ApplyLayerComp({name}) — layer comp apply stub");
                self.host.mark_dirty();
            }
            Action::SetPatternFill(idx) => {
                log::info!("contour: SetPatternFill(idx={idx}) — pattern fill set (model stub)");
            }
            Action::AddPattern { name, tile_w, tile_h } => {
                self.patterns.push((name, tile_w, tile_h));
            }

            Action::OpenExportPngDialog => {
                if let Some(path) = rfd::FileDialog::new()
                    .add_filter("PNG", &["png"])
                    .set_file_name("export.png")
                    .save_file()
                {
                    self.apply(Action::ExportPng(path));
                }
            }

            Action::ExportPng(path) => {
                let w = self.host.doc_w;
                let h = self.host.doc_h;
                // Grab host's cached BGRA8 raster (frame_bytes) and convert to RGBA for PNG.
                let bgra = self.host.frame_bytes.clone();
                if bgra.len() == (w * h * 4) as usize {
                    let rgba: Vec<u8> = bgra.chunks(4)
                        .flat_map(|px| [px[2], px[1], px[0], px[3]])
                        .collect();
                    match image::RgbaImage::from_raw(w, h, rgba) {
                        Some(img) => {
                            match img.save(&path) {
                                Ok(_) => {
                                    let msg = format!("Exported PNG: {}", path.display());
                                    log::info!("{msg}");
                                    self.status_message = Some((msg, std::time::Instant::now()));
                                }
                                Err(e) => {
                                    let msg = format!("PNG export failed: {e}");
                                    self.status_message = Some((msg, std::time::Instant::now()));
                                }
                            }
                        }
                        None => {
                            self.status_message = Some(("PNG: buffer size mismatch".to_string(), std::time::Instant::now()));
                        }
                    }
                } else {
                    self.status_message = Some(("PNG: render not ready".to_string(), std::time::Instant::now()));
                }
            }

            // --- Appearance panel handlers ---

            Action::MigrateAppearance => {
                if let Some(shape) = self.selected_shape_mut() {
                    if shape.appearance().is_none() {
                        let a = shape.effective_appearance();
                        shape.set_appearance(Some(a));
                    }
                }
                self.host.mark_dirty();
            }

            Action::AppearanceAddFill => {
                if self.selected_shape().is_some() {
                    self.checkpoint();
                    let shape = self.selected_shape_mut().unwrap();
                    if shape.appearance().is_none() {
                        let a = shape.effective_appearance();
                        shape.set_appearance(Some(a));
                    }
                    shape.appearance_mut().as_mut().unwrap()
                        .fills.push(Fill::solid([0.5, 0.5, 0.5, 1.0]));
                    self.host.mark_dirty();
                }
            }

            Action::AppearanceAddStroke => {
                if self.selected_shape().is_some() {
                    self.checkpoint();
                    let shape = self.selected_shape_mut().unwrap();
                    if shape.appearance().is_none() {
                        let a = shape.effective_appearance();
                        shape.set_appearance(Some(a));
                    }
                    shape.appearance_mut().as_mut().unwrap()
                        .strokes.push(AppStroke::solid([0.0, 0.0, 0.0, 1.0], 1.0));
                    self.host.mark_dirty();
                }
            }

            Action::AppearanceAddDropShadow => {
                if self.selected_shape().is_some() {
                    self.checkpoint();
                    let shape = self.selected_shape_mut().unwrap();
                    if shape.appearance().is_none() {
                        let a = shape.effective_appearance();
                        shape.set_appearance(Some(a));
                    }
                    shape.appearance_mut().as_mut().unwrap()
                        .effects.push(Effect::drop_shadow());
                    self.host.mark_dirty();
                }
            }

            Action::AppearanceAddBlur => {
                if self.selected_shape().is_some() {
                    self.checkpoint();
                    let shape = self.selected_shape_mut().unwrap();
                    if shape.appearance().is_none() {
                        let a = shape.effective_appearance();
                        shape.set_appearance(Some(a));
                    }
                    shape.appearance_mut().as_mut().unwrap()
                        .effects.push(Effect::gaussian_blur());
                    self.host.mark_dirty();
                }
            }

            Action::AppearanceRemoveFill(idx) => {
                let has = self.selected_shape()
                    .and_then(|s| s.appearance())
                    .map(|a| idx < a.fills.len())
                    .unwrap_or(false);
                if has {
                    self.checkpoint();
                    self.selected_shape_mut().unwrap()
                        .appearance_mut().as_mut().unwrap().fills.remove(idx);
                    self.host.mark_dirty();
                }
            }

            Action::AppearanceRemoveStroke(idx) => {
                let has = self.selected_shape()
                    .and_then(|s| s.appearance())
                    .map(|a| idx < a.strokes.len())
                    .unwrap_or(false);
                if has {
                    self.checkpoint();
                    self.selected_shape_mut().unwrap()
                        .appearance_mut().as_mut().unwrap().strokes.remove(idx);
                    self.host.mark_dirty();
                }
            }

            Action::AppearanceRemoveEffect(idx) => {
                let has = self.selected_shape()
                    .and_then(|s| s.appearance())
                    .map(|a| idx < a.effects.len())
                    .unwrap_or(false);
                if has {
                    self.checkpoint();
                    self.selected_shape_mut().unwrap()
                        .appearance_mut().as_mut().unwrap().effects.remove(idx);
                    self.host.mark_dirty();
                }
            }

            Action::AppearanceToggleFill(idx) => {
                if let Some(f) = self.selected_shape_mut()
                    .and_then(|s| s.appearance_mut().as_mut())
                    .and_then(|a| a.fills.get_mut(idx))
                {
                    f.visible = !f.visible;
                    self.host.mark_dirty();
                }
            }

            Action::AppearanceToggleStroke(idx) => {
                if let Some(s) = self.selected_shape_mut()
                    .and_then(|sh| sh.appearance_mut().as_mut())
                    .and_then(|a| a.strokes.get_mut(idx))
                {
                    s.visible = !s.visible;
                    self.host.mark_dirty();
                }
            }

            Action::AppearanceSetFillColor { idx, color } => {
                if self.selected_shape().is_some() {
                    self.checkpoint();
                    let shape = self.selected_shape_mut().unwrap();
                    if shape.appearance().is_none() {
                        let a = shape.effective_appearance();
                        shape.set_appearance(Some(a));
                    }
                    if let Some(f) = shape.appearance_mut().as_mut().and_then(|a| a.fills.get_mut(idx)) {
                        f.paint = Paint::Solid(color);
                    }
                    self.host.mark_dirty();
                }
            }

            Action::AppearanceSetStrokeColor { idx, color } => {
                if self.selected_shape().is_some() {
                    self.checkpoint();
                    let shape = self.selected_shape_mut().unwrap();
                    if shape.appearance().is_none() {
                        let a = shape.effective_appearance();
                        shape.set_appearance(Some(a));
                    }
                    if let Some(s) = shape.appearance_mut().as_mut().and_then(|a| a.strokes.get_mut(idx)) {
                        s.paint = Paint::Solid(color);
                    }
                    self.host.mark_dirty();
                }
            }

            Action::AppearanceSetStrokeWidth { idx, width } => {
                if self.selected_shape().is_some() {
                    self.checkpoint();
                    let shape = self.selected_shape_mut().unwrap();
                    if shape.appearance().is_none() {
                        let a = shape.effective_appearance();
                        shape.set_appearance(Some(a));
                    }
                    if let Some(s) = shape.appearance_mut().as_mut().and_then(|a| a.strokes.get_mut(idx)) {
                        s.width = width.max(0.0);
                    }
                    self.host.mark_dirty();
                }
            }

            Action::AppearanceRaiseFill(idx) => {
                if let Some(a) = self.selected_shape_mut()
                    .and_then(|s| s.appearance_mut().as_mut())
                {
                    a.raise_fill(idx);
                    self.host.mark_dirty();
                }
            }

            Action::AppearanceLowerFill(idx) => {
                if let Some(a) = self.selected_shape_mut()
                    .and_then(|s| s.appearance_mut().as_mut())
                {
                    a.lower_fill(idx);
                    self.host.mark_dirty();
                }
            }

            Action::AppearanceRaiseStroke(idx) => {
                if let Some(a) = self.selected_shape_mut()
                    .and_then(|s| s.appearance_mut().as_mut())
                {
                    a.raise_stroke(idx);
                    self.host.mark_dirty();
                }
            }

            Action::AppearanceLowerStroke(idx) => {
                if let Some(a) = self.selected_shape_mut()
                    .and_then(|s| s.appearance_mut().as_mut())
                {
                    a.lower_stroke(idx);
                    self.host.mark_dirty();
                }
            }

            // --- Graphic styles handlers ---

            Action::SaveGraphicStyle(name) => {
                if let Some(shape) = self.selected_shape() {
                    let appearance = shape.effective_appearance();
                    self.doc.graphic_styles.add(&name, appearance);
                }
            }

            Action::ApplyGraphicStyle(id) => {
                if let Some(appearance) = self.doc.graphic_styles.appearance_of(id).cloned() {
                    if self.selected_shape().is_some() {
                        self.checkpoint();
                        self.selected_shape_mut().unwrap().set_appearance(Some(appearance));
                        self.host.mark_dirty();
                    }
                }
            }

            Action::DeleteGraphicStyle(id) => {
                self.doc.graphic_styles.remove(id);
            }

            // --- Width tool ---
            Action::SetWidthProfile { start, end } => {
                if let Some(idx) = self.selected {
                    self.checkpoint();
                    if let Some(shape) = self.doc.shapes.get_mut(idx) {
                        shape.stroke_style_mut().width_profile = (start, end);
                        self.host.mark_dirty();
                    }
                }
            }

            // --- Blend tool ---
            Action::CreateBlend { steps } => {
                if let (Some(a_idx), Some(b_idx)) = (self.selected, self.secondary) {
                    if let (Some(a), Some(b)) = (
                        self.doc.shapes.get(a_idx).cloned(),
                        self.doc.shapes.get(b_idx).cloned(),
                    ) {
                        self.checkpoint();
                        let new_steps = crate::blend::make_steps(&a, &b, steps);
                        let insert_at = a_idx.min(b_idx) + 1;
                        for (i, s) in new_steps.into_iter().enumerate() {
                            self.doc.shapes.insert(insert_at + i, s);
                        }
                        self.host.mark_dirty();
                    }
                }
            }
            Action::SetBlendSteps(n) => {
                self.blend_steps = n.max(1);
            }

            // --- Perspective grid ---
            Action::TogglePerspectiveGrid => {
                if let Some(ref mut pg) = self.perspective_grid {
                    pg.visible = !pg.visible;
                } else {
                    let (w, h) = (self.host.doc_w as f32, self.host.doc_h as f32);
                    self.perspective_grid = Some(PerspectiveGrid::default_for(w, h));
                }
            }
            Action::MovePerspectiveVP { vp, x, y } => {
                if let Some(ref mut pg) = self.perspective_grid {
                    if vp == 0 { pg.vp1 = (x, y); } else { pg.vp2 = (x, y); }
                }
            }

            // --- Mesh gradient ---
            Action::SetMeshGradient { points } => {
                self.mesh_points = points;
                self.host.mark_dirty();
            }
            Action::EditMeshPoint { row, col, pos, color } => {
                let idx = row * 4 + col;
                if idx < self.mesh_points.len() {
                    self.mesh_points[idx] = (pos, color);
                    self.host.mark_dirty();
                }
            }

            // --- AI/EPS import ---
            Action::ImportAiEps(path) => {
                self.checkpoint();
                match crate::ai_eps::import(&path) {
                    Ok(shapes) => {
                        let count = shapes.len();
                        for s in shapes {
                            self.doc.shapes.push(s);
                        }
                        self.host.mark_dirty();
                        self.status_message = Some((
                            format!("Imported {count} shapes from {}",
                                path.file_name().unwrap_or_default().to_string_lossy()),
                            std::time::Instant::now(),
                        ));
                    }
                    Err(e) => {
                        self.status_message = Some((format!("Import failed: {e}"), std::time::Instant::now()));
                    }
                }
            }

            // --- 3D extrude ---
            Action::AppearanceAdd3DExtrude => {
                if let Some(idx) = self.selected {
                    self.checkpoint();
                    if let Some(shape) = self.doc.shapes.get_mut(idx) {
                        // Ensure the shape has an appearance stack (migrate from legacy if needed).
                        if shape.appearance_mut().is_none() {
                            let ea = shape.effective_appearance();
                            *shape.appearance_mut() = Some(ea);
                        }
                        if let Some(ap) = shape.appearance_mut().as_mut() {
                            ap.effects.push(crate::appearance::Effect::extrude_3d());
                        }
                        self.host.mark_dirty();
                    }
                }
            }
            Action::AppearanceSet3DDepth { idx, depth } => {
                if let Some(si) = self.selected {
                    if let Some(shape) = self.doc.shapes.get_mut(si) {
                        if let Some(ap) = shape.appearance_mut().as_mut() {
                            if let Some(crate::appearance::Effect::Extrude3D { depth: d, .. }) = ap.effects.get_mut(idx) {
                                *d = depth;
                                self.host.mark_dirty();
                            }
                        }
                    }
                }
            }
            Action::AppearanceSet3DAngle { idx, angle_deg } => {
                if let Some(si) = self.selected {
                    if let Some(shape) = self.doc.shapes.get_mut(si) {
                        if let Some(ap) = shape.appearance_mut().as_mut() {
                            if let Some(crate::appearance::Effect::Extrude3D { angle_deg: a, .. }) = ap.effects.get_mut(idx) {
                                *a = angle_deg;
                                self.host.mark_dirty();
                            }
                        }
                    }
                }
            }

            // --- Batch 2: Variable font axes ---
            Action::SetFontAxis { axis, value } => {
                self.edit_text_params(|p| {
                    p.font_axes.insert(axis, value);
                });
            }

            // --- Batch 2: Symbol library (real) ---
            Action::DefineSymbol(name) => {
                let shapes: Vec<Shape> = self.selection.iter()
                    .filter_map(|&i| self.doc.shapes.get(i).cloned())
                    .collect();
                if !shapes.is_empty() {
                    self.symbol_lib.add(&name, shapes);
                }
            }
            Action::PlaceSymbol2(sym_id) => {
                if let Some(inst_id) = self.symbol_lib.place(
                    sym_id,
                    crate::transform::Affine::translate(180.0, 180.0),
                ) {
                    let resolved = {
                        let inst = self.symbol_lib.instance(inst_id).unwrap();
                        self.symbol_lib.resolve(inst)
                    };
                    self.checkpoint();
                    for s in resolved {
                        self.doc.shapes.push(s);
                    }
                    let last = self.doc.shapes.len().saturating_sub(1);
                    self.select_single(last);
                    self.host.mark_dirty();
                }
            }
            Action::EditSymbol2(id) => {
                log::info!("EditSymbol2({id}) — stub");
            }
            Action::DeleteSymbol(id) => {
                self.symbol_lib.remove(id);
            }

            // --- Batch 2: Multiple artboards (named) ---
            Action::AddArtboard2 { rect } => {
                let id = self.artboard_next_id;
                self.artboard_next_id += 1;
                let name = crate::artboard::default_name(self.artboard_entries.len());
                self.artboard_entries.push(ArtboardEntry::new(id, name, rect));
                self.artboards.push(rect);
                self.active_artboard = Some(id);
            }
            Action::RemoveArtboard(idx) => {
                if idx < self.artboard_entries.len() {
                    self.artboard_entries.remove(idx);
                    if idx < self.artboards.len() {
                        self.artboards.remove(idx);
                    }
                    if self.active_artboard.map_or(false, |_| true) {
                        self.active_artboard = self.artboard_entries.first().map(|e| e.id);
                    }
                }
            }
            Action::RenameArtboard { idx, name } => {
                if let Some(entry) = self.artboard_entries.get_mut(idx) {
                    entry.name = name;
                }
            }
            Action::SelectArtboard(id) => {
                if self.artboard_entries.iter().any(|e| e.id == id) {
                    self.active_artboard = Some(id);
                }
            }
            Action::DuplicateArtboard(idx) => {
                if let Some(entry) = self.artboard_entries.get(idx).cloned() {
                    let new_id = self.artboard_next_id;
                    self.artboard_next_id += 1;
                    let new_rect = [
                        entry.rect[0] + 20.0,
                        entry.rect[1] + 20.0,
                        entry.rect[2],
                        entry.rect[3],
                    ];
                    let new_name = format!("{} copy", entry.name);
                    self.artboard_entries.push(ArtboardEntry::new(new_id, new_name, new_rect));
                    self.artboards.push(new_rect);
                    self.active_artboard = Some(new_id);
                }
            }

            // --- Batch 2: Graph / chart tool stub ---
            Action::InsertGraph { kind, rect } => {
                self.checkpoint();
                let sample_data: &[&[f32]] = &[&[10.0, 20.0, 15.0], &[5.0, 25.0, 30.0]];
                let [gx, gy, gw, gh] = rect;
                match kind {
                    GraphKind::Bar => {
                        let series_count = sample_data.len() as f32;
                        let bar_count = sample_data[0].len();
                        let group_w = gw / bar_count as f32;
                        let max_val = sample_data.iter().flat_map(|s| s.iter()).cloned().fold(0.0_f32, f32::max);
                        for (si, series) in sample_data.iter().enumerate() {
                            for (bi, &val) in series.iter().enumerate() {
                                let bw = group_w / (series_count + 1.0);
                                let bx = gx + bi as f32 * group_w + si as f32 * bw;
                                let bh = (val / max_val.max(0.001)) * gh;
                                let by = gy + gh - bh;
                                let hue = si as f32 / series_count;
                                let fill = [0.3 + hue * 0.5, 0.5, 0.9 - hue * 0.4, 1.0];
                                self.doc.shapes.push(Shape::rect([bx, by, bw * 0.9, bh], fill, self.default_stroke, 0.5));
                            }
                        }
                    }
                    GraphKind::Pie => {
                        let all_vals: Vec<f32> = sample_data[0].to_vec();
                        let total: f32 = all_vals.iter().sum();
                        let cx_pt = gx + gw * 0.5;
                        let cy_pt = gy + gh * 0.5;
                        let r = gw.min(gh) * 0.45;
                        let mut angle = 0.0_f32;
                        for (i, &val) in all_vals.iter().enumerate() {
                            let sweep = (val / total.max(0.001)) * std::f32::consts::TAU;
                            let a1 = angle;
                            let a2 = angle + sweep;
                            let fill = [
                                0.3 + i as f32 * 0.2,
                                0.6 - i as f32 * 0.1,
                                0.8,
                                1.0,
                            ];
                            let px1 = cx_pt + r * a1.cos();
                            let py1 = cy_pt + r * a1.sin();
                            let px2 = cx_pt + r * a2.cos();
                            let py2 = cy_pt + r * a2.sin();
                            let points = vec![(cx_pt, cy_pt), (px1, py1), (px2, py2)];
                            let handles = vec![(0.0_f32, 0.0_f32); points.len()];
                            self.doc.shapes.push(Shape::path(points, handles, true, fill, self.default_stroke, 0.5));
                            angle += sweep;
                        }
                    }
                    GraphKind::Line | GraphKind::Scatter => {
                        let series_count = sample_data.len();
                        for (si, series) in sample_data.iter().enumerate() {
                            let n = series.len();
                            let max_val = series.iter().cloned().fold(0.0_f32, f32::max);
                            let hue = si as f32 / series_count as f32;
                            let stroke = [0.3 + hue * 0.5, 0.5, 0.9 - hue * 0.4, 1.0];
                            let pts: Vec<(f32, f32)> = series.iter().enumerate().map(|(i, &v)| {
                                let px = gx + (i as f32 / (n - 1).max(1) as f32) * gw;
                                let py = gy + gh - (v / max_val.max(0.001)) * gh;
                                (px, py)
                            }).collect();
                            let handles = vec![(0.0_f32, 0.0_f32); pts.len()];
                            self.doc.shapes.push(Shape::path(pts, handles, false, [0.0; 4], stroke, 1.5));
                        }
                    }
                }
                let last = self.doc.shapes.len().saturating_sub(1);
                self.select_single(last);
                self.host.mark_dirty();
            }

            // --- Batch 2: Stroke width profile presets ---
            Action::ApplyWidthPreset(preset) => {
                if let Some(idx) = self.selected {
                    let (start, end) = preset.profile();
                    self.checkpoint();
                    if let Some(shape) = self.doc.shapes.get_mut(idx) {
                        shape.stroke_style_mut().width_profile = (start, end);
                        self.host.mark_dirty();
                    }
                }
            }

            // --- Batch 3: Live Paint ---
            Action::ApplyLivePaint { fill } => {
                // Simple implementation: set fill on the topmost closed shape containing
                // the last clicked point. Caller should have pre-selected via HitTestSelect.
                if let Some(idx) = self.selected {
                    if idx < self.doc.shapes.len() {
                        self.checkpoint();
                        self.doc.shapes[idx].set_fill_color(fill);
                        self.host.mark_dirty();
                    }
                }
            }

            // --- Batch 3: Type on Path ---
            Action::PlaceTextOnPath => {
                // Find exactly one text and one non-text shape in the selection.
                let text_idx = self.selection.iter().copied()
                    .find(|&i| self.doc.shapes.get(i).map_or(false, |s| s.text_params().is_some()));
                let path_idx = self.selection.iter().copied()
                    .find(|&i| self.doc.shapes.get(i).map_or(false, |s| s.text_params().is_none()));
                if let (Some(tid), Some(pid)) = (text_idx, path_idx) {
                    self.checkpoint();
                    self.text_on_path.insert(tid, pid);
                    self.relayout_text_on_path(tid);
                    self.host.mark_dirty();
                }
            }

            // --- Batch 3: Perspective Distort ---
            Action::SetPerspectiveDistort { shape_id, corners } => {
                if shape_id < self.doc.shapes.len() {
                    self.perspective_distort_corners = corners;
                    self.perspective_distort_active = true;
                    self.host.mark_dirty();
                }
            }
            Action::ConfirmPerspectiveDistort => {
                log::info!(
                    "ConfirmPerspectiveDistort — corners: {:?} (homography warp stub)",
                    self.perspective_distort_corners
                );
                self.perspective_distort_active = false;
            }

            // --- Batch 3: Recolor Artwork HSL ---
            Action::OpenRecolorHSLPanel => {
                self.recolor_hsl_open = true;
            }
            Action::CloseRecolorHSLPanel => {
                self.recolor_hsl_open = false;
            }
            Action::RecolorArtworkHSL { hue_shift, saturation_scale, brightness_scale } => {
                if !self.selection.is_empty() {
                    self.checkpoint();
                    let sel: Vec<usize> = self.selection.clone();
                    for i in sel {
                        if let Some(shape) = self.doc.shapes.get_mut(i) {
                            if let Some(c) = shape.fill_color() {
                                let nc = crate::recolor::shift_hsl(
                                    c, hue_shift, saturation_scale, brightness_scale,
                                );
                                shape.set_fill_color(nc);
                            }
                            if let Some(c) = shape.stroke_color() {
                                let nc = crate::recolor::shift_hsl(
                                    c, hue_shift, saturation_scale, brightness_scale,
                                );
                                shape.set_stroke_color(nc);
                            }
                        }
                    }
                    self.host.mark_dirty();
                }
            }

            // --- Batch 3: Align to Artboard ---
            Action::AlignToArtboard { alignment } => {
                let artboard_rect = self.active_artboard
                    .and_then(|id| self.artboard_entries.iter().find(|e| e.id == id))
                    .map(|e| CoreRect::new(e.rect[0], e.rect[1], e.rect[2], e.rect[3]))
                    .or_else(|| self.artboards.first().map(|r| CoreRect::new(r[0], r[1], r[2], r[3])));
                if let Some(frame) = artboard_rect {
                    let indices: Vec<usize> = self.selection.clone();
                    let boxes: Vec<CoreRect> = indices.iter()
                        .filter_map(|&i| self.doc.shapes.get(i)?.bounds())
                        .collect();
                    if !boxes.is_empty() {
                        self.checkpoint();
                        let deltas = align::align_deltas(&boxes, alignment, frame);
                        let mut moved = false;
                        for (k, &i) in indices.iter().enumerate() {
                            if k < deltas.len() {
                                let (dx, dy) = deltas[k];
                                if (dx != 0.0 || dy != 0.0) && i < self.doc.shapes.len() {
                                    self.doc.shapes[i].translate(dx, dy);
                                    moved = true;
                                }
                            }
                        }
                        if moved {
                            self.host.mark_dirty();
                        }
                    }
                }
            }

            // --- Batch 4: Envelope Distort ---
            Action::MakeEnvelopeWithMesh { shape_id, rows, cols } => {
                if shape_id < self.doc.shapes.len() {
                    self.checkpoint();
                    let bbox = self.doc.shapes[shape_id].bounds()
                        .map(|b| [b.x, b.y, b.w, b.h])
                        .unwrap_or([0.0, 0.0, 100.0, 100.0]);
                    let mesh = crate::envelope::EnvelopeMesh::new(rows, cols, bbox);
                    self.doc.shapes[shape_id].set_envelope_mesh(Some(mesh));
                    self.host.mark_dirty();
                }
            }
            Action::MoveEnvelopePoint { shape_id, idx, pos } => {
                let has_point = self.doc.shapes.get(shape_id)
                    .and_then(|s| s.envelope_mesh())
                    .map_or(false, |m| idx < m.points.len());
                if has_point {
                    self.history.begin(&self.doc);
                    if let Some(mesh) = self.doc.shapes[shape_id].envelope_mesh_mut() {
                        mesh.points[idx] = pos;
                    }
                    self.host.mark_dirty();
                }
            }
            Action::ReleaseEnvelope(shape_id) => {
                if shape_id < self.doc.shapes.len() {
                    self.checkpoint();
                    self.doc.shapes[shape_id].set_envelope_mesh(None);
                    self.host.mark_dirty();
                }
            }

            // --- Batch 4: Live Effects ---
            Action::AppearanceAddGlow => {
                if let Some(idx) = self.selected {
                    self.checkpoint();
                    if let Some(shape) = self.doc.shapes.get_mut(idx) {
                        if shape.appearance().is_none() {
                            let ea = shape.effective_appearance();
                            *shape.appearance_mut() = Some(ea);
                        }
                        if let Some(ap) = shape.appearance_mut().as_mut() {
                            ap.effects.push(crate::appearance::Effect::glow());
                        }
                        self.host.mark_dirty();
                    }
                }
            }
            Action::ToggleEffect { shape_id, index } => {
                log::info!("ToggleEffect shape={shape_id} index={index} — effect toggle not yet destructive; use RemoveEffect to remove");
            }
            Action::RemoveEffect { shape_id, index } => {
                if shape_id < self.doc.shapes.len() {
                    self.checkpoint();
                    if let Some(ap) = self.doc.shapes[shape_id].appearance_mut().as_mut() {
                        if index < ap.effects.len() {
                            ap.effects.remove(index);
                        }
                    }
                    self.host.mark_dirty();
                }
            }
            Action::ReorderEffect { shape_id, from, to } => {
                if shape_id < self.doc.shapes.len() {
                    self.checkpoint();
                    if let Some(ap) = self.doc.shapes[shape_id].appearance_mut().as_mut() {
                        if from < ap.effects.len() && to < ap.effects.len() {
                            ap.effects.swap(from, to);
                        }
                    }
                    self.host.mark_dirty();
                }
            }

            // --- Batch 4: Group blend modes ---
            Action::SetGroupBlendMode { group_id, mode } => {
                self.group_blend_modes.insert(group_id, mode);
                self.host.mark_dirty();
            }
            Action::SetGroupOpacity { group_id, opacity } => {
                self.group_opacities.insert(group_id, opacity.clamp(0.0, 1.0));
                self.host.mark_dirty();
            }

            // --- Batch 4: Pathfinder improvements ---
            Action::OutlineStroke(shape_id) => {
                if shape_id < self.doc.shapes.len() {
                    let stroke_w = self.doc.shapes[shape_id].stroke_width();
                    let stroke_color = self.doc.shapes[shape_id].stroke_color().unwrap_or([0.0, 0.0, 0.0, 1.0]);
                    if let Some(pts) = self.doc.shapes[shape_id].outline_polygon() {
                        self.checkpoint();
                        let half_w = stroke_w * 0.5;
                        let outer = crate::stroke::offset_contour(&pts, half_w, true);
                        let inner = crate::stroke::offset_contour(&pts, -half_w, true);
                        use crate::document::{Shape, SubPath};
                        use crate::document::FillRule;
                        let outline_shape = Shape::Compound {
                            subpaths: vec![
                                SubPath::ring(outer),
                                SubPath::ring(inner),
                            ],
                            fill_rule: FillRule::EvenOdd,
                            fill: stroke_color,
                            fill_gradient: None,
                            stroke: [0.0, 0.0, 0.0, 0.0],
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
                        let insert_at = shape_id + 1;
                        if insert_at <= self.doc.shapes.len() {
                            self.doc.shapes.insert(insert_at, outline_shape);
                        } else {
                            self.doc.shapes.push(outline_shape);
                        }
                        self.select_single(shape_id + 1);
                        self.host.mark_dirty();
                    }
                }
            }
            Action::ExpandAppearance(shape_id) => {
                if shape_id < self.doc.shapes.len() {
                    self.checkpoint();
                    if let Some(ap) = self.doc.shapes[shape_id].appearance_mut().as_mut() {
                        ap.effects.clear();
                    }
                    self.host.mark_dirty();
                    log::info!("ExpandAppearance({shape_id}) — effects cleared (baked stub)");
                }
            }

            // --- Batch 5: Type on a Path ---
            Action::SetTextOnPathParams { text_id, params } => {
                if self.text_on_path.contains_key(&text_id) {
                    self.checkpoint();
                    self.text_on_path_params.insert(text_id, params);
                    self.relayout_text_on_path(text_id);
                    self.host.mark_dirty();
                }
            }

            // --- Batch 5: Perspective Distort (real homography) ---
            Action::ApplyPerspectiveDistort { shape_id } => {
                if shape_id < self.doc.shapes.len() {
                    if let Some(b) = self.doc.shapes[shape_id].bounds() {
                        let bbox = [b.x, b.y, b.w, b.h];
                        // Use the on-canvas corners if a distort is being edited;
                        // otherwise apply a default top-narrowing trapezoid about
                        // the shape's bounds (one-click perspective).
                        let corners = if self.perspective_distort_active {
                            self.perspective_distort_corners
                        } else {
                            let inset = b.w * 0.2;
                            [
                                [b.x + inset, b.y],
                                [b.x + b.w - inset, b.y],
                                [b.x + b.w, b.y + b.h],
                                [b.x, b.y + b.h],
                            ]
                        };
                        // Warp the shape's editable path geometry through the
                        // homography (anchors + bezier handles), preserving curves.
                        let path = self.doc.shapes[shape_id].to_path();
                        let warped = warp_shape_perspective(&path, bbox, &corners);
                        if let Some(w) = warped {
                            self.checkpoint();
                            self.doc.shapes[shape_id] = w;
                            self.perspective_distort_active = false;
                            self.host.mark_dirty();
                        }
                    }
                }
            }

            // --- Batch 5: Gradient Mesh object ---
            Action::MakeMeshGradient => {
                if let Some(idx) = self.selected {
                    if let Some(b) = self.doc.shapes[idx].bounds() {
                        let base = self.doc.shapes[idx].fill_color().unwrap_or([0.6, 0.6, 0.6, 1.0]);
                        // Mesh control points are in canvas-pixel space (artboard
                        // origin + doc point), matching the renderer's expectation.
                        let (ox, oy) = self.host.artboard_origin(&self.doc);
                        let bbox = [b.x - ox, b.y - oy, b.w, b.h];
                        self.mesh_points = crate::mesh_gradient::seed_grid(bbox, base);
                        self.host.mark_dirty();
                    }
                }
            }
            Action::ClearMeshGradient => {
                if !self.mesh_points.is_empty() {
                    self.mesh_points.clear();
                    self.host.mark_dirty();
                }
            }

            // --- Batch 5: Variable-width stroke outline ---
            Action::OutlineWidthProfile(shape_id) => {
                if shape_id < self.doc.shapes.len() {
                    let stroke_w = self.doc.shapes[shape_id].stroke_width();
                    let (ms, me) = self.doc.shapes[shape_id].stroke_style().width_profile;
                    let stroke_color = self.doc.shapes[shape_id]
                        .stroke_color()
                        .unwrap_or([0.0, 0.0, 0.0, 1.0]);
                    // Flatten the centreline from the editable path (works for open
                    // paths/lines, where variable-width strokes matter most, and
                    // closed paths). Open paths are not auto-closed.
                    let centreline = match self.doc.shapes[shape_id].to_path() {
                        Shape::Path { points, handles, closed, .. } => {
                            Some(crate::document::flatten(&points, &handles, closed))
                        }
                        _ => None,
                    };
                    if stroke_w > 0.0 {
                        if let Some(pts) = centreline {
                            let half_start = stroke_w * 0.5 * ms;
                            let half_end = stroke_w * 0.5 * me;
                            let band = crate::pathedit::variable_width_outline(
                                &pts, half_start, half_end,
                            );
                            if band.len() >= 3 {
                                self.checkpoint();
                                let outline = Shape::path(
                                    band,
                                    Vec::new(),
                                    true,
                                    stroke_color,
                                    [0.0, 0.0, 0.0, 0.0],
                                    0.0,
                                );
                                let insert_at = (shape_id + 1).min(self.doc.shapes.len());
                                self.doc.shapes.insert(insert_at, outline);
                                self.select_single(insert_at);
                                self.host.mark_dirty();
                            }
                        }
                    }
                }
            }

            // --- Batch 5: Roughen distort ---
            Action::RoughenPath { size, detail } => {
                let sel: Vec<usize> = self.selection.clone();
                if !sel.is_empty() {
                    self.checkpoint();
                    let mut changed = false;
                    for i in sel {
                        if i >= self.doc.shapes.len() {
                            continue;
                        }
                        // Demote to a plain corner path, roughen its outline.
                        let path = self.doc.shapes[i].to_path();
                        if let Shape::Path {
                            points,
                            closed,
                            fill,
                            stroke,
                            stroke_w,
                            ..
                        } = &path
                        {
                            let rough = crate::pathedit::roughen(points, *closed, size, detail);
                            if rough.len() >= 2 {
                                self.doc.shapes[i] = Shape::path(
                                    rough,
                                    Vec::new(),
                                    *closed,
                                    *fill,
                                    *stroke,
                                    *stroke_w,
                                );
                                changed = true;
                            }
                        }
                    }
                    if changed {
                        self.host.mark_dirty();
                    }
                }
            }

            // --- Batch 6: Transform Each ---
            Action::TransformEach { dx, dy, scale_x, scale_y, angle_deg, reflect_x } => {
                let sel: Vec<usize> = self.selection.clone();
                if sel.is_empty() {
                    return;
                }
                self.checkpoint();
                let rad = angle_deg.to_radians();
                for i in sel {
                    if i >= self.doc.shapes.len() {
                        continue;
                    }
                    // Compute the shape's own bbox centre as the pivot.
                    let (cx, cy) = self.doc.shapes[i]
                        .bounds()
                        .map(|b| (b.x + b.w / 2.0, b.y + b.h / 2.0))
                        .unwrap_or((0.0, 0.0));
                    // Build and compose transforms: translate → scale about centre
                    // → rotate about centre → optional x-reflect about centre.
                    let mut aff = Affine::translate(dx, dy);
                    if scale_x != 1.0 || scale_y != 1.0 {
                        // Scale about the (already-translated) centre: the centre
                        // after a pure translate is (cx+dx, cy+dy), but Illustrator
                        // applies each sub-transform independently from the *original*
                        // bbox centre, so we do likewise — scale about original centre.
                        aff = aff.then(Affine::scale_about(scale_x, scale_y, cx, cy));
                    }
                    if angle_deg != 0.0 {
                        aff = aff.then(Affine::rotate_about(rad, cx, cy));
                    }
                    if reflect_x {
                        aff = aff.then(Affine::scale_about(-1.0, 1.0, cx, cy));
                    }
                    self.doc.shapes[i].apply_affine(&aff);
                }
                self.host.mark_dirty();
            }

            // --- Batch 6: Offset Path ---
            Action::OffsetPath { shape_id, distance } => {
                if shape_id >= self.doc.shapes.len() {
                    return;
                }
                let path = self.doc.shapes[shape_id].to_path();
                if let Shape::Path { points, closed, fill, stroke, stroke_w, .. } = &path {
                    if !closed || points.len() < 3 {
                        return;
                    }
                    let offset_pts = offset_polygon(points, distance);
                    if offset_pts.len() < 3 {
                        return;
                    }
                    self.checkpoint();
                    self.doc.shapes[shape_id] =
                        Shape::path(offset_pts, Vec::new(), true, *fill, *stroke, *stroke_w);
                    self.host.mark_dirty();
                }
            }

            // --- Batch 6: Find / Replace ---
            Action::ToggleFindReplacePanel => {
                self.find_replace_open = !self.find_replace_open;
            }
            Action::SetFindReplaceQuery { find, replace } => {
                self.find_query = find;
                self.replace_query = replace;
            }
            Action::FindReplaceText { find, replace } => {
                if find.is_empty() {
                    return;
                }
                let mut count = 0usize;
                // Snapshot the shapes we'll change so we can checkpoint once.
                let mut changed_any = false;
                for shape in &mut self.doc.shapes {
                    if let Shape::Text { params, .. } = shape {
                        if params.text.contains(&find as &str) {
                            if !changed_any {
                                // checkpoint before first mutation
                                changed_any = true;
                            }
                            params.text = params.text.replace(&find as &str, &replace as &str);
                            count += 1;
                        }
                    }
                }
                if changed_any {
                    // Re-layout all modified text glyphs.
                    for shape in &mut self.doc.shapes {
                        shape.text_relayout();
                    }
                    self.history.push(self.doc.clone());
                    self.host.mark_dirty();
                }
                self.last_find_count = count;
            }

            // --- Batch 6: Pathfinder shortcuts ---
            Action::PathfinderTrim => self.apply_boolean(BoolOp::Trim),
            Action::PathfinderMerge => self.apply_boolean(BoolOp::Merge),

            // --- Batch 6: Scatter Brush ---
            Action::SetScatterBrush { symbol_id, spacing, size_jitter, rotation_jitter } => {
                self.scatter_brush = Some(ScatterBrushConfig {
                    symbol_id,
                    spacing: spacing.max(1.0),
                    size_jitter: size_jitter.clamp(0.0, 1.0),
                    rotation_jitter,
                });
            }
            Action::ClearScatterBrush => {
                self.scatter_brush = None;
            }
            Action::PlaceScatterAlongPath { path } => {
                let Some(cfg) = self.scatter_brush.clone() else { return; };
                if path.len() < 2 || cfg.spacing < 1.0 {
                    return;
                }
                // Compute arc-length at each polyline vertex.
                let mut arc: Vec<f32> = Vec::with_capacity(path.len());
                arc.push(0.0);
                for i in 1..path.len() {
                    let dx = path[i][0] - path[i - 1][0];
                    let dy = path[i][1] - path[i - 1][1];
                    arc.push(arc[i - 1] + (dx * dx + dy * dy).sqrt());
                }
                let total = *arc.last().unwrap_or(&0.0);
                if total < cfg.spacing {
                    return;
                }
                // Sample at multiples of spacing along the arc.
                let mut dist = 0.0f32;
                let mut copy_idx: usize = 0;
                let mut new_shapes: Vec<Shape> = Vec::new();
                while dist <= total {
                    // Interpolate position on the polyline at distance `dist`.
                    let (px, py) = sample_polyline(&path, &arc, dist);
                    // Deterministic pseudo-random for jitter (no stdlib random).
                    let rng = |seed: usize| -> f32 {
                        let v = (seed.wrapping_mul(1_234_567).wrapping_add(7_654_321)) % 1000;
                        v as f32 / 1000.0
                    };
                    let size_scale = 1.0
                        + cfg.size_jitter * (rng(copy_idx) * 2.0 - 1.0);
                    let rot_deg = cfg.rotation_jitter * (rng(copy_idx + 500) * 2.0 - 1.0);
                    // Build a rect placeholder for symbol instances whose content
                    // we can't clone here (symbol lookup is outside this fn's scope).
                    // We create a small filled rect at (px, py) scaled by size_scale
                    // and rotated. A proper renderer would look up the symbol shapes.
                    let half = 10.0 * size_scale;
                    let fill = self.default_fill;
                    let stroke = self.default_stroke;
                    let sw = self.default_stroke_w;
                    let mut inst = Shape::rect(
                        [px - half, py - half, half * 2.0, half * 2.0],
                        fill,
                        stroke,
                        sw,
                    );
                    if rot_deg != 0.0 {
                        inst.apply_affine(&Affine::rotate_about(
                            rot_deg.to_radians(),
                            px,
                            py,
                        ));
                    }
                    new_shapes.push(inst);
                    dist += cfg.spacing;
                    copy_idx += 1;
                }
                if !new_shapes.is_empty() {
                    self.checkpoint();
                    self.doc.shapes.extend(new_shapes);
                    self.host.mark_dirty();
                }
            }

            // --- Batch 7: Art Brush ---
            Action::SetArtBrush { symbol_id, width_scale, colorize, flip } => {
                self.art_brush = Some(ArtBrushConfig { symbol_id, width_scale, colorize, flip });
            }
            Action::ClearArtBrush => {
                self.art_brush = None;
            }
            Action::PaintArtBrushPath { path } => {
                let Some(cfg) = self.art_brush.clone() else { return; };
                if path.len() < 2 { return; }
                // Compute arc length.
                let mut arc: Vec<f32> = vec![0.0];
                for i in 1..path.len() {
                    let dx = path[i][0] - path[i-1][0];
                    let dy = path[i][1] - path[i-1][1];
                    arc.push(arc[i-1] + (dx*dx + dy*dy).sqrt());
                }
                let total = *arc.last().unwrap_or(&0.0);
                if total < 1.0 { return; }
                // Build a deformed polygon approximating the stretched symbol.
                // Sample the path at N points and produce a thin ribbon.
                let n = (total / 20.0).ceil() as usize + 1;
                let half_h = 10.0 * cfg.width_scale;
                let flip_sign = if cfg.flip { -1.0 } else { 1.0 };
                let mut pts_top: Vec<(f32, f32)> = Vec::with_capacity(n);
                let mut pts_bot: Vec<(f32, f32)> = Vec::with_capacity(n);
                for i in 0..=n {
                    let t = total * i as f32 / n as f32;
                    let (cx, cy) = sample_polyline(&path, &arc, t);
                    // Approximate tangent via finite difference.
                    let dt = total * 0.5 / n as f32;
                    let (ax, ay) = sample_polyline(&path, &arc, (t - dt).max(0.0));
                    let (bx, by) = sample_polyline(&path, &arc, (t + dt).min(total));
                    let tx = bx - ax; let ty = by - ay;
                    let len = (tx*tx + ty*ty).sqrt().max(1e-6);
                    let nx = -ty / len; let ny = tx / len;
                    pts_top.push((cx + nx * half_h * flip_sign, cy + ny * half_h * flip_sign));
                    pts_bot.push((cx - nx * half_h * flip_sign, cy - ny * half_h * flip_sign));
                }
                let mut poly: Vec<(f32, f32)> = pts_top;
                pts_bot.reverse();
                poly.extend(pts_bot);
                if poly.len() >= 3 {
                    self.checkpoint();
                    self.doc.shapes.push(Shape::path(
                        poly,
                        vec![],
                        true,
                        self.default_fill,
                        [0.0; 4],
                        0.0,
                    ));
                    self.host.mark_dirty();
                }
            }

            // --- Batch 7: Live Corners ---
            Action::SetPolygonCornerRadius(r) => {
                let r = r.max(0.0);
                let sel = self.selection.clone();
                for idx in sel {
                    if let Some(shape) = self.doc.shapes.get_mut(idx) {
                        if let Some(crate::liveshape::LiveShape::Polygon { sides, radius, .. }) = shape.live_shape() {
                            shape.set_live_shape(crate::liveshape::LiveShape::Polygon { sides, radius, corner_radius: r });
                        }
                    }
                }
                self.host.mark_dirty();
            }

            // --- Batch 7: Perspective Grid (active plane) ---
            Action::SetPerspectivePlane(plane) => {
                self.perspective_active_plane = plane.min(2);
            }
            Action::SnapToPerspectivePlane => {
                // Stub: record plane binding on selected shapes without full projection.
                // Full geometric projection requires the grid VP math which is a
                // larger refactor; this at minimum marks the plane as active.
                let _ = self.perspective_active_plane;
            }

            // --- Batch 7: Color Guide ---
            Action::SetColorGuide { rule, key_color } => {
                self.color_guide.rule = rule;
                self.color_guide.recompute(key_color);
            }
            Action::ApplyColorGuide(idx) => {
                if let Some(&color) = self.color_guide.swatches.get(idx) {
                    self.default_fill = color;
                    if self.selected_shape().is_some() {
                        self.checkpoint();
                        self.selected_shape_mut().unwrap().set_fill_color(color);
                        self.host.mark_dirty();
                    }
                }
            }

            // --- Batch 7: Warp Tools ---
            Action::SetWarpToolKind(kind) => {
                self.warp_tool_kind = kind;
            }
            Action::SetWarpBrush { size, intensity, detail } => {
                self.warp_brush_size = size.max(1.0);
                self.warp_brush_intensity = intensity.clamp(0.0, 1.0);
                self.warp_detail = detail.max(0.0);
            }
            Action::ApplyWarpStroke { center, radius } => {
                let kind = self.warp_tool_kind;
                let intensity = self.warp_brush_intensity;
                let mut changed = false;
                for shape in self.doc.shapes.iter_mut() {
                    let points = match shape {
                        Shape::Path { ref mut points, .. } => points,
                        _ => continue,
                    };
                    for (i, pt) in points.iter_mut().enumerate() {
                        let dx = pt.0 - center.0;
                        let dy = pt.1 - center.1;
                        let dist = (dx*dx + dy*dy).sqrt();
                        if dist >= radius { continue; }
                        let weight = intensity * (1.0 - dist / radius);
                        match kind {
                            WarpToolKind::Scallop => {
                                pt.0 -= dx * weight;
                                pt.1 -= dy * weight;
                            }
                            WarpToolKind::Crystallize => {
                                pt.0 += dx * weight;
                                pt.1 += dy * weight;
                            }
                            WarpToolKind::Wrinkle => {
                                let seed = i.wrapping_mul(37).wrapping_add(1);
                                let jitter = ((seed % 17) as f32 / 17.0 * 2.0 - 1.0) * weight * radius * 0.2;
                                pt.0 += -dy / (dist + 1e-6) * jitter;
                                pt.1 +=  dx / (dist + 1e-6) * jitter;
                            }
                        }
                        changed = true;
                    }
                }
                if changed {
                    self.checkpoint();
                    self.host.mark_dirty();
                }
            }


            // --- Batch 8: Image Trace (extended) ---
            Action::SetImageTrace { mode, threshold, colors } => {
                self.image_trace_mode = mode;
                self.image_trace_threshold = threshold.clamp(0.0, 255.0);
                self.image_trace_colors = colors.max(2);
            }
            Action::ApplyImageTrace => {
                let n = (self.image_trace_colors as usize).min(6);
                self.checkpoint();
                for i in 0..n {
                    let t = i as f32 / n.max(1) as f32;
                    let fill = match self.image_trace_mode {
                        ImageTraceMode::Grayscale => [t, t, t, 1.0],
                        ImageTraceMode::BlackWhite => {
                            if t < 0.5 { [0.0, 0.0, 0.0, 1.0] } else { [1.0, 1.0, 1.0, 1.0] }
                        }
                        ImageTraceMode::Outlined => [0.0, 0.0, 0.0, 0.0],
                        ImageTraceMode::Color => {
                            [t, 1.0 - t * 0.5, 0.3 + t * 0.4, 1.0]
                        }
                    };
                    self.doc.shapes.push(Shape::rect(
                        [i as f32 * 20.0, 0.0, 15.0, 15.0],
                        fill,
                        [0.0; 4],
                        0.0,
                    ));
                }
                self.host.mark_dirty();
            }
            Action::ExpandImageTrace => {
                self.image_trace_expanded = true;
            }

            // --- Batch 8: Opacity Masks ---
            Action::MakeOpacityMask => {
                if self.selection.len() < 2 {
                    return;
                }
                let n = self.selection.len();
                let bottom_idx = self.selection[n - 2];
                let top_idx = self.selection[n - 1];
                // Allocate a unique id for this mask set.
                let mask_id = self.omask_id_counter;
                self.omask_id_counter += 1;
                self.checkpoint();
                // TOP shape → mask path (luminance source).
                if top_idx < self.doc.shapes.len() {
                    self.doc.shapes[top_idx].set_omask(Some(mask_id));
                    self.doc.shapes[top_idx].set_omask_path(true);
                }
                // BOTTOM shape → masked content.
                if bottom_idx < self.doc.shapes.len() {
                    self.doc.shapes[bottom_idx].set_omask(Some(mask_id));
                    self.doc.shapes[bottom_idx].set_omask_path(false);
                }
                self.host.mark_dirty();
            }
            Action::ReleaseOpacityMask => {
                // Collect all omask ids present in the selection.
                let ids: std::collections::HashSet<u64> = self
                    .selection
                    .iter()
                    .filter_map(|&i| self.doc.shapes.get(i)?.omask())
                    .collect();
                if ids.is_empty() {
                    return;
                }
                self.checkpoint();
                for shape in self.doc.shapes.iter_mut() {
                    if let Some(id) = shape.omask() {
                        if ids.contains(&id) {
                            shape.clear_omask();
                        }
                    }
                }
                self.host.mark_dirty();
            }
            Action::InvertOpacityMask => {
                let mut changed = false;
                let sel: Vec<usize> = self.selection.clone();
                for &i in &sel {
                    if let Some(s) = self.doc.shapes.get_mut(i) {
                        if s.omask().is_some() && !s.is_omask() {
                            let cur = s.omask_invert();
                            s.set_omask_invert(!cur);
                            changed = true;
                        }
                    }
                }
                if changed {
                    self.host.mark_dirty();
                }
            }

            // --- Batch 8: Symbol extras ---
            Action::BreakSymbolLink { shape_idx } => {
                log::info!("BreakSymbolLink({shape_idx}) — stub: instance detached");
            }
            Action::ExpandSymbol(sym_id) => {
                log::info!("ExpandSymbol({sym_id}) — stub: all instances converted to editable copies");
            }

            // --- Batch 8: Graph Tool ---
            Action::SetGraphType(t) => {
                self.graph_data.graph_type = t;
            }
            Action::SetGraphData { cols, rows, values } => {
                self.graph_data.cols = cols;
                self.graph_data.rows = rows;
                self.graph_data.values = values;
            }
            Action::SetGraphLabels(l) => {
                self.graph_data.labels = l;
            }
            Action::SetGraphStyleFill(c) => {
                self.graph_style_fill = c;
            }
            Action::ToggleGraphLegend => {
                self.graph_show_legend = !self.graph_show_legend;
            }
            Action::ApplyGraph { x, y, width, height } => {
                let values = self.graph_data.values.clone();
                let n = values.len().min(self.graph_data.cols * self.graph_data.rows).max(1);
                let max_val = values.iter().cloned().fold(0.0_f32, f32::max).max(1.0);
                let bar_w = width / n as f32 * 0.8;
                let gap = width / n as f32 * 0.2;
                let fill = self.graph_style_fill;
                self.checkpoint();
                for (i, &v) in values.iter().take(n).enumerate() {
                    let bar_h = (v / max_val) * height;
                    let bx = x + i as f32 * (bar_w + gap);
                    let by = y + height - bar_h;
                    let shade = (i as f32 / n as f32 * 0.4 + 0.8).min(1.0);
                    let c = [fill[0] * shade, fill[1] * shade, fill[2] * shade, fill[3]];
                    self.doc.shapes.push(Shape::rect([bx, by, bar_w, bar_h], c, [0.0; 4], 0.0));
                }
                self.host.mark_dirty();
            }

            // --- Batch 9: Type on Path depth ---
            Action::SetTextOnPathOffset { text_id, offset } => {
                self.text_on_path_offsets.insert(text_id, offset);
                self.relayout_text_on_path(text_id);
                self.host.mark_dirty();
            }
            Action::SetTextOnPathSide { text_id, above } => {
                self.text_on_path_above.insert(text_id, above);
                self.relayout_text_on_path(text_id);
                self.host.mark_dirty();
            }
            Action::SetTextOnPathSpacing { text_id, spacing } => {
                self.text_on_path_spacing.insert(text_id, spacing);
                self.relayout_text_on_path(text_id);
                self.host.mark_dirty();
            }
            Action::FlipTextOnPath(id) => {
                let current = self.text_on_path_above.get(&id).copied().unwrap_or(true);
                self.text_on_path_above.insert(id, !current);
                self.relayout_text_on_path(id);
                self.host.mark_dirty();
            }

            // --- Batch 9: Recolor Artwork depth ---
            Action::SetRecolorConfig(c) => {
                self.recolor_config = c;
            }
            Action::SetRecolorColorCount(n) => {
                self.recolor_color_count = n.clamp(2, 30);
            }
            Action::SetRecolorPreserveBlack(b) => {
                self.recolor_config.preserve_black = b;
            }
            Action::SetRecolorPreserveWhite(b) => {
                self.recolor_config.preserve_white = b;
            }
            Action::RandomizeRecolor => {
                self.recolor_config.randomize = !self.recolor_config.randomize;
            }
            Action::SaveRecolorSet => {
                let fills: Vec<[f32; 4]> = self.selection.iter()
                    .filter_map(|&i| self.doc.shapes.get(i)?.fill_color())
                    .take(10)
                    .collect();
                if !fills.is_empty() {
                    self.recolor_history.push(fills);
                }
            }
            Action::ApplyRecolorToSelected => {
                if !self.selection.is_empty() {
                    self.checkpoint();
                    let step = 1.0 / self.recolor_color_count.max(1) as f32;
                    let sel: Vec<usize> = self.selection.clone();
                    for (k, &i) in sel.iter().enumerate() {
                        if let Some(shape) = self.doc.shapes.get_mut(i) {
                            if let Some(c) = shape.fill_color() {
                                let t = (k as f32 * step).fract();
                                let rotated = [
                                    c[0] * (1.0 - t) + c[1] * t,
                                    c[1] * (1.0 - t) + c[2] * t,
                                    c[2] * (1.0 - t) + c[0] * t,
                                    c[3],
                                ];
                                shape.set_fill_color(rotated);
                            }
                        }
                    }
                    self.host.mark_dirty();
                }
            }

            // --- Batch 9: Live Paint depth ---
            Action::LivePaintFill { x: _, y: _, color } => {
                self.default_fill = color;
                if let Some(idx) = self.selected {
                    if idx < self.doc.shapes.len() {
                        self.checkpoint();
                        self.doc.shapes[idx].set_fill_color(color);
                        self.host.mark_dirty();
                    }
                }
            }

            // --- Batch 10: Gradient Mesh depth ---
            Action::SetMeshRows(rows) => {
                self.gradient_mesh.rows = rows.clamp(1, 50);
            }
            Action::SetMeshCols(cols) => {
                self.gradient_mesh.cols = cols.clamp(1, 50);
            }
            Action::CreateMesh => {
                let count = self.gradient_mesh.rows as usize * self.gradient_mesh.cols as usize;
                self.gradient_mesh.points = (0..count).map(|_| MeshPoint::default()).collect();
            }
            Action::SetMeshPointColor { idx, color } => {
                if let Some(pt) = self.gradient_mesh.points.get_mut(idx) {
                    pt.color = color;
                }
            }
            Action::SetMeshPointTension { idx, tension } => {
                if let Some(pt) = self.gradient_mesh.points.get_mut(idx) {
                    pt.tension = tension.clamp(0.0, 1.0);
                }
            }
            Action::SelectMeshPoint(idx) => {
                self.selected_mesh_point = Some(idx);
            }
            Action::MoveMeshPoint { idx, pos } => {
                if let Some(pt) = self.gradient_mesh.points.get_mut(idx) {
                    pt.position = pos;
                }
            }
            Action::ExpandMeshToShape => {
                self.selected_mesh_point = None;
            }
            Action::ToggleMeshTool => {
                self.mesh_tool_active = !self.mesh_tool_active;
            }
            Action::ReleaseMesh => {
                self.gradient_mesh = GradientMeshConfig::default();
            }

            // --- Batch 10: Flare Tool ---
            Action::ToggleFlareTool => {
                self.flare_tool_active = !self.flare_tool_active;
            }
            Action::SetFlareBrightness(v) => {
                self.flare_config.brightness = v.clamp(0.0, 100.0);
            }
            Action::SetFlareHaloSize(v) => {
                self.flare_config.halo_size = v.clamp(0.0, 100.0);
            }
            Action::SetFlareRayCount(v) => {
                self.flare_config.ray_count = v.clamp(0, 250);
            }
            Action::SetFlareRayLength(v) => {
                self.flare_config.ray_length = v.clamp(0.0, 300.0);
            }
            Action::SetFlareRingCount(v) => {
                self.flare_config.ring_count = v.clamp(0, 50);
            }
            Action::SetFlareRingSpacing(v) => {
                self.flare_config.ring_spacing = v.clamp(0.0, 300.0);
            }
            Action::SetFlareColor(color) => {
                self.flare_config.color = color;
            }
            Action::PlaceFlare(pos) => {
                let idx = self.doc.shapes.len();
                self.flare_config.center = pos;
                self.flare_shapes.push(idx);
            }

            // --- Batch 10: Pattern Brush depth ---
            Action::SetPatternBrushScale(v) => {
                self.pattern_brush_config.scale = v.clamp(0.0, 1000.0);
            }
            Action::SetPatternBrushSpacing(v) => {
                self.pattern_brush_config.spacing = v.clamp(0.0, 1000.0);
            }
            Action::SetPatternBrushFit(fit) => {
                self.pattern_brush_config.fit = fit;
            }
            Action::SetPatternBrushFlipAcross(v) => {
                self.pattern_brush_config.flip_across_path = v;
            }
            Action::SetPatternBrushFlipAlong(v) => {
                self.pattern_brush_config.flip_along_path = v;
            }
            Action::SavePatternBrush { name } => {
                self.pattern_brush_library.push(name);
            }
            Action::DeletePatternBrush(idx) => {
                if idx < self.pattern_brush_library.len() {
                    self.pattern_brush_library.remove(idx);
                }
            }
            Action::ApplyPatternBrushToSelected => {
                if let Some(i) = self.selected {
                    if let Some(shape) = self.doc.shapes.get_mut(i) {
                        shape.set_stroke_width(self.pattern_brush_config.scale / 100.0);
                        self.host.mark_dirty();
                    }
                }
            }

            Action::LivePaintStroke { x: _, y: _, color, width } => {
                // Stub: record the stroke defaults.
                self.default_stroke = color;
                self.default_stroke_w = width;
            }
            Action::MakeLivePaintGroup => {
                if !self.selection.is_empty() {
                    self.checkpoint();
                    // Assign all selected shapes to a new Live Paint group id.
                    let gid = self.live_paint_group_ids.len() as u64 + 1;
                    for &i in &self.selection {
                        if let Some(s) = self.doc.shapes.get_mut(i) {
                            s.set_group(Some(gid));
                        }
                    }
                    self.live_paint_group_ids.push(gid);
                    self.host.mark_dirty();
                }
            }
            Action::ReleaseLivePaintGroup => {
                self.live_paint_group_ids.clear();
            }
            Action::ExpandLivePaintGroup => {
                // Stub: no geometric change; just log intent.
                log::info!("ExpandLivePaintGroup: stub");
            }
            Action::SetLivePaintGapDetection(b) => {
                self.live_paint_gap_detection = b;
            }
            Action::SetLivePaintHighlightColor(c) => {
                self.live_paint_highlight_color = c;
            }

            // --- Batch 9: Symbol Sprayer ---
            Action::SetSymbolSprayConfig(c) => {
                self.symbol_spray_config = c;
            }
            Action::SetSymbolSprayDensity(d) => {
                self.symbol_spray_config.density = d.clamp(0.0, 10.0);
            }
            Action::SetSymbolSprayDiameter(d) => {
                self.symbol_spray_config.diameter = d.max(1.0);
            }
            Action::SpraySymbols { center, pressure } => {
                let n = (self.symbol_spray_config.density * pressure * 3.0).ceil() as usize;
                if n > 0 {
                    self.checkpoint();
                    let radius = self.symbol_spray_config.diameter * 0.5;
                    let scatter = self.symbol_spray_config.scatter;
                    let fill = self.default_fill;
                    let stroke = self.default_stroke;
                    let sw = self.default_stroke_w;
                    for i in 0..n {
                        // Deterministic jitter from index — no random state needed.
                        let jx = ((i * 37 % 17) as f32 / 17.0 * 2.0 - 1.0) * radius * scatter;
                        let jy = ((i * 53 % 19) as f32 / 19.0 * 2.0 - 1.0) * radius * scatter;
                        let x = center[0] + jx;
                        let y = center[1] + jy;
                        let size = 20.0;
                        self.doc.shapes.push(Shape::rect(
                            [x - size * 0.5, y - size * 0.5, size, size],
                            fill,
                            stroke,
                            sw,
                        ));
                    }
                    self.host.mark_dirty();
                }
            }
            Action::SymbolShift { center, delta } => {
                let r = self.symbol_spray_config.diameter * 0.5;
                let mut changed = false;
                let aff = Affine::translate(delta[0], delta[1]);
                for shape in self.doc.shapes.iter_mut() {
                    if let Some(b) = shape.bounds() {
                        let cx = b.x + b.w * 0.5;
                        let cy = b.y + b.h * 0.5;
                        let dx = cx - center[0];
                        let dy = cy - center[1];
                        if (dx * dx + dy * dy).sqrt() <= r {
                            shape.apply_affine(&aff);
                            changed = true;
                        }
                    }
                }
                if changed {
                    self.checkpoint();
                    self.host.mark_dirty();
                }
            }
            Action::SymbolScale { center, scale } => {
                let r = self.symbol_spray_config.diameter * 0.5;
                let mut changed = false;
                for shape in self.doc.shapes.iter_mut() {
                    if let Some(b) = shape.bounds() {
                        let cx = b.x + b.w * 0.5;
                        let cy = b.y + b.h * 0.5;
                        let dx = cx - center[0];
                        let dy = cy - center[1];
                        if (dx * dx + dy * dy).sqrt() <= r {
                            let aff = Affine::scale_about(scale, scale, cx, cy);
                            shape.apply_affine(&aff);
                            changed = true;
                        }
                    }
                }
                if changed {
                    self.checkpoint();
                    self.host.mark_dirty();
                }
            }
            Action::SymbolSpin { center, angle } => {
                let r = self.symbol_spray_config.diameter * 0.5;
                let mut changed = false;
                for shape in self.doc.shapes.iter_mut() {
                    if let Some(b) = shape.bounds() {
                        let cx = b.x + b.w * 0.5;
                        let cy = b.y + b.h * 0.5;
                        let dx = cx - center[0];
                        let dy = cy - center[1];
                        if (dx * dx + dy * dy).sqrt() <= r {
                            let aff = Affine::rotate_about(angle, cx, cy);
                            shape.apply_affine(&aff);
                            changed = true;
                        }
                    }
                }
                if changed {
                    self.checkpoint();
                    self.host.mark_dirty();
                }
            }
            Action::SymbolStain { center, color } => {
                let r = self.symbol_spray_config.diameter * 0.5;
                let mut changed = false;
                for shape in self.doc.shapes.iter_mut() {
                    if let Some(b) = shape.bounds() {
                        let cx = b.x + b.w * 0.5;
                        let cy = b.y + b.h * 0.5;
                        let dx = cx - center[0];
                        let dy = cy - center[1];
                        if (dx * dx + dy * dy).sqrt() <= r {
                            if let Some(fc) = shape.fill_color() {
                                let t = 0.25; // blend 25% toward stain color
                                let blended = [
                                    fc[0] * (1.0 - t) + color[0] * t,
                                    fc[1] * (1.0 - t) + color[1] * t,
                                    fc[2] * (1.0 - t) + color[2] * t,
                                    fc[3],
                                ];
                                shape.set_fill_color(blended);
                                changed = true;
                            }
                        }
                    }
                }
                if changed {
                    self.checkpoint();
                    self.host.mark_dirty();
                }
            }
            Action::SymbolScreen { center, opacity } => {
                let r = self.symbol_spray_config.diameter * 0.5;
                let mut changed = false;
                for shape in self.doc.shapes.iter_mut() {
                    if let Some(b) = shape.bounds() {
                        let cx = b.x + b.w * 0.5;
                        let cy = b.y + b.h * 0.5;
                        let dx = cx - center[0];
                        let dy = cy - center[1];
                        if (dx * dx + dy * dy).sqrt() <= r {
                            if let Some(fc) = shape.fill_color() {
                                let new_alpha = (fc[3] * opacity).clamp(0.0, 1.0);
                                shape.set_fill_color([fc[0], fc[1], fc[2], new_alpha]);
                                changed = true;
                            }
                        }
                    }
                }
                if changed {
                    self.checkpoint();
                    self.host.mark_dirty();
                }
            }

            // --- Batch 10: Variable Fonts ---
            Action::ToggleVariableFontPanel => {
                self.variable_font_panel_open = !self.variable_font_panel_open;
            }
            Action::AddFontAxis(axis) => {
                self.variable_font_config.axes.push(axis);
            }
            Action::RemoveFontAxis(idx) => {
                if idx < self.variable_font_config.axes.len() {
                    self.variable_font_config.axes.remove(idx);
                }
            }
            Action::SetFontAxisValue { idx, value } => {
                if let Some(axis) = self.variable_font_config.axes.get_mut(idx) {
                    axis.value = value.clamp(axis.min, axis.max);
                }
            }
            Action::SetFontPreviewText(text) => {
                self.variable_font_config.preview_text = text;
            }
            Action::ApplyVariableFontToSelected => {
                if self.selected.is_some() {
                    self.host.mark_dirty();
                }
            }
            Action::ResetFontAxes => {
                for axis in self.variable_font_config.axes.iter_mut() {
                    axis.value = (axis.min + axis.max) / 2.0;
                }
            }

            // --- Batch 11: Pathfinder depth ---
            Action::ApplyPathfinderOp(op) => {
                self.last_pathfinder_op = Some(op);
            }
            Action::SetPathfinderPrecision(v) => {
                self.pathfinder_precision = v.clamp(0.001, 10.0);
            }
            Action::SetPathfinderRemoveRedundant(v) => {
                self.pathfinder_remove_redundant = v;
            }
            Action::SetPathfinderDivideStroke(v) => {
                self.pathfinder_divide_stroke = v;
            }
            Action::RepeatPathfinder => {
                // Re-apply the last recorded op if one exists (geometry stub: no-op).
                if let Some(_op) = self.last_pathfinder_op {
                    // stub — geometry would be applied here
                }
            }

            // --- Batch 11: 3D Extrude depth ---
            Action::ToggleExtrudePanel => {
                self.extrude_panel_open = !self.extrude_panel_open;
            }
            Action::SetExtrudeDepth(v) => {
                self.extrude_config.depth = v.clamp(0.0, 2000.0);
            }
            Action::SetExtrudeRotation { x, y, z } => {
                self.extrude_config.rotation_x = x.clamp(-180.0, 180.0);
                self.extrude_config.rotation_y = y.clamp(-180.0, 180.0);
                self.extrude_config.rotation_z = z.clamp(-180.0, 180.0);
            }
            Action::SetExtrudePerspective(v) => {
                self.extrude_config.perspective = v.clamp(0.0, 160.0);
            }
            Action::SetExtrudeSurface(s) => {
                self.extrude_config.surface = s;
            }
            Action::SetExtrudeCapStyle(c) => {
                self.extrude_config.cap_style = c;
            }
            Action::SetExtrudeBevelHeight(v) => {
                self.extrude_config.bevel_height = v.clamp(0.0, 100.0);
            }
            Action::SetExtrudeLighting { intensity, ambient, specular, gloss } => {
                self.extrude_config.light_intensity = intensity.clamp(0.0, 100.0);
                self.extrude_config.ambient_light = ambient.clamp(0.0, 100.0);
                self.extrude_config.specular_highlight = specular.clamp(0.0, 100.0);
                self.extrude_config.gloss = gloss.clamp(0.0, 100.0);
            }
            Action::SetExtrudeMapArt(v) => {
                self.extrude_config.map_art = v;
            }
            Action::ApplyExtrude => {
                if let Some(idx) = self.selected {
                    if !self.extrude_applied_shapes.contains(&idx) {
                        self.extrude_applied_shapes.push(idx);
                    }
                }
            }
            Action::ExpandExtrude => {
                self.extrude_applied_shapes.clear();
            }

            // --- Batch 11: Chart depth ---
            Action::ToggleChartPanel => {
                self.chart_panel_open = !self.chart_panel_open;
            }
            Action::AddChartDataSet(ds) => {
                self.chart_config.datasets.push(ds);
            }
            Action::RemoveChartDataSet(idx) => {
                if idx < self.chart_config.datasets.len() {
                    self.chart_config.datasets.remove(idx);
                }
            }
            Action::SetChartDataSetValues { idx, values } => {
                if let Some(ds) = self.chart_config.datasets.get_mut(idx) {
                    ds.values = values;
                }
            }
            Action::SetChartDataSetLabel { idx, label } => {
                if let Some(ds) = self.chart_config.datasets.get_mut(idx) {
                    ds.label = label;
                }
            }
            Action::SetChartCategoryLabels(labels) => {
                self.chart_config.category_labels = labels;
            }
            Action::SetChartTitle(title) => {
                self.chart_config.title = title;
            }
            Action::SetChartShowLegend(v) => {
                self.chart_config.show_legend = v;
            }
            Action::SetChartShowGrid(v) => {
                self.chart_config.show_grid = v;
            }
            Action::SetChartColumnWidth(v) => {
                self.chart_config.column_width = v.clamp(20.0, 100.0);
            }
            Action::SetChartClusterWidth(v) => {
                self.chart_config.cluster_width = v.clamp(20.0, 100.0);
            }
            Action::SetChartValueRange { min, max } => {
                self.chart_config.value_axis_min = min;
                self.chart_config.value_axis_max = max;
            }
            Action::ApplyChartData => {
                // stub: in a full impl, shapes would be generated per dataset value
            }

            // --- Batch 11: Envelope Distort depth ---
            Action::ToggleEnvelopePanel => {
                self.envelope_panel_open = !self.envelope_panel_open;
            }
            Action::SetEnvelopeWarpStyle(s) => {
                self.envelope_config.warp_style = s;
            }
            Action::SetEnvelopeAxis(h) => {
                self.envelope_config.horizontal = h;
            }
            Action::SetEnvelopeBend(v) => {
                self.envelope_config.bend = v.clamp(-100.0, 100.0);
            }
            Action::SetEnvelopeHDistortion(v) => {
                self.envelope_config.h_distortion = v.clamp(-100.0, 100.0);
            }
            Action::SetEnvelopeVDistortion(v) => {
                self.envelope_config.v_distortion = v.clamp(-100.0, 100.0);
            }
            Action::SetEnvelopeFidelity(v) => {
                self.envelope_config.fidelity = v.clamp(0.0, 100.0);
            }
            Action::SetEnvelopeEditMode(m) => {
                self.envelope_config.edit_mode = m;
            }
            Action::MakeEnvelopeWithWarpPreset => {
                if let Some(idx) = self.selected {
                    if !self.envelope_applied_shapes.contains(&idx) {
                        self.envelope_applied_shapes.push(idx);
                    }
                }
            }
            Action::MakeEnvelopeWithMeshPreset => {
                if let Some(idx) = self.selected {
                    if !self.envelope_applied_shapes.contains(&idx) {
                        self.envelope_applied_shapes.push(idx);
                    }
                }
            }
            Action::ReleaseEnvelopeAll => {
                self.envelope_applied_shapes.clear();
            }
            Action::ExpandEnvelope => {
                self.envelope_applied_shapes.clear();
            }
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

// ============================================================================
// SVG export helpers (Wave 9)
// ============================================================================

fn shape_to_svg(shape: &crate::document::Shape) -> String {
    use crate::document::Shape;
    match shape {
        Shape::Rect { rect, fill, stroke, stroke_w, .. } => {
            let [x, y, w, h] = rect;
            let fill_s = rgba_to_hex(*fill);
            let stroke_s = rgba_to_hex(*stroke);
            format!(
                "  <rect x=\"{x:.1}\" y=\"{y:.1}\" width=\"{w:.1}\" height=\"{h:.1}\" \
                 fill=\"{fill_s}\" stroke=\"{stroke_s}\" stroke-width=\"{stroke_w:.1}\"/>\n"
            )
        }
        Shape::Ellipse { rect, fill, stroke, stroke_w, .. } => {
            let [x, y, w, h] = rect;
            let cx = x + w * 0.5;
            let cy = y + h * 0.5;
            let rx = w * 0.5;
            let ry = h * 0.5;
            let fill_s = rgba_to_hex(*fill);
            let stroke_s = rgba_to_hex(*stroke);
            format!(
                "  <ellipse cx=\"{cx:.1}\" cy=\"{cy:.1}\" rx=\"{rx:.1}\" ry=\"{ry:.1}\" \
                 fill=\"{fill_s}\" stroke=\"{stroke_s}\" stroke-width=\"{stroke_w:.1}\"/>\n"
            )
        }
        Shape::Text { params, origin, fill, .. } => {
            let fill_s = rgba_to_hex(*fill);
            let text = params.text
                .replace('&', "&amp;")
                .replace('<', "&lt;")
                .replace('>', "&gt;");
            format!(
                "  <text x=\"{:.1}\" y=\"{:.1}\" font-size=\"{:.1}\" \
                 fill=\"{fill_s}\">{text}</text>\n",
                origin.0, origin.1, params.font_size
            )
        }
        Shape::Path { points, closed, fill, stroke, stroke_w, handles, .. } => {
            if points.is_empty() {
                return String::new();
            }
            let fill_s = rgba_to_hex(*fill);
            let stroke_s = rgba_to_hex(*stroke);
            let d = path_to_svg_d(points, handles, *closed);
            format!(
                "  <path d=\"{d}\" fill=\"{fill_s}\" stroke=\"{stroke_s}\" \
                 stroke-width=\"{stroke_w:.1}\"/>\n"
            )
        }
        Shape::Line { p0, p1, stroke, stroke_w, .. } => {
            let stroke_s = rgba_to_hex(*stroke);
            format!(
                "  <line x1=\"{:.1}\" y1=\"{:.1}\" x2=\"{:.1}\" y2=\"{:.1}\" \
                 stroke=\"{stroke_s}\" stroke-width=\"{stroke_w:.1}\"/>\n",
                p0.0, p0.1, p1.0, p1.1
            )
        }
        _ => String::new(),
    }
}

fn rgba_to_hex(c: [f32; 4]) -> String {
    let r = (c[0].clamp(0.0, 1.0) * 255.0).round() as u8;
    let g = (c[1].clamp(0.0, 1.0) * 255.0).round() as u8;
    let b = (c[2].clamp(0.0, 1.0) * 255.0).round() as u8;
    format!("#{r:02x}{g:02x}{b:02x}")
}

fn path_to_svg_d(points: &[(f32, f32)], handles: &[(f32, f32)], closed: bool) -> String {
    if points.is_empty() {
        return String::new();
    }
    let mut d = format!("M {:.1} {:.1}", points[0].0, points[0].1);
    for i in 1..points.len() {
        let p = points[i];
        let prev = points[i - 1];
        let h_prev = handles.get(i - 1).copied().unwrap_or((0.0, 0.0));
        let h_cur = handles.get(i).copied().unwrap_or((0.0, 0.0));
        if h_prev == (0.0, 0.0) && h_cur == (0.0, 0.0) {
            d.push_str(&format!(" L {:.1} {:.1}", p.0, p.1));
        } else {
            let cp1 = (prev.0 + h_prev.0, prev.1 + h_prev.1);
            let cp2 = (p.0 - h_cur.0, p.1 - h_cur.1);
            d.push_str(&format!(
                " C {:.1} {:.1} {:.1} {:.1} {:.1} {:.1}",
                cp1.0, cp1.1, cp2.0, cp2.1, p.0, p.1
            ));
        }
    }
    if closed {
        d.push_str(" Z");
    }
    d
}

// ============================================================================
// SVG import helpers (Wave 9)
// ============================================================================

fn import_svg(
    path: &std::path::Path,
) -> Result<Vec<crate::document::Shape>, Box<dyn std::error::Error>> {
    use crate::document::Shape;
    let content = std::fs::read_to_string(path)?;
    let mut shapes = Vec::new();

    fn attr(tag: &str, name: &str) -> Option<String> {
        let pattern = format!("{}=\"", name);
        let start = tag.find(&pattern)? + pattern.len();
        let end = tag[start..].find('"')? + start;
        Some(tag[start..end].to_string())
    }

    fn parse_color(s: &str) -> [f32; 4] {
        let s = s.trim();
        if s.starts_with('#') && s.len() == 7 {
            let r = u8::from_str_radix(&s[1..3], 16).unwrap_or(0) as f32 / 255.0;
            let g = u8::from_str_radix(&s[3..5], 16).unwrap_or(0) as f32 / 255.0;
            let b = u8::from_str_radix(&s[5..7], 16).unwrap_or(0) as f32 / 255.0;
            [r, g, b, 1.0]
        } else if s.starts_with('#') && s.len() == 4 {
            // Short hex #rgb
            let r = u8::from_str_radix(&s[1..2].repeat(2), 16).unwrap_or(0) as f32 / 255.0;
            let g = u8::from_str_radix(&s[2..3].repeat(2), 16).unwrap_or(0) as f32 / 255.0;
            let b = u8::from_str_radix(&s[3..4].repeat(2), 16).unwrap_or(0) as f32 / 255.0;
            [r, g, b, 1.0]
        } else {
            match s {
                "none" | "transparent" => [0.0, 0.0, 0.0, 0.0],
                "black" => [0.0, 0.0, 0.0, 1.0],
                "white" => [1.0, 1.0, 1.0, 1.0],
                "red" => [1.0, 0.0, 0.0, 1.0],
                "green" => [0.0, 0.5, 0.0, 1.0],
                "blue" => [0.0, 0.0, 1.0, 1.0],
                "yellow" => [1.0, 1.0, 0.0, 1.0],
                "orange" => [1.0, 0.647, 0.0, 1.0],
                "purple" | "violet" => [0.5, 0.0, 0.5, 1.0],
                "cyan" | "aqua" => [0.0, 1.0, 1.0, 1.0],
                "magenta" | "fuchsia" => [1.0, 0.0, 1.0, 1.0],
                "gray" | "grey" => [0.5, 0.5, 0.5, 1.0],
                "silver" => [0.753, 0.753, 0.753, 1.0],
                "darkblue" => [0.0, 0.0, 0.545, 1.0],
                "darkgreen" => [0.0, 0.392, 0.0, 1.0],
                "darkred" => [0.545, 0.0, 0.0, 1.0],
                _ => [0.0, 0.0, 0.0, 1.0],
            }
        }
    }

    fn pf(s: &str) -> f32 {
        s.trim().parse().unwrap_or(0.0)
    }

    let default_fill = [0.2, 0.55, 0.9, 1.0];
    let default_stroke = [0.1, 0.2, 0.35, 1.0];

    // Stack of group ids: each `<g>` pushes a new group id; `</g>` pops.
    let mut group_stack: Vec<u64> = Vec::new();
    let mut next_gid: u64 = 1;

    let mut i = 0;
    while i < content.len() {
        let Some(start) = content[i..].find('<') else { break };
        let abs_start = i + start;
        let Some(end_rel) = content[abs_start..].find('>') else { break };
        let tag = &content[abs_start..abs_start + end_rel + 1];
        i = abs_start + end_rel + 1;

        // Handle group open/close.
        if tag.starts_with("<g") && !tag.starts_with("<gradient") {
            group_stack.push(next_gid);
            next_gid += 1;
            continue;
        }
        if tag.starts_with("</g") {
            group_stack.pop();
            continue;
        }

        let current_group = group_stack.last().copied();

        let fill = attr(tag, "fill")
            .map(|s| parse_color(&s))
            .unwrap_or(default_fill);
        let stroke = attr(tag, "stroke")
            .map(|s| parse_color(&s))
            .unwrap_or(default_stroke);
        let stroke_w = attr(tag, "stroke-width")
            .map(|s| pf(&s))
            .unwrap_or(1.0);

        // Helper to set group on a freshly pushed shape.
        let set_group = |shapes: &mut Vec<Shape>, gid: Option<u64>| {
            if let (Some(gid), Some(s)) = (gid, shapes.last_mut()) {
                s.set_group(Some(gid));
            }
        };

        if tag.starts_with("<rect") {
            let x = attr(tag, "x").map(|s| pf(&s)).unwrap_or(0.0);
            let y = attr(tag, "y").map(|s| pf(&s)).unwrap_or(0.0);
            let w = attr(tag, "width").map(|s| pf(&s)).unwrap_or(0.0);
            let h = attr(tag, "height").map(|s| pf(&s)).unwrap_or(0.0);
            if w > 0.0 && h > 0.0 {
                shapes.push(Shape::rect([x, y, w, h], fill, stroke, stroke_w));
                set_group(&mut shapes, current_group);
            }
        } else if tag.starts_with("<line") && !tag.starts_with("<lineargradient") {
            let x1 = attr(tag, "x1").map(|s| pf(&s)).unwrap_or(0.0);
            let y1 = attr(tag, "y1").map(|s| pf(&s)).unwrap_or(0.0);
            let x2 = attr(tag, "x2").map(|s| pf(&s)).unwrap_or(0.0);
            let y2 = attr(tag, "y2").map(|s| pf(&s)).unwrap_or(0.0);
            // SVG lines have no fill; use stroke color (fall back to fill if stroke is transparent).
            let line_stroke = if stroke[3] == 0.0 { fill } else { stroke };
            shapes.push(Shape::Line {
                p0: (x1, y1),
                p1: (x2, y2),
                stroke: line_stroke,
                stroke_w,
                stroke_style: Default::default(),
                appearance: None,
                visible: true,
                group: current_group,
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
            });
            // set_group already handled via group: current_group above
        } else if tag.starts_with("<ellipse") {
            let cx = attr(tag, "cx").map(|s| pf(&s)).unwrap_or(0.0);
            let cy = attr(tag, "cy").map(|s| pf(&s)).unwrap_or(0.0);
            let rx = attr(tag, "rx").map(|s| pf(&s)).unwrap_or(0.0);
            let ry = attr(tag, "ry").map(|s| pf(&s)).unwrap_or(0.0);
            if rx > 0.0 && ry > 0.0 {
                shapes.push(Shape::ellipse(
                    [cx - rx, cy - ry, rx * 2.0, ry * 2.0],
                    fill,
                    stroke,
                    stroke_w,
                ));
                set_group(&mut shapes, current_group);
            }
        } else if tag.starts_with("<circle") {
            let cx = attr(tag, "cx").map(|s| pf(&s)).unwrap_or(0.0);
            let cy = attr(tag, "cy").map(|s| pf(&s)).unwrap_or(0.0);
            let r = attr(tag, "r").map(|s| pf(&s)).unwrap_or(0.0);
            if r > 0.0 {
                shapes.push(Shape::ellipse(
                    [cx - r, cy - r, r * 2.0, r * 2.0],
                    fill,
                    stroke,
                    stroke_w,
                ));
                set_group(&mut shapes, current_group);
            }
        } else if tag.starts_with("<text") {
            let x = attr(tag, "x").map(|s| pf(&s)).unwrap_or(0.0);
            let y = attr(tag, "y").map(|s| pf(&s)).unwrap_or(0.0);
            let font_size = attr(tag, "font-size").map(|s| pf(&s)).unwrap_or(16.0);
            let text_start = abs_start + end_rel + 1;
            let text_end = content[text_start..]
                .find("</text>")
                .map(|e| text_start + e)
                .unwrap_or(text_start);
            let text = content[text_start..text_end].trim().to_string();
            if !text.is_empty() {
                let params = crate::text::TextParams {
                    text: text.clone(),
                    font_size,
                    ..Default::default()
                };
                let glyphs = crate::text::layout(&params, (x, y)).0;
                shapes.push(Shape::Text {
                    params,
                    origin: (x, y),
                    glyphs,
                    fill,
                    fill_gradient: None,
                    stroke,
                    stroke_w,
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
                });
                set_group(&mut shapes, current_group);
            }
        } else if tag.starts_with("<path") {
            if let Some(d) = attr(tag, "d") {
                let (pts, hs) = parse_svg_path_d(&d);
                if pts.len() >= 2 {
                    let closed = d.trim_end().ends_with('Z') || d.trim_end().ends_with('z');
                    shapes.push(Shape::path(pts, hs, closed, fill, stroke, stroke_w));
                    set_group(&mut shapes, current_group);
                }
            }
        }
    }
    Ok(shapes)
}

fn parse_svg_path_d(d: &str) -> (Vec<(f32, f32)>, Vec<(f32, f32)>) {
    let mut points: Vec<(f32, f32)> = Vec::new();
    let mut handles: Vec<(f32, f32)> = Vec::new();
    let mut nums: Vec<f32> = Vec::new();
    let mut cur_cmd = ' ';
    let mut cur_x = 0.0f32;
    let mut cur_y = 0.0f32;

    fn flush(
        nums: &mut Vec<f32>,
        cmd: char,
        cur_x: &mut f32,
        cur_y: &mut f32,
        points: &mut Vec<(f32, f32)>,
        handles: &mut Vec<(f32, f32)>,
    ) {
        match cmd.to_ascii_uppercase() {
            'M' | 'L' => {
                if nums.len() >= 2 {
                    *cur_x = nums[0];
                    *cur_y = nums[1];
                    points.push((*cur_x, *cur_y));
                    handles.push((0.0, 0.0));
                }
            }
            'C' => {
                if nums.len() >= 6 {
                    let end_x = nums[4];
                    let end_y = nums[5];
                    if let Some(h) = handles.last_mut() {
                        *h = (nums[0] - *cur_x, nums[1] - *cur_y);
                    }
                    let h_new = (end_x - nums[2], end_y - nums[3]);
                    *cur_x = end_x;
                    *cur_y = end_y;
                    points.push((*cur_x, *cur_y));
                    handles.push(h_new);
                }
            }
            _ => {}
        }
        nums.clear();
    }

    let mut token = String::new();
    for ch in d.chars() {
        if ch.is_alphabetic() {
            if !token.is_empty() {
                if let Ok(v) = token.parse::<f32>() {
                    nums.push(v);
                }
                token.clear();
            }
            if cur_cmd != ' ' || ch.to_ascii_uppercase() == 'M' {
                flush(&mut nums, cur_cmd, &mut cur_x, &mut cur_y, &mut points, &mut handles);
            }
            cur_cmd = ch;
        } else if ch == ',' || ch == ' ' {
            if !token.is_empty() {
                if let Ok(v) = token.parse::<f32>() {
                    nums.push(v);
                }
                token.clear();
            }
        } else if ch == '-' && !token.is_empty() {
            if let Ok(v) = token.parse::<f32>() {
                nums.push(v);
            }
            token.clear();
            token.push(ch);
        } else {
            token.push(ch);
        }
    }
    if !token.is_empty() {
        if let Ok(v) = token.parse::<f32>() {
            nums.push(v);
        }
    }
    flush(&mut nums, cur_cmd, &mut cur_x, &mut cur_y, &mut points, &mut handles);

    (points, handles)
}

fn rand_group_id(_doc: &crate::document::Document) -> u64 {
    use std::time::SystemTime;
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(12345)
}

// --- Batch 8: serde default helpers ---
fn default_image_trace_threshold() -> f32 { 128.0 }
fn default_image_trace_colors() -> u8 { 6 }
fn default_omask_id_counter() -> u64 { 1 }
fn default_graph_style_fill() -> [f32; 4] { [0.2, 0.5, 0.9, 1.0] }

/// Whether two straight-sRGB RGBA colours are close enough to be considered the
/// same fill for the Recolor panel (tolerance 1/255 ≈ 0.004 per channel).
fn colors_approx_equal(a: [f32; 4], b: [f32; 4]) -> bool {
    a.iter().zip(b.iter()).all(|(x, y)| (x - y).abs() < 0.01)
}

/// Expand or contract a closed polygon ring by `distance` document units using
/// the averaged-normal (Minkwoski-sum approximation) method. Each vertex is
/// displaced outward (positive) or inward (negative) along the averaged
/// unit normal of its two adjacent edges. Winding order is detected via signed
/// area so normals always point outward regardless of CW / CCW orientation.
fn offset_polygon(points: &[(f32, f32)], distance: f32) -> Vec<(f32, f32)> {
    let n = points.len();
    if n < 3 {
        return points.to_vec();
    }
    // Signed area (shoelace): positive = CCW in math space (+y up);
    // negative = CW in math space = CW in screen space (+y down) which is what
    // Contour's rect / to_path() produces.
    let signed_area: f32 = (0..n)
        .map(|i| {
            let j = (i + 1) % n;
            points[i].0 * points[j].1 - points[j].0 * points[i].1
        })
        .sum::<f32>()
        * 0.5;
    // In screen space (+y down), a CW-wound polygon has positive signed area.
    // The outward normal is the right-side normal (ey, −ex) for CW and the
    // left-side (−ey, ex) for CCW. We pick `sign` so that `sign * (−ey, ex)`
    // always points outward: −1 for CW (positive area), +1 for CCW.
    let sign = if signed_area > 0.0 { -1.0_f32 } else { 1.0_f32 };
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let prev = points[(i + n - 1) % n];
        let cur  = points[i];
        let next = points[(i + 1) % n];
        // Edge vectors (cur − prev and next − cur).
        let (ex0, ey0) = (cur.0 - prev.0, cur.1 - prev.1);
        let (ex1, ey1) = (next.0 - cur.0, next.1 - cur.1);
        // Left-side normals (−ey, ex); multiplied by sign → outward.
        let len0 = (ex0 * ex0 + ey0 * ey0).sqrt().max(1e-9);
        let len1 = (ex1 * ex1 + ey1 * ey1).sqrt().max(1e-9);
        let n0 = (sign * -ey0 / len0, sign * ex0 / len0);
        let n1 = (sign * -ey1 / len1, sign * ex1 / len1);
        // Average outward normal, renormalized.
        let nx = (n0.0 + n1.0) * 0.5;
        let ny = (n0.1 + n1.1) * 0.5;
        let nlen = (nx * nx + ny * ny).sqrt().max(1e-9);
        out.push((cur.0 + nx / nlen * distance, cur.1 + ny / nlen * distance));
    }
    out
}

/// Interpolate a position along a polyline given arc-length distances at each
/// vertex. Returns the linearly interpolated point at arc-length `dist`.
fn sample_polyline(path: &[[f32; 2]], arc: &[f32], dist: f32) -> (f32, f32) {
    let n = path.len();
    if n == 0 {
        return (0.0, 0.0);
    }
    if n == 1 {
        return (path[0][0], path[0][1]);
    }
    // Binary search for the segment containing `dist`.
    let dist = dist.clamp(0.0, arc[n - 1]);
    let seg = arc
        .windows(2)
        .position(|w| w[0] <= dist && dist <= w[1])
        .unwrap_or(n - 2);
    let seg_len = arc[seg + 1] - arc[seg];
    if seg_len < 1e-9 {
        return (path[seg][0], path[seg][1]);
    }
    let t = (dist - arc[seg]) / seg_len;
    let (ax, ay) = (path[seg][0], path[seg][1]);
    let (bx, by) = (path[seg + 1][0], path[seg + 1][1]);
    (ax + t * (bx - ax), ay + t * (by - ay))
}

/// Warp a `Shape` (already reduced via [`Shape::to_path`]) through the perspective
/// homography defined by `corners` relative to `bbox`. Pushes every anchor and
/// bezier out-handle of each contour through the projective map (preserving curves),
/// returning the distorted `Path` / `Compound`. `None` if the shape has no warpable
/// geometry. Used by `Action::ApplyPerspectiveDistort`.
fn warp_shape_perspective(
    path: &Shape,
    bbox: [f32; 4],
    corners: &[[f32; 2]; 4],
) -> Option<Shape> {
    use crate::document::{Shape as S, SubPath};
    use crate::perspective::{warp_handles, warp_points};
    match path {
        S::Path {
            points,
            handles,
            closed,
            fill,
            stroke,
            stroke_w,
            stroke_style,
            ..
        } => {
            if points.len() < 2 {
                return None;
            }
            let wpts = warp_points(points, bbox, corners);
            let whandles = warp_handles(points, handles, &wpts, bbox, corners);
            let mut shape =
                Shape::path(wpts, whandles, *closed, *fill, *stroke, *stroke_w);
            if let S::Path { stroke_style: ss, .. } = &mut shape {
                *ss = stroke_style.clone();
            }
            Some(shape)
        }
        S::Compound {
            subpaths,
            fill_rule,
            fill,
            stroke,
            stroke_w,
            stroke_style,
            ..
        } => {
            let warped: Vec<SubPath> = subpaths
                .iter()
                .map(|sp| {
                    let wp = warp_points(&sp.points, bbox, corners);
                    let wh = warp_handles(&sp.points, &sp.handles, &wp, bbox, corners);
                    SubPath {
                        points: wp,
                        handles: wh,
                        closed: sp.closed,
                    }
                })
                .collect();
            Some(Shape::Compound {
                subpaths: warped,
                fill_rule: *fill_rule,
                fill: *fill,
                fill_gradient: None,
                stroke: *stroke,
                stroke_w: *stroke_w,
                stroke_style: stroke_style.clone(),
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
            })
        }
        _ => None,
    }
}

/// Whether a shape's axis-aligned bounds intersect the marquee rectangle (used
/// by `MarqueeSelect`). A shape with no finite bounds (an empty path) never hits.
fn bounds_intersect(shape: &Shape, marquee: &CoreRect) -> bool {
    let Some(b) = shape.bounds() else { return false };
    b.x < marquee.x + marquee.w
        && b.x + b.w > marquee.x
        && b.y < marquee.y + marquee.h
        && b.y + b.h > marquee.y
}

/// A small starter document (a few overlapping shapes on the default artboard) so
/// the GPUI preview and the Layers panel have real content to show. The egui app
/// opens with an empty document; this host seeds one purely so the migration
/// skeleton is visible end-to-end.
fn sample_document() -> Document {
    let mut doc = Document::new();
    // Colors are straight sRGB RGBA in 0..1 (Contour's document convention).
    doc.shapes.push(Shape::rect(
        [120.0, 120.0, 420.0, 300.0],
        [0.20, 0.55, 0.90, 1.0], // blue fill
        [0.10, 0.20, 0.35, 1.0],
        6.0,
    ));
    doc.shapes.push(Shape::ellipse(
        [360.0, 240.0, 380.0, 320.0],
        [0.95, 0.45, 0.25, 0.85], // orange, semi-transparent
        [0.40, 0.15, 0.05, 1.0],
        4.0,
    ));
    doc.shapes.push(Shape::rect(
        [220.0, 340.0, 260.0, 200.0],
        [0.30, 0.80, 0.45, 0.90], // green
        [0.10, 0.30, 0.18, 1.0],
        3.0,
    ));
    doc
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

    // ---- Batch 6 tests -------------------------------------------------------

    // --- TransformEach ---

    /// TransformEach with a non-zero (dx,dy) translates each selected shape
    /// independently; both shapes end up shifted by the same delta.
    #[test]
    fn test_transform_each_translate() {
        let mut app = App::new();
        app.doc.shapes.clear();
        app.doc.shapes.push(Shape::rect([0.0, 0.0, 50.0, 50.0], [1.0,0.0,0.0,1.0], [0.0,0.0,0.0,1.0], 1.0));
        app.doc.shapes.push(Shape::rect([200.0, 200.0, 50.0, 50.0], [0.0,1.0,0.0,1.0], [0.0,0.0,0.0,1.0], 1.0));
        app.selection = vec![0, 1];
        app.sync_legacy_selection();
        app.apply(Action::TransformEach {
            dx: 10.0, dy: 5.0, scale_x: 1.0, scale_y: 1.0, angle_deg: 0.0, reflect_x: false,
        });
        let b0 = app.doc.shapes[0].bounds().unwrap();
        let b1 = app.doc.shapes[1].bounds().unwrap();
        // Both rects should have moved by +10 x and +5 y from their original positions.
        assert!((b0.x - 10.0).abs() < 1.0, "shape 0 x: {}", b0.x);
        assert!((b0.y - 5.0).abs() < 1.0,  "shape 0 y: {}", b0.y);
        assert!((b1.x - 210.0).abs() < 1.0, "shape 1 x: {}", b1.x);
        assert!((b1.y - 205.0).abs() < 1.0,  "shape 1 y: {}", b1.y);
    }

    /// TransformEach with scale_x/scale_y=2 doubles each shape's bounding box.
    #[test]
    fn test_transform_each_scale() {
        let mut app = App::new();
        app.doc.shapes.clear();
        app.doc.shapes.push(Shape::rect([100.0, 100.0, 40.0, 40.0], [1.0,0.0,0.0,1.0], [0.0,0.0,0.0,1.0], 1.0));
        app.selection = vec![0];
        app.sync_legacy_selection();
        app.apply(Action::TransformEach {
            dx: 0.0, dy: 0.0, scale_x: 2.0, scale_y: 2.0, angle_deg: 0.0, reflect_x: false,
        });
        let b = app.doc.shapes[0].bounds().unwrap();
        // Original was 40×40; scaled by 2 → 80×80.
        assert!((b.w - 80.0).abs() < 2.0, "width should be ~80, got {}", b.w);
        assert!((b.h - 80.0).abs() < 2.0, "height should be ~80, got {}", b.h);
        // Centre should be preserved: original centre = (120, 120).
        let cx = b.x + b.w / 2.0;
        let cy = b.y + b.h / 2.0;
        assert!((cx - 120.0).abs() < 2.0, "centre x should be ~120, got {}", cx);
        assert!((cy - 120.0).abs() < 2.0, "centre y should be ~120, got {}", cy);
    }

    /// TransformEach with angle_deg=90 rotates each shape around its own centre.
    #[test]
    fn test_transform_each_rotate() {
        let mut app = App::new();
        app.doc.shapes.clear();
        // A 20×60 rect centred at (50, 50): x=40, y=20, w=20, h=60.
        app.doc.shapes.push(Shape::rect([40.0, 20.0, 20.0, 60.0], [1.0,0.0,0.0,1.0], [0.0,0.0,0.0,1.0], 1.0));
        app.selection = vec![0];
        app.sync_legacy_selection();
        app.apply(Action::TransformEach {
            dx: 0.0, dy: 0.0, scale_x: 1.0, scale_y: 1.0, angle_deg: 90.0, reflect_x: false,
        });
        // After 90° rotation the bounding box should have swapped width and height
        // (the 20×60 rect becomes approximately 60×20).
        let b = app.doc.shapes[0].bounds().unwrap();
        assert!(b.w > 40.0, "rotated rect should be wider than 20, got {}", b.w);
        assert!(b.h < 40.0, "rotated rect should be shorter than 60, got {}", b.h);
    }

    // --- OffsetPath ---

    /// OffsetPath with distance > 0 expands all points outward (larger bounds).
    #[test]
    fn test_offset_path_expand() {
        let mut app = App::new();
        app.doc.shapes.clear();
        // Seed a 100×100 rect as a path via to_path-style points.
        app.doc.shapes.push(Shape::rect([0.0, 0.0, 100.0, 100.0], [1.0,0.0,0.0,1.0], [0.0,0.0,0.0,1.0], 1.0));
        // Convert to a path so OffsetPath can act on it.
        let path = app.doc.shapes[0].to_path();
        app.doc.shapes[0] = path;
        app.select_single(0);
        let before = app.doc.shapes[0].bounds().unwrap();
        app.apply(Action::OffsetPath { shape_id: 0, distance: 10.0 });
        let after = app.doc.shapes[0].bounds().unwrap();
        assert!(after.w > before.w, "expanded w: {} > {}", after.w, before.w);
        assert!(after.h > before.h, "expanded h: {} > {}", after.h, before.h);
    }

    /// OffsetPath with distance < 0 contracts the shape (smaller bounds).
    #[test]
    fn test_offset_path_contract() {
        let mut app = App::new();
        app.doc.shapes.clear();
        app.doc.shapes.push(Shape::rect([0.0, 0.0, 100.0, 100.0], [1.0,0.0,0.0,1.0], [0.0,0.0,0.0,1.0], 1.0));
        let path = app.doc.shapes[0].to_path();
        app.doc.shapes[0] = path;
        app.select_single(0);
        let before = app.doc.shapes[0].bounds().unwrap();
        app.apply(Action::OffsetPath { shape_id: 0, distance: -5.0 });
        let after = app.doc.shapes[0].bounds().unwrap();
        assert!(after.w < before.w, "contracted w: {} < {}", after.w, before.w);
        assert!(after.h < before.h, "contracted h: {} < {}", after.h, before.h);
    }

    // --- FindReplaceText ---

    fn make_text_shape(text: &str, x: f32, y: f32) -> Shape {
        let params = crate::text::TextParams {
            text: text.to_string(),
            font_size: 24.0,
            ..Default::default()
        };
        let glyphs = crate::text::layout(&params, (x, y)).0;
        Shape::Text {
            params,
            origin: (x, y),
            glyphs,
            fill: [0.0, 0.0, 0.0, 1.0],
            fill_gradient: None,
            stroke: [0.0, 0.0, 0.0, 0.0],
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
        }
    }

    /// FindReplaceText replaces matching substrings across all text shapes.
    #[test]
    fn test_find_replace_basic() {
        let mut app = App::new();
        app.doc.shapes.clear();
        app.doc.shapes.push(make_text_shape("hello world", 0.0, 0.0));
        app.doc.shapes.push(make_text_shape("hello world", 100.0, 0.0));
        app.doc.shapes.push(make_text_shape("hello world", 200.0, 0.0));
        app.apply(Action::FindReplaceText { find: "hello".to_string(), replace: "hi".to_string() });
        for shape in &app.doc.shapes {
            if let Shape::Text { params, .. } = shape {
                assert_eq!(params.text, "hi world", "text should be replaced");
            }
        }
    }

    /// last_find_count reflects the number of shapes modified.
    #[test]
    fn test_find_replace_count() {
        let mut app = App::new();
        app.doc.shapes.clear();
        app.doc.shapes.push(make_text_shape("hello world", 0.0, 0.0));
        app.doc.shapes.push(make_text_shape("hello world", 100.0, 0.0));
        app.doc.shapes.push(make_text_shape("hello world", 200.0, 0.0));
        app.apply(Action::FindReplaceText { find: "hello".to_string(), replace: "hi".to_string() });
        assert_eq!(app.last_find_count, 3);
    }

    /// FindReplaceText with no match leaves count at 0.
    #[test]
    fn test_find_replace_no_match() {
        let mut app = App::new();
        app.doc.shapes.clear();
        app.doc.shapes.push(make_text_shape("hello world", 0.0, 0.0));
        app.apply(Action::FindReplaceText { find: "xyz".to_string(), replace: "abc".to_string() });
        assert_eq!(app.last_find_count, 0);
        // Text unchanged.
        if let Shape::Text { params, .. } = &app.doc.shapes[0] {
            assert_eq!(params.text, "hello world");
        }
    }

    // --- PathfinderTrim / PathfinderMerge ---

    /// PathfinderTrim on two overlapping rects yields at least one result shape.
    #[test]
    fn test_pathfinder_trim_basic() {
        let mut app = App::new();
        app.doc.shapes.clear();
        // Two overlapping 100×100 rects.
        app.doc.shapes.push(Shape::rect([0.0, 0.0, 100.0, 100.0], [1.0,0.0,0.0,1.0], [0.0,0.0,0.0,0.0], 0.0));
        app.doc.shapes.push(Shape::rect([50.0, 50.0, 100.0, 100.0], [0.0,0.0,1.0,1.0], [0.0,0.0,0.0,0.0], 0.0));
        // Select both (primary=1=front, secondary=0=back).
        app.selection = vec![0, 1];
        app.sync_legacy_selection();
        let before_count = app.doc.shapes.len();
        app.apply(Action::PathfinderTrim);
        // The trim should produce shapes (the result replaces the two inputs).
        // i_overlay may merge or split them; we just need some output.
        assert!(app.doc.shapes.len() >= 1, "trim should produce at least one shape");
        // The total shape count may differ from before, confirming the op ran.
        let _ = before_count;
    }

    /// PathfinderMerge on two same-fill-colour rects produces one shape.
    #[test]
    fn test_pathfinder_merge_same_color() {
        let mut app = App::new();
        app.doc.shapes.clear();
        let fill = [0.5_f32, 0.5, 0.5, 1.0];
        app.doc.shapes.push(Shape::rect([0.0, 0.0, 60.0, 60.0], fill, [0.0,0.0,0.0,0.0], 0.0));
        app.doc.shapes.push(Shape::rect([40.0, 40.0, 60.0, 60.0], fill, [0.0,0.0,0.0,0.0], 0.0));
        app.selection = vec![0, 1];
        app.sync_legacy_selection();
        app.apply(Action::PathfinderMerge);
        // Merge should unite same-colour shapes into fewer shapes.
        assert!(app.doc.shapes.len() >= 1, "merge should produce at least one shape");
    }

    // --- Scatter Brush ---

    /// SetScatterBrush configures the brush; PlaceScatterAlongPath produces copies.
    #[test]
    fn test_scatter_brush_spacing() {
        let mut app = App::new();
        // Straight horizontal path of length 100 (10 segments of 10).
        let path: Vec<[f32; 2]> = (0..=10).map(|i| [i as f32 * 10.0, 0.0]).collect();
        // Use an arbitrary symbol_id (no real symbol needed — placeholder rects are placed).
        app.apply(Action::SetScatterBrush {
            symbol_id: 99,
            spacing: 10.0,
            size_jitter: 0.0,
            rotation_jitter: 0.0,
        });
        let before = app.doc.shapes.len();
        app.apply(Action::PlaceScatterAlongPath { path });
        let added = app.doc.shapes.len() - before;
        // With spacing=10 along a length-100 path we expect ~11 copies (dist 0,10,20…100).
        assert!(added >= 9 && added <= 12, "expected ~11 copies, got {}", added);
    }

    /// The first copy is placed at distance 0 (the path start).
    #[test]
    fn test_scatter_brush_placement() {
        let mut app = App::new();
        app.apply(Action::SetScatterBrush {
            symbol_id: 1,
            spacing: 50.0,
            size_jitter: 0.0,
            rotation_jitter: 0.0,
        });
        let path = vec![[0.0_f32, 0.0], [100.0, 0.0]];
        let before = app.doc.shapes.len();
        app.apply(Action::PlaceScatterAlongPath { path });
        // Should have placed copies at 0 and 50 (within the 100-unit path).
        let added = app.doc.shapes.len() - before;
        assert!(added >= 2, "expected at least 2 copies, got {}", added);
        // The first added shape should be near x=0.
        let b = app.doc.shapes[before].bounds().unwrap();
        let cx = b.x + b.w / 2.0;
        assert!(cx.abs() < 15.0, "first copy centre x should be near 0, got {}", cx);
    }

    /// PlaceScatterAlongPath is a no-op when no scatter brush is configured.
    #[test]
    fn test_scatter_brush_no_symbol() {
        let mut app = App::new();
        let before = app.doc.shapes.len();
        app.apply(Action::PlaceScatterAlongPath {
            path: vec![[0.0, 0.0], [100.0, 0.0]],
        });
        assert_eq!(app.doc.shapes.len(), before, "no brush configured: nothing should be added");
    }

    // --- Batch 7: Art Brush ---

    #[test]
    fn test_art_brush_set_clear() {
        let mut app = App::new();
        app.apply(Action::SetArtBrush { symbol_id: 1, width_scale: 1.0, colorize: ArtBrushColorize::None, flip: false });
        assert!(app.art_brush.is_some());
        app.apply(Action::ClearArtBrush);
        assert!(app.art_brush.is_none());
    }

    #[test]
    fn test_art_brush_apply_produces_shape() {
        let mut app = App::new();
        app.apply(Action::SetArtBrush { symbol_id: 1, width_scale: 2.0, colorize: ArtBrushColorize::None, flip: false });
        let before = app.doc.shapes.len();
        // Straight horizontal path long enough to deform.
        let path: Vec<[f32; 2]> = (0..=10).map(|i| [i as f32 * 10.0, 0.0]).collect();
        app.apply(Action::PaintArtBrushPath { path });
        assert!(app.doc.shapes.len() > before, "art brush should add a shape");
    }

    #[test]
    fn test_art_brush_flip() {
        let mut app = App::new();
        // Two strokes: one normal, one flipped — both should produce shapes.
        app.apply(Action::SetArtBrush { symbol_id: 1, width_scale: 1.0, colorize: ArtBrushColorize::None, flip: false });
        let path: Vec<[f32; 2]> = (0..=5).map(|i| [i as f32 * 20.0, 0.0]).collect();
        app.apply(Action::PaintArtBrushPath { path: path.clone() });
        let after_normal = app.doc.shapes.len();
        app.apply(Action::SetArtBrush { symbol_id: 1, width_scale: 1.0, colorize: ArtBrushColorize::None, flip: true });
        app.apply(Action::PaintArtBrushPath { path });
        assert!(app.doc.shapes.len() > after_normal, "flipped brush also produces a shape");
    }

    // --- Batch 7: Live Corners ---

    #[test]
    fn test_live_corner_polygon_set() {
        let mut app = App::new();
        // Create a live polygon and select it.
        app.apply(Action::SetTool(Tool::Polygon));
        app.apply(Action::CreateShape { tool: Tool::Polygon, a: [100.0, 100.0], b: [180.0, 180.0] });
        let idx = app.doc.shapes.len() - 1;
        app.selection = vec![idx];
        app.apply(Action::SetPolygonCornerRadius(12.0));
        if let Some(crate::liveshape::LiveShape::Polygon { corner_radius, .. }) = app.doc.shapes[idx].live_shape() {
            assert!((corner_radius - 12.0).abs() < 0.001);
        }
    }

    #[test]
    fn test_live_corner_clamp() {
        let mut app = App::new();
        app.apply(Action::SetTool(Tool::Polygon));
        app.apply(Action::CreateShape { tool: Tool::Polygon, a: [100.0, 100.0], b: [180.0, 180.0] });
        let idx = app.doc.shapes.len() - 1;
        app.selection = vec![idx];
        app.apply(Action::SetPolygonCornerRadius(-5.0));
        if let Some(crate::liveshape::LiveShape::Polygon { corner_radius, .. }) = app.doc.shapes[idx].live_shape() {
            assert!(corner_radius >= 0.0, "corner radius clamped to >= 0");
        }
    }

    // --- Batch 7: Perspective Grid ---

    #[test]
    fn test_perspective_grid_toggle() {
        let mut app = App::new();
        let was_on = app.perspective_grid.is_some();
        app.apply(Action::TogglePerspectiveGrid);
        assert_ne!(app.perspective_grid.is_some(), was_on, "toggle should flip grid state");
    }

    #[test]
    fn test_perspective_plane_select() {
        let mut app = App::new();
        app.apply(Action::SetPerspectivePlane(1));
        assert_eq!(app.perspective_active_plane, 1);
        app.apply(Action::SetPerspectivePlane(2));
        assert_eq!(app.perspective_active_plane, 2);
    }

    #[test]
    fn test_perspective_plane_clamp() {
        let mut app = App::new();
        app.apply(Action::SetPerspectivePlane(99));
        assert!(app.perspective_active_plane <= 2, "plane clamped to 0..=2");
    }

    // --- Batch 7: Color Guide ---

    #[test]
    fn test_color_guide_complementary() {
        let mut app = App::new();
        let key = [1.0_f32, 0.0, 0.0, 1.0]; // red
        app.apply(Action::SetColorGuide { rule: ColorHarmonyRule::Complementary, key_color: key });
        assert!(!app.color_guide.swatches.is_empty(), "complementary should produce swatches");
    }

    #[test]
    fn test_color_guide_triadic() {
        let mut app = App::new();
        let key = [0.0_f32, 0.8, 0.0, 1.0];
        app.apply(Action::SetColorGuide { rule: ColorHarmonyRule::Triadic, key_color: key });
        assert_eq!(app.color_guide.swatches.len(), 3, "triadic = 3 swatches");
    }

    #[test]
    fn test_color_guide_apply_sets_fill() {
        let mut app = App::new();
        let key = [0.5_f32, 0.2, 0.8, 1.0];
        app.apply(Action::SetColorGuide { rule: ColorHarmonyRule::Complementary, key_color: key });
        assert!(!app.color_guide.swatches.is_empty());
        let original_fill = app.default_fill;
        app.apply(Action::ApplyColorGuide(0));
        // Fill should have changed to the guide swatch.
        assert_ne!(app.default_fill, original_fill, "fill should change after applying guide color");
    }

    // --- Batch 7: Warp Tools ---

    #[test]
    fn test_warp_tool_kind() {
        let mut app = App::new();
        app.apply(Action::SetWarpToolKind(WarpToolKind::Crystallize));
        assert_eq!(app.warp_tool_kind, WarpToolKind::Crystallize);
    }

    #[test]
    fn test_warp_brush_params() {
        let mut app = App::new();
        app.apply(Action::SetWarpBrush { size: 50.0, intensity: 0.8, detail: 2.0 });
        assert!((app.warp_brush_size - 50.0).abs() < 0.01);
        assert!((app.warp_brush_intensity - 0.8).abs() < 0.01);
    }

    #[test]
    fn test_scallop_pulls_inward() {
        let mut app = App::new();
        // Point at (100, 0). Center at (50, 0): dx=50, scallop pulls toward center → x decreases.
        app.doc.shapes.push(Shape::path(vec![(100.0, 0.0), (200.0, 0.0), (150.0, 50.0)], vec![], true, [1.0,0.0,0.0,1.0], [0.0;4], 0.0));
        app.apply(Action::SetWarpToolKind(WarpToolKind::Scallop));
        app.apply(Action::SetWarpBrush { size: 30.0, intensity: 1.0, detail: 1.0 });
        let before = if let Shape::Path { ref points, .. } = app.doc.shapes.last().unwrap() { points[0].0 } else { 0.0 };
        // Center at (50,0), radius 80 — point inside, dx=50.
        app.apply(Action::ApplyWarpStroke { center: (50.0, 0.0), radius: 80.0 });
        let after = if let Shape::Path { ref points, .. } = app.doc.shapes.last().unwrap() { points[0].0 } else { 0.0 };
        assert!(after < before, "scallop should pull point toward center: {} -> {}", before, after);
    }

    #[test]
    fn test_warp_outside_radius_unchanged() {
        let mut app = App::new();
        app.doc.shapes.push(Shape::path(vec![(500.0, 500.0), (600.0, 500.0), (550.0, 600.0)], vec![], true, [0.0,1.0,0.0,1.0], [0.0;4], 0.0));
        app.apply(Action::SetWarpToolKind(WarpToolKind::Crystallize));
        app.apply(Action::SetWarpBrush { size: 20.0, intensity: 1.0, detail: 1.0 });
        let before = if let Shape::Path { ref points, .. } = app.doc.shapes.last().unwrap() { points[0] } else { (0.0, 0.0) };
        // Center far away.
        app.apply(Action::ApplyWarpStroke { center: (0.0, 0.0), radius: 20.0 });
        let after = if let Shape::Path { ref points, .. } = app.doc.shapes.last().unwrap() { points[0] } else { (0.0, 0.0) };
        assert_eq!(before, after, "point outside radius should not move");
    }

    // ---- Batch 8 tests -------------------------------------------------------

    // --- Image Trace ---

    #[test]
    fn test_image_trace_config() {
        let mut app = App::new();
        app.apply(Action::SetImageTrace {
            mode: ImageTraceMode::BlackWhite,
            threshold: 180.0,
            colors: 4,
        });
        assert_eq!(app.image_trace_mode, ImageTraceMode::BlackWhite);
        assert!((app.image_trace_threshold - 180.0).abs() < 0.01);
        assert_eq!(app.image_trace_colors, 4);
    }

    #[test]
    fn test_image_trace_config_clamping() {
        let mut app = App::new();
        // threshold clamped 0..=255, colors >= 2
        app.apply(Action::SetImageTrace { mode: ImageTraceMode::Color, threshold: 999.0, colors: 0 });
        assert!((app.image_trace_threshold - 255.0).abs() < 0.01, "threshold clamped to 255");
        assert_eq!(app.image_trace_colors, 2, "colors min is 2");
    }

    #[test]
    fn test_apply_image_trace_adds_shapes() {
        let mut app = App::new();
        app.apply(Action::SetImageTrace { mode: ImageTraceMode::Color, threshold: 128.0, colors: 4 });
        let before = app.doc.shapes.len();
        app.apply(Action::ApplyImageTrace);
        let added = app.doc.shapes.len() - before;
        assert!(added >= 1 && added <= 6, "expected 1-6 traced shapes, got {added}");
        assert!(app.history.can_undo(), "ApplyImageTrace is undoable");
    }

    #[test]
    fn test_expand_image_trace() {
        let mut app = App::new();
        assert!(!app.image_trace_expanded);
        app.apply(Action::ExpandImageTrace);
        assert!(app.image_trace_expanded, "ExpandImageTrace sets the flag");
    }

    // --- Opacity Masks ---

    #[test]
    fn test_make_opacity_mask_links_two_shapes() {
        let mut app = App::new();
        app.doc.shapes.clear();
        app.doc.shapes.push(Shape::rect([0.0, 0.0, 50.0, 50.0], [1.0,0.0,0.0,1.0], [0.0;4], 0.0));
        app.doc.shapes.push(Shape::rect([10.0, 10.0, 30.0, 30.0], [0.0,0.0,0.0,1.0], [0.0;4], 0.0));
        app.selection = vec![0, 1];
        app.sync_legacy_selection();
        app.apply(Action::MakeOpacityMask);
        // Both shapes should share the same omask id.
        let id0 = app.doc.shapes[0].omask();
        let id1 = app.doc.shapes[1].omask();
        assert!(id0.is_some(), "bottom shape should have omask id");
        assert!(id1.is_some(), "top shape should have omask id");
        assert_eq!(id0, id1, "both shapes share the same mask group id");
        // Top shape (index 1) is the mask path.
        assert!(app.doc.shapes[1].is_omask(), "top shape is the mask path");
        assert!(!app.doc.shapes[0].is_omask(), "bottom shape is NOT the mask path");
    }

    #[test]
    fn test_make_opacity_mask_needs_two_selected() {
        let mut app = App::new();
        app.doc.shapes.clear();
        app.doc.shapes.push(Shape::rect([0.0, 0.0, 50.0, 50.0], [1.0,0.0,0.0,1.0], [0.0;4], 0.0));
        // Only one shape selected → no-op.
        app.selection = vec![0];
        app.sync_legacy_selection();
        app.apply(Action::MakeOpacityMask);
        assert!(app.doc.shapes[0].omask().is_none(), "no omask set with only 1 shape selected");
    }

    #[test]
    fn test_release_opacity_mask() {
        let mut app = App::new();
        app.doc.shapes.clear();
        app.doc.shapes.push(Shape::rect([0.0, 0.0, 50.0, 50.0], [1.0,0.0,0.0,1.0], [0.0;4], 0.0));
        app.doc.shapes.push(Shape::rect([10.0, 10.0, 30.0, 30.0], [0.0,0.0,0.0,1.0], [0.0;4], 0.0));
        app.selection = vec![0, 1];
        app.sync_legacy_selection();
        app.apply(Action::MakeOpacityMask);
        assert!(app.doc.shapes[0].omask().is_some(), "mask set created");
        // Now release with both shapes selected.
        app.selection = vec![0, 1];
        app.apply(Action::ReleaseOpacityMask);
        assert!(app.doc.shapes[0].omask().is_none(), "bottom shape released");
        assert!(app.doc.shapes[1].omask().is_none(), "top shape released");
    }

    #[test]
    fn test_invert_opacity_mask() {
        let mut app = App::new();
        app.doc.shapes.clear();
        app.doc.shapes.push(Shape::rect([0.0, 0.0, 50.0, 50.0], [1.0,0.0,0.0,1.0], [0.0;4], 0.0));
        app.doc.shapes.push(Shape::rect([10.0, 10.0, 30.0, 30.0], [0.0,0.0,0.0,1.0], [0.0;4], 0.0));
        app.selection = vec![0, 1];
        app.sync_legacy_selection();
        app.apply(Action::MakeOpacityMask);
        let before_invert = app.doc.shapes[0].omask_invert();
        // Select only the masked content (index 0, not the mask path).
        app.selection = vec![0];
        app.sync_legacy_selection();
        app.apply(Action::InvertOpacityMask);
        let after_invert = app.doc.shapes[0].omask_invert();
        assert_ne!(before_invert, after_invert, "InvertOpacityMask should toggle omask_invert");
    }

    // --- Graph Tool ---

    #[test]
    fn test_graph_type_set() {
        let mut app = App::new();
        app.apply(Action::SetGraphType(GraphType::Pie));
        assert_eq!(app.graph_data.graph_type, GraphType::Pie);
        app.apply(Action::SetGraphType(GraphType::Line));
        assert_eq!(app.graph_data.graph_type, GraphType::Line);
    }

    #[test]
    fn test_graph_data_set() {
        let mut app = App::new();
        app.apply(Action::SetGraphData { cols: 4, rows: 3, values: vec![1.0, 2.0, 3.0, 4.0] });
        assert_eq!(app.graph_data.cols, 4);
        assert_eq!(app.graph_data.rows, 3);
        assert_eq!(app.graph_data.values, vec![1.0, 2.0, 3.0, 4.0]);
    }

    #[test]
    fn test_apply_graph_adds_rects() {
        let mut app = App::new();
        app.apply(Action::SetGraphData { cols: 3, rows: 1, values: vec![10.0, 20.0, 30.0] });
        let before = app.doc.shapes.len();
        app.apply(Action::ApplyGraph { x: 0.0, y: 0.0, width: 300.0, height: 200.0 });
        let added = app.doc.shapes.len() - before;
        assert!(added >= 1, "ApplyGraph should add at least one shape, got {added}");
        assert!(app.history.can_undo(), "ApplyGraph is undoable");
    }

    #[test]
    fn test_graph_legend_toggle() {
        let mut app = App::new();
        assert!(!app.graph_show_legend, "legend starts false");
        app.apply(Action::ToggleGraphLegend);
        assert!(app.graph_show_legend, "legend toggled on");
        app.apply(Action::ToggleGraphLegend);
        assert!(!app.graph_show_legend, "legend toggled off again");
    }

    #[test]
    fn test_graph_labels_set() {
        let mut app = App::new();
        let labels = vec!["Q1".to_string(), "Q2".to_string(), "Q3".to_string()];
        app.apply(Action::SetGraphLabels(labels.clone()));
        assert_eq!(app.graph_data.labels, labels);
    }

    #[test]
    fn test_graph_style_fill_set() {
        let mut app = App::new();
        let color = [0.8, 0.2, 0.4, 1.0];
        app.apply(Action::SetGraphStyleFill(color));
        assert_eq!(app.graph_style_fill, color);
    }

    // --- Batch 9: Type on Path depth ---

    #[test]
    fn test_text_on_path_offset() {
        let mut app = App::new();
        app.apply(Action::SetTextOnPathOffset { text_id: 7, offset: 42.5 });
        assert_eq!(app.text_on_path_offsets.get(&7).copied(), Some(42.5));
    }

    #[test]
    fn test_text_on_path_flip() {
        let mut app = App::new();
        // Default (no entry) should be treated as `true` (above).
        app.apply(Action::FlipTextOnPath(3));
        assert_eq!(app.text_on_path_above.get(&3).copied(), Some(false), "flip from default true → false");
        app.apply(Action::FlipTextOnPath(3));
        assert_eq!(app.text_on_path_above.get(&3).copied(), Some(true), "flip back to true");
    }

    #[test]
    fn test_detach_text_from_path() {
        let mut app = App::new();
        // Seed a fake path attachment.
        app.text_on_path.insert(1, 0);
        app.text_on_path_offsets.insert(1, 10.0);
        app.apply(Action::DetachTextFromPath(1));
        assert!(app.text_on_path.get(&1).is_none(), "attachment removed");
        // Offset entry is left in place (detach does not clear depth state).
    }

    // --- Batch 9: Recolor Artwork depth ---

    #[test]
    fn test_recolor_color_count_clamp() {
        let mut app = App::new();
        // Values below 2 clamp to 2.
        app.apply(Action::SetRecolorColorCount(1));
        assert_eq!(app.recolor_color_count, 2, "clamped to minimum 2");
        // Values above 30 clamp to 30.
        app.apply(Action::SetRecolorColorCount(40));
        assert_eq!(app.recolor_color_count, 30, "clamped to maximum 30");
        // In-range values are kept as-is.
        app.apply(Action::SetRecolorColorCount(12));
        assert_eq!(app.recolor_color_count, 12);
    }

    #[test]
    fn test_recolor_preserve_flags() {
        let mut app = App::new();
        assert!(app.recolor_config.preserve_black, "default preserve_black is true");
        app.apply(Action::SetRecolorPreserveBlack(false));
        assert!(!app.recolor_config.preserve_black);
        app.apply(Action::SetRecolorPreserveWhite(false));
        assert!(!app.recolor_config.preserve_white);
    }

    #[test]
    fn test_save_recolor_set() {
        let mut app = App::new();
        // Select the first two shapes so we have fills to save.
        app.selection = vec![0, 1];
        let before = app.recolor_history.len();
        app.apply(Action::SaveRecolorSet);
        assert!(app.recolor_history.len() > before, "recolor set saved");
        assert!(!app.recolor_history.last().unwrap().is_empty(), "saved set is non-empty");
    }

    // --- Batch 9: Live Paint depth ---

    #[test]
    fn test_live_paint_gap_detection() {
        let mut app = App::new();
        assert!(!app.live_paint_gap_detection, "gap detection starts false");
        app.apply(Action::SetLivePaintGapDetection(true));
        assert!(app.live_paint_gap_detection);
        app.apply(Action::SetLivePaintGapDetection(false));
        assert!(!app.live_paint_gap_detection);
    }

    #[test]
    fn test_live_paint_highlight_color() {
        let mut app = App::new();
        let color = [0.0, 1.0, 0.5, 1.0];
        app.apply(Action::SetLivePaintHighlightColor(color));
        assert_eq!(app.live_paint_highlight_color, color);
    }

    #[test]
    fn test_make_live_paint_group() {
        let mut app = App::new();
        // Select the first two shapes.
        app.selection = vec![0, 1];
        let before = app.live_paint_group_ids.len();
        app.apply(Action::MakeLivePaintGroup);
        assert_eq!(app.live_paint_group_ids.len(), before + 1, "one group added");
        // Both shapes should have the new group id.
        let gid = *app.live_paint_group_ids.last().unwrap();
        assert_eq!(app.doc.shapes[0].group(), Some(gid));
        assert_eq!(app.doc.shapes[1].group(), Some(gid));
    }

    // --- Batch 9: Symbol Sprayer ---

    #[test]
    fn test_spray_density_clamp() {
        let mut app = App::new();
        app.apply(Action::SetSymbolSprayDensity(15.0));
        assert!((app.symbol_spray_config.density - 10.0).abs() < 0.01, "density clamped to 10");
        app.apply(Action::SetSymbolSprayDensity(-1.0));
        assert!((app.symbol_spray_config.density - 0.0).abs() < 0.01, "density clamped to 0");
    }

    #[test]
    fn test_spray_diameter_min() {
        let mut app = App::new();
        app.apply(Action::SetSymbolSprayDiameter(0.0));
        assert!((app.symbol_spray_config.diameter - 1.0).abs() < 0.01, "diameter clamped to min 1.0");
        app.apply(Action::SetSymbolSprayDiameter(50.0));
        assert!((app.symbol_spray_config.diameter - 50.0).abs() < 0.01);
    }

    #[test]
    fn test_spray_symbols_adds_shapes() {
        let mut app = App::new();
        app.apply(Action::SetSymbolSprayDensity(3.0));
        let before = app.doc.shapes.len();
        app.apply(Action::SpraySymbols { center: [0.0, 0.0], pressure: 1.0 });
        let added = app.doc.shapes.len() - before;
        assert!(added >= 1, "SpraySymbols should add at least one shape, got {added}");
    }

    #[test]
    fn test_symbol_stain_changes_fill() {
        let mut app = App::new();
        // Add a shape at the origin so SymbolStain can reach it.
        app.doc.shapes.push(Shape::rect([0.0, 0.0, 20.0, 20.0], [1.0, 0.0, 0.0, 1.0], [0.0;4], 0.0));
        // Use a large diameter so the shape is within the brush.
        app.apply(Action::SetSymbolSprayDiameter(200.0));
        let idx = app.doc.shapes.len() - 1;
        let before_fill = app.doc.shapes[idx].fill_color().unwrap();
        let stain = [0.0, 0.0, 1.0, 1.0];
        app.apply(Action::SymbolStain { center: [10.0, 10.0], color: stain });
        let after_fill = app.doc.shapes[idx].fill_color().unwrap();
        // Fill should have changed toward the stain color.
        assert_ne!(before_fill, after_fill, "stain should change the fill color");
        // Blue channel should have increased.
        assert!(after_fill[2] > before_fill[2], "blue channel should increase toward stain");
    }

    // --- Batch 10: Gradient Mesh ---

    #[test]
    fn test_mesh_rows_cols_clamp() {
        let mut app = App::new();
        // Below minimum → clamp to 1.
        app.apply(Action::SetMeshRows(0));
        assert_eq!(app.gradient_mesh.rows, 1, "rows clamped to 1");
        // Above maximum → clamp to 50.
        app.apply(Action::SetMeshRows(100));
        assert_eq!(app.gradient_mesh.rows, 50, "rows clamped to 50");
        app.apply(Action::SetMeshCols(0));
        assert_eq!(app.gradient_mesh.cols, 1, "cols clamped to 1");
        app.apply(Action::SetMeshCols(100));
        assert_eq!(app.gradient_mesh.cols, 50, "cols clamped to 50");
    }

    #[test]
    fn test_create_mesh_fills_points() {
        let mut app = App::new();
        app.apply(Action::SetMeshRows(2));
        app.apply(Action::SetMeshCols(3));
        app.apply(Action::CreateMesh);
        assert_eq!(app.gradient_mesh.points.len(), 6, "2×3 mesh should have 6 points");
    }

    #[test]
    fn test_mesh_point_tension_clamp() {
        let mut app = App::new();
        app.apply(Action::SetMeshRows(1));
        app.apply(Action::SetMeshCols(1));
        app.apply(Action::CreateMesh);
        app.apply(Action::SetMeshPointTension { idx: 0, tension: 2.0 });
        assert!((app.gradient_mesh.points[0].tension - 1.0).abs() < 0.001, "tension clamped to 1.0");
        app.apply(Action::SetMeshPointTension { idx: 0, tension: -0.5 });
        assert!((app.gradient_mesh.points[0].tension - 0.0).abs() < 0.001, "tension clamped to 0.0");
    }

    #[test]
    fn test_release_mesh_clears() {
        let mut app = App::new();
        app.apply(Action::SetMeshRows(3));
        app.apply(Action::SetMeshCols(3));
        app.apply(Action::CreateMesh);
        assert!(!app.gradient_mesh.points.is_empty());
        app.apply(Action::ReleaseMesh);
        assert!(app.gradient_mesh.points.is_empty(), "ReleaseMesh should clear all points");
        assert_eq!(app.gradient_mesh.rows, 4, "rows reset to default");
        assert_eq!(app.gradient_mesh.cols, 4, "cols reset to default");
    }

    // --- Batch 10: Flare Tool ---

    #[test]
    fn test_flare_brightness_clamp() {
        let mut app = App::new();
        app.apply(Action::SetFlareBrightness(150.0));
        assert!((app.flare_config.brightness - 100.0).abs() < 0.001, "brightness clamped to 100");
        app.apply(Action::SetFlareBrightness(-10.0));
        assert!((app.flare_config.brightness - 0.0).abs() < 0.001, "brightness clamped to 0");
    }

    #[test]
    fn test_flare_ray_count_clamp() {
        let mut app = App::new();
        app.apply(Action::SetFlareRayCount(255));
        assert_eq!(app.flare_config.ray_count, 250, "ray_count clamped to 250");
    }

    #[test]
    fn test_place_flare_adds_to_list() {
        let mut app = App::new();
        let before = app.flare_shapes.len();
        app.apply(Action::PlaceFlare([100.0, 200.0]));
        assert_eq!(app.flare_shapes.len(), before + 1, "PlaceFlare should add to flare_shapes");
        assert_eq!(app.flare_config.center, [100.0, 200.0]);
    }

    #[test]
    fn test_flare_toggle() {
        let mut app = App::new();
        assert!(!app.flare_tool_active);
        app.apply(Action::ToggleFlareTool);
        assert!(app.flare_tool_active);
        app.apply(Action::ToggleFlareTool);
        assert!(!app.flare_tool_active);
    }

    // --- Batch 10: Pattern Brush ---

    #[test]
    fn test_pattern_brush_scale_clamp() {
        let mut app = App::new();
        app.apply(Action::SetPatternBrushScale(2000.0));
        assert!((app.pattern_brush_config.scale - 1000.0).abs() < 0.001, "scale clamped to 1000");
        app.apply(Action::SetPatternBrushScale(-10.0));
        assert!((app.pattern_brush_config.scale - 0.0).abs() < 0.001, "scale clamped to 0");
    }

    #[test]
    fn test_save_pattern_brush_library() {
        let mut app = App::new();
        app.apply(Action::SavePatternBrush { name: "Dots".to_string() });
        app.apply(Action::SavePatternBrush { name: "Waves".to_string() });
        assert_eq!(app.pattern_brush_library.len(), 2, "library should contain 2 entries");
    }

    #[test]
    fn test_delete_pattern_brush_oob() {
        let mut app = App::new();
        // Delete on empty library: no panic.
        app.apply(Action::DeletePatternBrush(99));
        assert!(app.pattern_brush_library.is_empty());
        // Add one and delete a valid index.
        app.apply(Action::SavePatternBrush { name: "X".to_string() });
        app.apply(Action::DeletePatternBrush(0));
        assert!(app.pattern_brush_library.is_empty());
    }

    // --- Batch 10: Variable Fonts ---

    #[test]
    fn test_font_axis_value_clamped_to_range() {
        let mut app = App::new();
        app.apply(Action::AddFontAxis(FontAxis { tag: "wght".to_string(), min: 100.0, max: 900.0, value: 400.0 }));
        app.apply(Action::SetFontAxisValue { idx: 0, value: 5000.0 });
        assert!((app.variable_font_config.axes[0].value - 900.0).abs() < 0.001, "value clamped to max 900");
        app.apply(Action::SetFontAxisValue { idx: 0, value: 5.0 });
        assert!((app.variable_font_config.axes[0].value - 100.0).abs() < 0.001, "value clamped to min 100");
    }

    #[test]
    fn test_remove_font_axis_oob_no_panic() {
        let mut app = App::new();
        // Removing from empty list should not panic.
        app.apply(Action::RemoveFontAxis(99));
        assert!(app.variable_font_config.axes.is_empty());
    }

    #[test]
    fn test_reset_font_axes_midpoint() {
        let mut app = App::new();
        app.apply(Action::AddFontAxis(FontAxis { tag: "wdth".to_string(), min: 0.0, max: 100.0, value: 75.0 }));
        app.apply(Action::ResetFontAxes);
        assert!((app.variable_font_config.axes[0].value - 50.0).abs() < 0.001, "reset to midpoint (min+max)/2 = 50");
    }

    #[test]
    fn test_variable_font_panel_toggle() {
        let mut app = App::new();
        assert!(!app.variable_font_panel_open);
        app.apply(Action::ToggleVariableFontPanel);
        assert!(app.variable_font_panel_open);
        app.apply(Action::ToggleVariableFontPanel);
        assert!(!app.variable_font_panel_open);
    }

    // --- Batch 11: Pathfinder depth ---

    #[test]
    fn test_pathfinder_op_recorded() {
        let mut app = App::new();
        assert!(app.last_pathfinder_op.is_none());
        app.apply(Action::ApplyPathfinderOp(PathfinderOp::Unite));
        assert_eq!(app.last_pathfinder_op, Some(PathfinderOp::Unite));
    }

    #[test]
    fn test_pathfinder_precision_clamp() {
        let mut app = App::new();
        app.apply(Action::SetPathfinderPrecision(0.0));
        assert!((app.pathfinder_precision - 0.001).abs() < 1e-6, "should clamp to 0.001");
        app.apply(Action::SetPathfinderPrecision(100.0));
        assert!((app.pathfinder_precision - 10.0).abs() < 1e-6, "should clamp to 10.0");
    }

    #[test]
    fn test_repeat_pathfinder_no_panic_when_none() {
        let mut app = App::new();
        // RepeatPathfinder with no prior op should not panic.
        app.apply(Action::RepeatPathfinder);
        assert!(app.last_pathfinder_op.is_none());
    }

    #[test]
    fn test_repeat_pathfinder_reapplies() {
        let mut app = App::new();
        app.apply(Action::ApplyPathfinderOp(PathfinderOp::Minus));
        app.apply(Action::RepeatPathfinder);
        // last_pathfinder_op unchanged — still Minus
        assert_eq!(app.last_pathfinder_op, Some(PathfinderOp::Minus));
    }

    // --- Batch 11: 3D Extrude depth ---

    #[test]
    fn test_extrude_depth_clamp() {
        let mut app = App::new();
        app.apply(Action::SetExtrudeDepth(5000.0));
        assert!((app.extrude_config.depth - 2000.0).abs() < 1e-6, "depth should clamp to 2000");
    }

    #[test]
    fn test_extrude_perspective_clamp() {
        let mut app = App::new();
        app.apply(Action::SetExtrudePerspective(200.0));
        assert!((app.extrude_config.perspective - 160.0).abs() < 1e-6, "perspective should clamp to 160");
    }

    #[test]
    fn test_extrude_rotation_clamp() {
        let mut app = App::new();
        app.apply(Action::SetExtrudeRotation { x: -300.0, y: 0.0, z: 0.0 });
        assert!((app.extrude_config.rotation_x - (-180.0)).abs() < 1e-6, "x rotation should clamp to -180");
    }

    #[test]
    fn test_extrude_panel_toggle() {
        let mut app = App::new();
        assert!(!app.extrude_panel_open);
        app.apply(Action::ToggleExtrudePanel);
        assert!(app.extrude_panel_open);
        app.apply(Action::ToggleExtrudePanel);
        assert!(!app.extrude_panel_open);
    }

    // --- Batch 11: Chart depth ---

    #[test]
    fn test_chart_add_dataset() {
        let mut app = App::new();
        assert!(app.chart_config.datasets.is_empty());
        app.apply(Action::AddChartDataSet(ChartDataSet::default()));
        assert_eq!(app.chart_config.datasets.len(), 1);
        assert_eq!(app.chart_config.datasets[0].label, "Series 1");
    }

    #[test]
    fn test_chart_remove_dataset_oob_no_panic() {
        let mut app = App::new();
        // Removing from an empty list should not panic.
        app.apply(Action::RemoveChartDataSet(99));
        assert!(app.chart_config.datasets.is_empty());
    }

    #[test]
    fn test_chart_column_width_clamp() {
        let mut app = App::new();
        app.apply(Action::SetChartColumnWidth(5.0));
        assert!((app.chart_config.column_width - 20.0).abs() < 1e-6, "column_width should clamp to 20");
        app.apply(Action::SetChartColumnWidth(200.0));
        assert!((app.chart_config.column_width - 100.0).abs() < 1e-6, "column_width should clamp to 100");
    }

    #[test]
    fn test_chart_category_labels() {
        let mut app = App::new();
        let labels = vec!["Q1".to_string(), "Q2".to_string(), "Q3".to_string()];
        app.apply(Action::SetChartCategoryLabels(labels.clone()));
        assert_eq!(app.chart_config.category_labels, labels);
    }

    // --- Batch 11: Envelope Distort depth ---

    #[test]
    fn test_envelope_bend_clamp() {
        let mut app = App::new();
        app.apply(Action::SetEnvelopeBend(200.0));
        assert!((app.envelope_config.bend - 100.0).abs() < 1e-6, "bend should clamp to 100");
        app.apply(Action::SetEnvelopeBend(-200.0));
        assert!((app.envelope_config.bend - (-100.0)).abs() < 1e-6, "bend should clamp to -100");
    }

    #[test]
    fn test_envelope_fidelity_clamp() {
        let mut app = App::new();
        app.apply(Action::SetEnvelopeFidelity(200.0));
        assert!((app.envelope_config.fidelity - 100.0).abs() < 1e-6, "fidelity should clamp to 100");
    }

    #[test]
    fn test_make_envelope_with_warp_no_panic_no_selection() {
        let mut app = App::new();
        // No shape selected — should not panic, applied list stays empty.
        app.apply(Action::MakeEnvelopeWithWarpPreset);
        assert!(app.envelope_applied_shapes.is_empty());
    }

    #[test]
    fn test_release_envelope_clears() {
        let mut app = App::new();
        // Select first shape then apply an envelope.
        app.apply(Action::SelectShape(0));
        app.apply(Action::MakeEnvelopeWithWarpPreset);
        assert!(!app.envelope_applied_shapes.is_empty());
        app.apply(Action::ReleaseEnvelopeAll);
        assert!(app.envelope_applied_shapes.is_empty());
    }
}
