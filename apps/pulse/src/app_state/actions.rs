//! The [`Action`] enum: every panel→state mutation Pulse panels can emit.
//!
//! The root view routes each `Action` into [`super::App::apply`], the single
//! choke point that mutates state. Adding a new interaction means adding a
//! variant here plus a matching `apply` arm — that is the entire contract.

use std::path::PathBuf;

use crate::comp::{BrowserEntry, Interp, Prop};
use crate::effect_params::EffectStack;
use crate::gpui_effects::GpuiEffectKind;
use crate::gizmo::Handle as GizmoHandle;

use super::{
    Tool,
    ExprControlKind, ExprControlValue, ExprLang,
    TextAnimPreset, MogrParam, MogrTemplate,
    MotionSketchStroke, StabilizeResult, StabilizeMethod, StabilizeFraming,
    ShapeMorphKeyframe, MorphMode, CorrespondenceMode,
    AudioVisMode, AudioVisSide,
    TrackMatteConfig, MatteMode,
    RenderQueueItem, RenderOutputFormat,
    PuppetPinMode,
    DepthOfField,
    EchoConfig,
    MoGrtControl,
};

/// Every panel->state mutation a panel can request. Panels emit these; the root
/// view routes each into [`super::App::apply`]. EXTENSIBLE: later waves add variants
/// here and a matching arm in `apply` — that is the entire contract a parallel
/// agent touches when wiring a new interaction.
// Several variants are wired in `apply` but not yet emitted by a stub panel;
// they are the seams parallel agents fill in. Keep them rather than churn.
#[allow(dead_code)]
#[derive(Clone, Debug)]
pub enum Action {
    /// Select the active tool (tools strip / toolbar).
    SetTool(Tool),

    // --- Welcome screen ---
    /// Dismiss the welcome screen and proceed to the main app.
    DismissWelcome,
    /// Dismiss the welcome screen and start a new composition.
    NewComposition,
    /// Dismiss the welcome screen and open an existing project.
    OpenProject,

    // --- Transport ---
    /// Set the playhead to an absolute time in seconds (re-renders the preview).
    SetTime(f32),
    /// Advance/rewind the playhead by `delta` seconds (re-renders the preview).
    StepTime(f32),
    /// Jump the playhead to the start of the active comp (re-renders).
    GoToStart,
    /// Jump the playhead to the end (duration) of the active comp.
    GoToEnd,
    /// Toggle the looping flag (playback loops at work-area end when on).
    ToggleLoop,
    /// Toggle play/pause. Starting play resets the wall-clock so the first tick
    /// advances by a small delta rather than the time since the last play.
    TogglePlay,
    /// Stop playback (without moving the playhead).
    Pause,

    // --- Layers ---
    /// Toggle a layer's visibility by index in the active comp (re-renders).
    ToggleLayerVisible(usize),
    /// Make a layer (by index) the selected layer (pure UI state).
    SelectLayer(usize),

    // --- Properties ---
    /// Set the **selected** layer's transform property `prop` to `value` at the
    /// current playhead time (re-renders).
    SetTransform(Prop, f32),
    /// Toggle animation for the selected layer's transform property `prop` (the
    /// per-property stopwatch, like After Effects).
    ToggleKeyframe(Prop),
    /// Move the keyframe at index `key_index` of the selected layer's `prop`
    /// track to a new absolute time (seconds).
    MoveKeyframe {
        prop: Prop,
        key_index: usize,
        time: f32,
    },

    // --- Effects ---
    /// Toggle the Effects & Presets browser open/closed (pure UI state).
    ToggleEffectBrowser,
    /// Set the effect browser's search query (pure UI state).
    SetEffectQuery(String),
    /// Add the effect described by `entry` to the selected layer's matching stack.
    AddEffect(BrowserEntry),
    /// Remove the effect at `index` from `stack` of the selected layer.
    RemoveEffect { stack: EffectStack, index: usize },
    /// Set the scalar parameter `param` of the effect at `index` in `stack`.
    SetEffectParam {
        stack: EffectStack,
        index: usize,
        param: usize,
        value: f32,
    },

    // --- Work area (loop / render region) ---
    /// Set the active comp's work-area start (in-point) to an absolute time in seconds.
    SetWorkAreaStart(f32),
    /// Set the active comp's work-area end (out-point) to an absolute time in seconds.
    SetWorkAreaEnd(f32),
    /// Reset the active comp's work area to span the whole `[0, duration]` timeline.
    ResetWorkArea,

    // --- Graph editor ---
    /// Toggle the graph (value-curve) editor on the timeline. Pure UI state.
    ToggleGraph,
    /// Toggle a property's visibility in the graph plot. Pure UI state.
    ToggleGraphProp(Prop),
    /// Clear the graph's explicit property selection (show all keyed props). Pure UI state.
    ClearGraphProps,
    /// Set the interpolation mode of key `key_index` on the selected layer's `prop` track.
    SetInterp {
        prop: Prop,
        key_index: usize,
        interp: Interp,
    },
    /// Move keyframe `key_index` of the selected layer's `prop` track to a new time and value.
    MoveKeyframeXY {
        prop: Prop,
        key_index: usize,
        time: f32,
        value: f32,
    },

