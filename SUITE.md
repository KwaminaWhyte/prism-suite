# The Prism Suite — an open source creative suite

Prism is a **six-app open-source creative suite** in Rust that works together the way
Adobe's Creative Cloud apps do (Dynamic Link, smart objects, shared color and
assets). The shared engine crates are designed from the start to be reused across
all six apps.

## The six apps

| # | App | Adobe analog | Domain | Open source we build on / fork from |
|---|-----|--------------|--------|--------------------------------------|
| 1 | **Pigment** | Photoshop | Raster image editing | greenfield (Rust/wgpu); ideas from Krita/mypaint |
| 2 | **Contour** | Illustrator | Vector graphics | `kurbo` + `lyon` + `i_overlay`; ideas from Inkscape |
| 3 | **Pulse** | After Effects | Motion graphics / VFX / compositing | OpenFX, OpenColorIO, OpenEXR; ideas from Natron |
| 4 | **Reel** | Premiere Pro | Video editing (NLE) | FFmpeg, MLT-style engine; ideas from Kdenlive |
| 5 | **Drift** | Adobe Animate + Character Animator | AI-first animation | GPUI + wgpu canvas; `ort` ONNX for motion gen + lip sync |
| 6 | **Tone** | Logic Pro / GarageBand / Ableton | AI-first music creation | GPUI + CPAL; `ort` ONNX for MusicGen/Demucs/Magenta |

## Why a suite, not four apps

Adobe's real moat isn't any single app — it's that they **interoperate**: a
shape pasted from Illustrator stays editable, an After Effects comp drops into a
Premiere timeline via Dynamic Link and updates live, everything shares the same
color. We get this for free *if* the shared layer/compositor/color engine is one
codebase. That is the whole architectural bet.

## Shared foundation (one engine, four apps)

```
                ┌────────────────────────────────────────────┐
   Pigment ─┐   │  prism-core   layer/scene graph, tiles,    │
   Contour ─┤   │               command/undo, doc model      │
   Reel    ─┼──▶│  prism-canvas wgpu render graph,           │
   Pulse   ─┤   │               blend/effect passes, tiles   │
   Drift   ─┤   │  prism-color  linear-light, ICC/OCIO, CMYK │
   Tone    ─┘   │  prism-media  FFmpeg decode/encode, audio  │
                │  prism-fx     OpenFX-style effect plugins   │
                │  prism-io     file formats + interchange    │
                │  prism-ai     ort ONNX runtime (Drift+Tone) │
                └────────────────────────────────────────────┘
```

The render graph, tile model, blend math, and color pipeline are **identical**
needs for raster, vector raster preview, video frames, comp layers, animation
frames, and audio-reactive visuals. Drift and Tone extend the suite into
AI-first territory: local ONNX inference (motion generation, lip sync, stem
separation, chord suggestion) via `prism-ai` (planned shared crate).

## Interop mechanisms (the Adobe-parity features)

1. **Dynamic Link** — a Pulse comp referenced in a Reel timeline renders live;
   editing the comp updates the edit. Same for a Contour artboard placed in
   Pigment. Implemented as a shared render-graph node that evaluates the linked
   document on demand (cached per frame/tile).
2. **Smart objects / live placement** — place a `.contour` vector doc or a
   `.pigment` doc inside another app; it stays editable at its source resolution,
   re-rasterized on transform.
3. **Common interchange format** — a `prism-doc` container (layer tree + scene
   graph + media refs) that every app reads. Lossy-but-faithful import/export to
   PSD/AI/SVG/Premiere XML/AEP-ish as bridges to Adobe.
4. **Shared clipboard** — copy a path, layer, keyframe, or color in one app,
   paste editable in another (shared in-memory model + serialized fallback).
5. **One color pipeline** — `prism-color` (linear-light, ICC + OpenColorIO)
   means a color/look is identical across all four apps and on export.
6. **Shared effects** — `prism-fx` (OpenFX-style) effects run in any app that
   composites: blurs, grades, distortions authored once.
7. **Shared asset library** — brushes, gradients, LUTs, fonts, templates in a
   common store all apps see.

## Suite roadmap (high level)

