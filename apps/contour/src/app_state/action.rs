//! The `Action` enum: every panel→state mutation a panel can request.
//!
//! Split out of `mod.rs` (mechanical extraction). Panels emit these `Action`
//! variants; the root view routes each into [`super::App::apply`].

use super::*;

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

    // --- Editable names / text content (TextField-driven, real typing) ---
    /// Rename the shape (Layers panel row) at paint index `idx`. A blank name
    /// clears back to the type label. One undo step.
    RenameLayer { idx: usize, name: String },
    /// Rename the symbol master with library id `id` (uniqued in `symbol_lib`).
    RenameSymbol { id: u64, name: String },
    /// Replace the full text content of the text object at paint index `idx`
    /// (re-lays-out its glyphs). Used by the Type tool's live text field.
    SetTextObjectContent { idx: usize, text: String },

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

    // --- Distort & Transform family (path_distort.rs) ---
    /// Offset path `shape_id` by `distance` (>0 out, <0 in) with corner `join`.
    DistortOffsetPath { shape_id: usize, distance: f32, join: OffsetJoin },
    /// Roughen: subdivide each segment to `detail` then jitter every point by up
    /// to `size`, seeded by `seed` (SplitMix64); `smooth` curves the result.
    DistortRoughen { shape_id: usize, size: f32, detail: usize, seed: u64, smooth: bool },
    /// Zig-Zag: `ridges` alternating peaks of amplitude `size` per segment;
    /// `smooth` makes a wave instead of corners.
    DistortZigZag { shape_id: usize, size: f32, ridges: usize, smooth: bool },
    /// Pucker (`amount`<0) / Bloat (`amount`>0) about the path centroid.
    DistortPuckerBloat { shape_id: usize, amount: f32 },
    /// Twist points about the centroid by `angle_deg`, scaled by distance.
    DistortTwist { shape_id: usize, angle_deg: f32 },
    /// Transform Each (about each shape's centre) with optional replication.
    DistortTransformEach { shape_id: usize, move_x: f32, move_y: f32, scale_x: f32, scale_y: f32, angle_deg: f32, copies: usize },

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

    // --- Welcome panel ---
    /// Dismiss the welcome panel (user clicked New / Open / a template).
    DismissWelcome,
    /// Create a fresh empty document (triggered from the welcome panel or File menu).
    NewDocument,
    /// Open an existing file (triggered from the welcome panel or File menu).
    OpenFile,

    // =========================================================
    // Batch 10 (new): Variable Fonts & OpenType
    // =========================================================
    /// Set one variable-font axis value for `shape_id` (adds or replaces the axis entry).
    SetVariableAxis { shape_id: usize, axis_tag: String, value: f32 },
    /// Remove all variable-font axis overrides for `shape_id`.
    ResetVariableAxes { shape_id: usize },
    /// Toggle a named OpenType feature for `shape_id`.
    /// Feature names: "ligatures", "disc_lig", "hist_lig", "calt", "smcp",
    /// "c2sc", "ordn", "frac", "zero", "tnum", "pnum", "sups", "subs".
    SetOpenTypeFeature { shape_id: usize, feature: String, enabled: bool },
    /// Set the stylistic-set number (1–20, or None to clear) for `shape_id`.
    SetStylisticSet { shape_id: usize, set: Option<u8> },
    /// Enable all-small-caps for `shape_id`.
    ApplyAllSmallCaps { shape_id: usize },

    // =========================================================
    // Batch 10 (new): Character & Paragraph Panel
    // =========================================================
    /// Set letter-tracking (em units) for `shape_id`.
    SetCharacterTracking { shape_id: usize, tracking: f32 },
    /// Set kerning mode for `shape_id`.
    SetCharacterKerning { shape_id: usize, mode: KerningMode },
    /// Set baseline shift (points) for `shape_id`.
    SetBaselineShift { shape_id: usize, shift: f32 },
    /// Set horizontal scale (%, clamped 1–1000) for `shape_id`.
    SetHorizontalScale { shape_id: usize, scale: f32 },
    /// Set vertical scale (%, clamped 1–1000) for `shape_id`.
    SetVerticalScale { shape_id: usize, scale: f32 },
    /// Toggle underline for `shape_id`.
    SetUnderline { shape_id: usize, underline: bool },
    /// Toggle strikethrough for `shape_id`.
    SetStrikethrough { shape_id: usize, strikethrough: bool },
    /// Set paragraph alignment for `shape_id`.
    SetParagraphAlignment { shape_id: usize, alignment: ParaAlignment },
    /// Set paragraph spacing-before and spacing-after (points) for `shape_id`.
    SetParagraphSpacing { shape_id: usize, before: f32, after: f32 },
    /// Set first-line indent (points) for `shape_id`.
    SetFirstLineIndent { shape_id: usize, indent: f32 },
    /// Toggle hyphenation for `shape_id`.
    SetHyphenation { shape_id: usize, enabled: bool },
    /// Add a tab stop (in points) for `shape_id`.
    AddTabStop { shape_id: usize, position: f32 },
    /// Remove a tab stop (by position) for `shape_id`.
    RemoveTabStop { shape_id: usize, position: f32 },

    // =========================================================
    // Batch 10 (new): Blend Tool (blend objects)
    // =========================================================
    /// Create a blend object between shapes `shape_id_a` and `shape_id_b`.
    MakeBlend { shape_id_a: usize, shape_id_b: usize, spacing: BlendSpacing },
    /// Release a blend object (remove the blend, keep source shapes).
    ReleaseBlend { blend_id: usize },
    /// Expand a blend object into independent intermediate shapes.
    ExpandBlend { blend_id: usize },
    /// Set the spacing/steps for a blend.
    SetBlendSpacing { blend_id: usize, spacing: BlendSpacing },
    /// Set the orientation for a blend.
    SetBlendOrientation { blend_id: usize, orientation: BlendOrientation },
    /// Replace the spine of a blend with `path_id`.
    ReplaceBlendSpine { blend_id: usize, path_id: usize },
    /// Reverse the order of shapes in a blend.
    ReverseBlend { blend_id: usize },
    /// Reverse the direction of the blend spine.
    ReverseBlendSpine { blend_id: usize },

    // =========================================================
    // Batch 10 (new): 3D Effects (Extrude & Bevel, Revolve)
    // =========================================================
    /// Apply a 3D Extrude & Bevel effect to `shape_id`.
    Apply3DExtrude { shape_id: usize, config: Extrude3D },
    /// Update specific parameters of an existing 3D extrude on `shape_id`.
    Update3DExtrude {
        shape_id: usize,
        depth: Option<f32>,
        rotate_x: Option<f32>,
        rotate_y: Option<f32>,
        rotate_z: Option<f32>,
    },
    /// Remove any 3D effect (extrude or revolve) from `shape_id`.
    Remove3DEffect { shape_id: usize },
    /// Apply a 3D Revolve effect to `shape_id`.
    Apply3DRevolve { shape_id: usize, config: Revolve3D },
    /// Update specific parameters of an existing 3D revolve on `shape_id`.
    Update3DRevolve { shape_id: usize, angle: Option<f32>, offset: Option<f32> },
    /// Set the lighting parameters for the 3D effect on `shape_id`.
    Set3DLighting { shape_id: usize, intensity: f32, ambient: f32 },
    /// Set the perspective (0–160 degrees) for the 3D extrude on `shape_id`.
    Set3DPerspective { shape_id: usize, degrees: f32 },

    // =========================================================
    // Batch 10 (new): PDF Export State
    // =========================================================
    /// Set the PDF standard compliance level.
    SetPdfStandard(PdfStandard),
    /// Set the Acrobat compatibility level.
    SetPdfCompatibility(PdfCompatibility),
    /// Toggle font embedding.
    SetPdfEmbedFonts(bool),
    /// Toggle transparency flattening.
    SetPdfFlattenTransparency(bool),
    /// Set the output colour space.
    SetPdfColorSpace(PdfColorSpace),
    /// Set bleed extents (points, all 4 sides).
    SetPdfBleed { top: f32, bottom: f32, left: f32, right: f32 },
    /// Set printer's marks.
    SetPdfMarks(PdfMarks),
    /// Set user and owner passwords.
    SetPdfPassword { user: String, owner: String },
    /// Set permissions flags.
    SetPdfPermissions { printing: bool, editing: bool, copying: bool },
    /// Export the document as PDF to `path` (stub: records path in status).
    ExportAsPdf { path: String },

    // =========================================================
    // Batch 12: Chart geometry, Document Setup, Symbol Edit
    // (Pattern Brush reuses the existing `ApplyPatternBrushToSelected` action.)
    // =========================================================
    /// Set the document setup default artboard size (document points).
    SetDocSetupSize { width: f32, height: f32 },
    /// Set the document display / ruler unit.
    SetDocSetupUnit(DocUnit),
    /// Set the document colour working space.
    SetDocSetupColorMode(DocColorMode),
    /// Set per-side bleed (document points): `[top, right, bottom, left]`.
    SetDocSetupBleed { top: f32, right: f32, bottom: f32, left: f32 },
    /// Create a brand-new document from the current [`DocumentSetup`] (clears the
    /// canvas and seeds an artboard at the configured size).
    NewDocumentFromSetup,

    /// Enter symbol-edit mode for symbol `id`: load its master shapes into the
    /// canvas for in-place editing.
    EnterSymbolEdit(u64),
    /// Exit symbol-edit mode, writing the edited shapes back to the symbol
    /// definition (propagating to every placed instance) and restoring the
    /// document artwork.
    ExitSymbolEdit,
    // Batch 13: real 3D expand, gradient mesh, export, picker, prefs
    // =========================================================
    /// **Object ▸ Expand** a 3D Extrude on `shape_id`: render its stored config
    /// into flat shaded vector faces, replacing the source shape.
    ExpandExtrudeRender { shape_id: usize },
    /// **Object ▸ Expand** a 3D Revolve on `shape_id` into flat lathe faces.
    ExpandRevolveRender { shape_id: usize },

    /// **Object ▸ Create Gradient Mesh** — build a `rows × cols` mesh over the
    /// selected shape's bounds, seeded from its fill colour.
    CreateGradientMeshObject { rows: usize, cols: usize },
    /// Recolour gradient-mesh node `(row, col)`.
    SetGradientMeshNodeColor { row: usize, col: usize, color: [f32; 4] },
    /// Move gradient-mesh node `(row, col)` to a document-space point.
    MoveGradientMeshNode { row: usize, col: usize, x: f32, y: f32 },
    /// Insert a gradient-mesh row after row `after` (interpolated).
    AddGradientMeshRow { after: usize },
    /// Insert a gradient-mesh column after column `after` (interpolated).
    AddGradientMeshColumn { after: usize },
    /// Drop the active gradient-mesh object.
    ClearGradientMeshObject,

    /// Export the document to `path` in `format` (PNG / SVG / EPS / PDF), writing
    /// the bytes to disk.
    ExportDocument { path: String, format: crate::export_formats::ExportFormat },

    /// Set the colour-picker RGB (each `0..1`).
    SetPickerRgb { r: f32, g: f32, b: f32 },
    /// Set the colour-picker HSB (hue degrees, sat/bri `0..1`).
    SetPickerHsb { h: f32, s: f32, b: f32 },
    /// Set the colour-picker CMYK (each `0..1`).
    SetPickerCmyk { c: f32, m: f32, y: f32, k: f32 },
    /// Set the colour-picker colour from a hex string.
    SetPickerHex(String),
    /// Apply the colour-picker colour as the fill of every selected shape.
    ApplyPickerToSelection,

    /// Set the preference: maximum undo levels.
    SetPrefUndoLevels(u32),
    /// Set the preference: snap to anchors / points.
    SetPrefSnapToPoint(bool),
    /// Set the preference: snap to grid.
    SetPrefSnapToGrid(bool),
    /// Set the preference: show the grid.
    SetPrefShowGrid(bool),
    /// Set the preference: grid spacing (document points).
    SetPrefGridSpacing(f32),
    /// Set the preference: ruler / display unit.
    SetPrefUnit(crate::app_state::prefs_color::PrefUnit),
    /// Replace all preferences from a JSON document (sanitized on load).
    LoadPreferencesJson(String),

    // --- Typeable numeric inspector + layers filter ---
    /// Move the whole selection so its bounding box's left edge sits at the given
    /// document-space X. No-op when the selection has no geometry.
    SetSelectionX(f32),
    /// Move the whole selection so its bounding box's top edge sits at the given
    /// document-space Y.
    SetSelectionY(f32),
    /// Scale the selection horizontally so its bounding-box width equals the
    /// given value (about the bbox centre). Clamped to a tiny positive minimum.
    SetSelectionWidth(f32),
    /// Scale the selection vertically so its bounding-box height equals the given
    /// value (about the bbox centre).
    SetSelectionHeight(f32),
    /// Set the case-insensitive substring filter shown in the Layers panel.
    SetLayerFilter(String),
}
