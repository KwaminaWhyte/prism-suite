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

// ── Tool enum ─────────────────────────────────────────────────────────────────

/// The currently active drawing/selection tool.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DriftTool {
    Select,
    Move,
    Pen,
    Rect,
    Ellipse,
    Lasso,
}

// ── Actions ───────────────────────────────────────────────────────────────────

/// Every mutation the UI can request. Apply via [`App::apply`].
#[derive(Debug)]
pub enum Action {
    // Document
    SetDocumentWidth(u32),
    SetDocumentHeight(u32),
    SetDocumentFps(f32),
    SetDocumentDuration(usize),
    SetDocumentBg(String),
    SetDocumentName(String),

    // Grid
    SetGridEnabled(bool),
    SetGridSnap(bool),
    SetGridSize(f32),
    SetGridColor(String),
    SetGridSubdivisions(u32),

    // Rulers
    ToggleRulers,
    SetRulerOrigin { x: f32, y: f32 },
    SetRulerUnit(RulerUnit),

    // Layers
    AddLayer { name: String, kind: LayerKind },
    DeleteLayer(usize),
    RenameLayer { id: usize, name: String },
    SetLayerVisible { id: usize, visible: bool },
    SetLayerLocked { id: usize, locked: bool },
    SetLayerSolo { id: usize, solo: bool },
    SetLayerParent { id: usize, parent_id: Option<usize> },
    ReorderLayers(Vec<usize>),
    SetLayerColorTag { id: usize, tag: String },
    DuplicateLayer(usize),
    SetLayerStartFrame { id: usize, frame: usize },
    SetLayerEndFrame { id: usize, frame: usize },
    SetActiveLayer(usize),

    // Keyframes
    AddKeyframe {
        layer_id: usize,
        property: String,
        frame: usize,
        value: f32,
        easing: EasingKind,
    },
    DeleteKeyframe(usize),
    MoveKeyframe { id: usize, frame: usize },
    SetKeyframeValue { id: usize, value: f32 },
    SetKeyframeEasing { id: usize, easing: EasingKind },
    SetPropertyAtFrame {
        layer_id: usize,
        property: String,
        frame: usize,
        value: f32,
    },

    // Transform
    SetLayerPosition { id: usize, x: f32, y: f32 },
    SetLayerScale { id: usize, x: f32, y: f32 },
    SetLayerRotation { id: usize, degrees: f32 },
    SetLayerOpacity { id: usize, opacity: f32 },
    SetLayerAnchor { id: usize, x: f32, y: f32 },
    ResetLayerTransform(usize),

    // Playback
    Play,
    Pause,
    Stop,
    SetCurrentFrame(usize),
    ToggleLoop,
    SetInPoint(usize),
    SetOutPoint(usize),
    StepForward,
    StepBackward,
    GoToFirstFrame,
    GoToLastFrame,

    // Puppet Rig
    AddBone {
        layer_id: usize,
        parent_id: Option<usize>,
        x: f32,
        y: f32,
        length: f32,
    },
    DeleteBone { layer_id: usize, bone_id: usize },
    MoveBone { layer_id: usize, bone_id: usize, x: f32, y: f32 },
    RotateBone { layer_id: usize, bone_id: usize, rotation: f32 },
    SetIKTarget { layer_id: usize, bone_id: usize, x: f32, y: f32 },
    AutoRigLayer(usize),

    // IK Chains (FABRIK)
    AddIkChain { layer_id: usize, bone_ids: Vec<usize>, target_x: f32, target_y: f32 },
    RemoveIkChain { chain_id: usize },
    SetIkTarget { chain_id: usize, target_x: f32, target_y: f32 },
    SolveIk { chain_id: usize },

    // Spring dynamics
    AddBoneSpring { bone_id: usize, stiffness: f32, damping: f32, mass: f32 },
    RemoveBoneSpring { bone_id: usize },
    SetSpringParams { bone_id: usize, stiffness: f32, damping: f32, mass: f32 },
    TickSprings { delta_t: f32 },

    // AI Features
    SetAiMotionPrompt(String),
    GenerateAiMotion { layer_id: usize },
    StartAiLipSync { layer_id: usize, audio_path: String },
    CompleteAiLipSync { layer_id: usize },
    RequestAiInterpolation { layer_id: usize, from_frame: usize, to_frame: usize },
    CompleteAiInterpolation { layer_id: usize },
    AutoRigWithAi(usize),

