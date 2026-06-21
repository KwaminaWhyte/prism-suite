//! The single shared application state for the Drift GPUI host.
//!
//! `App` owns everything panels read or mutate: the document, layers, keyframes,
//! transforms, puppet rigs, AI feature state, export config, and state machines.
//! Panels NEVER mutate `App` fields directly — they emit an [`Action`], and the
//! root view routes it through [`App::apply`], the single mutation choke point.
//!
//! This mirrors the pattern established in `pulse/src/app_state.rs`.

use std::collections::HashMap;

// ── Document ─────────────────────────────────────────────────────────────────

/// Top-level document metadata.
#[derive(Clone, Debug)]
pub struct DriftDocument {
    pub width: u32,
    pub height: u32,
    pub fps: f32,
    pub duration_frames: usize,
    pub background_color: String,
    pub name: String,
}

impl DriftDocument {
    pub fn new() -> Self {
        Self {
            width: 1920,
            height: 1080,
            fps: 24.0,
            duration_frames: 240,
            background_color: "#000000".to_string(),
            name: "Untitled".to_string(),
        }
    }
}

// ── Layers ────────────────────────────────────────────────────────────────────

/// The kind of content a layer holds.
#[derive(Clone, Debug, PartialEq)]
pub enum LayerKind {
    Vector,
    Bitmap,
    Audio,
    Camera,
    Guide,
    Null,
}

/// A single timeline layer.
#[derive(Clone, Debug)]
pub struct DriftLayer {
    pub id: usize,
    pub name: String,
    pub kind: LayerKind,
    pub visible: bool,
    pub locked: bool,
    pub solo: bool,
    pub parent_id: Option<usize>,
    pub z_order: usize,
    pub color_tag: String,
    pub start_frame: usize,
    pub end_frame: usize,
}

// ── Keyframes ─────────────────────────────────────────────────────────────────

/// Interpolation easing for a keyframe segment.
#[derive(Clone, Debug, PartialEq)]
pub enum EasingKind {
    Linear,
    EaseIn,
    EaseOut,
    EaseInOut,
    Bezier,
    Hold,
    Spring,
}

/// One keyframe on a named property of a layer.
#[derive(Clone, Debug)]
pub struct Keyframe {
    pub id: usize,
    pub layer_id: usize,
    pub property: String,
    pub frame: usize,
    pub value: f32,
    pub easing: EasingKind,
    pub bezier_handle_in: (f32, f32),
    pub bezier_handle_out: (f32, f32),
}

// ── Layer Transform ───────────────────────────────────────────────────────────

/// The full spatial transform for a layer at a given instant.
#[derive(Clone, Debug)]
pub struct LayerTransform {
    pub x: f32,
    pub y: f32,
    pub scale_x: f32,
    pub scale_y: f32,
    pub rotation: f32,
    pub opacity: f32,
    pub anchor_x: f32,
    pub anchor_y: f32,
}

impl LayerTransform {
    pub fn new() -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            scale_x: 1.0,
            scale_y: 1.0,
            rotation: 0.0,
            opacity: 1.0,
            anchor_x: 0.0,
            anchor_y: 0.0,
        }
    }
}

// ── Puppet Rig ────────────────────────────────────────────────────────────────

/// One bone in a puppet rig hierarchy.
#[derive(Clone, Debug)]
pub struct RigBone {
    pub id: usize,
    pub name: String,
    pub parent_id: Option<usize>,
    pub x: f32,
    pub y: f32,
    pub length: f32,
    pub rotation: f32,
    pub locked: bool,
}

/// The full puppet rig for a layer.
#[derive(Clone, Debug)]
pub struct LayerRig {
    pub layer_id: usize,
    pub bones: Vec<RigBone>,
    /// (bone_id, target_x, target_y)
    pub ik_targets: Vec<(usize, f32, f32)>,
}

// ── Export ────────────────────────────────────────────────────────────────────

/// Supported export formats.
#[derive(Clone, Debug, PartialEq)]
pub enum ExportFormat {
    Mp4,
    Gif,
    WebM,
    LottieJson,
    Apng,
    Spritesheet,
    Png,
}

/// Configuration for a single export job.
#[derive(Clone, Debug)]
pub struct ExportConfig {
    pub format: ExportFormat,
    pub fps: f32,
    pub scale: f32,
    pub quality: u8,
    pub transparent: bool,
    pub start_frame: usize,
    pub end_frame: usize,
    pub output_path: String,
}

impl ExportConfig {
    pub fn new() -> Self {
        Self {
            format: ExportFormat::Mp4,
            fps: 24.0,
            scale: 1.0,
            quality: 85,
            transparent: false,
            start_frame: 0,
            end_frame: 0,
            output_path: String::new(),
        }
    }
}

// ── State Machine ─────────────────────────────────────────────────────────────

/// Triggers that drive transitions between animation states.
#[derive(Clone, Debug, PartialEq)]
pub enum StateTransitionTrigger {
    OnClick,
    OnHover,
    OnComplete,
    OnCondition(String),
}

/// One named state in an animation state machine.
#[derive(Clone, Debug)]
pub struct AnimationState {
    pub id: usize,
    pub name: String,
    pub start_frame: usize,
    pub end_frame: usize,
    pub looping: bool,
}

/// A directed edge between two states with a trigger and blend duration.
#[derive(Clone, Debug)]
pub struct StateTransition {
    pub from_state: usize,
    pub to_state: usize,
    pub trigger: StateTransitionTrigger,
    pub duration_frames: usize,
}

