# Drift Research Notes

## Competitor Landscape

### Rive (rive.app)
**Strengths:**
- State machines are the core data model, not an afterthought — every
  interactive animation is built around a graph of states and conditions
- Runtime is tiny (< 500 KB WASM) and available in every platform target
- Inputs system (numbers, booleans, triggers) wires UI state to animation state
  without code
- Artboard concept separates the canvas coordinate system cleanly from content
- Nested artboards allow component-level animations

**Weaknesses:**
- No AI features whatsoever
- No audio support
- No video layer / bitmap frame sequences
- Character rigging is limited vs Character Animator
- No scripting language for complex interactivity

**Drift response:** Implement state machines as a first-class concept (done in
`StateMachine`, `AnimationState`, `StateTransition`), but surpass Rive with AI
motion generation and full puppet rigging.

---

### Jitter (jitter.video)
**Strengths:**
- Extremely approachable UX — non-animators can produce motion graphics
  in minutes
- Web-native: design in browser, preview in browser
- Good component/variant system borrowed from Figma
- Reasonable After Effects tween compatibility

**Weaknesses:**
- No code/scripting — limited expressiveness
- No rigging / character animation
- No AI features
- No self-hosting / open source

**Drift response:** UX simplicity is a goal (see UX Principles below), but Drift
targets the professional animator audience that needs rigging, scripting, and AI.

---

### Cavalry (cavalry.scenegroup.com)
**Strengths:**
- Procedural / generative approach — parameters drive everything
- Strong spreadsheet-like data system for driving animation from tables
- Reusable behaviors (like After Effects expressions but visual)
- Excellent for data-driven infographic animation

**Weaknesses:**
- macOS only (no Linux, no Windows stable)
- Steep learning curve — not approachable for character animators
- No puppet rigging
- No AI

**Drift response:** Support Rhai scripting (Phase 6) for procedural power, but
keep the default UX timeline-first like Animate.

---

### Adobe Animate 2025
**Strengths:**
- Industry standard for interactive web animation (HTML5 Canvas output)
- Full ActionScript 3 runtime — mature scripting
- Excellent symbol + nested timeline system
- Classic Tween and Motion Tween both well-developed
- Publishing directly to HTML5, SVG, WebGL, video

**Weaknesses:**
- No AI features
- Subscription-only ($55/mo or CC All Apps)
- Closed source
- Performance degrades on large documents
- ActionScript ecosystem is aging

**Drift response:** Target full timeline/symbol/tween parity in Phases 1–2, then
surpass with AI in Phase 4.

---

### Adobe Character Animator 2025
**Strengths:**
- Real-time webcam-driven puppet animation — unique in the market
- Lip sync from microphone input, near real-time
- Spring joints for secondary motion
- Trigger behaviors from keyboard shortcuts
- Scene panel for live recording

**Weaknesses:**
- Tightly coupled to webcam — offline AI fallback is primitive
- Only works well with pre-rigged Photoshop/Illustrator characters
- No Lottie / web export
- No state machine for interactive use

**Drift response:** Replicate webcam input in Phase 3 (MediaPipe pose tracking),
surpass with AI-driven offline lip sync (wav2vec2/Whisper) in Phase 4.

---

## AI Models

### AnimateDiff (motion generation)
- Architecture: UNet conditioned on CLIP text embeddings + temporal attention modules
- Format: ONNX fp16 export from HuggingFace diffusers (animatediff-motion-adapter-v1-5-3)
- Input: text prompt + noise latent (optional image conditioning)
- Output: N frames of motion deltas (suitable for keyframe generation)
- Latency (M3 Max, fp16): ~8s for 16 frames at 512×512
- Drift integration: `prism-ai::animatediff::generate(prompt, layer_id)` → Vec<Keyframe>

### FILM (Frame Interpolation for Large Motion, Google Research)
- Architecture: Feature pyramid + flow estimation + synthesis network
- Format: ONNX fp32 (TF Hub model converted)
- Input: two RGB frames (any resolution, padded to 64-divisible)
- Output: single interpolated frame
- Recursive application yields 2^N-1 in-betweens
- Latency (M3 Max, fp32): ~180ms per frame pair at 1080p
- Drift integration: `prism-ai::film::interpolate(frame_a, frame_b, num_steps)` → Vec<RgbaImage>

