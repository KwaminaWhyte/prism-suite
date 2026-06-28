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
mod filters_advanced;
mod filters_blur;
mod text;
mod transforms;
mod smart_objects;
mod smart_objects_rich;
mod automation;
mod scripting;
mod prefs;
mod artboards;
mod ai;
mod layer_3d;
mod export;
mod shapes;
mod layer_styles_extra;
mod transform_extra;
mod redeye;
mod healing;
mod liquify;
mod tests_shapes;
mod psd_export;
mod guides;
mod color_management;
mod shortcuts;
mod navigator;
mod action;
mod construct;
mod interaction;
mod helpers;
#[cfg(test)]
mod tests_state;
#[cfg(test)]
mod tests_state_b;
#[cfg(test)]
mod healing_tests;
#[cfg(test)]
mod liquify_tests;

pub use self::transforms::{CaFillMethod, ContentAwareCropConfig};
pub use self::smart_objects::{SmartObjectKind, SmartObject, EdgeDetectMode, SelectMaskConfig};
pub use self::smart_objects_rich::{EmbeddedSmartObject, SmartTransform, SmartFilter as SmartObjectFilter, SmartFilterStack};
pub use self::automation::{ActionSet, AutomationState};
pub use self::scripting::{ScriptCmd, ScriptOutcome};
pub use self::prefs::{PigmentPreferences, PerformancePrefs, ColorPrefs, InterfacePrefs, FileHandlingPrefs};
pub use self::prefs::AppPrefs;
pub use self::artboards::{ArtboardExportConfig, ArtboardExportFormat, ArtboardExportItem};
pub use self::ai::{GenerativeFillResult, SkyPreset, SkyReplaceConfig, SelectSubjectMode, SelectSubjectResult};
pub use self::layer_3d::{Shape3DKind, Layer3DProps};
pub use self::canvas::{Tool, Slice, ColorProfile, SoftProofMode, HistogramChannel, ColorMode};
pub use self::painting::{Brush, PenNode, LiquifyMode, HealMode, PatternDef, SpotHealMode, LiquifyTool, LiquifyStroke, LiquifyMesh};
pub use self::layers::{LayerComp, LayerCompState};
pub use self::selections::{VanishingToolMode, VanishingPlane, Artboard, ApplyImageChannel, ApplyImageParams, ProofProfile, RenderingIntent, SoftProofSettings};
pub use self::filters::{Filter, SmartSharpenMode, ToneMapMethod, NeuralFilter, NeuralFilterKind};
pub use self::filters_blur::GalleryBlur;
pub use self::layers::{AdjKind, Shadow, Glow, Bevel, LayerStyle, DropShadowFx, OuterGlowFx, BevelStyle, BevelTechnique, BevelEmbossFx, LayerEffects};
pub use self::export::{PrintLayout, SmartFilter, ExportFormat, ExportPreset, MatchColorConfig, CameraRawConfig, HdrToneMappingMethod, HdrMergeConfig};
pub use self::shapes::{ExtendedShapeKind, ExtendedShapeLayer, LineCap, LineJoin, BooleanOp, PsdEncoding, PsdExportConfig, SatinEffect, ColorOverlay, GradientOverlay, PatternOverlay, GradientOverlayStyle, ContourType, BevelDirection, StyleKind};
use self::selections::AlphaChannel;
pub use self::selections::BlendIf;
pub use self::psd_export::{PsdCompression, PsdDocument, PsdLayerInput, PsdHeader, serialize_psd, read_psd_header, blend_key_for_shader_id};
pub use self::guides::{Guide, GuideOrientation, GuideState, RulerUnit, SmartGuideHit, AlignKind, detect_smart_guides};
pub use self::color_management::{WorkingColorMode, WorkingSpace, ColorManagement, srgb_to_cmyk, cmyk_to_srgb, srgb_to_lab, lab_to_srgb, srgb_luma};
pub use self::shortcuts::{KeyChord, ShortcutMap};
pub use self::navigator::{DocTab, DocTabs, NavigatorView};
pub use self::action::Action;
// Free helper functions extracted to `helpers.rs`. Re-export at crate visibility
// so the dispatcher, sibling domain modules (`use super::*`), and tests keep
// calling them unqualified exactly as before the split.
pub(crate) use self::helpers::*;

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
    /// Free-transform rotation angle in degrees (0 = upright). Composed with
    /// scale/skew/translate by `compute_xform_full` when baking.
    pub xform_rotation_deg: f32,
    /// Free-transform horizontal skew in degrees.
    pub xform_skew_x_deg: f32,
    /// Free-transform vertical skew in degrees.
    pub xform_skew_y_deg: f32,

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
    /// In-progress non-destructive Liquify warp session (None = not liquifying).
    pub liquify_session: Option<self::liquify::LiquifySession>,

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

    // ---- Batch 8: Extended Shape Primitives ------------------------------------
    /// Extended shape layer definitions keyed by raw LayerId usize.
    pub extended_shapes: std::collections::HashMap<usize, crate::app_state::shapes::ExtendedShapeLayer>,

    // ---- Batch 8: Extended Layer Styles ----------------------------------------
    /// Per-layer Satin effects keyed by raw layer id (usize).
    pub satin_effects: std::collections::HashMap<usize, crate::app_state::shapes::SatinEffect>,
    /// Per-layer extended Color Overlay keyed by raw layer id (usize).
    pub color_overlays: std::collections::HashMap<usize, crate::app_state::shapes::ColorOverlay>,
    /// Per-layer Gradient Overlay keyed by raw layer id (usize).
    pub gradient_overlays: std::collections::HashMap<usize, crate::app_state::shapes::GradientOverlay>,
    /// Per-layer Pattern Overlay keyed by raw layer id (usize).
    pub pattern_overlays: std::collections::HashMap<usize, crate::app_state::shapes::PatternOverlay>,
    /// Clipboard for copy/paste of extended Satin effect.
    pub style_clipboard_satin: Option<crate::app_state::shapes::SatinEffect>,
    /// Clipboard for copy/paste of extended Color Overlay.
    pub style_clipboard_color_overlay: Option<crate::app_state::shapes::ColorOverlay>,
    /// Clipboard for copy/paste of extended Gradient Overlay.
    pub style_clipboard_gradient_overlay: Option<crate::app_state::shapes::GradientOverlay>,
    /// Clipboard for copy/paste of extended Pattern Overlay.
    pub style_clipboard_pattern_overlay: Option<crate::app_state::shapes::PatternOverlay>,

    // ---- Batch 8: PSD Export Config -------------------------------------------
    /// Configuration for the next PSD export.
    pub psd_export_config: crate::app_state::shapes::PsdExportConfig,
    /// Path of the last successful PSD export (stub).
    pub last_psd_export_path: Option<String>,

    // ---- New Feature: Rich (non-destructive) Smart Objects ------------------
    /// Embedded Smart Objects (source pixels + transform + filter stack).
    pub embedded_smart_objects: Vec<self::smart_objects_rich::EmbeddedSmartObject>,
    /// Auto-incrementing id counter for embedded Smart Objects.
    pub embedded_so_counter: usize,

    // ---- New Feature: Actions / batch automation ----------------------------
    /// Action recorder + saved replayable action sets.
    pub automation: self::automation::AutomationState,

    // ---- New Feature: Scripting sandbox -------------------------------------
    /// Persistent script-editor source buffer.
    pub script_source: String,
    /// Accumulated script log lines.
    pub script_log: Vec<String>,

    // ---- New Feature: Rich preferences --------------------------------------
    /// Full Performance/Color/Interface/File-Handling preferences.
    pub preferences: self::prefs::PigmentPreferences,
    /// Override path for the preferences file (defaults when None).
    pub preferences_path: Option<std::path::PathBuf>,

    // ---- New Feature: Per-artboard export metadata --------------------------
    /// Per-artboard export configs keyed by artboard id.
    pub artboard_export_configs: std::collections::HashMap<u64, self::artboards::ArtboardExportConfig>,
    /// Last computed per-artboard export plan.
    pub last_artboard_export_plan: Vec<self::artboards::ArtboardExportItem>,
    // ---- New: Native PSD serializer ------------------------------------------
    /// Whether the native PSD writer uses RLE (true) or raw (false) channels.
    pub psd_export_rle: bool,
    /// Path of the last native PSD export (set on success).
    pub last_psd_native_path: Option<String>,

    // ---- New: Guides / rulers / smart guides ---------------------------------
    /// Canvas guides, ruler units, and smart-guide settings.
    pub guide_state: self::guides::GuideState,

    // ---- New: Color management -----------------------------------------------
    /// Working color mode + space metadata for the document.
    pub color_management: self::color_management::ColorManagement,

    // ---- New: Keyboard shortcuts remap ---------------------------------------
    /// Command→keychord bindings (Photoshop-like defaults).
    pub shortcut_map: self::shortcuts::ShortcutMap,

    // ---- New: Navigator + multi-doc tabs -------------------------------------
    /// Open document tabs and the active index.
    pub doc_tabs: self::navigator::DocTabs,
    /// Navigator proxy viewport (pan/zoom over the document).
    pub navigator: self::navigator::NavigatorView,
}

