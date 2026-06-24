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

mod actions;
mod actions_undoable;
mod dispatch;
mod composition;
mod composition_layers;
mod keyframes;
mod effects_chain;
mod effects_apply;
mod render;
mod render_types;
mod tool;
mod tracking;
mod expressions;
mod precomp;
mod puppeting;
mod text_anim;
mod motion_paths;
mod shape_groups;
mod audio_mixer;
mod apply_batch5;
mod tests_batch5;
mod output_module;
mod preferences;
mod cache_manager;
mod masks;
mod effects_noise;
mod motion_blur;
mod time_stretch;
mod keying;
mod effects_distort;
mod lighting3d;
mod shape_repeater;
mod render_formats;
mod geom_util;
use geom_util::{point_in_quad, vec_remove};

pub use actions::Action;
pub use tool::Tool;

pub use expressions::{ExprControlKind, ExprControlValue, ExprControl};
pub use text_anim::{TextAnimPreset, TextAnimProperty, TextAnimRange, TextAnimator, MogrParamKind, MogrParam, MogrTemplate};
pub use tracking::{TrackPoint, CameraTracker, MotionSketchStroke, MotionSketchConfig, StabilizeResult, StabilizeMethod, StabilizeFraming, WarpStabConfig, CameraTrackStatus, CameraTrackPoint, CameraTrackSolve};
pub use puppeting::{MorphMode, CorrespondenceMode, ShapeMorphKeyframe, ShapeMorphConfig, PuppetPinMode, PuppetPin, PuppetMesh};
pub use render_types::{BrainstormVariation, BrainstormState, PreRenderStatus, AudioVisMode, AudioVisSide, AudioSpectrumConfig, RenderStatus, RenderOutputFormat, RenderQueueItem};
pub use precomp::{MatteMode, TrackMatteConfig, PrecompConfig, PrecompInfo};
pub use render_types::{RenderJobStatus, RenderFormat, RenderJob, OutputPreset, SubComp};
pub use composition::{PendingCompSettings, IrisShape, DepthOfField, CollectFilesConfig, Layer3DConfig};
pub use effects_chain::{
    MoGrtControl, MotionGraphicTemplate, EchoConfig,
    RadialBlurKind, SmartBlurMode, GlowColors, GlowChannel, TilingMode, CylinderRenderMode,
    ChannelSource, CalcOperation, CellPatternKind, OverflowMode, GradientEffectKind,
    StrokePath, StrokeComposite, Batch5Effect,
};
pub use tracking::RotobrushStroke;
pub use expressions::ExprLang;
pub use keyframes::{GizmoDrag, WorkAreaHandle, KeyframeDrag, PreviewRect, GraphGrab};
pub use crate::comp::Handle as GizmoHandle2;
pub use motion_paths::{MotionPath, MotionPathPoint, MotionEasing};
pub use shape_groups::{ShapeLayerGroup, ShapeGroupTransform, ShapeItemKind, MergeMode, TrimMultiple};
pub use audio_mixer::{AudioBus, MixerTrack, MasterBus, Mixdown, db_to_linear, linear_to_db, pan_law};
pub use expressions::{LoopMode, ExprTrack};
pub use tracking::{RotoMask, seed_color, segment_frame, propagate_mask};
pub use output_module::{OutputModule, OutputModuleFormat, OutputCodec, ColorDepth};
pub use preferences::{Preferences, GeneralPrefs, DisplayPrefs, MediaPrefs, PreviewPrefs, PreviewQuality};
pub use cache_manager::{DiskCacheManager, CacheEntry};
pub use effects_noise::{FractalNoiseConfig, TurbulentDisplaceConfig, FractalNoiseMap, TurbulentDisplaceMap};
pub use motion_blur::LayerMotionBlur;
pub use keying::{KeyKind, KeyConfig, KeyerMap};
pub use effects_distort::{
    CornerPinConfig, BezierWarpConfig, WaveWarpConfig, RoughenEdgesConfig,
    CornerPinMap, BezierWarpMap, WaveWarpMap, RoughenEdgesMap,
};
pub use lighting3d::{LightKind, Light3D, Material3D, Material3DMap};
pub use shape_repeater::{
    RepeaterConfig, CopyTransform, TrimKey, TrimPathsConfig, RepeaterMap, TrimPathsMap,
};
pub use render_formats::{RenderCodec, ProResProfile, RenderSpec};

const UNDO_LIMIT: usize = 64;

// ── Batch 2: Track Camera ─────────────────────────────────────────────────────

/// Shared cell holding the timeline track's painted bounds (window-relative).
///
/// The timeline panel paints a tiny `canvas` over its scrub track that records
/// the track's real laid-out [`Bounds`] here every frame; the track's mouse
/// listeners then read it to map a click/drag x-position back to a comp time.
/// Using a shared cell is how a panel (which only gets `&App`) can hand the root
/// view the geometry it needs without the root view re-deriving the flex layout.
pub type TrackBounds = Rc<Cell<Option<Bounds<Pixels>>>>;

// The editing-`Tool` enum lives in `tool.rs` (extracted for the workspace size
// rule); re-exported below so `crate::app_state::Tool` keeps resolving.

