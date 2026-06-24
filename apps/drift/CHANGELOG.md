# Changelog

All notable changes to **Drift** are documented here.
Format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
Versioning follows the workspace `version` in the root `Cargo.toml`.

---

## [Unreleased]

## [0.11.0] - 2026-06-24

### Added — Real text input (`prism_ui::TextField`)
- **Typeable AnimateDiff motion prompt** — type the prompt that drives motion generation → `SetAiMotionPrompt` → `QueueAnimateDiff` (style-tag buttons append to the real field).
- **AI-script prompt + script editor** — typeable natural-language prompt (`SetAiScriptPrompt`) + editable script source (`SetScriptSource`); Generate Script queues + writes derived code.
- **Layer rename** — double-click inline rename → `RenameLayer`.
- +4 tests (651 → 655).

## [0.10.0] - 2026-06-24

### Added — Real ONNX inference layer
- `onnx_runtime.rs` — `Inference` trait + `InferenceBackend` (`Stub` | `Ort`) over
  AnimateDiff (motion), FILM/RIFE (interpolate), Whisper/wav2vec2 (phonemes), AI
  scripting; a `ModelRegistry` mapping `DriftOnnxModel` → path + load state.
- Real `ort` 2.x session load/run/tensor-IO under the optional `onnx` feature
  (`apps/drift/Cargo.toml`: `ort = { version = "=2.0.0-rc.10", optional = true }`,
  `[features] onnx = ["dep:ort"]`). Default build pulls no `ort`; the deterministic
  stub stays default and is the fallback when the feature is off or a model file is
  missing. `QueueAnimateDiff` now sources keyframes through the backend. +16 tests.

## [0.9.0] - 2026-06-24

### Fixed
- `test_queue_animatediff` aligned with the synchronous-stub behaviour: queuing an
  AnimateDiff job completes immediately (adds motion keyframes, marks the job
  `Done`), so the test no longer asserts a transient `Queued` status.

### Added

#### Core Data Model
- `DriftDocument` struct — width, height, fps (clamped 1–120), duration_frames,
  background_color, name
- `SetDocumentWidth`, `SetDocumentHeight`, `SetDocumentFps`, `SetDocumentDuration`,
  `SetDocumentBg`, `SetDocumentName` actions

#### Layer System
- `DriftLayer` struct — id, name, `LayerKind`, visible, locked, solo, parent_id,
  z_order, color_tag, start_frame, end_frame
- `LayerKind` enum — Vector, Bitmap, Audio, Camera, Guide, Null
- `AddLayer`, `DeleteLayer`, `RenameLayer`, `SetLayerVisible`, `SetLayerLocked`,
  `SetLayerSolo`, `SetLayerParent`, `ReorderLayers`, `SetLayerColorTag`,
  `DuplicateLayer`, `SetLayerStartFrame`, `SetLayerEndFrame`, `SetActiveLayer` actions
- `AddLayer` automatically initialises a `LayerTransform` for the new layer
- `DeleteLayer` cascades to remove associated keyframes, transforms, and rigs

#### Keyframe System
- `Keyframe` struct — id, layer_id, property (String), frame, value (f32),
  easing, bezier_handle_in, bezier_handle_out
- `EasingKind` enum — Linear, EaseIn, EaseOut, EaseInOut, Bezier, Hold, Spring
- `AddKeyframe`, `DeleteKeyframe`, `MoveKeyframe`, `SetKeyframeValue`,
  `SetKeyframeEasing`, `SetPropertyAtFrame` actions
- `SetPropertyAtFrame` upserts: updates existing keyframe at same layer/property/frame,
  or auto-creates one with Linear easing

#### Layer Transforms
- `LayerTransform` struct — x, y, scale_x, scale_y, rotation, opacity, anchor_x, anchor_y
- Scale clamped to 0.001–100; opacity clamped to 0.0–1.0
- `SetLayerPosition`, `SetLayerScale`, `SetLayerRotation`, `SetLayerOpacity`,
  `SetLayerAnchor`, `ResetLayerTransform` actions

#### Playback
- `Play`, `Pause`, `Stop` (resets to in_point), `SetCurrentFrame` (clamped to
  duration), `ToggleLoop`, `SetInPoint`, `SetOutPoint`, `StepForward`,
  `StepBackward`, `GoToFirstFrame`, `GoToLastFrame` actions

#### Puppet Rigging
- `RigBone` struct — id, name, parent_id, x, y, length, rotation, locked
- `LayerRig` struct — layer_id, bones, ik_targets
- `AddBone`, `DeleteBone`, `MoveBone`, `RotateBone`, `SetIKTarget`,
  `AutoRigLayer` actions
- `AutoRigLayer` pushes 8 default humanoid bones: hip, torso, neck, head,
  l_arm, r_arm, l_leg, r_leg with correct parent chain
- IK target upsert — re-pinning an existing bone updates in place, not duplicate

#### AI Features (Stubs)
- `SetAiMotionPrompt`, `GenerateAiMotion` — generates 6 position_x keyframes
  at frames 0/5/10/15/20/25 with EaseInOut easing
- `StartAiLipSync`, `CompleteAiLipSync` — queue + resolve; generates 12
  mouth_open keyframes on completion
- `RequestAiInterpolation`, `CompleteAiInterpolation` — interpolation job queue
- `AutoRigWithAi` — delegates to the same 8-bone auto-rig logic as `AutoRigLayer`

#### Export
- `ExportConfig` struct — format, fps, scale (clamped 0.1–4.0), quality
  (clamped 0–100), transparent, start_frame, end_frame, output_path
- `ExportFormat` enum — Mp4, Gif, WebM, LottieJson, Apng, Spritesheet, Png
- `SetExportFormat`, `SetExportFps`, `SetExportScale`, `SetExportQuality`,
  `SetExportTransparent`, `SetExportStartFrame`, `SetExportEndFrame`,
  `SetExportPath`, `StartExport`, `CancelExport` actions

#### State Machine
- `StateMachine` struct — id, name, states, transitions, initial_state
- `AnimationState` struct — id, name, start_frame, end_frame, looping
- `StateTransition` struct — from_state, to_state, trigger, duration_frames
- `StateTransitionTrigger` enum — OnClick, OnHover, OnComplete, OnCondition(String)
- `CreateStateMachine`, `AddAnimationState`, `DeleteAnimationState`,
  `AddStateTransition`, `DeleteStateTransition`, `SetInitialState`,
  `SetStateLoop` actions
- `DeleteAnimationState` cascades to remove all transitions referencing that state

#### Host
- Minimal GPUI host (`src/main.rs`) — canvas placeholder, layers panel, AI panel,
  timeline strip; spacebar play/pause, arrow key frame stepping
- `Cargo.toml` following workspace pattern with `workspace = true` deps

#### Documentation
- `PLAN.md` — 6-phase roadmap to 90% Adobe Animate + Character Animator parity
- `ARCHITECTURE.md` — GPUI/wgpu rendering, keyframe engine, AI inference pipeline,
  Lottie mapping, state machine evaluator, crate boundaries
- `RESEARCH.md` — competitor analysis (Rive, Jitter, Cavalry, Animate,
  Character Animator), AI model registry, 10 UX principles
- `README.md` — feature overview, build instructions, competitor comparison table

#### Tests
- 100+ unit tests in `src/app_state.rs` covering all feature areas
