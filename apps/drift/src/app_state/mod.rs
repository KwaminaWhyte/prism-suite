//! The single shared application state for the Drift GPUI host.
//!
//! `App` owns everything panels read or mutate: the document, layers, keyframes,
//! transforms, puppet rigs, AI feature state, export config, state machines,
//! vector paths, symbols, symbol instances, tweens, onion skin config, scenes,
//! frame labels, library, grid/ruler config, IK chains, spring dynamics,
//! mesh warps, bone weights, easing curves, audio tracks, lip-sync data,
//! and Character Animator behaviors.
//! Character Animator behaviors, 3D layer transforms, camera viewport,
//! motion-capture imports, publishing config, and stage settings.
//! Panels NEVER mutate `App` fields directly — they emit an [`Action`], and the
//! root view routes it through [`App::apply`], the single mutation choke point.

use std::collections::HashMap;

// Domain modules
pub mod document;
pub mod layers;
pub mod keyframes;
pub mod transforms;
pub mod playback;
pub mod rig;
pub mod ai;
pub mod export;
pub mod state_machine;
pub mod vector;
pub mod symbols;
pub mod tweening;
pub mod scenes;
pub mod frame_labels;
pub mod library;
pub mod swap_sets;
pub mod mesh_warp;
pub mod bone_weights;
pub mod easing;
pub mod audio_sync;
pub mod behaviors;
// New — batch 6 domains
pub mod layer_3d;
pub mod camera;
pub mod mocap;
pub mod publish;
pub mod stage;
// Batch 7 domains
pub mod scripting;
pub mod facial_capture;
pub mod ai_motion;
pub mod advanced_tweening;
pub mod beat_sync;
// Batch 8 domains
pub mod deformation;
pub mod masking;
pub mod text_layer;
pub mod lottie;
pub mod blend_modes_layer;
pub mod plugin_api;
// History + document lifecycle
pub mod history;
pub mod state_machine_eval;
pub mod export_presets;
pub mod ai_motion_ext;
pub mod drawing_tools;
pub mod nested_timeline;
// Batch 8: ONNX inference stubs
pub mod onnx_inference;
// Batch 8: Lottie builder, media export queue, JS runtime, web publish, CRDT
pub mod export_web;
pub mod audio_ops;
// App constructor
pub mod init;

// Re-exports so callers can use `app_state::{App, Action, ...}` directly.
pub use document::{DriftDocument, GridConfig, RulerConfig, RulerUnit};
pub use layers::{DriftLayer, LayerKind};
pub use keyframes::{EasingKind, Keyframe};
pub use transforms::LayerTransform;
pub use rig::{RigBone, LayerRig, IkChain, BoneSpring};
pub use export::{ExportFormat, ExportConfig};
pub use state_machine::{StateTransitionTrigger, AnimationState, StateTransition, StateMachine};
pub use vector::{VectorPath, BezierPoint, Fill, GradientStop, Stroke, StrokeCap, StrokeJoin};
pub use symbols::{SymbolKind, Symbol, SymbolInstance};
pub use tweening::{TweenKind, Tween, RotateDirection, OnionSkinConfig};
pub use scenes::Scene;
pub use frame_labels::FrameLabel;
pub use library::LibraryFolder;
pub use swap_sets::{SwapSet, SwapSetItem};
pub use mesh_warp::{MeshWarp, WarpPoint};
pub use bone_weights::{BoneInfluence, LayerBoneWeights};
pub use easing::{EasingCurve, EasingCurveKind, StepPosition};
pub use audio_sync::{DriftAudioTrack, LipSyncData, PhonemeFrame, Phoneme};
pub use behaviors::{Behavior, BehaviorKind};
pub use layer_3d::{Layer3DTransform, Projection3D};
pub use camera::{DriftCamera, CameraKeyframe};
pub use mocap::{MocapFormat, MocapImport, MocapBoneMapping};
pub use publish::{PublishTarget, PublishConfig, VideoCodecKind, SpriteFormat, ExportStatus, ExportJob, ExportQueue};
pub use stage::{StageSettings, StageRulerUnit, LabelColor, SceneProperties};
pub use scripting::{Script, ScriptLanguage, ScriptTarget, ScriptEvent, LogLevel, ScriptLogEntry, ScriptConsole};
pub use facial_capture::{CaptureSource, TrackedFeature, FacialCaptureSession, CaptureMapping};
pub use ai_motion::{AiMotionModel, AiRequestStatus, AiMotionRequest, AiInterpolationRequest, AiStyleTransfer, AiBackend};
pub use advanced_tweening::{AdvancedTweenKind, MotionGuide, PropertyTween};
pub use beat_sync::{MarkerKind, AudioMarker, BeatSyncConfig, SyncGroup};
pub use deformation::{DeformKind, PinAnchor, DeformLayer, StretchSquash};
pub use masking::{MaskKind, LayerMask, ClippingGroup};
pub use text_layer::{TextAlign, TextStyle, DriftTextLayer, SvgImportStatus, SvgImportJob};
pub use lottie::{LottieImportStatus, LottieKf, LottiePropValue, LottieLayerKs, LottieLayerDef, LottieAnim, LottieExportConfig, LottieImportJob};
pub use blend_modes_layer::{LayerBlend, LayerBlendConfig};
pub use plugin_api::{PluginKind, PluginStatus, PluginManifest, LoadedPlugin, ExtensionPanel};
pub use drawing_tools::{PenMode, PenToolState, PenBezierPoint, PencilStroke, GizmoHandle, SelectionGizmo};
pub use nested_timeline::{NestedTimeline, NestedLayer, NestedKeyframe};
pub use onnx_inference::{
    OnnxJobStatus, DriftOnnxModel, OnnxModelStatus, DriftOnnxModelEntry,
    AnimateDiffJob, FilmRifeJob, PhonemeDetectJob, StyleTransferJob, AiBgGenJob, AiScriptJob,
};
pub use export_web::{
    LottieJsonBuilder, WebLottieImportSession, WebLottieImportStatus,
    MediaExportFormat, MediaExportStatus, MediaExportJob,
    JsRuntimeConfig, WebPublishJob, WebPublishStatus,
    WsLivePreviewConfig, CollabSession, CollabStatus,
};
pub use audio_ops::{AudioClipOps, WaveformPeakEntry, MultiTrackMixConfig, RhaiRuntimeConfig, As3ImportStatus, As3ImportJob, AudioSyncMarker};