    // Export (legacy single-job)
    SetExportFormat(ExportFormat),
    SetExportFps(f32),
    SetExportScale(f32),
    SetExportQuality(u8),
    SetExportTransparent(bool),
    SetExportStartFrame(usize),
    SetExportEndFrame(usize),
    SetExportPath(String),
    StartExport,
    CancelExport,

    // State Machine
    CreateStateMachine(String),
    AddAnimationState {
        machine_id: usize,
        name: String,
        start_frame: usize,
        end_frame: usize,
    },
    DeleteAnimationState { machine_id: usize, state_id: usize },
    AddStateTransition {
        machine_id: usize,
        from: usize,
        to: usize,
        trigger: StateTransitionTrigger,
        duration: usize,
    },
    DeleteStateTransition { machine_id: usize, from: usize, to: usize },
    SetInitialState { machine_id: usize, state_id: usize },
    SetStateLoop { machine_id: usize, state_id: usize, looping: bool },

    // Vector drawing (Phase 2)
    AddVectorPath { layer_id: usize, points: Vec<BezierPoint>, fill: Fill, stroke: Stroke, closed: bool },
    RemoveVectorPath { path_id: usize },
    UpdatePathFill { path_id: usize, fill: Fill },
    UpdatePathStroke { path_id: usize, stroke: Stroke },
    AddBezierPoint { path_id: usize, point: BezierPoint },
    ClosePath { path_id: usize },
    AddRectangle { layer_id: usize, x: f32, y: f32, width: f32, height: f32, fill: Fill, stroke: Stroke },
    AddEllipse { layer_id: usize, cx: f32, cy: f32, rx: f32, ry: f32, fill: Fill, stroke: Stroke },
    AddLine { layer_id: usize, x1: f32, y1: f32, x2: f32, y2: f32, stroke: Stroke },

    // Symbols (Phase 2)
    CreateSymbol { name: String, kind: SymbolKind, width: f32, height: f32 },
    DeleteSymbol { symbol_id: usize },
    PlaceSymbolInstance { symbol_id: usize, layer_id: usize, x: f32, y: f32 },
    RemoveSymbolInstance { instance_id: usize },
    SetInstanceTransform { instance_id: usize, x: f32, y: f32, scale_x: f32, scale_y: f32, rotation: f32, alpha: f32 },

    // Tweening (Phase 2)
    CreateTween { layer_id: usize, kind: TweenKind, start_frame: usize, end_frame: usize },
    RemoveTween { tween_id: usize },
    SetTweenEasing { tween_id: usize, easing: EasingKind },
    SetTweenRotation { tween_id: usize, direction: RotateDirection, count: i32 },

    // Onion skinning (Phase 2)
    SetOnionSkin { enabled: bool, frames_before: usize, frames_after: usize },

    // Scenes (Phase 2)
    AddScene { name: String, duration_frames: usize },
    RemoveScene { scene_id: usize },
    RenameScene { scene_id: usize, name: String },
    SetActiveScene { scene_id: usize },
    SetSceneDuration { scene_id: usize, frames: usize },
    DuplicateScene { scene_id: usize },
    MoveScene { scene_id: usize, new_index: usize },
    AddLayerToScene { scene_id: usize, layer_id: usize },
    RemoveLayerFromScene { scene_id: usize, layer_id: usize },

    // Frame labels (Phase 2)
    AddFrameLabel { layer_id: usize, frame: usize, label: String },
    SetFrameBlank { layer_id: usize, frame: usize, blank: bool },
    SetFrameComment { label_id: usize, comment: String },
    RemoveFrameLabel { label_id: usize },
    GoToLabel { label: String },

    // Library panel (Phase 2)
    CreateLibraryFolder { name: String, parent_id: Option<usize> },
    RenameLibraryFolder { folder_id: usize, name: String },
    DeleteLibraryFolder { folder_id: usize },
    MoveSymbolToFolder { symbol_id: usize, folder_id: Option<usize> },
    SetLibrarySearch { query: String },

    // Swap sets (Phase 3)
    CreateSwapSet { layer_id: usize, name: String },
    AddSwapItem { swap_set_id: usize, name: String, symbol_id: Option<usize> },
    RemoveSwapItem { swap_set_id: usize, item_id: usize },
    ActivateSwapItem { swap_set_id: usize, item_id: usize },
    DeleteSwapSet { swap_set_id: usize },