/// A named state machine grouping states and transitions.
#[derive(Clone, Debug)]
pub struct StateMachine {
    pub id: usize,
    pub name: String,
    pub states: Vec<AnimationState>,
    pub transitions: Vec<StateTransition>,
    pub initial_state: usize,
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
        }
    }

    /// The single mutation choke point. Every panel and keyboard handler routes
    /// through here so state changes are predictable and testable.
    pub fn apply(&mut self, action: Action) {
        match action {
            // ── Document ──────────────────────────────────────────────────────
            Action::SetDocumentWidth(w) => {
                self.document.width = w;
            }
            Action::SetDocumentHeight(h) => {
                self.document.height = h;
            }
            Action::SetDocumentFps(fps) => {
                self.document.fps = fps.clamp(1.0, 120.0);
            }
            Action::SetDocumentDuration(frames) => {
                self.document.duration_frames = frames;
            }
            Action::SetDocumentBg(color) => {
                self.document.background_color = color;
            }
            Action::SetDocumentName(name) => {
                self.document.name = name;
            }

            // ── Layers ────────────────────────────────────────────────────────
            Action::AddLayer { name, kind } => {
                let id = self.layer_counter;
                self.layer_counter += 1;
                let z = self.layers.len();
                self.layers.push(DriftLayer {
                    id,
                    name,
                    kind,
                    visible: true,
                    locked: false,
                    solo: false,
                    parent_id: None,
                    z_order: z,
                    color_tag: String::new(),
                    start_frame: 0,
                    end_frame: self.document.duration_frames,
                });
                self.transforms.insert(id, LayerTransform::new());
                self.active_layer = Some(id);
            }
            Action::DeleteLayer(id) => {
                self.layers.retain(|l| l.id != id);
                self.keyframes.retain(|k| k.layer_id != id);
                self.transforms.remove(&id);
                self.layer_rigs.retain(|r| r.layer_id != id);
                if self.active_layer == Some(id) {
                    self.active_layer = self.layers.last().map(|l| l.id);
                }
            }
            Action::RenameLayer { id, name } => {
                if let Some(l) = self.layers.iter_mut().find(|l| l.id == id) {
                    l.name = name;
                }
            }
            Action::SetLayerVisible { id, visible } => {
                if let Some(l) = self.layers.iter_mut().find(|l| l.id == id) {
                    l.visible = visible;
                }
            }
            Action::SetLayerLocked { id, locked } => {
                if let Some(l) = self.layers.iter_mut().find(|l| l.id == id) {
                    l.locked = locked;
                }
            }
            Action::SetLayerSolo { id, solo } => {
                if let Some(l) = self.layers.iter_mut().find(|l| l.id == id) {
                    l.solo = solo;
                }
            }
            Action::SetLayerParent { id, parent_id } => {
                if let Some(l) = self.layers.iter_mut().find(|l| l.id == id) {
                    l.parent_id = parent_id;
                }
            }
            Action::ReorderLayers(order) => {
                let mut new_layers: Vec<DriftLayer> = Vec::with_capacity(order.len());
                for (z, &oid) in order.iter().enumerate() {
                    if let Some(mut layer) = self.layers.iter().find(|l| l.id == oid).cloned() {
                        layer.z_order = z;
                        new_layers.push(layer);
                    }
                }
                self.layers = new_layers;
            }
            Action::SetLayerColorTag { id, tag } => {
                if let Some(l) = self.layers.iter_mut().find(|l| l.id == id) {
                    l.color_tag = tag;
                }
            }
            Action::DuplicateLayer(id) => {
                if let Some(src) = self.layers.iter().find(|l| l.id == id).cloned() {
                    let new_id = self.layer_counter;
                    self.layer_counter += 1;
                    let z = self.layers.len();
                    let src_transform = self
                        .transforms
                        .get(&src.id)
                        .cloned()
                        .unwrap_or_else(LayerTransform::new);
                    self.layers.push(DriftLayer {
                        id: new_id,
                        name: format!("{} copy", src.name),
                        z_order: z,
                        ..src
                    });
                    self.transforms.insert(new_id, src_transform);
                    self.active_layer = Some(new_id);
                }
            }
            Action::SetLayerStartFrame { id, frame } => {
                if let Some(l) = self.layers.iter_mut().find(|l| l.id == id) {
                    l.start_frame = frame;
                }
            }
            Action::SetLayerEndFrame { id, frame } => {
                if let Some(l) = self.layers.iter_mut().find(|l| l.id == id) {
                    l.end_frame = frame;
                }
            }
            Action::SetActiveLayer(id) => {
                if self.layers.iter().any(|l| l.id == id) {
                    self.active_layer = Some(id);
                }
            }

            // ── Keyframes ─────────────────────────────────────────────────────
            Action::AddKeyframe { layer_id, property, frame, value, easing } => {
                let id = self.keyframe_counter;
                self.keyframe_counter += 1;
                self.keyframes.push(Keyframe {
                    id,
                    layer_id,
                    property,
                    frame,
                    value,
                    easing,
                    bezier_handle_in: (0.0, 0.0),
                    bezier_handle_out: (1.0, 1.0),
                });
            }
            Action::DeleteKeyframe(id) => {
                self.keyframes.retain(|k| k.id != id);
            }
            Action::MoveKeyframe { id, frame } => {
                if let Some(k) = self.keyframes.iter_mut().find(|k| k.id == id) {
                    k.frame = frame;
                }
            }
            Action::SetKeyframeValue { id, value } => {
                if let Some(k) = self.keyframes.iter_mut().find(|k| k.id == id) {
                    k.value = value;
                }
            }
            Action::SetKeyframeEasing { id, easing } => {
                if let Some(k) = self.keyframes.iter_mut().find(|k| k.id == id) {
                    k.easing = easing;
                }
            }
            Action::SetPropertyAtFrame { layer_id, property, frame, value } => {
                // Update existing keyframe if one exists at this layer/property/frame,
                // otherwise auto-create one with Linear easing.
                let existing = self
                    .keyframes
                    .iter_mut()
                    .find(|k| k.layer_id == layer_id && k.property == property && k.frame == frame);
                if let Some(k) = existing {
                    k.value = value;
                } else {
                    let id = self.keyframe_counter;
                    self.keyframe_counter += 1;
                    self.keyframes.push(Keyframe {
                        id,
                        layer_id,
                        property,
                        frame,
                        value,
                        easing: EasingKind::Linear,
                        bezier_handle_in: (0.0, 0.0),
                        bezier_handle_out: (1.0, 1.0),
                    });
                }
            }

            // ── Transform ─────────────────────────────────────────────────────
            Action::SetLayerPosition { id, x, y } => {
                let t = self.transforms.entry(id).or_insert_with(LayerTransform::new);
                t.x = x;
                t.y = y;
            }
            Action::SetLayerScale { id, x, y } => {
                let t = self.transforms.entry(id).or_insert_with(LayerTransform::new);
                t.scale_x = x.clamp(0.001, 100.0);
                t.scale_y = y.clamp(0.001, 100.0);
            }
            Action::SetLayerRotation { id, degrees } => {
                let t = self.transforms.entry(id).or_insert_with(LayerTransform::new);
                t.rotation = degrees;
            }
            Action::SetLayerOpacity { id, opacity } => {
                let t = self.transforms.entry(id).or_insert_with(LayerTransform::new);
                t.opacity = opacity.clamp(0.0, 1.0);
            }
            Action::SetLayerAnchor { id, x, y } => {
                let t = self.transforms.entry(id).or_insert_with(LayerTransform::new);
                t.anchor_x = x;
                t.anchor_y = y;
            }
            Action::ResetLayerTransform(id) => {
                self.transforms.insert(id, LayerTransform::new());
            }

            // ── Playback ──────────────────────────────────────────────────────
            Action::Play => {
                self.playing = true;
            }
            Action::Pause => {
                self.playing = false;
            }
            Action::Stop => {
                self.playing = false;
                self.current_frame = self.in_point;
            }
            Action::SetCurrentFrame(frame) => {
                self.current_frame = frame.min(self.document.duration_frames);
            }
            Action::ToggleLoop => {
                self.loop_playback = !self.loop_playback;
            }
            Action::SetInPoint(frame) => {
                self.in_point = frame.min(self.out_point);
            }
            Action::SetOutPoint(frame) => {
                self.out_point = frame.max(self.in_point);
            }
            Action::StepForward => {
                if self.current_frame < self.document.duration_frames {
                    self.current_frame += 1;
                }
            }
            Action::StepBackward => {
                if self.current_frame > 0 {
                    self.current_frame -= 1;
                }
            }
            Action::GoToFirstFrame => {
                self.current_frame = 0;
            }
            Action::GoToLastFrame => {
                self.current_frame = self.document.duration_frames;
            }

            // ── Puppet Rig ────────────────────────────────────────────────────
            Action::AddBone { layer_id, parent_id, x, y, length } => {
                let bone_id = self.rig_bone_counter;
                self.rig_bone_counter += 1;
                let bone = RigBone {
                    id: bone_id,
                    name: format!("bone_{bone_id}"),
                    parent_id,
                    x,
                    y,
                    length,
                    rotation: 0.0,
                    locked: false,
                };
                if let Some(rig) = self.layer_rigs.iter_mut().find(|r| r.layer_id == layer_id) {
                    rig.bones.push(bone);
                } else {
                    self.layer_rigs.push(LayerRig {
                        layer_id,
                        bones: vec![bone],
                        ik_targets: Vec::new(),
                    });
                }
            }
            Action::DeleteBone { layer_id, bone_id } => {
                if let Some(rig) = self.layer_rigs.iter_mut().find(|r| r.layer_id == layer_id) {
                    rig.bones.retain(|b| b.id != bone_id);
                    rig.ik_targets.retain(|(bid, _, _)| *bid != bone_id);
                }
            }
            Action::MoveBone { layer_id, bone_id, x, y } => {
                if let Some(rig) = self.layer_rigs.iter_mut().find(|r| r.layer_id == layer_id) {
                    if let Some(b) = rig.bones.iter_mut().find(|b| b.id == bone_id) {
                        b.x = x;
                        b.y = y;
                    }
                }
            }
            Action::RotateBone { layer_id, bone_id, rotation } => {
                if let Some(rig) = self.layer_rigs.iter_mut().find(|r| r.layer_id == layer_id) {
                    if let Some(b) = rig.bones.iter_mut().find(|b| b.id == bone_id) {
                        b.rotation = rotation;
                    }
                }
            }
            Action::SetIKTarget { layer_id, bone_id, x, y } => {
                if let Some(rig) = self.layer_rigs.iter_mut().find(|r| r.layer_id == layer_id) {
                    if let Some(t) = rig.ik_targets.iter_mut().find(|(bid, _, _)| *bid == bone_id) {
                        *t = (bone_id, x, y);
                    } else {
                        rig.ik_targets.push((bone_id, x, y));
                    }
                }
            }
            Action::AutoRigLayer(layer_id) => {
                self.auto_rig_layer(layer_id);
            }

            // ── AI ────────────────────────────────────────────────────────────
            Action::SetAiMotionPrompt(prompt) => {
                self.ai_motion_prompt = prompt;
            }
            Action::GenerateAiMotion { layer_id } => {
                let desc = format!(
                    "AI motion for layer {layer_id}: \"{}\"",
                    self.ai_motion_prompt
                );
                self.ai_motion_results.push((layer_id, desc));
                // Stub: generate 6 keyframes on position_x at frames 0/5/10/15/20/25
                let positions = [0.0_f32, 50.0, 100.0, 50.0, 0.0, -50.0];
                for (i, &v) in positions.iter().enumerate() {
                    let frame = i * 5;
                    let id = self.keyframe_counter;
                    self.keyframe_counter += 1;
                    self.keyframes.push(Keyframe {
                        id,
                        layer_id,
                        property: "position_x".to_string(),
                        frame,
                        value: v,
                        easing: EasingKind::EaseInOut,
                        bezier_handle_in: (0.0, 0.0),
                        bezier_handle_out: (1.0, 1.0),
                    });
                }
            }
            Action::StartAiLipSync { layer_id, audio_path } => {
                self.ai_lipsync_jobs.push((layer_id, audio_path));
            }
            Action::CompleteAiLipSync { layer_id } => {
                self.ai_lipsync_jobs.retain(|(id, _)| *id != layer_id);
                // Stub: 12 mouth_open keyframes
                for i in 0..12_usize {
                    let id = self.keyframe_counter;
                    self.keyframe_counter += 1;
                    let value = if i % 2 == 0 { 0.0 } else { 1.0 };
                    self.keyframes.push(Keyframe {
                        id,
                        layer_id,
                        property: "mouth_open".to_string(),
                        frame: i * 2,
                        value,
                        easing: EasingKind::EaseInOut,
                        bezier_handle_in: (0.0, 0.0),
                        bezier_handle_out: (1.0, 1.0),
                    });
                }
            }
            Action::RequestAiInterpolation { layer_id, from_frame, to_frame } => {
                self.ai_interpolation_queue.push((layer_id, from_frame, to_frame));
            }
            Action::CompleteAiInterpolation { layer_id } => {
                if let Some(pos) =
                    self.ai_interpolation_queue.iter().position(|(id, _, _)| *id == layer_id)
                {
                    self.ai_interpolation_queue.remove(pos);
                }
            }
            Action::AutoRigWithAi(layer_id) => {
                self.auto_rig_layer(layer_id);
            }

            // ── Export ────────────────────────────────────────────────────────
            Action::SetExportFormat(fmt) => {
                self.export_config.format = fmt;
            }
            Action::SetExportFps(fps) => {
                self.export_config.fps = fps;
            }
            Action::SetExportScale(scale) => {
                self.export_config.scale = scale.clamp(0.1, 4.0);
            }
            Action::SetExportQuality(q) => {
                self.export_config.quality = q.min(100);
            }
            Action::SetExportTransparent(t) => {
                self.export_config.transparent = t;
            }
            Action::SetExportStartFrame(f) => {
                self.export_config.start_frame = f;
            }
            Action::SetExportEndFrame(f) => {
                self.export_config.end_frame = f;
            }
            Action::SetExportPath(path) => {
                self.export_config.output_path = path;
            }
            Action::StartExport => {
                self.export_in_progress = true;
            }
            Action::CancelExport => {
                self.export_in_progress = false;
            }

            // ── State Machine ─────────────────────────────────────────────────
            Action::CreateStateMachine(name) => {
                let id = self.state_machine_counter;
                self.state_machine_counter += 1;
                self.state_machines.push(StateMachine {
                    id,
                    name,
                    states: Vec::new(),
                    transitions: Vec::new(),
                    initial_state: 0,
                });
            }
            Action::AddAnimationState { machine_id, name, start_frame, end_frame } => {
                let state_id = self.anim_state_counter;
                self.anim_state_counter += 1;
                if let Some(sm) = self.state_machines.iter_mut().find(|m| m.id == machine_id) {
                    sm.states.push(AnimationState {
                        id: state_id,
                        name,
                        start_frame,
                        end_frame,
                        looping: false,
                    });
                }
            }
            Action::DeleteAnimationState { machine_id, state_id } => {
                if let Some(sm) = self.state_machines.iter_mut().find(|m| m.id == machine_id) {
                    sm.states.retain(|s| s.id != state_id);
                    sm.transitions
                        .retain(|t| t.from_state != state_id && t.to_state != state_id);
                }
            }
            Action::AddStateTransition { machine_id, from, to, trigger, duration } => {
                if let Some(sm) = self.state_machines.iter_mut().find(|m| m.id == machine_id) {
                    sm.transitions.push(StateTransition {
                        from_state: from,
                        to_state: to,
                        trigger,
                        duration_frames: duration,
                    });
                }
            }
            Action::DeleteStateTransition { machine_id, from, to } => {
                if let Some(sm) = self.state_machines.iter_mut().find(|m| m.id == machine_id) {
                    sm.transitions
                        .retain(|t| !(t.from_state == from && t.to_state == to));
                }
            }
            Action::SetInitialState { machine_id, state_id } => {
                if let Some(sm) = self.state_machines.iter_mut().find(|m| m.id == machine_id) {
                    sm.initial_state = state_id;
                }
            }
            Action::SetStateLoop { machine_id, state_id, looping } => {
                if let Some(sm) = self.state_machines.iter_mut().find(|m| m.id == machine_id) {
                    if let Some(s) = sm.states.iter_mut().find(|s| s.id == state_id) {
                        s.looping = looping;
                    }
                }
            }
        }
    }

    /// Shared helper: push 8 default bones for a humanoid character puppet rig.
    /// Called by both `AutoRigLayer` and `AutoRigWithAi`.
    fn auto_rig_layer(&mut self, layer_id: usize) {
        let defaults: &[(&str, Option<usize>, f32, f32, f32)] = &[
            ("hip",    None,    0.0,   0.0,  40.0),
            ("torso",  Some(0), 0.0,  -40.0, 50.0),
            ("neck",   Some(1), 0.0,  -90.0, 20.0),
            ("head",   Some(2), 0.0, -110.0, 30.0),
            ("l_arm",  Some(1), -30.0, -60.0, 45.0),
            ("r_arm",  Some(1),  30.0, -60.0, 45.0),
            ("l_leg",  Some(0), -20.0,  40.0, 50.0),
            ("r_leg",  Some(0),  20.0,  40.0, 50.0),
        ];

        // Collect base bone ids so child references are stable.
        let base_bone_id = self.rig_bone_counter;

        let bones: Vec<RigBone> = defaults
            .iter()
            .enumerate()
            .map(|(i, &(name, parent_local, x, y, length))| {
                let id = base_bone_id + i;
                RigBone {
                    id,
                    name: name.to_string(),
                    parent_id: parent_local.map(|p| base_bone_id + p),
                    x,
                    y,
                    length,
                    rotation: 0.0,
                    locked: false,
                }
            })
            .collect();

        self.rig_bone_counter += bones.len();

        if let Some(rig) = self.layer_rigs.iter_mut().find(|r| r.layer_id == layer_id) {
            rig.bones.extend(bones);
        } else {
            self.layer_rigs.push(LayerRig {
                layer_id,
                bones,
                ik_targets: Vec::new(),
            });
        }
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn app() -> App {
        App::new()
    }

    // ── Document tests ────────────────────────────────────────────────────────

    #[test]
    fn test_new_app_defaults() {
        let a = app();
        assert_eq!(a.document.width, 1920);
        assert_eq!(a.document.height, 1080);
        assert_eq!(a.document.fps, 24.0);
        assert_eq!(a.document.duration_frames, 240);
        assert_eq!(a.document.name, "Untitled");
        assert!(a.layers.is_empty());
        assert!(a.keyframes.is_empty());
        assert!(!a.playing);
    }

    #[test]
    fn test_set_document_width() {
        let mut a = app();
        a.apply(Action::SetDocumentWidth(3840));
        assert_eq!(a.document.width, 3840);
    }

    #[test]
    fn test_set_document_height() {
        let mut a = app();
        a.apply(Action::SetDocumentHeight(2160));
        assert_eq!(a.document.height, 2160);
    }

    #[test]
    fn test_set_document_fps_clamp_high() {
        let mut a = app();
        a.apply(Action::SetDocumentFps(999.0));
        assert_eq!(a.document.fps, 120.0);
    }

    #[test]
    fn test_set_document_fps_clamp_low() {
        let mut a = app();
        a.apply(Action::SetDocumentFps(0.0));
        assert_eq!(a.document.fps, 1.0);
    }

    #[test]
    fn test_set_document_fps_normal() {
        let mut a = app();
        a.apply(Action::SetDocumentFps(30.0));
        assert_eq!(a.document.fps, 30.0);
    }

    #[test]
    fn test_set_document_duration() {
        let mut a = app();
        a.apply(Action::SetDocumentDuration(480));
        assert_eq!(a.document.duration_frames, 480);
    }

    #[test]
    fn test_set_document_bg() {
        let mut a = app();
        a.apply(Action::SetDocumentBg("#ffffff".to_string()));
        assert_eq!(a.document.background_color, "#ffffff");
    }

    #[test]
    fn test_set_document_name() {
        let mut a = app();
        a.apply(Action::SetDocumentName("My Animation".to_string()));
        assert_eq!(a.document.name, "My Animation");
    }

    // ── Layer tests ───────────────────────────────────────────────────────────

    #[test]
    fn test_add_layer() {
        let mut a = app();
        a.apply(Action::AddLayer { name: "Layer 1".to_string(), kind: LayerKind::Vector });
        assert_eq!(a.layers.len(), 1);
        assert_eq!(a.layers[0].name, "Layer 1");
        assert_eq!(a.active_layer, Some(0));
    }

    #[test]
    fn test_add_multiple_layers() {
        let mut a = app();
        a.apply(Action::AddLayer { name: "A".to_string(), kind: LayerKind::Vector });
        a.apply(Action::AddLayer { name: "B".to_string(), kind: LayerKind::Bitmap });
        assert_eq!(a.layers.len(), 2);
        assert_eq!(a.active_layer, Some(1));
    }

    #[test]
    fn test_delete_layer() {
        let mut a = app();
        a.apply(Action::AddLayer { name: "A".to_string(), kind: LayerKind::Vector });
        let id = a.layers[0].id;
        a.apply(Action::DeleteLayer(id));
        assert!(a.layers.is_empty());
    }

    #[test]
    fn test_delete_layer_removes_keyframes() {
        let mut a = app();
        a.apply(Action::AddLayer { name: "A".to_string(), kind: LayerKind::Vector });
        let id = a.layers[0].id;
        a.apply(Action::AddKeyframe {
            layer_id: id,
            property: "position_x".to_string(),
            frame: 0,
            value: 0.0,
            easing: EasingKind::Linear,
        });
        a.apply(Action::DeleteLayer(id));
        assert!(a.keyframes.is_empty());
    }

    #[test]
    fn test_rename_layer() {
        let mut a = app();
        a.apply(Action::AddLayer { name: "Old".to_string(), kind: LayerKind::Vector });
        let id = a.layers[0].id;
        a.apply(Action::RenameLayer { id, name: "New".to_string() });
        assert_eq!(a.layers[0].name, "New");
    }

    #[test]
    fn test_set_layer_visible() {
        let mut a = app();
        a.apply(Action::AddLayer { name: "A".to_string(), kind: LayerKind::Vector });
        let id = a.layers[0].id;
        a.apply(Action::SetLayerVisible { id, visible: false });
        assert!(!a.layers[0].visible);
    }

    #[test]
    fn test_set_layer_locked() {
        let mut a = app();
        a.apply(Action::AddLayer { name: "A".to_string(), kind: LayerKind::Vector });
        let id = a.layers[0].id;
        a.apply(Action::SetLayerLocked { id, locked: true });
        assert!(a.layers[0].locked);
    }

    #[test]
    fn test_set_layer_solo() {
        let mut a = app();
        a.apply(Action::AddLayer { name: "A".to_string(), kind: LayerKind::Vector });
        let id = a.layers[0].id;
        a.apply(Action::SetLayerSolo { id, solo: true });
        assert!(a.layers[0].solo);
    }

    #[test]
    fn test_set_layer_parent() {
        let mut a = app();
        a.apply(Action::AddLayer { name: "Parent".to_string(), kind: LayerKind::Null });
        a.apply(Action::AddLayer { name: "Child".to_string(), kind: LayerKind::Vector });
        let parent_id = a.layers[0].id;
        let child_id = a.layers[1].id;
        a.apply(Action::SetLayerParent { id: child_id, parent_id: Some(parent_id) });
        assert_eq!(a.layers[1].parent_id, Some(parent_id));
    }

    #[test]
    fn test_reorder_layers() {
        let mut a = app();
        a.apply(Action::AddLayer { name: "A".to_string(), kind: LayerKind::Vector });
        a.apply(Action::AddLayer { name: "B".to_string(), kind: LayerKind::Vector });
        let id_a = a.layers[0].id;
        let id_b = a.layers[1].id;
        a.apply(Action::ReorderLayers(vec![id_b, id_a]));
        assert_eq!(a.layers[0].id, id_b);
        assert_eq!(a.layers[1].id, id_a);
    }

    #[test]
    fn test_set_layer_color_tag() {
        let mut a = app();
        a.apply(Action::AddLayer { name: "A".to_string(), kind: LayerKind::Vector });
        let id = a.layers[0].id;
        a.apply(Action::SetLayerColorTag { id, tag: "red".to_string() });
        assert_eq!(a.layers[0].color_tag, "red");
    }

    #[test]
    fn test_duplicate_layer() {
        let mut a = app();
        a.apply(Action::AddLayer { name: "Original".to_string(), kind: LayerKind::Vector });
        let orig_id = a.layers[0].id;
        a.apply(Action::DuplicateLayer(orig_id));
        assert_eq!(a.layers.len(), 2);
        assert!(a.layers[1].name.contains("copy"));
    }

    #[test]
    fn test_set_layer_start_end_frame() {
        let mut a = app();
        a.apply(Action::AddLayer { name: "A".to_string(), kind: LayerKind::Vector });
        let id = a.layers[0].id;
        a.apply(Action::SetLayerStartFrame { id, frame: 10 });
        a.apply(Action::SetLayerEndFrame { id, frame: 100 });
        assert_eq!(a.layers[0].start_frame, 10);
        assert_eq!(a.layers[0].end_frame, 100);
    }

    #[test]
    fn test_set_active_layer() {
        let mut a = app();
        a.apply(Action::AddLayer { name: "A".to_string(), kind: LayerKind::Vector });
        a.apply(Action::AddLayer { name: "B".to_string(), kind: LayerKind::Vector });
        let id_a = a.layers[0].id;
        a.apply(Action::SetActiveLayer(id_a));
        assert_eq!(a.active_layer, Some(id_a));
    }

    #[test]
    fn test_layer_adds_transform() {
        let mut a = app();
        a.apply(Action::AddLayer { name: "A".to_string(), kind: LayerKind::Vector });
        let id = a.layers[0].id;
        assert!(a.transforms.contains_key(&id));
    }

    // ── Keyframe tests ────────────────────────────────────────────────────────

    #[test]
    fn test_add_keyframe() {
        let mut a = app();
        a.apply(Action::AddLayer { name: "A".to_string(), kind: LayerKind::Vector });
        let lid = a.layers[0].id;
        a.apply(Action::AddKeyframe {
            layer_id: lid,
            property: "position_x".to_string(),
            frame: 5,
            value: 100.0,
            easing: EasingKind::EaseIn,
        });
        assert_eq!(a.keyframes.len(), 1);
        assert_eq!(a.keyframes[0].frame, 5);
        assert_eq!(a.keyframes[0].value, 100.0);
    }

    #[test]
    fn test_delete_keyframe() {
        let mut a = app();
        a.apply(Action::AddLayer { name: "A".to_string(), kind: LayerKind::Vector });
        let lid = a.layers[0].id;
        a.apply(Action::AddKeyframe {
            layer_id: lid,
            property: "position_x".to_string(),
            frame: 5,
            value: 100.0,
            easing: EasingKind::Linear,
        });
        let kid = a.keyframes[0].id;
        a.apply(Action::DeleteKeyframe(kid));
        assert!(a.keyframes.is_empty());
    }

    #[test]
    fn test_move_keyframe() {
        let mut a = app();
        a.apply(Action::AddLayer { name: "A".to_string(), kind: LayerKind::Vector });
        let lid = a.layers[0].id;
        a.apply(Action::AddKeyframe {
            layer_id: lid,
            property: "position_x".to_string(),
            frame: 5,
            value: 0.0,
            easing: EasingKind::Linear,
        });
        let kid = a.keyframes[0].id;
        a.apply(Action::MoveKeyframe { id: kid, frame: 15 });
        assert_eq!(a.keyframes[0].frame, 15);
    }

    #[test]
    fn test_set_keyframe_value() {
        let mut a = app();
        a.apply(Action::AddLayer { name: "A".to_string(), kind: LayerKind::Vector });
        let lid = a.layers[0].id;
        a.apply(Action::AddKeyframe {
            layer_id: lid,
            property: "opacity".to_string(),
            frame: 0,
            value: 1.0,
            easing: EasingKind::Linear,
        });
        let kid = a.keyframes[0].id;
        a.apply(Action::SetKeyframeValue { id: kid, value: 0.5 });
        assert_eq!(a.keyframes[0].value, 0.5);
    }

    #[test]
    fn test_set_keyframe_easing() {
        let mut a = app();
        a.apply(Action::AddLayer { name: "A".to_string(), kind: LayerKind::Vector });
        let lid = a.layers[0].id;
        a.apply(Action::AddKeyframe {
            layer_id: lid,
            property: "position_y".to_string(),
            frame: 0,
            value: 0.0,
            easing: EasingKind::Linear,
        });
        let kid = a.keyframes[0].id;
        a.apply(Action::SetKeyframeEasing { id: kid, easing: EasingKind::Spring });
        assert_eq!(a.keyframes[0].easing, EasingKind::Spring);
    }

    #[test]
    fn test_set_property_at_frame_creates_keyframe() {
        let mut a = app();
        a.apply(Action::AddLayer { name: "A".to_string(), kind: LayerKind::Vector });
        let lid = a.layers[0].id;
        a.apply(Action::SetPropertyAtFrame {
            layer_id: lid,
            property: "scale_x".to_string(),
            frame: 10,
            value: 2.0,
        });
        assert_eq!(a.keyframes.len(), 1);
        assert_eq!(a.keyframes[0].easing, EasingKind::Linear);
    }

    #[test]
    fn test_set_property_at_frame_updates_existing() {
        let mut a = app();
        a.apply(Action::AddLayer { name: "A".to_string(), kind: LayerKind::Vector });
        let lid = a.layers[0].id;
        a.apply(Action::SetPropertyAtFrame {
            layer_id: lid,
            property: "scale_x".to_string(),
            frame: 10,
            value: 2.0,
        });
        a.apply(Action::SetPropertyAtFrame {
            layer_id: lid,
            property: "scale_x".to_string(),
            frame: 10,
            value: 3.0,
        });
        assert_eq!(a.keyframes.len(), 1);
        assert_eq!(a.keyframes[0].value, 3.0);
    }

    // ── Transform tests ───────────────────────────────────────────────────────

    #[test]
    fn test_set_layer_position() {
        let mut a = app();
        a.apply(Action::AddLayer { name: "A".to_string(), kind: LayerKind::Vector });
        let id = a.layers[0].id;
        a.apply(Action::SetLayerPosition { id, x: 100.0, y: 200.0 });
        let t = &a.transforms[&id];
        assert_eq!(t.x, 100.0);
        assert_eq!(t.y, 200.0);
    }

    #[test]
    fn test_set_layer_scale_clamp() {
        let mut a = app();
        a.apply(Action::AddLayer { name: "A".to_string(), kind: LayerKind::Vector });
        let id = a.layers[0].id;
        a.apply(Action::SetLayerScale { id, x: -5.0, y: 999.0 });
        let t = &a.transforms[&id];
        assert_eq!(t.scale_x, 0.001);
        assert_eq!(t.scale_y, 100.0);
    }

    #[test]
    fn test_set_layer_rotation() {
        let mut a = app();
        a.apply(Action::AddLayer { name: "A".to_string(), kind: LayerKind::Vector });
        let id = a.layers[0].id;
        a.apply(Action::SetLayerRotation { id, degrees: 45.0 });
        assert_eq!(a.transforms[&id].rotation, 45.0);
    }

    #[test]
    fn test_set_layer_opacity_clamp() {
        let mut a = app();
        a.apply(Action::AddLayer { name: "A".to_string(), kind: LayerKind::Vector });
        let id = a.layers[0].id;
        a.apply(Action::SetLayerOpacity { id, opacity: 1.5 });
        assert_eq!(a.transforms[&id].opacity, 1.0);
        a.apply(Action::SetLayerOpacity { id, opacity: -0.5 });
        assert_eq!(a.transforms[&id].opacity, 0.0);
    }

    #[test]
    fn test_set_layer_anchor() {
        let mut a = app();
        a.apply(Action::AddLayer { name: "A".to_string(), kind: LayerKind::Vector });
        let id = a.layers[0].id;
        a.apply(Action::SetLayerAnchor { id, x: 50.0, y: 50.0 });
        let t = &a.transforms[&id];
        assert_eq!(t.anchor_x, 50.0);
        assert_eq!(t.anchor_y, 50.0);
    }

    #[test]
    fn test_reset_layer_transform() {
        let mut a = app();
        a.apply(Action::AddLayer { name: "A".to_string(), kind: LayerKind::Vector });
        let id = a.layers[0].id;
        a.apply(Action::SetLayerPosition { id, x: 500.0, y: 500.0 });
        a.apply(Action::ResetLayerTransform(id));
        let t = &a.transforms[&id];
        assert_eq!(t.x, 0.0);
        assert_eq!(t.y, 0.0);
        assert_eq!(t.scale_x, 1.0);
    }

    // ── Playback tests ────────────────────────────────────────────────────────

    #[test]
    fn test_play_pause() {
        let mut a = app();
        a.apply(Action::Play);
        assert!(a.playing);
        a.apply(Action::Pause);
        assert!(!a.playing);
    }

    #[test]
    fn test_stop_resets_frame() {
        let mut a = app();
        a.apply(Action::SetInPoint(10));
        a.apply(Action::SetCurrentFrame(50));
        a.apply(Action::Play);
        a.apply(Action::Stop);
        assert!(!a.playing);
        assert_eq!(a.current_frame, 10);
    }

    #[test]
    fn test_set_current_frame() {
        let mut a = app();
        a.apply(Action::SetCurrentFrame(100));
        assert_eq!(a.current_frame, 100);
    }

    #[test]
    fn test_set_current_frame_clamp() {
        let mut a = app();
        a.apply(Action::SetCurrentFrame(99999));
        assert_eq!(a.current_frame, a.document.duration_frames);
    }

    #[test]
    fn test_toggle_loop() {
        let mut a = app();
        assert!(!a.loop_playback);
        a.apply(Action::ToggleLoop);
        assert!(a.loop_playback);
        a.apply(Action::ToggleLoop);
        assert!(!a.loop_playback);
    }

    #[test]
    fn test_set_in_out_point() {
        let mut a = app();
        a.apply(Action::SetInPoint(10));
        a.apply(Action::SetOutPoint(100));
        assert_eq!(a.in_point, 10);
        assert_eq!(a.out_point, 100);
    }

    #[test]
    fn test_step_forward() {
        let mut a = app();
        a.apply(Action::SetCurrentFrame(5));
        a.apply(Action::StepForward);
        assert_eq!(a.current_frame, 6);
    }

    #[test]
    fn test_step_backward() {
        let mut a = app();
        a.apply(Action::SetCurrentFrame(5));
        a.apply(Action::StepBackward);
        assert_eq!(a.current_frame, 4);
    }

    #[test]
    fn test_step_backward_at_zero() {
        let mut a = app();
        a.apply(Action::SetCurrentFrame(0));
        a.apply(Action::StepBackward);
        assert_eq!(a.current_frame, 0);
    }

    #[test]
    fn test_go_to_first_frame() {
        let mut a = app();
        a.apply(Action::SetCurrentFrame(100));
        a.apply(Action::GoToFirstFrame);
        assert_eq!(a.current_frame, 0);
    }

    #[test]
    fn test_go_to_last_frame() {
        let mut a = app();
        a.apply(Action::GoToLastFrame);
        assert_eq!(a.current_frame, a.document.duration_frames);
    }

    // ── Puppet rig tests ──────────────────────────────────────────────────────

    #[test]
    fn test_add_bone_creates_rig() {
        let mut a = app();
        a.apply(Action::AddLayer { name: "Char".to_string(), kind: LayerKind::Bitmap });
        let lid = a.layers[0].id;
        a.apply(Action::AddBone {
            layer_id: lid,
            parent_id: None,
            x: 0.0,
            y: 0.0,
            length: 50.0,
        });
        assert_eq!(a.layer_rigs.len(), 1);
        assert_eq!(a.layer_rigs[0].bones.len(), 1);
    }

    #[test]
    fn test_add_bone_to_existing_rig() {
        let mut a = app();
        a.apply(Action::AddLayer { name: "Char".to_string(), kind: LayerKind::Bitmap });
        let lid = a.layers[0].id;
        a.apply(Action::AddBone { layer_id: lid, parent_id: None, x: 0.0, y: 0.0, length: 50.0 });
        a.apply(Action::AddBone { layer_id: lid, parent_id: Some(0), x: 0.0, y: -50.0, length: 30.0 });
        assert_eq!(a.layer_rigs[0].bones.len(), 2);
    }

    #[test]
    fn test_delete_bone() {
        let mut a = app();
        a.apply(Action::AddLayer { name: "Char".to_string(), kind: LayerKind::Bitmap });
        let lid = a.layers[0].id;
        a.apply(Action::AddBone { layer_id: lid, parent_id: None, x: 0.0, y: 0.0, length: 50.0 });
        let bone_id = a.layer_rigs[0].bones[0].id;
        a.apply(Action::DeleteBone { layer_id: lid, bone_id });
        assert!(a.layer_rigs[0].bones.is_empty());
    }

    #[test]
    fn test_move_bone() {
        let mut a = app();
        a.apply(Action::AddLayer { name: "Char".to_string(), kind: LayerKind::Bitmap });
        let lid = a.layers[0].id;
        a.apply(Action::AddBone { layer_id: lid, parent_id: None, x: 0.0, y: 0.0, length: 50.0 });
        let bone_id = a.layer_rigs[0].bones[0].id;
        a.apply(Action::MoveBone { layer_id: lid, bone_id, x: 10.0, y: 20.0 });
        let b = &a.layer_rigs[0].bones[0];
        assert_eq!(b.x, 10.0);
        assert_eq!(b.y, 20.0);
    }

    #[test]
    fn test_rotate_bone() {
        let mut a = app();
        a.apply(Action::AddLayer { name: "Char".to_string(), kind: LayerKind::Bitmap });
        let lid = a.layers[0].id;
        a.apply(Action::AddBone { layer_id: lid, parent_id: None, x: 0.0, y: 0.0, length: 50.0 });
        let bone_id = a.layer_rigs[0].bones[0].id;
        a.apply(Action::RotateBone { layer_id: lid, bone_id, rotation: 45.0 });
        assert_eq!(a.layer_rigs[0].bones[0].rotation, 45.0);
    }

    #[test]
    fn test_set_ik_target() {
        let mut a = app();
        a.apply(Action::AddLayer { name: "Char".to_string(), kind: LayerKind::Bitmap });
        let lid = a.layers[0].id;
        a.apply(Action::AddBone { layer_id: lid, parent_id: None, x: 0.0, y: 0.0, length: 50.0 });
        let bone_id = a.layer_rigs[0].bones[0].id;
        a.apply(Action::SetIKTarget { layer_id: lid, bone_id, x: 100.0, y: 200.0 });
        assert_eq!(a.layer_rigs[0].ik_targets.len(), 1);
        assert_eq!(a.layer_rigs[0].ik_targets[0], (bone_id, 100.0, 200.0));
    }

    #[test]
    fn test_auto_rig_layer_creates_8_bones() {
        let mut a = app();
        a.apply(Action::AddLayer { name: "Char".to_string(), kind: LayerKind::Bitmap });
        let lid = a.layers[0].id;
        a.apply(Action::AutoRigLayer(lid));
        assert_eq!(a.layer_rigs.len(), 1);
        assert_eq!(a.layer_rigs[0].bones.len(), 8);
    }

    #[test]
    fn test_auto_rig_with_ai_creates_8_bones() {
        let mut a = app();
        a.apply(Action::AddLayer { name: "Char".to_string(), kind: LayerKind::Bitmap });
        let lid = a.layers[0].id;
        a.apply(Action::AutoRigWithAi(lid));
        assert_eq!(a.layer_rigs[0].bones.len(), 8);
    }

    #[test]
    fn test_auto_rig_bone_names() {
        let mut a = app();
        a.apply(Action::AddLayer { name: "Char".to_string(), kind: LayerKind::Bitmap });
        let lid = a.layers[0].id;
        a.apply(Action::AutoRigLayer(lid));
        let names: Vec<&str> =
            a.layer_rigs[0].bones.iter().map(|b| b.name.as_str()).collect();
        assert!(names.contains(&"hip"));
        assert!(names.contains(&"head"));
        assert!(names.contains(&"l_arm"));
        assert!(names.contains(&"r_arm"));
    }

    // ── AI tests ──────────────────────────────────────────────────────────────

    #[test]
    fn test_set_ai_motion_prompt() {
        let mut a = app();
        a.apply(Action::SetAiMotionPrompt("walk cycle".to_string()));
        assert_eq!(a.ai_motion_prompt, "walk cycle");
    }

    #[test]
    fn test_generate_ai_motion_adds_result() {
        let mut a = app();
        a.apply(Action::AddLayer { name: "A".to_string(), kind: LayerKind::Vector });
        let lid = a.layers[0].id;
        a.apply(Action::SetAiMotionPrompt("bounce".to_string()));
        a.apply(Action::GenerateAiMotion { layer_id: lid });
        assert_eq!(a.ai_motion_results.len(), 1);
        assert_eq!(a.ai_motion_results[0].0, lid);
    }

    #[test]
    fn test_generate_ai_motion_creates_keyframes() {
        let mut a = app();
        a.apply(Action::AddLayer { name: "A".to_string(), kind: LayerKind::Vector });
        let lid = a.layers[0].id;
        a.apply(Action::GenerateAiMotion { layer_id: lid });
        // 6 keyframes on position_x
        let kfs: Vec<_> = a.keyframes.iter().filter(|k| k.property == "position_x").collect();
        assert_eq!(kfs.len(), 6);
        assert_eq!(kfs[0].frame, 0);
        assert_eq!(kfs[1].frame, 5);
    }

    #[test]
    fn test_start_ai_lipsync() {
        let mut a = app();
        a.apply(Action::AddLayer { name: "A".to_string(), kind: LayerKind::Bitmap });
        let lid = a.layers[0].id;
        a.apply(Action::StartAiLipSync { layer_id: lid, audio_path: "/tmp/voice.wav".to_string() });
        assert_eq!(a.ai_lipsync_jobs.len(), 1);
        assert_eq!(a.ai_lipsync_jobs[0].1, "/tmp/voice.wav");
    }

    #[test]
    fn test_complete_ai_lipsync_removes_job_and_adds_keyframes() {
        let mut a = app();
        a.apply(Action::AddLayer { name: "A".to_string(), kind: LayerKind::Bitmap });
        let lid = a.layers[0].id;
        a.apply(Action::StartAiLipSync { layer_id: lid, audio_path: "/tmp/voice.wav".to_string() });
        a.apply(Action::CompleteAiLipSync { layer_id: lid });
        assert!(a.ai_lipsync_jobs.is_empty());
        let mouth_kfs: Vec<_> =
            a.keyframes.iter().filter(|k| k.property == "mouth_open").collect();
        assert_eq!(mouth_kfs.len(), 12);
    }

    #[test]
    fn test_request_ai_interpolation() {
        let mut a = app();
        a.apply(Action::AddLayer { name: "A".to_string(), kind: LayerKind::Vector });
        let lid = a.layers[0].id;
        a.apply(Action::RequestAiInterpolation { layer_id: lid, from_frame: 0, to_frame: 24 });
        assert_eq!(a.ai_interpolation_queue.len(), 1);
        assert_eq!(a.ai_interpolation_queue[0], (lid, 0, 24));
    }

    #[test]
    fn test_complete_ai_interpolation_removes_from_queue() {
        let mut a = app();
        a.apply(Action::AddLayer { name: "A".to_string(), kind: LayerKind::Vector });
        let lid = a.layers[0].id;
        a.apply(Action::RequestAiInterpolation { layer_id: lid, from_frame: 0, to_frame: 24 });
        a.apply(Action::CompleteAiInterpolation { layer_id: lid });
        assert!(a.ai_interpolation_queue.is_empty());
    }

    // ── Export tests ──────────────────────────────────────────────────────────

    #[test]
    fn test_set_export_format() {
        let mut a = app();
        a.apply(Action::SetExportFormat(ExportFormat::Gif));
        assert_eq!(a.export_config.format, ExportFormat::Gif);
    }

    #[test]
    fn test_set_export_fps() {
        let mut a = app();
        a.apply(Action::SetExportFps(60.0));
        assert_eq!(a.export_config.fps, 60.0);
    }

    #[test]
    fn test_set_export_scale_clamp() {
        let mut a = app();
        a.apply(Action::SetExportScale(10.0));
        assert_eq!(a.export_config.scale, 4.0);
        a.apply(Action::SetExportScale(0.0));
        assert_eq!(a.export_config.scale, 0.1);
    }

    #[test]
    fn test_set_export_quality_clamp() {
        let mut a = app();
        a.apply(Action::SetExportQuality(200));
        assert_eq!(a.export_config.quality, 100);
    }

    #[test]
    fn test_set_export_transparent() {
        let mut a = app();
        a.apply(Action::SetExportTransparent(true));
        assert!(a.export_config.transparent);
    }

    #[test]
    fn test_set_export_path() {
        let mut a = app();
        a.apply(Action::SetExportPath("/output/animation.mp4".to_string()));
        assert_eq!(a.export_config.output_path, "/output/animation.mp4");
    }

    #[test]
    fn test_start_cancel_export() {
        let mut a = app();
        a.apply(Action::StartExport);
        assert!(a.export_in_progress);
        a.apply(Action::CancelExport);
        assert!(!a.export_in_progress);
    }

    #[test]
    fn test_set_export_start_end_frame() {
        let mut a = app();
        a.apply(Action::SetExportStartFrame(5));
        a.apply(Action::SetExportEndFrame(120));
        assert_eq!(a.export_config.start_frame, 5);
        assert_eq!(a.export_config.end_frame, 120);
    }

    #[test]
    fn test_export_lottie_format() {
        let mut a = app();
        a.apply(Action::SetExportFormat(ExportFormat::LottieJson));
        assert_eq!(a.export_config.format, ExportFormat::LottieJson);
    }

    // ── State machine tests ───────────────────────────────────────────────────

    #[test]
    fn test_create_state_machine() {
        let mut a = app();
        a.apply(Action::CreateStateMachine("Main".to_string()));
        assert_eq!(a.state_machines.len(), 1);
        assert_eq!(a.state_machines[0].name, "Main");
    }

    #[test]
    fn test_add_animation_state() {
        let mut a = app();
        a.apply(Action::CreateStateMachine("Main".to_string()));
        let mid = a.state_machines[0].id;
        a.apply(Action::AddAnimationState {
            machine_id: mid,
            name: "Idle".to_string(),
            start_frame: 0,
            end_frame: 60,
        });
        assert_eq!(a.state_machines[0].states.len(), 1);
        assert_eq!(a.state_machines[0].states[0].name, "Idle");
    }

    #[test]
    fn test_delete_animation_state() {
        let mut a = app();
        a.apply(Action::CreateStateMachine("Main".to_string()));
        let mid = a.state_machines[0].id;
        a.apply(Action::AddAnimationState {
            machine_id: mid,
            name: "Idle".to_string(),
            start_frame: 0,
            end_frame: 60,
        });
        let sid = a.state_machines[0].states[0].id;
        a.apply(Action::DeleteAnimationState { machine_id: mid, state_id: sid });
        assert!(a.state_machines[0].states.is_empty());
    }

    #[test]
    fn test_add_state_transition() {
        let mut a = app();
        a.apply(Action::CreateStateMachine("Main".to_string()));
        let mid = a.state_machines[0].id;
        a.apply(Action::AddAnimationState {
            machine_id: mid,
            name: "Idle".to_string(),
            start_frame: 0,
            end_frame: 60,
        });
        a.apply(Action::AddAnimationState {
            machine_id: mid,
            name: "Walk".to_string(),
            start_frame: 61,
            end_frame: 120,
        });
        let sid0 = a.state_machines[0].states[0].id;
        let sid1 = a.state_machines[0].states[1].id;
        a.apply(Action::AddStateTransition {
            machine_id: mid,
            from: sid0,
            to: sid1,
            trigger: StateTransitionTrigger::OnClick,
            duration: 5,
        });
        assert_eq!(a.state_machines[0].transitions.len(), 1);
        assert_eq!(a.state_machines[0].transitions[0].duration_frames, 5);
    }

    #[test]
    fn test_delete_state_transition() {
        let mut a = app();
        a.apply(Action::CreateStateMachine("Main".to_string()));
        let mid = a.state_machines[0].id;
        a.apply(Action::AddAnimationState {
            machine_id: mid,
            name: "Idle".to_string(),
            start_frame: 0,
            end_frame: 60,
        });
        a.apply(Action::AddAnimationState {
            machine_id: mid,
            name: "Walk".to_string(),
            start_frame: 61,
            end_frame: 120,
        });
        let sid0 = a.state_machines[0].states[0].id;
        let sid1 = a.state_machines[0].states[1].id;
        a.apply(Action::AddStateTransition {
            machine_id: mid,
            from: sid0,
            to: sid1,
            trigger: StateTransitionTrigger::OnHover,
            duration: 3,
        });
        a.apply(Action::DeleteStateTransition { machine_id: mid, from: sid0, to: sid1 });
        assert!(a.state_machines[0].transitions.is_empty());
    }

    #[test]
    fn test_set_initial_state() {
        let mut a = app();
        a.apply(Action::CreateStateMachine("Main".to_string()));
        let mid = a.state_machines[0].id;
        a.apply(Action::AddAnimationState {
            machine_id: mid,
            name: "Walk".to_string(),
            start_frame: 0,
            end_frame: 60,
        });
        let sid = a.state_machines[0].states[0].id;
        a.apply(Action::SetInitialState { machine_id: mid, state_id: sid });
        assert_eq!(a.state_machines[0].initial_state, sid);
    }

    #[test]
    fn test_set_state_loop() {
        let mut a = app();
        a.apply(Action::CreateStateMachine("Main".to_string()));
        let mid = a.state_machines[0].id;
        a.apply(Action::AddAnimationState {
            machine_id: mid,
            name: "Idle".to_string(),
            start_frame: 0,
            end_frame: 60,
        });
        let sid = a.state_machines[0].states[0].id;
        a.apply(Action::SetStateLoop { machine_id: mid, state_id: sid, looping: true });
        assert!(a.state_machines[0].states[0].looping);
    }

    #[test]
    fn test_state_machine_on_condition_trigger() {
        let mut a = app();
        a.apply(Action::CreateStateMachine("Logic".to_string()));
        let mid = a.state_machines[0].id;
        a.apply(Action::AddAnimationState {
            machine_id: mid,
            name: "S1".to_string(),
            start_frame: 0,
            end_frame: 30,
        });
        a.apply(Action::AddAnimationState {
            machine_id: mid,
            name: "S2".to_string(),
            start_frame: 31,
            end_frame: 60,
        });
        let s1 = a.state_machines[0].states[0].id;
        let s2 = a.state_machines[0].states[1].id;
        a.apply(Action::AddStateTransition {
            machine_id: mid,
            from: s1,
            to: s2,
            trigger: StateTransitionTrigger::OnCondition("speed > 0".to_string()),
            duration: 2,
        });
        let t = &a.state_machines[0].transitions[0];
        assert_eq!(t.trigger, StateTransitionTrigger::OnCondition("speed > 0".to_string()));
    }

    #[test]
    fn test_delete_state_removes_transitions() {
        let mut a = app();
        a.apply(Action::CreateStateMachine("M".to_string()));
        let mid = a.state_machines[0].id;
        a.apply(Action::AddAnimationState {
            machine_id: mid,
            name: "S0".to_string(),
            start_frame: 0,
            end_frame: 30,
        });
        a.apply(Action::AddAnimationState {
            machine_id: mid,
            name: "S1".to_string(),
            start_frame: 31,
            end_frame: 60,
        });
        let s0 = a.state_machines[0].states[0].id;
        let s1 = a.state_machines[0].states[1].id;
        a.apply(Action::AddStateTransition {
            machine_id: mid,
            from: s0,
            to: s1,
            trigger: StateTransitionTrigger::OnComplete,
            duration: 0,
        });
        a.apply(Action::DeleteAnimationState { machine_id: mid, state_id: s0 });
        assert!(a.state_machines[0].transitions.is_empty());
    }

    // ── Integration / cross-feature tests ─────────────────────────────────────

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

    #[test]
    fn test_layer_kind_camera() {
        let mut a = app();
        a.apply(Action::AddLayer { name: "Camera".to_string(), kind: LayerKind::Camera });
        assert_eq!(a.layers[0].kind, LayerKind::Camera);
    }

    #[test]
    fn test_layer_kind_audio() {
        let mut a = app();
        a.apply(Action::AddLayer { name: "VO".to_string(), kind: LayerKind::Audio });
        assert_eq!(a.layers[0].kind, LayerKind::Audio);
    }

    #[test]
    fn test_layer_kind_null() {
        let mut a = app();
        a.apply(Action::AddLayer { name: "Ctrl".to_string(), kind: LayerKind::Null });
        assert_eq!(a.layers[0].kind, LayerKind::Null);
    }

    #[test]
    fn test_multiple_rigs_different_layers() {
        let mut a = app();
        a.apply(Action::AddLayer { name: "Char1".to_string(), kind: LayerKind::Bitmap });
        a.apply(Action::AddLayer { name: "Char2".to_string(), kind: LayerKind::Bitmap });
        let l0 = a.layers[0].id;
        let l1 = a.layers[1].id;
        a.apply(Action::AutoRigLayer(l0));
        a.apply(Action::AutoRigLayer(l1));
        assert_eq!(a.layer_rigs.len(), 2);
        assert_eq!(a.layer_rigs[0].layer_id, l0);
        assert_eq!(a.layer_rigs[1].layer_id, l1);
    }

    #[test]
    fn test_keyframe_counter_monotonic() {
        let mut a = app();
        a.apply(Action::AddLayer { name: "A".to_string(), kind: LayerKind::Vector });
        let lid = a.layers[0].id;
        for i in 0..10 {
            a.apply(Action::AddKeyframe {
                layer_id: lid,
                property: "scale".to_string(),
                frame: i,
                value: 1.0,
                easing: EasingKind::Hold,
            });
        }
        let ids: Vec<usize> = a.keyframes.iter().map(|k| k.id).collect();
        for i in 1..ids.len() {
            assert!(ids[i] > ids[i - 1]);
        }
    }

    #[test]
    fn test_easing_bezier_variant() {
        let mut a = app();
        a.apply(Action::AddLayer { name: "A".to_string(), kind: LayerKind::Vector });
        let lid = a.layers[0].id;
        a.apply(Action::AddKeyframe {
            layer_id: lid,
            property: "rotation".to_string(),
            frame: 0,
            value: 0.0,
            easing: EasingKind::Bezier,
        });
        assert_eq!(a.keyframes[0].easing, EasingKind::Bezier);
    }

    #[test]
    fn test_export_webm_format() {
        let mut a = app();
        a.apply(Action::SetExportFormat(ExportFormat::WebM));
        assert_eq!(a.export_config.format, ExportFormat::WebM);
    }

    #[test]
    fn test_export_spritesheet_format() {
        let mut a = app();
        a.apply(Action::SetExportFormat(ExportFormat::Spritesheet));
        assert_eq!(a.export_config.format, ExportFormat::Spritesheet);
    }

    #[test]
    fn test_ik_target_update_in_place() {
        let mut a = app();
        a.apply(Action::AddLayer { name: "C".to_string(), kind: LayerKind::Bitmap });
        let lid = a.layers[0].id;
        a.apply(Action::AddBone { layer_id: lid, parent_id: None, x: 0.0, y: 0.0, length: 50.0 });
        let bid = a.layer_rigs[0].bones[0].id;
        a.apply(Action::SetIKTarget { layer_id: lid, bone_id: bid, x: 10.0, y: 20.0 });
        a.apply(Action::SetIKTarget { layer_id: lid, bone_id: bid, x: 30.0, y: 40.0 });
        assert_eq!(a.layer_rigs[0].ik_targets.len(), 1);
        assert_eq!(a.layer_rigs[0].ik_targets[0], (bid, 30.0, 40.0));
    }

    #[test]
    fn test_in_point_clamped_to_out_point() {
        let mut a = app();
        a.apply(Action::SetOutPoint(50));
        a.apply(Action::SetInPoint(200));
        assert!(a.in_point <= a.out_point);
    }

    #[test]
    fn test_layer_default_visible_unlocked() {
        let mut a = app();
        a.apply(Action::AddLayer { name: "A".to_string(), kind: LayerKind::Vector });
        assert!(a.layers[0].visible);
        assert!(!a.layers[0].locked);
        assert!(!a.layers[0].solo);
    }

    #[test]
    fn test_delete_nonexistent_layer_noop() {
        let mut a = app();
        a.apply(Action::DeleteLayer(999));
        assert!(a.layers.is_empty());
    }

    #[test]
    fn test_ai_motion_prompt_default_empty() {
        let a = app();
        assert!(a.ai_motion_prompt.is_empty());
    }

    #[test]
    fn test_export_default_config() {
        let a = app();
        assert_eq!(a.export_config.format, ExportFormat::Mp4);
        assert_eq!(a.export_config.fps, 24.0);
        assert_eq!(a.export_config.scale, 1.0);
        assert_eq!(a.export_config.quality, 85);
        assert!(!a.export_config.transparent);
    }

    #[test]
    fn test_state_machine_multiple() {
        let mut a = app();
        a.apply(Action::CreateStateMachine("SM1".to_string()));
        a.apply(Action::CreateStateMachine("SM2".to_string()));
        assert_eq!(a.state_machines.len(), 2);
        assert_ne!(a.state_machines[0].id, a.state_machines[1].id);
    }

    #[test]
    fn test_layer_z_order_assigned() {
        let mut a = app();
        a.apply(Action::AddLayer { name: "A".to_string(), kind: LayerKind::Vector });
        a.apply(Action::AddLayer { name: "B".to_string(), kind: LayerKind::Vector });
        assert_eq!(a.layers[0].z_order, 0);
        assert_eq!(a.layers[1].z_order, 1);
    }

    #[test]
    fn test_guide_layer_kind() {
        let mut a = app();
        a.apply(Action::AddLayer { name: "Guide".to_string(), kind: LayerKind::Guide });
        assert_eq!(a.layers[0].kind, LayerKind::Guide);
    }

    #[test]
    fn test_set_layer_parent_to_none() {
        let mut a = app();
        a.apply(Action::AddLayer { name: "A".to_string(), kind: LayerKind::Null });
        a.apply(Action::AddLayer { name: "B".to_string(), kind: LayerKind::Vector });
        let pa = a.layers[0].id;
        let pb = a.layers[1].id;
        a.apply(Action::SetLayerParent { id: pb, parent_id: Some(pa) });
        a.apply(Action::SetLayerParent { id: pb, parent_id: None });
        assert_eq!(a.layers[1].parent_id, None);
    }
}
