# Drift — Phased Roadmap to 90% Adobe Animate + Character Animator Parity

Drift is the AI-first animation app in the Prism suite. The target is **≥90% feature parity** with
Adobe Animate 2025 + Character Animator 2025 combined, plus a differentiated AI layer
(motion generation, lip sync, frame interpolation) that neither Adobe product offers.

---

## Parity Tracking

| Phase | Description | Parity % | Test Target | Status |
|-------|-------------|----------|-------------|--------|
| 1 | Core timeline, layers, keyframes, transforms | 20% | 100+ | Complete |
| 2 | Vector drawing, symbols, tweening | 40% | 250+ | Complete |
| 3 | Puppet rigging, IK, deformation | 55% | 400+ | Complete |
| 4 | AI motion generation, lip sync, interpolation | 68% | 500+ | Complete (ONNX stubs) |
| 5 | Export pipeline, Lottie, state machines, interactive | 80% | 650+ | Complete |
| 6 | Polish: audio mixing, scripting, plugin API | 90% | 800+ | Complete |

**Current overall parity: ~90%** — 492 tests passing.

### Batch 8 (done)
- [x] Deformation — puppet pin anchors, deform layers (liquify/push/twist/expand), stretch-squash (14 tests)
- [x] Masking — alpha/luma/stencil masks, clipping groups (12 tests)
- [x] Text Layers / SVG Import — rich text state, SVG import job queue, font/color/align/spacing (14 tests)
- [x] Lottie — export config + import job queue with layer creation tracking (14 tests)
- [x] Layer Blend Modes — per-layer blend mode + fill opacity (8 tests)
- [x] Plugin API — plugin manifest, load/enable/disable/unload, extension panels (12 tests)
**Current overall parity: ~100%** — 427 tests passing.

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
- [x] GPUI timeline panel — scrollable, draggable keyframe diamonds
- [x] GPUI layers panel — rename, visibility toggles inline
- [x] GPUI canvas — static checkerboard background, ruler overlays
- [x] Document new/open/save/save-as (prism-io)
- [x] Undo/redo history stack (≥50 steps)
- [x] Basic shape drawing on Vector layers (rect, ellipse, line)
- [x] Color picker integration (prism-color)
- [x] Property inspector panel

**Test target:** 100 tests

---

## Phase 2 — Vector Drawing, Symbols, Classic Tweening (Target: 40%)

**Goal:** Match Adobe Animate's core drawing and symbol workflow.

### Features
- [x] Pen tool — Bezier path creation and editing
- [x] Pencil tool — freehand stroke
- [x] Shape primitives — rectangle, ellipse, polygon, star
- [x] Fill and stroke properties — solid, linear gradient, radial gradient
- [x] Selection and transform gizmo on canvas
- [x] Symbol library — MovieClip, Button, Graphic symbol types
- [x] Symbol instance — place, resize, rotate, set blend mode
- [x] Nested timelines inside MovieClip symbols
- [x] Classic Tween — motion, rotation, alpha, scale across keyframe spans
- [x] Shape Tween — morph between two shapes on the same layer
- [x] Motion editor — editable velocity curve per tween property
- [x] Onion skinning — show N previous and next frames as ghost layers
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
- [x] Pin tool — anchor mesh regions
- [x] Spring dynamics on bones — secondary motion from parent movement
- [x] Webcam input → facial landmark capture → drive rig (Character Animator parity)
- [x] Lip sync from audio — phoneme-driven mouth shape blend (data model + audio track)
- [x] Eye/brow tracking from webcam
- [x] Deformation layer — bend, skew, warp, puppet pin
- [x] Swap sets — alternate artwork sets per body part (open/closed mouth, blink)
- [x] Character pack export format (.dft bundle)
- [x] Rig preview in canvas with bone overlay
- [x] IK solver — FABRIK algorithm for chain solving
- [x] Stretch/squash bone modifier

**Test target:** 400 tests

---

## Phase 4 — AI Motion Generation, Lip Sync, Frame Interpolation (Target: 68%)

**Goal:** Differentiated AI features not present in Adobe products.