// Tool enum and Action enum live in their own file.
pub mod action;
pub use action::{DriftTool, Action};
pub use history::{AppHistory, HistoryEntry};
pub use state_machine_eval::SmRuntime;
pub use export_presets::{ExportPreset, BatchJobStatus, BatchExportJob, RenderQueueBatch};
pub use ai_motion_ext::{AiExtJobStatus, MotionSmoothJob, EaseSuggestion, EaseSuggestJob, InbetweenJob, ExpressionTransferJob, MotionFromVideoJob, DftExportStatus, CharacterPackExport};
// ── App ───────────────────────────────────────────────────────────────────────

/// Shared application state. All mutation goes through [`App::apply`].
pub struct App {
    // Active tool
    pub active_tool: DriftTool,

    // Document
    pub document: DriftDocument,

    // Grid and ruler overlays
    pub grid: GridConfig,
    pub rulers: RulerConfig,

    // Layers
    pub layers: Vec<DriftLayer>,
    pub layer_counter: usize,
    pub active_layer: Option<usize>,

    // Keyframes
    pub keyframes: Vec<Keyframe>,
    pub keyframe_counter: usize,

    // Transforms  (layer_id → transform)
    pub transforms: HashMap<usize, LayerTransform>,

    // Playback
    pub current_frame: usize,
    pub playing: bool,
    pub loop_playback: bool,
    pub in_point: usize,
    pub out_point: usize,

    // Puppet rigs
    pub layer_rigs: Vec<LayerRig>,
    pub rig_bone_counter: usize,

    // IK chains
    pub ik_chains: Vec<IkChain>,
    pub next_chain_id: usize,

    // Spring dynamics
    pub bone_springs: Vec<BoneSpring>,

    // AI
    pub ai_motion_prompt: String,
    pub ai_motion_results: Vec<(usize, String)>,
    pub ai_lipsync_jobs: Vec<(usize, String)>,
    pub ai_interpolation_queue: Vec<(usize, usize, usize)>,

    // Export (legacy single-job)
    pub export_config: ExportConfig,
    pub export_in_progress: bool,

    // State machines
    pub state_machines: Vec<StateMachine>,
    pub state_machine_counter: usize,
    pub anim_state_counter: usize,

    // Phase 2: Vector paths
    pub vector_paths: Vec<VectorPath>,
    pub next_path_id: usize,

    // Phase 2: Symbols
    pub symbols: Vec<Symbol>,
    pub symbol_instances: Vec<SymbolInstance>,
    pub next_symbol_id: usize,
    pub next_instance_id: usize,

    // Phase 2: Tweens
    pub tweens: Vec<Tween>,
    pub next_tween_id: usize,

    // Phase 2: Onion skinning
    pub onion_skin: OnionSkinConfig,

    // Phase 2: Scenes
    pub scenes: Vec<Scene>,
    pub active_scene_id: usize,
    pub next_scene_id: usize,

    // Phase 2: Frame labels
    pub frame_labels: Vec<FrameLabel>,
    pub next_label_id: usize,

    // Phase 2: Library panel
    pub library_folders: Vec<LibraryFolder>,
    pub library_search: String,
    pub next_folder_id: usize,
    /// (symbol_id, folder_id) — None folder_id means root.
    pub symbol_folder_map: Vec<(usize, Option<usize>)>,

    // Phase 3: Swap sets
    pub swap_sets: Vec<SwapSet>,
    pub next_swap_set_id: usize,
    pub next_swap_item_id: usize,

    // Phase 3: Mesh Warp  (layer_id → warp)
    pub mesh_warps: HashMap<usize, MeshWarp>,
    pub next_warp_id: usize,

    // Phase 3: Bone Influence Weights  (layer_id → weights)
    pub bone_weights: HashMap<usize, LayerBoneWeights>,
    pub weight_painting_active: bool,
    pub active_weight_bone: Option<usize>,
    pub weight_brush_radius: f32,

    // Phase 3: Easing Curves Library
    pub easing_curves: Vec<EasingCurve>,
    pub next_easing_id: usize,
    /// Maps keyframe_id → easing_curve_id for per-keyframe curve overrides.
    pub keyframe_easing_map: HashMap<usize, usize>,

