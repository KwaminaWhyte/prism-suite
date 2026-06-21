# Drift Architecture

## Overview

Drift is built on the same GPUI + wgpu stack as Pigment, with the additional
requirement of a real-time animation evaluation engine and an AI inference
pipeline. The core principle: **all state mutations flow through `App::apply`**,
keeping panels, tests, and AI jobs all exercising the same code path.

```
┌────────────────────────────────────────────────────────────────────────────┐
│  Drift GPUI Root View                                                      │
│  ┌──────────┐  ┌──────────────────────────────────┐  ┌──────────────────┐ │
│  │ Layers   │  │ Canvas (frame compositor output)  │  │ AI Panel         │ │
│  │ Panel    │  │ + Timeline ruler overlay          │  │ (prompt/results) │ │
│  └──────────┘  └──────────────────────────────────┘  └──────────────────┘ │
│  ┌────────────────────────────────────────────────────────────────────────┐ │
│  │ Timeline Panel  (keyframe diamonds, layer bars, playhead)              │ │
│  └────────────────────────────────────────────────────────────────────────┘ │
│                                 │ Action                                     │
│                                 ▼                                            │
│                          App::apply(&mut self, Action)                       │
│                                 │                                            │
│            ┌────────────────────┼────────────────────┐                       │
│            ▼                   ▼                     ▼                       │
│     Keyframe Engine     Puppet Rig State      State Machine                  │
│     (interpolation)     (IK solver)           (transition eval)              │
│            │                   │                                             │
│            ▼                   ▼                                             │
│     Frame Compositor   (prism-canvas via CanvasHost)                         │
│            │                                                                 │
│            ▼                                                                 │
│     RenderImage → GPUI sprite atlas → window                                 │
└────────────────────────────────────────────────────────────────────────────┘
                                 │
                    ┌────────────┴────────────┐
                    ▼                         ▼
             prism-ai (planned)         prism-media
             ONNX Runtime              FFmpeg bridge
             AnimateDiff / FILM        MP4 / WebM encode
             wav2vec2 / Whisper        Audio mux
```

---

## GPUI + wgpu Rendering

Drift uses GPUI as the UI host (same as Pigment). The animation canvas is
rendered into a `RenderImage` via `prism-canvas`'s CPU/GPU compositor, then
bridged into GPUI's sprite atlas each frame via `CanvasHost`.

Unlike Pigment (which has its own wgpu device), Drift delegates compositing
entirely to `prism-canvas`. This means:

- WGSL shaders live in `prism-canvas`, not in `apps/drift/src/shaders/`
- Drift does not add new wgpu pipelines unless a Drift-specific effect
  (e.g., bone overlay wireframe) cannot be expressed in existing passes
- The GPU compositor runs in GPUI's `prepare_frame` hook, same as Pigment

Frame pipeline per tick:

1. `App::tick(dt)` — advance `current_frame` if playing, evaluate loop
2. `KeyframeEngine::evaluate(frame)` — sample all keyframe curves, write
   resolved property values into `LayerTransform` and layer state
3. `PuppetSolver::solve()` — run FABRIK IK for each rig, update bone world
   transforms
4. `StateMachineEvaluator::tick()` — check transition triggers, blend states
5. `CanvasHost::composite(layers, frame)` — flatten visible layers into the
   output `RenderImage` using `prism-canvas` passes
6. GPUI renders the `RenderImage` into the canvas panel

---

## Animation Frame Compositing

