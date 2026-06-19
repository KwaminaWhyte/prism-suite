# Prism Suite — Research Findings (June 2026)

Suite-level research backing [SUITE.md](./SUITE.md) (vision + interop) and the four app plans. This doc
covers the **shared engine, the interop mechanisms, and the cross-cutting policies** (color, AI) that all
four apps inherit. App-specific findings live in each app's own `RESEARCH.md`:

- [Pigment](https://github.com/KwaminaWhyte/prism-suite/blob/main/RESEARCH.md) — raster engine, compositor, blend math, brush, color IO, AI tools
- [Contour](https://github.com/KwaminaWhyte/prism-suite/blob/main/RESEARCH.md) — vector path engine, booleans, SVG/PDF, image trace, gradients
- [Pulse](https://github.com/KwaminaWhyte/prism-suite/blob/main/RESEARCH.md) — time-addressable compositing, keyframes/expressions, effects, media
- [Reel](https://github.com/KwaminaWhyte/prism-suite/blob/main/RESEARCH.md) — NLE editing model, video/audio decode, transitions, color, export

> Verify every crate version against crates.io at build time — third-party version metadata is sometimes
> stale. `wgpu` confirmed at **29.0.3** (2026-05-02); the `"29"` pin holds suite-wide.

---

## 1. The architectural bet — one engine, four apps

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

So Pulse = Pigment's compositor + time; Reel = the compositor + a clip/edit model + media. Build the
compositor **time-agnostic and clip-agnostic**; time and clips are each a thin layer on top. This is why
the shared crates must never bend toward one app's UI.

Sources: [SUITE.md](./SUITE.md) · [Pigment RESEARCH.md §2](https://github.com/KwaminaWhyte/prism-suite/blob/main/RESEARCH.md) (tile compositing) · [Pulse RESEARCH.md §1](https://github.com/KwaminaWhyte/prism-suite/blob/main/RESEARCH.md) · [Reel RESEARCH.md §1](https://github.com/KwaminaWhyte/prism-suite/blob/main/RESEARCH.md)

## 2. Shared crate matrix (current + planned)

Shared crates live at `prism/crates/` as their own workspace; every app path-deps in via `../crates/`
and **does not copy or modify** them.

| Crate | Status | Owns | Consumers |
|---|---|---|---|
| `prism-core` | **exists** | doc model, `Size`/`Rect`, color boundary, blend modes (18), tile types, adjustments, curve/histogram | pigment, contour, pulse, reel |
| `prism-color` | **exists** | sRGB/linear, ICC (`lcms2`/`qcms`) — grows OCIO, CMYK, spot, soft-proof | pigment (+ all on color tasks) |
| `prism-io` | **exists** | image load/export, PSD/EXR, resize, `.pigment`, text raster | pigment, reel (`load_image`) |
| `prism-canvas` | **exists** | GPU compositor (wgpu/WGSL passes: composite, display, dab, filter, selection) | pigment |
| `prism-media` | **exists** | FFmpeg decode/encode (H.264 MP4, audio mux), frame-accurate seek, audio tracks | pulse + reel (co-owned) |
| `prism-ui` | **exists** | GPUI design system: design tokens, SVG icon set, shared components | all four apps |
| `prism-vector` | **planned** | paths/anchors/handles, booleans (`i_overlay`), stroking/offset (`kurbo`), tessellation (`lyon`) | contour (owner), pigment (shape layers), pulse (masks/shape layers) |
| `prism-fx` | **planned** | OpenFX-style GPU effect/transition host — author once, run everywhere | pigment (filters), contour (live effects), pulse (effects), reel (transitions/effects) |
| `prism-ai` | **planned** | `ort` (ONNX) runtime + provider abstraction + on-demand model cache | all (segmentation, matting, upscale, inpaint, transcription) |
| `prism-doc` | **planned** | interchange container (layer tree + scene graph + media refs) + Dynamic-Link node | all (interop) |

**Promotion discipline:** code is promoted to a `prism-*` crate only when it is genuinely generic. The
README/PLAN of each app names a "coordinate before promoting" step so the owning agents agree on a shape
that serves *all* consumers (e.g. `prism-vector` must satisfy Contour authoring **and** Pigment shape
layers **and** Pulse masks). Never raster-couple, time-couple, or clip-couple a shared crate.

Sources: prism/Cargo.toml (workspace members) · each app's Cargo.toml (path deps) · [Contour RESEARCH.md §1](https://github.com/KwaminaWhyte/prism-suite/blob/main/RESEARCH.md) · [Pulse RESEARCH.md §6](https://github.com/KwaminaWhyte/prism-suite/blob/main/RESEARCH.md) · [Reel RESEARCH.md §2–3](https://github.com/KwaminaWhyte/prism-suite/blob/main/RESEARCH.md)

## 3. Interop mechanisms (the Adobe-parity features)

All four reduce to **one render-graph-node abstraction** plus a shared container; build it suite-aware
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
- **Generative fill / expand / extend — explicit suite policy: OPTIONAL and PLUGGABLE.** It runs via a
  provider abstraction with **two interchangeable backends — a local diffusion model (`candle`/ONNX) and
  a user-configured cloud endpoint (bring-your-own API key)** — plus "none". It is **never required** for
  core editing; the apps are fully functional with no AI backend configured. This applies uniformly to
  Pigment (Generative Fill), Contour (Generative Recolor/vectorize), Pulse (Generative Extend), and Reel
  (Generative Extend).

Sources: [Pigment RESEARCH.md §10](https://github.com/KwaminaWhyte/prism-suite/blob/main/RESEARCH.md) · [Contour RESEARCH.md §8](https://github.com/KwaminaWhyte/prism-suite/blob/main/RESEARCH.md) · [Pulse RESEARCH.md §7](https://github.com/KwaminaWhyte/prism-suite/blob/main/RESEARCH.md) · [Reel RESEARCH.md §9](https://github.com/KwaminaWhyte/prism-suite/blob/main/RESEARCH.md) · github.com/pykeio/ort

## 6. Shared app shell & tooling

- **App shell:** **GPUI 0.2.2** (Zed's GPU UI framework, blade-graphics Metal backend on macOS) across all
  four apps. All apps share the `prism-ui` design system crate (design tokens, SVG icon set, GPUI
  components). See [UI_SYSTEM.md](./UI_SYSTEM.md) for the full component reference.
- **Common deps:** `serde` (doc IO), `glam` (math), `bytemuck` (GPU casts), `rayon` (parallel
  tile/frame/boolean work), `thiserror`/`anyhow`, `rfd` (dialogs), `kurbo` (Bézier — vector + easing).
- **Automation/extensibility:** `rhai` (sandboxed scripting/actions) suite-wide; OpenFX-style plugins via
  `prism-fx`. Undo is per-app (tile-COW pixels in Pigment; small command stacks elsewhere) but the
  command pattern is shared.

Sources: each app's Cargo.toml · [UI_SYSTEM.md](./UI_SYSTEM.md) (GPUI components) · [Pigment RESEARCH.md §11](https://github.com/KwaminaWhyte/prism-suite/blob/main/RESEARCH.md) (rhai/OpenFX)

## 7. Current state of the suite (June 2026)

All four apps run on GPUI on `main`; the shared crates are promoted. Rough parity vs the Adobe analog:

| App | Analog | Built today | Approx parity | Next big lever |
|---|---|---|---|---|
| **Pigment** | Photoshop | Full raster editor: brush/erase, all selection tools, move/transform, fill/gradient/shapes, text, clone, heal, pen, masks, adjustment layers, channels panel, curves editor, history panel | ~70% | Layer styles; clipping masks; color picker wheel; PSD I/O; dockable workspaces |
| **Contour** | Illustrator | Pen/node, all shapes, boolean ops, SVG/PNG export, undo, multi-select + align/distribute, symbols, gradient-stop editor, image trace, recolor, artboard tool, text-on-path | ~45% | Gradient fill rendering; shape builder geometry; character/paragraph panels; PDF export |
| **Pulse** | After Effects | Keyframe lanes, graph editor, gizmo, 9 effect types, 3D layer controls, expressions, MP4 export, render queue, comp settings | ~35% | RAM preview cache; parenting; 3D camera/lights; more AE effects; full expression language |
| **Reel** | Premiere Pro | Multitrack timeline, clip move/trim/split, speed ramp, 4 transition types, audio waveform + fade + mixer, LUT/grade, title clips, multi-cam stub, MP4 export | ~40% | Audio playback output; Lumetri Scopes; snap to edges; ProRes export; source/program dual viewer |

Each app's PLAN.md defines its road to ≥85% parity and the phase where that line lands. Sequencing
principle suite-wide: **build the foundation that gates breadth first**, then fan out.

Sources: [Pigment PLAN.md](https://github.com/KwaminaWhyte/prism-suite/blob/main/PLAN.md) · [Contour PLAN.md](https://github.com/KwaminaWhyte/prism-suite/blob/main/PLAN.md) · [Pulse PLAN.md](https://github.com/KwaminaWhyte/prism-suite/blob/main/PLAN.md) · [Reel PLAN.md](https://github.com/KwaminaWhyte/prism-suite/blob/main/PLAN.md)