impl App {
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
        // Capture into the active recording (no-op unless recording / mid-replay).
        self.record_action(&action);
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
            | Action::RenameLayer { .. }
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

            // healing / clone / red-eye / content-aware patch (Phase 6 — healing.rs)
            Action::HealBrush { .. } | Action::CloneStampDab { .. }
            | Action::RemoveRedEye { .. } | Action::ContentAwarePatch { .. }
            => self.apply_healing(action),

            // liquify forward-warp mesh (Phase 6 — liquify.rs)
            Action::LiquifyPush { .. } | Action::LiquifyBloat { .. } | Action::LiquifyPucker { .. }
            | Action::LiquifyTwirl { .. } | Action::LiquifyReconstruct { .. }
            | Action::LiquifyCommit | Action::LiquifyReset
            => self.apply_liquify(action),

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

            // advanced filters (surface/path blur)
            Action::ApplySurfaceBlur { .. } | Action::ApplyPathBlur { .. }
            => self.apply_filters_advanced(action),
            // Phase 8 blur/sharpen/distort gallery (real pure pixel math)
            Action::ApplyGalleryBlur(_) | Action::ApplyDisplacementMap { .. } => self.apply_filters_blur(action),

            // transforms
            Action::ApplyCrop { .. } | Action::CancelCrop
            | Action::SetCaCropAngle(_) | Action::SetCaCropFillMethod(_)
            | Action::SetCaCropEnabled(_) | Action::ApplyCaCrop { .. }
            => self.apply_transforms(action),

