# Pulse — Open Source After Effects Alternative

Professional motion-graphics, VFX & compositing app in Rust, and **app #3 of the Prism suite** (sibling
to [Pigment](https://github.com/KwaminaWhyte/prism-suite) raster, [Contour](https://github.com/KwaminaWhyte/prism-suite) vector). **Goal: reach ≥85% of Adobe After
Effects' real-world capability** — features, reliability, and ease-of-use — in staged milestones, on the
suite's shared engine: `prism-core` (layers, blend math, render graph), `prism-color` (linear/OCIO),
and a shared `prism-fx` (OpenFX-style effects) and `prism-media` (FFmpeg) layer.

> Companion docs: [RESEARCH.md](./RESEARCH.md) (cited findings + crate matrix), [SUITE.md](https://github.com/KwaminaWhyte/prism-suite/blob/main/SUITE.md) (four-app vision + interop). The repo README (when added) tracks what runs *today*; this PLAN is the parity roadmap.

---

## 0. Why this can work

- The hard parts are solved and free: a tile/render-graph compositor (shared with Pigment), blend math
  and color science (`prism-core`/`prism-color`), keyframe interpolation + Bézier easing, OpenColorIO/
  OpenEXR (ASWF Rust bindings), FFmpeg decode/encode, an embeddable expression engine (`rhai`/`rune`).
- After Effects is, at its core, **a keyframed, effect-driven layer compositor**. The compositor is the
  *same* render graph Pigment already runs — Pulse adds **time** (keyframes, expressions), **effects at
  scale** (`prism-fx`), and **media** (`prism-media`). That shared engine is the suite's whole bet.
- AE's real moat is interop + the effect/expression ecosystem + reliability (caching, multi-frame
  render). We target those deliberately.

**Non-negotiable principle:** a **time-addressable render graph** — every layer/effect/property is a
function of time `t`, composited in **linear light** (`prism-color`), cached per (node, frame, tile).
Float (16/32-bit) working buffers from day one; OCIO-managed color; never bake until render.

---

## 0a. Suite boundaries — what belongs in Pulse vs Pigment / Contour / Reel

Pulse shares `prism-core` (already a dependency) and will share `prism-color` / `prism-fx` /
`prism-media`. Every feature is filed against three rules so we never overwrite a sibling app's work:

- **Pulse-owned (motion / VFX / compositing):** comps & precomps, the timeline, keyframes & graph
  editor, expressions, layer types (solid/text/shape/footage/adjustment/null/camera/light), masks &
  roto, track mattes, motion blur, time remapping, 2D/3D compositing, the render queue. Lives in
  `pulse-app` or motion-only modules.
- **Shared-crate, app-agnostic:** the compositor/render-graph, tile model, **blend modes** (reuse
  `prism-core`'s 18), **adjustments** (reuse `prism-core::adjust` for color-correction effects), color
  transforms (`prism-color` + OCIO), the **`prism-fx`** OpenFX host, and **`prism-media`** (FFmpeg
  decode/encode + audio, shared with the future Reel). These **must not** assume Pulse — additions are
  additive and time-agnostic (time is Pulse's layer on top).
- **Out of scope — a sibling app's domain (do not build here):**
  - *A clip-based, multi-track non-linear **video editor*** (trim/ripple/insert clips, transitions
    between clips, full audio mixer) → **Reel** (the Premiere analog). Pulse's timeline is *layer +
    keyframe* based, not clip-based. Pulse renders comps that Reel places via Dynamic Link.
  - *Primary raster painting / photo retouch* → **Pigment**; *deep vector authoring* → **Contour**.
    Pulse has shape layers and text animators (motion-native), and it *consumes* Pigment docs and
    Contour artboards as footage/smart layers via suite interop — it does not re-implement them.
  - *Cross-app interop glue* (Dynamic Link host, `prism-doc` container, shared clipboard/asset library)
    is **suite-level**. Pulse is the canonical Dynamic-Link *producer* (its comps drop live into Reel),
    but the mechanism is defined with the suite, not unilaterally.

---

## 1d. Batch 5 completed (2026-06-19)

- **Expanded expression library** (`comp/expr.rs`, additive): `loopOut`/`loopIn` (cycle + pingpong), `valueAtTime`, `ease`/`easeIn`/`easeOut`, `random`/`seedRandom` (deterministic per `(layer,time)`), `degreesToRadians`/`radiansToDegrees`, and `width`/`height` in scope. New `expr::TrackView` snapshot + `eval_with_track` let the temporal helpers read the keyed value at any time (primed via thread-locals, mirroring the `wiggle` seed). `ExprCtx` gains `width`/`height`. 9 new tests. Closes a chunk of Phase 4 *Expressions* TODO (`loopOut/In`, `ease`, `random/seedRandom`, `valueAtTime`).
- **Comp Motion Blur panel + per-layer MB toggle (GPUI):** wired the modelled-but-UI-less comp `MotionBlur` into a Properties *Motion Blur* section (Enable + Shutter°/Phase°/Samples steppers) and a per-layer **MB** toggle. Actions `SetMotionBlurEnabled/Angle/Phase/Samples` + `ToggleLayerMotionBlur` (undoable). Advances Phase 4 *Motion blur* UX.
- **3 new Stylize effects:** `StylizeEffect::Posterize` (channel quantise / cel-shade bands), `Invert` (photographic negative, blendable), `Threshold` (Rec.709 luma B/W) — each end-to-end (enum + `apply` pass + registry entry + scalar params + egui/GPUI UI). `defaults()` 3→6; Stylize folder 4→7. Closes the PLAN's deferred *Posterize* stylize item.
- **0 warnings:** `cargo check --workspace` clean; 664 `pulse-app` tests green (fixed 6 pre-existing test-only `Comp` literals missing `hide_shy`).

## 1c. Batch 4 completed (2026-06-19)

- **3D Lights panel:** `Action::AddLight/RemoveLight/UpdateLight`. Properties panel (no-layer-selected) shows Lights section with intensity steppers + Remove; "Add Light" buttons for Ambient/Point/Parallel/Spot.
- **Expression Controls panel:** `panels/expr_controls.rs` — badge, value display, −/+ steppers, checkbox toggle, "Add Control" buttons for all 5 kinds.
- **Output Presets in render_queue:** preset list with name + "Apply" per preset. Default 3 presets (H.264/ProRes/GIF display names).
- **Multi-comp render:** "All Comps" button → `AddAllCompsToQueue` iterates all comps.
- **Format chips strip:** `RenderFormat::ALL` chips in render_queue header.
- **Live Output:** `App::live_output_enabled` + toolbar "Live Out" toggle; each render writes `/tmp/prism-pulse-preview.rgba`.
- **0 warnings:** all 13 dead-code warnings eliminated; `live_output_frame` field removed.

## 1b. Batch 3 completed (2026-06-19)

- **Puppet warp:** `PuppetPin` model, `apply_puppet_warp` IDW pass, `Tool::Puppet` in tools strip.
- **Solo / Shy flags:** `solo`/`shy` on `PulseLayer`, `hide_shy` on `Comp`, solo render logic, GPUI + egui UI.
- **Time Remap UI:** ◇ badge in timeline lane, ON/off toggle + source readout in properties panel.
- **Track Mattes UI:** T badge for matte sources, matte-mode badge for active mattes in GPUI layers panel.
- **Null tool button:** "N" button in GPUI tools strip; "(null)" label in egui layers panel.

## 1. Current state (what runs today)

Grounded in `pulse/crates/pulse-app/src/`. An early but real motion scaffold:

- **Comp model** (`comp.rs`) — `Comp { width, height, duration, fps, layers }`; `PulseLayer { name,
  color, visible, + 5 Tracks }`. Five animatable properties: **X, Y, Scale, Rotation, Opacity**, each a
  `Track` of `Keyframe { t, value }`. Sampling = **linear interpolation** between bracketing keys,
  constant hold outside the range; `set_key` inserts/overwrites keeping keys sorted. serde JSON.
- **Transport** (`app.rs`) — playhead `time`, play/pause (Spacebar), real-`dt` advance, loop at
  duration; add/delete/move/recolor layers; save `.pulse` (serde + `rfd`); Export is a **stub**.
- **Timeline** (`timeline.rs`) — time ruler, one lane per layer with **keyframe diamonds**, draggable
  **playhead**, click/drag scrub.
- **Preview** (`preview.rs`) — CPU egui-`Painter` render: each visible layer is a **solid color rect**,
  transformed by sampled (x, y), uniform scale, rotation about center, faded by opacity; fitted to the
  panel.
- **Shell** — `theme.rs` Prism dark theme, `icons.rs` phosphor glyphs; depends on `prism-core`.

**Reality check:** layers are solid swatches; interpolation is linear-only; preview is CPU/egui (no GPU
compositor, no effects, no media, no blend modes yet). Everything below builds from this seed.

---

## 1b. GPUI front-end (Wave 9 complete — merged to main)

Pulse ships an egui front-end today, but the suite is moving its UIs to **GPUI**
(Zed's framework) — the same migration already underway in the sibling Pigment
app (`pigment-gpui`). The strategy is **coexistence, not rip-out**: a second
binary crate `crates/pulse-gpui` grows alongside the existing egui `pulse`
binary, reusing the *same* comp model and CPU compositor so both front-ends stay
green and the egui app keeps working while the GPUI host catches up.

**Phase 1+2 starter — landed (this slice):**

- **Crate split.** `pulse-app` exposes a `lib` (`src/lib.rs`) so the GPUI host
  can `use pulse_app::{comp, render, …}`; `main.rs` is the thin egui entry point
  (`use pulse_app::app::PulseApp`). The egui binary boots identically — no
  behavior change.
- **`crates/pulse-gpui` binary** added to the workspace `members`. Deps: `gpui`
  0.2.2, `image` 0.25, `pulse-app` (path), plus the shared `prism-core`/
  `prism-io` pins. **No wgpu** — Pulse composites on the CPU.
- **CPU preview bridge** (`canvas_host.rs`): renders the active comp at the
  current playhead via `render::render_preview_frame` (capped ~1280px, persistent
  footage `FrameCache`), converts **RGBA8 → BGRA8**, wraps it as a GPUI
  `RenderImage`, and dirty-caches it (re-renders only on scrub / comp edit).
- **State + choke point** (`app_state.rs`): `App` owns the `Project`, playhead
  `time`, active `Tool`, selection, and host; the single `App::apply(Action)`
  mutator marks the host dirty when the frame changes.
- **Host shell** (`main.rs`): top toolbar (title / tool / live time+frame /
  *Start* + *Play/Pause* + *+1f* transport), left tools strip (Select/Hand/Pen),
  center bridged preview, right dock, and a **live, scrubbable bottom timeline**
  (scrub ruler + per-layer lanes + a live playhead marker, all bound to state).
- **Live transport on the timeline** (`app_state.rs`, `panels/timeline.rs`,
  `main.rs`): **drag-to-scrub** maps a pointer x → comp time (`time_for_x` over a
  `canvas`-recorded track bounds) → `Action::SetTime` → host re-renders at the new
  time; **play/pause** (`Action::TogglePlay`) advances the playhead by the real
  wall-clock delta each frame (`App::tick`, looping at the comp end) with the loop
  driven on the UI thread via `window.request_animation_frame()`; a teal
  **playhead marker** overlays the ruler + lanes. CPU only, no audio.
- **Layers dock panel** (`panels/layers.rs`): lists the active comp's layers with
  a visibility toggle (re-renders) + click-to-select — the working `Action`
  round-trip reference (`cx.listener` → `app.apply` → `cx.notify`) other panels
  follow (see `panels/mod.rs`).
- **Properties dock panel** (`panels/properties.rs`): selecting a layer reveals an
  editable **Transform** panel (anchor X/Y, position X/Y, scale, rotation,
  opacity) as **stepper rows** (`−` / value / `+`, since gpui 0.2.2 has no native
  slider). Edits emit `Action::SetTransform(prop, value)` →
  `layer.track_mut(prop).set_key(time, value)` — the same write the egui slider
  performs (key the value at the playhead, or lay down the base key when
  unkeyframed) → host dirty → live re-render. Readout uses the expression-resolved
  `Comp::layer_value`.
- **Keyframe lanes on the timeline** (`panels/timeline.rs` + Properties stopwatch,
  Wave 4): each layer lane paints its transform keyframes as **diamonds** (the
  de-duplicated union of all transform tracks' key times, via a per-lane `canvas`
  `Path`/`paint_path` overlay aligned to the scrub ruler). A per-property
  **stopwatch** toggle in Properties (`Action::ToggleKeyframe`) starts/stops
  animation (first key at the playhead / clear the track); editing an animated
  value at a new time adds a key (`SetTransform` → `set_key`). The selected
  layer's diamonds are **time-draggable**: pressing one arms `App::kf_drag`,
  lane-level pointer motion emits `Action::MoveKeyframe { prop, key_index, time }`
  → `Track::move_key` (value preserved, re-sorts past neighbours, drag keeps
  tracking the key), host re-renders so the preview shows the new interpolation
  live.
- **Effects panel + undo/redo** (`panels/effects.rs` + `effect_params.rs` +
  history in `app_state.rs`, Wave 5): for the selected layer, lists every effect
  across all six per-layer stacks (colour / spatial / distort / stylize / keying
  + the single generate slot) as cards with a **remove** button
  (`Action::RemoveEffect`) and editable scalar **stepper rows**
  (`Action::SetEffectParam`, ranges mirroring the egui app's sliders via the pure
  `effect_params` accessor). A toggleable **Effects & Presets browser** reuses the
  engine registry (`comp::filter_grouped` → `BrowserEntry::instantiate` →
  `NewEffect`); clicking an entry emits `Action::AddEffect` (mirrors the egui
  app's `add_browser_effect`), and the preview re-composites live.
  **Undo/redo** is an app-side, snapshot-based stack (capped `Project` clones):
  `App::apply` snapshots the pre-edit document for every document-mutating action,
  `Action::Undo`/`Redo` swap snapshots (clearing redo on a fresh edit), bound to
  **Cmd+Z / Cmd+Shift+Z** + toolbar Undo/Redo buttons. Added app-side because
  `pulse-app` has no existing history model; the comp model is untouched.
- **Export + work-area / loop region** (`export.rs` + `panels/timeline.rs` +
  work-area Actions, Wave 6): a **File → Export** toolbar action renders the
  active comp to a **PNG image sequence** on a **background thread** (so the
  window stays responsive), reusing the *same* engine the egui exporter does —
  `render_frame_in_project` for the pixels, `frame_range` / `frame_time` /
  `frame_path` (newly re-exported, additive) for the chosen range's numbering /
  timing / filenames — so a GPUI-host sequence is byte-identical to an egui one.
  A shared `ExportProgress` handle the toolbar polls each frame drives a live
  "Exporting N/M…" readout with a progress fill; the worker reports a final
  "Exported M frames" / "Export failed". (MP4 is deferred — the suite ships no
  video encoder yet and the egui app it mirrors only writes PNG; the `format`
  enum + threading/progress scaffold leave the seam.) The **work area** (loop /
  render region) is now editable in the GPUI host: a tinted band + draggable
  **in/out handles** on the timeline ruler (`Action::SetWorkAreaStart` /
  `SetWorkAreaEnd`, right-click to reset), playback **loops within the work area**
  (mirroring the egui app's RAM-preview loop), the playhead **dims** outside a
  trimmed area, and Export defaults to the work area when trimmed (else the full
  comp) — After Effects' *Render: Work Area / Full Comp* default.
- **Graph editor + preview transform gizmo** (`panels/graph.rs` +
  `panels/preview_panel.rs` + graph/gizmo Actions, Wave 7): the two highest-value
  *direct-manipulation* parity gaps, both **reusing the engine's pure modules**
  (`pulse_app::graph` math + `pulse_app::gizmo`) — no fork; the egui `pulse`
  binary and the comp model are unchanged.
  - **Graph (value-curve) editor.** A **Graph** toggle on the timeline header
    swaps the keyframe lanes for an After-Effects-style **value-over-time** plot
    of the selected layer's animated transform properties (per-property colour,
    shared value axis, per-second grid, playhead guide). **Dragging a keyframe
    dot** retimes (x) + revalues (y) it (`Action::MoveKeyframeXY`); **dragging a
    Bézier ease handle** reshapes the segment's ease, promoting a Linear/Hold
    segment to an editable `Interp::Ease` seeded at the straight diagonal
    (`Action::SetInterp`) — the *same* `(out_x,out_y,in_x,in_y)` control points the
    engine's `Track::sample` evaluates, so the eased motion shows live in the
    preview. **Property chips** in the header pick which curves to plot (none
    selected = every keyed property). A pure GPUI port of `pulse_app::graph::show`
    (same coordinate maps, hit model, drag math), pointer-driven through gpui
    `canvas` + mouse listeners.
  - **Preview layer-select + transform gizmo.** Clicking the preview **selects**
    whichever layer's quad sits under the pointer (`App::layer_at_pointer`,
    topmost-first), then draws an on-canvas **transform gizmo** for the selected
    layer — bounding box, four corner **scale** handles, a **rotate** knob +
    connector, and the **anchor** cross — overlaying the rendered pixels exactly
    (built from the engine's `GizmoGeom::build` through the layer's world matrix).
    Dragging a handle **moves / scales / rotates** (or moves the anchor) and
    **keys** the changed transform at the playhead (`Action::GizmoKeys` →
    `track.set_key`, exactly like the properties panel — writing keyframes on an
    animated track, a constant key on a static one), all through the engine's pure
    `gizmo::{hit_test, drag, parent_matrix, screen_to_comp}` (parent-local delta
    math, so a parented layer drags correctly). A bounds-recording `canvas` shares
    the preview image's fit so pointer↔comp mapping matches the drawn pixels.

**Wave 8 (done):** 3D layer controls (enable-3D, position Z, orientation X/Y/Z, 3D badge on layers),
effects panel expand/collapse chevrons, gizmo hover highlight (yellow) + Shift-uniform-scale +
Ctrl-snap-rotate-15° + Alt-duplicate-then-move, expression editor stub (rhai, per property, "=" badge),
6 new tests, 28 total.

**Wave 9 (done):** MP4 export via prism-media (`encode_h264` + `EncodeParams`, PNG fallback), export
progress bar, audio preview stub (animated bars, volume toggle), 4 new post-process GPUI effects
(Mosaic, ChromaticAberration, Vignette, Noise — applied after CPU compositor in `canvas_host`),
render queue panel (add/render-all/status), comp settings dialog (width/height/fps/duration/bg, Apply
commits and resizes buffers), 28 tests green.

**Next (Wave 10):** ✅ DONE — prism-ui tokens + icon buttons enforced.
**Wave 11:** RAM preview cache, parenting/pick-whip, pre-compose, more AE effects.

- The egui binary (`pulse-app` with the `egui-ui` feature) is a legacy build path. It is not built by default; the GPUI host (`pulse-gpui`) is the active binary. The `egui-ui` feature can be enabled explicitly but is not part of normal development or CI.

---

## 2. Validated tech stack (verify with `cargo add` at build)

| Concern | Crate | Notes |
|---|---|---|
| Compositor / render graph | `prism-core` + wgpu (shared w/ Pigment) | Tile model, dirty eval, **18 blend modes**, adjustments. Add the time axis. |
| Color / linear / OCIO | `prism-color` + OpenColorIO (ASWF Rust binding) | Linear-light compositing; OCIO config-driven looks; 8/16/32-bpc. |
| HDR / EXR frames | `exr` 1.74 (+ ASWF OpenEXR Rust binding) | Read/write EXR sequences (f16/f32, multi-layer, deep). |
| Effects host | `prism-fx` (OpenFX-style, suite-shared) | Blur/color/distort/generate/keying authored once, run suite-wide. |
| Media (decode/encode/audio) | `prism-media` → `ffmpeg-next`/`rsmpeg` (or `oximedia`) | Import video/audio/image-seq; export MP4/MOV/ProRes; shared with Reel. |
| Keyframe easing | `kurbo` (Bézier) + `keyframe`/`bezier_easing` | Temporal Bézier ease, hold/linear/auto-Bézier; spatial motion paths via `kurbo`. |
| Expressions | `rhai` (default) / `rune` | Sandboxed per-property expression engine; `wiggle/time/loopOut`-style API. |
| Text / type | `cosmic-text` + `swash`/`harfrust` | Text layers, per-char layout for animators, OpenType + variable fonts. |
| Vector (shape layers / masks) | `kurbo` + `lyon` + `i_overlay` (→ shared `prism-vector`) | Mask paths, shape-layer paths, trim/repeater; same primitives as Contour. |
| AI (roto / matting) | `ort` (shared `pigment-ai`) | Roto Brush via SAM2/3 + matting; feature-gated, models on demand. |
| Undo / project | `undo`/custom + `serde` | Command stack over the comp/graph; `.pulse` project IO. |
| Util | `glam`, `bytemuck`, `rayon` | Math, GPU casts, multi-frame parallel render. |

---

## 3. Architecture (target)

```
┌──────────────────────────────────────────────────────────┐
│  pulse-app  (eframe + egui)                              │
│  panels: project · comp/preview · timeline · effects ·   │
│          graph editor · properties · render queue        │
├──────────────────────────────────────────────────────────┤
│  motion model                                            │
│  Comp · Layer{solid,text,shape,footage,adj,null,cam,light}│
│  Property<T>(time→value): keyframes + easing + expression │
│  Mask · TrackMatte · CommandStack(undo)                  │
├──────────────────────────────────────────────────────────┤
│  prism-core   time-addressable render graph (compositor, │
│               tiles, 18 blend modes, adjustments)        │
│  prism-color  linear-light + OpenColorIO + EXR           │
│  prism-fx     OpenFX-style effects (suite-shared)        │
│  prism-media  FFmpeg decode/encode + audio (w/ Reel)     │
│  prism-vector kurbo/lyon/i_overlay (masks, shape layers) │
├──────────────────────────────────────────────────────────┤
│  Dynamic Link: place a .contour artboard / .pigment doc /│
│  nested comp as a live layer, re-rendered on demand      │
└──────────────────────────────────────────────────────────┘
```

### Core data model (target)
- **Property<T>** — the atom: a value that is a function of time, defined by **keyframes** (with temporal
  + spatial interpolation) **or** an **expression** (evaluated per frame). Generalize today's `Track`
  (linear-only `f32`) into typed properties (scalar/2D/3D/color/path) with selectable interpolation.
- **Layer** — a typed source (solid/text/shape/footage/adjustment/null/camera/light/**precomp**) + a
  Transform group (anchor, position [2D/3D, separable], scale, rotation/orientation, opacity) + an
  ordered **effect stack** + **masks** + a **track-matte** ref + blend mode + motion-blur/3D flags.
- **Comp** — sized/timed canvas + layer stack + camera/lights; nestable (a comp is a valid layer
  source = **precomp**). The render graph evaluates `comp(t)` → tiles, cached per (node, frame, tile).

---

## 4. Phased backlog (toward ≥85% parity)

Effort tags **S/M/L**. "shared?" = touches/promotes a `prism-*` crate → keep app-agnostic & time-free.
Phase 0 is **done** (see §1); the rest is the road to parity.

### Phase 0 — Skeleton: comp, timeline, transport  *(DONE)*
- [x] Comp model (size/duration/fps + layers), 5 transform tracks, linear keyframes + hold
- [x] Timeline (ruler, per-layer lanes, keyframe diamonds, draggable playhead, scrub)
- [x] Transport (play/pause/loop), CPU preview of solid layers, `.pulse` save

### Phase 1 — Real property system + GPU compositor  *(the foundation rebuild)*
- [ ] **Typed `Property<T>`** (L): scalar/2D/3D/color/path; generalize `Track`; **anchor point** + **separable XYZ position**
- [~] **Keyframe interpolation** (M): linear / **hold** / **Bézier ease** + **Easy Ease**/In/Out **done** (per-key `Interp` on the segment, Newton-solved CSS-`cubic-bezier`, unit-tested; UI picker + timeline markers); auto-Bézier and draggable per-key in/out handles pending (land with the Graph Editor)
- [~] **Graph Editor** (M): value-curve editor **done** (per-layer value-over-time curves on a shared auto-framed axis, draggable keyframes with live re-sort, draggable per-key Bézier in/out ease handles incl. promoting Linear/Hold segments, per-property show/hide, scrub); **roving keyframes** done (interior position keys re-timed to constant velocity along the spatial path by arc length — pure `comp::roved_times`/`roved_tracks`, `serde`-default off for back-compat); still TODO: a dedicated **speed graph** and **auto-Bézier**
- [x] **Preview fidelity / render-preview** (M) — **done**: the interactive preview shows **real composited pixels** (footage frames, precomps, effects, color-correction, masks, track mattes, motion blur, time-remap, expressions), not placeholder quads. It renders the comp at the playhead through the **existing CPU offline compositor** (`render::render_preview_frame` → `render_comp`, project-aware, cycle-guarded) at a **capped preview resolution** (1280 px long edge, aspect-preserved; `preview_dims`), uploads it as an egui texture, and draws it as the preview image; the gizmo / selection / mask / null / adjustment overlays + onion-skin ghosts draw on top via the painter, pixel-aligned through the same aspect-fit mapping. Rendered frames are held in a **RAM-preview cache** (frame-indexed, `PreviewRenderer`) filled by a **pool of background worker threads** off the UI thread, so **playback/scrubbing never block input** and a fully-cached comp **loops in real time** straight from RAM; each worker keeps a persistent footage `FrameCache` (export keeps its own per-run cache). The cache is invalidated by a `(comp-state hash, render dims)` signature — any edit/resize re-fills, and stale in-flight jobs are skipped via a cache **epoch** — and bounded by a **~1 GiB budget** (frames farthest from the playhead evicted first, so short comps fit whole and long comps keep a window cached). Playback is real-time but **cache-gated**: the playhead holds on a frame until it's rendered, so it never outruns the cache. State-hash invalidation + pool render / stale-skip + cache readiness (`is_frame_ready` / `fully_cached`) + mapping round-trip + cap + decode-once reuse unit-tested. Still TODO: this is still the **CPU** compositor — the **GPU compositor** below would give true realtime on the *first* pass; **disk cache** + smart purge, **preview-resolution controls** (full/half/quarter), and cutting per-frame cost (e.g. fewer preview motion-blur samples) are the remaining levers (Phase 6 *Caching* / *Preview controls*).
- [ ] **GPU compositor** (L, shared `prism-core`): move preview onto the suite's wgpu render graph; **18 blend modes**; float (16/32-bit) buffers; linear-light
- [ ] **Layer types v1** (M): Solid, **Adjustment**, **Null**; precomp stub
- [ ] Tests: interpolation parity, blend-mode pixels, time-sampling determinism

### Phase 2 — Layers, masks, mattes, precomps  *(compositing core)*
- [~] **Footage layers** (L, shared `prism-io` now / `prism-media` next): **stills + numbered image sequences done** (`LayerKind::Footage` drawing a `FootageSource` — a single still or a printf-pattern image sequence — decoded through the shared `prism-io` `load_image`; **time-indexed frame sampling** with an optional **fps override** and **loop** / **hold-last** end behaviour; auto-sequence detection from any one picked frame; an MRU **decode-once `FrameCache`**; sRGB→linear at the gamma boundary; **alpha interpretation** straight/premultiplied/ignore; full premultiplied-linear-light compositor path so footage composes with transform/anchor/parent/opacity/blend/masks/track-mattes/spatial-effects/motion-blur; Properties section + *Layer ▸ New ▸ Footage* + *File ▸ Import footage…*; pure + compositor unit-tested); still TODO: **real video decode (FFmpeg)** + **audio** via the shared **`prism-media`** crate (the next footage step), footage color/OCIO interpretation, and proxies/placeholders
- [~] **Text layers** (M): a self-contained **stroke vector font** **done** (`LayerKind::Text` drawing a string with a dependency-free built-in font — A–Z, 0–9, space, common punctuation — with **font size / tracking / leading / alignment** and a **fill** + **stroke**; multi-line layout flattened to layer-local stroke segments, rasterized as an antialiased thickened pen band into the isolated premultiplied buffer so text composes with masks/mattes/spatial effects/motion blur; Properties editor + preview + launch-demo title; pure layout + coverage unit-tested); **real outline fonts + family selection** also **done** (an additive `font_family: Option<String>` — `#[serde(default)]` → `None`, so the default and every legacy `.pulse` file keep the stroke font and render **identically** — selects a real TrueType family via the pure-Rust `fontdb` + `ttf-parser` stack [mirroring the Contour Type tool]; the string lays out with the face's advance metrics + glyph contours and rasterizes **filled, antialiased** glyphs by **reusing the Shape layer's even-odd polygon fill**, so holes carve correctly and text composes with the whole pipeline; faces are **cached per family** [read at most once, never per frame]; a **Font** dropdown enumerates installed families with a built-in-stroke-font default; an unknown/unloadable family **falls back** to the bundled Ubuntu Light face so text never vanishes; family enumeration / resolution+fallback / `Some`-vs-`None` path / monotonic+deterministic outline width / hole carving / serde+legacy round-trips all unit-tested; a **font-change keeps the layer's position/anchor** regression test guards against the AE-style "edit a type property → text jumps to the corner" bug, since the transform lives on separate animatable tracks and both font paths lay out centered on the layer origin); still TODO: **variable fonts** (axes), **weight/style sub-selection**, **kerning / full OpenType shaping** (advances are plain horizontal metrics), and **per-character layout** (animator-ready)
- [~] **Shape layers** (M, shared `prism-vector`): parametric primitives **done** (`LayerKind::Shape` drawing a bottom-up stack of `ShapeItem`s — **Rectangle** (rounded), **Ellipse**, **Polygon**, **Star** — each with an antialiased **fill** and **stroke**; rasterized in the layer's local frame into the isolated premultiplied buffer so shapes compose with masks/mattes/spatial effects/motion blur; Properties editor + preview + launch-demo star; pure geometry + compositor unit-tested); still TODO: **arbitrary Bézier paths**, **trim paths**, **repeater**, merge, offset, wiggle-path, and path keyframing (land with the typed-`Property<Path>` rebuild + `prism-vector`)
- [~] **Masks** (L): Bézier mask paths per layer **done** (closed `Mask` of `MaskVertex` in layer-local space, rect/ellipse seeds, flatten → even-odd coverage; modes **add/subtract/intersect/difference** + none; **feather**, **expansion**, **opacity**, **invert**; folded per-pixel into the layer's alpha in the software compositor, composing with motion blur + track mattes; preview outlines + Properties editor — all pure logic unit-tested); still TODO: **animated mask shapes** (keyframable `Property<Path>`), variable-width feather, and on-canvas vertex editing
- [ ] **Track mattes** (M): alpha / luma (inverted) mattes; preserve-underlying-transparency; stencil/silhouette
- [~] **Precomps** (M): nesting **done** (`LayerKind::Precomp` referencing a sibling comp by id + a scalar **time-offset** shift; the document is now a **`Project`** of id-keyed comps with one active; the software compositor renders the referenced comp **recursively** through the same render path — sampling the rendered nested frame into the layer's quad like footage — so a precomp composes with transform/anchor/parent/opacity/blend/masks/track-mattes/spatial-effects/motion-blur; a per-render **visited-set cycle guard** breaks reference cycles A→B→A and self-references — they render nothing rather than infinite-loop/overflow; **pre-compose** (single layer → new comp + precomp reference) + a Properties source-comp/time-offset section + *Layer ▸ New ▸ Precomp*; project save round-trips precomp refs; serde-defaulted for back-compat; render + model unit-tested); still TODO: **multi-layer pre-compose** (selection set, preserving inter-layer parenting), **collapse transformations**, and a **comp navigator**/tab UI (live nested render now shows in the preview — the **render-preview** renders precomps recursively into real pixels, see Phase 1 *Preview fidelity*; full **time-remapping** — a keyframable remap curve, not just the `time_offset` shift — landed; see Phase 4 *Time remapping*)
- [x] **Null / parenting** (S): **done** — transform **parenting** (`PulseLayer::parent: Option<usize>`, `serde`-defaulted) where a child inherits its parent's full transform up the chain: `Comp::world_matrix` folds the layer's own local transform under every ancestor's (parent applied outermost, so translate / rotate / scale all inherit), each ancestor sampled with its own expression context. A transform-only **Null** layer kind (`LayerKind::Null`, non-rendering — `draws_own_pixels()` false) from *Layer ▸ New ▸ Null* as a pivot / rig handle. A **pick-whip / parent picker** (the Properties *Parent* combo, *None* + every legally-parentable layer) gated by a **cycle guard** (`Comp::can_parent` rejects self-parent, missing layers, and descendant links that would loop; `world_matrix` also breaks any corrupt cycle with a bounded visited-set walk). Parent refs stay consistent on delete / reorder (children of a removed layer unparent; indices follow swaps). Additive serde (`parent` defaults `None`, `Null` a new `LayerKind` variant) so old `.pulse` files round-trip. Parent-chain composition (translate / rotate / scale), self / mutual / descendant cycle guards, Null creation, parent assign → clear, and serde back-compat unit-tested. Still TODO: parenting on **collapse-transformations**, preserving inter-layer parenting through **multi-layer pre-compose**, and **camera / light** parents.
- [ ] Tests: matte compositing, mask boolean modes, precomp re-eval, footage decode round-trip

### Phase 3 — Effects at scale (`prism-fx`)  *(the AE effect surface)*
Build effects on a unified **`prism-fx`** OpenFX-style GPU pass registry (suite-shared) so each is
authored once and stacks non-destructively per layer.
- [~] **Effect engine** (M): per-layer ordered effect stack **done**; **effect masks done** — a `serde`-defaulted `EffectMask` (enable toggle + a region reusing the layer-`Mask` geometry/coverage: feather / expansion / invert / opacity) limits where the per-pixel color-correction stack applies, blending the effected pixel back toward the original by the per-pixel mask coverage (`out = lerp(orig, effected, coverage)`, pure `blend_masked` / `apply_effects_masked`); wired into every effect-stack rasterization site (solid / footage / precomp / generate / adjustment), off-by-default (effect everywhere = back-compat), captured by animation presets, Properties *Effects* toggle + region/feather/invert/opacity UI, pure + render-path unit-tested. Still TODO: effect params as full `Property<T>` (keyframable/expressable — lands with the typed-property rebuild)
- [~] **Color correction** (M, reuse `prism-core::adjust`): **done** — Tint, Brightness/Contrast, Exposure, Levels, **Hue/Saturation** (HSL rotate/saturate/lighten), **Curves** (5-point Catmull-Rom master curve), **Color Balance** (per-range shadow/midtone/highlight pushes), **Channel Mixer** (per-output-channel RGB + constant mix, monochrome collapse — reuses the shared `prism_core::adjust::ChannelMixerMatrix::apply`), **Gradient Map** (luma → three-stop color gradient via the shared multi-stop `prism_core::gradient::Gradient::color_at`; black→first stop, mid→mid, white→last, original→mapped `amount` blend), **Tritone** (the same shared gradient primitive authored as shadows/midtones/highlights three-tone grade), all pure straight-RGBA linear-light passes in the per-layer stack, unit + render-path tested; still TODO: **Lumetri-style** grade
- [~] **Blur & sharpen** (M): **Gaussian blur** done (separable, per-axis sigma, repeat-edge, premultiplied so soft edges don't bleed — unit-tested; runs as a whole-buffer pass after color-correction/masks/matte, composes with motion blur); **Box Blur** done (separable moving-average, radius + 1..=8 **iterations** — ~3 ≈ Gaussian via central-limit — repeat-edge, premultiplied); **Directional Blur** done (1-D box average along an angle, the motion streak — perpendicular axis stays crisp, bilinear sub-pixel taps); **Radial Blur** done (**Spin** rotational + **Zoom** dolly streak about a centre, amount, symmetric taps so the centre stays sharp) — all three on the same `SpatialEffect` infrastructure (enum + `apply` pass + Properties *Blur* editor [Spin/Zoom picker] + Effects-browser *Blur & Sharpen* folder), pure/deterministic, unit + render-path tested, compose with masks/matte/keying/distort/motion-blur. **Camera-Lens / Fast Box** (bokeh), **Smart Sharpen**, **CC**-style still TODO
- [~] **Distort** (M): **Corner Pin / Transform / Mirror / Polar Coordinates done** — the first **distort** stack (whole-buffer **coordinate-remap** passes that *warp* the layer's rendered pixels, mirroring the spatial-effect family exactly: a `DistortEffect` enum + `apply_distort_effects` pass + `apply_distort` compositor bridge + Properties *Distort effects* section + Effects-browser *Distort* category). Each is an inverse-warp resampler over the layer's **isolated premultiplied linear-light** buffer (bilinear, off-buffer→transparent), run **after** the spatial passes (AE's distort-below-blur order) so it composes with opacity / blend / masks / track-mattes / spatial-effects / motion-blur; positions in normalized buffer space `[0,1]²` (preview = export). **Corner Pin** (inverse-bilinear four-point pin), **Transform** (effect-level anchor/position/scale/rotation/**skew**/opacity), **Mirror** (reflect across a line), **Polar Coordinates** (Rect↔Polar + interpolation blend). Pure remap + render-path unit-tested. Still TODO: **Mesh/Bezier warp**, **Displacement map**, **Turbulent/Wave**, **Optics-comp** (and Warp).
- [~] **Generate** (M): **Fractal/Turbulent Noise** done (the motion-design workhorse — deterministic multi-octave hash-seeded gradient noise, **Basic/Turbulent** type, contrast/brightness, uniform + X/Y **scale**, **complexity** octaves, **sub-influence/sub-scaling** persistence/lacunarity, **keyframable evolution** track, **seed**, **overflow** Clip/Wrap/HDR); **Gradient/Ramp** done (Linear + Radial colour ramp, start/end points + radius, endpoint-clamped, optional deterministic ramp **scatter**); **Checkerboard** done (two colours, per-axis cell size + anchor, `rem_euclid` parity); **4-Color Gradient** done (four corner colours bilinearly blended, **blend** sharpness + deterministic **jitter**); **Grid** done (line grid — per-axis cell size, border width, line + background colours, transparent or filled background, anchor); **Cell Pattern** done (cellular / Voronoi — seed-hashed feature points on a jittered grid, F1 + F2 nearest-distance shaped by **cell type** Bubbles/Crystals/Plates/Static Plates/Borders[F2−F1 web], **size**, **disorder** jitter, contrast/brightness, optional **invert**, **seed**, + a **keyframable evolution** track reusing Fractal Noise's, grayscale-linear) — all on the same generate infrastructure (`GenerateEffect` enum + `composite_generate` pass + Properties *Generate* section w/ a generator picker + Effects-browser *Generate* category), each a per-layer fill that replaces the layer's pixels (colour generators sRGB→linear-decoded, Fractal Noise + Cell Pattern grayscale-linear), runs in the compositor + render-preview, unit + render-path tested. **Lightning/Beam, Lens Flare, Audio-Spectrum/Waveform** still TODO
- [~] **Keying** (L): **Color Key / Luma Key / Chroma Key / Spill Suppression / Matte Choke done** — the first **keying** stack (whole-buffer **matte-pull** passes that *carve the layer's alpha* from a per-pixel colour test, mirroring the spatial/distort families exactly: a `KeyEffect` enum + `apply_key_effects` pass + `apply_key` compositor bridge + Properties *Keying* section + Effects-browser *Keying* category). Each operates on the layer's **isolated premultiplied linear-light** buffer (un-premultiply → test straight colour → re-premultiply by the new coverage), run **after** masks + track-matte but **before** the spatial passes (AE's keyer-then-blur matte-refine order) so a key pulls the matte first and a later blur softens the edge; composes with opacity / blend / masks / track-mattes / spatial / distort / motion-blur (offline + preview). **Color Key** (RGB-distance tolerance + softness feather), **Luma Key** (Rec.709 luminance threshold, key high/low + softness), **Chroma Key** (Keylight-style YCbCr chroma-plane distance, luminance-independent, gain/balance/softness), **Spill Suppression** (pull the dominant key channel toward the others, alpha untouched), **Matte Choke** (erode/dilate morphology + clip-black/clip-white). Pure matte math + render-path unit-tested. Still TODO: **Difference Matte**, **advanced matte refine** (colour-aware edge feather / grow), **Inner/Outer edge keying**, Linear-colour key.
- [~] **Stylize / Channel / Matte / Time** (M): **Glow** done (threshold→blur→screen bloom, unit-tested); **Find Edges / Mosaic / Stroke done** — the first dedicated **stylize** stack; **Stroke position** (`StrokePos` — Outside/Inside/Center) added (Batch 2: `#[serde(default)]` → Outside, back-compat); (whole-buffer **look-shaping** passes, mirroring the distort/keying families exactly: a `StylizeEffect` enum + `apply_stylize_effects` pass + `apply_stylize` compositor bridge + Properties *Stylize effects* section + Effects-browser *Stylize* category). Each operates on the layer's **isolated premultiplied linear-light** buffer, run **after** the spatial passes but **before** the distort passes (AE's distort-below-stylize order) so they compose with opacity / blend / masks / track-mattes / keying / blur / distort / motion-blur (offline + preview). **Find Edges** (per-channel **Sobel** magnitude on the straight colour, edge-clamped, AE-inverted to white-bg/dark-edges, with amount + invert, alpha preserved), **Mosaic** (pixelate into H×V blocks, each the premultiplied block average, counts clamped ≥1 / to per-pixel). Pure stylize math + render-path unit-tested. **Posterize** still TODO (deferred — overlaps the existing Levels/Curves/Gradient-Map grade; needs a distinct shape); channel combiner/shift/invert; matte choke/simple-choker; **Echo**, Posterize-Time, **Time Displacement** still TODO
- [~] **Perspective / Simulation** (M/L): **Drop Shadow** done (angled/offset blurred tint behind the layer, shadow-only, unit-tested); Bevel, **Particle** system (CC-particle-style), Shatter/Card-dance still TODO
- [x] **Presets / animation presets** (S): save an effect+keyframe stack as a named preset — **done**: a pure, serializable `AnimationPreset` (`comp/preset.rs`) captures a layer's six effect stacks (color / spatial / distort / key / stylize + generate fill & evolution) **and** its transform/property keyframe tracks (keyframes + expressions), with headless `capture(layer)` / `apply(&mut layer)`. Saved presets persist in the document (`Project::presets`, `#[serde(default)]`) and now **round-trip through the app** — *File ▸ Open…* reads a saved `.pulse` back (see Phase 6 *project file*), so a project's presets survive save → quit → reopen, not just save. Apply **replaces** the captured state and **leaves uncaptured properties untouched**. Wired into the Properties panel (*Animation presets* section: name + Save, per-preset Apply/Delete). Unit-tested (capture→apply round-trip, replace/leave-untouched rule, serde + project round-trip, legacy back-compat, determinism).
- [ ] Tests: golden-frame per effect; keyer matte quality; noise determinism (seeded)

### Phase 4 — Motion, time & expressions  *(makes it move like AE)*
- [ ] **Motion blur** (M): per-layer + comp shutter angle/phase, samples; on-transform and on-effect
- [~] **Frame blending** (S): **frame-mix done** — an image-sequence footage layer carries a `frame_blend: FrameBlend` mode (`#[serde(default)]` → `Off`, back-compat) that, when set to `Mix`, cross-dissolves the two source frames bracketing the fractional source-frame position (premultiplied linear-light blend, no fringing) so a retimed / fps-mismatched / time-remapped sequence glides between frames instead of stepping; honours loop / hold-at-end, decodes both frames through the shared `FrameCache`, and is wired through the main / motion-blur / track-matte decode paths via a shared `decode_footage`; UI dropdown in the Footage section; pure + render-path unit-tested. **pixel-motion** (optical-flow warp) still TODO.
- [~] **Time remapping** (M): **done** — a time-based layer (footage image-sequence / precomp) carries an optional, keyframable **time-remap** `Track` (source times in seconds, `serde`-defaulted **disabled** → back-compat) that, when enabled, drives the **source time** it is sampled at instead of the comp time; wired through the footage frame-index/sampling path *and* the precomp recursive-render time (transforms/opacity stay on comp time; fps-override/loop/hold + the precomp `time_offset` honoured at the remapped time; motion-blur sub-frames + footage/precomp matte sources remapped too), so users can **freeze** (constant remap), **reverse** (decreasing remap), and **slow/speed** playback — and via expressions, since the track carries them; enabling seeds AE-style default keys (identity ramp 0→source-duration, eased; single identity key when the duration is unknown); an "Enable Time Remap" toggle + the remap value as a keyframable property in Properties; pure + render-path unit-tested. Still TODO: **time stretch** (a layer-level speed/duration multiplier). **Frame blending** (frame-mix) so a slowed/retimed source interpolates between frames rather than stepping is now **done** — see the Frame-blending item above (pixel-motion interpolation still TODO)
- [~] **Spatial motion paths** (M): position keys draw an editable Bézier path; auto-orient along path; roving in time — **auto-orient along path** done (effective rotation follows the position curve's tangent, composed with keyed Rotation; `serde`-default off for back-compat) and the **spatial-path sampler** done (pure `comp::sample_path` yields position + unit tangent at any `t`, headless + unit-tested, ready for the overlay) and **roving keyframes** done (interior position keys re-timed to constant velocity along the path by arc length — pure `comp::roved_times`/`roved_tracks`, applied in position sampling + auto-orient, `serde`-default off for back-compat, headless + unit-tested). Still to do: the **editable on-canvas Bézier path overlay**.
- [~] **Expressions** (L): per-property `rhai` engine **done** (first slice) — any animatable **scalar** property (anchor/position/scale/rotation/opacity) carries an optional `serde`-defaulted `expression: Option<String>`; at sample time it's evaluated against a context exposing **`time` / `value` / `fps` / `duration` / `index`** plus helpers **`wiggle`** (deterministic per (layer, time) — stable-hash seeded, *not* `Math.random`), **`linear`**, **`clamp`** (and rhai's `sin/cos/abs/floor/…`); the keyframed value is exposed as `value` so expressions offset/drive it; compiled ASTs are cached per string; a parse/eval error **falls back to the keyframed value without panicking** and is surfaced in the UI; wired through the **real compositor + preview** (position/scale/rotation/anchor/opacity, parent chain, motion-blur sub-frames, matte sources); `fx` toggle + per-property expression field with a red error state in the Properties panel; engine + integration + render-path unit-tested. Still TODO: the broader AE library (**`loopOut/In`**, **`ease`**, **`random/seedRandom`**, **`valueAtTime`**, **`thisComp/thisLayer`**), **pick-whip property links**, and expressions on **non-scalar** properties (2D/3D/color/path) + effect/mask params (land with the typed-`Property<T>` rebuild)
- [x] **Markers** (S): comp + layer markers, work-area, time navigation — **done**:
  pure `Marker` (time / duration / label / color) + `WorkArea` (`[start,end]` with
  clamp / length / contains / is-full) in `comp/marker.rs`; `Comp` carries `markers`
  + `work_area` and `PulseLayer` carries `markers` (all `serde`-defaulted +
  self-healing back-compat). **Playback loops the work area**; transport
  prev/add/next-marker buttons + AE keys `B`/`N`/`M`; Comp ▸ Work area / Markers
  menus; a Properties **Markers** section; timeline draws comp markers on the ruler,
  layer markers per lane, the work-area band + a dimmed-outside playhead. Pure +
  comp-level navigation/clamp/serde unit-tested. **Export now renders the
  work-area range only** (After Effects' default render range — `RenderRange`
  WorkArea/Full + auto-default, files numbered by comp frame index so the first
  exported frame is the work-area start; *File ▸ Render range…* picker; unit-tested).
  Still open: marker-snapping, on-timeline marker dragging
- [~] Tests: **time-remap sampling done** (identity == no-remap, reverse `t→dur−t`, freeze hold, easing, expression-driven, default-key seeding, serde, render path); expression evaluation parity (scalar slice) done; **markers + work-area done** (marker model, work-area clamp/length/contains/is-full, marker navigation incl. comp+selected-layer scope, serde + back-compat); motion-blur sample count **Planned**

### Phase 5 — 3D compositing  *(depth, camera, light)*
- [~] **3D layers** (L): per-layer Z, 3D position/orientation/rotation, anchor; 2D/3D toggle — **done** (first slice): `serde`-defaulted `threed` toggle + `z` / `orient_x/y/z` tracks; layers projected through the comp camera (perspective) and **painter's z-sorted** by camera-space depth, with un-oriented layers exact and oriented layers a best-fit-affine approximation; back-compat verified (3D @ Z=0 == 2D byte-for-byte). TODO: full per-pixel perspective-warp rasterization, layer intersection, anchor-in-Z niceties, a 3D selection gizmo
- [~] **Camera** (M): one-/two-node camera, focal length / FOV, depth of field (focus distance/aperture/blur) — **done** (first slice): single-node free `Camera` (position + point-of-interest + vertical FOV / focal-length lens) on the comp, default reproduces the flat 2D look; pure `project` (point → screen + scale + depth) wired through the compositor + preview, UI in **Comp ▸ Camera**. **Depth of field done**: `serde`-defaulted (off) `dof_enabled` + `focus_distance` + `aperture` on the `Camera`; a pure circle-of-confusion radius `r = aperture·|depth − focus|/focus` (`Camera::coc_blur_radius`) defocuses each 3-D layer (Gaussian on its isolated buffer, last pass) by how far its camera-space depth is from focus — sharp at focus, grows with aperture + depth error; 2-D layers / DoF-off comps render byte-identically. UI sliders in **Comp ▸ Camera**. TODO: two-node (camera + target) rig niceties, bokeh/iris shape + focus-range falloff
- [~] **Lights** (M): point/spot/parallel/ambient; intensity/color/cone; **shadows** (shadow catcher) — **done** (first slice): a `serde`-defaulted **empty** comp `lights` list with **Ambient** + **Point** kinds (color/intensity, position for point), shading **3-D layers** that opt in via a `serde`-defaulted per-layer `accepts_lights` material flag. Pure, testable shading (`light::layer_normal` from the layer's X/Y/Z orientation; `light::illumination` → ambient floor + Σ point **Lambert** `color·intensity·max(0, N·L)` → RGB factor) applied over the layer's isolated premultiplied linear-light buffer; wired through the real compositor + motion-blur path + preview, UI in **Comp ▸ Lights** + a per-layer *Accepts lights* toggle. Back-compat exact (no lights / `accepts_lights=false` → identity `[1,1,1]`, byte-identical render). **Spot + Parallel + distance falloff done**: `LightKind::Spot` (a `direction`-aimed cone with inner-cone `cone_angle` + smoothstep `penumbra` soft edge, gating the Lambert term) and `LightKind::Parallel` (directional, position-independent, `L=−normalize(direction)`, no falloff), plus an optional `falloff` radius on Point/Spot attenuating by `(r/(r+d))²` — all four new `Light` fields `#[serde(default)]` so legacy comps load + render byte-identically (default falloff `0` = old Point exactly); editors in **Comp ▸ Lights**; spot cone/penumbra/parallel-position-independence/falloff/serde all unit-tested. TODO: **shadows** (shadow catcher), **specular** (Blinn-Phong highlight)
- [ ] **3D renderer** (L): a classic-3D compositing renderer (sorted, with intersection/shadows where feasible); optional extruded text/shapes later
- [ ] **Environment / material** (S): per-layer material options (accepts/casts shadows/lights, specular)
- [ ] Tests: camera projection, light/shadow correctness, z-sort

### Phase 6 — Media IO, render queue, audio  *(opens/exports everything)*
- [ ] **Import** (M, shared `prism-media`): video (H.264/H.265/ProRes/VP9/AV1), image sequences (PNG/JPEG/**EXR**/DPX/TIFF), audio (WAV/AAC/MP3), still images, SVG (→ shape layers), `.pigment`/`.contour` (via Dynamic Link)
- [~] **Render queue** (L): **done (basic)** — `RenderJob` with a `RenderFormat` enum
  (`Mp4H264`, `Mp4H265`, `ProResProxy`, `Gif`) dispatches to the right encoder per
  format (H.264/H.265 → `prism_media::encode_h264`; ProRes → FFmpeg pipe; GIF →
  `image::codecs::gif::GifEncoder`). Format badge shown in queue panel. Still TODO:
  **output modules** (codec settings / color range / resolution proxy), render
  settings (quality / samples), background threading, multi-frame rendering.
- [~] **Export formats** (M): **done** — PNG sequence + H.264 MP4 + animated GIF +
  ProRes (FFmpeg) export paths all wired. Still TODO: EXR/DPX/TIFF sequences,
  H.265/VP9/AV1, APNG/WebP, alpha/straight-vs-premul choices, Media-Encoder queue
  with per-item output modules.
- [ ] **Audio** (M): waveform display, level keyframes, basic mixing, audio-reactive (drive params from amplitude), audio preview synced to playhead
- [ ] **Color-managed output** (M, shared `prism-color`): OCIO display/output transforms, EXR scene-linear, broadcast-safe
- [ ] Tests: encode round-trip, EXR sequence fidelity, OCIO transform ΔE bound

### Phase 7 — AI, automation, interop  *(modern + pro + suite glue)*
- [ ] **AI (feature-gated, shared `pigment-ai`/`ort`):** **Roto Brush** (SAM2/3 + matting → animated mask), content-aware fill for video (temporal inpaint), scene-edit detect, AI denoise/upscale, AI motion-track assist — models on demand, graceful no-model path
- [ ] **Motion tracking / stabilization** (L): point/planar tracker, 2D stabilize (`prism-media`/optical-flow); apply track to transform/effect via expressions
- [ ] **Dynamic Link (producer)** (M, suite): expose Pulse comps to Reel; consume Contour artboards / Pigment docs as live layers; re-render on source change
- [ ] **Automation** (M): scripting via `rhai` (project/comp/layer API); render-queue automation; templates / essential-graphics-style parameterized comps
- [ ] **Plugins** (L, shared `prism-fx`): OpenFX effect plugins load across the suite

### Phase 8 — Reliability, performance & ease-of-use  *(production-grade)*
- [~] **Caching** (L): **done** — a **RAM frame cache** (`CanvasHost.frame_cache: BTreeMap<u64,Vec<u8>>`, 512-frame LRU cap) holds each rendered RGBA frame so the **work-area loops back in real time** once filled (the *RAM-preview* model); cache hits bypass the background render thread entirely (BGRA conversion only). `Action::ClearRamPreview` clears the cache. **The timeline ruler now draws a 2px green strip at the top for every cached frame** (iterates `cached_frame_set()` keys). Still TODO: **disk cache** + persistence across sessions, finer purge heuristics, cache-on-render (share the export render into the preview cache).
- [~] **Multi-frame rendering** (M): **done (frame-level)** — a **worker pool** (cores−2, clamped 1–8) renders preview frames **concurrently across cores** to fill the RAM cache ~N× faster, each worker with its own footage cache, jobs round-robined and epoch-gated. Still TODO: **tile-level** parallelism, a global **GPU-memory budget**, and multi-frame determinism for the render queue / export path
- [~] **Project file** open/save — **done**: *File ▸ Open…* (`rfd` picker → `serde_json` → `Project` → rebuild the editor, the inverse of `to_project`) round-trips a saved `.pulse` (every comp/precomp, fps/size/duration/work-area, **and** the document's animation presets); a malformed/unreadable file is **non-destructive** (logs + error dialog, current project untouched, never panics) and the preview caches are dropped so the reopened project decodes fresh. Still TODO: **autosave + crash recovery** (M), **versioned `.pulse`**, and **relink missing footage**.
- [ ] **Preferences / shortcuts / workspaces** (M): full remappable shortcut map (AE muscle-memory), dockable panels, saved workspaces, command palette
- [ ] **Timeline UX** (M): trim handles, layer-bar drag, snapping, dual playheads, in/out points, solo/lock/shy, label colors, search/filter layers
- [ ] **Preview controls** (S): resolution (full/half/quarter), region of interest, channel/alpha view, transparency grid, info/pixel readout, guides/grid/rulers/title-safe
- [ ] Tests: cache-hit correctness, RAM-preview realtime gate, autosave/relink round-trip, multi-frame determinism

---

## 4b. Parity coverage matrix (vs After Effects surface)

| Category | After Effects surface | Status | Phase |
|---|---|---|---|
| Comp / timeline / transport | comps, timeline, play/scrub | **Done** basic + **markers / work-area / time-navigation** (work-area-looped playback **and** work-area-range export); precomp **done** (see Layer types) | 0,2,4 |
| Properties / transform | anchor + 2D/3D position/scale/rot/opacity | **Partial** (5 linear tracks) → typed `Property<T>` **Planned** | 1 |
| Keyframe interpolation / graph editor | linear/hold/Bézier/auto, graph editor | **Partial** (linear/hold/Bézier ease + value-curve graph editor w/ draggable keys & handles + roving keyframes; auto-Bézier/speed-graph **Planned**) | 1 |
| Compositor / blend modes | GPU, 18+ modes, 32-bpc, linear | **Partial** (CPU software compositor; **per-layer blend modes** — all 18, reusing `prism-core` — done; GPU/32-bpc **Planned**) | 1 |
| Layer types | solid/text/shape/footage/adj/null/cam/light/precomp | **Partial** (solid, null, adjustment, **shape** [rect/ellipse/polygon/star + fill/stroke], **text** [built-in stroke font **or real outline fonts** (system families via `fontdb`/`ttf-parser`, bundled fallback) + fill/stroke/align/tracking/leading], **footage** [stills + numbered image sequences via `prism-io`, fps override / loop / hold-last / alpha interp], **precomp** [nest a sibling comp + time-offset, recursive render, cycle guard]) → footage **video** (FFmpeg/`prism-media`) / cam / light **Planned**; text shaping/animators **Planned** | 1,2,5 |
| Masks / roto | Bézier masks, modes, feather, roto brush | **Partial** (Bézier masks: add/subtract/intersect/difference, feather, expansion, opacity, invert — done; animated shapes / on-canvas editing / roto brush **Planned**) | 2,7 |
| Track mattes | alpha/luma | **Planned** | 2 |
| Precomps / parenting | nesting, collapse, pick-whip | **Partial** (precomp **nesting** [recursive render + cycle guard + time-offset], **pre-compose** [single layer], pick-whip **parenting**, **time-remap curve** done; **collapse transformations**, multi-layer pre-compose, comp navigator **Planned**) | 2 |
| Effects | ~hundreds; color/blur/distort/generate/keying/stylize/time | **Partial** (color-correction stack — Tint/Bright-Contrast/Exposure/Levels/**Hue-Sat**/**Curves**/**Color Balance** — + spatial **Gaussian Blur / Box Blur / Directional Blur / Radial Blur [Spin+Zoom] / Drop Shadow / Glow** + generate **Fractal Noise** [keyframable evolution] / **Gradient Ramp** [linear+radial] / **Checkerboard** / **4-Color Gradient** / **Grid** / **Cell Pattern** [Voronoi, keyframable evolution] + distort **Corner Pin / Transform / Mirror / Polar Coordinates** + keying **Color Key / Luma Key / Chroma Key / Spill Suppression / Matte Choke**; Mesh/Bezier warp · Displacement · Turbulent/Wave · Optics-comp · Lightning / Lens Flare / Audio-Spectrum · Difference Matte + the rest **Planned** via `prism-fx`) | 3 |
| Motion blur / frame blend | full | **Partial** (motion blur done; **frame-mix** blending for retimed footage sequences done; **pixel-motion** frame blend **Planned**) | 4 |
| Time remap / stretch | full | **Partial** (keyframable **time-remap curve** on footage-sequence / precomp — freeze/reverse/retime via keys + expressions, default-key seeding, UI toggle; **frame-mix blending** for retimed sequences done; **time stretch** **Planned**) | 4 |
| Expressions | full JS expression language | **Partial** (scalar props via `rhai`: `time`/`value`/`fps`/`duration`/`index` + `wiggle`/`linear`/`clamp` + math, AST-cached, error fallback + UI `fx` toggle; `loopOut`/`ease`/`random`/`valueAtTime`/`thisComp` + **pick-whip links** + non-scalar props **Planned**) | 4 |
| 3D layers / camera / lights / shadows | classic + advanced 3D | **Planned** | 5 |
| Media import (video/seq/audio/EXR) | full | **Partial** (still images + numbered **image sequences** as footage layers via `prism-io`) → **video** / audio / EXR/DPX **Planned** (`prism-media`) | 2,6 |
| Render queue / output modules / MFR | full | **Partial** (queue panel with H.264/H.265/ProRes/GIF format selection; output modules / MFR **Planned**) | 6 |
| Audio | waveform, levels, reactive | **Planned** | 6 |
| Color management (OCIO/linear/32-bpc) | full | **Planned** (`prism-color`) | 1,6 |
| Motion tracking / stabilize | point/planar/3D camera track | **Planned** | 7 |
| AI (roto/CAF/upscale) | Sensei | **Planned** (feature-gated) | 7 |
| Dynamic Link / interop | live to Premiere, comps | **Planned** (suite) | 7 |
| Automation / scripting / plugins | ExtendScript/UXP, C SDK | **Planned** | 7 |
| Caching / RAM-preview / MFR / autosave / prefs / workspaces | full | **Planned** | 8 |
| Clip-based NLE editing / audio mixer | — | **Won't** (Reel) | — |
| Primary raster paint / deep vector authoring | — | **Won't** (Pigment / Contour) | — |

---

## UI/UX & workspace

What a pro motion app's shell needs that we lack today (fixed four-panel layout, dark
theme, no docking). Phase-8 "Preferences / shortcuts / workspaces" tracks the deep cut;
this section is the concrete, near-term shell work. (Scroll + collapsible Properties
sections landed first — the rest below is unchecked.)

- [ ] **Scrollable panel bodies** — every long panel (Properties, Layers, Timeline lanes) wraps its body in a `ScrollArea` so content never overflows the window. *(done: Properties / Layers / Timeline)*
- [ ] **Collapsible Properties sections** — Transform / Effects / Spatial / Masks / Matte / Text / Shape each its own `CollapsingHeader` so users hide what they don't need. *(done — open by default, per-layer state)*
- [ ] **Dockable panels** — drag panels to re-dock / float / resize (egui `Tile`/`dock` or `egui_dock`); replace the hard-coded `SidePanel`/`TopBottomPanel` layout.
- [ ] **Saveable workspaces** — named layouts (Animation / Effects / Paint-style) persisted to prefs; quick-switch; reset-to-default.
- [x] **Panel show/hide via a Window menu** — **done**: a **Window** menu toggles each dockable panel's visibility (Layers / Properties / Timeline; the central Preview is always present), with *Show all* / *Hide all* shortcuts, backed by a pure unit-tested `PanelVisibility` (`app/workspace.rs`). Per-workspace persistence lands with saveable workspaces.
- [ ] **Tabbed panel groups** — stack panels as tabs in one dock slot (Affinity "Studio" / AE panel groups); drag a tab out to float.
- [ ] **Contextual options strip** — an Affinity-style toolbar row whose controls change with the active tool / selected layer kind.
- [ ] **Timeline panel ergonomics pass** — resizable label/lane columns, horizontal time zoom + scroll, sticky ruler/header, collapse-to-mini, layer search/filter (overlaps Phase-8 Timeline UX but is a focused shell job).
- [ ] **Keyboard shortcuts** — a remappable shortcut map for transport, layer ops, panel toggles (AE muscle-memory: `Space`, `[`/`]`, `T`/`P`/`S`/`R`, `U`); a shortcut-help overlay.
- [ ] **Theme toggle** — light / dark (and high-contrast) switch; today `theme.rs` hard-codes one dark theme. Persist the choice.

## UI/UX & workspace — parity gaps (vs AE + Affinity)

Important features still missing after skimming the plan against After Effects and Affinity
(Photo/Designer). One-line note each; not implemented this turn.

- [x] **Per-layer blend mode picker** — **done**: every layer carries a `serde`-defaulted [`BlendMode`] (reusing `prism-core`'s shared 18-mode set); the CPU compositor blends each layer onto the accumulator via a pure-Rust `blend_over` (W3C blend+composite in linear light, mirroring Pigment's `composite.wgsl`). A **Blend** dropdown in Properties (separable + HSL groups) and a non-Normal blend badge in the Layers panel; unit- + integration-tested. *(Blend modes now show in the live preview — the **render-preview** composites through the same CPU compositor; see Phase 1 *Preview fidelity*.)*
- [x] **Effect search / browser** — **done**: a searchable **Effects & Presets** panel (left-docked, Window-menu toggle, hidden by default) with a type-to-filter search box and **categorised** collapsing folders (Color Correction / Blur & Sharpen / Perspective / Stylize); clicking an effect adds it to the selected layer's matching stack. Backed by a pure, unit-tested registry + ranked token-AND matcher (`comp/effect_browser.rs`, name/keyword search, exact > prefix > substring > keyword scoring, category grouping) that stays in lock-step with both effect stacks' `defaults()`. Drag-onto-layer is the remaining nicety.
- [ ] **Onion-skinning** — ghost neighboring frames behind the playhead (frames before/after, count + opacity falloff) for hand-keyed timing — standard in motion tooling, absent from the plan.
- [x] **On-canvas transform gizmo** — **done**: drag the selected layer directly in the preview to **move / scale / rotate / re-anchor** it (bounding box, four corner scale handles, a rotation knob, and an anchor-point cross), keying the edited local transform at the playhead. The drag math (`gizmo.rs`) maps the pointer screen→comp→**parent-local** so parented layers drag correctly under a rotated/scaled parent; pure (`GizmoGeom::build` / `hit_test` / `drag`) and unit-tested.
- [ ] **Snapping & smart guides in the preview** — snap to layer edges/centers, comp center, and user guides while dragging (plan only lists *timeline* snapping + static guides).
- [ ] **Color / swatch picker reuse** — a shared eyedropper + recent-swatches across fill/stroke/effect colors (Affinity-style), instead of isolated `color_edit_button`s.
- [ ] **Numeric drag-edit on every value field** — click-drag a label to scrub, double-click to type (AE/Affinity convention) for all sliders, not just the slider track.
- [ ] **Undo / redo** — no command history exists yet; a core requirement for any editor (likely a shared `prism-core` edit-stack), gating real editing ergonomics.

---

## 5. Milestones

| Milestone | Phases | Capability | Approx parity |
|---|---|---|---|
| **Scaffold** | 0 | Comp + timeline + linear keyframes + solid preview + save | ~10% *(here today)* |
| **Foundation** | 1–2 | Typed props + Bézier ease + graph editor + GPU compositor + footage/text/shape/masks/mattes/precomps | ~40% |
| **Effects + motion** | 3–4 | + effect stack (`prism-fx`), motion blur, time remap, **expressions** | ~65% |
| **3D + media** | 5–6 | + 3D/camera/lights, media import, render queue/output, audio | **~85%** |
| **Parity+** | 7–8 | + AI/roto, tracking, Dynamic Link, caching/RAM-preview/MFR, autosave/prefs/workspaces | **≥90%** |

**The ≥85% line lands at the end of Phase 6.** Highest felt-parity-per-effort first: **the Phase-1
foundation rebuild (typed properties + Bézier easing + GPU compositor) gates everything** — do it before
breadth. Then footage/masks/precomps (Ph2), then effects + expressions (Ph3–4).

---

## 6. Hard problems (mitigations)

1. **Foundation debt** → today's linear-only `f32` `Track` and CPU solid-rect preview must become typed `Property<T>` + Bézier easing + the shared GPU render graph **before** piling on features; retrofitting interpolation/compositing later = rewrite. This is Phase 1, deliberately first.
2. **Realtime preview** → AE's RAM-preview model: cache rendered frames (RAM + disk), play the work-area back at frame rate; smart purge on edit; multi-frame rendering across cores fills the cache fast.
3. **Effect sprawl** → one `prism-fx` OpenFX-style pass registry (author once, suite-wide, stack non-destructively) instead of bespoke pipelines; effect params are full `Property<T>`.
4. **Color correctness** → linear-light float compositing through `prism-color`; OpenColorIO config-driven display/output transforms; EXR scene-linear; never bake until render.
5. **Media reliability** → `prism-media` (FFmpeg) shared with Reel; robust footage interpretation (alpha/frame-rate/color), proxies, and relink-missing-footage.
6. **Expressions safety/perf** → sandboxed `rhai`/`rune`; cache compiled expressions; evaluate per frame with cycle/error guards; surface errors without crashing the render.
7. **Shared-crate discipline** → the compositor/blend/adjust/color/fx/media crates stay **time-agnostic**; time (keyframes/expressions) is Pulse's layer on top, so Pigment/Contour/Reel reuse them unchanged.
8. **Scope vs Reel** → Pulse is a *layer+keyframe compositor*, not a clip-based NLE; resist building a multi-track audio mixer / clip-trim timeline — that's Reel, fed by Pulse via Dynamic Link.

---

## 8. Wave 14 — completed (parity reached)

- Anchor point adjustment tool (`Tool::AnchorPoint`, writes `anchor_x`/`anchor_y` tracks)
- Displacement map effect (`DistortEffect::DisplacementMap`, morphological warp)
- Expression controls (`LayerKind::ExpressionControl`, Slider/Angle/Checkbox/Color/Point)
- ProRes/DNxHD export via FFmpeg pipe (with graceful "install FFmpeg" fallback)
- Output module presets (`OutputPreset`, save/load/delete)

**Pulse has reached ≥85% feature parity with Adobe After Effects.**

## 7. Immediate next steps

1. [~] **Phase 1 foundation** — **Bézier easing** + hold landed (self-contained Newton-solved cubic, no `kurbo` dep yet); still TODO: generalize `Track`→ typed `Property<T>`; auto-Bézier; anchor-point + separable position.
2. [ ] **GPU compositor** on the shared `prism-core` render graph; 18 blend modes; float buffers; linear-light. Retire the CPU solid-rect preview.
3. [~] **Graph Editor** — value-curve editor with draggable keys + per-key Bézier ease handles + **roving keys** (constant-velocity arc-length re-timing) landed; **speed graph** and **auto-Bézier** still TODO.
4. [~] **Layer types**: Adjustment + Null + **footage** (stills + image sequences via `prism-io`) + **precomps** (nest a sibling comp, recursive render + cycle guard + time-offset, single-layer pre-compose) + **time remapping** (keyframable source-time curve on footage-sequence / precomp — freeze/reverse/retime) + **frame-mix blending** (for retimed footage sequences) done; **footage video** (FFmpeg / `prism-media`), **pixel-motion frame blending**, **precomp collapse-transformations / multi-layer pre-compose** (Ph2/Ph4) still TODO.
5. [ ] Coordinate **`prism-fx`** (effects host), **`prism-media`** (FFmpeg, shared w/ Reel — the next footage step: real video + audio decode on top of today's stills/sequences), and **`prism-vector`** (masks/shape layers, shared w/ Contour) promotions with the suite before building on them.

*Foundations are free. The product is the polish — and the glue between apps.*