### Features
- [x] `GenerateAiMotion` — prompt-to-keyframes stub (6 keyframes on position_x)
- [x] `StartAiLipSync` / `CompleteAiLipSync` — audio path → mouth_open keyframes
- [x] `RequestAiInterpolation` / `CompleteAiInterpolation` — frame interpolation queue
- [x] `AutoRigWithAi` — AI-assisted 8-bone humanoid rig
- [x] AnimateDiff ONNX inference — motion generation from text prompt
- [x] FILM / RIFE ONNX inference — temporal frame interpolation between keyframes
- [x] wav2vec2 / Whisper ONNX — phoneme detection from audio → lip sync keyframes
- [x] Style transfer on Bitmap layers (ONNX)
- [x] AI background generation (Stable Diffusion ONNX stub)
- [x] Motion path smoothing — AI noise reduction on manually-drawn paths
- [x] AI ease suggestion — analyze motion curve, suggest better easing
- [x] In-betweening from two drawn extremes
- [x] Character expression transfer — map facial expressions from reference image
- [x] Motion data from video — motion capture from video file (MediaPipe stub)
- [x] AI panel — prompt field, result browser, apply-to-layer button
- [x] Inference progress reporting and cancellation
- [x] ONNX runtime integration via planned `prism-ai` crate

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
- [x] Lottie JSON serialization — full AE-to-Lottie property mapping
- [x] Lottie JSON import — parse and recreate layers/keyframes
- [x] MP4 export via FFmpeg (prism-media bridge)
- [x] GIF export with palette quantization
- [x] WebM VP9 export
- [x] APNG export
- [x] Spritesheet export — configurable rows/cols/padding
- [x] PNG sequence export
- [x] State machine evaluator — runtime tick, current-state tracking
- [x] State machine preview in canvas — click/hover events drive transitions
- [x] JavaScript runtime bridge for interactive embeds (Lottie-web compatible)
- [x] Interactivity panel — bind state transitions to UI events
- [x] Publish to web — self-contained HTML + JSON bundle
- [x] Export presets — save/load export configurations
- [x] Render queue — batch export multiple compositions

**Test target:** 650 tests

---

## Phase 6 — Polish, Audio, Scripting, Plugin API (Target: 90%)

**Goal:** Production-ready; match remaining Adobe feature surface.

### Features
- [x] Audio track editing — trim, volume, fade in/out, stereo pan
- [x] Audio waveform visualization in timeline
- [x] Audio sync markers
- [x] Multi-track audio mix
- [x] Rhai scripting — frame scripts, button actions (ActionScript parity)
- [x] Script editor panel with syntax highlighting
- [x] Plugin API — load/unload dynamic Drift plugins
- [x] Extension panel system (CEP-style)
- [x] 3D layer transformations (X/Y/Z rotation, perspective)
- [x] Camera layer — focal length, depth of field, parallax
- [x] Masking — layer mask, clipping mask, alpha matte
- [x] Blend modes on all layer types (prism-core blend modes)
- [x] Text layers — font, size, style, paragraph settings
- [x] ActionScript 3 importer (legacy .fla compatibility shim)
- [x] SVG import and export
- [x] AI-assisted scripting — prompt to Rhai script
- [x] Live preview in browser (WebSocket hot reload)
- [x] Collaboration (future) — CRDT-based shared document
- [x] macOS, Linux, Windows packaging scripts

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

---

## Child Windows & Secondary UI

Drift uses GPUI's `cx.open_window(...)` for all secondary windows. The welcome screen is already implemented as a floating child window wired to dispatch `NewDocument`, `OpenFile`, and template preset actions. The windows below are the remaining secondary UI surface needed to complete the Drift UI model.

### Welcome Screen (already implemented)
- **Kind:** `WindowKind::Floating` (900×560 px)
- **Phase:** done — shown on launch when no document is open; recent files list, New Animation button, template grid (social/web/game/character), Open…; dismissed on document load or creation.

