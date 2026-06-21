//! The single shared application state for the Drift GPUI host.
//!
//! `App` owns everything panels read or mutate: the document, layers, keyframes,
//! transforms, puppet rigs, AI feature state, export config, state machines,
//! vector paths, symbols, symbol instances, tweens, and onion skin config.
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

// Re-exports so callers can use `app_state::{App, Action, ...}` directly.
pub use document::DriftDocument;
pub use layers::{DriftLayer, LayerKind};
pub use keyframes::{EasingKind, Keyframe};
pub use transforms::LayerTransform;
pub use rig::{RigBone, LayerRig};
pub use export::{ExportFormat, ExportConfig};
pub use state_machine::{StateTransitionTrigger, AnimationState, StateTransition, StateMachine};
pub use vector::{VectorPath, BezierPoint, Fill, GradientStop, Stroke, StrokeCap, StrokeJoin};
pub use symbols::{SymbolKind, Symbol, SymbolInstance};
pub use tweening::{TweenKind, Tween, RotateDirection, OnionSkinConfig};

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

    // AI Features
    SetAiMotionPrompt(String),
    GenerateAiMotion { layer_id: usize },
    StartAiLipSync { layer_id: usize, audio_path: String },
    CompleteAiLipSync { layer_id: usize },
    RequestAiInterpolation { layer_id: usize, from_frame: usize, to_frame: usize },
    CompleteAiInterpolation { layer_id: usize },
    AutoRigWithAi(usize),

    // Export
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
}

// ── App ───────────────────────────────────────────────────────────────────────

/// Shared application state. All mutation goes through [`App::apply`].
pub struct App {
    // Document
    pub document: DriftDocument,

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

    // AI
    pub ai_motion_prompt: String,
    pub ai_motion_results: Vec<(usize, String)>,
    pub ai_lipsync_jobs: Vec<(usize, String)>,
    pub ai_interpolation_queue: Vec<(usize, usize, usize)>,

    // Export
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
}

impl App {
    pub fn new() -> Self {
        let doc = DriftDocument::new();
        let out = doc.duration_frames;
        Self {
            document: doc,
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
        }
    }

    /// The single mutation choke point. Every panel and keyboard handler routes
    /// through here so state changes are predictable and testable.
    pub fn apply(&mut self, action: Action) {
        match &action {
            // Document
            Action::SetDocumentWidth(_)
            | Action::SetDocumentHeight(_)
            | Action::SetDocumentFps(_)
            | Action::SetDocumentDuration(_)
            | Action::SetDocumentBg(_)
            | Action::SetDocumentName(_) => self.apply_document(action),

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

            // Rig
            Action::AddBone { .. }
            | Action::DeleteBone { .. }
            | Action::MoveBone { .. }
            | Action::RotateBone { .. }
            | Action::SetIKTarget { .. }
            | Action::AutoRigLayer(_) => self.apply_rig(action),

            // AI
            Action::SetAiMotionPrompt(_)
            | Action::GenerateAiMotion { .. }
            | Action::StartAiLipSync { .. }
            | Action::CompleteAiLipSync { .. }
            | Action::RequestAiInterpolation { .. }
            | Action::CompleteAiInterpolation { .. }
            | Action::AutoRigWithAi(_) => self.apply_ai(action),

            // Export
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