    // --- 3D layer controls ---
    /// Enable or disable the 3-D layer flag on the layer at `layer_id` (index).
    SetLayer3D(usize, bool),
    /// Set the Z position of the layer at `layer_id`.
    SetPositionZ(usize, f32),
    /// Set the X/Y/Z orientation (Euler degrees) of the layer at `layer_id`.
    Set3DRotation(usize, f32, f32, f32),

    // --- Gizmo hover / modifier state ---
    /// Record which gizmo handle the pointer is currently hovering over.
    SetHoveredGizmoHandle(Option<GizmoHandle>),
    /// Duplicate the selected layer then immediately begin a gizmo drag.
    DuplicateLayer(usize),

    // --- Expressions ---
    /// Set (or clear when `expr` is empty) the expression string for `prop` on `layer_id`.
    SetExpression { layer_id: usize, prop: String, expr: String },

    // --- Preview transform gizmo ---
    /// Key the changed transform properties of the selected layer at `time` from a gizmo drag.
    GizmoKeys { time: f32, keys: Vec<(Prop, f32)> },

    // --- History ---
    /// Undo the last undoable edit.
    Undo,
    /// Redo the last undone edit.
    Redo,

    // --- Wave 9: MP4 export ---
    /// Export the active comp to an MP4 at `path`.
    ExportMp4(PathBuf),
    /// Export the active comp to an animated GIF at `path`.
    ExportGif(PathBuf),

    // --- Wave 9: Audio preview stub ---
    /// Toggle the audio preview stub on/off.
    ToggleAudioPreview,
    /// Set the audio preview volume (0.0–1.5).
    SetAudioVolume(f32),

    // --- Wave 9: GPUI-side effects ---
    /// Add a GPUI-side post-process effect of the given kind to the selected layer.
    AddGpuiEffect(GpuiEffectKind),
    /// Remove the GPUI-side effect at index `i` from the selected layer.
    RemoveGpuiEffect(usize),
    /// Update the block size of a Mosaic effect at index `i` on the selected layer.
    SetMosaicBlock { effect_idx: usize, block: u32 },
    /// Update the offset of a ChromaticAberration effect at index `i`.
    SetChromaOffset { effect_idx: usize, offset: i32 },
    /// Update the intensity of a Vignette or Noise effect at index `i`.
    SetEffectIntensity { effect_idx: usize, intensity: f32 },
    /// Update the radius of a Vignette effect at index `i`.
    SetVignetteRadius { effect_idx: usize, radius: f32 },
    /// Toggle the expand/collapse state of a GPUI-side effect card at `effect_idx`.
    ToggleGpuiEffectExpand(usize),

    // --- Wave 9: Render queue ---
    /// Add the active comp (with a file-picker output path) to the render queue.
    AddToRenderQueue,
    /// Process all pending render queue jobs serially.
    RenderAll,
    /// Remove the job at index `i` from the render queue.
    RemoveFromRenderQueue(usize),

    // --- Wave 9: Composition settings dialog ---
    /// Open or close the composition settings dialog.
    ToggleCompSettings,
    /// Set a pending comp-setting field (does NOT apply until ApplyCompSettings).
    SetPendingCompWidth(u32),
    SetPendingCompHeight(u32),
    SetPendingCompFps(f32),
    SetPendingCompDuration(f32),
    SetPendingCompBgColor([f32; 4]),
    /// Apply the pending comp settings to the active comp and resize host buffers.
    ApplyCompSettings,

    // --- Wave 11: RAM preview ---
    BuildRamPreview,
    PlayRamPreview,
    PurgeRamPreview,
    /// Clear the host-side RAM preview frame cache.
    ClearRamPreview,

    // --- Wave 11: layer parenting ---
    /// Set layer `child` to have `parent` as its GPUI-side parent.
    SetParent(usize, usize),
    /// Clear the GPUI-side parent of layer `child`.
    ClearParent(usize),
    /// Arm or disarm the pick-whip: `Some(i)` means layer i is waiting to pick.
    SetPickingParent(Option<usize>),

    // --- Wave 11: pre-compose ---
    /// Move `indices` layers into a new SubComp named `name`, leaving a Null placeholder.
    PreCompose(Vec<usize>, String),
    /// Enter the sub-comp at index `idx` in the layers panel.
    OpenSubComp(usize),
    /// Return to the main comp from a sub-comp view.
    CloseSubComp,

    // --- Wave 11: null object ---
    /// Add a new Null layer to the active comp.
    AddNullLayer,
    /// Add a Guide layer to the active comp (skipped during compositing).
    AddGuideLayer,