            // free transform (rotation + skew, extends translate/scale)
            Action::SetTransformRotation(_) | Action::SetTransformSkew { .. }
            | Action::ApplyFreeTransform | Action::ResetFreeTransform
            => self.apply_transform_extra(action),

            // text
            Action::SetTextSize(_) | Action::SetTextContent(_) => self.apply_text(action),

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

            // welcome screen actions — dispatched from the welcome window buttons
            Action::NewDocument => {
                // Stub: mark host dirty so the canvas repaints with the current
                // (blank) document. Full new-document dialog is a later wave.
                self.host.mark_dirty();
            }
            Action::OpenFile => {
                // Delegate to the existing OpenImage picker flow.
                self.apply(Action::OpenImage);
            }

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

            // shapes (batch 8)
            Action::AddPolygonLayer { .. } | Action::AddStarLayer { .. } | Action::AddLineLayer { .. }
            | Action::AddRoundedRectLayer { .. } | Action::AddTriangleLayer { .. }
            | Action::SetShapeSides { .. } | Action::SetShapeCornerRadius { .. }
            | Action::SetStarPoints { .. } | Action::SetStarInnerRadius { .. }
            | Action::SetLineWidth { .. } | Action::SetLineCap { .. } | Action::SetLineJoin { .. }
            | Action::SetShapeStroke { .. } | Action::SetShapeFill { .. }
            | Action::BooleanShapeOp { .. } | Action::ExpandStroke { .. } | Action::FlattenToPixels { .. }
            | Action::SetClippingMask { .. } | Action::CreateClippingMask { .. } | Action::ReleaseClippingMask { .. }
            | Action::SetSatinEffect { .. } | Action::SetExtendedColorOverlay { .. }
            | Action::SetGradientOverlay { .. } | Action::SetPatternOverlay { .. }
            | Action::SetLayerStyleBlendMode { .. } | Action::SetLayerStyleOpacity { .. }
            | Action::CopyLayerStylesExt { .. } | Action::PasteLayerStylesExt { .. } | Action::ClearLayerStylesExt { .. }
            | Action::SetPsdExportPath(_) | Action::SetPsdMaximizeCompatibility(_) | Action::SetPsdEncoding(_)
            | Action::SetPsdEmbedColorProfile(_) | Action::ExportAsPsd { .. }
            => self.apply_shapes(action),