    // Mesh Warp (Phase 3)
    AddMeshWarp { layer_id: usize, cols: u32, rows: u32 },
    RemoveMeshWarp { layer_id: usize },
    SetWarpPoint { layer_id: usize, col: u32, row: u32, dx: f32, dy: f32 },
    ResetWarpPoints { layer_id: usize },
    SetMeshWarpEnabled { layer_id: usize, enabled: bool },
    SetMeshWarpGrid { layer_id: usize, cols: u32, rows: u32 },

    // Bone Influence Weights (Phase 3)
    SetBoneWeight { layer_id: usize, bone_id: usize, weight: f32 },
    RemoveBoneWeight { layer_id: usize, bone_id: usize },
    ClearBoneWeights { layer_id: usize },
    NormalizeBoneWeights { layer_id: usize },
    SetWeightPaintingActive(bool),
    SetActiveWeightBone { bone_id: Option<usize> },
    SetWeightBrushRadius(f32),

    // Easing Curves Library (Phase 3)
    AddEasingCurve { name: String, kind: EasingCurveKind },
    RemoveEasingCurve { curve_id: usize },
    RenameEasingCurve { curve_id: usize, name: String },
    SetKeyframeEasingCurve { keyframe_id: usize, curve_id: usize },

    // Audio Track + Lip Sync (Phase 3)
    AddAudioTrack { name: String },
    RemoveAudioTrack { id: usize },
    SetAudioTrackPath { id: usize, path: String },
    SetAudioTrackOffset { id: usize, frames: i32 },
    SetAudioTrackVolume { id: usize, volume: f32 },
    MuteAudioTrack { id: usize, muted: bool },
    ToggleWaveformVisible { id: usize },
    SetLipSyncData { audio_track_id: usize, rig_id: usize, phonemes: Vec<PhonemeFrame> },
    ClearLipSyncData { audio_track_id: usize },

    // Character Animator Behaviors (Phase 3)
    AddBehavior { name: String, kind: BehaviorKind },
    RemoveBehavior { id: usize },
    ToggleBehavior { id: usize },
    SetBehaviorPriority { id: usize, priority: i32 },
    RenameBehavior { id: usize, name: String },
    UpdateBehaviorKind { id: usize, kind: BehaviorKind },

    // 3D Layer Transforms (batch 6 — A)
    SetLayer3DRotationX { layer_id: usize, degrees: f32 },
    SetLayer3DRotationY { layer_id: usize, degrees: f32 },
    SetLayerZPosition { layer_id: usize, z: f32 },
    SetVanishingPoint { x: f32, y: f32 },
    SetProjection3D(Projection3D),
    Reset3DTransform { layer_id: usize },
    Enable3DLayer { layer_id: usize, enabled: bool },

    // Camera / Viewport Controls (batch 6 — B)
    SetCameraPosition { x: f32, y: f32 },
    SetCameraZoom(f32),
    SetCameraRotation(f32),
    ResetCamera,
    ToggleCameraEnabled,
    AddCameraKeyframe { frame: u32, x: f32, y: f32, zoom: f32, rotation: f32 },
    RemoveCameraKeyframe { kf_id: usize },
    SetCameraAnimatable(bool),

    // Motion Capture Import (batch 6 — C)
    ImportMocap { name: String, path: String, format: MocapFormat },
    RemoveMocap { mocap_id: usize },
    SetMocapBoneMapping { mocap_id: usize, mocap_bone: String, rig_bone_id: usize },
    SetMocapRetargetScale { mocap_id: usize, scale: f32 },
    SetMocapApplyRig { mocap_id: usize, rig_id: Option<usize> },
    BakeMocapToKeyframes { mocap_id: usize },

    // Publishing / Advanced Export (batch 6 — D)
    SetPublishTarget(PublishTarget),
    SetPublishOutputPath(String),
    SetPublishDimensions { width: u32, height: u32 },
    SetPublishFrameRate(f32),
    SetPublishQuality(u8),
    SetPublishLoop(bool),
    SetPublishTransparentBg(bool),
    AddToExportQueue { name: String, config: PublishConfig },
    RemoveFromExportQueue { job_id: usize },
    ClearExportQueue,
    SetExportJobStatus { job_id: usize, status: ExportStatus },

