# Drift

**AI-first desktop animation studio** — part of the [Prism Suite](../../README.md).

Drift targets ≥90% feature parity with Adobe Animate + Character Animator, with a
differentiated AI layer for motion generation, lip sync, and frame interpolation
that neither Adobe product offers. It is designed as a professional tool for
character animators, motion designers, and interactive developers.

---

## Key Features

### Animation Core
- **Timeline-first** — every property change is animatable via keyframes
- **7 easing types** — Linear, EaseIn/Out, EaseInOut, Bezier, Hold, Spring
- **Layer hierarchy** — Vector, Bitmap, Audio, Camera, Guide, Null layer kinds
- **Layer parenting** — nested transforms, z-ordering, color tags
- **Playback controls** — play/pause/stop, in/out points, loop, frame stepping

### Puppet Rigging
- **One-click auto-rig** — 8-bone humanoid scaffold (hip, torso, neck, head, arms, legs)
- **Manual bones** — add, move, rotate, delete; parent/child hierarchy
- **IK targets** — pin bone chain endpoints for natural limb motion
- **Mesh deformation** — freeform mesh warp on Bitmap layers (Phase 3)
- **Webcam live rig** — drive character from webcam pose tracking (Phase 3)

### AI Differentiators (vs Rive / Adobe Animate)
- **Text-to-animation** — describe motion in plain language; AI generates keyframes
  via AnimateDiff ONNX inference
- **AI lip sync** — drop in an audio file, get mouth-open keyframes automatically
  (wav2vec2 / Whisper phoneme detection)
- **AI frame interpolation** — FILM and RIFE ONNX models generate clean in-betweens
  from two extreme keyframes; no manual tweening required
- **AI auto-rig** — pose estimation from reference image seeds the bone scaffold
- All AI results produce **editable keyframes** — nothing is a black box

### Export
- **MP4** (H.264 via FFmpeg), **WebM**, **GIF**, **APNG**, **PNG sequence**
- **Lottie JSON** — first-class export for web interactive animation
- **Spritesheet** — configurable grid for game engine use
- Quality, scale, fps, transparency, and frame range all configurable

### State Machines (Interactive Animation — Rive parity)
- Named states with in/out frame ranges and looping
- Transitions with triggers: OnClick, OnHover, OnComplete, OnCondition(expr)
- Blend duration per transition
- Visual state machine editor (Phase 5)
- Publish as self-contained HTML + Lottie Interactivity bundle

---

## Build

```bash
# From the workspace root:
cargo run -p drift          # run Drift
cargo check -p drift        # typecheck
cargo test -p drift         # run all tests
```

Requirements: same as the rest of the Prism suite (Rust 1.78+, macOS 13+ or
Linux with Wayland/X11, a GPU with Vulkan/Metal/DX12 support for GPUI).

---

## Architecture

Drift uses **GPUI** as the UI host. All state lives in `App` (in
`src/app_state.rs`) and mutates only through `App::apply(Action)`. Panels read
`&App` and emit `Action` variants via `cx.listener`. This keeps the panel↔state
boundary narrow so panels can be built in parallel.

The animation canvas is composited by `prism-canvas` (shared with Pigment) and
bridged into GPUI as a `RenderImage`. The keyframe interpolation engine samples
all animated properties each frame; the puppet IK solver resolves bone chains;
the state machine evaluator determines which animation range is active.

For AI: the planned `prism-ai` crate wraps ONNX Runtime. Models live in
`~/.drift/models/` and are loaded lazily. AI actions in `App::apply` are stubs
today; they will call into `prism-ai` once the crate is promoted.

See [ARCHITECTURE.md](ARCHITECTURE.md) for the full module diagram and crate
boundaries.

---

## Roadmap

See [PLAN.md](PLAN.md) for the phase-by-phase feature roadmap and parity
tracking table.

Current status: **Phase 1 (20% parity)** — core data model, 120+ actions,
100+ tests.

---

## Comparison with Competitors

| Feature | Drift | Adobe Animate | Rive | Jitter | Cavalry |
|---------|-------|--------------|------|--------|---------|
| Timeline + keyframes | Yes | Yes | Yes | Yes | Yes |
| Puppet rigging | Yes | Limited | No | No | No |
| State machines | Yes | No | Yes | No | No |
| AI motion gen | Yes | No | No | No | No |
| AI lip sync | Yes | No | No | No | No |
| AI interpolation | Yes | No | No | No | No |
| Lottie export | Yes | Plugin | Yes | Yes | No |
| Open source | Yes | No | No | No | No |
| Price | Free | $55/mo | Free/Pro | Free/Pro | $20/mo |

---

## License

MIT OR Apache-2.0 — same as the rest of the Prism suite.