    // --- Wave 11: solid + adjustment layers ---
    /// Add a Solid layer with the given RGBA8 color to the active comp.
    AddSolidLayer([u8; 4]),
    /// Add an Adjustment layer to the active comp.
    AddAdjustmentLayer,

    // --- Wave 12: video footage import ---
    /// Open a file dialog to pick a video clip and add it as a Footage layer.
    ImportVideoFootage,
    /// Set the footage source for the selected layer to a video file at `path`.
    SetFootageVideo {
        layer_idx: usize,
        path: std::path::PathBuf,
        fps: f64,
        frame_count: u64,
        width: u32,
        height: u32,
    },

    // --- Wave 11: new GPUI effect param setters ---
    SetColorBalanceShadows { effect_idx: usize, channel: usize, value: f32 },
    SetColorBalanceMidtones { effect_idx: usize, channel: usize, value: f32 },
    SetColorBalanceHighlights { effect_idx: usize, channel: usize, value: f32 },
    SetLevelsInBlack { effect_idx: usize, value: u8 },
    SetLevelsInWhite { effect_idx: usize, value: u8 },
    SetLevelsGamma { effect_idx: usize, value: f32 },
    SetLevelsOutBlack { effect_idx: usize, value: u8 },
    SetLevelsOutWhite { effect_idx: usize, value: u8 },
    SetHueShift { effect_idx: usize, value: f32 },
    SetSaturation { effect_idx: usize, value: f32 },
    SetLightness { effect_idx: usize, value: f32 },
    SetNoiseFrequency { effect_idx: usize, value: f32 },
    SetNoiseEvolution { effect_idx: usize, value: f32 },

    // --- Wave 13: Comp + layer markers ---
    /// Add a marker to the active composition at `time` seconds.
    AddCompMarker { time: f32, label: String },
    /// Remove comp marker by index.
    RemoveCompMarker(usize),
    /// Add a marker to a specific layer at `time` seconds.
    AddLayerMarker { layer_idx: usize, time: f32, label: String },
    /// Remove a layer marker by layer + marker index.
    RemoveLayerMarker { layer_idx: usize, idx: usize },

    // --- Wave 13: Region of interest ---
    /// Set the region of interest for the preview render (doc px `[x,y,w,h]`).
    SetROI(Option<[f32; 4]>),

    // --- Wave 14: Anchor point, displacement map, expression controls, ProRes, presets ---
    /// Set anchor point for the selected layer (normalized 0..1 per axis).
    SetAnchorPoint { layer_idx: usize, x: f32, y: f32 },
    /// Add a Displacement Map distort effect to the selected layer.
    AddDisplacementMap { layer_idx: usize, map_layer: usize, scale_x: f32, scale_y: f32 },
    /// Set displacement map scale.
    SetDisplaceScale { layer_idx: usize, effect_idx: usize, scale_x: f32, scale_y: f32 },
    /// Add an expression control layer of the given kind.
    AddExpressionControl(ExprControlKind),
    /// Set the value of an expression control layer.
    SetExprControlValue { layer_idx: usize, value: ExprControlValue },
    /// Export current comp as ProRes via FFmpeg.
    ExportProRes(PathBuf),
    /// Export current comp as DNxHD via FFmpeg.
    ExportDnxHD(PathBuf),
    /// Save current render settings as a named output preset.
    SaveOutputPreset(String),
    /// Load (apply) a saved output preset by index.
    LoadOutputPreset(usize),
    /// Delete a saved output preset by index.
    DeleteOutputPreset(usize),

    // --- Batch 1: 3D Camera stub ---
    /// Set the active comp's camera position (comp-space XYZ).
    SetCameraPosition([f32; 3]),
    /// Set the active comp's camera field of view in degrees.
    SetCameraFov(f32),

    // --- Batch 1: Layer parenting (extended) ---
    /// Set the selected layer's parent to the layer at `parent_idx`.
    SetParentLayer { child: usize, parent: Option<usize> },

    // --- Batch 3: Time remap ---
    SetTimeRemapEnabled { layer_id: usize, enabled: bool },
    SetTimeRemapKey { layer_id: usize, comp_time: f64, source_time: f64 },

    // --- Batch 3: Track matte ---
    SetLayerMatte { layer_id: usize, mode: crate::comp::MatteMode },

    // --- Batch 3: Puppet pins ---
    AddPuppetPin { layer_id: usize, pos: [f32; 2] },
    MovePuppetPin { layer_id: usize, pin_id: u64, pos: [f32; 2] },
    RemovePuppetPin { layer_id: usize, pin_id: u64 },

    // --- Batch 3: Solo / Shy ---
    ToggleSolo(usize),
    ToggleShy(usize),
    ToggleHideShy,

    // --- Batch 4: 3D Lights ---
    /// Add a light to the active composition.
    AddLight(crate::comp::Light),
    /// Remove the light at index `i` from the active composition.
    RemoveLight(usize),
    /// Replace the light at `index` with `light` in the active composition.
    UpdateLight { index: usize, light: crate::comp::Light },

