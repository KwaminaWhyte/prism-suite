# Prism Suite — Research Findings (June 2026)

Suite-level research backing [SUITE.md](./SUITE.md) (vision + interop) and the six app plans. This doc
covers the **shared engine, the interop mechanisms, and the cross-cutting policies** (color, AI) that all
six apps inherit. App-specific findings live in each app's own `RESEARCH.md`:

- [Pigment](apps/pigment/RESEARCH.md) — raster engine, compositor, blend math, brush, color IO, AI tools
- [Contour](apps/contour/RESEARCH.md) — vector path engine, booleans, SVG/PDF, image trace, gradients
- [Pulse](apps/pulse/RESEARCH.md) — time-addressable compositing, keyframes/expressions, effects, media
- [Reel](apps/reel/RESEARCH.md) — NLE editing model, video/audio decode, transitions, color, export
- [Drift](apps/drift/RESEARCH.md) — animation engine, puppet rig, tweening, AI motion gen + lip sync
- [Tone](apps/tone/RESEARCH.md) — DAW architecture, MIDI, CPAL audio, AI music generation (MusicGen/Demucs/Magenta)

> Verify every crate version against crates.io at build time — third-party version metadata is sometimes
> stale. `wgpu` confirmed at **29.0.3** (2026-05-02); the `"29"` pin holds suite-wide.

---

## 1. The architectural bet — one engine, six apps

Adobe's moat isn't any single app; it's that the apps **interoperate** because they share a layer/
compositor/color engine. Prism gets the same property *if and only if* that engine is one codebase. The
key realization that makes this tractable:

> **Raster, vector preview, video frames, and motion comps all reduce to the same operation —
> composite tiles through a DAG of blend/effect nodes, in linear light, cached by what's dirty.**

- **Pigment** runs that compositor over raster layers.
- **Contour** is resolution-independent paths *rasterized through* the same compositor for preview/export.
- **Pulse** adds a **time axis**: every node is sampled at frame `t` (keyframes/expressions); cache key
  gains a frame dimension.
- **Reel** adds **clips on tracks with source in/out ranges**: the program frame is that same composite
  sampled at the playhead, plus an audio mix.
- **Drift** adds an **animation-first timeline**: vector/bitmap layers keyed over time, puppet rigs,
  tweening, and AI motion generation — the compositor serves as the wgpu canvas for frame preview.
- **Tone** is orthogonal: instead of pixels it owns **audio buffers** mixed through a DAG of tracks/clips/
  buses. The shared DAG/dirty-cache model still applies; the leaf nodes are PCM frames not tiles.

So Pulse = Pigment's compositor + time; Reel = compositor + clip model + media; Drift = compositor + animation
timeline + puppet rig; Tone = audio DAG + MIDI + AI synthesis. Build the shared layer **time-agnostic,
clip-agnostic, and pixel-agnostic** — app-specific semantics are each a thin layer on top.

Sources: [SUITE.md](./SUITE.md) · [Pigment RESEARCH.md §2](apps/pigment/RESEARCH.md) · [Pulse RESEARCH.md §1](apps/pulse/RESEARCH.md) · [Drift RESEARCH.md](apps/drift/RESEARCH.md) · [Tone RESEARCH.md](apps/tone/RESEARCH.md)

## 2. Shared crate matrix (current + planned)

Shared crates live at `prism/crates/` as their own workspace; every app path-deps in via `../crates/`
and **does not copy or modify** them.