Each frame's composite is built by walking the layer list in z_order (bottom to
top), applying the resolved `LayerTransform` (position, scale, rotation, opacity,
anchor) to each layer's content tile, then blending using the blend mode stored on
the layer (via `prism-core`'s blend engine).

For Bitmap layers:
- Content is a `Rgba16Float` texture owned by `prism-canvas`
- Mesh deformation vertices (Phase 3) are baked into a warp pass before blend

For Vector layers:
- Paths are rasterized by `tiny-skia` into a CPU `Rgba16Float` buffer
- The result is uploaded to GPU and treated as a Bitmap layer from there

For Audio layers:
- No visual contribution; audio is routed to `rodio` for playback sync

Camera layer:
- Transforms the canvas coordinate system (pan, zoom, rotate) rather than
  compositing pixels

---

## Keyframe Interpolation Engine

```rust
// Planned interface (Phase 1 extension)
pub struct KeyframeEngine<'a> {
    keyframes: &'a [Keyframe],
}

impl<'a> KeyframeEngine<'a> {
    /// Sample the value of `property` on `layer_id` at `frame`, interpolating
    /// between the two surrounding keyframes according to the earlier one's easing.
    pub fn sample(&self, layer_id: usize, property: &str, frame: f32) -> Option<f32>;
}
```

Easing implementation:

| `EasingKind`  | Algorithm |
|---------------|-----------|
| `Linear`      | `t` |
| `EaseIn`      | `t²` |
| `EaseOut`     | `1-(1-t)²` |
| `EaseInOut`   | smoothstep |
| `Bezier`      | 1D cubic Bezier via `bezier_handle_in/out`, Newton's method |
| `Hold`        | step function — value jumps at the destination keyframe |
| `Spring`      | damped spring oscillator: `x(t) = 1 - e^(-ζωt)(cos(ωdt) + ζ/ωd·sin(ωdt))` |

---

## AI Inference Pipeline

The planned `prism-ai` crate wraps ONNX Runtime (via the `ort` crate). Drift
loads models lazily on first use from `~/.drift/models/`.

```
prism-ai crate (planned)
├── src/
│   ├── runtime.rs         # OrtEnvironment singleton, session cache
│   ├── animatediff.rs     # Text-to-motion: prompt → keyframe deltas
│   ├── film_rife.rs       # Frame interpolation: frame_a + frame_b → in-betweens
│   ├── lipsync.rs         # wav2vec2/Whisper: audio → phoneme sequence → mouth_open values
│   └── pose.rs            # MediaPipe MoveNet: video → skeleton keypoints → rig keyframes
```

Drift's AI actions are stubs today (`GenerateAiMotion`, `CompleteAiLipSync`,
etc.). When `prism-ai` is ready, the `App::apply` arms for these actions will
call into `prism-ai` via async tasks, reporting results back through the same
`Action` variants.

### Model registry

| Task | Model | Format | ~Size |
|------|-------|--------|-------|
| Motion generation | AnimateDiff v3 | ONNX fp16 | 1.8 GB |
| Frame interpolation | FILM (Google) | ONNX fp32 | 180 MB |
| Fast interpolation | RIFE 4.6 | ONNX fp16 | 22 MB |
| Lip sync phonemes | wav2vec2-base | ONNX fp32 | 360 MB |
| Speech-to-text | Whisper tiny | ONNX fp32 | 74 MB |
| Pose estimation | MoveNet Thunder | ONNX int8 | 13 MB |

---

## Lottie JSON Format

Lottie is Drift's primary web export format. The serializer (Phase 5) maps:

| Drift concept | Lottie key |
|---------------|-----------|
| `DriftDocument` | `{ v, w, h, fr, ip, op }` |
| `DriftLayer` (Vector) | `{ ty: 4, shapes: [...] }` |
| `DriftLayer` (Bitmap) | `{ ty: 2, refId }` |
| `DriftLayer` (Null) | `{ ty: 3 }` |
| `DriftLayer` (Camera) | `{ ty: 13 }` |
| `LayerTransform` | `{ ks: { p, s, r, o, a } }` |
| `Keyframe` (EaseInOut) | `{ t, s, e, i: {x,y}, o: {x,y} }` |
| `Keyframe` (Hold) | `{ t, s, h: 1 }` |
| `StateMachine` | Lottie Interactivity JSON extension |

Lottie import follows the same mapping in reverse, reconstructing
`App` state from the JSON tree.

---

## State Machine Evaluation Engine

The `StateMachineEvaluator` (Phase 5) maintains:

```rust
pub struct Evaluator {
    machine: StateMachine,
    current_state_id: usize,
    blend_progress: f32,          // 0..=1 while transitioning
    pending_transition: Option<StateTransition>,
}

impl Evaluator {
    pub fn tick(&mut self, events: &[InputEvent], dt: f32) -> EvalOutput;
    fn check_triggers(&self, events: &[InputEvent]) -> Option<&StateTransition>;
    fn blend(&mut self, dt: f32);
}
```

Output includes:
- `playhead_range: (usize, usize)` — which frames to play from the active state
- `blend_alpha: f32` — crossfade weight when transitioning
- `looping: bool` — whether the active state loops

The evaluator is embedded in `CanvasHost::composite` so the canvas always
renders the correct blend of the current and next animation state.

---

## Crate Boundaries

```
apps/drift          — GPUI host, App state, panels, AI action stubs
shared/prism-canvas — GPU compositor (shared with Pigment)
shared/prism-core   — Blend modes, color adjustments (shared)
shared/prism-color  — sRGB↔linear (shared)
shared/prism-io     — PNG/JPEG/SVG I/O, .drift document serialization
shared/prism-media  — FFmpeg: MP4/WebM encode, audio mux (shared with Reel)
shared/prism-ui     — GPUI design tokens, icons, components (shared)
prism-ai (planned)  — ONNX runtime, model runners (shared with future apps)
```

Rule: code in `shared/` must be needed by ≥2 apps. Drift-specific logic
(state machines, puppet IK, Lottie serialization) lives in `apps/drift/` until
a second app needs it.
