//! The `Action` enum: every panel-to-state mutation a panel can request.
//!
//! Split out of `app_state/mod.rs` as a pure mechanical refactor (no behavior
//! change). Panels emit these; the root view routes each into [`super::App::apply`].

use super::*;


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
    /// Replace the in-progress Text-tool run's string content (from a typed
    /// `TextField`), re-rasterizing the layer. No-op when no text edit is active.
    SetTextContent(String),

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
    /// Rename a layer (pure model state; no recomposite needed).
    RenameLayer { id: LayerId, name: String },

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
    /// Create a new blank document (resets canvas to defaults). Dispatched from
    /// the welcome screen "New Document" button and template cards.
    NewDocument,
    /// Open a file via a picker dialog. Dispatched from the welcome screen
    /// "Open File…" button. Delegates to OpenImage internally.
    OpenFile,
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

    // --- Free Transform: rotation + skew (extends translate/scale) ---
    /// Set the free-transform rotation angle (degrees, wrapped to -180..180).
    SetTransformRotation(f32),
    /// Set the free-transform skew (degrees): `(skew_x, skew_y)`, each clamped
    /// to ±89° to avoid a singular shear.
    SetTransformSkew { skew_x: f32, skew_y: f32 },
    /// Bake the current free-transform (translate + scale + rotation + skew) into
    /// the active layer's pixels, then reset the live transform to identity.
    ApplyFreeTransform,
    /// Reset all free-transform parameters to identity without baking.
    ResetFreeTransform,

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

    // --- Advanced filters: Surface Blur / Path Blur ---
    /// Apply an edge-preserving Surface Blur to the active layer. `radius` px,
    /// `threshold` 0..1 (color distance above which neighbours are rejected).
    ApplySurfaceBlur { radius: f32, threshold: f32 },
    /// Apply a directional Path Blur along `segments` (start/end pairs in doc px),
    /// scaled by `strength`. Approximates Photoshop's path-driven motion blur.
    ApplyPathBlur { segments: Vec<crate::filters_extra::PathSegment>, strength: f32 },

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

    // --- Phase 6: Retouching core (real algorithms — app_state/healing.rs) ---
    /// Healing brush: gradient-domain (Poisson) texture transplant, tone-matched.
    HealBrush { src_center: [f32; 2], dst_center: [f32; 2], radius: f32 },
    /// Clone-stamp dab: soft-round offset copy with falloff + `opacity`.
    CloneStampDab { src_center: [f32; 2], dst_center: [f32; 2], radius: f32, opacity: f32 },
    /// Red-eye removal: desaturate + darken red-dominant pupil pixels by `strength`.
    RemoveRedEye { center: [f32; 2], radius: f32, strength: f32 },
    /// Content-aware patch fill (seeded PatchMatch-lite); `seed` = deterministic.
    ContentAwarePatch { center: [f32; 2], radius: f32, seed: u64 },

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

    // ---- Batch 8: Extended Shape Primitives ------------------------------------
    /// Add a polygon shape layer.
    AddPolygonLayer { sides: u32, radius: f32, corner_radius: f32, color: [u8; 4] },
    /// Add a star shape layer.
    AddStarLayer { points: u32, inner_radius: f32, outer_radius: f32, color: [u8; 4] },
    /// Add a line shape layer.
    AddLineLayer { x1: f32, y1: f32, x2: f32, y2: f32, width: f32, color: [u8; 4] },
    /// Add a rounded rectangle shape layer.
    AddRoundedRectLayer { width: f32, height: f32, corner_radius: f32, color: [u8; 4] },
    /// Add a triangle shape layer.
    AddTriangleLayer { base: f32, height: f32, rotation: f32, color: [u8; 4] },
    /// Set the number of sides on a polygon shape layer.
    SetShapeSides { layer_id: usize, sides: u32 },
    /// Set the corner radius on a polygon or rounded-rect shape layer.
    SetShapeCornerRadius { layer_id: usize, radius: f32 },
    /// Set the number of points on a star shape layer.
    SetStarPoints { layer_id: usize, points: u32 },
    /// Set the inner radius on a star shape layer.
    SetStarInnerRadius { layer_id: usize, inner_radius: f32 },
    /// Set the stroke width on a line shape layer.
    SetLineWidth { layer_id: usize, width: f32 },
    /// Set the line cap style.
    SetLineCap { layer_id: usize, cap: crate::app_state::shapes::LineCap },
    /// Set the line join style.
    SetLineJoin { layer_id: usize, join: crate::app_state::shapes::LineJoin },
    /// Set the stroke color and width on any shape layer.
    SetShapeStroke { layer_id: usize, color: [u8; 4], width: f32 },
    /// Set the fill color on any shape layer.
    SetShapeFill { layer_id: usize, color: [u8; 4] },

    // ---- Batch 8: Boolean Shape Operations ------------------------------------
    /// Apply a boolean operation across selected shape layers.
    BooleanShapeOp { layer_ids: Vec<usize>, op: crate::app_state::shapes::BooleanOp },
    /// Convert a shape's stroke outline to a filled shape.
    ExpandStroke { layer_id: usize },
    /// Rasterize selected vector/text/generated layers into one pixel layer.
    FlattenToPixels { layer_ids: Vec<usize> },

    // ---- Batch 8: Clipping Masks (extended) -----------------------------------
    /// Set or clear the clipping mask flag on a layer.
    SetClippingMask { layer_id: prism_core::LayerId, clipped: bool },
    /// Convenience: set clipped = true.
    CreateClippingMask { layer_id: prism_core::LayerId },
    /// Convenience: set clipped = false.
    ReleaseClippingMask { layer_id: prism_core::LayerId },

    // ---- Batch 8: Extended Layer Styles ---------------------------------------
    /// Set or clear the Satin effect on a layer.
    SetSatinEffect { layer_id: usize, config: crate::app_state::shapes::SatinEffect },
    /// Set or clear the extended Color Overlay on a layer.
    SetExtendedColorOverlay { layer_id: usize, config: crate::app_state::shapes::ColorOverlay },
    /// Set or clear the Gradient Overlay on a layer.
    SetGradientOverlay { layer_id: usize, config: crate::app_state::shapes::GradientOverlay },
    /// Set or clear the Pattern Overlay on a layer.
    SetPatternOverlay { layer_id: usize, config: crate::app_state::shapes::PatternOverlay },
    /// Destructively rasterize the stored Satin effect into the layer's pixels.
    BakeSatinEffect { layer_id: usize },
    /// Destructively rasterize the stored Pattern Overlay into the layer's pixels.
    BakePatternOverlay { layer_id: usize },
    /// Set the blend mode for a specific style kind on a layer.
    SetLayerStyleBlendMode { layer_id: usize, style: crate::app_state::shapes::StyleKind, blend_mode: String },
    /// Set the opacity for a specific style kind on a layer.
    SetLayerStyleOpacity { layer_id: usize, style: crate::app_state::shapes::StyleKind, opacity: u8 },
    /// Copy all extended layer styles from a layer to the style clipboard.
    CopyLayerStylesExt { from_layer_id: usize },
    /// Paste extended styles from the clipboard to given layers.
    PasteLayerStylesExt { to_layer_ids: Vec<usize> },
    /// Clear all extended layer styles from a layer.
    ClearLayerStylesExt { layer_id: usize },

    // ---- Batch 8: PSD Export Config -------------------------------------------
    /// Set the PSD export output path.
    SetPsdExportPath(String),
    /// Set maximize-compatibility flag for PSD export.
    SetPsdMaximizeCompatibility(bool),
    /// Set the PSD encoding method.
    SetPsdEncoding(crate::app_state::shapes::PsdEncoding),
    /// Set whether to embed the color profile in PSD export.
    SetPsdEmbedColorProfile(bool),
    /// Trigger PSD export (stub — records last export path).
    ExportAsPsd { path: String },

    // ---- New Feature: Rich (non-destructive) Smart Objects ------------------
    /// Capture a layer's pixels into a new embedded Smart Object.
    EmbedSmartObject { layer_id: usize },
    /// Set an embedded Smart Object's non-destructive scale.
    SetSmartObjectScale { so_id: usize, scale: f32 },
    /// Set an embedded Smart Object's non-destructive rotation (degrees).
    SetSmartObjectRotation { so_id: usize, degrees: f32 },
    /// Set an embedded Smart Object's non-destructive skew (degrees).
    SetSmartObjectSkew { so_id: usize, skew_x: f32, skew_y: f32 },
    /// Set an embedded Smart Object's non-destructive translation (output px).
    SetSmartObjectTranslate { so_id: usize, x: f32, y: f32 },
    /// Push a non-destructive filter onto an embedded Smart Object's stack.
    AddSmartObjectFilter { so_id: usize, filter: self::smart_objects_rich::SmartFilter },
    /// Clear an embedded Smart Object's filter stack.
    ClearSmartObjectFilters { so_id: usize },
    /// Reset an embedded Smart Object's transform to identity.
    ResetSmartObjectTransform { so_id: usize },
    /// Re-render an embedded Smart Object (source → transform → filters) and
    /// upload the result into its bound layer.
    UpdateSmartObject { so_id: usize },
    /// Bake an embedded Smart Object: render + upload, then drop the embed.
    BakeSmartObject { so_id: usize },

    // ---- New Feature: Actions / batch automation ----------------------------
    /// Begin (or resume) recording actions into a named set.
    StartRecording { name: String },
    /// Stop the active recording.
    StopRecording,
    /// Replay a recorded action set by index.
    PlayActionSet { index: usize },
    /// Replay a recorded action set by name.
    PlayActionSetByName { name: String },
    /// Delete a recorded action set by index.
    DeleteActionSet { index: usize },
    /// Clear all steps from a recorded action set (keep the set).
    ClearActionSet { index: usize },
    /// Rename a recorded action set.
    RenameActionSet { index: usize, name: String },

    // ---- New Feature: Scripting sandbox -------------------------------------
    /// Run an inline script (rhai or line-DSL, auto-detected) immediately.
    RunScript { source: String },
    /// Set the persistent script-editor source buffer.
    SetScriptSource(String),
    /// Run the current script-editor source buffer.
    RunCurrentScript,
    /// Clear the accumulated script log.
    ClearScriptLog,

    // ---- New Feature: Rich preferences --------------------------------------
    /// Set the RAM-usage fraction (Performance pane).
    SetPrefMemoryFraction(f32),
    /// Set the number of history (undo) states.
    SetPrefHistoryStates(u32),
    /// Toggle GPU compositing.
    SetPrefUseGpu(bool),
    /// Set the working RGB space name (Color pane).
    SetPrefWorkingRgb(String),
    /// Set the default rendering intent label.
    SetPrefRenderingIntent(String),
    /// Set the UI theme (Interface pane).
    SetPrefTheme(String),
    /// Set the UI scale factor.
    SetPrefUiScale(f32),
    /// Set the autosave interval in minutes (File Handling pane).
    SetPrefAutosaveMinutes(u32),
    /// Set the number of recent files to remember.
    SetPrefRecentFileCount(u32),
    /// Reset all rich preferences to defaults.
    ResetPreferences,
    /// Persist rich preferences to disk.
    SavePreferences,
    /// Load rich preferences from disk.
    LoadPreferences,

    // ---- New Feature: Per-artboard export metadata --------------------------
    /// Set an artboard's export format.
    SetArtboardExportFormat { id: u64, format: self::artboards::ArtboardExportFormat },
    /// Set an artboard's export scale multiplier.
    SetArtboardExportScale { id: u64, scale: f32 },
    /// Set an artboard's export filename prefix/suffix.
    SetArtboardExportNaming { id: u64, prefix: String, suffix: String },
    /// Set an artboard's export quality (JPEG/WebP).
    SetArtboardExportQuality { id: u64, quality: u8 },
    /// Enable/disable an artboard in batch export.
    SetArtboardExportEnabled { id: u64, enabled: bool },
    /// Compute (and store) the per-artboard export plan.
    PrepareArtboardExport,
    // ---- New: Native PSD serializer -------------------------------------------
    /// Choose RLE (true) or raw (false) channel compression for native PSD export.
    SetPsdExportCompression(bool),
    /// Serialize the live document with the native writer and save to `path`.
    ExportPsdNative(std::path::PathBuf),

    // ---- New: Guides / rulers / smart guides ----------------------------------
    /// Add a canvas guide; `horizontal` true = horizontal line, else vertical.
    AddCanvasGuide { horizontal: bool, position: f32 },
    /// Remove a canvas guide by id (ignored if locked).
    RemoveCanvasGuide(u64),
    /// Clear all unlocked canvas guides.
    ClearCanvasGuides,
    /// Lock/unlock a canvas guide.
    LockCanvasGuide { id: u64, locked: bool },
    /// Move an unlocked canvas guide to a new position.
    MoveCanvasGuide { id: u64, position: f32 },
    /// Toggle whether dragging snaps to guides.
    SetGuideSnapEnabled(bool),
    /// Set the snap distance (doc px) for guide snapping.
    SetGuideSnapDistance(f32),
    /// Toggle guide visibility.
    SetGuidesVisible(bool),
    /// Set the ruler measurement unit.
    SetRulerUnit(self::guides::RulerUnit),
    /// Toggle smart-guide alignment detection.
    SetSmartGuidesEnabled(bool),

    // ---- New: Color management ------------------------------------------------
    /// Set the document's working color mode (RGB/Grayscale/CMYK/Lab).
    SetWorkingColorMode(self::color_management::WorkingColorMode),
    /// Assign a working color space (tag only — no pixel conversion).
    AssignWorkingSpace(self::color_management::WorkingSpace),
    /// Convert to a working color mode + space.
    ConvertWorkingSpace {
        mode: self::color_management::WorkingColorMode,
        space: self::color_management::WorkingSpace,
    },
    /// Toggle whether the color profile is embedded on export.
    SetEmbedColorProfile(bool),

    // ---- New: Keyboard shortcuts remap ----------------------------------------
    /// Rebind a command to a chord string (e.g. "Cmd+Shift+S"); conflicts refused.
    RemapShortcut { command: String, chord: String },
    /// Remove a command's binding.
    UnbindShortcut(String),
    /// Reset all shortcuts to Photoshop-like defaults.
    ResetShortcuts,
    /// Load shortcuts from a JSON string.
    LoadShortcutsJson(String),
    /// Save shortcuts to a JSON file at `path`.
    SaveShortcutsJson(std::path::PathBuf),

    // ---- New: Navigator + multi-doc tabs --------------------------------------
    /// Open a new document tab.
    OpenDocTab { title: String, width: u32, height: u32 },
    /// Close a document tab by id.
    CloseDocTab(u64),
    /// Activate a document tab by id.
    ActivateDocTab(u64),
    /// Activate a document tab by ordinal index.
    ActivateDocTabIndex(usize),
    /// Reorder a tab from one index to another.
    ReorderDocTab { from: usize, to: usize },
    /// Set a tab's dirty (unsaved-changes) flag.
    SetDocTabDirty { id: u64, dirty: bool },
    /// Set the navigator zoom factor.
    NavigatorZoom(f32),
    /// Pan the navigator viewport by a doc-px delta.
    NavigatorPan { dx: f32, dy: f32 },
    /// Re-center the navigator viewport on a doc-px point.
    NavigatorCenter { x: f32, y: f32 },
}