    // Phase 3: Audio Tracks + Lip Sync
    pub audio_tracks: Vec<DriftAudioTrack>,
    pub next_audio_track_id: usize,
    pub lip_sync_data: Vec<LipSyncData>,

    // Phase 3: Character Animator Behaviors
    pub behaviors: Vec<Behavior>,
    pub next_behavior_id: usize,

    // Batch 6 — A: 3D Layer Transforms
    /// Per-layer 3D transform data.  layer_id → Layer3DTransform.
    pub layer_3d: HashMap<usize, Layer3DTransform>,
    /// Global vanishing point (fraction of stage). Default (0.5, 0.5).
    pub global_vanishing_point: (f32, f32),
    /// Global projection mode applied to all 3D layers.
    pub global_projection_3d: Projection3D,

    // Batch 6 — B: Camera / Viewport
    pub camera: DriftCamera,
    pub camera_keyframes: Vec<CameraKeyframe>,
    pub next_camera_kf_id: usize,
    pub camera_enabled: bool,

    // Batch 6 — C: Motion Capture
    pub mocap_imports: Vec<MocapImport>,
    pub next_mocap_id: usize,

    // Batch 6 — D: Publishing / Advanced Export
    pub publish_config: PublishConfig,
    pub export_queue: ExportQueue,

    // Batch 6 — E: Stage Settings
    pub stage: StageSettings,
    /// Extended per-scene properties.  scene_id → SceneProperties.
    pub scene_properties: HashMap<usize, SceneProperties>,
    // Batch 7 — A: Scripting
    pub scripts: Vec<Script>,
    pub next_script_id: usize,
    pub script_console: ScriptConsole,
    pub script_execution_enabled: bool,

    // Batch 7 — B: Facial Capture
    pub capture_sessions: Vec<FacialCaptureSession>,
    pub next_capture_id: usize,
    pub capture_mappings: Vec<CaptureMapping>,
    pub live_preview_enabled: bool,

    // Batch 7 — C: AI Motion
    pub ai_motion_requests: Vec<AiMotionRequest>,
    pub next_ai_motion_id: usize,
    pub ai_interpolation_requests: Vec<AiInterpolationRequest>,
    pub next_interp_id: usize,
    pub ai_style_transfers: Vec<AiStyleTransfer>,
    pub next_style_id: usize,
    pub ai_model_path: Option<String>,
    pub ai_compute_backend: AiBackend,

    // Batch 7 — D: Advanced Tweening
    pub motion_guides: Vec<MotionGuide>,
    pub next_guide_id: usize,
    pub property_tweens: Vec<PropertyTween>,
    pub next_prop_tween_id: usize,

    // Batch 7 — E: Beat Sync
    pub audio_markers: Vec<AudioMarker>,
    pub next_marker_id: usize,
    pub beat_sync: BeatSyncConfig,
    pub sync_groups: Vec<SyncGroup>,
    pub next_sync_group_id: usize,

    // Batch 8 — Deformation
    pub pin_anchors: Vec<PinAnchor>,
    pub deform_layers: Vec<DeformLayer>,
    pub stretch_squash: Vec<StretchSquash>,
    pub next_anchor_id: usize,
    pub next_deform_id: usize,

    // Batch 8 — Masking
    pub layer_masks: Vec<LayerMask>,
    pub clipping_groups: Vec<ClippingGroup>,
    pub next_mask_id: usize,
    pub next_clip_group_id: usize,

    // Batch 8 — Text Layers / SVG Import
    pub text_layers: Vec<DriftTextLayer>,
    pub svg_import_jobs: Vec<SvgImportJob>,
    pub next_text_layer_id: usize,
    pub next_svg_job_id: usize,
    // Batch 8 — Lottie
    pub lottie_export_config: Option<LottieExportConfig>,
    pub lottie_exporting: bool,
    pub lottie_import_jobs: Vec<LottieImportJob>,
    pub next_lottie_job_id: usize,

    // Batch 8 — Layer Blend Modes
    pub layer_blend_configs: Vec<LayerBlendConfig>,

    // Batch 8 — Plugin API
    pub loaded_plugins: Vec<LoadedPlugin>,
    pub extension_panels: Vec<ExtensionPanel>,
    pub next_ext_panel_id: usize,
    // History + document lifecycle
    pub history: AppHistory,
    pub document_path: Option<String>,
    pub document_dirty: bool,
    // Batch 8 — State machine runtime
    pub sm_runtimes: Vec<SmRuntime>,

    // Batch 8 — Export presets + render queue
    pub export_presets: Vec<ExportPreset>,
    pub next_preset_id: usize,
    pub render_batches: Vec<RenderQueueBatch>,
    pub next_batch_id: usize,
    pub next_batch_job_id: usize,
    // Batch 8: AI Motion Extensions
    pub motion_smooth_jobs: Vec<MotionSmoothJob>,
    pub next_ms_job_id: usize,
    pub ease_suggest_jobs: Vec<EaseSuggestJob>,
    pub next_ease_job_id: usize,
    pub inbetween_jobs: Vec<InbetweenJob>,
    pub next_inbetween_id: usize,
    pub expr_transfer_jobs: Vec<ExpressionTransferJob>,
    pub next_expr_transfer_id: usize,
    pub mfv_jobs: Vec<MotionFromVideoJob>,
    pub next_mfv_id: usize,
    pub char_pack_exports: Vec<CharacterPackExport>,
    pub next_char_pack_id: usize,
    // Batch 8 — Drawing Tools
    pub pen_tool: PenToolState,
    pub pencil_strokes: Vec<PencilStroke>,
    pub next_pencil_id: usize,
    pub active_pencil_stroke: Option<usize>,
    pub selection_gizmo: Option<SelectionGizmo>,

