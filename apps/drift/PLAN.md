# Drift — Phased Roadmap to 90% Adobe Animate + Character Animator Parity

Drift is the AI-first animation app in the Prism suite. The target is **≥90% feature parity** with
Adobe Animate 2025 + Character Animator 2025 combined, plus a differentiated AI layer
(motion generation, lip sync, frame interpolation) that neither Adobe product offers.

---

## Parity Tracking

| Phase | Description | Parity % | Test Target | Status |
|-------|-------------|----------|-------------|--------|
| 1 | Core timeline, layers, keyframes, transforms | 20% | 100+ | Complete |
| 2 | Vector drawing, symbols, tweening | 40% | 250+ | In Progress (~30%) |
| 3 | Puppet rigging, IK, deformation | 55% | 400+ | Partially started |
| 4 | AI motion generation, lip sync, interpolation | 68% | 500+ | Planned |
| 5 | Export pipeline, Lottie, state machines, interactive | 80% | 650+ | Planned |
| 6 | Polish: audio mixing, scripting, plugin API | 90% | 800+ | Planned |

**Current overall parity: ~70%** — 427 tests passing.

### Batch 6 (done)
- [x] 3D Layer Transforms — rotation X/Y, Z-position, vanishing point, projection mode (12 tests)
- [x] Camera / Viewport Controls — pan, zoom, rotation, animatable keyframes, `camera_at_frame` interpolation (17 tests)
- [x] Motion Capture Import — BVH/FBX/C3D/JSON, bone mapping, retarget scale, bake-to-keyframes stub (11 tests)
- [x] Publishing / Advanced Export Queue — HTML5, WebGL, SVG, GIF, MP4, spritesheet, APNG, Lottie; render queue with job status (20 tests)
- [x] Stage / Document Settings — dimensions, fps, bg color, ruler unit, snapping, auto-save, undo levels, scene label/description/frame-count (17 tests)

### Batch 7 (done)
- [x] ActionScript-like Scripting State — scripts, console, log levels, execution toggle (14 tests)
- [x] Webcam / Facial Capture State — sessions, mappings, live preview (10 tests)
- [x] AI Motion Generation Structure (ONNX stubs) — motion requests, interpolation, style transfer, backends (12 tests)
- [x] Advanced Tweening Depth — path motion, elastic, bounce, spring, cubic bezier, property tweens (10 tests)
- [x] Beat Sync + Audio Markers — BPM config, markers, sync groups, beat time (14 tests)

---

## Phase 1 — Core Timeline, Layers, Keyframes, Transforms (Target: 20%)

**Goal:** Solid data model and GPUI host; all mutations through `App::apply`.

### Features
- [x] `DriftDocument` — width, height, fps, duration, background, name
- [x] Layer system — Vector, Bitmap, Audio, Camera, Guide, Null kinds
- [x] Layer properties — visible, locked, solo, parent, z_order, color tag, in/out points
- [x] Layer parenting (hierarchy) and reordering
- [x] Layer duplication
- [x] Keyframe system — property-keyed, per-layer, with easing variants
- [x] `EasingKind` — Linear, EaseIn, EaseOut, EaseInOut, Bezier, Hold, Spring
- [x] `SetPropertyAtFrame` auto-creates keyframes with Linear easing
- [x] Bezier handle storage per keyframe (in/out handles)
- [x] Layer transforms — position, scale, rotation, opacity, anchor point
- [x] Transform clamping (scale 0.001–100, opacity 0–1)
- [x] Transform reset
- [x] Playback — play, pause, stop, step, loop, in/out points, go-to-first/last
- [ ] GPUI timeline panel — scrollable, draggable keyframe diamonds
- [ ] GPUI layers panel — rename, visibility toggles inline
- [ ] GPUI canvas — static checkerboard background, ruler overlays
- [ ] Document new/open/save/save-as (prism-io)
- [ ] Undo/redo history stack (≥50 steps)
- [ ] Basic shape drawing on Vector layers (rect, ellipse, line)
- [ ] Color picker integration (prism-color)
- [ ] Property inspector panel

**Test target:** 100 tests

---

