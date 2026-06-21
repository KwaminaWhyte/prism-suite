# Drift — Research Notes
*Updated June 2026 — deep competitive + AI model analysis*

---

## 1. Competitor Matrix

| Feature | Rive | Jitter | Cavalry | Adobe Animate | Char. Animator | LottieFiles |
|---------|------|--------|---------|---------------|----------------|-------------|
| Primary use | Interactive UI animation | Marketing motion | Procedural/data-driven | Frame-by-frame, tweens | Live puppet (webcam) | JSON playback/sharing |
| Platform | Web editor + runtime SDKs | Web-only | Desktop Mac/Win | Desktop (CC) | Desktop (CC) | Web platform |
| Price | Free (1 file, watermark); $14–$49/mo; runtime license per app | Free; ~$15/mo | $299/yr indie | CC ~$55/mo | Bundled CC | Free; $19/mo Pro |
| State machines | First-class (inputs, conditions, blends) | None | Macro logic only | None | Behavior rules (basic) | dotLottie interactivity |
| AI features | None | None | None | Animate from Audio (beta) | Live face/voice tracking (CV, not ML) | None |
| Auto-rigging | Manual mesh deform + bones | None | None | None | Auto-tag (landmark detection) | None |
| Lip sync | None | None | None | None | Phoneme-to-viseme (CV, rough) | None |
| Export | .riv, SVG (limited) | Lottie, MP4, GIF | MP4, PNG seq | HTML5, MP4, GIF, SVG | MP4, live | JSON (Lottie), dotLottie |
| Offline | No | No | Yes | Yes | Yes | No |
| Physics | None | None | Procedural only | None | None | None |
| Runtime SDK | Yes (WASM/native) | No | No | HTML5 Canvas | No | Lottie player |
| Learning curve | Medium | Low | High | High | Medium | Low |

---

## 2. UX Wins / Fails Per Competitor

### Rive

**Copy these:**
- **State machine editor**: visual graph of states connected by transitions with boolean/number/trigger inputs. Users drag input values on a simulated preview to see blending live. Collapses game-UI logic into the animation tool — no code glue needed.
- **Nested artboards**: each artboard is self-contained; embed artboards inside others for component reuse.
- **Bone + mesh deform**: pin bones to vector paths, cage-deform the mesh with live preview.
- **Feathered constraints**: IK chains with constraints visualized as colored overlays directly on canvas.
- **Responsive layout anchoring**: artboard anchoring lets animations reflow for different viewport sizes without re-keyframing.

**Avoid these:**
- **Runtime licensing complexity**: #1 complaint on Reddit and Product Hunt. A "free" file can't be deployed commercially without a runtime license per app. Drift: perpetual license, no runtime fee.
- **No timeline scrubbing on state machine previews**: can't scrub through a state machine, must simulate it. Debugging transition timing is frustrating.
- **Path editing not Illustrator-quality**: no knife tool, no pathfinder booleans.
- **Web-only editor**: heavy users on slow connections or offline are blocked. Desktop is a real gap.
- **Export only to .riv**: SVG export incomplete; Lottie not officially supported.

### Jitter

**Copy these:**
- **Figma-adjacent UI**: mirrors Figma almost exactly. Any designer can pick it up in minutes.
- **Ease presets as first-class objects**: named presets (Spring, Bounce, Ease In/Out) with visual preview swatches and intensity slider.
- **Lottie export accuracy**: fewer rendering artifacts than AE+Bodymovin.
- **Template library**: curated animation templates for common UI patterns.

**Avoid these:**
- No real graph editor: power users hit a wall, can't fine-tune velocity curves.
- Shallow timeline: only opacity/position/scale/rotation. No path morphing, no mesh deform.
- Web-only, no offline mode: assets vanish if subscription lapses.
- No component/symbol system: animations can't be reused across a project.

### Cavalry