    // --- Batch 4: Multi-comp render queue ---
    /// Add every comp in the project as a render queue job.
    AddAllCompsToQueue,

    // --- Batch 4: Live preview output ---
    /// Toggle writing each preview frame to /tmp/prism-pulse-preview.rgba.
    ToggleLiveOutput,

    // --- Batch 5: Comp motion blur (shutter) ---
    /// Toggle the active comp's master motion-blur switch.
    SetMotionBlurEnabled(bool),
    /// Set the comp's shutter angle in degrees (clamped to `(0, 720]`).
    SetMotionBlurAngle(f32),
    /// Set the comp's shutter phase in degrees.
    SetMotionBlurPhase(f32),
    /// Set the comp's motion-blur sample count (clamped to `[1, 64]`).
    SetMotionBlurSamples(u32),
    /// Toggle the per-layer motion-blur flag on the layer at `idx`.
    ToggleLayerMotionBlur(usize),

    // --- Batch 6: Text Animator ---
    /// Add a default TextAnimator to the text layer at `layer_id`.
    AddTextAnimator(usize),
    /// Remove the TextAnimator from the text layer at `layer_id`.
    RemoveTextAnimator(usize),
    /// Set the text animator's character range (both clamped to [0,1]).
    SetTextAnimatorRange { layer_id: usize, start: f32, end: f32 },
    /// Set the per-character x offset of the text animator.
    SetTextAnimatorOffsetX { layer_id: usize, value: f32 },
    /// Set the per-character y offset of the text animator.
    SetTextAnimatorOffsetY { layer_id: usize, value: f32 },
    /// Set the per-character rotation (degrees) of the text animator.
    SetTextAnimatorRotation { layer_id: usize, value: f32 },
    /// Set the per-character scale multiplier of the text animator.
    SetTextAnimatorScale { layer_id: usize, value: f32 },
    /// Set the per-character opacity multiplier of the text animator.
    SetTextAnimatorOpacity { layer_id: usize, value: f32 },

    // --- Batch 6: Shape Layer — Trim Paths ---
    /// Set (or replace) Trim Paths on the shape layer at `layer_id`.
    SetShapeTrimPaths { layer_id: usize, start: f32, end: f32, offset: f32 },
    /// Clear Trim Paths from the shape layer at `layer_id`.
    ClearShapeTrimPaths(usize),

    // --- Batch 6: Shape Layer — Repeater ---
    /// Add a default ShapeRepeater to the shape layer at `layer_id`.
    AddShapeRepeater(usize),
    /// Remove the ShapeRepeater from the shape layer at `layer_id`.
    RemoveShapeRepeater(usize),
    /// Set the number of copies on the shape layer's repeater.
    SetRepeaterCopies { layer_id: usize, copies: u32 },
    /// Set the per-copy x/y offset on the shape layer's repeater.
    SetRepeaterOffset { layer_id: usize, x: f32, y: f32 },
    /// Set the per-copy rotation (degrees) on the shape layer's repeater.
    SetRepeaterRotation { layer_id: usize, deg: f32 },
    /// Set the per-copy scale multiplier on the shape layer's repeater.
    SetRepeaterScale { layer_id: usize, scale: f32 },
    /// Set the first/last-copy opacity on the shape layer's repeater.
    SetRepeaterOpacity { layer_id: usize, start: f32, end: f32 },