    // Stage / Document Settings (batch 6 — E)
    SetStageDimensions { width: u32, height: u32 },
    SetStageFrameRate(f32),
    SetStageBackgroundColor([u8; 4]),
    SetStageRulerUnit(StageRulerUnit),
    SetSnapToObjects(bool),
    SetSnapToPixel(bool),
    SetAutoSave { enabled: bool, interval_min: u32 },
    SetUndoLevels(u32),
    SetSceneLabel { scene_id: usize, color: LabelColor },
    SetSceneDescription { scene_id: usize, description: String },
    SetSceneFrameCount { scene_id: usize, count: u32 },
    // Scripting (Batch 7 — A)
    AddScript { name: String, language: ScriptLanguage },
    RemoveScript { script_id: usize },
    RenameScript { script_id: usize, name: String },
    SetScriptSource { script_id: usize, source: String },
    SetScriptTarget { script_id: usize, target: ScriptTarget },
    SetScriptEvent { script_id: usize, event: ScriptEvent },
    ToggleScript { script_id: usize },
    SetScriptError { script_id: usize, error: Option<String> },
    ClearScriptError { script_id: usize },
    AppendScriptLog { level: LogLevel, message: String, script_id: Option<usize> },
    ClearScriptConsole,
    SetScriptExecutionEnabled(bool),

    // Facial Capture (Batch 7 — B)
    AddCaptureSession { name: String, source: CaptureSource },
    RemoveCaptureSession { session_id: usize },
    SetCaptureTarget { session_id: usize, layer_id: Option<usize> },
    ToggleCaptureRecording { session_id: usize },
    SetCaptureActive { session_id: usize, active: bool },
    AddCaptureMapping { feature: TrackedFeature, target_param: String },
    RemoveCaptureMapping { mapping_index: usize },
    SetCaptureMappingMultiplier { index: usize, multiplier: f32 },
    SetCaptureMappingOffset { index: usize, offset: f32 },
    ToggleLivePreview,

    // AI Motion (Batch 7 — C)
    RequestAiMotion { prompt: String, model: AiMotionModel, layer_ids: Vec<usize>, frames: u32 },
    UpdateAiMotionStatus { request_id: usize, status: AiRequestStatus },
    CancelAiMotion { request_id: usize },
    RequestAiInterpolation2 { layer_id: usize, from_frame: u32, to_frame: u32, inbetweens: u32 },
    UpdateAiInterpStatus { request_id: usize, status: AiRequestStatus },
    RequestStyleTransfer { source: String, layer_id: usize, strength: f32 },
    UpdateStyleTransferStatus { request_id: usize, status: AiRequestStatus },
    SetAiModelPath(Option<String>),
    SetAiComputeBackend(AiBackend),

    // Advanced Tweening (Batch 7 — D)
    AddMotionGuide { layer_id: usize, path_id: usize, start_frame: u32, end_frame: u32 },
    RemoveMotionGuide { guide_id: usize },
    SetMotionGuideOrient { guide_id: usize, orient: bool },
    SetMotionGuideSnap { guide_id: usize, snap: bool },
    AddPropertyTween {
        layer_id: usize,
        property: String,
        from_frame: u32,
        to_frame: u32,
        from_val: f32,
        to_val: f32,
        kind: AdvancedTweenKind,
    },
    RemovePropertyTween { tween_id: usize },
    UpdatePropertyTweenKind { tween_id: usize, kind: AdvancedTweenKind },
    SetPropertyTweenRange { tween_id: usize, from_frame: u32, to_frame: u32 },

    // Beat Sync (Batch 7 — E)
    AddAudioMarker { time_secs: f32, label: String, kind: MarkerKind },
    RemoveAudioMarker { marker_id: usize },
    MoveAudioMarker { marker_id: usize, time_secs: f32 },
    SetMarkerLabel { marker_id: usize, label: String },
    SetBeatSyncEnabled(bool),
    SetBeatSyncBpm(f32),
    SetBeatSyncOffset(f32),
    SetBeatSyncAudioTrack { track_id: Option<usize> },
    SetDetectedBpm(Option<f32>),
    ToggleSyncSnapping,
    AddSyncGroup { name: String, layer_ids: Vec<usize> },
    RemoveSyncGroup { group_id: usize },
    SetSyncGroupLayers { group_id: usize, layer_ids: Vec<usize> },
    // --- lottie ---
    StartLottieExport { config: LottieExportConfig },
    CompleteLottieExport,
    CancelLottieExport,
    StartLottieImport { source_path: String },
    UpdateLottieImport { job_id: usize, layers_created: usize },
    CompleteLottieImport { job_id: usize },
    CancelLottieImport { job_id: usize },
    // --- blend_modes_layer ---
    SetLayerBlend { layer_id: usize, blend: LayerBlend },
    SetLayerFillOpacity { layer_id: usize, opacity: f32 },
    ResetLayerBlend { layer_id: usize },
    // --- plugin_api ---
    RegisterPlugin { manifest: PluginManifest },
    EnablePlugin { plugin_id: String },
    DisablePlugin { plugin_id: String },
    UnloadPlugin { plugin_id: String },
    OpenExtensionPanel { plugin_id: String, title: String },
    CloseExtensionPanel { panel_id: usize },
    ToggleExtensionPanel { panel_id: usize },
    // Tool selection
    SetActiveTool(DriftTool),