| Crate | Status | Owns | Consumers |
|---|---|---|---|
| `prism-core` | **exists** | doc model, `Size`/`Rect`, color boundary, blend modes (18), tile types, adjustments, curve/histogram | pigment, contour, pulse, reel, drift |
| `prism-color` | **exists** | sRGB/linear, ICC (`lcms2`/`qcms`) — grows OCIO, CMYK, spot, soft-proof | all six apps |
| `prism-io` | **exists** | image load/export, PSD/EXR, resize, `.pigment`, text raster | pigment, reel, drift |
| `prism-canvas` | **exists** | GPU compositor (wgpu/WGSL passes: composite, display, dab, filter, selection) | pigment (owner); drift (animation frame preview) |
| `prism-media` | **exists** | FFmpeg decode/encode (H.264 MP4, audio mux), frame-accurate seek, audio tracks | pulse + reel (co-owned); tone (audio clips) |
| `prism-ui` | **exists** | GPUI design system: design tokens, SVG icon set, shared components | all six apps |
| `prism-vector` | **planned** | paths/anchors/handles, booleans (`i_overlay`), stroking/offset (`kurbo`), tessellation (`lyon`) | contour (owner), pigment (shape layers), pulse (masks), drift (vector layers) |
| `prism-fx` | **planned** | OpenFX-style GPU effect/transition host — author once, run everywhere | pigment, contour, pulse, reel |
| `prism-ai` | **planned** | `ort` (ONNX) runtime + provider abstraction + on-demand model cache | drift + tone (first users); pigment + pulse + reel (segmentation, inpaint, upscale) |
| `prism-doc` | **planned** | interchange container (layer tree + scene graph + media refs) + Dynamic-Link node | all (interop) |

**Promotion discipline:** code is promoted to a `prism-*` crate only when it is genuinely generic. The
README/PLAN of each app names a "coordinate before promoting" step so the owning agents agree on a shape
that serves *all* consumers (e.g. `prism-vector` must satisfy Contour authoring **and** Pigment shape
layers **and** Pulse masks). Never raster-couple, time-couple, or clip-couple a shared crate.

Sources: prism/Cargo.toml (workspace members) · each app's Cargo.toml (path deps) · [Contour RESEARCH.md §1](https://github.com/KwaminaWhyte/prism-suite/blob/main/RESEARCH.md) · [Pulse RESEARCH.md §6](https://github.com/KwaminaWhyte/prism-suite/blob/main/RESEARCH.md) · [Reel RESEARCH.md §2–3](https://github.com/KwaminaWhyte/prism-suite/blob/main/RESEARCH.md)

## 3. Interop mechanisms (the Adobe-parity features)

All six reduce to **one render-graph-node abstraction** plus a shared container; build it suite-aware
from the start.

1. **Dynamic Link** — a node that evaluates a *linked* document on demand (at the requested time/
   resolution/tile) and caches the result. A Pulse comp in a Reel timeline, a Contour artboard in a
   Pigment doc, a nested sequence — all the **same** node, differing only in what they evaluate. Producer
   = Pulse (comps → Reel); consumers = Reel/Pigment/Contour.
2. **Smart objects / live placement** — the same node, embedded rather than externally linked; stays
   editable at source resolution, re-rasterized on transform. Pigment's smart objects = this node.
3. **`prism-doc` interchange container** — one format every app reads (layer tree + scene graph + media
   refs); lossy-but-faithful bridges to PSD/AI/SVG/AEP/Premiere-XML/OTIO. Defined with the suite.
4. **Shared clipboard** — copy a path/layer/keyframe/color in one app, paste editable in another (shared
   in-memory model + serialized fallback).
5. **One color pipeline** — `prism-color` means a swatch/look is identical across all four and on export.
6. **Shared effects** — `prism-fx` effects authored once run in any compositing app.
7. **Shared asset library** — brushes, gradients, LUTs, fonts, templates, swatches in a common store.