### RIFE 4.6 (Real-Time Intermediate Flow Estimation)
- Architecture: IFNet with lightweight deformable convolution
- Format: ONNX fp16 (rife-onnx from nihui)
- Input: two RGB frames
- Output: single interpolated frame
- Latency: ~12ms per frame pair at 1080p on M3 (GPU ONNX)
- Better choice than FILM for real-time preview; FILM for final export

### wav2vec2-base (phoneme detection)
- Architecture: CNN encoder + Transformer, pre-trained on LibriSpeech
- Fine-tuned on TIMIT phoneme dataset
- Format: ONNX fp32 (HuggingFace Optimum export)
- Input: 16kHz mono audio PCM
- Output: phoneme logits at ~50 fps
- Phoneme-to-viseme mapping: P1=open, P2=mid-open, P3=rounded, etc. (Preston set)
- Drift integration: `prism-ai::lipsync::phonemes_from_audio(path)` → Vec<(frame, viseme)>

### Whisper tiny (speech-to-text for caption sync)
- Format: ONNX fp32 (openai/whisper-tiny)
- Output: word-level timestamps (for timeline caption sync, Phase 6)

### MoveNet Thunder (pose estimation)
- Architecture: MobileNetV2 + FPN
- Format: ONNX int8
- Input: 256×256 RGB frame
- Output: 17 keypoints with confidence scores
- Drift maps keypoints → bone targets for webcam-driven character animation

---

## UX Principles for Drift

1. **Timeline is sovereign.** Every property change that matters is animated. The
   inspector is a shortcut to the timeline, not a replacement for it.

2. **AI is a collaborator, not a magic button.** AI actions produce *editable*
   keyframes that the animator can inspect, tweak, and delete. No black-box results.

3. **State machines are visual.** Transitions are drawn as arrows between labeled
   state boxes. No JSON editing required.

4. **Bones are click-to-place.** Auto-rigging gives a starting point; the
   animator drags bones into position in the canvas, not via numeric fields.

5. **Non-destructive everything.** Layer order, blend modes, IK, and effect
   stacks are all reversible. No operation burns pixels into a layer without
   an explicit "rasterize" command.

6. **Preview fidelity matches export fidelity.** What you see in the canvas at
   1:1 zoom is exactly what the exported video/Lottie will look like. No
   "export surprises."

7. **Keyboard-first timeline navigation.** Space = play/pause. Arrow keys =
   step frame. Home/End = first/last frame. J/K/L = shuttle (planned). Same as
   Premiere/After Effects muscle memory.

8. **Onion skinning on by default.** New animators benefit from seeing previous
   frames. Power users can turn it off. Default to on with low opacity.

9. **Library is a first-class panel.** Symbols, imported assets, and AI results
   all live in one searchable library. Drag to canvas = instantiate.

10. **Export is one click from anywhere.** A persistent export button in the
    toolbar opens a panel with the last-used settings pre-filled. No wizard.
    Cmd+Shift+E exports immediately with current settings.

---

## Lottie Compatibility Matrix

| Feature | Lottie v5 | Drift Phase 5 |
|---------|-----------|--------------|
| Solid layers | Yes | Yes |
| Shape layers | Yes | Yes |
| Image layers | Yes | Yes |
| Null layers | Yes | Yes |
| Text layers | Yes | Planned |
| Camera | No | Planned (custom ext) |
| 3D layers | Partial | Planned |
| Expressions | AE-subset | Rhai (Phase 6) |
| State machines | Interactivity v1 | Yes (native) |
| Audio | No | Custom ext |
| Masks | Yes | Planned |
| Track matte | Yes | Planned |

---

## Open Questions

1. **prism-ai promotion timing.** When does the ONNX wrapper become its own
   shared crate vs staying in `apps/drift/src/ai/`? Criterion: if Reel or Pulse
   needs inference (e.g., Reel auto-captioning with Whisper), promote.

2. **FILM vs RIFE default.** Offer both via `ExportConfig::interpolation_model`.
   Default to RIFE for preview (fast), FILM for final export (quality).

3. **Lottie Interactivity v2 spec.** The current spec is incomplete around
   conditional expressions. May need to implement a custom extension and
   contribute back upstream.

4. **Webcam permission model on macOS/Linux.** Camera access requires
   `NSCameraUsageDescription` plist key on macOS and V4L2 on Linux. Need to
   gate this behind a capability flag in the build.

5. **Character Animator scene recorder.** Buffering live webcam frames at 60fps
   while also rendering the composited output is a significant threading problem.
   Design: dedicated capture thread → ring buffer → compositor reads latest frame.