    // --- deformation ---
    AddPinAnchor { layer_id: usize, x: f32, y: f32 },
    RemovePinAnchor { anchor_id: usize },
    MovePinAnchor { anchor_id: usize, x: f32, y: f32 },
    LockPinAnchor { anchor_id: usize, locked: bool },
    AddDeformLayer { layer_id: usize, kind: DeformKind },
    RemoveDeformLayer { deform_id: usize },
    SetDeformStrength { deform_id: usize, strength: f32 },
    ToggleDeform { deform_id: usize },
    SetStretchSquash { layer_id: usize, stretch: f32, squash: f32 },
    ToggleStretchSquash { layer_id: usize },

    // --- masking ---
    AddLayerMask { layer_id: usize, mask_source_id: usize, kind: MaskKind },
    RemoveLayerMask { mask_id: usize },
    SetMaskKind { mask_id: usize, kind: MaskKind },
    ToggleMask { mask_id: usize },
    CreateClippingGroup { base_layer_id: usize },
    AddToClippingGroup { group_id: usize, layer_id: usize },
    RemoveFromClippingGroup { group_id: usize, layer_id: usize },
    DissolveClippingGroup { group_id: usize },

    // --- text_layer ---
    AddTextLayer { layer_id: usize, content: String, font_family: String, font_size: f32 },
    RemoveTextLayer { text_id: usize },
    SetTextContent { text_id: usize, content: String },
    SetTextFont { text_id: usize, family: String, size: f32 },
    SetTextColor { text_id: usize, color: u32 },
    SetTextAlign { text_id: usize, align: TextAlign },
    SetTextStyle { text_id: usize, style: TextStyle },
    SetTextSpacing { text_id: usize, line_height: f32, letter_spacing: f32 },
    QueueSvgImport { path: String, layer_id: usize },
    CompleteSvgImport { job_id: usize },
}

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
}