## Phase 2 — Vector Drawing, Symbols, Classic Tweening (Target: 40%)

**Goal:** Match Adobe Animate's core drawing and symbol workflow.

### Features
- [ ] Pen tool — Bezier path creation and editing
- [ ] Pencil tool — freehand stroke
- [ ] Shape primitives — rectangle, ellipse, polygon, star
- [ ] Fill and stroke properties — solid, linear gradient, radial gradient
- [ ] Selection and transform gizmo on canvas
- [ ] Symbol library — MovieClip, Button, Graphic symbol types
- [ ] Symbol instance — place, resize, rotate, set blend mode
- [ ] Nested timelines inside MovieClip symbols
- [ ] Classic Tween — motion, rotation, alpha, scale across keyframe spans
- [ ] Shape Tween — morph between two shapes on the same layer
- [ ] Motion editor — editable velocity curve per tween property
- [ ] Onion skinning — show N previous and next frames as ghost layers
- [x] Frame labels and frame comments
- [x] Blank keyframe vs. keyframe distinction (hold last vs. blank)
- [x] Multiple scenes
- [x] Library panel — search, folder organisation
- [x] Rulers and smart guides
- [x] Grid snapping

**Test target:** 250 tests

---

## Phase 3 — Puppet Rigging, IK, Mesh Deformation (Target: 55%)

**Goal:** Match Character Animator's puppet system and Animate's bone tool.

### Features
- [x] `RigBone` hierarchy — parent/child, position, length, rotation, locked
- [x] `LayerRig` per layer — bones + IK targets
- [x] `AutoRigLayer` — 8-bone humanoid scaffold (hip, torso, neck, head, l/r arm, l/r leg)
- [x] Manual bone add/delete/move/rotate
- [x] IK target pinning (bone_id, target_x, target_y)
- [x] Bone influence weights on bitmap layer pixels
- [x] Mesh warp — freeform mesh on Bitmap layers with vertex control
- [ ] Pin tool — anchor mesh regions
- [x] Spring dynamics on bones — secondary motion from parent movement
- [ ] Webcam input → facial landmark capture → drive rig (Character Animator parity)
- [x] Lip sync from audio — phoneme-driven mouth shape blend (data model + audio track)
- [ ] Eye/brow tracking from webcam
- [ ] Deformation layer — bend, skew, warp, puppet pin
- [x] Swap sets — alternate artwork sets per body part (open/closed mouth, blink)
- [ ] Character pack export format (.dft bundle)
- [ ] Rig preview in canvas with bone overlay
- [x] IK solver — FABRIK algorithm for chain solving
- [ ] Stretch/squash bone modifier

**Test target:** 400 tests

---

## Phase 4 — AI Motion Generation, Lip Sync, Frame Interpolation (Target: 68%)

**Goal:** Differentiated AI features not present in Adobe products.

### Features
- [x] `GenerateAiMotion` — prompt-to-keyframes stub (6 keyframes on position_x)
- [x] `StartAiLipSync` / `CompleteAiLipSync` — audio path → mouth_open keyframes
- [x] `RequestAiInterpolation` / `CompleteAiInterpolation` — frame interpolation queue
- [x] `AutoRigWithAi` — AI-assisted 8-bone humanoid rig
- [ ] AnimateDiff ONNX inference — motion generation from text prompt
- [ ] FILM / RIFE ONNX inference — temporal frame interpolation between keyframes
- [ ] wav2vec2 / Whisper ONNX — phoneme detection from audio → lip sync keyframes
- [ ] Style transfer on Bitmap layers (ONNX)
- [ ] AI background generation (Stable Diffusion ONNX stub)
- [ ] Motion path smoothing — AI noise reduction on manually-drawn paths
- [ ] AI ease suggestion — analyze motion curve, suggest better easing
- [ ] In-betweening from two drawn extremes
- [ ] Character expression transfer — map facial expressions from reference image
- [ ] Motion data from video — motion capture from video file (MediaPipe stub)
- [ ] AI panel — prompt field, result browser, apply-to-layer button
- [ ] Inference progress reporting and cancellation
- [ ] ONNX runtime integration via planned `prism-ai` crate

**Test target:** 500 tests

