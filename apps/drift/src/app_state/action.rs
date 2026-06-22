//! Action enum and tool enum for the Drift application state.
//!
//! Every mutation the UI can request is expressed as an [`Action`] variant.
//! Pass actions through [`super::App::apply`] — never mutate App fields directly.

use super::{
    EasingKind, BezierPoint, Fill, Stroke, LayerKind, TweenKind, RotateDirection, SymbolKind,
    PhonemeFrame, StateTransitionTrigger, MocapFormat, PublishTarget, PublishConfig, ExportStatus,
    StageRulerUnit, LabelColor, ScriptLanguage, ScriptTarget, ScriptEvent, LogLevel,
    CaptureSource, TrackedFeature, AiMotionModel, AiBackend, AiRequestStatus, AdvancedTweenKind,
    MarkerKind, LottieExportConfig, LayerBlend, PluginManifest, DeformKind, MaskKind,
    TextAlign, TextStyle, Projection3D, RulerUnit,
};

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

/// Every mutation the UI can request. Apply via [`super::App::apply`].
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
    SetExportFormat(super::ExportFormat),
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
    AddEasingCurve { name: String, kind: super::EasingCurveKind },
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
    AddBehavior { name: String, kind: super::BehaviorKind },
    RemoveBehavior { id: usize },
    ToggleBehavior { id: usize },
    SetBehaviorPriority { id: usize, priority: i32 },
    RenameBehavior { id: usize, name: String },
    UpdateBehaviorKind { id: usize, kind: super::BehaviorKind },

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

    // Lottie (Batch 8)
    StartLottieExport { config: LottieExportConfig },
    CompleteLottieExport,
    CancelLottieExport,
    StartLottieImport { source_path: String },
    UpdateLottieImport { job_id: usize, layers_created: usize },
    CompleteLottieImport { job_id: usize },
    CancelLottieImport { job_id: usize },

    // Blend Modes (Batch 8)
    SetLayerBlend { layer_id: usize, blend: LayerBlend },
    SetLayerFillOpacity { layer_id: usize, opacity: f32 },
    ResetLayerBlend { layer_id: usize },

    // Plugin API (Batch 8)
    RegisterPlugin { manifest: PluginManifest },
    EnablePlugin { plugin_id: String },
    DisablePlugin { plugin_id: String },
    UnloadPlugin { plugin_id: String },
    OpenExtensionPanel { plugin_id: String, title: String },
    CloseExtensionPanel { panel_id: usize },
    ToggleExtensionPanel { panel_id: usize },

    // Tool selection
    SetActiveTool(DriftTool),

    // Deformation (Batch 8)
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

    // Masking (Batch 8)
    AddLayerMask { layer_id: usize, mask_source_id: usize, kind: MaskKind },
    RemoveLayerMask { mask_id: usize },
    SetMaskKind { mask_id: usize, kind: MaskKind },
    ToggleMask { mask_id: usize },
    CreateClippingGroup { base_layer_id: usize },
    AddToClippingGroup { group_id: usize, layer_id: usize },
    RemoveFromClippingGroup { group_id: usize, layer_id: usize },
    DissolveClippingGroup { group_id: usize },

    // Text Layers / SVG Import (Batch 8)
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
