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
pub mod prefs_color;
pub use prefs_color::*;

mod apply_advanced;
mod apply_extended;
mod apply_extended2;
mod apply_extended3;
mod tests_core;
mod apply_core;
mod apply_waves_w11;
mod tests_wave9;
mod apply_waves_wn;
mod apply_extended4;
mod apply_waves2_b24;
mod tests_wave_n;
mod tests_batch7;
mod tests_core2;
mod apply_waves;
mod apply_waves2;
mod apply_batch10;
mod tests_batch10;
mod apply_batch11;
mod apply_batch13;
mod geometry_warp;
mod apply_batch12;
mod apply_textfield;
pub(super) mod helpers;
use helpers::{shape_to_svg, rgba_to_hex, path_to_svg_d, import_svg, parse_svg_path_d};
pub(super) mod helpers_geo;
use helpers_geo::{rand_group_id, default_image_trace_threshold, default_image_trace_colors, default_omask_id_counter, default_graph_style_fill, colors_approx_equal, offset_polygon, sample_polyline, warp_shape_perspective, bounds_intersect, sample_document};
mod action;
pub use action::*;
mod apply_dispatch;

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
    /// Last document-space point the Live Paint bucket targeted, set by the
    /// canvas before an `ApplyLivePaint` (which carries only a fill colour) so the
    /// enclosed-face fill knows where the user clicked.
    pub live_paint_hit: Option<(f32, f32)>,

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

    // --- Welcome panel ---
    /// Whether the welcome panel is currently visible (true on launch, false after
    /// the user chooses New / Open / a template).
    pub show_welcome: bool,

    // --- Batch 10 (new): Variable Fonts & OpenType ---
    /// Per-shape variable-font axis values (shape_id → list of axis values).
    pub variable_axis_values: std::collections::HashMap<usize, Vec<VariableAxisValue>>,
    /// Per-shape OpenType feature flags (shape_id → feature set).
    pub opentype_features: std::collections::HashMap<usize, OpenTypeFeatures>,

    // --- Batch 10 (new): Character & Paragraph Panel ---
    /// Per-shape character style (tracking, kerning, baseline, scale, decoration).
    pub char_styles: std::collections::HashMap<usize, CharacterStyle>,
    /// Per-shape paragraph style (alignment, spacing, indent, hyphenation, tabs).
    pub para_styles: std::collections::HashMap<usize, ParagraphStyle>,

    // --- Batch 10 (new): Blend Tool ---
    /// All blend objects in the document.
    pub blends: Vec<BlendObject>,
    /// Counter for assigning stable blend ids.
    pub next_blend_id: usize,

    // --- Batch 10 (new): 3D Effects ---
    /// Per-shape 3D Extrude & Bevel configurations.
    pub extrude_3d: std::collections::HashMap<usize, Extrude3D>,
    /// Per-shape 3D Revolve configurations.
    pub revolve_3d: std::collections::HashMap<usize, Revolve3D>,

    // --- Batch 10 (new): PDF Export State ---
    /// Detailed PDF export configuration.
    pub pdf_export_config: PdfExportConfig,

    // --- Batch 12: Document Setup ---
    /// Document-level setup (artboard dimensions, units, colour mode, bleed).
    pub doc_setup: DocumentSetup,

    // --- Batch 12: Symbol Edit Mode ---
    /// When `Some(id)`, the editor is **inside** symbol `id`'s definition: the
    /// symbol's master shapes are loaded into `doc.shapes` for editing, and
    /// `ExitSymbolEdit` writes them back to the library (propagating to every
    /// instance). `None` = editing the document normally.
    pub editing_symbol: Option<u64>,
    /// The document shapes saved on entering symbol-edit mode, restored on exit
    /// so the main artwork is untouched by an in-place symbol edit.
    pub symbol_edit_backup: Option<Vec<Shape>>,
    // --- Batch 13: gradient mesh object, colour picker, preferences ---
    /// The active gradient-mesh object (a grid of colour nodes), if one has been
    /// created via `Object ▸ Create Gradient Mesh`.
    pub gradient_mesh_obj: Option<crate::gradient_mesh::GradientMesh>,
    /// The colour picker's live colour (RGB / HSB / CMYK / hex views).
    pub color_picker: prefs_color::ColorPicker,
    /// Application preferences (undo levels, snap, grid, units).
    pub preferences: prefs_color::Preferences,
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
            live_paint_hit: None,
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
            show_welcome: true,
            // Batch 10 (new)
            variable_axis_values: std::collections::HashMap::new(),
            opentype_features: std::collections::HashMap::new(),
            char_styles: std::collections::HashMap::new(),
            para_styles: std::collections::HashMap::new(),
            blends: Vec::new(),
            next_blend_id: 0,
            extrude_3d: std::collections::HashMap::new(),
            revolve_3d: std::collections::HashMap::new(),
            pdf_export_config: PdfExportConfig::default(),
            doc_setup: DocumentSetup::new(),
            editing_symbol: None,
            symbol_edit_backup: None,
            // Batch 13
            gradient_mesh_obj: None,
            color_picker: prefs_color::ColorPicker::default(),
            preferences: prefs_color::Preferences::default(),
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
        // Stage 1 of the dispatch chain lives in `apply_dispatch.rs`; it handles
        // the core inline arms and falls through to `apply_batch10` (and the rest
        // of the chain) for everything else.
        self.apply_inline_core(action);
    }
}


impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