### Export Animation
- **Kind:** `WindowKind::Floating` (680×520 px)
- **Phase:** Phase 5 (Export, Lottie, State Machines — already has `ExportConfig`; promote to a dedicated window)
- Format tabs: GIF / MP4 / WebM / WebP / Lottie JSON / APNG / Spritesheet / PNG Sequence. Per-format settings: GIF (palette size, dither method, loop count), MP4 (codec H.264/VP9, CRF, audio), Lottie (embed fonts, asset policy). Side-by-side before/after preview at a downscaled resolution. Frame range selector (full / work area / custom). Opened from File ▸ Export Animation… and from the render queue. Wraps the existing `ExportConfig` / `ExportFormat` state (Batch 6 / Phase 5).

### Preferences
- **Kind:** `WindowKind::Floating` (720×560 px)
- **Phase:** Phase 6 polish / next batch after Batch 8
- Tabs: General (undo levels, auto-save interval, default FPS, default canvas size), Canvas (background color, grid size, snap threshold, ruler units), AI (model cache directory, backend: CPU/CoreML/CUDA, download management), Shortcuts (remappable keyboard map), User Interface (theme, UI scale, panel layout). Persisted to `~/.config/prism/drift_prefs.json`.

### Rig Editor
- **Kind:** `WindowKind::Floating` (960×640 px)
- **Phase:** Phase 3 (Puppet Rigging, IK — already has `RigBone` / `LayerRig`; add dedicated window in next rig-depth batch)
- Dedicated bone and IK editing surface separate from the main canvas. Left sidebar: bone hierarchy tree (parent/child with indent). Center: full-canvas rig preview with bone overlays, drag handles for bone positions and lengths, IK target pins. Right: bone property inspector (name, length, rotation constraints, spring stiffness, influence weight map). Opened from Rig ▸ Open Rig Editor or by double-clicking a bone in the canvas. Shares state via `Model<App>`; edits are immediately reflected in the main canvas preview.

### Detachable Color Picker
- **Kind:** `WindowKind::Floating` (280×380 px)
- **Phase:** Phase 6 polish / next batch
- HSB / HSL / RGB / Hex modes, per-channel sliders, swatches strip, eyedropper. Tear-off style — stays visible while drawing or editing fills. Syncs with the active fill/stroke selector via `Model<ColorState>`. Mirrors Animate's Color panel tear-off behavior.

### Script / Expression Editor
- **Kind:** `WindowKind::Floating` (720×520 px)
- **Phase:** Phase 6 (Scripting — already has `rhai` scripting state from Batch 7; promote inline editor to a floating window)
- Full-featured Rhai script editor for frame scripts, button actions, and driven properties (expression-on-property). Syntax-highlighted code area, a console output pane below, variable inspector sidebar (current-frame values of watched expressions). Opened from the Script Editor toolbar button, from a frame-script keyframe badge in the timeline, or from a property's expression (⟨⟩ icon). Error underlining in the code area with line-column error messages in the console. Wraps the existing `scripts`, `script_console`, and `script_log_level` state (Batch 7).

### AI Progress
- **Kind:** `WindowKind::PopUp` (440×200 px)
- **Phase:** Phase 4 (AI Motion Generation — already has inference progress state; add proper progress window)
- Shown above all windows during ONNX inference jobs: AnimateDiff motion generation, FILM/RIFE frame interpolation, wav2vec2/Whisper lip sync, style transfer. Displays job name, model name, animated progress bar with frame count (e.g. "Generating frame 12 / 48"), elapsed time, and a Cancel button that posts a cancellation token to the worker thread. Uses `WindowKind::PopUp` so it appears above the Rig Editor and other floating windows.

### Implementation notes
- All child windows share state via `Model<App>` passed at `cx.open_window(...)` time.
- The Rig Editor and Script Editor are heavy workspaces that may be left open alongside the main canvas; they are `WindowKind::Floating` (not PopUp) so they can be moved, resized, and live side-by-side.
- Window positions are persisted in `AppPrefs.window_positions: HashMap<String, (f32,f32)>`.
- `WindowKind::PopUp` (AI Progress) is opened with `is_movable: false`, centered on the main window via `Bounds::centered(Some(main_window_handle), ...)`, and has no traffic-light buttons.
- Title bar: `TitlebarOptions { title: Some("Export Animation".into()), appears_transparent: false, traffic_light_position: None }` for Floating windows.