    // --- Batch 6: Lumetri Color ---
    /// Add (or reset) the Lumetri Color grade on the layer at `layer_id`.
    AddLumetriColor(usize),
    /// Remove the Lumetri Color grade from the layer at `layer_id`.
    RemoveLumetriColor(usize),
    /// Set a single Lumetri Color parameter by name.
    SetLumetriParam { layer_id: usize, param: &'static str, value: f32 },
    /// Toggle the Lumetri Color bypass flag on the layer at `layer_id`.
    ToggleLumetriEnabled(usize),
    /// Reset all Lumetri Color parameters to their defaults.
    ResetLumetriColor(usize),

    // --- Batch 6: Essential Graphics / Motion Graphics Templates ---
    /// Open or close the Essential Graphics panel.
    ToggleMoGrtPanel,
    /// Add a new Motion Graphics Template with the given name.
    AddMoGrtTemplate(String),
    /// Remove the template at `idx` from the MoGrt list.
    RemoveMoGrtTemplate(usize),
    /// Select the template at `idx` in the Essential Graphics panel.
    SelectMoGrtTemplate(usize),
    /// Add a control to the template at `template_idx`.
    AddMoGrtControl { template_idx: usize, control: MoGrtControl },
    /// Remove the control at `control_idx` from template `template_idx`.
    RemoveMoGrtControl { template_idx: usize, control_idx: usize },
    /// Update the text value of a Text control in a template.
    SetMoGrtTextValue { template_idx: usize, control_idx: usize, value: String },
    /// Update the color value of a Color control in a template.
    SetMoGrtColorValue { template_idx: usize, control_idx: usize, value: [f32; 4] },
    /// Update the slider value of a Slider control in a template.
    SetMoGrtSliderValue { template_idx: usize, control_idx: usize, value: f32 },
    /// Export the template at `template_idx` to a JSON file at `path`.
    ExportMoGrt { template_idx: usize, path: PathBuf },

    // --- Batch 2: Layer split ---
    /// Split the layer at `id` at the current playhead time into two layers.
    SplitLayer(usize),
    /// Split the layer at `layer_id` at an explicit `time`.
    SplitLayerAt { layer_id: usize, time: f32 },

    // --- Batch 2: Brainstorm ---
    /// Toggle the Brainstorm variations panel open/closed.
    ToggleBrainstorm,
    /// Generate `count` random keyframe-variation previews from the current comp state.
    GenerateBrainstormVariations { count: u32 },
    /// Mark variation `idx` as the selected preview.
    SelectBrainstormVariation(usize),
    /// Apply variation `idx`: bake its overrides into the comp, then close Brainstorm.
    ApplyBrainstormVariation(usize),
    /// Resize the Brainstorm grid.
    SetBrainstormGrid { cols: u32, rows: u32 },

    // --- Batch 2: Color Finesse ---
    /// Add a Color Finesse grade to the layer at `layer_id`.
    AddColorFinesse(usize),
    /// Remove the Color Finesse grade from the layer at `layer_id`.
    RemoveColorFinesse(usize),
    /// Enable or disable the Color Finesse grade on the layer at `layer_id`.
    SetColorFinesseEnabled { layer_id: usize, enabled: bool },
    /// Set one parameter of the Color Finesse grade.
    SetColorFinesseParam { layer_id: usize, range: &'static str, prop: &'static str, value: f32 },
    /// Reset all Color Finesse parameters to defaults on the layer at `layer_id`.
    ResetColorFinesse(usize),

    // --- Batch 2: Pre-render cache ---
    /// Begin rendering the work area to a PNG frame sequence in a temp dir.
    StartPreRender,
    /// Cancel an in-progress pre-render.
    CancelPreRender,
    /// Update the pre-render progress counters.
    SetPreRenderProgress { frames_done: u32, total: u32 },
    /// Mark pre-render complete.
    PreRenderComplete { frame_count: u32, cache_dir: std::path::PathBuf },
    /// Record a pre-render failure.
    PreRenderFailed(String),
    /// Clear the pre-render cache and reset status.
    ClearPreRenderCache,
    /// Toggle whether the compositor serves frames from the pre-render cache.
    ToggleUsePreRender,

    // --- Batch 2: Track Camera ---
    /// Toggle the Camera Tracker panel open/closed.
    ToggleCameraTracker,
    /// Add a track point at the given position (comp space) to the tracker.
    AddTrackPoint { name: String, pos: [f32; 2] },
    /// Remove the track point at `idx`.
    RemoveTrackPoint(usize),
    /// Record the position of track point `idx` at `time`.
    MoveTrackPoint { idx: usize, time: f32, pos: [f32; 2] },
    /// Solve the camera track from the current set of track points + their keyframes.
    SolveCameraTrack,
    /// Apply the solved camera motion to the comp's camera layer.
    CreateCameraFromTrack,
    /// Update the tracker's progress bar.
    SetCameraTrackerProgress(f32),
    /// Clear all track points and the solved camera keyframes.
    ClearCameraTrack,

    // --- Batch 3 extended: Rotobrush ---
    /// Set whether new rotobrush strokes subtract (background) or add (foreground).
    SetRotobrushMode { subtract: bool },
    /// Set the rotobrush brush radius (clamped to ≥1.0).
    SetRotobrushRadius(f32),
    /// Add a rotobrush stroke to the layer at `layer_id` at `frame`.
    AddRotobrushStroke { layer_id: usize, frame: u32, pts: Vec<[f32; 2]> },
    /// Remove all rotobrush strokes from the layer at `layer_id`.
    ClearRotobrushStrokes { layer_id: usize },
    /// Propagate the rotobrush segmentation `forward_frames` frames ahead.
    PropagateRotobrush { layer_id: usize, forward_frames: u32 },

    // --- Batch 3 extended: Time Stretch ---
    /// Set the time-stretch factor on the layer at `layer_id` (clamped to ≥0.01).
    SetLayerTimeStretch { layer_id: usize, factor: f32 },

    // --- Batch 3 extended: Audio Fades ---
    /// Set per-layer audio fade-in / fade-out durations (both clamped to ≥0.0).
    SetLayerAudioFade { layer_id: usize, fade_in: f32, fade_out: f32 },

    // --- Batch 3 extended: Puppet Pin Stiffness ---
    /// Set the stiffness of a specific puppet pin (clamped to 0.0..=1.0).
    SetPuppetPinStiffness { layer_id: usize, pin_id: u64, stiffness: f32 },
    /// Toggle the `is_stiff` flag on a specific puppet pin.
    TogglePuppetPinStiff { layer_id: usize, pin_id: u64 },
    /// Set the puppet mesh density on the layer at `layer_id` (clamped to ≥2).
    SetPuppetMeshDensity { layer_id: usize, density: u8 },

    // --- Batch 3 extended: Echo Effect ---
    /// Set (or replace) the echo effect on the layer at `layer_id`.
    SetLayerEcho { layer_id: usize, config: EchoConfig },
    /// Remove the echo effect from the layer at `layer_id`.
    ClearLayerEcho { layer_id: usize },

    // --- Batch 4: 3D Camera depth (DoF + rig controls) ---
    SetDepthOfField(DepthOfField),
    SetDofEnabled(bool),
    SetDofFocusDistance(f32),
    SetDofAperture(f32),
    SetDofBlurLevel(f32),
    SetCameraZoom(f32),
    SetCameraPointOfInterest([f32; 3]),
    SetCameraOrbitSpeed(f32),
    ResetCamera,

    // --- Batch 4: Expression Engine depth ---
    SetExpressionEnabled { layer_id: usize, prop: String, enabled: bool },
    AddExpressionError { layer_id: usize, prop: String, error: String },
    ClearExpressionErrors { layer_id: usize },
    SetExpressionLanguage(ExprLang),
    EvaluateExpression { layer_id: usize, prop: String, at_time: f32 },

    // --- Batch 4: Brainstorm depth ---
    SetBrainstormVariationCount(u8),
    ExportBrainstormVariation { idx: usize, path: std::path::PathBuf },
    CompareBrainstormVariations { a: usize, b: usize },
    LockBrainstormVariation(usize),

    // --- Batch 4: Collect Files / Package project ---
    ToggleCollectFilesPanel,
    SetCollectDestination(std::path::PathBuf),
    SetCollectIncludeFootage(bool),
    SetCollectIncludeProxies(bool),
    SetCollectGenerateReport(bool),
    SetCollectReduceProject(bool),
    RunCollectFiles,

    // --- Batch 5: Motion Sketch ---
    SetMotionSketchCaptureSpeed(f32),
    SetMotionSketchSmoothing(f32),
    SetMotionSketchShowWireframe(bool),
    ToggleMotionSketchRecord,
    ApplyMotionSketchStroke(MotionSketchStroke),
    ClearMotionSketchStrokes,
    ApplyMotionSketchToLayer { layer_id: usize },

    // --- Batch 5: Warp Stabilizer depth ---
    SetWarpStabResult(StabilizeResult),
    SetWarpStabSmoothness(f32),
    SetWarpStabMethod(StabilizeMethod),
    SetWarpStabFraming(StabilizeFraming),
    SetWarpStabCropSmooth(f32),
    SetWarpStabDetailedAnalysis(bool),
    SetWarpStabRollingShutter(f32),
    AnalyzeWarpStab { layer_id: usize },
    WarpStabAnalysisComplete,

    // --- Batch 5: Shape Layer Morphing ---
    SetShapeMorphEnabled(bool),
    AddMorphKeyframe(ShapeMorphKeyframe),
    RemoveMorphKeyframe(usize),
    SetMorphMode { kf_idx: usize, mode: MorphMode },
    SetCorrespondenceMode(CorrespondenceMode),
    SetMorphPreviewTime(f32),
    PreviewMorphAtTime(f32),
    ClearMorphKeyframes,

    // --- Batch 5: Audio Spectrum / Waveform Effects ---
    SetAudioVisMode(AudioVisMode),
    SetAudioVisLayer(Option<usize>),
    SetAudioStartFreq(f32),
    SetAudioEndFreq(f32),
    SetAudioMaxHeight(f32),
    SetAudioVisSide(AudioVisSide),
    SetAudioSoftness(f32),
    SetAudioMirror(bool),
    SetAudioDisplayedSamples(u32),
    SetAudioFrequencyBands(u32),
    SetAudioThickness(f32),
    SetAudioDigital(bool),
    ApplyAudioSpectrumEffect { layer_id: usize },

    // --- Batch 6 depth: Track Matte ---
    SetTrackMatte { layer_id: usize, config: TrackMatteConfig },
    SetTrackMatteMode { layer_id: usize, mode: MatteMode },
    SetTrackMatteSource { layer_id: usize, matte_layer: Option<usize> },
    ToggleTrackMatteInvert { layer_id: usize },
    SetTrackMattePreserveTransparency { layer_id: usize, preserve: bool },
    ClearTrackMatte { layer_id: usize },
    ToggleTrackMattePanel,

    // --- Batch 6 depth: Precomp ---
    SetPrecompName(String),
    SetPrecompMoveAttribs(bool),
    SetPrecompAdjustDuration(bool),
    PrecomposeSelected,
    OpenPrecomp(usize),
    ClosePrecomp,
    ReturnToMain,
    RenamePrecomp { idx: usize, name: String },
    DeletePrecomp(usize),
    CollapseTransformations { layer_id: usize },

    // --- Batch 6 depth: Render Queue (enhanced) ---
    ToggleRenderQueue,
    AddRenderQueueItem(RenderQueueItem),
    RemoveRenderQueueItem(usize),
    SetRenderItemFormat { idx: usize, format: RenderOutputFormat },
    SetRenderItemOutput { idx: usize, path: std::path::PathBuf },
    SetRenderItemRange { idx: usize, start: u32, end: u32 },
    SetRenderItemProxy { idx: usize, use_proxy: bool },
    StartRenderQueue,
    StopRenderQueue,
    RenderQueueItemComplete { idx: usize },
    SkipRenderItem(usize),
    DuplicateRenderItem(usize),

    // --- Batch 6 depth: 3D Layer ---
    Enable3DLayer { layer_id: usize, enabled: bool },
    Set3DPosition { layer_id: usize, pos: [f32; 3] },
    Set3DLayerRotation { layer_id: usize, rot: [f32; 3] },
    Set3DOrientation { layer_id: usize, orient: [f32; 3] },
    Set3DScale { layer_id: usize, scale: [f32; 3] },
    Set3DAnchor { layer_id: usize, anchor: [f32; 3] },
    Set3DShadows { layer_id: usize, casts: bool, accepts: bool },
    Set3DMaterial { layer_id: usize, shininess: f32, metal: f32 },
    Reset3DLayer { layer_id: usize },

    // --- New: PuppetPin (app-level) ---
    /// Activate or deactivate the puppet tool.
    ActivatePuppetTool(bool),
    /// Add a puppet pin to a layer at (x, y) with the given mode.
    AddPuppetPinExt { layer_id: usize, x: f32, y: f32, mode: PuppetPinMode },
    /// Move a puppet pin to a new position.
    MovePuppetPinExt { pin_id: usize, x: f32, y: f32 },
    /// Set the stiffness of a puppet pin (clamped 0..=100).
    SetPuppetPinStiffnessExt { pin_id: usize, stiffness: f32 },
    /// Remove a puppet pin by id.
    DeletePuppetPin(usize),
    /// Set puppet mesh density for a layer (clamped 1..=30, upsert).
    SetPuppetMeshDensityExt { layer_id: usize, density: u8 },
    /// Set puppet mesh expansion for a layer (clamped 3..=100, upsert).
    SetPuppetMeshExpansion { layer_id: usize, expansion: f32 },

    // --- New: CameraTracker (3D solve) ---
    /// Start a 3D camera track on a layer; generates stub track points.
    StartCameraTrackSolve { layer_id: usize },
    /// Solve the 3D camera for a layer; sets status=Done and solve_error.
    SolveCameraTrackExt { layer_id: usize },
    /// Select specific track points on a layer's solve (others deselected).
    SelectTrackPoints { layer_id: usize, point_ids: Vec<usize> },
    /// Create a solved camera layer from a completed solve.
    CreateSolvedCamera { layer_id: usize },
    /// Delete the 3D camera track solve for a layer.
    DeleteCameraTrackSolve { layer_id: usize },

    // --- New: TextAnimator (app-level) ---
    /// Add a new app-level text animator for a layer.
    AddTextAnimatorExt { layer_id: usize },
    /// Apply a built-in preset to a text animator.
    ApplyTextAnimPreset { animator_id: usize, preset: TextAnimPreset },
    /// Set the range start/end of a text animator (both clamped 0..=100).
    SetTextAnimRange { animator_id: usize, start: f32, end: f32 },
    /// Set the range units of a text animator.
    SetTextAnimRangeUnits { animator_id: usize, units: String },
    /// Set the "based on" of a text animator.
    SetTextAnimBasedOn { animator_id: usize, based_on: String },
    /// Remove a text animator by id.
    RemoveTextAnimatorExt(usize),

    // --- New: EssentialGraphicsPanel (MOGRT) ---
    /// Open the Essential Graphics panel.
    OpenEssentialGraphics,
    /// Close the Essential Graphics panel.
    CloseEssentialGraphics,
    /// Create a new MOGRT template.
    CreateMogrTemplate { name: String, composition_id: usize },
    /// Add a parameter to a MOGRT template.
    AddMogrParam { template_id: usize, param: MogrParam },
    /// Set the value of a MOGRT parameter.
    SetMogrParamValue { template_id: usize, param_id: String, value: String },
    /// Export a MOGRT template (stub: sets is_responsive=true).
    ExportMogrt { template_id: usize },
    /// Delete a MOGRT template by id.
    DeleteMogrTemplate(usize),
}

impl Action {
    /// Whether applying this action mutates the [`Project`] document (so the
    /// undo stack should snapshot the pre-state before it runs). Pure transport /
    /// selection / UI-toggle actions — and the history actions themselves — are
    /// NOT snapshotted (they don't change the document, or they manage the stack
    /// directly).
    pub(super) fn is_undoable(&self) -> bool {
        matches!(
            self,
            Action::ToggleLayerVisible(_)
                | Action::SetTransform(_, _)
                | Action::ToggleKeyframe(_)
                | Action::MoveKeyframe { .. }
                | Action::AddEffect(_)
                | Action::RemoveEffect { .. }
                | Action::SetEffectParam { .. }
                | Action::SetWorkAreaStart(_)
                | Action::SetWorkAreaEnd(_)
                | Action::ResetWorkArea
                | Action::SetInterp { .. }
                | Action::MoveKeyframeXY { .. }
                | Action::GizmoKeys { .. }
                | Action::SetLayer3D(_, _)
                | Action::SetPositionZ(_, _)
                | Action::Set3DRotation(_, _, _, _)
                | Action::DuplicateLayer(_)
                | Action::SetExpression { .. }
                | Action::AddGpuiEffect(_)
                | Action::RemoveGpuiEffect(_)
                | Action::SetMosaicBlock { .. }
                | Action::SetChromaOffset { .. }
                | Action::SetEffectIntensity { .. }
                | Action::SetVignetteRadius { .. }
                | Action::ApplyCompSettings
                | Action::SetParent(_, _)
                | Action::ClearParent(_)
                | Action::PreCompose(_, _)
                | Action::AddNullLayer
                | Action::AddGuideLayer
                | Action::AddSolidLayer(_)
                | Action::AddAdjustmentLayer
                | Action::SetFootageVideo { .. }
                | Action::SetColorBalanceShadows { .. }
                | Action::SetColorBalanceMidtones { .. }
                | Action::SetColorBalanceHighlights { .. }
                | Action::SetLevelsInBlack { .. }
                | Action::SetLevelsInWhite { .. }
                | Action::SetLevelsGamma { .. }
                | Action::SetLevelsOutBlack { .. }
                | Action::SetLevelsOutWhite { .. }
                | Action::SetHueShift { .. }
                | Action::SetSaturation { .. }
                | Action::SetLightness { .. }
                | Action::SetNoiseFrequency { .. }
                | Action::SetNoiseEvolution { .. }
                | Action::AddCompMarker { .. }
                | Action::RemoveCompMarker(_)
                | Action::AddLayerMarker { .. }
                | Action::RemoveLayerMarker { .. }
                | Action::SetCameraPosition(_)
                | Action::SetCameraFov(_)
                | Action::SetParentLayer { .. }
                | Action::SetTimeRemapEnabled { .. }
                | Action::SetTimeRemapKey { .. }
                | Action::SetLayerMatte { .. }
                | Action::AddPuppetPin { .. }
                | Action::MovePuppetPin { .. }
                | Action::RemovePuppetPin { .. }
                | Action::ToggleSolo(_)
                | Action::AddLight(_)
                | Action::RemoveLight(_)
                | Action::UpdateLight { .. }
                | Action::SetMotionBlurEnabled(_)
                | Action::SetMotionBlurAngle(_)
                | Action::SetMotionBlurPhase(_)
                | Action::SetMotionBlurSamples(_)
                | Action::ToggleLayerMotionBlur(_)
                | Action::AddTextAnimator(_)
                | Action::RemoveTextAnimator(_)
                | Action::SetTextAnimatorRange { .. }
                | Action::SetTextAnimatorOffsetX { .. }
                | Action::SetTextAnimatorOffsetY { .. }
                | Action::SetTextAnimatorRotation { .. }
                | Action::SetTextAnimatorScale { .. }
                | Action::SetTextAnimatorOpacity { .. }
                | Action::SetShapeTrimPaths { .. }
                | Action::ClearShapeTrimPaths(_)
                | Action::AddShapeRepeater(_)
                | Action::RemoveShapeRepeater(_)
                | Action::SetRepeaterCopies { .. }
                | Action::SetRepeaterOffset { .. }
                | Action::SetRepeaterRotation { .. }
                | Action::SetRepeaterScale { .. }
                | Action::SetRepeaterOpacity { .. }
                | Action::AddLumetriColor(_)
                | Action::RemoveLumetriColor(_)
                | Action::SetLumetriParam { .. }
                | Action::ToggleLumetriEnabled(_)
                | Action::ResetLumetriColor(_)
                | Action::SplitLayer(_)
                | Action::SplitLayerAt { .. }
                | Action::AddColorFinesse(_)
                | Action::RemoveColorFinesse(_)
                | Action::SetColorFinesseEnabled { .. }
                | Action::SetColorFinesseParam { .. }
                | Action::ResetColorFinesse(_)
                | Action::AddRotobrushStroke { .. }
                | Action::ClearRotobrushStrokes { .. }
                | Action::PropagateRotobrush { .. }
                | Action::SetLayerTimeStretch { .. }
                | Action::SetLayerAudioFade { .. }
                | Action::SetPuppetPinStiffness { .. }
                | Action::TogglePuppetPinStiff { .. }
                | Action::SetPuppetMeshDensity { .. }
                | Action::SetLayerEcho { .. }
                | Action::ClearLayerEcho { .. }
        )
    }
}