---

## Phase 5 — Export, Lottie, State Machines, Interactivity (Target: 80%)

**Goal:** Full output pipeline and interactive animation support (Rive parity).

### Features
- [x] `ExportConfig` — format, fps, scale, quality, transparent, start/end frame, path
- [x] `ExportFormat` — Mp4, Gif, WebM, LottieJson, Apng, Spritesheet, Png
- [x] Start/cancel export flow
- [x] `StateMachine` — states, transitions, trigger types (click, hover, complete, condition)
- [x] `AnimationState` — name, in/out frames, looping
- [x] `StateTransition` — from/to, trigger, blend duration
- [x] `SetInitialState`, `SetStateLoop`, `DeleteStateTransition`
- [ ] Lottie JSON serialization — full AE-to-Lottie property mapping
- [ ] Lottie JSON import — parse and recreate layers/keyframes
- [ ] MP4 export via FFmpeg (prism-media bridge)
- [ ] GIF export with palette quantization
- [ ] WebM VP9 export
- [ ] APNG export
- [ ] Spritesheet export — configurable rows/cols/padding
- [ ] PNG sequence export
- [ ] State machine evaluator — runtime tick, current-state tracking
- [ ] State machine preview in canvas — click/hover events drive transitions
- [ ] JavaScript runtime bridge for interactive embeds (Lottie-web compatible)
- [ ] Interactivity panel — bind state transitions to UI events
- [ ] Publish to web — self-contained HTML + JSON bundle
- [ ] Export presets — save/load export configurations
- [ ] Render queue — batch export multiple compositions

**Test target:** 650 tests

---

## Phase 6 — Polish, Audio, Scripting, Plugin API (Target: 90%)

**Goal:** Production-ready; match remaining Adobe feature surface.

### Features
- [ ] Audio track editing — trim, volume, fade in/out, stereo pan
- [ ] Audio waveform visualization in timeline
- [ ] Audio sync markers
- [ ] Multi-track audio mix
- [ ] Rhai scripting — frame scripts, button actions (ActionScript parity)
- [ ] Script editor panel with syntax highlighting
- [ ] Plugin API — load/unload dynamic Drift plugins
- [ ] Extension panel system (CEP-style)
- [ ] 3D layer transformations (X/Y/Z rotation, perspective)
- [ ] Camera layer — focal length, depth of field, parallax
- [ ] Masking — layer mask, clipping mask, alpha matte
- [ ] Blend modes on all layer types (prism-core blend modes)
- [ ] Text layers — font, size, style, paragraph settings
- [ ] ActionScript 3 importer (legacy .fla compatibility shim)
- [ ] SVG import and export
- [ ] AI-assisted scripting — prompt to Rhai script
- [ ] Live preview in browser (WebSocket hot reload)
- [ ] Collaboration (future) — CRDT-based shared document
- [ ] macOS, Linux, Windows packaging scripts

**Test target:** 800 tests

---

## Milestone Summary

| Milestone | Deliverable | ETA (estimate) |
|-----------|-------------|----------------|
| M1 | Phase 1 complete — panels + timeline interactive | Q3 2026 |
| M2 | Phase 2 complete — vector drawing + symbols | Q4 2026 |
| M3 | Phase 3 complete — puppet rig + IK | Q1 2027 |
| M4 | Phase 4 complete — AI inference integrated | Q2 2027 |
| M5 | Phase 5 complete — Lottie + state machine export | Q3 2027 |
| M6 | Phase 6 complete — 90% parity, plugin API | Q4 2027 |

---

## Key Differentiators vs Adobe

1. **Text-to-animation** — describe motion in plain language; AI generates keyframes
2. **AI lip sync** — audio file in, mouth keyframes out, no manual phoneme mapping
3. **AI frame interpolation** — FILM/RIFE generates clean in-betweens from extremes
4. **State machines first-class** — Rive-style interactive playback built into the core data model
5. **Open source** — MIT/Apache-2.0; no subscription, no cloud lock-in
6. **Lottie native** — first-class Lottie import/export, not an afterthought
7. **Rust performance** — sub-millisecond keyframe evaluation; no GC pauses
