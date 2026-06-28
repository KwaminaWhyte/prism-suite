# Changelog

All notable changes to **Pulse** (the Prism suite's motion/compositing app) are
documented here. Format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/);
this project is pre-1.0, so versions are `0.x` milestones and track the workspace
`version` in the root `Cargo.toml`. History before 0.9.0 lives in the git log.

## [Unreleased]

## [0.15.0] - 2026-06-28

### Added — Puppet Pin tool: real mesh deformation (no stubs)
- **Triangulated deformation mesh** (`app_state/puppet_mesh.rs`) — `grid_mesh(layer, bounds, resolution)` builds a uniform grid clipped to a layer's bounding box: `(resolution+1)²` vertices, `2·resolution²` triangles, degenerate-free.
- **Weighted handle deformation** — `deform_mesh(&DeformMesh, &[PinDisplacement])` displaces every vertex by a **normalized inverse-distance-squared blend** (Shepard interpolation) of the pin displacements, plus a *rest-anchor* ground weight so a **single** pin yields a smooth radial falloff (`Δ·falloff²/(falloff²+d²)`) instead of a rigid translation. A pin's `stiffness` scales its weight: a high-stiffness pin holding `delta = 0` keeps its neighborhood rigid (the **Starch** pin); `stiffness = 0` disables it. Pure + deterministic.
- **Barycentric warp map** — `sample_warp(point, &original, &deformed)` locates a point's enclosing rest triangle, takes barycentric coordinates, and reconstructs it in the matching deformed triangle (total map; outside points fall back to the nearest triangle).
- **App action layer** — `Action::PuppetMesh(PuppetMeshAction::{AddMesh,RemoveMesh,SetResolution,AddPin,MovePin,DeletePin,SetPinStiffness})` over a per-layer mesh map (app-side, like particles — not undoable); `App::deform_layer_mesh(layer)` / `App::warp_layer_point(layer, point)`. Nested under one `Action` variant to keep `actions.rs` under 1000 lines.
- +30 tests: identity (no pins / zero deltas), pin-on-vertex exactness, single-pin falloff (near > far), two-pin symmetric blend at the midpoint, starch/zero-stiffness, determinism, barycentric correctness, `sample_warp` translation/centroid/fallback, and the full action layer.

## [0.14.0] - 2026-06-28