impl App {
    pub fn new() -> Self {
        let doc = DriftDocument::new();
        let out = doc.duration_frames;
        let mut app = Self {
            active_tool: DriftTool::Select,
            document: doc,
            grid: GridConfig::new(),
            rulers: RulerConfig::new(),
            layers: Vec::new(),
            layer_counter: 0,
            active_layer: None,
            keyframes: Vec::new(),
            keyframe_counter: 0,
            transforms: HashMap::new(),
            current_frame: 0,
            playing: false,
            loop_playback: false,
            in_point: 0,
            out_point: out,
            layer_rigs: Vec::new(),
            rig_bone_counter: 0,
            ik_chains: Vec::new(),
            next_chain_id: 0,
            bone_springs: Vec::new(),
            ai_motion_prompt: String::new(),
            ai_motion_results: Vec::new(),
            ai_lipsync_jobs: Vec::new(),
            ai_interpolation_queue: Vec::new(),
            export_config: ExportConfig::new(),
            export_in_progress: false,
            state_machines: Vec::new(),
            state_machine_counter: 0,
            anim_state_counter: 0,
            // Phase 2
            vector_paths: Vec::new(),
            next_path_id: 0,
            symbols: Vec::new(),
            symbol_instances: Vec::new(),
            next_symbol_id: 0,
            next_instance_id: 0,
            tweens: Vec::new(),
            next_tween_id: 0,
            onion_skin: OnionSkinConfig::new(),
            // Phase 2: Scenes — always start with one default scene.
            scenes: vec![Scene {
                id: 0,
                name: "Scene 1".to_string(),
                duration_frames: 240,
                layer_ids: Vec::new(),
            }],
            active_scene_id: 0,
            next_scene_id: 1,
            // Phase 2: Frame labels
            frame_labels: Vec::new(),
            next_label_id: 0,
            // Phase 2: Library
            library_folders: Vec::new(),
            library_search: String::new(),
            next_folder_id: 0,
            symbol_folder_map: Vec::new(),
            // Phase 3: Swap sets
            swap_sets: Vec::new(),
            next_swap_set_id: 0,
            next_swap_item_id: 0,
            // Phase 3: Mesh Warp
            mesh_warps: HashMap::new(),
            next_warp_id: 0,
            // Phase 3: Bone Weights
            bone_weights: HashMap::new(),
            weight_painting_active: false,
            active_weight_bone: None,
            weight_brush_radius: 20.0,
            // Phase 3: Easing Curves (seeded below)
            easing_curves: Vec::new(),
            next_easing_id: 0,
            keyframe_easing_map: HashMap::new(),
            // Phase 3: Audio / Lip Sync
            audio_tracks: Vec::new(),
            next_audio_track_id: 0,
            lip_sync_data: Vec::new(),
            // Phase 3: Behaviors
            behaviors: Vec::new(),
            next_behavior_id: 0,
            // Batch 6 — A: 3D Layer Transforms
            layer_3d: HashMap::new(),
            global_vanishing_point: (0.5, 0.5),
            global_projection_3d: Projection3D::Perspective,
            // Batch 6 — B: Camera
            camera: DriftCamera::new(),
            camera_keyframes: Vec::new(),
            next_camera_kf_id: 0,
            camera_enabled: false,
            // Batch 6 — C: Mocap
            mocap_imports: Vec::new(),
            next_mocap_id: 0,
            // Batch 6 — D: Publish / Export Queue
            publish_config: PublishConfig::new(),
            export_queue: ExportQueue::new(),
            // Batch 6 — E: Stage
            stage: StageSettings::new(),
            scene_properties: HashMap::new(),
            // Batch 7 — A: Scripting
            scripts: Vec::new(),
            next_script_id: 0,
            script_console: ScriptConsole::new(),
            script_execution_enabled: true,
            // Batch 7 — B: Facial Capture
            capture_sessions: Vec::new(),
            next_capture_id: 0,
            capture_mappings: Vec::new(),
            live_preview_enabled: false,
            // Batch 7 — C: AI Motion
            ai_motion_requests: Vec::new(),
            next_ai_motion_id: 0,
            ai_interpolation_requests: Vec::new(),
            next_interp_id: 0,
            ai_style_transfers: Vec::new(),
            next_style_id: 0,
            ai_model_path: None,
            ai_compute_backend: AiBackend::Cpu,
            // Batch 7 — D: Advanced Tweening
            motion_guides: Vec::new(),
            next_guide_id: 0,
            property_tweens: Vec::new(),
            next_prop_tween_id: 0,
            // Batch 7 — E: Beat Sync
            audio_markers: Vec::new(),
            next_marker_id: 0,
            beat_sync: BeatSyncConfig::new(),
            sync_groups: Vec::new(),
            next_sync_group_id: 0,
            // Batch 8 — Deformation
            pin_anchors: vec![],
            deform_layers: vec![],
            stretch_squash: vec![],
            next_anchor_id: 1,
            next_deform_id: 1,
            // Batch 8 — Masking
            layer_masks: vec![],
            clipping_groups: vec![],
            next_mask_id: 1,
            next_clip_group_id: 1,
            // Batch 8 — Text Layers / SVG Import
            text_layers: vec![],
            svg_import_jobs: vec![],
            next_text_layer_id: 1,
            next_svg_job_id: 1,
            // Batch 8 — Lottie
            lottie_export_config: None,
            lottie_exporting: false,
            lottie_import_jobs: Vec::new(),
            next_lottie_job_id: 1,
            // Batch 8 — Layer Blend Modes
            layer_blend_configs: Vec::new(),
            // Batch 8 — Plugin API
            loaded_plugins: Vec::new(),
            extension_panels: Vec::new(),
            next_ext_panel_id: 1,
        };
        app.seed_easing_curves();
        app
    }

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
            | Action::CancelLottieImport { .. } => self.apply_lottie(action),

            // Blend Modes (Batch 8)
            Action::SetLayerBlend { .. }
            | Action::SetLayerFillOpacity { .. }
            | Action::ResetLayerBlend { .. } => self.apply_blend_modes(action),

            // Plugin API (Batch 8)
            Action::RegisterPlugin { .. }
            | Action::EnablePlugin { .. }
            | Action::DisablePlugin { .. }
            | Action::UnloadPlugin { .. }
            | Action::OpenExtensionPanel { .. }
            | Action::CloseExtensionPanel { .. }
            | Action::ToggleExtensionPanel { .. } => self.apply_plugin_api(action),

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