    // Batch 8 — Nested Timelines
    pub nested_timelines: Vec<NestedTimeline>,
    pub next_nested_timeline_id: usize,
    // Batch 8: ONNX inference stubs
    pub drift_onnx_models: Vec<DriftOnnxModelEntry>,
    pub animatediff_jobs: Vec<AnimateDiffJob>,
    pub next_animatediff_id: usize,
    pub filmrife_jobs: Vec<FilmRifeJob>,
    pub next_filmrife_id: usize,
    pub phoneme_jobs: Vec<PhonemeDetectJob>,
    pub next_phoneme_job_id: usize,
    pub style_jobs: Vec<StyleTransferJob>,
    pub next_style_job_id: usize,
    pub bg_gen_jobs: Vec<AiBgGenJob>,
    pub next_bg_gen_id: usize,
    pub ai_script_jobs: Vec<AiScriptJob>,
    pub next_ai_script_id: usize,
    // Batch 8: Lottie JSON builder
    pub lottie_builds: Vec<LottieJsonBuilder>,
    pub next_lottie_build_id: usize,

    // Batch 8: Lottie import sessions
    pub lottie_imports: Vec<WebLottieImportSession>,
    pub next_lottie_import_id: usize,

    // Batch 8: Media export queue
    pub media_export_jobs: Vec<MediaExportJob>,
    pub next_media_export_id: usize,

    // Batch 8: JS runtime bridge
    pub js_runtime: JsRuntimeConfig,

    // Batch 8: Web publish jobs
    pub web_publish_jobs: Vec<WebPublishJob>,
    pub next_web_publish_id: usize,

    // Batch 8: WebSocket live preview
    pub ws_live_preview: WsLivePreviewConfig,

    // Batch 8: CRDT collaboration session
    pub collab_session: CollabSession,
    // Batch 8 — A/B: Audio clip ops + waveform peaks
    pub audio_clip_ops: Vec<AudioClipOps>,
    pub waveform_peaks: Vec<WaveformPeakEntry>,
    // Batch 8 — C: Multi-track mix config
    pub mix_config: MultiTrackMixConfig,
    // Batch 8 — D: Rhai runtime config
    pub rhai_runtime: RhaiRuntimeConfig,
    // Batch 8 — E: Audio sync markers (frame-based)
    pub audio_sync_markers: Vec<AudioSyncMarker>,
    pub next_sync_marker_id: usize,
    // Batch 8 — F: AS3 importer jobs
    pub as3_jobs: Vec<As3ImportJob>,
    pub next_as3_job_id: usize,
}