// Action enum is defined in actions.rs and re-exported above as `pub use actions::Action;`.

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
    /// The export / render output path typed into the render-queue panel's
    /// editable path field (set via [`Action::SetRenderOutputPath`]).
    pub render_output_path: PathBuf,

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

    // --- Welcome screen ---
    /// Whether the welcome screen overlay is visible (shown on first launch).
    pub welcome_visible: bool,

    // --- Batch 5 (New): More Built-in Effects ---
    pub batch5_effects: HashMap<usize, Vec<Batch5Effect>>,

    // --- Batch 5 (New): Motion Paths ---
    pub motion_paths: Vec<MotionPath>,
    pub next_motion_path_id: usize,

    // --- Batch 5 (New): Shape Layer Groups ---
    pub shape_groups: Vec<ShapeLayerGroup>,
    pub next_shape_group_id: usize,

    // --- Batch 5 (New): Audio Mixer ---
    pub audio_buses: Vec<AudioBus>,
    pub master_volume: f32,
    pub master_pan: f32,
    pub next_bus_id: usize,

    // --- AE feature pass: audio mixer expansion ---
    /// Per-track mixer strips (gain/pan/solo/mute/routing).
    pub mixer_tracks: Vec<MixerTrack>,
    /// The master output bus.
    pub master_bus: MasterBus,

    // --- AE feature pass: Output Modules (render-queue output config) ---
    pub output_modules: Vec<OutputModule>,

    // --- AE feature pass: Preferences ---
    pub preferences: Preferences,
    pub preferences_open: bool,
    pub last_prefs_save_result: Option<String>,

    // --- AE feature pass: Disk Cache Manager ---
    pub disk_cache: DiskCacheManager,

    // --- AE feature pass: Fractal Noise / Turbulent Displace (effects_noise.rs) ---
    /// Per-layer Fractal Noise generator configs (app-side; not in the engine
    /// effect stacks). Keyed by layer index.
    pub fractal_noise: FractalNoiseMap,
    /// Per-layer Turbulent Displace configs (app-side). Keyed by layer index.
    pub turbulent_displace: TurbulentDisplaceMap,

    // --- AE feature pass: Per-layer motion blur (motion_blur.rs) ---
    /// Per-layer motion-blur shutter overrides (app-side). Keyed by layer index;
    /// a layer with no entry inherits the comp shutter.
    pub layer_motion_blur: HashMap<usize, LayerMotionBlur>,

    // --- Distortion / keying / lighting / shape / render passes (app-side) ---
    /// Per-layer keyers (chroma / color / luma + spill). Keyed by layer index.
    pub keyers: KeyerMap,
    /// Per-layer Corner Pin configs.
    pub corner_pins: CornerPinMap,
    /// Per-layer Bezier Warp configs.
    pub bezier_warps: BezierWarpMap,
    /// Per-layer Wave Warp configs.
    pub wave_warps: WaveWarpMap,
    /// Per-layer Roughen Edges configs.
    pub roughen_edges: RoughenEdgesMap,
    /// Comp 3D lights (point / spot / ambient).
    pub lights3d: Vec<Light3D>,
    /// Per-layer 3D materials (Blinn-Phong coefficients). Keyed by layer index.
    pub materials3d: Material3DMap,
    /// Per-layer shape Repeater configs.
    pub repeaters: RepeaterMap,
    /// Per-layer Trim Paths configs (with optional animation keys).
    pub trim_paths: TrimPathsMap,
    /// The current render-output spec (codec / fps / quality).
    pub render_spec: RenderSpec,

    // --- Wave 4 panel UI: visibility toggles for the new floating panels ---
    /// Output Module dialog (render-output config) open.
    pub output_module_open: bool,
    /// Expression editor pop-out open.
    pub expr_editor_open: bool,
    /// The property name the expression editor is bound to on the selected layer.
    pub expr_editor_prop: String,
    /// Keying + Lights inspector section open.
    pub keylight_open: bool,
}

/// Shared cell holding the preview image's painted bounds (window-relative), so
/// the root view's preview mouse listeners can map a pointer position into comp
/// space for the transform gizmo. Mirrors [`TrackBounds`] for the timeline.

impl App {
    /// Build the shared state: a fresh demo project (matching the egui app) and a
    /// host primed to render its first frame.
    pub fn new() -> Self {
        let project = Project::new();
        let host = CanvasHost::new(&project);
        // Seed the comp-settings staging buffer from the active comp so the
        // panel's typeable width/height/fps/duration fields have initial values
        // and the panel renders immediately when opened.
        let pending_comp_settings = project.comps.first().map(|c| PendingCompSettings {
            width: c.width,
            height: c.height,
            fps: c.fps,
            duration_secs: c.duration,
            bg_color: [0.0, 0.0, 0.0, 1.0],
        });
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
            pending_comp_settings,
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
            render_output_path: PathBuf::from("output.mp4"),
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
            welcome_visible: true,
            batch5_effects: HashMap::new(),
            motion_paths: Vec::new(),
            next_motion_path_id: 0,
            shape_groups: Vec::new(),
            next_shape_group_id: 0,
            audio_buses: Vec::new(),
            master_volume: 1.0,
            master_pan: 0.0,
            next_bus_id: 0,
            mixer_tracks: Vec::new(),
            master_bus: MasterBus::default(),
            output_modules: Vec::new(),
            preferences: Preferences::default(),
            preferences_open: false,
            last_prefs_save_result: None,
            disk_cache: DiskCacheManager::default(),
            fractal_noise: HashMap::new(),
            turbulent_displace: HashMap::new(),
            layer_motion_blur: HashMap::new(),
            keyers: HashMap::new(),
            corner_pins: HashMap::new(),
            bezier_warps: HashMap::new(),
            wave_warps: HashMap::new(),
            roughen_edges: HashMap::new(),
            lights3d: Vec::new(),
            materials3d: HashMap::new(),
            repeaters: HashMap::new(),
            trim_paths: HashMap::new(),
            render_spec: RenderSpec::default(),
            output_module_open: false,
            expr_editor_open: false,
            expr_editor_prop: "X".to_string(),
            keylight_open: false,
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
        self.dispatch(action);
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

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