**Status (June 2026):** all four apps have fully functional GPUI binaries on `main`, 9 parity waves deep. The shared engine crates (`prism-core` / `prism-color` / `prism-io` / `prism-canvas` / `prism-media`) are in `prism/crates/` and consumed by every app. The shared `prism-ui` design system (design tokens + SVG icon set + GPUI components) unifies chrome across all four apps.

| App | Analog | What's built | Plan |
|---|---|---|---|
| **Pigment** | Photoshop | Full raster editor: brush/erase, all selection tools, move/transform, fill/gradient/shapes, text, clone, heal, pen, masks, adjustment layers, channels, curves, history, content-aware fill, lens correction, perspective warp, filter gallery, Camera Raw, PSD I/O, export presets, smart filters, autosave, plugin system | [PLAN.md](apps/pigment/PLAN.md) |
| **Contour** | Illustrator | Vector editor: selection/multi-select, pen/node, all shapes, boolean ops, SVG/PDF/AI I/O, symbols, mesh gradient, shape builder, variable fonts, multi-artboard, graph tool, live paint, envelope distort, graphic styles, recolor artwork, type on path | [PLAN.md](apps/contour/PLAN.md) |
| **Pulse** | After Effects | Motion compositor: scrub/play, keyframe lanes, graph editor, gizmo, 20+ effects, 3D camera + lights, expressions (rhai), render queue (H264/ProRes/GIF), RAM preview, layer parenting, track mattes, null objects, puppet tool, solo/shy, time remapping, precompose | [PLAN.md](apps/pulse/PLAN.md) |
| **Reel** | Premiere Pro | NLE: multitrack timeline, clip move/trim/split, bezier speed ramp, 5 transition types, linked A/V, nested sequences, snap, dual viewer, 3-way color wheels, RGB curves, audio waveform + 3-band EQ + effects, LUT grade, multicam, ProRes/GIF export, captions, rate stretch, sequence settings | [PLAN.md](apps/reel/PLAN.md) |
| **Drift** | Animate + Char. Animator | Timeline + layers + keyframes + transforms + puppet rig bones + AI motion gen stubs + export config + state machines | [PLAN.md](apps/drift/PLAN.md) |
| **Tone** | Logic Pro / GarageBand | Project model + tracks + clips + MIDI notes + piano roll + mixer channels + AI generation stubs + bounce config + transport | [PLAN.md](apps/tone/PLAN.md) |

**UI host:** all six apps run on **GPUI 0.2.2** (Zed's GPU UI framework, blade-graphics Metal backend) on `main`. `prism-ui` shared design system (tokens + icons + GPUI components) in `shared/prism-ui/`.

**Next levers (UI polish + parity depth):**

- **Pigment:** layer styles; clipping masks; color picker wheel; PSD I/O; dockable workspaces.
- **Contour:** gradient fill rendering; shape builder geometry; character/paragraph panels; PDF export.
- **Pulse:** RAM preview cache; parenting; 3D camera/lights; more AE effects; full expression language.
- **Reel:** audio playback output; Lumetri Scopes; snap to edges; ProRes export; source/program dual viewer.
- **Drift:** GPUI timeline + layers panel + canvas; vector drawing (pen/shapes); classic/shape tweening; puppet IK; AI motion generation (real ONNX).
- **Tone:** CPAL real-time audio playback; GPUI piano roll canvas; GPUI mixer strips; AI stem separation (Demucs); chord generation (Magenta).

**Shared-crate promotions:**
- **Done:** `prism-canvas` (GPU compositor), `prism-media` (FFmpeg A/V decode/encode), `prism-ui` (design tokens, SVG icons, GPUI components — all six apps).
- **Planned:** `prism-vector` (paths/booleans/stroke — Contour + Pigment + Pulse + Drift), `prism-fx` (OpenFX-style effects/transitions — all four creative apps), `prism-ai` (`ort` runtime + on-demand models — Drift + Tone first, then all), `prism-doc` (interchange + Dynamic-Link node).

Each app is independently useful; the value compounds as interop lands.

*Foundations are free. The product is the polish — and the glue between apps.*