**Copy these:**
- **Data-driven animation**: bind CSV/JSON/live data feeds to animation properties.
- **Node graph for data flow visibility**: shows how generators feed into modifiers feed into renderers.
- **JavaScript scripting**: access properties directly without API overhead.

**Avoid these:**
- Extremely steep learning curve: node graph paradigm alienates traditional animators.
- No interactivity/state machines: output is video only.
- No web export: Lottie/SVG absent.
- Basic character animation: not designed for character work.

### Adobe Animate

**One thing worth copying:** the **dope sheet view** — all keyframes across all properties visible as dots in a spreadsheet-like grid. Best at-a-glance overview of timing density.

**Avoid everything else:**
- Flash legacy mental model (MovieClip, ActionScript, nested timeline symbols).
- Shape tweens are brittle: morph between two shapes and Animate guesses point correspondence badly.
- Classic vs Motion Tween confusion: two systems exist for historical reasons.
- HTML5 Canvas export uses proprietary `createjs` with browser compatibility issues.
- No GPU acceleration in editor: complex scenes drop below 12fps.

### Adobe Character Animator

**Study these patterns:**
- **Live performance capture loop**: performer sees puppet move as they perform — right feedback paradigm.
- **Puppet panel layer-name convention**: PSD layers auto-tag by name (left arm, right eye, etc.). Zero-setup for flat character designs.
- **Trigger palette**: keyboard keys trigger behaviors; timeline records the moments.

**Avoid these:**
- Completely dependent on Photoshop layer naming conventions.
- Face tracking is CV-based (dlib 68-point landmarks): collapses under poor lighting or glasses.
- No export to web interactive formats — output is video only.
- 8-viseme lip sync: "bobble-head mouth" without per-phoneme manual override.

### LottieFiles — Format Strategy Implications

- Lottie (JSON-based, AE + Bodymovin) is the de facto standard for lightweight interactive animation on web and mobile. iOS, Android, React Native, Flutter all have first-class Lottie players.
- **dotLottie** (.lottie): zip-wrapped Lottie JSON + bundled assets. Smaller payload; supports themes and state machine interactivity metadata. Drift writes dotLottie natively, Lottie JSON as fallback.
- Drift is Lottie-native: every primitive maps 1:1 to the Lottie spec. No "not supported in export" warnings. Property panel shows Lottie-compatibility indicator in real time.

---

## 3. AI Model Recommendations

### Auto-Rigging (2D Character Art)

**Primary — DWPose** (Yang et al., 2023)
- WholeBody pose estimator: 133 keypoints (body + hands + face). RTMDet-m + RTMPose-l backbone.
- Official repo includes `export_onnx.py`. Fully offline. Robust on 2D illustration (COCO-WholeBody + style augmentation).
- **Drift workflow**: run DWPose on imported character art → propose skeleton overlay → user confirms/adjusts → build IK rig.

**Secondary — SAM 2** (Meta, mid-2024)
- Video-consistent segmentation, ONNX-compatible via `sam2-hiera-small`.
- **Drift workflow**: run before DWPose to segment a flat PNG into semantic body-part layers.

**Mesh binding**: ARAP (As-Rigid-As-Possible) deformation (classical, OpenCV). Bind mesh triangles to nearest detected bone.

**Pipeline**: `DWPose (ONNX) → SAM 2 (ONNX) → ARAP mesh binding`

### Motion Generation (Text-to-Keyframe)

**Primary — MDM (Motion Diffusion Model)** (Tevet et al., 2022)
- Generates human skeleton motion (SMPL joint angles) from text prompts. Standard transformer, ONNX-exportable (~300MB).
- Output is **editable keyframes**, not pixels — critical differentiator vs AnimateDiff.
- **Drift use**: text-to-keyframe on existing rigs. Output 3D skeleton → reproject to 2D → retarget to Drift rig → lay down editable keyframes.

**Secondary — MotionGPT** (Jiang et al., 2023)
- T5-based, treats motion as language tokens. Better for complex multi-action sequences.
- **Drift use**: "run across the room, trip, catch yourself, look embarrassed."