Sources: [SUITE.md](./SUITE.md) §"Interop mechanisms" · [Pulse RESEARCH.md §10](https://github.com/KwaminaWhyte/prism-suite/blob/main/RESEARCH.md) · [Reel RESEARCH.md §1,7](https://github.com/KwaminaWhyte/prism-suite/blob/main/RESEARCH.md)

## 4. One color pipeline

All apps composite in **linear-light premultiplied** and manage color through `prism-color`:
- **`lcms2`** (Little CMS 2.17) primary engine — ICC v2/v4, CMYK/Lab/XYZ, soft-proof, intents;
  **`qcms`** pure-Rust RGB/gray fast path for wasm.
- **OpenColorIO** (ASWF Rust binding in progress) for the video/VFX apps (Pulse/Reel) — config-driven
  input/working/display/output transforms + creative looks; **`exr`** (pure Rust) covers scene-linear
  EXR meanwhile.
- Float working buffers (`Rgba16Float`; `f32` on demand); sRGB/transfer encode only at the final
  display/export boundary; HDR (>1.0, PQ/HLG) preserved end-to-end.
The payoff: a color picked in Contour, graded in Reel, and composited in Pulse is the **same color**, and
exports match.

Sources: [Pigment RESEARCH.md §2–3,9](https://github.com/KwaminaWhyte/prism-suite/blob/main/RESEARCH.md) · [Pulse RESEARCH.md §5](https://github.com/KwaminaWhyte/prism-suite/blob/main/RESEARCH.md) · [Reel RESEARCH.md §6](https://github.com/KwaminaWhyte/prism-suite/blob/main/RESEARCH.md) · aswf.io (OCIO/OpenEXR Rust)

## 5. Shared AI policy (`prism-ai` / `ort`)

One runtime, one policy across the suite:
- **Runtime:** **`ort`** (ONNX Runtime) with **CoreML** (macOS), **DirectML** (Windows), **CUDA/TensorRT**
  (NVIDIA) execution providers; **`candle`**/**`tract`** pure-Rust fallback (wasm / no-EP).
- **Models are never bundled.** Fetched to a shared cache on first use **behind a feature flag**, with
  each weight's **license surfaced** (segmentation/restoration weights mostly MIT/Apache; diffusion
  weights carry OpenRAIL terms). Every AI tool **degrades gracefully** when models/GPU are absent.
- **Shared models across apps:** segmentation/matting (**SAM2/SAM3**, **BiRefNet_dynamic**, **RMBG-2.0**)
  power select-subject (Pigment), trace-region (Contour), roto (Pulse), object-mask/auto-reframe (Reel);
  inpaint (**LaMa**) powers content-aware fill (Pigment) and video CAF (Reel); super-res (**Real-ESRGAN/
  SwinIR**) is shared; transcription (**Whisper-class**) powers captions/text-based editing (Reel).
- **Drift-specific models:** motion generation (**MoCoGAN/AnimateDiff**), frame interpolation
  (**FILM/RIFE**), lip sync from audio (**Wav2Lip**), facial landmark capture (webcam → rig drive).
- **Tone-specific models:** text-to-audio (**MusicGen-small** ONNX), stem separation (**Demucs HTDemucs-4**),
  MIDI generation (**Magenta Melody RNN / Music Transformer**), chord harmonization.
- **Generative fill / expand / extend — explicit suite policy: OPTIONAL and PLUGGABLE.** It runs via a
  provider abstraction with **two interchangeable backends — a local diffusion model (`candle`/ONNX) and
  a user-configured cloud endpoint (bring-your-own API key)** — plus "none". It is **never required** for
  core editing; the apps are fully functional with no AI backend configured. This applies uniformly to
  Pigment (Generative Fill), Contour (Generative Recolor/vectorize), Pulse (Generative Extend), Reel
  (Generative Extend), Drift (Motion Generate), and Tone (Generate Track / Harmonize).

Sources: [Pigment RESEARCH.md §10](https://github.com/KwaminaWhyte/prism-suite/blob/main/RESEARCH.md) · [Contour RESEARCH.md §8](https://github.com/KwaminaWhyte/prism-suite/blob/main/RESEARCH.md) · [Pulse RESEARCH.md §7](https://github.com/KwaminaWhyte/prism-suite/blob/main/RESEARCH.md) · [Reel RESEARCH.md §9](https://github.com/KwaminaWhyte/prism-suite/blob/main/RESEARCH.md) · github.com/pykeio/ort

## 6. Shared app shell & tooling

- **App shell:** **GPUI 0.2.2** (Zed's GPU UI framework, blade-graphics Metal backend on macOS) across all
  six apps. All apps share the `prism-ui` design system crate (design tokens, SVG icon set, GPUI
  components). See [UI_SYSTEM.md](./UI_SYSTEM.md) for the full component reference.
- **Common deps:** `serde` (doc IO), `glam` (math), `bytemuck` (GPU casts), `rayon` (parallel
  tile/frame/boolean work), `thiserror`/`anyhow`, `rfd` (dialogs), `kurbo` (Bézier — vector + easing).
- **Automation/extensibility:** `rhai` (sandboxed scripting/actions) suite-wide; OpenFX-style plugins via
  `prism-fx`. Undo is per-app (tile-COW pixels in Pigment; small command stacks elsewhere) but the
  command pattern is shared.

Sources: each app's Cargo.toml · [UI_SYSTEM.md](./UI_SYSTEM.md) (GPUI components) · [Pigment RESEARCH.md §11](https://github.com/KwaminaWhyte/prism-suite/blob/main/RESEARCH.md) (rhai/OpenFX)

## 7. Current state of the suite (June 2026)

All six apps run on GPUI on `main`; the shared crates are promoted. Rough parity vs the Adobe analog:

| App | Analog | Built today | Approx parity | Next big lever |
|---|---|---|---|---|
| **Pigment** | Photoshop | Full raster editor: brush/erase, all selection tools, move/transform, fill/gradient/shapes, text, clone, heal, pen, masks, adjustment layers, channels, curves, history, content-aware fill, lens correction, perspective warp, filter gallery, Camera Raw, PSD export, boolean shape ops, clipping masks, bevel/emboss + overlay layer styles | ~91% | Color picker wheel; dockable workspaces; RAW import |
| **Contour** | Illustrator | Pen/node, all shapes, boolean ops, SVG/PDF/AI I/O, symbols, mesh gradient, shape builder, variable fonts, multi-artboard, graph tool, live paint, envelope distort, graphic styles, recolor, type on path, character/paragraph panels, blend tool, 3D extrude/revolve, PDF export config | ~77% | Gradient fill rendering; shape builder geometry; SVG animation export |
| **Pulse** | After Effects | Keyframe lanes, graph editor, gizmo, 40+ effects (CC effects, motion blur, glow, generate), 3D camera + lights, expressions (rhai), render queue, RAM preview, layer parenting, track mattes, puppet tool, time remapping, precompose, motion paths, shape layer groups (star/repeater/trim/merge), audio mixer buses | ~72% | Full expression language depth; OCIO/log support; Dynamic Link |
| **Reel** | Premiere Pro | Multitrack timeline, clip move/trim/split, bezier speed ramp, 20+ transition types, linked A/V, nested sequences, snap, dual viewer, 3-way color wheels, RGB curves, audio waveform + EQ, LUT grade, multicam, ProRes/GIF export, captions, Lumetri scopes, project bins + media manager, export presets | ~62% | Audio playback output; source/program dual viewer; text-based editing |
| **Drift** | Animate + Char. Animator | Timeline + layers + keyframes + transforms + puppet rig (bones, IK/FABRIK, spring dynamics) + scenes + frame labels + symbol library + swap sets + grid/ruler config + vector paths + GPUI panels (toolbar, layers, timeline, AI panel) + welcome screen + export config + state machines | ~30% | Mesh warp; bone weights; easing curves; audio lip sync; Character Animator behaviors; real ONNX motion gen |
| **Tone** | Logic Pro / GarageBand | Project + tracks + clips + MIDI notes/CC + piano roll + mixer channels + transport + AI stubs + bounce + undo/redo + MIDI editing ops + clip ops (split/duplicate/trim) + track groups + insert effects chain + mixer sends/PFL/phase/width + loop region + GPUI panels | ~20% | CPAL audio playback; automation lanes; tempo map; scene/arrangement mode; plugin state |

Each app's PLAN.md defines its road to ≥90% parity and the phase where that line lands. Sequencing
principle suite-wide: **build the foundation that gates breadth first**, then fan out.

Sources: [Pigment PLAN.md](apps/pigment/PLAN.md) · [Contour PLAN.md](apps/contour/PLAN.md) · [Pulse PLAN.md](apps/pulse/PLAN.md) · [Reel PLAN.md](apps/reel/PLAN.md) · [Drift PLAN.md](apps/drift/PLAN.md) · [Tone PLAN.md](apps/tone/PLAN.md)