            // extended layer styles — destructive bake (satin / pattern overlay)
            Action::BakeSatinEffect { .. } | Action::BakePatternOverlay { .. }
            => self.apply_layer_styles_extra(action),

            // New Feature: rich (non-destructive) Smart Objects
            Action::EmbedSmartObject { .. } | Action::SetSmartObjectScale { .. }
            | Action::SetSmartObjectRotation { .. } | Action::SetSmartObjectSkew { .. }
            | Action::SetSmartObjectTranslate { .. } | Action::AddSmartObjectFilter { .. }
            | Action::ClearSmartObjectFilters { .. } | Action::ResetSmartObjectTransform { .. }
            | Action::UpdateSmartObject { .. } | Action::BakeSmartObject { .. }
            => self.apply_smart_objects_rich(action),

            // New Feature: actions / batch automation
            Action::StartRecording { .. } | Action::StopRecording
            | Action::PlayActionSet { .. } | Action::PlayActionSetByName { .. }
            | Action::DeleteActionSet { .. } | Action::ClearActionSet { .. }
            | Action::RenameActionSet { .. }
            => self.apply_automation(action),

            // New Feature: scripting sandbox
            Action::RunScript { .. } | Action::SetScriptSource(_)
            | Action::RunCurrentScript | Action::ClearScriptLog
            => self.apply_scripting(action),

            // New Feature: rich preferences
            Action::SetPrefMemoryFraction(_) | Action::SetPrefHistoryStates(_)
            | Action::SetPrefUseGpu(_) | Action::SetPrefWorkingRgb(_)
            | Action::SetPrefRenderingIntent(_) | Action::SetPrefTheme(_)
            | Action::SetPrefUiScale(_) | Action::SetPrefAutosaveMinutes(_)
            | Action::SetPrefRecentFileCount(_) | Action::ResetPreferences
            | Action::SavePreferences | Action::LoadPreferences
            => self.apply_prefs(action),

            // New Feature: per-artboard export metadata
            Action::SetArtboardExportFormat { .. } | Action::SetArtboardExportScale { .. }
            | Action::SetArtboardExportNaming { .. } | Action::SetArtboardExportQuality { .. }
            | Action::SetArtboardExportEnabled { .. } | Action::PrepareArtboardExport
            => self.apply_artboards_export(action),
            // native PSD serializer
            Action::SetPsdExportCompression(_) | Action::ExportPsdNative(_)
            => self.apply_psd_export(action),

            // guides / rulers / smart guides
            Action::AddCanvasGuide { .. } | Action::RemoveCanvasGuide(_)
            | Action::ClearCanvasGuides | Action::LockCanvasGuide { .. }
            | Action::MoveCanvasGuide { .. } | Action::SetGuideSnapEnabled(_)
            | Action::SetGuideSnapDistance(_) | Action::SetGuidesVisible(_)
            | Action::SetRulerUnit(_) | Action::SetSmartGuidesEnabled(_)
            => self.apply_guides(action),

            // color management
            Action::SetWorkingColorMode(_) | Action::AssignWorkingSpace(_)
            | Action::ConvertWorkingSpace { .. } | Action::SetEmbedColorProfile(_)
            => self.apply_color_management(action),

            // keyboard shortcuts remap
            Action::RemapShortcut { .. } | Action::UnbindShortcut(_)
            | Action::ResetShortcuts | Action::LoadShortcutsJson(_)
            | Action::SaveShortcutsJson(_)
            => self.apply_shortcuts(action),

            // navigator + multi-doc tabs
            Action::OpenDocTab { .. } | Action::CloseDocTab(_) | Action::ActivateDocTab(_)
            | Action::ActivateDocTabIndex(_) | Action::ReorderDocTab { .. }
            | Action::SetDocTabDirty { .. } | Action::NavigatorZoom(_)
            | Action::NavigatorPan { .. } | Action::NavigatorCenter { .. }
            => self.apply_navigator(action),

            _ => {}
        }
    }

}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}