**Reference only — AnimateDiff**
- UNet conditioned on CLIP text embeddings + temporal attention modules. ONNX-exportable from HuggingFace diffusers.
- Outputs pixels, not keyframes — use as motion reference layer only, not final output.

### Lip Sync

**Default — Rhubarb Lip Sync** (MIT license)
- Not ML, classical phoneme-to-viseme using Pocketsphinx. Extremely reliable offline, trivially embeddable.
- Maps to Preston Blair 12-viseme system. Speed: <3s for 30-second audio clip.

**Quality mode — wav2vec2-phoneme** (ONNX)
- `facebook/wav2vec2-base-960h` fine-tuned on TIMIT phoneme dataset. CTC head → ARPAbet phoneme probabilities.
- Runs entirely offline on CPU, under real-time for typical speech. HuggingFace Optimum ONNX export.
- Map ARPAbet → Preston Blair viseme set → keyframe viseme blend weights on facial rig.

**Recommendation**: Rhubarb as default (MIT, fast, reliable, zero inference cost), wav2vec2-phoneme as opt-in quality mode.

### Frame Interpolation

**Default — RIFE v4.6** (Huang et al., 2020)
- IFNet architecture (~20MB ONNX). Official repo provides ONNX export. Used in production by Flowframes.
- Performance: 1080p at ~30fps on RTX 3060, ~5fps on CPU.
- **Drift use**: auto-generate in-betweens between keyframed poses at preview or bake time.

**Quality mode — FILM** (Reda et al., Google, 2022)
- Feature pyramid; handles large displacements better than RIFE. ~80MB, slower (~180ms/frame at 1080p M3).
- ONNX-exportable via TF Lite → ONNX conversion (HuggingFace `film-net-style`).
- **Drift use**: slow-output/render-quality interpolation for large motion displacements.

### Style Transfer (Frame-by-Frame, Flicker-Free)

**Primary — EbSynth-style keyframe propagation**
- User paints one stylized keyframe; propagate to surrounding frames using optical flow. Near-zero flicker by design.
- Implement inline in Drift: user paints one frame with Drift's brush tools → propagates to selected frame range without leaving the app.

**AI style mode — Rerender-A-Video** (Yang et al., 2023)
- ControlNet (Canny/Depth/HED) anchors structure; cross-frame attention propagates appearance from first frame. Reduces flicker vs per-frame diffusion. ControlNet ONNX-exportable.

### Physics (Cloth / Hair)

**v1 — Verlet integration springs** (classical, CPU, real-time)
- Zero inference cost. Well-understood. Sufficient for 2D secondary motion.
- User selects a rig control → enables "Secondary Motion" → Drift adds procedural spring/damping follow on children.
- Two sliders: stiffness, damping. Jiggly ears, bouncy hair, clothing lag — all from these.

**Post-v1 — Neural cloth**: SNUG, HOOD (GNN cloth) not yet production-ready for 2D pipelines. Reserve for future.

### Rust Inference Runtime

**`ort` crate (pykeio/ort)** — Rust interface for ONNX Runtime (Microsoft C++ library)
- Supports CUDA, TensorRT, CoreML, DirectML, OpenVINO execution providers.
- Strategy: export every model to ONNX, load via `ort`, handle preprocessing/postprocessing in Rust.
- `tract` (pure Rust, no C++ deps) as alternative for simpler models; less hardware accelerator support.

---

## 4. Drift's Differentiating Features (No Competitor Has These)

1. **AI-Rigged Import Pipeline**: import flat PNG/SVG → DWPose detects skeleton → SAM 2 segments body parts → auto-IK rig with mesh deform. Character Animator requires pre-layered PSD. Rive requires manual bone placement. No one-click rig from flat image exists elsewhere.

2. **Text-to-Keyframe on Existing Rigs**: type motion description → MDM/MotionGPT → retarget to user's rig → editable keyframes. Output is data, not pixels — artist retains full control.