### Added — Spatial motion paths + graph-editor temporal easing
- **Spatial Bézier motion path** (`app_state/motion_paths.rs`) — a position keyframed in 2D with per-keyframe spatial tangent handles (`in_handle` / `out_handle`). `MotionPath::sample_position(t)` walks the cubic-Bézier spline through the keys with **De Casteljau** (passes exactly through every keyframe), `sample_constant_speed(u)` re-parameterizes it by **arc length** for constant-velocity travel, and `orientation_at(t)` reads the path tangent to drive *Orient Along Path* (auto-orient).
- **Graph-editor temporal easing** — each keyframe carries incoming/outgoing `KeyframeEase { influence: 0..100, speed }` handles forming the AE two-control-point value graph. `eased_progress(out, in, x)` solves cubic-Bézier `y` given `x` (Newton's method + bisection fallback); `MotionPath::value_at(t, axis)` maps it onto a scalar property. `EasePreset::{Linear, EasyEase, EaseIn, EaseOut, Hold}` supplies the F9 family (linear == lerp, easy-ease symmetric, ease-in slow→fast, hold == step).
- **App action layer** — `Action::MotionPaths(MotionPathAction::{SetSpatialTangents, SetTemporalEase, ApplyEasePreset, ToggleAutoOrient})` (app-side state, like particles — not undoable), nested under one `Action` variant to keep `actions.rs` under 1000 lines.
- +25 tests (1075 → 1100): endpoint pass-through, symmetric-handle centering, arc-length constant speed, auto-orient tangent angles, linear/ease-in/ease-out/easy-ease/hold value graphs, and the action layer.

## [0.13.0] - 2026-06-28

### Added — Particle system (CC Particle World / Particle Playground analog)
- **Deterministic 2D particle simulator** (`app_state/particles.rs`) — `simulate_particles(&EmitterConfig, t)` returns every live particle (position/velocity/age/size/opacity/color) at a frame. Seeded, reproducible: each particle's birth + randomness (cone angle / speed / lifetime) comes from a **SplitMix64** keyed by `(emitter_seed, particle_index)` — no global RNG, no `Instant`. Real physics integrated from birth to `t` with a fixed sub-step: **gravity**, **air-resistance drag**, and optional **value-noise turbulence** (reuses `effects_noise::fractal_noise`).
- **Per-particle envelopes** — size-over-life, opacity fade-in/out, and start→end color lerp.
- **App action layer** — `Action::Particles(ParticleAction::{Add,Remove,SetParam,SetSeed,SetPosition,SetGravity,SetColors})` manage a per-layer emitter map (app-side, like Fractal Noise — not undoable); `App::simulate_layer_particles(layer, t)`.
- +28 tests, 1047 → 1075 (determinism, gravity, drag, turbulence, cone/variance, lifetime death, emission-vs-rate, fade-out, color lerp). (`actions.rs` kept under 1000 via a nested `ParticleAction` enum + minor variant compaction.)

## [0.12.0] - 2026-06-24

### Added — Multi-line + comprehensive text input
- **Expression editor → multi-line `prism_ui::TextArea`** — type AE-style multi-line rhai expressions (Cmd+Enter → `SetExpression` + `EvaluateExpression`); chips still quick-insert.
- **Effect-browser search box** — live `SetEffectQuery` filtering as you type.
- **Typeable numerics** — comp settings (W/H/FPS/Duration) + layer transform (X/Y/scale/rotation/opacity); also fixed comp-settings seeding so the panel shows before first Apply.
- +13 tests (1034 → 1047). (`mod.rs` kept under 1000 via `geom_util.rs` extraction.)

## [0.11.0] - 2026-06-24

### Added — Real text input (`prism_ui::TextField`)
- **Typeable expression editor** — type ANY rhai expression (no longer preset-chip-only); Enter / Evaluate → `SetExpression` + `EvaluateExpression`, with result/error readout. Chips kept as quick-insert.
- **Layer + comp rename** — `RenameLayer` / `SetCompName` via inline fields.
- **Render output path** — typeable path → `SetRenderOutputPath`.
- +7 tests (1027 → 1034).

## [0.10.0] - 2026-06-24

### Added — UI (GPUI panels)
- **Expression editor** — property selector + preset-expression chips →
  `SetExpression`, "Evaluate" → `EvaluateExpression`, shows result/errors.
- **Output Module dialog** → `AddOutputModule` / `SetOutputModule*`, lists modules.
- **Preferences** panel → `SetPref*` + disk-cache dir/purge controls with a live
  usage bar.
- **Keying + Lights** inspector — per-layer keyers + comp 3D lights.
- UI-only (emits existing 0.9.0 actions, no new state); 8 helper/round-trip tests.

## [0.9.0] - 2026-06-24

### Added — Feature waves 1–3 (parity push)
- **After-Effects expression API** — deterministic Rust implementations of
  `loopOut/loopIn/loopInOut/loopOutDuration` (cycle/pingpong/offset/continue),
  `valueAtTime`, `velocityAtTime` (segment-slope), `wiggle` (SplitMix64-seeded, no
  global RNG), `linear`/`ease`/`easeIn`/`easeOut`/`clamp`, `posterizeTime`;
  `EvaluateExpression` runs the real string through the rhai engine
  (`app_state/expressions.rs`).
- **Output modules** — render-queue output config: format (MP4/MOV/PNGseq/EXRseq),
  codec, color depth, scale/resolution, frame range, audio (`output_module.rs`).
- **Preferences** — general/display/media/previews with serde JSON load/save
  (`preferences.rs`).
- **Disk-cache manager** — cache dir, max size, usage tracking, purge, LRU
  eviction (`cache_manager.rs`).
- **Rotobrush (ML-free)** — luma+chroma threshold segmentation + BFS flood-fill
  mask propagation across frames (`tracking.rs`).
- **Audio mixer** — per-track gain(dB)/pan/solo/mute/bus routing, master bus,
  equal-power pan law, solo-aware mixdown (`audio_mixer.rs`).
- **Mask editor** — bezier masks with Add/Subtract/Intersect/Difference modes,
  feather, opacity, expansion, inversion; rasterize the stack to an alpha matte
  (`masks.rs`).
- **Fractal Noise + Turbulent Displace** — deterministic multi-octave value noise
  and a noise-field pixel displacer (`effects_noise.rs`).
- **Per-layer motion blur** — shutter-angle/phase/samples override, transform
  sampled and averaged across the shutter (`motion_blur.rs`).
- **Time-stretch / time-remap** — layer stretch factor composed with a remap curve
  (`time_stretch.rs`).
- **Keying suite** — chroma (brightness-independent chromaticity distance), color,
  and luma keyers + Keylight-style spill suppression (`keying.rs`).
- **Distortion effects** — Corner Pin (homography), Bezier Warp, Wave Warp,
  Roughen Edges (`effects_distort.rs`).
- **3D lights & materials** — point/spot/ambient lights + per-layer Blinn-Phong
  material options (`lighting3d.rs`).
- **Shape repeater + trim paths** — N-copy repeater with per-copy transform/opacity
  ramp; static + keyed trim-paths (`shape_repeater.rs`).
- **Render formats** — codec/fps/CRF/audio output spec with an ffmpeg-arg builder
  (`render_formats.rs`).

### Changed — File organization
- Split files over the ~1000-line limit: `composition.rs` → `composition.rs` +
  `composition_layers.rs`; `effects_chain.rs` → `effects_chain.rs` +
  `effects_apply.rs`; `mod.rs` → `mod.rs` + a new `dispatch.rs` action router;
  `actions.rs` → `actions.rs` + `actions_undoable.rs`.