impl App {
    /// The single mutation choke point. Every panel and keyboard handler routes
    /// through here so state changes are predictable and testable.
    pub fn apply(&mut self, action: Action) {
        match &action {
            // Document + grid + rulers
            Action::SetDocumentWidth(_)
            | Action::SetDocumentHeight(_)
            | Action::SetDocumentFps(_)
            | Action::SetDocumentDuration(_)
            | Action::SetDocumentBg(_)
            | Action::SetDocumentName(_)
            | Action::SetGridEnabled(_)
            | Action::SetGridSnap(_)
            | Action::SetGridSize(_)
            | Action::SetGridColor(_)
            | Action::SetGridSubdivisions(_)
            | Action::ToggleRulers
            | Action::SetRulerOrigin { .. }
            | Action::SetRulerUnit(_) => self.apply_document(action),

            // Layers
            Action::AddLayer { .. }
            | Action::DeleteLayer(_)
            | Action::RenameLayer { .. }
            | Action::SetLayerVisible { .. }
            | Action::SetLayerLocked { .. }
            | Action::SetLayerSolo { .. }
            | Action::SetLayerParent { .. }
            | Action::ReorderLayers(_)
            | Action::SetLayerColorTag { .. }
            | Action::DuplicateLayer(_)
            | Action::SetLayerStartFrame { .. }
            | Action::SetLayerEndFrame { .. }
            | Action::SetActiveLayer(_) => self.apply_layers(action),

            // Keyframes
            Action::AddKeyframe { .. }
            | Action::DeleteKeyframe(_)
            | Action::MoveKeyframe { .. }
            | Action::SetKeyframeValue { .. }
            | Action::SetKeyframeEasing { .. }
            | Action::SetPropertyAtFrame { .. } => self.apply_keyframes(action),

            // Transforms
            Action::SetLayerPosition { .. }
            | Action::SetLayerScale { .. }
            | Action::SetLayerRotation { .. }
            | Action::SetLayerOpacity { .. }
            | Action::SetLayerAnchor { .. }
            | Action::ResetLayerTransform(_) => self.apply_transforms(action),

            // Playback
            Action::Play
            | Action::Pause
            | Action::Stop
            | Action::SetCurrentFrame(_)
            | Action::ToggleLoop
            | Action::SetInPoint(_)
            | Action::SetOutPoint(_)
            | Action::StepForward
            | Action::StepBackward
            | Action::GoToFirstFrame
            | Action::GoToLastFrame => self.apply_playback(action),

            // Rig (bones + IK chains + springs)
            Action::AddBone { .. }
            | Action::DeleteBone { .. }
            | Action::MoveBone { .. }
            | Action::RotateBone { .. }
            | Action::SetIKTarget { .. }
            | Action::AutoRigLayer(_)
            | Action::AddIkChain { .. }
            | Action::RemoveIkChain { .. }
            | Action::SetIkTarget { .. }
            | Action::SolveIk { .. }
            | Action::AddBoneSpring { .. }
            | Action::RemoveBoneSpring { .. }
            | Action::SetSpringParams { .. }
            | Action::TickSprings { .. } => self.apply_rig(action),

            // AI
            Action::SetAiMotionPrompt(_)
            | Action::GenerateAiMotion { .. }
            | Action::StartAiLipSync { .. }
            | Action::CompleteAiLipSync { .. }
            | Action::RequestAiInterpolation { .. }
            | Action::CompleteAiInterpolation { .. }
            | Action::AutoRigWithAi(_) => self.apply_ai(action),

            // Export (legacy)
            Action::SetExportFormat(_)
            | Action::SetExportFps(_)
            | Action::SetExportScale(_)
            | Action::SetExportQuality(_)
            | Action::SetExportTransparent(_)
            | Action::SetExportStartFrame(_)
            | Action::SetExportEndFrame(_)
            | Action::SetExportPath(_)
            | Action::StartExport
            | Action::CancelExport => self.apply_export(action),

            // State machine
            Action::CreateStateMachine(_)
            | Action::AddAnimationState { .. }
            | Action::DeleteAnimationState { .. }
            | Action::AddStateTransition { .. }
            | Action::DeleteStateTransition { .. }
            | Action::SetInitialState { .. }
            | Action::SetStateLoop { .. } => self.apply_state_machine(action),

            // Vector (Phase 2)
            Action::AddVectorPath { .. }
            | Action::RemoveVectorPath { .. }
            | Action::UpdatePathFill { .. }
            | Action::UpdatePathStroke { .. }
            | Action::AddBezierPoint { .. }
            | Action::ClosePath { .. }
            | Action::AddRectangle { .. }
            | Action::AddEllipse { .. }
            | Action::AddLine { .. } => self.apply_vector(action),

            // Symbols (Phase 2)
            Action::CreateSymbol { .. }
            | Action::DeleteSymbol { .. }
            | Action::PlaceSymbolInstance { .. }
            | Action::RemoveSymbolInstance { .. }
            | Action::SetInstanceTransform { .. } => self.apply_symbols(action),

            // Tweening (Phase 2)
            Action::CreateTween { .. }
            | Action::RemoveTween { .. }
            | Action::SetTweenEasing { .. }
            | Action::SetTweenRotation { .. }
            | Action::SetOnionSkin { .. } => self.apply_tweening(action),

            // Scenes (Phase 2)
            Action::AddScene { .. }
            | Action::RemoveScene { .. }
            | Action::RenameScene { .. }
            | Action::SetActiveScene { .. }
            | Action::SetSceneDuration { .. }
            | Action::DuplicateScene { .. }
            | Action::MoveScene { .. }
            | Action::AddLayerToScene { .. }
            | Action::RemoveLayerFromScene { .. } => self.apply_scenes(action),

            // Frame labels (Phase 2)
            Action::AddFrameLabel { .. }
            | Action::SetFrameBlank { .. }
            | Action::SetFrameComment { .. }
            | Action::RemoveFrameLabel { .. }
            | Action::GoToLabel { .. } => self.apply_frame_labels(action),

            // Library (Phase 2)
            Action::CreateLibraryFolder { .. }
            | Action::RenameLibraryFolder { .. }
            | Action::DeleteLibraryFolder { .. }
            | Action::MoveSymbolToFolder { .. }
            | Action::SetLibrarySearch { .. } => self.apply_library(action),

            // Swap sets (Phase 3)
            Action::CreateSwapSet { .. }
            | Action::AddSwapItem { .. }
            | Action::RemoveSwapItem { .. }
            | Action::ActivateSwapItem { .. }
            | Action::DeleteSwapSet { .. } => self.apply_swap_sets(action),

            // Mesh Warp (Phase 3)
            Action::AddMeshWarp { .. }
            | Action::RemoveMeshWarp { .. }
            | Action::SetWarpPoint { .. }
            | Action::ResetWarpPoints { .. }
            | Action::SetMeshWarpEnabled { .. }
            | Action::SetMeshWarpGrid { .. } => self.apply_mesh_warp(action),

            // Bone Weights (Phase 3)
            Action::SetBoneWeight { .. }
            | Action::RemoveBoneWeight { .. }
            | Action::ClearBoneWeights { .. }
            | Action::NormalizeBoneWeights { .. }
            | Action::SetWeightPaintingActive(_)
            | Action::SetActiveWeightBone { .. }
            | Action::SetWeightBrushRadius(_) => self.apply_bone_weights(action),

            // Easing Curves (Phase 3)
            Action::AddEasingCurve { .. }
            | Action::RemoveEasingCurve { .. }
            | Action::RenameEasingCurve { .. }
            | Action::SetKeyframeEasingCurve { .. } => self.apply_easing(action),

            // Audio / Lip Sync (Phase 3)
            Action::AddAudioTrack { .. }
            | Action::RemoveAudioTrack { .. }
            | Action::SetAudioTrackPath { .. }
            | Action::SetAudioTrackOffset { .. }
            | Action::SetAudioTrackVolume { .. }
            | Action::MuteAudioTrack { .. }
            | Action::ToggleWaveformVisible { .. }
            | Action::SetLipSyncData { .. }
            | Action::ClearLipSyncData { .. } => self.apply_audio_sync(action),

            // Behaviors (Phase 3)
            Action::AddBehavior { .. }
            | Action::RemoveBehavior { .. }
            | Action::ToggleBehavior { .. }
            | Action::SetBehaviorPriority { .. }
            | Action::RenameBehavior { .. }
            | Action::UpdateBehaviorKind { .. } => self.apply_behaviors(action),

            // 3D Layer Transforms (batch 6 — A)
            Action::SetLayer3DRotationX { .. }
            | Action::SetLayer3DRotationY { .. }
            | Action::SetLayerZPosition { .. }
            | Action::SetVanishingPoint { .. }
            | Action::SetProjection3D(_)
            | Action::Reset3DTransform { .. }
            | Action::Enable3DLayer { .. } => self.apply_layer_3d(action),

            // Camera (batch 6 — B)
            Action::SetCameraPosition { .. }
            | Action::SetCameraZoom(_)
            | Action::SetCameraRotation(_)
            | Action::ResetCamera
            | Action::ToggleCameraEnabled
            | Action::AddCameraKeyframe { .. }
            | Action::RemoveCameraKeyframe { .. }
            | Action::SetCameraAnimatable(_) => self.apply_camera(action),

            // Motion Capture (batch 6 — C)
            Action::ImportMocap { .. }
            | Action::RemoveMocap { .. }
            | Action::SetMocapBoneMapping { .. }
            | Action::SetMocapRetargetScale { .. }
            | Action::SetMocapApplyRig { .. }
            | Action::BakeMocapToKeyframes { .. } => self.apply_mocap(action),

            // Publishing / Export Queue (batch 6 — D)
            Action::SetPublishTarget(_)
            | Action::SetPublishOutputPath(_)
            | Action::SetPublishDimensions { .. }
            | Action::SetPublishFrameRate(_)
            | Action::SetPublishQuality(_)
            | Action::SetPublishLoop(_)
            | Action::SetPublishTransparentBg(_)
            | Action::AddToExportQueue { .. }
            | Action::RemoveFromExportQueue { .. }
            | Action::ClearExportQueue
            | Action::SetExportJobStatus { .. } => self.apply_publish(action),

            // Stage Settings (batch 6 — E)
            Action::SetStageDimensions { .. }
            | Action::SetStageFrameRate(_)
            | Action::SetStageBackgroundColor(_)
            | Action::SetStageRulerUnit(_)
            | Action::SetSnapToObjects(_)
            | Action::SetSnapToPixel(_)
            | Action::SetAutoSave { .. }
            | Action::SetUndoLevels(_)
            | Action::SetSceneLabel { .. }
            | Action::SetSceneDescription { .. }
            | Action::SetSceneFrameCount { .. } => self.apply_stage(action),
            // Scripting (Batch 7 — A)
            Action::AddScript { .. }
            | Action::RemoveScript { .. }
            | Action::RenameScript { .. }
            | Action::SetScriptSource { .. }
            | Action::SetScriptTarget { .. }
            | Action::SetScriptEvent { .. }
            | Action::ToggleScript { .. }
            | Action::SetScriptError { .. }
            | Action::ClearScriptError { .. }
            | Action::AppendScriptLog { .. }
            | Action::ClearScriptConsole
            | Action::SetScriptExecutionEnabled(_) => self.apply_scripting(action),

            // Facial Capture (Batch 7 — B)
            Action::AddCaptureSession { .. }
            | Action::RemoveCaptureSession { .. }
            | Action::SetCaptureTarget { .. }
            | Action::ToggleCaptureRecording { .. }
            | Action::SetCaptureActive { .. }
            | Action::AddCaptureMapping { .. }
            | Action::RemoveCaptureMapping { .. }
            | Action::SetCaptureMappingMultiplier { .. }
            | Action::SetCaptureMappingOffset { .. }
            | Action::ToggleLivePreview => self.apply_facial_capture(action),

            // AI Motion (Batch 7 — C)
            Action::RequestAiMotion { .. }
            | Action::UpdateAiMotionStatus { .. }
            | Action::CancelAiMotion { .. }
            | Action::RequestAiInterpolation2 { .. }
            | Action::UpdateAiInterpStatus { .. }
            | Action::RequestStyleTransfer { .. }
            | Action::UpdateStyleTransferStatus { .. }
            | Action::SetAiModelPath(_)
            | Action::SetAiComputeBackend(_) => self.apply_ai_motion(action),

            // Advanced Tweening (Batch 7 — D)
            Action::AddMotionGuide { .. }
            | Action::RemoveMotionGuide { .. }
            | Action::SetMotionGuideOrient { .. }
            | Action::SetMotionGuideSnap { .. }
            | Action::AddPropertyTween { .. }
            | Action::RemovePropertyTween { .. }
            | Action::UpdatePropertyTweenKind { .. }
            | Action::SetPropertyTweenRange { .. } => self.apply_advanced_tweening(action),

            // Beat Sync (Batch 7 — E)
            Action::AddAudioMarker { .. }
            | Action::RemoveAudioMarker { .. }
            | Action::MoveAudioMarker { .. }
            | Action::SetMarkerLabel { .. }
            | Action::SetBeatSyncEnabled(_)
            | Action::SetBeatSyncBpm(_)
            | Action::SetBeatSyncOffset(_)
            | Action::SetBeatSyncAudioTrack { .. }
            | Action::SetDetectedBpm(_)
            | Action::ToggleSyncSnapping
            | Action::AddSyncGroup { .. }
            | Action::RemoveSyncGroup { .. }
            | Action::SetSyncGroupLayers { .. } => self.apply_beat_sync(action),
            // Lottie (Batch 8)
            Action::StartLottieExport { .. }
            | Action::CompleteLottieExport
            | Action::CancelLottieExport
            | Action::StartLottieImport { .. }
            | Action::UpdateLottieImport { .. }
            | Action::CompleteLottieImport { .. }
            | Action::CancelLottieImport { .. } => self.apply_lottie(&action),

            // Blend Modes (Batch 8)
            Action::SetLayerBlend { .. }
            | Action::SetLayerFillOpacity { .. }
            | Action::ResetLayerBlend { .. } => self.apply_blend_modes(&action),

            // Plugin API (Batch 8)
            Action::RegisterPlugin { .. }
            | Action::EnablePlugin { .. }
            | Action::DisablePlugin { .. }
            | Action::UnloadPlugin { .. }
            | Action::OpenExtensionPanel { .. }
            | Action::CloseExtensionPanel { .. }
            | Action::ToggleExtensionPanel { .. } => self.apply_plugin_api(&action),

            // State machine eval (Batch 8)
            Action::InitSmRuntime { .. }
            | Action::TickSmRuntime { .. }
            | Action::TriggerSmInput { .. }
            | Action::StopSmRuntime { .. }
            | Action::ResetSmRuntime { .. } => self.apply_sm_eval(action),

            // Export presets / render queue (Batch 8)
            Action::SaveExportPreset { .. }
            | Action::DeleteExportPreset { .. }
            | Action::RenameExportPreset { .. }
            | Action::CreateRenderBatch
            | Action::AddJobToBatch { .. }
            | Action::StartRenderBatch { .. }
            | Action::UpdateBatchJobProgress { .. }
            | Action::CompleteBatchJob { .. }
            | Action::CancelRenderBatch { .. } => self.apply_export_presets(action),
            // AI Motion Extensions (Batch 8)
            Action::QueueMotionSmooth { .. }
            | Action::CompleteMotionSmooth { .. }
            | Action::CancelMotionSmooth { .. }
            | Action::QueueEaseSuggest { .. }
            | Action::CompleteEaseSuggest { .. }
            | Action::QueueInbetween { .. }
            | Action::CompleteInbetween { .. }
            | Action::CancelInbetween { .. }
            | Action::QueueExpressionTransfer { .. }
            | Action::CompleteExpressionTransfer { .. }
            | Action::QueueMotionFromVideo { .. }
            | Action::UpdateMotionFromVideoProgress { .. }
            | Action::CompleteMotionFromVideo { .. }
            | Action::StartCharPackExport { .. }
            | Action::CompleteCharPackExport { .. }
            | Action::CancelCharPackExport { .. } => self.apply_ai_motion_ext(&action),
            // Drawing Tools (Batch 8)
            Action::PenAddPoint { .. }
            | Action::PenSelectPoint { .. }
            | Action::PenMovePoint { .. }
            | Action::PenSetHandle { .. }
            | Action::PenClosePath
            | Action::PenCommitPath
            | Action::PenSetMode { .. }
            | Action::PencilBeginStroke { .. }
            | Action::PencilAddPoint { .. }
            | Action::PencilCommitStroke
            | Action::PencilCancelStroke
            | Action::SetOnionSkinEnabled { .. }
            | Action::SetOnionSkinFrames { .. }
            | Action::SetOnionSkinOpacity { .. }
            | Action::SetSelectionGizmo { .. }
            | Action::ClearSelectionGizmo
            | Action::GizmoDragHandle { .. }
            | Action::GizmoRelease
            | Action::GizmoRotate { .. } => self.apply_drawing_tools(action),

            // Nested Timelines (Batch 8)
            Action::CreateNestedTimeline { .. }
            | Action::AddNestedLayer { .. }
            | Action::AddNestedKeyframe { .. }
            | Action::SetNestedFrame { .. }
            | Action::SetNestedLoop { .. }
            | Action::DeleteNestedTimeline { .. } => self.apply_nested_timeline(action),
            // Batch 8: ONNX inference stubs
            Action::RegisterDriftModel { .. }
            | Action::QueueAnimateDiff { .. }
            | Action::CompleteAnimateDiff { .. }
            | Action::FailAnimateDiff { .. }
            | Action::QueueFilmRife { .. }
            | Action::CompleteFilmRife { .. }
            | Action::QueuePhonemeDetect { .. }
            | Action::CompletePhonemeDetect { .. }
            | Action::QueueStyleTransfer { .. }
            | Action::CompleteStyleTransfer { .. }
            | Action::QueueAiBgGen { .. }
            | Action::CompleteAiBgGen { .. }
            | Action::QueueAiScript { .. }
            | Action::CompleteAiScript { .. } => self.apply_onnx_inference(&action),
            // Batch 8: Lottie / export / web / collab
            Action::BuildLottieJson { .. }
            | Action::StartWebLottieImport { .. }
            | Action::CompleteWebLottieImport { .. }
            | Action::FailWebLottieImport { .. }
            | Action::QueueMediaExport { .. }
            | Action::StartMediaExport { .. }
            | Action::UpdateMediaExportProgress { .. }
            | Action::CompleteMediaExport { .. }
            | Action::FailMediaExport { .. }
            | Action::SetJsRuntime { .. }
            | Action::StartWebPublish { .. }
            | Action::CompleteWebPublish { .. }
            | Action::SetWsLivePreview { .. }
            | Action::SetWsClientCount { .. }
            | Action::StartCollabSession { .. }
            | Action::CollabSessionConnected { .. }
            | Action::StopCollabSession => self.apply_export_web(&action),
            // Audio Ops (Batch 8)
            Action::SetAudioClipTrim { .. }
            | Action::SetAudioFade { .. }
            | Action::SetAudioPan { .. }
            | Action::SetAudioClipVolume { .. }
            | Action::SetWaveformPeaks { .. }
            | Action::InvalidateWaveformPeaks { .. }
            | Action::SetMultiTrackMix { .. }
            | Action::SetRhaiRuntime { .. }
            | Action::AddAudioSyncMarker { .. }
            | Action::RemoveAudioSyncMarker { .. }
            | Action::StartAs3Import { .. }
            | Action::CompleteAs3Import { .. }
            | Action::FailAs3Import { .. } => self.apply_audio_ops(action),

            // Tool selection
            Action::SetActiveTool(t) => self.active_tool = *t,

            // Deformation (Batch 8)
            Action::AddPinAnchor { .. }
            | Action::RemovePinAnchor { .. }
            | Action::MovePinAnchor { .. }
            | Action::LockPinAnchor { .. }
            | Action::AddDeformLayer { .. }
            | Action::RemoveDeformLayer { .. }
            | Action::SetDeformStrength { .. }
            | Action::ToggleDeform { .. }
            | Action::SetStretchSquash { .. }
            | Action::ToggleStretchSquash { .. } => self.apply_deformation(action),

            // Masking (Batch 8)
            Action::AddLayerMask { .. }
            | Action::RemoveLayerMask { .. }
            | Action::SetMaskKind { .. }
            | Action::ToggleMask { .. }
            | Action::CreateClippingGroup { .. }
            | Action::AddToClippingGroup { .. }
            | Action::RemoveFromClippingGroup { .. }
            | Action::DissolveClippingGroup { .. } => self.apply_masking(action),

            // Text Layers / SVG Import (Batch 8)
            Action::AddTextLayer { .. }
            | Action::RemoveTextLayer { .. }
            | Action::SetTextContent { .. }
            | Action::SetTextFont { .. }
            | Action::SetTextColor { .. }
            | Action::SetTextAlign { .. }
            | Action::SetTextStyle { .. }
            | Action::SetTextSpacing { .. }
            | Action::QueueSvgImport { .. }
            | Action::CompleteSvgImport { .. } => self.apply_text_layer(action),
            // History + document lifecycle
            Action::Undo
            | Action::Redo
            | Action::ClearHistory
            | Action::NewDocument
            | Action::SaveDocument { .. }
            | Action::OpenDocument { .. }
            | Action::MarkDirty => self.apply_history(action),
            // History (stub — no-op until history stack is implemented)
            Action::Undo | Action::Redo => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{App, Action, LayerKind, EasingKind};

    fn app() -> App {
        App::new()
    }

    #[test]
    fn test_full_workflow_add_layer_keyframe_play() {
        let mut a = app();
        a.apply(Action::SetDocumentName("Demo".to_string()));
        a.apply(Action::SetDocumentFps(30.0));
        a.apply(Action::AddLayer { name: "Hero".to_string(), kind: LayerKind::Vector });
        let lid = a.layers[0].id;
        a.apply(Action::SetLayerPosition { id: lid, x: 100.0, y: 200.0 });
        a.apply(Action::AddKeyframe {
            layer_id: lid,
            property: "position_x".to_string(),
            frame: 0,
            value: 100.0,
            easing: EasingKind::Linear,
        });
        a.apply(Action::AddKeyframe {
            layer_id: lid,
            property: "position_x".to_string(),
            frame: 30,
            value: 500.0,
            easing: EasingKind::EaseOut,
        });
        a.apply(Action::Play);
        a.apply(Action::SetCurrentFrame(15));
        assert_eq!(a.document.name, "Demo");
        assert_eq!(a.document.fps, 30.0);
        assert!(a.playing);
        assert_eq!(a.current_frame, 15);
        assert_eq!(a.keyframes.len(), 2);
    }
}