3. **AI Lip Sync From Audio File**: drop audio → Rhubarb/wav2vec2 → auto-viseme keyframes on rig's face controls, editable. Character Animator requires live performance; Drift works offline from a file.

4. **State Machine + Timeline Unified**: Rive has state machine but no frame-by-frame timeline. Animate has timeline but no state machine. Drift merges both. Double-click a state → opens its timeline inline.

5. **RIFE Frame Interpolation on Keyframe Gaps**: between two posed keyframes, generate in-betweens via RIFE on rasterized frames. AI keyframes shown in purple; can be promoted to authored.

6. **Lottie-Native (Not Export-as-Afterthought)**: every Drift primitive maps 1:1 to Lottie spec. Real-time Lottie-compatibility indicator in property panel.

7. **Offline-First Desktop, No Runtime Licensing**: all AI inference local via ONNX Runtime. Perpetual license, no runtime deploy fee (Rive's #1 complaint).

8. **Data-Binding for UI Prototyping**: import JSON/CSV → bind to animation properties → preview with live data. Cavalry-style power, targeted at product designers.

9. **EbSynth-Style Style Propagation Inline**: paint one stylized frame → propagates to frame range without leaving Drift.

10. **Secondary Motion From a Single Toggle**: select rig control → enable Secondary Motion → Verlet spring/damping follow on children. No hand-keying overlapping action.

---

## 5. Export Format Strategy

### Priority 1 — dotLottie (.lottie)
Lingua franca of UI animation. iOS (lottie-ios), Android (lottie-android), React Native, Flutter, Web (lottie-web) all have first-class players. Typical icon animation: 5–30KB.
- dotLottie = zip-packaged Lottie JSON + bundled images/fonts + theme definitions + interactivity metadata.
- Drift writes dotLottie natively with: multi-theme slots (light/dark), state machine metadata, embedded bitmap fallbacks for non-spec features (mesh deform rasterizes per-frame, flagged explicitly).
- Target Lottie spec v5.x. No AE-specific undocumented properties.

### Priority 2 — MP4 (H.264 / HEVC)
Universal lowest-common-denominator. Render via FFmpeg at configurable quality (CRF 18–28).
- 2x resolution option for retina. sRGB output (encode at display, internal is linear). Social presets: 1:1, 9:16, 16:9.

### Priority 3 — WebM (VP9 with Alpha)
MP4 H.264 can't carry alpha. WebM VP9 is the web standard for video-with-alpha.
- Chrome, Firefox, Safari 16+ support WebM VP9 alpha. Export at quality 30–40 (VP9 CRF scale).

### Priority 4 — GIF
256 colors, 1-bit transparency, but universally supported (Slack, Discord, email). Generate via **gifski** (Rust-native, perceptual palette quantization — best quality of any open GIF encoder).

### Priority 5 — PNG Spritesheet / Sequence
- Spritesheet: all frames packed into single PNG with JSON atlas in Texture Packer format (Unity, Godot, Phaser compatible).
- PNG sequence: numbered frames for AE import, video compositing.

### Priority 6 — Animated SVG
SMIL-animated SVG for simple vector animations (icon animations, loaders). Export for complexity where browsers handle it well; warn otherwise.

### Format-Feature Compatibility Matrix (show in export panel)

| Feature | Lottie | MP4 | WebM | GIF | Spritesheet | SVG |
|---------|--------|-----|------|-----|-------------|-----|
| Vector paths | Yes | Rasterized | Rasterized | Rasterized | Rasterized | Yes |
| Alpha channel | Yes | No | Yes | 1-bit | Yes | Yes |
| State machine | dotLottie only | No | No | No | No | Partial |
| Mesh deform | No (raster fallback) | Yes | Yes | Yes | Yes | No |
| Audio | No | Yes | Yes | No | No | No |
| Max file size | ~500KB ideal | Unlimited | Unlimited | 10MB recommended | Unlimited | ~200KB ideal |

---

## 6. 10 UI/UX Principles for Drift

1. **Timeline is sovereign.** Fixed 20–35% vertical screen at all times. Never auto-collapsed. Both frame number and timecode (HH:MM:SS:FF), user-switchable. Layer list + keyframe channel layout matching AE.

2. **Easing is a first-class object, not a hidden panel.** Tangent handles directly on keyframes in timeline. Graph editor mode toggle. Ease presets palette as persistent dock with mini-curve thumbnails. "Ease arc" glyph between keyframes — timing intent readable at a glance.

3. **The canvas is the primary interaction surface — tools adapt to context.** Hover → transform handles. Click bone → IK controls. `E` → bezier path edit. `R` → rig pose mode. Keyboard shortcuts match After Effects (Space=play, `U`=show animated, `J`/`K`=jump keyframes, `,`/`.`=prev/next frame).

4. **Show what's AI-generated vs authored — never mix silently.** AI keyframes in purple, authored in white. RIFE in-betweens = AI keyframes. Promote to authored: right-click → "Promote to authored." Deleting AI keyframe triggers re-generation from neighbors.

5. **State machine editor must be spatial, not hierarchical.** States are rectangles; transitions are arrows with condition expressions annotated directly on arrow. Simulate panel: set input values → watch state transitions live. Each state double-clickable to open timeline inline. Blend trees drawn as horizontal blend bar with markers.

6. **Onion skinning that's actually useful.** Per-layer toggle. Configurable "N frames before, M frames after" slider. Past = blue, future = orange, opacity fades linearly. Rig ghost silhouettes for posed characters. Motion path display for animated points — click any point on arc to jump to that frame.

7. **Rigging UI must be spatial and inline.** Bones drawn as capsules with head/tail. Click tail → drag to extend child bone. `I` key on two selected bones → IK chain created. Weight painting via brush with color overlay. Constraint badges on each bone — click to expand inline.

8. **Real-time preview is non-negotiable.** GPU-accelerated composite at every scrub position. RIFE-interpolated segments cached at preview resolution on idle CPU/GPU. AI keyframes not yet computed show shimmer, never incorrect value. Never black frame during scrub. Performance budget: ≤16ms for scenes up to 50 layers.

9. **Import friction must be zero for common sources.** Figma: copy shapes → paste as native vectors via SVG clipboard. SVG files: drag-and-drop, paths preserved, groups become layers. PSD: each layer = Drift layer, masks respected. Lottie/dotLottie: import as editable Drift project. Video: import as reference layer for rotoscoping. No import wizard dialogs.

10. **Feedback loops close in under 200ms.** Keyframe drag: canvas redraws at new pose within 16ms. Ease change: playback updates within 100ms. AI lip sync: first viseme keyframes appear within 3s for 30-second clip (Rhubarb). RIFE cache: 24-frame gap caches within 2s on GPU. Rig pose update: mesh deforms within 8ms (GPU skinning). 200ms rule tested in CI as performance benchmark.

---

## 7. Open Questions

1. **prism-ai promotion timing.** Criterion: if Reel needs Whisper for auto-captioning or Pigment needs neural upscale, promote. Until then, ONNX wrappers stay in `apps/drift/src/ai/`.

2. **FILM vs RIFE default.** Offer both via `ExportConfig::interpolation_model`. Default RIFE for preview, FILM for final export.

3. **Lottie Interactivity v2 spec.** May need custom extension for conditional expressions. Contribute back upstream.

4. **Webcam permission model.** `NSCameraUsageDescription` on macOS, V4L2 on Linux. Gate behind capability flag in build.

5. **Character Animator scene recorder.** Buffering live webcam at 60fps while rendering composited output. Design: dedicated capture thread → ring buffer → compositor reads latest frame.

6. **Pricing model.** Perpetual license + optional update subscription. Avoid Rive's runtime license per-deploy-target fee — cited as #1 barrier to adoption by indie developers.
